# H14 — Chat Agent Shell 能力集成计划

> 状态：待实施  
> 目标：为对话 Agent 添加 Shell 执行工具，支持 **Yolo（免审批）** 和 **审批** 两种模式

---

## 一、背景与目标

当前 Chat Agent 只有 `web_search`、`fetch_url`、`read_wiki_page`、`search_wiki` 四个工具，  
无法操作本地文件系统或执行命令。本计划将 `run_shell` 工具引入 Chat ReAct 循环，  
并通过对话级 `shell_mode` 控制安全边界。

**两种模式定义：**

| 模式 | 行为 | 适用场景 |
|------|------|---------|
| `off` | 拒绝执行，返回"未启用"提示 | 默认，纯文本对话 |
| `yolo` | LLM 调用即执行，不弹窗 | 信任场景、快速测试 |
| `approval` | 暂停 ReAct 循环，前端弹审批卡 | 生产/敏感操作 |

---

## 二、现有基础（可复用）

| 组件 | 可复用内容 |
|------|-----------|
| `state.rs::run_shell_impl` | 完整的 shell 执行 + 策略分类 + 审计日志 |
| `agent_policy.rs` | 命令分级（destructive/write/read 等）、ticket 缓存 |
| `agent_chat/tools.rs::ToolExecResult` | `awaiting_approval: Option<i64>` 字段已有 |
| `agent_chat/runtime.rs` | `chat_awaiting_approval` 事件 + oneshot 等待机制 |
| `ToolCallCard.tsx` | `status="awaiting_approval"` 审批 UI 框架 |
| `commands.rs::approve_and_run_shell` | 审批后直接执行的命令 |

**核心复用策略：** wiki write 审批流程（emit → oneshot → approve/reject）几乎原样照搬给 shell。

---

## 三、改动清单

### Phase 1 — 数据库 & 配置 API（后端）

**1.1 `agent_chat/db.rs` — 表结构迁移**

```sql
ALTER TABLE agent_conversations
  ADD COLUMN shell_mode TEXT NOT NULL DEFAULT 'off';
```

同时更新建表 SQL，新建对话默认 `shell_mode = 'off'`。

**1.2 `agent_chat/commands.rs` — 新增两个 Tauri 命令**

```rust
/// 读取对话的 shell_mode
#[tauri::command]
pub async fn get_conv_shell_mode(conv_id: i64, state: State<AppState>) -> Result<String, String>

/// 设置对话的 shell_mode（"off" / "yolo" / "approval"）
#[tauri::command]
pub async fn set_conv_shell_mode(conv_id: i64, mode: String, state: State<AppState>) -> Result<(), String>
```

**1.3 `main.rs`** — 注册两个新命令

---

### Phase 2 — Shell 工具注册（后端）

**2.1 `agent_chat/db.rs` — 初始化时插入 run_shell 工具**

```sql
INSERT OR IGNORE INTO agent_tools (name, description, parameters_schema, handler_kind, enabled)
VALUES (
  'run_shell',
  '在本地执行 Shell 命令，返回 stdout/stderr 和退出码。仅在用户明确授权后可用。',
  '{
    "type": "object",
    "properties": {
      "command": {"type": "string", "description": "要执行的命令"},
      "cwd":     {"type": "string", "description": "工作目录（可选，默认项目根目录）"},
      "timeout_ms": {"type": "integer", "description": "超时毫秒数，默认 30000"}
    },
    "required": ["command"]
  }',
  'builtin',
  0  -- 默认禁用，由 shell_mode 动态控制是否注入
);
```

`enabled=0` 表示全局关闭，但在构建 LLM tools 列表时，若该对话 `shell_mode != 'off'`，动态注入此工具描述。

**2.2 `agent_chat/runtime.rs` — 动态注入工具**

在构建 `tools_for_llm` 列表处，增加逻辑：
```rust
let shell_mode = db::get_conv_shell_mode(conn, conv_id)?;
if shell_mode != "off" {
    tools_for_llm.push(run_shell_tool_schema());
}
```

---

### Phase 3 — 工具执行处理（后端核心）

**3.1 `agent_chat/tools.rs` — execute_tool_call 增加 run_shell 分支**

