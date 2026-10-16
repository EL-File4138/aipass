# 订阅能力与 provider 审查（2026-10-16）

本次覆盖 AIPass 当前支持的 17 个订阅 provider ID。比较基线为
[Magpie a6f908b](https://github.com/yetone/magpie/tree/a6f908ba587cc48af626752d9d9797752417b7e5)
及其已迁移的
[community plugins 6a3fe9b](https://github.com/magpie-community/plugins/tree/6a3fe9b49e7e638ed79f00d274ad6e1b93a384cd)。
插件源码只用于仓库外的只读比较；实现、凭据权威和协议转换继续由 AIPass 的 Rust crates 持有。

## 本次已落实的共享能力

| 能力 | 实现与边界 |
| --- | --- |
| 同组多个订阅账号 | 每个账号保留独立 entry、凭据标记和 provider 身份。同一 Claude 邮箱按组织区分；Copilot Enterprise Cloud 按 host 区分；Gemini 显式 project 和 WorkBuddy enterprise 也进入身份。绑定改变时，旧轮次不能冒充新账号。 |
| 响应额度反馈 | Codex 的 `x-codex-*` 与 `codex.rate_limits`、Claude CLI 的 `rate_limit_event` 更新加密状态并刷新路由。部分窗口单独记录时间；过期读数不会因其他窗口更新而恢复新鲜。额度写入失败不丢弃原始生成、不重放请求。 |
| 失效凭据 | HTTP 订阅仅在明确的结构化撤销码和 HTTP 401 同时出现时保存凭据摘要。新凭据可恢复。Codex 通过官方 CLI 做一次 20 秒以内的续期，只供后续请求使用。Claude 的认证和续期仍由真实 CLI 执行，未新增独立的 OAuth grant 管理器。 |
| Codex 请求与历史 | 两个传输共用 header/catalog 契约；移除不支持的显式缓存选项；保留调用方 instructions/tool choice；请求加密 reasoning；补历史搜索的 cached-only 声明；保留 V2 compaction 输入、触发字段和输出。加密 reasoning/compaction 摘要持久绑定账号及 target，不能组内跨账号续接。 |
| Claude 完成轮次恢复 | 保存最长 24 小时、每账号最多 8 个完成轮次的加密检查点。恢复绑定账号标记、target、上下文、历史摘要和 workspace；通过官方 `--resume --fork-session` 继续。文本轮次和已完成的工具历史可以恢复。未完成工具调用、图片/文档轮次和超过 4 MiB 的 transcript 不进入检查点；不重新执行工具来伪造恢复。CLI 原生 transcript 仍由 CLI 自己管理。 |
| 原生自动切号 | 默认关闭。首选账号显式指定候选分组，只在同一 provider 的可验证 CLI 账号间切换；已耗尽才离开，首选账号恢复到 90% 以下才按设置返回。不会把未知额度视为耗尽，也不会选择未知额度候选；手动改号、活动 Claude 轮次或配置冲突会阻止自动切换。复用现有加密备份、续期验证和事务回滚；运行中的外部 CLI 仍需要用户重启。 |
| Codex 点数与 reset | 付费点数、自动 reset 都默认关闭。关闭付费点数时，提交生成前经官方 CLI 读取额度；额度耗尽返回 429。手动和自动 reset 使用官方 `account/rateLimitResetCredit/consume`，先持久化幂等键；结果未知时复用原操作，不能另起一次消费。额度检查与服务商实际计费之间仍有竞态，不能声称是服务商账单硬上限。 |
| 操作入口 | 桌面账号额度设置、typed Agent IPC、CLI `accounts policy/reset`、MCP `subscription_policy_get/set` 与 `subscription_reset` 共用 Rust 权威。内部反馈和历史读取不刷新用户空闲时间。Agent protocol 升至 18。 |

## 逐 provider 结果

| Provider ID | 登录、续期、协议与额度审查 | 本次修正 / 仍有差距 |
| --- | --- | --- |
| `anthropic` | Claude Code 原生 home，组织身份，MCP 工具桥和 Anthropic streaming；五小时、周及模型族额度。 | 修复组织迁移、默认 profile、登录目录生命周期；新增被动额度与完成轮次恢复。未完成工具和附件检查点按上表边界处理。 |
| `codex` | 官方 CLI 管理授权及续期；Responses 流、live models、按 limit ID 的额度。 | 新增反馈、撤销恢复、加密历史归属、原生切号策略、点数/reset 设置；主额度不再被只有模型专项额度的 map 隐藏。现代压缩走原生 Responses V2；没有伪造供应商加密 compaction 项。 |
| `gemini-cli` | 官方 CLI 管理 OAuth；Code Assist project envelope、模型范围额度与签名续接。 | 原先忽略显式 project；现在读取该 CLI home 的 `.gemini/.env`，支持两个官方 project 环境键并拒绝执行 shell 表达式。project 参与身份。开户/onboarding 与额外资格验证仍需在官方 CLI 完成；尚无真实企业 project 联调证据。 |
| `copilot` | 独立 CLI token 直接使用其集成，账号 API、live models、premium 范围。 | 修复只发 Chat Completions；按模型声明选择 Chat/Responses/Messages，保留模型能力。新增同名不同 GHE host 的身份隔离、tenant API 白名单。明确 premium 标记参与额度范围。原生 CLI 订阅路径尚未实现 Copilot Auto 的完整 session/terms 流程，不再把 `auto` 当普通聊天模型发布；旧 editor 路径已有部分 Auto 处理。 |
| `grok` | 官方 CLI home/token、Responses、live reasoning levels、billing 额度。 | 共用 HTTP 撤销反馈。未执行真实 Grok 续期、on-demand 额度消费；原生自动切号仅覆盖 Claude/Codex。 |
| `zcode` | 国内/国际 site、组织/project 身份；Start Plan capability 额度与 GLM Coding 窗口、MCP advisory 窗口。 | 保留现有独立范围，不用重置时间相同来猜测 API key 与订阅是同一个 seat。赠送模型、签到及跨登录形态同 seat 合并仍有差距。 |
| `qoder` | job/device 两组 token 独立续期；国际站模型列表、protobuf/SSE、个人/共享/专属钱包。 | 现在保留供应商返回的周期结束时间。签到、活动和设备身份运行时没有接入；多钱包的可替代性仍需真实样本确认。 |
| `qoder-cn` | 与国际站分别处理 client、host、账号和目录，复用协议模块。 | 同上；不能用国际站账号或 device token 替代国内站。 |
| `devin` | 原生配置、server 白名单、live model families、Connect/protobuf streaming、日周及 ACU 额度。 | 修复 quota-billed protobuf JSON 省略零字段时漏掉耗尽窗口；保留 overage balance 和周期。隐藏窗口、非 quota 计费和缺数据不推断为耗尽。 |
| `zed` | RSA 浏览器登录、organization、短期 LLM token、按 vendor 原生协议包装；只对明确过期 token 重取。 | billing period 现在进入公共快照。与 Magpie 的接口一样，只能展示接口实际返回的计划/账期/欠费状态；没有伪造已用百分比。 |
| `factory` | WorkOS 旋转 token 的 durable ACK、organization 修复、region endpoint、Standard/Core 窗口及 extra usage。 | 修复把 Gemini 归入 Core 的错误；Google 与 Anthropic/OpenAI/xAI 属于 Standard。当前模型目录与特殊 wire 仍需随官方 Droid 更新维护。 |
| `commandcode-plan` | 浏览器/本地 API key、plan 缓存、Go 专用生成转换及其他计划的原生端点。 | 共用 HTTP 撤销状态；额度与周期解析继续按供应商数据。未扩展自动接受新条款、自动购买或额外付费操作。 |
| `mimo-app` | Xiaomi 登录、区域 SSO/cookie、原生桌面订阅与周额度。 | 修复 numeric/string user ID 比较不一致可能漏掉换号。账期保留；不通过默认值假装未知额度可用或已耗尽。 |
| `cursor` | 原生账号导入、Connect 双向请求、工具交互、live models 与 Cursor/Other pool。 | 现有 Rust 直连实现保留。Magpie 新增的独立 native-session transport、deferred usage 和完整缓存生命周期尚无等价实现；需单独按 Cursor 官方 CLI 契约补齐，不能导入其插件引擎。 |
| `kiro` | 区域与 profile 绑定、OIDC/原生续期、AWS eventstream、月度及活动 trial 额度。 | 现有凭据和 profile 保护保留。bonus、多池额度的替代/合并规则及更多企业 region 需要真实接口样本；未从不明字段推断可消费余额。 |
| `workbuddy` | 国内登录/本地导入、旋转 token、live product config 和 Chat 请求。 | 新增 enterprise identity、tenant headers、企业额度接口；免费模型排除付费窗口。旧记录若缺失企业范围且当前桌面已选企业，会要求重新连接，避免将企业凭据作为个人账号继续使用。企业选择不明确时要求在官方客户端选择。签到和详细 package 到期视图尚未补齐。 |
| `workbuddy-ai` | 国际站独立 host、原生来源与请求 headers，复用范围保护。 | 同上；国际/国内身份、令牌和企业配置不混用。 |

## 官方契约与验证边界

除比较源码外，本次核对了本机 `codex app-server generate-json-schema`、`claude --help`，以及：

- [Codex 当前 V2 compaction 实现](https://github.com/openai/codex/blob/main/codex-rs/core/src/compact_remote_v2.rs)。
- [Codex reset-credit 客户端](https://github.com/openai/codex/blob/main/codex-rs/backend-client/src/client/rate_limit_resets.rs)。
- [Gemini CLI project/setup 契约](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/code_assist/setup.ts)。
- [ProtoJSON 缺省值与字段存在性](https://protobuf.dev/programming-guides/json/#presence-and-default-values)。

Provider 源码审查、mock 回归和本地 CLI capability 检查不能证明真实订阅可用性。
本次没有对真实账号执行登录授权、付费生成、reset 消费或原生自动切号。
没有安装新桌面包、发布版本或触发远程 CI。macOS 本地验证结果见
[本次 review](subscription-review-2026-10-16.md)。
