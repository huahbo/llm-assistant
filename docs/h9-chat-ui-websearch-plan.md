# H9 — Chat UI 美化 + Web 搜索工具 实施计划

> 创建日期：2026-05-09  
> 主责：Claude Code  
> 状态：**阶段 A 进行中**

---

## 背景

H8 完成了对话模块核心功能（ReAct 循环、工具审批、流式输出）。
H9 在此基础上分两阶段：A 提升 UI 体验，B 新增网络工具。

---

## 阶段 A — UI 三项改进（2026-05-09）

### A1. Markdown 渲染 + 代码块高亮可复制

**新增依赖：**
```
web/: highlight.js
（marked + dompurify 已存在，不重复安装）
```

**新建文件：** `web/src/modules/chat/MarkdownRenderer.tsx`

策略：
- 将 markdown 文本按代码块（` ```lang\n...\n``` `）切割为若干段
- 非代码段：用 `marked` 转 HTML，`dompurify` 清洗，`dangerouslySetInnerHTML` 渲染
- 代码块段：渲染为 React 组件，使用 `highlight.js` 语法高亮
  - 顶部显示语言标签（右侧有复制按钮）
  - 点击复制使用 `navigator.clipboard.writeText`
  - 复制成功后 icon 短暂变成 ✓

**修改：** `web/src/modules/chat/MessageBubble.tsx`
- streaming 中（`streaming=true`）：仍用纯文本渲染（避免部分 Markdown 闪烁）
- 流结束后：切换为 `<MarkdownRenderer>` 渲染
- 历史消息（persisted）：始终用 `<MarkdownRenderer>`

### A2. 消息整体复制按钮

- Assistant bubble 右上角：hover 时出现复制图标按钮
- 复制完整 `content` 文本（非 HTML，原始 markdown）
- CSS `opacity: 0` → `opacity: 1` hover 过渡

### A3. 卡通头像图标

- **User**：圆头 + 浅蓝背景，简洁人形轮廓，28×28 inline SVG
- **Assistant**：方头机器人 + 浅紫背景，天线 + 圆眼，28×28 inline SVG
- 位置：bubble 上方左（assistant）/ 右（user）对齐
- 不引入新依赖

**CSS 改动：** `web/src/styles.css`
- `.chat-bubble--avatar` 布局
- `.chat-bubble__copy-btn` hover 效果
- `.md-code-block` 代码块容器样式
- `.md-code-block__header` 语言标签 + 复制按钮行
- 引入 highlight.js 主题（atom-one-dark 或 github-dark）

---

## 阶段 B — Web 搜索 + URL 抓取工具（后续）

### B1. 扩展 SearchConfig（`src-tauri/src/models.rs`）

新增字段：
```rust
pub brave_api_key: String,  // 默认空字符串
```

向后兼容（serde default = ""）。

### B2. Brave Search API 实现（`src-tauri/src/state.rs`）

```
POST https://api.search.brave.com/res/v1/web/search
Header: X-Subscription-Token: {brave_api_key}
Params: q, count, search_lang
```

返回 `Vec<WebSearchResult>`，与现有结构一致。

### B3. PowerShell DuckDuckGo 兜底（`src-tauri/src/state.rs`）

通过 `run_shell_impl` 执行：
```powershell
$r = Invoke-WebRequest -Uri "https://html.duckduckgo.com/html/?q=QUERY" -UseBasicParsing
# 正则提取 result__title / result__snippet
```
解析 HTML 结果，返回前 max_results 条。

### B4. 级联调度器（`src-tauri/src/state.rs`）

```rust
pub async fn search_web_cascade(
    &self, query: &str, max_results: usize
) -> Result<Vec<WebSearchResult>, String>
```

顺序：**SearXNG → Tavily → Brave → PowerShell**  
- 各步骤：配置不足自动跳过，有结果立即返回  
- 全部失败返回 `Err("搜索服务不可用：SearXNG/Tavily/Brave 均未配置或无响应，PowerShell 兜底也失败")`

### B5. fetch_url 工具（`src-tauri/src/agent_chat/tools.rs`）

使用 `scraper` crate（CSS 选择器 HTML 解析，高精度）：
- GET 目标 URL，跟随重定向
- 优先提取 `<main>`, `<article>`, `[role=main]` 内容
- 移除 `<script>`, `<style>`, `<nav>`, `<footer>`, `<aside>`, `<header>`
- 保留段落结构（`<p>`, `<h1-6>`, `<li>` 转换为可读文本）
- 返回最多 `max_chars`（默认 8000）字符

**Cargo.toml 新增：**
```toml
scraper = "0.20"
```

### B6. Agent 工具注册（`src-tauri/src/agent_chat/db.rs`）

seed_builtin_tools 追加：
```
web_search  | 使用级联搜索引擎查询网络信息 | {query, max_results?}
fetch_url   | 抓取并提取网页正文文本       | {url, max_chars?}
```

### B7. 设置 UI（`web/src/modules/lint/SearchConfigPanel.tsx`）

新增 Brave API Key 输入框：
- 仅在选择 Brave 时（或始终显示）
- 保存时同步新字段

---

## 关键约束

- 流式阶段不渲染 Markdown（防止闪烁）
- fetch_url 超时 30s，Content-Length > 5MB 拒绝
- 级联搜索总超时 45s（每个 provider 15s 各自独立）
- scraper 解析失败时降级为 regex strip HTML tags

---

## 验证基线

```powershell
cd src-tauri; cargo test    # 阶段 B 后运行
cd web; npm run typecheck   # 每阶段后运行
```

---

## 文件改动清单

### 阶段 A
| 文件 | 操作 |
|------|------|
| `web/src/modules/chat/MarkdownRenderer.tsx` | 新建 |
| `web/src/modules/chat/MessageBubble.tsx` | 修改 |
| `web/src/styles.css` | 修改（追加 chat 样式） |
| `web/package.json` | 修改（+highlight.js） |

### 阶段 B
| 文件 | 操作 |
|------|------|
| `src-tauri/src/models.rs` | 修改（+brave_api_key） |
| `src-tauri/src/state.rs` | 修改（+search_brave, +search_powershell, +search_web_cascade） |
| `src-tauri/src/agent_chat/tools.rs` | 修改（+exec_web_search, +exec_fetch_url） |
| `src-tauri/src/agent_chat/db.rs` | 修改（seed 追加 2 工具） |
| `src-tauri/Cargo.toml` | 修改（+scraper） |
| `web/src/modules/lint/SearchConfigPanel.tsx` | 修改（+brave_api_key 字段） |
