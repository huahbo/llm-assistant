# H10：MCP 客户端集成实施计划

> 状态：待实施 | 优先级：中（H11 Swarm 的前置条件）
> 依赖：无（但 Phase B 依赖 Phase A 完成）

---

## 1. 目标

实现 MCP（Model Context Protocol）客户端，让 llm-wiki 的 Agent 能够动态加载和调用外部 MCP Server 提供的工具，同时将内置工具系统从 `enum AgentToolAction` 重构为 `Tool` trait + `ToolRegistry` 动态注册机制。

```
用户在 Settings 中配置 MCP Server（本地路径/命令）
  ↓
llm-wiki 后端启动 MCP Server 子进程（stdio transport）
  ↓
通过 JSON-RPC 2.0 发现工具列表
  ↓
包装为 McpTool（impl Tool trait），注册到 ToolRegistry
  ↓
Agent loop 调用 ToolRegistry 统一分派（内置 + MCP 工具无区别）
```

---

## 2. 技术背景

### 2.1 MCP 协议简述

Model Context Protocol（Anthropic 开源）是 JSON-RPC 2.0 over stdio 的工具发现/调用协议：

```jsonc
// 工具发现请求
{ "jsonrpc": "2.0", "method": "tools/list", "id": 1 }

// 响应
{ "result": { "tools": [{ "name": "...", "description": "...", "inputSchema": {...} }] } }

// 工具调用
{ "jsonrpc": "2.0", "method": "tools/call", "params": { "name": "...", "arguments": {...} }, "id": 2 }
```

### 2.2 jcode 参考

jcode 有完整 MCP client 实现（MIT License），关键设计：
- 每个 MCP Server 一个独立子进程，通过 `Child.stdin`/`Child.stdout` 通信
- 工具发现结果缓存（进程生命周期内）
- McpTool 包装器将 `tools/call` 响应的内容字段作为工具输出返回
- Server 崩溃时自动重启（最多 3 次）

### 2.3 现有 AgentToolAction 枚举（需重构）

```rust
// src-tauri/src/agent_tools.rs（当前）
pub enum AgentToolAction {
    ReadWiki { path: String },
    WriteWiki { path: String, content: String },
    EditWiki { path: String, old_str: String, new_str: String },
    SearchWiki { query: String },
    RunShell { command: String },
    WebSearch { query: String },
}
```

---

## 3. 实施方案

### Phase A：Tool trait + ToolRegistry（内部重构）

**目的**：将 `AgentToolAction` 枚举重构为动态 trait 对象，为 MCP 工具注入奠定基础。

#### 3A.1 Tool trait 定义

新建 `src-tauri/src/agent/tool.rs`：

```rust
use serde_json::Value;
use async_trait::async_trait;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;  // JSON Schema，喂给 LLM function calling
    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError>;
}

pub struct ToolOutput {
    pub content: String,
    pub pending_approval: Option<u64>,  // 需要审批时携带 pending_id
}

pub struct ToolContext {
    pub state: Arc<AppState>,
    pub session_id: String,
    pub vault_path: PathBuf,
}
```

#### 3A.2 现有工具迁移

将现有 6 个枚举值逐一实现为 `struct XxxTool(impl Tool)` 并注册：

```rust
// src-tauri/src/agent/tools/read_wiki.rs
pub struct ReadWikiTool;
#[async_trait]
impl Tool for ReadWikiTool {
    fn name(&self) -> &str { "read_wiki" }
    // ...
}
```

#### 3A.3 ToolRegistry

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }
    pub fn get_schemas(&self) -> Vec<Value> {
        self.tools.values()
            .map(|t| json!({ "name": t.name(), "description": t.description(), "parameters": t.parameters_schema() }))
            .collect()
    }
    pub async fn dispatch(&self, name: &str, params: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        self.tools.get(name)
            .ok_or(ToolError::UnknownTool(name.to_string()))?
            .execute(params, ctx)
            .await
    }
}
```

#### 3A.4 Agent Loop 接入 ToolRegistry

`src-tauri/src/agent_chat/runtime.rs` 中替换 `AgentToolAction` match 分支为：

```rust
let output = registry.dispatch(&tool_name, params, &ctx).await?;
```

**向后兼容**：Phase A 完成后，内置工具行为与重构前完全一致，只是内部实现从 enum 变为 trait。

---

### Phase B：MCP Client 实现

#### 3B.1 MCP Server 配置

Settings 中新增"MCP 服务器"配置区：

```json
// 存储在 config.json 中
{
  "mcp_servers": [
    {
      "name": "filesystem",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"],
      "env": {}
    },
    {
      "name": "custom-db",
      "command": "C:\\tools\\my-mcp-server.exe",
      "args": [],
      "env": { "DB_URL": "..." }
    }
  ]
}
```

#### 3B.2 McpClient

新建 `src-tauri/src/agent/mcp_client.rs`：

```rust
pub struct McpClient {
    name: String,
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    next_id: AtomicU64,
    tools_cache: OnceCell<Vec<McpToolDef>>,
}

