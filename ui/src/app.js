// The panel shell: page routing, the scratchpad, the key and LSK handlers,
// and the wiring from the host's status stream onto the screen.
//
// Import rule: this is the only module under src/ that may import
// ./bridge.js, and it keeps what it learns there to itself. It builds the one
// small interface every page is allowed to use, hands it to each page module at
// registration and again as `ctx.fmc` on every callback, and the adapter itself
// is not a member of either — a page that could reach the host would, and the
// boundary would then only be as strong as the next reviewer's attention.
//
// This file owns the chrome and nothing else. Pages are separate modules that
// register themselves; the config pages are imported lazily on the first
// navigation to one — so the shell boots, and screenshots, with no config page
// present at all. If that import fails the scratchpad says PAGE UNAVAILABLE;
// it never throws and never leaves a blank screen, because a blank panel on a
// machine nobody can reach is the worst outcome this app has.

import bridge from './bridge.js';
import * as statusPage from './pages/status-page.js';
import { defaultStatus } from './status.js';

const MAX_SCRATCHPAD_CHARS = 4096;
const SCRATCHPAD_COLS = 22;
/** Painted ahead of a masked entry, so a scratchpad of dots says why. */
const MASKED_PREFIX = 'MASKED ';
/** Pages that live in the lazily-imported bundle. */
const LAZY_PAGE_IDS = new Set(['NETWORK', 'SIM', 'TRAFFIC', 'DL-INDEX', 'FPLN']);

const dom = {
  screen: document.getElementById('fmc-screen'),
  title: document.getElementById('page-title'),
  number: document.getElementById('page-number'),
  body: document.getElementById('page-body'),
  scratchpad: document.getElementById('scratchpad'),
  msgLine: document.getElementById('msg-line'),
  bridgeMode: document.getElementById('bridge-mode'),
  announcer: document.getElementById('fmc-announcer'),
};

const pages = new Map();
const state = {
  pageId: null,
  status: defaultStatus(),
  config: null,
  configPath: '',
  entry: '',
  // Set by a page that has armed a secret entry: the entry is then painted as
  // dots until it is replaced, emptied by CLR, or the page changes.
  entryMasked: false,
  message: null,
  pagesLoaded: false,
  pagesLoading: null,
  datalink: null,
};

// ── Scratchpad ───────────────────────────────────────────────────────────────

function paintScratchpad() {
  if (!dom.scratchpad) return;
  if (state.message) {
    dom.scratchpad.textContent = state.message.text;
    dom.scratchpad.setAttribute('data-message-kind', state.message.kind);
    return;
  }
  dom.scratchpad.textContent = state.entryMasked
    ? MASKED_PREFIX + '•'.repeat(Math.min(SCRATCHPAD_COLS - MASKED_PREFIX.length, state.entry.length))
    : state.entry.slice(-SCRATCHPAD_COLS);
  dom.scratchpad.setAttribute('data-message-kind', 'entry');
}

/**
 * `kind` of `entry` replaces what the user has typed; `error` and `advisory`
 * are messages layered over it, so clearing the message brings the typed
 * entry back — the CDU behaviour people expect.
 */
function setScratchpad(text, kind = 'entry') {
  const value = text === null || text === undefined ? '' : String(text);
  if (kind === 'entry') {
    // A new entry is never the secret that was armed for, so it is shown.
    writeEntry(value, false);
    return;
  }
  state.message = { text: value, kind: kind === 'advisory' ? 'advisory' : 'error' };
  paintScratchpad();
  announce(value);
}

/** Replaces the entry and sets its masking in one step, so it is painted once. */
function writeEntry(value, masked) {
  state.entry = value.slice(0, MAX_SCRATCHPAD_CHARS);
  state.message = null;
  state.entryMasked = masked;
  paintScratchpad();
}

function setScratchpadMasked(on) {
  state.entryMasked = on === true;
  paintScratchpad();
}

function getScratchpad() {
  return state.message ? '' : state.entry;
}

