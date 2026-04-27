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

## 验证基线（2026-04-27 H4 收口）

```powershell
# Windows PowerShell
cd src-tauri; cargo test          # 应通过（H4 改动含新 helper + 测试更新）
cd ../web; npm run typecheck      # 通过 ✅
cd ../web; npm run test -- --run  # WSL rollup 依赖缺失，暂跳过
cd ../web; npm run build
```

- H3 Windows 端到端：用户回传通过（2026-04-27）
- H4 Windows cargo test：待用户复核

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `1ca66d6` | feat(H4): research_mode — wiki content snippets for richer draft context |
| `92f67b7` | docs: H3 收口 + H4 Research 联动计划 |
| `2540c2b` | fix(H3): memory form layout — button full-width |
| `0597e54` | fix(H3): memory key optional — auto-derive from value |
| `730818b` | feat(H3): Agent skill CRUD + prompt injection + draft skill badge |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 1 | **H4 Windows 验证** | 待用户 | cargo test 复核；研究模式端到端测试 |
| 2 | **方向 B：项目模板/多项目体验** | 待收口 | 功能已落地，待用户端到端复核后收口 |
| 3 | **方向 C：会话持久化增强** | 待收口 | 一期+二期已完成，待用户端到端复核后收口 |
| 4 | **H5 方向** | 待用户定 | 用户验证 H4 后决定下一步 |

---

## 代码快照（2026-04-27，H4 收口）

```
src-tauri/src/
  commands.rs   # generate_agent_draft 含 skill_key + research_mode
  db.rs         # agent_runs/events/drafts/memories/skills 表
  models.rs     # AgentSkillItem / AgentDraftConflictInfo
  state.rs      # skill 注入 / AAAK-lite / research_mode / extract_content_after_frontmatter
  vault.rs      # 文件系统

web/src/
  App.tsx       # B2 双栏 + skill 面板 + 记忆芯片 + "检索增强"toggle
  tauri-client.ts  # generateAgentDraft(runId, topic, skillKey, researchMode)
  types.ts      # AgentSkillItem / AgentMemoryItem
  app-utils.test.ts
  styles.css
```

---

## H3/H4 功能速查

| 功能 | 入口 | 说明 |
|------|------|------|
| skill CRUD | Agent Studio 技能模板区 | 新建/删除 skill；下拉选择生效 skill |
| skill 注入 | 生成 draft 时自动 | 将 skill prompt 注入 LLM context |
| skill badge | 草稿头元信息 `· skill:<key>` | 从 run 事件解析 |
| 记忆 | 记忆上下文芯片 + 添加表单 | 键可选，留空自动派生 |
| 检索增强 | 输入栏下方 checkbox | 开启后读取 wiki 正文（400字），搜索 8 条 vs 5 条 |

---

## 关键架构约束

- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Ingest 超时**：`INGEST_TIMEOUT_MS = 300_000`；LLM 输入截断 8000 字符
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**：`.claude/`、`.codex/`、`.env` 禁止提交
- **Codex 在 WSL**：Rust 改动需标注"待 Windows cargo 复核"
- **审批约束**：写盘必须经确认弹窗，禁止静默覆盖
