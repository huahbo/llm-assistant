import { useEffect, useRef, useState } from "react";
import type { ChatMessage, ChatStreamingMessage } from "../../types";
import { listChatMessages, approveChatShell, rejectChatShell } from "../../tauri-client";
import MessageBubble from "./MessageBubble";
import ChatInputBar from "./ChatInputBar";

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
  streamingMessage: ChatStreamingMessage | null;
  isStreaming: boolean;
  onSend: (text: string) => void;
  onCancel: () => void;
  prefillText?: string;
  shellMode?: "off" | "approval" | "yolo";
  onShellModeChange?: (mode: "off" | "approval" | "yolo") => void;
}

export default function MessageThread({
  conversationId,
  streamingMessage,
  isStreaming,
  onSend,
  onCancel,
  prefillText,
  shellMode = "off",
  onShellModeChange,
}: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [pendingUserMsg, setPendingUserMsg] = useState<string | null>(null);
  const [shellApproval, setShellApproval] = useState<ShellApprovalPayload | null>(null);
  const [shellCountdown, setShellCountdown] = useState(30);
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

  const handleSend = (text: string) => {
    setPendingUserMsg(text);
    onSend(text);
  };

  return (
    <div className="chat-thread">
      <div className="chat-thread__messages">
        {messages
          .filter((msg) => !streamingMessage || msg.id !== streamingMessage.message_id)
          .map((msg) => (
            <MessageBubble
              key={msg.id}
              role={msg.role}
              content={msg.content}
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
      />
    </div>
  );
}
