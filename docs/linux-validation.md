# Linux validation

The default local-validation policy uses native macOS checks and remote Linux
runners. For this audit, the user explicitly authorized a local Linux desktop
on 2026-10-18 because no independent Linux machine is available. This task-specific
authorization supersedes the earlier local-VM exclusion; it does not establish a
new permanent repository policy. The final requested environment uses Apple Container with an XFCE X11 desktop
served through VNC/noVNC. Record container results separately from native
macOS and GitHub Actions results.

## Automated coverage

`.github/workflows/ci.yml` runs the complete Rust and Node checks on Ubuntu.
The additional `Linux packaged runtime` job builds a finished deb package and
extracts it into an isolated fixture. Under DBus and Xvfb it starts the real Tauri
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
disables browser/login registration and uses normal Agent discovery. Linux
install/cached-update modes are rejected explicitly; the existing macOS updater
gate retains signature verification and its four runtime scenarios.

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
- AppImage execution and signed updater install/restart
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
