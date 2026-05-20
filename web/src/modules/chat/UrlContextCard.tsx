import { useState } from "react";
import type { UrlContextCard as UrlContextCardData } from "../../tauri-client";
import { openExternalUrl, enqueueIngest } from "../../tauri-client";

interface Props {
  card: UrlContextCardData;
  onRemove: () => void;
  onPrefill?: (text: string) => void;
}

export default function UrlContextCard({ card, onRemove, onPrefill }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [copyLabel, setCopyLabel] = useState("复制摘要");
  const [ingestLabel, setIngestLabel] = useState("摄入Wiki");

  const handleCopy = async (e: React.MouseEvent) => {
    e.stopPropagation();
    await navigator.clipboard.writeText(card.summary);
    setCopyLabel("已复制");
    setTimeout(() => setCopyLabel("复制摘要"), 2000);
  };

  const handleOpen = (e: React.MouseEvent) => {
    e.stopPropagation();
    void openExternalUrl(card.url);
  };

  const handleIngest = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await enqueueIngest("url", card.url);
      setIngestLabel("已加入队列");
      setTimeout(() => setIngestLabel("摄入Wiki"), 3000);
    } catch {
      setIngestLabel("失败");
      setTimeout(() => setIngestLabel("摄入Wiki"), 2000);
    }
  };

  const handlePrefill = (e: React.MouseEvent) => {
    e.stopPropagation();
    onPrefill?.(`关于「${card.title}」（${card.url}），`);
  };

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
          <div className="url-ctx-card__summary">
            {card.summary}{card.summary.length >= 1200 ? "…" : ""}
          </div>
          <div className="url-ctx-card__meta">
            {card.charCount.toLocaleString()} 字符 · {card.fetchMethod}
          </div>
          <div className="url-ctx-card__actions">
            <button
              className="url-ctx-card__action-btn"
              onClick={(e) => void handleCopy(e)}
              title="复制摘要到剪贴板"
            >
              📋 {copyLabel}
            </button>
            <button
              className="url-ctx-card__action-btn"
              onClick={handleOpen}
              title="在浏览器中打开此页面"
            >
              🔗 打开链接
            </button>
            <button
              className="url-ctx-card__action-btn"
              onClick={(e) => void handleIngest(e)}
              title="将此页面加入摄入队列"
            >
              📥 {ingestLabel}
            </button>
            {onPrefill && (
              <button
                className="url-ctx-card__action-btn url-ctx-card__action-btn--primary"
                onClick={handlePrefill}
                title="以此页面为上下文在输入框提问"
              >
                💬 直接提问
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
