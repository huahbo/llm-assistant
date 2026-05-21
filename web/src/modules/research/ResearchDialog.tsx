import { useEffect, useRef, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import {
  approveResearchOutline,
  approveResearchQueries,
  fetchWikiPageDetail,
  getResearchTask,
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

type DialogMsg =
  | { kind: "user"; topic: string; depth: number; breadth: number }
  | { kind: "progress"; stage: string; text: string; sectionIndex?: number; totalSections?: number }
  | { kind: "outline"; outline: ResearchOutlineData; taskId: number }
  | { kind: "queries"; queries: string[]; taskId: number }
  | { kind: "synthesis"; content: string }
  | { kind: "done"; savedPath: string; sources?: number; learnings?: number }
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
  const initPhase = (): "running" | "awaiting-approval" | "synthesizing" | "done" | "failed" => {
    if (!initialTask) return "running";
    if (initialTask.status === "done") return "done";
    if (initialTask.status === "failed" || initialTask.status === "cancelled") return "failed";
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
  const [phase, setPhase] = useState<
    "running" | "awaiting-approval" | "awaiting-outline-approval" | "synthesizing" | "done" | "failed"
  >(initPhase);
  const [editableQueries, setEditableQueries] = useState<string[]>([]);
  const [editableOutline, setEditableOutline] = useState<ResearchOutlineData | null>(null);
  const [approvingOutline, setApprovingOutline] = useState(false);
  const [approving, setApproving] = useState(false);
  const [synthesisContent, setSynthesisContent] = useState("");
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

  const handleExportMd = async () => {
    let content = synthesisContent.trim();
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
    planning_outline: "📋",
    awaiting_outline_approval: "⏸️",
  };

  const phaseLabel: Record<string, { text: string; color: string }> = {
    running:                    { text: "进行中...", color: "var(--accent)" },
    "awaiting-approval":        { text: "等待确认研究方向", color: "#d97706" },
    "awaiting-outline-approval": { text: "等待大纲确认", color: "#d97706" },
    synthesizing:               { text: "生成报告中...", color: "var(--accent)" },
    done:                       { text: "已完成", color: "#16a34a" },
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
              return (
                <div key={i} style={{ display: "flex", gap: "10px", alignItems: "flex-start" }}>
                  <span style={{ fontSize: "15px", flexShrink: 0, marginTop: "1px" }}>
                    {stageIcon[msg.stage] ?? "💬"}
                  </span>
                  <div style={{ flex: 1 }}>
                    <span style={{
                      fontSize: "13px", color: "var(--text-muted)",
                      lineHeight: 1.6, paddingTop: "1px",
                    }}>
                      {msg.text}
                    </span>
                    {hasSectionBar && (
                      <div style={{ marginTop: "4px", display: "flex", alignItems: "center", gap: "8px" }}>
                        <div style={{
                          flex: 1, height: "4px", borderRadius: "2px",
                          background: "var(--border)", overflow: "hidden",
                        }}>
                          <div style={{
                            width: `${pct}%`, height: "100%",
                            background: "var(--accent)", borderRadius: "2px",
                            transition: "width 0.3s ease",
                          }} />
                        </div>
                        <span style={{ fontSize: "11px", color: "var(--text-faint)", minWidth: "32px" }}>
                          {msg.sectionIndex! + 1}/{msg.totalSections}
                        </span>
                      </div>
                    )}
                  </div>
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
                      研究完成！已保存到知识库
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

          {phase === "awaiting-outline-approval" && editableOutline && (
            <div className="research-dialog-outline-approval" style={{ marginTop: 12, padding: "12px 16px", background: "var(--bg-content)", borderRadius: 8, border: "1px solid var(--border)" }}>
              <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 10 }}>
                请确认研究大纲（可编辑章节标题）：
              </div>
              {editableOutline.sections.map((section, idx) => (
                <div key={idx} style={{ marginBottom: 10 }}>
                  <input
                    style={{ width: "100%", fontSize: 13, fontWeight: 600, padding: "4px 8px", borderRadius: 4, border: "1px solid var(--border)", background: "var(--bg)", color: "var(--text)", marginBottom: 4 }}
                    value={section.heading}
                    onChange={(e) => {
                      const newSections = [...editableOutline.sections];
                      newSections[idx] = { ...newSections[idx], heading: e.target.value };
                      setEditableOutline({ ...editableOutline, sections: newSections });
                    }}
                  />
                  <div style={{ fontSize: 11.5, color: "var(--text-muted)", paddingLeft: 4 }}>
                    {section.key_questions.join(" · ")}
                  </div>
                </div>
              ))}
              <button
                type="button"
                className="dev-panel__button dev-panel__button--accent"
                style={{ marginTop: 8, width: "100%" }}
                disabled={approvingOutline}
                onClick={async () => {
                  setApprovingOutline(true);
                  try {
                    const json = JSON.stringify(editableOutline);
                    await approveResearchOutline(taskId, json);
                    setPhase("running");
                    setMessages((prev) => [
                      ...prev,
                      { kind: "progress", stage: "searching", text: `✓ 大纲已确认（${editableOutline.sections.length} 章），开始搜索...` },
                    ]);
                  } catch {
                    // 静默，继续等待
                  } finally {
                    setApprovingOutline(false);
                  }
                }}
              >
                {approvingOutline ? "确认中..." : `确认大纲（${editableOutline.sections.length} 章）`}
              </button>
            </div>
          )}

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
        {(phase === "done" || phase === "failed" || phase === "awaiting-approval" || phase === "awaiting-outline-approval") && (
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
            {phase === "done" && (synthesisContent || doneSavedPath) && (
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
