# H8 ReAct 流式对话 Agent 重构计划（终态 C）

> 创建日期：2026-05-08
> 设计模型：Opus 4.7（架构）
> 执行模型：Sonnet 4.6（增量提取 + 编码）
> 前置条件：H7 已完成（App.tsx < 500 行，cargo test 233 全过，typecheck 0 错误）
> 终态：完整对话 Agent — OpenAI function_calling + 独立对话模型 + 多会话 + 审批集成

---

## 0. 摘要（先看）

### 0.1 问题

当前 Agent Studio 左侧聊天区只触发"生成 wiki 草稿"任务，显示静态状态标签（生成中/待审阅/失败），不能理解自然语言指令（如"列出当前目录的文件"）也不能流式输出。

### 0.2 目标终态

完整 Schema C：
- **OpenAI function_calling 协议**：替代现有自定义 JSON-in-prompt
- **流式对话**：文本 token 和工具调用 deltas 同时流式
- **多轮持久化对话**：messages[] 模型，对话独立于 agent_runs
- **独立 ModuleId**：左侧导航新增 `chat`，与现有模块平行
- **审批集成**：write/edit 工具复用现有 PendingAgentWrite 链路
- **MCP 预留**：`agent_tools` 表预留 `handler_kind='mcp'` 字段（不实施 MCP 客户端）

### 0.3 关键架构决策（已拍板）

| # | 决策 | 选择 |
|---|------|------|
| 1 | UI 入口形式 | 新独立 ModuleId（`chat`），保留旧 Agent Studio 入口做归档 |
| 2 | 完成后是否删除任务模式 | 不删，作为可选快捷入口（以特定 system prompt 创建对话） |
| 3 | Ollama 不支持 tools 时降级 | 直接禁用对话模式（`supports_tools()=false` 时 UI 提示） |
| 4 | MCP 客户端 | 预留接口（schema 字段），不实施 |

### 0.4 五阶段交付

| 阶段 | 天数 | 累计 | 可交付状态 |
|------|------|------|-----------|
| C1 | 2 | 2 | LLM 流式 + DB 就绪，无 UI |
| C2 | 2 | 4 | 后端"列出文件"跑通，事件序列正确 |
| C3 | 2 | 6 | **首个用户可用版本**：单会话流式对话 |
| C4 | 1.5 | 7.5 | 多会话 + Skill/Memory 集成 |
| C5 | 1 | 8.5 | 完整 C：写审批 + 取消 |

预计 commit 数：12-15 个，每个独立 typecheck + cargo test 验证。

---

## 1. 终态架构

### 1.1 数据模型（新增 3 张表）

```sql
-- 主表：会话
CREATE TABLE IF NOT EXISTS agent_conversations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    system_prompt TEXT,
    skill_key TEXT,
    memory_snapshot TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0
);

-- 消息表（messages[] 扁平化）
CREATE TABLE IF NOT EXISTS agent_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    role TEXT NOT NULL,                      -- 'system' | 'user' | 'assistant' | 'tool'
    content TEXT,
    tool_calls_json TEXT,
    tool_call_id TEXT,
    tool_name TEXT,
    created_at TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES agent_conversations(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_msg_conv ON agent_messages(conversation_id, sequence);

-- 工具注册表（动态扩展，含内置 + MCP 预留）
CREATE TABLE IF NOT EXISTS agent_tools (
    name TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    parameters_schema TEXT NOT NULL,
    handler_kind TEXT NOT NULL,              -- 'builtin' | 'mcp'（暂不用）
    enabled INTEGER NOT NULL DEFAULT 1
);
```

### 1.2 LLM Provider Trait 升级

```rust
// src-tauri/src/llm/provider.rs
pub trait LlmProvider: Send + Sync {
    // 保留现有方法
    async fn complete(&self, prompt: &str) -> Result<String, LlmError>;
    async fn complete_stream(&self, prompt: &str, on_chunk: &mut dyn FnMut(String) + Send)
        -> Result<String, LlmError>;
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError>;

    // 新增：messages + tools 流式
    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
        on_event: &mut dyn FnMut(StreamEvent) + Send,
    ) -> Result<ChatCompletion, LlmError>;

    fn supports_tools(&self) -> bool { true }
}
```

