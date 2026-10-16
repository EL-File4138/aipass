# Native CLI subscription ownership

Claude, Codex, Grok Build, GitHub Copilot and Gemini CLI use their official local
CLIs for authentication. The desktop can add an account or connect an existing
CLI account. Missing, unusable or incompatible CLIs block login with installation
and recheck actions. Grok Build is the single Grok subscription entry.

| Subscription | New-account login | Renewal | Quota | Local service inference |
| --- | --- | --- | --- | --- |
| Claude | `claude auth login --claudeai` | Claude Code, in the same home | Claude Code `/usage` | Claude Code bridge |
| Codex | `codex login` browser callback, without `--device-auth` | `codex app-server`, `account/read` with `refreshToken` | CLI `account/rateLimits/read` | Codex Responses API, using CLI access |
| Grok Build | `grok login --device-auth` | `grok models` | Official Build billing endpoint | Build Responses API, using CLI access |
| GitHub Copilot | `copilot login --web-flow` | CLI-owned GitHub credential; no AIPass OAuth refresh | Copilot entitlement endpoint | Copilot CLI API, using CLI access |
| Gemini CLI | ACP `initialize` and `authenticate`, `oauth-personal` | Same CLI ACP authentication, with browser disabled | Code Assist quota endpoint | Code Assist GenerateContent, using CLI access |

Other existing community adapters retain their own provider-specific contracts;
this CLI ownership refactor covers the five subscriptions above. Quota uses CLI
commands where a supported account-quota operation exists. Grok's `usage` command
is local session token/cost accounting, not remaining subscription allowance.
Copilot/Gemini allowance queries use the current CLI access without storing or
refreshing OAuth grants in AIPass.

## Data and routing

The encrypted provider record contains a routing marker, models, account identity
and `nativeHome`/`nativeProvider`/`nativeDevice` reference. Access/refresh/ID tokens
are excluded. Native credentials are read only in Rust for the immediate request,
kept in zeroizing buffers, and never returned to the frontend. Codex discovery may reuse a short-lived Keychain snapshot after a non-interactive attribute check; generation admission and account-switch backups require fresh secret reads. CLI-owned refresh
writes stay in the vendor's store. CLI children exclude inherited API keys and
custom provider endpoint variables, and apply the configured outbound proxy.

New accounts use private persistent vendor homes under `aipass-accounts/<account>`.
Gemini's `GEMINI_CLI_HOME` is a home root: credentials live in its `.gemini/`
subdirectory. Copilot supports github.com CLI config-file tokens or its macOS
Keychain entry. Enterprise Cloud hosts bind the account to its `*.ghe.com`
tenant and validated tenant API; unavailable secure stores report an error
rather than falling back to an editor account. CLI-managed homes are not deleted
when an AIPass provider entry is removed.

Account references are device-bound. Each computer must connect its local CLI;
AIPass does not replicate native credential files through vault sync. Account or
Codex-workspace changes fail closed and require explicit reconnect. Background
renewal is bounded and cannot launch an interactive browser flow.

CLI subscriptions use the existing Rust subscription backend and model-specific
wire codecs. Add their provider entry's retained secret ID to a local proxy group
as usual. Local OpenAI Chat/Responses and Anthropic Messages routes can dispatch
to these subscriptions, including streaming and tool conversions.
For groups mixing brands or different model names, bind each member's actual model
and use the public group model. See [cross-brand routing](proxy-routing.md).
Quick integration can select Codex and Claude Code native subscriptions in
`official` mode or an exact API credential ID. Other subscriptions retain local
proxy integration. Subscription references never become tool API keys. See
[native account switching](native-tool-switching.md).
WebSockets are disabled for the subscription backend. Gemini's Code Assist response envelope
preserves native thought signatures and tool history. Copilot premium quota is
scoped to models with explicit premium flags or positive billing multipliers; unknown or unrelated
quota buckets are advisory, so free models are not falsely exhausted.

## Subscription policies

Account quota settings expose Codex paid-credit and reset-credit policies and
Claude/Codex native automatic switching. All are disabled by default. Automatic
switching requires an enabled group with at least two verified accounts of the
same provider. It preserves encrypted backups and respects manual account
changes; running external CLIs still need a restart after their account changes.

