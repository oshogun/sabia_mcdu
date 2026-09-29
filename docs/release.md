# Release

Every release of this client is an annotated git tag `vX.Y.Z` on `main`, and a
GitHub Release created from that tag by CI. There is no `CHANGELOG.md`: the
tag's own message is the release notes.

## The tagging scheme

- **Annotated tags only.** The tag's **subject becomes the GitHub Release title**
  and the **rest of its message becomes the notes**.
- **Write the tag message as one title line, a blank line, then the body.** The
  title is the message's *first paragraph*, not its first line: `git`'s
  `%(contents:subject)`, which CI reads, folds a title wrapped across several
  lines into a single line — and those lines then disappear from the notes. A
  two-line title comes back as `Title that wraps onto a second line` and its
  second line is gone from the body — both the title and the notes come out
  wrong. CI catches this (it compares the subject against the message's first
  line) and fails the job rather than publishing it, so keep the title on one
  line.
- **A lightweight tag (`git tag v1.1.0` without `-a`) will not be released.** For
  a lightweight tag the ref resolves straight to the commit, so git reports the
  *commit's* subject and body as the tag's — which would title a public Release
  with whatever the head commit happened to say. CI therefore tests the ref's
  object type and **fails the job** with an error telling you to delete the tag
  and re-tag with `git tag -a`. Nothing is built and nothing is published, so
  recovery is just re-tagging. An annotated tag with an empty message fails the
  same way.
- **`vX.Y.Z`**, semantic versioning, matching the Sabiá server's scheme
  (`oshogun/sabia`, which adopted the same thing on 2026-09-26).
- Pushing the tag is what publishes the release. The `release` job in
  [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) is gated on
  `refs/tags/v*`: it checks the tag against every version field, skips if the
  Release already exists, builds the installer, creates the Release from the tag
  message and attaches the MSI and the NSIS `.exe`.
- The mechanics (writing the tag message, bumping the fields, pushing) are
  wrapped by the local `/version-release` skill. Its internals are not
  documented here, and it is not part of the repository: `.claude/` is
  gitignored, so the skill exists only in a maintainer's own checkout. Doing it
  by hand is `git tag -a vX.Y.Z` with a message, then `git push origin vX.Y.Z`.
- **Never push more than three tags in one push.** GitHub emits no workflow
  events at all for a push carrying more than three tags, so those releases
  would silently never be built. Push tags one at a time.

## Version fields

Seven version fields across six files, and CI enforces that **every one of them**
matches the tag exactly before it builds anything (`.github/workflows/ci.yml`,
step "Verify tag matches every version field"). `sidecar/package-lock.json`
accounts for two of the seven: its top-level `version` and the one on its
`packages[""]` entry. Bump them together.

| File | Field | Why it matters |
| --- | --- | --- |
| `src-tauri/Cargo.toml` | `[package].version` | The crate's version. Checked by CI. |
| `src-tauri/tauri.conf.json` | top-level `"version"` | Drives the installer filenames and the Windows product version. Checked by CI. |
| `sidecar/package.json` | `"version"` | The sidecar package. Checked by CI. |
| `sidecar/src/index.ts` | `SIDECAR_VERSION` | Runtime value: it goes out in the sidecar's `hello` message and is written into the navdata snapshot header the server stores. Checked by CI. |
| `src-tauri/Cargo.lock` | the `msfslogger-windows-client` entry's own `version` | Keeps the lockfile consistent with `Cargo.toml`; `cargo` would otherwise rewrite it mid-build. Checked by CI, which also fails the release on a stale lock. |
| `sidecar/package-lock.json` | top-level `"version"` and `packages[""].version` | Same, for npm. Both entries checked by CI. |

The root `package.json` / `package-lock.json` (`msfslogger-mcdu-dev`) are
**intentionally unversioned** and must stay that way. That manifest is the
private dev harness — the gauge preview server, the boundary check, the contract
check — it is never shipped and it deliberately has no `version` key. Do not add
one, and do not check it in CI.

## What MAJOR, MINOR and PATCH mean here

This client's public surface is the installer and the installed app's behaviour,
the config file at `%APPDATA%\msfslogger\config.json`, the CDU's pages, and the
server API versions it requires. Against that surface:

- **MAJOR** — something that worked stops working unless the user changes
  something. A config key removed or renamed; an install that is not an upgrade
  in place (as 1.0.0 is — see "Upgrading from an msfslogger installation"
  below); dropping a sim connection or a CDU feature; requiring a newer server
  MAJOR.
