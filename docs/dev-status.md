# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 读 `docs/交接状态卡.md`，确认当前接力状态
3. 读 `docs/实施过程记录.md` 最新 3 条了解背景
4. 查看下方 §活跃 TODO

## 本轮快讯（2026-05-04，Codex 接力）

### H7 Phase 1.4 ShellPolicyContext 已完成
- Codex 在 Claude 限流后接手 H7 拆分重构，先清理 `App.tsx` 行尾噪声，确认无语义 diff。
- 新增 `web/src/contexts/ShellPolicyContext.tsx`，迁出 Shell 策略配置、保存、刷新、档位切换与 dirty/saving 状态。
- `web/src/main.tsx` 已接入 `ShellPolicyProvider`；`App.tsx` 改用 `useShellPolicy()`，Settings 与 Agent 仍共用同一策略数据源。
- 本轮为前端 Context 串行临界步骤，后端不改，避免与 Claude 已提交的 H7 Phase 0~1.3 改动交叉冲突。
- 验证：`cd web && npm run typecheck` ✅；`npm run test -- --run` 仍因 WSL Rollup 可选依赖缺失启动失败；`cargo test` 因 WSL 缺 `pkg-config` 未进入业务测试。
- 下一步：H7 Phase 1.5 `ToastContext`。

### H7 Phase 1.5 ToastContext 已完成
- 新增 `web/src/contexts/ToastContext.tsx`，将全局提示与 Agent 提示状态从 `App.tsx` 移到统一 Context。
- `web/src/main.tsx` 已接入 `ToastProvider`；`App.tsx` 改用 `useToast()`，保留现有 `setStatusMessage` / `setAgentStatusMessage` 调用形态，降低本步风险。
- 验证：`cd web && npm run typecheck` ✅
- 下一步：进入 H7 Phase 2，优先拆 `SettingsModule`。

### H7 Phase 2.1 SettingsModule 已完成
- 新增 `web/src/modules/settings/SettingsModule.tsx`，将 Settings 渲染分支从 `App.tsx` 外移。
- Settings 内部直接消费 `ShellPolicyContext`；LLM Provider 表单与拖拽模式暂以 props 接入，保持行为不变。
- `App.tsx` 删除 Settings JSX 大块，仅保留 `<SettingsModule />` 路由接线。
- 验证：`cd web && npm run typecheck` ✅
- 下一步：H7 Phase 2.2 `OperationsModule`。

### H7 Phase 2.2 OperationsModule 已完成
- 新增 `web/src/modules/operations/OperationsModule.tsx`，将“运行”模块（队列 + 统计）从 `App.tsx` 外移。
- 继续复用已提取的 `QueuePanel`，队列刷新/取消/重试与统计加载通过 props 接入。
- `App.tsx` 仅保留 `<OperationsModule />` 路由接线。
- 验证：`cd web && npm run typecheck` ✅
- 下一步：H7 Phase 2.3 `InboxModule`。

### H7 Phase 2.3 InboxModule 已完成
- 新增 `web/src/modules/inbox/InboxModule.tsx`，将概览首页（运行模式、Vault 操作、摄入卡片、剪藏扩展、最近日志）从 `App.tsx` 外移。
- 本步只迁移渲染边界，摄入状态、模板初始化状态、模式切换与日志数据仍由 `App.tsx` 持有并通过 props 接入。
- `App.tsx` 仅保留 `<InboxModule />` 路由接线与队列/文件选择的桥接 handler。
- 验证：`cd web && npm run typecheck` ✅
- 下一步：H7 Phase 2.4 `LintModule`。

## 本轮快讯（2026-05-04，Claude/Opus 4.7 架构）

### H7 App.tsx 拆分重构计划（架构设计稿出炉）
- 新增 `docs/h7-app-tsx-refactor-plan.md`：完整可执行计划，Sonnet 4.6 接手即可执行
- 路线：**Context + 模块组件**（不引新依赖：无 Zustand/Redux/Router）
- 目标：App.tsx 从 13858 行降至 < 500 行，10 模块独立、5 个 Context 收口跨模块状态
- 阶段划分：Phase 0（4 个内嵌组件外移） → Phase 1（建立 5 个 Context） → Phase 2（9 个模块按风险递增提取） → Phase 3（收口验证）
- 预计 15-20 个 commit，每个 commit 独立 typecheck + cargo test 验证
- Sonnet 4.6 限流时可断点续作（计划里有 §5 进度勾选表）

### write_wiki / edit_wiki E2E 状态变更
- **决策**：跳过用户手测，以已有自动化测试为准
- 覆盖证据：4 条 Rust 测试已落 `src-tauri/src/state.rs`
  - `approve_agent_write_full_write_creates_file` ✅
  - `reject_agent_write_does_not_create_file` ✅
  - `approve_agent_write_patch_replaces_content` ✅
  - `approve_agent_write_patch_fails_when_old_str_not_found` ✅
