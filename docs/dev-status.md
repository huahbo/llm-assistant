# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 查看下方 §活跃 TODO
3. 阅读最新 5 条 git log 了解背景

---

## 本轮快讯（2026-05-22，Claude Opus 4.7）— H26 全面收口 + 手动保存改造

接续 Sonnet 4.6 的 H26-B/C/A/E 基础上，做了系统性质量打磨：

- **大纲审批 UI 完整化**（commit `8c1984b`）：key_questions 每条可编辑、可增删、章节可增删、空值校验
- **关闭后可重新打开**（commit `83d7b14`）：后端 `pending_outline_data` / `pending_query_data` / `pending_research_reports` 缓存 + `get_pending_*` 命令；ResearchPanel 加 `💬 打开对话` / `⏸ 需要确认` 按钮
- **大纲生成 JSON 解析强化**（commit `5c13f54`）：4 重 fallback（围栏剥离 / 括号配对 + 字符串内 brace 屏蔽 / 单引号容错 / 兜底）+ 7 个单测；超时/通道关闭/解析失败时 emit 明确提示
- **报告组装 bug 修复**（commit `4fbb326`）：Conclusion 位置（之前出现在 body 中间）+ References 换行（`\n` → `\n\n`）
- **来源排序统一**（commit `1ca7745`）：H25/H26 两条路径都按 quality_score 排序后 truncate
- **手动保存改造**（commit `4d82713`）：报告完成后默认不写盘，停在对话框 4 按钮（保存/导出/丢弃/关闭）；DB 状态新增 `awaiting_save` / `discarded`
- **保存超时根治**（commit `843e7f2`）：commit_research_to_wiki 拆快慢两阶段，ingest 后台 spawn；新增 research_indexing / research_indexed 事件；前端显示「⏳ 后台索引中」徽章；HashMap::remove 防并发重提
- **LLM 调用鲁棒性**（commit `209542d`）：complete_with_retry helper，section/intro/conclusion 失败自动重试一次 + 暴露具体错误
- **Ollama 默认超时**（commit `468a654`）：60s → 120s 与 OpenAI 对齐
- **跨模块同步 + UI 优化**（commits `3127599` / `86956e7` / `8e5a658`）：深度/广度从 SearchConfig 同步初始值；SearchConfigPanel 重排（搜索源左列跨两行，深度/广度右列）；搜索进度按 ✓/✗ 前缀决定是否折叠

**版本号 0.2.6 → 0.2.7**，测试基线 291 → **298**，typecheck 零错误

**下一轮接力**：P22 打包发布（`tauri build`）或 H27 自适应追踪搜索（独立立项）

---

## 上轮快讯（2026-05-20，Claude Code）— H24 完成

- **H24 无头浏览器服务**：`src-tauri/src/browser/mod.rs` 已实装
  - Chrome→Edge→静态HTTP 三级兜底，headless_chrome CDP + spawn_blocking
  - `ingest_service.rs` 旧 Edge 临时实现已清除，复用 crate::browser
  - `fetch_url_context` Tauri 命令 + 前端 UrlContextCard + ChatInputBar URL paste 检测
  - 顺手修复旧 `html_to_text` 反向引用 bug（`\1` Rust regex 不支持）
- **284 测试全绿**（新增 3 个 browser 单元测试），typecheck H24 零错误
- **最新 commit**：3fd7da7（2026-05-20）

---

## 上轮快讯（2026-05-17，Claude Code）— H21/H22/H23 完成

- **Pre**：search.ts 6 处裸 invoke() 包装 withTimeout
- **H21 全局命令面板**：Ctrl+K，模糊搜索 Wiki 页面 + 操作命令 + 最近访问
- **H22 Wiki 知识导出**：Markdown ZIP + 静态 HTML ZIP（pulldown-cmark 渲染，[[link]] 转换）
- **H23 Wiki 内联 AI 辅助**：选中文字 → 续写/改写/扩写，流式预览 + 接受/拒绝
- **268 测试全绿，typecheck 零错误**
- **最新 commit**：fd4ecda（2026-05-17）

---

## 验证基线（2026-05-22）

```powershell
cd E:\llm-wiki\src-tauri; cargo test    # 298 通过 0 失败
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
| P22 | 打包发布 | `npm run tauri:build`，生成 .msi/.exe 安装包 |
| H10A | ToolRegistry trait 重构 | 可选优化 |
| H27 | 自适应追踪搜索 | 参考 local-deep-research LangGraph 策略，单独立项后再做 |

---

## 关键架构约束

- **测试基线**：298 通过（2026-05-22，v0.2.7，commit 468a654）
- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**
- **子代理深度限制**：max depth 2（根→子→孙），`exec_spawn_subagent` 以 DB `depth` 字段判断
- **审批约束**：write_wiki/edit_wiki 写盘必须经用户确认
- **state/ 子模块体系**：H16 完成 12 阶段拆分，现为 12 个 service 文件
