# jcode 项目分析与对 llm-wiki 的借鉴价值

> 分析日期：2026-05-09 | 目标仓库：https://github.com/1jehuang/jcode

---

## 1. 项目速览

jcode（作者 Jeremy Huang）是一个**纯 Rust 编写的终端 AI 编码 Agent 平台**，定位为"编码 Agent 载具"（Coding Agent Harness）。它直接运行在终端内，不依赖 Electron 或 Node.js 运行时。

### 1.1 关键指标

| 指标 | 数值 | 说明 |
|------|------|------|
| 启动时间 | 14ms | 原生 Rust 二进制 |
| 内存占用 | 27.8MB | 基础运行时，不含模型 |
| TUI 帧率 | 1000+ FPS | 自研 ratatui 渲染 |
| 内置工具 | 45+ | shell / 文件 / grep / web / MCP / embed |
| 工作区 Crate | 48 | 高度模块化，各 crate 独立编译 |
| Provider 数量 | 30+ | 含 OAuth 登录流程 |
| 子代理层级 | 最高 3 层 | Swarm 递归 spawn |

### 1.2 核心架构

```
┌─────────────────────────────────────────┐
│  客户端层                                │
│  ┌──────────┐  ┌──────────┐             │
│  │ TUI 客户端│  │ Web 网关  │  ...        │
│  └────┬─────┘  └────┬─────┘             │
│       │              │                   │
│       └──────┬───────┘                   │
│              │ Unix-domain socket        │
├──────────────┼───────────────────────────┤
│  后端服务层  │                            │
│  ┌───────────┴──────────────────────┐    │
│  │  会话管理 / LLM 交互 / 工具执行    │    │
│  └───┬──────────────────────────────┘    │
│      │                                   │
│  ┌───┴────────┬──────────────┬───────┐   │
│  │ Agent      │ Tool         │ Memory│   │
│  │ Runtime    │ Registry     │ (ONNX)│   │
│  └────────────┴──────────────┴───────┘   │
└─────────────────────────────────────────┘
```

- **单服务多客户端**：一个长期运行的后端进程，多个前端通过 Unix-domain socket 连接
- **工作空间 Crate 拆分**：`jcode-agent-runtime`（核心引擎）、`jcode-embedding`（ONNX 向量）、`jcode-provider-core`（Provider 抽象）等多个独立 crate
- **会话持久化**：JSON 快照 + append-only journal，支持断点续跑和中断恢复

---

## 2. 与 llm-wiki 的对比

### 2.1 技术栈对照

| 维度 | jcode | llm-wiki | 兼容性 |
|------|-------|----------|--------|
| **后端语言** | Rust | Rust (Tauri) | ✅ 语言一致 |
| **前端渲染** | ratatui (终端 TUI) | React + WebView | ❌ 架构不同 |
| **IPC 机制** | Unix-domain socket | Tauri invoke/event | ❌ 机制不同 |
| **数据库** | 文件系统 + 嵌入向量 | SQLite + FTS5 | 部分重叠 |
| **打包目标** | 单二进制 (cargo install) | MSI/EXE (Tauri 打包) | 部分不同 |
| **首要平台** | Linux/macOS 终端 | Windows 桌面 | ❌ 平台不同 |
| **Provider 层** | 30+ 模型统一接口 | Ollama + Cloud trait | ✅ 模式一致 |

### 2.2 功能对照

| 功能 | jcode | llm-wiki |
|------|-------|----------|
| Agent 循环 | ✅ agent-runtime | ✅ agent_loop.rs |
| 工具注册/执行 | ✅ Tool Registry (45+) | ✅ AgentToolAction enum (6) |
| 安全策略 | ✅ 内置 | ✅ agent_policy.rs (5维) |
| Shell 执行 | ✅ | ✅ run_shell |
| 文件读写 | ✅ | ✅ write_wiki / edit_wiki |
| 语义检索 | ✅ ONNX 向量嵌入 | ⚠️ FTS5 全文索引 |
| 多 Agent 协作 | ✅ Swarm 三层递归 | ❌ 单 Agent 循环 |
| MCP 集成 | ✅ 原生支持 | ❌ 未实现 |
| Context 压缩 | ✅ | ❌ 未实现 |
| 会话持久化 | ✅ JSON + journal | ✅ SQLite agent_runs |

