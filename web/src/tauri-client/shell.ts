import type { ShellAuditEvent, ShellResult, ShellSessionInfo, ShellHistoryEntry, ShellStreamChunk, ShellPolicyConfig } from "../types";
import { isTauriRuntime, withTimeout } from "./base";

/** 执行 PowerShell 命令（H6-S1）。非 Tauri 环境返回 null。 */
export async function runShell(
  command: string,
  timeoutMs?: number,
  source?: "manual" | "agent",
  sessionId?: string | null,
  streamId?: string | null,
): Promise<ShellResult | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<ShellResult>("run_shell", { command, timeoutMs, source, sessionId, streamId }),
    (timeoutMs ?? 30_000) + 5_000
  );
}

/** 用户主动审批并执行被 require_approval 拦截的命令（H6-P2）。 */
export async function approveAndRunShell(
  command: string,
  timeoutMs?: number,
  sessionId?: string | null,
  streamId?: string | null,
): Promise<ShellResult | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<ShellResult>("approve_and_run_shell", { command, timeoutMs, sessionId, streamId }),
    (timeoutMs ?? 30_000) + 5_000
  );
}

/** 读取 Shell 审计日志（H6-P3）。 */
export async function listShellAuditEvents(limit?: number): Promise<ShellAuditEvent[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ShellAuditEvent[]>("list_shell_audit_events", { limit });
}

/** 颁发写操作审批票据（H6-P2）。用户勾选"记住 5 分钟"时调用。 */
export async function grantWriteTicket(path: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("grant_write_ticket", { path });
}

/** 创建 Shell 会话（会话内 cwd 持久化）。 */
export async function createShellSession(source?: "manual" | "agent"): Promise<ShellSessionInfo | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(invoke<ShellSessionInfo>("create_shell_session", { source }), 8000);
  } catch {
    return null;
  }
}

/** 关闭 Shell 会话。 */
export async function closeShellSession(sessionId: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(invoke<boolean>("close_shell_session", { sessionId }), 5000);
  } catch {
    return false;
  }
}

/** 订阅 Shell 流式输出事件。 */
export async function listenShellStreamChunk(
  handler: (payload: ShellStreamChunk) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ShellStreamChunk>("shell_stream_chunk", (e) => handler(e.payload));
  return unlisten;
}

/** 读取 Shell 策略配置。 */
export async function getShellPolicyConfig(): Promise<ShellPolicyConfig | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(invoke<ShellPolicyConfig>("get_shell_policy_config"), 5000);
  } catch {
    return null;
  }
}

/** 保存 Shell 策略配置。 */
export async function setShellPolicyConfig(
  config: ShellPolicyConfig,
): Promise<ShellPolicyConfig | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(invoke<ShellPolicyConfig>("set_shell_policy_config", { config }), 5000);
  } catch {
    return null;
  }
}

export async function approveAgentWrite(runId: number): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<string>("approve_agent_write", { runId }),
    30_000
  );
}

export async function rejectAgentWrite(runId: number): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<string>("reject_agent_write", { runId }),
    15_000
  );
}

