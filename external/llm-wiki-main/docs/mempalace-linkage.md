# llm-wiki 与 rust-mempalace 联动映射

目标是**独立仓库维护**，通过契约联动，而非代码融合。

## 1) 概念映射


| llm-wiki                     | rust-mempalace          | 说明                                  |
| ---------------------------- | ----------------------- | ----------------------------------- |
| `RawArtifact`                | `drawers` 行             | 原始资料正文进入 drawer content             |
| `Claim`                      | `kg_facts`              | 可映射为 `(subject, predicate, object)` |
| `Claim.supersedes` / `stale` | `valid_to`              | 新结论写入后，旧事实 `kg_invalidate`          |
| `WikiEvent::SourceIngested`  | `mine_path`/入库流程触发      | 写侧事件驱动                              |
| `WikiEvent::QueryServed`     | benchmark/telemetry     | 可用于检索效果观测                           |
| `Entity`/`TypedEdge`         | `kg_query` + `traverse` | 图路召回来源                              |


## 2) 推荐联动模式

### 模式 A：进程内（低延迟）

- 在独立 crate（如 `wiki-mempalace-adapter`）实现 `wiki_kernel::WikiHook`
- `on_event()` 中调用 mempalace 库 API（`kg_add` / `kg_invalidate` / 入 drawer）
- 优点：实时，结构清晰
- 缺点：本地构建需要 `path` 依赖到 `rust-mempalace`

### 模式 B：进程外（低耦合）

- `llm-wiki` 仅负责 outbox 持久化
- 定时任务读取 `export-outbox-ndjson` 并调用 mempalace CLI
- 优点：仓库完全解耦，部署简单
- 缺点：最终一致，非实时

## 3) 字段映射建议（Claim → kg_facts）

- `subject`: 项目/实体主语（由上层规则或 LLM 提取）
- `predicate`: 关系类型（如 `uses`, `depends_on`, `fixed`）
- `object`: 结论对象（库名、版本、配置值）
- `valid_from`: claim 创建或强化时间
- `valid_to`: supersede 时回填
- `source_drawer_id`: 可选，从 `RawArtifact` 联动后回填

## 4) 最小落地流程

1. `llm-wiki` 写入 sources/claims/pages 并 flush outbox
2. 消费器按 outbox 顺序处理 `ClaimUpserted`/`ClaimSuperseded`
3. 在 mempalace 中写 `kg_facts`，必要时执行 `kg_invalidate`
4. 读侧查询继续由各自系统负责，结果可在上层做 RRF 融合