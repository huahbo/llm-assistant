# LLM Wiki Desktop

> v0.2.7 · Windows 优先的个人 AI 知识库桌面应用

本地优先架构：Tauri v2 + React + TypeScript + SQLite + Markdown Vault，支持本地 AI（Ollama）与云端 OpenAI-compatible Provider，隐私友好。

---

## 功能概览

| 模块 | 说明 |
|------|------|
| **Chat（AI 对话）** | ReAct 多轮 Agent 对话；Markdown 渲染 + 代码高亮；8 个内置工具（含 web_search / fetch_url / spawn_subagent）；四级联网搜索；MCP 扩展工具支持；**Shell 三模式**（off/approval/yolo，会话级独立配置）；Shell 策略审批 + 票据缓存；文件上传附件（支持 txt/md/pdf/doc/docx/pptx/csv/json/代码）；流式输出；对话归档/重命名 |
| **Ingest** | 支持 Markdown / PDF / DOCX / PPTX / TXT / 图片 OCR，URL 抓取，拖拽摄入，持久化摄入队列（含重试/取消） |
| **Query / Ask** | FTS5 + embedding + 混合语义检索（四路 RRF）；流式对话；Ask 会话持久化管理 |
| **Wiki** | Markdown 编辑/渲染/重命名/删除，双向链接，内链补全，实体提取，Frontmatter 元数据 |
| **Lint** | 语义矛盾/陈旧/覆盖度检测，Wiki-link 级 broken/orphan 检测，可预览/应用修复补丁 |
| **Graph** | 知识图谱可视化（Global/Local 模式）；Chat↔Graph 双向联动（节点高亮、右键跳转、Ask 预填充）；洞察层 |
| **Deep Research** | LLM 驱动多轮联网研究 · Outline-First 报告架构（大纲审批 → 章节合成 → 拼装） · 学术 API（arXiv + Semantic Scholar） · 来源质量评分 · [N] 内联引用 + 学术/网页 References 双格式 · HTML 单文件导出 · 手动保存到知识库 |
| **Agent Studio** | LLM 驱动 Wiki 草稿生成；Skill 模板；检索增强；审批后自动提炼全局记忆（AAAK-lite）；Shell 审计日志 |
| **Settings** | LLM Provider 配置；搜索配置（Tavily / SearXNG / arXiv / Semantic Scholar / Brave，可多选并行）；OCR Provider；Shell 策略配置；**MCP 服务器管理** |

---

## 最近更新（H21–H26）

### v0.2.7（H25 + H26 — Deep Research 全面升级）

#### H26 — Outline-First 架构 + 学术 API + 手动保存

**报告架构（Outline-First）**
- LLM 先产出 JSON 大纲（N=breadth.clamp(3-5) 章），**用户在对话框中可视化审批**：
  - 章节标题、关键问题逐条可编辑
  - 支持任意增删章节、增删关键问题
  - 空值校验（至少 1 章 + 每章至少 1 个非空问题才能确认）
- 审批后按章节独立搜索 + 独立综合（每节 500-1000 字 + [N] 内联引用）
- Introduction / Conclusion 单独 LLM 生成（保证 Conclusion 位于报告末尾，**修复了之前 Conclusion 出现在 body 中间的 bug**）
- LLM 大纲生成失败时优雅降级到 H25 子查询分解路径，前端会明确提示"降级为标准查询分解流程"
- **JSON 提取 4 重 fallback**：剥离代码围栏 / 大括号配对（处理嵌套示例 + 字符串内 brace 屏蔽）/ 单引号容错 / 首尾大括号兜底

**学术数据源 + 来源质量评分**
- 新增 **arXiv**（Atom XML，免费无需 key）、**Semantic Scholar**（JSON，可选 API key）
- 4 个搜索源（Tavily / SearXNG / arXiv / Semantic Scholar）**可同时勾选并行查询**
- `score_by_domain(url)` 域名加权评分（0.4–0.95，arxiv/edu/gov 优先）
- 多源结果合并去重后按 quality_score 降序排序，高分源率先进入 LLM 上下文
- 合成 prompt 中高分来源带 `[high quality]` 注释