---

## 3. 集成可行性评估

### 3.1 结论：不适合直接集成

**核心原因（由重到轻排列）：**

#### 第一层：架构冲突（致命）

jcode 是**独立终端原生应用**，自带 ratatui 渲染引擎（1000+ FPS），通过 Unix-domain socket 做服务端/客户端分离。llm-wiki 是 **Tauri + WebView 桌面应用**，前端是 React 组件树，后端通过 Tauri 的 `#[tauri::command]` 暴露 IPC。

两种架构无法共存：
- jcode 的客户端必须连接其 socket 后端，不能嵌入 Tauri WebView
- Tauri 的命令模型与 jcode 的 socket 消息模型完全不同
- 即使只提取后端 crate，也需要重写所有 IPC 边界代码

**如果强行"集成"**，实际上是在做 jcode 后端 crate → Tauri 命令的完整移植，工作量和风险与从零实现相当。

#### 第二层：功能重叠（冗余）

llm-wiki 已有的模块与 jcode 核心能力高度重叠：

| llm-wiki 模块 | 对应 jcode 能力 | 状态 |
|---------------|----------------|------|
| `agent_loop.rs` | agent-runtime | 已有，daerwen 移植中 |
| `agent_tools.rs` | Tool Registry | 已有基础版 |
| `agent_policy.rs` | 策略引擎 | 已有 5 维策略 |
| `llm/` (provider 层) | provider-core | 已有 trait 抽象 |
| `search.rs` + FTS5 | embedding 检索 | 走不同路线 |

引入 jcode 会创建两套并行系统，维护负担翻倍。

#### 第三层：平台绑定

- jcode 的 Unix-domain socket 深度依赖 Linux/macOS 路径语义
- `jcode-embedding` crate 依赖 ONNX runtime（额外的 C++ 编译链）
- llm-wiki 的首要目标是 **Windows MSI/EXE**；Windows 上 socket 需改为 named pipe
- 适配成本 ≈ 整个后端 fork 并重写通信层

#### 第四层：Windows 平台测试覆盖率

jcode 以 Linux/macOS 为首要平台，README 中提到 Windows 支持，但实际 Windows 测试覆盖率不明。`jcode-embedding` 的 ONNX 编译链是否在 Windows MSVC 环境下能顺利编译，需实际验证（jcode 中该功能为 feature-gated，可选择不编译 ONNX 部分）。

> **许可证已确认**：仓库采用 **MIT License**，参考设计思想和代码无法律风险。

---

## 4. 与 daerwen-agent 参考项目的对比

llm-wiki 已有参考项目 `refer-rust-daerwen-agent/`，其技术栈与本项目完全一致（Rust + Tauri）。以下是两个参考源的对比：

| 维度 | daerwen-agent | jcode |
|------|--------------|-------|
| **技术栈** | Rust + Tauri（与 llm-wiki 一致） | Rust ratatui TUI（不一致） |
| **Agent 循环** | `Agent::run_with_history` | agent-runtime |
| **工具系统** | `ToolHandler` trait + builtins | Tool Registry（更庞大） |
| **策略引擎** | `PathGuard` 5区分级（明确设计） | 内置策略（细节未公开） |
| **移植路径** | H6 计划中已有详细路线 | 无成熟路径 |
| **模块化** | 12 个独立 crate，边界清晰 | 多 crate，但耦合到 socket 后端 |
| **移植结论** | ✅ 可直接移植 | ❌ 不适合移植 |

**daerwen-agent 是更合适的参考源**，理由：
1. 同为 Tauri 项目，`tauri::command` ↔ `#[tauri::command]` 直接对应
2. 工具系统 (`ToolHandler` trait) 已经有清晰的移植计划
3. `PathGuard` 的设计比 llm-wiki 当前策略更细腻，但适配成本可控
4. 12 个 crate 按需采摘，不需要全量引入

---

## 5. 值得借鉴的设计思想（重点）

虽然 jcode 不适合直接集成，但它的**设计模式**对 llm-wiki 的后续迭代有重要参考价值。以下按优先级排列。

---

### 5.1 Tool Registry 模式（优先级：高，建议 H10 轮次引入）

#### jcode 的做法

jcode 不把工具写死在枚举里，而是采用 **Tool Registry** 动态注册机制：

