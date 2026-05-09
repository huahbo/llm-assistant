import { useState } from "react";
import type { ChatStreamSegment } from "../../types";
import { approveChatWrite, rejectChatWrite } from "../../tauri-client";

type ToolStatus = (ChatStreamSegment & { kind: "tool" })["status"];

interface Props {
  toolName: string;
  args: Record<string, unknown>;
  result?: { ok: boolean; preview: string; latency_ms: number };
  status: ToolStatus;
  pendingId?: number;
}

function statusIcon(status: ToolStatus): string {
  switch (status) {
    case "running": return "⏳";
    case "ok": return "✓";
    case "err": return "✗";
    case "awaiting_approval": return "⏸";
    default: return "…";
  }
}

export default function ToolCallCard({ toolName, args, result, status, pendingId }: Props) {
  const [expanded, setExpanded] = useState(status === "awaiting_approval");

  const handleApprove = async () => {
    if (pendingId != null) await approveChatWrite(pendingId);
  };

  const handleReject = async () => {
    if (pendingId != null) await rejectChatWrite(pendingId);
  };

  return (
    <div className="chat-tool-card">
      <div className="chat-tool-card__header" onClick={() => setExpanded((v) => !v)}>
        <span>🔧</span>
        <span>{toolName}</span>
        <span>{statusIcon(status)}</span>
        {result && <span>{result.latency_ms}ms</span>}
        <span style={{ marginLeft: "auto" }}>{expanded ? "▲" : "▼"}</span>
      </div>
      {expanded && (
        <div className="chat-tool-card__body">
          <pre className="chat-tool-card__pre">{JSON.stringify(args, null, 2)}</pre>
          {result && (
            <>
              <div
                style={{
                  fontSize: 11,
                  color: result.ok
                    ? "var(--text-secondary, #6b7280)"
                    : "var(--color-error, #ef4444)",
                  marginTop: 4,
                }}
              >
                {result.ok ? "结果：" : "错误："}
              </div>
              <pre className="chat-tool-card__pre">{result.preview}</pre>
            </>
          )}
          {status === "awaiting_approval" && (
            <div className="chat-tool-card__approval">
              <span style={{ fontSize: 12, color: "var(--text-secondary, #6b7280)" }}>
                等待审批写操作
              </span>
              <div className="chat-tool-card__approval-btns">
                <button
                  type="button"
                  className="chat-tool-card__btn chat-tool-card__btn--approve"
                  onClick={(e) => { e.stopPropagation(); void handleApprove(); }}
                >
                  批准
                </button>
                <button
                  type="button"
                  className="chat-tool-card__btn chat-tool-card__btn--reject"
                  onClick={(e) => { e.stopPropagation(); void handleReject(); }}
                >
                  拒绝
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
