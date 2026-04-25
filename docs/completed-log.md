# completed-log.md — 已完成功能存档

> 归档层：仅追加，不修改。由主控 Agent 在每轮结束时追加新条目。
> 新 Agent 启动时**无需**全量阅读本文件；只在需要了解某功能历史时按需查阅。

## 已完成（截至 2026-04-25，持续追加）

| 优先级 | 功能 | 状态 |
|--------|------|------|
| P0 | `LlmProvider` trait + Ollama 实现 | ✅ `src-tauri/src/llm/` |
| P0 | Ingest 使用 LLM 生成摘要（替换 truncate） | ✅ `state.rs::ingest_markdown` |
| P0 | Query 使用 LLM 合成回答（FTS 召回 + Ollama 生成） | ✅ `state.rs::generate_query_answer_with_provider` |
| P0 | `get_llm_status` 命令 + 前端 LLM 状态显示 | ✅ |
| P1 | 实体提取（Ingest 时 LLM 提取关键实体） | ✅ `state.rs::extract_entities` |
| P1 | 双向链接注入（See Also 节，FTS 同步） | ✅ `state.rs::update_related_pages_with_link` + `vault.rs::append_see_also_link` |
| P1 | IngestResult 返回 `entities` + `updated_pages` | ✅ `models.rs` + `types.ts` |
| P2 | 语义 Lint（LLM 矛盾/陈旧/覆盖度检测） | ✅ `state.rs::lint_report_full_future` |
| P2 | `run_lint` 命令升级为 async + 语义合并 | ✅ `commands.rs` |
| P3-A | 进度反馈（Tauri emit + 前端 listenProgress） | ✅ `state.rs::emit_progress` + `tauri-client.ts::listenProgress` |
| P3-B | Cloud Provider（OpenAI-compatible + Hybrid 路由 + Settings UI） | ✅ `src-tauri/src/llm/openai.rs` + `get_llm_config/set_llm_config` |
| P3-C | Ingest 实体写入 frontmatter（`title/source/raw/imported_at/entities`） | ✅ `vault.rs::build_wiki_frontmatter` + `state.rs::ingest_markdown` |
| P4-A | Frontmatter 读取与 UI 展示（详情页元数据区） | ✅ `state.rs::wiki_page_detail` + `web/src/App.tsx` |
| P4-B | 摘要折叠按行数 + 调试面板独立化 + 文档对齐 | ✅ `web/src/App.tsx` + `styles.css` + `app-utils.test.ts` |
| P5-A | Lint 分组折叠（按路径）+ Query 保存后自动跳转 Wiki | ✅ `web/src/App.tsx` + `styles.css` + `app-utils.test.ts` |
| P5-B 前端 | Lint 补丁建议分组折叠 + "打开页面"按钮 | ✅ `web/src/App.tsx`（85→85 测试） |
| P5-B 后端 | `ingest_url` 命令（reqwest 拉取 + 复用 ingest） | ✅ `src-tauri/src/state.rs` + `commands.rs`（68 测试） |
| P6 前端 | URL 摄入 UI + `ingestUrl` tauri-client 封装 | ✅ `web/src/tauri-client.ts` + `App.tsx`（86 测试） |
| P6 后端 | `save_wiki_page` 命令（写回 vault + 更新 FTS） | ✅ `vault.rs` + `state.rs` + `commands.rs`（70 测试） |
| P7 前端 | Wiki 内联编辑器 UI（编辑/保存/取消，调用 `save_wiki_page`） | ✅ `web/src/App.tsx` + `tauri-client.ts` + `types.ts`（87 测试） |
| P7 后端 | PDF 摄入命令 `ingest_pdf`（提取文本后复用 ingest 流程） | ✅ `src-tauri/src/state.rs` + `commands.rs` + `main.rs`（73 cargo 测试） |
| P8 前端 | Inbox 增加 PDF 摄入入口 + 编辑器未保存离开提示 | ✅ `web/src/App.tsx` + `tauri-client.ts`（90 测试） |
| P8 后端 | PDF 摄入错误可读性增强 + `.PDF`/伪 PDF 回归测试 | ✅ `src-tauri/src/state.rs`（75 cargo 测试） |
| P8 修复 | PDF `ToUnicode CMap` 失败回退提取 + 前端错误映射 | ✅ `src-tauri/src/state.rs` + `web/src/App.tsx`（93/76 测试） |
| P9-A | 统一 `ingest_file` 路由 + 通用文件摄入入口（含 docx/pptx/txt/图片 OCR） | ✅ `state.rs` + `commands.rs` + `main.rs` + `App.tsx`（95/80 测试） |
| P9-B | OCR Provider 选择（tesseract/paddle）+ 双向失败回退 | ✅ `commands.rs` + `state.rs` + `App.tsx` + `tauri-client.ts`（95/82 测试） |
| P9-C 前端 | OCR provider localStorage 持久化 + 安装引导提示 + 格式说明 | ✅ `App.tsx`（99 测试） |
| P9-C 后端 | `default_ocr_provider` AppConfig + get/set 命令 + pptx 自然排序 + docx 段落结构 | ✅ `models.rs` + `state.rs` + `commands.rs`（84 cargo 测试） |
| P9-D 全栈 | PDF OCR 自动回退（解析失败自动 `pdftoppm` 转图 + OCR，回写回退事件与友好提示） | ✅ `src-tauri/src/state.rs` + `web/src/App.tsx` + `web/src/app-utils.test.ts`（WSL `typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-20，**Codex+子代理** 实施） |
| P10 后端 | `WikiPageDetail.content` 字段确认（已存在，补测试） | ✅ `state.rs`（85 cargo 测试） |
| P10 前端 A | OCR 配置后端同步（`fetchOcrConfig`/`saveOcrConfig`） | ✅ `tauri-client.ts` + `App.tsx`（100 测试） |
| P10 前端 B | Wiki 编辑器 Ctrl+S 快捷键 + 字符计数显示 | ✅ `App.tsx` + `styles.css`（102 测试） |
| P11 后端 | `WikiPageItem.score` + FTS5 bm25 排序（降级 instr 优先级评分） | ✅ `db.rs` + `models.rs` + `state.rs`（85 cargo 测试） |
| P11 前端 | Wiki 搜索按 score+标题命中排序 + Ask 历史 chip（localStorage 最多 10 条） | ✅ `App.tsx` + `types.ts` + `styles.css`（105 测试） |
| P12 后端 | `delete_wiki_page` 命令（删除 .md + wiki_pages/citations/fts_pages DB 记录） | ✅ `db.rs` + `models.rs` + `state.rs` + `commands.rs` + `main.rs`（86 cargo 测试） |
| P12 前端 | 搜索关键词高亮标题/摘要（`<mark>`）+ Wiki 详情页删除按钮（二次确认） | ✅ `App.tsx` + `tauri-client.ts` + `types.ts` + `styles.css`（105 测试） |
| P13 前端 | Wiki 正文 Markdown 渲染（marked + DOMPurify，支持 GFM 标题/列表/代码/表格等） | ✅ `App.tsx` + `styles.css`（105 测试） |
| P14 后端 | `rename_wiki_page` 命令（文件重命名 + wiki_pages/citations/fts_pages path 同步） | ✅ `db.rs` + `state.rs` + `commands.rs` + `main.rs`（87 cargo 测试） |
| P14 前端 | Wiki 详情页"重命名"按钮 + inline 输入栏（Enter 确认/Esc 取消，同步刷新列表） | ✅ `App.tsx` + `tauri-client.ts` + `types.ts` + `styles.css`（105 测试） |
| P15 全栈 | 文件路径 picker（tauri-plugin-dialog）+ 多选文件摄入顺序处理 | ✅ Cargo.toml + main.rs + capabilities + tauri-client.ts + App.tsx + styles.css（105 测试） |
| P16 后端 | Ask 历史持久化到 DB（去重迁移 + 上限裁剪 + 安全读取上限） | ✅ `db.rs` + `state.rs`（新增 2 个 db 测试，待 Windows cargo 复核） |
| P16 前端 | Ask 历史 DB 优先 + localStorage 回退（工具函数与测试补齐） | ✅ `App.tsx` + `tauri-client.ts` + `app-utils.test.ts`（108 测试） |
| P17-A | Wiki 标签/分类筛选（按 frontmatter entities 聚合 tag chips） | ✅ `state.rs` + `App.tsx` + `styles.css` |
| P17-B | Vault 文件树浏览（左侧树形层级 + 点击打开页面 + 折叠目录） | ✅ `App.tsx` + `styles.css` + `app-utils.test.ts`（110 测试） |
| P18-1 | Ask 伪流式对话（后端分片 emit + 前端聊天流增量渲染） | ✅ `state.rs` + `App.tsx` + `styles.css` + `app-utils.test.ts`（112 测试） |
| P18-2 | Provider 级真流式单轮（Ollama `/api/generate stream=true` + OpenAI SSE `stream=true`，`complete_stream` trait + fallback） | ✅ `llm/provider.rs` + `llm/ollama.rs` + `llm/openai.rs` + `state.rs`（89 Rust / 112 前端；**Gemini** 实施，2026-04-17 Windows 验证通过） |
| P18-3 | 真流式多轮会话（in-memory session 历史 + 软取消 + 新对话按钮；`query_ask_session` / `cancel_ask_session` / `clear_ask_session` 命令） | ✅ `models.rs` + `state.rs` + `commands.rs` + `main.rs` + `tauri-client.ts` + `App.tsx` + `styles.css`（91 Rust / 112 前端，2026-04-17） |
| P18-UI | Ask 面板 Chat-first UI 重构（底部固定输入栏 + 消息气泡 + Citations 折叠 toggle + 元信息 pills + 保存到 Wiki per-message + ⚙ 高级设置折叠 + auto-scroll + Enter 发送） | ✅ `App.tsx` + `styles.css`（112 前端，2026-04-17） |
| P19-1 | Ask 历史管理增强（时间显示 + 关键词过滤 + 清空入口） | ✅ `db.rs` + `state.rs` + `commands.rs` + `main.rs` + `tauri-client.ts` + `App.tsx` + `styles.css` + `app-utils.test.ts`（118 前端；Rust 待 Windows cargo 复核，2026-04-17，**Codex** 实施） |
| P19-2 | Wiki 文件树增强（📂图标化 + 全部折叠/展开 + 自动定位 Auto-Reveal） | ✅ `App.tsx` + `styles.css`（130 前端测试通过，2026-04-17，**Gemini** 实施） |
| P19-3 | 标签维度增强（多标签 AND 交集筛选 + 标签计数显示） | ✅ `App.tsx` + `styles.css`（130 前端测试通过，2026-04-17，**Gemini** 实施） |
| P19-4 前端 | 内链补全体验收口（`[[` 光标锚点定位 + 查询竞态保护 + `Tab/Enter` 插入 + 空结果提示） | ✅ `web/src/App.tsx` + `web/src/styles.css` + `web/src/app-utils.test.ts`（`typecheck` 通过；WSL `test` 因 Rollup Linux 可选依赖缺失待 Windows 复核，2026-04-20，**Codex** 实施） |
| P20-0 | 调研闸门（MCP/Skills/Workflows 清单与理由） | ✅ 已提交简报（见 2026-04-17 18:15 记录） |
| P20-1 后端 | Outbox 事件流基础（`wiki_outbox` + 导出/ack 命令 + 关键路径事件写入） | ✅ `db.rs` + `models.rs` + `state.rs` + `commands.rs` + `main.rs`（Rust 待 Windows cargo 复核，2026-04-17，**Codex** 实施） |
| P20-2 后端 | Wiki-link 级 lint（`broken_wikilink` / `orphan` / `xref_missing`）+ patch preview/apply 最小可用 | ✅ `state.rs`（新增 2 条 Rust 单测；待 Windows cargo 复核，2026-04-17，**Codex** 实施） |
| P21-Fix 前端 | 图谱点击白屏修复（统一打开链路 + 图谱可见错误提示 + 节点路径健壮性） | ✅ `web/src/App.tsx` + `web/src/app-utils.test.ts`（`typecheck` 通过；WSL `test/build` 因 Rollup Linux 可选依赖缺失待 Windows 复核，2026-04-17，**Codex** 实施） |
| P21-B 前端 | 图谱体验升级（左图谱+右详情、分组/孤儿/邻居筛选、适配视图、节点统计） | ✅ `web/src/App.tsx` + `web/src/styles.css` + `web/src/app-utils.test.ts`（`typecheck` 通过；WSL `test/build` 因 Rollup Linux 可选依赖缺失待 Windows 复核，2026-04-17，**Codex** 实施） |
| P21-C1 前端 | Global/Local 双模式（前端 BFS 子图）+ Hop 深度 + 布局冻结/恢复 + 偏好持久化 | ✅ `web/src/App.tsx` + `web/src/styles.css` + `web/src/app-utils.test.ts`（`typecheck` 通过；WSL `test/build` 因 Rollup Linux 可选依赖缺失待 Windows 复核，2026-04-17，**Codex** 实施） |
| P21-C2 前端 | 搜索高亮（光晕视觉反馈）+ 平滑相机聚焦 + 动态侧边栏搜索结果 | ✅ `web/src/App.tsx` + `web/src/styles.css`（130 前端测试通过，2026-04-17，**Gemini** 实施） |
| P21-D 前端 | 图谱收口（Outbox 自动刷新、可见范围搜索、稳定渲染 key、Ctrl+F、导出 JSON、>200 节点聚合） | ✅ `web/src/App.tsx` + `web/src/app-utils.test.ts`（132 前端测试通过，2026-04-19，**Codex** 实施） |
| P21-E 前端 | 图谱聚合交互深化（聚合节点右侧"展开查看成员页" + 一键切回明细模式） | ✅ `web/src/App.tsx` + `web/src/styles.css`（WSL `typecheck` 通过；完整测试待下一轮，2026-04-20，**Codex** 实施） |
| P20-5 后端 | Embedding 向量检索接入 RRF（`list_embeddings` + 余弦排序 + Query 第四路召回） | ✅ `src-tauri/src/db.rs` + `src-tauri/src/search.rs` + `src-tauri/src/state.rs`（Rust 待 Windows cargo 复核，2026-04-19，**Codex** 实施） |
| P23 全栈 | 持久化 ingest 队列（`ingest_queue_items` 表 + 状态机 + tokio worker + 队列面板 UI + 重启恢复/重试/取消） | ✅ `db.rs` + `models.rs` + `state.rs` + `commands.rs` + `main.rs` + `App.tsx` + `tauri-client.ts` + `types.ts`（116 Rust / 142 前端，2026-04-20，**Claude Code + 子代理** 实施） |
| P20-6 全栈 | 检索可解释性增强（`search_debug` 返回 RRF 各路候选/贡献，Ask 面板可折叠查看） | ✅ `src-tauri/src/models.rs` + `src-tauri/src/state.rs` + `web/src/types.ts` + `web/src/App.tsx` + `web/src/styles.css`（WSL `typecheck` 通过；Rust/前端完整测试待 Windows 复核，2026-04-20，**Codex** 实施） |
| P24 前端 | 移植包 B（阶段一+二+三）图谱洞察层（孤立/稀疏/桥接/异常连接 + 阈值参数化 + 证据可解释 + 异常连接置信度降噪） | ✅ `web/src/App.tsx` + `web/src/styles.css` + `web/src/app-utils.test.ts`（WSL `typecheck` 通过；`npm test` 待 Windows 复核，2026-04-20，**Codex** 实施） |
| P25 前端 | 拖拽摄入（窗口 drop 触发 ingest_file，扩展名过滤+去重+悬停提示） | ✅ `web/src/App.tsx` + `web/src/styles.css` + `web/src/app-utils.test.ts`（WSL `typecheck` 通过；`npm test` 待 Windows 复核，2026-04-20，**Codex** 实施） |
| P25-B 前端 | 拖拽模式切换（直接摄入/加入队列）+ 移植包 B 阶段四 embedding 相似度接入异常连接洞察 | ✅ `web/src/App.tsx` + `web/src/tauri-client.ts`（116 Rust / 149 前端 / typecheck 0 errors，Windows 验证通过，2026-04-20，**Claude Code 子代理** 实施） |
| P25-B 后端 | `get_page_embedding_similarities` Tauri 命令（DB embedding → 余弦对，MIN_SIM=0.25，MAX_PAIRS=1000） | ✅ `src-tauri/src/state.rs` + `src-tauri/src/commands.rs` + `src-tauri/src/main.rs`（118 Rust / 149 前端，2026-04-20，**Claude Code** 实施） |
| BugFix-Ingest | ingesting 卡住修复（前端识别 `ingest_failed` 结束态 + 后端失败路径补发 outbox 事件） | ✅ `web/src/App.tsx` + `src-tauri/src/state.rs`（2026-04-19，**Codex** 实施） |
| BugFix-PDF-Compat | 有效 PDF 误判无效修复（`lopdf` 多级容错加载：内存直读/头修复/尾修复/联合修复 + 前端兼容性错误映射） | ✅ `src-tauri/src/state.rs` + `web/src/App.tsx` + `web/src/app-utils.test.ts`（2026-04-19，**Codex** 实施；Rust 待 Windows cargo 复核） |
| BugFix-PDF-Compat-2 | PDF 兼容增强（`lopdf` 后新增 `pdf-extract` 二级解析 + `FlateDecode` 原始流扫描兜底） | ✅ `src-tauri/Cargo.toml` + `src-tauri/src/state.rs`（2026-04-19，**Codex** 实施；Rust 待 Windows cargo 复核） |
| BugFix-UI-1 | 启动时 `ingesting` 误判为 true（outbox 历史事件污染）→ 添加快进初始化 `outboxInitialized` + 轮询守卫 | ✅ `web/src/App.tsx`（130 前端，2026-04-19，**Claude Code** 实施） |
| BugFix-UI-2 | 孤立 wiki 页面（文件删除后 DB 记录残留）→ 启动时 `purge_orphaned_wiki_pages` 自动清理 | ✅ `src-tauri/src/state.rs` + `main.rs`（102 Rust，2026-04-19，**Claude Code** 实施） |
| BugFix-Index-Lint | `index.md` 残留失效链接导致 `MISSING_INDEX_ENTRY` 批量告警 → 删除/启动清理自动 prune + 本轮数据收口清零 | ✅ `src-tauri/src/state.rs` + `vault/index.md`（WSL `typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-20，**Codex** 实施） |
| BugFix-PDF | PDF 摄入不稳定（15s 超时/LLM 截断/embed 走云端）→ 300s + 8000 char 截断 + `get_embed_provider()` 本地 Ollama | ✅ `state.rs` + `tauri-client.ts`（2026-04-19，**Claude Code** 实施） |
| BugFix-Startup-Reactor | 启动闪退（`there is no reactor running`）→ `start_queue_worker` 从 `tokio::spawn` 改为 `tauri::async_runtime::spawn` | ✅ `src-tauri/src/state.rs`（Windows 启动链路待复核，2026-04-20，**Codex** 实施） |
| BugFix-OCR-Path | 已安装 Tesseract 但应用提示未找到 → OCR 调用增加 Windows 常见安装路径兜底与已尝试命令回显 | ✅ `src-tauri/src/state.rs`（Rust 待 Windows cargo 复核，2026-04-20，**Codex** 实施） |
| BugFix-Clipper-JSON | 浏览器扩展剪藏报 `Bad escaped character in JSON` → clip server 路径响应统一标准化 + 扩展端安全 JSON 解析与错误片段回显 | ✅ `src-tauri/src/clip_server.rs` + `extension/popup.js`（WSL `node --check` 通过；Rust 待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| Opt-Clipper-E2E | Clipper 端到端复核增强（`/status` 返回 `vault_open/vault_path` + 扩展端未开 Vault 提示 + Windows 自检脚本） | ✅ `src-tauri/src/clip_server.rs` + `extension/popup.js` + `scripts/verify_clipper_windows.ps1`（WSL `node --check` 通过；Windows 端脚本待复核，2026-04-22，**Codex** 实施） |
| BugFix-Clipper-Status | `get_clip_server_status` 固定返回 running → 改为真实运行态（Atomic 标记） | ✅ `src-tauri/src/clip_server.rs` + `src-tauri/src/commands.rs`（Windows cargo 待复核，2026-04-22，**Codex** 实施） |
| Opt-SearXNG-E2E-Script | SearXNG Windows 一键自检脚本（禁代理直连 + JSON 诊断输出） | ✅ `scripts/verify_searxng_windows.ps1`（2026-04-22，**Codex** 实施） |
| Opt-SearXNG-Activation | SearXNG 本地搜索激活增强（URL 规范化、`/search`→`/` 回退、配置前置校验、搜索错误可见化） | ✅ `src-tauri/src/state.rs`（新增 5 条 Rust 单测；待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| Opt-SearXNG-Params | SearXNG 检索参数优化（语言优选 + `all/general,news` 回退 + 结果 URL 去重合并 + 不可用引擎提示增强） | ✅ `src-tauri/src/state.rs` + `scripts/verify_searxng_windows.ps1`（WSL 无 cargo/pwsh；待 Windows 复核，2026-04-22，**Codex** 实施） |
| Opt-SearXNG-Windows-Template | SearXNG Windows 推荐配置模板（禁用高故障率引擎）+ PS7 一键启动脚本 | ✅ `configs/searxng/settings.windows.example.yml` + `scripts/run_searxng_windows.ps1` + `README.md`（2026-04-22，**Codex** 实施） |
| BugFix-SearXNG-Script-Robust | SearXNG 脚本健壮性修复（容器就绪等待 + docker logs 排错 + verify 异常链路明细 + 默认配置自动回退） | ✅ `scripts/run_searxng_windows.ps1` + `scripts/verify_searxng_windows.ps1`（2026-04-22，**Codex** 实施） |
| Docs-README-Services | README 增补模块依赖服务与安装配置速查（含 SearXNG/Clipper Windows 自检命令） | ✅ `README.md` + `docs/实施过程记录.md`（2026-04-22，**Codex** 实施） |
| BugFix-Research-Logs | Deep Research 失败"无日志可看" + 报告生成瞬时失败易中断 → 报告阶段自动重试一次 + 失败态任务卡展示日志流 | ✅ `src-tauri/src/state.rs` + `web/src/App.tsx`（WSL `web typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| BugFix-Research-WordExport | Deep Research "导出 Word"按钮在 Tauri 下无响应 → 改为保存对话框选路径 + 后端写盘，浏览器模式保留 Blob 回退 | ✅ `src-tauri/src/commands.rs` + `src-tauri/src/main.rs` + `web/src/tauri-client.ts` + `web/src/App.tsx`（WSL `web typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| BugFix-Research-TaskDelete | Deep Research 任务删除能力（终态任务删除 + 可选同步删除关联 Wiki + 运行中任务禁止直删） | ✅ `src-tauri/src/db.rs` + `src-tauri/src/state.rs` + `src-tauri/src/commands.rs` + `src-tauri/src/main.rs` + `web/src/App.tsx` + `web/src/tauri-client.ts`（WSL `web typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| BugFix-Research-DeleteConfirm | 任务删除确认链路稳定化（删除参数兼容 + 时间显示统一 + Tauri 原生 confirm 二次确认，取消后保留任务） | ✅ `src-tauri/src/commands.rs` + `src-tauri/capabilities/default.json` + `web/src/App.tsx` + `web/src/tauri-client.ts`（用户手测通过，2026-04-21，**Codex** 实施） |
| P26-ResearchDialog | Deep Research 对话框体验收口（状态同步轮询、事件重复订阅修复、Footer 操作统一、UI 配色修复、历史任务状态与重试） | ✅ `web/src/App.tsx`（2026-04-22，**Claude Code** 实施） |
| P26-ResearchExportMD | Research 报告导出 `.md`（按钮 + 本地保存）与历史任务兜底（`doneSavedPath` 读取 Wiki 内容） | ✅ `web/src/App.tsx` + `web/src/tauri-client.ts`（WSL `web typecheck` 通过，2026-04-22，**Claude Code/Codex** 接力） |
| P26-Graph-UX | 图谱双击节点跳转 Wiki（400ms 双击窗口） | ✅ `web/src/App.tsx`（2026-04-22，**Claude Code** 实施） |
| P26-Embed-HealthHint | Embed 健康检查提示优化（ModelNotFound 指引 `ollama pull nomic-embed-text:latest`） | ✅ `src-tauri/src/state.rs`（2026-04-22，**Claude Code** 实施） |
| ResearchExport-Regression | `.md` 历史导出回归复核：代码审查通过 + stale 注释修正（`save_research_doc` 注释从"Word HTML .doc"改为通用写盘描述） | ✅ `src-tauri/src/commands.rs` + `web/src/tauri-client.ts`（2026-04-23，**Claude Code** 实施） |
| P27-Ingest-HITL | 方向 A 摄入审核：`preview_ingest_file/apply_ingest_preview` + Inbox「摄入分析卡」审批后落盘（直摄入全链路） | ✅ `src-tauri/src/{models,state,commands,main}.rs` + `web/src/{App,tauri-client,types,styles,app-utils.test}.ts(x)`（WSL `typecheck` 通过；Rust 与前端全测待 Windows 复核，2026-04-23，**Codex+子代理** 实施） |
| P27-Session-C2 | 方向 C 二期：会话轮次 citations/meta 持久化 + 跨会话检索与命中定位（点击结果自动切会话并定位高亮） | ✅ `src-tauri/src/{db,models,state,commands,main}.rs` + `web/src/{App,tauri-client,types,styles,app-utils.test}.ts(x)`（WSL `typecheck` 通过；Rust 与前端全测待 Windows 复核，2026-04-24，**Codex** 实施） |
| P27-QuickLint-D | 方向 D：摄入后快速结构 Lint（断链+缺失实体页）— 同步无 LLM，fire-and-forget，右下角可关闭通知卡 | ✅ `src-tauri/src/{models,state,commands,main}.rs` + `web/src/{App,tauri-client,types,styles,app-utils.test}.ts(x)`（164 Rust / 161 前端 / typecheck 0 errors，Windows 验证通过，2026-04-24，**Claude Code + 并行子代理** 实施） |
| Refactor-BC-Quality | 方向 B/C A 级质量收口：FTS5 替换 LIKE 全表扫描 + N+1 CTE 修复 + 路径遍历防御 + 512KB 大小限制 + useEffect 自动持久化 + 过期会话防御 + 5 条边界测试 | ✅ `src-tauri/src/{db,state}.rs` + `web/src/{App,app-utils.test}.ts`（167 Rust / 165 前端，2026-04-25，**Claude Code + 并行子代理** 实施） |
| Direction-E-Stats | Vault 统计仪表盘（页面总数/孤立页/热门引用/来源分布/近 30 天增长） | ✅ `src-tauri/src/{db,models,state,commands,main}.rs` + `web/src/{App,tauri-client,types,styles}.ts(x)`（提交 `7da98b8`，2026-04-25） |
| Direction-F-AINewPage | Wiki 主动新建（AI 辅助生成结构化初稿并写入 `vault/wiki/`） | ✅ `src-tauri/src/state.rs` + `web/src/App.tsx` + `web/src/tauri-client.ts`（提交 `f3e559f`，2026-04-25） |
| Direction-G-PageHistory | 页面变更历史（保存前快照 + 历史列表 + 当前/历史行级 diff） | ✅ `src-tauri/src/{db,models,state,commands,main}.rs` + `web/src/{App,tauri-client,types,styles,app-utils.test}.ts(x)`（170 Rust / 169 前端 / build 通过，2026-04-25，**Codex + 子代理尝试后主控串行收口**） |
| RiskFix-G-Restore | 历史版本「一键恢复到此版本」+ 保存接口 checksum 编辑基线 | ✅ `src-tauri/src/{state,commands,main}.rs` + `web/src/{App,tauri-client,styles,app-utils.test}.ts(x)`（173 Rust / 174 前端，2026-04-25，**Claude Code 并行子代理** 收口） |
| Fix-GraphNodeLabel | 图谱节点命名修复：ingest-{timestamp} → 语义标题（entities[0] / source stem）；摄入时 wiki_title 也改用语义名称；清理 extract_wiki_display_name 冗余分支 | ✅ `src-tauri/src/{vault,state}.rs`（183 Rust / 174 前端，2026-04-25，**Claude Code** 实施） |