function appendScratchpad(char) {
  state.message = null;
  if (state.entry.length >= MAX_SCRATCHPAD_CHARS) {
    paintScratchpad();
    return;
  }
  state.entry = (state.entry + char).slice(0, MAX_SCRATCHPAD_CHARS);
  paintScratchpad();
}

function clearScratchpad() {
  if (state.message) {
    state.message = null;
  } else if (state.entry.length > 0) {
    state.entry = state.entry.slice(0, -1);
    // Removing the last character of a masked entry also cancels it.
    if (!state.entry) state.entryMasked = false;
  } else {
    // A CLR on an empty scratchpad with no message cancels an armed entry;
    // one that only dismissed a message leaves it armed.
    state.entryMasked = false;
  }
  paintScratchpad();
}

// ── Page registry and routing ────────────────────────────────────────────────

/**
 * A page is `{ id, title, group, n, m, render(ctx), onLsk(lsk, ctx),
 * onStatus(status), dispose() }`. `render` returns the element to place in
 * #page-body, or writes into `ctx.body` itself and returns nothing. Only
 * `id`, `title` and `render` are required.
 */
function registerPage(page) {
  if (!page || typeof page.id !== 'string' || typeof page.render !== 'function') {
    setScratchpad('PAGE UNAVAILABLE', 'error');
    return;
  }
  pages.set(page.id, page);
}

function pageContext() {
  return {
    body: dom.body,
    config: state.config,
    status: state.status,
    fmc: window.FMC,
  };
}

async function loadPages() {
  if (state.pagesLoaded) return true;
  if (!state.pagesLoading) {
    state.pagesLoading = import('./pages/index.js')
      .then((module) => {
        module.register(window.FMC);
        state.pagesLoaded = true;
        return true;
      })
      .catch(() => {
        state.pagesLoading = null;
        return false;
      });
  }
  return state.pagesLoading;
}

async function showPage(id) {
  if (!pages.has(id) && LAZY_PAGE_IDS.has(id)) {
    const loaded = await loadPages();
    if (!loaded || !pages.has(id)) {
      setScratchpad('PAGE UNAVAILABLE', 'error');
      return false;
    }
  }
  const page = pages.get(id);
  if (!page) {
    setScratchpad('PAGE UNAVAILABLE', 'error');
    return false;
  }

  const previous = state.pageId ? pages.get(state.pageId) : null;
  if (previous && typeof previous.dispose === 'function') {
    try {
      previous.dispose();
    } catch {
      /* a page that cannot tidy up must not block navigation */
    }
  }

  // Cleared child by child rather than with replaceChildren(): a throw here
  // would leave no current page and freeze the display.
  if (dom.body) while (dom.body.firstChild) dom.body.removeChild(dom.body.firstChild);
  // A new page starts with an empty, unmasked scratchpad: no page inherits
  // another's entry or its masking.
  if (state.pageId !== id) setScratchpad('');
  state.pageId = id;
  paintScratchpad();

  let view = null;
  try {
    view = page.render(pageContext());
  } catch {
    setScratchpad('PAGE UNAVAILABLE', 'error');
  }
  if (view instanceof Node && dom.body) dom.body.appendChild(view);

  if (dom.screen) dom.screen.setAttribute('data-page', id);
  if (dom.title) dom.title.textContent = page.title || id;
  if (dom.number) {
    dom.number.textContent = page.m && page.m > 1 ? `${page.n}/${page.m}` : '';
  }
  paintStatus();
  announce(page.m && page.m > 1 ? `${page.title || id} PAGE ${page.n} OF ${page.m}` : page.title || id);
  scheduleLskLabels();
  return true;
}

/** PREV/NEXT step through the current page's group, in order, and wrap. */
function stepGroup(delta) {
  const current = state.pageId ? pages.get(state.pageId) : null;
  if (!current) return;
  const group = [...pages.values()]
    .filter((page) => page.group && page.group === current.group)
    .sort((a, b) => (a.n || 0) - (b.n || 0));
  if (group.length < 2) {
    setScratchpad('KEY NOT ACTIVE', 'error');
    return;
  }
  const index = group.findIndex((page) => page.id === current.id);
  const next = group[(index + delta + group.length) % group.length];
  showPage(next.id);
}

