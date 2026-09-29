# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

The UI is plain HTML, CSS and ES modules in `ui/`, with no build step. It runs in a Tauri 2 WebView2 window on Windows 10/11. The Tauri shell wraps a web UI and adds no native design language.

## Users

- **Now:** the author, a flight simmer who flies Microsoft Flight Simulator 2020/2024 on a Windows PC and runs their own Sabiá server on another machine.
- **Later:** other simmers running their own Sabiá server, who would install the client from the published GitHub releases. The product is built for the author first, but nothing should assume a single user who already knows the internals.

The job: fly without thinking about logging. The flight uploads itself to the logbook. Mid-flight, the pilot uses the same panel for ACARS-style messaging, the flight plan and clearances.

## Product Purpose

The Sabiá Windows client reads MSFS over SimConnect and sends flight data to the user's Sabiá server, which holds the logbook. From the same panel it also drives these server features:
- DATALINK (ACARS-style messaging, WX, loadsheets);
- a SimBrief flight-plan prefile (`FPLN`);
- a simulated PDC clearance request;
- linking a flight to SayIntentions, importing its comms and pushing the leg's PDC to the live SayIntentions session.

Success means the uplink runs unattended, recovers on its own, and says plainly when it can't, and that everything else is one or two LSK presses away.

## Positioning

The whole interface is an FMC-style CDU: a character grid, line-select keys, a scratchpad and a key grid. A logbook uploader becomes a piece of avionics the pilot operates like the rest of the cockpit. The client talks only to the user's own server. SimBrief, PDC and SayIntentions are proxied or simulated by that server, and the client never holds a third-party API key.

## Operating Context

- A desktop window on a **second monitor** beside MSFS, used with the mouse and glanced at during the flight. The keyboard also drives it.
- The window is resizable (default 560×820, minimum 544×716), and the CDU unit scales to fill it. Resizing is the zoom. The minimum keeps informational text at 11px or more, and it doesn't fit a 1080p screen at 150% display scaling.
- It runs offline on a flight-sim PC. Nothing may wait on the network to render: fonts are local-only.
- A screenshot of the panel is meant to be evidence of state, so the panel animates only a key press and the scratchpad cursor.
- The user's app often runs live under `cargo tauri dev` while it's being developed.

## Capabilities and Constraints

- **Pages:** `STATUS`, `MENU`, the CFG pages (`NETWORK`, `SIM`, `TRAFFIC`), the DATALINK pages (`DL-*`), `FPLN` and the SayIntentions pages (`DL-SI`). The full map is in `docs/cdu-reference.md`.
- **Host contract:** `ui/src/bridge.js` is the only seam between the UI and the host. Pages reach the host only through the `window.FMC` interface built by `ui/src/app.js`.
- **Vocabulary:** CDU text is uppercase avionics vocabulary: prompts as rendered (`START>`, `SAVE>`, `<INDEX`), LSKs `L1`–`L6` and `R1`–`R6`, keys `MENU`, `EXEC`, `CLR`, `PREV`, `NEXT`. Product terminology is fixed in the docs (e.g. "ingest token", "uplink", "PDC clearance" versus the SayIntentions "CPDLC" push).
- **The ingest token is a secret.** It is never painted, announced or logged: the field shows dots once set, and entry is masked only while armed.
- **Colour:** colour follows severity, but every status line must read correctly in monochrome.

## Brand Commitments

- **Name:** Sabiá, with the accent. Release asset filenames lose it. The repository is `oshogun/sabia_mcdu`.
- **Mark:** the bird, `ui/img/sabia-bird.svg`, on the unit's moulded plate beside the `SABIÁ` wordmark and the `CDU-1` model badge.
- **Identity:** an avionics flight computer is binding. The current skeuomorphic CDU (bezel, glass screen, green/cyan/amber/white avionics colours, monospace grid) is its established expression. It may evolve toward an MFD if usability gains are significant.
- **Fidelity:** inspired by real airliner CDUs, not a replica. Keep their conventions (LSK prompts, small label over large value, scratchpad, colour meanings) and Sabiá's own choices where they read better (the 50-column grid, the bird plate).

## Evidence on Hand

- User and developer documentation in `docs/` and `README.md`.
- Published GitHub releases (semver tags) with the Windows installer.
- The preview harness (`gauge/dev/`) serves the real panel against a mock host with selectable scenarios, for demos without MSFS.
- There are no testimonials, user counts or third-party endorsements. Do not invent any.

## Product Principles

1. **The panel never lies.** State shown on screen is the host's real state, and a failure says what failed. A blank or frozen panel is the worst outcome.
2. **Operate it like avionics.** New features arrive as flight-computer pages, key prompts and scratchpad messages, not as web-app widgets. The CDU is the current form. Reworking it toward an MFD is acceptable if that improves usability significantly; breaking the avionics/flight-computer theme is not.
3. **Unattended by default.** The uplink starts, retries and recovers without the pilot. The pilot is interrupted only when something needs a decision.
4. **Secrets stay in the shell.** The ingest token and any third-party key never reach the screen, announcements, logs or the client at all.
5. **Ready for users who aren't the author.** Every flow must be learnable from the screen and the docs, not from knowing the code.

## Accessibility & Inclusion

WCAG 2.x AA is the standing baseline:
- text contrast of at least 4.5:1 on every surface it is painted on;
- full keyboard operation (Tab and Enter/Space on any key, F1–F6 and Shift+F1–F6 for the LSKs);
- screen-reader names for every LSK, taken from the text beside it;
- polite announcements for messages and page changes, never for typed or masked entry;
- reduced motion respected.
