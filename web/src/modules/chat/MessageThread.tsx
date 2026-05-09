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
}

export default function MessageThread({
  conversationId,
  streamingMessage,
  isStreaming,
  onSend,
  onCancel,
}: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const bottomRef = useRef<HTMLDivElement>(null);
  const prevIsStreaming = useRef(false);

  const loadMessages = async (id: number) => {
    const msgs = await listChatMessages(id);
    setMessages(msgs);
  };

  useEffect(() => {
    setMessages([]);
    void loadMessages(conversationId);
  }, [conversationId]);

  useEffect(() => {
    if (prevIsStreaming.current && !isStreaming) {
      void loadMessages(conversationId);
    }
    prevIsStreaming.current = isStreaming;
  }, [isStreaming, conversationId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingMessage]);

  return (
    <div className="chat-thread">
      <div className="chat-thread__messages">
        {messages.map((msg) => (
          <MessageBubble
            key={msg.id}
            role={msg.role}
            content={msg.content}
          />
        ))}
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
        onSend={onSend}
        onCancel={onCancel}
      />
    </div>
  );
}