The CLI uses the same Agent policy validation:

```sh
aipass accounts policy <account-id>
aipass accounts policy <account-id> --native-group <group-id> --return-preferred true
aipass accounts policy <account-id> --disable-native-switch
aipass accounts policy <codex-account-id> --allow-paid-credits true
aipass accounts policy <codex-account-id> --auto-reset-credits true
aipass accounts reset <codex-account-id>
```

`reset` spends an available reset credit. Unfinished operations reuse their
persisted idempotency key; an explicit `--request-id` can retry the same operation.
An unknown outcome must be resolved before starting a different reset. Disabling
paid credits checks official allowance immediately before generation; vendor
quota races mean this is not a hard billing cap. MCP exposes
`subscription_policy_get`, `subscription_policy_set` and `subscription_reset`.

See the [provider review](subscription-provider-parity-2026-10-16.md) for passive
allowance feedback, encrypted history ownership and remaining provider gaps.

## Legacy handoff

Managed Codex/Grok records undergo a one-time handoff. An existing CLI store with
the same account/workspace is preferred. Otherwise the recoverable legacy grant
is written once to a private vendor home. Its identity is checked before the
provider record becomes a reference and the managed-account record is removed.
Entry and secret IDs survive, preserving local route associations. A populated
foreign home is never overwritten. Failed handoffs retain recoverable encrypted
input, but the old managed OAuth forwarding path is blocked until reconnect.

Older imported Claude/Copilot mirrors bind to the matching local CLI account;
if unavailable, connect/sign in again. Successful rebinding removes mirrored
runtime grants. Identical references are idempotent; a changed home rotates the
binding generation to invalidate cached account/conversation state.

## Validation

AIPass's Rust Agent controls subscription references, request routing and
protocol conversion. Authentication and renewal for these five subscriptions
belong to their official local CLIs. Protocol changes require provider-specific
regression coverage and validation against the corresponding vendor behavior.

Focused checks (run natively on macOS with the repository wrappers/toolchains):

```sh
cargo fmt --all -- --check
cargo clippy -p aipass-agent -p aipass-vault -p aipass-proxy -p aipass-agent-protocol -p aipass-desktop --all-targets -- -D warnings
cargo test -p aipass-agent -p aipass-proxy -p aipass-vault -p aipass-agent-protocol
cargo test -p aipass-agent installed_native_clis_can_be_probed_without_credentials -- --ignored
pnpm --filter @aipass/desktop typecheck
pnpm --filter @aipass/desktop test
pnpm --filter @aipass/control-panel build
pnpm --filter @aipass/desktop build
```

Protocol 18 adds subscription policy/reset, passive allowance and durable history
operations. Restart an older resident Agent when using the updated clients.
It retains native tool status, bound previews and target-bound login tasks alongside `subscription.cli.status`, local subscription import tasks and the retained Claude
operation tags. Regression fixtures cover handoff identity and stable IDs,
credential rereads, account/device changes, CLI RPC, quota scopes, Code Assist
envelopes, login cancellation and lock/unmount. Browser fixtures cover 960×640 in
English/Chinese and light/dark. Help/version probes do not sign in, read existing
credentials or generate content. Real authorization, live quota and paid-model
requests still require opt-in account validation.

See [local account import](local-account-import.md) for batch discovery, custom
directories, result recovery, and the CLI/IPC contract.

## Client cache and retained provider sessions

Claude Code keeps its default native caching unless a Messages caller sets
`x-aipass-cache-marks: client`. Unmapped client marks are reported in
`x-aipass-cache-marks-dropped`. Cache creation totals and their one-hour subset
are retained separately for pricing.

Grok normalizes namespace/text-custom tools and integral numeric arguments in
Rust. Cursor can retain one native HTTP/2 Run across complete text-tool result
batches, with account/history validation and no replay after uncertain writes.
Expired Cursor continuation requires a new conversation; final usage covers the
whole native Run, identified by `x-aipass-usage-scope: native-run`.

See [capability boundaries and validation](subscription-capabilities-2026-10-16.md)
for supported mappings, expiry, billing scope and remaining gaps.