```rust
// 概念模型（非 jcode 实际代码，基于公开信息推演）
trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError>;
}

struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    fn register(&mut self, tool: Box<dyn Tool>);
    fn get_tool_schemas(&self) -> Vec<ToolSchema>;  // 喂给 LLM 做 function calling
    async fn dispatch(&self, name: &str, params: Value) -> Result<ToolOutput, ToolError>;
}
```

#### llm-wiki 现状

```rust
// src-tauri/src/agent_tools.rs 当前做法（枚举硬编码）
pub enum AgentToolAction {
    ReadWiki { path: String },
    WriteWiki { path: String, content: String },
    EditWiki { path: String, old_str: String, new_str: String },
    SearchWiki { query: String },
    RunShell { command: String },
}
```

当前只有 5 种工具，扩展需要修改枚举定义 + match 分支 + prompt 模板。

#### 借鉴意义

将 `AgentToolAction` 枚举替换为 `Tool` trait + `ToolRegistry`：

1. **扩展性**：新增工具只需实现 `Tool` trait + 注册进 Registry，不改已有 match 分支
2. **MCP 就绪**：MCP server 的工具可以直接包装为 `MCPTool` 并注册到同一个 Registry
3. **权限绑定**：每个工具绑定自己的 `PolicyDecision`，支持工具级安全策略
4. **LLM discovery**：Registry 自动生成 function calling schema，无需手工维护 prompt

#### 建议移植路径

```
当前 enum AgentToolAction
  ↓
中间态：enum 内部实现 Tool trait（适配现有代码）
  ↓
最终态：ToolRegistry + 动态注册（H10 轮次引入）
```

依赖关系：不依赖 jcode 代码，纯设计思想借鉴。

---

### 5.2 语义记忆 + 向量检索（优先级：中，建议 H11 轮次引入）

#### jcode 的做法

jcode 使用 **ONNX 嵌入模型**在本地做语义向量化：

```
用户输入 / Agent 响应
  ↓
ONNX Embedding Model（本地推理）
  ↓
向量存入记忆图 (in-memory graph + 持久化)
  ↓
每个新 turn 查询记忆图: cosine_similarity(当前向量, 历史向量)
  ↓
相关记忆注入当前对话上下文
```

特点：
- 纯本地（ONNX runtime），不需额外服务
- **ONNX 为 feature-gated（可选编译）**：不需要 embedding 功能时直接关闭 feature，避免引入约 163 个额外 crate 的编译负担
- 自动去噪：`memory sideagent` 可二次验证记忆相关性
- 向量维度与模型强相关（可能 768/1024/4096）
- **语义记忆图**：向量不只做检索，还构建节点间语义关联图，每个 turn 自动将高相似度历史注入当前上下文（与 llm-wiki 计划的"双路召回 FTS5 + 向量"有本质区别——jcode 的记忆是持续自动注入的，不需要 Agent 主动调用检索工具）

#### llm-wiki 现状

检索基于 SQLite FTS5 全文索引 + BM25，优势是快速精确匹配，劣势是：

- 同义词无法匹配（"AI 模型" vs "大语言模型"）
- 跨语言无法召回（"machine learning" vs "机器学习"）
- 概念相似但文字不同的知识完全漏掉
- 不适合做"这个观点和我已有的哪些知识矛盾"这类语义 Lint

#### 借鉴意义

在已有 FTS5 基础上叠加**本地向量检索**作为第二路召回：

```
Query
  ├─→ FTS5 全文检索（精确匹配，LLM 关键词查询）
  └─→ 向量检索（语义匹配，自动触发）
       ↓
  Rerank / 融合
       ↓
  组合结果注入 LLM context
```

具体来说：
1. **Ingest 时**：新页面自动生成 embedding 存入 SQLite（新增 `vector` BLOB 列）
2. **Query 时**：FTS5 + cosine similarity 双路召回 → RRF 融合
3. **Lint 时**：`claim_vector` vs `all_known_claim_vectors` 冲突检测

#### 技术选择

- **ONNX（jcode 路线）**：优点是不依赖 Ollama，缺点是引入 ONNX runtime 编译链
- **Ollama Embed API（llm-wiki 现有路线）**：llm-wiki 已有 `get_embed_provider()` 调用 Ollama，可直接复用

