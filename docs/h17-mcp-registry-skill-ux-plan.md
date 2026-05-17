# H17 MCP Registry 安装 + Skill UX 改进 计划

## 背景

**当前痛点：**
- MCP server 只能手动填写 command/args，用户必须先查文档再手动配置
- Skill prompt 编辑区域小，从网上复制长 prompt 体验差，没有剪贴板快捷操作
- 删除 Skill 时存在 activeSkillKey 未同步重置的 bug

**目标：**
1. 集成 Smithery registry，支持搜索 + 一键安装 MCP server
2. 新增独立「发现」页面（Discovery），聚焦 MCP 市场，不放在 Settings 内
3. 重设计 Skill 编辑体验，修复删除 bug

**不做（范围限制）：**
- Skill marketplace（生态未成熟，本地 CRUD 够用）
- GitHub URL 直接解析安装（Smithery 已覆盖绝大多数场景）
- uvx/pipx（先只支持 npx；用户已安装 Node.js）

---

## 数据模型补充

### 新增 Rust 结构体（models.rs）

```rust
pub struct SmitheryServer {
    pub qualified_name: String,
    pub display_name: String,
    pub description: String,
    pub verified: bool,
    pub use_count: u64,
    pub icon_url: Option<String>,
}

pub struct SmitheryServerDetail {
    pub qualified_name: String,
    pub display_name: String,
    pub description: String,
    pub command: String,                   // 通常 "npx"
    pub args: Vec<String>,                 // e.g. ["-y", "@org/server-name"]
    pub required_env_keys: Vec<String>,    // 需要用户填写的 env key（如 "GITHUB_TOKEN"）
}
```

---

## Phase A：Smithery Registry 后端

**新文件** `src-tauri/src/agent_chat/registry.rs`

```rust
pub async fn search_smithery_servers(query: &str, page_size: u32)
    -> Result<Vec<SmitheryServer>, String>

pub async fn get_smithery_server_detail(qualified_name: &str)
    -> Result<SmitheryServerDetail, String>
```

**Smithery API（公开，无需 auth）：**
- Search：`GET https://registry.smithery.ai/servers?q={query}&pageSize={n}`
- Detail：`GET https://registry.smithery.ai/servers/{qualifiedName}`

**命令生成规则：**
- 优先解析 Smithery `connections[0]` 中的 `command` / `args`
- 回退：`command = "npx"`，`args = ["-y", qualified_name]`
- `required_env_keys`：从 `configSchema` 中提取 `required` 字段的 env key 名

**新 Tauri 命令**（`src-tauri/src/agent_chat/commands.rs`）：

```rust
#[tauri::command]
pub async fn search_mcp_registry(query: String, page_size: Option<u32>)
    -> Result<Vec<SmitheryServer>, String>

#[tauri::command]
pub async fn get_mcp_registry_server(qualified_name: String)
    -> Result<SmitheryServerDetail, String>
```

注册到 `src-tauri/src/lib.rs` invoke_handler。

---

## Phase B：独立「发现」页面 + Env 编辑器（前端）

### B-1：新增 Discovery 模块

新文件 `web/src/modules/discovery/DiscoveryModule.tsx` + `discovery.css`

UI 布局：

