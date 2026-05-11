import { useEffect, useRef, useState } from "react";
import type { ChatMessage, ChatStreamingMessage } from "../../types";
import { listChatMessages } from "../../tauri-client";
import MessageBubble from "./MessageBubble";
import ChatInputBar from "./ChatInputBar";

interface Props {
  conversationId: number;
  streamingMessage: ChatStreamingMessage | null;
  isStreaming: boolean;
  onSend: (text: string) => void;
  onCancel: () => void;
  prefillText?: string;
}

export default function MessageThread({
  conversationId,
  streamingMessage,
  isStreaming,
  onSend,
  onCancel,
  prefillText,
}: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [pendingUserMsg, setPendingUserMsg] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const prevIsStreaming = useRef(false);

  const loadMessages = async (id: number) => {
    const msgs = await listChatMessages(id);
    setMessages(msgs);
  };

  useEffect(() => {
    setMessages([]);
    setPendingUserMsg(null);
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
  }, [messages, streamingMessage, pendingUserMsg]);

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
          <MessageBubble
            role="user"
            content={pendingUserMsg}
          />
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
        <div ref={bottomRef} />
      </div>
      <ChatInputBar
        isStreaming={isStreaming}
        onSend={handleSend}
        onCancel={onCancel}
        prefillText={prefillText}
      />
    </div>
  );
}
