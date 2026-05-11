import type { Conversation, ChatMessage } from "../types";
import { isTauriRuntime, withTimeout } from "./base";

// ── agent_chat (H8) ───────────────────────────────────────────────────────────

export async function createConversation(
  title: string,
  skillKey?: string,
  injectMemories?: boolean,
): Promise<number | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<number>("create_conversation", {
        title,
        skillKey: skillKey ?? null,
        injectMemories: injectMemories ?? false,
      }),
      10_000,
    );
  } catch {
    return null;
  }
}

export async function listConversations(
  includeArchived = false,
): Promise<Conversation[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<Conversation[]>("list_conversations", { includeArchived }),
      10_000,
    );
  } catch {
    return [];
  }
}

export async function sendChatMessage(
  conversationId: number,
  userMessage: string,
): Promise<number | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<number>("send_chat_message", { conversationId, userMessage }),
      120_000,
    );
  } catch {
    return null;
  }
}

export async function listChatMessages(
  conversationId: number,
): Promise<ChatMessage[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<ChatMessage[]>("list_chat_messages", { conversationId }),
      10_000,
    );
  } catch {
    return [];
  }
}

export async function cancelChatMessage(conversationId: number): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("cancel_chat_message", { conversationId }), 5_000);
  } catch {
    // ignore
  }
}

export async function renameConversation(
  conversationId: number,
  newTitle: string,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("rename_conversation", { conversationId, newTitle }), 5_000);
  } catch {
    // ignore
  }
}

export async function archiveConversation(
  conversationId: number,
  archived: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("archive_conversation", { conversationId, archived }), 5_000);
  } catch {
    // ignore
  }
}

export async function deleteConversation(conversationId: number): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("delete_conversation", { conversationId }), 5_000);
  } catch {
    // ignore
  }
}

export async function approveChatWrite(pendingId: number): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("approve_chat_write", { pendingId }), 30_000);
  } catch {
    // ignore
  }
}

export async function rejectChatWrite(pendingId: number): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await withTimeout(invoke<void>("reject_chat_write", { pendingId }), 10_000);
  } catch {
    // ignore
  }
}
