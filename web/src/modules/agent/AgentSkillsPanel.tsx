import { useState, useRef } from "react";
import type { AgentSkillItem } from "../../types";

// ── Types ─────────────────────────────────────────────────────────────────────

type AgentSkillsPanelProps = {
  skills: AgentSkillItem[];
  skillsLoading: boolean;
  activeSkillKey: string;
  setActiveSkillKey: (value: string) => void;
  onUpsertSkill: (key: string, prompt: string) => void | Promise<void>;
  onDeleteSkill: (id: number, skillKey: string) => void | Promise<void>;
};

// ── Presets ───────────────────────────────────────────────────────────────────

const PRESET_SKILLS = [
  {
    key: "写作助手",
    prompt:
      "你是一个专业的知识写作助手。写作时请做到：\n1. 结构清晰，使用标题和要点分层组织\n2. 语言客观严谨，避免口语化表达\n3. 核心观点有逻辑支撑，不做无根据的断言\n4. 专业术语准确，必要时给出简短解释",
  },
  {
    key: "代码审查",
    prompt:
      "你是一个经验丰富的代码审查专家。审查时请关注：\n1. 安全性：是否有注入风险、越权访问、敏感数据泄露\n2. 性能：是否有明显的 N+1、不必要的循环或内存浪费\n3. 可读性：命名是否清晰，逻辑是否直白\n4. 边界条件：是否处理了空值、越界、并发等异常场景\n每个问题请说明严重程度（高/中/低）和改进建议",
  },
  {
    key: "内容摘要",
    prompt:
      "你是一个高效的内容提炼专家。请将输入内容提炼为结构化摘要：\n1. 核心结论（1-3 句话）\n2. 关键要点（不超过 5 条，保留重要数据和数字）\n3. 适用场景或注意事项（如有）\n保持简洁，去掉冗余表达，不添加原文没有的内容",
  },
  {
    key: "翻译优化",
    prompt:
      "你是一个精通中英双语的翻译专家，遵循「信达雅」原则：\n- 信：忠实原文，不遗漏、不歪曲\n- 达：表达流畅自然，符合目标语言习惯\n- 雅：措辞得体，专业术语保持一致\n翻译时保留专有名词和技术术语的原文（首次出现时括号标注），对有歧义的段落在译文后加「译注」说明",
  },
  {
    key: "分析报告",
    prompt:
      "你是一个严谨的分析师。撰写分析报告时请遵循：\n1. 先给出执行摘要（核心结论，3句以内）\n2. 分层展开论据，区分「已知事实」和「推断/假设」\n3. 对关键数据给出来源或依据\n4. 结尾给出明确的行动建议或待确认事项\n避免模糊表述（「可能」「也许」），有不确定时明确说明",
  },
];

// ── Component ─────────────────────────────────────────────────────────────────

