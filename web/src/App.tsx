import { Component, type KeyboardEvent, lazy, Suspense, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";

const ForceGraph2D = lazy(() => import("react-force-graph-2d"));
import {
  fetchAppOverview,
  fetchDefaultPaths,
  fetchLlmStatus,
  fetchLlmConfig,
  fetchOcrConfig,
  fetchRecentLintPatchEvents,
  fetchQuerySettings,
  fetchRecentLogs,
  fetchAskHistory,
  fetchRecentWikiPages,
  fetchWikiPageDetail,
  fetchWikiPageCitations,
  getLlmProviderPresets,
  initVault,
  ingestMarkdown,
  ingestFile,
  ingestPdf,
  ingestUrl,
  isTauriRuntime,
  pickFiles,
  pickFolder,
  queryAskSession,
  cancelAskSession,
  clearAskHistory,
  clearAskSession,
  runLint,
  applyLintPatch,
  applyLintPatchesBatch,
  previewLintPatches,
  saveLlmConfig,
  saveAskHistory,
  saveOcrConfig,
  deleteWikiPage,
  getKnowledgeGraph,
  getKnowledgeSubgraph,
  markPageStale,
  renameWikiPage,
  saveWikiPage,
  searchWikiPages,
  saveQueryAnswer,
  get_outbox_events,
  setBackendMode,
  setQueryTopK as persistQueryTopK,
  formatLlmStatusSummary,
  resolveDisplayPath,
  listenProgress,
  type OcrProvider,
} from "./tauri-client";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import {
  filterLintIssuesByCode,
  filterLintIssuesByPath,
  filterLintIssuesBySuggestion,
  filterLintIssuesBySeverity,
  formatLintCheckedAt,
  normalizeLintSeverity,
  readLintFilterState,
  resolveLintSeverityStats,
  writeLintFilterState,
} from "./lint-utils";
import type { LintSeverityFilter } from "./lint-utils";
import type {
  AppOverview,
  AskHistoryItem,
  BackendAppMode,
  KnowledgeGraphData,
  KnowledgeGraphLink,
  KnowledgeGraphNode,
  LlmProviderConfig,
  LlmStatus,
  LintIssue,
  LintReport,
  LintPatchBatchResult,
  LintPatchEvent,
  LintPatchPreviewItem,
  LogEntry,
  ModuleId,
  ModuleItem,
  ModeId,
  ProgressPayload,
  QueryAnswerResult,
  WikiPageDetail,
  WikiPageCitation,
  WikiPageItem,
} from "./types";

const defaultVaultPath = "vault";
const defaultIngestSourcePath = "E:\\llm-wiki\\test-llm.md";
const defaultIngestPdfPath = "E:\\llm-wiki\\test.pdf";
const defaultIngestFilePath = "E:\\llm-wiki\\test.docx";
const defaultIngestFileOcrProvider: OcrProvider = "tesseract";
const defaultQueryTopKMin = 1;
const defaultQueryTopKMax = 8;
const defaultQueryTopK = 3;

const modeIdToBackendMode: Record<ModeId, BackendAppMode> = {
  hybrid: "Hybrid",
  "strict-local": "StrictLocal",
};

const backendModeToModeId: Record<BackendAppMode, ModeId> = {
  Hybrid: "hybrid",
  StrictLocal: "strict-local",
};

const modeIdLabels: Record<ModeId, string> = {
  hybrid: "Hybrid（自由模式）",
  "strict-local": "Strict Local（仅本地）",
};

const modeIdDescriptions: Record<ModeId, string> = {
  hybrid: "允许本地与云 Provider 按任务路由，适合常规工作流。",
  "strict-local": "只允许本地 Ollama，自动拦截云调用与外部模型请求。",
};

const answerStrategyLabels: Record<string, string> = {
  llm: "LLM 合成",
  rule: "规则回退",
  llm_synthesis: "LLM 合成",
  rule_fallback: "规则回退",
};

const lintSeverityFilterLabels: Record<LintSeverityFilter, string> = {
  all: "全部",
  error: "错误",
  warning: "警告",
  info: "信息",
};

const searchStrategyLabels: Record<string, string> = {
  fts: "FTS 检索",
  scan: "回退扫描",
  empty: "空结果",
  rrf: "RRF 融合检索",
};

const wikiSortModeLabels: Record<WikiSortMode, string> = {
  updated_desc: "更新时间（新到旧）",
  updated_asc: "更新时间（旧到新）",
  title_asc: "标题（A-Z）",
};

const ocrProviderLabels: Record<OcrProvider, string> = {
  tesseract: "tesseract（本地默认）",
  paddle: "paddle（高精度）",
};

export const defaultCloudProviderName = "DeepSeek";
export const defaultCloudBaseUrl = "https://api.deepseek.com/v1";
export const defaultCloudModel = "deepseek-chat";

type CloudProviderPresetId = "deepseek" | "glm" | "minimax";

export const cloudProviderPresets: Record<
  CloudProviderPresetId,
  {
    name: string;
    providerName: string;
    baseUrl: string;
    model: string;
  }
> = {
  deepseek: {
    name: "DeepSeek",
    providerName: "DeepSeek",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
  },
  glm: {
    name: "GLM",
    providerName: "GLM",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4-flash",
  },
  minimax: {
    name: "MiniMax",
    providerName: "MiniMax",
    baseUrl: "https://api.minimax.chat/v1",
    model: "abab6.5-chat",
  },
};

export const buildLlmProviderConfig = (input: {
  activeProvider: "cloud" | "ollama";
  cloudApiKey: string;
  cloudBaseUrl: string;
  cloudModel: string;
  cloudProviderName: string;
}) => {
  const active_provider = input.activeProvider;
  const cloud_api_key = input.cloudApiKey.trim();
  const cloud_base_url = input.cloudBaseUrl.trim();
  const cloud_model = input.cloudModel.trim();
  const cloud_provider_name = input.cloudProviderName.trim();

  return {
    cloud_api_key,
    cloud_base_url,
    cloud_model,
    cloud_provider_name,
    active_provider,
  };
};

export const resolveNextActiveProvider = (
  activeProvider: "cloud" | "ollama",
  cloudApiKey: string,
) => {
  if (activeProvider === "cloud" && !cloudApiKey.trim()) {
    return {
      activeProvider: "ollama" as const,
      fallbackMessage: "检测到你选择了云端 Provider，但 API Key 为空，已自动回退为本地 Ollama。",
    };
  }

  return {
    activeProvider,
    fallbackMessage: "",
  };
};

export const buildCloudProviderPresetConfig = (
  presetId: CloudProviderPresetId,
  activeProvider: "cloud" | "ollama",
  existingApiKey = "",
) => {
  const preset = cloudProviderPresets[presetId];

  // 预设只填充云端三项，保留用户已经输入的 API Key。
  return buildLlmProviderConfig({
    activeProvider,
    cloudApiKey: existingApiKey,
    cloudBaseUrl: preset.baseUrl,
    cloudModel: preset.model,
    cloudProviderName: preset.providerName,
  });
};

export const formatQueryAnswerStrategyLabel = (answerStrategy?: string | null) => {
  const normalizedStrategy = answerStrategy?.trim().toLowerCase();

  if (!normalizedStrategy) {
    return "未知";
  }

  return answerStrategyLabels[normalizedStrategy] ?? "未知";
};

export const formatQuerySearchStrategyLabel = (searchStrategy?: string | null) => {
  const normalizedStrategy = searchStrategy?.trim().toLowerCase();

  if (!normalizedStrategy) {
    return "未知";
  }

  return searchStrategyLabels[normalizedStrategy] ?? "未知";
};

export const buildFrontmatterCopyText = (field: string, value: string) => `${field}: ${value}`;

export const parseLegacyWikiMetadataFromContent = (content: string | null | undefined) => {
  const sourcePattern = /^-\s*source:\s*(.+)$/i;
  const rawPattern = /^-\s*raw:\s*(.+)$/i;
  const stripMarkdownCode = (value: string) => value.trim().replace(/^`/, "").replace(/`$/, "");
  const result: {
    source?: string;
    raw?: string;
  } = {};

  for (const line of (content ?? "").split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }

    const sourceMatched = trimmed.match(sourcePattern);
    if (sourceMatched) {
      result.source = stripMarkdownCode(sourceMatched[1] ?? "");
      continue;
    }

    const rawMatched = trimmed.match(rawPattern);
    if (rawMatched) {
      result.raw = stripMarkdownCode(rawMatched[1] ?? "");
    }
  }

  return result;
};

export const parseLegacyImportedAtFromContent = (content: string | null | undefined) => {
  const importedAtPattern = /^-\s*imported\s+at:\s*(.+)$/i;
  const stripMarkdownCode = (value: string) => value.trim().replace(/^`/, "").replace(/`$/, "");

  for (const line of (content ?? "").split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    const importedMatched = trimmed.match(importedAtPattern);
    if (importedMatched) {
      return stripMarkdownCode(importedMatched[1] ?? "");
    }
  }

  return "";
};

export const resolveWikiImportedAtDebugValue = (detail: WikiPageDetail | null | undefined) => {
  const frontmatterValue = detail?.frontmatter?.imported_at?.trim() ?? "";
  if (frontmatterValue) {
    return frontmatterValue;
  }
  return parseLegacyImportedAtFromContent(detail?.content);
};

export const buildWikiFrontmatterDisplay = (detail: WikiPageDetail | null | undefined) => {
  const frontmatter = detail?.frontmatter ?? null;
  const legacyMetadata = parseLegacyWikiMetadataFromContent(detail?.content);
  const sourceRaw = frontmatter?.source ?? legacyMetadata.source ?? "";
  const rawRaw = frontmatter?.raw ?? legacyMetadata.raw ?? "";
  const rows = [
    {
      key: "title",
      label: "title",
      value: frontmatter?.title ?? "",
      displayValue: (frontmatter?.title ?? "").trim(),
    },
    {
      key: "source",
      label: "source",
      value: sourceRaw,
      displayValue: sourceRaw.trim(),
    },
    {
      key: "raw",
      label: "raw",
      value: rawRaw,
      displayValue: rawRaw.trim(),
    },
  ].filter((item) => item.value.trim().length > 0);
  const entities = (frontmatter?.entities ?? [])
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
  const hasMeta = rows.length > 0 || entities.length > 0;

  return {
    frontmatter,
    rows,
    entities,
    totalCount: rows.length + (entities.length ? 1 : 0),
    hasMeta,
  };
};

export const normalizeWikiPathForCompare = (path: string | null | undefined) =>
  (path ?? "")
    .trim()
    // Windows 规范路径前缀：\\?\C:\... 或 \\?\UNC\server\share\...
    .replace(/^\\\\\?\\UNC\\/i, "\\\\")
    .replace(/^\\\\\?\\/i, "")
    .replaceAll("\\", "/")
    .toLowerCase();

export const isSameWikiPagePath = (left: string | null | undefined, right: string | null | undefined) => {
  const normalizedLeft = normalizeWikiPathForCompare(left);
  const normalizedRight = normalizeWikiPathForCompare(right);
  return Boolean(normalizedLeft) && normalizedLeft === normalizedRight;
};

export const resolveGraphNodePagePath = (node: Partial<KnowledgeGraphNode> | null | undefined) => {
  if (!node || typeof node.id !== "string") {
    return "";
  }
  return node.id.trim();
};

const resolveGraphLinkNodeId = (
  endpoint: string | KnowledgeGraphNode | null | undefined,
) => {
  if (typeof endpoint === "string") {
    return endpoint.trim();
  }
  return resolveGraphNodePagePath(endpoint);
};

export type GraphNormalizedEdge = {
  sourceId: string;
  targetId: string;
};

export type GraphViewMode = "global" | "local";
export type GraphTraversalDirection = "both" | "out" | "in";
export const GRAPH_VIEW_MODE_STORAGE_KEY = "llm_wiki_graph_view_mode_v1";
export const GRAPH_LOCAL_DEPTH_STORAGE_KEY = "llm_wiki_graph_local_depth_v1";
export const GRAPH_LOCAL_DIRECTION_STORAGE_KEY = "llm_wiki_graph_local_direction_v1";
export const GRAPH_LOCAL_DEPTH_MIN = 1;
export const GRAPH_LOCAL_DEPTH_MAX = 3;
export const GRAPH_LOCAL_BACKEND_NODE_THRESHOLD = 1200;
export const GRAPH_LOCAL_BACKEND_LINK_THRESHOLD = 4000;
export const GRAPH_LOCAL_BACKEND_MAX_NODES = 1500;
export const GRAPH_LOCAL_BACKEND_MAX_LINKS = 8000;

export const isGraphViewMode = (value: string): value is GraphViewMode =>
  value === "global" || value === "local";

export const isGraphTraversalDirection = (value: string): value is GraphTraversalDirection =>
  value === "both" || value === "out" || value === "in";

export const shouldUseBackendSubgraph = (input: {
  viewMode: GraphViewMode;
  selectedNodeId: string;
  totalNodes: number;
  totalLinks: number;
}) => {
  if (input.viewMode !== "local") {
    return false;
  }
  if (!input.selectedNodeId.trim()) {
    return false;
  }
  return (
    input.totalNodes > GRAPH_LOCAL_BACKEND_NODE_THRESHOLD ||
    input.totalLinks > GRAPH_LOCAL_BACKEND_LINK_THRESHOLD
  );
};

export const readGraphViewModeFromStorage = (): GraphViewMode => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return "global";
    }
    const raw = storage.getItem(GRAPH_VIEW_MODE_STORAGE_KEY);
    if (!raw) {
      return "global";
    }
    return isGraphViewMode(raw) ? raw : "global";
  } catch {
    return "global";
  }
};

export const writeGraphViewModeToStorage = (mode: GraphViewMode) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(GRAPH_VIEW_MODE_STORAGE_KEY, mode);
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphLocalDirectionFromStorage = (): GraphTraversalDirection => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return "both";
    }
    const raw = storage.getItem(GRAPH_LOCAL_DIRECTION_STORAGE_KEY);
    if (!raw) {
      return "both";
    }
    return isGraphTraversalDirection(raw) ? raw : "both";
  } catch {
    return "both";
  }
};

export const writeGraphLocalDirectionToStorage = (direction: GraphTraversalDirection) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(GRAPH_LOCAL_DIRECTION_STORAGE_KEY, direction);
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const clampGraphLocalDepth = (depth: number) =>
  Math.max(GRAPH_LOCAL_DEPTH_MIN, Math.min(GRAPH_LOCAL_DEPTH_MAX, depth));

export const readGraphLocalDepthFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return 1;
    }
    const raw = storage.getItem(GRAPH_LOCAL_DEPTH_STORAGE_KEY);
    if (!raw) {
      return 1;
    }
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) {
      return 1;
    }
    return clampGraphLocalDepth(Math.round(parsed));
  } catch {
    return 1;
  }
};

