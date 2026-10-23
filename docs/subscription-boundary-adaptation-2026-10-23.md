# Subscription boundary adaptation

Status: source-lock, scoped-identity and device-local grant deliveries implemented,
2026-10-23, on the working tree based on
`a54dea72646a4f6e57fba1692a33c16b4c2b9d9b`.
Builds on the [quota audit](subscription-quota-audit-2026-10-23.md) and its pinned
upstream snapshots. Fixture results are not live-account validation.

## Decisions and delivery order

| Order | Boundary | AIPass adaptation | Main owner |
| --- | --- | --- | --- |
| 1 | Qoder browser handoff | Remove the global device flow's desktop redirect; retain device polling and PKCE. | Agent Qoder adapter |
| 2 | Grok early token rejection | One official-CLI recovery; retry read-only requests once after a verified token change. Preserve generation's no-replay policy. | Agent CLI account/session code |
| 3 | Devin organizations | Bind principal and billing scope independently of labels; migrate proven legacy bindings atomically. | Agent identity/import and encrypted vault records |
| 4 | Shared refresh grants | Coordinate references to the same local CLI store; separately constrain ownership of copied, rotating grants. | Agent credential coordination and vault storage |

Authentication remains in Rust and, for the five CLI-owned subscriptions, in
the official CLI. No plugin host or third-party provider engine is introduced.

## Delivered behavior and remaining scope

- Qoder's shared login URL builder omits the desktop callback in both regions,
  preserving PKCE and device correlation. Polling and token exchange remain in
  the extracted [login module](../crates/aipass-agent/src/subscriptions/qoder/login.rs).
- Grok uses a bounded source lock, fresh owner-checked reads and one official
  CLI renewal. Only model/billing GETs retry once; generation recovery applies
  to the next request. Torn reads wait up to about 500 ms. A successful CLI exit
  with unchanged credentials is a failure; a failed exit with valid changed
  credentials can recover. Directory aliases share a lock; credential-file
  symlinks refuse renewal because CLI atomic replacement could split the source.
- Devin browser login and native import both resolve the actual key's principal,
  server, organization and team. Legacy runtime reads migrate in one encrypted
  record write, preserving entry/secret IDs and routes, replacing the generation
  and clearing old quota/model/feedback state. New records use
  `community_account_v2`; removing v1 makes older readers fail closed. Repeated
  imports of the same legacy key upgrade in place. Source-key replacement is
  checked against the established scope without requiring the old key to work.
  Organization names and display email can change without changing a user-ID
  based identity; an email-based fallback remains pinned.
- MiniMax, Qoder job/device grants, Antigravity and Trae reread the current
  authoritative account after structured refresh rejection. A newer grant is
  retained for a later attempt. An unchanged rejected grant is retained but
  marked unusable through the durable auth ACK, preventing repeated refreshes.
  A later grant replacement clears that field's rejection. Generic 403, 429
  and 5xx responses do not trigger this state.

The local source lock coordinates cooperating AIPass processes, not arbitrary
external CLI writes. Factory, WorkBuddy, Cline and Kiro retain their specialized
refresh contracts; the shared rejection helper is not claimed to cover them.
Managed refresh grants now use encrypted device-local records and synchronized
`community_account_v3` descriptors. Supported adapters are Factory, Antigravity,
MiniMax Code (both regions), Cline, Trae (both regions), Qoder (both regions),
Kiro, WorkBuddy/WorkBuddy AI and Cursor when they return real refresh grants.
API-key-only accounts and official-CLI references retain their existing paths.

New-device and backup restores require local sign-in. Legacy shared grants stay
recoverable, but runtime use is blocked until explicit fresh authorization;
reimporting the identical old grant cannot silently migrate it. Successful
migration preserves entry/secret IDs and routes, removes old runtime keys and
clears old quota. Reconnecting an already migrated account replaces only its
local authorization, so other devices retain their own grants. Old clients lack
the v3 runtime key and fail closed; old backups and offline old clients cannot
be retroactively revoked. Vendor-specific independent authorization semantics
remain unverified because live accounts are unavailable by user instruction.

The desktop's reconnect action pins the original provider and entry ID. The
Agent also verifies identity, scope, the unlocked-session ID, descriptor and
local binding at commit. Cancellation, another reconnect, archive/delete and
session replacement cannot retarget a late login. Local authorization failures
are not published as another device's shared quota failure.