**建议优先走 Ollama Embed API**，与现有 Provider 层完全一致，零额外依赖。

#### 建议移植路径

```
Phase 1: 新增 embed 缓存列（wiki_pages.vector BLOB）
Phase 2: Ingest 后异步生成嵌入
Phase 3: Query 双路召回 + RRF 融合
Phase 4: Lint 语义冲突检测（基于 cosine similarity）
```

依赖关系：不依赖 jcode 代码，纯设计思想借鉴。可参考 jcode 的 `memory sideagent` 设计做自动去噪。

---

### 5.3 多层 Swarm 子代理模式（优先级：中，建议 H12 轮次引入）

#### jcode 的做法

jcode 支持**三层递归子代理**：

```
父 Agent (用户任务)
  ├─→ 子 Agent A (子任务 1)
  │     └─→ 孙 Agent A1 (子子任务)
  └─→ 子 Agent B (子任务 2)
```

关键设计：
1. 父代理 spawn 子代理后**继续运行**（非阻塞等待）
2. 子代理拥有**独立上下文**，不污染父代理的 context window
3. 子代理返回**结构化结果**给父代理（不是全文转录）
4. 每个子代理独立做 tool-call，互不干扰
5. 深度限制（最多 3 层），防止递归爆炸

#### llm-wiki 现状

Agent Studio 是**单 Agent 循环**：一个 LLM 实例做 tool-call loop。对于复杂任务（如"读这 20 篇 PDF，交叉引用后更新 15 个 Wiki 页面"），单 Agent 面临：

- 上下文窗口爆炸（20 篇 PDF 全文 ≈ 200K tokens）
- 任务上下文混乱（LLM 同时处理多个子任务，容易丢失线索）
- 并行潜力浪费（PDF 阅读是天然可并行的）

#### 借鉴意义

将复杂 ingest 任务拆分为子代理树：

```
复杂 Ingest: "读 20 篇 PDF 并更新 Wiki"
  ├─→ 子代理 1-5: 各自读 4 篇 PDF，输出摘要 + 实体 + 引用
  │     (5 个子代理并行执行)
  ├─→ 子代理 6: 合并交叉引用，检测矛盾
  └─→ 子代理 7: 增量更新 15 个 Wiki 页面
```

**与 AGENTS.md §14 的要求天然吻合**：AGENTS.md 已经要求"后端/前端各用独立子代理并行开发"——这种并行分解在执行层面的落地就是 Swarm 模式。

#### 建议移植路径

```
Phase 1: 支持 spawn_subagent(task) 创建独立 AgentRun
Phase 2: 子代理结束后回调父代理（通过 run_events）
Phase 3: 父代理的 loop prompt 注入子代理结果摘要
Phase 4: 递归深度限制 + 并行度控制
```

依赖关系：不依赖 jcode 代码，但可以参考其上下文隔离的设计策略。

---

### 5.4 会话持久化：JSON 快照 + Append-Only Journal（优先级：中低，建议作为 agent_runs 的升级方向）

#### jcode 的做法

```
~/.jcode/sessions/
  ├── session_abc123.json          # 完整会话快照（周期性写入）
  └── session_abc123.journal       # 每条消息的追加日志
```

- **journal**：每条消息/tool_call 即时追加，保证不丢数据
- **snapshot**：周期性（每 N 次交互或会话暂停时）生成完整快照
- 恢复时：加载最近快照 → 重放 journal 中 snap 之后的增量 → 恢复到最新状态

#### llm-wiki 现状

```
agent_runs 表:
  id | instruction | status | final_output | created_at | updated_at

agent_run_events 表:
  id | run_id | event_type | message | metadata_json | created_at
```

当前方案能记录事件流，但"续跑"时需要重新构造完整对话上下文（从 events 重建 messages 数组），不如 journal 直接追加高效。

#### 借鉴意义

对 llm-wiki 的改进点：

1. **周期性保存完整对话状态**（messages 数组 + 当前迭代轮次 + 待审批列表），不再只依赖 events 表重建
2. **快速恢复**：加载上一个快照 + 少量 journal 增量，O(n) → O(1)
3. **Rollback**：可以回退到任意快照点重试

#### 建议移植路径

```
当前: agent_run_events 表作为事件流
  ↓
升级: 新增 agent_run_snapshots 表（BLOB 存完整 messages 数组）
  ↓
恢复时: load_last_snapshot + replay events since snapshot
```

