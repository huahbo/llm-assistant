# H6-S3 Shell 能力最大化与安全收敛计划

> 创建日期：2026-04-29  
> 目的：在不牺牲系统安全的前提下，把 Agent/Shell 的可用能力提升到“最大可控”。

---

## 1. 背景与目标

当前 Shell 已具备：
- Windows 下 PowerShell 执行（非交互式）
- 会话 cwd 持久化
- 流式输出
- 基础策略（destructive deny、agent write/unknown require_approval）

但仍缺：
- 策略可配置能力（按场景调节强度）
- 资源维度约束（路径/网络/命令类别）
- 审批“批次授权”能力（减少频繁打断）
- 更强审计与恢复

本计划按“先可配置、再细分、再授权优化”推进。

---

## 2. 实施阶段

## 阶段 P0（当前先做，必须先收口）

目标：
- 把 Shell 决策规则从硬编码升级为“可配置 + 可持久化 + 可读取”。

范围：
- 后端新增 `ShellPolicyConfig`（manual_unknown / agent_write / agent_unknown）
- 新增 Tauri 命令：
  - `get_shell_policy_config`
  - `set_shell_policy_config`
- `run_shell_impl` 决策逻辑改为读取当前配置执行
- 前端 SDK 补齐接口（先不强制 UI 面板）

验收：
1. `cargo test` 通过（Windows）
2. `npm run typecheck` 通过
3. 修改策略后，命令决策（auto_allow/require_approval/deny）立即生效

---

## 阶段 P1（能力上限提升的核心）

目标：
- 在“安全默认”前提下，把允许执行范围显式扩大，并可分级控制。

范围：
- 扩展策略维度：
  - `read/write/unknown/destructive`
  - `network`（curl/wget/Invoke-WebRequest 等）
  - `script`（.ps1/.bat/.cmd）
- 增加 `ShellPolicyProfile`（`strict`/`balanced`/`power_user`）
- 增加“白名单命令组”与“危险命令组”可维护清单

验收：
1. 配置切 profile 后，决策行为可预测
2. 网络命令可按 profile 配置为审批或放行
3. 关键 destructive 永久 deny（不可降级）

---

## 阶段 P2（降低人工审批摩擦）

目标：
- 在安全边界内减少重复审批，提升连续操作效率。

范围：
- 引入审批票据（Approval Ticket）：
  - 作用域：`action + cwd + session`
  - TTL：例如 5 分钟
  - 次数：例如 1/3/10 次
- 前端展示“当前授权上下文与剩余次数”

验收：
1. 同类命令在有效票据内无需重复确认
2. 过期自动恢复审批
3. 日志可追溯每次命令命中的票据

---

## 阶段 P3（审计与恢复）

目标：
- 形成“可审计、可回看、可复盘”的闭环。

范围：
- 命令审计事件结构化落库：
  - command/cwd/decision/action/source/latency/exit_code
- 高风险动作（写入、脚本、网络）支持操作摘要记录
- 增加“最近策略拒绝原因”可视化

验收：
1. 任一命令可在事件流中追踪
2. 拒绝原因可读且可定位
3. 交接时可依据审计快速复盘

---

## 3. 本轮落地边界（给接手模型）

本轮只承诺 **P0** 完整收口；P1-P3 作为后续迭代。

接手检查顺序：
1. 读 `docs/dev-status.md` 最新 TODO
2. 看 `src-tauri/src/agent_policy.rs` 与 `src-tauri/src/state.rs` 中 Shell 决策链
3. 在 Windows 执行 `cargo test` 与手工命令验证

---

## 4. 风险与约束

- 不能仅靠黑名单；必须保留“默认拒绝高危 + 可配置审批”双保险。
- 不允许为了能力上限而放开 destructive 命令。
- 不改变现有 `write_wiki/edit_wiki` 审批链的安全基线。

---

## 5. 手工验证脚本（PowerShell）

```powershell
cd src-tauri; cargo test
cd ../web; npm run typecheck
```

策略验证示例（需前端或脚本调用 set_shell_policy_config 后执行）：
1. `Get-Date`（read）应按策略放行
2. `foo-unknown-cmd`（unknown）应按当前策略 auto_allow/require_approval/deny
3. `rm -rf /` 或 `Remove-Item -Recurse` 仍应被拒绝

