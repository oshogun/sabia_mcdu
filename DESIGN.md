---
name: Sabiá CDU
description: A flight-computer panel for logging MSFS flights, whose screen is evidence of state.
colors:
  avionics-green: "#28E06E"
  entry-cyan: "#4FD8F0"
  caution-amber: "#E0A21A"
  fault-red: "#F25E55"
  legend-white: "#E8EDE9"
  idle-sage: "#7E9285"
  night-glass: "#0A1410"
  glass-glow: "#10241A"
  bezel-grey: "#2E3033"
  bezel-top: "#33363A"
  bezel-bottom: "#26282B"
  bezel-light: "#45484C"
  bezel-edge: "#17181A"
  screen-lip: "#3D4044"
  strip-grey: "#232528"
  plate-text: "#9AA39C"
  key-face: "#3A3D41"
  key-hi: "#4B4E53"
  key-lo: "#2F3236"
  key-legend: "#DDE2DE"
typography:
  screen-data:
    fontFamily: "Consolas, Lucida Console, DejaVu Sans Mono, Courier New, monospace"
    fontSize: "1em"
    lineHeight: "2.05em"
    letterSpacing: "normal"
  screen-title:
    fontFamily: "Consolas, Lucida Console, DejaVu Sans Mono, Courier New, monospace"
    fontSize: "1em"
    letterSpacing: "0.1em"
  screen-label:
    fontFamily: "Consolas, Lucida Console, DejaVu Sans Mono, Courier New, monospace"
    fontSize: "0.76em"
    lineHeight: 1
    letterSpacing: "0.06em"
  page-number:
    fontFamily: "Consolas, Lucida Console, DejaVu Sans Mono, Courier New, monospace"
    fontSize: "0.78em"
  key-legend:
    fontFamily: "Consolas, Lucida Console, DejaVu Sans Mono, Courier New, monospace"
    fontSize: "0.95em"
    letterSpacing: "0.04em"
  annunciator:
    fontFamily: "Consolas, Lucida Console, DejaVu Sans Mono, Courier New, monospace"
    fontSize: "0.76em"
    lineHeight: 1.15
    letterSpacing: "0.08em"
  plate:
    fontFamily: "Consolas, Lucida Console, DejaVu Sans Mono, Courier New, monospace"
    fontSize: "0.62em"
    fontWeight: 700
    letterSpacing: "0.22em"
rounded:
  glass: "5px"
  key: "4px"
  lsk: "3px"
  strip: "3px"
spacing:
  row: "2.05em"
  screen-pad: "0.9em"
  key-gap: "0.45em"
  face-gap: "0.7em"
  key-block-gap: "1.4em"
components:
  line-select-key:
    backgroundColor: "{colors.key-face}"
    rounded: "{rounded.lsk}"
    width: "2.6em"
    height: "1.35em"
  key:
    backgroundColor: "{colors.key-face}"
    textColor: "{colors.key-legend}"
    typography: "{typography.key-legend}"
    rounded: "{rounded.key}"
    height: "2.05em"
    width: "2.9em"
    padding: "0 0.2em"
  key-function:
    backgroundColor: "{colors.key-face}"
    textColor: "{colors.legend-white}"
    rounded: "{rounded.key}"
  key-exec:
    backgroundColor: "{colors.key-face}"
    textColor: "{colors.avionics-green}"
    rounded: "{rounded.key}"
    width: "4.2em"
  screen:
    backgroundColor: "{colors.night-glass}"
    textColor: "{colors.avionics-green}"
    rounded: "{rounded.glass}"
    padding: "0.9em"
  annunciator-strip:
    backgroundColor: "{colors.strip-grey}"
    textColor: "{colors.idle-sage}"
    typography: "{typography.annunciator}"
    rounded: "{rounded.strip}"
    padding: "0.1em 0.4em"
    height: "1.1em"
---

# Design System: Sabiá CDU

## Overview

**Creative North Star: "The Evidence Panel"**

A screenshot of this panel is proof of what the client is doing. Everything in the system serves that promise. The screen holds still, so a capture is never caught mid-transition. Every line of state is words first and colour second. A failure is named on the glass rather than implied. The look is a night-lit flight computer (a moulded grey bezel, a dark glass screen, avionics-coloured monospace characters) because that is the instrument a pilot already trusts to tell the truth mid-flight.

