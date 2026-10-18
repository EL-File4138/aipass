# 订阅目录扩展（2026-10-18）

在现有 Command Code 等订阅基础上新增 18 个 provider ID。账号目录目前包含
34 个通用适配入口和独立的 Claude 入口。认证、凭据、额度和协议转换均由本仓库
Rust crates 实现，UI 通过现有 typed IPC 连接 Agent。

## 新增入口

| 订阅 | 登录与推理 | 额度与限制 |
| --- | --- | --- |
| OpenCode Go / Zen | 专用 API key、本机 OpenCode 导入。按模型元数据选择 Chat、Responses、Messages 或 Gemini 协议。 | 控制台查看额度；不把本地用量或未知额度当套餐余额。首次连接需取得服务端模型目录；不提供仅限 OpenCode 客户端的匿名免费入口。 |
| Antigravity | Google 浏览器授权、PKCE、状态校验、续期；Code Assist project 与 Gemini 信封。 | 服务端模型额度。账号须先在官方客户端完成开通；不自动 onboarding。 |
| Kimi Work | 只读引用本机 Kimi Desktop Work 登录，按请求重读凭据。 | 会员接口；不保存或消费官方客户端 refresh token。设备或账号变更须重连。 |
| MiniMax Code 中国 / 国际 | 两个站点独立 OAuth、PKCE、旋转凭据持久化及原生 Messages。 | 会员余额、MPlan 窗口；不自动签到。 |
| Cline | WorkOS 设备授权或 API key，服务端模型目录。 | 余额和 Cline Pass 专项模型窗口；未知额度不伪装为零。 |
| Trae 中国 / 国际 | 浏览器回调、随机回调路径、账号及租户范围、续期；动态模型和自有 Rust SSE/工具转换。 | 中国积分与国际会员窗口。消费版固定服务域；未实现企业动态域、设备证明和自动签到。工具文本信封需收齐后校验，不能视为完整原生 IDE 会话等价实现。 |
| MiMo Token Plan | `tp-` 套餐 key；中国、新加坡、欧洲独立端点；可读取 live models。 | 控制台查看；与原有 Xiaomi MiMo 应用订阅分开。 |
| StepFun Step Plan 中国 / 国际 | 分站套餐 key、专用 Step Plan 命名空间、可读取 live models。 | 控制台查看；中国和国际 key 不互换。 |
| Qwen / 百炼 Token Plan | 对应平台的 Token Plan key、独立端点；百炼支持中国 / 国际。不强制 Coding Plan 的 `sk-sp-` 前缀。 | 控制台查看；当前使用文档模型目录。Token Plan 与固定月费 Coding Plan 是不同产品，计费以平台为准。 |
| 火山 Coding Plan / Agent Plan | 专用套餐 key，分别固定 `/coding` 与 `/plan` 端点。 | 控制台查看；不会回退到按量付费端点。 |
| 腾讯云 Token Plan / 企业版 | 独立个人、企业端点；企业版支持中国 / 国际。 | 控制台查看；未添加云管理 AK/SK，也不拿推理 key 调管理接口。 |

静态目录不代表账号拥有所有模型权限。没有非计费验证接口的套餐，连接只完成
key 格式、范围和目录保存，真实有效性由首次服务端请求判定。未知上下文长度保留未知，
未提供额度接口的套餐不参与基于虚构余额的判断。

## 桌面 UI

连接入口从 Agent 加载完整目录，支持搜索；不再维护第二份提供商白名单。
补齐品牌图标、方法/区域翻译、获取 key 按钮、设备授权码、本机导入和额度空状态。
中文区域统一使用“中国 / 国际”。生成的账号标题将品牌与账号名分开显示，保留自定义标题。
所有新增入口均在 960×640 检查；临时浏览器验证入口已移除。

## 参考与数据来源

