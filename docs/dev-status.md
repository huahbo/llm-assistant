# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。
> 完整历史存档见 `docs/completed-log.md`（仅按需查阅）。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 读 `docs/交接状态卡.md` 与 `docs/多Agent通信与交接协议.md`，确认当前接力状态
3. 读 `docs/实施过程记录.md` 最新 3 条了解背景
4. 读 `docs/参考项目差距与移植清单.md`，确认本轮选择的移植包与理由
5. 查看下方 §活跃 TODO，按优先级 1→2 开始，**必须使用 §14 子代理并行规则**

---

## 验证基线（2026-04-25 最新）

```powershell
# Windows PowerShell
cd src-tauri; cargo test          # 应: 183 passed, 0 failed
cd ../web; npm run test -- --run  # 应: 174 passed, 0 failed
cd ../web; npm run typecheck      # 应: 0 errors
cd ../web; npm run build          # 应: 通过
```

- `scripts/verify_clipper_windows.ps1`：用户回传通过（2026-04-22）
- `scripts/verify_searxng_windows.ps1`：用户回传通过（2026-04-22）
- Deep Research 端到端实跑：通过（城市复杂水网AI仿真，2026-04-23）

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `327b09d` | fix: 图谱节点命名（ingest-时间戳 → 语义标题） |
| `826f9e2` | feat(方向G): 页面变更历史 + 风险收口（恢复版本/checksum） |
| `f3e559f` | feat(F): AI 辅助新建 Wiki 页面 |
| `7da98b8` | feat(方向E): Vault 统计仪表盘 |
| `d67f656` | refactor(B/C): FTS5/CTE/路径防御/512KB 质量收口 |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 1 | **方向 B：项目模板/多项目体验** | 进行中 | 已实现模板初始化预览卡 + 最近 Vault 快速切换；**待用户 Windows 端到端复核后收口** |
| 2 | **方向 C：会话持久化增强** | 进行中 | 已完成一期 + 二期；**待用户 Windows 端到端复核后收口** |
| — | 用户新需求 | 待提 | 用户提到有新需求，尚未描述 |

---

## 代码快照（2026-04-25，基线 183 Rust / 174 前端）

```
src-tauri/src/
  llm/
    provider.rs     # LlmProvider trait（含 embed/complete_stream/health_check）
    ollama.rs       # OllamaProvider（Ollama /api/generate + /api/embeddings）
    openai.rs       # OpenAiProvider（OpenAI-compatible Chat Completions + embeddings）
  search.rs         # RRF + embedding 余弦排序（rank_embedding_paths_by_cosine）
  commands.rs       # 全部 Tauri 命令注册
  db.rs             # SQLite（含 FTS5 ask_turns 触发器、wiki_page_history 快照表）
  models.rs         # 全部数据模型
  state.rs          # 核心逻辑（含 resolve_graph_node_label/resolve_wiki_semantic_title、checksum 防并发）
  vault.rs          # 文件系统（hash去重, ingest_markdown + resolve_wiki_semantic_title）

web/src/
  App.tsx           # 主界面（Inbox/Wiki/Ask/Lint/图谱/统计/Settings 全集成）
  tauri-client.ts   # invoke 封装
  types.ts          # TS 类型定义
  app-utils.test.ts # 单元测试（174 通过）
  styles.css        # 样式
```

---

## 关键架构约束

- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`（云端优先）；Embed 走 `get_embed_provider()`（始终本地 Ollama，`nomic-embed-text:latest`）
- **Ingest 超时**：前端 `INGEST_TIMEOUT_MS = 300_000`（5分钟）；LLM 输入截断 8000 字符
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`；`lint_report_full_future` 模式先 drop(state) 再 await
- **API Key 禁止入仓**：`.claude/`、`.codex/`、`.env` 均在 §16.5 禁止提交
- **Codex 在 WSL**：Rust 编译/测试无法在 WSL 确认，Rust 改动需标注"待 Windows cargo 复核"；Claude Code/Gemini 在 Windows 全链路验证
