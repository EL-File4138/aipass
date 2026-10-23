# Desktop interaction review, 2026-10-24

The subscription release review exposed six desktop issues. This follow-up keeps
the existing Rust credential and usage authority and changes the desktop's
presentation and lifecycle coordination.

- Tray quit drains in-flight startup/repair, rejects future readiness and update
  tasks, unloads the macOS supervisor without sending Quit to itself, and exits
  once. If unloading fails, the app stays open for retry.
- Filled primary buttons use a separate blue fill and white foreground in both
  themes. Text/link accents retain their existing theme colors.
- The Agent workspace uses the same list/detail columns as other pages. The
  list reserves the custom titlebar strip. The detail follows the active
  entry/secret and keeps an unconfirmed credential selection separate.
- The provider header, credentials and editable metadata share their view/edit
  positions. API quota cards stay mounted and read-only; metadata saves use the
  latest selected observation rather than editable quota drafts.
- Domain overrides live in collapsed advanced settings. Rust derives empty
  domain lists from API hosts, excluding console URLs. Existing exact API-host
  defaults remain automatic on edit; explicit custom matching domains survive.
- Subscription limits and local token counts are separate. The latter are a
  seven-day, entry/secret-filtered summary of requests through AIPass, not a
  claim about all traffic in the official agent.

The account/window grouping and local-versus-upstream usage distinction were
reviewed against [Magpie's UI](https://github.com/yetone/magpie/blob/main/internal/gui/assets/app.js).
No implementation or provider engine was imported.

## Focused evidence

At 960x640, both dark and light themes were inspected using the real Svelte
components with synthetic fixtures. Header and body coordinates remained equal
across view/edit; the credential row retained its 102px height. The primary
button computed colors were `rgb(37, 99, 235)` and `rgb(255, 255, 255)`.

The regression suite covers stable mounted fields, read-only live quota,
optional domain defaults, account-bound usage while selecting another
credential, and retained preview confirmation. Rust tests cover startup/quit
serialization, API-host normalization, and supervisor suspension followed by
child exit without relaunch. Real subscription accounts remain outside this
validation, as requested. Release CI and finished-artifact startup/update checks
remain separate gates.
