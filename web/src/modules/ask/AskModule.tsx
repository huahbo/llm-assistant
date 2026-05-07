import type { Dispatch, RefObject, SetStateAction } from "react";
import { formatBackendMode } from "../../app-formatters";
import { resolveDisplayPath } from "../../tauri-client";
import type {
  AskHistoryItem,
  AskSessionItem,
  AskSessionSearchHitItem,
  BackendAppMode,
  QueryAnswerResult,
  QueryCitation,
  QuerySearchDebug,
} from "../../types";

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

type AskModuleProps = {
  isTauri: boolean;
  askSessionManaging: boolean;
  onCreateAskSession: () => void | Promise<void>;
  askSessions: AskSessionItem[];
  askSessionKeyword: string;
  setAskSessionKeyword: (value: string) => void;
  askSessionSearchKeyword: string;
  setAskSessionSearchKeyword: (value: string) => void;
  askSessionSearching: boolean;
  onSearchAskSessionTurns: () => void | Promise<void>;
  askSessionSearchHits: AskSessionSearchHitItem[];
  queryRunning: boolean;
  onOpenAskSearchHit: (hit: AskSessionSearchHitItem) => void | Promise<void>;
  formatAskSessionSearchSnippet: (raw: string) => string;
  formatAskHistoryCreatedAt: (createdAt: string) => string;
  askSessionsLoading: boolean;
  filteredAskSessions: AskSessionItem[];
  askSessionId: string;
  onSelectAskSession: (session: AskSessionItem) => void | Promise<void>;
  onRenameAskSession: (session: AskSessionItem) => void | Promise<void>;
  onExportAskSession: (session: AskSessionItem) => void | Promise<void>;
  onDeleteAskSession: (session: AskSessionItem) => void | Promise<void>;
  askMessages: AskMessage[];
  queryHistoryItemsCount: number;
  askHistoryKeyword: string;
  setAskHistoryKeyword: (value: string) => void;
  onClearQueryHistory: () => void | Promise<void>;
  filteredQueryHistoryItems: AskHistoryItem[];
  setQueryQuestion: (value: string) => void;
  askFocusedMessageId: string;
  expandedCitationIds: Set<string>;
  setExpandedCitationIds: Dispatch<SetStateAction<Set<string>>>;
  askSearchDebugVisible: boolean;
  formatQuerySearchStrategyLabel: (strategy: string | null | undefined) => string;
  formatQueryAnswerStrategyLabel: (strategy: string | null | undefined) => string;
  formatQuerySearchRouteLabel: (route: string | null | undefined) => string;
  searchDebugCopiedMessageId: string;
  onCopySearchDebug: (messageId: string, searchDebug: QuerySearchDebug) => void | Promise<void>;
  queryResult: QueryAnswerResult | null;
  onSaveQueryResult: () => void | Promise<void>;
  queryResultSaving: boolean;
  messagesEndRef: RefObject<HTMLDivElement>;
  showAskAdvanced: boolean;
  setShowAskAdvanced: Dispatch<SetStateAction<boolean>>;
  queryTopKMin: number;
  queryTopKMax: number;
  queryTopK: number;
  setQueryTopK: (value: number) => void;
  setAskSearchDebugVisible: (value: boolean) => void;
  onSaveQuerySettings: () => void | Promise<void>;
  querySettingsSaving: boolean;
  queryQuestion: string;
  setQueryQuestionDirect: (value: string) => void;
  onQueryAsk: () => void | Promise<void>;
  onCancelQuery: () => void | Promise<void>;
};

