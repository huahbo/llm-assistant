import { useEffect, useMemo, useRef, useState } from "react";
import { formatBackendMode } from "../../app-formatters";
import { useToast } from "../../contexts/ToastContext";
import { useRuntime } from "../../contexts/RuntimeContext";
import {
  fetchQuerySettings,
  fetchAskHistory,
  listAskSessions,
  createAskSession,
  fetchAskSessionTurns,
  searchAskSessionTurns,
  renameAskSession,
  deleteAskSession,
  isTauriRuntime,
  queryAskSession,
  cancelAskSession,
  clearAskHistory,
  saveAskHistory,
  saveQueryAnswer,
  listenProgress,
  pickSaveFile,
  saveResearchDoc,
  askConfirmDialog,
  resolveDisplayPath,
  setQueryTopK as persistQueryTopK,
} from "../../tauri-client";
import type {
  AskHistoryItem,
  AskSessionItem,
  AskSessionSearchHitItem,
  AskSessionTurnItem,
  BackendAppMode,
  QueryAnswerResult,
  QueryCitation,
  QuerySearchDebug,
} from "../../types";
import {
  filterQueryHistoryItems,
  filterAskSessions,
  formatAskHistoryCreatedAt,
  formatAskSessionSearchSnippet,
  normalizeQueryHistoryItems,
  readQueryHistoryItemsFromStorage,
  readAskSearchDebugVisibleFromStorage,
  writeAskSearchDebugVisibleToStorage,
  writeQueryHistoryItemsToStorage,
  mergeQueryHistoryItems,
  buildAskSessionExportMarkdown,
  parseQueryProgressPayload,
  QUERY_HISTORY_MAX,
} from "../../ask-utils";
import { formatQueryAnswerStrategyLabel, formatQuerySearchStrategyLabel } from "../../llm-utils";

// ─── Constants ──────────────────────────────────────────────────────────────

const defaultQueryTopKMin = 1;
const defaultQueryTopKMax = 8;
const defaultQueryTopK = 3;
const ASK_SESSION_LIST_MAX = 80;
const ASK_SESSION_SEARCH_LIMIT = 80;

// ─── Types ───────────────────────────────────────────────────────────────────

type AskMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
  citations?: QueryCitation[];
  meta?: {
    mode: BackendAppMode;
    searchStrategy?: string | null;
    answerStrategy?: string | null;
    topK: number;
    matchedPages: number;
    searchDebug?: QuerySearchDebug | null;
  };
};

// ─── Label helpers ────────────────────────────────────────────────────────────

const searchRouteLabels: Record<string, string> = {
  fts: "FTS",
  linked: "链接扩展",
  popular: "引用热度",
  embedding: "Embedding",
  scan: "文件扫描",
};

const formatQuerySearchRouteLabel = (route?: string | null): string => {
  const normalizedRoute = route?.trim().toLowerCase();
  if (!normalizedRoute) {
    return "未知路径";
  }
  return searchRouteLabels[normalizedRoute] ?? normalizedRoute;
};

// ─── Clipboard helper ─────────────────────────────────────────────────────────

const copyTextToClipboard = async (text: string): Promise<boolean> => {
  if (navigator.clipboard && window.isSecureContext) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // fall through to execCommand
    }
  }
  try {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    textarea.style.top = "-9999px";
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    const copied = document.execCommand("copy");
    document.body.removeChild(textarea);
    return copied;
  } catch {
    return false;
  }
};

// ─── Props ────────────────────────────────────────────────────────────────────

type AskModuleProps = {
  /** Called when the user saves a query result and we should navigate to wiki */
  onOpenWikiPage?: (path: string) => void | Promise<void>;
};

// ─── Component ───────────────────────────────────────────────────────────────

