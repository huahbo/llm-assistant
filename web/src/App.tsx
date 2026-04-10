import { useEffect, useState } from "react";
import {
  fetchAppOverview,
  fetchDefaultPaths,
  fetchQuerySettings,
  fetchRecentLogs,
  initVault,
  ingestMarkdown,
  isTauriRuntime,
  queryAskWithOptions,
  runLint,
  saveQueryAnswer,
  setBackendMode,
  setQueryTopK as persistQueryTopK,
} from "./tauri-client";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import { formatLintCheckedAt, normalizeLintSeverity } from "./lint-utils";
import type {
  AppOverview,
  BackendAppMode,
  LintReport,
  LogEntry,
  ModuleItem,
  ModeId,
  ModeOption,
  QueryAnswerResult,
} from "./types";

const defaultVaultPath = "vault";
const defaultIngestSourcePath = "README.md";
const defaultQueryTopKMin = 1;
const defaultQueryTopKMax = 8;
const defaultQueryTopK = 3;

const modes: ModeOption[] = [
  {
    id: "hybrid",
    name: "Hybrid",
    description: "本地优先，必要时可路由到云 Provider。",
    badge: "默认",
  },
  {
    id: "strict-local",
    name: "Strict Local",
    description: "仅允许本地 Ollama，阻断所有云调用。",
    badge: "受限",
  },
];

const modeIdToBackendMode: Record<ModeId, BackendAppMode> = {
  hybrid: "Hybrid",
  "strict-local": "StrictLocal",
};

const modules: ModuleItem[] = [
  { id: "inbox", name: "Inbox", description: "收集资料、待处理输入与任务入口。" },
  { id: "wiki", name: "Wiki", description: "Markdown Vault 的页面编辑与浏览。" },
  { id: "ask", name: "Ask", description: "基于索引与引用证据的问答入口。" },
  { id: "lint", name: "Lint", description: "一致性检查、孤儿页与过期结论扫描。" },
  { id: "settings", name: "Settings", description: "模式、Provider 与本地配置。" },
];

type DevAction = "init_vault" | "ingest_markdown";

type LoadResult = {
  overview: AppOverview | null;
  logs: LogEntry[];
};

const loadAppData = async (): Promise<LoadResult> => {
  const [overviewResult, logsResult] = await Promise.allSettled([
    fetchAppOverview(),
    fetchRecentLogs(),
  ]);

  return {
    overview: overviewResult.status === "fulfilled" ? overviewResult.value : null,
    logs: logsResult.status === "fulfilled" ? logsResult.value : [],
  };
};

