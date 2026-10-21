# Quick integrations on Linux

Reviewed on 2026-10-21. Tool discovery uses the login-shell PATH and user installs,
including `~/.local/bin`, NVM, FNM, pnpm, Bun and vendor directories. A file must
be executable; finding a configuration directory alone does not prove a working CLI.

| Tool | Configuration | AIPass local proxy |
| --- | --- | --- |
| Codex | `$CODEX_HOME/config.toml`, `auth.json`; defaults to `~/.codex` | Responses |
| Claude Code | `$CLAUDE_CONFIG_DIR/settings.json`; defaults to `~/.claude` | Anthropic Messages |
| Gemini CLI | `$GEMINI_CLI_HOME/.gemini/.env`, `settings.json`; defaults to `~/.gemini` | Unavailable: requires Gemini-native API |
| OpenCode | `$XDG_CONFIG_HOME/opencode/opencode.json[c]`; defaults to `~/.config/opencode` | Chat Completions, Responses, Messages |
| Grok Build | `$GROK_HOME/config.toml`; defaults to `~/.grok` | Chat Completions, Responses, Messages; default model required |
| Pi | `$PI_CODING_AGENT_DIR/models.json`, `settings.json`; defaults to `~/.pi/agent` | Chat Completions, Responses, Messages; default model required |
| Cursor Agent Local | `~/.aipass/tools/cursor-agent-local.env` | Chat Completions, Responses, Messages |
| Kimi Code | `$KIMI_CODE_HOME/config.toml`; defaults to `~/.kimi-code` | Chat Completions, Responses, Messages |
| Zed | `$XDG_CONFIG_HOME/zed/settings.json` | Chat Completions, Responses, Messages |
| DeepSeek Harness | `$DSH_HOME/profiles/web/cordis.patch.yml`; defaults to `~/.dsh` | Chat Completions, Responses, Messages |
| Antigravity CLI | `~/.gemini/antigravity-cli/settings.json` + sourced helper | Unavailable: requires Gemini-native API |
| Muse Code | `$XDG_CONFIG_HOME/muse/settings.json`, `auth.json` + sourced helper | Unavailable: requires Meta model catalog |
| Hermes Agent | `$HERMES_HOME/config.yaml`; defaults to `~/.hermes` | Chat Completions, Responses, Messages |
| Command Code | `~/.commandcode/providers.json`, `settings.json` | Chat Completions, Responses, Messages |
| CodeBuddy, WorkBuddy | `~/.codebuddy/models.json`; explicit plaintext mode | Chat Completions |
| VS Code Chat, Cline | Manual setup in the editor's model UI | Chat Completions, Responses, Messages |
| TRAE | Manual custom model setup | Chat Completions, Messages |
| Qoder | Manual custom model setup | Chat Completions, Responses, Messages |
| ZCode | Model Providers → Add Provider guide | Chat Completions, Messages |
| JetBrains Copilot | Model management → Add a custom endpoint guide | Chat Completions |

Environment/helper integrations need their generated env file sourced in the shell
that launches the tool. Claude's `apiKeyHelper`, Pi's `!command` and Command Code's
`!command` are invoked by the tool itself. $NAME is not portable across Pi releases. The legacy official package @mariozechner/pi-coding-agent 0.73.1 treats `$NAME` as a literal
key, unlike newer builds under @earendil-works/pi-coding-agent; command references work across both versions.
Use absolute config-directory overrides (Pi also accepts `~/...`).
The AIPass CLI must be on that shell's PATH and its vault must be unlocked.
Plaintext mode writes a key to the tool's configuration only after explicit selection.
Backups remain encrypted. Restart an already running tool after switching credentials.

## Cursor: three different surfaces

- **IDE:** officially supports provider BYOK under Settings → Models. The OpenAI
  integration also supports **Override OpenAI Base URL**. This does not apply to
  Tab completion or every Cursor model/feature. Enterprise policy can disable BYOK.
  All IDE BYOK requests pass through Cursor's servers, so the endpoint must be
  reachable from those servers; `localhost` and private LAN addresses do not work
  directly. AIPass does not automatically expose the local proxy to the internet.
- **Regular `agent` / `cursor-agent`:** the documented `CURSOR_API_KEY` authenticates
  a Cursor account. It is not an OpenAI/Anthropic provider key. The documented CLI
  has no provider BYOK configuration contract. A backend `--endpoint` is not an
  OpenAI model base URL.