```
┌─────────────────────────────────────────────────────┐
│  🔍 MCP 市场                                         │
│  [ 搜索 MCP Server... ]  [ 搜索 ]                    │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │ ✅ Filesystem  [已验证] 12.3k installs         │   │
│  │    Read and write files on the local system   │   │
│  │    @modelcontextprotocol/server-filesystem    │   │
│  │                              [ 安装 ]         │   │
│  ├──────────────────────────────────────────────┤   │
│  │    GitHub      8.1k installs                 │   │
│  │    GitHub API integration                    │   │
│  │    需要配置：GITHUB_TOKEN     [ 安装 ]         │   │
│  └──────────────────────────────────────────────┘   │
│                                                      │
│  已安装的 MCP Server                                  │
│  ┌──────────────────────────────────────────────┐   │
│  │ filesystem  npx -y @mcp/server-filesystem    │   │
│  │             [ 重载工具 ]  [ 删除 ]            │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

**主导航新增「发现」tab**（`App.tsx` / 侧边栏）。

**安装流程：**
1. 点"安装" → 调 `getMcpRegistryServer` 获取 command/args/required_env_keys
2. 若有 required_env_keys → 弹 EnvInputDialog（让用户填 API key）
3. 调 `upsertMcpServer` 保存
4. 自动 `reloadMcpServerTools` 验证连通性
5. 成功/失败 Toast + 更新"已安装"列表

### B-2：Env 键值对编辑器

已安装列表每行点"编辑" → 展开 env 键值对编辑区（key input + value input + 删除行 + 添加行）。

Settings 中手动添加表单同步增加 env 编辑区。

**`web/src/tauri-client/mcp.ts`** 新增：

```ts
export async function searchMcpRegistry(query: string, pageSize?: number): Promise<SmitheryServer[]>
export async function getMcpRegistryServer(qualifiedName: string): Promise<SmitheryServerDetail>
```

---

## Phase C：Skill UX 重设计 + 删除 Bug 修复

### C-0：删除 Bug 修复

**问题**：`handleDeleteAgentSkill` 删除 active skill 后，仅依赖 `loadAgentSkillsData` 的异步回调重置 `agentActiveSkillKey`。若 active skill 是被删除的那个，需在删除前**立即同步重置**，避免 localStorage 残留脏数据。

**修复**（`AgentStudio.tsx` `handleDeleteAgentSkill`）：

```ts
const handleDeleteAgentSkill = async (id: number, skillKey: string) => {
  setAgentActionRunning(true);
  // 删除前立即清除 active 状态，避免 localStorage 残留
  if (agentActiveSkillKey === skillKey) {
    setAgentActiveSkillKey("");
  }
  try {
    const ok = await deleteAgentSkill(id);
    if (!ok) { setAgentStatusMessage("删除技能模板失败。"); return; }
    setAgentStatusMessage(`技能模板「${skillKey}」已删除。`);
    await loadAgentSkillsData();
  } finally {
    setAgentActionRunning(false);
  }
};
```

### C-1：Skill 列表行改版

```
[  写作助手  v2  ]  你是一个知识写作专家……（前 80 字）
                          [ 📋 复制 ]  [ ✏️ 编辑 ]  [ 🗑️ 删除 ]
```

### C-2：Skill 编辑 Modal

- 宽度：`min(600px, 50vw)`，不超过屏幕一半
- 高度：自适应，textarea 最小 300px，可拖拽调整

```
┌─────── 编辑 Skill ───────────────────────┐
│  Skill Key:  [ 写作助手               ]  │
│                                          │
│  Prompt 模板：    [ 📋 从剪贴板粘贴 ]    │
│  ┌────────────────────────────────────┐  │
│  │                                    │  │
│  │  （textarea，min-height 300px，    │  │
│  │    resize: vertical）              │  │
│  │                                    │  │
│  └────────────────────────────────────┘  │
│                                          │
│  [ 📖 载入示例 ▼ ]                       │
│                      [ 取消 ] [ 保存 ]   │
└──────────────────────────────────────────┘
```

### C-3：内置示例（中文 key）

| Key | 说明 |
|-----|------|
| `写作助手` | 结构化知识写作，客观严谨，避免口语 |
| `代码审查` | 逐点分析安全/性能/可读性 |
| `内容摘要` | 要点提炼，保留关键数据 |
| `翻译优化` | 信达雅原则，保留专业术语 |
| `分析报告` | 分层结论，区分事实与推断 |

- 点击示例 → 填充 key + prompt（已有内容时提示确认覆盖）
- 用户可在 key 字段自由修改为英文或任意名称

### C-4：后端无需改动

---

## 实施顺序

| 阶段 | 内容 | 依赖 |
|------|------|------|
| **并行** A后端 + C前端 | registry.rs + Skill UX | 互不依赖 |
| **串行** B前端 | Discovery 页面 + env 编辑器 | 需要 A后端完成 |

---

## 验收标准

### MCP Discovery
- [ ] 搜索 "filesystem" 返回 Smithery 结果（含 verified 标识、install 数）
- [ ] 一键安装后 server 出现在已安装列表
- [ ] 需要 env 的 server 安装时弹出 env 填写对话框
- [ ] Reload 后可获取工具列表（连通性验证）
- [ ] env 键值对可编辑

### Skill UX
- [ ] 删除 active skill 后 activeSkillKey 立即清除，localStorage 无残留
- [ ] 编辑 Modal 宽度不超过屏幕一半
- [ ] textarea 可粘贴 1000+ 字符 prompt
- [ ] "从剪贴板粘贴"按钮正常工作
- [ ] 5 个中文 key 内置示例可一键载入
- [ ] 已有 skill 可点"编辑"打开 Modal 修改

### 质量
- [ ] `cargo test` 268 通过，0 失败
- [ ] `npm run typecheck` 零错误
- [ ] Smithery API 超时/网络失败有友好提示
