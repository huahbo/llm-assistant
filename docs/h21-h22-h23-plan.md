# H21 / H22 / H23 实施计划

> 起草时间：2026-05-17  
> 背景：经全面审查，H18（持久化 Ingest 队列）/ H19（Deep Research）/ H20（Web Clipper）均已完成。  
>       本计划聚焦三个真正的新方向，按优先级排列。

---

## 背景：项目实际状态（截至 2026-05-17）

dev-status.md 严重过期（仅记录到 H16）。真实完成项超过 104 个，重要功能包括：
- 持久化 Ingest 队列（P23 ✅）
- Deep Research 全链路（P26 ✅，含多轮修复）
- Web Clipper + Chrome 扩展（P23-P26 ✅）
- 图谱洞察层（P24 ✅）
- Embedding RRF 第四路召回（P20-5 ✅）
- 拖拽摄入（P25 ✅）
- 摄入 HITL 审核（P27 ✅）
- 页面变更历史（Direction-G ✅）
- Vault 统计仪表盘（Direction-E ✅）
- MCP 市场一键安装（H17 ✅）

**一项必须先做的清理任务**（在 H21/H22/H23 开干前）：
- 修复 `web/src/tauri-client/search.ts` 中 5 处裸 `invoke()` 无超时包装
- 更新 `docs/dev-status.md` 反映真实项目状态

---

## H21：全局命令面板（Command Palette）

### 价值
目前导航依靠侧边栏手动点击。Wiki 页面增多后，"找到并跳转"的效率急剧下降。  
Command Palette（⌘K / Ctrl+K）是单人知识工具里密度最高的 UX 升级：  
一次按键即可搜索页面、执行操作、切换模块——不打断思维流。

### 功能范围（MVP）

**触发**：全局 `Ctrl+K`，任意模块可用，按 Esc 关闭。

**三类结果**：
1. **Wiki 页面**：模糊匹配标题/路径，按 BM25 分数排序，前 8 条
2. **操作命令**：固定动作列表（新建页面、开始研究、打开设置、切换模块…）
3. **最近访问**：最近 5 个打开过的 Wiki 页面（localStorage 持久化）

**交互**：
- 键盘上下选择、Enter 执行
- 结果图标区分类型（📄 页面 / ⚡ 命令 / 🕐 最近）
- 输入 `>` 前缀过滤纯命令；输入 `#tag` 按标签过滤页面

### 技术方案

**前端**（纯前端，无需新 Tauri 命令）：
- `web/src/modules/palette/CommandPalette.tsx` — 新组件
- `web/src/modules/palette/palette.css` — 样式
- `web/src/contexts/CommandPaletteContext.tsx` — 全局开关 + 结果集
- App.tsx：绑定 `Ctrl+K` keydown，挂载 `<CommandPalette />`
- 复用现有 `searchWikiPages`（已有 Tauri 命令），加 100ms 防抖

**后端**（已具备，无需新增命令）：
- `search_wiki_pages`：搜索 Wiki 页面
- `list_wiki_pages` 的变体（最近访问用 localStorage 即可）

### 并行分工
- **Phase A（前端）**：CommandPalette 组件 + CSS + Esc/Enter 键盘 + 防抖搜索
- **Phase B（集成）**：App.tsx 挂载 + Ctrl+K 全局监听 + CommandPaletteContext

Phase A 与 B 串行（B 依赖 A）。整体预计 1.5 天。

### 文件清单
```
新增：
  web/src/modules/palette/CommandPalette.tsx
  web/src/modules/palette/palette.css
  web/src/contexts/CommandPaletteContext.tsx

修改：
  web/src/App.tsx（挂载 + 全局 keydown）
  web/src/types.ts（CommandPaletteResult 类型，可选）
```

### 验收标准
- Ctrl+K 在任意模块可弹出面板，Esc 关闭
- 输入关键词 300ms 内展示 Wiki 页面结果
- 键盘上下导航，Enter 打开 Wiki 页面或执行命令
- `npm run typecheck` 零错误，现有 268 测试通过

---

