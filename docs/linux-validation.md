# Linux validation

The default local-validation policy uses native macOS checks and remote Linux
runners. For this audit, the user explicitly authorized a local Linux desktop
on 2026-10-18 because no independent Linux machine is available. This task-specific
authorization supersedes the earlier local-VM exclusion; it does not establish a
new permanent repository policy. The final requested environment uses Apple Container with an XFCE X11 desktop
served through VNC/noVNC. Record container results separately from native
macOS and GitHub Actions results.

## Native CLI discovery and credential audit, 2026-10-20

`scripts/verify-linux-native-clis.mjs <binary-directory> [official-cli-bin-directory]`
runs the production Agent and CLI with a disposable HOME/vault and a desktop PATH
containing only `/usr/bin:/bin`. Synthetic credentials exercise import/dedup,
Codex expired/malformed/unreadable/missing credentials, recovery, provider isolation,
Unix execute permissions and plaintext leakage checks. It also checks custom
XDG discovery for Cursor and Kimi Work. Devin TOML credentials, three distinct
Kiro SQLite identities and OpenCode Go/Zen are imported from custom XDG data
directories. No login service or real vendor account is used.
The packaged-runtime CI invokes this verifier against extracted deb helpers.

The native discovery implementation is shared by detection, login and renewal.
It covers user/system PATH, bounded login-shell PATH, native/user installs,
npm prefixes, pnpm, Cargo, Bun, Volta, asdf, mise, proto, nvm and fnm. It rejects
relative search paths and files lacking Unix execute permission. Vendor child
processes retain explicit custom PATH order and prefer discovered user runtimes
over standard system bins, so an old distro Node cannot mask a user Node install.
Custom absolute XDG and vendor homes are shared by discovery and credential reads.

`dbus-run-session -- node scripts/verify-linux-keyring.mjs` starts a foreground,
disposable GNOME keyring under a private HOME and bus. Build the Agent and
config-writer library tests first. It checks actual Secret Service transport,
Codex Rust-keyring attributes, keytar schema separation, account isolation,
read/update/delete, bounded failure, locked-store behavior and Codex auto/file
fallback. A separate Agent test checks Copilot keyring precedence, token rotation
and file fallback. Reads never unlock a collection or request an authorization
prompt. Explicit Codex keyring mode never silently downgrades to a file; native
credential writes still require readable transaction preimages.

The authorized Ubuntu 24.04 ARM64 environment passed both store tests and the
production-process verifier using official Claude Code 2.1.303, Codex 0.163.0,
Gemini CLI 0.64.0 and Copilot 1.0.100. Grok's executable/capabilities and credential
format use a deterministic fixture. All grants are synthetic; official CLIs run
only version/help and Claude local authentication-status commands. Official
Cursor 2026.10.18-ea085cb also passed version/help; its shipped auth-path code
confirms the Linux XDG config path. Official Kiro 2.32.0, under an empty private
HOME, created its `auth_kv` SQLite database at the custom XDG data path and
reported no logged-in account. Its three credential key names match the reader.
These results
do not establish real vendor login, refresh or inference, Linux x86_64 runtime,
every historical installation layout, or protected credential modes beyond the
tested stores. Gemini's opt-in `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE=true` backend
and Codex encrypted-secrets backend are explicitly unsupported, rather than
importing a stale plaintext fallback as an active account.

Evidence is retained under
`/Volumes/BRData/containers/aipass-linux/share/native-cli-audit`: source manifest,
Linux test/build/clippy logs, official CLI import report and keyring test output.
Native macOS full Rust workspace tests passed serially (1045 passed, 4 ignored),
with unchanged assertions, after parallel runs exposed intermittent sync deadlines
and unsolicited local discovery traffic in HTTP fixtures. Those failure logs are
retained. Full workspace Clippy, repository health and dependency-license checks
also passed. Linux Agent/config-writer tests passed (473 passed, 5 ignored), with
the two private-keyring tests run explicitly and passing separately.
The local backend audit used current debug executables; it did not replace the
installed desktop package or validate a newly finished release artifact.
These working-tree changes have not been pushed or released;
the new CI checks still require a remote run after a future push.

## Window, UI and updater audit, 2026-10-20

The authorized XFCE/X11 desktop was used again with fresh release-profile
Deb/AppImage/RPM artifacts. Updater checks use an isolated signing key and loopback
feed; the production key and signature verification remain unchanged. No real
vendor credential or host vault is mounted. This is ARM64 software-rendered X11
evidence, not a general certification of every Linux desktop/GPU/display server.

