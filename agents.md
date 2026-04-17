# agents.md

## 1) 使命
按 A+C 路线构建 Windows 优先的个人 Wiki 桌面应用：
- A：本地优先架构（Tauri + React + SQLite + Markdown Vault）
- C：隐私兼容本地 AI 路径（Ollama + 严格本地模式）

系统必须端到端支持三类核心操作：
- ingest
- query
- lint

## 2) 事实来源
- 产品基线文档：`docs/v1-technical-design.md`
- Vault 内 Markdown 内容是知识数据事实来源。
- `index.md` 与 `log.md` 为强制文件，页面变更时必须同步维护。

## 3) 硬性约束
- 目标平台：Windows 桌面安装包（MSI/EXE）。
- 本地优先：应用运行不依赖后端服务。
- 与 Obsidian 的 Vault 格式兼容，但运行时不依赖 Obsidian。
- 检测到并发外部编辑时，禁止静默覆盖。
- 关键生成结论必须可追溯到 source citations。

## 4) 架构护栏
- UI：React + TypeScript
- 桌面壳与核心服务：Tauri/Rust
- 元数据与检索索引：SQLite + FTS5
- 知识文件：Markdown + frontmatter
- Provider 层必须可插拔：云 Provider 与本地 Ollama 走统一接口

## 5) 运行模式
### Hybrid Mode（默认）
- 允许云与本地 Provider。
- 允许按任务路由。
- 敏感资料可强制本地处理。

### Strict Local Mode
- 仅允许本地 Provider（Ollama）。
- 必须拦截云 Provider 调用。
- 禁止遥测和外部模型 API 请求。

## 6) 工作流契约
### Ingest 契约
- 输入：md/pdf/url 资料。
- 通过内容哈希去重。
- 生成页面更新计划。
- 对 Wiki 页执行增量编辑。
- 更新 `index.md` 并追加 `log.md`。
- 持久化 citations、links、task events。

### Query 契约
- 用 `index.md` + FTS 召回。
- 返回带引用证据的回答。
- 可选保存到 Wiki。
- 将行为写入任务日志。

### Lint 契约
- 检测矛盾陈述。
- 检测孤儿页面。
- 检测过期结论。
- 检测缺失关键实体页。
- 生成补丁建议并在明确审批后应用。

## 7) 写作与文件规则
- 小改动优先增量编辑，不整页重写。
- 保持稳定标题与 wiki links。
- 除非明确迁移任务，不覆盖用户原有内容。
- 写入前必须做按页加锁与 checksum 校验。

## 8) 任务与状态
所有后台操作都必须建模为任务，状态明确：
- queued
- running
- reviewing
- applied
- failed

每个任务必须保留事件日志和错误上下文，支持恢复。

## 9) 完成定义（DoD）
功能仅在以下条件全部满足时才算完成：
- 行为符合 `docs/v1-technical-design.md`。
- 已执行相关验证命令并检查结果。
- 无模式策略回归（尤其严格本地模式）。
- `index/log` 一致性保持。
- 对用户输出包含变更说明与残余风险。

## 10) 编码前调研闸门（必需）
开始主要开发前必须执行：
1. 搜索适合本项目的 MCP servers/tools。
2. 搜索能提升交付效率和可靠性的 skills/workflows。
3. 给出候选清单、理由与取舍。
4. 用户确认后再安装。
5. 记录最终选型及原因。

## 11) 中文注释与实施记录（新增硬性要求）
- 写代码时，新增/修改代码的注释使用中文。
- 注释保持必要、简洁，不写无信息注释。
- 实施过程中必须记录关键决策、变更与验证结果。
- 实施记录统一写入：`docs/实施过程记录.md`。

## 12) LLM Wiki 核心理念（参考 Karpathy gist）

- Wiki 是**持久复利的工件**，LLM 负责整理，人类负责策划。
- **Ingest 增量更新**：不仅创建摘要，还需更新 5-15 个相关页面（添加引用、标注矛盾）。
- **交叉引用**：自动提取实体，建立 `[[wiki-link]]` 双向链接，记录 citations。
- **语义 Lint**：LLM 驱动的矛盾检测、陈旧检测、覆盖度检测。
- **Provider 优先级**：P0 Ollama → P1 Cloud API → P2 路由策略。

