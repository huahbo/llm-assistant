# Outbox 持久化与消费语义

本文描述 `llm-wiki` 当前最小 outbox 实现与后续演进方向。

## 当前实现

- 事件类型：`wiki_core::WikiEvent`
- 运行时缓冲：`wiki_kernel::LlmWikiEngine::outbox`（`Vec<WikiEvent>`）
- 持久化接口：`wiki_storage::WikiRepository::append_outbox()`
- SQLite 存储表：`wiki_storage` 中的 `wiki_outbox(id, event_json)`
- 导出接口：`wiki_storage::WikiRepository::export_outbox_ndjson()`
- CLI 命令：`wiki-cli export-outbox-ndjson`

## 事件写入时机

`LlmWikiEngine` 的 `emit()` 会把事件放入内存 outbox；随后调用方应显式执行：

1. `save_to_repo()` 保存快照
2. `flush_outbox_to_repo()` 逐条写入 `wiki_outbox`

此顺序保证「状态先落地，事件后追记」，并降低消费端读到空状态的概率。

## 消费模式

目前支持两种消费方式：

- **拉取 ndjson**：定期执行 `wiki-cli export-outbox-ndjson`，将结果传给下游
- **直接读表**：外部进程读取 `wiki_outbox`（推荐带 offset/last_id）

## 幂等建议

消费端应以 `(event_type, payload key fields)` 做幂等，或引入 `wiki_outbox.id` 游标；
不建议假设事件严格只投递一次。

## 后续增强（兼容当前结构）

- 为 `wiki_outbox` 增加 `created_at`、`processed_at`、`consumer_tag`
- 增加 `claim check` 风格 payload（大对象走 blob）
- 在 `flush_outbox_to_repo` 增加分批与重试策略