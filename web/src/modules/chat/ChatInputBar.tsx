import { useEffect, useRef, useState } from "react";
import type { FileChunk } from "../../tauri-client";
import { setConvShellMode } from "../../tauri-client";
import PlusMenu from "./PlusMenu";

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

export default function ChatInputBar({ isStreaming, onSend, onCancel, disabled, prefillText, conversationId, shellMode = "off", onShellModeChange }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const appliedPrefillRef = useRef<string>("");
  const [showPlus, setShowPlus] = useState(false);
  const [fileChunks, setFileChunks] = useState<FileChunk[]>([]);

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

  const handleSend = () => {
    const text = textareaRef.current?.value.trim() ?? "";
    if ((!text && fileChunks.length === 0) || isStreaming) return;
    const finalText = buildMessageWithFiles(text, fileChunks);
    if (textareaRef.current) textareaRef.current.value = "";
    setFileChunks([]);
    onSend(finalText);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const removeChunk = (idx: number) =>
    setFileChunks((prev) => prev.filter((_, i) => i !== idx));

  return (
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

      <textarea
        ref={textareaRef}
        className="chat-inputbar__textarea"
        rows={3}
        placeholder="输入消息… (Enter 发送，Shift+Enter 换行)"
        disabled={isStreaming || disabled}
        onKeyDown={handleKeyDown}
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
            +
          </button>
          {showPlus && (
            <PlusMenu
              onFileAttached={(chunk) => {
                setFileChunks((prev) => [...prev, chunk]);
                setShowPlus(false);
              }}
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
              🖥
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
  );
}