// ── Built-in pages ───────────────────────────────────────────────────────────

function row(className, left, right) {
  const el = document.createElement('div');
  el.className = `row ${className}`;
  const l = document.createElement('span');
  l.className = 'cell-l';
  l.textContent = left || '';
  el.appendChild(l);
  if (right !== undefined) {
    const r = document.createElement('span');
    r.className = 'cell-r';
    r.textContent = right;
    el.appendChild(r);
  }
  return el;
}

const MENU_ITEMS = [
  { lsk: 'L1', label: '<STATUS', page: 'STATUS' },
  { lsk: 'L2', label: '<NETWORK', page: 'NETWORK' },
  { lsk: 'L3', label: '<SIM', page: 'SIM' },
  { lsk: 'L4', label: '<TRAFFIC', page: 'TRAFFIC' },
  { lsk: 'L5', label: '<DATALINK', page: 'DL-INDEX' },
  { lsk: 'L6', label: '<FPLN', page: 'FPLN' },
];

registerPage({
  id: 'MENU',
  title: 'SABIÁ',
  group: 'MENU',
  n: 1,
  m: 1,
  render() {
    const view = document.createElement('div');
    view.className = 'page-view';
    view.setAttribute('data-page-view', 'MENU');
    view.appendChild(row('row-label', 'SELECT PAGE'));
    for (const item of MENU_ITEMS) {
      const line = row('row-value row-prompts', item.label);
      line.firstChild.classList.add('prompt');
      view.appendChild(line);
      // Six items fill all twelve rows; the last one gets no spacer below it.
      if (view.childElementCount < 12) view.appendChild(row('row-label', ''));
    }
    while (view.childElementCount < 12) view.appendChild(row('row-label', ''));
    return view;
  },
  onLsk(lsk) {
    const item = MENU_ITEMS.find((entry) => entry.lsk === lsk);
    if (!item) return false;
    showPage(item.page);
    return true;
  },
});

// ── Host commands ────────────────────────────────────────────────────────────

/**
 * The three command members of the page interface share one failure message,
 * because a page has nothing useful to do with the error and every page that
 * offered a command prompt would otherwise repeat this try/catch.
 */
async function runCommand(fn) {
  try {
    await fn();
    return true;
  } catch {
    setScratchpad('COMMAND FAILED', 'error');
    return false;
  }
}

/**
 * Datalink calls answer with an `{ok, result|error}` envelope. Anything else,
 * including a rejected call, becomes the host-error envelope, so a datalink
 * page always has a code to show and never sees a throw. The scratchpad is
 * left to the page, which knows whether the failure is worth showing.
 */
function runDatalink(fn) {
  const hostError = { ok: false, error: { code: 'host-error', httpStatus: null, serverCode: null } };
  return Promise.resolve()
    .then(fn)
    .then(
      (result) => (result && typeof result === 'object' && typeof result.ok === 'boolean' ? result : hostError),
      () => hostError,
    );
}

/** The same `n/m` rule `showPage` applies, for pages that page themselves. */
function setPageNumber(n, m) {
  if (!dom.number) return;
  dom.number.textContent = m > 1 ? `${n}/${m}` : '';
  if (m > 1) {
    const page = state.pageId ? pages.get(state.pageId) : null;
    announce(`${page ? page.title || page.id : ''} PAGE ${n} OF ${m}`);
  }
}

// ── Status painting ──────────────────────────────────────────────────────────

function paintStatus() {
  const page = state.pageId ? pages.get(state.pageId) : null;
  if (page && typeof page.onStatus === 'function') {
    try {
      page.onStatus(state.status);
    } catch {
      /* a page that mishandles a status must not stop the axes updating */
    }
  }
}