- 详见 `docs/h7-app-tsx-refactor-plan.md` §0.1

### 代码质量加固（H6-P1 收口后）
- `db.rs`：新增 `get_agent_run_by_id()`，O(1) 查询替代原 list_agent_runs(500) 全表扫
- `state.rs`：`archive_agent_run_impl` 改用单行查询
- `agent_policy.rs`：match 去冗余 guard；`is_script_command` 修 `-file` 误判；新增 `bash xxx.sh` 检测
- `App.tsx`：`handleSaveShellPolicy` / `handleApplyAndSaveShellPolicyProfile` 补 `isTauriRuntime()` 前置守卫
- 评分：agent_policy A / db.rs A / state.rs archive A- / App.tsx handlers A-

---

## 本轮快讯（2026-04-30，Claude 收口）

### H6-P1 Shell 策略扩展
- `ShellPolicyConfig` 新增 `network_decision` / `script_decision`（默认 `require_approval`）
- 新增 `ShellPolicyProfile` 枚举与 `from_profile()`：`strict` / `balanced` / `power_user`
- 分类优先级：`destructive > script > network > read/write/unknown`
- `NETWORK_COMMANDS` 白名单 16 个（curl/wget/iwr/ping/ssh 等）
- `is_script_command`：.ps1/.bat/.cmd/.sh 及 `powershell -file` 调用检测
- Settings UI 新增 network/script 两个策略下拉控件
- 档位预设 strict/balanced/power_user 同步升级含 network/script 字段
- cargo test：**233 通过，0 失败**（新增 policy 测试 ×8 + archive/restore 约束测试 ×3）
- npm run typecheck：零错误

### 自动化测试覆盖（P0/P1 手测项代码验证）
- `archive_agent_run_rejects_running_status`：running 状态禁归档 ✅
- `archive_agent_run_rejects_when_pending_write_exists`：pending write 禁归档 ✅
- `archive_and_restore_agent_run_round_trip`：归档→恢复→幂等约束 ✅
- `approve_agent_write_full_write_creates_file` / `patch_replaces_content` / `patch_fails_when_old_str_not_found`：审批链路三条路径 ✅（已有）

---

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
- 端到端验证辅助：任务模式新增“写入审批验证 / 编辑审批验证”一键填充指令，便于快速手测审批链路。
- Agent Studio 右侧布局按“方案2”收敛：改为“主内容滚动区 + 底部 Shell 抽屉”。
  - Shell 抽屉固定在右侧底部，展开后输入框始终可见，不再依赖拉高窗口。
  - 工具入口改为右上显式按钮（打开/收起工具能力）+ 抽屉二级开关，避免误判“未展开”。
  - 清理旧的未使用 shell 容器样式，完成 JSX/CSS 对齐。

### 2026-04-29（Codex 继续）
- Agent Studio Shell 升级为“会话型流式终端（方案 B）+ 深色皮肤”：
  - 工具区加入快捷命令、历史清空、单条输出复制、`↑/↓` 历史回填、流式状态 badge。
  - 输入区固定在工具卡底部，长历史时仍可持续输入，不再被输出区挤走。
  - 深色终端皮肤重做（背景层次、边框对比、提示/输出可读性）。
- 后端 Shell 兼容增强：
  - `ls/ls -a/ls -la/ls --all` 在 manual 模式统一翻译为 `Get-ChildItem`，避免 PowerShell 参数歧义。
  - `cd` / `cd /d <path>` 在会话模式下作为内建命令处理并持久化 cwd。

### 2026-04-29（Codex 新推进）
- 新增独立计划文件：`docs/h6-shell-max-safety-plan.md`（P0~P3 全路径，便于限流后接手）。
- H6-S3 P0 已开工并落地后端骨架：
  - `ShellPolicyConfig` 入模（可持久化到 `app-config.json`）。
  - `run_shell_impl` 改为读取配置执行策略。
  - 新增 Tauri 命令：`get_shell_policy_config` / `set_shell_policy_config`。
  - 前端 SDK 新增同名接口，并已在 Agent 工具页接入最小策略面板（3 个决策旋钮 + 保存 + 档位预设）。

### 2026-04-30（Codex 继续）
- H6-S3 P1 小步推进：Shell 策略从 3 维扩展为 5 维：
  - 新增 `manual_write_decision`
  - 新增 `agent_read_decision`
- 策略面板与预设档位已同步升级，能更细粒度控制“高能力”放行边界。
- Settings 模块已新增“Shell 策略（全局）”入口，与 Agent 工具页策略面板共用同一配置。
- 按最新 UI 红线完成收敛：
  - Settings 已拆分为 3 个独立模块：`LLM Provider 配置` / `拖拽行为` / `Shell 策略（全局）`。
  - Agent 工具页 Shell 策略区已瘦身为“仅档位按钮”，移除重复 5 项下拉，释放终端可视空间。
