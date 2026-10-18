# 订阅接入与 Magpie 差距审计（2026-10-18）

本轮目标是识别可增加的入口、核对现有实现的能力差距，并修正能够通过当前官方协议确认的问题。候选入口和待修项目不代表已经实现或通过真实账号验收。

## 对照基线与覆盖

- [Magpie d783df4](https://github.com/yetone/magpie/tree/d783df46cbac9021e26a6bd71ca0b55544552eb2)，提交时间 2026-10-17 23:13 UTC。
- [community plugins 0a998a8](https://github.com/magpie-community/plugins/tree/0a998a8a7fea95161a3d13d03f3b4594cf6e2fad)。该版本与前两轮使用的 community 基线相同。
- AIPass 当前有 34 个通用订阅/套餐 provider ID，另有 Claude 官方 CLI 入口。中国、国际等独立 ID 不能重复计为不同产品。
- community 注册表有 24 个 provider ID，AIPass 覆盖其中 19 个。差集是 `opendesign`、`raccoon`、`autoclaw`、`autoclaw-cn` 和 `opencode-zen-free`；最后一项已在前轮实测发现官方客户端限制，继续保持不提供匿名免费入口。

只在仓库外读取对照源码与协议资料，没有添加第三方引擎、源代码依赖、运行时插件或适配器下载器。不能从注册表名称重合推导运行时能力等价。

## 可以增加的入口

以下优先级是结合本仓库架构与接入成本的建议。

| 优先级 | 入口 | 当前差距及实施要点 |
| --- | --- | --- |
| P1 | Kimi Code 中国 / 国际 | 目前只有按量 Moonshot 和本机 Kimi Work；缺少 Kimi Code 会员 key 入口。官方提供 Chat、Responses、Messages 与独立中国/国际地址；应读取账号模型和会员用量，保持客户端真实身份。 |
| P1 | MiniMax M Plan / Coding Plan key | 现有 MiniMax Code 是账号 OAuth，普通 MiniMax API 是通用 key；缺少订阅 key 的独立目录、区域与额度。官方现在以 M Plan 展示，仍须区分订阅 key、余额 key 和 credit packs。 |
| P1 | 百炼 Coding Plan 中国 / 国际 | 当前百炼入口是 Token Plan。固定月费 Coding Plan 使用 `coding.dashscope.aliyuncs.com/v1` / `coding-intl.dashscope.aliyuncs.com/v1`，与 Token Plan 产品、key 规则和计费不同，必须单独建入口。 |
| P2 | Open Design / Design Plan | community 已有设备授权、control/runtime 双 key、模型和额度实现可作协议对照。AIPass 尚无入口；需把两种 key 的用途和账号/工作区边界放在 Rust，按模型选择实际 API。尚未用真实账号确认。 |
| P2 | 商汤小浣熊 Raccoon | community 支持浏览器登录和本机 `~/.box-agent/config/auth.json` 只读引用。可优先做只读本机接入，校验账号后每次重读；浏览器续期和积分模型另行验证。 |
| P2 | Ollama Cloud | 有官方 cloud key、云模型和兼容 API；应与本地 Ollama 分开，补预设/账号入口。额度读取还需确认服务端契约，不从本地 token 反推套餐剩余。 |
| P2 | Kilo Gateway | 可增加 key、组织范围、模型和稳定 session 支持；属于网关余额/免费模型服务，不能把它宣传成包月订阅。采用当前官方 API 和自有客户端身份。 |
| P3 | AutoClaw 中国 / 国际 | community 有两种区域账号。涉及短信/验证码、浏览器回调、特殊网关和一次性 refresh；其文档说明与桌面端共享 refresh 会互相影响，需要独立授权与持久化恢复后再开放。 |

参考：[Kimi Code 官方接入](https://www.kimi.com/code/docs/)、[MiniMax M Plan](https://www.minimax.io/m-plan)、[Qwen Code 当前认证指南](https://qwenlm.github.io/qwen-code-docs/en/users/configuration/auth/)、[Ollama Cloud](https://docs.ollama.com/cloud)、[Kilo Gateway](https://kilo.ai/docs/gateway/api-reference)。Open Design、Raccoon 和 AutoClaw 的候选判断来自固定版本的 community 实现，不代表官方承诺或真实账号验证。

## 现有接入应修缮的部分

### 已修正：Token Plan key 被误套 Coding Plan 前缀

`subscriptions/plans.rs::fresh` 原先要求 Qwen/Bailian Token Plan key 必须以 `sk-sp-` 开头。当前官方 Qwen Code 认证指南明确指出：百炼的该前缀属于 Coding Plan，Token Plan 没有此要求。Qwen AI Token Plan 也不应在没有其格式约定的情况下套用另一个产品的前缀规则。

本轮移除这项推断校验，保留 provider/region 的固定专用端点、key digest 账号绑定、凭据持久化 ACK 和 changed-key 重连校验。没有增加其他计费产品的 fallback。新增回归覆盖 Qwen、百炼中国、百炼国际的非前缀 key、重复读取和更换 key。

中英文额度空态同步改为引导用户到控制台查看，避免把“尚未集成额度读取”误写为“服务商没有额度接口”。原扩展记录中 Token Plan 的 key 和计费表述也已纠正。

### P1：OpenCode Go 的模型与额度

本次无凭据公开 GET 的结果：

- `https://opencode.ai/inference/go/v1/models` 返回 HTTP 200，共 37 个模型。
- 其中 7 个 ID 没有出现在当前 Go 协议元数据中：`glm-5.1`、`grok-4.6`、`omen-alpha`、`kimi-k2.6`、`minimax-m2.5`、`qwen3.6-plus`、`qwen3.7-max`。由于列表本身只返回基本 ID，当前解析会跳过它们。
- `https://opencode.ai/inference/go/v1/usage` 返回 HTTP 401。官方 [usage 路由](https://github.com/anomalyco/opencode/blob/60a5a6ca7ce176b04cd2321ffd1819b1ea8e6006/packages/console/app/src/routes/zen/go/v1/usage.ts) 与 [inference 映射](https://github.com/anomalyco/opencode/blob/60a5a6ca7ce176b04cd2321ffd1819b1ea8e6006/packages/console/app/src/lib/inference-proxy.ts) 确认这个 key 认证接口存在；AIPass 当前仍只显示控制台提示。

应补可更新、校验过的模型协议元数据和 Go 的额度读取。不能从模型名字猜 wire，也不能把同一模型在 Zen 的 wire 直接套到 Go。读取额度后还需区分免费模型和计费模型，避免 Go 套餐耗尽后把免费模型也排除。[Go 官方文档](https://opencode.ai/v2/docs/console/go/) 明确允许达到限额后继续使用免费模型。

401 只证明路由及认证边界存在，不能作为成功解析真实额度的证据；需账号响应 fixture 和真实账号验收。

### P1：Trae 工具协议仍是部分实现

目前可处理完整 JSON `<tool_call>` 与标准 native tool calls，已验证 tool choice、历史 ID、图片顺序、文本代码引用、完成原因和用量。但 community Trae 还处理 GLM 的 `<arg_key>/<arg_value>` 模板、工具名中的信封，以及其他供应商输出形式；这些在 AIPass 尚未完整覆盖。

应以声明过的工具和参数 schema 为边界，补完整合法编码，测试拆包、字符串类型、重复字段、未知工具与截断；不要照搬对损坏 JSON 的宽松修复，也不能在已有输出后重放生成。当前启用文本工具时的缓冲策略也应在保留完整调用校验的同时改善文本延迟。

企业动态主机、设备证明和地区账号仍需真实账号样本。community 自身也明确说明企业账号未实测，不能将它的分支当作已验证的标准。

### P2：其余套餐额度与模型发现

| 入口 | 当前 AIPass | Magpie 对照与建议 |
| --- | --- | --- |
| StepFun Step Plan | 推理 key、分区、模型；额度控制台查看 | Magpie 另存平台 session，读取窗口并消费平台 refresh；推理 key 本身不能查同一接口。若补，应设计独立授权和每账号绑定，不能把其他浏览器账号的额度贴到 key 上。 |
| 火山 / 腾讯云 | 固定套餐推理端点，额度控制台查看 | Magpie 另接云管理 access key / secret。若补，应使用单独加密凭据、只读权限和明确计费范围，不拿推理 key 伪装成管理凭据。 |
| Qwen / 百炼 Token Plan | 文档模型表，暂未读取 live catalog | 当前 Qwen Code 认证指南描述从对应端点发现模型，应补动态列表、分页/类型过滤和最近成功缓存；保留静态目录作为失败回退。 |
| MiniMax Code | OAuth、动态模型、会员余额和 M Plan 窗口 | 补官方 M Plan 产品/字段演进的 fixture 和真实续期验收；不要将目前账号 OAuth 直接等同于新增订阅 key 模式。 |
| Kimi Work | 只读本机引用、动态模型、会员共享额度 | 与 community 的只读设计一致；保持独立于 Kimi Code，勿为追求统一而消费 Work 客户端 refresh。 |
| Cline | 登录、目录、余额与精确模型窗口 | 前轮已修正余额局部失败影响和模型范围；可另补 provider pinning 等可选路由参数。不要将网关 key 余额等同于 ClinePass 窗口。 |

### P2：共有的订阅体验

Magpie 还有 reset 优先的 smart、按剩余时间分配周额度的 pace，以及可选择的按窗口用量上限。AIPass 当前路由策略为 Fallback、RoundRobin、QuotaAware，应优先补“额度使用到指定百分比就切换”和按 reset 时间排序；配合每模型范围、余额支出策略、session affinity 和已有流不重放测试。

签到与 warm-up 是独立功能，不是查询额度必须附带的动作。若增加，要求默认关闭、单账号幂等、明确时间/结果，并保持额度查询只读。当前 paid credits 的生成前检查主要在 Codex 路径实现，不能把同名设置描述为对全部供应商都有同等支出控制。

额度空态还可补直接打开控制台的操作，目的地应绑定当前账号的 provider/region；读取失败、尚未读取和尚未集成读取应分别呈现，避免只有一个永远返回未知额度的刷新按钮。

Claude、Codex、Copilot、Gemini CLI、Grok 的官方 CLI 凭据归属，以及 ZCode、Qoder、Devin、Zed、Factory、Cursor、Kiro、WorkBuddy、MiMo App 的现有入口继续保留。本轮目录对照没有发现这些家族还缺一个独立同名订阅入口；这项结论不替代每家的登录/续期/工具/额度真实回归。

## 不建议增加的伪可用入口

- OpenCode Zen 匿名免费：前轮真实公开生成返回客户端限制，不通过冒用官方客户端 UA 绕过。
- Qwen OAuth 免费套餐：当前官方认证文档标明 2026-04-15 已停止，不能依据旧博客或仍留在代码中的 legacy provider 再提供一个可连接按钮。
- 通用 npm auth/plugin 引擎：与本仓库 Rust 拥有认证、额度、协议转换的边界不符。新增服务以原生模块接入。

## 实施顺序与验收

1. 先修正已确认的 Token Plan key 校验和文案；补百炼 Coding Plan、Kimi Code、MiniMax 订阅 key 的独立入口设计。
2. 补 OpenCode Go 模型元数据刷新与额度；补 Trae 合法工具编码。完成真实账号的登录、续期、流式生成、工具往返和耗尽切换。
3. 加 Open Design / Raccoon / Ollama Cloud / Kilo；AutoClaw 在验证码和旋转凭据恢复链路准备好后增加。
4. 最后扩展 StepFun 平台额度、云管理额度凭据、窗口上限和 reset/pace 路由。

新 UI 均从 Agent catalog 派生，区域用“中国 / 国际”，保留中英文、loading/error/empty 和 960×640 最小窗口验收。凭据和最终状态仍归 Rust Agent。

公开探测不使用用户凭据，也未发送生成请求。对照文件、固定 SHA 和探测 JSON 保存在 `/tmp/aipass-magpie-parity-20261018/`。本轮不把候选、协议源码审计或合成回归称为已通过真实账号验收。

## 本轮验证

- Rust 订阅模块：72 通过、1 忽略；Agent all-targets Clippy、fmt 和 repo check 通过。
- Node 全仓 lint、typecheck、test、build 通过，未改动的任务有 Turbo 缓存命中；桌面测试为 354 项。连接/额度组件另行实际复跑 14 项通过。
- 本轮只有额度空态文案调整，没有新增界面布局；没有新增真实浏览器截图。新入口和协议扩展实施时仍须按上文补 960×640 与真实账号验收。
- 没有执行新订阅的真实登录、续期、额度查询或付费生成；没有发布制品或运行远程 CI。

## 已批准修缮的落地状态（2026-10-18，后续实施）

本节记录后续实施结果，不把上文的候选与历史审计改写成真实账号验收。

- 新增五个套餐 ID：`kimi-code-cn` / `kimi-code`、`minimax-m-plan-cn` / `minimax-m-plan`、`bailian-coding-plan`。前两家使用独立中国/国际 ID，百炼在加密账号元数据中绑定区域。与 Kimi Work、MiniMax Code OAuth、Moonshot/MiniMax 按量 API、百炼 Token Plan 分开。
- Kimi Code 读取 `/coding/v1/models` 与 `/coding/v1/usages`；MiniMax M Plan 仅接受订阅 `sk-cp-` key，避免共享推理地址误用按量 key；使用原生 Anthropic 推理与 `/v1/token_plan/remains`。数值字符串、remaining、exhausted/unlimited、非语言池分区和缺失数据均有合成回归；没有把缺失读数补成 0%。百炼 Coding Plan 额度仍显示控制台，未把推理 key 当成云管理凭据。
- Qwen/百炼 Token Plan 加动态模型发现、类型过滤和最近成功目录回退。未知分页契约下，`has_more` 或超限目录不覆盖完整旧目录；尚未实现服务商未确认的 cursor 分页。
- OpenCode Go 增加 key 认证额度查询；按确切 metered 模型列表限制窗口，显式免费模型不受套餐窗口阻断。额度状态兼容 `ok` 和 `rate-limited`。协议元数据由固定产品的 models.dev 目录刷新，远程 URL 只作数据，不决定凭据目的地。公开列表仍缺乏协议信息的七个 ID 在当前 models.dev 中也不存在，继续跳过，未按名字或 Zen 协议猜测。
- Trae 工具转换拆为独立 parser：补完整 GLM arg_key/arg_value、name/parameter 标签及 native 工具名内信封；字符串按声明 schema 保留，重复字段、未知工具、截断和重复表示拒绝。启用工具时普通文本可提前流出，工具调用保留完整验证，已有输出后仍禁止重放。
- 增加 `ollama-cloud` 与 `kilo-gateway` key 入口、动态目录、自有 UA；Kilo 绑定可选组织与稳定 task/session，模型列表为公开目录，不表示该 key 的所有模型都已获组织授权。Ollama 使用云端模型原名，过滤 embedding/decision 等非 completion 模型。两者未实现额度 API；Kilo 以网关余额计费提示，不宣传为固定月费订阅。
- 新增账号额度使用阈值（1–100% 的后端校验，UI 常用值），只缩放路由判定，显示仍保留服务商实际用量。组可选择 `reset_soonest` 与 `quota_pace`：前者优先有效重置，后者按当前模型周额度的剩余份额/小时排序。旧配置策略和默认 100% 不变，过期/未知窗口、免费模型与 session affinity 有回归。
- 额度快照增可选 `consoleUrl` / `quotaUnavailable`；Rust 根据账号 provider/region 产生固定目的地，Tauri 再做 host/path allowlist 校验。未集成查询的空态提供直接控制台操作，避免无效刷新。设置页页脚改为本机安全处理，不再对 key 账号声称由官方 CLI 保存凭据。
- UI 已用真实浏览器在 960×640 检查七个入口，中英文/明暗共 14 个布局，另检查区域切换、设置阈值和控制台空态。使用合成登录/额度数据，不是厂商账号联调。

仍待单独实施/验收：Open Design 双 key 设备授权及 wallet 支出约束、小浣熊只读本机账号、AutoClaw 验证码及旋转 grant 恢复、StepFun 平台 session、火山/腾讯云管理凭据与用量 API、逐窗口单独 cap。它们没有本轮新建的可连接占位按钮，也没有借用其他服务的 key 或额度。新入口登录、续期、真实工具往返和耗尽切换仍需对应账号验收。

### 实施验证

- macOS / Homebrew Rust 1.99.1：完整 workspace 串行测试 1,038 通过、4 忽略。并行检查曾出现本机 `127.0.0.1` 连接 `EADDRNOTAVAIL`，串行完整重跑通过；保留失败与重跑日志，未改动系统网络配置。
- 末轮增加的 MiniMax 按量 key 拒绝与额度边界继续通过订阅模块定向回归（80 通过、1 忽略）；workspace all-targets Clippy、build、fmt、repo check 通过。
- Node 全仓 lint、typecheck、test、build 通过；桌面 364、扩展 183、共享 UI 8、控制面板 23 项通过。新增区域/组织显示也在 schemas 测试覆盖。
- 七个新增 ID 在 960×640 下的中英文与明暗布局共 14 项通过，另检查了 Kilo 失败保留组织、清空 key，百炼国际区域、额度阈值、控制台空态。账号列表保留区域/组织显示，避免同一 key 的不同计费范围只显示相同指纹。
- 本轮记录和截图：`/tmp/aipass-subscription-fixes-20261018/`。临时 UI harness 已移除，浏览器视口已恢复。没有真实订阅账号生成/续期、安装制品、提交推送、发布或远程 CI 验证。