The sections below retain the design contract and its broader acceptance
criteria; the delivered scope and evidence above/below identify what has run.

## Qoder: finish device login in the browser

Before this delivery, [login](../crates/aipass-agent/src/subscriptions/qoder.rs) added
`redirect_uri=qoder-app://` for the global site even though completion arrives
through `/api/v1/deviceToken/poll`. The
[upstream correction](https://github.com/magpie-community/plugins/commit/f5548bd5ca30b663c41ad2671c2f0a1558552617)
reports the same parameter is absent from Qoder CLI's device flow.

- Extract a small login-URL builder and omit `redirect_uri` for both regions.
  Keep their existing hosts and client IDs, verifier/challenge, nonce and machine
  ID. Keep the CN device-token fallback and global job-token exchange separate.
- Do not add a loopback server or register a custom URL scheme. No stored-account
  migration is needed; the change affects newly started login attempts.
- Cover success, pending, cancellation and timeout in the polling fixture. Only
  add terminal denial/expiry mappings for documented or captured vendor codes;
  unknown responses must not be interpreted as successful authorization.
- Accept when URL tests prove all PKCE/correlation fields survive and neither
  region requests a desktop redirect. Live acceptance additionally requires
  sign-in with Qoder.app installed and absent, plus a CN account. Existing
  credentials must not be overwritten by a cancelled or late login result.

## Grok: rejected access is a recovery event

Before this delivery, [session renewal](../crates/aipass-agent/src/subscriptions/cli_accounts/session.rs)
already invoked `grok models` in the bound home. However, forcing it did not
check whether the rejected token changed; [Grok requests](../crates/aipass-agent/src/subscriptions/grok.rs)
did not recover on 401, and [revocation feedback](../crates/aipass-agent/src/subscriptions/feedback.rs)
only attempted Codex recovery. The
[upstream change](https://github.com/magpie-community/plugins/commit/92d555769b5d5b14282ad053d94fb12318930310)
also identifies interrupted reads while the CLI rewrites `auth.json`.

Use a provider-scoped recovery helper with the rejected access token held only
in zeroizing memory. Its result distinguishes changed credentials, unchanged
credentials, temporary read/CLI failure and account ownership change.

1. On HTTP 401 from the fixed Grok endpoint, take the credential-source renewal
   guard and reread the CLI store without a cache. Validate device, issuer,
   bound home and account identity. If another operation already replaced the
   rejected token with a usable token, consume that result without renewing.
2. Otherwise run `grok models` once in that home, within a bounded deadline and
   without interactive login. Reread after completion, including a failed CLI
   exit: an exit error alone does not prove the credential was not updated.
   Require the same owner, a usable expiry and a different token before calling
   recovery successful. A CLI success exit with the same token is unrenewed.
3. Retry model-list and billing GETs once after successful recovery. A second
   401 ends the attempt; 403, 429, 5xx and transport failures do not enter this
   auth loop. Keep the previous model/quota snapshot with its original
   observation time on a failed poll.
4. For generation, renew for subsequent requests but return the original
   failure for the current request. Retain the existing proxy replay rules;
   this change grants no additional retry permission after a send, stream,
   tool call or uncertain outcome. Do not copy upstream's generation replay.
5. A generic 401 or failed CLI execution does not mark the whole account
   revoked. Apply explicit revocation only to the rejected credential version,
   and only after a reread rules out a successful concurrent renewal. A changed
   account fails closed and requires reconnect.

Keep Grok file-read handling separate from the generic JSON reader. Retry an
empty or malformed file asynchronously for a bounded interval, initially about
500 ms. If it stays unreadable, report temporary credential unavailability and
retain account/quota data. Do not reuse a last-good token for a new generation
when the current store's owner cannot be verified. Permission failure, stable
missing login and a valid record for another owner remain distinct conditions.

Recovery guidance must name the actual source: the default Grok Build account
versus an AIPass-created private home. A default `grok login` does not repair a
private home. Prefer reconnecting the affected AIPass account; never expose raw
CLI stdout/stderr or credential material in diagnostics.

Acceptance fixtures: early 401 with future expiry; token already replaced;
one successful renewal and GET retry; unchanged token despite exit 0; changed
token despite nonzero exit; repeated 401; torn file repaired and unrepaired;
owner/device change; cancellation; and no generation replay. Real early-401
and official-CLI behavior remain an account-validation gate.

## Devin: separate the person from the billing scope

Before this delivery, [native import](../crates/aipass-agent/src/subscriptions/native_import.rs)
only asked `GetUserStatus` for an identity when no generic identity was available.
[Browser login](../crates/aipass-agent/src/subscriptions/devin.rs) returned the key
and server without the same enrichment. A JWT subject or email therefore does
not establish which organization funds a request. CLI import could rebind an
existing same-identity entry; browser login used a separate persistence path.

The [upstream fix](https://github.com/magpie-community/plugins/commit/a20c1b9d541da78ff10b62373d57293dc0974603)
uses an organization display suffix to distinguish saved slots. AIPass should
instead use a versioned structured identity: provider, normalized server,
principal and billing scope. Resolve these using the actual key's status:

- Principal: verified `userStatus.userId` when supplied, otherwise a scoped
  email fallback. Record the fallback kind so later enrichment cannot silently
  rekey an account. Do not use a display name or an unverified JWT alone.
- Billing scope: `userStatus.planStatus.planInfo.devinInfo.orgId`, with
  `userStatus.teamId` as an explicitly typed fallback. Retain both when present
  and check their consistency on subsequent observations. Do not use the
  unestablished `selectedOrganizationId` field from the earlier audit wording.
- Display: email and `accountDisplayName` are presentation metadata. Renaming
  an organization must not change its identity. Keep any user-defined title;
  do not place a composite display string in the email field.
- Missing scope is unknown, not a claim of personal-account ownership. Require
  a verified scope for organization deduplication; preserve ambiguous legacy
  entries separately and do not merge or retarget them on email alone.

Create one resolver used by browser login, CLI import, changed-key refresh and
legacy migration. New binds must resolve identity before final persistence.
Subsequent observations compare every established identity anchor; a scope or
principal change requires explicit reconnect. Check CLI model/credential reads
against the same source binding, including team information when available.

### Legacy migration transaction

[Account writes](../crates/aipass-agent/src/community/account.rs) intentionally
reject changes to identity and generation. Do not relax `ensure_owner` or make
ordinary `Context::save` capable of changing organizations.

1. Under the existing account coordinator, capture entry ID, secret ID,
   generation, revision and old auth. Query status with that stored key, not an
   ambient CLI key or a newly imported account that happens to share its email.
2. Derive the scoped identity and validate any established principal, server
   and scope anchors. After the network call, recheck the unlocked session and
   exact generation/revision before committing. No proof means no mutation.
3. Add a narrow Agent-internal scope migration backed by one encrypted record
   write. Preserve entry/secret IDs, route associations and user metadata;
   replace identity and runtime bundle together and rotate the binding
   generation. Use the atomic-record pattern of `bind_cli_subscription`, but
   give scope migration its own validation and audit semantics.
4. Clear old subscription snapshots and feedback within that record write;
   invalidate model/session/task caches and refresh active proxy credentials
   after commit. Reject stale in-flight writes. Preserve historical usage as
   history, not as a fresh balance for the new binding.
5. Make retries idempotent by reading back the committed scoped binding. A
   crash or failed notification must not create another entry or revert it.
   If the target scoped identity already exists, keep both entries and surface
   a conflict for explicit resolution; do not silently combine their routes.

Implement the vault operation in a real module: `aipass-vault/src/lib.rs` is
already under a no-growth limit. Reuse existing authenticated internal request
paths where possible; version the protocol if a new cross-process operation is
required. Frontend input must not constitute ownership proof.

Acceptance: same email/different org creates separate accounts; same org rename
retains identity; old identity migrates with stable entry/secret IDs; key/source
switch cannot retarget a binding; missing scope, network failure, duplicate
target, vault lock, changed revision and post-commit failure preserve recovery.
Exercise browser login and import, not just the status parser. Old clients must
reject the new binding version if they cannot enforce its scope.

## Shared refresh: coordinate the credential source

AIPass already has a canonical per-vault Agent, an entry-level account mutex,
revision checks and a durable auth ACK. `Context::refresh_guard` defers
cancellation during renewal; it is not a cross-process mutex. The
[Magpie lock](https://github.com/yetone/magpie/commit/bac5be067744f1bcf5609d5bb790f41c5dc5c078)
is relevant for rereading under a lock and checking state after failure. Its
shared plaintext-store assumptions do not transfer to separate encrypted vaults.

### References to one official CLI store

Add an AIPass process-shared renewal guard keyed by the canonical credential
source, not entry UUID or email. Different vaults can reference the same home;
different private homes for the same email must remain independent. Include
the actual secure-store namespace where a vendor uses Keychain rather than a
file. First ship this for Grok's file store; audit each other CLI separately.

Use an OS advisory lock with bounded asynchronous acquisition, consistent lock
order and crash release. A live holder is not displaced just because a lease
age elapsed. The guard spans reread, CLI renewal and verified reread, not the
whole generation stream. Lock identifiers must not expose credentials or
account labels; introduce no plaintext token cache. Coordinate AIPass native
account-switch writes to the same source with the same guard.

This serializes cooperating AIPass agents only. The official CLI retains its
own locking; do not pre-lock its refresh lock and then launch a child that
needs that lock. Reread after failures to recognize external renewal. Do not
claim exclusion against arbitrary external writers.

### Copies of a rotating grant in different vaults or devices

Before this delivery, managed adapters persisted refresh tokens in encrypted
runtime extensions, which participate in vault sync. Those records differ from the
[five CLI-owned references](native-subscriptions.md#data-and-routing). If vault
A rotates token R into R2, a lock does not give vault B access to A's encrypted
R2. Eventual sync also cannot provide exclusive refresh across offline devices.

The recommended long-term contract is one writable owner per rotating grant.
Use independent vendor authorizations per vault/device when supported. For a
CLI-owned account on one device, reuse its local source reference instead of
copying the grant. Verify per vendor that a new login actually yields an
independent authorization; separate directories alone do not prove this.

AIPass-managed rotating grants now have an explicit local binding and an
encrypted device-local credential record excluded from ordinary sync and backup. Sync the
account descriptor/reference; another device reconnects locally and receives
its own grant. This requires a dedicated storage/sync migration, not an extra
field on the currently synchronized runtime extension. Keep the logical
account's entry/secret IDs and routes stable across local binds.

Do not silently strip existing recoverable grants or elect a refresh owner
from two offline replicas. Existing ambiguous shared grants stay recoverable
and require an explicit local reauthorization/ownership transition. Their safe
migration needs version gating so an older client cannot continue refreshing
the copied grant. Backups containing old grants cannot be retroactively fixed;
vendor-side reauthorization or revocation is required where applicable.

The storage transition also retains the earlier failure handling: reread the current
authoritative source after `invalid_grant`; accept a newer validated revision,
otherwise retain the account and require reconnect. Do not repeatedly spend
the same rejected refresh token, clear all accounts, or claim this eliminates
cross-device races. A device-global credential broker is out of this scope.

Acceptance uses a fake single-use-token server and two actual Agent processes:
same source renews once, different sources proceed independently, waiter uses
the new token, holder crash releases the lock, slow live holder is not stolen,
old generation cannot write, and cancellation/durable-ACK failure remains
recoverable. Separately test two vaults with copied grants and offline synced
devices; they must demand the defined ownership transition rather than appear
fixed by the local lock. Process death after vendor rotation but before any
durable write remains an explicit reconnect case unless the vendor provides a
recovery contract.

## Other provider coverage

Keep the remaining provider work separate from credential changes: Trae needs
function-specific model catalogs and native tool-history fixtures; MiMo Token
Plan needs its actual product and allowance contract, distinct from Coding
Plan; WorkBuddy needs separate daily free-model windows and expiring personal
packages. None should become a generic account-wide quota inferred from model
names. Qoder queue/`ask/finish` behavior likewise needs vendor lifecycle evidence
before claiming full task parity or billing savings.

## Validation and completion criteria

Each delivery item gets focused failing-before/passing-after regressions.
Identity, auth ownership, storage or sync implementation also requires the
complete native macOS gate in
[repo guardrails](../.codex/skills/aipass-repo-guardrails/SKILL.md), with relevant
caller checks and artifact checks where prescribed. Shared-source locking
requires process-level tests, not only tasks sharing one memory mutex.

Keep fixture, installed-CLI probe, authenticated account, remote CI and release
results distinct. Credential operations never run against the user's current
accounts just to satisfy a test. The implementation adds URL, scoped-identity, same-source import, encrypted
migration/CAS, structured-rejection and HTTP recovery regressions. Grok's
single-use-token fixture runs two independent test processes through the real
renewal path and its injected fake CLI: both observe the new token and the
server receives exactly one rotation. Separate process tests cover independent
sources, bounded lock waits, live-owner exclusion and crash release. These are
production-module tests in independent processes, not two packaged Agents or
real vendor authorization tests.

Prior delivery checks (before the device-local grant change):

- `rust (macOS)`: workspace fmt, Clippy with warnings denied, test and build
  passed. Tests: 1,182 passed, 4 explicitly ignored (local CLI capability,
  interactive browser and opt-in Keychain fixtures).
- `node (macOS)`: frozen install, license audit, lint, typecheck, test and build
  passed. Embedded control-panel output remained unchanged.
- Repository source/dependency gate: 662 source files checked, with the existing
  21 size exceptions unchanged. Edited skill validation, diff whitespace and
  affected relative documentation links passed.
- `macOS desktop bundle`: ad hoc signed app/DMG and disposable-key updater
  archive passed. DMG layout/executables passed; main startup, tray startup,
  manual install/restart and cached install/restart all passed at 960x640 with
  responsive frontend/Agent and normal updater signature verification enabled.
  Runtime summary and artifact hashes: `runtime-reports/summary.json` under the
  log directory below. The disposable private signing key was removed.

Logs: `/tmp/aipass-boundary-validation-20261023`. No authenticated vendor account,
remote CI, Linux/Windows runtime or publication is included in this evidence.

### Device-local storage and current validation

`local-subscriptions/<generation>.aiplocal` is outside the export/snapshot
allowlist. Each write generates a random DEK; existing Rust local-state wrapping
protects the DEK with domain-separated vault-ID AAD. Payload AAD also binds the
canonical vault directory, machine identity and generation. Directory aliases
share an owner; copying or moving the vault requires a new local sign-in.
Decrypted buffers are zeroized, writes are atomic and owner-only, and unknown
versions/corruption fail closed. This local-state wrapping is stable across
password and synchronized epoch changes; it does not claim provider-record
epoch ratcheting or retroactive revocation of copied credentials.

Focused checks passed: encrypted backup exclusion, ciphertext tamper/version
rejection, changed device/directory, local and synced epoch/password changes,
two-vault isolation, fresh legacy migration, stale writes and reconnect races.
Two actual Agent subprocesses also exercised authenticated read/write IPC,
missing local grants, independent grants, cross-binding write refusal, vault
lock and unlock. Quota polling was disabled so fixtures contact no vendor.
This found and fixed a successful-write ACK mismatch (empty-object response
parsed as unit) that had unnecessarily entered the lost-ACK recovery path.
Desktop quota requests also use selection/request generations: stale errors,
loading state and late responses cannot follow a user to another account,
including switching back to the original account before its old request ends.

Current-tree validation completed on native macOS:

- `rust (macOS)`: format, Clippy with denied warnings, workspace test and build
  passed. 1,192 tests passed and 4 remained explicitly ignored. The initial
  default-parallel test run hit the existing sync-watcher fixture's 8-second
  bootstrap deadline. That test passed alone, then the entire workspace passed
  with `RUST_TEST_THREADS=8`; implementation and deadlines were not relaxed.
  Both the initial failure and successful rerun logs are retained.
- `node (macOS)`: frozen install, license audit, lint, typecheck, test and build
  passed. The later selection-race fix reran all affected Node checks. Embedded
  control-panel output remained reproducible; desktop has 412 passing tests.
- Repository checks passed for 670 source files with the existing 21 no-growth
  exceptions. Skill validation, whitespace and affected relative links passed.
- Chinese and English reconnect components were visually checked in a browser
  at 960x640, including the fixed provider, local-storage note and visible
  actions. This is component evidence, separate from packaged runtime checks.
- `macOS desktop bundle`: ad hoc app/DMG and disposable-key updater archive
  passed DMG layout/executables and all four runtime scenarios at 960x640:
  main startup, tray startup, manual install/restart and cached install/restart.
  Frontend and Agent responded, and updater signature verification stayed on.
  The disposable signing key was removed.

Logs, screenshots, source manifests and artifact hashes:
`/tmp/aipass-local-grants-validation-20261023`. Source HEAD remained
`a54dea72646a4f6e57fba1692a33c16b4c2b9d9b`, with the validated uncommitted diff;
no commit, push or release was performed. No authenticated vendor, remote CI,
Linux/Windows runtime or production signing/notarization result is claimed.
Live-account validation is explicitly deferred by the user.