- 继续推进“Agent 运行历史 UI”：
  - 左侧聊天区顶部新增 `历史 Runs` 横向卡片条（按更新时间倒序）。
  - 每个 run 显示状态标签与时间，点击可快速切换当前 run。
  - run 状态可见性增强（running/reviewing/applied/failed/queued 颜色语义）。

### 2026-04-30（Codex 继续）
- 历史 Runs 收敛为 B 方案 + 软删除：
  - 展开区改为“竖向限高列表 + 内部滚动”，移除横向长滑问题。
  - 增加“管理模式（显示已归档）”切换。
  - 新增软删除链路：`archive_agent_run` / `restore_agent_run`，默认列表隐藏归档 run。
  - 归档安全约束：`running/reviewing` 禁止归档；存在待审批写入时禁止归档。

### 2026-04-30（Codex 继续）
- 历史 Runs 细节修复：
  - “归档”按钮样式已修复为横向显示（避免竖排）。
  - 管理模式语义强化：非管理模式仅显示未归档项；管理模式显示全部且提供“归档/恢复”操作。

### 2026-04-30（Codex 继续）
- 修复“运行模块空白”：
  - 已新增 `operations` 页面渲染分支。
  - 队列+统计已并入“运行”模块内页签（任务队列/运行统计）。

### 2026-04-30（Codex 继续）
- 已移除 `queue/stats` 旧路由冗余：
  - `ModuleId` 去除 `queue`、`stats`。
  - 删除旧页面分支 `activeModule === "queue/stats"`。
  - 保留唯一链路：`operations` 模块 + 内部页签。

### 2026-04-30（Codex 继续）
- 运行模块视觉微调收口（运行页签头 / 间距 / 空态提示）：
  - 页签下新增动态提示文案（队列/统计各自说明）。
  - 压缩运行模块页签区间距，统一内容区节奏与留白。
  - 队列空态升级为“标题 + 下一步动作”提示卡。
  - 统计空态升级为边框提示卡，文案更明确（初始化或先摄入）。
  - `QueuePanel` 去除重复外层标题与 panel 包裹，改为嵌入式内容块。
  - 验证：`cd web && npm run typecheck` ✅
- Claude 专用接手稿：`docs/claude-handoff-2026-04-30.md`

---

## 验证基线（2026-04-30）

```powershell
cd src-tauri; cargo test          # 233 通过 ✅
cd ../web; npm run typecheck      # 零错误 ✅
```

> 说明（WSL 本轮）：`cd web; npm run test -- --run` 在当前 Linux 侧因缺失可选依赖 `@rollup/rollup-linux-x64-gnu` 失败；且 `npm i` 会被 `@tauri-apps/cli-win32-x64-msvc` 平台限制拦截。建议在 Windows 本机环境执行前端测试。

---

## 最新提交（main 分支，最近 5 条）

| commit | 描述 |
|--------|------|
| `9b5e3a7` | refactor: 代码质量提升至 A — db/policy/state/App 四处加固 |
| `aa8f9e1` | docs: 更新 dev-status 基线 233 + P1 完成 + TODO 重排 |
| `5d6355d` | feat(H6-P1): 扩展 Shell 策略 network/script 两个维度 + 档位预设 + 自动化测试 |
| `44f9029` | chore: ignore opencode.json |
| `109319c` | chore: 清理 docs/archive 旧路径 |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 🔴 0 | **H7 App.tsx 拆分重构** | 计划已出，待 Sonnet 4.6 执行 | 按 `docs/h7-app-tsx-refactor-plan.md` Phase 0→3 增量执行；15-20 个 commit |
| 🟢 1 | **write_wiki / edit_wiki E2E** | 自动化覆盖（用户决定跳过手测） | `state.rs` 4 条测试已覆盖，详见 h7 计划 §0.1 |
| 🟢 2 | **H6-P1 Shell 策略扩展 + 代码加固** | 已完成（2026-04-30） | network/script + Profile + 233 测试 + db/policy/state/App 四处加固 |
| 🟢 3 | **H6-S3 archive/restore 约束** | 已测试（2026-04-30） | 3 条自动化测试：running 禁归档、pending write 禁归档、归档恢复闭环 |
| 🟢 4 | **Agent 运行历史 UI** | 已完成（2026-04-30） | 新增历史 runs 卡片条，可点击切换并查看对应事件流 |
| 🟢 5 | **上下文面板实质注入** | 已完成（2026-04-29） | `memory_context` 已进入 `build_loop_prompt`；`{{topic}}/{{memories}}` 在任务模式可用 |
| 🟡 6 | **H6-P2 审批票据（降低摩擦）** | 未开始 | 作用域 action+cwd+session，TTL 5min，见 h6-shell-max-safety-plan.md P2 |
| 🟡 7 | **H6-P3 审计落库** | 未开始 | 命令+决策+exit_code 结构化落库，见 h6-shell-max-safety-plan.md P3 |
| 🟡 8 | **styles.css / tauri-client.ts 拆分** | 未开始 | 在 H7 之后做，避免一次拆太多文件 |

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