- **MINOR** — new behaviour that takes nothing away: a new CDU page or feature, a
  new config key that is additive and optional.
- **PATCH** — fixes, docs, CI, refactors. Nothing new on the surface above.

## Server compatibility

Every Release states which Sabiá server versions it works with.

**1.0.0 requires server >= 1.0.0.** That is the first server release without the
Node SimConnect agent, which makes this Windows client the only way to connect a
simulator to it.

The version floor is a deliberate statement in the notes, not an enforced
handshake. The actual runtime mechanism is capability-based: the sidecar
advertises a `features` list in its `hello` (`datalink`, `simbrief-prefile`,
`pdc-clearance`, …) and the Tauri shell gates on that list, never on a version
string. So an older server degrades to an unavailable CDU page (`SIDECAR UPDATE
REQUIRED`, or the feature's page reporting it is not offered) rather than failing
hard. Read the floor as "this is the pairing we tested and support", and the
feature list as "this is what actually happens if you don't".

**Cross-repo protocol.** When a server MAJOR breaks an API this client uses —
ingest, ACARS/datalink, navdata, ground-session, prefile — the server side
announces the exact break before tagging, and this side does the same when a
client release needs a newer server. Neither repo discovers the other's break
from a failing flight.

## Release build steps

From the repository root:

```powershell
cargo tauri build
```

This runs `beforeBuildCommand` (`npm --prefix sidecar run build`) first, then
produces an MSI and an NSIS `.exe` under
`src-tauri/target/release/bundle/`. Both artefact names derive from
`tauri.conf.json`'s `productName` (`Sabiá`) and `version`, e.g.
`Sabiá_1.0.0_x64_en-US.msi` under `bundle/msi/` and
`Sabiá_1.0.0_x64-setup.exe` under `bundle/nsis/`. The names are non-ASCII;
scripts that pick them up should glob, not string-build them.

**The published Release assets are named without the accent** —
`Sabia_1.0.0_x64_en-US.msi` and `Sabia_1.0.0_x64-setup.exe`. GitHub normalises
the non-ASCII `á` away when it stores an asset, so the file a user downloads is
not named the same as the file the build produced. Verified on v1.0.0: the
release job logged `collected Sabiá_1.0.0_x64_en-US.msi (38.5 MB)` and published
`Sabia_1.0.0_x64_en-US.msi`. Neither name is wrong; do not "fix" one to match
the other.

The bundle embeds `sidecar/dist`, `sidecar/node_modules` and
`sidecar/package.json` as resources — whatever is on disk in those
directories at build time is exactly what ships. **Install the sidecar's
production dependencies before building** (`npm --prefix sidecar ci` or
`npm --prefix sidecar install`, not a partial or `--omit=dev` state you
haven't verified); there is no separate "prune dev dependencies for the
bundle" step.

That install **takes the `node-v137` prebuild**: `better-sqlite3` publishes a
prebuilt binary for Node 24's ABI (module version 137), so `npm ci` fetches it
directly and needs no C++ toolchain or Python. It still has to match the Node
the sidecar runs under — a prebuild for the wrong ABI fails to load with a
recognizable `ERR_DLOPEN_FAILED` message, which the sidecar reports as
`STATUS` showing `NODE 24 REQD FOR NAVDATA` rather than a crash (any other
load failure is `NAVDATA DRIVER FAULT` — see
[cdu-reference](cdu-reference.md) and [troubleshooting](troubleshooting.md)).
The sidecar loads the SQLite driver lazily and fails soft, so a build
that quietly did not produce a working binding ships an installer with
navdata silently dead rather than visibly broken — always confirm
`node -e "require('better-sqlite3')"` from `sidecar/` before trusting one. CI
asserts exactly that before it bundles anything.

CI does exactly this on `windows-latest` for a tag push, and attaches both
files to the Release (and to the workflow run as an artifact, so a failed
release step doesn't throw the build away).

Every installer this project produces is unsigned — no code-signing
certificate is configured, by design, not as an unfinished step. Windows
SmartScreen will warn on first run of an installer built this way; that's
expected, including for the installers CI attaches to a Release.

## Pre-release checklist

CI (`.github/workflows/ci.yml`) runs the automated checks on every push and
pull request: sidecar typecheck and unit tests, `cargo fmt --check`,
`cargo test`, `contract-check.mjs`, `check:ui`, `test:gauge`. Confirm the run on
the release commit is green rather than re-running them by hand. Two UI checks
are *not* in CI by design — `ui/tools/render-check.mjs` and
`ui/tools/host-swap-check.mjs` need puppeteer, which this project does not
install; treat them as unverified.

What remains is human:

1. All seven version fields bumped and agreeing (CI fails the tag push if not).
2. **An in-sim smoke test on a real Windows box with MSFS**: install the built
   installer, configure `CFG NETWORK`, confirm the Sim axis reaches
   `SIM LINK ONLINE` and the Backend axis reaches `ACARS UPLINK` against a real
   server. **In-sim acceptance is manual and cannot be automated here** — no CI
   runner has a simulator, so nothing in this repository proves it.
3. **Server compatibility confirmed deliberately**: check the target server's
   version against the floor this release declares, and state that floor in the
   tag message.
4. The tag message written as release notes, and the tag created with `git tag -a`:
   **one** title line, a blank line, then the body. CI rejects a lightweight tag,
   an empty tag message, and a title that runs onto a second line — see "The
   tagging scheme" above.

## Release history

The version history this scheme assigns to the existing commits. **None of these
tags exist yet** — `git tag` in this repository still returns nothing, and no
Release has been published. This table is the intended tag set, to be created as
part of adopting the scheme; treat a row as real only once `git ls-remote --tags
origin` shows it.

| Tag | Commit | Theme |
| --- | --- | --- |
| v0.1.0 | a79a8bc | First Windows CDU client and the MSFS toolbar gauge |
| v0.2.0 | 6bee46d | Gauge removed; the client is the whole surface |
| v0.3.0 | 57fe3e7 | DATALINK: SimBrief prefile and simulated PDC clearance |
| v0.4.0 | e303825 | SayIntentions support, and docs brought in line with it |
| v0.5.0 | f3bd839 | Navdata snapshots with bulk facts in the header |
| v0.6.0 | c87b5bd | Navdata on demand: fixes, VORs and NDBs the server asks for |
| v1.0.0 | current `main` | Sabiá rebrand, semver releases and CI; requires server >= 1.0.0 |

v0.1.0 through v0.6.0 are **retroactive**: they name commits that shipped before
this scheme existed, and they are being applied after the fact to give the pre-1.0
history the same shape as the server's. Each of those commits has a tree with no
`.github/workflows/`, so pushing its tag starts **no** workflow run at all and the
`release` job can never publish it. That is what
[`.github/workflows/backfill-releases.yml`](../.github/workflows/backfill-releases.yml)
is for: run it manually from `main` (it defaults to `dry_run: true`, so run it
once to read the plan and again with `dry_run: false` to publish). Backfilled
Releases carry notes only — no installer is attached, because those old trees are
not rebuilt — and the workflow pins `--latest=false` so backfilling an old tag
cannot steal the "Latest release" badge from v1.0.0.

## Upgrading from an msfslogger installation

The bundle identifier changed from `com.msfslogger.windows-client` to
`com.sabia.windows-client` as part of the Sabiá rebrand, and `productName`
changed from `msfslogger` to `Sabiá`. Windows treats this as a different
application, not an upgrade in place: installing the Sabiá build does not
remove or replace an existing msfslogger installation, the old entry stays in
Add/Remove Programs, and the new build gets its own WebView2 data directory.
Uninstall the old `msfslogger` entry by hand if you no longer need it. The
config file at `%APPDATA%\msfslogger\config.json` is unchanged by the rebrand,
so the existing server URL, ingest token and other settings carry over to the
new install automatically.

## Installed-app runtime requirement

The installed app is not self-contained: the shell spawns the sidecar by
running `node` (or the executable at the config file's `nodePath`, if set)
as a child process — the bundle ships the sidecar's JavaScript and
`node_modules`, not a Node runtime. **Node 24 must be on the installed
machine's `PATH`, or `nodePath` must point at one**, or the app will fail to
launch the sidecar (`STATUS` reads a launch-failure state naming the
problem — see [troubleshooting](troubleshooting.md)). This applies to every
installed copy, not just dev machines.

A `node` that is present but not Node 24 doesn't fail to launch: the sidecar
still starts, but the bundled SQLite driver was built for Node 24's ABI, so it
fails to load and navdata is disabled. `STATUS` shows `NODE 24 REQD FOR
NAVDATA` beside the SIDECAR line rather than a launch failure — see
[cdu-reference](cdu-reference.md) and [troubleshooting](troubleshooting.md).
