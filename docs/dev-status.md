# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。
> 完整历史存档见 `docs/completed-log.md`（仅按需查阅）。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 读 `docs/交接状态卡.md` 与 `docs/多Agent通信与交接协议.md`，确认当前接力状态
3. 读 `docs/实施过程记录.md` 最新 3 条了解背景
4. 查看下方 §活跃 TODO，按优先级 1→2 开始

---

## 验证基线（2026-04-27 H3 收口，用户已确认）

```powershell
# Windows PowerShell
cd src-tauri; cargo test          # 待 Windows 复核（Rust 改动已有）
cd ../web; npm run typecheck      # 通过 ✅（本轮多次验证）
cd ../web; npm run test -- --run  # WSL rollup 依赖缺失，暂跳过
cd ../web; npm run build
```

- H3 Windows 端到端验证：用户回传通过（2026-04-27）
  - skill CRUD + 草稿头 skill badge ✅
  - 记忆保存键可选修复 ✅
  - 记忆表单布局修复（按钮不再竖排）✅

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `2540c2b` | fix(H3): memory form layout — inputs row + button full-width below |
| `0597e54` | fix(H3): memory key optional — auto-derive from value if left blank |
| `730818b` | feat(H3): Agent skill CRUD + prompt injection + draft skill badge |
| `9d7a0be` | feat(agent-studio): complete B2 lite/review/flow and handoff docs |
| `64b2e65` | docs: 更新 dev-status 基线至 190/179（H2 完成） |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 1 | **方向 H：H4 Research 联动** | 进行中 | 在 generate_agent_draft 中增加 research_mode 开关；先 search 再生成；前端 toggle |
| 2 | **方向 B：项目模板/多项目体验** | 待收口 | 功能已落地，待用户端到端复核后收口 |
| 3 | **方向 C：会话持久化增强** | 待收口 | 一期 + 二期已完成，待用户端到端复核后收口 |

---

## H4 实施计划（Research 联动）

**目标**：生成 draft 时可选"检索增强"，先跑 search 拿相关 wiki 片段再喂给 LLM。

**涉及文件**：
- `src-tauri/src/state.rs` — `generate_agent_draft_impl` 增加 `research_mode: bool` 参数；开启时调用 `search_impl` 注入相关片段
- `src-tauri/src/commands.rs` — `generate_agent_draft` 增加 `research_mode: Option<bool>`
- `web/src/tauri-client.ts` — `generateAgentDraft` 增加 `researchMode?: boolean`
- `web/src/App.tsx` — 增加 `agentResearchMode` 状态 + toggle 开关 UI

**无需新表、无 schema 改动**

**后端逻辑**：
```
generate_agent_draft_impl(run_id, topic, skill_key, research_mode)
  if research_mode:
    hits = search_impl(topic, limit=5)           // 已有函数
    related_context = format hits as markdown    // 注入 prompt
  else:
    related_context = (原有逻辑：按 topic 向量搜)
```

**前端 UI**：在输入栏旁增加一个"🔍 检索增强"toggle（checkbox 或小按钮）。

---

## 代码快照（2026-04-27，H3 收口）

```
src-tauri/src/
  commands.rs       # 全部 Tauri 命令（generate_agent_draft 含 skill_key）
  db.rs             # SQLite（agent_runs/events/drafts/memories/skills 表）
  models.rs         # 数据模型（AgentSkillItem / AgentDraftConflictInfo）
  state.rs          # 核心逻辑（skill prompt 注入 / AAAK-lite / draft 生成）
  vault.rs          # 文件系统

web/src/
  App.tsx           # B2 双栏布局 + skill 面板 + 记忆芯片 + 草稿头 skill badge
  tauri-client.ts   # invoke 封装（generateAgentDraft 支持 skillKey）
  types.ts          # TS 类型（AgentSkillItem / AgentMemoryItem）
  app-utils.test.ts # 单元测试
  styles.css        # 全部样式
```

---

## 关键架构约束

- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Ingest 超时**：`INGEST_TIMEOUT_MS = 300_000`；LLM 输入截断 8000 字符
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**：`.claude/`、`.codex/`、`.env` 在 §16.5 禁止提交
- **Codex 在 WSL**：Rust 改动需标注"待 Windows cargo 复核"
- **审批约束**：写盘必须经确认弹窗，禁止静默覆盖