function applyStatus(status) {
  if (!status || typeof status !== 'object') return;
  state.status = status;
  // A host whose link came up only after boot failed the boot-time config
  // read; a status proves the link is up, so read it again once.
  if (!state.configLoaded && !state.configLoading) {
    state.configLoading = true;
    const done = () => { state.configLoading = false; };
    loadConfig().then(done, done);
  }
  const problems = status.app && Array.isArray(status.app.problems) ? status.app.problems : null;
  if (problems && problems.length > 0) {
    setMessageLine(`CFG ${problems[0].field}: ${problems[0].message}`, 'warn');
  }
  paintStatus();
}

/** Datalink availability is its own channel; it never touches `state.status`. */
function applyDatalink(value) {
  if (!value || typeof value !== 'object' || value.type !== 'datalink-state') return;
  state.datalink = value;
  const page = state.pageId ? pages.get(state.pageId) : null;
  if (page && typeof page.onDatalink === 'function') {
    try {
      page.onDatalink(value);
    } catch {
      /* a page that mishandles a datalink state must not stop the next one */
    }
  }
}

function setMessageLine(text, level = 'info') {
  if (!dom.msgLine) return;
  const value = text || '';
  dom.msgLine.textContent = value;
  dom.msgLine.setAttribute('data-level', level);
  // applyStatus re-sends the same CFG problem on every heartbeat; announcing
  // it once and staying quiet until it actually changes is what makes that
  // tolerable to hear.
  if ((level === 'warn' || level === 'error') && value && value !== lastAnnouncedMessageLine) {
    lastAnnouncedMessageLine = value;
    announce(value);
  }
}

function applyLog(log) {
  if (!log || typeof log !== 'object') return;
  setMessageLine(String(log.message || ''), String(log.level || 'info'));
}

function applyExit(payload) {
  const info = payload && typeof payload === 'object' ? payload : {};
  const code = info.code === null || info.code === undefined ? info.signal || '?' : info.code;
  const tail = info.restarting ? ' - RESTARTING' : '';
  setMessageLine(`SIDECAR EXIT ${code}${tail}`, 'error');
}

// ── Accessibility ────────────────────────────────────────────────────────────

let lastAnnouncedMessageLine = null;

/**
 * The one thing a screen reader hears from this panel. Everything else here
 * is painted, not spoken, so a repeated message (KEY NOT ACTIVE twice) has to
 * come out as two announcements, not one: emptying the node and filling it on
 * the next tick is what makes an unchanged value still get read again.
 */
function announce(text) {
  if (!dom.announcer) return;
  const value = text === null || text === undefined ? '' : String(text);
  if (!value) return;
  dom.announcer.textContent = '';
  setTimeout(() => {
    dom.announcer.textContent = value;
  }, 0);
}

/** Every element under `root` with no element children of its own — the
 *  actual text-bearing spans and divs a row is built from, whatever page
 *  built it. A row with no wrapper cell (a CFG label div, say) is its own
 *  only leaf. */
function leafElements(root, out) {
  const children = root.children ? [...root.children] : [];
  if (children.length === 0) {
    out.push(root);
    return out;
  }
  for (const child of children) leafElements(child, out);
  return out;
}

function isPaintedOut(el) {
  if (el.getAttribute('aria-hidden') === 'true') return true;
  if (typeof window !== 'undefined' && typeof window.getComputedStyle === 'function') {
    const style = window.getComputedStyle(el);
    if (style && style.visibility === 'hidden') return true;
  }
  return false;
}

/**
 * The rect of a leaf's own painted text, not the leaf's box: a leaf with no
 * wrapping cell of its own (a CFG label div, say — the whole row is its only
 * element child) is as wide as the row, and its box's centre would land on
 * the midline regardless of which side the short run of text actually sits
 * on. A Range over the leaf's contents measures the text itself instead —
 * clamped to the leaf's own box, because a value CSS clips with `overflow:
 * hidden` (a long server URL, a Windows certificate path) still carries its
 * full, untruncated string in the DOM: the Range alone would measure past
 * the visible edge. The accessible name built from this is still allowed to
 * say the whole thing — a screen reader hearing the full URL is fine, and
 * none of these fields are secret (CFG NETWORK's token paints only dots) —
 * only the geometry used to pick a side needs to stay inside what is drawn.
 */