The world is **avionics and flight computers**, and the current CDU is its current expression, not its only possible one. It uses a 14-row character grid, six line-select keys per side and an alphanumeric key grid. It takes CDU conventions (a small label over a large value, LSK prompts at the row ends, one scratchpad, fixed colour meanings) and keeps Sabiá's own choices where they read better: a 50-column grid instead of 24, and the bird on the plate. A rework toward an MFD-style display (soft keys around a graphic screen, pages that draw rather than list) is legitimate if it improves usability significantly. It must stay a flight computer.

The unit is the window. One scale, `--fmc-scale`, sizes everything in `em` and `ch`, and whichever window axis runs out first sets it. So the unit fills the window edge to edge and the grid never reflows. Spare height becomes a taller screen: the rows open up, and the characters and keys keep their size. Spare width shows as more bezel. Neither shows as a margin around a floating panel.

**Key Characteristics:**
- One monospace family, one scale, one grid; the size of every element is a multiple of `--fmc-scale`.
- Colour carries severity and role, never information that the words don't also carry.
- Physical depth: the glass is set into the bezel, key caps are raised, and a press sinks the cap.
- Motion is limited to a key press (a 1px drop) and the scratchpad cursor blink, which is off under reduced motion.
- Local fonts only; nothing waits on the network to paint.

## Colors

Avionics colours at night: saturated character colours on near-black glass, set in a family of warm-neutral moulded greys.

### Primary
- **Avionics Green** (`avionics-green`): data. Value rows, a healthy status line (`ok`), the plain DATALINK text, and the legend on `EXEC`. It's the colour the eye reads as "the answer".

### Secondary
- **Entry Cyan** (`entry-cyan`): what the pilot is doing right now. The scratchpad cursor, a CFG field edited but not yet saved, the traffic advisory line, and the keyboard focus ring.

### Tertiary
- **Caution Amber** (`caution-amber`): something needs attention but nothing has failed. `caution` status lines, the CFG feedback line, scratchpad advisories, and warn-level message-line text.
- **Fault Red** (`fault-red`): something failed. `fault` status lines, scratchpad errors (`KEY NOT ACTIVE`, `COMMAND FAILED`), and error-level message-line text.

### Neutral
- **Legend White** (`legend-white`): structure. The page title, label rows, prompts (`<INDEX`, `SAVE>`), the page number, and the scratchpad entry.
- **Idle Sage** (`idle-sage`): present but inactive. `idle` status lines (`SIM LINK STANDBY`), the config path, the host label. A desaturated green, so it reads as "the green, resting" rather than as another colour.
- **Night Glass** (`night-glass`) and **Glass Glow** (`glass-glow`): the screen, and the brighter centre of its vignette.
- **Bezel Grey** (`bezel-grey`), **Bezel Top/Bottom** (`bezel-top`, `bezel-bottom`), **Bezel Light/Edge** (`bezel-light`, `bezel-edge`), **Screen Lip** (`screen-lip`): the moulded unit, and the lit and shadowed edges of the recess the glass sits in.
- **Strip Grey** (`strip-grey`): the annunciator strip under the screen.
- **Plate Text** (`plate-text`): the `SABIÁ` wordmark and the `CDU-1` badge.
- **Key Face / Hi / Lo** (`key-face`, `key-hi`, `key-lo`) and **Key Legend** (`key-legend`): key caps and their lettering.

The hairline under the title row and over the scratchpad is Avionics Green at 16% alpha (`--fmc-rule`).

### Named Rules
**The Monochrome Rule.** Every line must read correctly with colour removed. Colour follows severity; the words carry the meaning.

**The 4.5 Floor Rule.** Every character colour clears 4.5:1 on every surface it is painted on: the glass, the vignette's bright centre, and the annunciator strip. A new colour or surface means re-checking every pair. `idle-sage` and `fault-red` were raised specifically to clear the strip.

**The Fixed Meanings Rule.** Green is data, cyan is the pilot's own input, amber is caution, red is fault, white is structure. A new page reuses these roles; it doesn't invent a colour.

## Typography

**Display Font:** none. The whole system is one family.
**Body Font:** Consolas (with Lucida Console, DejaVu Sans Mono, Courier New, monospace)
**Label/Mono Font:** the same.

**Character:** a technical monospace with a Windows-native first choice. Every character is one cell of the grid, so alignment is arithmetic, not layout. The stack is local-only on purpose: the app runs offline on a flight-sim PC.

