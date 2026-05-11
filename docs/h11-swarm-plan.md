# H11：Agent Swarm 多层子代理实施计划

> 状态：待实施 | 优先级：中
> 依赖：H10 Phase A（ToolRegistry）✗ 未完成

---

## 1. 目标

让 llm-wiki 的 Agent 能够**递归地 spawn 子代理**处理子任务，实现并行执行和上下文隔离：

```
父 Agent（用户直接交互）
  ├─→ 子代理 A（读 20 篇 PDF，输出摘要）
  │     └─→ 孙代理 A1（处理 PDF 1-5）
  ├─→ 子代理 B（交叉引用分析）
  └─→ 子代理 C（更新 15 个 Wiki 页面）
```

核心设计原则：
- **上下文隔离**：子代理有独立 message history，不污染父代理 context window
- **结构化返回**：子代理完成后返回不超过 4KB 的摘要，不是全文转录
- **深度限制**：最多 3 层递归，防止无限嵌套
- **软中断**：父代理可以在子代理运行中发出中断信号（参考 jcode 设计）

---

## 2. 技术背景

### 2.1 现有 Agent 基础设施

| 组件 | 位置 | 状态 |
|------|------|------|
| `agent_runs` 表 | `src-tauri/src/db.rs` | ✅ 已有 `id, instruction, status, final_output, created_at, updated_at` |
| `agent_run_events` 表 | `src-tauri/src/db.rs` | ✅ 事件流记录 |
| Agent ReAct Loop | `src-tauri/src/agent_chat/runtime.rs` | ✅ 单 Agent 循环 |
| Tauri 命令 | `src-tauri/src/commands.rs` | ✅ start_agent_run, get_agent_run 等 |

### 2.2 jcode 参考设计

- 父代理通过 `spawn_subagent(task: str) -> str` 工具调用触发子代理
- 子代理作为独立 tokio task 运行，父代理立即返回（non-blocking spawn）
- 子代理完成后通过 `agent_run_events` 机制通知父代理（run_id 关联）
- 深度由 `run_depth` 字段追踪（0=根，1=子，2=孙，3=最大）
- 软中断：父代理可以向子代理的 `interrupt_rx: watch::Receiver<bool>` 发送信号

---

## 3. 实施方案

### Phase A：数据库 Schema 扩展

#### 3A.1 agent_runs 表新增列

```sql
ALTER TABLE agent_runs ADD COLUMN parent_run_id TEXT;   -- 父代理 run_id，根代理为 NULL
ALTER TABLE agent_runs ADD COLUMN run_depth INTEGER NOT NULL DEFAULT 0;  -- 递归深度
ALTER TABLE agent_runs ADD COLUMN summary TEXT;  -- 子代理完成后的结构化摘要（≤4KB）
```

在 `src-tauri/src/db.rs` 中更新 `AgentRun` 结构体和查询语句。

#### 3A.2 子代理查询支持

```rust
pub fn get_child_runs(db_path: &Path, parent_run_id: &str) -> Result<Vec<AgentRun>> {
    // SELECT * FROM agent_runs WHERE parent_run_id = ?1
}
```

---

### Phase B：spawn_subagent 工具

#### 3B.1 工具注册

在 H10 的 ToolRegistry 中注册 `SpawnSubagentTool`：

```rust
pub struct SpawnSubagentTool;

#[async_trait]
impl Tool for SpawnSubagentTool {
    fn name(&self) -> &str { "spawn_subagent" }
    fn description(&self) -> &str {
        "在独立上下文中启动子代理处理子任务。子代理完成后返回结构化摘要。"
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string", "description": "子代理的任务描述" },
                "tools": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "子代理可用的工具列表（空=继承父代理所有工具）"
                },
                "wait": { "type": "boolean", "description": "是否等待子代理完成后再继续（默认 false）" }
            },
            "required": ["task"]
        })
    }
    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
        // 检查深度限制
        // 创建子 AgentRun（parent_run_id = ctx.run_id, depth = ctx.depth + 1）
        // tokio::spawn 独立任务运行子 Agent Loop
        // 如果 wait=true，等待子代理完成并返回 summary
        // 如果 wait=false，立即返回子代理 run_id
    }
}
```

#### 3B.2 深度限制

```rust
const MAX_SWARM_DEPTH: u32 = 3;

async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolOutput, ToolError> {
    if ctx.run_depth >= MAX_SWARM_DEPTH {
        return Err(ToolError::Policy("已达最大子代理嵌套深度 (3)".to_string()));
    }
    // ...
}
```

#### 3B.3 子代理 Loop 启动

```rust
async fn spawn_subagent_loop(
    task: String,
    parent_run_id: String,
    depth: u32,
    interrupt_tx: watch::Sender<bool>,
    ctx: Arc<SubagentContext>,
) -> Result<String> {
    let run_id = create_agent_run(&ctx.db_path, &task, Some(&parent_run_id), depth)?;
    let interrupt_rx = interrupt_tx.subscribe();

    // 运行 ReAct Loop（复用 run_agent_loop，传入 interrupt_rx）
    let result = run_agent_loop(run_id.clone(), task, interrupt_rx, ctx).await;

    // 让 LLM 生成摘要（≤4KB）
    let summary = generate_summary(&result, &ctx.llm_provider).await?;
    update_run_summary(&ctx.db_path, &run_id, &summary)?;
    Ok(summary)
}
```

---

### Phase C：软中断机制

#### 3C.1 Agent Loop 软中断支持