export default function AskModule({ onOpenWikiPage }: AskModuleProps) {
  const { setStatusMessage } = useToast();
  const { isTauri } = useRuntime();

  // ── State ──────────────────────────────────────────────────────────────────
  const [queryResult, setQueryResult] = useState<QueryAnswerResult | null>(null);
  const [queryRunning, setQueryRunning] = useState(false);
  const [queryQuestion, setQueryQuestion] = useState("");
  const [queryTopK, setQueryTopK] = useState(defaultQueryTopK);
  const [queryTopKMin, setQueryTopKMin] = useState(defaultQueryTopKMin);
  const [queryTopKMax, setQueryTopKMax] = useState(defaultQueryTopKMax);
  const [querySettingsSaving, setQuerySettingsSaving] = useState(false);
  const [queryResultSaving, setQueryResultSaving] = useState(false);
  const [queryHistoryItems, setQueryHistoryItems] = useState<AskHistoryItem[]>(() =>
    readQueryHistoryItemsFromStorage(),
  );
  const [askSearchDebugVisible, setAskSearchDebugVisible] = useState<boolean>(() =>
    readAskSearchDebugVisibleFromStorage(),
  );
  const [searchDebugCopiedMessageId, setSearchDebugCopiedMessageId] = useState("");
  const [askHistoryKeyword, setAskHistoryKeyword] = useState("");
  const [askSessions, setAskSessions] = useState<AskSessionItem[]>([]);
  const [askSessionsLoading, setAskSessionsLoading] = useState(false);
  const [askSessionKeyword, setAskSessionKeyword] = useState("");
  const [askSessionSearchKeyword, setAskSessionSearchKeyword] = useState("");
  const [askSessionSearchHits, setAskSessionSearchHits] = useState<AskSessionSearchHitItem[]>([]);
  const [askSessionSearching, setAskSessionSearching] = useState(false);
  const [askSessionManaging, setAskSessionManaging] = useState(false);
  const [askFocusedMessageId, setAskFocusedMessageId] = useState("");
  const [askMessages, setAskMessages] = useState<AskMessage[]>([]);
  const [askSessionId, setAskSessionId] = useState<string>(() => crypto.randomUUID());
  const [showAskAdvanced, setShowAskAdvanced] = useState(false);
  const [expandedCitationIds, setExpandedCitationIds] = useState<Set<string>>(new Set());

  // ── Refs ───────────────────────────────────────────────────────────────────
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const askFocusTimerRef = useRef<number | null>(null);

  // ── Derived / Memos ────────────────────────────────────────────────────────
  const filteredQueryHistoryItems = useMemo(
    () => filterQueryHistoryItems(queryHistoryItems, askHistoryKeyword),
    [queryHistoryItems, askHistoryKeyword],
  );
  const filteredAskSessions = useMemo(
    () => filterAskSessions(askSessions, askSessionKeyword),
    [askSessions, askSessionKeyword],
  );

  // ── Effects ────────────────────────────────────────────────────────────────

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [askMessages]);

  // Persist askSearchDebugVisible to localStorage
  useEffect(() => {
    writeAskSearchDebugVisibleToStorage(askSearchDebugVisible);
  }, [askSearchDebugVisible]);

  // Initial load: fetch query settings + ask history + sessions
  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [querySettings, dbAskHistory, dbAskSessions] = await Promise.all([
        fetchQuerySettings(),
        fetchAskHistory(QUERY_HISTORY_MAX),
        listAskSessions(ASK_SESSION_LIST_MAX),
      ]);

      if (cancelled) return;

      if (querySettings) {
        setQueryTopK(querySettings.top_k);
        setQueryTopKMin(querySettings.min_top_k);
        setQueryTopKMax(querySettings.max_top_k);
      }

      // Ask history: prefer backend DB, fall back to localStorage
      if (dbAskHistory) {
        const normalized = normalizeQueryHistoryItems(dbAskHistory, QUERY_HISTORY_MAX);
        setQueryHistoryItems(normalized);
        writeQueryHistoryItemsToStorage(normalized, QUERY_HISTORY_MAX);
      } else {
        setQueryHistoryItems(readQueryHistoryItemsFromStorage());
      }

      // Ask sessions
      const normalizedSessions = dbAskSessions ?? [];
      if (normalizedSessions.length > 0) {
        setAskSessions(normalizedSessions);
        const currentId = askSessionId;
        const activeSession =
          normalizedSessions.find((item) => item.session_id === currentId) ??
          normalizedSessions[0];
        setAskSessionId(activeSession.session_id);
        const turns = await fetchAskSessionTurns(activeSession.session_id, 400);
        if (!cancelled) {
          setAskMessages(buildAskMessagesFromSessionTurns(turns ?? []));
        }
      } else if (isTauriRuntime()) {
        const currentId = askSessionId || crypto.randomUUID();
        const created = await createAskSession(currentId, "新对话");
        if (!cancelled && created) {
          setAskSessions([created]);
          setAskSessionId(created.session_id);
          setAskMessages([]);
        }
      } else {
        setAskSessions([]);
        setAskMessages([]);
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ── Handlers ───────────────────────────────────────────────────────────────

  const buildAskMessagesFromSessionTurns = (turns: AskSessionTurnItem[]): AskMessage[] =>
    turns.map((turn) => ({
      id: `session-turn-${turn.id}`,
      role: turn.role === "assistant" ? "assistant" : "user",
      content: turn.content,
      streaming: false,
      citations: turn.citations ?? [],
      meta: turn.meta
        ? {
            mode: turn.meta.mode,
            searchStrategy: turn.meta.search_strategy ?? null,
            answerStrategy: turn.meta.answer_strategy ?? null,
            topK: turn.meta.top_k ?? 0,
            matchedPages: turn.meta.matched_pages ?? 0,
            searchDebug: turn.meta.search_debug ?? null,
          }
        : undefined,
    }));

  const focusAskMessageById = (messageId: string) => {
    if (!messageId) return;
    globalThis.setTimeout(() => {
      const node = globalThis.document?.querySelector<HTMLElement>(
        `[data-ask-message-id="${messageId}"]`,
      );
      if (!node) return;
      node.scrollIntoView({ behavior: "smooth", block: "center" });
      setAskFocusedMessageId(messageId);
      if (askFocusTimerRef.current !== null) {
        globalThis.clearTimeout(askFocusTimerRef.current);
      }
      askFocusTimerRef.current = globalThis.setTimeout(() => {
        setAskFocusedMessageId((prev) => (prev === messageId ? "" : prev));
      }, 2200);
    }, 40);
  };

  const refreshAskSessionList = async (): Promise<AskSessionItem[]> => {
    if (!isTauriRuntime()) {
      setAskSessions([]);
      return [];
    }
    setAskSessionsLoading(true);
    try {
      const sessions = await listAskSessions(ASK_SESSION_LIST_MAX);
      const normalized = sessions ?? [];
      setAskSessions(normalized);
      return normalized;
    } finally {
      setAskSessionsLoading(false);
    }
  };

  const loadAskSessionMessages = async (sessionId: string): Promise<AskSessionTurnItem[]> => {
    if (!isTauriRuntime()) {
      setAskMessages([]);
      return [];
    }
    setAskMessages([]); // clear first to avoid showing stale messages during switch
    try {
      const turns = await fetchAskSessionTurns(sessionId, 400);
      const normalizedTurns = turns ?? [];
      setAskMessages(buildAskMessagesFromSessionTurns(normalizedTurns));
      return normalizedTurns;
    } catch {
      setAskMessages([]);
      return [];
    }
  };

  const handleCreateAskSession = async () => {
    if (!isTauriRuntime() || askSessionManaging) return;
    setAskSessionManaging(true);
    try {
      const nextSessionId = crypto.randomUUID();
      const created = await createAskSession(nextSessionId, "新对话");
      if (!created) {
        setStatusMessage("创建会话失败，请稍后重试。");
        return;
      }
      const nextSessions = [created, ...askSessions].slice(0, ASK_SESSION_LIST_MAX);
      setAskSessions(nextSessions);
      setAskSessionId(nextSessionId);
      setAskMessages([]);
      setAskSessionSearchHits([]);
      setQueryResult(null);
      setExpandedCitationIds(new Set());
      setStatusMessage("新会话已创建。");
    } finally {
      setAskSessionManaging(false);
    }
  };

  const handleSelectAskSession = async (session: AskSessionItem) => {
    if (!isTauriRuntime() || askSessionManaging) return;
    setAskSessionManaging(true);
    try {
      setAskSessionId(session.session_id);
      await loadAskSessionMessages(session.session_id);
      setQueryResult(null);
      setExpandedCitationIds(new Set());
      setAskSessionSearchHits((prev) =>
        prev.filter((item) => item.session_id === session.session_id),
      );
      setAskFocusedMessageId("");
      setStatusMessage(`已切换到会话：${session.title}`);
    } finally {
      setAskSessionManaging(false);
    }
  };

  const handleSearchAskSessionTurns = async () => {
    if (!isTauriRuntime()) return;
    const keyword = askSessionSearchKeyword.trim();
    if (!keyword) {
      setAskSessionSearchHits([]);
      return;
    }
    setAskSessionSearching(true);
    try {
      const hits = await searchAskSessionTurns(keyword, ASK_SESSION_SEARCH_LIMIT);
      const normalized = hits ?? [];
      setAskSessionSearchHits(normalized);
      setStatusMessage(
        normalized.length > 0
          ? `跨会话检索完成：命中 ${normalized.length} 条记录。`
          : "跨会话检索完成：未命中。",
      );
    } finally {
      setAskSessionSearching(false);
    }
  };

  const handleOpenAskSearchHit = async (hit: AskSessionSearchHitItem) => {
    if (!isTauriRuntime() || askSessionManaging || queryRunning) return;
    setAskSessionManaging(true);
    try {
      setAskSessionId(hit.session_id);
      const turns = await loadAskSessionMessages(hit.session_id);
      setQueryResult(null);
      setExpandedCitationIds(new Set());
      if (turns.length === 0) {
        setStatusMessage(`未找到会话「${hit.session_title}」的消息，该会话可能已被删除。`);
        return;
      }
      focusAskMessageById(`session-turn-${hit.turn_id}`);
      setStatusMessage(`已定位到会话「${hit.session_title}」中的匹配消息。`);
    } finally {
      setAskSessionManaging(false);
    }
  };

  const handleRenameAskSession = async (session: AskSessionItem) => {
    if (!isTauriRuntime() || askSessionManaging) return;
    const nextTitle = globalThis.prompt("请输入新的会话标题", session.title)?.trim();
    if (!nextTitle || nextTitle === session.title) return;
    setAskSessionManaging(true);
    try {
      const renamed = await renameAskSession(session.session_id, nextTitle);
      if (!renamed) {
        setStatusMessage("重命名会话失败。");
        return;
      }
      const latest = await refreshAskSessionList();
      const renamedSession = latest.find((item) => item.session_id === session.session_id);
      setStatusMessage(renamedSession ? `会话已重命名为：${renamedSession.title}` : "会话已重命名。");
    } finally {
      setAskSessionManaging(false);
    }
  };

  const handleDeleteAskSession = async (session: AskSessionItem) => {
    if (!isTauriRuntime() || askSessionManaging) return;
    const confirmed = await askConfirmDialog(
      `确认删除会话"${session.title}"？该会话消息将被移除。`,
      { title: "删除会话", kind: "warning", okLabel: "删除", cancelLabel: "取消" },
    );
    if (!confirmed) return;
    setAskSessionManaging(true);
    try {
      const deleted = await deleteAskSession(session.session_id);
      if (!deleted) {
        setStatusMessage("删除会话失败。");
        return;
      }
      let latest = await refreshAskSessionList();
      if (latest.length === 0) {
        const nextSessionId = crypto.randomUUID();
        const created = await createAskSession(nextSessionId, "新对话");
        if (created) {
          latest = [created];
          setAskSessions(latest);
        }
      }
      const fallbackSessionId = latest[0]?.session_id ?? "";
      if (askSessionId === session.session_id && fallbackSessionId) {
        setAskSessionId(fallbackSessionId);
        await loadAskSessionMessages(fallbackSessionId);
      } else if (askSessionId === session.session_id) {
        setAskMessages([]);
      }
      setQueryResult(null);
      setExpandedCitationIds(new Set());
      setStatusMessage("会话已删除。");
    } finally {
      setAskSessionManaging(false);
    }
  };

  const handleExportAskSession = async (session: AskSessionItem) => {
    if (askSessionManaging) return;
    setAskSessionManaging(true);
    try {
      const turns = await fetchAskSessionTurns(session.session_id, 800);
      const markdown = buildAskSessionExportMarkdown(session, turns ?? []);
      const defaultFilename = `${session.title || "ask-session"}-${session.session_id.slice(0, 8)}.md`;
      if (isTauriRuntime()) {
        const savePath = await pickSaveFile({
          defaultPath: defaultFilename,
          filters: [{ name: "Markdown", extensions: ["md"] }],
        });
        if (!savePath) return;
        await saveResearchDoc(savePath, markdown);
        setStatusMessage(`会话已导出：${savePath}`);
        return;
      }
      const blob = new Blob([markdown], { type: "text/markdown;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = defaultFilename;
      link.click();
      URL.revokeObjectURL(url);
      setStatusMessage("会话已导出。");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`导出会话失败：${message}`);
    } finally {
      setAskSessionManaging(false);
    }
  };

  const handleCopySearchDebug = async (
    messageId: string,
    searchDebug: QuerySearchDebug,
  ) => {
    const payload = JSON.stringify(searchDebug, null, 2);
    const copied = await copyTextToClipboard(payload);
    if (!copied) {
      setStatusMessage("复制失败：当前环境不支持写入剪贴板。");
      return;
    }
    setSearchDebugCopiedMessageId(messageId);
    setStatusMessage("已复制检索调试 JSON。");
    window.setTimeout(() => {
      setSearchDebugCopiedMessageId((current) => (current === messageId ? "" : current));
    }, 1500);
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

    let activeSessionId = askSessionId.trim();
    if (!activeSessionId) {
      activeSessionId = crypto.randomUUID();
      const created = await createAskSession(activeSessionId, "新对话");
      if (!created) {
        setStatusMessage("创建会话失败，请稍后重试。");
        return;
      }
      setAskSessions((prev) => [created, ...prev].slice(0, ASK_SESSION_LIST_MAX));
      setAskSessionId(activeSessionId);
    }

    setQueryRunning(true);
    setStatusMessage("查询中...");
    setQueryResult(null);
    let unlisten: (() => void) | null = null;
    const requestId = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    const assistantMessageId = `assistant-${requestId}`;
    setAskMessages((prev) => [
      ...prev,
      { id: `user-${requestId}`, role: "user", content: nextQuestion },
      { id: assistantMessageId, role: "assistant", content: "", streaming: true },
    ]);

    try {
      try {
        unlisten = await listenProgress("query_progress", (payload) => {
          const update = parseQueryProgressPayload(payload);
          if (update.kind === "chunk") {
            if (update.text === "") return;
            setAskMessages((prev) =>
              prev.map((message) =>
                message.id === assistantMessageId
                  ? { ...message, content: `${message.content}${update.text}` }
                  : message,
              ),
            );
            return;
          }
          if (update.text.trim()) {
            setStatusMessage(update.text);
          }
        });
      } catch (error) {
        console.warn("订阅 query 进度事件失败，继续执行查询流程。", error);
      }

      const result = await queryAskSession(activeSessionId, nextQuestion, { top_k: nextTopK });
      if (!result) {
        setStatusMessage("当前环境不支持查询。");
        return;
      }

      setQueryResult(result);
      setAskMessages((prev) =>
        prev.map((message) =>
          message.id === assistantMessageId
            ? {
                ...message,
                content: result.answer,
                streaming: false,
                citations: result.citations ?? [],
                meta: {
                  mode: result.mode,
                  searchStrategy: result.search_strategy,
                  answerStrategy: result.answer_strategy,
                  topK: nextTopK,
                  matchedPages: result.matched_pages.length,
                  searchDebug: result.search_debug ?? null,
                },
              }
            : message,
        ),
      );
      setQueryHistoryItems((prev) => {
        const createdAt = Math.floor(Date.now() / 1000).toString();
        const next = mergeQueryHistoryItems(prev, nextQuestion, createdAt, QUERY_HISTORY_MAX);
        writeQueryHistoryItemsToStorage(next, QUERY_HISTORY_MAX);
        return next;
      });
      void saveAskHistory(nextQuestion);
      await refreshAskSessionList();
      setQueryQuestion("");
      setStatusMessage(`Query 已完成：TopK=${nextTopK}，命中 ${result.matched_pages.length} 页。`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setAskMessages((prev) =>
        prev.map((item) =>
          item.id === assistantMessageId
            ? { ...item, content: item.content || `生成失败：${message}`, streaming: false }
            : item,
        ),
      );
      setStatusMessage(`Query 失败：${message}`);
    } finally {
      if (unlisten) unlisten();
      setQueryRunning(false);
    }
  };

  const handleCancelQuery = async () => {
    try {
      await cancelAskSession(askSessionId);
      setStatusMessage("查询已取消。");
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`取消查询失败：${message}`);
    } finally {
      setQueryRunning(false);
    }
  };

  const handleClearQueryHistory = async () => {
    if (queryHistoryItems.length === 0) return;
    if (!globalThis.confirm("确定清空 Ask 历史吗？此操作不可撤销。")) return;

    let backendCleared = true;
    if (isTauriRuntime()) {
      backendCleared = await clearAskHistory();
    }

    setQueryHistoryItems([]);
    setAskHistoryKeyword("");
    writeQueryHistoryItemsToStorage([], QUERY_HISTORY_MAX);
    setStatusMessage(backendCleared ? "Ask 历史已清空。" : "本地历史已清空（后端清理失败）。");
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

      setStatusMessage(`${result.message}：${result.wiki_path}`);
      if (onOpenWikiPage) {
        await onOpenWikiPage(result.wiki_path);
      }
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`保存 Query 结果失败：${message}`);
    } finally {
      setQueryResultSaving(false);
    }
  };

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="ask-layout">
      <div className="ask-topbar">
        <div className="ask-topbar__title-group">
          <h1 className="ask-topbar__title">Ask</h1>
          <span className="ask-topbar__sub">基于 Wiki 索引的多轮问答</span>
        </div>
        <div className="ask-topbar__actions">
          <button
            type="button"
            className="ask-new-session-btn"
            disabled={!isTauri || askSessionManaging}
            onClick={() => void handleCreateAskSession()}
          >
            ↺ 新会话
          </button>
        </div>
      </div>

      <div className="ask-main">
        <aside className="ask-sessions">
          <div className="ask-sessions__head">
            <span className="ask-sessions__title">会话管理</span>
            <span className="ask-sessions__count">{askSessions.length}</span>
          </div>
          <input
            className="ask-sessions__filter"
            type="text"
            placeholder="筛选会话"
            value={askSessionKeyword}
            onChange={(event) => setAskSessionKeyword(event.target.value)}
          />
          <div className="ask-sessions__search-row">
            <input
              className="ask-sessions__search-input"
              type="text"
              placeholder="跨会话检索内容"
              value={askSessionSearchKeyword}
              onChange={(event) => setAskSessionSearchKeyword(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void handleSearchAskSessionTurns();
                }
              }}
            />
            <button
              type="button"
              className="ask-sessions__search-btn"
              disabled={askSessionSearching || askSessionManaging}
              onClick={() => void handleSearchAskSessionTurns()}
            >
              {askSessionSearching ? "检索中..." : "检索"}
            </button>
          </div>
          {askSessionSearchHits.length > 0 && (
            <div className="ask-sessions__search-results">
              {askSessionSearchHits.map((hit) => (
                <button
                  key={`${hit.session_id}-${hit.turn_id}`}
                  type="button"
                  className="ask-sessions__search-hit"
                  disabled={askSessionManaging || queryRunning}
                  onClick={() => void handleOpenAskSearchHit(hit)}
                >
                  <span className="ask-sessions__search-hit-title">{hit.session_title}</span>
                  <span className="ask-sessions__search-hit-snippet">
                    {formatAskSessionSearchSnippet(hit.snippet)}
                  </span>
                  <span className="ask-sessions__search-hit-meta">
                    {hit.role === "assistant" ? "助手" : "用户"} ·{" "}
                    {formatAskHistoryCreatedAt(hit.created_at) || "-"}
                  </span>
                </button>
              ))}
            </div>
          )}
          <div className="ask-sessions__list">
            {askSessionsLoading ? (
              <p className="ask-sessions__empty">加载中...</p>
            ) : filteredAskSessions.length === 0 ? (
              <p className="ask-sessions__empty">暂无会话</p>
            ) : (
              filteredAskSessions.map((session) => {
                const isActive = session.session_id === askSessionId;
                return (
                  <article
                    key={session.session_id}
                    className={`ask-session-card${isActive ? " ask-session-card--active" : ""}`}
                  >
                    <button
                      type="button"
                      className="ask-session-card__main"
                      disabled={askSessionManaging || queryRunning}
                      onClick={() => void handleSelectAskSession(session)}
                    >
                      <span className="ask-session-card__title">{session.title}</span>
                      <span className="ask-session-card__meta">
                        {session.turn_count} 轮 · {formatAskHistoryCreatedAt(session.updated_at) || "-"}
                      </span>
                      <span className="ask-session-card__preview">
                        {session.last_turn_content
                          ? session.last_turn_content.slice(0, 56)
                          : "暂无消息"}
                      </span>
                    </button>
                    <div className="ask-session-card__actions">
                      <button
                        type="button"
                        title="重命名"
                        disabled={askSessionManaging || queryRunning}
                        onClick={() => void handleRenameAskSession(session)}
                      >
                        重命名
                      </button>
                      <button
                        type="button"
                        title="导出"
                        disabled={askSessionManaging || queryRunning}
                        onClick={() => void handleExportAskSession(session)}
                      >
                        导出
                      </button>
                      <button
                        type="button"
                        title="删除"
                        className="ask-session-card__danger"
                        disabled={askSessionManaging || queryRunning}
                        onClick={() => void handleDeleteAskSession(session)}
                      >
                        删除
                      </button>
                    </div>
                  </article>
                );
              })
            )}
          </div>
        </aside>

        <div className="ask-messages">
          {askMessages.length === 0 ? (
            <div className="ask-empty">
              <div className="ask-empty__icon">💬</div>
              <p className="ask-empty__text">输入问题，基于 Wiki 知识库获得有引用来源的回答</p>
              {queryHistoryItems.length > 0 && (
                <div className="ask-empty__history">
                  <div className="ask-empty__history-toolbar">
                    <span className="ask-empty__history-label">最近提问</span>
                    <div className="ask-empty__history-actions">
                      <input
                        className="ask-empty__history-filter"
                        type="text"
                        value={askHistoryKeyword}
                        placeholder="筛选历史问题"
                        onChange={(event) => setAskHistoryKeyword(event.target.value)}
                      />
                      <button
                        type="button"
                        className="ask-empty__history-clear"
                        onClick={() => void handleClearQueryHistory()}
                      >
                        清空历史
                      </button>
                    </div>
                  </div>
                  <div className="ask-history">
                    {filteredQueryHistoryItems.length > 0 ? (
                      filteredQueryHistoryItems.map((item) => (
                        <button
                          key={`${item.id}-${item.question}`}
                          type="button"
                          className="ask-history__chip"
                          onClick={() => setQueryQuestion(item.question)}
                          title={item.question}
                        >
                          <span className="ask-history__chip-question">
                            {item.question.length > 50
                              ? `${item.question.slice(0, 50)}…`
                              : item.question}
                          </span>
                          {formatAskHistoryCreatedAt(item.created_at) ? (
                            <span className="ask-history__chip-time">
                              {formatAskHistoryCreatedAt(item.created_at)}
                            </span>
                          ) : null}
                        </button>
                      ))
                    ) : (
                      <span className="ask-history__empty">没有匹配的历史问题</span>
                    )}
                  </div>
                </div>
              )}
            </div>
          ) : (
            askMessages.map((message) => {
              const isLastAssistant =
                message.role === "assistant" &&
                !message.streaming &&
                message.id ===
                  [...askMessages].reverse().find((item) => item.role === "assistant")?.id;
              const hasCitations =
                message.role === "assistant" && (message.citations?.length ?? 0) > 0;
              const citationsExpanded = expandedCitationIds.has(message.id);

              return (
                <article
                  key={message.id}
                  data-ask-message-id={message.id}
                  className={`ask-message ask-message--${message.role}${
                    askFocusedMessageId === message.id ? " ask-message--focused" : ""
                  }`}
                >
                  <div className="ask-message__avatar">
                    {message.role === "user" ? "你" : "AI"}
                  </div>
                  <div className="ask-message__body">
                    <div className="ask-message__content">
                      {message.content || (message.streaming ? "" : "")}
                      {message.streaming ? <span className="ask-chat__cursor" /> : null}
                    </div>

                    {hasCitations && (
                      <div className="ask-message__citations-wrap">
                        <button
                          type="button"
                          className="ask-citations-toggle"
                          onClick={() =>
                            setExpandedCitationIds((prev) => {
                              const next = new Set(prev);
                              if (next.has(message.id)) {
                                next.delete(message.id);
                              } else {
                                next.add(message.id);
                              }
                              return next;
                            })
                          }
                        >
                          📎 {message.citations!.length} 个引用来源
                          <span className="ask-citations-toggle__chevron">
                            {citationsExpanded ? "▾" : "▸"}
                          </span>
                        </button>
                        {citationsExpanded && (
                          <div className="ask-citations-list">
                            {message.citations!.map((citation) => (
                              <div
                                key={`${citation.page_path}-${citation.score}`}
                                className="ask-citation"
                              >
                                <div className="ask-citation__top">
                                  <code>{resolveDisplayPath(citation)}</code>
                                  <span>score: {citation.score}</span>
                                </div>
                                <p>{citation.excerpt}</p>
                              </div>
                            ))}
                          </div>
                        )}
                      </div>
                    )}

                    {message.role === "assistant" && !message.streaming && message.meta && (
                      <div className="ask-message__meta">
                        <span className="pill pill--info">
                          {formatBackendMode(message.meta.mode)}
                        </span>
                        {message.meta.searchStrategy && (
                          <span className="pill pill--lint">
                            {formatQuerySearchStrategyLabel(message.meta.searchStrategy)}
                          </span>
                        )}
                        {message.meta.answerStrategy && (
                          <span className="pill pill--lint">
                            {formatQueryAnswerStrategyLabel(message.meta.answerStrategy)}
                          </span>
                        )}
                        <span className="pill">命中 {message.meta.matchedPages} 页</span>
                      </div>
                    )}

                    {message.role === "assistant" &&
                      !message.streaming &&
                      askSearchDebugVisible &&
                      message.meta?.searchDebug &&
                      message.meta.searchDebug.routes.length > 0 && (
                        <details className="ask-message__debug">
                          <summary>
                            检索调试：
                            {formatQuerySearchStrategyLabel(message.meta.searchDebug.strategy)}
                            {typeof message.meta.searchDebug.rrf_k === "number" &&
                              `（k=${message.meta.searchDebug.rrf_k}）`}
                          </summary>
                          <div className="ask-message__debug-actions">
                            <button
                              type="button"
                              className="ask-message__debug-copy"
                              onClick={() =>
                                void handleCopySearchDebug(message.id, message.meta!.searchDebug!)
                              }
                            >
                              {searchDebugCopiedMessageId === message.id ? "已复制" : "复制 JSON"}
                            </button>
                          </div>
                          <div className="ask-message__debug-routes">
                            {message.meta.searchDebug.routes.map((route) => (
                              <article
                                key={`${message.id}-${route.route}`}
                                className="ask-message__debug-route"
                              >
                                <div className="ask-message__debug-route-head">
                                  <strong>{formatQuerySearchRouteLabel(route.route)}</strong>
                                  <span>
                                    候选 {route.candidate_count} / 贡献{" "}
                                    {route.contributed_paths.length}
                                  </span>
                                </div>
                                {route.contributed_paths.length > 0 && (
                                  <p>
                                    贡献路径：
                                    {route.contributed_paths
                                      .slice(0, 3)
                                      .map((path) => resolveDisplayPath({ page_path: path }))
                                      .join("，")}
                                  </p>
                                )}
                                {route.top_candidates.length > 0 && (
                                  <p>
                                    候选示例：
                                    {route.top_candidates
                                      .slice(0, 3)
                                      .map((path) => resolveDisplayPath({ page_path: path }))
                                      .join("，")}
                                  </p>
                                )}
                              </article>
                            ))}
                          </div>
                        </details>
                      )}

                    {isLastAssistant && queryResult && (
                      <div className="ask-message__actions">
                        <button
                          type="button"
                          className="ask-message__save-btn"
                          onClick={() => void handleSaveQueryResult()}
                          disabled={!isTauri || queryResultSaving}
                        >
                          {queryResultSaving ? "保存中..." : "保存回答到 Wiki"}
                        </button>
                      </div>
                    )}
                  </div>
                </article>
              );
            })
          )}
          <div ref={messagesEndRef} />
        </div>
      </div>

      <div className="ask-input-area">
        {showAskAdvanced && (
          <div className="ask-advanced">
            <label className="ask-advanced__label">
              TopK（{queryTopKMin}–{queryTopKMax}）
              <input
                type="number"
                className="ask-advanced__input"
                min={queryTopKMin}
                max={queryTopKMax}
                step={1}
                value={queryTopK}
                onChange={(event) => setQueryTopK(Number(event.target.value))}
              />
            </label>
            <label className="ask-advanced__toggle">
              <input
                type="checkbox"
                checked={askSearchDebugVisible}
                onChange={(event) => setAskSearchDebugVisible(event.target.checked)}
              />
              显示检索调试区
            </label>
            <button
              type="button"
              className="ask-advanced__save-btn"
              onClick={() => void handleSaveQuerySettings()}
              disabled={!isTauri || querySettingsSaving}
            >
              {querySettingsSaving ? "保存中..." : "保存参数"}
            </button>
          </div>
        )}

        <textarea
          className="ask-input__textarea"
          value={queryQuestion}
          onChange={(event) => setQueryQuestion(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              if (!queryRunning && isTauri) {
                void handleQueryAsk();
              }
            }
          }}
          placeholder="输入问题后按 Enter 发送，Shift+Enter 换行"
          rows={2}
          disabled={queryRunning}
        />

        <div className="ask-input__footer">
          <button
            type="button"
            className={`ask-advanced-toggle${showAskAdvanced ? " ask-advanced-toggle--active" : ""}`}
            onClick={() => setShowAskAdvanced((value) => !value)}
            title="高级设置（TopK）"
          >
            ⚙ 高级
          </button>
          <div className="ask-input__footer-right">
            {queryRunning ? (
              <button
                type="button"
                className="ask-stop-btn"
                onClick={() => void handleCancelQuery()}
              >
                ⏹ 停止
              </button>
            ) : (
              <button
                type="button"
                className="ask-send-btn"
                onClick={() => void handleQueryAsk()}
                disabled={!isTauri || queryRunning}
              >
                发送 ↵
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
