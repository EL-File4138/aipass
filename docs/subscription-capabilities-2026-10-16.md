# Subscription capability follow-up, 2026-10-16

This change implements the locally owned gaps from the [Magpie follow-up review](magpie-followup-review-2026-10-16.md). It does not establish live-vendor parity. Subscription authentication, quota, adapters, retained sessions and credentials remain in Rust; no third-party engine or plugin loader was imported.

## Claude client cache policy

The default keeps Claude Code's native cache behavior. Client `cache_control` annotations are removed before sending the native prompt unless an Anthropic Messages request explicitly supplies:

```http
x-aipass-cache-marks: client
```

The opt-in validates ephemeral 5m/1h TTLs, ordering and the four-mark limit before launch. Native user content can carry message-block marks; a current tail or automatic request-level mark is passed through `CLAUDE_CODE_EXTRA_BODY`, with CLI auto-marking disabled only in this mode. Cache mode/tail bind the native context. Default checkpoints keep their previous context identity. The existing CLI environment isolation remains authoritative; explicit command overrides preserve unrelated extra-body fields.

System instructions keep their system role, and tools keep the existing MCP/permission path. Their client cache marks cannot be mapped through those CLI interfaces. Historical marks in a serialized full-history replay and marks on MCP tool-result continuations are also unmapped. The response reports counts through `x-aipass-cache-marks-dropped`, for example `tools=1,system=1,messages=2`. Unsupported marks are never reported as applied. Switching the tail during a pending tool continuation requires restarting the conversation; live CLI settings-control ACKs are not implemented.

Invalid client cache plans are local HTTP 400 responses. The proxy removes the private opt-in body flag supplied by clients; only the header enables it. Local preflight rejections cannot trigger account fallback or vendor circuit failure.

## Mixed cache TTL accounting

`cache_creation_1h_tokens` is a subset of total cache creation, not an additional input-token bucket. Native Messages usage reads `cache_creation.ephemeral_1h_input_tokens`; Chat/Responses conversion preserves it under `*_tokens_details.cache_creation_1h_tokens`. Sparse updates merge before normalization. Missing/null fields preserve known values and explicit zero is authoritative.

SQLite retains total creation and the 1h subset separately. Existing databases migrate the new column with a zero default. Existing aggregate token views continue to count creation once. Both immediate cost estimates and query-time historical repricing charge `(creation - 1h)` at the existing creation rate and `1h` at the optional hour rate. Versions, off-peak rules, overrides and credential multipliers retain their priority. A missing 1h rate falls back to the existing creation rate; an explicit zero stays zero. The public price loader reads LiteLLM's explicit `cache_creation_input_token_cost_above_1hr` field. It never invents a free hour rate when the field is absent.

Price-group and model-override editors expose an optional hour rate. The group editor preserves both regular and off-peak hour rates, including zero. Time inputs remain readable at 960x640.

