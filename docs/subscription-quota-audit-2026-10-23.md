# Subscription balance and allowance audit — 2026-10-23

## Scope and evidence

Reviewed all 41 IDs in `community::PROVIDERS` plus the separate Claude CLI
account path: 42 subscription entries, including regional variants. Traced the
vendor request, response normalization, snapshot, refresh failure, model scope
and routing freshness. This is a source/contract audit with local regression
tests, not a claim that 42 authenticated vendor accounts were exercised.

Comparison snapshots:

- [yetone/magpie at e00fbdbb](https://github.com/yetone/magpie/tree/e00fbdbbc74f9829027a78769cbeb265d6271fb4), checked out outside AIPass for inspection.
- [magpie-community/plugins at 08ff18a4](https://github.com/magpie-community/plugins/tree/08ff18a43853ccf23b30fc6dd39b3f10591331ee). Many Magpie subscriptions now delegate their queries to this repository, so reviewing only Magpie's retired built-ins misses current fixes.

Provider behavior was implemented in AIPass's existing Rust adapters. No upstream
engine, source tree, generated adapter or runtime plugin was added to AIPass.

## Corrections

| Provider | Defect and corrected behavior | Contract reference |
| --- | --- | --- |
| Copilot | An enterprise pool can say `unlimited: true` and `has_quota: false` simultaneously. Preserve its exhaustion, scoped to premium models. Prefer precise entitlement/remaining counts over rounded percentages, skip zero-entitlement buckets, retain the reset-date fallback, count Free chat limits and distinguish personal overage from managed seats. | [Magpie quota parser](https://github.com/yetone/magpie/blob/e00fbdbbc74f9829027a78769cbeb265d6271fb4/internal/provider/subscription_usage.go), [seat regressions](https://github.com/yetone/magpie/blob/e00fbdbbc74f9829027a78769cbeb265d6271fb4/internal/provider/copilot_usage_test.go) |
| Cursor | Included pools alone falsely exhausted accounts with permitted on-demand spending. Read the hard limit and period spend, respect cents versus dollars and team versus individual budgets. Include bare model IDs alongside catalog context variants. Exhausted on-demand spending only gates pools whose included allowance is also exhausted. | [Current plugin](https://github.com/magpie-community/plugins/blob/08ff18a43853ccf23b30fc6dd39b3f10591331ee/packages/cursor/index.mjs), [usage fixtures](https://github.com/magpie-community/plugins/blob/08ff18a43853ccf23b30fc6dd39b3f10591331ee/packages/cursor/usage.test.mjs) |
| Command Code | Numeric millisecond resets disappeared at the snapshot's string-only date boundary. Normalize dates, use the current response's plan ID, retain balances alongside windows and do not infer exhausted monthly credits from an omitted field. Reject empty allowance responses and nonfinite numeric strings. | [Credits contract](https://github.com/yetone/magpie/blob/e00fbdbbc74f9829027a78769cbeb265d6271fb4/internal/provider/commandcode_plan.go) |
| Kiro | API keys incorrectly used the OAuth management GET. Use the key's AWS JSON `GetUsageLimits` POST on the same management service. Missing counters remain unknown; a free trial is not assigned a fabricated 30-day span. | [Current plugin](https://github.com/magpie-community/plugins/blob/08ff18a43853ccf23b30fc6dd39b3f10591331ee/packages/kiro/index.mjs) |
| OpenCode Go | Inference's new base was also used for billing. Query the documented-in-implementation Zen Go usage endpoint, preserving model scope and excluding only explicitly free models. | [Magpie plan sources](https://github.com/yetone/magpie/blob/e00fbdbbc74f9829027a78769cbeb265d6271fb4/internal/provider/planquota.go) |
| Ollama Cloud | Always reported quota unavailable despite an account balance API. Read included monetary allowance, purchased credits and legacy session/week limits. Missing values stay unknown; explicit zero remains visible; available purchased credits keep included exhaustion advisory. | [Ollama response contracts](https://github.com/yetone/magpie/blob/e00fbdbbc74f9829027a78769cbeb265d6271fb4/internal/provider/ollama_usage.go) |
| Factory | Extra credits lived only in a `display` field which the typed snapshot drops. Return them as the balance, including explicit zero. | [Factory limit fields](https://github.com/yetone/magpie/blob/e00fbdbbc74f9829027a78769cbeb265d6271fb4/internal/provider/factory_usage.go) |
| Gemini CLI | Negative/out-of-range remaining fractions or missing model IDs could produce invalid or unusable scopes. Require a valid model and a fraction in `[0,1]`; accept finite numeric strings. | Existing Code Assist `retrieveUserQuota` response contract |
| MiniMax Code CN/global | The account parser duplicated the key parser, omitted interval spans and ignored business failure codes. Share the normalizer; preserve exhausted/unmetered status, non-language advisory pools and reject empty/failed readings. | [MiniMax account parser](https://github.com/magpie-community/plugins/blob/08ff18a43853ccf23b30fc6dd39b3f10591331ee/packages/minimax/index.mjs) |
| ZCode | `unit=6, number=7` became 49 days. Both observed weekly encodings now mean 7 days. | [Zhipu units](https://github.com/yetone/magpie/blob/e00fbdbbc74f9829027a78769cbeb265d6271fb4/internal/provider/planquota.go) |
| Grok / Devin | An on-demand/ACU limit without a used count became zero usage. Keep it unknown. Preserve an explicit Devin zero balance. Devin's existing, narrowly conditioned protobuf omission rule for quota windows remains separate. | [Grok](https://github.com/magpie-community/plugins/blob/08ff18a43853ccf23b30fc6dd39b3f10591331ee/packages/grok/index.mjs), [Devin](https://github.com/magpie-community/plugins/blob/08ff18a43853ccf23b30fc6dd39b3f10591331ee/packages/devin/index.mjs) |
| MiMo app | A failed weekly usage read was suppressed after a successful plan read, clearing the previous allowance. Propagate that failure through the existing stale-snapshot path, reject business errors and malformed readings, and preserve the free offer's legitimate absence of an allowance window. | [MiMo envelopes and allowance fields](https://github.com/magpie-community/plugins/blob/08ff18a43853ccf23b30fc6dd39b3f10591331ee/packages/mimo/index.mjs); failure preservation follows AIPass's shared refresh contract |

## Complete entry inventory

“Reviewed” below means the implemented source path was inspected; live tenant,
plan and region compatibility still needs an account for that case.

| IDs | Query and audit result |
| --- | --- |
| `claude` | Official CLI `/usage`; session/week/model-family parsing, timezone handling, account lock and passive feedback reviewed. Failed snapshots stay stale and cannot gate routing. See freshness limitation below. |
| `codex` | Official CLI `account/rateLimits/read`; base bucket, additional limit IDs, credits and reset policy reviewed. Existing tests preserve the base allowance when extra buckets exist. |
| `copilot` | CLI-owned credential, tenant-specific entitlement API; corrected as above. |
| `gemini-cli` | Code Assist project-bound `retrieveUserQuota`; corrected validation as above. |
| `zcode` | Start Plan balances or GLM personal/team quota; corrected weekly duration. |
| `qoder`, `qoder-cn` | Device-token usage API; reviewed one-time refresh, both JSON naming conventions and multiple credit buckets. Enterprise response supplies plan only. |
| `devin` | `GetUserStatus`; reviewed quota-billing omission rule, hidden windows and account API server. Corrected ACU/balance parsing; the follow-up adds account-specific free-model scope from `GetCliModelConfigs`. |
| `zed` | `/client/users/me`; plan, billing-period end and overdue-invoice error. No numerical quota; current upstream likewise reports plan state. |
| `factory` | `/api/billing/limits`; standard/core model scopes and extra credits reviewed; corrected balance propagation. |
| `grok` | Build `/billing?format=credits`; period and on-demand allowance. Corrected missing counts. This is different from CLI local token accounting. |
| `commandcode-plan` | Billing credits plus retained subscription metadata; corrected dates/current plan/unknown counts. |
| `mimo-app` | App subscription and weekly usage endpoints; remaining percent is inverted correctly. Corrected failure propagation and response validation. Platform Token Plan remains a gap below. |
| `cursor` | Current-period usage plus hard limit; corrected budgets and exact model scopes. Separate Grok Bot allowance is not queried. |
| `kiro` | OAuth management route or API-key control RPC; corrected request and counters. |
| `workbuddy`, `workbuddy-ai` | Personal resource summary or enterprise usage meter; reviewed tenant pinning, numeric strings, UTC+8 cycle dates and exclusion of verified free models. Personal per-package expiration detail is not queried. |
| `opencode-go` | Zen Go `/v1/usage`; corrected request URL; rolling/week/month and free-model scope retained. |
| `opencode-zen` | Automatic balance unavailable; Console link retained. Magpie also explicitly reports no supported Zen balance query. |
| `antigravity` | Model catalog quota fractions, known enabled model scopes and reset dates reviewed. |
| `kimi-work` | Membership `subscriptionBalance.amountUsedRatio`; ratio-to-percent conversion and local identity binding reviewed. |
| `minimax-code`, `minimax-code-global` | Personal workspace membership/credits plus M Plan remains; shared parser correction. |
| `cline` | Account balance plus ClinePass limits; reviewed micro-dollar conversion, exact catalog membership and preservation of plan windows when only the paid balance is unavailable. |
| `trae-cn`, `trae-global` | Regional entitlement endpoints; reviewed active pack filtering, credit versus basic/bonus accounting, unknown-count rejection and unlimited CN plans. |
| `ollama-cloud` | Newly queried `/api/balance`; current and legacy allowance shapes tested. |
| `kilo-gateway` | Automatic balance unavailable; Console link retained. |
| `kimi-code-cn`, `kimi-code` | Region-specific `/coding/v1/usages`; reviewed numeric strings, remaining/used denominator and reset dates. |
| `minimax-m-plan-cn`, `minimax-m-plan` | Region-specific `/v1/token_plan/remains`; shared validated parser. |
| `bailian-coding-plan` | Automatic usage unavailable; bound region selects the matching Console. |
| `mimo-plan` | Automatic usage unavailable; Token Plan Console. |
| `stepfun-plan-cn`, `stepfun-plan` | Automatic usage unavailable; platform sign-in is not part of these key-only records. |
| `qwen-token-plan`, `bailian-token-plan` | Automatic usage unavailable; Console only, distinct from Coding Plan. |
| `volcengine-coding-plan`, `volcengine-agent-plan` | Automatic usage unavailable; generation key is not the account AccessKey required for platform metering. |
| `tencent-token-plan`, `tencent-token-plan-enterprise` | Automatic usage unavailable; the enterprise platform query needs separate cloud-account credentials. |

There are **12 entries with `quotaUnavailable`** after this change: the ten
plan entries from Bailian Coding Plan through Tencent above, plus OpenCode Zen
and Kilo. This is an implementation capability boundary, not a zero balance or
proof that every vendor lacks an API. Magpie's StepFun/platform-cloud support
uses additional account authorization which these AIPass records do not hold.

## Remaining differences and live validation

- **Devin free models:** addressed in the follow-up below. Account organization
  identity remains a separate concern; free-model scoping does not repair it.
- **MiMo:** the app adapter does not query the additional platform Token Plan.
- **Claude freshness display:** a failed poll retains the previous windows and
  marks them stale, but `observed_at` records the new attempt. Routing excludes
  the stale snapshot. Separating last-success time from retry scheduling would
  make this path consistent with `community/quota.rs`.
- **Additional presentation:** Cursor Grok Bot and WorkBuddy personal package
  expiration details are absent. The primary account quota queries are present.
- **Scope safety:** ClinePass has exact IDs, not wildcard matching at routing;
  Factory and Cursor have distinct model pools; Copilot premium windows do not
  block known free models. Unknown readings do not authorize automatic native
  account switching.
- **Refresh failures:** community refresh preserves the previous visible
  snapshot with its original success time, writes a separate attempt receipt,
  and excludes failed raw quota from routing. Busy generation does not erase
  the last reading. Routing only uses current observations, with the existing
  five-minute age bound and reset expiration.

Unauthenticated public probes returned HTTP 401 JSON for Ollama balance and
HTTP 403 for both OpenCode URL candidates. These establish neither authenticated
quota correctness nor that the former OpenCode route definitely fails for a
valid key. The Go URL correction is based on the pinned upstream contract.

## Follow-up: recent subscription handling fixes

Refreshed the comparison on 2026-10-23 at approximately 07:20 UTC, including
recent commits and issue discussions, against
[Magpie `be002aca`](https://github.com/yetone/magpie/tree/be002aca050f01fce83f58728961614d3b987a63)
and [plugins `421117ef`](https://github.com/magpie-community/plugins/tree/421117ef7504bfd62de44c95924fbabb53a03e47).
An issue's existence is evidence to investigate, not evidence of an AIPass bug
or proof of a vendor-side cause.

### Applied in AIPass

| Area | Finding and change | Upstream evidence |
| --- | --- | --- |
| Devin empty replies | Reproduced a Connect reply containing only usage and a valid completion becoming a successful empty answer. It now returns 502 before any success headers. Text, reasoning or a tool call counts as output. Truncated/malformed streams and failures after content remain non-replayable. | [Devin 0.1.23](https://github.com/magpie-community/plugins/commit/397a890df5246f9c8470a0ee112b7a86f9bb3096) |
| Devin free-model quotas | Read account-specific protobuf model cost tiers. Exclude explicit tier-4 variants and family aliases only when every selectable effort resolves to a confirmed free variant. A failed metadata read leaves the visible quota advisory rather than imposing an unverified account-wide gate. | [Cost-tier parser and free IDs](https://github.com/magpie-community/plugins/blob/421117ef7504bfd62de44c95924fbabb53a03e47/packages/devin/index.mjs) |
| Qoder task continuity | AIPass supplied a changing `begin_at`, always used stage `start`, and omitted request/session IDs. Calls answering one user prompt now retain task ID/start time; successful completion advances the stage, while each call gets a new request/chat-record ID. State is scoped to the existing account generation and session, stores no credentials or prompt text, and is bounded to 512 tasks with a two-hour idle expiry. | [Qoder 0.2.31](https://github.com/magpie-community/plugins/commit/421117ef7504bfd62de44c95924fbabb53a03e47) |
| Qoder cache envelope | Preserve the user's block representation and mark the final eligible user block for the native prompt cache. Tool-result turns receive no invented cache mark. Align the body and existing signing envelope's client version to 1.1.69. | [Cache envelope](https://github.com/magpie-community/plugins/commit/dc59000), [runtime version](https://github.com/magpie-community/plugins/commit/d41a074) |
| Native session headers | Recognize DSH's `x-deepseek-harness-session-id` and WorkBuddy's `x-conversation-id` through the existing affinity path, preserving existing explicit-header precedence. Qoder hashes the account/session scope before placing its identifier in the vendor body. | [Qoder session change](https://github.com/magpie-community/plugins/commit/421117ef7504bfd62de44c95924fbabb53a03e47), [Magpie session contract](https://github.com/yetone/magpie/blob/be002aca050f01fce83f58728961614d3b987a63/docs/subsystems/gateway-routing.md) |
| Kimi Work tool schemas | Its direct subscription path skipped Moonshot schema normalization. Added bounded, deterministic schema-position traversal for derived `$ref` types, `anyOf`, enum/const types and untyped reference targets. Messages and literal defaults/examples remain untouched. The fix is scoped to Kimi Work's tool parameters. | [Kimi Work 0.1.5](https://github.com/magpie-community/plugins/commit/d324a96e1a599273109faf121d60d45da2e05c66) |
| Factory instructions | Reproduced the loss of all instructions following an Anthropic billing header in the same system block/string. Remove only the header line and preserve subsequent instructions and block metadata. | [Factory 0.1.37](https://github.com/magpie-community/plugins/commit/bb020ce80472d45eac2ce49b04a12e2821dcf163) |

Qoder's daily-count rule is not verified. Matching task fields to the official
client's reported contract is not a measured billing reduction. Its task cache
does not survive an Agent restart, and a request without a client session uses
a heuristic opening-message fallback. Qoder queue polling and the native
`ask/finish` lifecycle remain absent; no generation replay was added here.

### Checked without importing a change

| Recent upstream topic | AIPass assessment |
| --- | --- |
| [Cross-process single-use refresh tokens](https://github.com/yetone/magpie/commit/bac5be067744f1bcf5609d5bb790f41c5dc5c078) and [remaining in-band race](https://github.com/magpie-community/plugins/issues/332) | AIPass has one Agent per vault, an account lock around load/renew/use, generation/revision validation and durable ACKs. Its ordinary same-vault path does not create independent plugin hosts. This does not prove exclusion across different vaults, devices or an external CLI using the same vendor grant; that needs a credential-ownership design and concurrency tests, not an extra uncoordinated lock file. |
| [Devin permission_denied](https://github.com/magpie-community/plugins/issues/244) | The upstream investigation found the official CLI refused the same affected account too. Do not invent a device fingerprint or change authentication on that evidence. Distinguish vendor account/region refusal from parser faults. |
| [Antigravity disabled models](https://github.com/yetone/magpie/issues/3339) | AIPass already excludes explicit `disabled` entries, internal completion models and quota entries outside the enabled catalog. Its existing regression covers that contract. A vendor's missing/incorrect disabled flag still requires live evidence. |
| [MiniMax native effort](https://github.com/magpie-community/plugins/issues/316) | AIPass already maps Chat/Responses effort to Messages `output_config.effort`, and the MiniMax adapter retains it. It does not need the plugin-host conversion workaround described upstream. |
| [Kiro same-email sign-in methods](https://github.com/magpie-community/plugins/issues/309) | AIPass normally anchors Kiro on `profileArn` and rejects changed profile ownership; it does not use email alone for those records. Do not replace that anchor with a display label. |

### Further confirmed gaps and priorities

1. **Devin organization identity:** native import can derive `accountId` from
   `userId` or email, but neither that fallback nor the browser login explicitly
   binds the organization. The reviewed `GetUserStatus` fixture exposes
   `userStatus.planStatus.planInfo.devinInfo.orgId` and `userStatus.teamId`;
   `selectedOrganizationId` was not established by this evidence. Same-user
   organizations therefore lack an explicit billing scope.
   [Upstream fix](https://github.com/magpie-community/plugins/commit/a20c1b9)
   is relevant, but existing encrypted account identities need a migration and
   ownership-validation plan before changing this contract.
2. **Qoder browser sign-in:** its global login still supplies
   `redirect_uri=qoder-app://`. [Upstream removed it](https://github.com/magpie-community/plugins/commit/f5548bd)
   because the browser can hand authorization to the desktop app. No interactive
   sign-in was exercised in this audit.
3. **Grok rejected-token recovery:** the official CLI is asked to renew near
   expiry, but AIPass's explicit-revocation recovery branch is Codex-only.
   [Grok 0.2.1](https://github.com/magpie-community/plugins/commit/92d5557)
   adds one renewal on 401. AIPass should distinguish a revoked token from an
   unrenewable CLI and preserve its rule against replaying uncertain generation.
4. **Trae, MiMo and WorkBuddy coverage:** Trae's recent multi-function roster
   and native tool-history changes, MiMo platform Token Plan, WorkBuddy's free
   model daily window and personal package expirations need separate provider
   fixtures and account evidence. They are not covered by the fixes above.

These remaining identity, login and recovery changes were not implemented in
this pass. No direct grant-renewal engine, new credential storage or upstream
plugin runtime was introduced.

The [boundary adaptation design](subscription-boundary-adaptation-2026-10-23.md)
traces these gaps through AIPass's current ownership, import and persistence
paths, including migration and concurrency acceptance cases. It is a proposed
implementation sequence, not additional completed fixes.

## Validation

The changes cover Rust query/protocol normalization, session-header recognition,
bounded in-memory vendor task metadata and documentation. They use existing
authentication, outbound clients, typed snapshots and account-generation
bindings; they introduce no IPC, persistence schema, dependency or release
change. Checks follow the ordinary-logic scope in `agents.md` and include all
three affected crates and their shared conversion/routing tests.

- `cargo test -p aipass-agent -p aipass-proxy-conversion -p aipass-proxy`:
  827 passed, 3 ignored across seven suites. Includes subscription normalizers,
  shared refresh/snapshots, protocol conversions and proxy routing.
- The Devin completed-empty-reply and Factory joined-header regressions failed
  against their prior implementations before their corrections were applied.
- After the final Qoder fallback refinement, its three focused tests, Agent
  Clippy and Agent build were checked again; unrelated passing checks were reused.
- Desktop `SubscriptionUsageCard.test.ts`: 9 passed, including explicit zero,
  stale errors and the Console-only state.
- Formatting, Clippy with `--all-targets -- -D warnings`, and build passed for
  all three affected Rust crates.
- `pnpm repo:check` passed: 647 source files checked; the 21 existing oversized
  files retain their unchanged no-growth limits.
- The updated `aipass-revise` skill passed its validator. Inventory coverage,
  local documentation links and `git diff --check` passed.

No authenticated all-provider run, paid inference, remote CI, packaged desktop
runtime, deployment or release was performed by this audit. The existing UI
layout and IPC schema were not changed.
