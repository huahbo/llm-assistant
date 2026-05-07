import type { AgentSkillItem } from "../../types";

type AgentSkillsPanelProps = {
  panelOpen: boolean;
  onTogglePanel: () => void;
  skills: AgentSkillItem[];
  skillsLoading: boolean;
  actionRunning: boolean;
  isTauri: boolean;
  activeSkillKey: string;
  setActiveSkillKey: (value: string) => void;
  composerOpen: boolean;
  onToggleComposer: () => void;
  skillKeyInput: string;
  setSkillKeyInput: (value: string) => void;
  skillPromptInput: string;
  setSkillPromptInput: (value: string) => void;
  onUpsertSkill: () => void | Promise<void>;
  onDeleteSkill: (id: number, skillKey: string) => void | Promise<void>;
  formatTimestamp: (value: string) => string;
};

export default function AgentSkillsPanel({
  panelOpen,
  onTogglePanel,
  skills,
  skillsLoading,
  actionRunning,
  isTauri,
  activeSkillKey,
  setActiveSkillKey,
  composerOpen,
  onToggleComposer,
  skillKeyInput,
  setSkillKeyInput,
  skillPromptInput,
  setSkillPromptInput,
  onUpsertSkill,
  onDeleteSkill,
  formatTimestamp,
}: AgentSkillsPanelProps) {
  return (
    <section className="agent-studio__context-section">
      <button
        type="button"
        className="agent-studio__section-toggle"
        onClick={onTogglePanel}
      >
        <span>{panelOpen ? "▼" : "▶"} 技能模板</span>
        <span className="agent-studio__section-meta">
          {skills.length} 个
        </span>
      </button>
      {panelOpen ? (
        <div className="agent-studio__section-body">
          <div className="agent-studio__skills">
            <div className="agent-studio__skills-head">
              <div className="agent-studio__skills-head-main">
                <select
                  className="dev-panel__input agent-studio__skill-active-select"
                  value={activeSkillKey}
                  onChange={(event) => setActiveSkillKey(event.target.value)}
                  disabled={skillsLoading || skills.length === 0}
                  title="选择本次生成生效的技能模板"
                >
                  <option value="">不使用技能模板</option>
                  {skills.map((skill) => (
                    <option key={`active-skill-${skill.id}`} value={skill.skill_key}>
                      {skill.skill_key} (v{skill.version})
                    </option>
                  ))}
                </select>
              </div>
              <button
                type="button"
                className="agent-studio__memory-chip-add"
                disabled={actionRunning || !isTauri}
                onClick={onToggleComposer}
              >
                {composerOpen ? "收起" : "+ 新建"}
              </button>
            </div>
            {composerOpen ? (
              <div className="agent-studio__skill-form">
                <input
                  type="text"
                  className="dev-panel__input"
                  placeholder="技能键（如：writer）"
                  value={skillKeyInput}
                  onChange={(event) => setSkillKeyInput(event.target.value)}
                />
                <textarea
                  className="dev-panel__input"
                  placeholder="模板提示词（将用于后续技能化编排）"
                  value={skillPromptInput}
                  onChange={(event) => setSkillPromptInput(event.target.value)}
                  rows={3}
                />
                <p className="agent-studio__skill-form-hint">
                  同名技能键会覆盖并递增版本；如需并存请使用不同技能键。
                </p>
                <button
                  type="button"
                  className="dev-panel__button"
                  disabled={
                    actionRunning
                    || !skillKeyInput.trim()
                    || !skillPromptInput.trim()
                    || !isTauri
                  }
                  onClick={() => void onUpsertSkill()}
                >
                  保存技能
                </button>
              </div>
            ) : null}
            {skillsLoading ? (
              <p className="agent-studio__empty">技能模板加载中...</p>
            ) : skills.length === 0 ? (
              <p className="agent-studio__empty">暂无技能模板，可先创建 writer/reviewer 等角色提示。</p>
            ) : (
              <>
                <p className="agent-studio__skill-active-hint">
                  当前生效：<strong>{activeSkillKey || "不使用技能模板"}</strong>
                </p>
                <ul className="agent-studio__skill-list">
                  {skills.map((skill) => (
                    <li
                      key={skill.id}
                      className={`agent-studio__skill-item${activeSkillKey === skill.skill_key ? " agent-studio__skill-item--active" : ""}`}
                    >
                      <div className="agent-studio__skill-main">
                        <button
                          type="button"
                          className="agent-studio__skill-select"
                          onClick={() => setActiveSkillKey(skill.skill_key)}
                        >
                          <strong>{skill.skill_key}</strong>
                          {activeSkillKey === skill.skill_key ? (
                            <span className="agent-studio__skill-badge">生效中</span>
                          ) : null}
                        </button>
                        <span>v{skill.version}</span>
                      </div>
                      <p title={skill.prompt_template}>{skill.prompt_template}</p>
                      <div className="agent-studio__skill-actions">
                        <time dateTime={skill.updated_at}>
                          {formatTimestamp(skill.updated_at)}
                        </time>
                        <button
                          type="button"
                          className="dev-panel__button"
                          disabled={actionRunning || !isTauri}
                          onClick={() => void onDeleteSkill(skill.id, skill.skill_key)}
                        >
                          删除
                        </button>
                      </div>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        </div>
      ) : null}
    </section>
  );
}
