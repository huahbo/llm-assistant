import { useState, useRef, useEffect } from "react";
import type { Conversation } from "../../types";

interface Props {
  conversations: Conversation[];
  selectedId: number | null;
  showArchived: boolean;
  onSelect: (id: number) => void;
  onNew: () => void;
  onRename: (id: number, title: string) => void;
  onArchive: (id: number, archived: boolean) => void;
  onDelete: (id: number) => void;
  onToggleArchived: () => void;
}

function formatConvTime(dateStr: string): string {
  // updated_at 来自 current_timestamp_ms()，是纯数字毫秒时间戳字符串
  const ms = /^\d{10,}$/.test(dateStr) ? Number(dateStr) : new Date(dateStr).getTime();
  const date = new Date(ms);
  if (isNaN(date.getTime())) return "-";

  const diff = Date.now() - date.getTime();
  const mins = Math.floor(diff / 60_000);
  if (mins < 1) return "刚刚";
  if (mins < 60) return `${mins}分钟前`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}小时前`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}天前`;

  const y = date.getFullYear();
  const m = date.getMonth() + 1;
  const d = date.getDate();
  return y === new Date().getFullYear()
    ? `${m}月${d}日`
    : `${y}-${String(m).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
}

export default function ConversationList({
  conversations,
  selectedId,
  showArchived,
  onSelect,
  onNew,
  onRename,
  onArchive,
  onDelete,
  onToggleArchived,
}: Props) {
  const [openMenuId, setOpenMenuId] = useState<number | null>(null);
  const [search, setSearch] = useState("");
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

  const filtered = search.trim()
    ? conversations.filter((c) =>
        c.title.toLowerCase().includes(search.toLowerCase()),
      )
    : conversations;

  return (
    <div className="chat-convlist">
      <div className="chat-convlist__header">
        <button type="button" className="chat-convlist__new-btn" onClick={onNew}>
          + 新对话
        </button>
        <input
          className="chat-convlist__search"
          type="text"
          placeholder="搜索对话..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <label className="chat-convlist__archive-toggle">
          <input
            type="checkbox"
            checked={showArchived}
            onChange={onToggleArchived}
          />
          显示已归档
        </label>
      </div>
      <div className="chat-convlist__list">
        {filtered.map((conv) => (
          <div
            key={conv.id}
            className={`chat-convlist__item${selectedId === conv.id ? " chat-convlist__item--active" : ""}${conv.archived ? " chat-convlist__item--archived" : ""}`}
            onClick={() => onSelect(conv.id)}
            style={{ position: "relative" }}
          >
            <div className="chat-convlist__item-title" title={conv.title}>
              {conv.title}
              <div style={{ fontSize: 11, opacity: 0.6, marginTop: 2 }}>
                {formatConvTime(conv.updated_at)}
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
                {[
                  {
                    label: "重命名",
                    color: undefined,
                    action: () => handleRename(conv.id, conv.title),
                  },
                  {
                    label: conv.archived ? "取消归档" : "归档",
                    color: undefined,
                    action: () => {
                      setOpenMenuId(null);
                      onArchive(conv.id, !conv.archived);
                    },
                  },
                  {
                    label: "删除",
                    color: "var(--color-error, #ef4444)",
                    action: () => handleDelete(conv.id),
                  },
                ].map((item) => (
                  <button
                    key={item.label}
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
                      color: item.color,
                    }}
                    onClick={(e) => {
                      e.stopPropagation();
                      item.action();
                    }}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
            )}
          </div>
        ))}
        {filtered.length === 0 && (
          <div style={{ padding: "12px 8px", fontSize: 13, opacity: 0.5, textAlign: "center" }}>
            {search ? "无匹配对话" : "暂无对话"}
          </div>
        )}
      </div>
    </div>
  );
}
