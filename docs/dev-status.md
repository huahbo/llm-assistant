# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 查看下方 §活跃 TODO
3. 阅读最新 5 条 git log 了解背景

---

## 本轮快讯（2026-05-20，Claude Code）— H24 进行中

- **H24 无头浏览器服务**：正在并行实施 4 路子任务
  - [A] `src-tauri/src/browser/mod.rs` — CDP 服务模块（headless_chrome + reqwest 兜底）
  - [B] `ingest_service.rs` 重构 — 删旧 Edge 临时实现，改调 crate::browser
  - [C] `commands.rs + main.rs` — 新增 fetch_url_context Tauri 命令
  - [D] 前端 — UrlContextCard 组件 + ChatInputBar URL paste 检测
- 详见：`docs/实施计划-H24-浏览器服务.md`
- **基线（2026-05-17）**：268 测试全绿，typecheck 零错误，最新 commit fd4ecda

---

## 上轮快讯（2026-05-17，Claude Code）— H21/H22/H23 完成

- **Pre**：search.ts 6 处裸 invoke() 包装 withTimeout
- **H21 全局命令面板**：Ctrl+K，模糊搜索 Wiki 页面 + 操作命令 + 最近访问
- **H22 Wiki 知识导出**：Markdown ZIP + 静态 HTML ZIP（pulldown-cmark 渲染，[[link]] 转换）
- **H23 Wiki 内联 AI 辅助**：选中文字 → 续写/改写/扩写，流式预览 + 接受/拒绝
- **268 测试全绿，typecheck 零错误**
- **最新 commit**：fd4ecda（2026-05-17）

---

## 验证基线（2026-05-17）

```powershell
cd E:\llm-wiki\src-tauri; cargo test    # 268 通过 0 失败
cd E:\llm-wiki\web; npm run typecheck   # 零错误
```

---

## 已完成功能总览

### 基础层（P0–P16）

- LLM Provider trait：Ollama + OpenAI-compatible + Hybrid 路由
- Ingest 全链路：md/pdf/url/docx/pptx/txt/图片 OCR，多格式路由，持久化队列（重启恢复/重试/取消）
- Wiki CRUD：保存/删除/重命名/内联编辑/Markdown 渲染/Ctrl+S
- Ask/Query：FTS5 + 向量 RRF 四路混合检索，多轮流式会话，历史持久化
- Lint：语义 lint（LLM 矛盾/陈旧）+ 结构 lint（断链/孤儿），摄入后快速结构 Lint 通知卡
- Graph：可视化图谱，Global/Local 模式，洞察层（孤立/桥接/异常连接）
- Outbox 事件流，页面变更历史 + 恢复，Vault 统计仪表盘
- Deep Research 全链路（多查询 web 搜索→综合→写回 Wiki，任务管理 + 进度流）
- Web Clipper（Chrome 扩展 + 本地 HTTP 服务端口 19827）
- SearXNG 本地搜索（四级联搜，Windows 配置模板 + 自检脚本）
- 摄入 HITL 审核（preview_ingest_file / apply_ingest_preview）

### 高阶功能（H6–H17）

- **H6-S2**：Agent 多轮工具循环（run_shell/search_wiki/read_wiki/write_wiki 审批）
- **H7**：App.tsx 架构拆分（5 个 Context + 9 个模块，消除 9500 行单组件）
- **H8**：ReAct 流式对话 Agent（多轮 LLM + 工具循环，chat_stream，工具卡折叠）
- **H9**：Chat UI 优化（web_search 四级联搜，fetch_url，代码块主题，重复消息修复）
- **H10**：MCP 动态装载（JSON-RPC stdio，upsert/delete/reload 命令，SettingsModule UI）
- **H11**：Swarm 子代理（层级追踪 max depth 2，list_wiki_pages，read_wiki 分页，search_wiki 混合）
- **H12**：本地嵌入（Ollama embed_page，FTS5 + 向量 RRF，Ollama 不可用自动降级）
- **H13**：图谱双向联动（GraphBridgeContext，右键"问这个"/"检索相关"，AI 回复高亮）
- **H14-H15**：跨会话消息搜索，对话导出（Markdown + 存 Wiki），spawn_subagent 工具卡
- **H16**：state.rs 12 阶段重构（state/ 子模块体系，12 个 service 文件）
- **H17**：MCP 市场（Smithery Registry 搜索 + 一键安装 + Env Key 对话框）+ Skill UX 重设计（preset/clipboard）
- **H21**：全局命令面板 Ctrl+K（CommandPalette.tsx，3类结果，键盘导航，最近访问 localStorage）
- **H22**：Wiki 知识导出（export_wiki_markdown_zip / export_wiki_html_zip，Operations 导出 Tab）
- **H23**：Wiki 内联 AI 辅助编辑（ai_assist_wiki_edit，SelectionToolbar，AiAssistPreview，流式）

---

## 活跃 TODO

| 优先级 | 任务 | 说明 |
|--------|------|------|
| P22 | **打包发布** | `npm run tauri:build`，生成 .msi/.exe 安装包 |
| H10A | ToolRegistry trait 重构 | 可选优化 |

---

## 关键架构约束

- **测试基线**：268 通过（2026-05-17，v0.2.4，commit fd4ecda）
- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**
- **子代理深度限制**：max depth 2（根→子→孙），`exec_spawn_subagent` 以 DB `depth` 字段判断
- **审批约束**：write_wiki/edit_wiki 写盘必须经用户确认
- **state/ 子模块体系**：H16 完成 12 阶段拆分，现为 12 个 service 文件
