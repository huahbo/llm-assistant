import { useEffect, useRef, useState } from "react";
import type { ChatMessage, ChatStreamSegment, ChatStreamingMessage } from "../../types";
import type { UrlContextCard as UrlContextCardData } from "../../tauri-client";
import { listChatMessages, approveChatShell, rejectChatShell, exportConversationMarkdown, saveWikiPage } from "../../tauri-client";
import MessageBubble from "./MessageBubble";
import ChatInputBar from "./ChatInputBar";

interface DisplayGroup {
  messageId: number;
  role: "user" | "assistant";
  content: string | null;
  segments: ChatStreamSegment[] | undefined;
}

function buildDisplayGroups(messages: ChatMessage[]): DisplayGroup[] {
  const toolResultsByCallId = new Map<string, string>();
  for (const msg of messages) {
    if (msg.role === "tool" && msg.tool_call_id && msg.content) {
      toolResultsByCallId.set(msg.tool_call_id, msg.content);
    }
  }

  const groups: DisplayGroup[] = [];
  for (const msg of messages) {
    if (msg.role === "system" || msg.role === "tool") continue;

    if (msg.role === "user") {
      groups.push({ messageId: msg.id, role: "user", content: msg.content, segments: undefined });
      continue;
    }

    // assistant — reconstruct segments if tool_calls present
    const toolSegs: ChatStreamSegment[] = [];
    if (msg.tool_calls && msg.tool_calls.length > 0) {
      for (const tc of msg.tool_calls) {
        let args: Record<string, unknown> = {};
        try { args = JSON.parse(tc.function.arguments) as Record<string, unknown>; } catch { /* ok */ }
        const resultContent = toolResultsByCallId.get(tc.id);
        toolSegs.push({
          kind: "tool",
          call_id: tc.id,
          tool_name: tc.function.name,
          args,
          result: resultContent !== undefined
            ? { ok: true, preview: resultContent, latency_ms: 0 }
            : undefined,
          status: resultContent !== undefined ? "ok" : "err",
        });
      }
    }

    const segments: ChatStreamSegment[] | undefined =
      toolSegs.length > 0
        ? [
            ...toolSegs,
            ...(msg.content ? [{ kind: "text" as const, text: msg.content, streaming: false }] : []),
          ]
        : undefined;

    groups.push({ messageId: msg.id, role: "assistant", content: msg.content, segments });
  }
  return groups;
}

interface ShellApprovalPayload {
  conversation_id: number;
  message_id: number;
  call_id: string;
  pending_id: number;
  tool_name: string;
  args: string;
}

interface Props {
  conversationId: number;
  conversationTitle?: string;
  streamingMessage: ChatStreamingMessage | null;
  isStreaming: boolean;
  onSend: (text: string) => void;
  onCancel: () => void;
  prefillText?: string;
  shellMode?: "off" | "approval" | "yolo";
  onShellModeChange?: (mode: "off" | "approval" | "yolo") => void;
  onSavedToWiki?: (path: string) => void;
}

