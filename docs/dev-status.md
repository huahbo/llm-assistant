# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 读 `docs/交接状态卡.md`，确认当前接力状态
3. 读 `docs/实施过程记录.md` 最新 3 条了解背景
4. 查看下方 §活跃 TODO

## 本轮快讯（2026-04-28，Claude 收口）

- 已修复 Windows 任务栏白色方块图标：`src-tauri/icons/icon.ico` 替换为多尺寸 ico，并在 `tauri.conf` 显式绑定窗口图标。
- H6-S2 模块化第一步已完成：新增 `src-tauri/src/agent_tools.rs`，决策解析逻辑已从 `state.rs` 抽离。
- H6-S2 继续收敛：新增 `src-tauri/src/agent_loop.rs`，`run_agent_task` 的 prompt 组装已迁出。
- H6-S2 主循环已迁移到 `agent_loop::run_agent_task_loop`，`state.rs` 仅保留运行入口与 runtime 实现。
- H6-S2 总结阶段已迁移到 `agent_loop::summarize_agent_task`，任务模式闭环进一步去 state 化。
- H6-S2 runtime 实现已迁移到 `src-tauri/src/agent_runtime.rs`，`state.rs` 进一步瘦身。
- H6-S2 runtime 工具执行已按分支拆分（shell/search_wiki/read_wiki），便于后续扩展审批流与新工具。
- H6-S2 PathGuard（read）已接入：`read_wiki` 仅允许访问 `vault/wiki/*.md`。
- H6-S2 `read_wiki` 已从 `state.rs` 下沉到 `agent_runtime.rs`，state 继续瘦身。
- H6-S2 runtime 返回结果已统一为 `ToolExecOutcome`，并补充了 `read_wiki` 集成测试。
- H6-S2 主服务已下沉到 `src-tauri/src/agent_service.rs`，`run_agent_task_impl` 变为薄委托。
- H6-S2 PathGuard(write) 已前置：新增 `validate_agent_write_path`（仅允许 `vault/wiki/*.md`）。
- H6-S2 `write_wiki` 工具链已打通到审批前置：支持决策解析/事件展示/路径校验，当前统一 `require_approval` 且不落盘。
- H6-S2 循环新增审批暂停语义：遇到 `decision=require_approval` 会写入 `awaiting_approval` 事件并中止后续迭代。
- **Claude 质量收口**：`ToolActionResult` struct 替代字符串解析，`requires_approval` 靠字段判断；`PendingAgentWrite` 存储 + `approve/reject_agent_write` 命令 + 前端审批确认栏；代码质量 B+ → **A**。

---

## 验证基线（2026-04-27 H5 全部推送，Windows 复核通过）

```powershell
cd src-tauri; cargo test          # 通过 ✅
cd ../web; npm run typecheck      # 通过 ✅
```

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `ad0bc6f` | feat(H6-S2): write_wiki 审批确认闭环 + ToolActionResult 重构 |
| `6d4f2a0` | Stabilize Agent S2 by modularizing runtime loop before next phase |
| `81ce437` | docs: H6 计划落盘（Shell Tool + Agentic Loop 两阶段方案） |
| `262edc6` | chore: H5 Windows 验证全绿，更新基线 |
| `dcd3f0a` | docs: 归档实施过程记录 + 整理协作文档 + 更新 README |
| `690fa87` | docs: H5 全部收口 |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 1 | **H6-S2 Windows 验证** | 待用户 | cargo test；write_wiki 触发审批 → 批准/拒绝端到端测试 |
| 2 | **H6-S2 edit_wiki（patch）** | 待开发 | 审批验证通过后，增加 edit_wiki 增量修改工具 |

---

## H6 详细计划

### 背景

Agent Studio 目前是"一次性 LLM 调用 → 生成草稿"，H6 目标是升级为**可操作本机的真实 Agent**——LLM 自主决策调用工具（shell、文件读写），配合三层护栏保证 OS 安全。参考项目：`refer-rust-daerwen-agent/`（Rust + Tauri，与本项目技术栈完全一致）。

---

### H6-S1：Shell Tool MVP（待手测）

**目标**：Agent Studio 支持手动执行 shell（PowerShell/bash），用于后续 agent 工具调用打底。  
**当前收口项**：
1. 前端排版：补齐 `.agent-studio__shell*` 样式并做移动端适配。
2. 前端编译：修复 `React.useRef` 模块化导入错误（改为 `useRef`）。
3. 最小验证：`npm run typecheck` 通过；Windows 端手动跑 `Get-Date` / 非法命令回显。

---

### H6-S1.5：安全前置（新增，S2 前必须完成）

**目标**：在真正 agent 自动执行前，先把策略判定前置到 `run_shell`，避免黑名单单点风险。  
**实施要点**：
1. `ShellResult` 新增策略元信息：`policy_action`、`policy_decision`、`executor`。
2. `run_shell` 新增 `source` 入参（`manual|agent`），默认 `manual`。
3. 后端增加最小策略分类：
   - destructive → `deny`（直接拦截）
   - `source=agent` 且 write/unknown → `require_approval`（先拦截，等待审批流）
   - 其他 → `auto_allow`