**分章节进度推送**
- 章节合成阶段独立卡片样式（18px 图标 + 加粗章节文字 + 8px 高紫色发光进度条 + `X/Y` 徽章）
- 搜索阶段每个 query 完成单独成行（✓/✗ 前缀消息不折叠，便于追踪并发查询进度）
- Introduction / Conclusion 分别 emit "撰写中..." → "✓ 已生成（N 字）" 两条进度

**报告归属：手动保存到知识库**
- 报告生成完成后**默认不写盘**，停留在对话框展示全文
- 底部 4 按钮：💾 保存到知识库 / ⬇ 导出 .md / 🗑 丢弃 / ✕ 关闭
- 保存路径走「快慢两阶段」：写盘 + DB 更新（< 1s 同步返回）+ ingest 索引（LLM 摘要/实体提取/向量化，后台 spawn，60-180s）
- 前端实时显示「✓ 已保存（⏳ 后台索引中...）」→ 「✓ 已索引」
- ResearchPanel 任务卡新增 `💬 打开对话` 按钮（运行中）/ `⏸ 需要确认` 按钮（等待大纲/保存）

**LLM 鲁棒性**
- Section / Introduction / Conclusion 调用失败时**自动重试一次**（800ms 退避）
- 重试仍失败则 emit 具体错误信息（如 `HTTP 429 / Timeout / ConnectionFailed`），不再静默吞为空字符串
- 大纲审批超时 / 通道关闭 / JSON 解析失败时分别 emit 明确提示，**用户不再面对"莫名跳过"**
- Ollama 默认超时 60s → 120s（与 OpenAI 对齐）

**其他**
- References 区每条单独成行（`\n\n` 分隔，之前的 `\n` 在 Markdown 中会被折叠成一段）
- 报告组装顺序修复：`# Title → ## Introduction → ## 1. Section → ... → ## N. Section → ## Conclusion → ## References`
- ResearchPanel `深度/广度` pill 从 Settings → 搜索配置的「默认研究深度/默认搜索广度」同步初始值
- SearchConfigPanel UI 重排：搜索提供商左列（跨两行），深度/广度右列上下并排 + 选项语义标签 + 11px 参数说明

**Bug 修复**
- 关闭对话框后无法继续审批：新增 `pending_outline_data` / `pending_query_data` / `pending_research_reports` 三个 in-memory cache + `get_pending_*` Tauri 命令，对话框重开自动恢复
- 保存到 Wiki 超时假警报：拆快慢两阶段后前端不再因 ingest 慢而误报"超时，请查日志"
- 并发保存竞态：`HashMap::remove` 替代 `get+clone`，第二次点击保存立刻得到"找不到报告"明确反馈而非启动并发 ingest

#### H25 — 报告格式 + 多搜索 + HTML 导出（v0.2.6 累积改进）

- `[N]` 行内引用 + 学术（arxiv/doi 等）vs 网页 References 双格式输出
- Tavily + SearXNG 多 provider 并行 + URL 去重合并
- `export_research_html`：HTML 单文件导出，CSS 内嵌可独立浏览
- ResearchPanel UI：terminal 风格日志 + 状态色徽章 + 语义按钮色

#### 测试基线 v0.2.7

- Rust 单测 **298** 通过 0 失败（新增 7 个 JSON 提取测试 + 5 个学术 API XML/JSON 解析测试 + 3 个 score_by_domain 测试）
- TypeScript typecheck 零错误

---

### v0.2.6（URL 上下文卡片交互增强 + Markdown 渲染）

#### Chat — `/fetch` 指令与卡片全交互
- **`/fetch <url>` 斜杠指令**：输入框检测到 `/fetch`（或 `/url` / `/抓取`）后弹出带高亮的提示条；
  Enter 抓取，Esc 取消；`/fetch` 关键字在提示条内以 accent 徽章显示，与 URL 文字区分
- 输入框在斜杠指令激活时自动加 accent 色边框+阴影（视觉反馈）
- **卡片全交互**（展开后）：
  - 📋 **复制摘要** — 写入剪贴板，2 s 反馈
  - 🔗 **打开链接** — 系统默认浏览器打开
  - 📥 **摄入 Wiki** — 加入摄入队列，3 s 反馈
  - 💬 **直接提问** — 以「关于「标题」（url），」预填输入框，焦点跳到文本域
- 摘要文字**可选中复制**；卡片 hover 时上移 1 px + 加深阴影
- 同一会话重复 `/fetch` 同一 URL 时**刷新**卡片（原有卡片先移除再重建）
- 抓取失败改为显示 5 秒红色错误条，不再静默吞噬

