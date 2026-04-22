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
- 使用 `docs/参考项目差距与移植清单.md` 作为对标差距与迭代方向来源。
- 使用 `docs/多Agent通信与交接协议.md` 作为通信与交接流程来源。
- 使用 `docs/交接状态卡.md` 作为“当前轮次与接力状态”的快速事实来源。
- **新 Agent 启动时必须先读取这五个文件（`agents.md`、`docs/实施过程记录.md`、`docs/参考项目差距与移植清单.md`、`docs/多Agent通信与交接协议.md`、`docs/交接状态卡.md`），然后执行 §16.3 检查清单验证基线。**
- **Claude Code / Codex / Gemini 三方每轮开始前必须重读 `docs/参考项目差距与移植清单.md`，确保对齐最新差距与移植优先级。**
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

> 本节由每轮结束的主控 Agent 维护。新 Agent 启动时先读本节，再读 `docs/实施过程记录.md` 最新条目。

### 18.0 快速恢复步骤

1. `cargo test`（src-tauri/）→ 应 **142 passed**
2. `npm run test -- --run`（web/）→ 应 **151 passed**
3. `npm run typecheck`（web/）→ 应 **0 errors**
4. 读 `docs/交接状态卡.md` 与 `docs/多Agent通信与交接协议.md`，确认当前接力状态与交接格式
5. 读 `docs/实施过程记录.md` 最新 3 条了解背景
6. 读 `docs/参考项目差距与移植清单.md`，确认本轮选择的移植包（A/B/C/D）与理由
7. 按 §18.3 TODO 开始下一轮，**必须使用 §14 子代理并行规则**

### 18.1 已完成（截至 2026-04-19，持续更新）

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
| P21-C2 前端 | 搜索高亮（光晕视觉反馈） + 平滑相机聚焦 + 动态侧边栏搜索结果 | ✅ `web/src/App.tsx` + `web/src/styles.css`（130 前端测试通过，2026-04-17，**Gemini** 实施） |
| P21-D 前端 | 图谱收口（Outbox 自动刷新、可见范围搜索、稳定渲染 key、Ctrl+F、导出 JSON、>200 节点聚合） | ✅ `web/src/App.tsx` + `web/src/app-utils.test.ts`（132 前端测试通过，2026-04-19，**Codex** 实施） |
| P21-E 前端 | 图谱聚合交互深化（聚合节点右侧“展开查看成员页” + 一键切回明细模式） | ✅ `web/src/App.tsx` + `web/src/styles.css`（WSL `typecheck` 通过；完整测试待下一轮，2026-04-20，**Codex** 实施） |
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
| BugFix-Research-Logs | Deep Research 失败“无日志可看” + 报告生成瞬时失败易中断 → 报告阶段自动重试一次 + 失败态任务卡展示日志流 | ✅ `src-tauri/src/state.rs` + `web/src/App.tsx`（WSL `web typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| BugFix-Research-WordExport | Deep Research “导出 Word”按钮在 Tauri 下无响应 → 改为保存对话框选路径 + 后端写盘，浏览器模式保留 Blob 回退 | ✅ `src-tauri/src/commands.rs` + `src-tauri/src/main.rs` + `web/src/tauri-client.ts` + `web/src/App.tsx`（WSL `web typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| BugFix-Research-TaskDelete | Deep Research 任务删除能力（终态任务删除 + 可选同步删除关联 Wiki + 运行中任务禁止直删） | ✅ `src-tauri/src/db.rs` + `src-tauri/src/state.rs` + `src-tauri/src/commands.rs` + `src-tauri/src/main.rs` + `web/src/App.tsx` + `web/src/tauri-client.ts`（WSL `web typecheck` 通过；Rust 待 Windows cargo 复核，2026-04-21，**Codex** 实施） |
| BugFix-Research-DeleteConfirm | 任务删除确认链路稳定化（删除参数兼容 + 时间显示统一 + Tauri 原生 confirm 二次确认，取消后保留任务） | ✅ `src-tauri/src/commands.rs` + `src-tauri/capabilities/default.json` + `web/src/App.tsx` + `web/src/tauri-client.ts`（用户手测通过，2026-04-21，**Codex** 实施） |
| P26-ResearchDialog | Deep Research 对话框体验收口（状态同步轮询、事件重复订阅修复、Footer 操作统一、UI 配色修复、历史任务状态与重试） | ✅ `web/src/App.tsx`（2026-04-22，**Claude Code** 实施） |
| P26-ResearchExportMD | Research 报告导出 `.md`（按钮 + 本地保存）与历史任务兜底（`doneSavedPath` 读取 Wiki 内容） | ✅ `web/src/App.tsx` + `web/src/tauri-client.ts`（WSL `web typecheck` 通过，2026-04-22，**Claude Code/Codex** 接力） |
| P26-Graph-UX | 图谱双击节点跳转 Wiki（400ms 双击窗口） | ✅ `web/src/App.tsx`（2026-04-22，**Claude Code** 实施） |
| P26-Embed-HealthHint | Embed 健康检查提示优化（ModelNotFound 指引 `ollama pull nomic-embed-text:latest`） | ✅ `src-tauri/src/state.rs`（2026-04-22，**Claude Code** 实施） |