function textRect(el) {
  if (typeof document.createRange !== 'function') return null;
  try {
    const range = document.createRange();
    range.selectNodeContents(el);
    const text = range.getBoundingClientRect();
    if (!text || text.width === 0) return null;
    const box = el.getBoundingClientRect();
    if (!box || box.width === 0) return null;
    const left = Math.max(text.left, box.left);
    const right = Math.min(text.right, box.right);
    if (right <= left) return null;
    return { left, width: right - left };
  } catch {
    return null;
  }
}

/** The concatenated text of a row's leaves that sit on one half of the screen. */
function rowSideText(row, side, midX) {
  if (!row) return '';
  const words = [];
  for (const leaf of leafElements(row, [])) {
    if (isPaintedOut(leaf)) continue;
    const rect = textRect(leaf);
    if (!rect) continue;
    const centerX = rect.left + rect.width / 2;
    const onSide = side === 'L' ? centerX < midX : centerX >= midX;
    if (!onSide) continue;
    const text = (leaf.textContent || '').trim();
    if (text) words.push(text);
  }
  return words.join(' ').trim();
}

/**
 * A value made entirely of one placeholder glyph run reads as what it means,
 * not the glyphs themselves — nobody wants "bullet bullet bullet..." read out
 * for a set token. Mixed text, or a glyph sitting inside real text, is left
 * exactly as painted: this only fires when the *whole* trimmed side is one
 * run of the same placeholder character.
 */
export function describePlaceholder(text) {
  if (/^•+$/.test(text)) return 'SET';
  if (/^□+$/.test(text)) return 'EMPTY, REQUIRED';
  if (/^-{4,}$/.test(text)) return 'EMPTY';
  return text;
}

function rowAtY(rows, y) {
  for (const row of rows) {
    const rect = row.getBoundingClientRect();
    if (rect.top <= y && y <= rect.bottom) return row;
  }
  return null;
}

function setAriaLabelIfChanged(el, value) {
  if (el.getAttribute('aria-label') !== value) el.setAttribute('aria-label', value);
}

/**
 * Reads the screen the way a person looking at it would: each LSK's name is
 * whatever is painted next to it, found by geometry so no page has to spell
 * it out twice. Only runs when the DOM can report real layout — a test DOM
 * or a detached document leaves whatever label was already there alone.
 */
function updateLskLabels() {
  if (!dom.screen || !dom.body) return;
  if (typeof window === 'undefined' || typeof window.getComputedStyle !== 'function') return;
  if (typeof document.createRange !== 'function') return;
  // #fmc-screen's own box has asymmetric borders (2px left, 1px right), so its
  // border-box centre is not the screen's visual midline; .screen-grid is the
  // content itself, sized and centred by the padding on both sides equally.
  const gridEl = dom.screen.querySelector('.screen-grid');
  const gridRect = gridEl ? gridEl.getBoundingClientRect() : null;
  if (!gridRect || gridRect.width === 0) return;
  const rows = [...dom.body.querySelectorAll('.row')];
  if (rows.length === 0) return;
  const rowHeight = rows[0].getBoundingClientRect().height;
  if (!rowHeight) return;
  const midX = gridRect.left + gridRect.width / 2;

  for (const button of document.querySelectorAll('[data-lsk]')) {
    const id = button.getAttribute('data-lsk');
    if (!id) continue;
    const rect = button.getBoundingClientRect();
    if (!rect || (rect.width === 0 && rect.height === 0)) continue;
    const side = id.charAt(0);
    const centerY = rect.top + rect.height / 2;
    const row = rowAtY(rows, centerY);
    if (!row) {
      setAriaLabelIfChanged(button, `${id}, not active`);
      continue;
    }
    const value = rowSideText(row, side, midX);
    const labelRow = rowAtY(rows, centerY - rowHeight);
    const label = labelRow ? rowSideText(labelRow, side, midX) : '';
    const parts = [id];
    if (label) parts.push(label);
    // The placeholder mapping only ever applies to the value: a label row is
    // always real text (a field's name), never a glyph run.
    parts.push(value ? describePlaceholder(value) : 'not active');
    setAriaLabelIfChanged(button, parts.join(', '));
  }
}

