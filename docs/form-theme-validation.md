# Desktop form and theme validation

Validated on 2026-10-20 against the uncommitted working tree based on
`95f6f8ac8e838710f8ae55ea42cbfa2af0f18d49`.

## Runtime and artifact

- Linux: existing Apple Container desktop, Ubuntu 24.04 ARM64, XFCE/X11,
  compositing enabled, WebKitGTK, ordinary `aipass` user, software rendering.
- Every application capture uses the Tauri minimum viewport, **960 × 640**.
- Installed/extracted preview Deb:
  `/home/aipass/ui-form-theme-v2-artifacts/AIPass_0.2.0-beta.1_arm64.deb`.
- SHA-256: `aea7d4d012b2f95b009b60981b067394a3727b8a4539f47ca01e612fe3e06c39`.
- `artifact-v2.json` and `source-sha256-v2.json` record the preview identity and
  matching host/guest source hashes. This is a local UI preview, not a release.
- macOS comparison: native Tauri/WKWebView, same minimum viewport and UI source;
  retained settings snapshots and computed control styles in both themes. All
  **13 measured comparisons** passed, covering selector, switch, number/password
  fields and expanded dark-menu options (colors, scheme, opacity and height).

## Findings repaired

1. A forced dark theme did not theme GTK native select **popups**, even with
   `color-scheme` and option colors. Replace desktop native selectors with the
   shared `SelectField`, including auto-lock, credential format, usage credential
   and the balance-method editor. Retain numeric auto-lock values and per-key
   protocol metadata.
2. Advanced forms referenced missing surface/error/info tokens. Map those
   semantic names to the existing theme palette.
3. Primary white text on the dark accent had 2.98:1 contrast. A theme-aware
   foreground raises normal dark-button contrast to 6.39:1 (7.59:1 on hover);
   light-button contrast remains 5.17:1.
4. Make input/search disabled states and select/switch/segmented keyboard focus
   explicit. Native checks follow the application accent and color scheme.

The isolated runtime diagnostic records geometry, computed colors, focus,
validity, checked and disabled state. It does **not** collect control values.

## Linux interaction coverage

The runtime evidence covers light and dark themes for these reachable forms.
English settings, provider selection, runtime-settings tabs and pricing were
also checked at the minimum viewport:

| Surface | Controls and states exercised |
| --- | --- |
| Settings | All six tabs, segmented theme/language controls, auto-lock popup, numeric input, keyboard switch focus and toggle |
| Provider create/edit | Empty form, provider/protocol popup, advanced fields, multiline notes, focus, cancel |
| Credentials | Add/edit, format popup and keyboard selection, group, URL/model overrides, billing reference |
| Subscription/proxy | Quota and disabled refresh selector, custom proxy URL and credentials, invalid URL and backend validation error |
| Balance/webhooks | GET/POST choice, URL, JSON body textarea, native event checkboxes and keyboard navigation |
| Pricing | Source popup, rules, decimal numbers, UTC time fields, off-peak checkbox and keyboard time editing |
| Local proxy/Agent | Route strategy, advanced retry numbers, member picker, listener editing and Agent credential picker |
| Account connect | Provider search/picker and subscription credential form, without external authorization |
| Certificates | HTTPS options, file controls, read-only access code and selection, opening and cancelling the OS file chooser |
| Unlock | Empty, password focus, show/hide, incorrect-password error and successful unlock |
| New vault/import | Backup and WebDAV forms, source popup, strength, mismatch/match and recovery acknowledgement |
| Recovery | Recovery-key/password fields and scrolling through reset confirmation without executing a reset |

GTK/XSettings theme changes `Adwaita-dark → Adwaita → Adwaita-dark` switch an
already running application in System mode through `dark → light → dark`.
Restore the desktop's original `Greybird` setting after testing. GNOME's
`org.gnome.desktop.interface color-scheme` alone does not control this XFCE
session; that probe is retained separately and is not a passing system-theme
claim.

The native OS file chooser follows the desktop's own theme and language. The
application's inline file controls follow the selected application theme.
System file dialogs, fonts and native number/time affordances retain platform
appearance; the form palette, dimensions and interactions are compared rather
than requiring pixel-identical OS chrome. Wayland and other desktop environments
were not covered by this run.

## Validation and evidence

- Shared UI, desktop and control-panel typechecks: 0 errors / 0 warnings.
- Desktop tests: **380 passed in 46 files**, including auto-lock custom/never
  selection and credential protocol persistence after selector replacement.
- Desktop and embedded control-panel frontend builds passed.
- Desktop Rust formatting and Clippy (`--all-targets -- -D warnings`) passed.
- Repository source-size/ownership gate and `git diff --check` passed.
- No commit, push, tag, production installation or release was performed.

Runtime screenshots and read-only snapshots:
`/Volumes/BRData/containers/aipass-linux/share/ui-audit/form-theme-20261020/`.
The generated `index.html`, `summary.json` and `parity.json` provide the final
case inventory and measured macOS/Linux comparison: **158 current passing
Linux capture states**, plus separately marked historical/probe evidence.
`running-artifact-proof.json` verifies all four packaged executables against a
fresh extraction of the recorded Deb. Auto-lock Never and per-key format changes
were saved through the UI, verified after desktop restart, then restored. Earlier native-popup
captures are preserved as failing evidence, alongside the repaired `linux-v2-*`
captures. Failed automation attempts are not counted as passing cases.

