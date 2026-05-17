import type { McpServerConfig, SmitheryServer, SmitheryServerDetail } from "../types";
import { isTauriRuntime, withTimeout } from "./base";

// ── MCP 服务器管理 ────────────────────────────────────────────────────────────

export async function listMcpServers(): Promise<McpServerConfig[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<McpServerConfig[]>("list_mcp_servers");
}

export async function upsertMcpServer(
  name: string,
  command: string,
  args: string[],
  env: Record<string, string>,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("upsert_mcp_server", { name, command, args, env });
}

export async function deleteMcpServer(name: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("delete_mcp_server", { name });
}

/** Spawn the MCP server process, list tools, and sync them into agent_tools.
 *  Returns the list of tool names registered. */
export async function reloadMcpServerTools(name: string): Promise<string[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<string[]>("reload_mcp_server_tools", { name }),
    30_000,
  );
}

// ── Smithery Registry ─────────────────────────────────────────────────────────

export async function searchMcpRegistry(query: string, pageSize = 20): Promise<SmitheryServer[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<SmitheryServer[]>("search_mcp_registry", { query, pageSize });
}

export async function getMcpRegistryServer(qualifiedName: string): Promise<SmitheryServerDetail> {
  if (!isTauriRuntime()) throw new Error("非 Tauri 环境");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<SmitheryServerDetail>("get_mcp_registry_server", { qualifiedName });
}