- 比较基线：[Magpie b14784c](https://github.com/yetone/magpie/tree/b14784c03fd9e410f34e6aa8ff1ae357dfe6078b)、[community plugins 0a998a8](https://github.com/magpie-community/plugins/tree/0a998a8a7fea95161a3d13d03f3b4594cf6e2fad)。源码只在仓库外只读核对，未引入其引擎、源码依赖或运行时插件。
- [OpenCode Go](https://opencode.ai/v2/docs/console/go/)、[官方源码](https://github.com/anomalyco/opencode)。Go 请求保留自有 User-Agent 和稳定会话标识。
- OpenCode 模型元数据来自 [models.dev 54c4217](https://github.com/anomalyco/models.dev/tree/54c4217053eb90e66e19c1e811133e158ba3325c)，保留 MIT 许可及 NOTICE。
- [MiniMax Code 官方实现](https://github.com/MiniMax-AI/minimax-code)、[Cline 官方实现](https://github.com/cline/cline)。
- [Qwen Token Plan](https://platform.qianwenai.com/docs/token-plan/overview)、[百炼 Qwen Code 配置](https://help.aliyun.com/en/model-studio/qwen-code)、[腾讯云套餐](https://cloud.tencent.cn/document/product/1823/130060)、[火山 Coding Plan](https://docs.volcengine.com/docs/ark/coding-plan-personal-faq?lang=en)。
- Trae 中国模型域名已与[官方技术支持给出的地址](https://forum.trae.cn/t/topic/15428)核对；不接受登录返回的任意主机来接收凭据。
- 新品牌图标使用 Lobe Icons MIT 素材，来源及固定版本见 `packages/ui/src/assets/provider-icons/SOURCES.md`。

## 验证边界

本次执行 macOS 本地 Rust 和 Node 全仓检查、Tauri 外链白名单测试，以及真实浏览器
960×640 的 36 组布局检查。登录等待、失败与未知额度通过合成数据检查；合成数据不调用供应商。
截图和布局记录保存在 `/tmp/aipass-subscription-ui-20261018/`。

- Rust 全仓串行测试：1009 通过、4 忽略；fmt、Clippy 与 workspace build 通过。
- Node 全仓 lint、typecheck、test、build、许可证审计通过；桌面 353 项测试通过。
- Tauri 授权外链检查通过。首次并行 Rust 测试中的既有代理 WS 重试测试波动，单独与全仓串行复跑均通过。

没有使用真实订阅凭据执行新提供商登录、续期、额度查询或付费生成；这些路径仍需真实账号
验收，尤其 Trae 与 Antigravity。未安装或发布桌面包，未运行发布制品 gate，也未触发远程 CI。

## 实现复审

本轮复审修正以下问题，并增加对应回归：

- Trae 返回工具调用须满足调用方的 `none`、`required`、指定工具和并行限制；Markdown 代码示例不会变成工具调用，JSON 参数内的结束标签不会截断参数，历史保留调用 ID。
- Trae 兼容嵌套用量、驼峰事件名和完成原因，保留 reasoning/cache 计数；不能处理的内容块在提交请求前报错。
- 套餐 Anthropic 请求补齐 `/v1/messages`。端点仍固定在套餐命名空间，不回退到按量计费。
- Trae 数字租户 ID 参与账号身份，期限采用服务端/JWT 中较早的有效值；过期套餐不进入额度，缺少 bonus 用量不猜作零。数字字符串时间戳也能正确显示。
- 续期型登录拒绝缺失 refresh grant，Cline 拒绝已经过期的令牌，额度百分比的非法负值保持未知。
- 百炼国际区域的获取 key 按钮打开国际 Token Plan 控制台，Tauri 外链白名单同步更新。连接失败后保留选中的区域，清除 key，避免重试默默切回中国；新错误出现时滚到提示位置，避免焦点滚动把错误留在视区外。
- OpenCode 公共模型目录的真实 GET 返回 200；无账号免费生成探测返回 HTTP 403 `FreeTierError`，明确仅限官方客户端。因此移除匿名免费连接方式，并拒绝将 `public` 当成有效订阅 key。未冒用官方客户端标识绕过限制。

上述公开探测没有使用用户凭据，也没有发生付费生成。其他供应商仍维持前述真实账号验收边界。

复审全仓 Rust 串行回归为 1016 通过、4 忽略，Clippy、workspace build 与 Tauri 外链测试通过。
最终 Node 全仓 lint、typecheck、test、build 和许可证审计通过，桌面 354 项测试通过。
最小窗口实测复现了区域重置和错误提示被滚动遮住的问题；区域保持已实测，最终错误定位修复通过组件回归，浏览器控制连接随后无响应，未取得该修复的最终视觉截图。已移除临时验证入口。

## 第二轮复审

- Trae 无工具或禁用工具时，普通文本按事件及时发送，避免等到回复结束才输出。启用文本工具信封时仍收齐校验，防止执行不完整调用。
- Trae 对无法保留的结构化输出、停止序列和多候选请求，在提交前明确报错，避免静默丢弃调用方约束。
- Cline 余额查询失败时，成功取得的 ClinePass 窗口仍保持有效；缺失余额保持未知。额度范围采用目录的订阅模型标记和精确模型 ID，空范围不会扩大成全账号限制。

对应新增 3 项 Rust 回归；连接与额度 UI 的 18 项组件回归通过。第二轮仍未使用真实订阅凭据，浏览器控制重试超时，最终错误定位的视觉验证边界保持不变。

最终版本的全仓 Rust 回归：1019 通过、0 失败、4 忽略，共 31 组测试结果；fmt、Clippy、workspace build 和仓库结构检查通过。Node 全仓 lint、typecheck、test 通过（未改动的任务复用 Turbo 缓存），上述 18 项 UI 组件回归另行实际复跑。

首趟回归中，既有 Claude 组织切换用例曾返回 `storage_unavailable` 而非预期的 `conflict`；单独复跑和最终版本全仓复跑均通过，未复现该存储错误，未据此改动 Claude 认证实现。保留首趟失败日志 `/tmp/aipass-subscription-review2-workspace.log` 和最终成功日志 `/tmp/aipass-subscription-review2-final-workspace.log`。