```rust
"run_shell" => {
    let command = args["command"].as_str().unwrap_or("").to_string();
    let cwd     = args["cwd"].as_str().map(str::to_string);
    let timeout = args["timeout_ms"].as_u64().unwrap_or(30_000);
    let shell_mode = db::get_conv_shell_mode(conn, conv_id)?;

    match shell_mode.as_str() {
        "off" => ToolExecResult::error(call_id, "Shell 未启用，请在对话设置中开启"),

        "yolo" => {
            // 先经过 agent_policy 分类，destructive 仍然阻断
            let result = state.run_shell_impl(
                command, timeout, Some("chat_yolo".into()), None, None
            ).await?;
            ToolExecResult::ok(call_id, format_shell_result(&result))
        }

        "approval" => {
            // 写入 pending_shells 并等待 oneshot
            let pending_id = db::insert_pending_shell(conn, conv_id, &command, cwd.as_deref())?;
            // emit chat_shell_approval 事件，前端展示审批卡
            app_handle.emit("chat_shell_approval", ShellApprovalPayload {
                conversation_id: conv_id,
                pending_id,
                call_id: call_id.clone(),
                command: command.clone(),
            })?;
            // 等待用户审批（30s 超时自动拒绝）
            let (tx, rx) = oneshot::channel::<bool>();
            state.pending_shell_approvals.lock().unwrap().insert(pending_id, tx);
            let approved = timeout(Duration::from_secs(30), rx).await
                .map(|r| r.unwrap_or(false))
                .unwrap_or(false);
            if approved {
                let result = state.run_shell_impl(
                    command, timeout, Some("chat_approved".into()), None, None
                ).await?;
                ToolExecResult::ok(call_id, format_shell_result(&result))
            } else {
                ToolExecResult::error(call_id, "用户拒绝或审批超时")
            }
        }
        _ => ToolExecResult::error(call_id, "未知 shell_mode"),
    }
}
```

**3.2 `agent_chat/db.rs` — pending_shells 表**

```sql
CREATE TABLE IF NOT EXISTS chat_pending_shells (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL,
    command        TEXT NOT NULL,
    cwd            TEXT,
    created_at     TEXT NOT NULL
);
```

**3.3 `state.rs` / `commands.rs` — 审批命令**

```rust
/// 用户点击批准
#[tauri::command]
pub async fn approve_chat_shell(pending_id: i64, state: State<AppState>) -> Result<(), String>
// → 从 pending_shell_approvals map 取出 sender，send(true)

/// 用户点击拒绝
#[tauri::command]  
pub async fn reject_chat_shell(pending_id: i64, state: State<AppState>) -> Result<(), String>
// → send(false)
```

**3.4 `AppState` — 新增字段**

```rust
pub pending_shell_approvals: Mutex<HashMap<i64, oneshot::Sender<bool>>>,
```

**Yolo 模式的安全边界：**  
即使是 yolo，`run_shell_impl` 内部仍会经过 `classify_shell_policy_with_config`，  
`destructive` 级别命令（`rm -rf`、`format`、`shutdown` 等）**仍然阻断**，不受 yolo 影响。  
这是不可绕过的硬编码保护。

---

### Phase 4 — 前端 UI

**4.1 `web/src/tauri-client/chat.ts` — 新增函数**

```typescript
export async function getConvShellMode(convId: number): Promise<string>
export async function setConvShellMode(convId: number, mode: string): Promise<void>
export async function approveChatShell(pendingId: number): Promise<void>
export async function rejectChatShell(pendingId: number): Promise<void>
```

**4.2 `ChatInputBar.tsx` — Shell 模式切换按钮**

在输入栏底部行（+ 按钮和发送按钮之间）增加 Shell 模式指示器：

```
[+]   [⚡ Yolo]   [发送]
 或
[+]   [🔒 审批]   [发送]
 或
[+]              [发送]   ← off 时不显示
```

点击在 `off → approval → yolo → off` 三态循环，调用 `setConvShellMode`。

**4.3 `MessageThread.tsx` — 监听 chat_shell_approval 事件**

```typescript
useEffect(() => {
  const unlisten = listen<ShellApprovalPayload>("chat_shell_approval", (e) => {
    setPendingShellApproval(e.payload);
  });
  return () => { unlisten.then(f => f()); };
}, [conversationId]);
```

