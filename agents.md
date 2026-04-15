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

## 16) 多 Agent 交接规范（Claude Code / Codex 兼容）

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

### 18.1 已完成（截至 2026-04-15）

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

### 18.2 下一轮待开发（TODO）

**P9：多格式摄入扩展设计与实现（Word/PPT/图片）**
- **前端子任务**：摄入入口升级为“按文件类型路由”的统一表单（md/pdf/docx/pptx/img）。
- **后端子任务**：新增多格式提取适配层（文档解析 + OCR + 错误分层），统一复用 ingest 主流程。
- **测试任务**：为每类格式补最小可重复回归样例和失败场景用例。

### 18.3 当前代码快照

```
src-tauri/src/
  llm/
    mod.rs          # pub use provider + ollama + openai
    provider.rs     # LlmProvider trait, LlmError
    ollama.rs       # OllamaProvider (health_check, complete, summarize)
    openai.rs       # OpenAiProvider (OpenAI-compatible Chat Completions API, Hybrid 模式)
  commands.rs       # 所有 Tauri 命令（含 get_llm_config/set_llm_config、ingest_pdf、save_wiki_page）
  db.rs             # SQLite 操作
  main.rs           # Tauri app 入口（含 setup hook 注入 AppHandle）
  models.rs         # 全部数据模型（AppConfig 含 cloud_* 字段并兼容 openai_* 旧字段，LlmProviderConfig）
  state.rs          # AppState 核心逻辑（含 provider 路由、get_llm_config/set_llm_config）
  vault.rs          # 文件系统操作（含 append_see_also_link）

web/src/
  App.tsx           # 主界面（含 Settings 面板：cloud API Key + Provider/Model 配置与 DeepSeek/GLM/MiniMax 预设）
  tauri-client.ts   # Tauri invoke 封装（含 fetchLlmConfig/saveLlmConfig/saveWikiPage/ingestPdf）
  types.ts          # TS 类型定义（含 LlmProviderConfig）
  app-utils.test.ts # 前端单元测试（90 个）
```

### 18.4 验证基线

- `cargo check`（src-tauri/）：**通过（2026-04-15，2 个 dead_code 警告）**
- `cargo test`（src-tauri/）：**75 通过，0 失败（2026-04-15）**
- `npm run test -- --run`（web/）：**90 通过，0 失败（2026-04-15）**
- `npm run typecheck`（web/）：**零错误（2026-04-15）**
- `npm run build`（web/）：**通过（2026-04-15）**

### 18.5 关键约束提醒

- **LLM 调用全部在异步上下文**，不使用 `block_in_place`（已清理）。
- **Tauri 异步命令带引用参数必须返回 `Result<T, String>`**（见 `run_lint`, `get_llm_status`）。
- **`lint_report_full_future` / `llm_status_future` 模式**：先同步提取数据，`drop(state)` 后再 await。
- **文件写入幂等**：`append_see_also_link` 先检查链接是否已存在再写入。
- **API Key 禁止入仓**：`.claude/`、`.codex/`、`.env` 均在 §16.5 禁止提交。
