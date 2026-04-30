# 开工前 MCP 与 Skills 调研（A+C 路线）

## 1. 调研目标
- 在正式编码前，筛选适合本项目（Windows 本地优先 + 严格隐私模式）的 MCP 与 Skills。
- 形成“可立即安装”和“暂缓安装”清单。
- 约束：优先官方或高可信来源；避免引入不必要的运行时耦合。

## 2. 结论总览
- 结论一：当前 Codex 环境内置技能已覆盖 v1 开发主链路，Skills 无需额外安装即可开工。
- 结论二：MCP 建议分为“开发期使用”和“产品运行期集成”两层；v1 先做最小集。
- 结论三：Obsidian 相关 MCP 不建议作为 v1 必选依赖，保持“兼容而不绑定”。

### 2026-04-17（P20 对标轮）补充结论
- 对标参考：`external/article.txt` + `external/llm-wiki-main`（功能对标，非实现绑定）。
- 本轮选型：**不新增 MCP/Skills 安装**，继续使用已启用的 `filesystem/git/time` 与现有技能链路。
- 取舍理由：
  - `P20-1`（outbox 事件流）主要是本地 Rust/SQLite 改造，现有工具足够。
  - 避免在功能实现轮引入新工具变量，降低环境噪声与回归成本。
- 预留候选（后续按需启用）：
  - SQLite 专项 MCP（用于复杂 schema/查询调试）
  - Fetch MCP（用于 URL ingest 安全策略验证）

## 3. 推荐清单（按优先级）

### P0（建议先启用/确认）
1. Filesystem MCP（官方 reference）
- 用途：安全受控的文件读写与目录操作，适配 markdown vault 工作流。
- 价值：直接对应 ingest/query/lint 对文件层的操作需求。
- 备注：官方 `modelcontextprotocol/servers` 提供 reference 实现，生产化需自行加固。

2. Git MCP（官方 reference）
- 用途：变更追踪、回滚辅助、审计配合。
- 价值：与“可回归记录 + 变更审计”目标一致。
- 备注：同上，reference 方案需安全评估后用于生产。

3. Time MCP（官方 reference）
- 用途：统一时间戳与时区处理（日志、任务、审计）。
- 价值：低成本提升一致性。

### P1（可选，按阶段启用）
4. Memory MCP（官方 reference）
- 用途：额外记忆/关系图支持。
- 价值：对长期知识组织有帮助，但 v1 可先用 SQLite+Markdown 实现。

5. Fetch MCP（官方 reference）
- 用途：网页抓取与转换，支持 url ingest。
- 价值：有用，但要配合安全策略（域名白名单、下载大小限制）。

### P2（暂缓）
6. Obsidian 第三方 MCP（多个社区实现）
- 用途：直连 Obsidian 或其 Local REST API。
- 暂缓原因：
  - 实现分散，质量与维护差异较大。
  - 会引入 Obsidian 插件依赖，与“应用独立运行”目标冲突。
  - v1 已通过“共享 Vault 目录”满足兼容诉求，无需强绑定。

## 4. Skills 评估

### 当前已满足（无需新增安装）
- `systematic-debugging`：排障规范
- `verification-before-completion`：完成前验证
- `playwright` / `playwright-interactive`：桌面 UI 流程验证
- `pdf` / `doc` / `spreadsheet`：资料摄入扩展处理
- `skill-installer`：后续按需增装能力

### 建议策略
- v1 阶段不新增 Skills 安装，先用现有技能链路。
- 若后续出现明确短板，再通过 `skill-installer` 定向安装，不做“先装一堆”。

## 5. 安装建议（待你确认后执行）

### 安装批次 A（最小可行）
- MCP：Filesystem、Git、Time
- Skills：不新增

### 安装批次 B（增强）
- MCP：Memory、Fetch（在 A 稳定后）
- Skills：按缺口增补

## 6. 风险与控制
- 风险：官方 servers 仓库明确标注 reference 属性，不等同生产级默认安全。
- 控制：
  - 最小权限（目录白名单、只读优先）
  - 明确工具超时与资源上限
  - 所有写操作保留任务与日志审计

## 7. 建议的执行顺序
1. 你确认“安装批次 A”。
2. 我执行安装与配置，并写入实施记录。
3. 跑一轮连通性验证（文件读写、git 查询、时间工具）。
4. 你确认后再进入正式编码。

## 8. 主要来源
- MCP 官方文档：`https://modelcontextprotocol.io/docs/getting-started/intro`
- MCP 官方 servers 仓库：`https://github.com/modelcontextprotocol/servers`
- 官方 servers 发布记录：`https://github.com/modelcontextprotocol/servers/releases`
- OpenAI Skills 仓库：`https://github.com/openai/skills`
- Ollama Windows 文档：`https://docs.ollama.com/windows`
- Ollama API 文档：`https://docs.ollama.com/api`
-（补充参考）多项 Obsidian 社区 MCP 仓库（用于“暂缓”判断）

## 9. 已执行安装结果（2026-04-08）
- 已安装并注册（批次 A）：
  - `filesystem`
  - `git`
  - `time`
- 本机注册命令（双栈对照；以当前执行器为准）
  - 当前执行器：Codex
    - `filesystem` -> `/mnt/e/llm-wiki/.tools/mcp-node/node_modules/.bin/mcp-server-filesystem /mnt/e/llm-wiki /tmp`
    - `git` -> `/mnt/e/llm-wiki/.tools/mcp-venv/bin/mcp-server-git --repository /mnt/e/llm-wiki`
    - `time` -> `/mnt/e/llm-wiki/.tools/mcp-venv/bin/mcp-server-time`
    - 验证命令：`codex mcp list`、`codex mcp get <name>`
  - Claude Code 对照口径：
    - `filesystem` -> `<path-to-mcp-server-filesystem> /mnt/e/llm-wiki /tmp`
    - `git` -> `<path-to-mcp-server-git> --repository /mnt/e/llm-wiki`
    - `time` -> `<path-to-mcp-server-time>`
    - 验证命令：`claude mcp list`、`claude mcp get <name>`
- 验证结果：
  - 当前执行器下的 `codex mcp list` 与 `codex mcp get <name>` 均显示 enabled。
  - 三个服务进程均可启动并保持运行（3 秒超时退出码 `124`）。
- 备注：
  - 上述 Claude Code 命令为兼容性对照示例，实际执行口径以当前执行器为准，不绑定单一工具。
  - WSL 下通过 Windows 代理 `172.25.64.1:7897` 访问外网安装。
  - `mcp-server-git/time` 来源为 PyPI；`filesystem` 来源为 npm。
