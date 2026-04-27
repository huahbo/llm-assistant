# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 读 `docs/交接状态卡.md`，确认当前接力状态
3. 读 `docs/实施过程记录.md` 最新 3 条了解背景
4. 查看下方 §活跃 TODO

---

## 验证基线（2026-04-27 H5 全部推送）

```powershell
cd src-tauri; cargo test          # 待 Windows 复核
cd ../web; npm run typecheck      # 通过 ✅
```

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `2bdd80c` | feat(H5-D): skill 模板变量 `{{topic}}` `{{memories}}` |
| `efe238e` | feat(H5): auto-memory + draft rewrite + ask-first injection |
| `1ca66d6` | feat(H4): research_mode |
| `92f67b7` | docs: H3 收口 + H4 计划 |
| `2540c2b` | fix(H3): memory form layout |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 1 | **H5 Windows 验证** | 待用户 | cargo test；四个功能端到端测试 |
| 2 | **H6 方向** | 待用户定 | H5 验证后决定 |

---

## H5 功能速查

| 功能 | 入口 | 说明 |
|------|------|------|
| 自动记忆提炼 (B) | 审批写盘后自动触发 | LLM 从草稿提炼 3-5 条全局记忆；事件日志记录数量 |
| 批注重写 (A) | 草稿头"基于批注重写"输入栏 | 原草稿 + 批注 → LLM → 新草稿；自动选中 |
| Ask 联动 (C) | 输入栏"Ask 联动"checkbox | 先 query_ask(top_k=3) 获取现有知识库答案注入 prompt |
| Skill 模板变量 (D) | Skill prompt 内写 `{{topic}}` `{{memories}}` | 生成时自动替换，skill 可复用 |
| 检索增强 (H4) | 输入栏"检索增强"checkbox | 读 wiki 正文 400 字，搜 8 条 |

---

## 关键架构约束

- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**
- **审批约束**：写盘必须经确认弹窗，禁止静默覆盖
- **Ask 联动速度**：ask_first=true 会有两次 LLM 调用，生成时间翻倍
