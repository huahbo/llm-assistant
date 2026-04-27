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

## 验证基线（2026-04-27 H3，B2-Flow 收口）

```powershell
# Windows PowerShell（待用户复核）
cd src-tauri; cargo test
cd ../web; npm run typecheck
cd ../web; npm run test -- --run
cd ../web; npm run build
cd ..; npm run tauri:dev
```

- WSL 自动化验证结果占位（本轮）：
  - `cd web && npm run typecheck`：通过 ✅
  - `cd web && npm run test -- --run`：失败 ❌（`@rollup/rollup-linux-x64-gnu` 可选依赖缺失）
  - `cd web && npm run build`：失败 ❌（同上）
- 结论：`B2-Flow` 已到可验收状态，最终通过条件为用户 Windows 复核回传。

- `scripts/verify_clipper_windows.ps1`：用户回传通过（2026-04-22）
- `scripts/verify_searxng_windows.ps1`：用户回传通过（2026-04-22）
- Deep Research 端到端实跑：通过（城市复杂水网AI仿真，2026-04-23）

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `9ea3610` | feat(H2): agent_memories CRUD + AAAK-lite 压缩层 + 记忆面板 UI |
| `ebacf92` | docs: 更新 dev-status 基线至 190/177（H1-next 完成） |
| `6cb88e6` | feat(H1-next): 草稿 Markdown 渲染 + 审批确认弹窗 + 冲突预检 |
| `0f153b2` | feat(H1): Agent Studio 时间优化 + Draft 生成/审批链路（Codex 实现） |
| `112c988` | fix(test): 修复 agent_draft_generate_and_approve_impl_works 测试 |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 1 | **方向 H：Agent Studio B2 三阶段改造** | reviewing | `B2-Lite`、`B2-Review`、`B2-Flow`（结构先行 + 增量渲染 + 暂停/继续）已完成，待用户 Windows 复核 |
| 2 | **方向 B：项目模板/多项目体验** | 待收口 | 功能已落地，待用户 Windows 端到端复核后收口 |
| 3 | **方向 C：会话持久化增强** | 待收口 | 一期 + 二期已完成，待用户 Windows 端到端复核后收口 |
| — | 用户新需求 | 待提 | 用户提到有新需求，尚未描述 |

---

## 代码快照（2026-04-27，基线 190 Rust / 179 前端）

```
src-tauri/src/
  llm/
    provider.rs     # LlmProvider trait（含 embed/complete_stream/health_check）
    ollama.rs       # OllamaProvider
    openai.rs       # OpenAiProvider
  search.rs         # RRF + embedding 余弦排序
  commands.rs       # 全部 Tauri 命令（含 check_agent_draft_conflict）
  db.rs             # SQLite（含 agent_runs/events/drafts/memories 表）
  models.rs         # 全部数据模型（含 AgentDraftConflictInfo）
  state.rs          # 核心逻辑（含 check_agent_draft_conflict_impl、make_test_state_bare）
  vault.rs          # 文件系统（ingest_markdown + resolve_wiki_semantic_title）

web/src/
  App.tsx           # 主界面（含 Agent Studio B2：双栏 + Draft/Diff/Citations + Flow 暂停/继续）
  tauri-client.ts   # invoke 封装（含 checkAgentDraftConflict）
  types.ts          # TS 类型（含 AgentDraftConflictInfo）
  app-utils.test.ts # 单元测试（177 通过，含 marked 渲染测试）
  styles.css        # 样式（含 agent-draft-confirm-dialog、agent-studio__draft-markdown）
```

---

## 关键架构约束

- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`（云端优先）；Embed 走 `get_embed_provider()`（始终本地 Ollama，`nomic-embed-text:latest`）
- **Ingest 超时**：前端 `INGEST_TIMEOUT_MS = 300_000`（5分钟）；LLM 输入截断 8000 字符
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**：`.claude/`、`.codex/`、`.env` 均在 §16.5 禁止提交
- **Codex 在 WSL**：Rust 编译/测试无法在 WSL 确认，Rust 改动需标注"待 Windows cargo 复核"
- **Agent Studio 审批约束**：写盘必须经审批流程（confirm dialog），禁止静默覆盖
