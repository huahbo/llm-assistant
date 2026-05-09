import { useEffect, useState } from "react";
import type { Conversation } from "../../types";
import {
  listConversations,
  createConversation,
  renameConversation,
  archiveConversation,
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
  const [showArchived, setShowArchived] = useState(false);

  const { streamingMessage, isStreaming, sendMessage, cancelMessage } =
    useChatStream(selectedConvId);

  const loadConversations = async () => {
    const convs = await listConversations(showArchived);
    setConversations(convs);
  };

  useEffect(() => {
    void loadConversations();
  }, [showArchived]);

  // Listen for auto-generated title updates
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

  const handleArchive = async (id: number, archived: boolean) => {
    await archiveConversation(id, archived);
    if (!showArchived && archived && selectedConvId === id) setSelectedConvId(null);
    await loadConversations();
  };

  const handleDelete = async (id: number) => {
    await deleteConversation(id);
    if (selectedConvId === id) setSelectedConvId(null);
    await loadConversations();
  };

  return (
    <>
      <div className="chat-module">
        <div className="chat-module__left">
          <ConversationList
            conversations={conversations}
            selectedId={selectedConvId}
            showArchived={showArchived}
            onSelect={setSelectedConvId}
            onNew={() => setShowNewDialog(true)}
            onRename={(id, title) => void handleRename(id, title)}
            onArchive={(id, archived) => void handleArchive(id, archived)}
            onDelete={(id) => void handleDelete(id)}
            onToggleArchived={() => setShowArchived((v) => !v)}
          />
        </div>
        <div className="chat-module__right">
          {selectedConvId ? (
            <MessageThread
              conversationId={selectedConvId}
              streamingMessage={streamingMessage}
              isStreaming={isStreaming}
              onSend={(text) => void sendMessage(text)}
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
