# H21–H24 完整实施计划

> 起草：2026-05-17 | 目标：清零差距清单全部剩余缺口 + 新增产品价值  
> 覆盖：Pre（可靠性修复）/ H21（命令面板）/ H22（知识导出）/ H23（内联AI辅助）/ H24（多Vault/模板）

---

## 执行总览

```
阶段     任务                      估时    并行策略
──────────────────────────────────────────────────────
Pre      search.ts 超时修复         0.5天  单线程（小）
H21      全局命令面板 Ctrl+K        1.5天  串行（纯前端）
H22A     Wiki导出后端               1天    ┐ 并行
H22B     Wiki导出前端               1天    ┘
H23A     内联AI辅助后端             1.5天  ┐ 并行
H23B     内联AI辅助前端             1.5天  ┘
H24A     多Vault后端（命令+DB）     2天    ┐ 并行
H24B     多Vault前端（UI+流程）     2天    ┘
──────────────────────────────────────────────────────
合计                               ~7天（并行后约5天）
```

---

## Pre：search.ts 可靠性修复

### 问题
`web/src/tauri-client/search.ts` 中 6 处裸 `invoke()` 无 `withTimeout` 包装，长时 Tauri 进程阻塞时 UI 永远等待。

### 修复点（全部在 search.ts）
| 行号 | 命令 | 建议超时 |
|------|------|---------|
| 151 | `cancel_ask_session` | 10_000 |
| 166 | `clear_ask_session` | 10_000 |
| 249 | `rename_ask_session` | 10_000 |
| 263 | `delete_ask_session` | 10_000 |
| 306 | `save_ask_history` | 10_000 |
| 396 | `approve_research_queries` | 30_000 |

### 修改方式
```ts
// Before
await invoke("cancel_ask_session", { sessionId });

// After
await withTimeout(invoke("cancel_ask_session", { sessionId }), 10_000);
```

### 验收
- `npm run typecheck` 零错误
- 6 处全部包装

---

## H21：全局命令面板（Command Palette）

### 价值
当前所有操作依赖侧边栏点击。Wiki 页面增多后，查找与跳转摩擦明显。  
Ctrl+K 命令面板是个人知识工具中密度最高的 UX 升级：一次按键完成跳转、搜索、执行。

### 功能范围

**触发**：全局 `Ctrl+K`（任意模块），`Esc` 关闭，点击遮罩关闭。

**三类结果**（显示图标区分类型）：
1. **Wiki 页面**（📄）：实时搜索 `search_wiki_pages`，前 8 条，300ms 防抖
2. **操作命令**（⚡）：固定列表（新建页面 / 开始研究 / 打开 MCP 市场 / 切换模块…）
3. **最近访问**（🕐）：localStorage 存最近 5 个打开页面，输入框空时优先展示

**输入前缀过滤**：
- 无前缀：混合结果（页面 + 最近）
- `>` 前缀：只显示命令
- `#标签名`：按 entity 标签过滤页面

**键盘**：`↑↓` 选择，`Enter` 执行，`Esc` 关闭

### 技术方案

**新增文件**
```
web/src/modules/palette/CommandPalette.tsx
web/src/modules/palette/palette.css
web/src/contexts/CommandPaletteContext.tsx
```

**修改文件**
```
web/src/App.tsx — 挂载 <CommandPalette />，绑定全局 keydown Ctrl+K
```

**API 依赖**：复用现有 `search_wiki_pages` Tauri 命令，无需新增后端。

**CommandPaletteContext 接口**
```ts
type CommandPaletteContextValue = {
  open: boolean;
  setOpen: (v: boolean) => void;
  recentPages: RecentPage[];  // {path, title}
  pushRecentPage: (page: RecentPage) => void;
};
```

**CommandPalette 组件核心逻辑**
```tsx
// 搜索防抖 300ms
// 结果分组：最近访问 → Wiki 页面 → 操作命令
// activeIndex 键盘控制
// Ctrl+K 全局监听（App.tsx 层注册，避免 modal/input 内触发时冲突）
```