### 1.3 类型定义（src-tauri/src/llm/types.rs，新文件）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,                        // "system" | "user" | "assistant" | "tool"
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,                // tool 消息的工具名
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub kind: String,                        // 通常是 "function"
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,                   // JSON 字符串
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,       // JSON Schema
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCallStart { idx: u32, id: String, name: String },
    ToolCallArgsDelta { idx: u32, args_chunk: String },
    ToolCallEnd { idx: u32 },
    FinishReason(FinishReason),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct ChatCompletion {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
}
```

### 1.4 Tauri 事件协议

| 事件名 | payload | 何时 emit |
|--------|---------|-----------|
| `chat_text_delta` | `{conversation_id, message_id, chunk}` | LLM 流式输出文本 |
| `chat_tool_call` | `{conversation_id, message_id, call_id, tool_name, args, idx}` | 工具调用开始（args 累积完成时一次性 emit） |
| `chat_tool_result` | `{conversation_id, message_id, call_id, ok, content_preview, latency_ms}` | 工具执行完毕 |
| `chat_message_done` | `{conversation_id, message_id}` | 整轮 assistant 消息完成 |
| `chat_awaiting_approval` | `{conversation_id, message_id, write_id, call_id}` | 写操作待审批 |
| `chat_error` | `{conversation_id, message_id, error}` | 错误中止 |
| `chat_cancelled` | `{conversation_id, message_id}` | 用户取消 |

### 1.5 模块/文件清单

**后端新增**：
```
src-tauri/src/
├── llm/
│   ├── types.rs              # 新文件：ChatMessage/ToolCall/StreamEvent 等
│   └── stream_parser.rs      # 新文件：SSE deltas 累积器
└── agent_chat/
    ├── mod.rs                # 模块入口
    ├── db.rs                 # conversations/messages CRUD
    ├── runtime.rs            # ReAct 主循环
    ├── tools.rs              # 工具 schema 注册和执行分发
    └── commands.rs           # Tauri 命令实现
```

**后端修改**：
```
src-tauri/src/
├── llm/
│   ├── provider.rs           # trait 加 chat_stream + supports_tools
│   ├── openai.rs             # 实现 chat_stream（OpenAI/DeepSeek/兼容）
│   └── ollama.rs             # 实现 chat_stream（Ollama 0.4+ tools 协议）
├── db.rs                     # 3 张新表的 migration
├── commands.rs               # 注册新命令
└── main.rs                   # invoke_handler 加新命令
```

**前端新增**：
```
web/src/modules/chat/
├── ChatModule.tsx            # 模块入口（左右分栏布局）
├── ConversationList.tsx      # 会话列表（左栏）
├── MessageThread.tsx         # 消息流（右栏主体）
├── MessageBubble.tsx         # 单条消息气泡
├── ToolCallCard.tsx          # 工具调用卡（可折叠）
├── ChatInputBar.tsx          # 底部输入区
├── NewConversationDialog.tsx # 新建会话弹窗（选 skill）
└── hooks/
    ├── useChatStream.ts      # 事件订阅 + 状态机
    └── useConversations.ts   # 会话列表管理