The minimum-window walkthrough covers onboarding/create/recovery acknowledgment,
unlock, provider creation and inline editing, dynamic fields, credential editing,
pricing/subscription dialogs, account-provider selection, search, favorites,
archive/trash empty states, Agent detection/credential selection/preview/apply,
proxy group creation and listener editing, and all six Settings tabs. Both UI
languages and light/dark themes were inspected. Actual official Codex, Claude Code
and Gemini CLI installs appear in Agent management; a synthetic Claude credential
was previewed and written under the disposable HOME. Vendor OAuth completion and
live subscription use remain separate external-service tests.

Confirmed window checks include native edge resize, minimum-size constraints,
title dragging, a normal-interval double-click, maximize/restore, minimize/restore,
close-to-tray, singleton reopen and native tray hide/reopen. A maximized window
fits the 1280x773 work area above the XFCE panel. The WebView compositor dataset
tracks real XFWM compositing toggles; the window uses transparent rounded corners
when composited and square edges when maximized or uncomposited. Window-button
and detail-action right edges share the same inset.

Runtime fixes from this walkthrough:

- Enable RGBA Linux windows, follow compositor state and align window buttons
  and Agent headers without moving the app title out of the middle column.
- Keep Chinese segmented labels on one line and bind native select colors and
  `color-scheme` to the chosen theme. Empty selects show an explicit empty state.
- Close the restart confirmation after a completed attempt and prevent repeated
  confirmation while installation is busy. Keep update failures above the
  available-update status so they remain visible at 960x640. Extract the downloaded-update notification into a compact opaque
  surface whose text can wrap without colliding with its actions.
- Require exact architecture/installer feed entries, verify bytes reread from
  cache, and preflight AppImage write access. System packages use one pkexec
  operation with no password collection or fallback after cancellation. Linux
  runtime shutdown follows successful installation, preserving the existing
  Agent/proxy on authorization failure. Cached system packages never request
  authorization merely because the app started.

Signed AppImage checks have demonstrated both same-version replacement and a
real beta.1 to beta.2 process/version transition. A Deb authorization failure was
also executed against the real pkexec/Polkit service: the app and Agent remained
responsive with the original executable and one Agent process. The current
container has no logind login session, so its graphical Polkit agent cannot
register. Authorization cancellation exit codes are covered by Rust tests;
normal graphical password approval/cancellation and privileged package update
with automatic restart require a full Linux login environment. The final RPM
also passed an actual installation into a disposable RPM root; that package
format check does not substitute for graphical authorization.

Linux release preparation now builds both architectures and all three installer
formats, keeps their signatures distinct, merges their feed entries, and rejects
missing or mismatched fragments before publication. These workflow changes are
unpublished. At the time of this audit the public beta/nightly feeds still contain
only macOS entries; local loopback success must not be described as a deployed
Linux update service or a completed remote CI run.

Evidence, screenshots, source manifests, failed-attempt diagnostics and finished
artifact reports are retained under
`/Volumes/BRData/containers/aipass-linux/share/ui-audit`. Native macOS checks and
its finished DMG/updater runtime reports are recorded separately. Rebuilt artifacts
are revalidated; a prior artifact's hash/result is never reused as the new one.

## Implementation review, 2026-10-20

The review covers every current tracked/untracked change in CLI discovery,
credential stores/readers, desktop chrome/components, updater/runtime diagnostics,
package gates, release manifests/workflows, dependencies and embedded UI output.
Final vault/config authority remains in Rust; the interactive verifier only writes
read-only snapshots inside its disposable runtime fixture. Production updater
signature verification and the configured production key remain enabled.

Three functional findings were repaired:

1. Child PATH ordering put the discovered executable's system directory ahead of
   user Node runtimes after sorting only the fallback directories. Partition the
   complete child PATH after adding executable parents. A regression first failed
   with `/usr/bin` before the fixture's Node 26 directory, then passed after repair.
2. GUI detection accepted unrelated `agent`/`grok` executables while vendor login
   applied stricter identity/path rules. Share vendor-aware candidates between
   detection and login. Preserve Cursor's known legacy alias and explicit absolute
   `GROK_BIN_DIR`, reject relative overrides and unrelated names, and test all of
   these cases with executable files/symlinks.