export const writeGraphLocalDepthToStorage = (depth: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(GRAPH_LOCAL_DEPTH_STORAGE_KEY, String(clampGraphLocalDepth(depth)));
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const buildGraphVisibleData = (input: {
  nodes: KnowledgeGraphNode[];
  edges: GraphNormalizedEdge[];
  totalDegree: Map<string, number>;
  groupFilter: string;
  showOrphans: boolean;
  neighborOnly: boolean;
  selectedNodeId: string;
}): KnowledgeGraphData => {
  let visibleNodes = input.nodes.filter((node) => {
    const group = node.group?.trim() ?? "";
    if (input.groupFilter === "__all__") {
      return true;
    }
    if (input.groupFilter === "__ungrouped__") {
      return !group;
    }
    return group === input.groupFilter;
  });

  if (!input.showOrphans) {
    visibleNodes = visibleNodes.filter((node) => (input.totalDegree.get(node.id) ?? 0) > 0);
  }

  let visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
  let visibleEdges = input.edges.filter(
    (edge) => visibleNodeIds.has(edge.sourceId) && visibleNodeIds.has(edge.targetId),
  );

  if (input.neighborOnly && input.selectedNodeId && visibleNodeIds.has(input.selectedNodeId)) {
    const neighborIds = new Set<string>([input.selectedNodeId]);
    for (const edge of visibleEdges) {
      if (isSameWikiPagePath(edge.sourceId, input.selectedNodeId)) {
        neighborIds.add(edge.targetId);
      }
      if (isSameWikiPagePath(edge.targetId, input.selectedNodeId)) {
        neighborIds.add(edge.sourceId);
      }
    }
    visibleNodes = visibleNodes.filter((node) => neighborIds.has(node.id));
    visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
    visibleEdges = visibleEdges.filter(
      (edge) => visibleNodeIds.has(edge.sourceId) && visibleNodeIds.has(edge.targetId),
    );
  }

  return {
    nodes: visibleNodes.map((node) => ({ ...node })),
    links: visibleEdges.map((edge) => ({ source: edge.sourceId, target: edge.targetId })),
  };
};

export const buildGraphLocalData = (input: {
  nodes: KnowledgeGraphNode[];
  edges: GraphNormalizedEdge[];
  selectedNodeId: string;
  maxDepth: number;
  direction: GraphTraversalDirection;
}): KnowledgeGraphData => {
  const centerId = input.selectedNodeId.trim();
  if (!centerId) {
    return { nodes: [], links: [] };
  }

  const undirectedAdjacency = new Map<string, Set<string>>();
  const outboundAdjacency = new Map<string, Set<string>>();
  const inboundAdjacency = new Map<string, Set<string>>();
  for (const node of input.nodes) {
    undirectedAdjacency.set(node.id, new Set());
    outboundAdjacency.set(node.id, new Set());
    inboundAdjacency.set(node.id, new Set());
  }
  for (const edge of input.edges) {
    if (!undirectedAdjacency.has(edge.sourceId)) {
      undirectedAdjacency.set(edge.sourceId, new Set());
      outboundAdjacency.set(edge.sourceId, new Set());
      inboundAdjacency.set(edge.sourceId, new Set());
    }
    if (!undirectedAdjacency.has(edge.targetId)) {
      undirectedAdjacency.set(edge.targetId, new Set());
      outboundAdjacency.set(edge.targetId, new Set());
      inboundAdjacency.set(edge.targetId, new Set());
    }
    undirectedAdjacency.get(edge.sourceId)?.add(edge.targetId);
    undirectedAdjacency.get(edge.targetId)?.add(edge.sourceId);
    outboundAdjacency.get(edge.sourceId)?.add(edge.targetId);
    inboundAdjacency.get(edge.targetId)?.add(edge.sourceId);
  }

  if (!undirectedAdjacency.has(centerId)) {
    return { nodes: [], links: [] };
  }

  const depthLimit = clampGraphLocalDepth(input.maxDepth);
  const visited = new Set<string>([centerId]);
  const queue: Array<{ id: string; depth: number }> = [{ id: centerId, depth: 0 }];

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) {
      continue;
    }
    if (current.depth >= depthLimit) {
      continue;
    }
    const neighbors =
      input.direction === "out"
        ? outboundAdjacency.get(current.id)
        : input.direction === "in"
          ? inboundAdjacency.get(current.id)
          : undirectedAdjacency.get(current.id);
    if (!neighbors) {
      continue;
    }
    for (const neighborId of neighbors) {
      if (visited.has(neighborId)) {
        continue;
      }
      visited.add(neighborId);
      queue.push({ id: neighborId, depth: current.depth + 1 });
    }
  }

  const visibleNodes = input.nodes
    .filter((node) => visited.has(node.id))
    .map((node) => ({ ...node }));
  const visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
  const visibleEdges = input.edges
    .filter((edge) => visibleNodeIds.has(edge.sourceId) && visibleNodeIds.has(edge.targetId))
    .map((edge) => ({ source: edge.sourceId, target: edge.targetId }));

  return {
    nodes: visibleNodes,
    links: visibleEdges,
  };
};

export const shouldAutoDismissStatusMessage = (message: string) => {
  const normalized = message.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  const stickyKeywords = ["失败", "错误", "error", "failed", "warning", "告警"];
  if (stickyKeywords.some((keyword) => normalized.includes(keyword))) {
    return false;
  }

  const progressKeywords = ["中...", "加载中", "切换中", "running", "处理中"];
  if (progressKeywords.some((keyword) => normalized.includes(keyword))) {
    return false;
  }

  return true;
};

export const formatPdfIngestErrorMessage = (error: unknown) => {
  const rawMessage = error instanceof Error ? error.message : String(error ?? "");
  const compactRaw = rawMessage.replace(/\s+/g, " ").trim();
  const normalized = compactRaw.toLowerCase();

  let friendlyReason = "读取 PDF 失败，请确认文件可访问且内容有效。";
  if (normalized.includes("tounicode") || normalized.includes("cmap")) {
    friendlyReason = "PDF 字体映射解析失败，建议先用 PDF 工具另存为新文件后重试。";
  } else if (
    normalized.includes("未提取到任何文本")
    || normalized.includes("未提取到可用文本")
    || normalized.includes("empty text")
    || normalized.includes("no text")
    || normalized.includes("扫描件")
  ) {
    friendlyReason = "PDF 中没有可提取文本，可能是扫描件或图片型文档，建议先做 OCR。";
  } else if (normalized.includes("is not a pdf") || normalized.includes("不是 pdf")) {
    friendlyReason = "文件类型不是有效的 PDF，请检查路径或文件格式。";
  }

  if (!compactRaw) {
    return `PDF 摄入失败：${friendlyReason}`;
  }

  // 原始原因仅保留短片段，避免整段底层错误直接透出。
  const rawSnippetMaxLength = 60;
  const rawSnippet = compactRaw.length > rawSnippetMaxLength
    ? `${compactRaw.slice(0, rawSnippetMaxLength)}...`
    : compactRaw;
  return `PDF 摄入失败：${friendlyReason}（原因：${rawSnippet}）`;
};

// 编辑态下内容与原文不一致时，视为存在未保存改动。
export const hasUnsavedWikiEditChanges = (
  wikiEditMode: boolean,
  wikiEditContent: string,
  detailContent: string | null | undefined,
) => {
  if (!wikiEditMode) {
    return false;
  }
  return wikiEditContent !== (detailContent ?? "");
};

/** 格式化字符计数显示文本（用于测试） */
export const formatEditorCharCount = (count: number): string =>
  `${count.toLocaleString()} 字符`;

// 摘要折叠阈值：超过此行数时才显示展开按钮
const wikiSummaryPreviewLines = 3;

export const tokenizeWikiKeyword = (keyword: string) => {
  const tokens = keyword
    .split(/[\s,，。;；、|/]+/)
    .map((item) => item.trim().toLowerCase())
    .filter(Boolean);
  const unique = Array.from(new Set(tokens));

  return unique.filter((token) => {
    if (/^[a-z0-9_-]+$/i.test(token)) {
      return token.length >= 2;
    }
    return true;
  });
};

// 按行数截断摘要，比按字符截断更符合阅读习惯
export const buildWikiSummaryDisplay = (summary: string, expanded: boolean, maxLines = wikiSummaryPreviewLines) => {
  const normalized = summary.trim();
  const lines = normalized.split('\n');
  if (expanded || lines.length <= maxLines) {
    return {
      text: normalized,
      isTruncated: false,
    };
  }

  return {
    text: `${lines.slice(0, maxLines).join('\n')}...`,
    isTruncated: true,
  };
};

const escapeRegex = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export type WikiHighlightSegment = {
  text: string;
  matched: boolean;
};

export const buildWikiHighlightSegments = (text: string, keywords: string[]): WikiHighlightSegment[] => {
  if (!text) {
    return [];
  }
  if (!keywords.length) {
    return [{ text, matched: false }];
  }

  const normalizedKeywords = Array.from(
    new Set(
      keywords
        .map((item) => item.trim().toLowerCase())
        .filter(Boolean),
    ),
  ).sort((left, right) => right.length - left.length);

  if (!normalizedKeywords.length) {
    return [{ text, matched: false }];
  }

  const regex = new RegExp(`(${normalizedKeywords.map(escapeRegex).join("|")})`, "ig");
  const parts = text.split(regex).filter((part) => part.length > 0);

  return parts.map((part) => ({
    text: part,
    matched: normalizedKeywords.includes(part.toLowerCase()),
  }));
};

export type WikiSortMode = "updated_desc" | "updated_asc" | "title_asc";
export const WIKI_SORT_MODE_STORAGE_KEY = "llm_wiki_wiki_sort_mode_v1";

export const isWikiSortMode = (value: string): value is WikiSortMode =>
  value === "updated_desc" || value === "updated_asc" || value === "title_asc";

export const readWikiSortModeFromStorage = (): WikiSortMode => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return "updated_desc";
    }
    const raw = storage.getItem(WIKI_SORT_MODE_STORAGE_KEY);
    if (!raw) {
      return "updated_desc";
    }
    return isWikiSortMode(raw) ? raw : "updated_desc";
  } catch {
    return "updated_desc";
  }
};

export const writeWikiSortModeToStorage = (mode: WikiSortMode) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(WIKI_SORT_MODE_STORAGE_KEY, mode);
  } catch {
    // 本地存储不可用时静默降级，避免影响主流程。
  }
};

export const OCR_PROVIDER_STORAGE_KEY = "llm_wiki_ocr_provider_v1";

export const QUERY_HISTORY_STORAGE_KEY = "llm_wiki_query_history";
export const QUERY_HISTORY_MAX = 30;

export const normalizeQueryHistory = (
  questions: string[],
  max = QUERY_HISTORY_MAX,
): string[] => {
  const normalized: string[] = [];
  const seen = new Set<string>();

  for (const rawQuestion of questions) {
    const question = rawQuestion.trim();
    if (!question || seen.has(question)) {
      continue;
    }
    normalized.push(question);
    seen.add(question);
    if (normalized.length >= max) {
      break;
    }
  }

  return normalized;
};

export const mergeQueryHistory = (
  previous: string[],
  nextQuestion: string,
  max = QUERY_HISTORY_MAX,
): string[] => normalizeQueryHistory([nextQuestion, ...previous], max);

export const readQueryHistoryFromStorage = (): string[] => {
  return readQueryHistoryItemsFromStorage().map((item) => item.question);
};

export const readQueryHistoryItemsFromStorage = (): AskHistoryItem[] => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return [];
    }
    const raw = storage.getItem(QUERY_HISTORY_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    const items = parsed
      .map((item, index) => {
        if (typeof item === "string") {
          return {
            id: index + 1,
            question: item,
            created_at: "",
          } as AskHistoryItem;
        }
        if (!item || typeof item !== "object") {
          return null;
        }
        const record = item as Partial<AskHistoryItem>;
        if (typeof record.question !== "string") {
          return null;
        }
        return {
          id: typeof record.id === "number" ? record.id : index + 1,
          question: record.question,
          created_at: typeof record.created_at === "string" ? record.created_at : "",
        } as AskHistoryItem;
      })
      .filter((item): item is AskHistoryItem => Boolean(item));

    return normalizeQueryHistoryItems(items);
  } catch {
    return [];
  }
};

export const writeQueryHistoryToStorage = (
  history: string[],
  max = QUERY_HISTORY_MAX,
): void => {
  const items = history.map((question, index) => ({
    id: index + 1,
    question,
    created_at: "",
  }));
  writeQueryHistoryItemsToStorage(items, max);
};

export const writeQueryHistoryItemsToStorage = (
  history: AskHistoryItem[],
  max = QUERY_HISTORY_MAX,
): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(QUERY_HISTORY_STORAGE_KEY, JSON.stringify(normalizeQueryHistoryItems(history, max)));
  } catch {
    // 本地存储不可用时静默降级，避免影响查询主流程。
  }
};

export const normalizeQueryHistoryItems = (
  history: AskHistoryItem[],
  max = QUERY_HISTORY_MAX,
): AskHistoryItem[] => {
  const normalized: AskHistoryItem[] = [];
  const seen = new Set<string>();

  for (const item of history) {
    const question = item.question.trim();
    if (!question || seen.has(question)) {
      continue;
    }
    normalized.push({
      id: item.id,
      question,
      created_at: item.created_at,
    });
    seen.add(question);
    if (normalized.length >= max) {
      break;
    }
  }

  return normalized;
};

export const mergeQueryHistoryItems = (
  previous: AskHistoryItem[],
  nextQuestion: string,
  createdAt: string,
  max = QUERY_HISTORY_MAX,
): AskHistoryItem[] =>
  normalizeQueryHistoryItems(
    [
      {
        id: Date.now(),
        question: nextQuestion,
        created_at: createdAt,
      },
      ...previous,
    ],
    max,
  );

export const filterQueryHistoryItems = (
  history: AskHistoryItem[],
  keyword: string,
): AskHistoryItem[] => {
  const normalizedKeyword = keyword.trim().toLocaleLowerCase("zh-CN");
  if (!normalizedKeyword) {
    return history;
  }
  return history.filter((item) =>
    item.question.toLocaleLowerCase("zh-CN").includes(normalizedKeyword),
  );
};

export const formatAskHistoryCreatedAt = (createdAt: string): string => {
  const trimmed = createdAt.trim();
  if (!trimmed) {
    return "";
  }

  let timestampMs = 0;
  if (/^\d+$/.test(trimmed)) {
    const value = Number(trimmed);
    if (Number.isFinite(value) && value > 0) {
      timestampMs = value < 1_000_000_000_000 ? value * 1000 : value;
    }
  } else {
    const parsed = Date.parse(trimmed);
    if (!Number.isNaN(parsed)) {
      timestampMs = parsed;
    }
  }

  if (!timestampMs) {
    return "";
  }

  const date = new Date(timestampMs);
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  const hours = `${date.getHours()}`.padStart(2, "0");
  const minutes = `${date.getMinutes()}`.padStart(2, "0");
  return `${month}-${day} ${hours}:${minutes}`;
};

export const isOcrProvider = (value: string): value is OcrProvider =>
  value === "tesseract" || value === "paddle";

export const readOcrProviderFromStorage = (): OcrProvider => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return "tesseract";
    const raw = storage.getItem(OCR_PROVIDER_STORAGE_KEY);
    if (!raw) return "tesseract";
    return isOcrProvider(raw) ? raw : "tesseract";
  } catch {
    return "tesseract";
  }
};

export const writeOcrProviderToStorage = (provider: OcrProvider): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return;
    storage.setItem(OCR_PROVIDER_STORAGE_KEY, provider);
  } catch {
    // 本地存储不可用时静默降级
  }
};

const parseWikiUpdatedAt = (value: string) => {
  const normalized = value.trim();
  if (!normalized) {
    return 0;
  }
  if (/^\d+$/.test(normalized)) {
    return Number(normalized) || 0;
  }
  const parsed = Date.parse(normalized);
  if (Number.isNaN(parsed)) {
    return 0;
  }
  return parsed;
};

export const sortWikiPages = (pages: WikiPageItem[], mode: WikiSortMode) => {
  const next = pages.slice();
  next.sort((left, right) => {
    if (mode === "title_asc") {
      return left.title.localeCompare(right.title, "zh-CN", { sensitivity: "base" });
    }

    const leftUpdatedAt = parseWikiUpdatedAt(left.updated_at);
    const rightUpdatedAt = parseWikiUpdatedAt(right.updated_at);
    if (leftUpdatedAt === rightUpdatedAt) {
      return left.title.localeCompare(right.title, "zh-CN", { sensitivity: "base" });
    }
    if (mode === "updated_asc") {
      return leftUpdatedAt - rightUpdatedAt;
    }
    return rightUpdatedAt - leftUpdatedAt;
  });
  return next;
};

export type WikiTreeNode = {
  key: string;
  kind: "folder" | "file";
  name: string;
  fullPath: string;
  pagePath: string | null;
  children: WikiTreeNode[];
};

type MutableWikiTreeNode = WikiTreeNode;

const normalizeWikiTreeDisplayPath = (path: string) =>
  path
    .trim()
    .replaceAll("\\", "/")
    .split("/")
    .map((segment) => segment.trim())
    .filter(Boolean)
    .join("/");

const resolveWikiTreeDisplayPath = (page: WikiPageItem) => {
  const preferred = (page.display_path ?? page.displayPath ?? "").trim();
  if (preferred) {
    return preferred;
  }
  const resolved = resolveDisplayPath(page).trim();
  if (resolved) {
    return resolved;
  }
  return page.path.trim();
};

const sortWikiTreeNodes = (nodes: MutableWikiTreeNode[]) => {
  nodes.sort((left, right) => {
    if (left.kind !== right.kind) {
      return left.kind === "folder" ? -1 : 1;
    }
    return left.name.localeCompare(right.name, "zh-CN", { sensitivity: "base" });
  });
  for (const node of nodes) {
    if (node.children.length > 0) {
      sortWikiTreeNodes(node.children);
    }
  }
};

