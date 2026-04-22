# LLM Wiki Desktop

> v0.1.0 · Windows 优先的个人 Wiki 桌面应用

本地优先架构：Tauri v2 + React + TypeScript + SQLite + Markdown Vault，隐私兼容本地 AI 路径（Ollama）。

---

## 功能概览

| 模块 | 说明 |
|------|------|
| **Ingest** | 支持 Markdown / PDF / DOCX / PPTX / TXT / 图片 OCR，URL 抓取，拖拽摄入，持久化 ingest 队列（含重试/取消） |
| **Query / Ask** | FTS5 + embedding + 引用热度 + 链接扩展 四路 RRF 检索；Ollama / OpenAI-compatible 流式对话 |
| **Wiki** | Markdown 编辑/渲染/重命名/删除，双向链接，内链补全，实体提取，Frontmatter 元数据 |
| **Lint** | 语义矛盾/陈旧/覆盖度检测，Wiki-link 级 broken/orphan 检测，可预览/应用修复补丁 |
| **Graph** | 知识图谱可视化，Global/Local 模式，洞察层（孤立节点/稀疏社区/桥接节点/异常连接 + embedding 相似度评分） |
| **Settings** | LLM Provider 配置（Ollama / OpenAI-compatible），OCR Provider，拖拽摄入模式 |

---

## 模块依赖与服务速查

| 模块 | 必需服务 | 可选服务 | 关键配置 |
|------|----------|----------|----------|
| **Ingest（文本）** | 无额外服务 | - | 初始化 Vault 后即可使用 |
| **Ingest（图片/PDF OCR）** | Tesseract（建议含 `eng` + `chi_sim`） | PaddleOCR | `Settings → OCR Provider` 选择 `tesseract` 或 `paddle` |
| **Query / Ask** | Ollama（或 OpenAI-compatible 云 Provider） | - | 推荐本地：`http://localhost:11434` + 可用模型 |
| **语义检索 / 图谱异常连接** | Ollama embedding 模型（`nomic-embed-text`） | - | `embed_ollama_model=nomic-embed-text` |
| **Deep Research** | 搜索 Provider（二选一：Tavily / SearXNG） | - | `Settings → 搜索配置` 填 API Key 或 `http://127.0.0.1:8080` |
| **Clipper 扩展** | 桌面 App 运行中 + Vault 已打开 | Chrome/Edge 扩展 | 本地服务 `127.0.0.1:19827` 可访问 |
| **Strict Local Mode** | Ollama（本地） | - | 禁止云 Provider，敏感任务强制本地 |

---

## 安装

### 前提软件

安装顺序建议按下表执行，带 **必须** 标记的为运行时必要依赖。

| 软件 | 说明 | 下载 |
|------|------|------|
| **Ollama**（必须，本地 AI） | 本地 LLM 推理服务，提供对话/摘要/embedding | https://ollama.com |
| Tesseract OCR（可选） | 图片/扫描 PDF 文字识别（含 `chi_sim` 语言包） | https://github.com/UB-Mannheim/tesseract/wiki |
| Poppler（可选） | PDF → 图片转换（OCR 回退路径所需） | 随 Tesseract Windows 安装包附带，或 https://github.com/oschwartz10612/poppler-windows/releases |
| Docker Desktop（可选） | 本地运行 SearXNG（Deep Research 搜索） | https://www.docker.com/products/docker-desktop |
| Chrome / Edge（可选） | 使用浏览器 Clipper 扩展 | 浏览器官方安装页 |

> **Tesseract 语言包说明**  
> 默认安装包含英文（`eng`），中文需手动下载 `chi_sim.traineddata`：  
> 1. 前往 https://github.com/tesseract-ocr/tessdata 下载 `chi_sim.traineddata`  
> 2. 复制到 `%USERPROFILE%\tessdata\`（若该目录不存在则新建）  
> 3. 在系统环境变量中添加 `TESSDATA_PREFIX` = `%USERPROFILE%\tessdata`（用户变量，无需管理员权限）

### 可选：启动本地 SearXNG（Deep Research）

```powershell
# 启动容器（首次会自动拉取镜像）
docker run -d --name searxng `
  -p 8080:8080 `
  -e SEARXNG_LIMITER=false `
  -e SEARXNG_PUBLIC_INSTANCE=false `
  searxng/searxng