在 `runtime.rs` 的 ReAct Loop 中，每次迭代检查中断信号：

```rust
loop {
    // 检查软中断
    if *interrupt_rx.borrow() {
        return Ok(AgentResult::Interrupted { partial_output: accumulated_output });
    }

    // 正常 LLM 推理 + 工具调用
    let response = llm.complete(messages).await?;
    // ...
}
```

#### 3C.2 父代理中断子代理

父代理通过新增 Tauri 命令 `interrupt_agent_run(run_id: String)` 发送中断信号：

```rust
#[tauri::command]
pub async fn interrupt_agent_run(run_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.interrupt_agent_run(&run_id)?;
    Ok(())
}
```

`AppState` 维护 `run_interrupt_senders: DashMap<String, watch::Sender<bool>>`。

---

### Phase D：前端任务树可视化

#### 3D.1 子代理列表展示

在 Agent Studio 中，运行中的 Agent 下方显示子代理树：

```
▶ 主代理 (run_abc) — 正在运行
  ├─ 子代理 A (run_def) — ✓ 完成 (摘要: "已读取 5 篇 PDF...")
  ├─ 子代理 B (run_ghi) — ⏳ 运行中
  │   ├─ 孙代理 B1 (run_jkl) — ✓ 完成
  │   └─ 孙代理 B2 (run_mno) — ⏳ 运行中
  └─ 子代理 C (run_pqr) — ○ 等待中
```

#### 3D.2 子代理摘要折叠展示

每个子代理卡片可展开查看摘要（≤4KB），不展示完整 event 流（减少 UI 噪声）。

#### 3D.3 中断按钮

运行中的子代理旁边显示 [中断] 按钮，调用 `interrupt_agent_run`。

---

## 4. 文件变动清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `src-tauri/src/db.rs` | 修改 | agent_runs 新增 3 列，`get_child_runs()` |
| `src-tauri/src/agent_chat/runtime.rs` | 修改 | 软中断支持，父子代理上下文传递 |
| `src-tauri/src/agent/tools/spawn_subagent.rs` | 新建 | SpawnSubagentTool |
| `src-tauri/src/state.rs` | 修改 | `run_interrupt_senders: DashMap<>` 管理 |
| `src-tauri/src/commands.rs` | 修改 | 新增 `interrupt_agent_run`, `get_child_runs` |
| `src-tauri/src/models_new.rs` | 修改 | AgentRun 结构体新增字段 |
| `web/src/modules/agent/AgentRunTree.tsx` | 新建 | 子代理树状展示组件 |
| `web/src/modules/agent/AgentStudio.tsx` | 修改 | 嵌入 AgentRunTree |
| `web/src/tauri-client/` | 修改 | `interruptAgentRun`, `getChildRuns` |
| `web/src/modules/agent/agent.css` | 修改 | 树状视图样式 |

---

## 5. 验收标准

- [ ] Agent 可以通过 `spawn_subagent` 工具创建子代理
- [ ] 子代理在独立 tokio task 中运行，父代理不阻塞（`wait=false` 时）
- [ ] 子代理完成后，summary ≤ 4KB 并存入 `agent_runs.summary`
- [ ] 深度超过 3 时，`spawn_subagent` 返回错误，父代理继续运行
- [ ] [中断] 按钮可以提前终止运行中的子代理
- [ ] UI 显示完整的子代理树状结构（父子关系、状态、摘要）
- [ ] `cargo test` 全绿；`npm run typecheck` 零错误

---

## 6. 风险与注意事项

1. **ReAct Loop 并发安全**：多个子代理并行运行时，`AppState` 中的 `Mutex` 锁竞争可能成瓶颈。评估是否需要将重型锁拆分（如 pending_writes 从全局 Mutex 改为 per-run 的 Mutex）。
2. **LLM 摘要生成开销**：子代理完成后调用 LLM 生成摘要增加额外延迟。可以在实现初期让子代理的 `final_output` 直接作为摘要（如果 ≤4KB），超出则截断。
3. **DB migration**：新增 3 列需要迁移脚本。确保现有数据（parent_run_id=NULL, run_depth=0）在默认值下语义正确。
4. **tokio task 泄漏**：子代理 spawn 后父代理需要有机制知道所有子 task 完成。使用 `JoinSet<>` 管理，App 退出时 `abort_all()`。
5. **中断信号传播**：父代理中断时，应该级联中断所有子代理（DFS 遍历子代理树，逐一发送中断）。

---

## 7. 工作量估算

| Phase | 估算 | 关键风险 |
|-------|------|---------|
| A（DB Schema 扩展） | 0.5 天 | migration，数据一致性 |
| B（spawn_subagent 工具） | 2 天 | tokio task 生命周期 |
| C（软中断机制） | 1.5 天 | watch channel 跨 task |
| D（前端任务树 UI） | 1.5 天 | 递归树组件，轮询子代理状态 |
| **总计** | **~5.5 天** | B + C 是核心复杂度 |

---

## 8. 最小可行版本（MVP）建议

如果工作量超预期，可以先实现 MVP：
- ✅ Phase A（DB Schema）
- ✅ Phase B（spawn_subagent，wait=true 同步模式）
- ❌ Phase C（软中断，推迟到下一轮）
- ✅ Phase D 简化版（只显示子代理列表，无树状，无中断按钮）

MVP 工作量约 **3 天**，可以先验证核心 Swarm 逻辑，软中断和完整 UI 留待后续。
