# v1 技术设计（A+C）：本地优先 + 隐私模式

## 1. 范围
- 构建一个 Windows 桌面个人 Wiki 应用。
- 遵循 Karpathy 风格工作流：ingest、query、lint。
- 以 Markdown Vault 作为知识内容事实来源。
- 支持云/本地混合推理，以及严格本地隐私模式。

## 2. 产品形态（Windows）
- 打包形态：Tauri MSI/EXE 安装包。
- 运行形态：单机桌面应用，不依赖后端服务。
- 数据驻留：Wiki 数据默认存储在用户本地可选 Vault 目录。
- 网络访问：仅云 Provider 可选使用；严格本地模式下禁用。

## 3. 技术栈
- 桌面壳：Tauri（Rust）
- 前端：React + TypeScript + Vite
- 存储：SQLite + FTS5
- 内容格式：Markdown + YAML frontmatter
- 本地模型运行时：Ollama
- 可选云模型：OpenAI/Claude（通过 Provider 抽象层）

## 4. 数据布局
- `vault/raw/`：导入的原始资料（追加/只读策略）
- `vault/wiki/`：维护后的 Wiki 页面
- `vault/index.md`：全局结构化索引
- `vault/log.md`：追加式时间日志
- `vault/.app/meta.db`：内部元数据库
- `vault/.app/config.toml`：应用配置

## 5. 核心工作流

### 5.1 Ingest
1. 导入资料（`md`、`pdf`、`url`）。
2. 文本标准化并计算哈希用于去重。
3. 生成页面更新计划（创建/更新哪些 wiki 页）。
4. 以增量方式应用 Markdown 编辑。
5. 更新 `index.md` 并追加 `log.md`。
6. 持久化 citations、links 与 task events。

### 5.2 Query
1. 通过 `index.md` + SQLite FTS 召回候选页面。
2. 基于证据和引用生成回答。
3. 可选将回答保存为新页面或更新页面。
4. 将写入行为记录到任务日志。

### 5.3 Lint
1. 扫描矛盾陈述与过期结论。
2. 检测孤儿页面与缺失关键实体页。
3. 生成 lint 报告与建议补丁集。
4. 仅在用户批准后应用补丁。

## 6. 运行模式

### Hybrid Mode（默认）
- 云与本地 Provider 均可用。
- 支持按任务路由。
- 敏感资料可标记为 `local_only`。

### Strict Local Mode
- 仅允许本地 Provider（Ollama）。
- 云调用由策略守卫拦截。
- 禁用遥测与外部模型 API 请求。

## 7. Provider 抽象
接口约定：
- `summarize(source)`
- `plan_edits(context)`
- `generate_answer(question, context)`
- `validate_claims(claims, evidence)`

路由规则：
- 先读取运行模式与 source 标记。
- 严格本地模式下禁止路由到云 Provider。
- 在任务事件中保存模型与提示词元信息。

## 8. SQLite 模式（v1）
- `sources(id, path, kind, hash, status, ingested_at)`
- `wiki_pages(id, path, title, type, checksum, updated_at)`
- `page_links(id, from_page_id, to_page_id, link_text)`
- `citations(id, page_id, source_id, locator, quote_hash)`
- `tasks(id, kind, status, payload_json, error, created_at, updated_at)`
- `task_events(id, task_id, event_type, message, created_at)`
- `fts_pages`（Wiki 页面全文索引，FTS5）

## 9. 一致性与冲突控制
- 内部写入采用按页写锁。
- 写入前进行 checksum 校验。
- 检测到外部修改时进入合并流程，不允许静默覆盖。
- 支持基于任务级补丁快照回滚。

## 10. 安全与隐私
- 本地文件是默认事实来源。
- API Key 存储到操作系统安全存储。
- 所有 AI 生成写入必须带引用追溯。
- 删除流程必须在日志中保留审计轨迹。

## 11. v1 验收标准
- Ingest 能更新多个 Wiki 页面并附带引用。
- Query 返回带引用证据的答案，且可保存到 Wiki。
- Lint 至少覆盖四类问题检测。
- Strict Local 模式下无云模型调用。
- 与 Obsidian 共用 Vault 时不发生静默覆盖。

## 12. 里程碑
1. 基建：Vault 初始化、DB、任务状态机、设置。
2. Ingest v1：先 md，再补 pdf/url 基础抽取。
3. Query v1：FTS 检索、证据回答、保存到 Wiki。
4. Lint v1：报告 + 用户审批后补丁应用。
5. Privacy v1：Ollama 接入 + 严格本地模式策略落地。

## 13. 开发与记录约定（新增）
- 写代码时，注释使用中文，且注释只写必要信息。
- 关键实施过程必须写入实施记录文件，便于回归与审计。
- 实施记录文件位置：`docs/实施过程记录.md`。
- 项目实施默认采用 subagents 并行模式；若临时串行处理，需在实施记录说明原因。

## 14. 测试与验收约定（新增）
- 新增功能必须补充对应语言的最小单元测试：
  - 前端（TypeScript/React）逻辑测试
  - 后端（Rust）状态与命令逻辑测试
- 每轮实现完成后，必须输出可复现的手动验证流程。
- 手动验证命令默认使用 Windows PowerShell 形式。
- 最终验收以“自动化测试通过 + 用户手动验证通过”为准。