3. A complete signed package in a read-only cache bypassed download writes. The
   old AppImage really installed/restarted and left a pending package, failing the
   new regression. Remove the verified download cache before replacing the app or
   stopping runtime processes; retain a final cleanup for background downloads
   completed during authorization. Pre-install cleanup failure returns the localized permission
   error with the current process/Agent intact. Failed later installation may
   require a fresh download; the expended cache is not user vault data.

A full native macOS Rust run also exposed a device-OAuth test fixture consuming
an unexpected GET as a token POST. Route unrelated paths to 404 without advancing
its pending/success response sequence. The regression now deliberately sends a
root GET first; its POST, device code, verifier and rotated-token assertions stay
in place. The initial failure and intermediate diagnostics are retained.
A later no-replay proxy test failed on an unidentified fallback TCP connection;
the old assertion did not capture its request, so that failure's exact cause is
not established. Its fixtures now reuse the shared probe classifier on primary
and fallback listeners. Only unauthenticated root GETs are ignored; a dedicated
regression proves API POSTs, authenticated root GETs and other routes still fail
the no-replay guard. No production retry policy is changed.

Updater runtime coverage adds tampered-cache and populated read-only-cache cases: modified cached bytes
must be discarded, the signed package downloaded again, and a new process must
run with the exact verified artifact hash. Release-manifest failure tests also
require a late missing/version/format failure to preserve both the original
manifest and unconsumed fragments. The existing Wayland, full Polkit/login-session,
physical GPU/FUSE and live vendor-account limits still apply.

Final review validation:

- Native macOS Rust workspace: 1048 passed, 4 ignored; full fmt, Clippy with
  warnings denied and workspace build passed. After the final cache-ordering
  change, full fmt/Clippy/workspace build and all 67 desktop Rust tests passed again.
- Node tests passed all seven tasks, including the unchanged 379 desktop tests;
  the expanded release-manifest failure tests passed. Previously passing
  frontend lint/typecheck/build results retain matching source/dependency inputs;
  both final desktop bundle builds also rebuilt the frontend. Embedded control
  panel hashes and lockfiles remained unchanged during this review.
- Linux: 404 Agent tests, 71 final config-writer tests and 241 final proxy tests
  passed (5 ignored across Agent/writers); the device-route regression also
  passed separately after its repair. Final desktop tests/Clippy passed. Official CLI
  capability probes plus synthetic credential import/dedup/failure isolation
  passed; these do not establish real vendor OAuth/renewal/inference.
- Final Deb and RPM passed main/tray, CLI/native imports and persistent helper
  checks. All three signatures passed. Both final system-package authorization
  failures preserved their original desktop process and responsive Agent.
- The final AppImage passed all nine cases: explicit install, cached install,
  tampered-cache recovery, read-only image directory, empty read-only cache,
  populated read-only cache, bad signature, signed wrong format and failed feed.
  Final macOS DMG/updater passed all four startup/install/restart scenarios.

The final Linux packages are in `/home/aipass/ui-review-v3-artifacts`; their hashes
are in `share/ui-audit/review-v3-artifact-hashes.json`. Reports are in
`review-v3-linux-runtime-reports.tar.gz` and `review-macos-v3-runtime/`. Source
comparison matched all 615 runtime-source files. Durable review logs/source
identities are in `review-macos-and-source-evidence/`; original failures and prior
artifact hashes remain available. The readonly-cache test initially used an
extra no-download assertion that also counted a separate background channel
check; the final complete nine-case gate instead requires the permission error,
unchanged executable/process/Agent and intact signed-artifact checks.

The current source ref is `95f6f8ac8e838710f8ae55ea42cbfa2af0f18d49` plus this
reviewed working tree. No commit, push, publication or new remote CI run is
performed by this review. Prior v15 hashes below identify earlier audit artifacts.

## Final artifact results, 2026-10-20

The final ARM64 artifacts are frozen in `/home/aipass/ui-v15-artifacts` inside
`aipass-linux-desktop`. Their SHA-256 values are:

| Package | SHA-256 |
| --- | --- |
| Deb | `71d407f19e3ce29136c12a3a4561202cddea9a8acb03afc56b8e3a6bff239f25` |
| RPM | `3385c2ccf6c027f2d77d9367715a0e49c2275ff4dc518ad2cf8d09e453a4f658` |
| AppImage | `83f37e5faeb98c1fff672e1a30f4760d2c8090a198934fa4004b39d32642e27b` |