let lskLabelsQueued = false;

/** Coalesced to one pass per frame: a single render can touch a dozen rows. */
function scheduleLskLabels() {
  if (lskLabelsQueued) return;
  lskLabelsQueued = true;
  const run = () => {
    lskLabelsQueued = false;
    try {
      updateLskLabels();
    } catch {
      /* a layout that cannot be read must not disturb the existing labels */
    }
  };
  if (typeof requestAnimationFrame === 'function') requestAnimationFrame(run);
  else setTimeout(run, 0);
}

/** Keeps the LSK labels current between renders too: a page that repaints a
 *  field in place (CFG's edited-value colour, say) doesn't call showPage. */
function watchLskLabels() {
  if (typeof MutationObserver !== 'function' || !dom.body) return;
  const observer = new MutationObserver(() => scheduleLskLabels());
  observer.observe(dom.body, { subtree: true, childList: true, characterData: true });
  if (dom.title) observer.observe(dom.title, { childList: true, characterData: true });
}

// ── Keys ────────────────────────────────────────────────────────────────────

function handleKey(key) {
  if (key === '+/-') {
    // Edits the entry in place, so a masked entry stays masked.
    writeEntry(state.entry.startsWith('-') ? state.entry.slice(1) : `-${state.entry}`, state.entryMasked);
    return;
  }
  if (typeof key === 'string' && key.length === 1) {
    appendScratchpad(key);
    return;
  }
  if (key === 'PREV' || key === 'NEXT') {
    // A page with pages of its own (a message thread, a long uplink) steps
    // through them first; the group stepping below is for the rest.
    const page = state.pageId ? pages.get(state.pageId) : null;
    if (page && typeof page.onPageKey === 'function') {
      let handled = false;
      try {
        handled = page.onPageKey(key === 'NEXT' ? 1 : -1, pageContext()) === true;
      } catch {
        setScratchpad('COMMAND FAILED', 'error');
        return;
      }
      if (handled) return;
    }
  }
  switch (key) {
    case 'SP':
      appendScratchpad(' ');
      return;
    case 'CLR':
      clearScratchpad();
      return;
    case 'DEL':
      // The CDU way to empty a field: put DELETE on the scratchpad and line
      // select it onto the field you want cleared.
      if (getScratchpad().length === 0) setScratchpad('DELETE');
      else setScratchpad('');
      return;
    case 'MENU':
      showPage('MENU');
      return;
    case 'PREV':
      stepGroup(-1);
      return;
    case 'NEXT':
      stepGroup(1);
      return;
    default:
      break;
  }
  const page = state.pageId ? pages.get(state.pageId) : null;
  if (page && typeof page.onKey === 'function' && page.onKey(key, pageContext())) return;
  setScratchpad('KEY NOT ACTIVE', 'error');
}

function handleLsk(lsk) {
  const page = state.pageId ? pages.get(state.pageId) : null;
  if (page && typeof page.onLsk === 'function') {
    let handled = false;
    try {
      handled = page.onLsk(lsk, pageContext()) === true;
    } catch {
      setScratchpad('COMMAND FAILED', 'error');
      return;
    }
    if (handled) return;
  }
  setScratchpad('KEY NOT ACTIVE', 'error');
}

function flashKey(el) {
  if (!el) return;
  el.classList.add('is-pressed');
  setTimeout(() => el.classList.remove('is-pressed'), 110);
}

