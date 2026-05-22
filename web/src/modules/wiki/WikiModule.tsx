import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import { formatLintCheckedAt } from "../../lint-utils";
import {
  resolveDisplayPath,
  isTauriRuntime,
  fetchWikiPageDetail,
  fetchWikiPageCitations,
  searchWikiPages,
  searchWikiPagesHybrid,
  searchWikiPaths,
  saveWikiPage,
  deleteWikiPage,
  renameWikiPage,
  markPageStale,
  listWikiPageHistory,
  getWikiPageHistoryEntry,
  restoreWikiPageFromHistory,
  createWikiPageWithAi,
  fetchRecentWikiPages,
} from "../../tauri-client";
import { useToast } from "../../contexts/ToastContext";
import SelectionToolbar from "./SelectionToolbar";
import AiAssistPreview from "./AiAssistPreview";
import {
  aiAssistWikiEdit,
  listenAiAssistChunk,
  listenAiAssistDone,
  listenAiAssistError,
  type AssistAction,
} from "../../tauri-client/ai-assist";
import {
  simpleHash,
  buildFrontmatterCopyText,
  buildWikiFrontmatterDisplay,
  resolveWikiImportedAtDebugValue,
  hasUnsavedWikiEditChanges,
  buildWikiLineDiff,
  isSameWikiPagePath,
  tokenizeWikiKeyword,
  buildWikiSummaryDisplay,
  buildWikiHighlightSegments,
  sortWikiPages,
  buildWikiTreeNodes,
  writeWikiSortModeToStorage,
  readWikiSortModeFromStorage,
  applyWikiAutocompleteSelection,
  resolveWikiAutocompleteMatch,
  type WikiSortMode,
  type WikiTreeNode,
} from "../../wiki-utils";
import type {
  NewPageResult,
  WikiPageDetail,
  WikiPageCitation,
  WikiPageHistoryEntry,
  WikiPageHistorySummary,
  WikiPageItem,
} from "../../types";

// ─── re-exported types used internally ───────────────────────────────────────

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

const wikiSortModeLabels: Record<WikiSortMode, string> = {
  updated_desc: "更新时间（新到旧）",
  updated_asc: "更新时间（旧到新）",
  title_asc: "标题（A-Z）",
};

const wikiSummaryPreviewLines = 3;

// Helper: collect all folder keys from wiki tree
function collectWikiTreeFolderKeys(nodes: WikiTreeNode[]): Set<string> {
  const keys = new Set<string>();
  const walk = (items: WikiTreeNode[]) => {
    for (const node of items) {
      if (node.kind === "folder") {
        keys.add(node.key);
        walk(node.children);
      }
    }
  };
  walk(nodes);
  return keys;
}

// ─── public API exposed via ref ───────────────────────────────────────────────

export type WikiModuleHandle = {
  openPage: (pagePath: string) => Promise<void>;
};

// ─── props ────────────────────────────────────────────────────────────────────

type WikiModuleProps = {
  /** Page list managed by App (shared with InboxModule / refreshAppData) */
  pages: WikiPageItem[];
  onPagesChange: (pages: WikiPageItem[]) => void;
};

// ─── component ────────────────────────────────────────────────────────────────

