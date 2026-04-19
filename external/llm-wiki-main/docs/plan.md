# llm-wiki 实施里程碑

本文是 `llm-wiki` 对齐并超越 Karpathy LLM Wiki 方案的执行版路线图。

## M1: Wiki 投影层

- CLI 支持 `--wiki-dir` 与 `--sync-wiki`，将结构化状态投影到 markdown。
- 自动生成 `index.md`、`log.md`，并维护 `pages/`、`concepts/`、`sources/`。
- Query 支持 `--write-page`，将回答结果沉淀为 wiki 页面。

验收标准：
- 连续 ingest 后，`index.md` 和 `log.md` 都产生增量。
- query 结果可选择落盘并出现在 `index.md` 中。

## M2: 一致性与消费语义

- lint 增强：`page.orphan`、`claim.stale`、`xref.missing`。
- 产出 lint 报告文件到 `wiki/reports/`。
- outbox 增加游标导出与处理确认：
  - `export-outbox-ndjson-from --last-id`
  - `ack-outbox --up-to-id --consumer-tag`
- outbox flush 改为分批 + 重试策略。

验收标准：
- lint 结果可直接用于 wiki 修复。
- consumer 可以 offset 重放并标记消费进度。

## M3: mempalace 联动与标准化流程

- 提供 `consume-to-mempalace --last-id` 最小消费器。
- bridge 提供 `consume_outbox_ndjson` 事件分发能力。
- 增加 `AGENTS.md` 规范新会话可重复执行。

验收标准：
- 演示 ingest -> outbox -> consume -> query -> file back 全流程。
- 关键操作不依赖隐式记忆，按规范文档可复现。