#### 摘要 Markdown 渲染管线
- 后端新增 `html_to_markdown()`：剔除 `<nav>/<header>/<footer>/<aside>/<form>/<svg>` 噪声块，
  `<h1-h4>` → `# ## ### ####`，`<strong>/<em>/<code>/<li>` → 行内/列表 Markdown
- `FetchResult` 新增 `markdown` 字段（与纯文本 `text` 字段并存）；
  摘要上限扩至 **3000 字符**，高度上限 **400 px 可滚动**
- 前端用 `marked.parse` + `DOMPurify.sanitize` 渲染，样式作用域限定在卡片内（紧凑 11 px 字号）

#### Bug 修复
- 正则词边界修复：`<b[^>]*>` 错误匹配 `<body>`，`<i[^>]*>` 匹配 `<img>/<input>/<iframe>`，
  导致整段 body 被误包入 `**…**` 使 `marked` 解析崩溃 → 全部加 `\b` 词边界
- 修复重复 URL 早退：同一 URL 已存在时静默 `return`，用户无任何响应

---

### v0.2.5（H24 — 无头浏览器服务 + URL 上下文卡片）

#### 无头浏览器服务（`browser` 模块）
- 新增 `src-tauri/src/browser/mod.rs`：统一 URL 内容抓取服务
  - **Chrome 优先 → Edge 次之 → 静态 HTTP 三级兜底**（`headless_chrome` CDP + `reqwest`）
  - `spawn_blocking` + `catch_unwind` 双重隔离同步 CDP 调用，防 panic 穿透
  - 顺手修复旧 `html_to_text` 中 `\1` 反向引用 bug（Rust regex 不支持）
- `ingest_service.rs` URL 抓取层重构：删除临时 Edge `--dump-dom` 实现，改调 `crate::browser`
- 新增 Tauri 命令 `fetch_url_context` → 返回 `UrlContextCard`（标题/摘要/域名/字数/抓取方式）

#### Chat — URL 上下文卡片
- **粘贴 URL** 时自动触发 `fetch_url_context`，渲染可折叠**「页面摘要卡片」**
  - 默认折叠展示：域名 + 页面标题
  - 展开后：前 300 字摘要 + 字符数 + 抓取方式
  - 点 × 一键移除
- **发送时**自动将卡片内容作为页面上下文注入消息前缀，AI 直接读取页面内容
- **测试基线**：284 通过（新增 3 个 browser 单元测试），0 失败

---

### v0.2.4（H21/H22/H23）

#### H21：全局命令面板
- `Ctrl+K` 唤出模糊搜索面板：Wiki 页面 + 操作命令 + 最近访问，键盘完全操控

#### H22：Wiki 知识导出
- **Markdown ZIP**：导出全部 Wiki 页面为标准 Markdown 压缩包
- **静态 HTML ZIP**：`pulldown-cmark` 渲染 + `[[wiki-link]]` 自动转换为 HTML 相对链接，可直接在浏览器中浏览

#### H23：Wiki 内联 AI 辅助
- 选中 Wiki 文字 → 弹出操作菜单：**续写 / 改写 / 扩写**
- 流式预览窗口，支持接受/拒绝，不污染原文直到确认

---

### v0.2.3（代码质量 + 体验修复）

#### Chat：历史工具调用显示
- 重载历史对话时，AI 工具调用卡片（`run_shell` / `search_wiki` / `spawn_subagent` 等）与流式阶段视觉完全一致
- `Message` 结构体新增 `tool_calls` 字段，`list_messages` 解析后直达前端

#### 代码质量
- **state/ 测试全模块拆分**：136 个 state.rs 测试迁移至对应 service 文件，`state.rs` 仅保留跨服务集成测试
- **编译警告清零**：消除全部 Rust 警告（unused import / dead_code / deprecated）
- **测试基线**：268 通过，0 失败

---

### v0.2.2（H14 + 今日修复）

#### H14：Chat Agent Shell 三模式
- 每个对话独立配置 Shell 权限：`off`（禁用）/ `approval`（每次审批）/ `yolo`（直接执行）
- 输入栏 Shell 图标一键循环切换，审批模式下弹出审批卡片 + 30 秒倒计时自动拒绝
- Shell 模式持久化到会话数据库，重启后保留