4. 前端 shell 历史展示策略元信息，便于调试和交接。

**验收标准**：
1. `npm run typecheck` 通过；
2. 手动执行常见只读命令正常；
3. 高危命令返回 `blocked=true` 且带策略原因；
4. （后续）`source=agent` 的写入命令被要求审批而非直接执行。

---

### H6-S2：Agentic Tool-Call Loop（daerwen 移植）

> **前置**：H6-S1 与 H6-S1.5 验收通过后才开始 S2。

**目标**：Agent Studio 新增"任务模式"——用户输入自然语言指令，LLM 自主循环调用工具（shell、文件读写、wiki 检索）直到完成任务。

**当前进展（已完成 skeleton+next）**：
1. 后端新增 `run_agent_task` 命令与 `run_agent_task_impl`。
2. `run_agent_task_impl` 已支持多轮决策与受控工具调用（`run_shell/search_wiki/read_wiki/write_wiki`）。
3. 前端新增任务模式 Beta 面板（任务指令、预算轮次、结果区）。
4. 任务执行后自动写入 run events，并将 run 状态置为 `reviewing`。
5. `run_shell` 策略判定已统一到 `src-tauri/src/agent_policy.rs`，避免重复实现漂移。
6. `write_wiki` 当前走审批前置策略：触发 `require_approval` 日志，暂不执行真实写盘。

#### 要移植的模块（来自 `refer-rust-daerwen-agent/`）

| 源路径 | 目标路径 | 说明 |
|--------|----------|------|
| `crates/daerwen-tools/src/builtins.rs` | `src-tauri/src/agent_tools.rs`（新建） | `ReadFile/WriteFile/EditFile/ShellTool` + `ToolHandler` trait |
| `crates/daerwen-policy/src/lib.rs` | `src-tauri/src/agent_policy.rs`（新建） | `PathGuard` 5区分级 + `PolicyEngine` |
| `crates/daerwen-agent/src/lib.rs` | `src-tauri/src/agent_loop.rs`（新建） | `Agent::run_with_history` 多迭代 tool-call loop |

#### 关键适配点

1. **BashTool → ShellTool**：`Command::new("bash")` 改为 Windows/Unix 条件编译（同 S1 方案）
2. **PathGuard workspace**：绑定到当前 vault 路径而非 `~/.daerwen`
3. **LlmClient**：复用现有 `get_llm_provider()` 而非 daerwen 的独立 llm crate
4. **工具集**：`read_file / write_file / edit_file / run_shell / search_wiki / read_wiki`（后两个接现有 BM25 检索）
5. **Callbacks**：通过现有 Tauri `app_handle.emit` 把 `on_tool_start/on_tool_end` 推送到前端事件流

#### 新增 Tauri 命令

```rust
// 执行 agent 任务（多轮工具调用）
run_agent_task(run_id: i64, instruction: String, max_iterations: Option<u32>, state)
  -> Result<String, String>  // 返回最终 LLM 文本
```

#### 前端变更

- Agent Studio 新增"任务模式"标签（与现有草稿模式并列）
- 任务输入框 + "运行任务"按钮
- 工具调用实时可视化：每次 `on_tool_start` 事件显示 `🔧 tool_name(input...)`，结束后折叠
- 最终答案区

#### 交接注意（给 Claude/Codex/Gemini）

1. 先完成 S1/S1.5 收口，再进入 S2；不要跳阶段并行改 S2。
2. 若遇到“技能不能自由选择”反馈，先确认 `skill_key` 唯一 upsert 语义（不是多选能力缺失）。
3. `opencode.json` 是协作辅助文件，保持忽略，不纳入提交。

---

## H5 功能速查（已完成）

| 功能 | 入口 | 说明 |
|------|------|------|
| 自动记忆提炼 (B) | 审批写盘后自动触发 | LLM 从草稿提炼 3-5 条全局记忆；事件日志记录数量 |
| 批注重写 (A) | 草稿头"基于批注重写"输入栏 | 原草稿 + 批注 → LLM → 新草稿；自动选中 |
| Ask 联动 (C) | 输入栏"Ask 联动"checkbox | 先 query_ask(top_k=3) 获取现有知识库答案注入 prompt |
| Skill 模板变量 (D) | Skill prompt 内写 `{{topic}}` `{{memories}}` | 生成时自动替换，skill 可复用 |
| 检索增强 (H4) | 输入栏"检索增强"checkbox | 读 wiki 正文 400 字，搜 8 条 |

---

## 关键架构约束

- **LLM vs Embed 分离**：LLM 走 `get_llm_provider()`；Embed 走 `get_embed_provider()`（本地 Ollama）
- **Tauri 异步命令**：带引用参数必须返回 `Result<T, String>`
- **API Key 禁止入仓**
- **审批约束**：写盘必须经确认弹窗，禁止静默覆盖
- **Shell 安全**：黑名单拦截 + 超时控制（max 120s）；S2 加 PathGuard 5区分级
- **参考项目**：`refer-rust-daerwen-agent/`，技术栈与本项目完全一致，可直接移植
