import { isTauriRuntime } from "./base";

export type AssistAction = "continue" | "rewrite" | "expand";

export interface AiAssistChunkPayload {
  chunk: string;
}

export interface AiAssistErrorPayload {
  error: string;
}

/** 触发 AI 辅助编辑，内容通过 ai_assist_chunk / ai_assist_done / ai_assist_error 事件流式回传。 */
export async function aiAssistWikiEdit(
  action: AssistAction,
  selectedText: string,
  context: string,
  pageTitle: string,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("ai_assist_wiki_edit", {
    action,
    selectedText,
    context,
    pageTitle,
  });
}

export async function listenAiAssistChunk(
  handler: (payload: AiAssistChunkPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<AiAssistChunkPayload>("ai_assist_chunk", (e) => handler(e.payload));
}

export async function listenAiAssistDone(handler: () => void): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen("ai_assist_done", () => handler());
}

export async function listenAiAssistError(
  handler: (payload: AiAssistErrorPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen<AiAssistErrorPayload>("ai_assist_error", (e) => handler(e.payload));
}