#### 体验修复
- **外部链接**：点击 AI 回复中的超链接改为在系统默认浏览器打开，不再替换 App 界面
- **文件附件展示**：上传文档后用户消息气泡只显示附件徽章（文件名 + 字数），不再展开原文内容
- **`.doc` 格式支持**：新增旧版 Word 文件上传（OLE 复合文档，含 ASCII + UTF-16LE CJK 启发式文本提取）
- **UI 细节**：Shell 未启用图标换为 SVG 终端图标；`+` 按钮缩小（34px→30px）并换用 SVG 十字精确居中

---

### H13：Chat ↔ Graph 双向联动
- AI 回复中的 Wiki 路径自动高亮图谱节点
- 图谱节点右键菜单：「向 Chat 提问」「在 Chat 中检索」
- GraphBridgeContext 统一管理双向通信状态

### H12：Wiki 混合语义检索
- `searchWikiPagesHybrid` 融合 FTS5 全文 + embedding 余弦相似度
- 后台自动为未索引页面生成向量（`VectorIndexWorker`）
- 搜索策略自动降级：embedding 可用时混合，否则纯 FTS

### H11：Agent Swarm（spawn_subagent 工具）
- Chat Agent 可动态派生子对话，异步执行子任务并返回摘要
- 深度限制：子代理无法再次派生，防止无限递归
- 子对话标题前缀 `[子代理]`，可在对话列表追踪

### H10：MCP Client 集成
- 实现基于 stdio JSON-RPC 2.0 的 MCP 客户端（`tokio::process`）
- Settings 面板支持添加/删除/重载 MCP 服务器及其工具
- Agent 工具调用自动路由到对应 MCP 服务器（`handler_kind = "mcp:<name>"`）

### H6-P2/P3：Shell 安全增强
- **审批票据缓存**：勾选「记住 5 分钟」后，相同路径/动作免重复弹窗（TTL 300s）
- **Shell 审计落库**：所有命令执行（含被拦截）写入 `shell_audit_log` 表
- Agent Studio → Shell 历史区底部可查看最近 20 条审计记录

### 代码结构重构
- `tauri-client.ts`（2309 行）拆分为 10 个领域子模块（chat / shell / wiki / ingest / search / agent / config / dialog / mcp），主文件改为 barrel re-export
- `styles.css`（1523 行 → 838 行，-45%），模块专属样式迁至各模块 CSS 文件
- Graph UI：工具栏标签换行修复、右键菜单文字可见性修复、图谱背景改为亮色主题

---

## 模块依赖与服务速查

| 模块 | 必需服务 | 可选服务 | 关键配置 |
|------|----------|----------|----------|
| **Ingest（文本）** | 无额外服务 | - | 初始化 Vault 后即可使用 |
| **Ingest（图片/PDF OCR）** | Tesseract（建议含 `eng` + `chi_sim`） | PaddleOCR | `Settings → OCR Provider` |
| **Query / Ask / Chat** | Ollama 或 OpenAI-compatible 云 Provider | - | `Settings → LLM Provider` |
| **语义检索 / 混合搜索 / 图谱** | Ollama embedding 模型（`nomic-embed-text`） | - | `embed_ollama_model=nomic-embed-text` |
| **Chat web_search** | 无（DuckDuckGo 兜底无需配置） | SearXNG / Tavily / Brave Search | `Settings → 搜索配置` |
| **Deep Research** | 搜索 Provider（至少配置一个：arXiv/SearXNG 免费，Tavily/Semantic Scholar 可选 key） | - | `Settings → 搜索配置`（支持多源并行） |
| **Chat MCP 工具** | 任意 MCP 服务器进程 | - | `Settings → MCP 服务器` 添加并重载 |
| **Clipper 扩展** | 桌面 App 运行中 + Vault 已打开 | Chrome/Edge 扩展 | 本地服务 `127.0.0.1:19827` |
| **Strict Local Mode** | Ollama（本地） | - | 禁止云 Provider，敏感任务强制本地 |

---

## 安装

### 前提软件