export default function App() {
  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [lintReport, setLintReport] = useState<LintReport | null>(null);
  const [queryResult, setQueryResult] = useState<QueryAnswerResult | null>(null);
  const [statusMessage, setStatusMessage] = useState("");
  const [switchingMode, setSwitchingMode] = useState<ModeId | null>(null);
  const [devAction, setDevAction] = useState<DevAction | null>(null);
  const [lintRunning, setLintRunning] = useState(false);
  const [queryRunning, setQueryRunning] = useState(false);
  const [vaultPath, setVaultPath] = useState(defaultVaultPath);
  const [ingestSourcePath, setIngestSourcePath] = useState(defaultIngestSourcePath);
  const [queryQuestion, setQueryQuestion] = useState("这个项目的核心目标是什么？");
  const [queryTopK, setQueryTopK] = useState(defaultQueryTopK);
  const [queryTopKMin, setQueryTopKMin] = useState(defaultQueryTopKMin);
  const [queryTopKMax, setQueryTopKMax] = useState(defaultQueryTopKMax);
  const [querySettingsSaving, setQuerySettingsSaving] = useState(false);
  const [queryResultSaving, setQueryResultSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [data, defaultPaths, querySettings] = await Promise.all([
        loadAppData(),
        fetchDefaultPaths(),
        fetchQuerySettings(),
      ]);

      if (!cancelled) {
        setOverview(data.overview);
        setLogs(data.logs);
        if (defaultPaths) {
          setVaultPath(defaultPaths.vault_path);
          setIngestSourcePath(defaultPaths.ingest_source_path);
        }
        if (querySettings) {
          setQueryTopK(querySettings.top_k);
          setQueryTopKMin(querySettings.min_top_k);
          setQueryTopKMax(querySettings.max_top_k);
        }
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const refreshAppData = async () => {
    const data = await loadAppData();
    setOverview(data.overview);
    setLogs(data.logs);
  };

  const handleModeSelect = async (modeId: ModeId) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法切换运行模式。");
      return;
    }

    if (!overview) {
      return;
    }

    const nextMode = modeIdToBackendMode[modeId];
    if (overview.mode === nextMode) {
      return;
    }

    setSwitchingMode(modeId);
    setStatusMessage("");

    try {
      const result = await setBackendMode(nextMode);
      if (!result) {
        setStatusMessage("当前环境不支持运行模式切换。");
        return;
      }

      await refreshAppData();
      setStatusMessage(`已切换到 ${formatBackendMode(result.current_mode)}。`);
    } catch (error) {
      console.error(error);
      setStatusMessage("模式切换失败，请稍后重试。");
    } finally {
      setSwitchingMode(null);
    }
  };

  const handleInitVault = async () => {
    setStatusMessage("收到初始化请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法初始化 Vault。");
      return;
    }

    const nextVaultPath = vaultPath.trim() || defaultVaultPath;
    setDevAction("init_vault");
    setStatusMessage("");

    try {
      const result = await initVault(nextVaultPath);
      if (!result) {
        setStatusMessage("当前环境不支持 Vault 初始化。");
        return;
      }

      await refreshAppData();
      setStatusMessage(result.message || `Vault 已初始化：${result.vault_path}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`Vault 初始化失败：${message}`);
    } finally {
      setDevAction(null);
    }
  };

  const handleDemoIngest = async () => {
    setStatusMessage("收到摄入请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行示例摄入。");
      return;
    }

    const nextSourcePath = ingestSourcePath.trim() || defaultIngestSourcePath;
    setDevAction("ingest_markdown");
    setStatusMessage("");

    try {
      const result = await ingestMarkdown(nextSourcePath);
      if (!result) {
        setStatusMessage("当前环境不支持示例摄入。");
        return;
      }

      await refreshAppData();
      setStatusMessage(result.message || `已处理 ${result.source_path}。`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`示例摄入失败：${message}`);
    } finally {
      setDevAction(null);
    }
  };

  const handleRunLint = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法运行 Lint。");
      return;
    }

    setLintRunning(true);
    setStatusMessage("");

    try {
      const report = await runLint();
      if (!report) {
        setStatusMessage("当前环境不支持运行 Lint。");
        return;
      }

      setLintReport(report);
      await refreshAppData();
      setStatusMessage(`Lint 已完成：${report.summary}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`Lint 运行失败：${message}`);
    } finally {
      setLintRunning(false);
    }
  };

  const handleQueryAsk = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行查询。");
      return;
    }

    const nextQuestion = queryQuestion.trim();
    if (!nextQuestion) {
      setStatusMessage("请输入问题后再查询。");
      return;
    }
    const nextTopK = Math.min(
      queryTopKMax,
      Math.max(queryTopKMin, Math.trunc(queryTopK || defaultQueryTopK)),
    );
    setQueryTopK(nextTopK);

    setQueryRunning(true);
    setStatusMessage("");

    try {
      const result = await queryAskWithOptions(nextQuestion, { top_k: nextTopK });
      if (!result) {
        setStatusMessage("当前环境不支持查询。");
        return;
      }

      setQueryResult(result);
      // Query 会在后端写入日志，这里主动刷新一次前端日志面板。
      await refreshAppData();
      setStatusMessage(`Query 已完成：TopK=${nextTopK}，命中 ${result.matched_pages.length} 页。`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`Query 失败：${message}`);
    } finally {
      setQueryRunning(false);
    }
  };

  const handleSaveQuerySettings = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法保存 Query 参数。");
      return;
    }

    const nextTopK = Math.min(
      queryTopKMax,
      Math.max(queryTopKMin, Math.trunc(queryTopK || defaultQueryTopK)),
    );

    setQuerySettingsSaving(true);
    setStatusMessage("");

    try {
      const settings = await persistQueryTopK(nextTopK);
      if (!settings) {
        setStatusMessage("当前环境不支持保存 Query 参数。");
        return;
      }

      setQueryTopK(settings.top_k);
      setQueryTopKMin(settings.min_top_k);
      setQueryTopKMax(settings.max_top_k);
      await refreshAppData();
      setStatusMessage(`Query 参数已保存：TopK=${settings.top_k}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`保存 Query 参数失败：${message}`);
    } finally {
      setQuerySettingsSaving(false);
    }
  };

  const handleSaveQueryResult = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法保存 Query 结果。");
      return;
    }
    if (!queryResult) {
      setStatusMessage("请先执行 Query，再保存结果。");
      return;
    }

    setQueryResultSaving(true);
    setStatusMessage("");

    try {
      const result = await saveQueryAnswer({
        question: queryResult.question,
        answer: queryResult.answer,
        citations: queryResult.citations,
      });
      if (!result) {
        setStatusMessage("当前环境不支持保存 Query 结果。");
        return;
      }

      await refreshAppData();
      setStatusMessage(`${result.message}：${result.wiki_path}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`保存 Query 结果失败：${message}`);
    } finally {
      setQueryResultSaving(false);
    }
  };

  return (
    <main className="app-shell">
      <section className="hero">
        <div className="hero__copy">
          <p className="eyebrow">Windows 优先 · 本地优先 · Tauri + React + SQLite</p>
          <h1>LLM Wiki</h1>
          <p className="hero__lead">
            面向 Markdown Vault 的个人知识桌面骨架，支持 ingest、query、lint 三类核心工作流。
          </p>
        </div>
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>运行模式</h2>
          <span className="section-head__hint">
            {overview ? overview.supported_modes.map(formatBackendMode).join(" / ") : "浏览器预览"}
          </span>
        </div>
        <div className="runtime-banner">
          <div className="runtime-banner__item">
            <span>当前后端模式</span>
            <strong>{overview ? formatBackendMode(overview.mode) : "Browser Preview"}</strong>
          </div>
          <div className="runtime-banner__item">
            <span>Vault 路径</span>
            <strong>{overview?.vault_path ?? "未连接 Tauri"}</strong>
          </div>
        </div>
        {overview ? (
          <p className="runtime-hint">
            当前运行模式：<strong>{formatBackendMode(overview.mode)}</strong>，最近日志
            <strong> {overview.recent_log_count}</strong> 条。
          </p>
        ) : (
          <p className="runtime-hint">当前处于浏览器骨架预览模式。</p>
        )}
        {statusMessage ? <p className="runtime-status">{statusMessage}</p> : null}
        <div className="mode-grid">
          {modes.map((mode) => (
            <button
              key={mode.id}
              type="button"
              className={`mode-card ${
                overview?.mode === modeIdToBackendMode[mode.id] ? "mode-card--active" : ""
              }`}
              onClick={() => void handleModeSelect(mode.id)}
              disabled={!isTauriRuntime() || switchingMode !== null}
              aria-pressed={overview?.mode === modeIdToBackendMode[mode.id]}
            >
              <div className="mode-card__top">
                <h3>{mode.name}</h3>
                <span className="pill">{mode.badge}</span>
              </div>
              <p>{mode.description}</p>
            </button>
          ))}
        </div>
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>开发操作</h2>
          <span className="section-head__hint">{isTauriRuntime() ? "Tauri 可用" : "浏览器预览"}</span>
        </div>
        <div className="dev-panel">
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="vault-path">
              Vault 路径
            </label>
            <input
              id="vault-path"
              className="dev-panel__input"
              type="text"
              value={vaultPath}
              onChange={(event) => setVaultPath(event.target.value)}
              placeholder={defaultVaultPath}
              spellCheck={false}
            />
          </div>
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="ingest-source-path">
              示例摄入文件
            </label>
            <input
              id="ingest-source-path"
              className="dev-panel__input"
              type="text"
              value={ingestSourcePath}
              onChange={(event) => setIngestSourcePath(event.target.value)}
              placeholder={defaultIngestSourcePath}
              spellCheck={false}
            />
          </div>
          <div className="dev-panel__actions">
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void handleInitVault()}
              disabled={!isTauriRuntime() || devAction !== null}
            >
              {devAction === "init_vault" ? "初始化中..." : "初始化 Vault"}
            </button>
            <button
              type="button"
              className="dev-panel__button dev-panel__button--accent"
              onClick={() => void handleDemoIngest()}
              disabled={!isTauriRuntime() || devAction !== null}
            >
              {devAction === "ingest_markdown" ? "摄入中..." : "示例摄入"}
            </button>
          </div>
        </div>
        <p className="dev-panel__hint">
          {isTauriRuntime()
            ? "按钮会调用本地 Tauri 命令，成功后自动刷新运行概览和最近日志。"
            : "浏览器预览模式下按钮保持禁用，仅用于界面预览。"}
        </p>
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>Ask 面板</h2>
          <span className="section-head__hint">
            {isTauriRuntime() ? "可调用 query_ask" : "浏览器预览"}
          </span>
        </div>
        <div className="ask-panel">
          <label className="dev-panel__label" htmlFor="ask-question">
            问题
          </label>
          <textarea
            id="ask-question"
            className="ask-panel__textarea"
            value={queryQuestion}
            onChange={(event) => setQueryQuestion(event.target.value)}
            placeholder="输入你要检索的问题"
          />
          <div className="ask-panel__options">
            <label className="dev-panel__label" htmlFor="ask-top-k">
              TopK（{queryTopKMin}-{queryTopKMax}）
            </label>
            <input
              id="ask-top-k"
              className="dev-panel__input ask-panel__topk"
              type="number"
              min={queryTopKMin}
              max={queryTopKMax}
              step={1}
              value={queryTopK}
              onChange={(event) => setQueryTopK(Number(event.target.value))}
            />
          </div>
          <div className="ask-panel__actions">
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void handleSaveQuerySettings()}
              disabled={!isTauriRuntime() || querySettingsSaving}
            >
              {querySettingsSaving ? "保存中..." : "保存参数"}
            </button>
            <button
              type="button"
              className="dev-panel__button dev-panel__button--accent"
              onClick={() => void handleQueryAsk()}
              disabled={!isTauriRuntime() || queryRunning}
            >
              {queryRunning ? "检索中..." : "执行 Query"}
            </button>
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void handleSaveQueryResult()}
              disabled={!isTauriRuntime() || queryResultSaving || !queryResult}
            >
              {queryResultSaving ? "保存中..." : "保存回答到 Wiki"}
            </button>
          </div>
        </div>
        {queryResult ? (
          <div className="ask-result">
            <div className="ask-result__meta">
              <span>模式：{formatBackendMode(queryResult.mode)}</span>
              <span>TopK：{queryTopK}</span>
              <span>命中：{queryResult.matched_pages.length}</span>
              <span>时间：{formatLintCheckedAt(queryResult.checked_at)}</span>
            </div>
            <pre className="ask-result__answer">{queryResult.answer}</pre>
            <div className="ask-result__citations">
              {queryResult.citations.map((citation) => (
                <article key={`${citation.page_path}-${citation.score}`} className="ask-citation">
                  <div className="ask-citation__top">
                    <code>{citation.page_path}</code>
                    <span>score: {citation.score}</span>
                  </div>
                  <p>{citation.excerpt}</p>
                </article>
              ))}
            </div>
          </div>
        ) : (
          <p className="empty-state">
            {isTauriRuntime()
              ? "尚未执行 Query。输入问题后点击“执行 Query”查看本地检索结果。"
              : "浏览器预览模式下不连接后端，无法生成真实问答结果。"}
          </p>
        )}
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>Lint 面板</h2>
          <span className="section-head__hint">
            {isTauriRuntime() ? "可调用 run_lint" : "浏览器预览"}
          </span>
        </div>
        <div className="lint-panel__actions">
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={() => void handleRunLint()}
            disabled={lintRunning}
          >
            {lintRunning ? "运行中..." : "运行 Lint"}
          </button>
          <p className="lint-panel__note">
            {isTauriRuntime()
              ? "按钮会调用本地 run_lint 命令并刷新摘要、时间与问题列表。"
              : "浏览器预览模式下可查看界面结构，点击仅更新状态提示。"}
          </p>
        </div>
        <div className="runtime-banner lint-panel__summary">
          <div className="runtime-banner__item">
            <span>报告摘要</span>
            <strong>{lintReport?.summary ?? "尚未运行 Lint"}</strong>
          </div>
          <div className="runtime-banner__item">
            <span>检查时间</span>
            <strong>{lintReport ? formatLintCheckedAt(lintReport.checked_at) : "尚未运行 Lint"}</strong>
          </div>
          <div className="runtime-banner__item">
            <span>问题数量</span>
            <strong>{lintReport ? lintReport.issues.length : 0}</strong>
          </div>
        </div>
        {lintReport ? (
          lintReport.issues.length ? (
            <div className="lint-issue-list">
              {lintReport.issues.map((issue) => {
                const severity = normalizeLintSeverity(issue.severity);

                return (
                  <article key={`${issue.code}-${issue.path ?? "global"}`} className={`lint-issue lint-issue--${severity}`}>
                    <div className="lint-issue__head">
                      <div className="lint-issue__code">{issue.code}</div>
                      <span className={`pill pill--lint pill--lint-${severity}`}>{severity}</span>
                    </div>
                    <p className="lint-issue__message">{issue.message}</p>
                    <div className="lint-issue__field">
                      <span>路径</span>
                      <code>{issue.path ?? "全局"}</code>
                    </div>
                    <div className="lint-issue__field">
                      <span>建议</span>
                      <p className="lint-issue__suggestion">{issue.suggestion}</p>
                    </div>
                  </article>
                );
              })}
            </div>
          ) : (
            <p className="empty-state">本次 lint 检查未发现问题。</p>
          )
        ) : (
          <p className="empty-state">
            {isTauriRuntime()
              ? "尚未运行 Lint。点击按钮后会在此展示报告摘要、检查时间和问题列表。"
              : "浏览器预览模式下不连接后端，无法生成真实 lint 报告。"}
          </p>
        )}
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>最近日志</h2>
          <span className="section-head__hint">{logs.length ? `最近 ${logs.length} 条` : "暂无日志"}</span>
        </div>
        {logs.length ? (
          <div className="log-list">
            {logs.map((log) => (
              <article key={log.id} className={`log-item log-item--${log.level.toLowerCase()}`}>
                <div className="log-item__head">
                  <span className="log-item__level">{formatLogLevel(log.level)}</span>
                  <time dateTime={log.created_at}>{log.created_at}</time>
                </div>
                <p>{log.message}</p>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-state">
            {isTauriRuntime()
              ? "后端尚未返回最近日志。"
              : "浏览器预览模式下不加载 Tauri 日志。"}
          </p>
        )}
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>核心模块</h2>
          <span className="section-head__hint">功能占位</span>
        </div>
        <div className="module-grid">
          {modules.map((module) => (
            <article key={module.id} className="module-card">
              <div className="module-card__index">{module.id.toUpperCase()}</div>
              <h3>{module.name}</h3>
              <p>{module.description}</p>
            </article>
          ))}
        </div>
      </section>
    </main>
  );
}
