import type {
  AskHistoryItem,
  AskSessionItem,
  AskSessionSearchHitItem,
  AskSessionTurnItem,
  QueryAnswerResult,
  QueryAskOptions,
  QuerySettings,
  ResearchOutlineData,
  ResearchOutlinePayload,
  ResearchTaskItem,
  SaveQueryAnswerInput,
  SaveQueryAnswerResult,
  SearchConfig,
} from "../types";
import { isTauriRuntime, withTimeout } from "./base";

// ── Args builders ─────────────────────────────────────────────────────────────

export const createQueryAskArgs = (question: string) => ({
  question,
});

export const createQueryAskWithOptionsArgs = (
  question: string,
  options?: QueryAskOptions,
) => ({
  question,
  options: options
    ? {
        topK: options.top_k,
        top_k: options.top_k,
      }
    : undefined,
});

export const createSetQueryTopKArgs = (topK: number) => ({
  topK,
  top_k: topK,
});

export const createSaveQueryAnswerArgs = (input: SaveQueryAnswerInput) => ({
  input: {
    question: input.question,
    answer: input.answer,
    citations: input.citations,
    title: input.title,
  },
});

// ── Research event payload interfaces ────────────────────────────────────────

/** 研究进度事件载荷 */
export interface ResearchProgressPayload {
  task_id: number;
  stage: string;
  message: string;
  /** 仅 writing_section 阶段携带（0-based） */
  section_index?: number;
  section_title?: string;
  total_sections?: number;
}

/** 研究完成事件载荷 */
export interface ResearchDonePayload {
  task_id: number;
  saved_path: string;
}

/** 研究错误事件载荷 */
export interface ResearchErrorPayload {
  task_id: number;
  error: string;
}

/** 子查询就绪事件载荷（用于用户审批） */
export interface ResearchQueriesReadyPayload {
  task_id: number;
  queries: string[];
}

/** 综合报告流式块载荷 */
export interface ResearchStreamChunkPayload {
  task_id: number;
  chunk: string;
}

// ── Query / ask sessions ──────────────────────────────────────────────────────

export async function queryAsk(question: string): Promise<QueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<QueryAnswerResult>("query_ask", {
      ...createQueryAskArgs(question),
    }),
  );
}

export async function queryAskWithOptions(
  question: string,
  options?: QueryAskOptions,
): Promise<QueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<QueryAnswerResult>("query_ask_with_options", {
        ...createQueryAskWithOptionsArgs(question, options),
      }),
    );
  } catch {
    // 兼容旧后端：当新命令不可用时回退到 query_ask。
    return queryAsk(question);
  }
}

/** 多轮会话问答（携带 session_id，后端维护历史上下文） */
export async function queryAskSession(
  sessionId: string,
  question: string,
  options?: QueryAskOptions,
): Promise<QueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await invoke<QueryAnswerResult>("query_ask_session", {
      sessionId,
      question,
      options: options ?? null,
    });
  } catch {
    return null;
  }
}

/** 取消正在进行的会话查询 */
export async function cancelAskSession(sessionId: string): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    await withTimeout(invoke("cancel_ask_session", { sessionId }), 10_000);
  } catch {
    // 忽略错误
  }
}

/** 清空会话历史（开启新对话） */
export async function clearAskSession(sessionId: string): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    await withTimeout(invoke("clear_ask_session", { sessionId }), 10_000);
  } catch {
    // 忽略错误
  }
}

/** 创建 Ask 会话（若已存在则刷新更新时间） */
export async function createAskSession(
  sessionId: string,
  title?: string,
): Promise<AskSessionItem | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionItem>("create_ask_session", {
      sessionId,
      title: title ?? null,
    });
  } catch {
    return null;
  }
}

/** 读取 Ask 会话列表 */
export async function listAskSessions(limit = 50): Promise<AskSessionItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionItem[]>("list_ask_sessions", { limit });
  } catch {
    return null;
  }
}

/** 读取指定 Ask 会话轮次（正序） */
export async function fetchAskSessionTurns(
  sessionId: string,
  limit = 400,
): Promise<AskSessionTurnItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionTurnItem[]>("get_ask_session_turns", {
      sessionId,
      limit,
    });
  } catch {
    return null;
  }
}

