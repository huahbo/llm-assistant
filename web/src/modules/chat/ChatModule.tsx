import { useEffect, useState } from "react";
import type { Conversation } from "../../types";
import {
  listConversations,
  createConversation,
  renameConversation,
  deleteConversation,
} from "../../tauri-client";
import { useChatStream } from "./hooks/useChatStream";
import ConversationList from "./ConversationList";
import MessageThread from "./MessageThread";

export default function ChatModule() {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [selectedConvId, setSelectedConvId] = useState<number | null>(null);

  const { streamingMessage, isStreaming, sendMessage, cancelMessage } =
    useChatStream(selectedConvId);

  const loadConversations = async () => {
    const convs = await listConversations();
    setConversations(convs);
  };

  useEffect(() => {
    void loadConversations();
  }, []);

  const handleNew = async () => {
    const title = window.prompt("新对话名称", "新对话");
    if (!title || !title.trim()) return;
    const id = await createConversation(title.trim());
    if (id !== null) {
      await loadConversations();
      setSelectedConvId(id);
    }
  };

  const handleRename = async (id: number, newTitle: string) => {
    await renameConversation(id, newTitle);
    await loadConversations();
  };

  const handleDelete = async (id: number) => {
    await deleteConversation(id);
    if (selectedConvId === id) setSelectedConvId(null);
    await loadConversations();
  };

  const handleSend = async (text: string) => {
    await sendMessage(text);
  };

  return (
    <div className="chat-module">
      <div className="chat-module__left">
        <ConversationList
          conversations={conversations}
          selectedId={selectedConvId}
          onSelect={setSelectedConvId}
          onNew={() => void handleNew()}
          onRename={(id, title) => void handleRename(id, title)}
          onDelete={(id) => void handleDelete(id)}
        />
      </div>
      <div className="chat-module__right">
        {selectedConvId ? (
          <MessageThread
            conversationId={selectedConvId}
            streamingMessage={streamingMessage}
            isStreaming={isStreaming}
            onSend={(text) => void handleSend(text)}
            onCancel={cancelMessage}
          />
        ) : (
          <div className="chat-module__empty">选择或新建对话</div>
        )}
      </div>
    </div>
  );
}