```

**前端修改**：
```
web/src/
├── App.tsx                   # ModuleId 加 'chat'，路由分发
├── types.ts                  # 新增对话相关类型
├── tauri-client.ts           # 新增对话相关函数
├── modules/Sidebar.tsx       # 左侧导航加入口（如有此文件）
└── styles.css                # 新增对话模块样式
```

---

## 2. Phase C1：LLM Provider + DB 基础设施（2 天）

### 2.1 Step C1.1 — 类型与 Trait（半天）

**新建文件**：
- `src-tauri/src/llm/types.rs`（按 §1.3 类型定义全部实现，含 `Serialize/Deserialize/Debug/Clone`）

**修改 `src-tauri/src/llm/provider.rs`**：
- 加 `chat_stream` 方法（默认实现 `panic!("not implemented")`，后续 step 覆盖）
- 加 `supports_tools(&self) -> bool { true }`
- 在 `mod` 中导出 types 模块

**修改 `src-tauri/src/llm/mod.rs`**：
- `pub mod types;`
- 重新导出常用类型：`pub use types::{ChatMessage, ToolSchema, StreamEvent, ChatCompletion, FinishReason, ToolCall};`

**验收**：
- `cargo check` 通过
- `cargo test` 233 全过（暂不增加测试）

**Commit message**：`feat(C1.1): LlmProvider trait 加入 chat_stream + 类型定义`

### 2.2 Step C1.2 — OpenAI 实现 chat_stream（1 天）

**修改 `src-tauri/src/llm/openai.rs`**：

1. 扩展 `ChatRequest`：
   ```rust
   #[derive(Serialize)]
   struct ChatRequest<'a> {
       model: &'a str,
       messages: &'a [ChatMessage],
       stream: bool,
       #[serde(skip_serializing_if = "Option::is_none")]
       tools: Option<&'a [ToolSchemaWire]>,
       #[serde(skip_serializing_if = "Option::is_none")]
       tool_choice: Option<&'a str>,
   }
   ```
   注：定义 `ToolSchemaWire`（`type=function`/`function={name,description,parameters}` 包装），将 `&[ToolSchema]` 转换为 wire 格式。

2. 实现 `chat_stream`：
   - 构造 SSE 请求（与现有 `complete_stream` 一致，但加 `tools` 字段）
   - 解析 SSE 事件，每条 `data: {...}` 一行
   - 累积器：
     - `content` deltas → emit `TextDelta`
     - `tool_calls[*].function.arguments` deltas → 按 idx 累积，emit `ToolCallArgsDelta`
     - `tool_calls[*].id/function.name` 首次出现 → emit `ToolCallStart`
     - 每次 SSE 事件解析完 → 检查是否 idx 切换或 finish_reason，必要时 emit `ToolCallEnd`
   - 收到 `finish_reason` → emit `FinishReason(...)`，结束流
   - 返回 `ChatCompletion { content, tool_calls, finish_reason }`

**新建 `src-tauri/src/llm/stream_parser.rs`**：
- `pub struct StreamAccumulator` 统一处理 OpenAI SSE deltas
- 测试覆盖：
  - 纯文本流（5 个 chunks）
  - 单个工具调用（args 跨 4 个 chunks）
  - 文本 + 单工具调用混合
  - 多个工具调用（idx 0, 1, 2 交替 args）
  - finish_reason='length' 截断

**验收**：
- `cargo test` 全过 + 新增 ≥ 5 条 stream_parser 测试
- 用真实 DeepSeek API key（如本地有）跑一次 manual 测试：`Get-Date` 工具调用，确认 emit 序列正确

**Commit message**：`feat(C1.2): OpenAI/DeepSeek chat_stream 实现 + SSE deltas 累积器`

### 2.3 Step C1.3 — Ollama 实现 + 能力探测（半天）

**修改 `src-tauri/src/llm/ollama.rs`**（如不存在则在 openai.rs 同级新建）：

1. Ollama `/api/chat` 端点同样支持 `tools` 字段（0.4+），格式与 OpenAI 类似但 SSE 包装不同（NDJSON）
2. 实现 `chat_stream`：解析 NDJSON 流，每行一个 JSON 对象
3. `supports_tools` 实现：调用 `/api/version` 检查版本号 ≥ 0.4.0；缓存结果

**注意**：Ollama 流式协议是 NDJSON（每行一个 JSON），不是 SSE。需单独的 parser。

**验收**：
- `cargo test` 全过 + ≥ 2 条 Ollama parser 测试
- 文档更新：`agents.md` 加注 "对话模式需 Ollama ≥ 0.4.0"

**Commit message**：`feat(C1.3): Ollama chat_stream + version 探测能力降级`

### 2.4 Step C1.4 — DB Schema + CRUD（半天）

**新建 `src-tauri/src/agent_chat/mod.rs` + `agent_chat/db.rs`**：

CRUD 函数（在 db.rs 中）：
```rust
pub fn create_conversation(conn: &Connection, title: &str, system_prompt: Option<&str>,
    skill_key: Option<&str>, memory_snapshot: Option<&str>, now: &str) -> Result<i64, String>;

pub fn list_conversations(conn: &Connection, include_archived: bool) -> Result<Vec<Conversation>, String>;

pub fn get_conversation(conn: &Connection, id: i64) -> Result<Option<Conversation>, String>;

pub fn rename_conversation(conn: &Connection, id: i64, title: &str, now: &str) -> Result<(), String>;

pub fn archive_conversation(conn: &Connection, id: i64, now: &str) -> Result<(), String>;

pub fn delete_conversation(conn: &Connection, id: i64) -> Result<(), String>;