export default function MessageThread({
  conversationId,
  conversationTitle,
  streamingMessage,
  isStreaming,
  onSend,
  onCancel,
  prefillText,
  shellMode = "off",
  onShellModeChange,
  onSavedToWiki,
}: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [pendingUserMsg, setPendingUserMsg] = useState<string | null>(null);
  const [urlCards, setUrlCards] = useState<UrlContextCardData[]>([]);
  const [shellApproval, setShellApproval] = useState<ShellApprovalPayload | null>(null);
  const [shellCountdown, setShellCountdown] = useState(30);
  const [exportLabel, setExportLabel] = useState("导出");
  const [saveLabel, setSaveLabel] = useState("存入 Wiki");
  const countdownRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const prevIsStreaming = useRef(false);

  const loadMessages = async (id: number) => {
    const msgs = await listChatMessages(id);
    setMessages(msgs);
  };

  useEffect(() => {
    setMessages([]);
    setPendingUserMsg(null);
    setUrlCards([]);
    setShellApproval(null);
    void loadMessages(conversationId);
  }, [conversationId]);

  useEffect(() => {
    if (prevIsStreaming.current && !isStreaming) {
      void loadMessages(conversationId).then(() => setPendingUserMsg(null));
    }
    prevIsStreaming.current = isStreaming;
  }, [isStreaming, conversationId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingMessage, pendingUserMsg, shellApproval]);

  // 监听 shell 审批事件
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void import("@tauri-apps/api/event").then(({ listen }) => {
      void listen<ShellApprovalPayload>("chat_awaiting_approval", (event) => {
        const p = event.payload;
        if (p.tool_name !== "run_shell") return;
        if (p.conversation_id !== conversationId) return;
        setShellApproval(p);
        setShellCountdown(30);
      }).then((fn) => { unlisten = fn; });
    });
    return () => { unlisten?.(); };
  }, [conversationId]);

  // 倒计时 30s 自动拒绝
  useEffect(() => {
    if (!shellApproval) { if (countdownRef.current) clearInterval(countdownRef.current); return; }
    countdownRef.current = setInterval(() => {
      setShellCountdown((n) => {
        if (n <= 1) {
          void handleRejectShell();
          return 0;
        }
        return n - 1;
      });
    }, 1000);
    return () => { if (countdownRef.current) clearInterval(countdownRef.current); };
  }, [shellApproval?.pending_id]);

  const handleApproveShell = async () => {
    if (!shellApproval) return;
    const id = shellApproval.pending_id;
    setShellApproval(null);
    await approveChatShell(id);
  };

  const handleRejectShell = async () => {
    if (!shellApproval) return;
    const id = shellApproval.pending_id;
    setShellApproval(null);
    await rejectChatShell(id);
  };

  const handleExport = async () => {
    const md = await exportConversationMarkdown(conversationId);
    if (!md) return;
    await navigator.clipboard.writeText(md);
    setExportLabel("已复制");
    setTimeout(() => setExportLabel("导出"), 2000);
  };

  const handleSaveToWiki = async () => {
    const md = await exportConversationMarkdown(conversationId);
    if (!md) return;
    const title = conversationTitle ?? `对话 #${conversationId}`;
    // Generate a URL-safe slug from the title
    const slug = title
      .toLowerCase()
      .replace(/[^\w一-龥\s-]/g, "")
      .trim()
      .replace(/\s+/g, "-")
      .slice(0, 60);
    const path = `chat-exports/${slug}.md`;
    const result = await saveWikiPage(path, md);
    if (result) {
      setSaveLabel("已保存");
      setTimeout(() => setSaveLabel("存入 Wiki"), 2000);
      onSavedToWiki?.(path);
    }
  };

  const handleSend = (text: string) => {
    setPendingUserMsg(text);
    onSend(text);
  };

  return (
    <div className="chat-thread">
      <div className="chat-thread__toolbar">
        <button
          type="button"
          className="chat-thread__export-btn"
          onClick={() => void handleExport()}
          disabled={messages.length === 0}
          title="导出为 Markdown（复制到剪贴板）"
        >
          {exportLabel}
        </button>
        <button
          type="button"
          className="chat-thread__export-btn"
          onClick={() => void handleSaveToWiki()}
          disabled={messages.length === 0}
          title="将对话保存为 Wiki 页面（chat-exports/）"
        >
          {saveLabel}
        </button>
      </div>
      <div className="chat-thread__messages">
        {buildDisplayGroups(
            messages.filter((msg) => !streamingMessage || msg.id !== streamingMessage.message_id)
          ).map((group) => (
            <MessageBubble
              key={group.messageId}
              role={group.role}
              content={group.segments ? null : group.content}
              segments={group.segments}
              streaming={false}
            />
          ))}
        {pendingUserMsg !== null && (
          <MessageBubble role="user" content={pendingUserMsg} />
        )}
        {streamingMessage && (
          <MessageBubble
            role="assistant"
            content={null}
            segments={streamingMessage.segments}
            streaming={isStreaming}
            streamStatus={streamingMessage.status}
          />
        )}
        {shellApproval && (
          <div className="shell-approval-card">
            <div className="shell-approval-card__header">
              <span className="shell-approval-card__icon">🖥</span>
              <span className="shell-approval-card__title">Shell 审批请求</span>
              <span className="shell-approval-card__countdown">{shellCountdown}s</span>
            </div>
            <pre className="shell-approval-card__command">$ {(() => { try { return (JSON.parse(shellApproval.args) as Record<string, string>).command ?? shellApproval.args; } catch { return shellApproval.args; } })()}</pre>
            <div className="shell-approval-card__actions">
              <button
                className="shell-approval-card__btn shell-approval-card__btn--approve"
                onClick={() => void handleApproveShell()}
              >批准执行</button>
              <button
                className="shell-approval-card__btn shell-approval-card__btn--reject"
                onClick={() => void handleRejectShell()}
              >拒绝</button>
            </div>
          </div>
        )}
        <div ref={bottomRef} />
      </div>
      <ChatInputBar
        isStreaming={isStreaming}
        onSend={handleSend}
        onCancel={onCancel}
        prefillText={prefillText}
        conversationId={conversationId}
        shellMode={shellMode}
        onShellModeChange={onShellModeChange}
        urlCards={urlCards}
        onUrlCardAdded={(card) => setUrlCards(prev => [...prev, card])}
        onUrlCardRemoved={(url) => setUrlCards(prev => prev.filter(c => c.url !== url))}
      />
    </div>
  );
}
