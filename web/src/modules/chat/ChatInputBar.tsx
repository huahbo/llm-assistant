import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { FileChunk } from "../../tauri-client";
import { setConvShellMode } from "../../tauri-client";
import UrlContextCard from "./UrlContextCard";
import type { UrlContextCard as UrlContextCardData } from "../../tauri-client";
import { fetchUrlContext } from "../../tauri-client";
import PlusMenu from "./PlusMenu";
import McpGallery from "./McpGallery";
import SkillInstaller from "./SkillInstaller";

type ShellMode = "off" | "approval" | "yolo";

const SHELL_CYCLE: ShellMode[] = ["off", "approval", "yolo"];
const SHELL_LABEL: Record<ShellMode, string> = { off: "", approval: "🔒 审批", yolo: "⚡ Yolo" };
const SHELL_TITLE: Record<ShellMode, string> = {
  off: "Shell 未启用，点击开启审批模式",
  approval: "Shell 审批模式（每次询问），点击切换到 Yolo",
  yolo: "Shell Yolo 模式（直接执行），点击关闭",
};

interface Props {
  isStreaming: boolean;
  onSend: (text: string) => void;
  onCancel: () => void;
  disabled?: boolean;
  prefillText?: string;
  conversationId?: number;
  shellMode?: ShellMode;
  onShellModeChange?: (mode: ShellMode) => void;
  urlCards?: UrlContextCardData[];
  onUrlCardAdded?: (card: UrlContextCardData) => void;
  onUrlCardRemoved?: (url: string) => void;
}

const CHAR_LIMIT = 40_000;

function buildMessageWithFiles(text: string, chunks: FileChunk[]): string {
  if (chunks.length === 0) return text;
  const fileBlocks = chunks
    .map((c) => {
      const truncNote = c.truncated ? `（已截断至 ${CHAR_LIMIT.toLocaleString()} 字）` : "";
      return `[📎 ${c.filename} · ${c.char_count.toLocaleString()}字${truncNote}]\n${"━".repeat(40)}\n${c.content}\n${"━".repeat(40)}`;
    })
    .join("\n\n");
  return text.trim() ? `${fileBlocks}\n\n${text}` : fileBlocks;
}

// ── 全屏弹窗容器 ──────────────────────────────────────────────────────────────

interface ModalProps {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
}

function ChatModal({ title, onClose, children }: ModalProps) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  return createPortal(
    <div className="chat-modal" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="chat-modal__dialog">
        <div className="chat-modal__header">
          <span className="chat-modal__title">{title}</span>
          <button className="chat-modal__close" onClick={onClose} title="关闭 (Esc)">✕</button>
        </div>
        <div className="chat-modal__body">{children}</div>
      </div>
    </div>,
    document.body,
  );
}

// ── 主组件 ────────────────────────────────────────────────────────────────────