### 18.2 下一轮 TODO（按优先级）

| 优先级 | 任务 | 说明 |
|---|---|---|
| ✅ | **P22 Windows 打包** | MSI + EXE 双产物生成，安装验证通过（2026-04-20） |
| ✅ | **Windows 基线复核** | cargo test 113/0，npm test 138/0，typecheck 零错误（2026-04-20） |
| ✅ | **P23 移植包 A：持久化 ingest 队列** | ingest_queue_items 表 + 状态机 + worker + 队列面板 UI，基线 116 Rust / 142 前端（2026-04-20） |
| ✅ | **移植包 B：图谱洞察层（阶段一）** | 孤立页/桥接节点/稀疏社区洞察卡片 + 图谱联动（2026-04-20） |
| ✅ | **移植包 B：图谱洞察层（阶段二）** | 异常连接洞察 + 阈值参数化 + 洞察来源可解释信息（2026-04-20） |
| ✅ | **移植包 B：图谱洞察层（阶段三）** | 异常连接降噪（复合置信度 + 门槛阈值）与 UI 持久化调参（2026-04-20） |
| ✅ | **拖拽摄入（App 内）** | llm-wiki 应用窗口支持拖拽文件触发 ingest_file（图片/PDF/md）（2026-04-20） |
| ✅ | **移植包 B：图谱洞察层（阶段四）** | 前端 embedding 相似度接入异常连接洞察（可选 `embeddingSim` param），证据显示语义相似度；`getPageEmbeddingPairs` tauri-client 封装（2026-04-20） |
| ✅ | **拖拽模式切换（直接/队列）** | `DROP_MODE_STORAGE_KEY` + Settings `<select>` + drop 时按模式路由（2026-04-20） |
| ✅ | **移植包 B 阶段四后端** | `get_page_embedding_similarities` Tauri 命令：拉取 DB embedding → 余弦相似度 → HashMap key=`pathA\|\|pathB`，MIN_SIM=0.25，MAX_PAIRS=1000，2 新 Rust 测试（118/149，2026-04-20） |
| ✅ | **移植包 D：Deep Research 全栈** | Tavily 搜索 + LLM 自动分解子查询(breadth) + 可选深度 Phase C + 持久化 research_tasks + ResearchPanel UI + SearchConfigPanel（118 Rust / 151 前端，2026-04-20，**Claude Code 并行子代理**） |
| ✅ | **Deep Research 代码质量 A 级收口** | report_research_failure 具名化 + parse_learnings 多格式容忍 + 23 个单元测试（141 Rust，2026-04-21） |
| ✅ | **ingest 队列重启恢复修复** | init_vault 后重置遗留 running 任务 + 路径安全检查 + decompose fallback 日志（142 Rust，2026-04-21） |
| ✅ | **Clipper Windows 端到端复核** | 用户已回传 `scripts/verify_clipper_windows.ps1` 通过，并确认扩展写入与 Wiki 可见（2026-04-22） |
| ✅ | **移植包 D：SearXNG 本地搜索（后端激活增强）** | 已补齐 URL/端点容错与错误可见化；待 Windows + Docker 端到端复核（2026-04-21） |
| ✅ | **Deep Research 导出能力复核** | 已验证 “导出 Word” 在 Tauri 下弹保存对话框并成功落盘（2026-04-21） |
| ✅ | **Deep Research 失败态体验复核** | 失败任务卡日志展示与“自动重试一次”提示已可见（2026-04-21） |
| ✅ | **Deep Research 任务删除耦合确认复核** | 已验证删除任务二次确认与“是否同步删除 Wiki”确认链路（2026-04-21） |
| 1 | **SearXNG Windows 端到端复核** | 执行 `scripts/verify_searxng_windows.ps1` + 应用内 Deep Research 实跑验证 |
| 2 | **移植包 D：项目模板 / Clipper / SearXNG 端到端验证** | Gemini/Codex 已实现，需用户端到端验证 |
| 3 | **Research `.md` 导出历史任务回归复核** | 验证无流式内容的历史 done 任务仍可见“导出 .md”并成功导出 |

