import { useReducer, useRef, useCallback, useEffect } from "react";
import type { ChatStreamingMessage, ChatStreamSegment } from "../../../types";
import {
  isTauriRuntime,
  sendChatMessage,
  cancelChatMessage,
} from "../../../tauri-client";

type State = {
  streamingMessage: ChatStreamingMessage | null;
};

type Action =
  | { type: "RESET" }
  | { type: "TEXT_FLUSH"; conversationId: number; messageId: number; text: string }
  | {
      type: "TOOL_CALL_START";
      conversationId: number;
      messageId: number;
      callId: string;
      toolName: string;
      args: Record<string, unknown>;
      idx: number;
    }
  | {
      type: "TOOL_CALL_RESULT";
      conversationId: number;
      messageId: number;
      callId: string;
      ok: boolean;
      preview: string;
      latencyMs: number;
    }
  | {
      type: "TOOL_AWAITING";
      conversationId: number;
      messageId: number;
      callId: string;
      pendingId: number;
    }
  | {
      type: "MESSAGE_DONE";
      conversationId: number;
      messageId: number;
    };

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case "RESET":
      return { streamingMessage: null };

    case "TEXT_FLUSH": {
      const prev = state.streamingMessage;
      if (!prev) {
        const seg: ChatStreamSegment = { kind: "text", text: action.text, streaming: true };
        return {
          streamingMessage: {
            conversation_id: action.conversationId,
            message_id: action.messageId,
            segments: [seg],
            status: "streaming",
          },
        };
      }
      const segments = [...prev.segments];
      const lastIdx = segments.length - 1;
      const last = segments[lastIdx];
      if (last && last.kind === "text") {
        segments[lastIdx] = { kind: "text", text: last.text + action.text, streaming: true };
      } else {
        segments.push({ kind: "text", text: action.text, streaming: true });
      }
      return {
        streamingMessage: { ...prev, segments, status: "streaming" },
      };
    }

    case "TOOL_CALL_START": {
      const prev = state.streamingMessage;
      const toolSeg: ChatStreamSegment = {
        kind: "tool",
        call_id: action.callId,
        tool_name: action.toolName,
        args: action.args,
        status: "running",
      };
      if (!prev) {
        return {
          streamingMessage: {
            conversation_id: action.conversationId,
            message_id: action.messageId,
            segments: [toolSeg],
            status: "tool_running",
          },
        };
      }
      const segments = [...prev.segments];
      const existingIdx = segments.findIndex(
        (s) => s.kind === "tool" && s.call_id === action.callId,
      );
      if (existingIdx >= 0) {
        segments[existingIdx] = toolSeg;
      } else {
        segments.push(toolSeg);
      }
      return {
        streamingMessage: { ...prev, segments, status: "tool_running" },
      };
    }

    case "TOOL_CALL_RESULT": {
      const prev = state.streamingMessage;
      if (!prev) return state;
      const segments = prev.segments.map((s) => {
        if (s.kind === "tool" && s.call_id === action.callId) {
          return {
            ...s,
            result: {
              ok: action.ok,
              preview: action.preview,
              latency_ms: action.latencyMs,
            },
            status: (action.ok ? "ok" : "err") as "ok" | "err",
          };
        }
        return s;
      });
      return {
        streamingMessage: { ...prev, segments, status: "streaming" },
      };
    }

    case "TOOL_AWAITING": {
      const prev = state.streamingMessage;
      if (!prev) return state;
      const segments = prev.segments.map((s) => {
        if (s.kind === "tool" && s.call_id === action.callId) {
          return { ...s, status: "awaiting_approval" as const, pending_id: action.pendingId };
        }
        return s;
      });
      return {
        streamingMessage: { ...prev, segments, status: "awaiting_approval" },
      };
    }

    case "MESSAGE_DONE": {
      const prev = state.streamingMessage;
      if (!prev) return state;
      const segments = prev.segments.map((s) => {
        if (s.kind === "text") return { ...s, streaming: false };
        return s;
      });
      return {
        streamingMessage: { ...prev, segments, status: "done" },
      };
    }

    default:
      return state;
  }
}

