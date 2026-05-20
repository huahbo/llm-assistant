import { useState } from "react";
import type { UrlContextCard as UrlContextCardData } from "../../tauri-client";

interface Props {
  card: UrlContextCardData;
  onRemove: () => void;
}

export default function UrlContextCard({ card, onRemove }: Props) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="url-ctx-card">
      <div className="url-ctx-card__header" onClick={() => setExpanded(v => !v)}>
        <span className="url-ctx-card__domain">🔗 {card.domain}</span>
        <span className="url-ctx-card__toggle">{expanded ? "▲" : "▼"}</span>
        <button
          className="url-ctx-card__remove"
          onClick={(e) => { e.stopPropagation(); onRemove(); }}
          title="移除页面上下文"
        >
          ×
        </button>
      </div>
      <div className="url-ctx-card__title">{card.title || card.url}</div>
      {expanded && (
        <div className="url-ctx-card__body">
          <div className="url-ctx-card__summary">{card.summary}{card.summary.length >= 300 ? "…" : ""}</div>
          <div className="url-ctx-card__meta">
            {card.charCount.toLocaleString()} 字符 · {card.fetchMethod}
          </div>
        </div>
      )}
    </div>
  );
}