const WikiModule = forwardRef<WikiModuleHandle, WikiModuleProps>(function WikiModule(
  { pages, onPagesChange },
  ref,
) {
  const { setStatusMessage } = useToast();

  // ── list/search state ──────────────────────────────────────────────────────
  const [wikiKeyword, setWikiKeyword] = useState("");
  const [wikiSearching, setWikiSearching] = useState(false);
  const [wikiSortMode, setWikiSortMode] = useState<WikiSortMode>(() =>
    readWikiSortModeFromStorage(),
  );
  const [wikiActiveTags, setWikiActiveTags] = useState<Set<string>>(new Set());
  const [wikiTagBarExpanded, setWikiTagBarExpanded] = useState(false);
  const [wikiTagShowAll, setWikiTagShowAll] = useState(false);
  const [wikiTagFilterMode, setWikiTagFilterMode] = useState<"or" | "and">("or");
  const [wikiExpandedPaths, setWikiExpandedPaths] = useState<string[]>([]);
  const [wikiTreeCollapsedFolders, setWikiTreeCollapsedFolders] = useState<Set<string>>(
    new Set(),
  );

  // ── new page modal state ───────────────────────────────────────────────────
  const [newPageTopic, setNewPageTopic] = useState("");
  const [newPageCreating, setNewPageCreating] = useState(false);
  const [newPageResult, setNewPageResult] = useState<NewPageResult | null>(null);
  const [showNewPageModal, setShowNewPageModal] = useState(false);

  // ── page detail / preview state ────────────────────────────────────────────
  const [wikiActivePagePath, setWikiActivePagePath] = useState("");
  const [wikiPageDetail, setWikiPageDetail] = useState<WikiPageDetail | null>(null);
  const [wikiPageCitations, setWikiPageCitations] = useState<WikiPageCitation[]>([]);
  const [wikiPageDetailLoading, setWikiPageDetailLoading] = useState(false);
  const [wikiPageCitationsLoading, setWikiPageCitationsLoading] = useState(false);
  const [wikiPageDetailError, setWikiPageDetailError] = useState("");
  const [wikiPageCitationsError, setWikiPageCitationsError] = useState("");

  // ── frontmatter / debug state ──────────────────────────────────────────────
  const [wikiFrontmatterCollapsed, setWikiFrontmatterCollapsed] = useState(false);
  const [wikiFrontmatterCopiedKey, setWikiFrontmatterCopiedKey] = useState("");
  const [wikiDebugInfoVisible, setWikiDebugInfoVisible] = useState(false);

  // ── edit state ─────────────────────────────────────────────────────────────
  const [wikiEditMode, setWikiEditMode] = useState(false);
  const [wikiEditContent, setWikiEditContent] = useState("");
  const [wikiEditBaselineChecksum, setWikiEditBaselineChecksum] = useState("");
  const [wikiSaveRunning, setWikiSaveRunning] = useState(false);
  const [wikiSaveError, setWikiSaveError] = useState("");
  const [wikiDeleteRunning, setWikiDeleteRunning] = useState(false);

  // ── AI assist state ───────────────────────────────────────────────────────
  const [assistSelectedText, setAssistSelectedText] = useState("");
  const [assistText, setAssistText] = useState("");
  const [assistLoading, setAssistLoading] = useState(false);
  const [assistError, setAssistError] = useState("");

  // ── rename state ───────────────────────────────────────────────────────────
  const [wikiRenameMode, setWikiRenameMode] = useState(false);
  const [wikiRenameInput, setWikiRenameInput] = useState("");
  const [wikiRenameRunning, setWikiRenameRunning] = useState(false);
  const [wikiRenameError, setWikiRenameError] = useState("");

  // ── history state ──────────────────────────────────────────────────────────
  const [wikiHistoryOpen, setWikiHistoryOpen] = useState(false);
  const [wikiHistoryEntries, setWikiHistoryEntries] = useState<WikiPageHistorySummary[]>([]);
  const [wikiHistorySelectedEntry, setWikiHistorySelectedEntry] =
    useState<WikiPageHistoryEntry | null>(null);
  const [wikiHistoryLoading, setWikiHistoryLoading] = useState(false);
  const [wikiHistoryEntryLoading, setWikiHistoryEntryLoading] = useState(false);
  const [wikiHistoryError, setWikiHistoryError] = useState("");

  // ── autocomplete state ─────────────────────────────────────────────────────
  const [wikiAutocompleteOpen, setWikiAutocompleteOpen] = useState(false);
  const [wikiAutocompleteResults, setWikiAutocompleteResults] = useState<string[]>([]);
  const [wikiAutocompleteIndex, setWikiAutocompleteIndex] = useState(0);
  const [wikiAutocompleteQuery, setWikiAutocompleteQuery] = useState("");
  const [wikiAutocompletePos, setWikiAutocompletePos] = useState({ top: 0, left: 0 });
  const wikiEditorRef = useRef<HTMLTextAreaElement>(null);
  const wikiAutocompleteRequestIdRef = useRef(0);

  // ── persist sort mode ──────────────────────────────────────────────────────
  useEffect(() => {
    writeWikiSortModeToStorage(wikiSortMode);
  }, [wikiSortMode]);

  // ── sync expanded paths when pages change ─────────────────────────────────
  useEffect(() => {
    setWikiExpandedPaths((prev) =>
      prev.filter((path) => pages.some((page) => isSameWikiPagePath(page.path, path))),
    );
  }, [pages]);

  // ── derived values ─────────────────────────────────────────────────────────
  const sortedWikiPages = sortWikiPages(pages, wikiSortMode);

  const allWikiTags = useMemo<WikiTagSummary[]>(() => {
    const tagCountMap = new Map<string, number>();
    for (const page of sortedWikiPages) {
      for (const tag of page.tags ?? []) {
        const trimmed = tag.trim();
        if (trimmed) {
          tagCountMap.set(trimmed, (tagCountMap.get(trimmed) ?? 0) + 1);
        }
      }
    }
    return Array.from(tagCountMap.entries())
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name, "zh-CN"));
  }, [sortedWikiPages]);

  const TOP_TAG_COUNT = 15;

  const visibleTags = useMemo(() => {
    if (wikiTagShowAll) return allWikiTags;
    const top = allWikiTags.slice(0, TOP_TAG_COUNT);
    const topNames = new Set(top.map((t) => t.name));
    const extraActive = allWikiTags.slice(TOP_TAG_COUNT).filter((t) => wikiActiveTags.has(t.name));
    return extraActive.length > 0
      ? [...top, ...extraActive.filter((t) => !topNames.has(t.name))]
      : top;
  }, [allWikiTags, wikiTagShowAll, wikiActiveTags]);

  const matchesActiveTags = (pageTags: string[]) => {
    if (wikiActiveTags.size === 0) return true;
    const tags = Array.from(wikiActiveTags);
    return wikiTagFilterMode === "and"
      ? tags.every((tag) => pageTags.includes(tag))
      : tags.some((tag) => pageTags.includes(tag));
  };

  const displayedWikiPages = wikiKeyword.trim()
    ? [...sortedWikiPages]
        .filter((p) => matchesActiveTags(p.tags ?? []))
        .sort((a, b) => {
          const kw = wikiKeyword.toLowerCase();
          const aTitleMatch = a.title.toLowerCase().includes(kw) ? 1 : 0;
          const bTitleMatch = b.title.toLowerCase().includes(kw) ? 1 : 0;
          if (bTitleMatch !== aTitleMatch) return bTitleMatch - aTitleMatch;
          return (b.score ?? 0) - (a.score ?? 0);
        })
    : sortedWikiPages.filter((p) => matchesActiveTags(p.tags ?? []));

  const wikiTreeNodes = useMemo(
    () => buildWikiTreeNodes(displayedWikiPages),
    [displayedWikiPages],
  );

  const wikiTreeFolderKeys = useMemo(
    () => collectWikiTreeFolderKeys(wikiTreeNodes),
    [wikiTreeNodes],
  );

  useEffect(() => {
    setWikiTreeCollapsedFolders((prev) => {
      if (prev.size === 0) return prev;
      const next = new Set<string>();
      for (const key of prev) {
        if (wikiTreeFolderKeys.has(key)) next.add(key);
      }
      if (next.size === prev.size && Array.from(next).every((key) => prev.has(key))) return prev;
      return next;
    });
  }, [wikiTreeFolderKeys]);

  const wikiHasUnsavedChanges = hasUnsavedWikiEditChanges(
    wikiEditMode,
    wikiEditContent,
    wikiPageDetail?.content,
  );

  const isActiveWikiDetailInList = Boolean(
    wikiActivePagePath &&
      sortedWikiPages.some((page) => isSameWikiPagePath(page.path, wikiActivePagePath)),
  );

  const wikiFrontmatterDisplay = buildWikiFrontmatterDisplay(wikiPageDetail);
  const wikiFrontmatterRows = wikiFrontmatterDisplay.rows;
  const wikiFrontmatterEntities = wikiFrontmatterDisplay.entities;

  const wikiImportedAtDebugRaw = resolveWikiImportedAtDebugValue(wikiPageDetail);
  const wikiImportedAtDebugDisplay = wikiImportedAtDebugRaw
    ? formatLintCheckedAt(wikiImportedAtDebugRaw)
    : "";

  const wikiHighlightKeywords = tokenizeWikiKeyword(wikiKeyword);

  const wikiRenderedContent = useMemo(() => {
    const raw = wikiPageDetail?.content ?? "";
    if (!raw) return "";
    const html = marked.parse(raw, { gfm: true, breaks: false }) as string;
    return DOMPurify.sanitize(html);
  }, [wikiPageDetail?.content]);

  const wikiHistoryDiffRows = useMemo(
    () =>
      buildWikiLineDiff(
        wikiPageDetail?.content ?? "",
        wikiHistorySelectedEntry?.content ?? "",
      ),
    [wikiPageDetail?.content, wikiHistorySelectedEntry?.content],
  );

  // ── helpers ────────────────────────────────────────────────────────────────

  function confirmDiscardWikiPreview(reason: "close" | "switch") {
    if (!wikiHasUnsavedChanges) return true;
    const confirmMessage =
      reason === "close"
        ? "当前页面有未保存改动，确认关闭预览吗？"
        : "当前页面有未保存改动，确认切换到其他页面吗？";
    const confirmFn = globalThis.confirm;
    if (typeof confirmFn !== "function") {
      setStatusMessage("当前环境不支持确认弹窗，已保留未保存改动。");
      return false;
    }
    const confirmed = confirmFn(confirmMessage);
    if (!confirmed) setStatusMessage("已取消操作，保留未保存改动。");
    return confirmed;
  }

  const autoRevealWikiPage = (pagePath: string) => {
    const findAncestors = (
      nodes: WikiTreeNode[],
      targetPath: string,
      ancestors: string[] = [],
    ): string[] | null => {
      for (const node of nodes) {
        if (node.kind === "file" && node.pagePath && isSameWikiPagePath(node.pagePath, targetPath)) {
          return ancestors;
        }
        if (node.kind === "folder") {
          const result = findAncestors(node.children, targetPath, [...ancestors, node.key]);
          if (result) return result;
        }
      }
      return null;
    };

    const ancestors = findAncestors(wikiTreeNodes, pagePath);
    if (ancestors && ancestors.length > 0) {
      setWikiTreeCollapsedFolders((prev) => {
        const next = new Set(prev);
        let changed = false;
        for (const ancestorKey of ancestors) {
          if (next.has(ancestorKey)) {
            next.delete(ancestorKey);
            changed = true;
          }
        }
        return changed ? next : prev;
      });
    }

    globalThis.setTimeout(() => {
      const activeNode = document.querySelector(".wiki-tree__file--active");
      if (activeNode) activeNode.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }, 150);
  };

  // ── page open ──────────────────────────────────────────────────────────────

  const handleOpenWikiPage = async (pagePath: string) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法查看 Wiki 页面内容。");
      return;
    }
    const currentPath = wikiPageDetail?.path ?? wikiActivePagePath;
    const isSwitchingPage = currentPath && !isSameWikiPagePath(currentPath, pagePath);
    if (isSwitchingPage) {
      const shouldSwitch = confirmDiscardWikiPreview("switch");
      if (!shouldSwitch) return;
    }

    setWikiActivePagePath(pagePath);
    setWikiPageDetailLoading(true);
    setWikiPageCitationsLoading(true);
    setWikiPageDetailError("");
    setWikiPageCitationsError("");
    setWikiFrontmatterCopiedKey("");
    setWikiFrontmatterCollapsed(false);
    setWikiDebugInfoVisible(false);
    setWikiActiveTags(new Set());
    setWikiEditMode(false);
    setWikiEditContent("");
    setWikiSaveRunning(false);
    setWikiSaveError("");
    setWikiHistoryOpen(false);
    setWikiHistoryEntries([]);
    setWikiHistorySelectedEntry(null);
    setWikiHistoryError("");
    setStatusMessage("");

    try {
      const [detail, citations] = await Promise.all([
        fetchWikiPageDetail(pagePath),
        fetchWikiPageCitations(pagePath),
      ]);

      if (!detail) {
        setWikiPageDetailError("当前环境不支持读取页面内容。");
        setWikiPageDetail(null);
        setWikiPageCitations([]);
        return;
      }

      setWikiPageDetail(detail);
      setWikiEditContent(detail.content ?? "");
      setWikiEditMode(false);
      setWikiSaveRunning(false);
      setWikiSaveError("");
      setWikiPageCitations(citations ?? []);
      if (citations === null) {
        setWikiPageCitationsError("当前环境不支持读取页面引用。");
      }
      setStatusMessage(`已打开页面：${detail.title}`);
      autoRevealWikiPage(pagePath);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setWikiPageDetailError(`读取页面失败：${message}`);
      setWikiPageCitationsError("");
      setWikiPageDetail(null);
      setWikiPageCitations([]);
    } finally {
      setWikiPageDetailLoading(false);
      setWikiPageCitationsLoading(false);
    }
  };

  // ── expose openPage via ref ────────────────────────────────────────────────
  useImperativeHandle(ref, () => ({
    openPage: handleOpenWikiPage,
  }));

  // ── close preview ──────────────────────────────────────────────────────────

  const handleCloseWikiPreview = () => {
    const shouldClose = confirmDiscardWikiPreview("close");
    if (!shouldClose) return;

    setWikiActivePagePath("");
    setWikiPageDetail(null);
    setWikiPageCitations([]);
    setWikiPageDetailError("");
    setWikiPageCitationsError("");
    setWikiFrontmatterCopiedKey("");
    setWikiFrontmatterCollapsed(false);
    setWikiDebugInfoVisible(false);
    setWikiEditMode(false);
    setWikiEditContent("");
    setWikiSaveRunning(false);
    setWikiSaveError("");
    setWikiHistoryOpen(false);
    setWikiHistoryEntries([]);
    setWikiHistorySelectedEntry(null);
    setWikiHistoryError("");
    setStatusMessage("已关闭页面预览。");
  };

  // ── search / reset ─────────────────────────────────────────────────────────

  const handleSearchWikiPages = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法搜索 Wiki 页面。");
      return;
    }
    setWikiSearching(true);
    setStatusMessage("");
    try {
      const result = await searchWikiPagesHybrid(wikiKeyword.trim());
      onPagesChange(result);
      if (wikiKeyword.trim()) {
        setStatusMessage(`Wiki 搜索完成：关键词"${wikiKeyword.trim()}"，命中 ${result.length} 页。`);
      } else {
        setStatusMessage(`已刷新最近 Wiki 页面：${result.length} 页。`);
      }
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`搜索 Wiki 页面失败：${message}`);
    } finally {
      setWikiSearching(false);
    }
  };

  const handleWikiKeywordKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (wikiSearching) return;
    void handleSearchWikiPages();
  };

  const handleResetWikiPages = async () => {
    setWikiKeyword("");
    setWikiActiveTags(new Set());
    setWikiExpandedPaths([]);
    setWikiTreeCollapsedFolders(new Set());
    // Reload recent pages
    try {
      const result = await fetchRecentWikiPages();
      onPagesChange(result);
    } catch (_) {
      // ignore
    }
    setStatusMessage("已恢复显示最近 Wiki 页面。");
  };

  // ── new page modal ─────────────────────────────────────────────────────────

  const handleOpenNewWikiPageModal = () => {
    setShowNewPageModal(true);
    setNewPageResult(null);
    setNewPageTopic("");
  };

  const handleCloseNewWikiPageModal = () => {
    setShowNewPageModal(false);
  };

  const handleUseNewWikiPageResult = () => {
    if (!newPageResult) return;
    setShowNewPageModal(false);
    setWikiKeyword(newPageResult.title);
  };

  const handleCreatePageWithAi = async () => {
    const topic = newPageTopic.trim();
    if (!topic || newPageCreating) return;
    setNewPageCreating(true);
    setNewPageResult(null);
    try {
      const result = await createWikiPageWithAi(topic);
      setNewPageResult(result);
      setStatusMessage(`已创建页面「${result.title}」`);
      setWikiKeyword(result.title);
      void handleSearchWikiPages();
    } catch (err) {
      setStatusMessage(`创建失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setNewPageCreating(false);
    }
  };

  // ── tag filtering ──────────────────────────────────────────────────────────

  const handleClearWikiActiveTags = () => {
    setWikiActiveTags(new Set());
  };

  const handleToggleWikiActiveTag = (tagName: string) => {
    setWikiActiveTags((prev) => {
      const next = new Set(prev);
      if (next.has(tagName)) {
        next.delete(tagName);
      } else {
        next.add(tagName);
      }
      return next;
    });
  };

  // ── summary expand/collapse ────────────────────────────────────────────────

  const handleToggleWikiSummary = (pagePath: string) => {
    setWikiExpandedPaths((prev) => {
      const exists = prev.some((path) => isSameWikiPagePath(path, pagePath));
      if (exists) return prev.filter((path) => !isSameWikiPagePath(path, pagePath));
      return [...prev, pagePath];
    });
  };

  const isWikiPageSummaryExpanded = (pagePath: string) =>
    wikiExpandedPaths.some((path) => isSameWikiPagePath(path, pagePath));

  const highlightWikiText = (text: string): WikiHighlightSegment[] =>
    buildWikiHighlightSegments(text, wikiHighlightKeywords);

  const isWikiPageActive = (pagePath: string) =>
    isSameWikiPagePath(pagePath, wikiActivePagePath);

  const isWikiPageDetailActive = (pagePath: string) =>
    Boolean(wikiPageDetail && isSameWikiPagePath(pagePath, wikiPageDetail.path));

  // ── tree folder expand/collapse ────────────────────────────────────────────

  const toggleWikiTreeFolder = (folderKey: string) => {
    setWikiTreeCollapsedFolders((prev) => {
      const next = new Set(prev);
      if (next.has(folderKey)) {
        next.delete(folderKey);
      } else {
        next.add(folderKey);
      }
      return next;
    });
  };

  const expandAllWikiFolders = () => {
    setWikiTreeCollapsedFolders(new Set());
  };

  const collapseAllWikiFolders = () => {
    setWikiTreeCollapsedFolders(new Set(wikiTreeFolderKeys));
  };

  // ── AI assist ─────────────────────────────────────────────────────────────

  const handleAssistAction = useCallback(async (action: AssistAction) => {
    if (!wikiEditorRef.current) return;
    const ta = wikiEditorRef.current;
    const selected = ta.value.substring(ta.selectionStart, ta.selectionEnd);
    const context = ta.value.substring(
      Math.max(0, ta.selectionStart - 500),
      Math.min(ta.value.length, ta.selectionEnd + 500),
    );
    const title = wikiPageDetail?.title ?? "";

    setAssistText("");
    setAssistError("");
    setAssistLoading(true);

    const unlistenChunk = await listenAiAssistChunk(({ chunk }) => {
      setAssistText((prev) => prev + chunk);
    });
    const unlistenDone = await listenAiAssistDone(() => {
      setAssistLoading(false);
    });
    const unlistenError = await listenAiAssistError(({ error }) => {
      setAssistError(error);
      setAssistLoading(false);
    });

    try {
      await aiAssistWikiEdit(action, selected, context, title);
    } catch (err) {
      setAssistError(String(err));
      setAssistLoading(false);
    } finally {
      unlistenChunk();
      unlistenDone();
      unlistenError();
    }
  }, [wikiPageDetail]);

  const handleAssistAccept = useCallback(() => {
    if (!wikiEditorRef.current || !assistText) return;
    const ta = wikiEditorRef.current;
    const before = ta.value.substring(0, ta.selectionEnd);
    const after = ta.value.substring(ta.selectionEnd);
    const newContent = before + assistText + after;
    setWikiEditContent(newContent);
    setAssistText("");
    setAssistError("");
    setAssistSelectedText("");
  }, [assistText]);

  const handleAssistReject = useCallback(() => {
    setAssistText("");
    setAssistError("");
    setAssistSelectedText("");
    setAssistLoading(false);
  }, []);

  // ── edit ───────────────────────────────────────────────────────────────────

  const handleStartWikiEdit = () => {
    if (!wikiPageDetail) return;
    if (!isTauriRuntime()) {
      setWikiSaveError("浏览器预览模式下不支持编辑保存，请在桌面应用中操作。");
      setStatusMessage("浏览器预览模式下无法保存 Wiki 页面。");
      return;
    }
    setWikiSaveError("");
    setWikiEditContent(wikiPageDetail.content ?? "");
    setWikiEditBaselineChecksum(simpleHash(wikiPageDetail.content ?? ""));
    setWikiEditMode(true);
  };

  const handleCancelWikiEdit = () => {
    setWikiEditMode(false);
    setWikiSaveError("");
    setWikiEditContent(wikiPageDetail?.content ?? "");
  };

  const updateWikiAutocompletePosition = (cursor: number, contentOverride?: string) => {
    if (!wikiEditorRef.current) return;
    const textarea = wikiEditorRef.current;
    const value = typeof contentOverride === "string" ? contentOverride : textarea.value;
    const mirror = document.createElement("div");
    const marker = document.createElement("span");
    const computed = window.getComputedStyle(textarea);
    const mirroredProps = [
      "box-sizing", "width", "height",
      "padding-top", "padding-right", "padding-bottom", "padding-left",
      "border-top-width", "border-right-width", "border-bottom-width", "border-left-width",
      "font-family", "font-size", "font-style", "font-weight", "font-variant", "font-stretch",
      "line-height", "letter-spacing", "text-transform", "text-indent", "text-align",
      "white-space", "word-break", "overflow-wrap",
    ];
    mirroredProps.forEach((prop) => {
      mirror.style.setProperty(prop, computed.getPropertyValue(prop));
    });
    mirror.style.position = "absolute";
    mirror.style.visibility = "hidden";
    mirror.style.pointerEvents = "none";
    mirror.style.overflow = "hidden";
    mirror.style.whiteSpace = "pre-wrap";
    mirror.style.wordBreak = "break-word";
    mirror.style.left = "-9999px";
    mirror.style.top = "0";
    mirror.textContent = value.slice(0, cursor);
    marker.textContent = "​";
    mirror.appendChild(marker);
    document.body.appendChild(mirror);
    const lineHeight = Number.parseFloat(computed.lineHeight) || 20;
    const rawTop = marker.offsetTop - textarea.scrollTop + lineHeight + 6;
    const rawLeft = marker.offsetLeft - textarea.scrollLeft + 4;
    const maxLeft = Math.max(8, textarea.clientWidth - 260);
    document.body.removeChild(mirror);
    setWikiAutocompletePos({
      top: textarea.offsetTop + Math.max(8, rawTop),
      left: textarea.offsetLeft + Math.max(8, Math.min(rawLeft, maxLeft)),
    });
  };

  const handleWikiAutocompleteSelect = (path: string) => {
    if (!wikiEditorRef.current) return;
    const textarea = wikiEditorRef.current;
    const start = textarea.selectionStart;
    const applied = applyWikiAutocompleteSelection({
      content: wikiEditContent,
      cursor: start,
      path,
    });
    if (!applied) {
      setWikiAutocompleteOpen(false);
      return;
    }
    setWikiEditContent(applied.content);
    setWikiAutocompleteOpen(false);
    setWikiAutocompleteResults([]);
    setTimeout(() => {
      textarea.focus();
      textarea.setSelectionRange(applied.cursor, applied.cursor);
    }, 10);
  };

  const handleWikiEditorChange = async (val: string) => {
    setWikiEditContent(val);
    if (!wikiEditorRef.current) return;
    const textarea = wikiEditorRef.current;
    const start = textarea.selectionStart;
    const textBefore = val.slice(0, start);
    const match = resolveWikiAutocompleteMatch(textBefore);
    if (match) {
      const query = match.query;
      const currentRequestId = wikiAutocompleteRequestIdRef.current + 1;
      wikiAutocompleteRequestIdRef.current = currentRequestId;
      setWikiAutocompleteQuery(query);
      updateWikiAutocompletePosition(start, val);
      setWikiAutocompleteOpen(true);
      setWikiAutocompleteIndex(0);
      try {
        const results = await searchWikiPaths(query);
        if (wikiAutocompleteRequestIdRef.current !== currentRequestId) return;
        setWikiAutocompleteResults(results.slice(0, 10));
      } catch (e) {
        console.warn("自动补全搜索失败", e);
        if (wikiAutocompleteRequestIdRef.current !== currentRequestId) return;
        setWikiAutocompleteResults([]);
      }
    } else {
      wikiAutocompleteRequestIdRef.current += 1;
      setWikiAutocompleteOpen(false);
      setWikiAutocompleteResults([]);
    }
  };

  // ── save ───────────────────────────────────────────────────────────────────

  const handleSaveWikiPage = async () => {
    if (!wikiPageDetail) return;
    if (!isTauriRuntime()) {
      setWikiSaveError("浏览器预览模式下不支持编辑保存，请在桌面应用中操作。");
      setStatusMessage("浏览器预览模式下无法保存 Wiki 页面。");
      return;
    }
    const targetPath = wikiPageDetail.path;
    setWikiSaveRunning(true);
    setWikiSaveError("");
    setStatusMessage("");
    try {
      const result = await saveWikiPage(
        targetPath,
        wikiEditContent,
        wikiEditBaselineChecksum || undefined,
      );
      if (!result) {
        setWikiSaveError("当前环境不支持保存页面。请检查 Tauri 后端是否可用。");
        return;
      }
      setWikiEditMode(false);
      setWikiEditBaselineChecksum("");
      // Refresh page list
      try {
        const updated = await fetchRecentWikiPages();
        onPagesChange(updated);
      } catch (_) { /* ignore */ }
      await handleOpenWikiPage(targetPath);
      setStatusMessage(result.message || `已保存页面：${targetPath}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      if (message.includes("checksum") || message.includes("校验和")) {
        setWikiSaveError("保存失败：页面在编辑期间被外部修改，已阻止覆盖。请重新打开页面后再试。");
        return;
      }
      const errorMessage = `保存页面失败：${message}`;
      setWikiSaveError(errorMessage);
      setStatusMessage(errorMessage);
    } finally {
      setWikiSaveRunning(false);
    }
  };

  // ── stale toggle ───────────────────────────────────────────────────────────

  const handleToggleStale = async () => {
    if (!wikiPageDetail) return;
    const currentStale = wikiPageDetail.frontmatter?.stale === true;
    const nextStale = !currentStale;
    try {
      await markPageStale(wikiPageDetail.path, nextStale);
      const updated = await fetchWikiPageDetail(wikiPageDetail.path);
      if (updated) setWikiPageDetail(updated);
      setStatusMessage(nextStale ? "已标记为过时。" : "已取消过时标记。");
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`操作失败：${message}`);
    }
  };

  // ── delete ─────────────────────────────────────────────────────────────────

  const handleDeleteWikiPage = async () => {
    if (!wikiPageDetail) return;
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法删除 Wiki 页面。");
      return;
    }
    const confirmFn = globalThis.confirm;
    if (typeof confirmFn !== "function") {
      setStatusMessage("当前环境不支持确认弹窗，操作已取消。");
      return;
    }
    const confirmed = confirmFn(`确认删除页面「${wikiPageDetail.title}」吗？此操作不可恢复。`);
    if (!confirmed) return;

    const targetPath = wikiPageDetail.path;
    setWikiDeleteRunning(true);
    setStatusMessage("");
    try {
      const result = await deleteWikiPage(targetPath);
      if (!result) {
        setStatusMessage("当前环境不支持删除页面。请检查 Tauri 后端是否可用。");
        return;
      }
      handleCloseWikiPreview();
      onPagesChange(pages.filter((p) => !isSameWikiPagePath(p.path, targetPath)));
      setStatusMessage(result.message || `已删除页面：${targetPath}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`删除页面失败：${message}`);
    } finally {
      setWikiDeleteRunning(false);
    }
  };

  // ── rename ─────────────────────────────────────────────────────────────────

  const handleStartWikiRename = () => {
    if (!wikiPageDetail) return;
    const currentName = wikiPageDetail.path.split(/[/\\]/).pop() ?? "";
    setWikiRenameInput(currentName.replace(/\.md$/, ""));
    setWikiRenameError("");
    setWikiRenameMode(true);
  };

  const handleCancelWikiRename = () => {
    setWikiRenameMode(false);
    setWikiRenameInput("");
    setWikiRenameError("");
  };

  const handleConfirmWikiRename = async () => {
    if (!wikiPageDetail) return;
    if (!isTauriRuntime()) {
      setWikiRenameError("浏览器预览模式下无法重命名。");
      return;
    }
    const newName = wikiRenameInput.trim();
    if (!newName) {
      setWikiRenameError("文件名不能为空。");
      return;
    }
    setWikiRenameRunning(true);
    setWikiRenameError("");
    try {
      const result = await renameWikiPage(wikiPageDetail.path, newName);
      if (!result) {
        setWikiRenameError("当前环境不支持重命名。");
        return;
      }
      handleCancelWikiRename();
      // Refresh page list then reopen
      try {
        const updated = await fetchRecentWikiPages();
        onPagesChange(updated);
      } catch (_) { /* ignore */ }
      await handleOpenWikiPage(result.new_path);
      setStatusMessage(result.message);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWikiRenameError(`重命名失败：${message}`);
    } finally {
      setWikiRenameRunning(false);
    }
  };

  // ── frontmatter copy ───────────────────────────────────────────────────────

  const handleCopyFrontmatterValue = async (field: string, value: string) => {
    const normalized = value.trim();
    if (!normalized) {
      setStatusMessage(`字段 ${field} 为空，已跳过复制。`);
      return;
    }
    const clipboard = globalThis.navigator?.clipboard;
    if (!clipboard?.writeText) {
      setStatusMessage("当前环境不支持复制到剪贴板。");
      return;
    }
    try {
      await clipboard.writeText(buildFrontmatterCopyText(field, normalized));
      setWikiFrontmatterCopiedKey(field);
      setStatusMessage(`已复制 ${field}。`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`复制失败：${message}`);
    }
  };

  // ── history ────────────────────────────────────────────────────────────────

  const handleOpenWikiHistory = async () => {
    if (!wikiPageDetail) return;
    setWikiHistoryOpen(true);
    setWikiHistoryLoading(true);
    setWikiHistoryError("");
    setWikiHistorySelectedEntry(null);
    try {
      const entries = await listWikiPageHistory(wikiPageDetail.path, 30);
      setWikiHistoryEntries(entries);
      if (!isTauriRuntime()) {
        setWikiHistoryError("浏览器预览模式下无法读取页面历史。");
      } else if (entries.length === 0) {
        setWikiHistoryError("当前页面暂无历史版本。");
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWikiHistoryError(`读取历史版本失败：${message}`);
      setWikiHistoryEntries([]);
    } finally {
      setWikiHistoryLoading(false);
    }
  };

  const handleSelectWikiHistoryEntry = async (entry: WikiPageHistorySummary) => {
    setWikiHistoryEntryLoading(true);
    setWikiHistoryError("");
    try {
      const detail = await getWikiPageHistoryEntry(entry.id);
      if (!detail) {
        setWikiHistoryError("无法读取该历史版本内容。");
        setWikiHistorySelectedEntry(null);
        return;
      }
      setWikiHistorySelectedEntry(detail);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWikiHistoryError(`读取历史内容失败：${message}`);
      setWikiHistorySelectedEntry(null);
    } finally {
      setWikiHistoryEntryLoading(false);
    }
  };

  const handleRestoreWikiHistory = async () => {
    if (!wikiHistorySelectedEntry) return;
    if (!globalThis.confirm("确定恢复到此历史版本吗？当前内容将被覆盖。")) return;
    setWikiHistoryLoading(true);
    setWikiHistoryError("");
    try {
      const result = await restoreWikiPageFromHistory(wikiHistorySelectedEntry.id);
      if (!result) {
        setWikiHistoryError("恢复失败（后端不可用）。");
        return;
      }
      setWikiHistoryOpen(false);
      setWikiHistorySelectedEntry(null);
      if (wikiPageDetail?.path) {
        const updated = await fetchWikiPageDetail(wikiPageDetail.path);
        if (updated) {
          setWikiPageDetail(updated);
          setWikiEditContent(updated.content ?? "");
        }
      }
      setStatusMessage(`已从历史版本恢复页面：${wikiPageDetail?.path ?? ""}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWikiHistoryError(`恢复失败：${message}`);
    } finally {
      setWikiHistoryLoading(false);
    }
  };

  const handleCloseWikiHistory = () => {
    setWikiHistoryOpen(false);
    setWikiHistorySelectedEntry(null);
    setWikiHistoryError("");
  };

  // ── tree render ────────────────────────────────────────────────────────────

  const renderWikiTreeNodes = (nodes: WikiTreeNode[], depth = 0): ReactNode => (
    <ul className="wiki-tree__list">
      {nodes.map((node) => {
        if (node.kind === "folder") {
          const collapsed = wikiTreeCollapsedFolders.has(node.key);
          return (
            <li key={node.key} className="wiki-tree__item">
              <button
                type="button"
                className="wiki-tree__folder"
                style={{ paddingLeft: `${depth * 14 + 8}px` }}
                onClick={() => toggleWikiTreeFolder(node.key)}
              >
                <span className="wiki-tree__caret" aria-hidden="true">
                  {collapsed ? "▸" : "▾"}
                </span>
                <span className="wiki-tree__icon" aria-hidden="true">
                  {collapsed ? "📁" : "📂"}
                </span>
                <span className="wiki-tree__name">{node.name}</span>
              </button>
              {!collapsed && node.children.length > 0
                ? renderWikiTreeNodes(node.children, depth + 1)
                : null}
            </li>
          );
        }

        const isActive = node.pagePath
          ? isSameWikiPagePath(node.pagePath, wikiActivePagePath)
          : false;
        const isMarkdown = node.name.toLowerCase().endsWith(".md");

        return (
          <li key={node.key} className="wiki-tree__item">
            <button
              type="button"
              className={`wiki-tree__file ${isActive ? "wiki-tree__file--active" : ""}`}
              style={{ paddingLeft: `${depth * 14 + 28}px` }}
              onClick={() => {
                if (!node.pagePath) return;
                void handleOpenWikiPage(node.pagePath);
              }}
              disabled={!isTauriRuntime() || wikiPageDetailLoading}
              title={node.fullPath}
            >
              <span className="wiki-tree__icon" aria-hidden="true">
                {isMarkdown ? "📄" : "📎"}
              </span>
              <span className="wiki-tree__name">{node.name}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );

  const renderWikiTree = () => renderWikiTreeNodes(wikiTreeNodes);

  // ── preview render ─────────────────────────────────────────────────────────

  const renderWikiPreview = () => (
    <article className="wiki-preview">
      <div className="wiki-preview__head">
        <div className="wiki-preview__title">
          <h3>{wikiPageDetail?.title ?? "页面详情"}</h3>
          {wikiPageDetail ? <p><code>{resolveDisplayPath(wikiPageDetail)}</code></p> : null}
        </div>
        <div className="wiki-preview__actions">
          {wikiPageDetail ? <span>{formatLintCheckedAt(wikiPageDetail.updated_at)}</span> : null}
          {wikiPageDetail ? (
            wikiEditMode ? (
              <>
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--accent"
                  onClick={() => void handleSaveWikiPage()}
                  disabled={wikiSaveRunning || !isTauriRuntime()}
                >
                  {wikiSaveRunning ? "保存中..." : "保存"}
                </button>
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={handleCancelWikiEdit}
                  disabled={wikiSaveRunning}
                >
                  取消
                </button>
                <span className="wiki-editor__charcount">
                  {wikiEditContent.length.toLocaleString()} 字符
                </span>
              </>
            ) : (
              <>
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={handleStartWikiEdit}
                  disabled={wikiSaveRunning || !isTauriRuntime()}
                  title={isTauriRuntime() ? "编辑当前页面内容" : "浏览器预览模式下不可编辑"}
                >
                  编辑内容
                </button>
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={handleStartWikiRename}
                  disabled={wikiDeleteRunning || !isTauriRuntime()}
                  title={isTauriRuntime() ? "重命名文件" : "浏览器预览模式下不可重命名"}
                >
                  重命名
                </button>
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={() => void handleOpenWikiHistory()}
                  disabled={!isTauriRuntime()}
                  title={isTauriRuntime() ? "查看当前页面历史版本" : "浏览器预览模式下不可读取历史"}
                >
                  历史版本
                </button>
                <button
                  type="button"
                  className={`dev-panel__button wiki-detail__action-btn ${wikiPageDetail.frontmatter?.stale ? "wiki-detail__action-btn--stale-active" : ""}`}
                  onClick={() => void handleToggleStale()}
                  title={wikiPageDetail.frontmatter?.stale ? "取消过时标记" : "标记为过时"}
                >
                  {wikiPageDetail.frontmatter?.stale ? "↺ 取消过时" : "⚑ 标记过时"}
                </button>
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--danger"
                  onClick={() => void handleDeleteWikiPage()}
                  disabled={wikiDeleteRunning || !isTauriRuntime()}
                  title={isTauriRuntime() ? "删除当前页面（不可恢复）" : "浏览器预览模式下不可删除"}
                >
                  {wikiDeleteRunning ? "删除中..." : "删除"}
                </button>
              </>
            )
          ) : null}
          <button type="button" className="dev-panel__button" onClick={handleCloseWikiPreview}>
            关闭预览
          </button>
        </div>
      </div>
      {wikiHistoryOpen ? (
        <div
          className="wiki-history-modal"
          role="dialog"
          aria-modal="true"
          aria-label="Wiki 页面历史版本"
          onClick={handleCloseWikiHistory}
        >
          <div className="wiki-history-modal__panel" onClick={(event) => event.stopPropagation()}>
            <div className="wiki-history-modal__head">
              <div>
                <h3>历史版本</h3>
                <p>{wikiPageDetail?.title ?? "当前页面"}</p>
              </div>
              <button type="button" className="dev-panel__button" onClick={handleCloseWikiHistory}>
                关闭
              </button>
            </div>
            {wikiHistoryError ? <p className="runtime-status">{wikiHistoryError}</p> : null}
            <div className="wiki-history-modal__body">
              <aside className="wiki-history-list" aria-label="历史版本列表">
                {wikiHistoryLoading ? <p className="runtime-hint">正在读取历史版本...</p> : null}
                {!wikiHistoryLoading && wikiHistoryEntries.length === 0 ? (
                  <p className="runtime-hint">暂无可展示的历史版本。</p>
                ) : null}
                {wikiHistoryEntries.map((entry) => (
                  <button
                    key={entry.id}
                    type="button"
                    className={`wiki-history-list__item ${
                      wikiHistorySelectedEntry?.id === entry.id ? "wiki-history-list__item--active" : ""
                    }`}
                    onClick={() => void handleSelectWikiHistoryEntry(entry)}
                  >
                    <span>{formatLintCheckedAt(entry.created_at)}</span>
                    <code>{entry.content_hash}</code>
                  </button>
                ))}
              </aside>
              <section className="wiki-history-diff">
                <div className="wiki-history-diff__head">
                  <h4>当前内容 vs 历史内容</h4>
                  <div className="wiki-history-diff__head-actions">
                    <span>
                      {wikiHistoryEntryLoading
                        ? "加载中..."
                        : wikiHistorySelectedEntry
                          ? `${wikiHistoryDiffRows.length} 行`
                          : "请选择一个版本"}
                    </span>
                    {wikiHistorySelectedEntry && !wikiHistoryEntryLoading && isTauriRuntime() ? (
                      <button
                        type="button"
                        className="dev-panel__button dev-panel__button--danger"
                        onClick={() => void handleRestoreWikiHistory()}
                      >
                        恢复到此版本
                      </button>
                    ) : null}
                  </div>
                </div>
                {!wikiHistorySelectedEntry ? (
                  <p className="runtime-hint">点击左侧版本后查看纯文本行级 diff。</p>
                ) : (
                  <div className="wiki-history-diff__rows">
                    {wikiHistoryDiffRows.map((row, index) => (
                      <div
                        key={`${row.kind}-${index}-${row.oldLineNumber ?? 0}-${row.newLineNumber ?? 0}`}
                        className={`wiki-history-diff__row wiki-history-diff__row--${row.kind}`}
                      >
                        <span className="wiki-history-diff__sign">
                          {row.kind === "added" ? "+" : row.kind === "removed" ? "-" : " "}
                        </span>
                        <span className="wiki-history-diff__line-no">
                          {row.kind === "added" ? row.newLineNumber : row.oldLineNumber}
                        </span>
                        <code>{row.line || " "}</code>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            </div>
          </div>
        </div>
      ) : null}
      {wikiRenameMode ? (
        <div className="wiki-rename-bar">
          <input
            type="text"
            className="wiki-rename-bar__input"
            value={wikiRenameInput}
            onChange={(e) => setWikiRenameInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void handleConfirmWikiRename();
              if (e.key === "Escape") handleCancelWikiRename();
            }}
            placeholder="新文件名（不含 .md）"
            disabled={wikiRenameRunning}
            autoFocus
          />
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={() => void handleConfirmWikiRename()}
            disabled={wikiRenameRunning}
          >
            {wikiRenameRunning ? "重命名中..." : "确认"}
          </button>
          <button
            type="button"
            className="dev-panel__button"
            onClick={handleCancelWikiRename}
            disabled={wikiRenameRunning}
          >
            取消
          </button>
          {wikiRenameError ? <span className="wiki-rename-bar__error">{wikiRenameError}</span> : null}
        </div>
      ) : null}
      {!isTauriRuntime() ? (
        <p className="runtime-hint wiki-preview__editor-hint">浏览器预览模式下仅支持查看，不支持编辑保存。</p>
      ) : null}
      {wikiSaveError ? <p className="runtime-status wiki-preview__save-error">{wikiSaveError}</p> : null}
      {wikiPageDetail?.frontmatter?.stale === true && (
        <div className="wiki-stale-banner">
          <span className="wiki-stale-banner__icon">⚠</span>
          <span className="wiki-stale-banner__text">此页面已标记为过时，内容可能不再准确</span>
        </div>
      )}
      {wikiFrontmatterDisplay.hasMeta ? (
        <div className="wiki-preview__meta">
          <div className="wiki-preview__meta-head">
            <h4>Frontmatter</h4>
            <div className="wiki-preview__meta-head-actions">
              <span>{wikiFrontmatterDisplay.totalCount} 项</span>
              <button
                type="button"
                className="dev-panel__button wiki-preview__meta-toggle"
                onClick={() => setWikiFrontmatterCollapsed((value) => !value)}
              >
                {wikiFrontmatterCollapsed ? "展开" : "折叠"}
              </button>
            </div>
          </div>
          {wikiFrontmatterCollapsed ? (
            <p className="runtime-hint">Frontmatter 已折叠，点击"展开"查看详情。</p>
          ) : (
            <>
              {wikiFrontmatterRows.length ? (
                <div className="wiki-preview__meta-grid">
                  {wikiFrontmatterRows.map((item) => (
                    <div key={item.key} className="wiki-preview__meta-item">
                      <div className="wiki-preview__meta-item-head">
                        <span>{item.label}</span>
                        <button
                          type="button"
                          className="dev-panel__button wiki-preview__meta-copy"
                          onClick={() => void handleCopyFrontmatterValue(item.key, item.value)}
                        >
                          {wikiFrontmatterCopiedKey === item.key ? "已复制" : "复制"}
                        </button>
                      </div>
                      <code>{item.displayValue}</code>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="runtime-hint">未解析出可展示的 frontmatter 标量字段。</p>
              )}
              {wikiFrontmatterEntities.length ? (
                <div className="wiki-preview__meta-item wiki-preview__meta-item--entities">
                  <div className="wiki-preview__meta-item-head">
                    <span>entities</span>
                    <button
                      type="button"
                      className="dev-panel__button wiki-preview__meta-copy"
                      onClick={() =>
                        void handleCopyFrontmatterValue(
                          "entities",
                          wikiFrontmatterEntities.join(", "),
                        )
                      }
                    >
                      {wikiFrontmatterCopiedKey === "entities" ? "已复制" : "复制"}
                    </button>
                  </div>
                  <div className="wiki-preview__entity-list">
                    {wikiFrontmatterEntities.map((entity, index) => (
                      <code key={`${entity}-${index}`}>{entity}</code>
                    ))}
                  </div>
                </div>
              ) : null}
            </>
          )}
        </div>
      ) : null}
      {wikiEditMode ? (
        <div className="wiki-preview__editor-wrap" style={{ position: "relative" }}>
          <textarea
            className="wiki-preview__editor"
            ref={wikiEditorRef}
            value={wikiEditContent}
            onChange={(event) => void handleWikiEditorChange(event.target.value)}
            onKeyDown={(event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
              if (wikiAutocompleteOpen) {
                if (event.key === "Escape") {
                  event.preventDefault();
                  setWikiAutocompleteOpen(false);
                  return;
                }
                if (wikiAutocompleteResults.length > 0) {
                  if (event.key === "ArrowDown") {
                    event.preventDefault();
                    setWikiAutocompleteIndex((i) => (i + 1) % wikiAutocompleteResults.length);
                    return;
                  }
                  if (event.key === "ArrowUp") {
                    event.preventDefault();
                    setWikiAutocompleteIndex(
                      (i) => (i - 1 + wikiAutocompleteResults.length) % wikiAutocompleteResults.length,
                    );
                    return;
                  }
                  if (event.key === "Enter" || event.key === "Tab") {
                    event.preventDefault();
                    handleWikiAutocompleteSelect(wikiAutocompleteResults[wikiAutocompleteIndex]);
                    return;
                  }
                }
              }
              if ((event.ctrlKey || event.metaKey) && event.key === "s") {
                event.preventDefault();
                if (!wikiSaveRunning) void handleSaveWikiPage();
              }
            }}
            onClick={(event) => {
              if (!wikiAutocompleteOpen) return;
              updateWikiAutocompletePosition(
                event.currentTarget.selectionStart,
                event.currentTarget.value,
              );
            }}
            onKeyUp={(event) => {
              if (!wikiAutocompleteOpen) return;
              updateWikiAutocompletePosition(
                event.currentTarget.selectionStart,
                event.currentTarget.value,
              );
            }}
            onMouseUp={(event) => {
              const ta = event.currentTarget;
              const sel = ta.value.substring(ta.selectionStart, ta.selectionEnd).trim();
              setAssistSelectedText(sel);
              if (!sel) { setAssistText(""); setAssistError(""); }
            }}
            onSelect={(event) => {
              const ta = event.currentTarget;
              const sel = ta.value.substring(ta.selectionStart, ta.selectionEnd).trim();
              setAssistSelectedText(sel);
              if (!sel) { setAssistText(""); setAssistError(""); }
            }}
            onScroll={() => {
              if (!wikiAutocompleteOpen || !wikiEditorRef.current) return;
              updateWikiAutocompletePosition(wikiEditorRef.current.selectionStart);
            }}
            onBlur={() => {
              setTimeout(() => setWikiAutocompleteOpen(false), 120);
            }}
            disabled={wikiSaveRunning}
            spellCheck={false}
            rows={16}
          />
          {wikiAutocompleteOpen && (
            <div
              className="wikilink-autocomplete"
              style={{ top: wikiAutocompletePos.top, left: wikiAutocompletePos.left }}
            >
              <div className="wikilink-autocomplete__head">
                插入 Wiki 链接{wikiAutocompleteQuery ? `：${wikiAutocompleteQuery}` : ""}
              </div>
              {wikiAutocompleteResults.length > 0 ? (
                <ul className="wikilink-autocomplete__list">
                  {wikiAutocompleteResults.map((path, idx) => (
                    <li
                      key={path}
                      className={`wikilink-autocomplete__item ${idx === wikiAutocompleteIndex ? "wikilink-autocomplete__item--selected" : ""}`}
                      onMouseDown={(event) => event.preventDefault()}
                      onClick={() => handleWikiAutocompleteSelect(path)}
                    >
                      {path}
                    </li>
                  ))}
                </ul>
              ) : (
                <div className="wikilink-autocomplete__empty">未找到匹配页面</div>
              )}
            </div>
          )}
          {assistSelectedText && !assistText && !assistLoading && !assistError ? (
            <SelectionToolbar
              onAction={(action) => { void handleAssistAction(action); }}
              disabled={assistLoading}
            />
          ) : null}
          {(assistText || assistLoading || assistError) ? (
            <AiAssistPreview
              text={assistText}
              loading={assistLoading}
              error={assistError}
              onAccept={handleAssistAccept}
              onReject={handleAssistReject}
            />
          ) : null}
        </div>
      ) : (
        <div
          className="wiki-preview__content wiki-preview__content--rendered"
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{ __html: wikiRenderedContent }}
        />
      )}
      <div className="wiki-preview__debug-section">
        <button
          type="button"
          className="wiki-preview__debug-toggle"
          onClick={() => setWikiDebugInfoVisible((value) => !value)}
        >
          <span>{wikiDebugInfoVisible ? "▾" : "▸"}</span>
          {wikiDebugInfoVisible ? "隐藏调试信息" : "调试信息"}
        </button>
        {wikiDebugInfoVisible ? (
          <div className="wiki-preview__debug">
            <div className="wiki-preview__debug-head">
              <h4>诊断数据</h4>
            </div>
            {wikiImportedAtDebugRaw ? (
              <div className="wiki-preview__debug-grid">
                <div className="wiki-preview__debug-item">
                  <span>imported_at（展示）</span>
                  <code>{wikiImportedAtDebugDisplay}</code>
                </div>
                <div className="wiki-preview__debug-item">
                  <span>imported_at（原始）</span>
                  <code>{wikiImportedAtDebugRaw}</code>
                </div>
              </div>
            ) : (
              <p className="runtime-hint">当前页面未检测到 imported_at 元数据。</p>
            )}
          </div>
        ) : null}
      </div>
      <div className="wiki-preview__citations">
        <div className="section-head wiki-preview__citations-head">
          <h3>页面引用</h3>
          <span className="section-head__hint">
            {wikiPageCitations.length ? `${wikiPageCitations.length} 条` : "暂无引用"}
          </span>
        </div>
        {wikiPageCitationsError ? <p className="runtime-status">{wikiPageCitationsError}</p> : null}
        {wikiPageCitationsLoading ? <p className="runtime-hint">正在读取页面引用...</p> : null}
        {wikiPageCitations.length ? (
          <div className="wiki-citation-list">
            {wikiPageCitations.map((citation) => (
              <article key={`${citation.cited_page_path}-${citation.score}`} className="wiki-citation">
                <div className="wiki-citation__top">
                  <code>{resolveDisplayPath(citation)}</code>
                  <span className={`pill ${citation.target_exists ? "pill--ok" : "pill--danger"}`}>
                    {citation.target_exists ? "目标存在" : "目标缺失"}
                  </span>
                </div>
                <div className="wiki-citation__meta">score: {citation.score}</div>
                <p>{citation.excerpt}</p>
                <div className="wiki-citation__actions">
                  <button
                    type="button"
                    className="dev-panel__button wiki-citation__button"
                    onClick={() => void handleOpenWikiPage(citation.cited_page_path)}
                    disabled={!isTauriRuntime() || !citation.target_exists || wikiPageDetailLoading}
                  >
                    {citation.target_exists ? "查看被引页面" : "目标页面缺失"}
                  </button>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-state">当前页面没有可展示的引用。</p>
        )}
      </div>
    </article>
  );

  // ── JSX ────────────────────────────────────────────────────────────────────

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
            {sortedWikiPages.length
              ? `${sortedWikiPages.length} 页 · ${wikiSortModeLabels[wikiSortMode]}`
              : "暂无页面"}
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
              onChange={(event) => setWikiKeyword(event.target.value)}
              onKeyDown={handleWikiKeywordKeyDown}
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
              onChange={(event) => setWikiSortMode(event.target.value as WikiSortMode)}
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
              onClick={() => void handleSearchWikiPages()}
              disabled={!isTauriRuntime() || wikiSearching}
            >
              {wikiSearching ? "搜索中..." : "搜索 Wiki"}
            </button>
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void handleResetWikiPages()}
              disabled={wikiSearching}
            >
              恢复最近
            </button>
            <button
              type="button"
              className="dev-panel__button wiki-new-btn"
              onClick={handleOpenNewWikiPageModal}
              title="AI 辅助新建 Wiki 页面"
            >
              ✦ AI 新建
            </button>
            {allWikiTags.length > 0 && (
              <button
                type="button"
                className={`dev-panel__button wiki-tag-toggle-btn${wikiTagBarExpanded ? " wiki-tag-toggle-btn--open" : ""}${wikiActiveTags.size > 0 ? " wiki-tag-toggle-btn--active" : ""}`}
                onClick={() => {
                  if (wikiTagBarExpanded) setWikiTagShowAll(false);
                  setWikiTagBarExpanded((v) => !v);
                }}
                title={`按标签筛选（共 ${allWikiTags.length} 个）`}
              >
                # 标签
                {wikiActiveTags.size > 0 && (
                  <span className="wiki-tag-toggle-btn__badge">{wikiActiveTags.size}</span>
                )}
                <span className="wiki-tag-toggle-btn__arrow">{wikiTagBarExpanded ? "▴" : "▾"}</span>
              </button>
            )}
          </div>
        </div>
        {showNewPageModal && (
          <div className="new-page-modal-backdrop" onClick={handleCloseNewWikiPageModal}>
            <div className="new-page-modal" onClick={(event) => event.stopPropagation()}>
              <h3 className="new-page-modal__title">AI 辅助新建 Wiki 页面</h3>
              <p className="new-page-modal__hint">
                输入主题，AI 将参考现有知识库生成结构化初稿。
              </p>
              <div className="new-page-modal__row">
                <input
                  className="new-page-modal__input"
                  type="text"
                  placeholder="例如：量子纠缠、黑洞蒸发、Rust 生命周期…"
                  value={newPageTopic}
                  onChange={(event) => setNewPageTopic(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") void handleCreatePageWithAi();
                  }}
                  disabled={newPageCreating}
                  autoFocus
                />
                <button
                  className="new-page-modal__btn new-page-modal__btn--primary"
                  onClick={() => void handleCreatePageWithAi()}
                  disabled={newPageCreating || !newPageTopic.trim()}
                >
                  {newPageCreating ? "AI 生成中…" : "生成页面"}
                </button>
              </div>
              {newPageResult && (
                <div className="new-page-modal__result">
                  <p className="new-page-modal__result-title">✅ 已创建：{newPageResult.title}</p>
                  <pre className="new-page-modal__preview">{newPageResult.content_preview}</pre>
                </div>
              )}
              <div className="new-page-modal__actions">
                {newPageResult && (
                  <button className="new-page-modal__btn" onClick={handleUseNewWikiPageResult}>
                    查看页面
                  </button>
                )}
                <button className="new-page-modal__btn" onClick={handleCloseNewWikiPageModal}>
                  关闭
                </button>
              </div>
            </div>
          </div>
        )}
        {allWikiTags.length > 0 && wikiTagBarExpanded && (
          <div className="wiki-tag-bar">
            {wikiActiveTags.size >= 2 && (
              <div className="wiki-tag-filter-mode">
                <span className="wiki-tag-filter-mode__label">多选：</span>
                <div className="wiki-tag-filter-mode__pill">
                  <button
                    type="button"
                    className={`wiki-tag-filter-mode__btn${wikiTagFilterMode === "or" ? " wiki-tag-filter-mode__btn--active" : ""}`}
                    onClick={() => setWikiTagFilterMode("or")}
                    title="OR — 包含任意选中标签"
                  >
                    OR
                  </button>
                  <button
                    type="button"
                    className={`wiki-tag-filter-mode__btn${wikiTagFilterMode === "and" ? " wiki-tag-filter-mode__btn--active" : ""}`}
                    onClick={() => setWikiTagFilterMode("and")}
                    title="AND — 同时包含所有选中标签"
                  >
                    AND
                  </button>
                </div>
                <span className="wiki-tag-filter-mode__hint">
                  {wikiTagFilterMode === "or" ? "任意匹配" : "全部匹配"}
                </span>
              </div>
            )}
            <button
              type="button"
              className={`wiki-tag-chip ${wikiActiveTags.size === 0 ? "wiki-tag-chip--active" : ""}`}
              onClick={handleClearWikiActiveTags}
            >
              全部
            </button>
            {visibleTags.map((tag) => (
              <button
                key={tag.name}
                type="button"
                className={`wiki-tag-chip ${wikiActiveTags.has(tag.name) ? "wiki-tag-chip--active" : ""}`}
                onClick={() => handleToggleWikiActiveTag(tag.name)}
              >
                {tag.name} <span className="wiki-tag-chip__count">({tag.count})</span>
              </button>
            ))}
            {!wikiTagShowAll && allWikiTags.length > TOP_TAG_COUNT && (
              <button
                type="button"
                className="wiki-tag-chip wiki-tag-chip--more"
                onClick={() => setWikiTagShowAll(true)}
              >
                还有 {allWikiTags.length - TOP_TAG_COUNT} 个 ▸
              </button>
            )}
            {wikiTagShowAll && allWikiTags.length > TOP_TAG_COUNT && (
              <button
                type="button"
                className="wiki-tag-chip wiki-tag-chip--more"
                onClick={() => setWikiTagShowAll(false)}
              >
                收起 ▴
              </button>
            )}
          </div>
        )}
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
                    onClick={expandAllWikiFolders}
                    title="全部展开"
                  >
                    展开全部
                  </button>
                  <button
                    type="button"
                    className="wiki-tree__action-btn"
                    onClick={collapseAllWikiFolders}
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
                  const isActiveCard = isWikiPageActive(page.path);
                  const isDetailForCard = isWikiPageDetailActive(page.path);
                  const isSummaryExpanded = isWikiPageSummaryExpanded(page.path);
                  const canToggleSummary =
                    page.summary.trim().split("\n").length > wikiSummaryPreviewLines;
                  const summaryDisplay = buildWikiSummaryDisplay(
                    page.summary,
                    isSummaryExpanded,
                  ) as WikiSummaryDisplay;
                  const summarySegments = highlightWikiText(summaryDisplay.text);
                  const titleSegments = highlightWikiText(page.title);

                  return (
                    <article key={page.path} className="ask-citation">
                      <div className="ask-citation__top">
                        <code>
                          {titleSegments.map((segment, index) =>
                            segment.matched ? (
                              <mark
                                key={`${page.path}-title-${index}`}
                                className="wiki-summary__mark"
                              >
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
                              <mark
                                key={`${page.path}-summary-${index}`}
                                className="wiki-summary__mark"
                              >
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
                            onClick={() => handleToggleWikiSummary(page.path)}
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
                              handleCloseWikiPreview();
                              return;
                            }
                            void handleOpenWikiPage(page.path);
                          }}
                          disabled={!isTauriRuntime() || wikiPageDetailLoading}
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
            {isTauriRuntime()
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
            <div className="wiki-inline-preview wiki-inline-preview--floating">
              {renderWikiPreview()}
            </div>
          ) : null
        ) : null}
      </section>
    </>
  );
});

export default WikiModule;
