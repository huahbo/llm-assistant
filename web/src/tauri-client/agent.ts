import type {
  AgentDraftConflictInfo,
  AgentDraftItem,
  AgentMemoryItem,
  AgentRunEventItem,
  AgentRunEventLevel,
  AgentRunItem,
  AgentRunStatus,
  AgentSkillItem,
  OutboxEventItem,
} from "../types";
import { isTauriRuntime, withTimeout } from "./base";

// ── Args builders ─────────────────────────────────────────────────────────────

/** 构造 start_agent_run 命令参数（用于测试） */
export const createStartAgentRunArgs = (topic: string) => ({ topic });

/** 构造 append_agent_run_event 命令参数（用于测试） */
export const createAppendAgentRunEventArgs = (
  runId: number,
  level: AgentRunEventLevel,
  message: string,
) => ({
  runId,
  run_id: runId,
  level,
  message,
});

/** 构造 list_agent_runs 命令参数（用于测试） */
export const createListAgentRunsArgs = (limit?: number, includeArchived?: boolean) => ({
  limit,
  includeArchived,
  include_archived: includeArchived,
});

/** 构造 list_agent_run_events 命令参数（用于测试） */
export const createListAgentRunEventsArgs = (runId: number, limit?: number) => ({
  runId,
  run_id: runId,
  limit,
});

/** 构造 complete_agent_run 命令参数（用于测试） */
export const createCompleteAgentRunArgs = (runId: number, status: AgentRunStatus | string) => ({
  runId,
  run_id: runId,
  status,
});

/** 构造 generate_agent_draft 命令参数（用于测试） */
export const createGenerateAgentDraftArgs = (
  runId: number,
  topic: string,
  skillKey?: string | null,
  researchMode?: boolean,
  askFirst?: boolean,
) => ({
  runId,
  run_id: runId,
  topic,
  skillKey,
  skill_key: skillKey,
  researchMode,
  research_mode: researchMode,
  askFirst,
  ask_first: askFirst,
});

/** 构造 run_agent_task 命令参数（用于测试） */
export const createRunAgentTaskArgs = (
  runId: number,
  instruction: string,
  maxIterations?: number,
  memoryContext?: string,
) => ({
  runId,
  run_id: runId,
  instruction,
  maxIterations,
  max_iterations: maxIterations,
  memoryContext,
  memory_context: memoryContext,
});

/** 构造 list_agent_drafts 命令参数（用于测试） */
export const createListAgentDraftsArgs = (runId: number, limit?: number) => ({
  runId,
  run_id: runId,
  limit,
});

/** 构造 approve_agent_draft 命令参数（用于测试） */
export const createApproveAgentDraftArgs = (draftId: number) => ({
  draftId,
  draft_id: draftId,
});

export const createCheckAgentDraftConflictArgs = (draftId: number) => ({
  draftId,
  draft_id: draftId,
});

/** 构造 upsert_agent_skill 命令参数（用于测试） */
export const createUpsertAgentSkillArgs = (skillKey: string, promptTemplate: string) => ({
  skillKey,
  skill_key: skillKey,
  promptTemplate,
  prompt_template: promptTemplate,
});

/** 构造 list_agent_skills 命令参数（用于测试） */
export const createListAgentSkillsArgs = (limit?: number) => ({ limit });

/** 构造 delete_agent_skill 命令参数（用于测试） */
export const createDeleteAgentSkillArgs = (id: number) => ({ id });

// ── Agent runs ────────────────────────────────────────────────────────────────

/** 启动 Agent Run（H0 脚手架）。后端返回 run_id。 */
export async function startAgentRun(topic: string): Promise<number | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<number>("start_agent_run", {
        ...createStartAgentRunArgs(topic),
      }),
    );
  } catch {
    return null;
  }
}

/** 追加 Agent Run 事件。后端成功即视为 true。 */
export async function appendAgentRunEvent(
  runId: number,
  level: AgentRunEventLevel,
  message: string,
): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(
      invoke<void>("append_agent_run_event", {
        ...createAppendAgentRunEventArgs(runId, level, message),
      }),
    );
    return true;
  } catch {
    return false;
  }
}

/** 列出 Agent Runs。非 Tauri 环境返回空数组。 */
export async function listAgentRuns(limit = 50, includeArchived = false): Promise<AgentRunItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentRunItem[]>("list_agent_runs", {
        ...createListAgentRunsArgs(limit, includeArchived),
      }),
    );
  } catch {
    return [];
  }
}

/** 归档 Agent Run（软删除）。 */
export async function archiveAgentRun(runId: number): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("archive_agent_run", { runId, run_id: runId }));
    return true;
  } catch {
    return false;
  }
}

/** 恢复已归档 Agent Run。 */
export async function restoreAgentRun(runId: number): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("restore_agent_run", { runId, run_id: runId }));
    return true;
  } catch {
    return false;
  }
}

/** 列出指定 Agent Run 的事件。非 Tauri 环境返回空数组。 */
export async function listAgentRunEvents(runId: number, limit = 200): Promise<AgentRunEventItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentRunEventItem[]>("list_agent_run_events", {
        ...createListAgentRunEventsArgs(runId, limit),
      }),
    );
  } catch {
    return [];
  }
}