| 软件 | 说明 | 下载 |
|------|------|------|
| **Ollama**（必须，本地 AI） | 本地 LLM 推理服务，提供对话/摘要/embedding | https://ollama.com |
| Tesseract OCR（可选） | 图片/扫描 PDF 文字识别（含 `chi_sim` 语言包） | https://github.com/UB-Mannheim/tesseract/wiki |
| Poppler（可选） | PDF → 图片转换（OCR 回退路径） | 随 Tesseract Windows 安装包附带 |
| Docker Desktop（可选） | 本地运行 SearXNG（联网搜索） | https://www.docker.com/products/docker-desktop |

> **Tesseract 中文支持**  
> 1. 下载 `chi_sim.traineddata` → https://github.com/tesseract-ocr/tessdata  
> 2. 复制到 `%USERPROFILE%\tessdata\`  
> 3. 系统环境变量添加 `TESSDATA_PREFIX` = `%USERPROFILE%\tessdata`

### 可选：启动本地 SearXNG

```powershell
.\scripts\run_searxng_windows.ps1 -Recreate
.\scripts\verify_searxng_windows.ps1 -Query "rust async runtime"
```

配置路径：`Settings → 搜索配置 → SearXNG，地址 http://127.0.0.1:8080`

### 拉取 Ollama 模型

```powershell
# 对话 / 推理（选其一）
ollama pull qwen2.5:7b          # 推荐，中英双语
ollama pull deepseek-r1:7b      # 带 reasoning 推理模式

# Embedding（必须，用于语义检索 + 混合搜索）
ollama pull nomic-embed-text
```

### 安装应用

1. 从 [Releases](../../releases) 下载最新安装包
2. 双击安装（默认 `%LOCALAPPDATA%\LLM Wiki\`）
3. 启动后在 **Settings → LLM Provider** 填写 Ollama 地址并选择模型

---

## Chat 模块使用指南

Chat 是能力较强的 ReAct Agent，内置 8 个工具：

| 工具 | 说明 |
|------|------|
| `run_shell` | 执行 PowerShell 命令（受 Shell 策略控制；支持 5 分钟免审批票据缓存） |
| `search_wiki` | 在本地知识库全文 + 语义混合搜索 |
| `read_wiki` | 读取指定 Wiki 页面内容 |
| `write_wiki` | 写入新 Wiki 页面（需审批，支持记住 5 分钟） |
| `edit_wiki` | 编辑现有 Wiki 页面（需审批，支持记住 5 分钟） |
| `web_search` | 联网搜索（自动级联：SearXNG → Tavily → Brave → DuckDuckGo；结果显示来源标签） |
| `fetch_url` | 获取网页正文（精准提取，最大 8000 字符） |
| `spawn_subagent` | 派生子 Agent 对话异步执行子任务，返回摘要（最大深度 1） |

**MCP 工具扩展**：在 `Settings → MCP 服务器` 添加任意 MCP 服务器后，其工具自动注册到 Chat Agent 工具列表，调用时通过 stdio JSON-RPC 2.0 转发。

**Shell 策略**（`Settings → Shell 策略`）：
- 按命令类型（read / write / network / script / destructive）× 来源（manual / agent）分类
- 支持三档策略：`auto_allow` / `require_approval` / `deny`
- 审批时勾选「记住 5 分钟」可创建 TTL 300s 的审批票据，同范围内自动放行

**联网搜索配置**（可选，不配置时走 DuckDuckGo 兜底）：
- Tavily：https://app.tavily.com 申请 API Key → `Settings → 搜索配置 → Tavily`
- Brave Search：https://api.search.brave.com 申请 → `Settings → 搜索配置 → Brave Search`
- SearXNG：本地自托管，见上文

---

## 开发环境搭建

### 必要工具

| 工具 | 版本 | 说明 |
|------|------|------|
| Rust + Cargo | stable ≥ 1.78 | https://rustup.rs |
| Node.js | ≥ 20 LTS | https://nodejs.org |
| Tauri CLI v2 | latest | `cargo install tauri-cli --version "^2"` |
| WebView2 Runtime | Windows 内置或手动安装 | https://developer.microsoft.com/microsoft-edge/webview2/ |

### 克隆并运行

```powershell
git clone <repo-url>
cd llm-wiki
npm install          # 从根目录安装（workspace 模式）
cargo tauri dev
```

### 打包发布

```powershell
cargo tauri build
# 产物在 src-tauri/target/release/bundle/
```

### 测试

```powershell
# Rust 单测（298 项）
cargo test --manifest-path src-tauri/Cargo.toml

