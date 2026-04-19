# llm-wiki

`llm-wiki` 是一个“持久、可累积”的 LLM 知识库内核与最小 CLI：不是每次 query 现做 RAG，而是把知识沉淀为可维护的结构化状态，并可投影成可浏览的 Markdown wiki（适配 Obsidian 等）。

## 核心理念（对齐 Karpathy LLM Wiki）

- **Raw sources**：不可变的原始资料（`RawArtifact`）。
- **Wiki**：可持续维护的 wiki 页面（`WikiPage`，markdown + `[[wikilink]]`）。
- **Schema**：约束与策略（`DomainSchema`：允许的实体/关系、质量阈值、保留/晋升参数）。
- **Operations**：`ingest / query / lint / crystallize`，并通过 **outbox** 事件让外部 consumer（如 mempalace）接入。

相对 idea-only 的方案，这个仓库更偏“工程内核”：事件、审计、生命周期、RRF 融合检索、保留强度加权、outbox 语义等都已最小落地。

## 快速开始

### 1) 运行一次 ingest（并同步 markdown wiki 投影）

```bash
cargo run -p wiki-cli -- \
  --db wiki.db \
  --wiki-dir wiki \
  --sync-wiki \
  ingest "file:///notes/a.md" "项目使用 Redis\nAuthorization: Bearer secret" \
  --scope private:cli
```

会生成/更新：

- `wiki/index.md`
- `wiki/log.md`
- `wiki/pages/`、`wiki/concepts/`、`wiki/sources/`

### 2) query（可选落盘为 wiki 页面）

```bash
cargo run -p wiki-cli -- \
  --db wiki.db \
  --wiki-dir wiki \
  --sync-wiki \
  query "Redis API" --write-page --page-title "analysis-redis-api"
```

### 3) lint（并输出报告）

```bash
cargo run -p wiki-cli -- \
  --db wiki.db \
  --wiki-dir wiki \
  --sync-wiki \
  lint
```

会写入 `wiki/reports/lint-*.md`，并在 stdout 打印报告路径。

### 4) outbox 增量导出与消费确认

增量导出（offset 模式）：

```bash
cargo run -p wiki-cli -- --db wiki.db export-outbox-ndjson-from --last-id 100
```

消费确认（标记 processed）：

```bash
cargo run -p wiki-cli -- --db wiki.db ack-outbox --up-to-id 120 --consumer-tag mempalace
```

### 5) mempalace（最小桥接消费演示）

```bash
cargo run -p wiki-cli -- --db wiki.db consume-to-mempalace --last-id 100
```

当前实现为“打印型 sink”，用于验证 outbox 消费链路与事件映射；后续可替换为真实 mempalace 写入实现。

## 模型配置（由你填写）

当前代码库的核心能力不依赖模型调用；如果你要接入 DeepSeek（或其它 OpenAI-compatible API），可先填写模板文件：

- `llm-config.example.toml`：复制为 `llm-config.toml` 后填写 `base_url / api_key / model`

注意：**不要提交真实 `api_key` 到 git**。

## 测试

```bash
cargo test
```

测试覆盖：

- wiki 投影输出（`index.md/log.md` 与目录结构）
- outbox 游标导出与 ack
- mempalace bridge 的 NDJSON 消费分发

### 端到端回归（推荐）

```bash
./scripts/e2e.sh
```

该脚本会自动执行并断言：

- ingest / file-claim / supersede-claim / query / lint 全链路
- outbox 增量导出与 ack
- mempalace 消费结果 `consumed > 0`
- 若存在 `llm-config.toml`，自动执行 `llm-smoke`（DeepSeek 冒烟）

## 文档

- `AGENTS.md`：面向 agent 的稳定工作流规范
- `docs/plan.md`：里程碑与验收标准

