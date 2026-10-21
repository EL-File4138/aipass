# Windows compatibility review, 2026-10-21

The review covers every Rust crate and JavaScript workspace, shared persistence,
desktop/CLI/Agent/Native Messaging boundaries, scripts, CI and release packaging.
It extends the [Agent integration review](agent-integrations.md#cross-platform-review-2026-10-21).

| Area | Review result and changes |
| --- | --- |
| Agent IPC | The Windows implementation of interprocess 2.4.2 returns Unsupported for socket send/receive timeouts. Clients now use nonblocking framed IO with real deadlines. PeekNamedPipe distinguishes an empty live pipe from EOF; writes that make no progress fall back to smaller fragments instead of retrying an oversized buffer forever. Authentication and protocol checks remain mandatory. |
| Desktop singleton | Reads/writes have deadlines; Windows names include the profile and optional fixture namespace. Both desktop and Agent pipe listeners use a protected DACL bound to the current user SID and deny network access. An exclusive profile-local file prevents competing server owners; the pipe name alone is not exclusive. Clients verify the pipe object owner before sending tokens or deep links, so a foreign preclaimed pipe name cannot impersonate the Agent. |
| Process lifetime | Background Agent/desktop launches use CreateProcess with handle inheritance disabled, retaining literal argv and Unicode environment overrides. Null stdio alone could leak Node/browser output handles and prevent command completion. Shutdown retains the peer process handle on the same authenticated connection and waits for actual exit. Timeout/cancel cleanup terminates the owned Windows process tree, including node.exe behind npm CMD shims; native regression tests retain the descendant process handle when checking exit. |
| Startup | Desktop release builds register hidden tray login through HKCU Run; the tray starts its sibling Agent. CLI Agent install/start/stop/uninstall uses its own per-user Run entry without SCM privileges. The headless Agent uses the GUI subsystem and suppresses the desktop at login. Legacy explicit SCM APIs remain separate. Unchanged Run values and Windows StartupApproved settings are preserved. |
| Lock policy | Direct Agents now receive lock/logoff/disconnect and suspend/resume notifications through a hidden native window. Input idle time comes from GetLastInputInfo. Service lock notifications now honor the saved policy. |
| Storage, backup and sync | Atomic replacement uses tempfile's Windows implementation. Export, snapshot, sync-object and conflict-record relative paths now use `/` on the wire. Older Windows `\\` paths are normalized before validation; drive paths, UNC paths, traversal, alternate data streams and NUL bytes are rejected on every OS. A decrypted/re-encrypted legacy Windows backup is tested through actual restore. |
| CLI installation | A copied Windows CLI also receives its Agent and Native Host siblings, so it can work outside the desktop installation directory. Existing running executables can still reject replacement; failures remain visible in logs rather than producing partial executables. |
| Browser integration | Native Messaging manifests stay in APPDATA and references in HKCU. Registry command output is captured so successful JSON output remains valid. Standalone smoke tests isolate APPDATA/LOCALAPPDATA and restore previous registry values. |
| Quick integration | Only real installed apps, binaries or live extensions qualify. Recheck before apply. Windows executable extensions, npm shims, PowerShell literal quoting, Unicode BOM, Chinese/spaced paths and encrypted rollback have native regression tests. WSL installations are separate. |
| Node tooling | Build, dev registration, license audit and disposable signing-key generation resolve pnpm's JS/native entry point and preserve separate argv. They do not try to exec pnpm.cmd or interpolate arguments through a shell. |
| Proxy, protocol conversion, registry and crypto | Reviewed platform branches and callers. Protocol and encryption behavior remains Rust-owned and OS-independent. Windows workspace compilation/tests cover their shared callers. |
| Extension, desktop UI, control panel and website | Reviewed subprocess/filesystem assumptions and platform selection. The minimum desktop viewport remains 960×640. Shared generated control-panel input hashes retain explicit LF checkout rules. |
| Release | Standalone CLI runtime checks now include Windows. Desktop release publication currently targets macOS/Linux; the previous status document claiming Windows desktop publication was corrected. |
| Login, quit and uninstall | Login commands preserve Unicode, spaces, metacharacters and custom vaults without shell expansion, and reject commands above the documented 260 UTF-16-unit Run limit. A companion launch does not hide an open resident window. A version token reveals main when an updater replays the previous version's login argv without inherited environment. NSIS quits only the exact installed desktop, removes that copy's custom-vault login entries and shuts down their authenticated Agents. Cleanup errors fail uninstall. There is no Run-key restart supervisor. |
| New vault paths | Resolve the longest existing ancestor and normalize verbatim Windows prefixes before namespace hashing, so first creation of nested vault directories does not move the Agent to a different pipe name. |
| Windows linking | The desktop Rust library is linked only into its executable. Unused staticlib/cdylib outputs were removed after the GNU Windows build exceeded PE's 65,535-export DLL limit. |

## Native Windows gate

The Windows branch-CI job runs the entire Node and Rust workspaces, including
Clippy with all targets, tests, build and control-panel reproducibility. It then
builds an NSIS installer with a disposable updater key and checks:

1. Silent installation and the four installed executables.
2. Real CLI CRUD/search, reveal, encrypted backup, folder sync, lock/unlock,
   Native Messaging ping, plaintext scan and per-user Agent login install/stop/start/uninstall.
3. Installed main and tray startup at 960×640, frontend readiness, event-loop
   responsiveness and authenticated Agent IPC.
4. Silent uninstall and removal of the main executable.

The tracked installer smoke only runs on disposable GitHub runners because it
registers URI handlers and uninstall metadata. It does not publish an artifact.
Logs and runtime results are uploaded even on failure. The local CrossOver run
below uses a separate adapter restricted to the task's disposable bottle; the
tracked runner guards remain intact.

## CrossOver executable and installer verification

Actual Windows executables were run locally using CrossOver 26.3.0.39832 in an
isolated 64-bit Windows 10 bottle, with Microsoft WebView2 155.0.4283.62 installed.
The tested version is `0.2.0-beta.1`, cross-built with Homebrew Rust and MinGW GNU.
Debug assertions were disabled to exercise packaged login and singleton behavior.
This is an unsigned test build, not a production MSVC release.

The following passed:

- Actual NSIS installation into a path containing Chinese characters and spaces,
  with the desktop, Agent, CLI and Native Host present.
- Installed main and tray startup at 960×640, real frontend readiness, responsive
  event loop and authenticated Agent IPC. Closing main went to tray and reopened.
- CLI CRUD/search/reveal, encrypted backup, folder sync, lock/unlock, Native
  Messaging, MCP stdio and denial, plaintext scan, and per-user Agent login
  registration, stop/start and removal.
- The literal HKCU login command launched a hidden tray. Launching main revealed
  the resident window; a background companion preserved that visible window.
  Exact-copy cleanup stopped the GUI and Agent and removed the Run entry.
- NSIS uninstall while the production login tray and Agent were resident,
  including a custom vault. Both processes exited, the login entry was removed,
  and the installed main executable was deleted.
- Focused Windows registry/argv/namespace, pipe-framing, background-process,
  shutdown and desktop singleton/launch-options regression tests. All selected
  tests passed; the interactive test remained ignored.

This verifies process behavior and actual WebView DOM readiness. It is not a
pixel-level Windows UI review or proof of a real OS login. Windows update modes
were not exercised. The retained evidence manifest includes exact binary/source
hashes and paths to the test logs and JSON reports.

## Evidence boundaries

The current local machine is macOS. Source review, macOS regression tests and
PowerShell execution do **not** establish a passing Windows run. The new Windows
CI job has not been run for this uncommitted tree.

Production-signed Windows desktop installation, real old-version upgrade and
updater restart, multiple Windows users, actual cross-user pipe access denial,
real screen-lock/suspend events, service registration and clean-machine repair
still need native Windows acceptance. Runtime verification on Windows currently
accepts startup mode only; it deliberately rejects update modes until an isolated
installer/restart fixture exists. No Windows desktop release is claimed here.

iCloud/CloudKit remains macOS-only. Native credential-store adapters that do not
implement a vendor's Windows store fail closed; this review does not introduce
plaintext fallbacks. CrossOver can provide additional Windows-executable evidence,
but it cannot establish native boot, Explorer notification-area behavior,
cross-user isolation, screen lock or sleep/wake correctness.

Implementation references: [Node CMD launcher limitation](https://nodejs.org/api/child_process.html#spawning-bat-and-cmd-files-on-windows),
[named-pipe default access](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
[session-local input clock](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getlastinputinfo),
[session notifications](https://learn.microsoft.com/en-us/windows/win32/termserv/wm-wtssession-change).


During local regression, LocalStack's running service scanner sent `GET /` with
`User-Agent: LocalStack/1.0` to new fixture ports; a disposable observer confirmed
this traffic. Credential-reload and image fixtures now answer unauthenticated
root discovery without counting it as a model request. Authenticated root and
API-path requests are still counted and asserted. This changes test fixtures
only, and focused regression tests cover the distinction. No unrelated app was
stopped to obtain passing results.


## Local verification of the reviewed tree

- macOS: workspace formatting, all-target Clippy with warnings denied, 1,127
  Rust tests passed (four test threads; four native/interactive tests ignored),
  and workspace build passed.
- macOS Node: frozen install, license audit, lint, typecheck, 659 tests, workspace
  build and unchanged embedded control-panel output passed.
- The staged CLI passed CRUD/search, secret reveal, encrypted backup, folder
  sync, lock/unlock, Native Messaging ping, MCP stdio/denial and plaintext scanning.
- The finished ad-hoc signed DMG/updater passed main and tray startup at 960×640,
  manual install/restart and cached install/restart with updater signatures enabled.
- Windows GNU cross-build: workspace all-target Clippy with warnings denied,
  all four executables and test binaries, and the real NSIS installer passed.
- Workflow syntax, repository source-size/dependency rules and the revised skill
  validator passed. No commit, push or publication was performed.

Native Windows CI remains unexecuted for this tree. Native macOS checks and
CrossOver results are distinct evidence, as recorded above. The Windows-only
Apple provisioning-profile parser test is excluded;
CloudKit signing remains macOS-only. The Windows NSIS gate installs into a path
containing Chinese characters and spaces and uses NSIS's documented unquoted,
final directory parameter. It also refuses persistent/self-hosted runners.


## Follow-up review

Cline discovery now pairs Stable, Insiders and Flatpak extension directories with
the corresponding installed host. A different surviving VS Code edition cannot
turn an uninstalled edition's extension leftovers into a quick-integration entry.
The regression includes all three roots, missing hosts and actual extension code.

The same namespace review found a Unix first-launch issue: a new nested vault
under a symlink used an unresolved ancestor until directory creation. Unix now
resolves the longest existing ancestor before deriving the IPC namespace, while
preserving existing symlink/parent semantics. This is covered by a reproduced
failing test followed by the fixed before/after-creation regression. Windows
namespace behavior is unchanged by this follow-up.

Windows stop/uninstall now treats an older protocol response as a live Agent and
uses the existing authenticated legacy-shutdown handshake. Previously the typed
status decoder rejected that response and cleanup silently skipped the resident.
The new Windows regression starts a real named-pipe server, returns the older
protocol status, verifies the token and legacy shutdown request, and completes
cleanup. It passed as a Windows test executable under CrossOver; no credential
mutation is retried against a legacy schema.

The follow-up's native macOS workspace gate passed 1,127 Rust tests (four ignored),
Clippy, formatting, workspace build and CLI runtime. The unchanged Node sources
reuse the preceding 659-test full gate. Windows GNU all-target Clippy passed for
Agent and configuration writers; the Windows executable tests passed the older
Agent shutdown case and six discovery cases. Earlier retained Windows desktop
EXE/NSIS artifacts belong to the preceding installer review, not this follow-up
source snapshot. Native Windows CI and signed upgrade acceptance remain pending.

The rebuilt macOS DMG and updater also passed all four final artifact scenarios:
main startup, tray startup, manual install/restart and cached install/restart at
960×640, with updater signature verification enabled. No user installation was
replaced, and no commit, push or publication was performed.


## Final pre-push review, 2026-10-22

Reviewed all pending integration, runtime, persistence, script, workflow and
license/provenance changes. The reviewed source matched the accepted 982-file
snapshot before the final display fix. The setup dialog used undefined surface
and text tokens; it now uses the shared theme's actual tokens.

The final display fix passed desktop typecheck (zero errors/warnings), 24 focused
integration tests and the production frontend build. Current components were
visually checked in an isolated native WKWebView at 960×640 in Chinese/dark and
English/light, including the installed-only list and setup dialog. Both had no
horizontal overflow. These component screenshots are separate from the passing
Tauri artifact lifecycle checks and from native Windows acceptance.

Rust, IPC, persistence, dependencies and CI inputs are unchanged since the passing
full macOS gate; those results are reused. Workflow syntax, repository checks and
both updated repository skills were validated again. No Linux VM/container or
production release is part of this branch push.
