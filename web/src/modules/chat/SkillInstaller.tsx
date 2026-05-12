import { useState } from "react";
import { installSkillFromUrl } from "../../tauri-client";

interface Props {
  onClose: () => void;
  onInstalled: (name: string) => void;
}

export default function SkillInstaller({ onClose, onInstalled }: Props) {
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleInstall = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setLoading(true);
    setError("");
    try {
      await installSkillFromUrl(trimmed);
      // Extract name from URL for feedback
      const parts = trimmed.split("/");
      const guessName = parts[parts.length - 1].replace(/\.json$/, "") || "技能";
      onInstalled(guessName);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="skill-installer">
      <div className="skill-installer__header">
        <span className="skill-installer__title">安装 Skill</span>
        <button className="skill-installer__close" onClick={onClose}>✕</button>
      </div>

      <div className="skill-installer__body">
        <p className="skill-installer__hint">
          粘贴 Skill JSON 文件的 URL，格式需包含 <code>name</code> 和 <code>system_prompt</code> 字段。
        </p>
        <input
          className="skill-installer__input"
          placeholder="https://example.com/skill.json"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void handleInstall()}
          autoFocus
        />
        {error && <div className="skill-installer__error">{error}</div>}
        <button
          className="skill-installer__btn"
          onClick={handleInstall}
          disabled={loading || !url.trim()}
        >
          {loading ? "下载安装中…" : "下载并安装"}
        </button>
      </div>
    </div>
  );
}
