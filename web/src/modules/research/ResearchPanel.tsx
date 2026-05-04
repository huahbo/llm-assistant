import { useCallback, useEffect, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
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

type DeleteModalOutcome = "cancel" | "task-only" | "task-and-wiki";
type DeleteModalState = {
  latestTask: ResearchTaskItem;
  hasSavedWiki: boolean;
  resolve: (outcome: DeleteModalOutcome) => void;
};

export const getResearchStatusLabel = (status: ResearchTaskStatus): string => {
  const labels: Record<ResearchTaskStatus, string> = {
    queued: "等待",
    decomposing: "分解中",
    searching: "搜索中",
    synthesizing: "合成中",
    saving: "保存中",
    done: "完成",
    failed: "失败",
    cancelled: "已取消",
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
  const [showAdvanced, setShowAdvanced] = useState(false);
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
      .then((cfg) => setHasSearchProvider(cfg.search_provider !== "none"))
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
      setStartError(err instanceof Error ? err.message : "启动研究任务失败，请重试");
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
      setTaskActionError(err instanceof Error ? err.message : "重试失败，请稍后再试");
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

      {/* 无搜索提供商警告 */}
      {!hasSearchProvider && (
        <div className="panel" style={{ background: "var(--color-warning-bg, #fffbe6)", border: "1px solid var(--color-warning, #e6a817)", borderRadius: "6px", padding: "10px 14px", marginBottom: "8px", color: "var(--color-warning-text, #7c5a00)" }}>
          ⚠️ 尚未配置搜索提供商。请先在「搜索设置」中填写 Tavily API Key 或 SearXNG 地址。
        </div>
      )}

      {/* 错误提示 */}
      {startError && (
        <div className="panel" style={{ background: "var(--color-error-bg, #fff0f0)", border: "1px solid var(--color-error, #d94f4f)", borderRadius: "6px", padding: "10px 14px", marginBottom: "8px", color: "var(--color-error, #d94f4f)" }}>
          ✕ {startError}
        </div>
      )}
      {downloadError && (
        <div className="panel" style={{ background: "var(--color-error-bg, #fff0f0)", border: "1px solid var(--color-error, #d94f4f)", borderRadius: "6px", padding: "10px 14px", marginBottom: "8px", color: "var(--color-error, #d94f4f)" }}>
          ✕ 导出失败：{downloadError}
        </div>
      )}
      {taskActionError && (
        <div className="panel" style={{ background: "var(--color-error-bg, #fff0f0)", border: "1px solid var(--color-error, #d94f4f)", borderRadius: "6px", padding: "10px 14px", marginBottom: "8px", color: "var(--color-error, #d94f4f)" }}>
          ✕ 任务操作失败：{taskActionError}
        </div>
      )}

      {/* 输入区 */}
      <section className="panel">
        <div className="section-head">
          <h2>新建研究任务</h2>
        </div>
        <div className="dev-panel__field" style={{ display: "flex", gap: "8px", marginBottom: "8px" }}>
          <input
            className="dev-panel__input"
            style={{ flex: 1 }}
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

        {/* 高级选项 */}
        <div style={{ marginBottom: "8px" }}>
          <button
            type="button"
            className="dev-panel__button"
            onClick={() => setShowAdvanced((v) => !v)}
            style={{ fontSize: "12px" }}
          >
            {showAdvanced ? "▲ 收起高级选项" : "▼ 展开高级选项"}
          </button>
        </div>
        {showAdvanced && (
          <div style={{ display: "flex", gap: "16px", flexWrap: "wrap", marginBottom: "8px" }}>
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="research-depth">研究深度</label>
              <select
                id="research-depth"
                className="dev-panel__input"
                value={depth}
                onChange={(e) => setDepth(Number(e.target.value))}
              >
                <option value={1}>1 - 标准 (快速)</option>
                <option value={2}>2 - 进阶 (更全面)</option>
                <option value={3}>3 - 深度 (多轮迭代)</option>
                <option value={4}>4 - 极深</option>
                <option value={5}>5 - 极限研究</option>
              </select>
            </div>
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="research-breadth">搜索广度</label>
              <select
                id="research-breadth"
                className="dev-panel__input"
                value={breadth}
                onChange={(e) => setBreadth(Number(e.target.value))}
              >
                <option value={2}>2</option>
                <option value={3}>3</option>
                <option value={4}>4</option>
                <option value={5}>5</option>
              </select>
            </div>
          </div>
        )}
      </section>

      {/* 任务列表 */}
      <section className="panel">
        <div className="section-head">
          <h2>任务列表</h2>
          <span className="section-head__hint">
            运行中 {runningCount} 条 · 历史 {doneCount} 条
          </span>
        </div>
        {researchTasks.length === 0 ? (
          <p className="empty-state">暂无研究任务。输入研究主题后点击"开始研究"。</p>
        ) : (
          <div className="queue-list">
            {researchTasks.map((task) => (
              <div
                key={task.id}
                className="queue-item"
                style={{ flexDirection: "column", alignItems: "stretch", padding: "12px" }}
              >
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: "8px" }}>
                  <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
                    <span
                      style={{ fontWeight: 600, fontSize: "14px", cursor: "pointer", textDecoration: "underline dotted", textUnderlineOffset: "3px" }}
                      title="点击打开研究对话框"
                      onClick={() => setDialogTask({ taskId: task.id, topic: task.topic, depth: task.depth ?? 1, breadth: task.breadth ?? 3, initialTask: task })}
                    >{task.topic}</span>
                    <span style={{ fontSize: "10px", color: "var(--text-secondary)" }}>
                      创建：{formatLintCheckedAt(task.created_at)} ·
                      更新：{formatLintCheckedAt(task.updated_at)} ·
                      深度 {task.depth || 1} · 广度 {task.breadth || 3}
                    </span>
                  </div>
                  <span className={`queue-badge ${getResearchStatusColor(task.status)}`}>
                    {getResearchStatusLabel(task.status)}
                  </span>
                </div>

                {/* 日志区：运行中与失败任务均显示，便于定位失败原因 */}
                {((taskLogs[task.id] && taskLogs[task.id].length > 0) ||
                  (task.status !== "done" && task.status !== "cancelled")) && (
                  <div
                    style={{
                      backgroundColor: "rgba(0,0,0,0.2)",
                      padding: "8px",
                      borderRadius: "4px",
                      fontFamily: "monospace",
                      fontSize: "11px",
                      marginBottom: "8px",
                      borderLeft:
                        task.status === "failed"
                          ? "3px solid var(--error)"
                          : "3px solid var(--accent)",
                      maxHeight: task.status === "failed" ? "120px" : "80px",
                      overflowY: "auto"
                    }}
                  >
                    {((taskLogs[task.id] && taskLogs[task.id].length > 0)
                      ? taskLogs[task.id]
                      : task.status === "failed" && task.error
                        ? [`✗ ${task.error}`]
                        : ["正在初始化..."]).map((log, i, arr) => (
                      <div key={i} style={{ color: "var(--text-secondary)", opacity: i === arr.length - 1 ? 1 : 0.5 }}>
                        <span
                          style={{
                            color: task.status === "failed" ? "var(--error)" : "var(--accent)",
                            marginRight: "4px",
                          }}
                        >
                          &gt;
                        </span>
                        {log}
                      </div>
                    ))}
                  </div>
                )}

                {/* 完成后：显示结果统计 */}
                {task.status === "done" && (
                  <div style={{ marginBottom: "8px", fontSize: "12px", color: "var(--text-secondary)" }}>
                    ✨ 已从 {task.web_results_count} 个来源中提取并综合信息。
                  </div>
                )}

                <div style={{ display: "flex", gap: "8px", justifyContent: "flex-end", alignItems: "center" }}>
                  {task.status === "done" && task.saved_path && (
                    <>
                      <button
                        type="button"
                        className="dev-panel__button"
                        style={{ fontSize: "12px" }}
                        onClick={() => onOpenWikiPage(task.saved_path!)}
                      >
                        📖 查看 Wiki
                      </button>
                      <button
                        type="button"
                        className="dev-panel__button"
                        style={{ fontSize: "12px" }}
                        onClick={() => void handleDownloadWord(task)}
                      >
                        📄 导出 Word
                      </button>
                    </>
                  )}
                  {task.status === "failed" && task.error && (
                    <span style={{ fontSize: "11px", color: "var(--error)", marginRight: "auto" }}>
                      错误: {task.error}
                    </span>
                  )}
                  {(task.status === "queued" || task.status === "decomposing" || task.status === "searching" || task.status === "synthesizing") && (
                    <button
                      type="button"
                      className="dev-panel__button"
                      style={{ fontSize: "12px" }}
                      onClick={() => handleCancel(task.id)}
                    >
                      🛑 取消
                    </button>
                  )}
                  {(task.status === "failed" || task.status === "cancelled") && (
                    <button
                      type="button"
                      className="dev-panel__button dev-panel__button--accent"
                      style={{ fontSize: "12px" }}
                      onClick={() => void handleRetryTask(task)}
                    >
                      🔄 重试
                    </button>
                  )}
                  {(task.status === "done" || task.status === "failed" || task.status === "cancelled") && (
                    <button
                      type="button"
                      className="dev-panel__button"
                      style={{ fontSize: "12px" }}
                      onClick={() => void handleDeleteTask(task)}
                    >
                      🗑 删除任务
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </section>
    </>
  );
}
