# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 查看下方 §活跃 TODO
3. 阅读最新 5 条 git log 了解背景

## 本轮快讯（2026-05-15，Claude Code）

### H11 Swarm 子代理层级追踪 已完成
- `agent_conversations` 表新增 `parent_conv_id INTEGER` / `depth INTEGER NOT NULL DEFAULT 0`
- SQLite 幂等迁移（`let _ = conn.execute("ALTER TABLE ...")`）
- `list_conversations` 两个分支均加 `WHERE depth = 0`，子代理会话不出现在主列表
- 新增 `list_child_conversations` / `get_conv_depth` DB 函数
- `exec_spawn_subagent` 改用结构化深度检测（max depth 2，即根→子→孙三层）
- 新增 Tauri 命令：`list_child_conversations`
- 新增测试：`test_depth_tracking_and_list_excludes_subagents`

### H11 list_wiki_pages 工具 已完成
- 新增工具 `list_wiki_pages`，支持 `path_prefix` 过滤，最多返回 100 条
- 工具描述已注入 seed tools（9 个工具）

### H11 read_wiki 分页 已完成
- `exec_read_wiki` 新增 `start_char` 参数，8000 字符分块输出
- 重构：直接 `fs::read_to_string` 读文件，不再解析格式化字符串（防止内容含 `content=` 时截断）

### H11 search_wiki 混合检索 已完成
- `exec_search_wiki` 改为 `async fn`，调用 `search_wiki_pages_hybrid`（FTS5 + Ollama 向量 RRF）
- Ollama 不可用时优雅降级到 FTS5

### 跨会话消息搜索 已完成
- DB 新增 `search_messages(query, limit)` — LIKE 搜索 user/assistant 消息，JOIN conversations，只返回 depth=0 的对话
- 新增 Tauri 命令 `search_chat_messages`
- 前端 `ConversationList` 新增消息搜索结果区（防抖 300ms，2+ 字符触发）

### 对话导出功能 已完成
- 新增 Tauri 命令 `export_conversation_markdown` — 格式化 user/assistant 消息为 Markdown
- `MessageThread` 新增"导出"按钮（复制到剪贴板）和"存入 Wiki"按钮（保存到 `chat-exports/`）

### spawn_subagent 工具卡片渲染 已完成
- `ToolCallCard` 特判 `spawn_subagent`，显示 🤖 图标 + 子对话 conv badge + 可滚动摘要

---

## 验证基线（2026-05-15）

```powershell
cd src-tauri; cargo test    # 264 通过 ✅
cd ../web; npm run typecheck # 待验证（上一轮零错误）
```

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `612e8c5` | refactor(agent): exec_read_wiki 直接读文件而非解析格式化字符串 |
| `09d3808` | feat(agent): 跨会话消息内容搜索 |
| `81b1062` | feat(agent): read_wiki 支持 start_char 翻页 |
| `163d67a` | feat(agent): 新增 list_wiki_pages 工具 |
| `c32eefa` | perf(agent): search_wiki 工具升级为 FTS5+向量 RRF 混合检索 |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 🔴 1 | **H11 子代理结果反馈** | 未开始 | 父对话轮次结束后，子代理最终答案自动拼入父对话 tool_result；当前只有 conv badge 但父上下文看不到子代理内容 |
| 🔴 2 | **前端 typecheck 验证** | 待确认 | `npm run typecheck` 需在 Windows 侧执行（WSL 缺 rollup 可选依赖） |
| 🟡 3 | **H10 MCP 扩展** | 计划已有 (`docs/h10-mcp-plan.md` 若有) | MCP server 动态装载 |
| 🟡 4 | **H12 ONNX 本地推理** | 未开始 | 离线向量嵌入，不依赖 Ollama |
| 🟡 5 | **H13 图谱双向联动** | 未开始 | 从对话/Agent 结果自动更新知识图谱 |
| 🟢 6 | **打包发布 (P22)** | 未开始 | `npm run tauri build` —— 需先确认 typecheck ✅ |

---

## 关键架构约束

- **测试基线**：264 通过（2026-05-15）
- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**
- **子代理深度限制**：最大 depth 2（根 0 → 子 1 → 孙 2），`exec_spawn_subagent` 以 DB `depth` 字段判断，不用标题前缀
- **审批约束**：write_wiki/edit_wiki 写盘必须经用户确认
- **state.rs 模块化**：H16 完成 12 阶段拆分，现为 `state/` 子模块体系（wiki/ingest/lint/graph/ask/chat/agent/research_service 等）
