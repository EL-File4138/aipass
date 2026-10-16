# 本地代理正确性复查（2026-10-16）

范围是当前未提交工作区的本地代理及订阅传输：Agent 凭据和运行时配置、HTTP 请求提交、
SSE 预取与聚合、协议转换、WebSocket 适配、失败切换、额度选路、取消和配置撤销。
本轮继续使用仓库内 Rust 实现，没有引入第三方 provider 引擎。

## 发现与修复

| 优先级 | 问题与影响 | 修复及回归 |
| --- | --- | --- |
| P1 | 普通 HTTP 已取消生成响应超时，订阅 Adapter 仍设置 90 秒读取超时，整个操作仍受 20 分钟上限约束，非流式订阅聚合另有 120 秒空闲超时。慢推理可能被截断。 | 抽出 Agent `subscriptions/lifecycle.rs`，只限制连接和准备阶段；在 provider 生成分派前退休准备计时器，取消订阅响应读取及聚合空闲超时，并明确禁止 reqwest 自动重试。虚拟时钟和真实本地 TCP 验证慢响应头、慢正文、超过 20 分钟的生成、取消及准备阶段超时。 |
| P1 | 准备阶段计时器到期可能打断已经旋转的一次性凭据，早于 durable ACK 丢弃操作。 | 与取消逻辑一样，计时器也等待旋转保护解除；同一轮 future 轮询进入生成／旋转时重新检查阶段。虚拟时钟测试覆盖过期准备计时器与旋转保护及随后生成分派的交互；现有真实 HTTP 旋转／ACK 回归继续执行。 |
| P2 | 挂起请求保留旧路由的额度快照。额度恢复的配置更新不会撤销活动生成，但等待中的请求也因此一直看不到恢复。 | 每轮重新选路前从可信运行时读取相同 target、provider、secret 和 credential 的额度、并发、优先级及权重；保留请求原有凭据／协议快照及历史来源约束。实际本地 HTTP 测试验证等待中的请求能使用恢复额度；另测新耗尽、并发上限和账号替换隔离。 |
| P2 | 通用 SSE 分帧器遗漏 LF 数据行与 CRLF 空行组合。已完成事件或明确错误可能被当作未完成传输。 | 共用分帧器识别两种换行及混合格式。回归覆盖跨 chunk 的明确错误和非流式订阅完成事件。 |
| P1 | 模型组的来源检测包含 reasoning 加密内容，却遗漏 compaction 加密历史，存在错误跨来源续接的风险。 | 将 compaction 加密内容纳入同一归属规则；无来源、来源冲突时拒绝，已记录来源即使不可用也保留绑定。两类历史使用相同回归。 |

## 检查的行为边界

- 普通 HTTP、SSE、原生 WS 和适配 WS：已经提交的生成不因响应慢、hold 预算或
  不确定的断连而重放；明确拒绝或完整收到的无效响应才允许正常失败切换。
- 配置：额度与调度元数据更新不打断活动生成；凭据、账号绑定、协议或路由授权变化
  仍走现有配置撤销路径。新选路不会把新账号凭据移植进旧请求。
- 取消：下游关闭或撤销结束活动传输、释放并发占用；取消本身不计为 provider 故障。
- 续期：登录、刷新、账户元数据请求保留各自有界等待；一次性凭据旋转保持 durable ACK
  保护。原生 CLI 仍拥有其账号存储，Rust Agent 保持凭据权威。
- 历史：provider 绑定的 reasoning、compaction、签名 thinking 和续接标识保留来源约束；
  工具定义或用户文本中的同名字段不会被误判为绑定历史。

## 验证

所有检查原生运行于 macOS：

- `cargo test --workspace`：**942 passed、0 failed、4 ignored，31 suites**；其中 Agent
  346 项通过，代理 232 项通过，协议转换 91 项通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo build --workspace` 通过（本地 dev profile，不是发布产物验证）。
- `cargo fmt --all -- --check`、`pnpm repo:check`、`git diff --check` 通过；repo gate
  检查 523 个源码文件，26 个已有大文件仍受原有限额约束。
- 新增真实 TCP 测试最初在连接建立前暂停时钟，Tokio 自动推进导致连接超时；已改为
  服务端确认收到请求后才暂停。额度恢复 fixture 最初同时切换系统代理模式，正确触发了
  配置撤销 503；已改为启动请求前设置直连，确保该回归只变更额度。这两次失败没有被
  当作生产修复证据；最终全量结果以上述完整通过运行记录为准。

日志：`/tmp/aipass-local-proxy-review-workspace-20261016.log`、
`/tmp/aipass-local-proxy-review-clippy-20261016.log`、
`/tmp/aipass-local-proxy-review-build-20261016.log`。

本轮不执行真实 OAuth 授权、付费生成、远端 token 旋转、reset 消费或真实 CLI 自动切号；
不包含已安装 Tauri／发布产物的运行验证，也没有运行远程 CI。源码与本地 fixture
不能证明所有 provider 的线上契约都有效。其他订阅差距见
[provider 对比](subscription-provider-parity-2026-10-16.md) 和
[上一轮总体 review](subscription-correctness-review-2026-10-16.md)。