pub fn append_message(conn: &Connection, conv_id: i64, role: &str, content: Option<&str>,
    tool_calls_json: Option<&str>, tool_call_id: Option<&str>, tool_name: Option<&str>,
    now: &str) -> Result<i64, String>;

pub fn list_messages(conn: &Connection, conv_id: i64) -> Result<Vec<Message>, String>;

pub fn list_enabled_tools(conn: &Connection) -> Result<Vec<ToolSchema>, String>;

pub fn upsert_tool(conn: &Connection, name: &str, description: &str, parameters_schema: &str,
    handler_kind: &str, enabled: bool) -> Result<(), String>;
```

**修改 `src-tauri/src/db.rs`**：
- 在 `run_migrations()` 中调用 `agent_chat::db::ensure_schema()`，创建 3 张表
- 在初始化逻辑中调用 `agent_chat::db::seed_builtin_tools()`，写入 5 个内置工具的 schema：
  - `run_shell` / `search_wiki` / `read_wiki` / `write_wiki` / `edit_wiki`
  - 所有 schema 与现有 agent_loop.rs prompt 中的格式一致

**单元测试**（新增 ≥ 6 条）：
- `create_and_list_conversation`
- `append_messages_in_order`
- `archive_excluded_from_default_list`
- `seed_builtin_tools_idempotent`
- `delete_conversation_cascades_messages`
- `tool_calls_json_roundtrip`

**验收**：
- `cargo test` 全过（233 + 6 = 239+）
- 在测试 vault 启动 app 一次，确认 3 张表创建成功

**Commit message**：`feat(C1.4): agent_chat 模块 DB schema + CRUD + 内置工具种子`

---

## 3. Phase C2：单会话流式对话引擎（2 天）

### 3.1 Step C2.1 — 工具执行分发（半天）

**新建 `src-tauri/src/agent_chat/tools.rs`**：

```rust
pub async fn execute_tool_call(
    state: &AppState,
    conv_id: i64,
    call: &ToolCall,
) -> Result<ToolExecResult, String>;

pub struct ToolExecResult {
    pub call_id: String,
    pub content: String,                    // 给 LLM 的结果文本（注入 tool message）
    pub display_preview: String,            // 给 UI 的简短预览（≤ 200 字符）
    pub latency_ms: u64,
    pub awaiting_approval: Option<i64>,     // write/edit 工具：返回 PendingAgentWrite id
}
```

按 `call.function.name` 分发到现有实现：
- `run_shell` → `state.run_shell_impl(...)`，source="agent"
- `search_wiki` → `state.search_wiki_matches(...)`
- `read_wiki` → `state.read_wiki_page_for_agent(...)`
- `write_wiki` / `edit_wiki` → 创建 `PendingAgentWrite`，返回 `awaiting_approval`

**单元测试**：mock state，验证 dispatch 路由正确

**Commit message**：`feat(C2.1): agent_chat 工具执行分发器`

### 3.2 Step C2.2 — ReAct 主循环（1 天）

**新建 `src-tauri/src/agent_chat/runtime.rs`**：

```rust
pub async fn process_message_turn(
    state: &AppState,
    app_handle: &AppHandle,
    conv_id: i64,
    user_message: String,
    cancel_token: CancelToken,
) -> Result<i64, String>  // 返回最终 assistant message_id
```

主循环伪代码：
```
1. 持久化 user message，emit 不需要
2. iter = 0, max_iter = 8
3. loop:
     a. 加载完整 messages from DB
     b. 加载 enabled tools
     c. provider.chat_stream(messages, tools, on_event)
        on_event 转换为 Tauri emit:
          TextDelta → "chat_text_delta"
          ToolCallStart → 缓存
          ToolCallArgsDelta → 累积到该 idx 的 args
          ToolCallEnd → emit "chat_tool_call"
          FinishReason → 退出 stream
     d. 收到 ChatCompletion，持久化 assistant message
        - content → message.content
        - tool_calls → message.tool_calls_json
     e. match completion.finish_reason:
          Stop → emit chat_message_done，break
          ToolCalls → 顺序执行所有 tool_calls:
            for call in completion.tool_calls:
              execute_tool_call(...) →
                if awaiting_approval: emit chat_awaiting_approval, return Pending
                else: emit chat_tool_result, append tool message to DB
            iter += 1
            if iter >= max_iter: emit chat_message_done with note "max iterations", break
            continue (回到 a)
          Length → emit chat_message_done with note "truncated", break