## 13) v1 范围外
- 多人协作与在线同步后端。
- 移动端客户端。
- 重型插件市场。
- 无审阅路径的全自动改写。

## 14) Subagents 并行实施规则（硬性要求）
- 整个项目实施过程必须采用 subagents 并行开发模式推进实现。
- 任何一轮实际编码前，主控必须先完成任务拆分，并至少把后端与前端拆成独立子任务；需要时再继续细分。
- 若遇到必须串行处理的临界步骤，主控需在实施记录中说明原因、持续时段与影响范围。
- 主控负责：任务拆分、接口契约、冲突调解、合并验收、风险收敛。
- 子代理负责：在分配的目录/文件所有权范围内实现，不跨范围改动。
- 并行时禁止多个子代理写同一文件；若冲突，先由主控重分配后再改。
- 每轮结束必须输出：改动文件、验证结果、未完成项，并写入实施记录。
- 进入下一轮前必须经过用户确认。
- 若当前运行环境缺少子代理执行能力，主控不得假装并行执行，必须在实施记录中注明并以最接近的可用方式继续，但不能删除该硬性约束。

## 15) 测试与手动验证要求
- 使用当前项目技术栈补充对应单元测试：
  - 前端（TypeScript/React）：为新增业务逻辑补充最小单元测试。
  - 后端（Rust）：为核心状态与命令逻辑补充最小单元测试。
- 每轮功能开发完成后，必须提供一组可执行的手动验证步骤。
- 手动验证步骤默认提供 Windows PowerShell 命令，不使用 cmd 命令风格。
- 在用户手动验证前，不将功能标记为"最终完成"；需要等待用户回传验证结果。
- 每轮记录中必须包含：自动化测试结果 + 用户侧 PowerShell 验证命令。

## 16) 多 Agent 交接规范（Claude Code / Codex / Gemini 三方协作）

### 16.1 代码风格一致性
- Rust: 遵循 `cargo fmt` 和 `cargo clippy` 默认规则。
- TypeScript: 遵循项目 ESLint/Prettier 配置。
- 命名：snake_case (Rust) / camelCase (TS)。

### 16.2 文件所有权
- 每个 Agent 在一轮中只能修改分配给它的文件。
- 冲突时由主控 Agent 协调，不允许自行合并。

### 16.3 交接检查清单
每次 Agent 切换前，必须确保：
1. `cargo check` 和 `npm run build` 无错误。
2. 所有测试通过：`cargo test` 和 `npm test`。
3. 变更已记录在 `docs/实施过程记录.md`。
4. 未完成项明确列出，带 TODO 标记。

### 16.4 上下文传递
- 使用 `agents.md` 作为规范来源（含 §18 当前状态）。
- 使用 `docs/实施过程记录.md` 作为进度来源（最新条目在最前）。
- **新 Agent 启动时必须先读取这两个文件，然后执行 §16.3 检查清单验证基线。**
- 基线验证命令（WSL/Linux）：
  ```bash
  cd src-tauri && cargo check && cargo test
  cd ../web && npm run typecheck && npm run test -- --run
  ```
- 基线验证命令（Windows PowerShell）：
  ```powershell
  cd src-tauri; cargo check; cargo test
  cd ../web; npm run typecheck; npm run test -- --run
  ```

### 16.5 本机私有配置提交禁令
- 本机私有配置与授权信息不得提交到仓库，包括但不限于 `.claude/`、`.codex/` 以及本机授权、凭证类文件。

## 17) Round 2 实施优先级建议

### P0 - 核心 LLM 集成（必须先完成）
1. **Provider 抽象层**：定义统一的 `LlmProvider` trait。
2. **Ollama 集成**：实现本地模型调用（llama3, mistral 等）。
3. **Ingest LLM 调用**：用 LLM 生成真正的摘要（替换当前 truncate）。

### P1 - Wiki 复利机制
4. **实体提取**：Ingest 时 LLM 识别关键实体。
5. **相关页面更新**：Ingest 后 LLM 扫描并更新相关 Wiki 页面。
6. **双向链接**：自动建立 `[[wiki-link]]` 并更新 citations。

