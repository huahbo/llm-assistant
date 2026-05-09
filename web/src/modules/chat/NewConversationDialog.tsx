import { useEffect, useState } from "react";
import type { AgentSkillItem } from "../../types";
import { listAgentSkills, listAgentMemories } from "../../tauri-client";

interface Props {
  onConfirm: (title: string, skillKey?: string, injectMemories?: boolean) => void;
  onCancel: () => void;
}

export default function NewConversationDialog({ onConfirm, onCancel }: Props) {
  const [title, setTitle] = useState("新对话");
  const [skills, setSkills] = useState<AgentSkillItem[]>([]);
  const [selectedSkill, setSelectedSkill] = useState("");
  const [injectMemories, setInjectMemories] = useState(false);
  const [memoriesCount, setMemoriesCount] = useState(0);

  useEffect(() => {
    void listAgentSkills().then(setSkills);
    void listAgentMemories(null).then((m) => setMemoriesCount(m.length));
  }, []);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const t = title.trim();
    if (!t) return;
    onConfirm(t, selectedSkill || undefined, injectMemories || undefined);
  };

  return (
    <div className="chat-dialog-overlay" onClick={onCancel}>
      <div className="chat-dialog" onClick={(e) => e.stopPropagation()}>
        <h3 className="chat-dialog__title">新建对话</h3>
        <form onSubmit={handleSubmit}>
          <div className="chat-dialog__field">
            <label className="chat-dialog__label">名称</label>
            <input
              className="chat-dialog__input"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              autoFocus
            />
          </div>
          {skills.length > 0 && (
            <div className="chat-dialog__field">
              <label className="chat-dialog__label">Skill 模板</label>
              <select
                className="chat-dialog__input"
                value={selectedSkill}
                onChange={(e) => setSelectedSkill(e.target.value)}
              >
                <option value="">— 无 —</option>
                {skills.map((s) => (
                  <option key={s.skill_key} value={s.skill_key}>
                    {s.skill_key}
                  </option>
                ))}
              </select>
            </div>
          )}
          {memoriesCount > 0 && (
            <div className="chat-dialog__field">
              <label className="chat-dialog__checkbox-label">
                <input
                  type="checkbox"
                  checked={injectMemories}
                  onChange={(e) => setInjectMemories(e.target.checked)}
                />
                注入当前记忆（{memoriesCount} 条）
              </label>
            </div>
          )}
          <div className="chat-dialog__actions">
            <button type="button" className="chat-dialog__btn chat-dialog__btn--cancel" onClick={onCancel}>
              取消
            </button>
            <button type="submit" className="chat-dialog__btn chat-dialog__btn--ok">
              创建
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