export function useChatStream(conversationId: number | null) {
  const [state, dispatch] = useReducer(reducer, { streamingMessage: null });

  const pendingTextRef = useRef("");
  const rafRef = useRef<number | null>(null);
  const activeConvIdRef = useRef<number | null>(null);
  const activeMessageIdRef = useRef<number | null>(null);

  useEffect(() => {
    dispatch({ type: "RESET" });
    pendingTextRef.current = "";
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    activeConvIdRef.current = conversationId;

    if (!conversationId) return;
    if (!isTauriRuntime()) return;

    const unlisteners: Array<() => void> = [];

    const setup = async () => {
      const { listen } = await import("@tauri-apps/api/event");

      const flushPending = (convId: number, msgId: number) => {
        const text = pendingTextRef.current;
        pendingTextRef.current = "";
        rafRef.current = null;
        if (text) {
          dispatch({ type: "TEXT_FLUSH", conversationId: convId, messageId: msgId, text });
        }
      };

      const u1 = await listen(
        "chat_text_delta",
        (e: { payload: { conversation_id: number; message_id: number; chunk: string } }) => {
          const p = e.payload;
          if (p.conversation_id !== activeConvIdRef.current) return;
          activeMessageIdRef.current = p.message_id;
          pendingTextRef.current += p.chunk;
          if (rafRef.current === null) {
            rafRef.current = requestAnimationFrame(() => {
              flushPending(p.conversation_id, p.message_id);
            });
          }
        },
      );
      unlisteners.push(u1);

      const u2 = await listen(
        "chat_tool_call",
        (e: {
          payload: {
            conversation_id: number;
            message_id: number;
            call_id: string;
            tool_name: string;
            args: string;
            idx: number;
          };
        }) => {
          const p = e.payload;
          if (p.conversation_id !== activeConvIdRef.current) return;
          activeMessageIdRef.current = p.message_id;
          let args: Record<string, unknown> = {};
          try {
            args = JSON.parse(p.args) as Record<string, unknown>;
          } catch {
            args = {};
          }
          dispatch({
            type: "TOOL_CALL_START",
            conversationId: p.conversation_id,
            messageId: p.message_id,
            callId: p.call_id,
            toolName: p.tool_name,
            args,
            idx: p.idx,
          });
        },
      );
      unlisteners.push(u2);

      const u3 = await listen(
        "chat_tool_result",
        (e: {
          payload: {
            conversation_id: number;
            message_id: number;
            call_id: string;
            ok: boolean;
            content_preview: string;
            latency_ms: number;
          };
        }) => {
          const p = e.payload;
          if (p.conversation_id !== activeConvIdRef.current) return;
          dispatch({
            type: "TOOL_CALL_RESULT",
            conversationId: p.conversation_id,
            messageId: p.message_id,
            callId: p.call_id,
            ok: p.ok,
            preview: p.content_preview,
            latencyMs: p.latency_ms,
          });
        },
      );
      unlisteners.push(u3);

      const u4 = await listen(
        "chat_message_done",
        (e: { payload: { conversation_id: number; message_id: number; note?: string } }) => {
          const p = e.payload;
          if (p.conversation_id !== activeConvIdRef.current) return;
          if (rafRef.current !== null) {
            cancelAnimationFrame(rafRef.current);
            flushPending(p.conversation_id, p.message_id);
          }
          dispatch({
            type: "MESSAGE_DONE",
            conversationId: p.conversation_id,
            messageId: p.message_id,
          });
        },
      );
      unlisteners.push(u4);

      const u5 = await listen(
        "chat_awaiting_approval",
        (e: {
          payload: {
            conversation_id: number;
            message_id: number;
            call_id: string;
            pending_id: number;
          };
        }) => {
          const p = e.payload;
          if (p.conversation_id !== activeConvIdRef.current) return;
          dispatch({
            type: "TOOL_AWAITING",
            conversationId: p.conversation_id,
            messageId: p.message_id,
            callId: p.call_id,
            pendingId: p.pending_id,
          });
        },
      );
      unlisteners.push(u5);
    };

    void setup();

    return () => {
      for (const u of unlisteners) u();
      if (rafRef.current !== null) {
        cancelAnimationFrame(rafRef.current);
        rafRef.current = null;
      }
      pendingTextRef.current = "";
    };
  }, [conversationId]);

  const sendMessage = useCallback(
    async (text: string) => {
      if (!conversationId) return;
      dispatch({ type: "RESET" });
      await sendChatMessage(conversationId, text);
    },
    [conversationId],
  );

  const cancelMessage = useCallback(() => {
    if (!conversationId) return;
    void cancelChatMessage(conversationId);
  }, [conversationId]);

  const isStreaming =
    state.streamingMessage?.status === "streaming" ||
    state.streamingMessage?.status === "tool_running" ||
    state.streamingMessage?.status === "awaiting_approval";

  return {
    streamingMessage: state.streamingMessage,
    isStreaming,
    sendMessage,
    cancelMessage,
  };
}