export default function AskModule({
  isTauri,
  askSessionManaging,
  onCreateAskSession,
  askSessions,
  askSessionKeyword,
  setAskSessionKeyword,
  askSessionSearchKeyword,
  setAskSessionSearchKeyword,
  askSessionSearching,
  onSearchAskSessionTurns,
  askSessionSearchHits,
  queryRunning,
  onOpenAskSearchHit,
  formatAskSessionSearchSnippet,
  formatAskHistoryCreatedAt,
  askSessionsLoading,
  filteredAskSessions,
  askSessionId,
  onSelectAskSession,
  onRenameAskSession,
  onExportAskSession,
  onDeleteAskSession,
  askMessages,
  queryHistoryItemsCount,
  askHistoryKeyword,
  setAskHistoryKeyword,
  onClearQueryHistory,
  filteredQueryHistoryItems,
  setQueryQuestion,
  askFocusedMessageId,
  expandedCitationIds,
  setExpandedCitationIds,
  askSearchDebugVisible,
  formatQuerySearchStrategyLabel,
  formatQueryAnswerStrategyLabel,
  formatQuerySearchRouteLabel,
  searchDebugCopiedMessageId,
  onCopySearchDebug,
  queryResult,
  onSaveQueryResult,
  queryResultSaving,
  messagesEndRef,
  showAskAdvanced,
  setShowAskAdvanced,
  queryTopKMin,
  queryTopKMax,
  queryTopK,
  setQueryTopK,
  setAskSearchDebugVisible,
  onSaveQuerySettings,
  querySettingsSaving,
  queryQuestion,
  setQueryQuestionDirect,
  onQueryAsk,
  onCancelQuery,
}: AskModuleProps) {
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
            onClick={() => void onCreateAskSession()}
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
                  void onSearchAskSessionTurns();
                }
              }}
            />
            <button
              type="button"
              className="ask-sessions__search-btn"
              disabled={askSessionSearching || askSessionManaging}
              onClick={() => void onSearchAskSessionTurns()}
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
                  onClick={() => void onOpenAskSearchHit(hit)}
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
                      onClick={() => void onSelectAskSession(session)}
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
                        onClick={() => void onRenameAskSession(session)}
                      >
                        重命名
                      </button>
                      <button
                        type="button"
                        title="导出"
                        disabled={askSessionManaging || queryRunning}
                        onClick={() => void onExportAskSession(session)}
                      >
                        导出
                      </button>
                      <button
                        type="button"
                        title="删除"
                        className="ask-session-card__danger"
                        disabled={askSessionManaging || queryRunning}
                        onClick={() => void onDeleteAskSession(session)}
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
              {queryHistoryItemsCount > 0 && (
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
                        onClick={() => void onClearQueryHistory()}
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
                            {item.question.length > 50 ? `${item.question.slice(0, 50)}…` : item.question}
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
                message.role === "assistant"
                && !message.streaming
                && message.id === [...askMessages].reverse().find((item) => item.role === "assistant")?.id;
              const hasCitations = message.role === "assistant" && (message.citations?.length ?? 0) > 0;
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

                    {message.role === "assistant"
                      && !message.streaming
                      && askSearchDebugVisible
                      && message.meta?.searchDebug
                      && message.meta.searchDebug.routes.length > 0 && (
                        <details className="ask-message__debug">
                          <summary>
                            检索调试：{formatQuerySearchStrategyLabel(
                              message.meta.searchDebug.strategy,
                            )}
                            {typeof message.meta.searchDebug.rrf_k === "number"
                              && `（k=${message.meta.searchDebug.rrf_k}）`}
                          </summary>
                          <div className="ask-message__debug-actions">
                            <button
                              type="button"
                              className="ask-message__debug-copy"
                              onClick={() =>
                                void onCopySearchDebug(
                                  message.id,
                                  message.meta!.searchDebug!,
                                )
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
                          onClick={() => void onSaveQueryResult()}
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
              onClick={() => void onSaveQuerySettings()}
              disabled={!isTauri || querySettingsSaving}
            >
              {querySettingsSaving ? "保存中..." : "保存参数"}
            </button>
          </div>
        )}

        <textarea
          className="ask-input__textarea"
          value={queryQuestion}
          onChange={(event) => setQueryQuestionDirect(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              if (!queryRunning && isTauri) {
                void onQueryAsk();
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
                onClick={() => void onCancelQuery()}
              >
                ⏹ 停止
              </button>
            ) : (
              <button
                type="button"
                className="ask-send-btn"
                onClick={() => void onQueryAsk()}
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