All three detached updater signatures passed. Both Deb and RPM passed main/tray
startup, packaged CLI operations, synthetic native-CLI credential imports and
persistent helper byte/permission checks. The final AppImage passed all seven
signed updater cases: explicit install, cached install, read-only installation,
read-only cache, invalid signature, signed wrong format and unavailable feed.
A separate earlier fixture demonstrated beta.1 to beta.2; the final seven-case
gate reinstalls the same version to test its own exact bytes and new-process
restart. AppImage validation also rejects truncated and wrong-architecture ELF
images before installation.

Normal AppImage startup without `--verify-runtime` or renderer environment
overrides also passed. It fit the 1280x773 work area, resized to 1100x700,
maximized/restored, persisted its dimensions on close-to-tray, and restored
1100x700 on a fresh process. The Xvnc desktop still uses software rendering.
Real Deb and RPM authorization failures preserved the current process and Agent;
a separate visible Deb attempt preserved the unlocked vault and running proxy.

The final runtime source manifest matches all 615 files in the Linux checkout.
The 67 Linux desktop Rust tests and Clippy passed; native macOS full Rust results
remain 1045 passed/4 ignored, and frontend validation passed all seven Node tasks,
including 379 desktop tests. The final macOS UI build passed all four finished
DMG/updater scenarios. Its subsequent source change is Linux-only architecture
validation, which does not alter the macOS compiled input.

Within `share/ui-audit`, see `final-linux-artifact-hashes.json`,
`final-runtime-source-comparison.json`, `final-linux-runtime-reports.tar.gz`,
`normal-resize-result.json`, `live-proxy-permission.json`,
`rpm-permission-result.json`, and `macos-v14-runtime/`. Screenshots include
`final-v15-detail.png`, `final-v15-agent.png`, `v14-notification.png` and
`v14-permission-error.png`. Failed attempts and their diagnostics remain alongside
the passing records. This audit has not pushed these changes or executed their
new remote workflows. Workflow syntax passes actionlint with the existing
SC2129 redirect-style finding excluded; no functional workflow error was found.

## Automated coverage

`.github/workflows/ci.yml` runs the complete Rust and Node checks on Ubuntu.
The additional `Linux packaged runtime` job builds signed Deb and AppImage test
packages with a disposable updater key. It extracts the Deb into an isolated fixture. Under DBus and Xvfb it starts the real Tauri
WebKitGTK application in both main and tray modes at 960×640. The application
requires rendered frontend completion, a responsive native event loop and typed
authenticated Agent IPC for ten seconds, followed by successful process exit.
The verifier checks the running version against the deb metadata, records the
artifact SHA-256 and uploads runtime reports and logs even after a failure.

The packaged CLI smoke uses a disposable HOME, vault, settings, runtime and log
directory, with no Agent binary override or vendor credentials. It checks vault
creation, provider CRUD, full-key search without list leakage, reveal, encrypted
backup, local folder sync, archive/restore, lock/unlock, Native Host discovery and
an actual framed Native Messaging ping. It scans vault, sync, backup and logs for
the fixed fake key. It starts Agent explicitly so tests do not install a login
service. Standalone macOS/Linux release staging runs this same check before
uploading the archive; all three required executables must be present.

Remote Linux commands:

```sh
pnpm --dir apps/desktop tauri build --ci --bundles deb
dbus-run-session -- xvfb-run -a -s '-screen 0 1280x800x24' \
  node scripts/verify-linux-runtime.mjs
```

The desktop fixture accepts only its extracted executable beneath
`/tmp/aipass-runtime-*/package/usr/bin/aipass-desktop`. It isolates user data,
disables browser/login registration and uses normal Agent discovery. AppImage fixtures additionally require a private HOME and a real AppImage path
inside the fixture; extracted AppDirs remain temporary. Linux manual/cached update
modes retain normal signature verification and require a new process, expected
running version and cleared cache. `scripts/verify-linux-updater.mjs` exercises
signed installation, cached installation, read-only installation/cache locations,
bad signatures, a signed wrong-format package and an unavailable feed. It checks
installed hashes and verifies failed installs do not restart the Agent.
The macOS updater gate retains its four runtime scenarios.

## Local interactive desktop

