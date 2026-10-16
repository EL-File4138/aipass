# 订阅改动总体正确性 review（2026-10-16）

范围是当前未提交工作区的订阅改动，包括之前的 Claude 组织身份修复、17 个
provider 的补齐，以及桌面、CLI、MCP、Agent IPC、代理路由和原生 CLI 切换。
本报告区分源码／fixture 回归与真实服务商联调，不将前者视为完整的线上正确性证明。

## 本轮发现并修复

| 优先级 | 触发条件与影响 | 修正与回归 |
| --- | --- | --- |
| P1 | Claude 定时查询已得到耗尽的周窗口，后续只收到五小时窗口的响应事件。原代码覆盖整个快照，周限制从路由中消失；持续响应也会刷新定时查询的调度时间。 | 在发布响应视图前保留已知定时窗口及其原始时间；独立保留查询时间。`passive_updates_preserve_scheduled_scopes_ages_and_poll_time_without_reading_grants` 验证两个周窗口仍耗尽、五小时窗口更新、旧窗口没有变新、重复反馈没有推迟查询。 |
| P1 | Claude 被动额度校验调用 `NativeStore::claude().identity()`，构造及读取都会访问原生凭据，macOS 上可能触发 Keychain 读取。 | 被动路径只读取非敏感 profile，核对邮箱和组织；账号 marker、device 校验保留。上述测试不创建任何 grant，仍能接受正确 profile，换组织后拒绝。主动登录、续期和原生切换仍使用现有 CLI／安全存储流程。 |
| P1 | Codex 只返回计划或模型专项额度时，主额度未知被当作未耗尽，“禁止付费点数”仍可提交生成。 | 未明确允许付费点数时，必须得到主额度窗口；未知额度拒绝提交。`unknown_or_model_only_allowance_cannot_authorize_paid_disabled_generation` 覆盖空、专项、可用、耗尽数据。此策略仍不能消除服务商并发消费与实际扣费之间的竞态。 |
| P1 | 自动切号观察当前账号后，外部 CLI 改了账号，或首选账号被归档／移出分组。之前最终检查只覆盖策略文本及目标成员。 | 续期前、最终写入前均核对当前 CLI 身份、首选／当前／目标成员及主凭据绑定；复用事务 fingerprint。`automatic_switch_revalidates_manual_choice_group_membership_and_archived_preference` 覆盖外部换号、移除成员、归档。 |
| P2 | 最后一张 reset credit 已被一次结果未知的请求消费，余额变成零。自动恢复此前只在余额大于零时重试，无法解析原操作。 | 未知操作优先复用原幂等键和 credit ID，零余额也可恢复；已完成操作不会再次创建消费。`an_unknown_reset_retries_its_key_after_the_last_credit_disappears` 覆盖此边界。原有 durable-intent ACK、重复结果及禁止新操作的测试保留。 |
| P2 | CLI Copilot 把工具结果续接也标为 `x-initiator: user`，与另一条传输的服务商计数语义不一致；图像标记缺失。 | 两个传输共享结构化用户／工具／图像识别，避免从普通文本猜测图像；Messages 请求附带协议版本，模型元数据保留实际 tenant API。`user_turns_tool_continuations_and_images_have_the_same_accounting_contract` 及已有两条传输契约测试覆盖。 |
| P2 | WorkBuddy 数字企业 ID 会丢失；额度只有总量、缺少已用量时被展示为零消耗。 | 数字和字符串 ID 统一规范化；缺失、异常的企业或 package 用量保持未知。新增缺失用量、明确零消耗、数字企业范围回归。Devin 的越界百分比也不再通过 clamp 变成可用／耗尽窗口。 |
| P2 | Codex SSE 观察器没有识别数据行使用 LF、空行使用 CRLF 的混合分隔，导致反馈事件被吞。 | 按空行识别混合分隔；已有分块、CRLF、超大事件恢复测试新增两种混合格式。 |

## 交叉检查

- 账号与历史：组织／project／enterprise 范围保持独立；旧 marker、revision、
  disabled target、旧主凭据绑定和混合账号历史均不能续接到新账号。
- 持久化与重放：凭据旋转、reset 消费要求 durable ACK；不确定写入回读精确 revision。
  被动反馈失败保留原始生成，不触发重放；Codex 撤销恢复只用于后续请求。
- Claude 生命周期：检查点只保存完成轮次，绑定 generation、target、上下文、历史和
  workspace；有限大小／数量／时间。恢复使用 fork，不覆盖已改变的外部 transcript。
  未完成工具、附件及大 transcript 的限制保留。
- 策略与入口：付费点数、自动 reset、自动切号默认关闭；桌面、CLI、MCP 经 typed Agent
  校验和 Rust 持久化。前端不接收 OAuth grant。内部反馈／归属读取不续用户空闲时间。
- Provider：复查 Copilot host 白名单、Gemini 官方 setup 的 project 返回契约、WorkBuddy
  tenant headers、Devin ProtoJSON 零值条件、Factory 池归属、MiMo 身份和公共账期转换。

## 验证记录

最终代码的 `cargo test --workspace` 默认并发回归通过：**935 passed、4 ignored、
31 suites**。此前主要修正后的两轮也分别通过 934 项，最后补丁新增一项 reset 恢复测试。
`cargo clippy --workspace --all-targets -- -D warnings`、`cargo build --workspace`、
`cargo fmt --all -- --check`、`pnpm repo:check`、`git diff --check` 均通过。

`pnpm lint`、`pnpm typecheck`、`pnpm test`、`pnpm build` 通过；本轮未修改前端源码，Turbo 复用了
相同源码的通过结果，其中桌面 331 项测试。前一轮的 960×640 中英文、明暗浏览器
fixture 证据仍适用；没有新增安装后 Tauri 运行验证。
前端构建仍报告大 chunk；测试缓存日志包含已有的 Svelte `derived_inert` 诊断。

本地 HTTP runtime fixture 已显式使用直连和 `no_proxy()`，移除宿主机系统代理的影响。
这不是历史所有间歇失败已经修复的证明。前轮 HTTP 502、WebSocket、macOS autostart
失败日志仍保留；见[原 review 的记录](subscription-review-2026-10-16.md)。

未执行真实授权、远端 token 旋转、付费生成、reset 消费或真实 CLI 自动切号；未发布、
安装或运行远程 CI。剩余 provider 能力差距见
[17-provider 对比](subscription-provider-parity-2026-10-16.md)。