### Hierarchy
Sizes are multiples of `--fmc-scale` (about 14.9px at the default 560×820 window, and 14.5px at the 544×716 minimum). The minimum window is set so that the smallest informational role, `0.76em`, never paints below 11px. Resizing the window is the zoom: browser zoom has no effect, because the unit always fills the window.
- **Screen title** (`1em`, `0.1em` tracking, centred, Legend White): the page title row. It's exposed to assistive tech as the page's level-1 heading.
- **Screen data** (`1em`, line height = one grid row): value rows, prompts and the scratchpad.
- **Screen label** (`0.76em`, `0.06em` tracking, bottom-aligned in its row): the small label above each value. Size alone separates label from data, the CDU convention, with no colour needed.
- **Page number** (`0.78em`): `n/m` at the right of the title row.
- **Key legend** (`0.95em`, `0.04em` tracking): key-cap lettering.
- **Annunciator** (`0.76em`, line height `1.15`, `0.08em` tracking): the message line and host label under the screen.
- **Plate** (`0.62em`, bold wordmark, `0.22em` tracking): `SABIÁ` and `CDU-1`. It's a logotype, not information.

### Named Rules
**The Label Floor Rule.** Nothing that carries information is set smaller than the screen label (`0.76em`). Only the plate goes below it.

**The Uppercase Vocabulary Rule.** On-screen copy is uppercase avionics vocabulary: prompts as rendered (`START>`, `<INDEX`, `SEND*`), terse state words (`STANDBY`, `NO COMM`). Typed entries keep the case the pilot typed, because URLs and tokens are case-sensitive.

## Layout

The screen is a character grid: `--fmc-cols` (50) columns by 14 rows of `--fmc-row-h` (2.05em). Row 1 is the title, rows 2–13 are the page body, and row 14 is the scratchpad. The page body uses the CDU pair rhythm: six label/value row pairs. Left content sits flush left and right content flush right in the same row, with at least `1ch` between them.

The six LSKs per side sit in their own 14-row grid, beside the value row of each pair (rows 3, 5, 7, 9, 11 and 13), so a key is physically aligned with the line it acts on. Anything on the left half of a row belongs to the L key, and anything on the right half to the R key. The assistive-tech names of the LSKs are derived from that same geometry.

Around the screen, top to bottom: the plate row, then the face (LSK column, screen, LSK column, separated by `0.7em`), the annunciator strip (exactly as wide as the glass), then two key blocks side by side separated by `1.4em`: function keys with the number pad, and the alpha grid with `EXEC`. Keys are separated by `0.45em`.

**Scaling.** `--fmc-unit-w` (37.5) and `--fmc-unit-h` (49.35) are the unit's own size in scale units, and `--fmc-scale = min(100vw / unit-w, 100vh / unit-h)`. There are no breakpoints. When the window is taller than those proportions, the scale is set by width and the leftover height is shared equally among the 14 screen rows (`--fmc-row-stretch`, at most `0.8 × --fmc-scale` per row). The glass grows taller, and the label/value pairs and their LSKs spread apart together. Only height beyond that cap shows as bezel. Any layout change that adds height or width must update those two numbers, or the bezel silently clips its own contents.

### Named Rules
**The One Scale Rule.** Every dimension on the unit is in `em` or `ch` of `--fmc-scale`. A `px` value appears only for hairlines, bevel edges and corner radii.

**The Row Ownership Rule.** A key acts on the row it sits beside. Content never floats between rows or sits in the middle of the screen where no key can reach it.

## Elevation & Depth

Depth is physical and stays constant. It is modelled with light from above, not with floating layers. The bezel is lighter at the top (`bezel-top` → `bezel-grey` → `bezel-bottom`). The glass is recessed: dark edges top and left (`bezel-edge`, 2px), a lit lip bottom and right (`screen-lip`, 1px), and a deep inner shadow. Key caps are raised, and pressing one flips the cap's gradient and sinks its shadow. Nothing in the system sits above anything else. There are no overlays, popovers or floating cards.

### Shadow Vocabulary
- **Key raised** (`box-shadow: inset 0 1px 0 rgba(255,255,255,0.2), inset 0 -1px 0 rgba(0,0,0,0.55), 0 2px 2px rgba(0,0,0,0.45)`): every key cap and LSK at rest.
- **Key sunk** (`box-shadow: inset 0 1px 3px rgba(0,0,0,0.65)`): a pressed key, together with the flipped cap gradient and a `translateY(1px)`.
- **Glass recess** (`box-shadow: inset 0 2px 10px rgba(0,0,0,0.85), 0 1px 0 #45484C`): the screen set into the bezel.
- **Strip inset** (`box-shadow: inset 0 1px 2px rgba(0,0,0,0.6)`): the annunciator strip.
- **Vignette** (a radial `glass-glow` centre at 45% height, plus a top-lit linear fade): the screen's only effect.