/** 完成 Agent Run（写入终态）。后端成功即视为 true。 */
export async function completeAgentRun(
  runId: number,
  status: AgentRunStatus | string,
): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(
      invoke<void>("complete_agent_run", {
        ...createCompleteAgentRunArgs(runId, status),
      }),
    );
    return true;
  } catch {
    return false;
  }
}

/** 为指定 run 生成 Draft。后端成功即视为 true。 */
export async function generateAgentDraft(
  runId: number,
  topic: string,
  skillKey?: string | null,
  researchMode?: boolean,
  askFirst?: boolean,
): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(
      invoke<void>("generate_agent_draft", {
        ...createGenerateAgentDraftArgs(runId, topic, skillKey, researchMode, askFirst),
      }),
    );
    return true;
  } catch {
    return false;
  }
}

/** 运行 Agent 任务模式（H6-S2 skeleton）。 */
export async function runAgentTask(
  runId: number,
  instruction: string,
  maxIterations?: number,
  memoryContext?: string,
): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<string>("run_agent_task", {
        ...createRunAgentTaskArgs(runId, instruction, maxIterations, memoryContext),
      }),
      95_000,
    );
  } catch {
    return null;
  }
}

// ── Agent drafts ──────────────────────────────────────────────────────────────

/** 列出指定 run 的 Draft。非 Tauri 环境返回空数组。 */
export async function listAgentDrafts(runId: number, limit = 100): Promise<AgentDraftItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentDraftItem[]>("list_agent_drafts", {
        ...createListAgentDraftsArgs(runId, limit),
      }),
    );
  } catch {
    return [];
  }
}

/** 审批 Draft 并写盘。后端成功即视为 true。 */
export async function approveAgentDraft(draftId: number): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(
      invoke<void>("approve_agent_draft", {
        ...createApproveAgentDraftArgs(draftId),
      }),
    );
    return true;
  } catch {
    return false;
  }
}

/** 审批前冲突预检：返回同名页面是否存在（H1 确认弹窗用） */
export async function checkAgentDraftConflict(
  draftId: number,
): Promise<AgentDraftConflictInfo | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentDraftConflictInfo>("check_agent_draft_conflict", {
        ...createCheckAgentDraftConflictArgs(draftId),
      }),
    );
  } catch {
    return null;
  }
}

// ── Agent memories ────────────────────────────────────────────────────────────

/** 写入或更新 agent 记忆（按 run_id + key upsert）。 */
export async function upsertAgentMemory(
  runId: number | null,
  key: string,
  value: string,
): Promise<AgentMemoryItem | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentMemoryItem>("upsert_agent_memory", {
        runId,
        run_id: runId,
        key,
        value,
      }),
    );
  } catch {
    return null;
  }
}

/** 列出 agent 记忆（null = 全局记忆）。 */
export async function listAgentMemories(
  runId: number | null,
  limit = 50,
): Promise<AgentMemoryItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentMemoryItem[]>("list_agent_memories", {
        runId,
        run_id: runId,
        limit,
      }),
    );
  } catch {
    return [];
  }
}

/** 删除单条 agent 记忆。 */
export async function deleteAgentMemory(id: number): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("delete_agent_memory", { id }));
    return true;
  } catch {
    return false;
  }
}

/** 基于批注重写 Agent Draft，返回新草稿（H5-A）。 */
export async function rewriteAgentDraft(
  draftId: number,
  comment: string,
): Promise<AgentDraftItem | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentDraftItem>("rewrite_agent_draft", {
        draftId,
        draft_id: draftId,
        comment,
      }),
      120_000,
    );
  } catch {
    return null;
  }
}

// ── Agent skills ──────────────────────────────────────────────────────────────

/** 写入或更新 Agent 技能模板（H3）。 */
export async function upsertAgentSkill(
  skillKey: string,
  promptTemplate: string,
): Promise<AgentSkillItem | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentSkillItem>("upsert_agent_skill", {
        ...createUpsertAgentSkillArgs(skillKey, promptTemplate),
      }),
    );
  } catch {
    return null;
  }
}

/** 列出 Agent 技能模板（H3）。 */
export async function listAgentSkills(limit = 50): Promise<AgentSkillItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<AgentSkillItem[]>("list_agent_skills", {
        ...createListAgentSkillsArgs(limit),
      }),
    );
  } catch {
    return [];
  }
}

/** 删除单条 Agent 技能模板（H3）。 */
export async function deleteAgentSkill(id: number): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(
      invoke<void>("delete_agent_skill", {
        ...createDeleteAgentSkillArgs(id),
      }),
    );
    return true;
  } catch {
    return false;
  }
}

// ── Skill URL 安装 ────────────────────────────────────────────────────────────

export async function installSkillFromUrl(url: string): Promise<number | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke<number>("install_skill_from_url", { url }), 20_000);
}

// ── Outbox events ─────────────────────────────────────────────────────────────

/** 获取 Outbox 事件（增量轮询） */
export async function get_outbox_events(options: {
  last_id?: number;
  limit?: number;
}): Promise<OutboxEventItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<OutboxEventItem[]>("get_outbox_events", {
      lastId: options.last_id,
      last_id: options.last_id,
      limit: options.limit,
    });
  } catch {
    return [];
  }
}
