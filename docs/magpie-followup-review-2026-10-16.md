# Magpie 对照复查（2026-10-16）

本轮重新核对远端，比较基线更新为：

- [Magpie `63be477`](https://github.com/yetone/magpie/tree/63be47717a9ca91fc89102078a1f1fff3e03e0ce)，
  比上一轮 `a6f908b` 多 31 个提交。
- [Community plugins `9fd59cf`](https://github.com/magpie-community/plugins/tree/9fd59cf6e83c0f456239b5f3fd605896e77a6859)，
  比上一轮 `6a3fe9b` 多 10 个提交。

比较源码保存在仓库外 `/tmp`，没有导入或执行其引擎／插件。AIPass 行为继续由本仓库
Rust Agent、proxy 和 conversion crates 持有。以下区分已修复缺陷与未实现能力。

## 本轮已修复

| 优先级 | 触发与影响 | 修正和证据 |
| --- | --- | --- |
| P1 | Anthropic → Responses 已输出完成事件，但没有设置终止状态，EOF 的 `finish()` 仍误报未完成；重复 `message_stop` 也可能重复发出结果。 | 在完成／不完整结果后统一终止。新增回归验证 EOF 成功、终止仅一次，实际 HTTP 返回和 SQLite 记录均成功。 |
| P2 | Anthropic → Chat 未把缓存读取／创建计入 prompt 总量；Anthropic → Responses 忽略后续输入／缓存更新；Chat → Messages／Responses 用后续缺省字段覆盖已知计数。 | 公共 `UsageAccumulator` 合并原始累计字段后再按协议分离普通／缓存输入，保留缺省或 null，识别显式零；流式与非流式共用用量映射。回归覆盖双向转换、晚到缓存详情、稀疏输出更新和 `max_tokens` 后的 usage-only 事件。 |
| P2 | 代理把每个 OpenAI 用量事件分别换算成非缓存输入，再取最大值。例：先报 prompt=100，后报 cached=80，旧逻辑可能记成 100 普通输入加 80 缓存读取。 | 落库也使用同一合并器。真实本地 HTTP fixture 验证普通 Chat、Anthropic → Chat、Anthropic → Responses：客户端与 SQLite 都得到 5 普通输入、80 缓存读取、15 缓存写入、11 输出，而不是重复计数。 |
| P1 | Anthropic → Chat 工具调用没有 `input_json_delta` 时，起始 `input` 未送出；空参数会成为无效的空字符串。 | 在 block 完成时发送起始 JSON；有真实增量时使用增量。回归覆盖 `{}` 和非空起始对象，检查两种 OpenAI 格式的完整参数。 |
| P2 | Factory 的 Responses 通道仍转发 temperature/top_p，旧模型缓存也标为可采样。 | 按该通道的 Droid 契约移除两个采样字段；准备模型目录时校正现有缓存能力。保留 reasoning 和用户指令，不改变 Anthropic 通道；测试覆盖幂等性与两个通道的差别。 |
| P1 | 加密历史识别只包含顶层 reasoning/compaction，遗漏 agent_message 及明确标为 `encrypted_content` 的消息／工具输出块。 | 入站摘要、出站归属记录、模型组路由及跨协议转换共用 typed wire 识别。密文保持原样，未知来源拒绝；不递归解析任意工具 JSON，不删除用户数据或 call_id。回归覆盖嵌套来源、去重、仅存摘要、原生保留和跨协议拒绝。 |

Magpie 的[部分用量矩阵](https://github.com/yetone/magpie/blob/63be47717a9ca91fc89102078a1f1fff3e03e0ce/internal/gateway/usage_cache_partial_test.go)
提示了稀疏更新问题；我们按
[Anthropic 累计用量规范](https://platform.claude.com/docs/en/build-with-claude/streaming)
和[缓存输入定义](https://platform.claude.com/docs/en/build-with-claude/prompt-caching)
实现并验证协议单位，没有复制它的启发式规则。

## 别人的特殊处理与剩余差距

此表记录本轮对照时的快照；后续本地实现与仍未覆盖的边界见[能力补齐记录](subscription-capabilities-2026-10-16.md)。

| 处理 | Magpie 当前做法 | AIPass 当前边界 |
| --- | --- | --- |
| Claude 客户端缓存断点 | [显式 opt-in](https://github.com/yetone/magpie/blob/63be47717a9ca91fc89102078a1f1fff3e03e0ce/internal/gateway/claude_cache.go) 后映射 system/message/tail cache marks，校验 5m/1h 顺序和数量，报告无法保留的断点；活动原生进程的历史断点不能随意改。 | 原生 CLI 自主管理缓存，没有等价的断点映射与丢弃报告。API 同协议转发仍保留调用方原始字段。不能声称 Claude CLI 桥保留了客户端全部缓存控制。 |
| 缓存 TTL 与账单 | 区分 5m/1h 写入，专门处理后续缺省、零填充及多轮汇总。 | 本轮修正总 token 与缓存读写计数；尚无独立 1h 写入价格字段。当前估算不能精确区分这两种 TTL 价格。 |
| Grok 工具兼容 | [内置兼容层](https://github.com/yetone/magpie/blob/63be47717a9ca91fc89102078a1f1fff3e03e0ce/internal/gateway/grok_integral.go) 处理工具参数 `2500.0`、命名空间还原和 regex 字符类；[迁移后插件](https://github.com/magpie-community/plugins/blob/9fd59cf6e83c0f456239b5f3fd605896e77a6859/packages/grok/index.mjs) 另有 root schema 与可携带 agent_message 转换，不能只看已弃用内置文件。 | 基础 Responses、原生 headers、模型／额度和 reasoning null 清理已有；完整工具兼容尚不等价。当前拒绝不支持的工具类型，未静默删掉工具让请求看似成功。 |
| Codex 历史过滤 | 新增[可选 middleware](https://github.com/magpie-community/plugins/blob/9fd59cf6e83c0f456239b5f3fd605896e77a6859/packages/codex-history/codex-history.middleware.js)，按配置去除部分历史或拒绝不可携带引用。 | 保留原始密文并绑定原账号／target；本轮补齐嵌套识别。没有照搬删除 reasoning 或历史 ID 的策略。 |
| Cursor native-session | [保留会话](https://github.com/magpie-community/plugins/blob/9fd59cf6e83c0f456239b5f3fd605896e77a6859/packages/cursor/native-session.mjs) 在临近绝对寿命时于提交前重建，工具批次等待随活动续期，区分整轮账单与最后一步上下文用量。 | 当前 Rust 双向传输没有同等的独立 native-session pool 与 deferred usage。此差距仍需专门实现、联调，不能仅靠通用超时修改视为完成。 |
| Codex Keychain 提示 | [凭据存储](https://github.com/yetone/magpie/blob/63be47717a9ca91fc89102078a1f1fff3e03e0ce/internal/provider/codex_store.go) 以非敏感修改时间判断是否重读，合并并发读取，对拒绝／未答复保留状态并提供重试。 | 现有 NativeStore 按官方 file/keyring/auto 存储读取、拒绝静默退回；尚无等价的提示合并和修改时间缓存。这里是源码确认的体验／阻塞风险，未在真实 Keychain 权限弹窗下验证。 |
| provider 特定 schema | Magpie 还对 Kimi enum 类型、DeepSeek regex、部分路由的 oneOf 进行适配。 | 当前主要支持模型参数、reasoning、协议和身份边界；这些规则没有全量等价覆盖。尤其 oneOf → anyOf 会放宽验证语义，不能作为所有 provider 的通用替换。 |

上一轮补齐的组织／project／enterprise 身份、额度反馈、默认关闭的付费／reset／自动
切号、Claude 完成轮次恢复和现代 Codex V2 compaction 仍保留。
特殊能力不同不等于应该无条件复制第三方行为；所有新增适配需要明确 provider 范围，
并保持原始工具／历史语义。

## 验证记录

本地 macOS 最终 `cargo test --workspace`：**949 passed、0 failed、4 ignored，31 suites**。
其中 Agent 348 项、proxy 233 项、conversion 95 项通过。
`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、
`pnpm repo:check` 和 `git diff --check` 通过。repo gate 检查 527 个源码文件；原有 stream
大文件已按职责拆分并移出例外清单，剩余 25 个已有大文件继续受原有限额约束。
`cargo build --workspace` 通过（dev profile，不代表安装产物验证）。

可复核日志：`/tmp/aipass-magpie-review-workspace-final-20261016.log`、
`/tmp/aipass-magpie-review-clippy-20261016.log`、`/tmp/aipass-magpie-review-build-20261016.log`。
嵌套历史修正前的完整一轮也通过
947 项；最终结果以上述 949 项运行记录为准。

本轮没有真实授权、付费生成、reset 消费、
远端 token 旋转或 Keychain 权限交互测试；没有安装／发布产物或触发远程 CI。