export default function ChatInputBar({ isStreaming, onSend, onCancel, disabled, prefillText, conversationId, shellMode = "off", onShellModeChange, urlCards = [], onUrlCardAdded, onUrlCardRemoved }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const appliedPrefillRef = useRef<string>("");
  const [showPlus, setShowPlus] = useState(false);
  const [fileChunks, setFileChunks] = useState<FileChunk[]>([]);
  const [mcpOpen, setMcpOpen] = useState(false);
  const [skillOpen, setSkillOpen] = useState(false);

  const handleShellToggle = async () => {
    const next = SHELL_CYCLE[(SHELL_CYCLE.indexOf(shellMode) + 1) % SHELL_CYCLE.length];
    if (conversationId != null) await setConvShellMode(conversationId, next);
    onShellModeChange?.(next);
  };

  useEffect(() => {
    if (!prefillText || !textareaRef.current) return;
    if (appliedPrefillRef.current === prefillText) return;
    appliedPrefillRef.current = prefillText;
    textareaRef.current.value = prefillText;
    textareaRef.current.focus();
  }, [prefillText]);

  const [urlFetching, setUrlFetching] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [slashCmd, setSlashCmd] = useState<{ url: string } | null>(null);

  // /fetch <url> 或 /url <url> slash command 检测
  const SLASH_CMD_RE = /^\/(?:fetch|url|抓取)\s+(https?:\/\/\S+)$/i;

  const handleChange = () => {
    const val = textareaRef.current?.value ?? "";
    const match = val.trim().match(SLASH_CMD_RE);
    setSlashCmd(match ? { url: match[1] } : null);
  };

  const executeSlashFetch = async () => {
    if (!slashCmd || urlFetching) return;
    const url = slashCmd.url;
    if (textareaRef.current) textareaRef.current.value = "";
    setSlashCmd(null);
    setFetchError(null);
    // 已有该 URL 的卡片时先移除，让用户可以刷新
    if (urlCards.some(c => c.url === url)) {
      onUrlCardRemoved?.(url);
    }
    setUrlFetching(true);
    try {
      const card = await fetchUrlContext(url);
      onUrlCardAdded?.(card);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setFetchError(`抓取失败：${msg}`);
      setTimeout(() => setFetchError(null), 5000);
    } finally {
      setUrlFetching(false);
    }
  };

  const handleSend = () => {
    const rawText = textareaRef.current?.value.trim() ?? "";
    if ((!rawText && fileChunks.length === 0 && urlCards.length === 0) || isStreaming) return;

    let text = rawText;
    if (urlCards.length > 0) {
      const ctxBlock = urlCards
        .map(c => `[页面上下文: ${c.url}]\n标题：${c.title}\n摘要：${c.summary}`)
        .join("\n\n---\n\n");
      text = ctxBlock + (rawText ? "\n\n---\n\n" + rawText : "");
    }

    const finalText = buildMessageWithFiles(text, fileChunks);
    if (textareaRef.current) textareaRef.current.value = "";
    setFileChunks([]);
    setSlashCmd(null);
    onSend(finalText);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Escape" && slashCmd) {
      e.preventDefault();
      if (textareaRef.current) textareaRef.current.value = "";
      setSlashCmd(null);
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (slashCmd) {
        void executeSlashFetch();
      } else {
        handleSend();
      }
    }
  };

  const removeChunk = (idx: number) =>
    setFileChunks((prev) => prev.filter((_, i) => i !== idx));

  return (
    <>
      <div className="chat-inputbar">
        {fileChunks.length > 0 && (
          <div className="chat-inputbar__attachments">
            {fileChunks.map((c, i) => (
              <span key={i} className="chat-inputbar__attach-badge">
                📎 {c.filename}
                <span className="chat-inputbar__attach-size">{c.char_count.toLocaleString()}字{c.truncated ? "（截断）" : ""}</span>
                <button
                  className="chat-inputbar__attach-remove"
                  onClick={() => removeChunk(i)}
                  title="移除附件"
                >✕</button>
              </span>
            ))}
          </div>
        )}

        {urlCards.length > 0 && (
          <div className="chat-inputbar__url-cards">
            {urlCards.map(card => (
              <UrlContextCard
                key={card.url}
                card={card}
                onRemove={() => onUrlCardRemoved?.(card.url)}
                onPrefill={(text) => {
                  if (textareaRef.current) {
                    textareaRef.current.value = text;
                    textareaRef.current.focus();
                  }
                }}
              />
            ))}
          </div>
        )}
        {slashCmd && (
          <div className="chat-inputbar__slash-hint">
            <code className="chat-inputbar__slash-kw">/fetch</code>
            <span className="chat-inputbar__slash-hint-url">{slashCmd.url}</span>
            <span className="chat-inputbar__slash-hint-tip">Enter 抓取 · Esc 取消</span>
            <button
              className="chat-inputbar__slash-hint-btn"
              onClick={() => void executeSlashFetch()}
              disabled={urlFetching}
            >
              {urlFetching ? "抓取中…" : "抓取"}
            </button>
          </div>
        )}

        {urlFetching && !slashCmd && (
          <div className="chat-inputbar__url-loading">正在抓取页面内容…</div>
        )}
        {fetchError && (
          <div className="chat-inputbar__url-error">{fetchError}</div>
        )}

        <textarea
          ref={textareaRef}
          className={`chat-inputbar__textarea${slashCmd ? " chat-inputbar__textarea--slash-active" : ""}`}
          rows={3}
          placeholder="输入消息… (Enter 发送，/fetch <url> 抓取页面，Shift+Enter 换行)"
          disabled={isStreaming || disabled}
          onKeyDown={handleKeyDown}
          onChange={handleChange}
        />

        <div className="chat-inputbar__row chat-inputbar__row--actions">
          <div className="chat-inputbar__plus-wrap">
            <button
              type="button"
              className="chat-inputbar__plus-btn"
              onClick={() => setShowPlus((v) => !v)}
              title="添加文件 / MCP / Skill"
              disabled={isStreaming || disabled}
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                <line x1="7" y1="1.5" x2="7" y2="12.5" stroke="white" strokeWidth="2.2" strokeLinecap="round"/>
                <line x1="1.5" y1="7" x2="12.5" y2="7" stroke="white" strokeWidth="2.2" strokeLinecap="round"/>
              </svg>
            </button>
            {showPlus && (
              <PlusMenu
                onFileAttached={(chunk) => {
                  setFileChunks((prev) => [...prev, chunk]);
                  setShowPlus(false);
                }}
                onOpenMcp={() => setMcpOpen(true)}
                onOpenSkill={() => setSkillOpen(true)}
                onClose={() => setShowPlus(false)}
              />
            )}
          </div>

          <div className="chat-inputbar__right">
            {shellMode !== "off" && (
              <button
                type="button"
                className={`chat-inputbar__shell-btn chat-inputbar__shell-btn--${shellMode}`}
                onClick={() => void handleShellToggle()}
                title={SHELL_TITLE[shellMode]}
                disabled={isStreaming || disabled}
              >
                {SHELL_LABEL[shellMode]}
              </button>
            )}
            {shellMode === "off" && (
              <button
                type="button"
                className="chat-inputbar__shell-btn chat-inputbar__shell-btn--off"
                onClick={() => void handleShellToggle()}
                title={SHELL_TITLE["off"]}
                disabled={isStreaming || disabled}
              >
                <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                  <rect x="0.75" y="0.75" width="14.5" height="14.5" rx="2.5" stroke="currentColor" strokeWidth="1.5"/>
                  <path d="M3.5 5.5L6.5 8L3.5 10.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
                  <line x1="8.5" y1="10.5" x2="12" y2="10.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
                </svg>
              </button>
            )}
            {isStreaming ? (
              <button
                type="button"
                className="chat-inputbar__btn chat-inputbar__btn--cancel"
                onClick={onCancel}
              >
                取消
              </button>
            ) : (
              <button
                type="button"
                className="chat-inputbar__btn chat-inputbar__btn--send"
                disabled={disabled}
                onClick={handleSend}
              >
                发送
              </button>
            )}
          </div>
        </div>
      </div>

      {mcpOpen && (
        <ChatModal title="MCP 服务器" onClose={() => setMcpOpen(false)}>
          <McpGallery
            onClose={() => setMcpOpen(false)}
            onAdded={() => {/* 弹窗内已有状态反馈，不需要额外 toast */}}
          />
        </ChatModal>
      )}

      {skillOpen && (
        <ChatModal title="Skill 技能管理" onClose={() => setSkillOpen(false)}>
          <SkillInstaller
            onClose={() => setSkillOpen(false)}
            onInstalled={() => {/* 弹窗内已有状态反馈 */}}
          />
        </ChatModal>
      )}
    </>
  );
}