/** Physical keyboard, mapped onto the same handlers as the drawn keys. */
function keyFromEvent(event) {
  const key = event.key;
  if (typeof key !== 'string') return null;
  if (key.length === 1) {
    if (key === ' ') return 'SP';
    return key;
  }
  if (key === 'Backspace') return 'CLR';
  if (key === 'Delete') return 'DEL';
  if (key === 'Enter') return 'EXEC';
  if (key === 'Escape') return 'CLR';
  if (key === 'PageUp') return 'PREV';
  if (key === 'PageDown') return 'NEXT';
  return null;
}

// Whether the currently focused element got there from the keyboard (Tab) or
// a pointer. A key press only bypasses its own EXEC/SP mapping in the former
// case, so clicking a key and then pressing Enter still means EXEC.
let keyboardModality = false;

function wireKeys() {
  document.addEventListener('click', (event) => {
    const target = event.target instanceof Element ? event.target.closest('[data-key],[data-lsk]') : null;
    if (!target) return;
    flashKey(target);
    const lsk = target.getAttribute('data-lsk');
    if (lsk) handleLsk(lsk);
    else handleKey(target.getAttribute('data-key'));
  });

  document.addEventListener('paste', (event) => {
    const text = event.clipboardData?.getData('text/plain');
    if (text) {
      event.preventDefault();
      appendScratchpad(text.replace(/[\r\n]/g, ''));
    }
  });
  let clearTimer;
  document.addEventListener('pointerdown', (event) => {
    // A pointer press means whatever gets focus next isn't from tabbing, so
    // the next Enter is EXEC again, not a re-press of the key just clicked.
    keyboardModality = false;
    if (event.target.closest?.('[data-key="CLR"]')) {
      clearTimer = setTimeout(() => setScratchpad(''), 500);
    }
  });
  for (const type of ['pointerup', 'pointercancel']) {
    document.addEventListener(type, () => clearTimeout(clearTimer));
  }
  if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
    // Switching windows mid-press must not leave a timer running that clears
    // the scratchpad once the user is looking at something else entirely.
    window.addEventListener('blur', () => clearTimeout(clearTimer));
  }

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Tab') keyboardModality = true;
    if (event.ctrlKey || event.metaKey || event.altKey) return;

    const lskShortcut = /^F([1-6])$/.exec(event.key);
    if (lskShortcut) {
      event.preventDefault();
      const lsk = `${event.shiftKey ? 'R' : 'L'}${lskShortcut[1]}`;
      flashKey(document.querySelector(`[data-lsk="${lsk}"]`));
      handleLsk(lsk);
      return;
    }

    const key = keyFromEvent(event);
    if (!key) return;

    if ((key === 'EXEC' || key === 'SP') && keyboardModality) {
      // Tab landed on a real key: let the browser's own activation press it
      // rather than also running the global EXEC/SP mapping on top of it.
      const focused = event.target instanceof Element ? event.target.closest('[data-key],[data-lsk]') : null;
      if (focused) return;
    }

    event.preventDefault();
    // A physical letter key flashes its uppercase face even when typed in
    // lowercase; what lands on the scratchpad keeps the case actually typed,
    // since URLs and tokens are case-sensitive.
    const flashTarget = key.length === 1 ? key.toUpperCase() : key;
    const escaped = typeof CSS !== 'undefined' && typeof CSS.escape === 'function'
      ? CSS.escape(flashTarget)
      : flashTarget.replace(/["\\]/g, '\\$&');
    flashKey(document.querySelector(`[data-key="${escaped}"]`));
    handleKey(key);
  });
}

// ── Boot ─────────────────────────────────────────────────────────────────────

async function loadConfig() {
  try {
    const result = await bridge.getConfig();
    if (result && typeof result === 'object') {
      state.config = result.raw || result.config || null;
      if (typeof result.path === 'string') state.configPath = result.path;
    }
    state.configLoaded = true;
  } catch {
    setMessageLine('CONFIG READ FAILED', 'error');
  }
  try {
    const path = await bridge.getConfigPath();
    if (typeof path === 'string' && path) state.configPath = path;
  } catch {
    /* the path is a convenience; its absence must not stop the boot */
  }
  paintStatus();
}

