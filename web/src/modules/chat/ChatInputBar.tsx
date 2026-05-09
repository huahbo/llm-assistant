import { useRef } from "react";

interface Props {
  isStreaming: boolean;
  onSend: (text: string) => void;
  onCancel: () => void;
  disabled?: boolean;
}

export default function ChatInputBar({ isStreaming, onSend, onCancel, disabled }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSend = () => {
    const text = textareaRef.current?.value.trim() ?? "";
    if (!text || isStreaming) return;
    if (textareaRef.current) textareaRef.current.value = "";
    onSend(text);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="chat-inputbar">
      <textarea
        ref={textareaRef}
        className="chat-inputbar__textarea"
        rows={3}
        placeholder="输入消息... (Enter 发送，Shift+Enter 换行)"
        disabled={isStreaming || disabled}
        onKeyDown={handleKeyDown}
      />
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
  );
}
