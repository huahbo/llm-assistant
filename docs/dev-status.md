# dev-status.md — 当前开发状态（Agent 交接必读）

> **活跃层**：每轮结束由主控 Agent 更新。新 Agent 启动时**必须先读本文件**，再读 `docs/实施过程记录.md` 最新 3 条。

---

## 快速恢复步骤

1. 运行基线验证（见下方 §验证基线）
2. 读 `docs/交接状态卡.md`，确认当前接力状态
3. 读 `docs/实施过程记录.md` 最新 3 条了解背景
4. 查看下方 §活跃 TODO

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
| `262edc6` | chore: H5 Windows 验证全绿，更新基线 |
| `dcd3f0a` | docs: 归档实施过程记录 + 整理协作文档 + 更新 README |
| `2bdd80c` | feat(H5-D): skill 模板变量 `{{topic}}` `{{memories}}` |
| `efe238e` | feat(H5): auto-memory + draft rewrite + ask-first injection |
| `1ca66d6` | feat(H4): research_mode |

---

## 活跃 TODO（按优先级）

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| 1 | **H6-S1：Shell Tool MVP** | 进行中 | 见下方详细计划 |
| 2 | **H6-S2：Agentic Loop** | 待 S1 完成 | daerwen-agent 移植，见下方计划 |

---

## H6 详细计划

### 背景

Agent Studio 目前是"一次性 LLM 调用 → 生成草稿"，H6 目标是升级为**可操作本机的真实 Agent**——LLM 自主决策调用工具（shell、文件读写），配合三层护栏保证 OS 安全。参考项目：`refer-rust-daerwen-agent/`（Rust + Tauri，与本项目技术栈完全一致）。

---

### H6-S1：Shell Tool MVP（不含 agentic loop）

**目标**：Agent Studio 获得 PowerShell 执行面板，用户手动输入命令，Agent 将来可以建议命令。

#### 后端改动

**`src-tauri/src/models.rs`** — 新增：
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellResult {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub blocked: bool,
    pub blocked_reason: Option<String>,
}
```

**`src-tauri/src/state.rs`** — 在 `AppStateData` impl 块末尾新增：
```rust
pub async fn run_shell_impl(&self, command: String, timeout_ms: u64) -> Result<ShellResult, String> {
    // 1. 黑名单检查（case-insensitive）
    const BLACKLIST: &[&str] = &[
        "rm -rf", "rm -r", "format ", "reg delete", "reg add", "shutdown",
        "del /f", "rmdir /s", "remove-item -recurse", "clear-recyclebin",
        "diskpart", "bcdedit", "mkfs", "dd if=", ":(){ :|:& };:",
    ];
    let lower = command.to_lowercase();
    for pat in BLACKLIST {
        if lower.contains(pat) {
            return Ok(ShellResult {
                command: command.clone(),
                stdout: String::new(),
                stderr: String::new(),
                exit_code: -1,
                blocked: true,
                blocked_reason: Some(format!("命令包含高危模式: {pat}")),
            });
        }
    }
    // 2. 确定 cwd（vault 路径优先，fallback 到 temp）
    let cwd = {
        let data = self.data.lock().unwrap();
        data.vault_path.clone().map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    };
    // 3. 执行（Windows: pwsh，其他: bash）
    let child = {
        #[cfg(target_os = "windows")]
        { tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &command])
            .current_dir(&cwd)
            .stdout(Stdio::piped()).stderr(Stdio::piped()) }
        #[cfg(not(target_os = "windows"))]
        { tokio::process::Command::new("bash")
            .arg("-c").arg(&command)
            .current_dir(&cwd)
            .stdout(Stdio::piped()).stderr(Stdio::piped()) }
    };
    let timeout = std::time::Duration::from_millis(timeout_ms.min(120_000));
    match tokio::time::timeout(timeout, child.output()).await {
        Ok(Ok(out)) => Ok(ShellResult {
            command,
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            exit_code: out.status.code().unwrap_or(-1),
            blocked: false,
            blocked_reason: None,
        }),
        Ok(Err(e)) => Err(format!("执行失败: {e}")),
        Err(_) => Err(format!("超时（{timeout_ms}ms）")),
    }
}
```

需要在文件顶部补 import：`use std::process::Stdio; use std::path::PathBuf;`（若尚未引入）

**`src-tauri/src/commands.rs`** — 新增命令 + import：
```rust
// import 区加入 ShellResult
use crate::models::ShellResult;

