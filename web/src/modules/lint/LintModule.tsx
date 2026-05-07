import { useState, useEffect, useMemo } from "react";
import type { Dispatch, SetStateAction } from "react";
import { useToast } from "../../contexts/ToastContext";
import {
  filterLintIssuesBySeverity,
  filterLintIssuesByCode,
  filterLintIssuesByPath,
  filterLintIssuesBySuggestion,
  formatLintCheckedAt,
  normalizeLintSeverity,
  readLintFilterState,
  resolveLintSeverityStats,
  writeLintFilterState,
  type LintSeverityFilter,
} from "../../lint-utils";
import {
  isTauriRuntime,
  runLint,
  previewLintPatches,
  applyLintPatch,
  applyLintPatchesBatch,
  fetchRecentLintPatchEvents,
} from "../../tauri-client";
import type {
  LintIssue,
  LintPatchBatchResult,
  LintPatchEvent,
  LintPatchPreviewItem,
  LintReport,
  LintSeverityStats,
} from "../../types";

type LintIssueGroup = {
  path: string;
  issues: LintIssue[];
};

type LintPatchPreviewGroup = {
  path: string;
  items: LintPatchPreviewItem[];
};

const groupLintIssuesByPath = (issues: LintIssue[]): LintIssueGroup[] => {
  const map = new Map<string, LintIssue[]>();
  for (const issue of issues) {
    const key = issue.path ?? "全局";
    const existing = map.get(key);
    if (existing) {
      existing.push(issue);
    } else {
      map.set(key, [issue]);
    }
  }
  return Array.from(map.entries()).map(([path, items]) => ({ path, issues: items }));
};

const groupPatchPreviewItemsByPath = (items: LintPatchPreviewItem[]): LintPatchPreviewGroup[] => {
  const map = new Map<string, LintPatchPreviewItem[]>();
  for (const item of items) {
    const key = item.path ?? "全局";
    const existing = map.get(key);
    if (existing) {
      existing.push(item);
    } else {
      map.set(key, [item]);
    }
  }
  return Array.from(map.entries()).map(([path, its]) => ({ path, items: its }));
};

type LintModuleProps = {
  onRefreshAppData: () => Promise<void>;
  onCreateBrokenWikiLinkPage: (targetTitle: string) => void | Promise<void>;
  onOpenPatchPage: (path: string) => void | Promise<void>;
};

const lintSeverityFilterLabels: Record<LintSeverityFilter, string> = {
  all: "全部",
  error: "错误",
  warning: "警告",
  info: "信息",
};