**内置命令列表**（硬编码）
```ts
const BUILTIN_COMMANDS = [
  { id: "new-wiki", label: "新建 Wiki 页面", icon: "📝", action: () => navigate("wiki", {mode: "new"}) },
  { id: "start-research", label: "开始深度研究", icon: "🔬", action: () => navigate("research") },
  { id: "open-mcp", label: "MCP 市场", icon: "🔌", action: () => navigate("discovery") },
  { id: "open-settings", label: "打开设置", icon: "⚙️", action: () => navigate("settings") },
  { id: "open-graph", label: "知识图谱", icon: "🕸️", action: () => navigate("graph") },
  { id: "open-lint", label: "知识 Lint", icon: "🔍", action: () => navigate("lint") },
];
```

### 验收标准
- Ctrl+K 在 6 个模块下均可弹出面板
- 输入后 300ms 内展示 Wiki 搜索结果
- 键盘完整可用，Enter 正确跳转
- `npm run typecheck` 零错误，268 测试通过

---

## H22：Wiki 知识导出

### 价值
知识进得去，也要出得来。目前 Wiki 内容只能在 App 内查看，无法分享或存档。  
完成"知识构建→使用→输出"的完整闭环。

### 功能范围

**模式 A — Markdown 包（.zip）**
- 所有 Wiki 页面的原始 `.md` 文件（含 frontmatter）
- 按 vault/wiki/ 目录结构保留层级
- 包含 index.md / log.md

**模式 B — 静态 HTML 包（.zip）**
- 每个 `.md` 生成对应 `.html`（用 `pulldown-cmark` 渲染）
- 生成 `index.html` 列出所有页面（标题 + 摘要 + 更新时间）
- `[[wiki-link]]` 转为相对路径 `<a href="page-slug.html">`
- 内嵌最小 CSS（可读，不依赖外部 CDN）

### 技术方案（后端）

**依赖检查**：`zip` crate 是否已在 Cargo.toml。若无，添加 `zip = "0.6"`。

**新增到 `src-tauri/src/state/wiki_service.rs`**
```rust
pub fn export_wiki_markdown_zip(
    db_path: &PathBuf,
    vault_path: &PathBuf,
    dest_path: &str,        // 用户选择的保存路径
) -> Result<u32, String>   // 返回导出页面数

pub fn export_wiki_html_zip(
    db_path: &PathBuf,
    vault_path: &PathBuf,
    dest_path: &str,
) -> Result<u32, String>
```

**内部逻辑**
```rust
// 1. 遍历 vault/wiki/**/*.md
// 2. Markdown export：直接 read_to_string + zip write
// 3. HTML export：pulldown-cmark 渲染 + wiki_link 替换 + 包装 HTML 模板
// 4. 写入 zip，返回文件数
```

**新增 Tauri 命令（commands.rs）**
```rust
#[tauri::command]
pub async fn export_wiki_markdown_zip(dest_path: String, state: State<'_, AppState>) -> Result<u32, String>

#[tauri::command]
pub async fn export_wiki_html_zip(dest_path: String, state: State<'_, AppState>) -> Result<u32, String>
```

**文件变更（后端）**
```
修改：src-tauri/src/state/wiki_service.rs（+2 个 export 函数）
修改：src-tauri/src/commands.rs（+2 个命令）
修改：src-tauri/src/main.rs（注册命令）
修改：src-tauri/Cargo.toml（+zip crate，如不存在）
```

### 技术方案（前端）

**位置**：Operations 模块新增"导出"Tab（与"队列"/"统计"并列）。

**UI 结构**
```
[Operations]
  ├─ 队列
  ├─ 统计
  └─ 导出（新增）
       ├─ [导出为 Markdown 包]  [导出为静态 HTML 包]
       └─ 上次导出：XXX 页面，xxx.zip（可选历史）
```

**交互**：点击按钮 → `save` 文件对话框选路径 → 调 Tauri 命令 → 进度提示 → 完成通知。

**文件变更（前端）**
```
修改：web/src/modules/operations/OperationsModule.tsx（+导出 Tab）
新增：web/src/tauri-client/export.ts（2 个函数）
修改：web/src/types.ts（如需）
```

### 验收标准
- `.zip` 解压后 Markdown 包含完整 frontmatter，目录结构与 vault 一致
- HTML 包在浏览器离线可读，`[[链接]]` 已转为可点击 `<a>` 标签
- `index.html` 列出所有页面
- 268 测试通过，typecheck 零错误

---

## H23：Wiki 内联 AI 辅助编辑

### 价值
当前 Wiki 编辑器是纯 textarea，AI 只参与"生成阶段"。  
内联 AI 辅助把 AI 引入"编辑阶段"，让修订过程本身变成人机协作。

