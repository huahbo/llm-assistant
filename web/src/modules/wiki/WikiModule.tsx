import type { KeyboardEvent as ReactKeyboardEvent, ReactNode } from "react";
import { formatLintCheckedAt } from "../../lint-utils";
import { resolveDisplayPath } from "../../tauri-client";
import type { NewPageResult, WikiPageDetail, WikiPageItem } from "../../types";

type WikiSortMode = "updated_desc" | "updated_asc" | "title_asc";

type WikiTagSummary = {
  name: string;
  count: number;
};

type WikiSummaryDisplay = {
  text: string;
  isTruncated: boolean;
};

type WikiHighlightSegment = {
  text: string;
  matched: boolean;
};

type WikiModuleProps = {
  isTauri: boolean;
  sortedWikiPages: WikiPageItem[];
  wikiSortMode: WikiSortMode;
  wikiSortModeLabels: Record<WikiSortMode, string>;
  onWikiSortModeChange: (mode: WikiSortMode) => void;
  wikiKeyword: string;
  onWikiKeywordChange: (value: string) => void;
  onWikiKeywordKeyDown: (event: ReactKeyboardEvent<HTMLInputElement>) => void;
  wikiSearching: boolean;
  onSearchWikiPages: () => void | Promise<void>;
  onResetWikiPages: () => void | Promise<void>;
  showNewPageModal: boolean;
  onOpenNewPageModal: () => void;
  onCloseNewPageModal: () => void;
  newPageTopic: string;
  onNewPageTopicChange: (value: string) => void;
  newPageCreating: boolean;
  onCreatePageWithAi: () => void | Promise<void>;
  newPageResult: NewPageResult | null;
  onUseNewPageResult: () => void;
  allWikiTags: WikiTagSummary[];
  wikiActiveTags: Set<string>;
  onClearWikiActiveTags: () => void;
  onToggleWikiTag: (tag: string) => void;
  displayedWikiPages: WikiPageItem[];
  onExpandAllWikiFolders: () => void;
  onCollapseAllWikiFolders: () => void;
  renderWikiTree: () => ReactNode;
  wikiActivePagePath: string;
  wikiPageDetail: WikiPageDetail | null;
  isPageSummaryExpanded: (path: string) => boolean;
  wikiSummaryPreviewLines: number;
  buildWikiSummaryDisplay: (summary: string, expanded: boolean) => WikiSummaryDisplay;
  highlightWikiText: (text: string) => WikiHighlightSegment[];
  isPageActive: (path: string) => boolean;
  isPageDetailActive: (path: string) => boolean;
  onToggleWikiSummary: (path: string) => void;
  wikiPageDetailLoading: boolean;
  onCloseWikiPreview: () => void;
  onOpenWikiPage: (path: string) => void | Promise<void>;
  wikiPageDetailError: string;
  renderWikiPreview: () => ReactNode;
  isActiveWikiDetailInList: boolean;
};

