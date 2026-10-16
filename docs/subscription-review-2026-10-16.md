# Subscription review, 2026-10-16

Follow-up: [overall correctness review and additional fixes](subscription-correctness-review-2026-10-16.md).

Compared AIPass's current working tree with Magpie
[`1a16fc0b5b43dcdc77b6212c49f98156a836bef6`](https://github.com/yetone/magpie/tree/1a16fc0b5b43dcdc77b6212c49f98156a836bef6).
The review covers the pending Claude identity changes, Claude/Codex account
lifecycle and wire contracts, and shared native-subscription/quota routing.
It does not establish parity for every endpoint of every provider.

## Confirmed defects corrected

| Priority | Trigger and effect | Correction and regression evidence |
| --- | --- | --- |
| P1 | Default Claude accounts use the parent `.claude.json`, but the command environment and login reader selected an inner profile. Discovery, quota and native switching could disagree about the same account's organization. | Share `native_auth::claude_identity::profile_path`; leave default `CLAUDE_CONFIG_DIR` unset and preserve its Keychain selection. Malformed existing profiles fail rather than downgrade to email-only identity. Covered by default-environment, default-profile/native-store agreement and malformed-profile tests. |
| P1 | A newly connected Claude organization triggered upgrades of unrelated legacy organizations. Their markers could rotate without the caller refreshing those sibling proxy bindings. | Limit login-time upgrades to the organization being connected. Background migration refreshes live runtime readers and the UI revision even when quota is disabled/recent or a later migration fails. Covered by sibling-binding and background-upgrade tests. |
| P1 | `seven_day_sonnet`/`seven_day_opus` windows were displayed but discarded before routing, so an exhausted model family kept receiving requests. | Carry known Claude family scopes into runtime quotas. Exact account-wide and model-list rules retain their existing behavior. Covered by Agent allowance-resolution and proxy effective-model/freshness tests. Unknown scopes remain advisory. |
| P1 | CLI-owned Codex sent client version 0.116 while the proxy negotiated a newer compatibility floor; underscored session headers also differed from the current Codex cache contract. | Share Codex signing headers and catalog URL negotiation between both transports, use `session-id`/`thread-id`, and filter hidden models in the CLI catalog. The version is the adapter's supported floor, not a claim that the newest installed CLI was detected. Covered by header/catalog contract tests and the real local HTTP forwarding fixture. |
| P2 | A canceled/failed Claude login left a persistent private directory. Completed login receipts also counted toward the four-pending-login limit. | Temporary ownership removes abandoned directories with best-effort Keychain cleanup. A persistence attempt retains its home because a failed vault operation can already have committed a record. Saved receipts remain idempotent without consuming pending capacity. Tests cover cancellation, ambiguous writes, successful save/retry and a fifth account after four completed receipts. |

The earlier organization fix is retained: account identity is email plus the
organization UUID, with status/profile consistency checks and strict ownership
checks during native switching and targeted reauthentication. Entry and secret
IDs remain stable across verified legacy upgrades.

## Subscription implementation follow-up

The follow-up compares the newer Magpie `a6f908ba587cc48af626752d9d9797752417b7e5`
and community-plugins `6a3fe9b49e7e638ed79f00d274ad6e1b93a384cd` snapshots.
See the [17-provider matrix](subscription-provider-parity-2026-10-16.md) for
implemented capabilities, remaining differences and vendor-contract sources.

Implemented in the same working tree:

- Account/revision-bound Codex response quota and credential-revocation feedback;
  bounded official-CLI recovery applies only to future requests.
- Claude live allowance observations with independent window timestamps and
  local organization checks; encrypted completed-turn checkpoints and forked
  native resume, with strict pending-tool and attachment boundaries.
- Codex reasoning/search/cache/compaction adaptations and durable account-bound
  encrypted-history routing. Passive storage failure cannot replay generation.
- Default-off paid-credit, reset-credit and coordinated native-switch policies,
  with desktop, typed IPC, CLI and MCP entry points. Reset intents are persisted
  before the official idempotent CLI RPC.
- Copilot model endpoint/capability/premium scopes and Enterprise Cloud account
  isolation; Gemini explicit project scopes; WorkBuddy enterprise and free-model
  scopes; Devin omitted-zero quota; Factory Standard/Core correction; MiMo
  numeric identity checks; shared billing-period persistence.

The matrix explicitly retains Copilot Auto, provider check-in/campaign flows,
Cursor's separate native-session transport, and unverified multi-pool/bonus
semantics as remaining differences. No automatic provider-engine import is used.

## Validation boundary

Initial implementation results on native macOS; see the correctness follow-up
linked above for the final code's updated results:

- `cargo test --workspace -- --test-threads=1`: 929 passed, 4 ignored,
  across 31 suites.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo build --workspace`: passed.
- `cargo fmt --all -- --check`, `pnpm repo:check` and `git diff --check`: passed.
- `pnpm lint`, `pnpm typecheck`, `pnpm test` and `pnpm build`: passed.
  Desktop tests: 331 passed in 43 files. Frontend checks still emit existing
  Svelte `derived_inert` diagnostics in tests; builds report large chunks.
- Browser fixtures at 960×640: English/Chinese, light/dark, default policies
  and expanded native-switch settings passed. This is frontend fixture evidence,
  not an installed Tauri runtime or artifact verification.

Parallel workspace tests are not consistently green. An earlier run passed
928 tests before the final automatic-switch refinement; a subsequent parallel run
failed `provider_update_refreshes_running_credentials_and_headers` on a local
HTTP 502. Its isolated rerun passed; the final serial workspace run above passed.
Earlier parallel runs also failed a WebSocket fallback assertion and a macOS
autostart process-preservation check. Their causes are not established as fixed.
RTK failure logs are retained under its `tee` directory, including
`1792093843_cargo_test.log`, `1792094118_cargo_test.log`,
`1792096009_cargo_test.log` and `1792097544_cargo_test.log`.

Tests use synthetic accounts, temporary vendor homes and local mock servers.
No real OAuth authorization, remote refresh-token rotation, paid inference or
subscription billing was exercised. The review does not authorize release or
prove live account eligibility. Local checks run natively on macOS; remote CI
and desktop artifact validation remain separate.