### Named Rules
**The Light From Above Rule.** Every bevel agrees on one light source at the top: lit top edges and shadowed bottoms on raised parts, the reverse on recessed ones.

**The One Effect Rule.** The vignette is the only screen effect. There are no scanlines, bloom, flicker or glow halos.

## Shapes

Machined and slightly softened. The glass has 5px corners, the key caps 4px, and the LSKs and annunciator strip 3px. Those are the only radii. Key caps are rectangles about 1.4:1 (`2.9em` × `2.05em`); LSKs are low and wide (`2.6em` × `1.35em`), like a real bezel key. `EXEC` and `DEL`/`CLR` are wider variants of the same cap, never a different shape.

## Components

### Line-select keys
- **Character:** low, wide, unlabelled bezel keys. The screen tells you what they do.
- **Shape:** 3px corners, `2.6em` × `1.35em`, centred on its row.
- **States:** raised at rest; pressed flips the gradient, sinks the shadow and drops 1px. A click, a physical key (F1–F6, Shift+F1–F6), or Tab then Enter/Space all show the same pressed state for about 110ms. Focus is a 1px Entry Cyan outline, offset 1px.
- **Accessible name:** the key id plus the text painted beside it (`L1, SERVER URL, …`), or `not active`.

### Keys
- **Character:** tactile moulded caps with engraved-looking lettering (a 1px dark text shadow).
- **Variants:** alphanumeric keys in Key Legend; function keys (`MENU`, `PREV`, `NEXT`) in Legend White; `EXEC` in Avionics Green, wider (`4.2em`); `DEL` and `CLR` wide.
- **States:** the same raised, pressed and focus treatment as the LSKs.

### Screen
- **Character:** dark night-lit glass recessed into the bezel, brighter at the centre.
- **Rows:** a title row with a green hairline under it; label rows (small, Legend White, bottom-aligned); value rows (large, Avionics Green unless severity says otherwise); prompts in Legend White with their chevron pointing at their key (`<INDEX` left, `SAVE>` right).

### Scratchpad
- **Character:** the one input line, the last row of the screen, under a green hairline.
- **States:**
  - **Entry:** Legend White with an Entry Cyan block cursor.
  - **Masked entry:** `MASKED ` followed by dots; it's never announced.
  - **Error:** Fault Red, no cursor.
  - **Advisory:** Caution Amber, no cursor.
  A message is layered over the entry, and `CLR` dismisses it and restores what was typed.

### Status lines
- **Character:** one line per axis, the words first.
- **Severity:** `ok` Avionics Green, `caution` Caution Amber, `fault` Fault Red, `idle` Idle Sage, advisory Entry Cyan. The label always carries the state on its own (Monochrome Rule).

### Annunciator strip
- **Character:** a recessed strip exactly as wide as the glass, carrying the latest sidecar message on the left and the host name on the right.
- **States:** info in Idle Sage, warn in Caution Amber, error in Fault Red; the host label turns Caution Amber when the client is running on the built-in stub bridge instead of a real host. Warn and error lines are announced once each.

## Do's and Don'ts

### Do:
- **Do** size every new element from `--fmc-scale` in `em`/`ch`, and update `--fmc-unit-w`/`--fmc-unit-h` when the unit's outline changes.
- **Do** put every action on a key: a row with an LSK prompt, a key-grid key, or (in an MFD rework) a soft key at the screen edge beside its label.
- **Do** reuse the fixed colour roles, and check every new colour/surface pair against 4.5:1 on the glass, the glow centre and the strip.
- **Do** keep the screen still between states: change text in place, with no transitions, so any screenshot is a clean state.
- **Do** keep physical keyboard parity for anything a key can do, and give every key an accessible name from what is painted beside it.
- **Do** name failures on the glass (`COMMAND FAILED`, `PAGE UNAVAILABLE`) instead of leaving a row blank.

### Don't:
- **Don't** break the avionics and flight-computer theme. No web-app chrome (cards, toasts, modal dialogs, dropdown menus, spinners, hamburger menus), no system UI fonts, and no light theme inside the unit.
- **Don't** let colour carry information the words don't (Monochrome Rule).
- **Don't** add screen effects beyond the vignette (One Effect Rule), or animate anything beyond the key press and the cursor.
- **Don't** set informational text below the screen label size, `0.76em` (Label Floor Rule).
- **Don't** load a webfont or any network asset to paint the panel.
- **Don't** paint, announce or log the ingest token; a set token shows only as dots.
