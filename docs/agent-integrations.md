# Agent integration catalog

Agent management and provider/proxy quick integration share the desktop catalog
in `apps/desktop/src/lib/utils/integrations.ts`. Both show only Agents whose
executable, app bundle or supported editor extension is detected. Configuration directories
alone are not installation evidence. Zed's macOS app executable also counts.
Cline extensions must match their installed VS Code edition (Stable, Insiders or
Flatpak); another edition does not make retained extension files installed.
Detection does not launch an Agent or read its credentials.

| Agent | API credentials | Local proxy |
| --- | --- | --- |
| Codex | OpenAI | Responses |
| Claude Code | Anthropic | Messages |
| Gemini CLI | Gemini | Unavailable |
| OpenCode | Existing provider formats | Responses, Chat Completions, Messages |
| Grok Build, Pi | OpenAI, Anthropic | Responses, Chat Completions, Messages |
| Cursor Agent Local | OpenAI-compatible | Responses, Chat Completions, Messages |
| Kimi Code | OpenAI, Anthropic, Gemini | Responses, Chat Completions, Messages |
| Zed, DeepSeek Harness | OpenAI, Anthropic | Responses, Chat Completions, Messages |
| Antigravity CLI | Gemini | Unavailable |

Grok Build, Pi, Kimi Code, Zed and DeepSeek Harness require a default model.
Missing models disable a compatible installed Agent instead of hiding it.
Muse Code uses Responses. Hermes Agent and Command Code support all three local
proxy wires. CodeBuddy and WorkBuddy's shared models.json writer uses Chat
Completions. VS Code Chat, Cline, TRAE, Qoder, ZCode and JetBrains Copilot expose a clearly labelled
**Setup guide**, never a successful automatic-switch notice.

Direct subscription switching remains specific to Codex and Claude Code.
The complete current 22-tool catalog and runtime limits are maintained in
[quick-integrations.md](quick-integrations.md), including the native Meta catalog
requirement that prevents generic Muse Responses integration.
Cursor's IDE, regular CLI and separate Local runtime have different BYOK and
endpoint contracts; see [Linux quick integration and Cursor compatibility](quick-integrations.md).

## Additional writers

Rust owns configuration plans, redaction, writes, encrypted backups and imports.
New tool IDs pass through the existing authenticated Agent protocol and Tauri
bridge; the frontend never handles their keys.

- **Kimi Code**: `$KIMI_CODE_HOME/config.toml`, defaulting to
  `~/.kimi-code/config.toml`. Owns the `aipass` provider and selects an
  `aipass/<model>` alias. Helper mode uses `api_key_env` and generates
  `~/.aipass/tools/kimi-code.env`; source it before running `kimi`.
  This targets the current Kimi Code schema, not the retired `~/.kimi` schema.
- **Zed**: the shared XDG resolver supplies `zed/settings.json`. Selects the
  `aipass` OpenAI-compatible or Anthropic-compatible provider. Writes no key into
  settings or the system keychain. Source `~/.aipass/tools/zed.env` in a terminal
  and launch a fresh Zed process from that terminal. An already running process
  retains its original environment. The helper exports `AIPASS_API_KEY`, the
  variable derived from provider ID `aipass`.
- **DeepSeek Harness**: `$DSH_HOME/profiles/web/cordis.patch.yml`, defaulting to
  `~/.dsh`. Merges the `llm-pi-ai` provider and `agent-default-model` patches for
  the standard `dsh web` profile. Other profiles are not changed. Plaintext mode
  merges `AIPASS_DSH_API_KEY` into the versioned `.credentials.yaml` refs.
  Helper mode generates `~/.aipass/tools/deepseek-harness.env` and refuses a
  saved ref that would override that environment. Existing plugins, refs and
  grant records are preserved; YAML formatting/comments may be rewritten.
- **Antigravity CLI**: sets `modelProvider: gemini` in
  `~/.gemini/antigravity-cli/settings.json`. Source
  `~/.aipass/tools/antigravity-cli.env` before launching `agy`. It reads
  `GEMINI_API_KEY` and `GOOGLE_GEMINI_BASE_URL` from the process environment;
  it does not load `.env` files automatically. The Antigravity desktop app
  alone does not count as a CLI installation.

