# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 读 `docs/交接状态卡.md`，确认当前接力状态
3. 读 `docs/实施过程记录.md` 最新 3 条了解背景
4. 查看下方 §活跃 TODO

## 本轮快讯（2026-04-29，Claude 收口）

### H6-S2 工具链补全
- `edit_wiki` 工具已实现：`AgentToolAction::EditWiki { path, old_str, new_str }`，`replacen` 字符串替换，走与 `write_wiki` 相同审批流
- `PendingAgentWrite.old_str: Option<String>` 区分全写（None）与 patch（Some）
- `approve_agent_write` 双模式：有 old_str 时执行 patch，无则全写
- loop prompt 增加 edit_wiki 工具 #5，规则：修改现有页面用 edit_wiki，新建用 write_wiki
- cargo test：**213 通过，0 失败**

### Agent Studio UI 完善
- 内联执行日志（tool_start/tool_end/awaiting_approval 直接显示在任务区）
- 三栏拖拽分割 + 侧边栏折叠为图标模式（52px）
- `body.split-dragging` 持久高亮分割线
- 聊天框内滚动修复：module-viewport--agent + flex:1 传导，chat-thread 内部 overflow: auto 生效

### 窗口 UI 打磨
- 标题栏高 32px，按钮 44px，X=14px / —=11px translateY(-2px) / □=11px 1.5px边
- DWM shadow 保留（投影+三侧边框），CSS border-top 补顶边，焦点监听动态切换颜色深浅
- `data-tauri-drag-region` 修复窗口拖动

### 2026-04-29（Codex 续做）
- 已补齐任务模式“上下文面板实质注入”链路：
  - 前端 `runAgentTask` 传入 `memory_context`
  - 后端 `run_agent_task` 命令签名与服务链路接收 `memory_context`
  - `build_loop_prompt` 新增“上下文配置（记忆/技能）”段落
- 技能模板变量在任务模式生效：`{{topic}}` / `{{memories}}` 在前端传参前完成替换后注入。
- 工具调用可视化升级：任务区执行日志已升级为“结构化工具时间线”（start/end 配对、耗时、详情折叠）。
- 任务续跑最小闭环：新增“继续任务”按钮，自动拼接原指令 + 最近工具轨迹 + 上次结果，支持同 run 快速恢复执行。

---

## 验证基线（2026-04-29）

```powershell
cd src-tauri; cargo test          # 213 通过 ✅
cd ../web; npm run typecheck      # 零错误 ✅
```

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `9e23631` | Reclaim Agent Studio workspace by collapsing non-critical controls |
| `f4f9708` | fix: agent 聊天框内滚动 + 更新 dev-status 待补任务 |
| `04128ea` | style: agent 聊天框背景色优化 |
| `6b482b1` | fix: 最小化 — 向上偏移修正垂直居中 |
| `4acd166` | fix: 标题栏窗口控制按钮细节对齐 |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 🔴 1 | **write_wiki / edit_wiki 端到端验证** | 待用户手测 | 任务模式触发审批 → 批准 → 验证文件写入；拒绝 → 验证无变化 |
| 🔴 2 | **Agent 运行历史 UI** | 待开发 | 左侧聊天区应能列出历史 runs 并点击查看各 run 的事件流；目前只有当前 run 可见 |
| 🟢 3 | **上下文面板实质注入** | 已完成（2026-04-29） | `memory_context` 已进入 `build_loop_prompt`；`{{topic}}/{{memories}}` 在任务模式可用 |
| 🟢 4 | **工具调用结构化可视化** | 已完成（2026-04-29） | 已支持 `tool_start/tool_end` 配对、耗时显示、详情折叠 |
| 🟢 5 | **任务续跑 / 错误恢复** | 已完成（2026-04-29，最小闭环） | 新增“继续任务”按钮，基于历史轨迹续跑；后续可再做精细 checkpoint 恢复 |
| 🟢 6 | **任务模式 vs 草稿模式 UX 清晰度** | 待评估 | 用户是否清楚两个模式的区别？入口是否足够清晰 |
| 🟢 7 | **run 状态标签可见度** | 待评估 | pending/reviewing/done/failed 状态在 UI 里是否显眼 |

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
