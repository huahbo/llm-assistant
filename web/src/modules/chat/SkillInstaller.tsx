import { useEffect, useState } from "react";
import type { AgentSkillItem } from "../../types";
import { listAgentSkills, deleteAgentSkill, installSkillFromUrl, upsertAgentSkill } from "../../tauri-client";

type Tab = "url" | "paste";

interface Props {
  onClose: () => void;
  onInstalled: (name: string) => void;
}

export default function SkillInstaller({ onClose, onInstalled }: Props) {
  const [skills, setSkills] = useState<AgentSkillItem[]>([]);
  const [tab, setTab] = useState<Tab>("url");
  const [url, setUrl] = useState("");
  const [json, setJson] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const loadSkills = () => void listAgentSkills().then(setSkills);
  useEffect(() => { loadSkills(); }, []);

  const handleDelete = async (skill: AgentSkillItem) => {
    await deleteAgentSkill(skill.id);
    loadSkills();
  };

  const handleInstallUrl = async () => {
    const trimmed = url.trim();
    if (!trimmed) return;
    setLoading(true);
    setError("");
    try {
      await installSkillFromUrl(trimmed);
      const parts = trimmed.split("/");
      const name = parts[parts.length - 1].replace(/\.json$/, "") || "技能";
      setUrl("");
      loadSkills();
      onInstalled(name);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleInstallJson = async () => {
    const trimmed = json.trim();
    if (!trimmed) return;
    setLoading(true);
    setError("");
    try {
      let parsed: Record<string, string>;
      try {
        parsed = JSON.parse(trimmed) as Record<string, string>;
      } catch {
        throw new Error("JSON 格式有误，请检查括号和引号");
      }
      const name = (parsed.name ?? parsed.skill_key ?? "").trim();
      const prompt = (parsed.system_prompt ?? parsed.prompt_template ?? "").trim();
      if (!name) throw new Error("缺少 name 字段");
      if (!prompt) throw new Error("缺少 system_prompt 字段");
      await upsertAgentSkill(name, prompt);
      setJson("");
      loadSkills();
      onInstalled(name);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  const canSubmit = tab === "url" ? !!url.trim() : !!json.trim();
  const handleSubmit = tab === "url" ? handleInstallUrl : handleInstallJson;

  return (
    <div className="skill-installer">
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

        <div className="skill-installer__tabs">
          <button
            className={`skill-installer__tab${tab === "url" ? " skill-installer__tab--active" : ""}`}
            onClick={() => { setTab("url"); setError(""); }}
          >从 URL 安装</button>
          <button
            className={`skill-installer__tab${tab === "paste" ? " skill-installer__tab--active" : ""}`}
            onClick={() => { setTab("paste"); setError(""); }}
          >粘贴 JSON</button>
        </div>

        {tab === "url" ? (
          <>
            <p className="skill-installer__hint">
              粘贴 Skill JSON 文件的 URL，格式需包含 <code>name</code> 和 <code>system_prompt</code> 字段。
            </p>
            <input
              className="skill-installer__input"
              placeholder="https://example.com/skill.json"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void handleInstallUrl()}
              autoFocus
            />
          </>
        ) : (
          <>
            <p className="skill-installer__hint">
              直接粘贴 JSON，需包含 <code>name</code> 和 <code>system_prompt</code> 字段。
            </p>
            <textarea
              className="skill-installer__input skill-installer__textarea"
              placeholder={'{\n  "name": "我的技能",\n  "system_prompt": "你是..."\n}'}
              value={json}
              onChange={(e) => setJson(e.target.value)}
              rows={5}
              autoFocus
            />
          </>
        )}

        {error && <div className="skill-installer__error">{error}</div>}
        <button
          className="skill-installer__btn"
          onClick={handleSubmit}
          disabled={loading || !canSubmit}
        >
          {loading ? "处理中…" : tab === "url" ? "下载并安装" : "添加技能"}
        </button>
      </div>
    </div>
  );
}