- **`agent-local` / `cursor-agent-local`:** a separate package distributed by Cursor's
  internal Local installer, not installed by `https://cursor.com/install`. The
  installer currently offers Linux x64/arm64 and macOS. Verified runtime version:
  `2026.10.19-6bbf2c4`. It reads `CURSOR_LOCAL_AGENT_BASE_URL`,
  `CURSOR_LOCAL_AGENT_API_KEY`, and `CURSOR_ENABLE_AUTHLESS=1`.
  This is the surface AIPass configures; availability and compatibility are
  version dependent and are not a guarantee for the regular Cursor CLI.

Local startup after applying AIPass configuration:

```sh
source ~/.aipass/tools/cursor-agent-local.env
cursor-agent-local --authless --model <model>
```

Use an OpenAI-compatible credential for direct Local integration. For Anthropic
Messages, create an AIPass proxy route and integrate that route. Agent Local
discovers the protocol from AIPass's `/v1/models` `api_types` metadata. Its SDK
then appends `/messages` to the `/v1` base. Supplying `/v1/messages` produces
`/v1/messages/messages` in the verified runtime. A standard Anthropic models
response does not supply the protocol discovery extension. AIPass retains an
private `AIPASS_CURSOR_API_TYPE` comment in the proxy helper so subsequent
credential imports preserve the protocol without inferring it from the URL.

Official sources:

