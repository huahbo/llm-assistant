# AGENTS Workflow

本文件定义 `llm-wiki` 的最小稳定操作流程，目标是让不同会话中的 agent 可重复执行。

## 1. Ingest

```bash
cargo run -p wiki-cli -- --db wiki.db --wiki-dir wiki --sync-wiki ingest \
  "file:///notes/a.md" "source body text" --scope private:cli
```

要求：
- ingest 后必须 `save_snapshot` + `flush_outbox`。
- 若开启 `--sync-wiki`，必须同步 markdown 投影层。

## 2. Query 与结果沉淀

```bash
cargo run -p wiki-cli -- --db wiki.db --wiki-dir wiki --sync-wiki query \
  "what changed?" --write-page --page-title "analysis-change-log"
```

要求：
- 默认返回 top ranked docs。
- 若 `--write-page`，将 query 结果写入 wiki 页面，并更新 `index.md`。

## 3. Lint 与健康检查

```bash
cargo run -p wiki-cli -- --db wiki.db --wiki-dir wiki --sync-wiki lint
```

要求：
- 关注 `page.orphan`、`claim.stale`、`xref.missing`。
- lint 报告写入 `wiki/reports/`。

## 4. Outbox 消费

导出增量事件：
```bash
cargo run -p wiki-cli -- --db wiki.db export-outbox-ndjson-from --last-id 100
```

消费确认：
```bash
cargo run -p wiki-cli -- --db wiki.db ack-outbox --up-to-id 120 --consumer-tag mempalace
```

桥接消费（最小实现）：
```bash
cargo run -p wiki-cli -- --db wiki.db consume-to-mempalace --last-id 100
```

## 5. Supersede 策略

- 新结论优先通过 `supersede` 替换旧 claim。
- 旧 claim 标记 `stale` 后，不删除，保留审计与可回溯性。
- lint 中若出现 stale 相关提示，应在页面层补充新旧结论关系。
