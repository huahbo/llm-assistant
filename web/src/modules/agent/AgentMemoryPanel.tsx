import type { AgentMemoryItem } from "../../types";

type AgentMemoryPanelProps = {
  panelOpen: boolean;
  onTogglePanel: () => void;
  memories: AgentMemoryItem[];
  memoriesLoading: boolean;
  actionRunning: boolean;
  isTauri: boolean;
  onDeleteMemory: (id: number) => void | Promise<void>;
  composerOpen: boolean;
  onToggleComposer: () => void;
  memoryKeyInput: string;
  setMemoryKeyInput: (value: string) => void;
  memoryValueInput: string;
  setMemoryValueInput: (value: string) => void;
  onUpsertMemory: () => void | Promise<void>;
};

export default function AgentMemoryPanel({
  panelOpen,
  onTogglePanel,
  memories,
  memoriesLoading,
  actionRunning,
  isTauri,
  onDeleteMemory,
  composerOpen,
  onToggleComposer,
  memoryKeyInput,
  setMemoryKeyInput,
  memoryValueInput,
  setMemoryValueInput,
  onUpsertMemory,
}: AgentMemoryPanelProps) {
  return (
    <section className="agent-studio__context-section">
      <button
        type="button"
        className="agent-studio__section-toggle"
        onClick={onTogglePanel}
      >
        <span>{panelOpen ? "▼" : "▶"} 记忆上下文</span>
        <span className="agent-studio__section-meta">{memories.length} 条</span>
      </button>
      {panelOpen ? (
        <div className="agent-studio__section-body">
          <div className="agent-studio__memory-chipbar">
            <div className="agent-studio__memory-chipbar-list">
              {memoriesLoading ? (
                <span className="agent-studio__memory-chip-placeholder">加载中...</span>
              ) : memories.length === 0 ? (
                <span className="agent-studio__memory-chip-placeholder">暂无记忆</span>
              ) : (
                memories.map((mem) => (
                  <span
                    key={mem.id}
                    className="agent-studio__memory-chip"
                    title={`${mem.memory_key}: ${mem.memory_value}`}
                  >
                    <strong>{mem.memory_key}</strong>
                    <span>{mem.memory_value}</span>
                    <button
                      type="button"
                      className="agent-studio__memory-chip-remove"
                      disabled={actionRunning || !isTauri}
                      onClick={() => void onDeleteMemory(mem.id)}
                      aria-label={`删除记忆 ${mem.memory_key}`}
                    >
                      ×
                    </button>
                  </span>
                ))
              )}
              <button
                type="button"
                className="agent-studio__memory-chip-add"
                disabled={actionRunning || !isTauri}
                onClick={onToggleComposer}
              >
                {composerOpen ? "收起" : "+ 添加"}
              </button>
            </div>
          </div>
          {composerOpen ? (
            <div className="agent-studio__memory-inline-form">
              <div className="agent-studio__memory-inline-form-row">
                <input
                  type="text"
                  className="dev-panel__input"
                  placeholder="键（可选）"
                  value={memoryKeyInput}
                  onChange={(event) => setMemoryKeyInput(event.target.value)}
                />
                <input
                  type="text"
                  className="dev-panel__input"
                  placeholder="记忆内容"
                  value={memoryValueInput}
                  onChange={(event) => setMemoryValueInput(event.target.value)}
                />
              </div>
              <button
                type="button"
                className="dev-panel__button"
                disabled={actionRunning || !memoryValueInput.trim() || !isTauri}
                onClick={() => void onUpsertMemory()}
              >
                保存记忆
              </button>
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