### 功能范围

**三个动作**（选中文字后浮动工具栏触发）：
1. **续写**：在光标/选中末尾续写 1-3 段
2. **改写**：更简洁或更专业地改写选中内容
3. **扩写**：将 bullet/要点扩展为完整段落

**触发方式**：
- 选中文字后出现浮动工具栏（mouseup 事件，定位到选区上方）
- 工具栏三个按钮：续写 / 改写 / 扩写

**输出方式**：
- 行内预览区（textarea 下方插入预览框，流式渲染）
- 用户点"接受"→ 插入/替换编辑器内容；"拒绝"→ 关闭预览

### 技术方案（后端）

**新增到 `src-tauri/src/state/wiki_service.rs`**
```rust
pub async fn ai_assist_wiki_edit(
    app_handle: &AppHandle,
    action: &str,           // "continue" | "rewrite" | "expand"
    selected_text: &str,    // 选中内容
    context_before: &str,   // 光标前 500 字
    context_after: &str,    // 光标后 200 字
    page_title: &str,
    state: &AppState,
) -> Result<(), String>     // 实际内容通过流式事件传递
```

**事件**（Tauri emit）
```
ai_assist_chunk  → { text: String }          // 流式 token
ai_assist_done   → { action: String }        // 完成
ai_assist_error  → { message: String }       // 错误
```

**Prompt 模板**
```
续写：你是一个知识写作助手，正在编辑《{page_title}》。
      前文：{context_before}
      请在以下内容之后续写 1-3 段（风格一致，不重复前文）：
      {selected_text}

改写：将以下内容改写得更简洁专业（保留核心信息，减少冗余）：
      {selected_text}

扩写：将以下要点扩展为完整段落（逻辑清晰，每点 2-4 句）：
      {selected_text}
```

**文件变更（后端）**
```
修改：src-tauri/src/state/wiki_service.rs（+ai_assist_wiki_edit）
修改：src-tauri/src/commands.rs（+命令）
修改：src-tauri/src/main.rs（注册命令）
```

### 技术方案（前端）

**新增组件**
```
web/src/modules/wiki/SelectionToolbar.tsx   — 浮动工具栏
web/src/modules/wiki/AiAssistPreview.tsx    — 流式预览区
```

**SelectionToolbar 逻辑**
```tsx
// 监听 mouseup，window.getSelection() 检测选区
// 计算 getBoundingClientRect() 定位到选区上方
// 三个按钮触发 onAction("continue" | "rewrite" | "expand")
// 选区消失或点击外部时隐藏
```

**AiAssistPreview 逻辑**
```tsx
// 订阅 ai_assist_chunk/done/error 事件
// 流式追加到 previewText state
// 接受：onAccept(previewText) → 调用方替换编辑器内容
// 拒绝：关闭预览，取消订阅
```

**WikiModule 集成**
```tsx
// 编辑态下挂载 SelectionToolbar 和 AiAssistPreview
// handleAiAction(action, selectedText, cursorContext)
//   → invoke ai_assist_wiki_edit
//   → 打开 AiAssistPreview
// handleAiAccept(text) → 替换 textarea 选中区域
```

**文件变更（前端）**
```
新增：web/src/modules/wiki/SelectionToolbar.tsx
新增：web/src/modules/wiki/AiAssistPreview.tsx
修改：web/src/modules/wiki/WikiModule.tsx（集成）
修改：web/src/tauri-client/（新增 ai_assist 函数）
修改：web/src/types.ts（新增事件类型）
```

### 验收标准
- 编辑模式下选中文字，出现工具栏（续写/改写/扩写）
- 点击任意操作后流式展示 AI 响应
- "接受"正确替换编辑器内容，"拒绝"关闭预览
- LLM 不可用时给出友好提示
- 268 测试通过，typecheck 零错误

---

## H24：多 Vault / 项目模板

### 价值
当前 App 每次只能操作一个 Vault，且切换需要重启。  
多 Vault 支持 + 项目模板让工具可以管理多个知识库（工作/个人/研究），  
对标参考项目的"模板化项目创建"能力，完全闭合差距清单。

### 功能范围

**多 Vault 管理**：
- 最近使用的 Vault 列表（持久化到 app 数据目录）
- 菜单/按钮"切换 Vault"（关闭当前，打开新路径）
- 欢迎页：无 Vault 时展示最近列表 + "打开已有" + "新建"

