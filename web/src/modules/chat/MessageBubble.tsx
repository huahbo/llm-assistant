import { useState } from "react";
import type { ChatMessage, ChatStreamingMessage, ChatStreamSegment } from "../../types";
import ToolCallCard from "./ToolCallCard";
import MarkdownRenderer from "./MarkdownRenderer";

type ToolSeg = ChatStreamSegment & { kind: "tool" };

function ToolGroup({ tools, streaming }: { tools: ToolSeg[]; streaming: boolean }) {
  const isRunning = tools.some((t) => t.status === "running");
  const isAwaiting = tools.some((t) => t.status === "awaiting_approval");
  const [open, setOpen] = useState(false);
  const showCards = open || isRunning || isAwaiting;

  const doneCount = tools.filter((t) => t.status === "ok" || t.status === "err").length;
  const totalMs = tools.reduce((s, t) => s + (t.result?.latency_ms ?? 0), 0);

  let summary = `🔧 ${tools.length} 次工具调用`;
  if (isRunning) summary += " ⏳";
  else if (isAwaiting) summary += " ⏸ 等待审批";
  else summary += `  ${doneCount}/${tools.length} 完成 · ${totalMs}ms`;

  return (
    <div className="chat-tool-group">
      <button
        type="button"
        className="chat-tool-group__toggle"
        onClick={() => setOpen((v) => !v)}
        disabled={streaming && isRunning}
      >
        <span className="chat-tool-group__label">{summary}</span>
        <span className="chat-tool-group__arrow">{showCards ? "▲" : "▼"}</span>
      </button>
      {showCards && (
        <div className="chat-tool-group__cards">
          {tools.map((seg) => (
            <ToolCallCard
              key={seg.call_id}
              toolName={seg.tool_name}
              args={seg.args}
              result={seg.result}
              status={seg.status}
              pendingId={seg.pending_id}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface Props {
  role: ChatMessage["role"];
  content: string | null;
  segments?: ChatStreamSegment[];
  streaming?: boolean;
  streamStatus?: ChatStreamingMessage["status"];
}

// ── Avatars ────────────────────────────────────────────────────────────────────

function UserAvatar() {
  return (
    <div className="chat-avatar chat-avatar--user" title="你">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
        <circle cx="12" cy="8" r="4" />
        <path d="M4 20c0-4 3.6-7 8-7s8 3 8 7" />
      </svg>
    </div>
  );
}

function BotAvatar() {
  return (
    <div className="chat-avatar chat-avatar--bot" title="AI 助手">
      <svg width="18" height="18" viewBox="0 0 32 32">
        {/* antenna */}
        <line x1="16" y1="3" x2="16" y2="8" stroke="#7c3aed" strokeWidth="2" strokeLinecap="round" />
        <circle cx="16" cy="2" r="2" fill="#a78bfa" />
        {/* body */}
        <rect x="5" y="8" width="22" height="17" rx="5" fill="#7c3aed" />
        {/* ears */}
        <rect x="2" y="13" width="3" height="7" rx="1.5" fill="#6d28d9" />
        <rect x="27" y="13" width="3" height="7" rx="1.5" fill="#6d28d9" />
        {/* eyes */}
        <circle cx="11.5" cy="15.5" r="3" fill="white" />
        <circle cx="11.5" cy="15.5" r="1.5" fill="#3b82f6" />
        <circle cx="12.2" cy="14.8" r="0.6" fill="white" />
        <circle cx="20.5" cy="15.5" r="3" fill="white" />
        <circle cx="20.5" cy="15.5" r="1.5" fill="#3b82f6" />
        <circle cx="21.2" cy="14.8" r="0.6" fill="white" />
        {/* smile */}
        <path d="M11 21 Q16 24 21 21" stroke="white" strokeWidth="1.5" fill="none" strokeLinecap="round" />
      </svg>
    </div>
  );
}

// ── Copy button ────────────────────────────────────────────────────────────────

function CopyBtn({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = () => {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    });
  };
  return (
    <button className="chat-bubble__copy-btn" onClick={handleCopy} title="复制消息">
      {copied ? (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="#22c55e" strokeWidth="2.5">
          <polyline points="20 6 9 17 4 12" />
        </svg>
      ) : (
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      )}
    </button>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────

export default function MessageBubble({ role, content, segments, streaming, streamStatus }: Props) {
  if (role === "system" || role === "tool") return null;

  // User message
  if (role === "user") {
    return (
      <div className="chat-row chat-row--user">
        <div className="chat-bubble chat-bubble--user">
          {content ?? ""}
        </div>
        <UserAvatar />
      </div>
    );
  }

  // Assistant — streaming (segments present)
  if (segments && segments.length > 0) {
    const toolSegs = segments.filter((s): s is ToolSeg => s.kind === "tool");
    const fullText = segments
      .filter((s) => s.kind === "text")
      .map((s) => (s.kind === "text" ? s.text : ""))
      .join("");

    return (
      <div className="chat-row chat-row--assistant">
        <BotAvatar />
        <div className="chat-bubble chat-bubble--assistant">
          {toolSegs.length > 0 && <ToolGroup tools={toolSegs} streaming={!!streaming} />}
          {segments.map((seg, i) => {
            if (seg.kind === "text") {
              return streaming ? (
                <span key={i}>
                  {seg.text}
                  {seg.streaming && <span className="chat-bubble__cursor">▍</span>}
                </span>
              ) : (
                <MarkdownRenderer key={i} content={seg.text} />
              );
            }
            if (seg.kind === "error") {
              return (
                <span key={i} style={{ color: "var(--color-error, #ef4444)" }}>
                  {seg.message}
                </span>
              );
            }
            return null;
          })}
          {!streaming && fullText && <CopyBtn text={fullText} />}
        </div>
      </div>
    );
  }

  // Assistant — persisted from DB (skip empty placeholder messages)
  const text = content ?? "";
  if (!text && streamStatus !== "cancelled") return null;
  return (
    <div className="chat-row chat-row--assistant">
      <BotAvatar />
      <div className="chat-bubble chat-bubble--assistant">
        {text ? <MarkdownRenderer content={text} /> : null}
        {streamStatus === "cancelled" && (
          <span style={{ fontSize: 11, opacity: 0.6, marginLeft: 6 }}>[已取消]</span>
        )}
        {text && <CopyBtn text={text} />}
      </div>
    </div>
  );
}
