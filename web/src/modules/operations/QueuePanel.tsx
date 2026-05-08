import { useEffect } from "react";
import type { IngestQueueItem } from "../../types";

const queueStatusBadgeClass: Record<string, string> = {
  queued: "queue-badge queue-badge--queued",
  running: "queue-badge queue-badge--running",
  done: "queue-badge queue-badge--done",
  failed: "queue-badge queue-badge--failed",
  cancelled: "queue-badge queue-badge--cancelled",
};

const queueStatusLabel: Record<string, string> = {
  queued: "等待",
  running: "运行中",
  done: "完成",
  failed: "失败",
  cancelled: "已取消",
};

export default function QueuePanel({
  queue,
  onRefresh,
  onCancel,
  onRetry,
  onDelete,
}: {
  queue: IngestQueueItem[];
  onRefresh: () => void;
  onCancel: (id: number) => void;
  onRetry: (id: number) => void;
  onDelete: (id: number) => void;
}) {
  useEffect(() => {
    onRefresh();
    const timer = setInterval(() => {
      onRefresh();
    }, 3000);
    return () => clearInterval(timer);
  }, [onRefresh]);

  return (
    <section className="queue-panel">
      <div className="section-head">
        <h2>任务列表</h2>
        <span className="section-head__hint">每 3 秒自动刷新 · {queue.length} 条任务</span>
      </div>
      {queue.length === 0 ? (
        <p className="empty-state operations-empty-state">
          <strong>暂无摄入任务</strong>
          <span>可在 Wiki 的 Inbox 添加文件，或通过 Agent / 研究流程触发摄入。</span>
        </p>
      ) : (
        <div className="queue-list">
          {queue.map((item) => (
            <div key={item.id} className="queue-item">
              <div className="queue-item__main">
                <span className={queueStatusBadgeClass[item.status] ?? "queue-badge"}>
                  {queueStatusLabel[item.status] ?? item.status}
                </span>
                <span className="queue-item__path" title={item.source_path}>
                  {item.source_path.split(/[/\\]/).pop() ?? item.source_path}
                </span>
                <span className="queue-item__type">{item.source_type}</span>
                <time className="queue-item__time" dateTime={item.updated_at}>
                  {item.updated_at.slice(0, 16).replace("T", " ")}
                </time>
              </div>
              {item.status === "failed" && item.error && (
                <p className="queue-item__error">{item.error}</p>
              )}
              <div className="queue-item__actions">
                {(item.status === "queued" || item.status === "running") && (
                  <button
                    type="button"
                    className="dev-panel__button"
                    onClick={() => onCancel(item.id)}
                  >
                    取消
                  </button>
                )}
                {item.status === "failed" && (
                  <>
                    <button
                      type="button"
                      className="dev-panel__button dev-panel__button--accent"
                      onClick={() => onRetry(item.id)}
                    >
                      重试
                    </button>
                    <button
                      type="button"
                      className="dev-panel__button"
                      onClick={() => onDelete(item.id)}
                      title="永久删除该记录"
                    >
                      删除
                    </button>
                  </>
                )}
                {(item.status === "done" || item.status === "cancelled") && (
                  <button
                    type="button"
                    className="dev-panel__button"
                    onClick={() => onDelete(item.id)}
                    title="从列表中移除"
                  >
                    移除
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
