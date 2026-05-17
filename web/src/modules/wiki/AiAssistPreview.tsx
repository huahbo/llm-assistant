interface Props {
  text: string;
  loading: boolean;
  error?: string;
  onAccept: () => void;
  onReject: () => void;
}

export default function AiAssistPreview({ text, loading, error, onAccept, onReject }: Props) {
  return (
    <div className="ai-assist-preview">
      <div className="ai-assist-preview__header">
        <span className="ai-assist-preview__label">AI 建议</span>
        {loading && <span className="ai-assist-preview__spinner" aria-label="生成中" />}
      </div>

      {error ? (
        <p className="ai-assist-preview__error">{error}</p>
      ) : (
        <pre className="ai-assist-preview__content">{text || " "}</pre>
      )}

      <div className="ai-assist-preview__actions">
        <button
          type="button"
          className="btn btn--primary ai-assist-preview__accept"
          disabled={loading || !!error || !text}
          onClick={onAccept}
        >
          接受
        </button>
        <button
          type="button"
          className="btn ai-assist-preview__reject"
          onClick={onReject}
        >
          拒绝
        </button>
      </div>
    </div>
  );
}