export default function LintModule({
  onRefreshAppData,
  onCreateBrokenWikiLinkPage,
  onOpenPatchPage,
}: LintModuleProps) {
  const { setStatusMessage } = useToast();

  // ---- lint-local state ----
  const [lintReport, setLintReport] = useState<LintReport | null>(null);
  const [lintRunning, setLintRunning] = useState(false);
  const [lintSeverityFilter, setLintSeverityFilter] = useState<LintSeverityFilter>("all");
  const [lintCodeKeyword, setLintCodeKeyword] = useState("");
  const [lintPathKeyword, setLintPathKeyword] = useState("");
  const [lintSuggestionKeyword, setLintSuggestionKeyword] = useState("");
  const [lintFilterStateLoaded, setLintFilterStateLoaded] = useState(false);
  const [lintPatchPreviewLoading, setLintPatchPreviewLoading] = useState(false);
  const [lintPatchPreviewItems, setLintPatchPreviewItems] = useState<LintPatchPreviewItem[]>([]);
  const [lintPatchPreviewError, setLintPatchPreviewError] = useState("");
  const [lintPatchApplyingKey, setLintPatchApplyingKey] = useState<string | null>(null);
  const [lintPatchBatchApplying, setLintPatchBatchApplying] = useState(false);
  const [lintPatchBatchSummary, setLintPatchBatchSummary] = useState<LintPatchBatchResult | null>(null);
  const [recentLintPatchEvents, setRecentLintPatchEvents] = useState<LintPatchEvent[]>([]);
  const [lintCollapsedGroups, setLintCollapsedGroups] = useState<Set<string>>(new Set());
  const [patchPreviewCollapsedGroups, setPatchPreviewCollapsedGroups] = useState<Set<string>>(new Set());

  // ---- on mount: load filter state + recent events ----
  useEffect(() => {
    void (async () => {
      const events = await fetchRecentLintPatchEvents();
      setRecentLintPatchEvents(events);

      const filterState = readLintFilterState();
      setLintSeverityFilter(filterState.severity);
      setLintCodeKeyword(filterState.codeKeyword);
      setLintPathKeyword(filterState.pathKeyword);
      setLintSuggestionKeyword(filterState.suggestionKeyword);
      setLintFilterStateLoaded(true);
    })();
  }, []);

  // ---- persist filter state to localStorage whenever it changes ----
  useEffect(() => {
    if (!lintFilterStateLoaded) {
      return;
    }
    writeLintFilterState({
      severity: lintSeverityFilter,
      codeKeyword: lintCodeKeyword,
      pathKeyword: lintPathKeyword,
      suggestionKeyword: lintSuggestionKeyword,
    });
  }, [lintCodeKeyword, lintFilterStateLoaded, lintPathKeyword, lintSeverityFilter, lintSuggestionKeyword]);

  // ---- derived/computed values ----
  const lintSeverityStats = resolveLintSeverityStats(lintReport);
  const lintIssues = lintReport?.issues ?? [];
  const lintIssuesCount = lintIssues.length;
  const lintCodeKeywordNormalized = lintCodeKeyword.trim();
  const lintPathKeywordNormalized = lintPathKeyword.trim();
  const lintSuggestionKeywordNormalized = lintSuggestionKeyword.trim();
  const lintSeverityFilteredIssues = filterLintIssuesBySeverity(lintIssues, lintSeverityFilter);
  const lintCodeFilteredIssues = filterLintIssuesByCode(lintIssues, lintCodeKeywordNormalized);
  const lintPathFilteredIssues = filterLintIssuesByPath(lintIssues, lintPathKeywordNormalized);
  const lintSuggestionFilteredIssues = filterLintIssuesBySuggestion(lintIssues, lintSuggestionKeywordNormalized);
  const filteredLintIssues = filterLintIssuesBySuggestion(
    filterLintIssuesByPath(
      filterLintIssuesByCode(lintSeverityFilteredIssues, lintCodeKeywordNormalized),
      lintPathKeywordNormalized,
    ),
    lintSuggestionKeywordNormalized,
  );
  const filteredLintIssuesCount = filteredLintIssues.length;
  const groupedLintIssues = useMemo(() => groupLintIssuesByPath(filteredLintIssues), [filteredLintIssues]);
  const groupedLintPatchPreviewItems = useMemo(
    () => groupPatchPreviewItemsByPath(lintPatchPreviewItems),
    [lintPatchPreviewItems],
  );
  const lintHasSeverityHit = lintSeverityFilteredIssues.length > 0;
  const lintHasCodeHit = lintCodeFilteredIssues.length > 0;
  const lintHasPathHit = lintPathFilteredIssues.length > 0;
  const lintHasSuggestionHit = lintSuggestionFilteredIssues.length > 0;
  const lintEmptyFilterLabels = [
    !lintHasSeverityHit ? "严重级别" : null,
    !lintHasCodeHit ? "code 关键词" : null,
    !lintHasPathHit ? "path 关键词" : null,
    !lintHasSuggestionHit ? "suggestion 关键词" : null,
  ].filter(Boolean) as string[];
  const lintFilterEmptyText =
    lintIssues.length === 0
      ? "本次 lint 检查未发现问题。"
      : lintEmptyFilterLabels.length === 1
        ? `当前筛选的${lintEmptyFilterLabels[0]}没有命中任何问题。`
        : lintEmptyFilterLabels.length > 1
          ? `当前筛选的${lintEmptyFilterLabels.join("、")}组合后没有命中任何问题。`
          : "当前筛选条件没有命中任何问题。";

  // ---- handlers ----
  const refreshRecentLintPatchEvents = async () => {
    const events = await fetchRecentLintPatchEvents();
    setRecentLintPatchEvents(events);
  };

  const handleRunLint = async (): Promise<boolean> => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法运行 Lint。");
      return false;
    }

    setLintRunning(true);
    setLintPatchPreviewItems([]);
    setLintPatchPreviewError("");
    setStatusMessage("");

    try {
      const report = await runLint();
      if (!report) {
        setStatusMessage("当前环境不支持运行 Lint。");
        return false;
      }

      setLintReport(report);
      await onRefreshAppData();
      setStatusMessage(`Lint 已完成：${report.summary}`);
      return true;
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`Lint 运行失败：${message}`);
      return false;
    } finally {
      setLintRunning(false);
    }
  };

  const handleClearLintFilters = () => {
    setLintSeverityFilter("all");
    setLintCodeKeyword("");
    setLintPathKeyword("");
    setLintSuggestionKeyword("");
  };

  const handlePreviewLintPatches = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法生成补丁建议。");
      return;
    }
    if (!lintReport) {
      setStatusMessage("请先运行 Lint，再生成补丁建议。");
      return;
    }

    setLintPatchPreviewLoading(true);
    setLintPatchPreviewError("");
    setStatusMessage("");

    try {
      const items = await previewLintPatches();
      if (!items) {
        setStatusMessage("当前环境不支持生成补丁建议。");
        setLintPatchPreviewItems([]);
        setLintPatchBatchSummary(null);
        return;
      }

      setLintPatchPreviewItems(items);
      setLintPatchBatchSummary(null);
      setStatusMessage(`补丁建议已生成：${items.length} 项。`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setLintPatchPreviewError(`生成补丁建议失败：${message}`);
      setLintPatchPreviewItems([]);
      setLintPatchBatchSummary(null);
    } finally {
      setLintPatchPreviewLoading(false);
    }
  };

  const handleApplyLintPatch = async (item: LintPatchPreviewItem) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法应用补丁建议。");
      return;
    }

    const patchKey = `${item.issue_code}-${item.path ?? "global"}`;
    setLintPatchApplyingKey(patchKey);
    setStatusMessage("");

    try {
      const result = await applyLintPatch(item);
      if (!result) {
        setStatusMessage("当前环境不支持应用补丁建议。");
        return;
      }

      await refreshRecentLintPatchEvents();
      const lintRefreshed = await handleRunLint();
      if (!lintRefreshed) {
        return;
      }

      const resultMessage = result.message?.trim();
      if (result.applied === false) {
        setStatusMessage(
          resultMessage
            ? `补丁建议已处理（无实际改动）：${resultMessage}`
            : `补丁建议已处理（无实际改动）：${item.issue_code}。`,
        );
      } else {
        setStatusMessage(
          resultMessage
            ? `补丁建议已应用：${resultMessage}`
            : `补丁建议已应用：${item.issue_code}。已刷新概览、日志和 Lint。`,
        );
      }
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`应用建议失败：${message}`);
    } finally {
      setLintPatchApplyingKey(null);
    }
  };

  const handleApplyLintPatchesBatch = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法批量应用补丁建议。");
      return;
    }

    if (!lintPatchPreviewItems.length) {
      setStatusMessage("当前没有可批量应用的补丁建议。");
      return;
    }

    setLintPatchBatchApplying(true);
    setStatusMessage("");

    try {
      const result = await applyLintPatchesBatch(lintPatchPreviewItems);
      if (!result) {
        setStatusMessage("当前环境不支持批量应用补丁建议。");
        return;
      }

      setLintPatchBatchSummary(result);
      await refreshRecentLintPatchEvents();
      const lintRefreshed = await handleRunLint();
      if (!lintRefreshed) {
        return;
      }

      const summaryText =
        result.summary?.trim() ||
        `成功 ${result.success_count}，失败 ${result.failure_count}，跳过 ${result.skipped_count}。`;
      setStatusMessage(`批量应用已完成：${summaryText}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`批量应用失败：${message}`);
    } finally {
      setLintPatchBatchApplying(false);
    }
  };

  const isTauri = isTauriRuntime();

  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">Lint</h1>
        <p className="module-header__sub">一致性检查、孤儿页与过期结论扫描</p>
      </div>
      <section className="panel">
        <div className="section-head">
          <h2>Lint 面板</h2>
          <span className="section-head__hint">
            {lintReport
              ? `${formatLintCheckedAt(lintReport.checked_at)} · ${lintIssuesCount} 个问题`
              : "尚未运行"}
          </span>
        </div>
        <div className="dev-panel__actions" style={{ marginBottom: "16px" }}>
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={() => void handleRunLint()}
            disabled={lintRunning}
          >
            {lintRunning ? "运行中..." : "运行 Lint"}
          </button>
          <button
            type="button"
            className="dev-panel__button"
            onClick={handleClearLintFilters}
            disabled={!lintFilterStateLoaded}
          >
            清空筛选
          </button>
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={() => void handlePreviewLintPatches()}
            disabled={!isTauri || lintPatchPreviewLoading || !lintReport}
          >
            {lintPatchPreviewLoading ? "生成中..." : "生成补丁建议"}
          </button>
        </div>
        {lintReport ? (
          <div className="lint-stats-row">
            <span className="lint-stat lint-stat--error">错误 {lintSeverityStats.error}</span>
            <span className="lint-stat lint-stat--warning">警告 {lintSeverityStats.warning}</span>
            <span className="lint-stat lint-stat--info">信息 {lintSeverityStats.info}</span>
            <span style={{ fontSize: "12px", color: "var(--text-muted)", alignSelf: "center" }}>
              {lintReport.summary}
            </span>
          </div>
        ) : null}
        {lintReport ? (
          <div className="lint-severity-tabs">
            {(["all", "error", "warning", "info"] as LintSeverityFilter[]).map((severity) => {
              const count =
                severity === "all" ? lintIssuesCount
                : severity === "error" ? lintSeverityStats.error
                : severity === "warning" ? lintSeverityStats.warning
                : lintSeverityStats.info;
              return (
                <button
                  key={severity}
                  type="button"
                  className={`lint-severity-tab${lintSeverityFilter === severity ? " lint-severity-tab--active" : ""}`}
                  onClick={() => setLintSeverityFilter(severity)}
                >
                  {lintSeverityFilterLabels[severity]} ({count})
                </button>
              );
            })}
          </div>
        ) : null}
        <div className="lint-filter-row">
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="lint-code-keyword">code 关键词</label>
            <input
              id="lint-code-keyword"
              className="dev-panel__input"
              type="text"
              value={lintCodeKeyword}
              onChange={(event) => setLintCodeKeyword(event.target.value)}
              placeholder="按 code 筛选"
              spellCheck={false}
            />
          </div>
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="lint-path-keyword">path 关键词</label>
            <input
              id="lint-path-keyword"
              className="dev-panel__input"
              type="text"
              value={lintPathKeyword}
              onChange={(event) => setLintPathKeyword(event.target.value)}
              placeholder="按 path 筛选"
              spellCheck={false}
            />
          </div>
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="lint-suggestion-keyword">suggestion 关键词</label>
            <input
              id="lint-suggestion-keyword"
              className="dev-panel__input"
              type="text"
              value={lintSuggestionKeyword}
              onChange={(event) => setLintSuggestionKeyword(event.target.value)}
              placeholder="按 suggestion 筛选"
              spellCheck={false}
            />
          </div>
        </div>
        {lintReport ? (
          filteredLintIssuesCount ? (
            <div className="lint-issue-list">
              {groupedLintIssues.map((group) => {
                const isCollapsed = lintCollapsedGroups.has(group.path);
                return (
                  <div key={group.path} className="lint-group">
                    {/* 分组标题行：路径 + 问题数 + 折叠切换 */}
                    <button
                      type="button"
                      className="lint-group__header"
                      onClick={() =>
                        setLintCollapsedGroups((prev) => {
                          const next = new Set(prev);
                          if (isCollapsed) {
                            next.delete(group.path);
                          } else {
                            next.add(group.path);
                          }
                          return next;
                        })
                      }
                    >
                      <span className="lint-group__arrow">{isCollapsed ? "▸" : "▾"}</span>
                      <code className="lint-group__path">{group.path}</code>
                      <span className="lint-group__count">{group.issues.length} 个问题</span>
                    </button>
                    {!isCollapsed && (
                      <div className="lint-group__body">
                        {group.issues.map((issue) => {
                          const severity = normalizeLintSeverity(issue.severity);
                          return (
                            <article
                              key={`${issue.code}-${issue.path ?? "global"}`}
                              className={`lint-issue lint-issue--${severity}`}
                            >
                              <div className="lint-issue__head">
                                <div className="lint-issue__code">{issue.code}</div>
                                <span className={`pill pill--lint pill--lint-${severity}`}>{severity}</span>
                              </div>
                              <p className="lint-issue__message">{issue.message}</p>
                              {issue.code === "BROKEN_WIKILINK" ? (
                                <div className="lint-issue__action">
                                  <p>
                                    页面 <strong>{issue.path}</strong> 引用了不存在的页面 <strong>{issue.target_page || "未知页面"}</strong>
                                  </p>
                                  <button
                                    type="button"
                                    onClick={() => void onCreateBrokenWikiLinkPage(issue.target_page || "新页面")}
                                  >
                                    创建 {issue.target_page ? `[${issue.target_page}]` : "页面"}
                                  </button>
                                </div>
                              ) : (
                                <div className="lint-issue__field">
                                  <span>建议</span>
                                  <p className="lint-issue__suggestion">{issue.suggestion}</p>
                                </div>
                              )}
                            </article>
                          );
                        })}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ) : (
            <p className="empty-state">{lintFilterEmptyText}</p>
          )
        ) : (
          <p className="empty-state">
            {isTauri
              ? "尚未运行 Lint。点击按钮后会在此展示报告摘要、检查时间和问题列表。"
              : "浏览器预览模式下不连接后端，无法生成真实 lint 报告。"}
          </p>
        )}
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>最近补丁记录</h2>
          <span className="section-head__hint">
            {recentLintPatchEvents.length ? `最近 ${recentLintPatchEvents.length} 条` : "暂无记录"}
          </span>
        </div>
        {recentLintPatchEvents.length ? (
          <div className="lint-patch-events">
            {recentLintPatchEvents.map((event) => (
              <article
                key={`${event.issue_code}-${event.path ?? "global"}-${event.created_at}`}
                className="lint-patch-event"
              >
                <div className="lint-patch-event__head">
                  <span className="lint-patch-event__code">{event.issue_code}</span>
                  <span className={`pill ${event.applied ? "pill--ok" : "pill--danger"}`}>
                    {event.applied ? "已应用" : "未应用"}
                  </span>
                  <time dateTime={event.created_at}>{formatLintCheckedAt(event.created_at)}</time>
                </div>
                <div className="lint-issue__field">
                  <span>path</span>
                  <code>{event.path ?? "全局"}</code>
                </div>
                <div className="lint-issue__field">
                  <span>message</span>
                  <p>{event.message || "无"}</p>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-state">
            {isTauri
              ? "尚无补丁应用记录。应用补丁后会在这里显示最近历史。"
              : "浏览器预览模式下不加载补丁应用记录。"}
          </p>
        )}
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>补丁建议</h2>
          <span className="section-head__hint">
            {lintPatchPreviewItems.length ? `${lintPatchPreviewItems.length} 项` : "暂无建议"}
          </span>
        </div>
        <div className="dev-panel__actions" style={{ marginBottom: "12px" }}>
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={() => void handleApplyLintPatchesBatch()}
            disabled={!isTauri || lintPatchBatchApplying || lintPatchPreviewItems.length === 0}
          >
            {lintPatchBatchApplying ? "批量应用中..." : "批量应用可应用项"}
          </button>
          {lintPatchBatchSummary ? (
            <span className="pill pill--ok">
              {lintPatchBatchSummary.summary?.trim() ||
                `成功 ${lintPatchBatchSummary.success_count} · 失败 ${lintPatchBatchSummary.failure_count} · 跳过 ${lintPatchBatchSummary.skipped_count}`}
            </span>
          ) : null}
        </div>
        {lintPatchPreviewError ? <p className="runtime-status">{lintPatchPreviewError}</p> : null}
        {lintPatchPreviewItems.length ? (
          <div className="lint-issue-list">
            {groupedLintPatchPreviewItems.map((group) => {
              const isCollapsed = patchPreviewCollapsedGroups.has(group.path);
              return (
                <div key={group.path} className="lint-group">
                  {/* 分组标题行：路径 + 补丁数 + 折叠切换 */}
                  <button
                    type="button"
                    className="lint-group__header"
                    onClick={() =>
                      setPatchPreviewCollapsedGroups((prev) => {
                        const next = new Set(prev);
                        if (isCollapsed) {
                          next.delete(group.path);
                        } else {
                          next.add(group.path);
                        }
                        return next;
                      })
                    }
                  >
                    <span className="lint-group__arrow">{isCollapsed ? "▸" : "▾"}</span>
                    <code className="lint-group__path">{group.path}</code>
                    <span className="lint-group__count">{group.items.length} 个建议</span>
                  </button>
                  {!isCollapsed && (
                    <div className="lint-group__body">
                      {group.items.map((item) => (
                        <article key={`${item.issue_code}-${item.path ?? "global"}`} className="lint-issue">
                          <div className="lint-issue__head">
                            <div className="lint-issue__code">{item.issue_code}</div>
                            <span className="pill pill--lint pill--lint-info">suggestion</span>
                          </div>
                          <p className="lint-issue__message">{item.title}</p>
                          <div className="lint-issue__field">
                            <span>建议动作</span>
                            <p className="lint-issue__suggestion">{item.proposed_action}</p>
                          </div>
                          <div className="lint-issue__field">
                            <span>路径</span>
                            <code>{item.path ?? "全局"}</code>
                          </div>
                          <div className="lint-issue__field">
                            <span>补丁预览</span>
                            <pre className="wiki-preview__content">{item.patch_preview}</pre>
                          </div>
                          <div className="lint-issue__actions">
                            <button
                              type="button"
                              className="dev-panel__button dev-panel__button--accent"
                              onClick={() => void handleApplyLintPatch(item)}
                              disabled={!isTauri || lintPatchApplyingKey !== null}
                            >
                              {lintPatchApplyingKey === `${item.issue_code}-${item.path ?? "global"}`
                                ? "应用中..."
                                : "应用建议"}
                            </button>
                            {item.path != null && (
                              <button
                                type="button"
                                className="dev-panel__button"
                                onClick={() => void onOpenPatchPage(item.path!)}
                              >
                                打开页面
                              </button>
                            )}
                          </div>
                        </article>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : lintPatchPreviewLoading ? (
          <p className="runtime-hint">正在生成补丁建议...</p>
        ) : (
          <p className="empty-state">
            {lintReport ? '点击"生成补丁建议"后在此查看候选补丁预览。' : "请先运行 Lint，再生成补丁建议。"}
          </p>
        )}
      </section>
    </>
  );
}
