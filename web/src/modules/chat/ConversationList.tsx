import { useState, useRef, useEffect } from "react";
import type { Conversation } from "../../types";

interface Props {
  conversations: Conversation[];
  selectedId: number | null;
  onSelect: (id: number) => void;
  onNew: () => void;
  onRename: (id: number, title: string) => void;
  onDelete: (id: number) => void;
}

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60_000);
  if (mins < 1) return "刚刚";
  if (mins < 60) return `${mins}分钟前`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}小时前`;
  const days = Math.floor(hours / 24);
  return `${days}天前`;
}

export default function ConversationList({
  conversations,
  selectedId,
  onSelect,
  onNew,
  onRename,
  onDelete,
}: Props) {
  const [openMenuId, setOpenMenuId] = useState<number | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (openMenuId === null) return;
    const handleClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setOpenMenuId(null);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [openMenuId]);

  const handleRename = (id: number, currentTitle: string) => {
    setOpenMenuId(null);
    const newTitle = window.prompt("重命名对话", currentTitle);
    if (newTitle && newTitle.trim()) {
      onRename(id, newTitle.trim());
    }
  };

  const handleDelete = (id: number) => {
    setOpenMenuId(null);
    if (window.confirm("确认删除该对话？")) {
      onDelete(id);
    }
  };

  return (
    <div className="chat-convlist">
      <div className="chat-convlist__header">
        <button
          type="button"
          className="chat-convlist__new-btn"
          onClick={onNew}
        >
          + 新对话
        </button>
      </div>
      <div className="chat-convlist__list">
        {conversations.map((conv) => (
          <div
            key={conv.id}
            className={`chat-convlist__item${selectedId === conv.id ? " chat-convlist__item--active" : ""}`}
            onClick={() => onSelect(conv.id)}
            style={{ position: "relative" }}
          >
            <div className="chat-convlist__item-title" title={conv.title}>
              {conv.title}
              <div style={{ fontSize: 11, opacity: 0.6, marginTop: 2 }}>
                {relativeTime(conv.updated_at)}
              </div>
            </div>
            <button
              type="button"
              className="chat-convlist__item-menu"
              onClick={(e) => {
                e.stopPropagation();
                setOpenMenuId(openMenuId === conv.id ? null : conv.id);
              }}
            >
              ···
            </button>
            {openMenuId === conv.id && (
              <div
                ref={menuRef}
                style={{
                  position: "absolute",
                  right: 0,
                  top: "100%",
                  zIndex: 100,
                  background: "var(--bg-primary, white)",
                  border: "1px solid var(--border-color, #e5e7eb)",
                  borderRadius: 6,
                  boxShadow: "0 4px 12px rgba(0,0,0,0.1)",
                  minWidth: 100,
                }}
              >
                <button
                  type="button"
                  style={{
                    display: "block",
                    width: "100%",
                    padding: "6px 12px",
                    textAlign: "left",
                    border: "none",
                    background: "transparent",
                    cursor: "pointer",
                    fontSize: 13,
                  }}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleRename(conv.id, conv.title);
                  }}
                >
                  重命名
                </button>
                <button
                  type="button"
                  style={{
                    display: "block",
                    width: "100%",
                    padding: "6px 12px",
                    textAlign: "left",
                    border: "none",
                    background: "transparent",
                    cursor: "pointer",
                    fontSize: 13,
                    color: "var(--color-error, #ef4444)",
                  }}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDelete(conv.id);
                  }}
                >
                  删除
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