The task-specific test environment is a named Apple Container
`aipass-linux-desktop`, using the official Ubuntu 24.04 ARM64 image, 8 CPUs,
12 GiB RAM and a 1 GiB shared-memory mount. XFCE runs in TigerVNC on X11 at
1280×800. noVNC exposes the same interactive session in the host browser.
The VNC and HTTP port mappings bind to `127.0.0.1` only; VNC requires a
disposable test password. No host vault, browser profile or credentials are
mounted. Source is transferred as a snapshot with per-file SHA-256 hashes.

The desktop has been connected through noVNC. AIPass was installed from its
finished deb, operated with mouse and keyboard, and resized to its 960×640
minimum. The session uses a stable user DBus socket so an existing Agent can
reconnect after the X11 session is restarted. A regular container does not
provide a complete systemd login session, so its results cannot establish
systemd user-service startup after a real machine login.

## Remaining acceptance limits

The local X11 results below do not establish:

- Wayland, GNOME/KDE-specific rendering, a physical GPU, or physical suspend/resume
- systemd user-service startup after a real machine login
- Browser extension store installation and browser-initiated Native Messaging
- FUSE-mounted AppImage startup (the container uses standard extract-and-run)
- Full desktop Polkit password dialogs and successful Deb/RPM privileged
  install-and-restart under a real login session
- Live vendor OAuth/subscription calls or authenticated remote WebDAV services

iCloud Drive/CloudKit and macOS biometric APIs are platform capabilities, rather
than Linux parity targets. Linux supports encrypted local-folder and WebDAV sync;
their unit/integration coverage must not be reported as a live remote-service test.

Linux desktop CLI installation uses `~/.local/bin/aipass` and refreshes Agent and
Native Host under `~/.local/lib/AIPass`. AppImage helper discovery prefers these
persistent copies so browser manifests and later CLI invocations do not depend
on an expired mount. Executable replacement uses atomic rename to preserve a
running helper's inode. A native regression test removes the fixture mount and
checks replacement content, executable permissions and the old open inode.

## Audit evidence, 2026-10-18