当收到事件且 `conversation_id` 匹配时，在消息流末尾插入 ShellApprovalCard。

**4.4 新建 `ShellApprovalCard.tsx`**

```tsx
// 独立于 ToolCallCard，专门为 shell 审批设计
interface Props {
  pendingId: number;
  command: string;
  onApproved: () => void;
  onRejected: () => void;
}
```

UI 布局：
```
┌─────────────────────────────────┐
│ 🔒 Shell 审批请求               │
│                                 │
│  $ git status --porcelain       │  ← monospace 命令显示
│                                 │
│  [批准执行]    [拒绝]            │
│  (30s 后自动拒绝)                │
└─────────────────────────────────┘
```

倒计时显示剩余秒数，超时后按钮变灰、显示"已超时"。

**4.5 `ToolCallCard.tsx` — Shell 结果展示**

当 `toolName === "run_shell"` 且有结果时：

```
┌─────────────────────────────────┐
│ ▶ run_shell          ✓ 0  42ms  │  ← exit code + 耗时
│   $ git status                  │
│   ─────────────────             │
│   On branch main                │  ← stdout（可折叠）
│   nothing to commit             │
└─────────────────────────────────┘
```

exit_code 非 0 时卡片标红，输出超过 50 行时折叠显示"展开全部"。

---

### Phase 5 — 验证

```powershell
# 后端
cd E:\llm-wiki\src-tauri && cargo test

# 前端
cd E:\llm-wiki\web && npm run typecheck
```

**功能验证清单：**
- [ ] 新建对话默认 shell_mode = off，LLM 没有 run_shell 工具
- [ ] 切换到 approval，LLM 可以调用 run_shell，每次弹审批卡
- [ ] 审批卡 30s 倒计时后自动拒绝，ReAct 继续（以拒绝消息告知 LLM）
- [ ] 切换到 yolo，LLM 直接执行，destructive 命令仍被阻断
- [ ] 结果卡正确显示 stdout/exit_code/耗时
- [ ] 审计日志记录 yolo 和 approved 两种来源的执行记录
- [ ] 对话切换时 shell_mode 指示器正确更新

---

## 四、Commit 计划

```
feat(H14-1): DB 迁移 + shell_mode API
feat(H14-2): run_shell 工具注册 + 动态注入
feat(H14-3): Yolo 和 Approval 执行逻辑 + pending_shell_approvals
feat(H14-4): 前端 ShellApprovalCard + 模式切换按钮
feat(H14-5): ToolCallCard Shell 结果展示
```

---

## 五、风险与对策

| 风险 | 对策 |
|------|------|
| Yolo 执行危险命令 | destructive 分类硬阻断，不受 yolo 影响 |
| 审批等待用户无响应 | 30s oneshot timeout，自动 send(false) |
| Shell 输出过长阻塞 | 输出截断 10,000 字符 + "已截断"提示 |
| 长时间命令卡住 ReAct | timeout_ms 参数控制，默认 30s |
| 对话切换时审批卡残留 | MessageThread 切换 conv_id 时清空 pendingShellApproval 状态 |
| 多个并发 shell 审批 | pending_id 唯一，Map 支持多条并行等待 |

---

## 六、审计日志保留策略

审计日志（`shell_audit_log` 表）不永久保存，采用**双重限制**自动清理：

| 参数 | 默认值 | 可选值 |
|------|--------|--------|
| 保留天数 | 30 天 | 7 / 30 / 90 / 永久 |
| 最大条数 | 10,000 条 | 硬上限，不可配置 |

**清理时机：每次写入新审计记录后异步触发**，伪代码：

```rust
async fn write_audit_and_prune(conn, entry) {
    insert_audit_log(conn, entry);
    // 异步，不阻塞主流程
    tokio::spawn(async move {
        let retain_days = get_setting("shell_audit_retain_days").unwrap_or(30);
        if retain_days > 0 {
            delete_audit_where("created_at < datetime('now', ? || ' days')", -retain_days);
        }
        // 条数兜底：超过 10000 条删最旧
        delete_audit_oldest_over(10_000);
    });
}
```

**配置入口**：设置页 → 系统 → Shell 审计日志保留天数（下拉选择）。  
设置值存入 `app_settings` 表的 `shell_audit_retain_days` 键。

---

*计划版本：2026-05-12*