/** 跨会话检索 Ask 轮次 */
export async function searchAskSessionTurns(
  keyword: string,
  limit = 50,
): Promise<AskSessionSearchHitItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionSearchHitItem[]>("search_ask_session_turns", {
      keyword,
      limit,
    });
  } catch {
    return null;
  }
}

/** 重命名 Ask 会话 */
export async function renameAskSession(sessionId: string, title: string): Promise<boolean> {
  if (!isTauriRuntime()) {
    return false;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke("rename_ask_session", { sessionId, title }), 10_000);
    return true;
  } catch {
    return false;
  }
}

/** 删除 Ask 会话 */
export async function deleteAskSession(sessionId: string): Promise<boolean> {
  if (!isTauriRuntime()) {
    return false;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke("delete_ask_session", { sessionId }), 10_000);
    return true;
  } catch {
    return false;
  }
}

export async function fetchQuerySettings(): Promise<QuerySettings | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await invoke<QuerySettings>("get_query_settings");
  } catch {
    return null;
  }
}

export async function setQueryTopK(topK: number): Promise<QuerySettings | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<QuerySettings>("set_query_top_k", {
        ...createSetQueryTopKArgs(topK),
      }),
    );
  } catch {
    return null;
  }
}

export async function saveAskHistory(question: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke("save_ask_history", { question }), 10_000);
  } catch {
    // 历史保存失败不阻断主流程
  }
}

export async function fetchAskHistory(limit = 30): Promise<AskHistoryItem[] | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskHistoryItem[]>("get_ask_history", { limit });
  } catch {
    return null;
  }
}

export async function clearAskHistory(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke<number>("clear_ask_history");
    return true;
  } catch {
    return false;
  }
}

// ── Research event listeners ──────────────────────────────────────────────────

/** 监听研究进度事件（research_progress / research_done / research_error）。非 Tauri 环境为空操作。 */
export async function listenResearchProgress(
  handler: (payload: ResearchProgressPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchProgressPayload>("research_progress", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchDone(
  handler: (payload: ResearchDonePayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchDonePayload>("research_done", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchError(
  handler: (payload: ResearchErrorPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchErrorPayload>("research_error", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchQueriesReady(
  handler: (payload: ResearchQueriesReadyPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchQueriesReadyPayload>("research_queries_ready", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchStreamChunk(
  handler: (payload: ResearchStreamChunkPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchStreamChunkPayload>("research_stream_chunk", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchOutlineReady(
  handler: (payload: ResearchOutlinePayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchOutlinePayload>("research_outline_ready", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

// ── Research tasks ────────────────────────────────────────────────────────────

export async function approveResearchQueries(taskId: number, queries: string[]): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke("approve_research_queries", { taskId, queries }), 30_000);
}

export async function approveResearchOutline(taskId: number, outlineJson: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke("approve_research_outline", { taskId, outlineJson }), 30_000);
}

/** 启动研究任务，返回 task_id。非 Tauri 环境返回 -1。 */
export async function startResearch(
  topic: string,
  depth: number,
  breadth: number,
): Promise<number> {
  if (!isTauriRuntime()) return -1;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<number>("start_research", { topic, depth, breadth });
}

/** 列出历史研究任务（最多 100 条，倒序）。非 Tauri 环境返回 []。 */
export async function listResearchTasks(): Promise<ResearchTaskItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<ResearchTaskItem[]>("list_research_tasks");
  } catch {
    return [];
  }
}

/** 获取单条研究任务的最新状态。非 Tauri 环境返回 null。 */
export async function getResearchTask(id: number): Promise<ResearchTaskItem | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<ResearchTaskItem | null>("get_research_task", { id });
  } catch {
    return null;
  }
}

/** 取消研究任务（queued 状态）。非 Tauri 环境静默忽略。 */
export async function cancelResearchTask(id: number): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke<void>("cancel_research_task", { id });
  } catch {
    // 静默忽略
  }
}

/** 删除研究任务；可选同时删除关联的 Wiki 页面。 */
export async function deleteResearchTask(
  id: number,
  deleteSavedWiki: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("delete_research_task", {
    payload: {
      id,
      deleteSavedWiki,
      delete_saved_wiki: deleteSavedWiki,
    },
  });
}

/** 保存 Deep Research 导出文件（.md 或其他格式）到用户指定路径。 */
export async function saveResearchDoc(path: string, content: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("save_research_doc", { path, content });
}