export const buildWikiTreeNodes = (pages: WikiPageItem[]): WikiTreeNode[] => {
  const roots: MutableWikiTreeNode[] = [];
  const folderMap = new Map<string, MutableWikiTreeNode>();
  const fileKeySet = new Set<string>();

  for (const page of pages) {
    const normalized = normalizeWikiTreeDisplayPath(resolveWikiTreeDisplayPath(page));
    if (!normalized) {
      continue;
    }

    const segments = normalized.split("/").filter(Boolean);
    if (segments.length === 0) {
      continue;
    }

    let parentChildren = roots;
    let currentPath = "";

    for (let index = 0; index < segments.length; index += 1) {
      const segment = segments[index];
      const isFile = index === segments.length - 1;
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;

      if (isFile) {
        const fileKey = `file:${normalizeWikiPathForCompare(page.path)}`;
        if (fileKeySet.has(fileKey)) {
          continue;
        }
        fileKeySet.add(fileKey);
        parentChildren.push({
          key: fileKey,
          kind: "file",
          name: segment,
          fullPath: currentPath,
          pagePath: page.path,
          children: [],
        });
        continue;
      }

      const folderKey = `folder:${currentPath.toLowerCase()}`;
      let folderNode = folderMap.get(folderKey);
      if (!folderNode) {
        folderNode = {
          key: folderKey,
          kind: "folder",
          name: segment,
          fullPath: currentPath,
          pagePath: null,
          children: [],
        };
        folderMap.set(folderKey, folderNode);
        parentChildren.push(folderNode);
      }
      parentChildren = folderNode.children;
    }
  }

  sortWikiTreeNodes(roots);
  return roots;
};

const collectWikiTreeFolderKeys = (nodes: WikiTreeNode[]) => {
  const keys = new Set<string>();
  const walk = (items: WikiTreeNode[]) => {
    for (const item of items) {
      if (item.kind === "folder") {
        keys.add(item.key);
        walk(item.children);
      }
    }
  };
  walk(nodes);
  return keys;
};

// Lint 问题按页面路径分组，用于分组折叠展示
export type LintIssueGroup = { path: string; issues: LintIssue[] };

