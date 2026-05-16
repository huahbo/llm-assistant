# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 查看下方 §活跃 TODO
3. 阅读最新 5 条 git log 了解背景

## 本轮快讯（2026-05-16，Claude Code）— H10/H12/H13 完成审计

### H10 MCP 动态装载 已完成（实现早于计划）
- `agent_chat/mcp.rs`：完整 MCP client（JSON-RPC 2.0 over stdio，initialize 握手，list_tools / call_tool）
- `agent_chat/db.rs`：MCP server CRUD + `sync_mcp_tools` + `get_tool_handler_kind`
- `agent_chat/commands.rs`：`list/upsert/delete_mcp_server` + `reload_mcp_server_tools` Tauri 命令
- `state/chat_service.rs`：`spawn/stop/get/list_running_mcp_clients`
- `agent_chat/tools.rs`：`exec_mcp_or_unknown` 路由至对应 MCP 服务器
- `SettingsModule.tsx`：MCP 服务器配置区（添加/连接/删除/状态）
- 新增 4 条 MCP CRUD 测试（268 通过）

### H12 本地嵌入 已完成（Phase A + C，Phase B ONNX 保留可选）
- `state/ingest_service.rs`：入库后异步触发 `embed_page`，写入 `page_embeddings`
- `state/wiki_service.rs`：`search_wiki_pages_hybrid` — FTS5 + Ollama 向量 RRF，Ollama 不可用时自动降级
- `agent_chat/tools.rs`：`exec_search_wiki` 调用混合检索
- `SettingsModule.tsx`：Embedding 模型 / Base URL 配置区

### H13 图谱双向联动 已完成（全部三个 Phase）
- `contexts/GraphBridgeContext.tsx`：`highlightedPaths / chatPrefill / askPrefill` + `extractWikiPaths`
- `GraphModule.tsx`：消费 `highlightedPaths` 高亮节点；右键菜单"问这个"→ `setChatPrefill`，"检索相关"→ `setAskPrefill`
- `ChatModule.tsx`：AI 响应完成后提取 wiki 路径更新高亮；消费 `chatPrefill` 预填输入框
- `AskModule.tsx`：消费 `askPrefill` 预填并执行搜索

### 历史工具调用显示 已完成
- `Message` 结构体新增 `tool_calls: Option<serde_json::Value>`（解析自 `tool_calls_json`）
- `list_messages` 构建时顺带解析，Tauri 序列化直达前端
- `MessageThread.tsx` 新增 `buildDisplayGroups`：将 DB 消息（含 tool/tool_calls）重组为带 ToolGroup 的展示段
- 历史对话重载后，工具调用卡片与流式阶段视觉一致



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

## 验证基线（2026-05-16）

```powershell
cd src-tauri; cargo test    # 268 通过 ✅
cd ../web; npm run typecheck # 零错误 ✅
```

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| *(待提交)* | test(agent_chat): 补充 H10 MCP CRUD 测试 +4 |
| `52d6b31` | docs: 收口 — 更新 dev-status，历史工具调用已完成，基线 264 |
| `31df6c8` | feat(chat): 对话重载时显示历史工具调用记录 |
| `612e8c5` | refactor(agent): exec_read_wiki 直接读文件而非解析格式化字符串 |
| `09d3808` | feat(agent): 跨会话消息内容搜索 |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 🟢 1 | **历史工具调用显示** | 已完成（2026-05-15） | `buildDisplayGroups` 将 DB 消息重组为 ToolGroup 展示段；typecheck ✅ 264测试 ✅ |
| 🔴 2 | **前端 typecheck 验证** | 已确认 ✅ | `npm run typecheck` 零错误 |
| 🟢 3 | **H10 MCP 扩展** | 已完成（2026-05-16 审计确认） | Phase A(ToolRegistry) 跳过，Phase B+C 全实现 |
| 🟢 4 | **H12 本地嵌入** | 已完成（Phase A+C，ONNX Phase B 保留可选） | Ollama + RRF，ingest 自动 embed |
| 🟢 5 | **H13 图谱双向联动** | 已完成（2026-05-16 审计确认） | 全三个 Phase 均实现 |
| 🟢 6 | **打包发布 (P22)** | 未开始 | `npm run tauri build` —— 需先确认 typecheck ✅ |

---

## 关键架构约束

- **测试基线**：268 通过（2026-05-16）
- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**
- **子代理深度限制**：最大 depth 2（根 0 → 子 1 → 孙 2），`exec_spawn_subagent` 以 DB `depth` 字段判断，不用标题前缀
- **审批约束**：write_wiki/edit_wiki 写盘必须经用户确认
- **state.rs 模块化**：H16 完成 12 阶段拆分，现为 `state/` 子模块体系（wiki/ingest/lint/graph/ask/chat/agent/research_service 等）
