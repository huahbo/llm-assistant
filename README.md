# LLM Wiki Desktop

> v0.2.0 · Windows 优先的个人 AI 知识库桌面应用

本地优先架构：Tauri v2 + React + TypeScript + SQLite + Markdown Vault，支持本地 AI（Ollama）与云端 OpenAI-compatible Provider，隐私友好。

---

## 功能概览

| 模块 | 说明 |
|------|------|
| **Chat（AI 对话）** | ReAct 多轮 Agent 对话；Markdown 渲染 + 代码高亮 + 复制按钮；内置工具：run_shell / search_wiki / read_wiki / write_wiki / edit_wiki / **web_search / fetch_url**；四级联网搜索（SearXNG → Tavily → Brave → DuckDuckGo）；DeepSeek reasoning 支持；流式输出；对话归档/重命名/搜索 |
| **Ingest** | 支持 Markdown / PDF / DOCX / PPTX / TXT / 图片 OCR，URL 抓取，拖拽摄入，持久化 ingest 队列（含重试/取消） |
| **Query / Ask** | FTS5 + embedding + 引用热度 + 链接扩展 四路 RRF 检索；Ollama / OpenAI-compatible 流式对话；Ask 会话持久化管理 |
| **Wiki** | Markdown 编辑/渲染/重命名/删除，双向链接，内链补全，实体提取，Frontmatter 元数据 |
| **Lint** | 语义矛盾/陈旧/覆盖度检测，Wiki-link 级 broken/orphan 检测，可预览/应用修复补丁 |
| **Graph** | 知识图谱可视化，Global/Local 模式，洞察层（孤立节点/稀疏社区/桥接节点/异常连接 + embedding 相似度评分） |
| **Deep Research** | LLM 驱动的多轮联网研究，自动分解子查询、多源聚合、综合报告，结果写入 Wiki |
| **Agent Studio** | LLM 驱动的 Wiki 草稿生成工作区：Skill 模板、检索增强、审批后自动提炼全局记忆（AAAK-lite） |
| **Settings** | LLM Provider 配置（Ollama / OpenAI-compatible）；搜索配置（SearXNG / Tavily / Brave Search）；OCR Provider；Shell 策略 |

---

## 模块依赖与服务速查

| 模块 | 必需服务 | 可选服务 | 关键配置 |
|------|----------|----------|----------|
| **Ingest（文本）** | 无额外服务 | - | 初始化 Vault 后即可使用 |
| **Ingest（图片/PDF OCR）** | Tesseract（建议含 `eng` + `chi_sim`） | PaddleOCR | `Settings → OCR Provider` |
| **Query / Ask / Chat** | Ollama 或 OpenAI-compatible 云 Provider | - | `Settings → LLM Provider` |
| **语义检索 / 图谱** | Ollama embedding 模型（`nomic-embed-text`） | - | `embed_ollama_model=nomic-embed-text` |
| **Chat web_search** | 无（DuckDuckGo 兜底无需配置） | SearXNG / Tavily / Brave Search | `Settings → 搜索配置`，配置 API Key 可提升质量 |
| **Deep Research** | 搜索 Provider（至少配置一个） | - | `Settings → 搜索配置` |
| **Clipper 扩展** | 桌面 App 运行中 + Vault 已打开 | Chrome/Edge 扩展 | 本地服务 `127.0.0.1:19827` 可访问 |
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

# Embedding（必须，用于语义检索）
ollama pull nomic-embed-text
```

### 安装应用

1. 从 [Releases](../../releases) 下载 `LLM-Wiki_0.2.0_x64-setup.exe` 或 `.msi`
2. 双击安装（默认 `%LOCALAPPDATA%\LLM Wiki\`）
3. 启动后在 **Settings → LLM Provider** 填写 Ollama 地址并选择模型

---

## Chat 模块使用指南

Chat 是一个能力较强的 AI Agent，内置 7 个工具：

| 工具 | 说明 |
|------|------|
| `run_shell` | 执行 PowerShell 命令（受 Shell 策略控制） |
| `search_wiki` | 在本地知识库全文/语义搜索 |
| `read_wiki` | 读取指定 Wiki 页面内容 |
| `write_wiki` | 写入新 Wiki 页面（需审批） |
| `edit_wiki` | 编辑现有 Wiki 页面（需审批） |
| `web_search` | 联网搜索（自动级联：SearXNG → Tavily → Brave → DuckDuckGo） |
| `fetch_url` | 获取网页正文（scraper 精准提取，最大 8000 字符） |

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
# Rust 单测
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
│       ├── state.rs        # 业务逻辑（ingest/query/lint/search/agent）
│       ├── db.rs           # SQLite（FTS5 + embedding + 队列）
│       ├── vault.rs        # Markdown Vault 读写
│       ├── models.rs       # 数据模型（含 SearchConfig）
│       ├── llm/            # LLM Provider（Ollama / OpenAI-compatible）
│       │   ├── stream_parser.rs  # SSE 流式解析 + reasoning_content
│       │   └── types.rs    # ChatMessage / ToolCall / StreamEvent
│       └── agent_chat/     # Chat 模块后端
│           ├── commands.rs # 会话/消息/工具 Tauri 命令
│           ├── runtime.rs  # ReAct 主循环 + Tauri 事件 emit
│           ├── tools.rs    # 工具执行分发（7个内置工具）
│           └── db.rs       # 会话/消息/工具 SQLite CRUD
├── web/                    # React + TypeScript 前端
│   └── src/
│       ├── modules/chat/   # Chat 模块 UI
│       │   ├── ChatModule.tsx
│       │   ├── MessageThread.tsx
│       │   ├── MessageBubble.tsx      # 头像 + Markdown 渲染 + 复制
│       │   ├── MarkdownRenderer.tsx   # marked + DOMPurify + highlight.js
│       │   ├── ToolCallCard.tsx
│       │   ├── ConversationList.tsx
│       │   └── hooks/useChatStream.ts # 流式 SSE → React state
│       ├── App.tsx
│       ├── tauri-client.ts
│       ├── types.ts
│       └── styles.css
├── docs/                   # 设计与开发文档
├── agents.md               # 三方 Agent 协作协议
├── scripts/                # SearXNG / Clipper 自检脚本
└── extension/              # 浏览器 Clipper 扩展
```

---

## 数据存储

- **Vault**（Markdown）：`%APPDATA%\llm-wiki\vault\`
- **主数据库**：同目录 `meta.db`（wiki_pages / ask_sessions / agent_runs / ingest_queue 等表）
- **Chat 数据库**：同目录 `agent_chat.db`（conversations / messages / tools 表）
- **搜索配置**：`%APPDATA%\llm-wiki\search-config.json`

---

## License

MIT
