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
- 使用 `agents.md` 作为规范来源。
- 使用 `docs/实施过程记录.md` 作为进度来源。
- 新 Agent 启动时必须先读取这两个文件。

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
10. **Cloud Provider**：OpenAI / Claude API 集成（Hybrid 模式）。