Sources: [Anthropic cache TTLs](https://platform.claude.com/docs/en/build-with-claude/prompt-caching), [LiteLLM price metadata](https://github.com/BerriAI/litellm/blob/main/model_prices_and_context_window.json).

## Grok tools

The shared Rust adapter covers Grok Build subscriptions and official xAI API profiles, including HTTP JSON, SSE and the HTTP bridge used for downstream WebSockets. Generic gateways are unchanged.

- Responses namespaces flatten to deterministic, collision-checked function names and restore on output items and completed responses. History and tool choice use the same mapping.
- Text custom tools use a function envelope with one string input and restore custom tool events. Grammar-constrained tools are explicitly rejected because the backend cannot preserve their constraint.
- Integral decimal arguments normalize lexically across interleaved stream fragments, including large integers without f64 rounding. Strings, nonzero fractions and exponents are preserved. Delta concatenation and done payloads agree.
- Schema patterns escape ECMAScript character-class syntax for the backend's regex parser. The walker visits schema positions; defaults/examples/enums are not application schema nodes. `oneOf` is not weakened into `anyOf`.

Sources: [xAI function tools](https://docs.x.ai/developers/tools/function-calling), [xAI structured outputs](https://docs.x.ai/developers/model-capabilities/text/structured-outputs). The implementation is maintained here, independently of Magpie's deprecated built-in/plugin split.

## Cursor retained native Runs

A bounded Rust-owned pool keeps the bidirectional HTTP/2 Run open across caller tool rounds. It services native blob reads/writes and returns caller-provided results plus exec-close frames to the same connection. It never executes a local tool itself.

Owner, credential generation, model/options, input-history hash, pending assistant text, call names/arguments and the complete result batch are checked before upload. Call IDs also bind routing to the originating target. Continuation can resolve from those IDs without requiring a client session header. Concurrent, duplicate, changed, unknown, cancelled or foreign-account results fail closed. Proxy target/credential changes revoke retained Runs. Calls remain process-local; agent restart loses this state.

The pool allows 32 Runs and 8 per target, bounded blobs/output and 4096 retained call bindings. Idle tool waiting expires after ten minutes; an old connection retires before a further result upload. Neither rule terminates an ongoing generation. Partial or uncertain result uploads never reconstruct or replay. Expired continuation currently requires a new conversation; clean checkpoint-based reconstruction is not implemented. Integral protobuf arguments are restored as exact JSON integers within the native double format's safe range. Text tool results are supported; native result images are explicitly rejected during preflight.

Final measured token usage is emitted once for the whole native Run. `cache_creation_tokens` now uses the common wire field. The response declares `x-aipass-usage-scope: native-run`. No estimate of the last model step's context usage is presented as an exact vendor count. Partial Runs that end before the provider supplies usage can remain unmetered locally; vendor allowance polling is separate.

A local HTTP/2 server fixture confirms one Run across tool output/result/exec-close/final completion, verifies the result protobuf independently and checks altered history/call refusal. This is not a request to Cursor's service.

## Codex Keychain reads

Discovery/allowance reads share a short-lived zeroizing cache keyed by service/account and non-sensitive creation/modification dates. Attribute queries explicitly use `kSecUseAuthenticationUIFail` with secret-data return disabled. `auto` storage selection uses those attributes rather than reading the secret twice. Attribute failure remains distinct from absence and cannot silently select a file fallback.

Concurrent secret reads share one OS operation. A caller waits at most five seconds for setup; an unanswered prompt remains in flight, so later callers do not create another prompt. Failed authorization cools down for ten minutes. Explicit reconnect clears only the selected item's refusal; it does not duplicate an unanswered OS operation. Rotation during a read invalidates the result. Native writes invalidate the affected item; vault lock invalidates all positive snapshots/epochs while preserving other accounts' refusal cooldowns. A single expiry worker erases cached secret buffers within twenty seconds even without later traffic.

Generation admission and account-switch backups require fresh secret reads: metadata timestamps can be coarse, so a completed discovery cache must not authorize a paid request or an outgoing backup. Prompt/read coalescing still applies to in-flight fresh reads. Credentials remain in Rust and are never returned to UI.

Fixtures cover concurrency, unanswered prompts, refusal/retry, metadata change, fresh-read bypass and invalidation during an in-flight read. Actual macOS authorization UI and live Codex account rotation were not exercised.

## Additional correctness pass

The subsequent worktree review found and fixed two lifecycle defects:

1. Keychain fresh-read completions shared one generation counter. A faster subsequent completion could strand earlier waiters until they incorrectly reported pending authorization. Each read now retains its own zeroizing result for its callers. Slot eviction also preserves active callers and unexpired refusal cooldowns. A 16-caller/1,024-read regression reproduced failures before the fix and passes afterward; capacity protection has a separate regression.
2. A parked Cursor Run survived its request adapter, but outbound-proxy reconciliation inspected only still-live adapters. The pool now reconciles its retained connections independently and rejects continuation with changed outbound settings. A regression verifies preservation for unchanged settings and cancellation on a proxy change without a live adapter. Endpoint discovery now happens only when opening the Run, removing an unrelated network dependency from tool continuations. The local HTTP/2 continuation fixture still passes.

Account isolation, reset intent/ACK, native switching, checkpoint non-overwrite behavior, Grok request/stream mapping, sparse usage and mixed-TTL cost paths were also inspected. The full workspace checks below cover their existing regressions; they do not prove all vendor behavior.

## Validation

Local checks are native macOS. No Ubuntu VM/container, remote CI, installed Tauri artifact, real OAuth login, paid vendor generation or interactive Keychain prompt is included in this evidence.

Final current-worktree results:

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 974 passed, 4 ignored, 31 suites, zero failures |
| Installed native CLI capability probes | 2 normally ignored tests explicitly run and passed; Claude, Codex, Grok and Gemini CLI command capabilities only, no credentials or generation |
| `cargo clippy --workspace --all-targets -- -D warnings` | passed |
| `cargo build --workspace` | passed |
| `cargo fmt --all -- --check`, `git diff --check` | passed |
| `pnpm repo:check` | passed; 548 source files, 23 tracked legacy files, no external subscription engine dependency |
| `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm build` | passed; desktop 332 tests, control panel 23 tests |
| `pnpm licenses:audit` | passed, 355 packages |
| Browser UI at 960x640 | unchanged UI retains the prior pass's price-group/override evidence: Chinese/English, light/dark, 8 cases with zero page errors or viewport overflow; no new browser run in the additional correctness pass |

The full test run exposed an existing launchd fixture race: shell redirection created the PID file before its contents were written (`kill: illegal process id`). The fixture now publishes its PID with atomic rename; production tray suspension is unchanged. The final full-workspace run includes the repaired child-survival test.

Evidence is local and temporary:

- Rust logs: `/tmp/aipass-final-review-rust-tests-20261016.log`, `/tmp/aipass-final-review-clippy-20261016.log`, `/tmp/aipass-final-review-build-20261016.log`.
- Node logs: `/tmp/aipass-final-review-node-{lint,typecheck,tests,build}-20261016.log`.
- Fresh-read reproduction/fix: `/tmp/aipass-review-keychain-{before,after}-20261016.log`; Cursor fixtures: `/tmp/aipass-review-cursor-20261016.log`.
- Installed CLI probes: `/tmp/aipass-final-review-native-cli-20261016.log`. The interactive browser and Keychain transaction fixtures remain excluded from this pass.
- UI screenshots/results: `/tmp/aipass-cache-ui-20261016/`, including `dialog-en-dark.png`, `dialog-zh-light.png`, `fields-zh-light.png` and `results.json`.
- Original fixture-race diagnosis: `/tmp/aipass-autostart-pid-race-20261016.txt`.

Temporary UI harness files and the task's Vite server were removed after review. No commits, push, publication or installed-artifact validation were performed.