Zed and Antigravity management use helper mode. The new quick integrations also
default to helpers. Local proxy writes contain a local route token rather than
a provider key; Zed still requires sourcing its generated environment.
Kimi and Zed use a conservative 32,768-token context declaration because AIPass
credentials do not yet carry model context limits. This is not model discovery.

Each additional writer groups all its encrypted backups under
`~/.aipass/tools/.aipass-backups` so operation lookup and rollback cover settings
and credentials across directories. Managed switches fingerprint the source
files and import literal outgoing API credentials before writing. Existing Zed
keychain items, Kimi OAuth records and unrelated vendor sign-ins are not mutated.
Unsupported credential formats fail the managed switch before writes.

## Schema references

- [Kimi Code configuration](https://www.kimi.com/code/docs/en/kimi-code-cli/configuration/config-files.html)
- [Zed API access and compatible providers](https://zed.dev/docs/ai/use-api-access)
- [DeepSeek Harness provider configuration](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/user/guide/providers.md)
- [Harness local credential format](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/credentials/credentials-local/README.md)
- [Antigravity CLI API-key authentication](https://www.antigravity.google/docs/install?tab=cli)

Schema review and isolated fixture tests do not prove live vendor inference.

## Expanded BYOK catalog

| Surface | BYOK support and integration |
| --- | --- |
| Muse Code | Responses endpoint pinned in XDG `muse/settings.json`; helper exports `META_API_KEY`, plaintext uses versioned `auth.json`. |
| Hermes Agent | `$HERMES_HOME/config.yaml` or `~/.hermes/config.yaml`; merges `model` settings, exact API mode and literal key or an environment reference. Helper writes `hermes-agent.env`. |
| Command Code | Merges a credential-specific provider in `~/.commandcode/providers.json` and selects it in user `settings.json`. Helper uses the supported `!aipass get ...` reference. Local route tokens use `command-code.env`. Native auth is retained and snapshotted. |
| CodeBuddy | Merges user `~/.codebuddy/models.json`, keeps other models and `availableModels`, writes the full Chat Completions URL. Explicit plaintext confirmation is required. Select the added model in the editor. |
| WorkBuddy | Uses the officially retained `~/.codebuddy/models.json` format, shared with CodeBuddy. Select the added model in the editor. |
| VS Code Chat | Guided Custom Endpoint setup, supporting Chat Completions, Responses and Messages. VS Code owns its secret storage. |
| Cline | Guided provider setup in the extension or `cline auth` in the CLI. The installed extension entry point counts independently of a CLI installation. |
| TRAE | Guided custom model setup for the documented Chat Completions and Messages formats. No undocumented state database writes. |
| ZCode | Guided Model Providers → Add Provider setup for OpenAI-compatible Chat Completions and Anthropic Messages; add the model ID and enable it. ZCode retains its own credentials. |
| JetBrains Copilot | Guided model management → Add a custom endpoint setup for OpenAI-compatible Chat Completions. Requires a current Copilot plugin and an account/policy that permits BYOK. Detect the installed Copilot plugin JAR, rather than a JetBrains settings directory or an IDE alone. |
| Qoder | Guided setup through the IDE's Models page or CLI `/model → Custom`. Availability and endpoint fields depend on the account, edition and live provider catalog. The official CLI documentation explicitly says not to hand-edit BYOK in settings.json. Chat Completions proxy guidance is offered; account/catalog support must still be checked in Qoder. |

The setup guide copies API keys with the existing Rust `secret_copy` command;
only non-secret endpoint/model values are rendered. A guide neither changes
external state nor reports a managed switch. Windows app roots and Linux common
install paths are checked in addition to shared CLI discovery. Local validation
is on macOS; it is not Windows or Linux runtime evidence.

Additional references:

- [Muse's official endpoint and credential fixture](https://meta-models.github.io/muse-code-sdk/next/generated/examples/quickstart-journey/)
- [Hermes model providers](https://hermes-agent.nousresearch.com/docs/integrations/providers/)
- [Command Code BYOK schema](https://commandcode.ai/docs/byok)
- [Command Code settings](https://commandcode.ai/docs/settings)
- [VS Code Custom Endpoint reference](https://code.visualstudio.com/docs/agent-customization/language-models)
- [Cline CLI](https://github.com/cline/cline/blob/main/apps/cli/README.md)
- [CodeBuddy models.json](https://www.codebuddy.ai/docs/ide/Features/models)
- [WorkBuddy model configuration](https://www.codebuddy.ai/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model)
- [TRAE custom models](https://docs.trae.cn/ide_models)
- [Qoder custom models](https://docs.qoder.com/cli/custom-models)

## Audited surfaces outside the current quick catalog

- **Devin**: normal Cloud/Desktop/CLI documentation does not expose an arbitrary
  third-party model endpoint/key contract. The official enterprise FAQ explicitly
  rules out third-party LLM keys. Air-gapped federal deployments have a separate,
  licensed models-file contract; they are not treated as the normal Devin client.
  Do not add a misleading generic BYOK button or reinterpret a Devin service key
  as a model key. ACP agents retain their own separate configuration.
- **JetBrains Air**: both the app and IDE surface support provider API keys and
  existing agent authorization. This alone does not establish a stable arbitrary
  OpenAI-compatible endpoint contract for every built-in agent. Custom base URL
  support has separate upstream issues. Configure the supported underlying CLI
  through its existing AIPass integration; do not overwrite Air's account store.
  Air and JetBrains AI Assistant are different surfaces.

Sources for this audit:

- [ZCode custom providers](https://zcode.z.ai/en/docs/configuration)
- [Copilot for JetBrains custom endpoints](https://github.blog/changelog/2026-07-14-github-copilot-for-jetbrains-expands-byok-capabilities/)
- [Devin enterprise API-key limitation](https://docs.devin.ai/enterprise/deployment/overview)
- [Devin CLI model configuration](https://docs.devin.ai/cli/models)
- [Air-gapped Devin model schema](https://docs.devin.ai/federal/air-gapped/models-file)
- [Air provider authentication](https://www.jetbrains.com/help/air-ides/air-configure-authorization.html)
- [Air app provider setup](https://www.jetbrains.com/help/air/install-the-air-app.html)
- [Air custom base URL tracking](https://youtrack.jetbrains.com/issue/AIR-4541)

## Cross-platform review (2026-10-21)

- Both desktop entry points require current executable/app/plugin evidence. A
  configuration directory, extension marked obsolete, broken symlink or npm shim
  whose JavaScript target is gone does not count. Cline requires its VS Code
  host; JetBrains Copilot requires a matching installed IDE manifest and launcher,
  plus the installed plugin archive. Ordinary Cursor CLI does not count as Local.
- Protocol 19 retires older resident Agents before clients use the expanded tool
  IDs or installation-before-write semantics.
- API/proxy configuration writes recheck installation in Rust. Removing the
  executable after preview rejects the apply before imports or writes. Rollback
  remains available after uninstall.
- Windows uses USERPROFILE before a Git Bash HOME. Zed uses
  `%APPDATA%/Zed/settings.json`; Unix Zed uses XDG configuration. A detected Linux
  Flatpak-only Zed uses its sandbox configuration, not the host's unused XDG path.
  WSL tools and their Linux configuration are separate from native Windows tools;
  Windows AIPass does not treat `wsl.exe` as an installed vendor CLI.
- Launch helpers are `.env` on Unix and UTF-8 BOM `.ps1` on Windows. Load the
  displayed command in the same shell that starts the tool. Windows uses literal
  PowerShell strings and an encoded credential command compatible with both
  PowerShell and cmd.exe callers. No permanent environment edits or execution
  policy bypasses are introduced. Failed or empty credential lookup clears the
  previous value and stops helper loading.
- Credential previews redact complete sourced assignments, including compound
  shell quoting and doubled PowerShell apostrophes. Readers import only literal
  values; they never evaluate scripts or import a dynamic expression as a key.
- Native Windows CI now compiles desktop/Agent/CLI callers and runs the filesystem,
  PowerShell, detection, all-writer and rollback checks in
  `tests/windows_integrations.rs`. Existing Linux CI runs the Unix shell/rollback
  integration test and packaged Linux runtime gates. Adding a CI job is not a
  completed remote run; report its result only after the tested revision runs.
- Discovery covers PATH, package-manager shims and documented common install
  roots. Arbitrary portable/custom CLI directories outside those locations must
  be exposed through PATH. IDE plugin checks also require a matching host in the
  common app/Toolbox roots. It does not discover tools on another machine, WSL or
  removed IDE profiles as if installed in the current host environment.