## Focused Linux textarea recheck

A separate native-input pass checks provider notes in both the create dialog
and inline editor, plus the balance POST JSON body, in light and dark themes.
It uses the same recorded preview Deb and the 960 × 640 viewport.

- Check empty placeholders, filled text, Chinese/English multiline input,
  focus/blur, theme colors, borders, inset spacing and the native scrollbar.
- Paste 35–40 lines into the fields and a long unbroken token into notes; verify wrapping,
  Ctrl+Home/Ctrl+End and wheel scrolling inside the textarea.
- Drag the native resize corner diagonally: height grows by 85 px while width
  stays fixed. Scroll the surrounding form when needed to reach the corner,
  then shrink back to the original height. Dialog actions remain reachable.
- Cancel every synthetic draft after testing. The functional pass did not
  catch the cramped visual treatment described in the correction below; it
  must not be treated as visual design acceptance.

All **51 focused capture states** passed the geometry/theme checks, with no
layout overflow. `textarea-followup-results.json` retains the measured control states;
`textarea-index.html` links the focused screenshots. These are Linux runtime
results, not additional measured macOS textarea comparisons. The old standalone
balance/webhook config components have no current call sites and are not counted
as reachable runtime coverage.

## Textarea design correction

User visual review identified that provider notes still looked cramped despite
passing functional checks. `ProviderFormFields` applied its compact single-line
input overrides to textarea too: zero horizontal padding, transparent borders,
a bottom inset line and a second focus outline. This shared CSS affects macOS
and Linux; it is not a Linux-native textarea design.

Remove textarea from those single-line overrides. The shared `Field` treatment
now uses 10 px vertical / 12 px horizontal padding, a minimum height of 88 px,
1.5 line height, a complete theme-colored border and the standard radius/focus
ring. Hover only strengthens the border. Notes in both create and edit forms
and the JSON request body share this treatment; vertical resizing is retained.

The earlier 51-state functional captures describe the previous artifact.
Revised runtime evidence and source hashes use `textarea-v3-*` and
`source-sha256-textarea-v3.json` so the old captures remain distinguishable.

The revised Linux preview is
`/home/aipass/ui-form-theme-textarea-v3-artifacts/AIPass_0.2.0-beta.1_arm64.deb`,
SHA-256 `aa749a5b249e2a78c51760b989ccdbfc625f21c6b68947fbe5e74a67cce78287`.
`textarea-v3-artifact-proof.json` compares all four running packaged executables
with a fresh extraction of that Deb. Shared UI/desktop/control-panel typechecks,
the embedded panel build and the macOS desktop preview build passed. This is a
CSS-only correction; no additional behavior tests or release gates were run.

The macOS console is currently locked (`CGSSessionScreenIsLocked = 1`). Native
input/screenshot attempts cannot establish a macOS visual result in this state;
the new macOS binary was built, but revised macOS textarea screenshots remain
pending console unlock. Shared source establishes that the same style correction
is applied to both platforms; it is not a new measured pixel-parity result.

The revised Linux pass completed **66 capture states**, all at 960 × 640 with
no layout overflow. `textarea-v3-index.html` and `textarea-v3-results.json` retain
the captures and measurements; the original screenshots remain unchanged.

## Unified select option popups

The desktop's reachable standard selectors already use Bits UI `SelectField`,
not native GTK select popups. The rich credential picker also uses Bits UI, but
had a separate popup skin. Share `styles/select-menu.scss` between both pickers
and the embedded control panel. Preserve credential metadata layout while using
the same border, radius, popup padding, selected tint, indicator column and
keyboard/pointer highlight. Keep the selected check distinct from navigation
focus; constrain popup height to the available window space and retain label
ellipsis, keyboard selection and focus return. No new selection state or
credential/storage path is introduced.

Local macOS Node checks: UI/desktop/control-panel typechecks passed with zero
errors/warnings; **57 desktop and 23 control-panel tests** passed. The embedded
panel build, repository gate and skill validation passed. macOS UI runtime
validation remains skipped at the user's request.

Revised Linux preview:
`/home/aipass/ui-form-theme-select-v4-artifacts/AIPass_0.2.0-beta.1_arm64.deb`.
SHA-256 `b50a45475656f03f07bc38df75a7c1896dc50d68ce4f7f09f31e15cea95a1db9`.
`source-sha256-select-v4.json` records 18 matching source/embedded inputs;
`select-v4-artifact-proof.json` verifies the four packaged executables against
a fresh Deb extraction. This is a private preview, not a new published release.

Linux runtime cases at 960 × 640 cover auto-lock selection and restoration,
keyboard/Escape focus return, provider-list Home/End scrolling, a selector near
a dialog edge, outbound mode, balance GET/POST, rich credential metadata and
long labels, plus Agent credential options, in light and dark themes. Eight
synthetic credentials in the isolated test vault exercise scrolling and ellipsis;
they are removed after the run. No configuration is applied to external CLIs.

The completed pass retains **40 capture states**, including English auto-lock
options in both themes. `select-v4-index.html` links application-window screenshots;
`select-v4-results.json` retains computed option styles and geometry. All captures
use 960 × 640 and have no detected layout overflow. The synthetic credentials
were removed through the private Agent/CLI, retaining only the original primary
credential. Native GTK popup styling is no longer involved in these selectors.
