import { useEffect, useRef, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import {
  approveResearchOutline,
  approveResearchQueries,
  commitResearchToWiki,
  discardResearchReport,
  fetchWikiPageDetail,
  getPendingResearchContent,
  getPendingResearchOutline,
  getPendingResearchQueries,
  getResearchTask,
  listenResearchComplete,
  listenResearchDone,
  listenResearchError,
  listenResearchOutlineReady,
  listenResearchProgress,
  listenResearchQueriesReady,
  listenResearchStreamChunk,
  pickSaveFile,
  saveResearchDoc,
} from "../../tauri-client";
import type { ResearchOutlineData, ResearchTaskItem } from "../../types";

type DialogPhase =
  | "running"
  | "awaiting-approval"
  | "awaiting-outline-approval"
  | "synthesizing"
  | "awaiting-save"
  | "saving"
  | "done"
  | "failed";

type DialogMsg =
  | { kind: "user"; topic: string; depth: number; breadth: number }
  | { kind: "progress"; stage: string; text: string; sectionIndex?: number; totalSections?: number }
  | { kind: "outline"; outline: ResearchOutlineData; taskId: number }
  | { kind: "queries"; queries: string[]; taskId: number }
  | { kind: "synthesis"; content: string }
  | { kind: "report"; content: string }
  | { kind: "done"; savedPath: string; sources?: number; learnings?: number }
  | { kind: "discarded" }
  | { kind: "error"; text: string };

export default function ResearchDialog({
  taskId,
  topic,
  depth,
  breadth,
  initialTask,
  onClose,
  onRetry,
  onOpenWikiPage,
}: {
  taskId: number;
  topic: string;
  depth: number;
  breadth: number;
  initialTask?: ResearchTaskItem;
  onClose: () => void;
  onRetry?: () => void;
  onOpenWikiPage: (path: string) => void;
}) {
  const initPhase = (): DialogPhase => {
    if (!initialTask) return "running";
    if (initialTask.status === "done") return "done";
    if (initialTask.status === "failed" || initialTask.status === "cancelled" || initialTask.status === "discarded") return "failed";
    if (initialTask.status === "awaiting_outline_approval") return "awaiting-outline-approval";
    if (initialTask.status === "awaiting_save") return "awaiting-save";
    return "running";
  };

  const initMessages = (): DialogMsg[] => {
    const msgs: DialogMsg[] = [{ kind: "user", topic, depth, breadth }];
    if (!initialTask) return msgs;
    if (initialTask.status === "done" && initialTask.saved_path) {
      if ((initialTask.web_results_count ?? 0) > 0) {
        msgs.push({ kind: "progress", stage: "searching", text: `已从 ${initialTask.web_results_count} 个来源提取信息` });
      }
      msgs.push({ kind: "done", savedPath: initialTask.saved_path });
    } else if (initialTask.status === "failed" || initialTask.status === "cancelled") {
      if (initialTask.error) {
        msgs.push({ kind: "error", text: initialTask.error });
      } else {
        msgs.push({ kind: "error", text: initialTask.status === "cancelled" ? "任务已取消" : "任务失败" });
      }
    }
    return msgs;
  };

  const [messages, setMessages] = useState<DialogMsg[]>(initMessages);
  const [phase, setPhase] = useState<DialogPhase>(initPhase);
  const [editableQueries, setEditableQueries] = useState<string[]>([]);
  const [editableOutline, setEditableOutline] = useState<ResearchOutlineData | null>(null);
  const [approvingOutline, setApprovingOutline] = useState(false);
  const [approving, setApproving] = useState(false);
  const [synthesisContent, setSynthesisContent] = useState("");
  /** 报告生成完成后的最终全文（含 frontmatter + 正文 + References），来自 research_complete 事件 */
  const [reportContent, setReportContent] = useState("");
  const [savingToWiki, setSavingToWiki] = useState(false);
  const [discarding, setDiscarding] = useState(false);
  const [doneSavedPath, setDoneSavedPath] = useState<string>(
    initialTask?.status === "done" ? (initialTask.saved_path ?? "") : ""
  );
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, synthesisContent]);

  const isTerminal = initialTask && (initialTask.status === "done" || initialTask.status === "failed" || initialTask.status === "cancelled");
  useEffect(() => {
    if (isTerminal) return;

    let cancelled = false;
    const unlisteners: (() => void)[] = [];

    const setup = async () => {
      const u1 = await listenResearchProgress((p) => {
        if (p.task_id !== taskId) return;
        if (p.stage === "synthesizing") setPhase("synthesizing");
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          const entry: DialogMsg = {
            kind: "progress", stage: p.stage, text: p.message,
            sectionIndex: p.section_index,
            totalSections: p.total_sections,
          };
          if (last?.kind === "progress" && last.stage === p.stage) {
            return [...prev.slice(0, -1), entry];
          }
          return [...prev, entry];
        });
      });
      if (cancelled) { u1(); return; }
      unlisteners.push(u1);

      const uOutline = await listenResearchOutlineReady((p) => {
        if (p.task_id !== taskId) return;
        setEditableOutline(p.outline);
        setPhase("awaiting-outline-approval");
        setMessages((prev) => {
          if (prev.some((m) => m.kind === "outline")) return prev;
          return [...prev, { kind: "outline", outline: p.outline, taskId: p.task_id }];
        });
      });
      if (cancelled) { uOutline(); return; }
      unlisteners.push(uOutline);

      const u2 = await listenResearchQueriesReady((p) => {
        if (p.task_id !== taskId) return;
        setEditableQueries(p.queries);
        setPhase("awaiting-approval");
        setMessages((prev) => {
          if (prev.some((m) => m.kind === "queries")) return prev;
          return [...prev, { kind: "queries", queries: p.queries, taskId: p.task_id }];
        });
      });
      if (cancelled) { u2(); return; }
      unlisteners.push(u2);

      const u3 = await listenResearchStreamChunk((p) => {
        if (p.task_id !== taskId) return;
        setSynthesisContent((prev) => prev + p.chunk);
      });
      if (cancelled) { u3(); return; }
      unlisteners.push(u3);

      const uComplete = await listenResearchComplete((p) => {
        if (p.task_id !== taskId) return;
        setReportContent(p.content);
        setPhase("awaiting-save");
        setMessages((prev) => {
          if (prev.some((m) => m.kind === "report")) return prev;
          return [...prev, { kind: "report", content: p.content }];
        });
      });
      if (cancelled) { uComplete(); return; }
      unlisteners.push(uComplete);

      const u4 = await listenResearchDone((p) => {
        if (p.task_id !== taskId) return;
        setPhase("done");
        setDoneSavedPath(p.saved_path);
        setMessages((prev) => {
          if (prev.some((m) => m.kind === "done")) return prev;
          return [...prev, { kind: "done", savedPath: p.saved_path }];
        });
      });
      if (cancelled) { u4(); return; }
      unlisteners.push(u4);

      const u5 = await listenResearchError((p) => {
        if (p.task_id !== taskId) return;
        setPhase("failed");
        setMessages((prev) => {
          if (prev.some((m) => m.kind === "error")) return prev;
          return [...prev, { kind: "error", text: p.error }];
        });
      });
      if (cancelled) { u5(); return; }
      unlisteners.push(u5);
    };

    void setup();

    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u());
    };
  }, [taskId]);

  useEffect(() => {
    if (isTerminal) return;
    void getResearchTask(taskId).then((task) => {
      if (!task) return;
      const s = task.status as string;
      if (s === "done" && task.saved_path) {
        setPhase("done");
        setDoneSavedPath(task.saved_path);
        setMessages((prev) => {
          if (prev.some((m) => m.kind === "done")) return prev;
          return [...prev, { kind: "done", savedPath: task.saved_path! }];
        });
      } else if (s === "failed" || s === "cancelled") {
        setPhase("failed");
        setMessages((prev) => {
          if (prev.some((m) => m.kind === "error")) return prev;
          return [...prev, { kind: "error", text: task.error ?? (s === "cancelled" ? "任务已取消" : "任务失败") }];
        });
      }
    });
  }, [taskId, isTerminal]);

  // 关闭对话框后重新打开：恢复待审批的大纲 / 子查询数据。
  // 后端把数据存在内存缓存里（pending_outline_data / pending_query_data），
  // 不依赖 Tauri 事件的"一次性"特性。
  useEffect(() => {
    if (isTerminal) return;
    let cancelled = false;
    void getPendingResearchOutline(taskId).then((json) => {
      if (cancelled || !json) return;
      try {
        const outline = JSON.parse(json) as ResearchOutlineData;
        setEditableOutline((prev) => prev ?? outline);
        setPhase((prev) => (prev === "running" ? "awaiting-outline-approval" : prev));
        setMessages((prev) =>
          prev.some((m) => m.kind === "outline")
            ? prev
            : [...prev, { kind: "outline", outline, taskId }],
        );
      } catch {
        // 静默：缓存内容损坏时由事件流接管
      }
    });
    void getPendingResearchQueries(taskId).then((queries) => {
      if (cancelled || !queries || queries.length === 0) return;
      setEditableQueries((prev) => (prev.length > 0 ? prev : queries));
      setPhase((prev) => (prev === "running" ? "awaiting-approval" : prev));
      setMessages((prev) =>
        prev.some((m) => m.kind === "queries")
          ? prev
          : [...prev, { kind: "queries", queries, taskId }],
      );
    });
    void getPendingResearchContent(taskId).then((content) => {
      if (cancelled || !content) return;
      setReportContent((prev) => prev || content);
      setPhase((prev) =>
        prev === "running" || prev === "awaiting-outline-approval" ? "awaiting-save" : prev,
      );
      setMessages((prev) =>
        prev.some((m) => m.kind === "report")
          ? prev
          : [...prev, { kind: "report", content }],
      );
    });
    return () => {
      cancelled = true;
    };
  }, [taskId, isTerminal]);

  const handleExportMd = async () => {
    let content = reportContent.trim() || synthesisContent.trim();
    if (!content && doneSavedPath) {
      const detail = await fetchWikiPageDetail(doneSavedPath);
      content = detail?.content?.trim() ?? "";
    }
    if (!content) return;
    const safeTopic = topic.replace(/[\\/:*?"<>|]/g, "_").slice(0, 60);
    const savePath = await pickSaveFile({
      defaultPath: `${safeTopic}.md`,
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    if (!savePath) return;
    await saveResearchDoc(savePath, content);
  };

  const handleSaveToWiki = async () => {
    setSavingToWiki(true);
    setPhase("saving");
    setMessages((prev) => [
      ...prev,
      { kind: "progress", stage: "saving", text: "正在写入 Wiki 并索引..." },
    ]);
    try {
      const savedPath = await commitResearchToWiki(taskId);
      setPhase("done");
      setDoneSavedPath(savedPath);
      setMessages((prev) => {
        if (prev.some((m) => m.kind === "done")) return prev;
        return [...prev, { kind: "done", savedPath }];
      });
    } catch (err) {
      setPhase("awaiting-save");
      setMessages((prev) => [
        ...prev,
        { kind: "error", text: err instanceof Error ? err.message : String(err) },
      ]);
    } finally {
      setSavingToWiki(false);
    }
  };

  const handleDiscard = async () => {
    setDiscarding(true);
    try {
      await discardResearchReport(taskId);
      setPhase("failed");
      setMessages((prev) => {
        if (prev.some((m) => m.kind === "discarded")) return prev;
        return [...prev, { kind: "discarded" }];
      });
    } catch {
      // 静默
    } finally {
      setDiscarding(false);
    }
  };

  const handleApprove = async () => {
    const valid = editableQueries.map((q) => q.trim()).filter(Boolean);
    if (valid.length === 0) return;
    setApproving(true);
    try {
      await approveResearchQueries(taskId, valid);
      setPhase("running");
      setMessages((prev) =>
        prev.map((m) =>
          m.kind === "queries" && m.taskId === taskId
            ? { ...m, queries: valid }
            : m,
        ),
      );
    } catch {
      // 超时或已失效，不报错，继续
    } finally {
      setApproving(false);
    }
  };

  const renderMarkdown = (md: string) => {
    try {
      return DOMPurify.sanitize(marked.parse(md, { async: false }) as string);
    } catch {
      return md;
    }
  };

  const stageIcon: Record<string, string> = {
    decomposing: "🔍",
    searching: "🌐",
    synthesizing: "✍️",
    writing_section: "📝",
    assembling: "🔧",
    saving: "💾",
    awaiting_approval: "⏸️",
    awaiting_save: "📨",
    planning_outline: "📋",
    awaiting_outline_approval: "⏸️",
  };

  const phaseLabel: Record<string, { text: string; color: string }> = {
    running:                    { text: "进行中...", color: "var(--accent)" },
    "awaiting-approval":        { text: "等待确认研究方向", color: "#d97706" },
    "awaiting-outline-approval": { text: "等待大纲确认", color: "#d97706" },
    synthesizing:               { text: "生成报告中...", color: "var(--accent)" },
    "awaiting-save":            { text: "报告就绪 · 等待保存", color: "#d97706" },
    saving:                     { text: "保存到知识库中...", color: "var(--accent)" },
    done:                       { text: "已保存到知识库", color: "#16a34a" },
    failed:                     { text: "失败", color: "var(--error, #dc2626)" },
  };

  return (
    <div
      role="dialog"
      aria-modal="true"
      style={{
        position: "fixed", inset: 0, zIndex: 9998,
        background: "rgba(15,23,42,0.55)",
        backdropFilter: "blur(2px)",
        display: "flex", alignItems: "center", justifyContent: "center",
        padding: "20px",
      }}
    >
      <div style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: "14px",
        width: "100%", maxWidth: "740px",
        maxHeight: "90vh",
        display: "flex", flexDirection: "column",
        boxShadow: "0 20px 60px rgba(0,0,0,0.18)",
      }}>
        {/* 标题栏 */}
        <div style={{
          display: "flex", alignItems: "center", justifyContent: "space-between",
          padding: "14px 18px 14px 20px",
          borderBottom: "1px solid var(--border-light)",
          flexShrink: 0,
        }}>
          <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
            <span style={{ fontSize: "18px" }}>🔬</span>
            <span style={{ fontWeight: 700, fontSize: "15px", color: "var(--text)" }}>深度研究</span>
            <span style={{
              fontSize: "11px", fontWeight: 500,
              color: phaseLabel[phase]?.color ?? "var(--text-muted)",
              background: "var(--bg-page)",
              padding: "2px 8px", borderRadius: "999px",
              border: "1px solid var(--border-light)",
            }}>
              {phaseLabel[phase]?.text}
            </span>
          </div>
          <button
            type="button"
            aria-label="关闭对话框（任务继续在后台运行）"
            onClick={onClose}
            title="关闭（任务继续后台运行）"
            style={{
              background: "none", border: "none", cursor: "pointer",
              fontSize: "16px", color: "var(--text-muted)",
              padding: "4px 6px", borderRadius: "6px", lineHeight: 1,
            }}
          >
            ✕
          </button>
        </div>

        {/* 消息列表 */}
        <div style={{
          flex: 1, overflowY: "auto", padding: "20px",
          display: "flex", flexDirection: "column", gap: "14px",
        }}>

          {messages.map((msg, i) => {
            if (msg.kind === "user") {
              return (
                <div key={i} style={{ display: "flex", justifyContent: "flex-end" }}>
                  <div style={{
                    background: "var(--accent-grad)", color: "#fff",
                    borderRadius: "14px 14px 3px 14px",
                    padding: "10px 16px", maxWidth: "80%",
                    boxShadow: "0 2px 8px rgba(124,58,237,0.25)",
                  }}>
                    <div style={{ fontWeight: 600, fontSize: "14px", lineHeight: 1.4 }}>{msg.topic}</div>
                    <div style={{ fontSize: "11px", opacity: 0.85, marginTop: "4px" }}>
                      深度 {msg.depth} · 广度 {msg.breadth}
                    </div>
                  </div>
                </div>
              );
            }

            if (msg.kind === "progress") {
              const hasSectionBar = msg.stage === "writing_section"
                && msg.sectionIndex !== undefined
                && msg.totalSections !== undefined
                && msg.totalSections > 0;
              const pct = hasSectionBar
                ? Math.round(((msg.sectionIndex! + 1) / msg.totalSections!) * 100)
                : 0;

              if (hasSectionBar) {
                return (
                  <div key={i} style={{
                    display: "flex", gap: "10px", alignItems: "flex-start",
                    padding: "10px 12px",
                    background: "var(--bg-page)",
                    border: "1px solid var(--border-light)",
                    borderRadius: 10,
                  }}>
                    <span style={{ fontSize: "18px", flexShrink: 0, marginTop: "1px" }}>
                      {stageIcon[msg.stage] ?? "📝"}
                    </span>
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div style={{
                        display: "flex", justifyContent: "space-between", alignItems: "baseline",
                        gap: 8, marginBottom: 6,
                      }}>
                        <span style={{
                          fontSize: 13, fontWeight: 600, color: "var(--text)",
                          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                          flex: 1, minWidth: 0,
                        }}>
                          {msg.text}
                        </span>
                        <span style={{
                          fontSize: 12, fontWeight: 700, color: "var(--accent)",
                          flexShrink: 0, fontVariantNumeric: "tabular-nums",
                        }}>
                          {msg.sectionIndex! + 1}/{msg.totalSections}
                        </span>
                      </div>
                      <div style={{
                        height: 8, borderRadius: 4,
                        background: "var(--border-light, var(--border))", overflow: "hidden",
                      }}>
                        <div style={{
                          width: `${pct}%`, height: "100%",
                          background: "var(--accent)", borderRadius: 4,
                          transition: "width 0.4s ease",
                          boxShadow: "0 0 8px rgba(124,58,237,0.4)",
                        }} />
                      </div>
                    </div>
                  </div>
                );
              }

              return (
                <div key={i} style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
                  <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "1px" }}>
                    {stageIcon[msg.stage] ?? "💬"}
                  </span>
                  <span style={{
                    fontSize: "13px", color: "var(--text-muted)",
                    lineHeight: 1.6, paddingTop: "1px",
                  }}>
                    {msg.text}
                  </span>
                </div>
              );
            }

            if (msg.kind === "outline") {
              return null;
            }

            if (msg.kind === "queries") {
              const isActive = phase === "awaiting-approval";
              return (
                <div key={i} style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
                  <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "2px" }}>⏸️</span>
                  <div style={{ flex: 1 }}>
                    <div style={{
                      fontSize: "13px", color: "var(--text)",
                      fontWeight: 500, marginBottom: "10px",
                    }}>
                      已分解为 {editableQueries.length} 个研究方向
                      {isActive && <span style={{ color: "var(--text-muted)", fontWeight: 400 }}>，可编辑后开始搜索</span>}
                      ：
                    </div>
                    <div style={{
                      background: "var(--bg-page)",
                      border: "1px solid var(--border)",
                      borderRadius: "10px", padding: "10px 12px",
                      display: "flex", flexDirection: "column", gap: "6px",
                      marginBottom: isActive ? "10px" : "0",
                    }}>
                      {editableQueries.map((q, qi) => (
                        <div key={qi} style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                          <span style={{
                            fontSize: "11px", fontWeight: 600,
                            color: "var(--accent)", minWidth: "20px",
                            background: "var(--bg-card)", borderRadius: "4px",
                            padding: "1px 5px", textAlign: "center",
                            border: "1px solid var(--border-light)",
                          }}>{qi + 1}</span>
                          {isActive ? (
                            <input
                              type="text"
                              value={q}
                              onChange={(e) => {
                                const updated = [...editableQueries];
                                updated[qi] = e.target.value;
                                setEditableQueries(updated);
                              }}
                              style={{
                                flex: 1,
                                background: "var(--bg-card)",
                                border: "1.5px solid var(--border)",
                                borderRadius: "7px",
                                padding: "6px 10px",
                                fontSize: "13px",
                                color: "var(--text)",
                                outline: "none",
                              }}
                            />
                          ) : (
                            <span style={{ fontSize: "13px", color: "var(--text)", flex: 1 }}>{q}</span>
                          )}
                          {isActive && editableQueries.length > 1 && (
                            <button
                              type="button"
                              title="删除此方向"
                              onClick={() => setEditableQueries(editableQueries.filter((_, idx) => idx !== qi))}
                              style={{
                                background: "none", border: "1px solid var(--border)",
                                borderRadius: "5px", cursor: "pointer",
                                fontSize: "12px", color: "var(--text-muted)",
                                padding: "3px 7px", lineHeight: 1,
                              }}
                            >✕</button>
                          )}
                        </div>
                      ))}
                    </div>
                    {isActive && (
                      <div style={{ display: "flex", gap: "8px", alignItems: "center", marginTop: "4px" }}>
                        <button
                          type="button"
                          className="dev-panel__button"
                          style={{ fontSize: "12px" }}
                          onClick={() => setEditableQueries([...editableQueries, ""])}
                        >
                          + 添加方向
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              );
            }

            if (msg.kind === "report") {
              return (
                <div key={i} style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
                  <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "2px" }}>📨</span>
                  <div style={{
                    flex: 1,
                    background: "var(--bg-page)",
                    border: "1px solid var(--border)",
                    borderRadius: 10,
                    padding: "14px 16px",
                    maxHeight: 420,
                    overflowY: "auto",
                  }}>
                    <div style={{
                      fontSize: 12, color: "var(--text-muted)", fontWeight: 600,
                      marginBottom: 8,
                    }}>
                      报告已生成（{msg.content.length.toLocaleString()} 字符）— 请在下方选择保存到知识库 / 导出 / 丢弃
                    </div>
                    <div
                      className="wiki-content"
                      style={{ fontSize: 13, lineHeight: 1.7, color: "var(--text)" }}
                      // biome-ignore lint/security/noDangerouslySetInnerHtml: sanitized by DOMPurify
                      dangerouslySetInnerHTML={{ __html: renderMarkdown(msg.content) }}
                    />
                  </div>
                </div>
              );
            }

            if (msg.kind === "done") {
              return (
                <div key={i} style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
                  <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "2px" }}>✅</span>
                  <div style={{
                    flex: 1, background: "#f0fdf4",
                    border: "1px solid #bbf7d0",
                    borderRadius: "10px", padding: "12px 14px",
                  }}>
                    <div style={{ fontSize: "13px", color: "#15803d", fontWeight: 600, marginBottom: "10px" }}>
                      ✓ 报告已保存到知识库
                    </div>
                    <div style={{ display: "flex", gap: "8px" }}>
                      <button
                        type="button"
                        className="dev-panel__button dev-panel__button--accent"
                        style={{ fontSize: "12px" }}
                        onClick={() => { onOpenWikiPage(msg.savedPath); onClose(); }}
                      >
                        📖 查看 Wiki
                      </button>
                    </div>
                  </div>
                </div>
              );
            }

            if (msg.kind === "discarded") {
              return (
                <div key={i} style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
                  <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "2px" }}>🗑️</span>
                  <div style={{
                    flex: 1,
                    background: "var(--bg-page)",
                    border: "1px solid var(--border)",
                    borderRadius: 10, padding: "12px 14px",
                    fontSize: 13, color: "var(--text-muted)",
                  }}>
                    报告已丢弃，未写入知识库。
                  </div>
                </div>
              );
            }

            if (msg.kind === "error") {
              return (
                <div key={i} style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
                  <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "2px" }}>❌</span>
                  <div style={{
                    flex: 1, background: "#fef2f2",
                    border: "1px solid #fecaca",
                    borderRadius: "10px", padding: "12px 14px",
                  }}>
                    <div style={{ fontSize: "13px", color: "#b91c1c", lineHeight: 1.5 }}>
                      {msg.text}
                    </div>
                  </div>
                </div>
              );
            }

            return null;
          })}

          {phase === "awaiting-outline-approval" && editableOutline && (() => {
            const outline = editableOutline;
            const updateSection = (idx: number, patch: Partial<typeof outline.sections[number]>) => {
              const newSections = outline.sections.map((s, i) =>
                i === idx ? { ...s, ...patch } : s
              );
              setEditableOutline({ ...outline, sections: newSections });
            };
            const updateQuestion = (sIdx: number, qIdx: number, val: string) => {
              const newSections = outline.sections.map((s, i) => {
                if (i !== sIdx) return s;
                const qs = [...s.key_questions];
                qs[qIdx] = val;
                return { ...s, key_questions: qs };
              });
              setEditableOutline({ ...outline, sections: newSections });
            };
            const addQuestion = (sIdx: number) => {
              const newSections = outline.sections.map((s, i) =>
                i === sIdx ? { ...s, key_questions: [...s.key_questions, ""] } : s
              );
              setEditableOutline({ ...outline, sections: newSections });
            };
            const removeQuestion = (sIdx: number, qIdx: number) => {
              const newSections = outline.sections.map((s, i) => {
                if (i !== sIdx) return s;
                return { ...s, key_questions: s.key_questions.filter((_, j) => j !== qIdx) };
              });
              setEditableOutline({ ...outline, sections: newSections });
            };
            const addSection = () => {
              const nextIdx = outline.sections.length + 1;
              setEditableOutline({
                ...outline,
                sections: [
                  ...outline.sections,
                  { heading: `## ${nextIdx}. 新章节`, key_questions: [""], search_queries: [] },
                ],
              });
            };
            const removeSection = (idx: number) => {
              setEditableOutline({
                ...outline,
                sections: outline.sections.filter((_, i) => i !== idx),
              });
            };

            // 提交前清洗：去空、剪空 key_questions、空 section 删除
            const sanitized = (): typeof outline => ({
              ...outline,
              sections: outline.sections
                .map((s) => ({
                  ...s,
                  heading: s.heading.trim(),
                  key_questions: s.key_questions.map((q) => q.trim()).filter(Boolean),
                  // 没有 search_queries 时由后端从 key_questions 推断；这里保留原值
                  search_queries: s.search_queries.filter((q) => q.trim()),
                }))
                .filter((s) => s.heading.length > 0),
            });
            const cleaned = sanitized();
            const isValid = cleaned.sections.length > 0
              && cleaned.sections.every((s) => s.key_questions.length > 0 || s.search_queries.length > 0);

            return (
              <div className="research-dialog-outline-approval" style={{
                marginTop: 12, padding: "14px 16px",
                background: "var(--bg-page)", borderRadius: 10,
                border: "1px solid var(--border)",
              }}>
                <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 12, color: "var(--text)" }}>
                  请确认研究大纲（可编辑章节标题、关键问题，支持增删）：
                </div>
                {outline.sections.map((section, idx) => (
                  <div key={idx} style={{
                    marginBottom: 12,
                    padding: "10px 12px",
                    background: "var(--bg-card)",
                    border: "1px solid var(--border-light)",
                    borderRadius: 8,
                  }}>
                    <div style={{ display: "flex", gap: 6, alignItems: "center", marginBottom: 8 }}>
                      <span style={{
                        fontSize: 11, fontWeight: 700, color: "var(--accent)",
                        minWidth: 22, textAlign: "center",
                        background: "var(--bg-page)", borderRadius: 4,
                        padding: "2px 6px", border: "1px solid var(--border-light)",
                      }}>{idx + 1}</span>
                      <input
                        type="text"
                        value={section.heading}
                        placeholder="章节标题"
                        onChange={(e) => updateSection(idx, { heading: e.target.value })}
                        style={{
                          flex: 1, fontSize: 13, fontWeight: 600,
                          padding: "6px 10px", borderRadius: 6,
                          border: "1.5px solid var(--border)",
                          background: "var(--bg-card)", color: "var(--text)",
                          outline: "none",
                        }}
                      />
                      {outline.sections.length > 1 && (
                        <button
                          type="button"
                          title="删除此章节"
                          onClick={() => removeSection(idx)}
                          style={{
                            background: "none", border: "1px solid var(--border)",
                            borderRadius: 5, cursor: "pointer",
                            fontSize: 12, color: "var(--text-muted)",
                            padding: "4px 8px", lineHeight: 1,
                          }}
                        >✕</button>
                      )}
                    </div>
                    <div style={{ marginLeft: 28, display: "flex", flexDirection: "column", gap: 5 }}>
                      <div style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 2 }}>
                        关键问题（每条 LLM 都会回答，可增删）
                      </div>
                      {section.key_questions.map((q, qIdx) => (
                        <div key={qIdx} style={{ display: "flex", gap: 6, alignItems: "center" }}>
                          <span style={{
                            fontSize: 10, color: "var(--text-faint, var(--text-muted))",
                            minWidth: 14, textAlign: "right",
                          }}>·</span>
                          <input
                            type="text"
                            value={q}
                            placeholder="新的关键问题..."
                            onChange={(e) => updateQuestion(idx, qIdx, e.target.value)}
                            style={{
                              flex: 1, fontSize: 12,
                              padding: "5px 8px", borderRadius: 5,
                              border: "1px solid var(--border-light)",
                              background: "var(--bg-page)", color: "var(--text)",
                              outline: "none",
                            }}
                          />
                          {section.key_questions.length > 1 && (
                            <button
                              type="button"
                              title="删除此问题"
                              onClick={() => removeQuestion(idx, qIdx)}
                              style={{
                                background: "none", border: "1px solid var(--border-light)",
                                borderRadius: 4, cursor: "pointer",
                                fontSize: 11, color: "var(--text-muted)",
                                padding: "3px 6px", lineHeight: 1,
                              }}
                            >✕</button>
                          )}
                        </div>
                      ))}
                      <button
                        type="button"
                        onClick={() => addQuestion(idx)}
                        style={{
                          alignSelf: "flex-start",
                          marginTop: 2,
                          background: "none", border: "1px dashed var(--border)",
                          borderRadius: 5, cursor: "pointer",
                          fontSize: 11, color: "var(--text-muted)",
                          padding: "4px 10px",
                        }}
                      >+ 添加问题</button>
                    </div>
                  </div>
                ))}
                <button
                  type="button"
                  onClick={addSection}
                  style={{
                    width: "100%", marginTop: 4, marginBottom: 10,
                    background: "none", border: "1px dashed var(--accent)",
                    borderRadius: 8, cursor: "pointer",
                    fontSize: 12, color: "var(--accent)",
                    padding: "8px 10px", fontWeight: 600,
                  }}
                >+ 添加章节</button>
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--accent"
                  style={{ width: "100%", opacity: isValid ? 1 : 0.5 }}
                  disabled={approvingOutline || !isValid}
                  title={!isValid ? "至少需要 1 个有效章节，每章至少 1 个关键问题或搜索词" : ""}
                  onClick={async () => {
                    setApprovingOutline(true);
                    try {
                      const json = JSON.stringify(cleaned);
                      await approveResearchOutline(taskId, json);
                      setPhase("running");
                      setMessages((prev) => [
                        ...prev,
                        { kind: "progress", stage: "searching", text: `✓ 大纲已确认（${cleaned.sections.length} 章），开始搜索...` },
                      ]);
                    } catch {
                      // 静默，继续等待
                    } finally {
                      setApprovingOutline(false);
                    }
                  }}
                >
                  {approvingOutline ? "确认中..." : `确认大纲（${cleaned.sections.length} 章）`}
                </button>
              </div>
            );
          })()}

          {/* 流式综合报告预览 */}
          {synthesisContent && (
            <div style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
              <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "2px" }}>✍️</span>
              <div style={{
                flex: 1,
                background: "var(--bg-page)",
                border: "1px solid var(--border)",
                borderRadius: "10px", padding: "14px 16px",
                maxHeight: "380px", overflowY: "auto",
              }}>
                <div
                  className="wiki-content"
                  style={{ fontSize: "13px", lineHeight: 1.75, color: "var(--text)" }}
                  // biome-ignore lint/security/noDangerouslySetInnerHtml: sanitized by DOMPurify
                  dangerouslySetInnerHTML={{ __html: renderMarkdown(synthesisContent) }}
                />
                {phase === "synthesizing" && (
                  <span style={{
                    display: "inline-block", width: "7px", height: "15px",
                    background: "var(--accent)", borderRadius: "2px",
                    animation: "blink 1s step-end infinite", marginLeft: "2px",
                    verticalAlign: "middle",
                  }} />
                )}
              </div>
            </div>
          )}

          <div ref={bottomRef} />
        </div>

        {/* 底部操作栏：根据阶段显示主操作按钮 */}
        {(phase === "done" || phase === "failed" || phase === "awaiting-approval" || phase === "awaiting-outline-approval" || phase === "awaiting-save") && (
          <div style={{
            borderTop: "1px solid var(--border-light)",
            padding: "10px 18px",
            display: "flex", justifyContent: "flex-end", gap: "8px",
            flexShrink: 0,
            background: "var(--bg-card)",
            borderRadius: "0 0 14px 14px",
          }}>
            {phase === "awaiting-approval" && (
              <button
                type="button"
                className="dev-panel__button dev-panel__button--accent"
                style={{ fontSize: "13px" }}
                disabled={approving || editableQueries.every((q) => !q.trim())}
                onClick={() => void handleApprove()}
              >
                {approving ? "提交中..." : "开始搜索 →"}
              </button>
            )}
            {phase === "awaiting-save" && (
              <>
                <button
                  type="button"
                  className="dev-panel__button"
                  style={{ fontSize: 13 }}
                  onClick={() => void handleExportMd()}
                  title="导出为 Markdown 文件（不写入 Wiki）"
                  disabled={savingToWiki || discarding}
                >
                  ⬇ 导出 .md
                </button>
                <button
                  type="button"
                  className="dev-panel__button"
                  style={{ fontSize: 13, color: "var(--error, #dc2626)" }}
                  onClick={() => void handleDiscard()}
                  title="丢弃本次报告（不保存）"
                  disabled={savingToWiki || discarding}
                >
                  {discarding ? "丢弃中..." : "🗑 丢弃"}
                </button>
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--accent"
                  style={{ fontSize: 13 }}
                  onClick={() => void handleSaveToWiki()}
                  disabled={savingToWiki || discarding}
                >
                  {savingToWiki ? "保存中..." : "💾 保存到知识库"}
                </button>
              </>
            )}
            {phase === "done" && (reportContent || synthesisContent || doneSavedPath) && (
              <button
                type="button"
                className="dev-panel__button"
                style={{ fontSize: "13px" }}
                onClick={() => void handleExportMd()}
                title="导出综合报告为 Markdown 文件"
              >
                ⬇ 导出 .md
              </button>
            )}
            {phase === "done" && doneSavedPath && (
              <button
                type="button"
                className="dev-panel__button dev-panel__button--accent"
                style={{ fontSize: "13px" }}
                onClick={() => { onOpenWikiPage(doneSavedPath); onClose(); }}
              >
                📖 查看 Wiki
              </button>
            )}
            {phase === "failed" && onRetry && (
              <button
                type="button"
                className="dev-panel__button dev-panel__button--accent"
                style={{ fontSize: "13px" }}
                onClick={() => onRetry()}
              >
                🔄 重试
              </button>
            )}
            <button
              type="button"
              className="dev-panel__button"
              style={{ fontSize: "13px" }}
              onClick={onClose}
            >
              关闭
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
