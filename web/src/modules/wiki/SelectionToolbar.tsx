import type { AssistAction } from "../../tauri-client/ai-assist";

interface Props {
  onAction: (action: AssistAction) => void;
  disabled?: boolean;
}

const ACTIONS: { key: AssistAction; label: string; title: string }[] = [
  { key: "continue", label: "续写", title: "在选中位置后续写内容" },
  { key: "rewrite", label: "改写", title: "将选中文字改写得更清晰简洁" },
  { key: "expand",  label: "扩写", title: "将选中要点扩展为完整段落" },
];

export default function SelectionToolbar({ onAction, disabled }: Props) {
  return (
    <div className="ai-assist-toolbar">
      <span className="ai-assist-toolbar__label">AI 辅助</span>
      {ACTIONS.map(({ key, label, title }) => (
        <button
          key={key}
          type="button"
          className="ai-assist-toolbar__btn"
          title={title}
          disabled={disabled}
          onClick={() => onAction(key)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
