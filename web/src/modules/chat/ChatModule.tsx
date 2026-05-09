import { useEffect, useState } from "react";
import type { Conversation } from "../../types";
import {
  listConversations,
  createConversation,
  renameConversation,
  deleteConversation,
  isTauriRuntime,
} from "../../tauri-client";
import { useChatStream } from "./hooks/useChatStream";
import ConversationList from "./ConversationList";
import MessageThread from "./MessageThread";
import NewConversationDialog from "./NewConversationDialog";

export default function ChatModule() {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [selectedConvId, setSelectedConvId] = useState<number | null>(null);
  const [showNewDialog, setShowNewDialog] = useState(false);

  const { streamingMessage, isStreaming, sendMessage, cancelMessage } =
    useChatStream(selectedConvId);

  const loadConversations = async () => {
    const convs = await listConversations();
    setConversations(convs);
  };

  useEffect(() => {
    void loadConversations();
  }, []);

  // 监听后端自动生成的标题更新事件
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | null = null;
    const setup = async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<{ conversation_id: number; title: string }>(
        "chat_title_updated",
        () => { void loadConversations(); },
      );
    };
    void setup();
    return () => { if (unlisten) unlisten(); };
  }, []);

  const handleNewConfirm = async (
    title: string,
    skillKey?: string,
    injectMemories?: boolean,
  ) => {
    setShowNewDialog(false);
    const id = await createConversation(title, skillKey, injectMemories);
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
    <>
    <div className="chat-module">
      <div className="chat-module__left">
        <ConversationList
          conversations={conversations}
          selectedId={selectedConvId}
          onSelect={setSelectedConvId}
          onNew={() => setShowNewDialog(true)}
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
    {showNewDialog && (
      <NewConversationDialog
        onConfirm={(t, sk, im) => void handleNewConfirm(t, sk, im)}
        onCancel={() => setShowNewDialog(false)}
      />
    )}
    </>
  );
}