- [Cursor BYOK and request routing](https://cursor.com/help/models-and-usage/api-keys)
- [Cursor CLI authentication](https://cursor.com/docs/cli/reference/authentication)
- [Cursor CLI parameters](https://cursor.com/docs/cli/reference/parameters)
- [Cursor support: IDE base URL override and CLI limitations](https://forum.cursor.com/t/glm-5-2-support/163533/6)
- [Cursor Local installer](https://cursor.com/internal-local-install-nHZIc6RZgTXIfai0aO?darwin=true&channel=prod)

## Other tools: verified contracts and limits

- **Codex:** the official current configuration accepts only `wire_api = "responses"`.
  The existing writer sets this and selects its provider explicitly.
- **OpenCode:** this integration targets the released v1 `provider` / `npm` /
  `options` schema. The separately published v2 `providers` / `package` / `settings`
  schema is not interchangeable; v2 is not certified by this integration.
- **Anthropic authentication:** direct setup uses `x-api-key` except Claude Code,
  whose writer explicitly supports `ANTHROPIC_AUTH_TOKEN`. Bearer-only Anthropic
  gateways must use an AIPass proxy route for the other tools; the frontend and
  Rust reject unsupported direct setup instead of silently sending the wrong header.
- **Anthropic SDK endpoints:** Pi, Kimi Code, Zed and Harness remove a terminal
  `/v1` before their SDK appends `/v1/messages`, preserving gateway prefixes.
  Official Pi 0.73.1 was run against localhost: the old base requested
  `/v1/v1/messages` and exited 1; the corrected base requested `/v1/messages`,
  authenticated, printed `OK` and exited 0.
- **Grok Build and Pi:** detection, writing and outgoing credential snapshots share
  the configured home resolver. Set overrides in the AIPass Agent's environment
  as well as the shell launching the vendor tool.
- **Kimi Code, Zed, Harness, Hermes and Command Code:** current official configuration
  contracts support custom model endpoints. Source generated environment helpers
  before launching, where required. Zed must start as a fresh process to inherit
  the key. Harness targets the standard `web` profile only. Hermes changes the
  main model; existing auxiliary model overrides retain their own configuration.
- **Muse Code:** real macOS binary `1.4.11-R6667.1` was verified against its official
  manifest hash. With a configured model and a custom `/v1` base, it still requests
  `GET /muse-code/models` at the endpoint origin and exits on 404 before inference.
  The writer therefore accepts only the native `https://api.meta.ai/v1` endpoint;
  generic Responses credentials and AIPass proxy routes are not offered.
- **Gemini CLI and Antigravity CLI:** their custom endpoint must speak the native
  Gemini protocol. AIPass's three current proxy protocols are not substitutes.
- **CodeBuddy and WorkBuddy:** official model settings retain `~/.codebuddy/models.json`.
  Their CLI, IDE and extension builds have different capabilities; do not infer
  native WorkBuddy Linux desktop availability from a CodeBuddy CLI installation.
  No native WorkBuddy Linux desktop artifact was verified in this audit.
- **VS Code Chat, Cline, TRAE and Qoder:** these are setup guides, not automatic
  secret-store edits. Select the exact API type in the tool's UI. Current Qoder
  desktop supports both OpenAI API types and Anthropic-compatible custom endpoints;
  Qoder CLI has a separate, account-dependent wizard. Its presence does not prove
  every desktop option exists in that CLI build. VS Code's custom endpoint Messages
  auth defaults to `x-api-key`; bearer-only gateways require custom auth headers.

Linux packages or install paths are documented for the CLI tools above, Zed,
VS Code, TRAE, Qoder and ZCode. That establishes platform availability, not completed
vendor inference on Linux. The manual guides and WorkBuddy's desktop require
separate platform/runtime acceptance. No local Linux container or VM was used.

ZCode and JetBrains Copilot are also manual guides. ZCode supports OpenAI and
Anthropic custom providers; its current documentation does not establish Responses
support. GitHub documents OpenAI-compatible custom endpoints for its JetBrains
Copilot plugin; this is distinct from JetBrains AI Assistant and Junie. Their Linux
IDE availability does not establish a successful plugin BYOK model call.

Official references (reviewed 2026-10-21):

- [Codex configuration](https://learn.chatgpt.com/docs/config-file/config-reference)
- [Claude Code settings](https://code.claude.com/docs/en/settings)
- [Gemini CLI configuration](https://geminicli.com/docs/reference/configuration/)
- [OpenCode v1 providers](https://opencode.ai/docs/providers), [v2 providers](https://opencode.ai/v2/docs/providers)
- [Grok Build settings](https://docs.x.ai/build/settings)
- [Pi model configuration](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/models.md)
- [Kimi Code configuration](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files.html)
- [Zed API access](https://zed.dev/docs/ai/use-api-access)
- [DeepSeek Harness providers](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/user/guide/providers.md)
- [Antigravity authentication](https://www.antigravity.google/docs/install?tab=cli)
- [Muse Code](https://dev.meta.ai/docs/muse-code), [official catalog fixture](https://meta-models.github.io/muse-code-sdk/next/generated/examples/quickstart-journey/)
- [Hermes model configuration](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/configuring-models.md)
- [Command Code BYOK](https://commandcode.ai/docs/byok)
- [WorkBuddy model configuration](https://www.codebuddy.ai/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model)
- [VS Code custom endpoints](https://code.visualstudio.com/docs/agent-customization/language-models)
- [Cline compatible endpoints](https://docs.cline.bot/provider-config/openai-compatible)
- [TRAE models](https://docs.trae.ai/ide/models), [official Linux announcement](https://forum.trae.cn/t/topic/2520)
- [Qoder custom models](https://docs.qoder.com/qoder/custom-models), [CLI custom models](https://docs.qoder.com/cli/custom-models)

- [ZCode custom providers](https://zcode.z.ai/en/docs/configuration), [Linux distributions](https://zcode.z.ai/en/docs/install)
- [GitHub Copilot for JetBrains custom endpoints](https://github.blog/changelog/2026-07-14-github-copilot-for-jetbrains-expands-byok-capabilities/)

## Verification and limits

`cargo test -p aipass-config-writers` includes a Unix integration test that executes
shell helpers with synthetic credentials, exercises all 16 automatic writers in isolated
homes containing spaces and Chinese characters, checks owner-only file permissions,
and applies/rolls back primary and auxiliary files. It runs under the existing Linux
Rust CI job as well as native macOS. These checks do not run vendor model inference.

For an installed official Local runtime, run:

```sh
node scripts/verify-cursor-local.mjs /absolute/path/to/cursor-agent-local
```

This tests three localhost mock APIs, authenticated requests, endpoint selection,
streamed `OK` output and successful process exit. It does not use real provider keys.
Run it on each target OS to obtain runtime evidence for that OS.
The Linux packaged-runtime CI job downloads the pinned official Local package
and runs this check. The same shared harness tests both official Pi packages (legacy 0.73.1 and
current 1.3.0) via
`node scripts/verify-pi-local.mjs /absolute/path/to/pi`; it exercises all three
protocols and a credential command under an isolated home, with extensions,
tools, context discovery, telemetry and startup network refresh disabled.
The Linux job also downloads both pinned Pi packages and writes `pi-local.jsonl`.
Pi 1.3.0 adds `?beta=true` to Anthropic requests; the path remains `/v1/messages`.
Its Cursor artifact includes `cursor-local.jsonl` with the OS,
architecture, vendor version and all three request paths.

`scripts/verify-linux-runtime.mjs` separately checks packaged startup and invokes
`verify-linux-native-clis.mjs`. Its default vendor executables are fixtures:
discovery/import success is not proof that each official CLI completed paid
generation. Linux CI results apply only to the source SHA actually tested.


For Windows helper paths, strict installation evidence, stale-preview rejection
and the native Windows CI boundary, see the [cross-platform review](agent-integrations.md#cross-platform-review-2026-10-21).