export default function AgentSkillsPanel({
  skills,
  skillsLoading,
  activeSkillKey,
  setActiveSkillKey,
  onUpsertSkill,
  onDeleteSkill,
}: AgentSkillsPanelProps) {
  const [panelOpen, setPanelOpen] = useState(false);
  const [modalOpen, setModalOpen] = useState(false);
  const [editingSkill, setEditingSkill] = useState<AgentSkillItem | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [promptInput, setPromptInput] = useState("");
  const [presetOpen, setPresetOpen] = useState(false);
  const presetRef = useRef<HTMLDivElement>(null);

  // Open modal for creating a new skill
  const openNewModal = () => {
    setEditingSkill(null);
    setKeyInput("");
    setPromptInput("");
    setPresetOpen(false);
    setModalOpen(true);
  };

  // Open modal for editing an existing skill
  const openEditModal = (skill: AgentSkillItem) => {
    setEditingSkill(skill);
    setKeyInput(skill.skill_key);
    setPromptInput(skill.prompt_template);
    setPresetOpen(false);
    setModalOpen(true);
  };

  const closeModal = () => {
    setModalOpen(false);
    setPresetOpen(false);
  };

  const handleSave = async () => {
    const key = keyInput.trim();
    const prompt = promptInput.trim();
    if (!key || !prompt) return;
    await onUpsertSkill(key, prompt);
    closeModal();
  };

  const handlePasteFromClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText();
      setPromptInput(text);
    } catch {
      alert("无法读取剪贴板，请确认浏览器权限或手动粘贴。");
    }
  };

  const handleCopyPrompt = async (prompt: string) => {
    try {
      await navigator.clipboard.writeText(prompt);
    } catch {
      // Silent fail — clipboard write is best-effort
    }
  };

  const applyPreset = (preset: { key: string; prompt: string }) => {
    setKeyInput(preset.key);
    setPromptInput(preset.prompt);
    setPresetOpen(false);
  };

  return (
    <section className="agent-studio__context-section">
      {/* Section toggle header */}
      <button
        type="button"
        className="agent-studio__section-toggle"
        onClick={() => setPanelOpen((prev) => !prev)}
      >
        <span>{panelOpen ? "▼" : "▶"} 技能模板</span>
        <span className="agent-studio__section-meta">{skills.length} 个</span>
      </button>

      {panelOpen ? (
        <div className="agent-studio__section-body">
          {/* Toolbar: active select + new button */}
          <div className="skill-panel__toolbar">
            <select
              className="dev-panel__input skill-panel__active-select"
              value={activeSkillKey}
              onChange={(e) => setActiveSkillKey(e.target.value)}
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
            <button
              type="button"
              className="agent-studio__memory-chip-add"
              onClick={openNewModal}
            >
              + 新建
            </button>
          </div>

          {/* Skill list */}
          {skillsLoading ? (
            <p className="agent-studio__empty">技能模板加载中...</p>
          ) : skills.length === 0 ? (
            <p className="agent-studio__empty">
              暂无技能模板，可先创建 writer/reviewer 等角色提示。
            </p>
          ) : (
            <ul className="skill-panel__list">
              {skills.map((skill) => {
                const isActive = activeSkillKey === skill.skill_key;
                return (
                  <li
                    key={skill.id}
                    className={`skill-panel__item${isActive ? " skill-panel__item--active" : ""}`}
                    onClick={() => setActiveSkillKey(isActive ? "" : skill.skill_key)}
                  >
                    <div className="skill-panel__item-meta">
                      <span className="skill-panel__key-badge">{skill.skill_key}</span>
                      <span className="skill-panel__version">v{skill.version}</span>
                      <span className="skill-panel__preview" title={skill.prompt_template}>
                        {skill.prompt_template.length > 80
                          ? `${skill.prompt_template.slice(0, 80)}…`
                          : skill.prompt_template}
                      </span>
                    </div>
                    <div
                      className="skill-panel__item-actions"
                      onClick={(e) => e.stopPropagation()}
                    >
                      <button
                        type="button"
                        className="skill-panel__action-btn"
                        title="复制提示词"
                        onClick={() => void handleCopyPrompt(skill.prompt_template)}
                      >
                        📋
                      </button>
                      <button
                        type="button"
                        className="skill-panel__action-btn"
                        title="编辑"
                        onClick={() => openEditModal(skill)}
                      >
                        ✏️
                      </button>
                      <button
                        type="button"
                        className="skill-panel__action-btn skill-panel__action-btn--danger"
                        title="删除"
                        onClick={() => void onDeleteSkill(skill.id, skill.skill_key)}
                      >
                        🗑️
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      ) : null}

      {/* Edit / Create Modal */}
      {modalOpen ? (
        <div className="skill-panel__modal-overlay" onClick={closeModal}>
          <div
            className="skill-panel__modal"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="skill-panel__modal-title">
              {editingSkill ? `编辑技能：${editingSkill.skill_key}` : "新建技能模板"}
            </h3>

            {/* skill_key input */}
            <label className="skill-panel__label">
              技能键（skill_key）
              <input
                type="text"
                className="dev-panel__input skill-panel__key-input"
                placeholder="例：writer、code-review"
                value={keyInput}
                onChange={(e) => setKeyInput(e.target.value)}
              />
            </label>

            {/* prompt textarea + helpers */}
            <label className="skill-panel__label">
              提示词模板（prompt_template）
              <div className="skill-panel__textarea-wrap">
                <div className="skill-panel__textarea-toolbar">
                  <button
                    type="button"
                    className="dev-panel__button skill-panel__helper-btn"
                    onClick={() => void handlePasteFromClipboard()}
                  >
                    📋 从剪贴板粘贴
                  </button>
                  <div className="skill-panel__preset-dropdown" ref={presetRef}>
                    <button
                      type="button"
                      className="dev-panel__button skill-panel__helper-btn"
                      onClick={() => setPresetOpen((prev) => !prev)}
                    >
                      📖 载入示例 ▼
                    </button>
                    {presetOpen ? (
                      <ul className="skill-panel__preset-menu">
                        {PRESET_SKILLS.map((preset) => (
                          <li key={preset.key}>
                            <button
                              type="button"
                              className="skill-panel__preset-item"
                              onClick={() => applyPreset(preset)}
                            >
                              {preset.key}
                            </button>
                          </li>
                        ))}
                      </ul>
                    ) : null}
                  </div>
                </div>
                <textarea
                  className="skill-panel__textarea"
                  placeholder="输入提示词模板，同名技能键会覆盖并递增版本"
                  value={promptInput}
                  onChange={(e) => setPromptInput(e.target.value)}
                />
              </div>
            </label>

            <div className="skill-panel__modal-actions">
              <button
                type="button"
                className="dev-panel__button"
                onClick={closeModal}
              >
                取消
              </button>
              <button
                type="button"
                className="dev-panel__button dev-panel__button--primary"
                disabled={!keyInput.trim() || !promptInput.trim()}
                onClick={() => void handleSave()}
              >
                保存
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}