依赖关系：不依赖 jcode 代码，纯数据模型设计。

---

### 5.5 Context Compaction 策略（优先级：中低，当 Agent 长会话成为瓶颈时引入）

#### jcode 的做法

jcode 在上下文接近模型限制时自动执行压缩：

- 早期消息自动 summarize → 保留摘要替换原文
- Tool output 自动截断（保留关键信息，丢弃冗余输出）
- 子代理返回摘要而非全文（见 §5.3）

#### llm-wiki 现状

当前没有上下文压缩机制。Agent 循环的 prompt 包含完整历史，轮次多了会爆炸。

#### 借鉴意义

在 `agent_loop.rs` 中增加 compaction 层：

```
当 prompt 长度 > context_limit * 0.7:
  1. 识别对话中的"已完成子任务"
  2. LLM 将这些子任务 summarize 为 2-3 行
  3. 用摘要替换对应的原始 tool_call + tool_output
  4. 保留最近 N 轮不动（保留即时上下文）
```

与 AGENTS.md 的 `/compact` 概念一致——将对话模式推广到 Agent 内部。

---

### 5.6 MCP 原生集成策略（优先级：**中，建议 H10 轮次与 Tool Registry 同步引入**）

#### jcode 的做法

- 首次运行时自动导入 `~/.claude/mcp.json` 和 `~/.codex/config.toml` 中的 MCP servers
- MCP 服务器的工具自动注册到 Tool Registry（与 §5.1 联动）
- 对用户透明：MCP 工具和内置工具在 LLM 看来完全一样
- 使用 JSON-RPC 2.0 over stdio 传输协议，标准实现，已有成熟 Rust SDK 可用（`mcp-sdk`）

> **优先级上调原因**：jcode 已有完整、生产级的 MCP 实现可供参考（包括 stdio transport、工具发现、调用代理、配置文件格式）。llm-wiki 实现 MCP 的路径已被 jcode 验证，不再是"摸黑探索"，风险等级从高降到中。

#### llm-wiki 现状

没有 MCP 支持。但 AGENTS.md §10 明确要求"搜索适合本项目的 MCP servers/tools"。

#### 借鉴意义

如果 llm-wiki 未来实现 §5.1 的 Tool Registry，MCP 集成就非常自然：

```
MCP Server 发现（settings.json 配置）
  ↓
stdio transport（spawn 子进程）
  ↓
JSON-RPC 2.0 工具发现（tools/list）
  ↓
包装为 McpTool (impl Tool trait)
  ↓
注册到 ToolRegistry
  ↓
LLM 通过 function calling 调用（与内置工具无区别）
```

### 5.7 软中断与后台工具执行模式（优先级：中，建议 H11 Swarm 轮次引入）

#### jcode 的做法

jcode 实现了**软中断机制**：Agent 在执行长耗时工具（如大文件读取、web 搜索、子代理任务）时，不阻塞主循环，而是：

```
主 Agent Loop
  ├─→ 发出工具调用请求
  ├─→ 注册 interrupt_flag（原子布尔）
  ├─→ 工具在后台 tokio::task::spawn 中执行
  ├─→ 主 loop 可以检查 interrupt_flag，提前取消
  └─→ 工具完成后通过 mpsc channel 返回结果
```

同时支持**后台工具**：某些工具（如监控、日志收集）可以在 Agent 生命周期内持续后台运行，不占用 LLM 的 turn 配额。

#### llm-wiki 现状

当前 Agent 循环是严格串行的：每次 tool_call 必须同步等待 `execute_tool_call` 返回，才能进入下一轮 LLM 推理。写操作等待用户审批时，整个循环阻塞。

#### 借鉴意义

在 Swarm 子代理实现（§5.3/H11）中引入软中断：

```
子代理执行期间，父代理可以发出 interrupt 信号
  ↓
子代理收到信号，完成当前工具返回，不启动下一轮
  ↓
父代理获得子代理当前状态摘要，决定是否继续
```

这解决了 Swarm 中最难处理的场景："子任务偏航"——父代理不需要等子代理跑完才能纠正方向。

---

## 6. 建议的参考优先级总览