## H22：Wiki 知识导出

### 价值
知识进得去，也要出得来。目前 Wiki 内容只能在 App 内查看。  
导出功能让知识可以分享、存档、离线阅读，完成"知识构建闭环"。

### 功能范围（MVP）

**两种导出模式**：

**模式 A — 单页 / 批量 Markdown 导出**
- 从 Wiki 详情页导出单个 `.md` 文件（含 frontmatter）
- 从 Operations / Settings 批量导出所有页面为 `.zip`（Markdown 目录）

**模式 B — 静态 HTML 包**
- 导出全部 Wiki 为可离线阅读的静态 HTML 包（`.zip`）
- 每个页面一个 `.html`，Markdown 渲染为 HTML（用 `pulldown-cmark`）
- 包含一个 `index.html` 列出所有页面（标题 + 摘要）
- 内链 `[[page]]` 转为相对路径 `<a href="page.html">`

**排除**：Hugo/Jekyll 主题定制、在线发布（v1 范围外）

### 技术方案

**后端（Rust）**：
- `src-tauri/src/state/wiki_service.rs` 新增 `export_wiki_zip`：
  - 遍历 `vault/wiki/` 下所有 `.md` 文件
  - 按选项生成 markdown-only zip 或 html zip
  - 写临时文件后通过 `tauri-plugin-dialog` 让用户选保存路径
- 依赖：`zip = "0.6"` crate（已有或需加），`pulldown-cmark`（已有）

**前端（React）**：
- Operations 模块增加「导出」Tab
- 两个按钮：「导出为 Markdown 包」/ 「导出为静态 HTML 包」
- 进度提示（页面总数）+ 完成通知

**新增 Tauri 命令**：
```rust
export_wiki_markdown_zip(dest_path: String) -> Result<u32, String>  // 返回页面数
export_wiki_html_zip(dest_path: String) -> Result<u32, String>
```

### 并行分工
- **Phase A（后端）**：zip 生成逻辑 + 两个 Tauri 命令 + 测试
- **Phase B（前端）**：Operations 导出 Tab + 按钮 + 进度展示

Phase A / B 可全并行（接口已定义）。预计 2 天。

### 文件清单
```
修改（后端）：
  src-tauri/src/state/wiki_service.rs（+export_wiki_zip 逻辑）
  src-tauri/src/commands.rs（+2 个命令）
  src-tauri/src/main.rs（注册命令）
  src-tauri/Cargo.toml（+zip crate，如不存在）

修改（前端）：
  web/src/modules/operations/OperationsModule.tsx（新增导出 Tab）
  web/src/tauri-client/（新增导出函数）
  web/src/types.ts（如需）
```

### 验收标准
- 点击「导出 Markdown 包」生成 `.zip`，解压后每个 `.md` 内容完整
- 点击「导出静态 HTML 包」生成 `.zip`，`index.html` 列出所有页面，页面内容可在浏览器离线阅读
- 内链 `[[title]]` 在 HTML 包中转为可点击链接
- 268 测试通过 + typecheck 零错误

---

## H23：Wiki 内联 AI 辅助编辑

### 价值
当前 Wiki 编辑器是纯文本 textarea。AI 存在于"生成阶段"（Agent / Ask），  
而编辑阶段（人工修订时）完全没有 AI 参与。  
内联 AI 辅助让编辑行为本身变成"人机协作"：  
续写、改写、扩写、段落摘要——不打断编辑节奏。

### 功能范围（MVP）

**三个内联动作**（通过选中文本后浮动工具栏触发）：
1. **续写**：光标位置 → AI 补充接下来的句子/段落（用当前页面标题 + 上下文作 prompt）
2. **改写**：选中文字 → AI 提供更简洁/更专业的改写版本（inline diff 预览）
3. **扩写**：选中要点 → AI 将 bullet 扩展为段落

**触发方式**：
- 选中文字后出现小工具栏（类似 Notion AI 风格）
- 或快捷键：`Ctrl+Shift+Space`（跳出 AI 菜单）

**输出**：流式渲染到内联预览区，用户点"接受"才写入编辑器