# 前端类型检查
cd web && npm run typecheck
```

---

## 项目结构

```
llm-wiki/
├── src-tauri/              # Rust 后端（Tauri v2）
│   └── src/
│       ├── commands.rs     # Tauri 命令注册入口
│       ├── state.rs        # AppState 入口 + 公共工具函数
│       ├── state/          # 业务逻辑子模块（H16 拆分，12 个 service 文件）
│       ├── browser/        # 无头浏览器服务（CDP + reqwest，H24）
│       │   └── mod.rs      # fetch_url / FetchResult / html_to_text / html_to_markdown
│       ├── db.rs           # SQLite（FTS5 + embedding + 队列 + shell 审计）
│       ├── vault.rs        # Markdown Vault 读写
│       ├── models.rs       # 数据模型
│       ├── agent_policy.rs # Shell 策略分类 + 审批票据缓存（TTL 5min）
│       ├── llm/            # LLM Provider（Ollama / OpenAI-compatible）
│       │   ├── stream_parser.rs  # SSE 流式解析 + reasoning_content
│       │   └── types.rs    # ChatMessage / ToolCall / StreamEvent
│       └── agent_chat/     # Chat 模块后端
│           ├── commands.rs # 会话/消息/MCP 管理 Tauri 命令
│           ├── runtime.rs  # ReAct 主循环 + 事件 emit
│           ├── tools.rs    # 工具执行分发（8 个内置 + MCP 路由）
│           ├── db.rs       # 会话/消息/工具/MCP配置 SQLite CRUD
│           └── mcp.rs      # MCP 客户端（stdio JSON-RPC 2.0）
├── web/                    # React + TypeScript 前端
│   └── src/
│       ├── modules/
│       │   ├── chat/       # Chat UI（对话列表/消息流/工具卡片/审批）
│       │   ├── wiki/       # Wiki 编辑/浏览
│       │   ├── graph/      # 知识图谱可视化（含 Chat 双向联动）
│       │   ├── agent/      # Agent Studio（运行/草稿/记忆/技能/审计）
│       │   ├── ask/        # Query 问答
│       │   ├── lint/       # Lint 检测与修复
│       │   ├── settings/   # 设置面板（LLM/搜索/OCR/Shell/MCP）
│       │   ├── research/   # Deep Research
│       │   └── operations/ # 摄入队列管理
│       ├── tauri-client/   # Tauri 命令封装（10 个领域子模块）
│       │   ├── base.ts     # isTauriRuntime / withTimeout / openExternalUrl
│       │   ├── chat.ts     # 对话 CRUD + 消息 + 审批
│       │   ├── shell.ts    # Shell 执行 / 审计 / 策略 / 票据
│       │   ├── wiki.ts     # Wiki CRUD / 图谱 / Lint
│       │   ├── ingest.ts   # 摄入流水线 / 队列
│       │   ├── search.ts   # 问答会话 / 研究任务
│       │   ├── agent.ts    # Agent 运行 / 草稿 / 记忆 / 技能
│       │   ├── config.ts   # LLM / OCR / Vault 配置
│       │   ├── dialog.ts   # 文件/文件夹选择对话框
│       │   ├── mcp.ts      # MCP 服务器管理
│       │   └── browser.ts  # URL 上下文抓取（fetch_url_context）
│       ├── tauri-client.ts # barrel re-export（11 行）
│       ├── contexts/       # React Context（GraphBridgeContext 等）
│       ├── App.tsx
│       ├── types.ts
│       └── styles.css      # 全局基础样式（838 行，模块样式已拆至各模块）
├── docs/                   # 设计与开发文档
├── agents.md               # 三方 Agent 协作协议
├── scripts/                # SearXNG / Clipper 自检脚本
└── extension/              # 浏览器 Clipper 扩展
```

---

## 数据存储

- **Vault**（Markdown）：`%APPDATA%\llm-wiki\vault\`
- **主数据库**：同目录 `meta.db`（wiki_pages / ask_sessions / agent_runs / ingest_queue / shell_audit_log 等表）
- **Chat 数据库**：同目录 `agent_chat.db`（conversations / messages / agent_tools / mcp_server_configs 表）
- **搜索配置**：`%APPDATA%\llm-wiki\search-config.json`

---

## License

MIT