| 优先级 | 设计模式 | 建议引入轮次 | 依赖 | 风险 |
|--------|----------|-------------|------|------|
| **高** | Chat ↔ Graph 双向联动 | H13 | 纯前端 | 低，无后端改动 |
| **高** | Tool Registry 动态注册 | H10 | 无，独立实现 | 中，需重构 agent_tools.rs |
| **高** | MCP 集成 | H10（与 Tool Registry 同步） | §5.1 Tool Registry 先落地 | 中（jcode 有完整参考实现） |
| **中** | 向量检索（双路召回） | H12 | Ollama Embed API（已有） | 低，与 FTS5 互补 |
| **中** | Swarm 子代理 | H11 | H10 Tool Registry | 中，需改 agent 生命周期 |
| **中** | 软中断 + 后台工具 | H11（Swarm 配套） | Swarm 父子代理通道 | 中，tokio task 改造 |
| **中低** | 会话快照 + Journal | 当续跑性能成瓶颈时 | 无 | 低，数据模型小改 |
| **中低** | Context Compaction | 当长会话成瓶颈时 | LLM summarize 能力 | 低，prompt 层修改 |

---

## 7. 对当前开发主线的影响

**H6/H7/H9 已完成**，下一步直接进入 H10+ 扩展阶段：

```
H13 Chat ↔ Graph 双向联动  ← 纯前端，低风险，优先落地
  ↓
H12 本地向量检索（Ollama + ONNX）
  ↓
H10 Tool Registry + MCP 客户端  ← H11 的前置
  ↓
H11 Agent Swarm 子代理
```

jcode 的设计思想已全部写入对应轮次的 plan 文件：
- H13：`docs/h13-graph-chat-bridge-plan.md`
- H12：`docs/h12-local-embedding-plan.md`
- H10：`docs/h10-mcp-plan.md`
- H11：`docs/h11-swarm-plan.md`

---

## 8. 待确认问题

以下问题在深度参考代码前可进一步确认：

1. **Crate 发布状态**：jcode 的 48 个 crate 是否发布到 crates.io；如未发布，参考设计思想即可，不可直接 `cargo add`
2. **Windows 适配程度**：README 中提到 Windows 支持，但实际 Windows 测试覆盖率不明；关键路径（socket 传输、文件路径）需要在 Windows 环境单独验证
3. **ONNX runtime 在 Windows MSVC 下的编译**：由于 ONNX 是 feature-gated，可以先关闭该 feature，待 H12 实现本地 embedding 时再验证

> **许可证（已确认）**：MIT License，无使用限制。

---

## 附录 A：信息来源

| 来源 | URL | 内容 |
|------|-----|------|
| GitHub 仓库 | https://github.com/1jehuang/jcode | 主仓库 |
| DeepWiki | https://deepwiki.com/1jehuang/jcode | 自动生成的技术文档 |
| RepoRead | https://www.reporead.com/discover/jcode-technical-29-apr-26 | 架构分析 |
| AISignal | https://www.aisignal.dev/analysis/1jehuang-jcode | 产品分析 |
| Pyshine | https://pyshine.com/jcode-Next-Generation-Coding-Agent-Harness/ | 技术评测 |
| cnjack 文档 | https://cnjack.github.io/jcode/ | 官方文档镜像 |

## 附录 B：llm-wiki 当前已实现模块对照

| llm-wiki 模块 | 文件 | 对应概念 |
|---------------|------|----------|
| Agent 循环 | `src-tauri/src/agent_loop.rs` | jcode agent-runtime |
| Agent 工具 | `src-tauri/src/agent_tools.rs` | jcode Tool Registry |
| Agent 策略 | `src-tauri/src/agent_policy.rs` | jcode 内置策略 |
| Agent 服务 | `src-tauri/src/agent_service.rs` | jcode 服务端 |
| Agent 运行时 | `src-tauri/src/agent_runtime.rs` | jcode agent-runtime |
| LLM Provider | `src-tauri/src/llm/` | jcode provider-core |
| 全文检索 | `src-tauri/src/search.rs` | jcode embedding（不同路线） |
| Shell 执行 | `src-tauri/src/commands.rs` | jcode Shell Tool |
| Wiki 读写 | `src-tauri/src/state.rs` | jcode File Read/Write/Edit |
| 任务状态机 | `src-tauri/src/models_new.rs` | jcode session persistence |