### 技术方案

**后端（Rust）**：
- `src-tauri/src/state/wiki_service.rs` 新增 `ai_assist_wiki_edit`：
  ```rust
  pub async fn ai_assist_wiki_edit(
      app_handle: AppHandle,
      action: &str,        // "continue" | "rewrite" | "expand"
      selected_text: &str,
      context: &str,       // 前后各 500 字
      page_title: &str,
  ) -> Result<(), String>  // 流式 emit "ai_assist_chunk" 事件
  ```
- 复用现有 LLM provider / streaming 基础设施（已有 `emit` 流式框架）

**前端（React）**：
- Wiki 编辑器内追加 `SelectionToolbar` 浮动组件
- 监听 `mouseup` + `selectionchange` 事件，定位工具栏
- 订阅 `ai_assist_chunk` 事件，内联预览区流式渲染
- "接受" / "拒绝" 按钮

**新增 Tauri 命令**：
```rust
ai_assist_wiki_edit(action, selected_text, context, page_title) -> Result<(), String>
```
（实际内容由流式事件 `ai_assist_chunk` / `ai_assist_done` 传递）

### 并行分工
- **Phase A（后端）**：prompt 构建 + LLM 调用 + 流式 emit + Tauri 命令
- **Phase B（前端）**：SelectionToolbar 组件 + 流式预览区 + 接受/拒绝

Phase A / B 可并行（事件名称与类型在计划阶段约定）。预计 3 天。

### 文件清单
```
新增（前端）：
  web/src/modules/wiki/SelectionToolbar.tsx
  web/src/modules/wiki/AiAssistPreview.tsx（内联预览组件）

修改（后端）：
  src-tauri/src/state/wiki_service.rs（+ai_assist_wiki_edit）
  src-tauri/src/commands.rs（+命令）
  src-tauri/src/main.rs（注册）

修改（前端）：
  web/src/modules/wiki/WikiModule.tsx（集成工具栏 + 预览区）
  web/src/tauri-client/（新增 ai_assist 函数）
  web/src/types.ts（新增事件类型）
```

### 验收标准
- 选中 Wiki 编辑器内文字，出现浮动工具栏（续写/改写/扩写）
- 点击任意操作后流式展示 AI 响应
- 点"接受"将响应插入编辑器，"拒绝"关闭预览
- LLM provider 不可用时给出友好提示，不崩溃
- 268 测试通过 + typecheck 零错误

---

## 执行顺序 & 并行策略

```
优先级  任务                         预计工时  并行可行
────────────────────────────────────────────────────
Pre     修复 search.ts 5处超时        0.5天    单线程
Pre     更新 dev-status.md           0.5天    单线程
────────────────────────────────────────────────────
H21A    CommandPalette 组件           1天      ─
H21B    App.tsx 集成 + Context        0.5天    串行于A后
────────────────────────────────────────────────────
H22A    后端 zip 导出逻辑 + 命令      1天      ┐ 并行
H22B    前端 Operations 导出 Tab      1天      ┘
────────────────────────────────────────────────────
H23A    后端 AI 辅助命令 + 流式 emit  1.5天    ┐ 并行
H23B    前端 SelectionToolbar        1.5天    ┘
────────────────────────────────────────────────────
```

**MCP & Skills 使用**：
- H21 搜索防抖：复用 `search_wiki_pages` Tauri 命令，不需要 MCP
- H23 prompt 设计：可使用 `写作助手` / `内容摘要` skill 作为参考模板
- context7 MCP：查阅 `pulldown-cmark` crate API（H22 HTML 渲染）

---

## 风险

| 风险 | H21 | H22 | H23 |
|---|---|---|---|
| 性能 | 搜索 300ms 防抖足够 | zip 大 vault 可能慢 → 后台异步进度 | 流式延迟感知 → 占位动画 |
| 兼容 | 无 | zip crate 需 Windows 路径处理 | LLM provider 必须可配置 |
| 范围蔓延 | 不做多步命令链 | 不做在线发布 | 不做全文 AI 重写 |