impl McpClient {
    pub async fn spawn(config: &McpServerConfig) -> Result<Self> {
        // Command::new(&config.command).args(&config.args).spawn()
    }
    pub async fn list_tools(&self) -> Result<Vec<McpToolDef>> { ... }
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<String> { ... }
    fn write_request(&mut self, method: &str, params: Value) -> Result<u64> { ... }
    fn read_response(&mut self, id: u64) -> Result<Value> { ... }
}
```

#### 3B.3 McpTool 包装器

```rust
pub struct McpTool {
    def: McpToolDef,
    client: Arc<Mutex<McpClient>>,
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str { &self.def.name }
    fn description(&self) -> &str { &self.def.description }
    fn parameters_schema(&self) -> Value { self.def.input_schema.clone() }
    async fn execute(&self, params: Value, _ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        let content = self.client.lock().await.call_tool(&self.def.name, params).await?;
        Ok(ToolOutput { content, pending_approval: None })
    }
}
```

#### 3B.4 启动时注册 MCP 工具

在 `AppState::new()` 或 Agent Session 初始化时：

```rust
for server_config in &config.mcp_servers {
    match McpClient::spawn(server_config).await {
        Ok(client) => {
            let client = Arc::new(Mutex::new(client));
            for tool_def in client.lock().await.list_tools().await? {
                registry.register(Arc::new(McpTool { def: tool_def, client: client.clone() }));
            }
        }
        Err(e) => {
            tracing::warn!("Failed to start MCP server '{}': {e}", server_config.name);
            // 不阻塞启动，记录警告即可
        }
    }
}
```

---

### Phase C：前端配置 UI

#### 3C.1 设置页新增"MCP 服务器"配置区

```
[ MCP 服务器 ]
  + 添加服务器

  filesystem
  命令: npx -y @modelcontextprotocol/server-filesystem /tmp
  状态: ● 运行中，3 个工具已加载
  [ 编辑 ] [ 删除 ]

  custom-db
  命令: C:\tools\my-mcp-server.exe
  状态: ○ 未启动（错误：文件不存在）
  [ 编辑 ] [ 删除 ]
```

#### 3C.2 Agent Studio 工具列表展示

在 AgentToolsPane 中区分内置工具 vs MCP 工具，MCP 工具显示来源服务器名称。

---

## 4. 文件变动清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `src-tauri/src/agent/tool.rs` | 新建 | Tool trait, ToolOutput, ToolContext |
| `src-tauri/src/agent/registry.rs` | 新建 | ToolRegistry |
| `src-tauri/src/agent/tools/` | 新建目录 | 6 个内置工具各一个文件 |
| `src-tauri/src/agent/mcp_client.rs` | 新建 | McpClient + McpTool |
| `src-tauri/src/agent_chat/runtime.rs` | 修改 | 接入 ToolRegistry |
| `src-tauri/src/agent_tools.rs` | 删除或保留空 | 逐步迁移后删除 |
| `src-tauri/src/commands.rs` | 修改 | 新增 MCP 管理命令 |
| `src-tauri/src/config.rs` | 修改 | `mcp_servers: Vec<McpServerConfig>` |
| `web/src/modules/settings/` | 修改 | MCP Server 配置 UI |
| `web/src/modules/agent/AgentToolsPane.tsx` | 修改 | 展示 MCP 工具来源 |
| `web/src/tauri-client/` | 修改 | MCP 配置相关命令 |

---

## 5. 验收标准

- [ ] Phase A：所有内置工具通过 ToolRegistry 调用，行为与重构前一致
- [ ] Phase A：`cargo test` 全绿（Agent 相关测试用例覆盖）
- [ ] Phase B：可在设置中添加 MCP server 配置
- [ ] Phase B：配置的 MCP server 在 Agent 会话中可调用（工具名出现在 function schema 中）
- [ ] Phase B：MCP server 不可用时，Agent 仍可启动（降级为仅内置工具）
- [ ] Phase C：Settings UI 显示 MCP server 状态（运行中/错误）
- [ ] `cargo test` 全绿；`npm run typecheck` 零错误

---

## 6. 风险与注意事项

1. **async_trait 在 dyn Trait 上的 Send + Sync 约束**：Tauri 的 `#[tauri::command]` 要求数据跨线程安全，所有 Tool 实现必须 `Send + Sync`，`async_trait` 库已处理，但需要确认 `McpClient` 中 `BufReader<ChildStdout>` 的 Send 性。
2. **MCP Server 进程生命周期**：子进程随 Tauri App 启动而启动，App 退出时需要优雅终止（`Child::kill()`）。在 `AppState::Drop` 中处理。
3. **Phase A 是高风险重构**：`AgentToolAction` 枚举在代码中有多处 match。建议先写单元测试固定现有行为，再做重构，最后运行测试验证。
4. **Windows 下的 stdio 子进程**：`Command::new("npx")` 需要 Node.js 在 PATH 中。MCP server 配置错误应有友好的错误信息，不能导致 App 崩溃。

---

## 7. 工作量估算

| Phase | 估算 | 关键风险 |
|-------|------|---------|
| A（Tool trait + Registry + 内置工具迁移） | 3 天 | 大范围重构，需完善测试 |
| B（MCP Client + McpTool） | 2 天 | stdio 进程管理，JSON-RPC 序列化 |
| C（Settings UI） | 1 天 | 低风险 |
| **总计** | **~6 天** | Phase A 最高风险，建议分 PR |

---

## 8. 与 H11 的关系

H11（Agent Swarm）的 `spawn_subagent` 工具需要注册到 ToolRegistry 才能被 LLM 调用。因此 **H10 Phase A 是 H11 的硬性前置条件**。

建议执行顺序：
```
H10 Phase A（ToolRegistry 重构）
  ↓
H11 Phase A（Swarm 基础设施）
  ↓
H10 Phase B（MCP Client）  ← 可与 H11 Phase B 并行
```