#[tauri::command]
pub async fn run_shell(
    command: String,
    timeout_ms: Option<u64>,
    state: State<'_, AppState>,
) -> Result<ShellResult, String> {
    state.run_shell_impl(command, timeout_ms.unwrap_or(30_000)).await
}
```

**`src-tauri/src/main.rs`** — `.invoke_handler` 中加入 `run_shell`。

#### 前端改动

**`web/src/types.ts`** — 新增：
```ts
export interface ShellResult {
  command: string;
  stdout: string;
  stderr: string;
  exit_code: number;
  blocked: boolean;
  blocked_reason: string | null;
}
export interface ShellHistoryEntry {
  id: number;
  command: string;
  result: ShellResult;
  ts: number;
}
```

**`web/src/tauri-client.ts`** — 新增：
```ts
export async function runShell(
  command: string,
  timeoutMs?: number
): Promise<ShellResult | null> {
  return withTimeout(
    invoke<ShellResult>("run_shell", { command, timeoutMs }),
    (timeoutMs ?? 30_000) + 5_000
  );
}
```

**`web/src/App.tsx`** — 在 Agent Studio 区块：
- 新增 state：
  ```ts
  const [agentShellCmd, setAgentShellCmd] = useState("");
  const [agentShellHistory, setAgentShellHistory] = useState<ShellHistoryEntry[]>([]);
  const [agentShellRunning, setAgentShellRunning] = useState(false);
  const [agentShellOpen, setAgentShellOpen] = useState(false);
  ```
- `handleRunShell`：调 `runShell`，push 到 `agentShellHistory`
- Shell 面板 JSX（折叠区，放在 rewrite-bar 下方）：
  ```jsx
  <div className="agent-studio__shell">
    <button className="agent-studio__shell-toggle"
      onClick={() => setAgentShellOpen(o => !o)}>
      {agentShellOpen ? "▼" : "▶"} Shell
    </button>
    {agentShellOpen && (
      <div className="agent-studio__shell-body">
        <div className="agent-studio__shell-history">
          {agentShellHistory.map(e => (
            <div key={e.id} className={`agent-studio__shell-entry ${e.result.exit_code === 0 ? "ok" : e.result.blocked ? "blocked" : "err"}`}>
              <span className="agent-studio__shell-prompt">❯ {e.command}</span>
              {e.result.blocked
                ? <span className="agent-studio__shell-blocked">⛔ {e.result.blocked_reason}</span>
                : <pre className="agent-studio__shell-output">{e.result.stdout || e.result.stderr}</pre>}
            </div>
          ))}
        </div>
        <div className="agent-studio__shell-input-row">
          <input
            type="text"
            className="agent-studio__shell-input"
            placeholder="PowerShell 命令（Enter 执行）"
            value={agentShellCmd}
            onChange={e => setAgentShellCmd(e.target.value)}
            onKeyDown={e => e.key === "Enter" && !agentShellRunning && handleRunShell()}
            disabled={agentShellRunning}
          />
          <button
            className="agent-studio__shell-run-btn"
            disabled={!agentShellCmd.trim() || agentShellRunning}
            onClick={handleRunShell}>
            {agentShellRunning ? "…" : "运行"}
          </button>
        </div>
      </div>
    )}
  </div>
  ```

**`web/src/styles.css`** — 新增 shell 相关样式：
- `.agent-studio__shell`：`margin-top: 6px; border: 1px solid var(--border); border-radius: 4px;`
- `.agent-studio__shell-toggle`：`width: 100%; text-align: left; padding: 4px 8px; background: var(--bg-subtle); border: none; cursor: pointer; font-size: 12px;`
- `.agent-studio__shell-body`：`padding: 6px 8px;`
- `.agent-studio__shell-history`：`max-height: 240px; overflow-y: auto; margin-bottom: 6px;`
- `.agent-studio__shell-entry`：`margin-bottom: 8px; font-size: 12px; font-family: monospace;`
- `.agent-studio__shell-entry.ok .agent-studio__shell-prompt`：`color: var(--success);`
- `.agent-studio__shell-entry.err .agent-studio__shell-prompt`：`color: var(--danger);`
- `.agent-studio__shell-entry.blocked .agent-studio__shell-blocked`：`color: var(--warning);`
- `.agent-studio__shell-output`：`margin: 2px 0 0 12px; white-space: pre-wrap; word-break: break-all;`
- `.agent-studio__shell-input-row`：`display: flex; gap: 6px;`
- `.agent-studio__shell-input`：`flex: 1; font-family: monospace; font-size: 12px;`

#### 验收标准
1. `cargo test` 全绿
2. `npm run typecheck` 0 errors
3. 在 Agent Studio 打开 Shell 面板，执行 `Get-Date` → 显示当前时间
4. 执行 `rm -rf /` → 显示 ⛔ 黑名单拦截
5. 执行不存在命令 → 显示 stderr + 非零 exit code（红色）

---

### H6-S2：Agentic Tool-Call Loop（daerwen 移植）

> **前置**：H6-S1 验收通过后才开始 S2。

**目标**：Agent Studio 新增"任务模式"——用户输入自然语言指令，LLM 自主循环调用工具（shell、文件读写、wiki 检索）直到完成任务。

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