4. cancel_token check at every loop boundary
```

**集成测试**（新增 ≥ 4 条，使用 mock provider）：
- `single_turn_text_only`：mock 返回 stop，仅文本
- `single_turn_with_run_shell`：mock 返回 tool_calls + execute + 二次 stream stop
- `multi_iteration_search_then_read`：search_wiki → read_wiki → final answer
- `cancellation_mid_stream`：CancelToken 触发后立即停止

**验收**：
- `cargo test` 全过（≥ 243）
- 用 mock provider 调用 process_message_turn，控制台 print emit 序列检查

**Commit message**：`feat(C2.2): agent_chat ReAct 主循环 + 流式事件 emit`

### 3.3 Step C2.3 — Tauri 命令 + 注册（半天）

**新建 `src-tauri/src/agent_chat/commands.rs`**：

```rust
#[tauri::command]
pub async fn create_conversation(
    title: Option<String>,
    skill_key: Option<String>,
    inject_memories: Option<bool>,
    state: State<'_, AppState>,
) -> Result<i64, String>;

#[tauri::command]
pub fn list_conversations(
    include_archived: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<Conversation>, String>;

#[tauri::command]
pub async fn send_chat_message(
    conversation_id: i64,
    user_message: String,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<i64, String>;  // 返回 message_id

#[tauri::command]
pub fn list_chat_messages(
    conversation_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Message>, String>;

#[tauri::command]
pub async fn cancel_chat_message(
    conversation_id: i64,
    message_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String>;

#[tauri::command]
pub fn rename_conversation(...) -> Result<(), String>;

#[tauri::command]
pub fn archive_conversation(...) -> Result<(), String>;

#[tauri::command]
pub fn delete_conversation(...) -> Result<(), String>;
```

**修改 `src-tauri/src/main.rs`**：
- `invoke_handler` 中注册全部 8 个命令

**修改 `src-tauri/src/state.rs`**：
- AppState 加 `chat_cancellations: Arc<Mutex<HashMap<i64, CancelToken>>>` 用于取消

**验收**：
- `cargo test` 全过
- `cargo check` 通过

**Commit message**：`feat(C2.3): 对话 Tauri 命令注册`

---

## 4. Phase C3：前端独立对话 UI（2 天）

### 4.1 Step C3.1 — 类型 + tauri-client（半天）

**修改 `web/src/types.ts`**：

```typescript
export interface Conversation {
  id: number;
  title: string;
  system_prompt: string | null;
  skill_key: string | null;
  memory_snapshot: string | null;
  created_at: string;
  updated_at: string;
  archived: boolean;
}

export interface ChatMessage {
  id: number;
  conversation_id: number;
  role: "system" | "user" | "assistant" | "tool";
  content: string | null;
  tool_calls: ToolCall[] | null;
  tool_call_id: string | null;
  tool_name: string | null;
  created_at: string;
  sequence: number;
}

export interface ToolCall {
  id: string;
  type: "function";
  function: { name: string; arguments: string };
}

// UI 内部状态（非后端类型）
export type ChatStreamSegment =
  | { kind: "text"; text: string; streaming: boolean }
  | { kind: "tool"; call_id: string; tool_name: string; args: any;
      result?: { ok: boolean; preview: string; latency_ms: number };
      status: "running" | "ok" | "err" | "awaiting_approval" }
  | { kind: "error"; message: string };

export interface ChatStreamingMessage {
  conversation_id: number;
  message_id: number;
  segments: ChatStreamSegment[];
  status: "streaming" | "tool_running" | "awaiting_approval" | "done" | "cancelled" | "error";
}
```

**修改 `web/src/tauri-client.ts`** — 新增 8 个函数：
- `createConversation(title?, skillKey?, injectMemories?)`
- `listConversations(includeArchived?)`
- `sendChatMessage(conversationId, userMessage)`
- `listChatMessages(conversationId)`
- `cancelChatMessage(conversationId, messageId)`
- `renameConversation(id, title)`
- `archiveConversation(id)`
- `deleteConversation(id)`

**验收**：
- `npx tsc --noEmit` 通过
- `npx vitest run` 全过

**Commit message**：`feat(C3.1): 对话相关前端类型 + tauri-client`

### 4.2 Step C3.2 — useChatStream hook + 基础组件（1 天）

**新建 `web/src/modules/chat/hooks/useChatStream.ts`**：

核心职责：
1. 订阅 Tauri 事件 `chat_text_delta` / `chat_tool_call` / `chat_tool_result` / `chat_message_done` / `chat_awaiting_approval` / `chat_error` / `chat_cancelled`
2. 维护 `streamingMessage: ChatStreamingMessage | null`
3. 提供 `sendMessage(text: string)` 方法：调 `sendChatMessage`，立即新建 streamingMessage placeholder
4. 优化：用 `useReducer` + `requestAnimationFrame` 节流，避免每个 token 都 re-render

**新建组件**（5 个）：

`ChatModule.tsx`（左右分栏布局）：
- 左 240px：`<ConversationList />`
- 右 flex:1：当前 selectedConversation 的 `<MessageThread />` + `<ChatInputBar />`

`ConversationList.tsx`：
- 顶部：`+ 新对话`按钮（弹 `NewConversationDialog`）
- 列表项：title + updated_at + 归档/删除右键菜单
- 选中状态高亮

`MessageThread.tsx`：
- 加载 messages from `listChatMessages(convId)`
- 流式中：append `streamingMessage` 到末尾
- 渲染每条消息为 `<MessageBubble />`
- 自动滚动到底（auto-follow，与 shell 一致）

`MessageBubble.tsx`：
- role=user：右对齐蓝色气泡
- role=assistant：左对齐灰色气泡，segments 顺序渲染
  - text segment：流式 → 显示文本 + ▍光标
  - tool segment：渲染 `<ToolCallCard />`
- role=tool：不渲染（已合并到 assistant 的 tool segment）

`ToolCallCard.tsx`：
- 折叠态：`🔧 run_shell ✓ 120ms`（点击展开）
- 展开态：args（JSON 高亮）+ result preview
- 状态：running 显示 spinner，awaiting_approval 显示"待审批"链接

`ChatInputBar.tsx`：
- textarea + 发送按钮
- Enter 发送，Shift+Enter 换行
- 流式中：发送按钮变"取消"

**验收**：
- 单会话：新建 → 发"列出文件" → 流式可见全过程 → 刷新页面对话保留
- typecheck + 179 测试全过

**Commit message**：`feat(C3.2): 对话 UI 组件 + 流式 hook`

### 4.3 Step C3.3 — 模块路由 + 导航集成（半天）

**修改 `web/src/types.ts`**：
- `ModuleId` 加 `"chat"`

**修改 `web/src/App.tsx`**：
- 路由分发：`activeModule === "chat" ? <ChatModule /> : ...`

**修改左侧 Sidebar**（找现有 sidebar 组件）：
- 在"运营"组上方新增"对话"独立条目（按 §1.1 决策 1，独立 ModuleId）

**修改 `web/src/styles.css`**：
- 新增 `.chat-module__*` 样式（参考 ask-module 风格）
- 流式光标动画 `@keyframes chat-cursor-blink`

**验收**：
- 左侧导航看到"对话"入口，点击进入 ChatModule
- 切换到其他模块再回来，conversationList 状态保留（或重新加载也可）

**Commit message**：`feat(C3.3): chat ModuleId 接入路由 + 导航`

---

## 5. Phase C4：多会话 + 上下文集成（1.5 天）

### 5.1 Step C4.1 — 自动标题生成（半天）

**修改 `agent_chat/runtime.rs`**：
- 首条用户消息发送后，异步触发 `generate_title_async(conv_id, first_user_msg)`
- 用一个简短 prompt 调 `complete()`：
  ```
  请用 4-12 个字概括以下用户问题作为对话标题，只输出标题文字：
  {user_message}
  ```
- 结果调 `db::rename_conversation`

**前端**：监听 `chat_title_updated` 事件刷新列表

**Commit message**：`feat(C4.1): 自动生成对话标题`

### 5.2 Step C4.2 — Skill 模板 + 记忆注入（半天）

**修改 `create_conversation` 命令**：
- `skill_key` 提供时：从 `agent_skills` 读取 prompt_template，作为 system message
- `inject_memories=true`：从 `agent_memories` 读取所有，序列化为 JSON 存入 `memory_snapshot`，并作为 system message 的一部分注入

**前端 `NewConversationDialog`**：
- 选择 skill（下拉，从 `listAgentSkills` 加载）
- checkbox：`注入当前记忆（{count} 条）`

**修改 `runtime.rs`**：
- 加载 messages 时，如果 conv 有 system_prompt，作为 messages[0] 注入

**Commit message**：`feat(C4.2): Skill 模板 + 记忆快照集成`

### 5.3 Step C4.3 — 会话管理 UI 完善（半天）

**功能补全**：
- 重命名：右键菜单或编辑按钮
- 归档：右键归档；列表顶部 toggle "显示已归档"
- 删除：二次确认弹窗
- 搜索：列表上方搜索框（按标题模糊匹配）

**Commit message**：`feat(C4.3): 会话列表搜索/归档/重命名`

---

## 6. Phase C5：写审批 + 取消 + 收尾（1 天）

### 6.1 Step C5.1 — 写操作审批接入（半天）

**修改 `agent_chat/tools.rs`**：
- write_wiki/edit_wiki：创建 `PendingAgentWrite`，返回 `awaiting_approval=Some(write_id)`

**修改 `runtime.rs`**：
- 检测到 `awaiting_approval`：
  - emit `chat_awaiting_approval`
  - 把 ReAct 循环状态保存到内存 `pending_loops: Arc<Mutex<HashMap<i64, PendingLoop>>>`（key=conv_id）
  - 暂停循环，return Pending

**新 Tauri 命令**：
- `approve_chat_write(conversation_id, write_id, message_id)`：
  - 调用现有 `state.approve_agent_write_impl`
  - 把审批结果作为 tool message 写入 DB
  - 恢复 pending loop，继续 ReAct 循环
- `reject_chat_write(conversation_id, write_id, message_id)`：
  - 调用 `state.reject_agent_write_impl`
  - 注入"User rejected"作为 tool message
  - 恢复循环

**前端**：
- ToolCallCard 状态 `awaiting_approval` 时显示"批准/拒绝"按钮
- 审批后状态变 ok/err，循环自动继续

**Commit message**：`feat(C5.1): 写操作审批集成对话 ReAct 循环`

### 6.2 Step C5.2 — 取消 + 错误处理（半天）

**修改 `runtime.rs`**：
- 每次 loop 边界检查 cancel_token
- LLM 流中：on_event 闭包检查 cancel_token，触发后 return Err

**前端**：
- 流式中按钮变"取消"，点击调 `cancelChatMessage`
- 收到 `chat_cancelled` 事件：streamingMessage.status='cancelled'，UI 显示"已取消"

**错误处理**：
- LLM 错误：emit `chat_error`，UI 显示红色错误条
- 工具执行错误：tool segment.status='err'，但 ReAct 循环继续（注入错误结果作为 tool message）

**最终验收**（手测清单）：
1. 新建对话 → "列出当前目录文件" → 流式 + 工具卡 + 自然语言回答 ✅
2. "找最大的文件" → 引用上轮结果，多步推理 ✅
3. "把 X 写到 wiki/test.md" → 审批弹窗 → 批准 → 文件落盘 + LLM 收尾 ✅
4. 流式中点取消 → 立即停止 ✅
5. 多会话切换无串流 ✅
6. 刷新页面 → 历史对话保留 ✅
7. Ollama < 0.4.0 → 模块入口禁用，提示升级 ✅

**Commit message**：`feat(C5.2): 取消 + 错误处理 + 收尾`

---

## 7. 进度勾选表

### Phase C1 — 基础设施（2 天）
- [ ] C1.1 LlmProvider trait 升级 + 类型定义
- [ ] C1.2 OpenAI/DeepSeek chat_stream 实现
- [ ] C1.3 Ollama chat_stream + 版本探测
- [ ] C1.4 DB schema + CRUD + 内置工具种子

### Phase C2 — 后端引擎（2 天）
- [ ] C2.1 工具执行分发器
- [ ] C2.2 ReAct 主循环 + 流式 emit
- [ ] C2.3 Tauri 命令注册

### Phase C3 — 前端 UI（2 天）
- [ ] C3.1 类型 + tauri-client
- [ ] C3.2 useChatStream + 基础组件
- [ ] C3.3 模块路由 + 导航集成

### Phase C4 — 上下文集成（1.5 天）
- [ ] C4.1 自动标题生成
- [ ] C4.2 Skill 模板 + 记忆注入
- [ ] C4.3 会话管理 UI 完善

### Phase C5 — 收尾（1 天）
- [ ] C5.1 写操作审批集成
- [ ] C5.2 取消 + 错误处理 + 手测验收

---

## 8. 验收基线

每个 step commit 之前必须通过：
```bash
cd src-tauri && cargo test          # ≥ 233（C1.4 后会增加）
cd web && npx tsc --noEmit          # 0 errors
cd web && npx vitest run            # 179 全过（C3.x 之后可能增加）
```

C5 完成后用户手测清单见 §6.2。

---

## 9. 异常处理

### 9.1 限流断点续作

每个 Step 独立 commit，被限流时按 §7 进度勾选表：
1. 找到最后一个完成的 step
2. 读 `git log --oneline` 确认与 plan 一致
3. 从下一个未勾选 step 开始

### 9.2 设计冲突或 spec 模糊

遇到 plan 没覆盖的细节：
1. 先在本地实现一个最小可行方案
2. 在 commit message 中标注 `[design-deviation]: <说明>`
3. 在 `docs/实施过程记录.md` 中追加一条决策记录

### 9.3 测试失败

- 先 `cargo test --no-fail-fast 2>&1 | tail -50` 看完整失败
- 不允许跳过测试（`#[ignore]`）来通过 CI
- 如果测试 spec 本身有 bug，更新测试并在 commit 中说明

### 9.4 前端流式渲染卡顿

如果 C3.2 出现明显卡顿：
1. 用 React DevTools Profiler 找瓶颈
2. 优先尝试：`useReducer` 合批 + `requestAnimationFrame` 节流
3. 仍卡 → `useDeferredValue` 包裹 streaming text
4. 极端情况 → 改用 vanilla DOM 操作（escape hatch）

---

## 10. 关键设计决策记录（ADR）

### ADR-1：放弃 XML 标签，直接用 OpenAI function_calling

**Context**：B 方案曾考虑 `<thought>/<action>/<final>` XML 标签
**Decision**：终态 C 直接用 OpenAI function_calling 协议
**Reason**：业界标准、Ollama 0.4+/DeepSeek/通义全支持、LLM 本身训练时已优化此格式、无需自定义 parser
**Consequence**：低版本 Ollama 不可用，需要降级提示

### ADR-2：Conversation 独立于 agent_runs

**Decision**：新建 `agent_conversations` 主表，不复用 `agent_runs`
**Reason**：runs 设计为"任务"语义（有 status/draft/approval），对话语义是"消息序列"，强行复用会让两边都变形
**Consequence**：旧 runs 只读保留；新功能完全走新表

### ADR-3：tools 表预留 MCP 字段但不实施

**Decision**：`agent_tools.handler_kind` 字段保留 `'mcp'` 值
**Reason**：避免后续接 MCP 时再做 schema migration
**Consequence**：当前所有内置工具 handler_kind='builtin'

### ADR-4：流式 emit 用 Tauri events 而非 channel

**Decision**：用 `app_handle.emit()` 推送，前端 `listen()` 订阅
**Reason**：与现有 Ask 模块/Shell stream 一致，减少架构异质性
**Consequence**：事件可能跨 conversation 串扰，前端必须按 conversation_id 过滤

---

## 11. 与现有功能的关系

| 现有功能 | C 完成后状态 |
|----------|------------|
| Agent Studio 任务模式 | 保留：作为"创建对话 + 注入特定 system prompt"的快捷入口 |
| Agent Studio 草稿模式 | 保留：单独 generate_draft 命令仍可用 |
| 历史 Runs | 只读保留：归档展示，不再产生新 run |
| Skills | 复用：作为对话的可选系统 prompt |
| Memories | 复用：作为对话创建时的快照注入 |
| Shell 工具页 | 保留：手动 shell 终端独立场景，与对话模式平行存在 |
| Inbox / Wiki / Ask 等 | 不变 |

---

## 12. 起手指引（Sonnet 4.6 必读）

1. 读本文件 §0-§3（理解架构和 C1 实施细节）
2. 读 `agents.md` §11、§16（中文注释 + 多 Agent 交接）
3. 读 `docs/dev-status.md`（确认基线数字）
4. 从 §7 进度勾选表找到下一个未完成 step
5. 严格按 §2-§6 中对应 step 的 scope 实施，不要跨 step
6. 每个 step 完成后：
   - 更新 §7 勾选表
   - cargo test + typecheck + vitest 全过
   - 独立 commit（按 step 给出的 commit message）
   - 不要批量提交多个 step
7. 限流时停在 step 边界，下次接力从 §7 找位置