### P2 - 智能 Query 与 Lint
7. **Query LLM 调用**：用 FTS 召回 + LLM 合成答案。
8. **语义 Lint**：LLM 检测矛盾、陈旧、覆盖度问题。

### P3 - 用户体验
9. **进度指示**：长时间 LLM 调用的流式反馈。
10. **Cloud Provider**：OpenAI-compatible 云端 API 集成（Hybrid 模式，含 DeepSeek / GLM / MiniMax 预设）。

---

## 18) 当前开发状态（Agent 交接必读）

> 本节由每轮结束的主控 Agent 维护，是下一轮开发的起点。

### 18.1 已完成（截至 2026-04-17）

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
| P5-A | Lint 分组折叠（按路径） + Query 保存后自动跳转 Wiki | ✅ `web/src/App.tsx` + `styles.css` + `app-utils.test.ts` |
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
| P20-1 后端 | Outbox 事件流基础（`wiki_outbox` + 导出/ack 命令 + 关键路径事件写入） | ✅ `db.rs` + `models.rs` + `state.rs` + `commands.rs` + `main.rs`（Rust 待 Windows cargo 复核，2026-04-17，**Codex** 实施） |
| P20-2 后端 | Wiki-link 级 lint（`broken_wikilink` / `orphan` / `xref_missing`）+ patch preview/apply 最小可用 | ✅ `state.rs`（新增 2 条 Rust 单测；待 Windows cargo 复核，2026-04-17，**Codex** 实施） |

### 18.2 下一轮待开发（TODO）

**P19：遗留 UX 增强（可并行于 P20）**
- ~~P19-1 Ask 历史管理增强（时间显示/清空入口/按关键词过滤）~~ ✅ 已完成（Codex 2026-04-17）
- P19-2 Wiki 文件树增强（按目录批量折叠/展开、当前页自动定位）
- P19-3 标签维度增强（多标签交集筛选 + 标签计数）

**P20：功能对标 external/llm-wiki-main（优先级提升）**
- 对标参考（功能层，不绑定实现）：
  - `E:\llm-wiki\external\article.txt`
  - `E:\llm-wiki\external\llm-wiki-main`
  - WSL 路径：`/mnt/e/llm-wiki/external/article.txt`、`/mnt/e/llm-wiki/external/llm-wiki-main`
- P20-0 调研闸门（§10 必做）：先提交 MCP/skills/workflow 候选清单、理由与取舍，待用户确认后安装/启用。
- ~~P20-1 事件流能力：新增可消费 outbox（offset 导出 + ack），覆盖 ingest/query/lint/页面变更关键事件。~~ ✅ 已完成后端基础（Codex 2026-04-17，待 Windows cargo 复核）
- ~~P20-2 Wiki 语义健康：新增 wiki-link 级 lint（`broken_wikilink` / `orphan` / `xref_missing`）与补丁建议。~~ ✅ 后端已完成（Codex 2026-04-17，待 Windows cargo 复核）
- ~~P20-3 Query 融合召回：FTS5 BM25 + 链接扩展 + Citation 热度，RRF 纯函数融合。~~ ✅ 完成（Claude Code 2026-04-17，99 Rust / 118 前端）
- ~~P20-4a 页面 stale 标记：frontmatter `stale` 字段 + lint `STALE_PAGE` 规则 + wiki 详情页横幅 + 标记/取消按钮。~~ ✅ 完成（Claude Code 2026-04-17，101 Rust / 118 前端）
- ~~P20-4b 完整 Claim 模型~~ 暂缓，等知识积累后再评估。

~~**P21：知识图谱可视化**~~ ✅ 完成（Claude Code 2026-04-17，102 Rust / 118 前端）
- `react-force-graph-2d`（Canvas，lazy-loaded，Tauri WebView2 兼容）
- 后端：`get_knowledge_graph()` 命令，nodes = wiki_pages，links = citations（去重）
- 前端：新”图谱”模块，`ForceGraph2D` + `groupColor()` + 节点点击跳转详情 + resize 响应

