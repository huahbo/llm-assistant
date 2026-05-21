import { useCallback, useEffect, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import "./research.css";
import {
  askConfirmDialog,
  cancelResearchTask,
  deleteResearchTask,
  fetchWikiPageDetail,
  getSearchConfig,
  isTauriRuntime,
  listenResearchDone,
  listenResearchError,
  listenResearchProgress,
  listResearchTasks,
  pickSaveFile,
  saveResearchDoc,
  startResearch,
} from "../../tauri-client";
import type { ResearchTaskItem, ResearchTaskStatus } from "../../types";
import { formatLintCheckedAt } from "../../lint-utils";
import ResearchDialog from "./ResearchDialog";
import { exportResearchHtml } from "../../tauri-client/export";

type DeleteModalOutcome = "cancel" | "task-only" | "task-and-wiki";
type DeleteModalState = {
  latestTask: ResearchTaskItem;
  hasSavedWiki: boolean;
  resolve: (outcome: DeleteModalOutcome) => void;
};

export const getResearchStatusLabel = (status: ResearchTaskStatus): string => {
  const labels: Record<ResearchTaskStatus, string> = {
    queued:                   "等待",
    decomposing:              "分解中",
    planning_outline:         "规划大纲",
    awaiting_outline_approval:"等待大纲确认",
    searching:                "搜索中",
    synthesizing:             "合成中",
    writing_section:          "章节写作",
    assembling:               "组装报告",
    saving:                   "保存中",
    awaiting_save:            "⏸ 等待保存",
    done:                     "✓ 已保存",
    failed:                   "✕ 失败",
    cancelled:                "已取消",
    discarded:                "已丢弃",
  };
  return labels[status] ?? status;
};

export const getResearchStatusColor = (status: ResearchTaskStatus): string => {
  switch (status) {
    case "done":
      return "research-badge--done";
    case "failed":
      return "research-badge--failed";
    case "cancelled":
      return "research-badge--cancelled";
    case "queued":
      return "research-badge--queued";
    default:
      return "research-badge--running";
  }
};

export default function ResearchPanel({ onOpenWikiPage }: { onOpenWikiPage: (path: string) => void }) {
  const [topic, setTopic] = useState("");
  const [depth, setDepth] = useState(1);
  const [breadth, setBreadth] = useState(3);

  const [researchTasks, setResearchTasks] = useState<ResearchTaskItem[]>([]);
  const [taskLogs, setTaskLogs] = useState<Record<number, string[]>>({});
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [taskActionError, setTaskActionError] = useState<string | null>(null);
  const [deleteModal, setDeleteModal] = useState<DeleteModalState | null>(null);
  const [dialogTask, setDialogTask] = useState<{ taskId: number; topic: string; depth: number; breadth: number; initialTask?: ResearchTaskItem } | null>(null);
  const [hasSearchProvider, setHasSearchProvider] = useState(true);

  const refreshTasks = useCallback(async () => {
    try {
      const tasks = await listResearchTasks();
      setResearchTasks(tasks);
    } catch {
      // 静默忽略，保持当前列表
    }
  }, []);

  useEffect(() => {
    void refreshTasks();
    getSearchConfig()
      .then((cfg) => {
        const hasMulti = Array.isArray(cfg.search_providers) && cfg.search_providers.length > 0;
        const hasSingle = cfg.search_provider !== "none";
        setHasSearchProvider(hasMulti || hasSingle);
        // 同步默认 depth / breadth（SearchConfigPanel 中的"默认研究深度/默认搜索广度"）
        if (typeof cfg.depth === "number" && cfg.depth >= 1 && cfg.depth <= 5) {
          setDepth(cfg.depth);
        }
        if (typeof cfg.breadth === "number" && cfg.breadth >= 2 && cfg.breadth <= 5) {
          setBreadth(cfg.breadth);
        }
      })
      .catch(() => {});
  }, [refreshTasks]);

  useEffect(() => {
    let unlistenProgress: (() => void) | null = null;
    let unlistenDone: (() => void) | null = null;
    let unlistenError: (() => void) | null = null;

    listenResearchProgress((payload) => {
      setTaskLogs((prev) => {
        const logs = prev[payload.task_id] || [];
        if (logs[logs.length - 1] === payload.message) return prev;
        return { ...prev, [payload.task_id]: [...logs, payload.message].slice(-10) };
      });
      setResearchTasks((prev) =>
        prev.map((t) =>
          t.id === payload.task_id ? { ...t, status: payload.stage as ResearchTaskStatus } : t,
        ),
      );
    })
      .then((fn) => { unlistenProgress = fn; })
      .catch(() => {});

    listenResearchDone((payload) => {
      setTaskLogs((prev) => {
        const logs = prev[payload.task_id] || [];
        const message = `✓ 研究完成：${payload.saved_path}`;
        if (logs[logs.length - 1] === message) return prev;
        return { ...prev, [payload.task_id]: [...logs, message].slice(-20) };
      });
      void refreshTasks();
    })
      .then((fn) => { unlistenDone = fn; })
      .catch(() => {});

    listenResearchError((payload) => {
      setTaskLogs((prev) => {
        const logs = prev[payload.task_id] || [];
        const message = `✗ ${payload.error}`;
        if (logs[logs.length - 1] === message) return prev;
        return { ...prev, [payload.task_id]: [...logs, message].slice(-20) };
      });
      void refreshTasks();
    })
      .then((fn) => { unlistenError = fn; })
      .catch(() => {});

    return () => {
      unlistenProgress?.();
      unlistenDone?.();
      unlistenError?.();
    };
  }, [refreshTasks]);

  const handleStartResearch = async () => {
    const trimmed = topic.trim();
    if (!trimmed) return;
    if (!hasSearchProvider) {
      setStartError("请先在「搜索设置」中配置搜索提供商（Tavily 或 SearXNG）");
      return;
    }
    setStartError(null);
    setStarting(true);
    try {
      const taskId = await startResearch(trimmed, depth, breadth);
      const optimisticTask: ResearchTaskItem = {
        id: taskId,
        topic: trimmed,
        status: "queued",
        sub_queries: [],
        web_results_count: 0,
        depth,
        breadth,
        saved_path: null,
        error: null,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      setResearchTasks((prev) => [optimisticTask, ...prev]);
      setDialogTask({ taskId, topic: trimmed, depth, breadth });
      setTopic("");
    } catch (err) {
      setStartError(
        err instanceof Error ? err.message :
        typeof err === "string" ? err :
        "启动研究任务失败，请重试"
      );
    } finally {
      setStarting(false);
    }
  };

  const handleCancel = (id: number) => {
    cancelResearchTask(id)
      .then(() => refreshTasks())
      .catch(() => {});
  };

  const handleRetryTask = async (task: ResearchTaskItem) => {
    if (!hasSearchProvider) {
      setTaskActionError("请先在「搜索设置」中配置搜索提供商");
      return;
    }
    setTaskActionError(null);
    try {
      const taskId = await startResearch(task.topic, task.depth ?? 1, task.breadth ?? 3);
      const optimisticTask: ResearchTaskItem = {
        id: taskId,
        topic: task.topic,
        status: "queued",
        sub_queries: [],
        web_results_count: 0,
        depth: task.depth ?? 1,
        breadth: task.breadth ?? 3,
        saved_path: null,
        error: null,
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      setResearchTasks((prev) => [optimisticTask, ...prev]);
    } catch (err) {
      setTaskActionError(
        err instanceof Error ? err.message :
        typeof err === "string" ? err :
        "重试失败，请稍后再试"
      );
    }
  };

  const showDeleteModal = (latestTask: ResearchTaskItem, hasSavedWiki: boolean): Promise<DeleteModalOutcome> =>
    new Promise((resolve) => setDeleteModal({ latestTask, hasSavedWiki, resolve }));

  const handleDeleteTask = async (task: ResearchTaskItem) => {
    if (!isTauriRuntime()) return;

    setTaskActionError(null);

    const confirmedDeleteTask = await askConfirmDialog(
      `确认删除研究任务「${task.topic}」吗？\n此操作会删除任务记录与该任务日志。`,
      { title: "删除研究任务", kind: "warning", okLabel: "删除", cancelLabel: "取消" },
    );
    if (!confirmedDeleteTask) return;

    let latestTask = task;
    try {
      const latestTasks = await listResearchTasks();
      const matched = latestTasks.find((item) => item.id === task.id);
      if (matched) latestTask = matched;
    } catch {
      // fall through with snapshot
    }

    const savedPath = (latestTask.saved_path ?? "").trim();

    const outcome = await showDeleteModal(latestTask, !!savedPath);
    setDeleteModal(null);
    if (outcome === "cancel") return;

    const deleteSavedWiki = outcome === "task-and-wiki";

    try {
      await deleteResearchTask(latestTask.id, deleteSavedWiki);
      setTaskLogs((prev) => {
        const next = { ...prev };
        delete next[latestTask.id];
        return next;
      });
      await refreshTasks();
    } catch (err) {
      if (err instanceof Error) {
        setTaskActionError(err.message);
      } else if (typeof err === "string") {
        setTaskActionError(err);
      } else {
        setTaskActionError("删除任务失败");
      }
    }
  };

  const handleDownloadWord = async (task: ResearchTaskItem) => {
    if (!task.saved_path) return;
    try {
      setDownloadError(null);
      const detail = await fetchWikiPageDetail(task.saved_path);
      if (!detail) return;

      const htmlContent = await marked.parse(detail.content);
      const sanitizedHtml = DOMPurify.sanitize(htmlContent);

      const documentHtml = `
        <html xmlns:o='urn:schemas-microsoft-com:office:office' xmlns:w='urn:schemas-microsoft-com:office:word' xmlns='http://www.w3.org/TR/REC-html40'>
        <head><meta charset='utf-8'><title>${detail.title}</title></head>
        <body>${sanitizedHtml}</body>
        </html>
      `;

      const safeTitle =
        (detail.title || "research-result")
          .replace(/[\\/:*?"<>|]/g, "-")
          .trim() || "research-result";
      const defaultFileName = `${safeTitle}.doc`;

      if (isTauriRuntime()) {
        const targetPath = await pickSaveFile({
          defaultPath: defaultFileName,
          filters: [{ name: "Word Document", extensions: ["doc"] }],
        });
        if (!targetPath) {
          return;
        }
        await saveResearchDoc(targetPath, documentHtml);
        return;
      }

      const blob = new Blob([documentHtml], { type: "application/msword" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = defaultFileName;
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
    } catch (err) {
      if (err instanceof Error) {
        setDownloadError(err.message);
      } else if (typeof err === "string") {
        setDownloadError(err);
      } else {
        try {
          setDownloadError(JSON.stringify(err));
        } catch {
          setDownloadError("Word 文件下载失败");
        }
      }
    }
  };

  const runningCount = researchTasks.filter(
    (t) => !["done", "failed", "cancelled"].includes(t.status),
  ).length;
  const doneCount = researchTasks.filter((t) =>
    ["done", "failed", "cancelled"].includes(t.status),
  ).length;

  return (
    <>
      {/* 研究对话框 */}
      {dialogTask && (
        <ResearchDialog
          taskId={dialogTask.taskId}
          topic={dialogTask.topic}
          depth={dialogTask.depth}
          breadth={dialogTask.breadth}
          initialTask={dialogTask.initialTask}
          onClose={() => { setDialogTask(null); void refreshTasks(); }}
          onRetry={async () => {
            const { topic: t, depth: d, breadth: b } = dialogTask;
            setDialogTask(null);
            try {
              const newId = await startResearch(t, d, b);
              setResearchTasks((prev) => [{
                id: newId, topic: t, status: "queued", sub_queries: [],
                web_results_count: 0, depth: d, breadth: b,
                saved_path: null, error: null,
                created_at: new Date().toISOString(), updated_at: new Date().toISOString(),
              }, ...prev]);
              setDialogTask({ taskId: newId, topic: t, depth: d, breadth: b });
            } catch { /* silent */ }
          }}
          onOpenWikiPage={onOpenWikiPage}
        />
      )}

      {/* 删除任务确认 Modal（支持三态：取消 / 仅删任务 / 删任务+Wiki） */}
      {deleteModal && (
        <div
          role="dialog"
          aria-modal="true"
          style={{
            position: "fixed", inset: 0, zIndex: 9999,
            background: "rgba(0,0,0,0.55)",
            display: "flex", alignItems: "center", justifyContent: "center",
          }}
          onClick={() => { deleteModal.resolve("cancel"); setDeleteModal(null); }}
        >
          <div
            style={{
              background: "var(--bg-content, #1e1e2e)",
              border: "1.5px solid var(--border, #333)",
              borderRadius: "10px",
              padding: "24px",
              minWidth: "340px",
              maxWidth: "420px",
              boxShadow: "0 8px 32px rgba(0,0,0,0.5)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: "16px" }}>
              <span style={{ fontWeight: 700, fontSize: "15px" }}>删除研究任务</span>
              <button
                type="button"
                aria-label="取消"
                onClick={() => { deleteModal.resolve("cancel"); setDeleteModal(null); }}
                style={{ background: "none", border: "none", cursor: "pointer", fontSize: "18px", color: "var(--text-secondary)", lineHeight: 1, padding: "0 2px" }}
              >
                ✕
              </button>
            </div>

            <p style={{ fontSize: "13px", color: "var(--text-secondary)", marginBottom: "12px", lineHeight: 1.6 }}>
              确认删除任务「<strong style={{ color: "var(--text)" }}>{deleteModal.latestTask.topic}</strong>」吗？此操作不可撤销。
            </p>

            {deleteModal.hasSavedWiki ? (
              <p style={{ fontSize: "12px", color: "var(--text-secondary)", background: "rgba(255,200,100,0.1)", border: "1px solid rgba(255,200,100,0.3)", borderRadius: "6px", padding: "8px 10px", marginBottom: "20px" }}>
                ⚠️ 该任务已关联 Wiki 页面：<br />
                <code style={{ wordBreak: "break-all", fontSize: "11px" }}>{deleteModal.latestTask.saved_path}</code>
              </p>
            ) : (
              <p style={{ fontSize: "12px", color: "var(--text-secondary)", marginBottom: "20px" }}>
                仅删除任务记录，不影响任何 Wiki 页面。
              </p>
            )}

            <div style={{ display: "flex", gap: "8px", justifyContent: "flex-end" }}>
              <button
                type="button"
                className="dev-panel__button"
                onClick={() => { deleteModal.resolve("cancel"); setDeleteModal(null); }}
              >
                取消
              </button>
              <button
                type="button"
                className="dev-panel__button"
                onClick={() => { deleteModal.resolve("task-only"); setDeleteModal(null); }}
              >
                仅删任务
              </button>
              {deleteModal.hasSavedWiki && (
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--danger"
                  onClick={() => { deleteModal.resolve("task-and-wiki"); setDeleteModal(null); }}
                >
                  删除任务 + Wiki
                </button>
              )}
            </div>
          </div>
        </div>
      )}

      <div className="module-header">
        <h1 className="module-header__title">Deep Research</h1>
        <p className="module-header__sub">自动分解主题、搜索互联网并合成 Wiki 页面</p>
      </div>

      {/* 横幅提示 */}
      {!hasSearchProvider && (
        <div className="research-banner research-banner--warn">
          ⚠ 尚未配置搜索提供商，请在「设置 → 搜索」中填写 Tavily API Key 或 SearXNG 地址。
        </div>
      )}
      {startError && (
        <div className="research-banner research-banner--error">✕ {startError}</div>
      )}
      {downloadError && (
        <div className="research-banner research-banner--error">✕ 导出失败：{downloadError}</div>
      )}
      {taskActionError && (
        <div className="research-banner research-banner--error">✕ 任务操作失败：{taskActionError}</div>
      )}

      {/* 输入区 */}
      <section className="panel">
        <div className="research-composer">
          <input
            className="dev-panel__input"
            type="text"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            placeholder="输入研究主题，如：大模型 RAG 架构演进"
            onKeyDown={(e) => {
              if (e.key === "Enter" && !starting) void handleStartResearch();
            }}
            spellCheck={false}
          />
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={() => void handleStartResearch()}
            disabled={starting || !topic.trim()}
          >
            {starting ? "启动中..." : "开始研究"}
          </button>
        </div>
        <div className="research-options">
          <div className="research-option-group">
            <span className="research-option-group__label">深度</span>
            <div className="research-option-pills">
              {([{ v: 1, l: "标准" }, { v: 2, l: "进阶" }, { v: 3, l: "深度" }, { v: 4, l: "极深" }, { v: 5, l: "极限" }] as const).map(({ v, l }) => (
                <button
                  key={v}
                  type="button"
                  className={`research-option-pill${depth === v ? " research-option-pill--active" : ""}`}
                  onClick={() => setDepth(v)}
                >{l}</button>
              ))}
            </div>
          </div>
          <div className="research-option-group">
            <span className="research-option-group__label">广度</span>
            <div className="research-option-pills">
              {[2, 3, 4, 5].map((v) => (
                <button
                  key={v}
                  type="button"
                  className={`research-option-pill${breadth === v ? " research-option-pill--active" : ""}`}
                  onClick={() => setBreadth(v)}
                >{v}</button>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* 任务列表 */}
      <section className="panel research-tasks-panel">
        <div className="section-head">
          <h2>任务历史</h2>
          <span className="section-head__hint">
            运行中 {runningCount} · 历史 {doneCount}
          </span>
        </div>
        {researchTasks.length === 0 ? (
          <p className="empty-state">暂无研究任务，输入主题后点击「开始研究」。</p>
        ) : (
          <div className="research-task-list">
            {researchTasks.map((task) => {
              const RUNNING_STATES: ResearchTaskStatus[] = [
                "queued", "decomposing", "planning_outline", "awaiting_outline_approval",
                "searching", "synthesizing", "writing_section", "assembling", "saving",
                "awaiting_save",
              ];
              const AWAITING_USER: ResearchTaskStatus[] = [
                "awaiting_outline_approval",
                "awaiting_save",
              ];
              const isRunning = RUNNING_STATES.includes(task.status);
              const needsUserAction = AWAITING_USER.includes(task.status);
              const cardMod = isRunning ? "running" : task.status === "done" ? "done" : task.status === "failed" ? "failed" : task.status === "cancelled" ? "cancelled" : "queued";
              const logs = taskLogs[task.id] ?? [];
              const showLog = logs.length > 0 || (task.status !== "done" && task.status !== "cancelled");
              const logLines = logs.length > 0 ? logs : task.status === "failed" && task.error ? [`✗ ${task.error}`] : ["正在初始化..."];
              return (
                <div key={task.id} className={`research-task-card research-task-card--${cardMod}`}>
                  <div className="research-task-head">
                    <div className="research-task-info">
                      <span
                        className="research-task-title"
                        title="点击打开研究对话框"
                        onClick={() => setDialogTask({ taskId: task.id, topic: task.topic, depth: task.depth ?? 1, breadth: task.breadth ?? 3, initialTask: task })}
                      >{task.topic}</span>
                      <span className="research-task-meta">
                        {formatLintCheckedAt(task.created_at)} · 深度 {task.depth ?? 1} · 广度 {task.breadth ?? 3}
                        {task.status === "done" && task.web_results_count ? ` · 来源 ${task.web_results_count} 个` : ""}
                      </span>
                    </div>
                    <span className={`research-badge ${getResearchStatusColor(task.status)}`}>
                      {getResearchStatusLabel(task.status)}
                    </span>
                  </div>

                  {showLog && (
                    <div className={`research-task-log${task.status === "failed" ? " research-task-log--failed" : ""}`}>
                      {logLines.map((log, i, arr) => (
                        <div
                          key={i}
                          className={`research-task-log-line${i === arr.length - 1 ? " research-task-log-line--latest" : ""}`}
                        >
                          <span className={`research-task-log-prompt${task.status === "failed" ? " research-task-log-prompt--failed" : ""}`}>&gt;</span>
                          {log}
                        </div>
                      ))}
                    </div>
                  )}

                  <div className="research-task-actions">
                    {task.status === "failed" && task.error && (
                      <span className="research-task-error" title={task.error}>{task.error}</span>
                    )}
                    {isRunning && (
                      <button
                        type="button"
                        className={`dev-panel__button${needsUserAction ? " dev-panel__button--accent" : ""}`}
                        onClick={() => setDialogTask({
                          taskId: task.id,
                          topic: task.topic,
                          depth: task.depth ?? 1,
                          breadth: task.breadth ?? 3,
                          initialTask: task,
                        })}
                        title={needsUserAction ? "任务等待你的输入" : "查看实时进度"}
                      >
                        {needsUserAction ? "⏸ 需要确认" : "💬 打开对话"}
                      </button>
                    )}
                    {task.status === "done" && task.saved_path && (
                      <>
                        <button type="button" className="dev-panel__button btn--wiki" onClick={() => onOpenWikiPage(task.saved_path!)}>查看 Wiki</button>
                        <button type="button" className="dev-panel__button btn--word" onClick={() => void handleDownloadWord(task)}>导出 Word</button>
                        <button type="button" className="dev-panel__button btn--html" onClick={() => void exportResearchHtml(task.saved_path!, task.topic)}>导出 HTML</button>
                      </>
                    )}
                    {isRunning && (
                      <button type="button" className="dev-panel__button btn--cancel" onClick={() => handleCancel(task.id)}>取消</button>
                    )}
                    {(task.status === "failed" || task.status === "cancelled") && (
                      <button type="button" className="dev-panel__button btn--retry" onClick={() => void handleRetryTask(task)}>重试</button>
                    )}
                    {(task.status === "done" || task.status === "failed" || task.status === "cancelled") && (
                      <button type="button" className="dev-panel__button btn--danger" onClick={() => void handleDeleteTask(task)}>删除</button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
    </>
  );
}
