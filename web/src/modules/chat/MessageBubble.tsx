import type { ChatMessage, ChatStreamingMessage, ChatStreamSegment } from "../../types";
import ToolCallCard from "./ToolCallCard";

interface Props {
  role: ChatMessage["role"];
  content: string | null;
  segments?: ChatStreamSegment[];
  streaming?: boolean;
  streamStatus?: ChatStreamingMessage["status"];
}

export default function MessageBubble({ role, content, segments, streaming, streamStatus }: Props) {
  if (role === "system" || role === "tool") return null;

  if (role === "user") {
    return (
      <div className="chat-bubble chat-bubble--user">
        {content ?? ""}
      </div>
    );
  }

  if (segments && segments.length > 0) {
    return (
      <div className="chat-bubble chat-bubble--assistant">
        {segments.map((seg, i) => {
          if (seg.kind === "text") {
            return (
              <span key={i}>
                {seg.text}
                {streaming && seg.streaming && (
                  <span className="chat-bubble__cursor">▍</span>
                )}
              </span>
            );
          }
          if (seg.kind === "tool") {
            return (
              <ToolCallCard
                key={seg.call_id}
                toolName={seg.tool_name}
                args={seg.args}
                result={seg.result}
                status={seg.status}
                pendingId={seg.pending_id}
              />
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
      </div>
    );
  }

  return (
    <div className="chat-bubble chat-bubble--assistant">
      {content ?? ""}
      {streamStatus === "cancelled" && (
        <span style={{ fontSize: 11, opacity: 0.6, marginLeft: 6 }}>[已取消]</span>
      )}
    </div>
  );
}