**Claude Code / Gemini 入手点（下轮开始先做）**
1. 基线已复核（2026-04-17 P21）：`cargo test` 102/102，`npm test` 118/118，`typecheck` 零错误，`build` 通过。
2. P20/P21 全部完成。下一步：P19-2（文件树增强）或 P19-3（多标签筛选），或用户指定新功能。
3. 每轮必须按 §14 并行规则拆分子任务（至少前端/后端两条），并在记录中写明文件所有权。
4. 每轮结束必须更新 `docs/实施过程记录.md` 与 `agents.md §18`；三方交接仅以这两处为准。
5. 对标结论强调”功能优先、实现第二”：允许本项目使用不同工程路径，只要满足 A+C 架构与 Strict Local 约束。

### 18.3 当前代码快照

```
src-tauri/src/
  llm/
    mod.rs          # pub use provider + ollama + openai
    provider.rs     # LlmProvider trait, LlmError
    ollama.rs       # OllamaProvider (health_check, complete, summarize)
    openai.rs       # OpenAiProvider (OpenAI-compatible Chat Completions API, Hybrid 模式)
  search.rs         # reciprocal_rank_fusion() 纯函数（P20-3，3 条单测）
  commands.rs       # 所有 Tauri 命令（含 mark_page_stale, get_knowledge_graph）
  db.rs             # SQLite 操作（含 query_linked_page_paths, query_citation_popular_paths, list_all_wiki_pages）
  main.rs           # Tauri app 入口
  models.rs         # 全部数据模型（含 WikiPageFrontmatter.stale, KnowledgeGraphNode/Link/Data）
  state.rs          # AppState 核心逻辑（含 search_wiki_matches_rrf, set_page_stale, get_knowledge_graph_impl）
  vault.rs          # 文件系统操作

web/src/
  App.tsx           # 主界面（含图谱模块、stale横幅/按钮、RRF策略标签）
  tauri-client.ts   # Tauri invoke 封装（含 markPageStale, getKnowledgeGraph）
  types.ts          # TS 类型定义（含 LlmProviderConfig）
  app-utils.test.ts # 前端单元测试（118 个）
```

### 18.4 验证基线

- `cargo check`（src-tauri/）：**通过（2026-04-17 P18-3）**；P19-1 后端新增命令待 Windows 复核
- `cargo test`（src-tauri/）：**91 通过，0 失败（2026-04-17 P18-3）**；P19-1 后端新增测试待 Windows 复核
- `cargo check`（src-tauri/，2026-04-17 P20-1 热修复复核）：用户在 Windows 报告 `E0063`（`ollama_model/ollama_base_url` 缺失）后已修复代码，**待用户再次复核**
- `cargo test`（src-tauri/）：**102 通过，0 失败（2026-04-17 P21）**
- `npm run test -- --run`（web/）：**118 通过，0 失败（2026-04-17 P21）**
- `npm run typecheck`（web/）：**零错误（2026-04-17 P21）**
- `npm run build`（web/）：**通过（2026-04-17 P21，含 react-force-graph-2d 懒加载 chunk 62KB gzipped）**

### 18.5 关键约束提醒

- **LLM 调用全部在异步上下文**，不使用 `block_in_place`（已清理）。
- **Tauri 异步命令带引用参数必须返回 `Result<T, String>`**（见 `run_lint`, `get_llm_status`）。
- **`lint_report_full_future` / `llm_status_future` 模式**：先同步提取数据，`drop(state)` 后再 await。
- **文件写入幂等**：`append_see_also_link` 先检查链接是否已存在再写入。
- **API Key 禁止入仓**：`.claude/`、`.codex/`、`.env` 均在 §16.5 禁止提交。

### 18.6 多 Agent 运行环境说明（交接必读）

- **Codex（本轮）运行环境**：WSL/Linux（`/mnt/e/llm-wiki`），当前会话 `cargo` 不可用（`cargo: command not found`），因此 Rust 编译/测试不能在本端确认。
- **Claude Code / Gemini 运行环境**：Windows（PowerShell 原生），可执行完整 Rust/前端验证链路。
- **交接规则**：
  1. Codex 提交 Rust 相关改动时，必须在记录中明确标注“待 Windows cargo 复核”。
  2. Claude Code 或 Gemini 接手后，先在 Windows 执行：`cargo check`、`cargo test`、`npm run typecheck`、`npm run test -- --run`、`npm run build`。
  3. 仅当 Windows 验证回传通过后，相关轮次可标记为最终收口完成。