The existing CI at commit `1e4bff9028af7193e2402c4980d096c445126b0b` passed
[Ubuntu Rust/Node checks](https://github.com/backrunner/aipass/actions/runs/39045759995).
The [nightly release](https://github.com/backrunner/aipass/actions/runs/39046187929)
compiled the Linux CLI, but did not build or run a Linux desktop artifact.
These runs predate the working-tree Linux fixes and new runtime job and cannot
validate them. Record the new remote run URL and revision after execution.

## Local desktop results, 2026-10-18/19

The final Ubuntu 24.04 ARM64 deb is `AIPass_0.2.0-beta.1_arm64.deb`, SHA-256
`c9daadfa37876e78573f1f9a62a4802927826730f6902d038423fe2c8e2ab519`.
It was installed with dpkg and launched with its normal defaults, without the
runtime verifier's software-rendering environment overrides. The finished
artifact also passed the isolated verifier in both main and tray modes.

Completed checks:

- Linux: full Rust fmt, clippy with warnings denied, 1040 passing workspace tests
  (3 ignored), and workspace build. macOS: 1045 passing Rust tests (4 ignored)
  and full workspace clippy/build; platform-specific counts differ. Native
  macOS Node lint/typecheck/tests/build and the current CLI process smoke also
  passed.
- Linux: license audit, repository health, lint, typecheck and all seven Node
  test tasks, including 367 desktop tests. Frontend and deb builds succeeded.
- Packaged process checks: CLI CRUD/search/reveal, encrypted backup, folder sync,
  archive/restore, lock/unlock, framed Native Messaging ping and plaintext scan;
  main/tray frontend completion, native event loop, authenticated Agent IPC and
  package-version matching at 960×640.
- Installed X11 UI: provider creation and list/detail refresh, secret reveal/hide,
  native GTK clipboard contents, settings/form scrolling at the minimum size,
  Chinese/English and light/dark presentation, native tray menus and tray lock.
- GUI-started proxy: invalid authentication rejected with 401, authenticated
  non-streaming and SSE requests forwarded through a local fixture upstream,
  response content and usage statistics verified; GUI stop closed its socket.
- GUI local-folder sync: settings persisted by Agent, encrypted objects created,
  and no plaintext fixture key found in the sync destination.
- Login-session behavior: restarting the X11 session automatically relaunched
  the tray through its XDG desktop entry; ending the session bus locked the
  existing Agent. That same PID reconnected to the restarted user bus and locked
  again on a later screensaver event. Password unlock restored access.
- Power integration: installed Agent reacted to a controlled screensaver service
  and a controlled logind `PrepareForSleep(true)` signal. Five private-DBus
  regression tests cover sender filtering, own/other UID sessions, policy
  switches, idle-time units and delay-inhibitor descriptor lifetime. These are
  protocol/runtime checks, not a physical suspend/resume result.

Additional fixes found through the visible desktop:

1. Cap restored/default windows to the current monitor work area in logical
   pixels. A 1280×820 default had clipped bottom navigation on 1280×800 X11.
   The tray reveal path also caps the current size before showing the window;
   first reveal fit 1280×773 and hide/open preserved the user-resized dimensions.
2. Omit unsupported CloudKit choices on non-Apple builds, block an existing
   unsupported selection, and retain local/OneDrive/WebDAV controls.
3. Reconstruct GTK tray menus on locale changes because the menu library's
   custom submenu labels did not update with `set_text`.
4. Replace the Linux power-watcher stub with native DBus monitoring, respect
   current sleep/screen-lock policies, lock on desktop-bus loss, reconnect, and
   hold a logind delay inhibitor until the vault is locked.

A full macOS run exposed an intermittent proxy fixture failure. The shared mock
upstream accepted unauthenticated `GET /` discovery probes as the expected API
request. Its extracted helper now skips only those probes; regression tests
verify that actual API requests and authenticated root requests remain accepted.
The failing 502 did not retain the upstream request, so its exact cause is not
proven. The regression tests establish the fixture defect; both complete Rust
suites passed after this correction.

Local evidence is retained outside the checkout under
`/Volumes/BRData/containers/aipass-linux/evidence`: artifact copies, SHA-256 source
manifest/archive, command logs, JSON runtime reports and actual noVNC screenshots.
`/Volumes/BRData/containers/aipass-linux/README.md` records connection/restart
instructions. The temporary DBus signal services are test fixtures; their results
must not be described as real logind hardware suspension or a real OS login test.
A final macOS parity run also exposed absent-Keychain reads racing global
cache invalidation from unrelated vault locks. An attributes-only absence now
erases cached bytes and returns without launching an authorization worker.
Attribute/ACL errors remain failures. The new concurrent-invalidation regression
and both full Rust suites passed.

Installed desktop, Agent, CLI and Native Host hashes were compared against the
finished deb; the running desktop/Agent and persistent helper copies matched.
The working tree has not been committed, pushed or released by this audit.

## Historical native Linux window chrome trial, 2026-10-19

This trial was validated, then superseded by the custom-header revision below
after the user rejected native decoration appearance.

The installed Linux desktop now uses opaque native window decorations. XFCE or
another window manager owns button placement, theme, window dragging and borders.
Linux omits the web Windows-style window buttons and web corner mask. The web
header receives pointer input only in its app-menu row, leaving detail actions
uncovered. Native decoration is configured before window creation for packaged,
development and tray-first launches.

GTK frame extents become available after mapping. The window geometry handler
therefore refits non-maximized windows on native resize/scale events. In the
1280×800 XFCE session, the work area is 1280×773 below the 27-pixel desktop panel.
The restored full-size client fits at 1270×739 with frame extents 5/5/29/5,
producing an outer 1280×773 window. A user-resized 960×640 client remains 960×640
after maximize/restore, minimize/restore and close/reopen from the tray.

Mouse interaction over noVNC verified the native minimize, maximize/restore and
close buttons. Minimize produced the X11 Iconic/Hidden state; the taskbar restored
the window. Close hid the window while its tray remained available, and the tray
reopened it. At the 960×640 content minimum, provider edit/cancel and the app menu
worked without touching or overlapping the native titlebar controls.

The final installed deb is retained as
`/Volumes/BRData/containers/aipass-linux/evidence/AIPass_0.2.0-beta.1_arm64_native-chrome.deb`.
Its SHA-256 is
`c917a04e3b9e37daaff2cf465c56940c6526143d52ad2166ac86451000cced05`.
The running desktop executable matches the installed binary hash. The final
package passed both main/tray checks at 960×640, frontend/event-loop/Agent IPC and
both CLI/Native Messaging smoke runs. These checks used the actual X11 session;
Xvfb is not installed in the local container. The configured remote CI job uses
Xvfb.

This follow-up also passed native macOS desktop Clippy with warnings denied,
70 desktop Rust tests, desktop typechecking, 367 frontend tests, frontend build,
repository structure checks, Rust formatting and diff whitespace checks. The full
workspace counts in the earlier audit are historical; this UI follow-up reran the
affected desktop checks and the final Linux package runtime checks. It does not
extend the Wayland, physical suspend, live-service or update validation scope.

Evidence is retained under the external `evidence/` directory:
`runtime-native-chrome-final.log`, `guest/runtime-native-chrome-final/`,
`native-minimized.txt`, `native-chrome-final-identity.txt`,
`aipass-linux-native-chrome.png` and `native-chrome-source-manifest.json`.

## Historical full-width Linux header trial, 2026-10-19

This full-width header was superseded by the column-aligned layout below.

The user requested replacing the native titlebar trial with custom chrome.
Linux now uses an opaque undecorated window and a themed 40-pixel header in
normal layout flow. The header contains the app brand, menu and three circular
window buttons; maximize switches to a restore icon when appropriate. It cannot
overlap detail actions because the workspace occupies the remaining height.
The sidebar/list top inset is reduced from 42 to 12 pixels to reclaim space.
The minimum client viewport remains 960×640.

Tauri owns Linux drag-region movement and double-click maximization. The page
must not toggle maximize again on the same double-click. macOS styling and
window behavior retain their existing implementation.

The final package passed main/tray startup at 960×640, frontend/event-loop/Agent
IPC checks and both CLI/Native Messaging smoke runs. Real noVNC mouse input
verified the minimize button (X11 Iconic/Hidden), maximize button (1280×773),
restore (960×640), taskbar restore, drag, close-to-tray, tray reopening, provider
edit/cancel and the application menu. The final viewport has zero native frame
extents, with page actions below the 40-pixel header. Titlebar double-click did
not produce a confirmed maximize state through the current noVNC automation;
this interaction is not counted as verified. The page avoids a second maximize
toggle because Tauri already owns Linux drag-region double-click handling.

The final artifact is
`/Volumes/BRData/containers/aipass-linux/evidence/AIPass_0.2.0-beta.1_arm64_custom-chrome.deb`,
SHA-256 `cb8de80c406c5fd1e0a60384e6b72062aa9435a8bcc15277b6c7af211074c9f0`.
The installed/live desktop identity is checked against this package. Reports are
in `runtime-custom-chrome-final.log` and `guest/runtime-custom-chrome-final/`;
`aipass-linux-custom-chrome.png` shows the final 960×640 running application.

Focused native macOS validation passed desktop Clippy with warnings denied,
70 desktop Rust tests, desktop typechecking, 367 frontend tests and repository
checks. The final Linux package build and runtime checks cover the Linux-target
frontend and Rust shell. Earlier complete workspace results and platform limits
remain historical evidence; no new remote CI, publication or signed-update result
is implied by this UI change.

## Current column-aligned Linux chrome, 2026-10-19

The app title/menu remain in the middle list column; custom window buttons remain
in the right detail column. The sidebar uses a normal 8-pixel top inset rather
than a window-title placeholder. The detail identity shares the first row with
window buttons; metadata and edit/save actions occupy the second row. No separate
full-window header or empty 34-pixel detail strip is added.

`linux-window.scss` owns the Linux layout. One shared `--linux-header-inset: 8px`
keeps the sidebar, middle title/menu, detail identity and window controls away
from pane edges. The list toolbar inset grows by the same amount to preserve
spacing below its app title. Provider view/edit and local-proxy headers share the
compact row layout; the spanning Agent page retains its own app-title row.
The 960×640 client minimum is unchanged. Linux remains opaque and undecorated.

The preceding compact revision was visually checked at 960×640 in Provider
view/edit, proxy and Agent pages, and at 1280×773 maximized. Its desktop frontend
suite passed all 367 tests. This final 8-pixel spacing adjustment passed desktop
typechecking and repository checks, with the rebuilt Linux package checked again
at runtime. Earlier full-workspace results and external-service/Wayland/update
limits retain their earlier scope.

Latest artifact:
`/Volumes/BRData/containers/aipass-linux/evidence/AIPass_0.2.0-beta.1_arm64_header-gap.deb`.
SHA-256: `b69906dec786e2ddd5d77ae476daad1d0dbea66469c7259373dd5d22c012d572`.
Runtime reports: `runtime-header-gap-final.log` and `guest/runtime-header-gap-final/`.
Final screenshot: `aipass-linux-header-gap.png`.
