import { useEffect, useState } from "react";
import type { AgentSkillItem } from "../../types";
import { listAgentSkills, deleteAgentSkill, installSkillFromUrl } from "../../tauri-client";

interface Props {
  onClose: () => void;
  onInstalled: (name: string) => void;
}

export default function SkillInstaller({ onClose, onInstalled }: Props) {
  const [skills, setSkills] = useState<AgentSkillItem[]>([]);
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const loadSkills = () => void listAgentSkills().then(setSkills);

  useEffect(() => { loadSkills(); }, []);

  const handleDelete = async (skill: AgentSkillItem) => {
    await deleteAgentSkill(skill.id);
    loadSkills();
  };

  const handleInstall = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setLoading(true);
    setError("");
    try {
      await installSkillFromUrl(trimmed);
      const parts = trimmed.split("/");
      const guessName = parts[parts.length - 1].replace(/\.json$/, "") || "技能";
      setUrl("");
      loadSkills();
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
        <span className="skill-installer__title">管理 Skill</span>
        <button className="skill-installer__close" onClick={onClose}>✕</button>
      </div>

      <div className="skill-installer__body">
        {skills.length > 0 && (
          <div className="skill-installer__list">
            {skills.map((s) => (
              <div key={s.id} className="skill-installer__list-item">
                <span className="skill-installer__list-name">{s.skill_key}</span>
                <button
                  className="skill-installer__list-del"
                  onClick={() => void handleDelete(s)}
                  title="删除"
                >✕</button>
              </div>
            ))}
            <div className="skill-installer__divider" />
          </div>
        )}

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