export default function WikiModule({
  isTauri,
  sortedWikiPages,
  wikiSortMode,
  wikiSortModeLabels,
  onWikiSortModeChange,
  wikiKeyword,
  onWikiKeywordChange,
  onWikiKeywordKeyDown,
  wikiSearching,
  onSearchWikiPages,
  onResetWikiPages,
  showNewPageModal,
  onOpenNewPageModal,
  onCloseNewPageModal,
  newPageTopic,
  onNewPageTopicChange,
  newPageCreating,
  onCreatePageWithAi,
  newPageResult,
  onUseNewPageResult,
  allWikiTags,
  wikiActiveTags,
  onClearWikiActiveTags,
  onToggleWikiTag,
  displayedWikiPages,
  onExpandAllWikiFolders,
  onCollapseAllWikiFolders,
  renderWikiTree,
  wikiActivePagePath,
  wikiPageDetail,
  isPageSummaryExpanded,
  wikiSummaryPreviewLines,
  buildWikiSummaryDisplay,
  highlightWikiText,
  isPageActive,
  isPageDetailActive,
  onToggleWikiSummary,
  wikiPageDetailLoading,
  onCloseWikiPreview,
  onOpenWikiPage,
  wikiPageDetailError,
  renderWikiPreview,
  isActiveWikiDetailInList,
}: WikiModuleProps) {
  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">Wiki</h1>
        <p className="module-header__sub">浏览、搜索和查看 Markdown Vault 页面</p>
      </div>
      <section className="panel">
        <div className="section-head">
          <h2>Wiki 页面</h2>
          <span className="section-head__hint">
            {sortedWikiPages.length ? `${sortedWikiPages.length} 页 · ${wikiSortModeLabels[wikiSortMode]}` : "暂无页面"}
          </span>
        </div>
        <div className="dev-panel">
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="wiki-keyword">关键字</label>
            <input
              id="wiki-keyword"
              className="dev-panel__input"
              type="text"
              value={wikiKeyword}
              onChange={(event) => onWikiKeywordChange(event.target.value)}
              onKeyDown={onWikiKeywordKeyDown}
              placeholder="按标题、摘要、路径搜索"
              spellCheck={false}
            />
          </div>
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="wiki-sort-mode">排序</label>
            <select
              id="wiki-sort-mode"
              className="dev-panel__input"
              value={wikiSortMode}
              onChange={(event) => onWikiSortModeChange(event.target.value as WikiSortMode)}
            >
              <option value="updated_desc">{wikiSortModeLabels.updated_desc}</option>
              <option value="updated_asc">{wikiSortModeLabels.updated_asc}</option>
              <option value="title_asc">{wikiSortModeLabels.title_asc}</option>
            </select>
          </div>
          <div className="dev-panel__actions">
            <button
              type="button"
              className="dev-panel__button dev-panel__button--accent"
              onClick={() => void onSearchWikiPages()}
              disabled={!isTauri || wikiSearching}
            >
              {wikiSearching ? "搜索中..." : "搜索 Wiki"}
            </button>
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void onResetWikiPages()}
              disabled={wikiSearching}
            >
              恢复最近
            </button>
            <button
              type="button"
              className="wiki-new-btn"
              onClick={onOpenNewPageModal}
              title="AI 辅助新建 Wiki 页面"
            >
              + AI 新建
            </button>
          </div>
        </div>
        {showNewPageModal && (
          <div className="new-page-modal-backdrop" onClick={onCloseNewPageModal}>
            <div className="new-page-modal" onClick={(event) => event.stopPropagation()}>
              <h3 className="new-page-modal__title">AI 辅助新建 Wiki 页面</h3>
              <p className="new-page-modal__hint">
                输入主题，AI 将参考现有知识库生成结构化初稿。
              </p>
              <input
                className="new-page-modal__input"
                type="text"
                placeholder="例如：量子纠缠、黑洞蒸发、Rust 生命周期…"
                value={newPageTopic}
                onChange={(event) => onNewPageTopicChange(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    void onCreatePageWithAi();
                  }
                }}
                disabled={newPageCreating}
                autoFocus
              />
              {newPageResult && (
                <div className="new-page-modal__result">
                  <p className="new-page-modal__result-title">✅ 已创建：{newPageResult.title}</p>
                  <pre className="new-page-modal__preview">{newPageResult.content_preview}</pre>
                </div>
              )}
              <div className="new-page-modal__actions">
                <button
                  className="new-page-modal__btn new-page-modal__btn--primary"
                  onClick={() => void onCreatePageWithAi()}
                  disabled={newPageCreating || !newPageTopic.trim()}
                >
                  {newPageCreating ? "AI 生成中…" : "生成页面"}
                </button>
                {newPageResult && (
                  <button className="new-page-modal__btn" onClick={onUseNewPageResult}>
                    查看页面
                  </button>
                )}
                <button className="new-page-modal__btn" onClick={onCloseNewPageModal}>
                  关闭
                </button>
              </div>
            </div>
          </div>
        )}
        {allWikiTags.length > 0 ? (
          <div className="wiki-tag-bar">
            <button
              type="button"
              className={`wiki-tag-chip ${wikiActiveTags.size === 0 ? "wiki-tag-chip--active" : ""}`}
              onClick={onClearWikiActiveTags}
            >
              全部
            </button>
            {allWikiTags.map((tag) => (
              <button
                key={tag.name}
                type="button"
                className={`wiki-tag-chip ${wikiActiveTags.has(tag.name) ? "wiki-tag-chip--active" : ""}`}
                onClick={() => onToggleWikiTag(tag.name)}
              >
                {tag.name} <span className="wiki-tag-chip__count">({tag.count})</span>
              </button>
            ))}
          </div>
        ) : null}
        {displayedWikiPages.length ? (
          <div className="wiki-layout">
            <aside className="wiki-layout__tree">
              <div className="wiki-tree__head">
                <div className="wiki-tree__head-title">
                  <h3>Vault 文件树</h3>
                  <span>{displayedWikiPages.length} 页</span>
                </div>
                <div className="wiki-tree__head-actions">
                  <button
                    type="button"
                    className="wiki-tree__action-btn"
                    onClick={onExpandAllWikiFolders}
                    title="全部展开"
                  >
                    展开全部
                  </button>
                  <button
                    type="button"
                    className="wiki-tree__action-btn"
                    onClick={onCollapseAllWikiFolders}
                    title="全部收起"
                  >
                    收起全部
                  </button>
                </div>
              </div>
              <div className="wiki-tree__body">{renderWikiTree()}</div>
            </aside>
            <div className="wiki-layout__list">
              <div className="ask-result__citations">
                {displayedWikiPages.map((page) => {
                  const isActiveCard = isPageActive(page.path);
                  const isDetailForCard = isPageDetailActive(page.path);
                  const isSummaryExpanded = isPageSummaryExpanded(page.path);
                  const canToggleSummary = page.summary.trim().split("\n").length > wikiSummaryPreviewLines;
                  const summaryDisplay = buildWikiSummaryDisplay(page.summary, isSummaryExpanded);
                  const summarySegments = highlightWikiText(summaryDisplay.text);
                  const titleSegments = highlightWikiText(page.title);

                  return (
                    <article key={page.path} className="ask-citation">
                      <div className="ask-citation__top">
                        <code>
                          {titleSegments.map((segment, index) =>
                            segment.matched ? (
                              <mark key={`${page.path}-title-${index}`} className="wiki-summary__mark">
                                {segment.text}
                              </mark>
                            ) : (
                              <span key={`${page.path}-title-${index}`}>{segment.text}</span>
                            ),
                          )}
                        </code>
                        <span>{formatLintCheckedAt(page.updated_at)}</span>
                      </div>
                      <div className="wiki-summary">
                        <p className="wiki-summary__text">
                          {summarySegments.map((segment, index) =>
                            segment.matched ? (
                              <mark key={`${page.path}-summary-${index}`} className="wiki-summary__mark">
                                {segment.text}
                              </mark>
                            ) : (
                              <span key={`${page.path}-summary-${index}`}>{segment.text}</span>
                            ),
                          )}
                        </p>
                        {canToggleSummary ? (
                          <button
                            type="button"
                            className="dev-panel__button wiki-summary__toggle"
                            onClick={() => onToggleWikiSummary(page.path)}
                          >
                            {isSummaryExpanded ? "收起摘要" : "展开摘要"}
                          </button>
                        ) : null}
                      </div>
                      <div className="wiki-card__footer">
                        <code>{resolveDisplayPath(page)}</code>
                        <button
                          type="button"
                          className="dev-panel__button wiki-card__button"
                          onClick={() => {
                            if (isActiveCard && !wikiPageDetailLoading) {
                              onCloseWikiPreview();
                              return;
                            }
                            void onOpenWikiPage(page.path);
                          }}
                          disabled={!isTauri || wikiPageDetailLoading}
                        >
                          {isActiveCard && isDetailForCard ? "收起内容" : "查看内容"}
                        </button>
                      </div>
                      {isActiveCard ? (
                        wikiPageDetailLoading ? (
                          <p className="runtime-hint wiki-inline-status">正在读取页面内容...</p>
                        ) : wikiPageDetailError ? (
                          <p className="runtime-status wiki-inline-status">{wikiPageDetailError}</p>
                        ) : isDetailForCard ? (
                          <div className="wiki-inline-preview">{renderWikiPreview()}</div>
                        ) : null
                      ) : null}
                    </article>
                  );
                })}
              </div>
            </div>
          </div>
        ) : (
          <p className="empty-state">
            {isTauri
              ? "当前没有可展示的 wiki 页面。先执行示例摄入或保存 Query 结果。"
              : "浏览器预览模式下不加载后端 wiki 页面列表。"}
          </p>
        )}
        {!isActiveWikiDetailInList && wikiActivePagePath ? (
          wikiPageDetailLoading ? (
            <p className="runtime-hint wiki-inline-status">正在读取页面内容...</p>
          ) : wikiPageDetailError ? (
            <p className="runtime-status wiki-inline-status">{wikiPageDetailError}</p>
          ) : wikiPageDetail ? (
            <div className="wiki-inline-preview wiki-inline-preview--floating">{renderWikiPreview()}</div>
          ) : null
        ) : null}
      </section>
    </>
  );
}
