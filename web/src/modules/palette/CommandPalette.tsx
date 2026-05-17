import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useMode } from "../../contexts/ModeContext";
import { searchWikiPages } from "../../tauri-client/wiki";
import type { ModuleId, WikiPageItem } from "../../types";
import "./palette.css";

// ── Recent pages (localStorage) ──────────────────────────────────────────────

const RECENT_KEY = "cmd-palette-recent";

type RecentEntry = { title: string; path: string };

function loadRecent(): RecentEntry[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]") as RecentEntry[];
  } catch {
    return [];
  }
}

function pushRecent(entry: RecentEntry) {
  const list = loadRecent().filter((r) => r.path !== entry.path);
  localStorage.setItem(RECENT_KEY, JSON.stringify([entry, ...list].slice(0, 5)));
}

// ── Result types ──────────────────────────────────────────────────────────────

type PageItem    = { kind: "page";    title: string; path: string };
type RecentItem  = { kind: "recent";  title: string; path: string };
type CommandItem = { kind: "command"; label: string; icon: string; action: () => void };
type Selectable  = PageItem | RecentItem | CommandItem;

type Section = { label: string; items: Selectable[]; startIdx: number };

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  open: boolean;
  onClose: () => void;
  onOpenWikiPage: (path: string) => Promise<void>;
}

export default function CommandPalette({ open, onClose, onOpenWikiPage }: Props) {
  const { navigateTo } = useMode();

  const [query, setQuery]           = useState("");
  const [pageResults, setPageResults] = useState<WikiPageItem[]>([]);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // Static commands list (stable reference via useMemo)
  const commands = useMemo<CommandItem[]>(
    () => [
      { kind: "command", label: "Wiki — 浏览 / 新建页面", icon: "📄", action: () => navigateTo("wiki" as ModuleId) },
      { kind: "command", label: "Ask — 问答检索",           icon: "🔍", action: () => navigateTo("ask" as ModuleId) },
      { kind: "command", label: "对话 — Chat",               icon: "💬", action: () => navigateTo("chat" as ModuleId) },
      { kind: "command", label: "Agent Studio",              icon: "🧠", action: () => navigateTo("agent" as ModuleId) },
      { kind: "command", label: "Deep Research — 研究",      icon: "🔬", action: () => navigateTo("research" as ModuleId) },
      { kind: "command", label: "图谱 — 知识图谱",           icon: "🕸", action: () => navigateTo("graph" as ModuleId) },
      { kind: "command", label: "MCP 市场",                  icon: "🔌", action: () => navigateTo("discovery" as ModuleId) },
      { kind: "command", label: "运行队列 & 统计",           icon: "📦", action: () => navigateTo("operations" as ModuleId) },
      { kind: "command", label: "系统设置",                  icon: "⚙",  action: () => navigateTo("settings" as ModuleId) },
    ],
    [navigateTo],
  );

  // Reset state each time palette opens
  useEffect(() => {
    if (open) {
      setQuery("");
      setPageResults([]);
      setSelectedIdx(0);
      const t = setTimeout(() => inputRef.current?.focus(), 40);
      return () => clearTimeout(t);
    }
  }, [open]);

  // Debounced wiki search (only for non-command-prefix queries)
  useEffect(() => {
    if (!open) return;
    const q = query.trim();
    if (!q || q.startsWith(">")) { setPageResults([]); return; }

    const timer = setTimeout(() => {
      void searchWikiPages(q).then((res) => setPageResults(res.slice(0, 8)));
    }, 280);

    return () => clearTimeout(timer);
  }, [query, open]);

  // Reset selection when query changes
  useEffect(() => { setSelectedIdx(0); }, [query]);

  // Build sections (re-runs when palette opens to pick up fresh recents)
  const sections = useMemo<Section[]>(() => {
    const q = query.trim();

    if (q.startsWith(">")) {
      const filter = q.slice(1).trim().toLowerCase();
      const filtered = commands.filter((c) => !filter || c.label.toLowerCase().includes(filter));
      return filtered.length > 0 ? [{ label: "操作命令", items: filtered, startIdx: 0 }] : [];
    }

    if (!q) {
      const recent = loadRecent().map<RecentItem>((r) => ({ kind: "recent", title: r.title, path: r.path }));
      const secs: Section[] = [];
      let idx = 0;
      if (recent.length > 0) {
        secs.push({ label: "最近访问", items: recent, startIdx: idx });
        idx += recent.length;
      }
      secs.push({ label: "操作命令", items: commands, startIdx: idx });
      return secs;
    }

    const pages = pageResults.map<PageItem>((p) => ({
      kind: "page",
      title: p.title || p.path,
      path: p.path,
    }));
    return pages.length > 0 ? [{ label: "Wiki 页面", items: pages, startIdx: 0 }] : [];
  }, [query, pageResults, commands, open]); // `open` triggers re-read of recent

  const totalItems = sections.reduce((n, s) => n + s.items.length, 0);

  // Flatten for keyboard navigation
  const flatItems = useMemo<Selectable[]>(
    () => sections.flatMap((s) => s.items),
    [sections],
  );

  const execute = useCallback(
    async (item: Selectable | undefined) => {
      if (!item) return;
      onClose();
      if (item.kind === "page" || item.kind === "recent") {
        pushRecent({ title: item.title, path: item.path });
        navigateTo("wiki" as ModuleId);
        await onOpenWikiPage(item.path);
      } else {
        item.action();
      }
    },
    [onClose, navigateTo, onOpenWikiPage],
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") { onClose(); return; }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIdx((i) => Math.min(i + 1, totalItems - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIdx((i) => Math.max(i - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      void execute(flatItems[selectedIdx]);
    }
  };

  if (!open) return null;

  return (
    <div
      className="palette-overlay"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="palette-panel" role="dialog" aria-label="命令面板">
        <input
          ref={inputRef}
          className="palette-input"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="搜索 Wiki 页面或输入命令…（> 仅显示命令）"
          autoComplete="off"
          spellCheck={false}
        />

        {sections.length > 0 ? (
          <ul className="palette-results" role="listbox">
            {sections.map((section) =>
              section.items.map((item, localIdx) => {
                const flatIdx = section.startIdx + localIdx;
                const isSelected = flatIdx === selectedIdx;
                const icon =
                  item.kind === "recent"  ? "🕐" :
                  item.kind === "command" ? item.icon :
                  "📄";
                const label = item.kind === "command" ? item.label : item.title;
                const sub   = item.kind !== "command" ? item.path : null;

                return (
                  <>
                    {localIdx === 0 && (
                      <li key={`hdr-${section.label}`} className="palette-section-label">
                        {section.label}
                      </li>
                    )}
                    <li
                      key={`item-${flatIdx}`}
                      role="option"
                      aria-selected={isSelected}
                      className={`palette-item${isSelected ? " palette-item--selected" : ""}`}
                      onClick={() => { void execute(item); }}
                      onMouseEnter={() => setSelectedIdx(flatIdx)}
                    >
                      <span className="palette-icon">{icon}</span>
                      <span className="palette-label">{label}</span>
                      {sub && <span className="palette-path">{sub}</span>}
                    </li>
                  </>
                );
              })
            )}
          </ul>
        ) : query.trim() && !query.trim().startsWith(">") ? (
          <div className="palette-empty">无匹配 Wiki 页面</div>
        ) : null}

        <div className="palette-footer">
          <span>↑↓ 导航</span>
          <span>↵ 确认</span>
          <span>Esc 关闭</span>
        </div>
      </div>
    </div>
  );
}