**项目模板**（新建 Vault 时选择）：
1. **个人知识库**：空 vault，附入门说明
2. **研究项目**：预建 `research/`、`notes/`、`references/` 目录 + 专用 index.md
3. **技术文档**：预建 `architecture/`、`api/`、`guides/` + 技术专用 index.md
4. **自定义空白**：只建基础 index.md / log.md

### 技术方案（后端）

**最近 Vault 列表持久化**
```rust
// 存储路径：tauri app_local_data_dir / "vaults.json"
// 结构：Vec<RecentVault> = [{path, name, last_opened_at}]
// 最多保留 10 条，按 last_opened_at 排序
```

**新增 Tauri 命令**
```rust
list_recent_vaults() -> Result<Vec<RecentVaultItem>, String>
open_recent_vault(path: String) -> Result<(), String>
// 注：open_recent_vault 触发 AppState 重新初始化（关闭当前 DB，打开新路径）
// 发出 "vault_changed" Tauri 事件，前端重置状态

create_vault_from_template(path: String, template: String) -> Result<(), String>
// template: "personal" | "research" | "technical" | "blank"
// 创建目录结构 + 写入模板文件

remove_recent_vault(path: String) -> Result<(), String>
```

**模板文件内容（嵌入 Rust 源码，`include_str!` 宏）**
```
templates/personal/index.md    — 个人知识库首页模板
templates/personal/log.md
templates/research/index.md    — 研究项目模板
templates/research/log.md
templates/technical/index.md   — 技术文档模板
templates/technical/log.md
```

**AppState vault 切换**
```rust
// 新增 AppState::reinitialize_vault(new_path: PathBuf)
//   1. 关闭现有 DB 连接（drop RwLock 内容）
//   2. 初始化新路径（复用 initialize_vault 内部逻辑）
//   3. emit "vault_changed" 事件到前端
```

**文件变更（后端）**
```
修改：src-tauri/src/state/config_service.rs（+最近Vault CRUD）
修改：src-tauri/src/state/wiki_service.rs 或新增 vault_template.rs（模板写入）
修改：src-tauri/src/commands.rs（+4个命令）
修改：src-tauri/src/main.rs（注册）
新增：src-tauri/templates/（4套模板文件）
```

### 技术方案（前端）

**欢迎页（无 Vault 时展示，替换现有 SetupVault 组件）**
```tsx
// 最近 Vault 列表（listRecentVaults）
// 打开已有按钮（tauri-plugin-dialog 选目录）
// 新建按钮 → 模板选择对话框
```

**模板选择对话框**
```tsx
// 4 个卡片：个人知识库 / 研究项目 / 技术文档 / 自定义空白
// 每个卡片：icon + 名称 + 简述 + 目录预览
// 选择 + 输入 Vault 名称/路径 → createVaultFromTemplate
```

**Vault 切换（已有 Vault 状态下）**
```tsx
// App.tsx 顶部/设置中：当前 Vault 名称 + "切换"按钮
// 点击切换：展示最近列表 + 打开已有 + 新建
// 切换完成后监听 "vault_changed" 事件，重置所有模块状态
```

**SettingsModule 增加 Vault 管理区**
```tsx
// 显示当前 Vault 路径
// 最近 Vault 列表（可删除条目）
// 打开已有 / 新建 Vault 入口
```

**文件变更（前端）**
```
修改：web/src/App.tsx（监听 vault_changed 事件，重置状态）
修改：web/src/modules/settings/SettingsModule.tsx（+Vault 管理区）
新增：web/src/modules/welcome/WelcomeModule.tsx（欢迎/无Vault页）
新增：web/src/modules/welcome/VaultTemplateDialog.tsx（模板选择对话框）
修改：web/src/tauri-client/vault.ts 或新增
修改：web/src/types.ts（RecentVaultItem 类型）
```

### 验收标准
- 无 Vault 时显示欢迎页，含最近列表和新建入口
- 选择模板创建 Vault 后，目录结构和 index.md 符合模板
- 切换 Vault 后模块状态完全重置（搜索结果/历史/图谱全清）
- 最近 Vault 列表持久化（重启后保留）
- 268+ 测试通过，typecheck 零错误

---

## 并行子代理分工规则

### H22 并行（Phase A + Phase B 同时开工）