# 自检（仓库内脚本，带无代理直连与最优参数档位探测）
Set-Location E:\llm-wiki
.\scripts\verify_searxng_windows.ps1 -Query "rust async runtime"
```

- 应用中配置：`Settings → 搜索配置 → 搜索提供商 = searxng`，地址填 `http://127.0.0.1:8080`
- 若脚本显示 `best.results.count = 0`，通常是搜索引擎被限流/不可用，需调整 SearXNG engines 配置。

### 可选：Clipper 扩展（浏览器一键剪藏）

```powershell
Set-Location E:\llm-wiki
.\scripts\verify_clipper_windows.ps1
```

- 先启动桌面 App 并打开 Vault，再在浏览器加载 `extension/` 目录作为本地扩展。
- 自检通过后，扩展点击 `Clip to Wiki` 会写入 `raw/clips/` 并进入应用摄入链路。

### 拉取 Ollama 模型

```powershell
# 对话 / 摘要 / 实体提取（选其一）
ollama pull qwen2.5:7b        # 推荐，中英双语
# ollama pull llama3.1:8b

# Embedding（必须，用于语义检索与图谱洞察）
ollama pull nomic-embed-text
```

### 安装应用

1. 从 [Releases](../../releases) 下载 `LLM-Wiki_0.1.0_x64-setup.exe`（NSIS 安装包）或 `LLM-Wiki_0.1.0_x64_en-US.msi`
2. 双击安装，默认安装到 `%LOCALAPPDATA%\LLM Wiki\`
3. 启动应用，在 **Settings → LLM Provider** 填写 Ollama 地址（默认 `http://localhost:11434`）并选择已拉取的模型

---

## 开发环境搭建

### 必要工具

| 工具 | 版本要求 | 说明 |
|------|----------|------|
| Rust + Cargo | stable（≥ 1.78） | https://rustup.rs |
| Node.js | ≥ 20 LTS | https://nodejs.org |
| Tauri CLI v2 | 随 Cargo 安装 | `cargo install tauri-cli --version "^2"` |
| WebView2 Runtime | Windows 内置或手动安装 | https://developer.microsoft.com/en-us/microsoft-edge/webview2/ |

### 克隆并运行

```powershell
git clone <repo-url>
cd llm-wiki
npm --prefix web install
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

# 前端单测（Vitest）
cd web && npm run test -- --run

# 类型检查
cd web && npm run typecheck
```

当前基线：**116 Rust / 149 前端 / typecheck 0 errors**

---

## 项目结构

```
llm-wiki/
├── src-tauri/          # Rust 后端（Tauri v2）
│   └── src/
│       ├── commands.rs # 全部 Tauri 命令注册
│       ├── state.rs    # 业务逻辑（ingest/query/lint/ocr/队列 worker）
│       ├── db.rs       # SQLite 操作（FTS5 + embedding + 队列）
│       ├── vault.rs    # Markdown Vault 文件读写
│       ├── search.rs   # RRF 四路检索
│       ├── models.rs   # 数据模型
│       └── llm/        # LLM Provider（Ollama / OpenAI-compatible）
├── web/                # React + TypeScript 前端
│   └── src/
│       ├── App.tsx     # 主界面（所有模块 UI）
│       ├── tauri-client.ts  # Tauri 命令封装
│       ├── types.ts    # 前端类型定义
│       └── styles.css
├── docs/               # 设计与过程文档
│   ├── v1-technical-design.md
│   ├── 实施过程记录.md
│   └── 交接状态卡.md
└── agents.md           # 三方 Agent 协作协议与进度
```

---

## 数据存储

- **Vault**（Markdown 文件）：`%APPDATA%\llm-wiki\vault\`（Windows）或 `~/.local/share/llm-wiki/vault/`（开发）
- **SQLite DB**：同目录下 `wiki.db`（含 wiki_pages / citations / fts_pages / ask_history / wiki_outbox / ingest_queue_items 等表）

---

## License

MIT