function boot() {
  if (dom.bridgeMode) {
    // The host names itself. Hardcoding TAURI here would make the one piece of
    // chrome whose job is saying which host you are on the piece that lies.
    dom.bridgeMode.setAttribute('data-stub', bridge.isStub ? 'true' : 'false');
    dom.bridgeMode.textContent = bridge.hostLabel;
  }

  // Everything a page is allowed to do, and the adapter is deliberately not
  // among it. Built as a local first so it can be injected into page modules
  // that are loaded by something other than a browser's module loader.
  const fmc = {
    registerPage,
    showPage,
    setScratchpad,
    getScratchpad,
    hasScratchpadError: () => state.message?.kind === 'error',
    setScratchpadMasked,
    isScratchpadMasked: () => state.entryMasked,
    getConfigCache: () => state.config,
    getConfigPath: () => state.configPath,
    getStatus: () => state.status,
    refreshConfig: loadConfig,
    setConfig: (patch) => bridge.setConfig(patch),
    startUplink: () => runCommand(() => bridge.startUplink()),
    stopUplink: () => runCommand(() => bridge.stopUplink()),
    restartSidecar: () => runCommand(() => bridge.restartSidecar()),
    getDatalinkState: () => state.datalink,
    watchDatalink: (on) => runDatalink(() => bridge.watchDatalink(on === true)),
    refreshDatalink: () => runDatalink(() => bridge.refreshDatalink()),
    getDatalinkThread: (req) => runDatalink(() => bridge.getDatalinkThread(req)),
    getCannedMessages: () => runDatalink(() => bridge.getCannedMessages()),
    sendCannedMessage: (req) => runDatalink(() => bridge.sendCannedMessage(req)),
    requestWeather: (req) => runDatalink(() => bridge.requestWeather(req)),
    requestLoadsheet: (req) => runDatalink(() => bridge.requestLoadsheet(req)),
    setPageNumber,
    getSimbriefSettings: () => runDatalink(() => bridge.getSimbriefSettings()),
    prefileSimbrief: () => runDatalink(() => bridge.prefileSimbrief()),
    clearPrefiledLeg: () => runDatalink(() => bridge.clearPrefiledLeg()),
    requestClearance: (req) => runDatalink(() => bridge.requestClearance(req)),
    getSayIntentionsStatus: (req) => runDatalink(() => bridge.getSayIntentionsStatus(req)),
    linkSayIntentions: (req) => runDatalink(() => bridge.linkSayIntentions(req)),
    unlinkSayIntentions: (req) => runDatalink(() => bridge.unlinkSayIntentions(req)),
    importSayIntentionsComms: (req) => runDatalink(() => bridge.importSayIntentionsComms(req)),
    sendSayIntentionsPdc: (req) => runDatalink(() => bridge.sendSayIntentionsPdc(req)),
  };
  window.FMC = fmc;

  wireKeys();
  watchLskLabels();
  paintScratchpad();
  statusPage.register(fmc);
  showPage('STATUS');

  bridge.onStatus(applyStatus);
  bridge.onLog(applyLog);
  bridge.onExit(applyExit);
  bridge.onDatalink(applyDatalink);

  // Paint from the last known status immediately rather than waiting up to
  // five seconds for the next heartbeat.
  Promise.resolve()
    .then(() => bridge.getStatus())
    .then((status) => applyStatus(status))
    .catch(() => {});
  Promise.resolve()
    .then(() => bridge.getDatalinkState())
    .then(applyDatalink)
    .catch(() => {});

  loadConfig();

  // The retry countdown ticks locally: the sidecar sends nextRetryAt once and
  // the seconds come off the clock here.
  setInterval(paintStatus, 1000);
}

boot();