**Phase A（后端）拥有文件**：
```
src-tauri/src/state/wiki_service.rs
src-tauri/src/commands.rs
src-tauri/src/main.rs
src-tauri/Cargo.toml
```

**Phase B（前端）拥有文件**：
```
web/src/modules/operations/OperationsModule.tsx
web/src/tauri-client/export.ts（新增）
web/src/types.ts
```

**接口约定（并行开工前已定义）**：
```ts
// export.ts
export async function exportWikiMarkdownZip(destPath: string): Promise<number>
export async function exportWikiHtmlZip(destPath: string): Promise<number>
// 返回导出页面数
```

### H23 并行（Phase A + Phase B 同时开工）

**Phase A（后端）拥有文件**：
```
src-tauri/src/state/wiki_service.rs
src-tauri/src/commands.rs
src-tauri/src/main.rs
```

**Phase B（前端）拥有文件**：
```
web/src/modules/wiki/SelectionToolbar.tsx（新增）
web/src/modules/wiki/AiAssistPreview.tsx（新增）
web/src/modules/wiki/WikiModule.tsx
web/src/tauri-client/（新增 ai_assist 函数）
web/src/types.ts
```

**接口约定**：
```ts
// tauri-client
export async function aiAssistWikiEdit(
  action: "continue" | "rewrite" | "expand",
  selectedText: string,
  contextBefore: string,
  contextAfter: string,
  pageTitle: string,
): Promise<void>  // 内容通过事件传递

// 事件名称
const AI_ASSIST_CHUNK = "ai_assist_chunk";    // { text: string }
const AI_ASSIST_DONE  = "ai_assist_done";     // { action: string }
const AI_ASSIST_ERROR = "ai_assist_error";    // { message: string }
```

### H24 并行（Phase A + Phase B 同时开工）

**Phase A（后端）拥有文件**：
```
src-tauri/src/state/config_service.rs
src-tauri/src/commands.rs
src-tauri/src/main.rs
src-tauri/templates/（新增目录）
```

**Phase B（前端）拥有文件**：
```
web/src/modules/welcome/WelcomeModule.tsx（新增）
web/src/modules/welcome/VaultTemplateDialog.tsx（新增）
web/src/modules/settings/SettingsModule.tsx
web/src/tauri-client/vault.ts（新增或修改）
web/src/types.ts
web/src/App.tsx（仅 vault_changed 监听，不大改）
```

**接口约定**：
```ts
export async function listRecentVaults(): Promise<RecentVaultItem[]>
export async function openRecentVault(path: string): Promise<void>
export async function createVaultFromTemplate(path: string, template: "personal"|"research"|"technical"|"blank"): Promise<void>
export async function removeRecentVault(path: string): Promise<void>

type RecentVaultItem = {
  path: string;
  name: string;
  last_opened_at: string;  // ISO 8601
};
```

---

## 测试要求

| 阶段 | 必须自动化 | 可手测 |
|------|-----------|--------|
| Pre | — | 6 处超时包装确认 |
| H21 | — | Ctrl+K 弹出，搜索，回车跳转 |
| H22 | Rust: 导出函数单测（临时目录，验证 zip 结构） | 解压检查 HTML/MD 内容 |
| H23 | Rust: prompt 构建单测 | 实际 LLM 流式响应端到端 |
| H24 | Rust: 模板写入单测（目录结构）、最近Vault CRUD | 创建→切换→重置 端到端 |

---

## 里程碑 & 差距清单最终状态

完成 Pre + H21 + H22 + H23 + H24 后，差距清单所有项均为 ✅：

| 清单项 | H | 状态 |
|--------|---|------|
| 摄入任务编排 | P23 | ✅ |
| 两步摄入质量 | P27 | ✅ |
| 图谱能力 | P24 | ✅ |
| 查询检索策略 | P20-5 | ✅ |
| Deep Research | P26 | ✅ |
| 审核流（HITL） | P27 | ✅ |
| 外部采集入口 | P23 | ✅ |
| 会话持久化 | H8 | ✅ |
| 安全与治理 | — | ✅ 我方优势 |
| 多项目体验 | **H24** | 本计划 |
| 全局导航效率 | **H21** | 本计划 |
| 知识导出 | **H22** | 本计划 |
| 内联AI编辑 | **H23** | 本计划 |
| search.ts 可靠性 | **Pre** | 本计划 |