export const groupLintIssuesByPath = (issues: LintIssue[]): LintIssueGroup[] => {
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

// 补丁建议按路径分组，用于折叠展示
export const groupPatchPreviewItemsByPath = (items: LintPatchPreviewItem[]): { path: string; items: LintPatchPreviewItem[] }[] => {
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
  return Array.from(map.entries()).map(([path, items]) => ({ path, items }));
};

export type QueryProgressUpdate =
  | { kind: "status"; text: string }
  | { kind: "chunk"; text: string };

export const parseQueryProgressPayload = (payload: ProgressPayload): QueryProgressUpdate => {
  const step = payload.step.trim().toLowerCase();
  const text = payload.message ?? "";
  if (step === "answer_chunk") {
    return { kind: "chunk", text };
  }
  return { kind: "status", text };
};

// 根据分组标签生成稳定颜色
function groupColor(group: string): string {
  const palette = [
    "#4a9eff", "#ff7043", "#66bb6a", "#ab47bc",
    "#ffa726", "#26c6da", "#ec407a", "#8d6e63",
  ];
  let hash = 0;
  for (let i = 0; i < group.length; i++) {
    hash = group.charCodeAt(i) + ((hash << 5) - hash);
  }
  return palette[Math.abs(hash) % palette.length];
}

type GraphErrorBoundaryProps = {
  children: ReactNode;
};

type GraphErrorBoundaryState = {
  hasError: boolean;
  message: string;
};

// 图谱渲染兜底：避免第三方图形库异常导致整个应用白屏。
class GraphErrorBoundary extends Component<GraphErrorBoundaryProps, GraphErrorBoundaryState> {
  override state: GraphErrorBoundaryState = {
    hasError: false,
    message: "",
  };

  static getDerivedStateFromError(error: unknown): GraphErrorBoundaryState {
    const message = error instanceof Error ? error.message : String(error);
    return {
      hasError: true,
      message: message.trim(),
    };
  }

  override componentDidCatch(error: unknown) {
    console.error("图谱渲染异常:", error);
  }

  override render() {
    if (this.state.hasError) {
      return (
        <div className="graph-module__empty">
          <p>{this.state.message ? `图谱渲染失败：${this.state.message}` : "图谱渲染失败。"}</p>
          <p>请先检查图谱依赖是否安装完整，或切换模块后重试。</p>
        </div>
      );
    }
    return this.props.children;
  }
}

const modules: ModuleItem[] = [
  { id: "inbox", name: "Inbox", description: "收集资料、待处理输入与任务入口。" },
  { id: "wiki", name: "Wiki", description: "Markdown Vault 的页面编辑与浏览。" },
  { id: "ask", name: "Ask", description: "基于索引与引用证据的问答入口。" },
  { id: "lint", name: "Lint", description: "一致性检查、孤儿页与过期结论扫描。" },
  { id: "graph", name: "图谱", description: "Wiki 页面知识图谱可视化。" },
  { id: "settings", name: "Settings", description: "模式、Provider 与本地配置。" },
];

type DevAction = "init_vault" | "ingest_markdown" | "ingest_pdf" | "ingest_file" | "ingest_url";

type AskMessage = {
  id: string;
  role: "user" | "assistant";
  content: string;
  streaming?: boolean;
  citations?: import("./types").QueryCitation[];
  meta?: {
    mode: import("./types").BackendAppMode;
    searchStrategy?: string | null;
    answerStrategy?: string | null;
    topK: number;
    matchedPages: number;
  };
};

type LoadResult = {
  overview: AppOverview | null;
  logs: LogEntry[];
  pages: WikiPageItem[];
  llmStatus: LlmStatus | null;
};

const loadAppData = async (): Promise<LoadResult> => {
  const [overviewResult, logsResult, pagesResult, llmStatusResult] = await Promise.allSettled([
    fetchAppOverview(),
    fetchRecentLogs(),
    fetchRecentWikiPages(),
    fetchLlmStatus(),
  ]);

  return {
    overview: overviewResult.status === "fulfilled" ? overviewResult.value : null,
    logs: logsResult.status === "fulfilled" ? logsResult.value : [],
    pages: pagesResult.status === "fulfilled" ? pagesResult.value : [],
    llmStatus: llmStatusResult.status === "fulfilled" ? llmStatusResult.value : null,
  };
};

export default function App() {
  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [pages, setPages] = useState<WikiPageItem[]>([]);
  const [llmStatus, setLlmStatus] = useState<LlmStatus | null>(null);
  const [llmStatusLoaded, setLlmStatusLoaded] = useState(false);
  const [lintReport, setLintReport] = useState<LintReport | null>(null);
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
  const [lintPatchBatchSummary, setLintPatchBatchSummary] = useState<LintPatchBatchResult | null>(
    null,
  );
  const [recentLintPatchEvents, setRecentLintPatchEvents] = useState<LintPatchEvent[]>([]);
  // 折叠的 lint 分组路径集合，默认全部展开
  const [lintCollapsedGroups, setLintCollapsedGroups] = useState<Set<string>>(new Set());
  // 折叠的补丁建议分组路径集合，默认全部展开
  const [patchPreviewCollapsedGroups, setPatchPreviewCollapsedGroups] = useState<Set<string>>(new Set());
  const [queryResult, setQueryResult] = useState<QueryAnswerResult | null>(null);
  const [statusMessage, setStatusMessage] = useState("");
  const [switchingMode, setSwitchingMode] = useState<ModeId | null>(null);
  const [devAction, setDevAction] = useState<DevAction | null>(null);
  const [lintRunning, setLintRunning] = useState(false);
  const [queryRunning, setQueryRunning] = useState(false);
  const [vaultPath, setVaultPath] = useState(defaultVaultPath);
  const [ingestSourcePath, setIngestSourcePath] = useState(defaultIngestSourcePath);
  const [ingestPdfPath, setIngestPdfPath] = useState(defaultIngestPdfPath);
  const [ingestFilePath, setIngestFilePath] = useState(defaultIngestFilePath);
  const [ingestFilePickedPaths, setIngestFilePickedPaths] = useState<string[]>([]);
  const [ingestFileOcrProvider, setIngestFileOcrProvider] = useState<OcrProvider>(
    () => readOcrProviderFromStorage(),
  );
  // URL 摄入输入框的状态，避免与 ingestUrl 函数名冲突，使用 ingestUrlInput。
  const [ingestUrlInput, setIngestUrlInput] = useState("");
  const [queryQuestion, setQueryQuestion] = useState("这个项目的核心目标是什么？");
  const [queryTopK, setQueryTopK] = useState(defaultQueryTopK);
  const [queryTopKMin, setQueryTopKMin] = useState(defaultQueryTopKMin);
  const [queryTopKMax, setQueryTopKMax] = useState(defaultQueryTopKMax);
  const [querySettingsSaving, setQuerySettingsSaving] = useState(false);
  const [queryResultSaving, setQueryResultSaving] = useState(false);
  const [queryHistoryItems, setQueryHistoryItems] = useState<AskHistoryItem[]>(() =>
    readQueryHistoryItemsFromStorage(),
  );
  const [askHistoryKeyword, setAskHistoryKeyword] = useState("");
  const [askMessages, setAskMessages] = useState<AskMessage[]>([]);
  // 当前会话 ID（每次"新对话"重新生成）
  const [askSessionId, setAskSessionId] = useState(() => crypto.randomUUID());
  const [showAskAdvanced, setShowAskAdvanced] = useState(false);
  const [expandedCitationIds, setExpandedCitationIds] = useState<Set<string>>(new Set());
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const [wikiKeyword, setWikiKeyword] = useState("");
  // 当前激活的标签集合，支持多选
  const [wikiActiveTags, setWikiActiveTags] = useState<Set<string>>(new Set());
  const [wikiTreeCollapsedFolders, setWikiTreeCollapsedFolders] = useState<Set<string>>(
    new Set(),
  );
  const [wikiSearching, setWikiSearching] = useState(false);
  const [wikiSortMode, setWikiSortMode] = useState<WikiSortMode>(() => readWikiSortModeFromStorage());
  const [wikiExpandedPaths, setWikiExpandedPaths] = useState<string[]>([]);
  const [wikiPageDetail, setWikiPageDetail] = useState<WikiPageDetail | null>(null);
  const [wikiPageCitations, setWikiPageCitations] = useState<WikiPageCitation[]>([]);
  const [wikiPageDetailLoading, setWikiPageDetailLoading] = useState(false);
  const [wikiPageCitationsLoading, setWikiPageCitationsLoading] = useState(false);
  const [wikiPageDetailError, setWikiPageDetailError] = useState("");
  const [wikiPageCitationsError, setWikiPageCitationsError] = useState("");
  const [wikiActivePagePath, setWikiActivePagePath] = useState("");
  const [wikiFrontmatterCollapsed, setWikiFrontmatterCollapsed] = useState(false);
  const [wikiFrontmatterCopiedKey, setWikiFrontmatterCopiedKey] = useState("");
  const [wikiDebugInfoVisible, setWikiDebugInfoVisible] = useState(false);
  const [wikiEditMode, setWikiEditMode] = useState(false);
  const [wikiEditContent, setWikiEditContent] = useState("");
  const [wikiSaveRunning, setWikiSaveRunning] = useState(false);
  const [wikiSaveError, setWikiSaveError] = useState("");
  const [wikiDeleteRunning, setWikiDeleteRunning] = useState(false);
  const [wikiRenameMode, setWikiRenameMode] = useState(false);
  const [wikiRenameInput, setWikiRenameInput] = useState("");
  const [wikiRenameRunning, setWikiRenameRunning] = useState(false);
  const [wikiRenameError, setWikiRenameError] = useState("");
  // LLM Provider 配置（Settings 面板）
  const [llmConfig, setLlmConfig] = useState<LlmProviderConfig | null>(null);
  const [llmPresets, setLlmPresets] = useState<[string, string, string][]>([]);
  const [selectedPreset, setSelectedPreset] = useState<string>("Custom");
  const [llmConfigCloudApiKey, setLlmConfigCloudApiKey] = useState("");
  const [llmConfigCloudBaseUrl, setLlmConfigCloudBaseUrl] = useState("");
  const [llmConfigCloudModel, setLlmConfigCloudModel] = useState("");
  const [llmConfigCloudProviderName, setLlmConfigCloudProviderName] = useState("");
  const [llmConfigActiveProvider, setLlmConfigActiveProvider] = useState<"cloud" | "ollama">(
    "ollama",
  );
  const [llmConfigSaving, setLlmConfigSaving] = useState(false);
  // 知识图谱模块状态
  const [graphData, setGraphData] = useState<KnowledgeGraphData | null>(null);
  const [graphLoading, setGraphLoading] = useState(false);
  const [graphError, setGraphError] = useState("");
  const [graphLocalSubgraphData, setGraphLocalSubgraphData] = useState<KnowledgeGraphData | null>(null);
  const [graphLocalSubgraphLoading, setGraphLocalSubgraphLoading] = useState(false);
  const [graphLocalSubgraphError, setGraphLocalSubgraphError] = useState("");
  const [graphLocalSubgraphTruncated, setGraphLocalSubgraphTruncated] = useState(false);
  const [graphSelectedNodeId, setGraphSelectedNodeId] = useState("");
  const [graphViewMode, setGraphViewMode] = useState<GraphViewMode>(() => readGraphViewModeFromStorage());
  const [graphLocalDepth, setGraphLocalDepth] = useState(() => readGraphLocalDepthFromStorage());
  const [graphLocalDirection, setGraphLocalDirection] = useState<GraphTraversalDirection>(
    () => readGraphLocalDirectionFromStorage(),
  );
  const [graphGroupFilter, setGraphGroupFilter] = useState("__all__");
  const [graphShowOrphans, setGraphShowOrphans] = useState(true);
  const [graphNeighborOnly, setGraphNeighborOnly] = useState(false);
  const [graphLayoutFrozen, setGraphLayoutFrozen] = useState(false);
  const [graphSearchQuery, setGraphSearchQuery] = useState("");
  const [outboxLastId, setOutboxLastId] = useState(0);
  const [ingesting, setIngesting] = useState(false);

  const graphSearchHits = useMemo(() => {
    const query = graphSearchQuery.trim().toLowerCase();
    if (!query || !graphData) {
      return new Set<string>();
    }
    const hits = new Set<string>();
    for (const node of graphData.nodes) {
      if (
        (node.label || "").toLowerCase().includes(query) ||
        (node.id || "").toLowerCase().includes(query)
      ) {
        hits.add(node.id);
      }
    }
    return hits;
  }, [graphSearchQuery, graphData]);

  const graphContainerRef = useRef<HTMLDivElement>(null);
  const graphRef = useRef<any>(null);
  const [graphDimensions, setGraphDimensions] = useState({ width: 800, height: 600 });
  // 当前激活的导航模块
  const [activeModule, setActiveModule] = useState<ModuleId>("inbox");

  const filteredQueryHistoryItems = useMemo(
    () => filterQueryHistoryItems(queryHistoryItems, askHistoryKeyword),
    [queryHistoryItems, askHistoryKeyword],
  );

  const graphNodes = graphData?.nodes ?? [];
  const graphNodeById = useMemo(() => {
    const map = new Map<string, KnowledgeGraphNode>();
    for (const node of graphNodes) {
      map.set(node.id, node);
    }
    return map;
  }, [graphNodes]);
  const graphNormalizedLinks = useMemo(() => {
    if (!graphData) {
      return [] as Array<{ sourceId: string; targetId: string }>;
    }
    const normalized: Array<{ sourceId: string; targetId: string }> = [];
    for (const link of graphData.links ?? []) {
      const edge = link as KnowledgeGraphLink;
      const sourceId = resolveGraphLinkNodeId(
        edge.source as string | KnowledgeGraphNode | null | undefined,
      );
      const targetId = resolveGraphLinkNodeId(
        edge.target as string | KnowledgeGraphNode | null | undefined,
      );
      if (!sourceId || !targetId) {
        continue;
      }
      normalized.push({ sourceId, targetId });
    }
    return normalized;
  }, [graphData]);
  const graphMetrics = useMemo(() => {
    const inDegree = new Map<string, number>();
    const outDegree = new Map<string, number>();
    const totalDegree = new Map<string, number>();
    const adjacency = new Map<string, Set<string>>();

    for (const node of graphNodes) {
      inDegree.set(node.id, 0);
      outDegree.set(node.id, 0);
      totalDegree.set(node.id, 0);
      adjacency.set(node.id, new Set());
    }

    for (const edge of graphNormalizedLinks) {
      const nextOut = (outDegree.get(edge.sourceId) ?? 0) + 1;
      const nextIn = (inDegree.get(edge.targetId) ?? 0) + 1;
      outDegree.set(edge.sourceId, nextOut);
      inDegree.set(edge.targetId, nextIn);
      totalDegree.set(edge.sourceId, (totalDegree.get(edge.sourceId) ?? 0) + 1);
      totalDegree.set(edge.targetId, (totalDegree.get(edge.targetId) ?? 0) + 1);

      if (!adjacency.has(edge.sourceId)) {
        adjacency.set(edge.sourceId, new Set());
      }
      if (!adjacency.has(edge.targetId)) {
        adjacency.set(edge.targetId, new Set());
      }
      adjacency.get(edge.sourceId)?.add(edge.targetId);
      adjacency.get(edge.targetId)?.add(edge.sourceId);
    }

    const orphanCount = graphNodes.filter((node) => (totalDegree.get(node.id) ?? 0) === 0).length;
    return { inDegree, outDegree, totalDegree, adjacency, orphanCount };
  }, [graphNodes, graphNormalizedLinks]);
  const graphGroupOptions = useMemo(() => {
    const groups = new Set<string>();
    for (const node of graphNodes) {
      const group = node.group?.trim() ?? "";
      if (group) {
        groups.add(group);
      }
    }
    return Array.from(groups).sort((left, right) => left.localeCompare(right, "zh-CN"));
  }, [graphNodes]);
  const graphShouldUseBackendSubgraph = useMemo(
    () =>
      shouldUseBackendSubgraph({
        viewMode: graphViewMode,
        selectedNodeId: graphSelectedNodeId,
        totalNodes: graphNodes.length,
        totalLinks: graphNormalizedLinks.length,
      }),
    [graphNormalizedLinks, graphNodes.length, graphSelectedNodeId, graphViewMode],
  );
  const graphVisibleData = useMemo<KnowledgeGraphData | null>(() => {
    if (!graphData) {
      return null;
    }
    const baseVisible = buildGraphVisibleData({
      nodes: graphNodes,
      edges: graphNormalizedLinks,
      totalDegree: graphMetrics.totalDegree,
      groupFilter: graphGroupFilter,
      showOrphans: graphShowOrphans,
      neighborOnly: graphNeighborOnly,
      selectedNodeId: graphSelectedNodeId,
    });
    if (graphViewMode === "local" && graphShouldUseBackendSubgraph && graphLocalSubgraphData) {
      return buildGraphVisibleData({
        nodes: graphLocalSubgraphData.nodes,
        edges: graphLocalSubgraphData.links
          .map((link) => ({
            sourceId: resolveGraphLinkNodeId(link.source as string | KnowledgeGraphNode | null | undefined),
            targetId: resolveGraphLinkNodeId(link.target as string | KnowledgeGraphNode | null | undefined),
          }))
          .filter((edge) => edge.sourceId && edge.targetId),
        totalDegree: graphMetrics.totalDegree,
        groupFilter: graphGroupFilter,
        showOrphans: graphShowOrphans,
        neighborOnly: graphNeighborOnly,
        selectedNodeId: graphSelectedNodeId,
      });
    }
    if (graphViewMode === "local") {
      return buildGraphLocalData({
        nodes: baseVisible.nodes,
        edges: baseVisible.links
          .map((link) => ({
            sourceId: resolveGraphLinkNodeId(link.source as string | KnowledgeGraphNode | null | undefined),
            targetId: resolveGraphLinkNodeId(link.target as string | KnowledgeGraphNode | null | undefined),
          }))
          .filter((edge) => edge.sourceId && edge.targetId),
        selectedNodeId: graphSelectedNodeId,
        maxDepth: graphLocalDepth,
        direction: graphLocalDirection,
      });
    }
    return baseVisible;
  }, [
    graphData,
    graphGroupFilter,
    graphLocalDepth,
    graphLocalDirection,
    graphLocalSubgraphData,
    graphMetrics.totalDegree,
    graphNeighborOnly,
    graphNodes,
    graphNormalizedLinks,
    graphSelectedNodeId,
    graphShouldUseBackendSubgraph,
    graphViewMode,
    graphShowOrphans,
  ]);
  const graphSelectedNode = useMemo(() => {
    if (!graphSelectedNodeId) {
      return null;
    }
    return graphNodeById.get(graphSelectedNodeId) ?? null;
  }, [graphNodeById, graphSelectedNodeId]);
  const graphSelectedNeighbors = useMemo(() => {
    if (!graphSelectedNode) {
      return [] as KnowledgeGraphNode[];
    }
    const adjacency = graphMetrics.adjacency.get(graphSelectedNode.id);
    if (!adjacency || adjacency.size === 0) {
      return [] as KnowledgeGraphNode[];
    }
    return Array.from(adjacency)
      .map((neighborId) => graphNodeById.get(neighborId))
      .filter((node): node is KnowledgeGraphNode => Boolean(node))
      .sort((left, right) => left.label.localeCompare(right.label, "zh-CN"));
  }, [graphMetrics.adjacency, graphNodeById, graphSelectedNode]);
  const graphVisibleOrphanCount = useMemo(() => {
    if (!graphVisibleData) {
      return 0;
    }
    return graphVisibleData.nodes.filter((node) => (graphMetrics.totalDegree.get(node.id) ?? 0) === 0).length;
  }, [graphMetrics.totalDegree, graphVisibleData]);

  // 消息更新时自动滚动到底部
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [askMessages]);

  // 切换到 graph 模块时加载图谱数据
  useEffect(() => {
    if (activeModule !== "graph") return;
    void (async () => {
      setGraphLoading(true);
      setGraphError("");
      try {
        const data = await getKnowledgeGraph();
        setGraphData(data);
      } catch (err) {
        console.error("图谱加载失败:", err);
        const message = err instanceof Error ? err.message : String(err);
        setGraphData(null);
        setGraphError(`图谱加载失败：${message}`);
      } finally {
        setGraphLoading(false);
      }
    })();
  }, [activeModule]);

  // 大图 Local 模式切换到后端子图计算，避免前端 BFS 在高规模图上卡顿。
  useEffect(() => {
    if (
      activeModule !== "graph" ||
      !graphShouldUseBackendSubgraph ||
      !graphSelectedNodeId ||
      graphViewMode !== "local"
    ) {
      setGraphLocalSubgraphData(null);
      setGraphLocalSubgraphLoading(false);
      setGraphLocalSubgraphError("");
      setGraphLocalSubgraphTruncated(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      setGraphLocalSubgraphLoading(true);
      setGraphLocalSubgraphError("");
      try {
        const subgraph = await getKnowledgeSubgraph({
          centerPagePath: graphSelectedNodeId,
          hop: graphLocalDepth,
          direction: graphLocalDirection,
          limitNodes: GRAPH_LOCAL_BACKEND_MAX_NODES,
          limitLinks: GRAPH_LOCAL_BACKEND_MAX_LINKS,
        });
        if (cancelled) {
          return;
        }
        if (!subgraph) {
          setGraphLocalSubgraphData(null);
          setGraphLocalSubgraphTruncated(false);
          setGraphLocalSubgraphError("后端未返回子图数据，已回退前端计算。");
          return;
        }
        setGraphLocalSubgraphData({
          nodes: subgraph.nodes,
          links: subgraph.links,
        });
        setGraphLocalSubgraphTruncated(Boolean(subgraph.meta?.truncated));
      } catch (err) {
        if (cancelled) {
          return;
        }
        const message = err instanceof Error ? err.message : String(err);
        setGraphLocalSubgraphData(null);
        setGraphLocalSubgraphTruncated(false);
        setGraphLocalSubgraphError(`后端子图计算失败：${message}。已回退前端计算。`);
      } finally {
        if (!cancelled) {
          setGraphLocalSubgraphLoading(false);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [
    activeModule,
    graphLocalDepth,
    graphLocalDirection,
    graphSelectedNodeId,
    graphShouldUseBackendSubgraph,
    graphViewMode,
  ]);

  // 监听图谱容器尺寸变化
  useEffect(() => {
    const updateSize = () => {
      if (graphContainerRef.current) {
        setGraphDimensions({
          width: graphContainerRef.current.clientWidth || 800,
          height: graphContainerRef.current.clientHeight || 600,
        });
      }
    };
    updateSize();
    window.addEventListener("resize", updateSize);
    return () => window.removeEventListener("resize", updateSize);
  }, [activeModule]);

  useEffect(() => {
    if (!graphSelectedNodeId || !graphVisibleData) {
      return;
    }
    const exists = graphVisibleData.nodes.some((node) =>
      isSameWikiPagePath(node.id, graphSelectedNodeId),
    );
    if (!exists) {
      setGraphSelectedNodeId("");
      setGraphNeighborOnly(false);
    }
  }, [graphSelectedNodeId, graphVisibleData]);

  useEffect(() => {
    if (graphGroupFilter === "__all__" || graphGroupFilter === "__ungrouped__") {
      return;
    }
    if (!graphGroupOptions.includes(graphGroupFilter)) {
      setGraphGroupFilter("__all__");
    }
  }, [graphGroupFilter, graphGroupOptions]);

  useEffect(() => {
    writeGraphViewModeToStorage(graphViewMode);
  }, [graphViewMode]);

  useEffect(() => {
    writeGraphLocalDepthToStorage(graphLocalDepth);
  }, [graphLocalDepth]);

  useEffect(() => {
    writeGraphLocalDirectionToStorage(graphLocalDirection);
  }, [graphLocalDirection]);

  useEffect(() => {
    if (!graphRef.current) {
      return;
    }
    if (graphLayoutFrozen) {
      graphRef.current.pauseAnimation?.();
      return;
    }
    graphRef.current.resumeAnimation?.();
    graphRef.current.d3ReheatSimulation?.();
  }, [graphLayoutFrozen, graphVisibleData?.links.length, graphVisibleData?.nodes.length]);

  // Outbox 事件轮询：实现 ingest 完成/Wiki 变更后的 UI 自动刷新与状态同步。
  useEffect(() => {
    let timerId: ReturnType<typeof globalThis.setInterval> | null = null;
    let polling = false;

    const poll = async () => {
      if (polling) return;
      polling = true;

      try {
        const events = await get_outbox_events({ last_id: outboxLastId });
        if (events && events.length > 0) {
          let shouldRefresh = false;
          let newIngesting = ingesting;
          let maxId = outboxLastId;

          for (const event of events) {
            maxId = Math.max(maxId, event.id);
            const type = event.event_type;

            // 若 ingest 完成、Wiki 页面删除、重命名或查询结果存入 Wiki，触发全局数据刷新。
            if (
              type === "ingest_completed" ||
              type === "wiki_page_deleted" ||
              type === "wiki_page_renamed" ||
              type === "query_saved_to_wiki"
            ) {
              shouldRefresh = true;
            }

            // 处理 ingest 状态标记
            if (type === "ingest_started") {
              newIngesting = true;
            } else if (type === "ingest_completed") {
              newIngesting = false;
            }
          }

          if (shouldRefresh) {
            void refreshAppData();
          }
          if (newIngesting !== ingesting) {
            setIngesting(newIngesting);
          }
          if (maxId > outboxLastId) {
            setOutboxLastId(maxId);
          }
        }
      } catch (err) {
        console.error("Outbox 轮询失败:", err);
      } finally {
        polling = false;
      }
    };

    // 每 3 秒执行一次增量检查
    timerId = globalThis.setInterval(poll, 3000);
    return () => {
      if (timerId) globalThis.clearInterval(timerId);
    };
  }, [outboxLastId, ingesting]);

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [
        data,
        defaultPaths,
        querySettings,
        lintPatchEvents,
        llmConfigResult,
        backendOcrProvider,
        dbAskHistory,
      ] =
        await Promise.all([
          loadAppData(),
          fetchDefaultPaths(),
          fetchQuerySettings(),
          fetchRecentLintPatchEvents(),
          fetchLlmConfig(),
          fetchOcrConfig(),
          fetchAskHistory(QUERY_HISTORY_MAX),
        ]);

      if (!cancelled) {
        setOverview(data.overview);
        setLogs(data.logs);
        setPages(data.pages);
        if (defaultPaths) {
          setVaultPath(defaultPaths.vault_path);
          setIngestSourcePath(defaultPaths.ingest_source_path);
        }
        setLlmStatus(data.llmStatus);
        setLlmStatusLoaded(true);
        if (querySettings) {
          setQueryTopK(querySettings.top_k);
          setQueryTopKMin(querySettings.min_top_k);
          setQueryTopKMax(querySettings.max_top_k);
        }
        if (llmConfigResult) {
          setLlmConfig(llmConfigResult);
          setLlmConfigCloudApiKey(llmConfigResult.cloud_api_key);
          setLlmConfigCloudBaseUrl(llmConfigResult.cloud_base_url);
          setLlmConfigCloudModel(llmConfigResult.cloud_model);
          setLlmConfigCloudProviderName(llmConfigResult.cloud_provider_name);
          setLlmConfigActiveProvider(
            llmConfigResult.active_provider === "cloud" ? "cloud" : "ollama",
          );
        }

        // 优先级：后端配置 > localStorage；后端有值时覆盖本地状态并同步到 localStorage。
        if (backendOcrProvider && isOcrProvider(backendOcrProvider)) {
          setIngestFileOcrProvider(backendOcrProvider);
          writeOcrProviderToStorage(backendOcrProvider);
        }

        setRecentLintPatchEvents(lintPatchEvents);
        const lintFilterState = readLintFilterState();
        setLintSeverityFilter(lintFilterState.severity);
        setLintCodeKeyword(lintFilterState.codeKeyword);
        setLintPathKeyword(lintFilterState.pathKeyword);
        setLintSuggestionKeyword(lintFilterState.suggestionKeyword);
        setLintFilterStateLoaded(true);

        // Ask 历史优先读取后端 DB；后端不可用时回退到 localStorage。
        if (dbAskHistory) {
          const normalized = normalizeQueryHistoryItems(dbAskHistory, QUERY_HISTORY_MAX);
          setQueryHistoryItems(normalized);
          writeQueryHistoryItemsToStorage(normalized, QUERY_HISTORY_MAX);
        } else {
          setQueryHistoryItems(readQueryHistoryItemsFromStorage());
        }
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

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

  useEffect(() => {
    void (async () => {
      const presets = await getLlmProviderPresets();
      setLlmPresets(presets);
    })();
  }, []);

  const handlePresetChange = (presetName: string) => {
    setSelectedPreset(presetName);
    if (presetName !== "Custom") {
      const preset = llmPresets.find((p) => p[0] === presetName);
      if (preset) {
        setLlmConfigCloudProviderName(preset[0]);
        setLlmConfigCloudBaseUrl(preset[1]);
        setLlmConfigCloudModel(preset[2]);
      }
    }
  };

  useEffect(() => {
    setWikiExpandedPaths((prev) =>
      prev.filter((path) => pages.some((page) => isSameWikiPagePath(page.path, path))),
    );
  }, [pages]);

  useEffect(() => {
    writeWikiSortModeToStorage(wikiSortMode);
  }, [wikiSortMode]);

  const refreshAppData = async () => {
    const data = await loadAppData();
    setOverview(data.overview);
    setLogs(data.logs);
    setPages(data.pages);
    setLlmStatus(data.llmStatus);
    setLlmStatusLoaded(true);
  };

  const refreshRecentLintPatchEvents = async () => {
    const events = await fetchRecentLintPatchEvents();
    setRecentLintPatchEvents(events);
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
    setStatusMessage("摄入中...");
    let unlisten: (() => void) | null = null;

    try {
      // 进度订阅失败不应阻塞主流程，避免按钮状态无法复位。
      try {
        unlisten = await listenProgress("ingest_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 ingest 进度事件失败，继续执行摄入流程。", error);
      }

      const result = await ingestMarkdown(nextSourcePath);
      if (!result) {
        setStatusMessage("当前环境不支持示例摄入。");
        return;
      }

      await refreshAppData();
      const entitiesMsg =
        result.entities && result.entities.length > 0
          ? `\n提取实体：${result.entities.join("、")}`
          : "";
      const updatedMsg =
        result.updated_pages && result.updated_pages.length > 0
          ? `\n更新相关页面：${result.updated_pages.length} 个`
          : "";
      setStatusMessage(
        `${result.message || `已处理 ${result.source_path}`}${entitiesMsg}${updatedMsg}`
      );
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`示例摄入失败：${message}`);
    } finally {
      if (unlisten) {
        unlisten();
      }
      setDevAction(null);
    }
  };

  const handleUrlIngest = async () => {
    setStatusMessage("收到 URL 摄入请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行 URL 摄入。");
      return;
    }

    const trimmedUrl = ingestUrlInput.trim();
    if (!trimmedUrl) {
      setStatusMessage("请输入有效的 URL。");
      return;
    }

    setDevAction("ingest_url");
    setStatusMessage("摄入中...");
    let unlisten: (() => void) | null = null;

    try {
      // 进度订阅失败不应阻塞主流程，避免按钮状态无法复位。
      try {
        unlisten = await listenProgress("ingest_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 ingest 进度事件失败，继续执行 URL 摄入流程。", error);
      }

      const result = await ingestUrl(trimmedUrl);
      if (!result) {
        setStatusMessage("当前环境不支持 URL 摄入。");
        return;
      }

      await refreshAppData();
      const entitiesMsg =
        result.entities && result.entities.length > 0
          ? `\n提取实体：${result.entities.join("、")}`
          : "";
      const updatedMsg =
        result.updated_pages && result.updated_pages.length > 0
          ? `\n更新相关页面：${result.updated_pages.length} 个`
          : "";
      setStatusMessage(
        `${result.message || `已处理 ${trimmedUrl}`}${entitiesMsg}${updatedMsg}`
      );
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`URL 摄入失败：${message}`);
    } finally {
      if (unlisten) {
        unlisten();
      }
      setDevAction(null);
    }
  };

  const handlePdfIngest = async () => {
    setStatusMessage("收到 PDF 摄入请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行 PDF 摄入。");
      return;
    }

    const trimmedPath = ingestPdfPath.trim();
    if (!trimmedPath) {
      setStatusMessage("请输入 PDF 文件路径。");
      return;
    }

    setDevAction("ingest_pdf");
    setStatusMessage("摄入中...");
    let unlisten: (() => void) | null = null;

    try {
      // 进度订阅失败不应阻塞主流程，避免按钮状态无法复位。
      try {
        unlisten = await listenProgress("ingest_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 ingest 进度事件失败，继续执行 PDF 摄入流程。", error);
      }

      const result = await ingestPdf(trimmedPath);
      if (!result) {
        setStatusMessage("当前环境不支持 PDF 摄入。");
        return;
      }

      await refreshAppData();
      const entitiesMsg =
        result.entities && result.entities.length > 0
          ? `\n提取实体：${result.entities.join("、")}`
          : "";
      const updatedMsg =
        result.updated_pages && result.updated_pages.length > 0
          ? `\n更新相关页面：${result.updated_pages.length} 个`
          : "";
      setStatusMessage(`${result.message || `已处理 ${trimmedPath}`}${entitiesMsg}${updatedMsg}`);
    } catch (error) {
      console.error(error);
      const pdfErrorMessage = error instanceof Error ? error.message : String(error);
      // PDF 摄入一般不依赖 OCR，但若后端返回"未检测到"类错误也给出友好提示
      if (pdfErrorMessage.includes("未检测到")) {
        setStatusMessage(`OCR 工具未找到：${pdfErrorMessage}`);
        return;
      }
      setStatusMessage(formatPdfIngestErrorMessage(error));
    } finally {
      if (unlisten) {
        unlisten();
      }
      setDevAction(null);
    }
  };

  const handleFileIngest = async () => {
    setStatusMessage("收到通用文件摄入请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行通用文件摄入。");
      return;
    }

    const pathsToIngest = ingestFilePickedPaths.length > 0
      ? ingestFilePickedPaths
      : [ingestFilePath.trim()].filter(Boolean);
    if (pathsToIngest.length === 0) {
      setStatusMessage("请选择或输入要摄入的文件路径（支持 md/pdf/docx/pptx/txt/图片）。");
      return;
    }

    setDevAction("ingest_file");
    setStatusMessage("摄入中...");
    let unlisten: (() => void) | null = null;

    try {
      // 进度订阅失败不应阻塞主流程，避免按钮状态无法复位。
      try {
        unlisten = await listenProgress("ingest_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 ingest 进度事件失败，继续执行通用文件摄入流程。", error);
      }

      let successCount = 0;
      for (const filePath of pathsToIngest) {
        setStatusMessage(`摄入中 (${successCount + 1}/${pathsToIngest.length})：${filePath.split(/[/\\]/).pop() ?? filePath}`);
        const result = await ingestFile(filePath, ingestFileOcrProvider);
        if (!result) {
          setStatusMessage("当前环境不支持通用文件摄入。");
          return;
        }

        await refreshAppData();
        const entitiesMsg =
          result.entities && result.entities.length > 0
            ? `\n提取实体：${result.entities.join("、")}`
            : "";
        const updatedMsg =
          result.updated_pages && result.updated_pages.length > 0
            ? `\n更新相关页面：${result.updated_pages.length} 个`
            : "";
        if (pathsToIngest.length === 1) {
          setStatusMessage(`${result.message || `已处理 ${filePath}`}${entitiesMsg}${updatedMsg}`);
        }
        successCount++;
      }
      if (pathsToIngest.length > 1) {
        setStatusMessage(`摄入完成：${successCount}/${pathsToIngest.length} 个文件。`);
      }
      setIngestFilePickedPaths([]);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      // 检测 OCR 工具未找到的特征字符串，给出安装引导提示
      const isOcrNotFound =
        message.includes("未检测到 tesseract") ||
        message.includes("未检测到 paddleocr") ||
        message.includes("not found") ||
        message.includes("No such file");
      if (isOcrNotFound) {
        const guide =
          ingestFileOcrProvider === "paddle"
            ? "PaddleOCR 未安装，请运行：pip install paddleocr paddlepaddle"
            : "Tesseract 未安装，请从 https://github.com/UB-Mannheim/tesseract/wiki 下载安装后加入 PATH";
        setStatusMessage(`OCR 工具未找到：${guide}`);
        return;
      }
      setStatusMessage(`通用文件摄入失败：${message}`);
    } finally {
      if (unlisten) {
        unlisten();
      }
      setDevAction(null);
    }
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
      await refreshAppData();
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
      // 进度订阅失败不应阻塞查询执行，避免按钮持续处于“执行中”。
      try {
        unlisten = await listenProgress("query_progress", (payload) => {
          const update = parseQueryProgressPayload(payload);
          if (update.kind === "chunk") {
            if (update.text === "") {
              return;
            }
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

      const result = await queryAskSession(askSessionId, nextQuestion, { top_k: nextTopK });
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
      void saveAskHistory(nextQuestion); // 异步保存到 DB，不阻断
      // Query 会在后端写入日志，这里主动刷新一次前端日志面板。
      await refreshAppData();
      setStatusMessage(`Query 已完成：TopK=${nextTopK}，命中 ${result.matched_pages.length} 页。`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setAskMessages((prev) =>
        prev.map((item) =>
          item.id === assistantMessageId
            ? {
                ...item,
                content: item.content || `生成失败：${message}`,
                streaming: false,
              }
            : item,
        ),
      );
      setStatusMessage(`Query 失败：${message}`);
    } finally {
      if (unlisten) {
        unlisten();
      }
      setQueryRunning(false);
    }
  };

  const handleClearQueryHistory = async () => {
    if (queryHistoryItems.length === 0) {
      return;
    }
    if (!globalThis.confirm("确定清空 Ask 历史吗？此操作不可撤销。")) {
      return;
    }

    let backendCleared = true;
    if (isTauriRuntime()) {
      backendCleared = await clearAskHistory();
    }

    setQueryHistoryItems([]);
    setAskHistoryKeyword("");
    writeQueryHistoryItemsToStorage([], QUERY_HISTORY_MAX);
    setStatusMessage(backendCleared ? "Ask 历史已清空。" : "本地历史已清空（后端清理失败）。");
  };

  const handleSaveLlmConfig = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法保存 LLM 配置。");
      return;
    }

    setLlmConfigSaving(true);
    setStatusMessage("");

    try {
      const providerDecision = resolveNextActiveProvider(llmConfigActiveProvider, llmConfigCloudApiKey);
      const nextConfig = buildLlmProviderConfig({
        activeProvider: providerDecision.activeProvider,
        cloudApiKey: llmConfigCloudApiKey,
        cloudBaseUrl: llmConfigCloudBaseUrl,
        cloudModel: llmConfigCloudModel,
        cloudProviderName: llmConfigCloudProviderName,
      });

      // 将选中的 Preset 同时存入后端配置（通过在 LlmProviderConfig 中扩展字段，或者假设需要同步到后端）
      // 由于 LlmProviderConfig 类型定义未显示在当前 context，直接修改 saveLlmConfig 行为，
      // 这里暂且使用现有的 buildLlmProviderConfig，并假设后端可以处理 metadata。
      const configWithPreset = { ...nextConfig, selected_preset: selectedPreset };

      const result = await saveLlmConfig(configWithPreset as LlmProviderConfig);
      if (!result) {
        setStatusMessage("当前环境不支持保存 LLM 配置。");
        return;
      }
      setLlmConfig(result);
      setLlmConfigCloudApiKey(result.cloud_api_key);
      setLlmConfigCloudBaseUrl(result.cloud_base_url);
      setLlmConfigCloudModel(result.cloud_model);
      setLlmConfigCloudProviderName(result.cloud_provider_name);
      setLlmConfigActiveProvider(result.active_provider === "cloud" ? "cloud" : "ollama");
      // 刷新 LLM 状态显示（Provider 可能已切换）
      await refreshAppData();
      const savedMessage =
        result.active_provider === "cloud"
          ? `LLM 配置已保存（Preset: ${selectedPreset}），当前使用 ${result.cloud_provider_name || "云端 Provider"}（${result.cloud_model || defaultCloudModel}）。`
          : "LLM 配置已保存，当前使用本地 Ollama。";
      setStatusMessage(
        providerDecision.fallbackMessage
          ? `${providerDecision.fallbackMessage} ${savedMessage}`
          : savedMessage,
      );
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`保存 LLM 配置失败：${message}`);
    } finally {
      setLlmConfigSaving(false);
    }
  };

  const handleApplyCloudPreset = (presetId: CloudProviderPresetId) => {
    const presetConfig = buildCloudProviderPresetConfig(
      presetId,
      llmConfigActiveProvider,
      llmConfigCloudApiKey,
    );
    setLlmConfigCloudProviderName(presetConfig.cloud_provider_name);
    setLlmConfigCloudBaseUrl(presetConfig.cloud_base_url);
    setLlmConfigCloudModel(presetConfig.cloud_model);
    setStatusMessage(`已填充 ${cloudProviderPresets[presetId].name} 预设。`);
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
      // 保存成功后跳转到 Wiki 模块并打开对应页面
      setActiveModule("wiki");
      await handleOpenWikiPage(result.wiki_path);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`保存 Query 结果失败：${message}`);
    } finally {
      setQueryResultSaving(false);
    }
  };

  const handleSearchWikiPages = async () => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法搜索 Wiki 页面。");
      return;
    }

    setWikiSearching(true);
    setStatusMessage("");
    try {
      const result = await searchWikiPages(wikiKeyword.trim());
      setPages(result);
      if (wikiKeyword.trim()) {
        setStatusMessage(`Wiki 搜索完成：关键词“${wikiKeyword.trim()}”，命中 ${result.length} 页。`);
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

  const handleWikiKeywordKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) {
      return;
    }
    if (event.key !== "Enter") {
      return;
    }
    event.preventDefault();
    if (wikiSearching) {
      return;
    }
    void handleSearchWikiPages();
  };

  const handleResetWikiPages = async () => {
    setWikiKeyword("");
    // 重置已选标签集合
    setWikiActiveTags(new Set());
    setWikiExpandedPaths([]);
    setWikiTreeCollapsedFolders(new Set());
    await refreshAppData();
    setStatusMessage("已恢复显示最近 Wiki 页面。");
  };

  /**
   * 自动展开父级目录并将当前活跃页面滚动到可视区域
   */
  const autoRevealWikiPage = (pagePath: string) => {
    // 寻找该 pagePath 在 wikiTreeNodes 中的父级节点序列
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
      // 更新 wikiTreeCollapsedFolders 状态，确保所有祖先节点都处于展开状态
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

    // 在页面激活后延迟执行（确保 DOM 已渲染），调用 .scrollIntoView() 使 .wiki-tree__file--active 节点可见
    globalThis.setTimeout(() => {
      const activeNode = document.querySelector(".wiki-tree__file--active");
      if (activeNode) {
        activeNode.scrollIntoView({ behavior: "smooth", block: "nearest" });
      }
    }, 150);
  };

  const handleOpenWikiPage = async (pagePath: string) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法查看 Wiki 页面内容。");
      return;
    }
    const currentPath = wikiPageDetail?.path ?? wikiActivePagePath;
    const isSwitchingPage = currentPath && !isSameWikiPagePath(currentPath, pagePath);
    if (isSwitchingPage) {
      const shouldSwitch = confirmDiscardWikiPreview("switch");
      if (!shouldSwitch) {
        return;
      }
    }

    setWikiActivePagePath(pagePath);
    setWikiPageDetailLoading(true);
    setWikiPageCitationsLoading(true);
    setWikiPageDetailError("");
    setWikiPageCitationsError("");
    setWikiFrontmatterCopiedKey("");
    setWikiFrontmatterCollapsed(false);
    setWikiDebugInfoVisible(false);
    // 重置已选标签集合
    setWikiActiveTags(new Set());
    setWikiEditMode(false);
    setWikiEditContent("");
    setWikiSaveRunning(false);
    setWikiSaveError("");
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

      // 成功打开后自动触发 Auto-Reveal
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

  const handleGraphNodeClick = (node: object) => {
    const graphNode = node as any;
    const pagePath = resolveGraphNodePagePath(graphNode);
    if (!pagePath) {
      setStatusMessage("图谱节点数据异常，无法选中。");
      return;
    }
    setGraphSelectedNodeId(pagePath);

    // 自动聚焦到点击的节点
    if (graphRef.current && typeof graphNode.x === "number" && typeof graphNode.y === "number") {
      graphRef.current.centerAt(graphNode.x, graphNode.y, 400);
      graphRef.current.zoom(2.0, 400);
    }
  };

  const handleOpenSelectedGraphNode = async () => {
    if (!graphSelectedNode) {
      return;
    }
    setActiveModule("wiki");
    await handleOpenWikiPage(graphSelectedNode.id);
  };

  const handleGraphZoomToFit = () => {
    graphRef.current?.zoomToFit?.(350, 40);
  };

  const handleGraphViewModeChange = (mode: GraphViewMode) => {
    setGraphViewMode(mode);
    if (mode === "global") {
      return;
    }
    if (!graphSelectedNodeId) {
      setStatusMessage("已切换为 Local 图模式，请先点击一个节点作为中心。");
    }
  };

  const handleGraphLocalDepthChange = (value: number) => {
    setGraphLocalDepth(clampGraphLocalDepth(value));
  };

  const handleGraphLocalDirectionChange = (value: string) => {
    if (!isGraphTraversalDirection(value)) {
      return;
    }
    setGraphLocalDirection(value);
  };

  const handleToggleGraphLayoutFreeze = () => {
    setGraphLayoutFrozen((prev) => !prev);
  };

  const handleCloseWikiPreview = () => {
    const shouldClose = confirmDiscardWikiPreview("close");
    if (!shouldClose) {
      return;
    }

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
    setStatusMessage("已关闭页面预览。");
  };

  const handleStartWikiEdit = () => {
    if (!wikiPageDetail) {
      return;
    }
    if (!isTauriRuntime()) {
      setWikiSaveError("浏览器预览模式下不支持编辑保存，请在桌面应用中操作。");
      setStatusMessage("浏览器预览模式下无法保存 Wiki 页面。");
      return;
    }

    setWikiSaveError("");
    setWikiEditContent(wikiPageDetail.content ?? "");
    setWikiEditMode(true);
  };

  const handleCancelWikiEdit = () => {
    setWikiEditMode(false);
    setWikiSaveError("");
    setWikiEditContent(wikiPageDetail?.content ?? "");
  };

  const handleSaveWikiPage = async () => {
    if (!wikiPageDetail) {
      return;
    }

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
      const result = await saveWikiPage(targetPath, wikiEditContent);
      if (!result) {
        setWikiSaveError("当前环境不支持保存页面。请检查 Tauri 后端是否可用。");
        return;
      }

      setWikiEditMode(false);
      await refreshAppData();
      await handleOpenWikiPage(targetPath);
      setStatusMessage(result.message || `已保存页面：${targetPath}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      const errorMessage = `保存页面失败：${message}`;
      setWikiSaveError(errorMessage);
      setStatusMessage(errorMessage);
    } finally {
      setWikiSaveRunning(false);
    }
  };

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

  const handleDeleteWikiPage = async () => {
    if (!wikiPageDetail) {
      return;
    }

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
    if (!confirmed) {
      return;
    }

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
      setPages((prev) => prev.filter((p) => !isSameWikiPagePath(p.path, targetPath)));
      setStatusMessage(result.message || `已删除页面：${targetPath}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`删除页面失败：${message}`);
    } finally {
      setWikiDeleteRunning(false);
    }
  };

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
      // 刷新页面列表并重新打开新路径
      await refreshAppData();
      await handleOpenWikiPage(result.new_path);
      setStatusMessage(result.message);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWikiRenameError(`重命名失败：${message}`);
    } finally {
      setWikiRenameRunning(false);
    }
  };

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

  const handleToggleWikiSummary = (pagePath: string) => {
    setWikiExpandedPaths((prev) => {
      const exists = prev.some((path) => isSameWikiPagePath(path, pagePath));
      if (exists) {
        return prev.filter((path) => !isSameWikiPagePath(path, pagePath));
      }
      return [...prev, pagePath];
    });
  };

  const llmStatusSummary = llmStatus ? formatLlmStatusSummary(llmStatus) : null;
  const llmAvailabilityText = !isTauriRuntime()
    ? "浏览器预览"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.availabilityText
      : "加载中...";
  const llmModelText = !isTauriRuntime()
    ? "未连接 Tauri"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.modelText
      : "加载中...";
  const llmAddressText = !isTauriRuntime()
    ? "未连接 Tauri"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.addressText
      : "加载中...";
  const llmHintText = !isTauriRuntime()
    ? "浏览器预览模式下无法读取本地 LLM 状态。"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.hintText
      : "正在读取 LLM 状态...";
  const lintSeverityStats = resolveLintSeverityStats(lintReport);
  const lintIssues = lintReport?.issues ?? [];
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
  // 过滤后的问题按路径分组
  const groupedLintIssues = groupLintIssuesByPath(filteredLintIssues);
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
  const lintFilterEmptyText = lintIssues.length === 0
    ? "本次 lint 检查未发现问题。"
    : lintEmptyFilterLabels.length === 1
      ? `当前筛选的${lintEmptyFilterLabels[0]}没有命中任何问题。`
      : lintEmptyFilterLabels.length > 1
        ? `当前筛选的${lintEmptyFilterLabels.join("、")}组合后没有命中任何问题。`
        : "当前筛选条件没有命中任何问题。";
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
  const sortedWikiPages = sortWikiPages(pages, wikiSortMode);
  const allWikiTags = useMemo(() => {
    // 统计标签出现的次数
    const tagCountMap = new Map<string, number>();
    for (const page of sortedWikiPages) {
      for (const tag of page.tags ?? []) {
        const trimmed = tag.trim();
        if (trimmed) {
          tagCountMap.set(trimmed, (tagCountMap.get(trimmed) ?? 0) + 1);
        }
      }
    }
    // 返回 Array<{ name: string, count: number }> 并按名称排序
    return Array.from(tagCountMap.entries())
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => a.name.localeCompare(b.name, "zh-CN"));
  }, [sortedWikiPages]);
  const displayedWikiPages = wikiKeyword.trim()
    ? [...sortedWikiPages]
        .filter((p) => {
          // 使用 AND 逻辑：只有当页面包含了 wikiActiveTags 中的所有标签时才显示
          if (wikiActiveTags.size === 0) return true;
          const pageTags = p.tags ?? [];
          return Array.from(wikiActiveTags).every((tag) => pageTags.includes(tag));
        })
        .sort((a, b) => {
          const kw = wikiKeyword.toLowerCase();
          const aTitleMatch = a.title.toLowerCase().includes(kw) ? 1 : 0;
          const bTitleMatch = b.title.toLowerCase().includes(kw) ? 1 : 0;
          if (bTitleMatch !== aTitleMatch) return bTitleMatch - aTitleMatch;
          return (b.score ?? 0) - (a.score ?? 0);
        })
    : sortedWikiPages.filter((p) => {
        // 使用 AND 逻辑：只有当页面包含了 wikiActiveTags 中的所有标签时才显示
        if (wikiActiveTags.size === 0) return true;
        const pageTags = p.tags ?? [];
        return Array.from(wikiActiveTags).every((tag) => pageTags.includes(tag));
      });
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
      if (prev.size === 0) {
        return prev;
      }
      const next = new Set<string>();
      for (const key of prev) {
        if (wikiTreeFolderKeys.has(key)) {
          next.add(key);
        }
      }
      if (next.size === prev.size && Array.from(next).every((key) => prev.has(key))) {
        return prev;
      }
      return next;
    });
  }, [wikiTreeFolderKeys]);

  const wikiHasUnsavedChanges = hasUnsavedWikiEditChanges(
    wikiEditMode,
    wikiEditContent,
    wikiPageDetail?.content,
  );
  const isActiveWikiDetailInList = Boolean(
    wikiActivePagePath
    && sortedWikiPages.some((page) => isSameWikiPagePath(page.path, wikiActivePagePath)),
  );
  function confirmDiscardWikiPreview(reason: "close" | "switch") {
    if (!wikiHasUnsavedChanges) {
      return true;
    }

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
    if (!confirmed) {
      setStatusMessage("已取消操作，保留未保存改动。");
    }
    return confirmed;
  }

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

  /** 全部展开文件树文件夹 */
  const expandAllWikiFolders = () => {
    setWikiTreeCollapsedFolders(new Set());
  };

  /** 全部收起文件树文件夹 */
  const collapseAllWikiFolders = () => {
    setWikiTreeCollapsedFolders(new Set(wikiTreeFolderKeys));
  };

  const renderWikiTreeNodes = (nodes: WikiTreeNode[], depth = 0) => (
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
      {/* stale 警告横幅 */}
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
            <p className="runtime-hint">Frontmatter 已折叠，点击“展开”查看详情。</p>
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
        <div className="wiki-preview__editor-wrap">
          <textarea
            className="wiki-preview__editor"
            value={wikiEditContent}
            onChange={(event) => setWikiEditContent(event.target.value)}
            onKeyDown={(event: KeyboardEvent<HTMLTextAreaElement>) => {
              // Ctrl+S（Windows/Linux）或 Cmd+S（macOS）触发保存
              if ((event.ctrlKey || event.metaKey) && event.key === "s") {
                event.preventDefault();
                if (!wikiSaveRunning) {
                  void handleSaveWikiPage();
                }
              }
            }}
            disabled={wikiSaveRunning}
            spellCheck={false}
            rows={16}
          />
        </div>
      ) : (
        <div
          className="wiki-preview__content wiki-preview__content--rendered"
          // eslint-disable-next-line react/no-danger
          dangerouslySetInnerHTML={{ __html: wikiRenderedContent }}
        />
      )}
      {/* 独立调试面板：与 frontmatter 解耦，仅开发/诊断用 */}
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

  // 侧边栏导航项定义
  const navItems: { id: ModuleId; icon: string; label: string }[] = [
    { id: "inbox",    icon: "⊞", label: "概览" },
    { id: "wiki",     icon: "📄", label: "Wiki" },
    { id: "ask",      icon: "💬", label: "Ask" },
    { id: "lint",     icon: "🔍", label: "Lint" },
    { id: "graph",    icon: "🕸", label: "图谱" },
    { id: "settings", icon: "⚙", label: "设置" },
  ];

  return (
    <div className="app-shell">
      {/* 侧边栏导航 */}
      <nav className="sidebar">
        <div className="sidebar__brand">
          {/* LLM Wiki 品牌图标：开卷书 + AI 星芒，纯白填充保证 WebView 渲染 */}
          <div className="sidebar__brand-logo">
            <svg width="20" height="20" viewBox="0 0 20 20" fill="white" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
              {/* 左页 */}
              <path d="M10 16V5C8.2 4.2 5.5 4.2 3 5V16C5.5 15.2 8.2 15.2 10 16Z" fillOpacity="0.95"/>
              {/* 右页 */}
              <path d="M10 16V5C11.8 4.2 14.5 4.2 17 5V16C14.5 15.2 11.8 15.2 10 16Z" fillOpacity="0.55"/>
              {/* 四角星芒（AI 元素），右页右上角 */}
              <path d="M14.5 6.5 L15 8 L16.5 8.5 L15 9 L14.5 10.5 L14 9 L12.5 8.5 L14 8 Z" fillOpacity="0.95"/>
            </svg>
          </div>
          <span className="sidebar__brand-name">LLM Wiki</span>
        </div>
        <ul className="sidebar__nav">
          {navItems.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                className={`sidebar__nav-item${activeModule === item.id ? " sidebar__nav-item--active" : ""}`}
                onClick={() => setActiveModule(item.id)}
              >
                <span className="sidebar__nav-icon">{item.icon}</span>
                <span className="sidebar__nav-label">{item.label}</span>
              </button>
            </li>
          ))}
        </ul>
        <div className="sidebar__footer">
          <div className="sidebar__llm-status">
            <span
              className={`sidebar__llm-dot${llmStatus?.available ? " sidebar__llm-dot--ok" : " sidebar__llm-dot--off"}`}
            />
            <span className="sidebar__llm-label">{llmModelText}</span>
          </div>
        </div>
      </nav>

      {/* 主内容区 */}
      <div className="main-content">
        {(statusMessage || ingesting) ? (
          <div className={`status-bar${ingesting ? " status-bar--ingesting" : ""}`}>
            {ingesting && <span className="status-bar__loader">⏳</span>}
            <span>{ingesting ? "正在摄入并处理文档..." : statusMessage}</span>
            {!ingesting && (
              <button
                type="button"
                className="status-bar__close"
                onClick={() => setStatusMessage("")}
              >
                ✕
              </button>
            )}
          </div>
        ) : null}

        <div className={`module-viewport${activeModule === "ask" ? " module-viewport--ask" : ""}`}>
          {/* ---- 概览模块 ---- */}
          {activeModule === "inbox" && (
            <>
              <div className="module-header">
                <h1 className="module-header__title">概览</h1>
                <p className="module-header__sub">应用状态、Vault 操作与最近日志</p>
              </div>

              {/* 统计行 */}
              {overview ? (
                <div className="stats-row">
                  <div className="stat-card">
                    <div className="stat-card__value">{pages.length}</div>
                    <div className="stat-card__label">Wiki 页面</div>
                  </div>
                  <div className="stat-card">
                    <div className="stat-card__value">{overview.recent_log_count}</div>
                    <div className="stat-card__label">最近日志</div>
                  </div>
                  <div className="stat-card">
                    <div className="stat-card__value">{overview.pending_tasks}</div>
                    <div className="stat-card__label">待处理任务</div>
                  </div>
                  {ingesting && (
                    <div className="stat-card stat-card--ingesting">
                      <div className="stat-card__value stat-card__loader">⏳</div>
                      <div className="stat-card__label">正在摄入...</div>
                    </div>
                  )}
                </div>
              ) : null}

              {/* 运行模式 */}
              <section className="panel">
                <div className="section-head">
                  <h2>运行模式</h2>
                  <span className="section-head__hint">
                    {overview ? overview.supported_modes.map(formatBackendMode).join(" / ") : "浏览器预览"}
                  </span>
                </div>
                <div className="runtime-banner">
                  <div>
                    <span className="runtime-banner__mode">
                      {overview ? formatBackendMode(overview.mode) : "Browser Preview"}
                      <span className="runtime-banner__badge">
                        {overview ? backendModeToModeId[overview.mode] : "—"}
                      </span>
                    </span>
                    <p className="runtime-banner__description">
                      {overview
                        ? modeIdDescriptions[backendModeToModeId[overview.mode]]
                        : "浏览器预览模式下不可切换运行策略。"}
                    </p>
                  </div>
                  <div className="dev-panel__actions">
                    {(["hybrid", "strict-local"] as ModeId[]).map((modeId) => (
                      <button
                        key={modeId}
                        type="button"
                        className={`mode-option${overview && backendModeToModeId[overview.mode] === modeId ? " mode-option--active" : ""}`}
                        onClick={() => void handleModeSelect(modeId)}
                        disabled={!isTauriRuntime() || !overview || switchingMode !== null}
                      >
                        <span className="mode-option__name">
                          {modeIdLabels[modeId]}
                          {switchingMode === modeId ? (
                            <span className="mode-option__badge">切换中...</span>
                          ) : overview && backendModeToModeId[overview.mode] === modeId ? (
                            <span className="mode-option__badge">当前</span>
                          ) : null}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
                {/* LLM 状态卡片 */}
                <div className="llm-status-grid">
                  <div className="llm-status-card">
                    <div className="llm-status-card__label">LLM 状态</div>
                    <div className="llm-status-card__value">{llmAvailabilityText}</div>
                  </div>
                  <div className="llm-status-card">
                    <div className="llm-status-card__label">模型</div>
                    <div className="llm-status-card__value">{llmModelText}</div>
                  </div>
                  <div className="llm-status-card">
                    <div className="llm-status-card__label">地址</div>
                    <div className="llm-status-card__value">{llmAddressText}</div>
                  </div>
                  <div className="llm-status-card">
                    <div className="llm-status-card__label">提示</div>
                    <div className="llm-status-card__value">{llmHintText}</div>
                  </div>
                </div>
              </section>

              {/* Vault 操作 */}
              <section className="panel">
                <div className="section-head">
                  <h2>Vault 操作</h2>
                  <span className="section-head__hint">
                    {isTauriRuntime() ? "Tauri 可用" : "浏览器预览"}
                  </span>
                </div>
                <div className="dev-panel">
                  {/* Vault 路径 + 示例摄入（辅助）+ 初始化 */}
                  <div className="dev-panel__vault-row">
                    <div className="dev-panel__field dev-panel__vault-path">
                      <label className="dev-panel__label" htmlFor="vault-path">
                        Vault 路径
                      </label>
                      <div className="path-input-row">
                        <input
                          id="vault-path"
                          className="dev-panel__input"
                          type="text"
                          value={vaultPath}
                          onChange={(event) => setVaultPath(event.target.value)}
                          placeholder={defaultVaultPath}
                          spellCheck={false}
                        />
                        <button
                          type="button"
                          className="dev-panel__button path-pick-btn"
                          onClick={() => void pickFolder().then((p) => { if (p) setVaultPath(p); })}
                          disabled={!isTauriRuntime()}
                          title="选择文件夹"
                        >
                          📁
                        </button>
                      </div>
                    </div>
                    <button
                      type="button"
                      className="dev-panel__button dev-panel__vault-action"
                      onClick={() => void handleDemoIngest()}
                      disabled={!isTauriRuntime() || devAction !== null}
                      title="用内置示例文件测试摄入流程"
                    >
                      {devAction === "ingest_markdown" ? "摄入中..." : "示例摄入"}
                    </button>
                    <button
                      type="button"
                      className="dev-panel__button dev-panel__vault-action"
                      onClick={() => void handleInitVault()}
                      disabled={!isTauriRuntime() || devAction !== null}
                    >
                      {devAction === "init_vault" ? "初始化中..." : "初始化 Vault"}
                    </button>
                  </div>

                  {/* 两列主摄入卡片 */}
                  <div className="ingest-grid">

                    {/* URL 摄入 */}
                    <div className="ingest-card">
                      <span className="ingest-card__title">URL 摄入</span>
                      <div className="dev-panel__field">
                        <label className="dev-panel__label" htmlFor="ingest-url-input">
                          网页地址
                        </label>
                        <input
                          id="ingest-url-input"
                          className="dev-panel__input"
                          type="url"
                          value={ingestUrlInput}
                          onChange={(event) => setIngestUrlInput(event.target.value)}
                          placeholder="https://example.com/article"
                          spellCheck={false}
                        />
                      </div>
                      <div className="ingest-card__footer">
                        <button
                          type="button"
                          className="dev-panel__button dev-panel__button--accent"
                          onClick={() => void handleUrlIngest()}
                          disabled={!isTauriRuntime() || devAction !== null}
                        >
                          {devAction === "ingest_url" ? "摄入中..." : "URL 摄入"}
                        </button>
                      </div>
                    </div>

                    {/* 文件摄入（自动识别格式，含 PDF） */}
                    <div className="ingest-card">
                      <span className="ingest-card__title">文件摄入</span>
                      <div className="dev-panel__field">
                        <label className="dev-panel__label" htmlFor="ingest-file-path">
                          文件路径
                        </label>
                        <div className="path-input-row">
                          <input
                            id="ingest-file-path"
                            className="dev-panel__input"
                            type="text"
                            value={ingestFilePickedPaths.length > 0 ? "" : ingestFilePath}
                            onChange={(event) => { setIngestFilePath(event.target.value); setIngestFilePickedPaths([]); }}
                            placeholder={ingestFilePickedPaths.length > 0 ? `已选 ${ingestFilePickedPaths.length} 个文件` : defaultIngestFilePath}
                            disabled={ingestFilePickedPaths.length > 0}
                            spellCheck={false}
                          />
                          <button
                            type="button"
                            className="dev-panel__button path-pick-btn"
                            onClick={() =>
                              void pickFiles({
                                multiple: true,
                                filters: [{ name: "支持的文件", extensions: ["md","txt","pdf","docx","pptx","png","jpg","jpeg","bmp","webp","tif","tiff"] }],
                              }).then((paths) => {
                                if (paths && paths.length > 0) {
                                  setIngestFilePickedPaths(paths);
                                  setIngestFilePath("");
                                }
                              })
                            }
                            disabled={!isTauriRuntime()}
                            title="选择文件（支持多选）"
                          >
                            📄
                          </button>
                        </div>
                        {ingestFilePickedPaths.length > 0 ? (
                          <div className="picked-files">
                            <div className="picked-files__head">
                              <span>{ingestFilePickedPaths.length} 个文件</span>
                              <button
                                type="button"
                                className="dev-panel__button picked-files__clear"
                                onClick={() => setIngestFilePickedPaths([])}
                              >
                                清除
                              </button>
                            </div>
                            <ul className="picked-files__list">
                              {ingestFilePickedPaths.map((p) => (
                                <li key={p} className="picked-files__item" title={p}>
                                  {p.split(/[/\\]/).pop()}
                                </li>
                              ))}
                            </ul>
                          </div>
                        ) : (
                          <p className="dev-panel__hint">
                            md · txt · pdf · docx · pptx · png · jpg · bmp · webp · tif
                          </p>
                        )}
                      </div>
                      <div className="dev-panel__field">
                        <label className="dev-panel__label" htmlFor="ingest-file-ocr-provider">
                          OCR
                        </label>
                        <select
                          id="ingest-file-ocr-provider"
                          className="dev-panel__input"
                          value={ingestFileOcrProvider}
                          onChange={(event) => {
                            const provider: OcrProvider =
                              event.target.value === "paddle" ? "paddle" : "tesseract";
                            setIngestFileOcrProvider(provider);
                            writeOcrProviderToStorage(provider);
                            void saveOcrConfig(provider);
                          }}
                        >
                          <option value="tesseract">{ocrProviderLabels.tesseract}</option>
                          <option value="paddle">{ocrProviderLabels.paddle}</option>
                        </select>
                      </div>
                      <div className="ingest-card__footer">
                        <button
                          type="button"
                          className="dev-panel__button dev-panel__button--accent"
                          onClick={() => void handleFileIngest()}
                          disabled={!isTauriRuntime() || devAction !== null}
                        >
                          {devAction === "ingest_file" ? "摄入中..." : "文件摄入"}
                        </button>
                      </div>
                    </div>

                  </div>

                  <p className="dev-panel__hint">
                    {isTauriRuntime()
                      ? "文件摄入自动按扩展名路由，图片/PDF 默认 tesseract OCR，失败自动回退。成功后刷新概览与日志。"
                      : "浏览器预览模式下按钮保持禁用，仅用于界面预览。"}
                  </p>
                </div>
              </section>

              {/* 最近日志 */}
              <section className="panel">
                <div className="section-head">
                  <h2>最近日志</h2>
                  <span className="section-head__hint">
                    {logs.length ? `最近 ${logs.length} 条` : "暂无日志"}
                  </span>
                </div>
                {logs.length ? (
                  <div className="log-list">
                    {logs.map((log) => (
                      <article
                        key={log.id}
                        className={`log-item log-item--${log.level.toLowerCase()}`}
                      >
                        <div className="log-item__head">
                          <span className="log-item__level">{formatLogLevel(log.level)}</span>
                          <time dateTime={log.created_at}>{formatLintCheckedAt(log.created_at)}</time>
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
            </>
          )}
          {/* ---- Wiki 模块 ---- */}
          {activeModule === "wiki" && (
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
                  </div>
                </div>
                {allWikiTags.length > 0 ? (
                  <div className="wiki-tag-bar">
                    <button
                      type="button"
                      className={`wiki-tag-chip ${wikiActiveTags.size === 0 ? "wiki-tag-chip--active" : ""}`}
                      onClick={() => {
                        // 清空所有已选标签（显示全部）
                        setWikiActiveTags(new Set());
                      }}
                    >
                      全部
                    </button>
                    {allWikiTags.map((tag) => (
                      <button
                        key={tag.name}
                        type="button"
                        className={`wiki-tag-chip ${wikiActiveTags.has(tag.name) ? "wiki-tag-chip--active" : ""}`}
                        onClick={() =>
                          setWikiActiveTags((prev) => {
                            // 切换当前标签的激活状态
                            const next = new Set(prev);
                            if (next.has(tag.name)) {
                              next.delete(tag.name);
                            } else {
                              next.add(tag.name);
                            }
                            return next;
                          })
                        }
                      >
                        {/* 标签名及其页面计数，计数使用淡色 span 包裹 */}
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
                      <div className="wiki-tree__body">
                        {renderWikiTreeNodes(wikiTreeNodes)}
                      </div>
                    </aside>
                    <div className="wiki-layout__list">
                      <div className="ask-result__citations">
                        {displayedWikiPages.map((page) => {
                          const isActiveCard = isSameWikiPagePath(page.path, wikiActivePagePath);
                          const isDetailForCard = Boolean(
                            wikiPageDetail && isSameWikiPagePath(page.path, wikiPageDetail.path),
                          );
                          const isSummaryExpanded = wikiExpandedPaths.some((path) =>
                            isSameWikiPagePath(path, page.path),
                          );
                          const canToggleSummary = page.summary.trim().split('\n').length > wikiSummaryPreviewLines;
                          const summaryDisplay = buildWikiSummaryDisplay(page.summary, isSummaryExpanded);
                          const summarySegments = buildWikiHighlightSegments(
                            summaryDisplay.text,
                            wikiHighlightKeywords,
                          );
                          const titleSegments = buildWikiHighlightSegments(
                            page.title,
                            wikiHighlightKeywords,
                          );

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
                    <div className="wiki-inline-preview wiki-inline-preview--floating">{renderWikiPreview()}</div>
                  ) : null
                ) : null}
              </section>
            </>
          )}

          {/* ---- Ask 模块 ---- */}
          {activeModule === "ask" && (
            <div className="ask-layout">
              {/* ── 顶部工具栏 ── */}
              <div className="ask-topbar">
                <div className="ask-topbar__title-group">
                  <h1 className="ask-topbar__title">Ask</h1>
                  <span className="ask-topbar__sub">基于 Wiki 索引的多轮问答</span>
                </div>
                <div className="ask-topbar__actions">
                  {askMessages.length > 0 && (
                    <button
                      type="button"
                      className="ask-new-session-btn"
                      onClick={() => {
                        void clearAskSession(askSessionId);
                        setAskSessionId(crypto.randomUUID());
                        setAskMessages([]);
                        setQueryResult(null);
                        setExpandedCitationIds(new Set());
                        setStatusMessage("新对话已开始。");
                      }}
                    >
                      ↺ 新对话
                    </button>
                  )}
                </div>
              </div>

              {/* ── 消息区 ── */}
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
                      message.role === "assistant" &&
                      !message.streaming &&
                      message.id === [...askMessages].reverse().find((m) => m.role === "assistant")?.id;
                    const hasCitations = message.role === "assistant" && (message.citations?.length ?? 0) > 0;
                    const citationsExpanded = expandedCitationIds.has(message.id);

                    return (
                      <article
                        key={message.id}
                        className={`ask-message ask-message--${message.role}`}
                      >
                        <div className="ask-message__avatar">
                          {message.role === "user" ? "你" : "AI"}
                        </div>
                        <div className="ask-message__body">
                          <div className="ask-message__content">
                            {message.content || (message.streaming ? "" : "")}
                            {message.streaming ? <span className="ask-chat__cursor" /> : null}
                          </div>

                          {/* Citations 折叠展开 */}
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

                          {/* 元信息 pills：仅 assistant 完成后显示 */}
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

                          {/* 保存到 Wiki（仅最后一条 assistant 消息） */}
                          {isLastAssistant && queryResult && (
                            <div className="ask-message__actions">
                              <button
                                type="button"
                                className="ask-message__save-btn"
                                onClick={() => void handleSaveQueryResult()}
                                disabled={!isTauriRuntime() || queryResultSaving}
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
                {/* 自动滚动锚点 */}
                <div ref={messagesEndRef} />
              </div>

              {/* ── 底部输入区 ── */}
              <div className="ask-input-area">
                {/* 高级设置展开区 */}
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
                        onChange={(e) => setQueryTopK(Number(e.target.value))}
                      />
                    </label>
                    <button
                      type="button"
                      className="ask-advanced__save-btn"
                      onClick={() => void handleSaveQuerySettings()}
                      disabled={!isTauriRuntime() || querySettingsSaving}
                    >
                      {querySettingsSaving ? "保存中..." : "保存参数"}
                    </button>
                  </div>
                )}

                {/* 输入框 */}
                <textarea
                  className="ask-input__textarea"
                  value={queryQuestion}
                  onChange={(e) => setQueryQuestion(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && !e.shiftKey) {
                      e.preventDefault();
                      if (!queryRunning && isTauriRuntime()) void handleQueryAsk();
                    }
                  }}
                  placeholder="输入问题后按 Enter 发送，Shift+Enter 换行"
                  rows={2}
                  disabled={queryRunning}
                />

                {/* 操作行 */}
                <div className="ask-input__footer">
                  <button
                    type="button"
                    className={`ask-advanced-toggle${showAskAdvanced ? " ask-advanced-toggle--active" : ""}`}
                    onClick={() => setShowAskAdvanced((v) => !v)}
                    title="高级设置（TopK）"
                  >
                    ⚙ 高级
                  </button>
                  <div className="ask-input__footer-right">
                    {queryRunning ? (
                      <button
                        type="button"
                        className="ask-stop-btn"
                        onClick={async () => {
                          await cancelAskSession(askSessionId);
                          setQueryRunning(false);
                          setStatusMessage("查询已取消。");
                        }}
                      >
                        ⏹ 停止
                      </button>
                    ) : (
                      <button
                        type="button"
                        className="ask-send-btn"
                        onClick={() => void handleQueryAsk()}
                        disabled={!isTauriRuntime() || queryRunning}
                      >
                        发送 ↵
                      </button>
                    )}
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* ---- Lint 模块 ---- */}
          {activeModule === "lint" && (
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
                      ? `${formatLintCheckedAt(lintReport.checked_at)} · ${lintReport.issues.length} 个问题`
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
                    disabled={!isTauriRuntime() || lintPatchPreviewLoading || !lintReport}
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
                        severity === "all" ? lintIssues.length
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
                  filteredLintIssues.length ? (
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
                                            onClick={async () => {
                                              try {
                                                const targetTitle = issue.target_page || "新页面";
                                                setStatusMessage(`正在创建页面：${targetTitle}...`);
                                                // 使用 target_page 作为新页面的标题
                                                await saveWikiPage(
                                                  targetTitle,
                                                  `# ${targetTitle}\n`
                                                );
                                                setStatusMessage(`页面 ${targetTitle} 创建成功！`);
                                                await refreshAppData();
                                              } catch (err) {
                                                console.error("创建页面失败:", err);
                                                setStatusMessage("页面创建失败，请重试。");
                                              }
                                            }}
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
                    {isTauriRuntime()
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
                    {isTauriRuntime()
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
                    disabled={!isTauriRuntime() || lintPatchBatchApplying || lintPatchPreviewItems.length === 0}
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
                    {groupPatchPreviewItemsByPath(lintPatchPreviewItems).map((group) => {
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
                                      disabled={!isTauriRuntime() || lintPatchApplyingKey !== null}
                                    >
                                      {lintPatchApplyingKey === `${item.issue_code}-${item.path ?? "global"}`
                                        ? "应用中..."
                                        : "应用建议"}
                                    </button>
                                    {/* 当补丁建议有关联路径时，显示打开页面按钮 */}
                                    {item.path != null && (
                                      <button
                                        type="button"
                                        className="dev-panel__button"
                                        onClick={() => {
                                          setActiveModule("wiki");
                                          void handleOpenWikiPage(item.path!);
                                        }}
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
                    {lintReport ? '点击\u201c生成补丁建议\u201d后在此查看候选补丁预览。' : "请先运行 Lint，再生成补丁建议。"}
                  </p>
                )}
              </section>
            </>
          )}

          {/* ---- 图谱模块 ---- */}
          {activeModule === "graph" && (
            <>
              <div className="module-header">
                <h1 className="module-header__title">知识图谱</h1>
                <p className="module-header__sub">Wiki 页面与引用关系可视化</p>
              </div>
              <div className="graph-layout">
                <section className="graph-workspace">
                  <div className="graph-toolbar">
                    <div className="graph-toolbar__controls">
                      <div className="graph-mode-switch" role="group" aria-label="图谱模式切换">
                        <button
                          type="button"
                          className={`dev-panel__button ${graphViewMode === "global" ? "dev-panel__button--accent" : ""}`}
                          onClick={() => handleGraphViewModeChange("global")}
                        >
                          Global
                        </button>
                        <button
                          type="button"
                          className={`dev-panel__button ${graphViewMode === "local" ? "dev-panel__button--accent" : ""}`}
                          onClick={() => handleGraphViewModeChange("local")}
                        >
                          Local
                        </button>
                        </div>

                        <div className="graph-control graph-control--search">
                          <input
                            type="text"
                            className="dev-panel__input"
                            placeholder="搜索节点..."
                            value={graphSearchQuery}
                            onChange={(e) => setGraphSearchQuery(e.target.value)}
                          />
                          {graphSearchQuery && (
                            <button
                              type="button"
                              className="dev-panel__button dev-panel__button--small"
                              onClick={() => setGraphSearchQuery("")}
                              title="清除搜索"
                            >
                              ✕
                            </button>
                          )}
                        </div>

                        <label className="graph-control" htmlFor="graph-group-filter">                        <span className="graph-control__label">分组</span>
                        <select
                          id="graph-group-filter"
                          className="dev-panel__input"
                          value={graphGroupFilter}
                          onChange={(event) => setGraphGroupFilter(event.target.value)}
                        >
                          <option value="__all__">全部</option>
                          <option value="__ungrouped__">未分组</option>
                          {graphGroupOptions.map((group) => (
                            <option key={group} value={group}>
                              {group}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label className="graph-control graph-control--depth" htmlFor="graph-local-depth">
                        <span className="graph-control__label">Hop 深度</span>
                        <input
                          id="graph-local-depth"
                          type="range"
                          min={GRAPH_LOCAL_DEPTH_MIN}
                          max={GRAPH_LOCAL_DEPTH_MAX}
                          step={1}
                          value={graphLocalDepth}
                          disabled={graphViewMode !== "local" || !graphSelectedNode}
                          onChange={(event) => handleGraphLocalDepthChange(Number(event.target.value))}
                        />
                        <span className="graph-control__value">{graphLocalDepth}</span>
                      </label>
                      <label className="graph-control" htmlFor="graph-local-direction">
                        <span className="graph-control__label">方向</span>
                        <select
                          id="graph-local-direction"
                          className="dev-panel__input"
                          value={graphLocalDirection}
                          disabled={graphViewMode !== "local" || !graphSelectedNode}
                          onChange={(event) => handleGraphLocalDirectionChange(event.target.value)}
                        >
                          <option value="both">双向</option>
                          <option value="out">向外</option>
                          <option value="in">向内</option>
                        </select>
                      </label>
                      <button
                        type="button"
                        className={`dev-panel__button ${graphShowOrphans ? "dev-panel__button--accent" : ""}`}
                        onClick={() => setGraphShowOrphans((prev) => !prev)}
                      >
                        {graphShowOrphans ? "显示孤儿页" : "隐藏孤儿页"}
                      </button>
                      <button
                        type="button"
                        className={`dev-panel__button ${graphNeighborOnly ? "dev-panel__button--accent" : ""}`}
                        onClick={() => setGraphNeighborOnly((prev) => !prev)}
                        disabled={!graphSelectedNode}
                        title={graphSelectedNode ? "仅显示当前节点与一跳邻居" : "先选中节点后可启用"}
                      >
                        {graphNeighborOnly ? "仅看邻居：开" : "仅看邻居：关"}
                      </button>
                      <button
                        type="button"
                        className="dev-panel__button"
                        onClick={handleGraphZoomToFit}
                        disabled={!graphVisibleData || graphVisibleData.nodes.length === 0}
                      >
                        适配视图
                      </button>
                      <button
                        type="button"
                        className={`dev-panel__button ${graphLayoutFrozen ? "dev-panel__button--accent" : ""}`}
                        onClick={handleToggleGraphLayoutFreeze}
                        disabled={!graphVisibleData || graphVisibleData.nodes.length === 0}
                      >
                        {graphLayoutFrozen ? "恢复布局" : "冻结布局"}
                      </button>
                      <button
                        type="button"
                        className="dev-panel__button"
                        onClick={() => {
                          setGraphViewMode("global");
                          setGraphLocalDepth(1);
                          setGraphLocalDirection("both");
                          setGraphLocalSubgraphData(null);
                          setGraphLocalSubgraphLoading(false);
                          setGraphLocalSubgraphError("");
                          setGraphLocalSubgraphTruncated(false);
                          setGraphGroupFilter("__all__");
                          setGraphShowOrphans(true);
                          setGraphNeighborOnly(false);
                          setGraphLayoutFrozen(false);
                        }}
                      >
                        重置筛选
                      </button>
                    </div>
                    <div className="graph-toolbar__stats">
                      <span className="pill">{`模式 ${graphViewMode === "local" ? "Local" : "Global"}`}</span>
                      {graphViewMode === "local" ? (
                        <span className="pill">{`方向 ${graphLocalDirection}`}</span>
                      ) : null}
                      {graphViewMode === "local" ? (
                        <span className="pill">{`局部计算 ${graphShouldUseBackendSubgraph ? "后端" : "前端"}`}</span>
                      ) : null}
                      {graphViewMode === "local" && graphLocalSubgraphTruncated ? (
                        <span className="pill pill--warn">子图已裁剪</span>
                      ) : null}
                      <span className="pill">{`节点 ${graphVisibleData?.nodes.length ?? 0}/${graphNodes.length}`}</span>
                      <span className="pill">{`边 ${graphVisibleData?.links.length ?? 0}/${graphNormalizedLinks.length}`}</span>
                      <span className="pill">{`孤儿页 ${graphVisibleOrphanCount}/${graphMetrics.orphanCount}`}</span>
                    </div>
                  </div>

                  {graphViewMode === "local" && graphShouldUseBackendSubgraph ? (
                    <p className="runtime-hint">当前图规模较大，Local 模式已自动启用后端子图计算。</p>
                  ) : null}
                  {graphViewMode === "local" && graphLocalSubgraphLoading ? (
                    <p className="runtime-hint">正在请求后端子图...</p>
                  ) : null}
                  {graphViewMode === "local" && graphLocalSubgraphError ? (
                    <p className="runtime-status">{graphLocalSubgraphError}</p>
                  ) : null}

                  <div className="graph-module" ref={graphContainerRef}>
                    {graphLoading && (
                      <div className="graph-module__loading">加载图谱中...</div>
                    )}
                    {!graphLoading && graphError && (
                      <div className="graph-module__empty">
                        <p>{graphError}</p>
                        <p>请检查后端日志或稍后重试。</p>
                      </div>
                    )}
                    {!graphLoading && graphData && graphData.nodes.length === 0 && (
                      <div className="graph-module__empty">
                        <p>暂无 Wiki 页面数据。</p>
                        <p>请先在 Inbox 中摄入文档，生成 Wiki 页面后图谱将自动显示。</p>
                      </div>
                    )}
                    {!graphLoading && !graphError && !graphData && (
                      <div className="graph-module__empty">
                        <p>当前未获取到图谱数据。</p>
                        <p>请稍后重试，或先确认后端服务运行正常。</p>
                      </div>
                    )}
                    {!graphLoading && graphData && graphData.nodes.length > 0 && graphVisibleData && graphVisibleData.nodes.length === 0 && (
                      <div className="graph-module__empty">
                        {graphViewMode === "local" && !graphSelectedNode ? (
                          <>
                            <p>Local 模式需要先选择中心节点。</p>
                            <p>请先在图中点击一个节点，或切回 Global 模式。</p>
                          </>
                        ) : (
                          <>
                            <p>当前筛选条件下无节点。</p>
                            <p>请放宽分组、孤儿页、邻居或 Hop 深度筛选条件。</p>
                          </>
                        )}
                      </div>
                    )}
                    {!graphLoading && graphVisibleData && graphVisibleData.nodes.length > 0 && (
                      <GraphErrorBoundary key={`${graphVisibleData.nodes.length}-${graphVisibleData.links.length}`}>
                        <Suspense fallback={<div className="graph-module__loading">图谱渲染中...</div>}>
                          {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
                          <ForceGraph2D
                            ref={graphRef}
                            graphData={graphVisibleData as any}
                            width={graphDimensions.width}
                            height={graphDimensions.height}
                            nodeLabel="label"
                            nodeRelSize={6}
                            nodeVal={(node: object) => {
                              const n = node as KnowledgeGraphNode;
                              const degree = graphMetrics.totalDegree.get(n.id) ?? 0;
                              return 2 + Math.min(8, degree);
                            }}
                            nodeColor={(node: object) => {
                              const n = node as KnowledgeGraphNode;
                              return n.group ? groupColor(n.group) : "#4a9eff";
                            }}
                            linkColor={() => "rgba(120,120,180,0.4)"}
                            linkWidth={1}
                            onNodeClick={(node: object) => {
                              handleGraphNodeClick(node);
                            }}
                            nodeCanvasObject={(node: object, ctx: CanvasRenderingContext2D, globalScale: number) => {
                              const n = node as KnowledgeGraphNode;
                              const label = n.label || n.id;
                              const selected = Boolean(
                                graphSelectedNodeId && isSameWikiPagePath(graphSelectedNodeId, n.id),
                              );
                              const isSearchHit = graphSearchHits.has(n.id);
                              const radius = selected ? 8 : 5;

                              // 绘制搜索命中光晕
                              if (isSearchHit) {
                                ctx.beginPath();
                                ctx.arc(n.x ?? 0, n.y ?? 0, radius + 8, 0, 2 * Math.PI, false);
                                ctx.fillStyle = "rgba(255, 235, 59, 0.4)";
                                ctx.fill();
                              }

                              const fontSize = Math.max(10 / globalScale, 3);                              ctx.font = `${fontSize}px Sans-Serif`;
                              ctx.fillStyle = n.group ? groupColor(n.group) : "#4a9eff";
                              ctx.beginPath();
                              ctx.arc(n.x ?? 0, n.y ?? 0, radius, 0, 2 * Math.PI, false);
                              ctx.fill();
                              if (selected) {
                                ctx.strokeStyle = "rgba(255, 255, 255, 0.9)";
                                ctx.lineWidth = Math.max(2 / globalScale, 1);
                                ctx.beginPath();
                                ctx.arc(n.x ?? 0, n.y ?? 0, radius + 2, 0, 2 * Math.PI, false);
                                ctx.stroke();
                              }
                              if (globalScale > 1.4 || selected) {
                                ctx.fillStyle = "rgba(255,255,255,0.9)";
                                ctx.fillText(label, (n.x ?? 0) + 10, (n.y ?? 0) + 4);
                              }
                            }}
                            cooldownTicks={100}
                            d3AlphaDecay={0.02}
                            d3VelocityDecay={0.3}
                          />
                        </Suspense>
                      </GraphErrorBoundary>
                    )}
                  </div>
                </section>

                <aside className="graph-sidepanel">
                  {graphSearchQuery.trim() !== "" && (
                    <div className="graph-search-results">
                      <div className="graph-sidepanel__head">
                        <h3>搜索结果</h3>
                        <span>{graphSearchHits.size} 条命中</span>
                      </div>
                      {graphSearchHits.size === 0 ? (
                        <p className="graph-sidepanel__hint">未找到匹配的节点。</p>
                      ) : (
                        <ul className="graph-neighbors__list" style={{ maxHeight: "240px", overflowY: "auto" }}>
                          {Array.from(graphSearchHits).slice(0, 50).map((id) => {
                            const node = graphData?.nodes.find((n) => n.id === id);
                            if (!node) return null;
                            return (
                              <li key={node.id}>
                                <button
                                  type="button"
                                  className={`graph-neighbors__item ${graphSelectedNodeId === node.id ? "graph-neighbors__item--active" : ""}`}
                                  onClick={() => handleGraphNodeClick(node)}
                                  title={node.id}
                                >
                                  <span>{node.label || node.id}</span>
                                  <code>{node.group || "未分组"}</code>
                                </button>
                              </li>
                            );
                          })}
                        </ul>
                      )}
                      <hr className="graph-sidepanel__divider" />
                    </div>
                  )}

                  <div className="graph-sidepanel__head">
                    <h3>节点详情</h3>
                    <span>{graphSelectedNode ? "已选中" : "未选中"} · {graphViewMode === "local" ? "Local" : "Global"}</span>
                  </div>
                  {!graphSelectedNode ? (
                    <p className="graph-sidepanel__hint">点击左侧节点可查看标题、度数和关联页面。</p>
                  ) : (
                    <>
                      <div className="graph-node-card">
                        <p className="graph-node-card__title">{graphSelectedNode.label || "未命名页面"}</p>
                        <code className="graph-node-card__path">{graphSelectedNode.id}</code>
                        <div className="graph-node-card__stats">
                          <span className="pill">{`入度 ${graphMetrics.inDegree.get(graphSelectedNode.id) ?? 0}`}</span>
                          <span className="pill">{`出度 ${graphMetrics.outDegree.get(graphSelectedNode.id) ?? 0}`}</span>
                          <span className="pill">{`总连接 ${graphMetrics.totalDegree.get(graphSelectedNode.id) ?? 0}`}</span>
                        </div>
                        <div className="graph-node-card__actions">
                          <button
                            type="button"
                            className="dev-panel__button dev-panel__button--accent"
                            onClick={() => void handleOpenSelectedGraphNode()}
                          >
                            打开页面
                          </button>
                          <button
                            type="button"
                            className={`dev-panel__button ${graphNeighborOnly ? "dev-panel__button--accent" : ""}`}
                            onClick={() => setGraphNeighborOnly((prev) => !prev)}
                          >
                            {graphNeighborOnly ? "退出邻居模式" : "仅看该节点邻居"}
                          </button>
                        </div>
                      </div>
                      <div className="graph-neighbors">
                        <div className="graph-neighbors__head">
                          <h4>关联页面</h4>
                          <span>{graphSelectedNeighbors.length}</span>
                        </div>
                        {graphSelectedNeighbors.length === 0 ? (
                          <p className="graph-sidepanel__hint">该页面当前没有关联边。</p>
                        ) : (
                          <ul className="graph-neighbors__list">
                            {graphSelectedNeighbors.slice(0, 12).map((neighbor) => (
                              <li key={neighbor.id}>
                                <button
                                  type="button"
                                  className="graph-neighbors__item"
                                  onClick={() => handleGraphNodeClick(neighbor)}
                                  title={neighbor.id}
                                >
                                  <span>{neighbor.label}</span>
                                  <code>{neighbor.group || "未分组"}</code>
                                </button>
                              </li>
                            ))}
                          </ul>
                        )}
                      </div>
                    </>
                  )}
                </aside>
              </div>
            </>
          )}

          {/* ---- Settings 模块 ---- */}
          {activeModule === "settings" && (
            <>
              <div className="module-header">
                <h1 className="module-header__title">设置</h1>
                <p className="module-header__sub">Provider 配置与运行策略</p>
              </div>
              <section className="panel">
                <div className="section-head">
                  <h2>LLM Provider 配置</h2>
                  <span className="section-head__hint">
                    {isTauriRuntime() ? "本地配置文件" : "浏览器预览"}
                  </span>
                </div>
                <div className="settings-panel">
                  <p className="dev-panel__hint settings-panel__status">
                    当前活跃 Provider：
                    <strong>
                      {llmConfig
                        ? llmConfig.active_provider === "cloud"
                          ? `${llmConfig.cloud_provider_name || "云端 Provider"}（${llmConfig.cloud_model || defaultCloudModel}）`
                          : "本地 Ollama"
                        : "加载中..."}
                    </strong>
                  </p>
                  <div className="settings-panel__presets">
                    <label className="dev-panel__label" htmlFor="preset-select">Provider 预设</label>
                    <select
                      id="preset-select"
                      className="dev-panel__input"
                      value={selectedPreset}
                      onChange={(e) => handlePresetChange(e.target.value)}
                    >
                      <option value="Custom">Custom (自定义)</option>
                      {llmPresets.map(([name]) => (
                        <option key={name} value={name}>{name}</option>
                      ))}
                    </select>
                  </div>
                  <div className="settings-panel__fields">
                    <div className="dev-panel__field">
                      <label className="dev-panel__label" htmlFor="active-provider">活跃 Provider</label>
                      <select
                        id="active-provider"
                        className="dev-panel__input"
                        value={llmConfigActiveProvider}
                        onChange={(event) =>
                          setLlmConfigActiveProvider(event.target.value === "cloud" ? "cloud" : "ollama")
                        }
                      >
                        <option value="ollama">ollama（本地）</option>
                        <option value="cloud">cloud（云端）</option>
                      </select>
                    </div>
                    <div className="dev-panel__field">
                      <label className="dev-panel__label" htmlFor="cloud-provider-name">云端 Provider 名称</label>
                      <input
                        id="cloud-provider-name"
                        className="dev-panel__input"
                        type="text"
                        value={llmConfigCloudProviderName}
                        onChange={(event) => setLlmConfigCloudProviderName(event.target.value)}
                        placeholder={`${defaultCloudProviderName}（可改为 OpenAI / DeepSeek / GLM / MiniMax）`}
                        spellCheck={false}
                      />
                    </div>
                    <div className="dev-panel__field">
                      <label className="dev-panel__label" htmlFor="cloud-api-key">云端 API Key（OpenAI-compatible）</label>
                      <input
                        id="cloud-api-key"
                        className="dev-panel__input"
                        type="password"
                        value={llmConfigCloudApiKey}
                        onChange={(event) => setLlmConfigCloudApiKey(event.target.value)}
                        placeholder="sk-...（选择 cloud 时必填）"
                        spellCheck={false}
                        autoComplete="off"
                      />
                    </div>
                    <div className="dev-panel__field">
                      <label className="dev-panel__label" htmlFor="cloud-base-url">云端 Base URL</label>
                      <input
                        id="cloud-base-url"
                        className="dev-panel__input"
                        type="text"
                        value={llmConfigCloudBaseUrl}
                        onChange={(event) => setLlmConfigCloudBaseUrl(event.target.value)}
                        placeholder={defaultCloudBaseUrl}
                        spellCheck={false}
                      />
                    </div>
                    <div className="dev-panel__field">
                      <label className="dev-panel__label" htmlFor="cloud-model">云端模型名</label>
                      <input
                        id="cloud-model"
                        className="dev-panel__input"
                        type="text"
                        value={llmConfigCloudModel}
                        onChange={(event) => setLlmConfigCloudModel(event.target.value)}
                        placeholder={defaultCloudModel}
                        spellCheck={false}
                      />
                    </div>
                  </div>
                  <div className="settings-panel__save">
                    <button
                      type="button"
                      className="dev-panel__button dev-panel__button--accent"
                      onClick={() => void handleSaveLlmConfig()}
                      disabled={!isTauriRuntime() || llmConfigSaving}
                    >
                      {llmConfigSaving ? "保存中..." : "保存 LLM 配置"}
                    </button>
                  </div>
                  <p className="settings-panel__hint">
                    {isTauriRuntime()
                      ? "云端配置仅保存在本地配置文件中，不会提交到仓库。可用 DeepSeek、GLM、MiniMax 三家预设，也可自由编辑为任意 OpenAI-compatible Provider。StrictLocal 模式下云 Provider 将被忽略。"
                      : "浏览器预览模式下无法保存配置。"}
                  </p>
                </div>
              </section>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