**开发规则（每轮必读）：**
- **§14 强制**：后端/前端/测试各用独立子代理并行开发，主控不直接写代码
- 每轮结束更新本节 + `docs/实施过程记录.md`，验证基线全绿后 git commit

### 18.3 当前代码快照（2026-04-22）

```
src-tauri/src/
  llm/
    provider.rs     # LlmProvider trait（含 embed/complete_stream/health_check）
    ollama.rs       # OllamaProvider（Ollama /api/generate + /api/embeddings）
    openai.rs       # OpenAiProvider（OpenAI-compatible Chat Completions + embeddings）
  search.rs         # RRF + embedding 余弦排序（`rank_embedding_paths_by_cosine`）
  commands.rs       # 全部 Tauri 命令注册（含 mark_page_stale, get_knowledge_graph）
  db.rs             # SQLite（含 upsert_embedding/list_embeddings, list_all_wiki_pages, query_citation_popular_paths）
  models.rs         # 全部数据模型（含 stale, KnowledgeGraph*, embed_ollama_model 字段）
  state.rs          # 核心逻辑（含 get_embed_provider, RRF+embedding 检索融合, search_debug 贡献明细, PDF OCR 自动回退）
  vault.rs          # 文件系统（hash去重, ingest_markdown, append_see_also_link）

web/src/
  App.tsx           # 主界面（Inbox/Wiki/Ask/Lint/图谱/Settings 模块全集成，含 ResearchDialog/导出.md/图谱双击跳转）
  tauri-client.ts   # invoke 封装（INGEST_TIMEOUT_MS=300s, getKnowledgeGraph, markPageStale）
  types.ts          # TS 类型（含 LlmProviderConfig.embed_ollama_model/base_url, QuerySearchDebug）
  app-utils.test.ts # 单元测试（含图谱洞察新增用例，数量待 Windows 复核）
  styles.css        # 样式（含 graph-module, graph-insights, wiki-stale-banner, lint 分组等）
```

### 18.4 验证基线（2026-04-22 最新）

- `cargo check` / `cargo test`：**待 Windows 复核**（本轮 Codex 在 WSL：`cargo: command not found`）
- `node --check extension/popup.js`：**通过**
- `npm run typecheck`：**通过（0 errors）**（2026-04-22，WSL）
- `npm run test -- --run`：**本轮未执行**（WSL 受 Rollup Linux 可选依赖缺失影响，待 Windows 复核）
- `npm run build`：**本轮未执行**（按当前轮次指令跳过打包相关验证）

最新提交（main 分支）：
- `feat: 完成 P1+P2 功能收口` (d1ab47c)
- `fix: 修复 ResearchDialog 事件重复订阅导致 queries/done 消息重复` (c4205fa)
- `fix: ResearchDialog 操作按钮收口为底部统一 Footer` (3362e57)
- `fix: 修复 ResearchDialog UI 配色与交互问题` (47048dd)

### 18.5 关键架构约束

- **LLM vs Embed 分离**：LLM（摘要/实体/对话）走 `get_llm_provider()`（云端优先）；Embed 走 `get_embed_provider()`（始终本地 Ollama，默认 `nomic-embed-text:latest`）
- **Ingest 超时**：前端 `INGEST_TIMEOUT_MS = 300_000`（5分钟）；LLM 输入截断 8000 字符
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`；`lint_report_full_future` 模式先 drop(state) 再 await
- **API Key 禁止入仓**：`.claude/`、`.codex/`、`.env` 均在 §16.5 禁止提交
- **Codex 在 WSL**：Rust 编译/测试无法在 WSL 确认，Rust 改动需标注”待 Windows cargo 复核”；Claude Code/Gemini 在 Windows 全链路验证
