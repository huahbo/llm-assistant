import { useEffect, useRef, useState } from "react";
import type { Conversation } from "../../types";
import {
  listConversations,
  createConversation,
  renameConversation,
  archiveConversation,
  deleteConversation,
  isTauriRuntime,
  getConvShellMode,
} from "../../tauri-client";
import { useChatStream } from "./hooks/useChatStream";
import ConversationList from "./ConversationList";
import MessageThread from "./MessageThread";
import NewConversationDialog from "./NewConversationDialog";
import { useGraphBridge, extractWikiPaths } from "../../contexts/GraphBridgeContext";

export default function ChatModule() {
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [selectedConvId, setSelectedConvId] = useState<number | null>(null);
  const [showNewDialog, setShowNewDialog] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const [shellMode, setShellMode] = useState<"off" | "approval" | "yolo">("off");
  const autoSelectedRef = useRef(false);

  const { streamingMessage, isStreaming, sendMessage, cancelMessage } =
    useChatStream(selectedConvId);

  useEffect(() => {
    if (selectedConvId == null) { setShellMode("off"); return; }
    void getConvShellMode(selectedConvId).then((m) =>
      setShellMode((m as "off" | "approval" | "yolo") ?? "off")
    );
  }, [selectedConvId]);

  const { setHighlightedPaths, chatPrefill, setChatPrefill } = useGraphBridge();
  const prevStatusRef = useRef<string | null>(null);

  // Extract wiki paths from AI response and highlight in graph
  useEffect(() => {
    const status = streamingMessage?.status ?? null;
    if (status === "done" && prevStatusRef.current !== "done") {
      const text = (streamingMessage?.segments ?? [])
        .filter((s) => s.kind === "text")
        .map((s) => (s as { kind: "text"; text: string }).text)
        .join("");
      const paths = extractWikiPaths(text);
      if (paths.length > 0) setHighlightedPaths(paths);
    }
    prevStatusRef.current = status;
  }, [streamingMessage?.status]);

  // Clear chatPrefill from bridge context after ChatInputBar has had a render cycle
  // to apply it. Without this, switching to a NEW conversation re-applies the stale prefill.
  useEffect(() => {
    if (!chatPrefill || !selectedConvId) return;
    const tid = window.setTimeout(() => setChatPrefill(""), 0);
    return () => window.clearTimeout(tid);
  }, [chatPrefill, selectedConvId]);

  const loadConversations = async () => {
    const convs = await listConversations(showArchived);
    setConversations(convs);
    // 首次加载：自动选最近对话，或静默创建首个对话
    if (!autoSelectedRef.current) {
      autoSelectedRef.current = true;
      if (convs.length > 0) {
        setSelectedConvId(convs[0].id);
      } else {
        const id = await createConversation("新对话");
        if (id !== null) {
          const fresh = await listConversations(showArchived);
          setConversations(fresh);
          setSelectedConvId(id);
        }
      }
    }
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
              prefillText={chatPrefill}
              shellMode={shellMode}
              onShellModeChange={setShellMode}
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
