import type { AgentRunItem } from "../../types";

type AgentRunHistoryProps = {
  runStripOpen: boolean;
  setRunStripOpen: (updater: (prev: boolean) => boolean) => void;
  runManageMode: boolean;
  setRunManageMode: (value: boolean) => void;
  runCards: AgentRunItem[];
  visibleRunCards: AgentRunItem[];
  archivedRunCount: number;
  runsLoading: boolean;
  selectedRunId: number | null;
  runMutatingId: number | null;
  onSelectRun: (runId: number) => void;
  onArchiveRun: (runId: number) => void;
  onRestoreRun: (runId: number) => void;
  formatTime: (value: string) => string;
  formatStatusLabel: (status: string) => string;
  getStatusTone: (status: string) => "running" | "reviewing" | "applied" | "failed" | "queued" | "unknown";
};

export default function AgentRunHistory({
  runStripOpen,
  setRunStripOpen,
  runManageMode,
  setRunManageMode,
  runCards,
  visibleRunCards,
  archivedRunCount,
  runsLoading,
  selectedRunId,
  runMutatingId,
  onSelectRun,
  onArchiveRun,
  onRestoreRun,
  formatTime,
  formatStatusLabel,
  getStatusTone,
}: AgentRunHistoryProps) {
  return (
    <div className={`agent-studio__run-strip${runStripOpen ? " agent-studio__run-strip--open" : ""}`}>
      <button
        type="button"
        className="agent-studio__context-toggle agent-studio__run-strip-toggle"
        onClick={() => setRunStripOpen((prev) => !prev)}
      >
        <span>{runStripOpen ? "▼" : "▶"} 历史 Runs</span>
        <span className="agent-studio__context-meta">
          {runManageMode ? runCards.length : visibleRunCards.length} 条
        </span>
      </button>
      {runStripOpen ? (
        <div className="agent-studio__run-strip-body">
          <label className="agent-studio__run-strip-manage">
            <input
              type="checkbox"
              checked={runManageMode}
              onChange={(event) => setRunManageMode(event.target.checked)}
            />
            管理模式（显示已归档）
          </label>
          <p className="agent-studio__run-strip-note">
            {runManageMode
              ? `当前显示全部 run（含已归档 ${archivedRunCount} 条）`
              : `默认隐藏已归档 run（已归档 ${archivedRunCount} 条）`}
          </p>
          {runsLoading ? (
            <p className="agent-studio__run-strip-empty">正在加载...</p>
          ) : visibleRunCards.length === 0 ? (
            <p className="agent-studio__run-strip-empty">暂无历史 run</p>
          ) : (
            <div className="agent-studio__run-strip-list">
              {visibleRunCards.map((run) => {
                const active = run.id === selectedRunId;
                const statusTone = getStatusTone(String(run.status || ""));
                const topic = run.topic?.trim() || `Run #${run.id}`;
                return (
                  <div
                    key={`run-card-${run.id}`}
                    className={`agent-studio__run-card${active ? " agent-studio__run-card--active" : ""}`}
                  >
                    <button
                      type="button"
                      className="agent-studio__run-card-main"
                      onClick={() => onSelectRun(run.id)}
                    >
                      <span className="agent-studio__run-card-title" title={topic}>
                        #{run.id} {topic}
                      </span>
                      <span className={`agent-studio__run-card-status agent-studio__run-card-status--${statusTone}`}>
                        {formatStatusLabel(String(run.status || ""))}
                      </span>
                      {run.archived_at ? (
                        <span className="agent-studio__run-card-archived">已归档</span>
                      ) : null}
                      <time dateTime={run.updated_at || run.created_at}>
                        {formatTime(run.updated_at || run.created_at)}
                      </time>
                    </button>
                    <div className="agent-studio__run-card-actions">
                      {runManageMode ? (
                        run.archived_at ? (
                          <button
                            type="button"
                            className="dev-panel__button"
                            disabled={runMutatingId != null}
                            onClick={() => {
                              onRestoreRun(run.id);
                            }}
                          >
                            恢复
                          </button>
                        ) : (
                          <button
                            type="button"
                            className="dev-panel__button"
                            disabled={runMutatingId != null}
                            onClick={() => {
                              onArchiveRun(run.id);
                            }}
                          >
                            归档
                          </button>
                        )
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}
