import { Component, lazy, Suspense, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent, type ReactNode, useEffect, useMemo, useRef, useState, useCallback } from "react";
import InboxModule from "./modules/inbox/InboxModule";
import AskModule from "./modules/ask/AskModule";
import AgentStudio from "./modules/agent/AgentStudio";
import LintModule from "./modules/lint/LintModule";
import OperationsModule from "./modules/operations/OperationsModule";
import ResearchModule from "./modules/research/ResearchModule";
import SettingsModule from "./modules/settings/SettingsModule";
import WikiModule, { type WikiModuleHandle } from "./modules/wiki/WikiModule";
import GraphModule from "./modules/graph/GraphModule";
import { useVault } from "./contexts/VaultContext";
import { useMode } from "./contexts/ModeContext";
import { useToast } from "./contexts/ToastContext";
export { mergeRecentVaultPaths, readRecentVaultPathsFromStorage, writeRecentVaultPathsToStorage, normalizeRecentVaultPaths, RECENT_VAULT_PATHS_STORAGE_KEY } from "./vault-utils";
import { mergeRecentVaultPaths, readRecentVaultPathsFromStorage, writeRecentVaultPathsToStorage, normalizeRecentVaultPaths, RECENT_VAULT_PATHS_STORAGE_KEY } from "./vault-utils";
import { marked } from "marked";
import DOMPurify from "dompurify";
import appLogo from "./assets/LLM_Wiki.png";
import { getCurrentWindow } from "@tauri-apps/api/window";

const ForceGraph2D = lazy(() => import("react-force-graph-2d"));
import {
  getAppOverview,
  fetchDefaultPaths,
  fetchLlmStatus,
  fetchOcrConfig,
  fetchQuerySettings,
  fetchRecentLogs,
  fetchAskHistory,
  listAskSessions,
  createAskSession,
  fetchAskSessionTurns,
  searchAskSessionTurns,
  renameAskSession,
  deleteAskSession,
  fetchRecentWikiPages,
  fetchWikiPageDetail,
  initVault,
  ingestFile,
  previewIngestFile,
  applyIngestPreview,
  isTauriRuntime,
  pickFiles,
  pickFolder,
  queryAskSession,
  cancelAskSession,
  clearAskHistory,
  saveAskHistory,
  saveOcrConfig,
  getKnowledgeGraph,
  getKnowledgeSubgraph,
  saveWikiPage,
  saveQueryAnswer,
  get_outbox_events,
  enqueueIngest,
  getPageEmbeddingPairs,
  setBackendMode,
  setQueryTopK as persistQueryTopK,
  formatLlmStatusSummary,
  resolveDisplayPath,
  listenProgress,
  startResearch,
  listResearchTasks,
  getResearchTask,
  cancelResearchTask,
  deleteResearchTask,
  getSearchConfig,
  setSearchConfig,
  pickSaveFile,
  askConfirmDialog,
  saveResearchDoc,
  initVaultWithTemplate,
  listenResearchProgress,
  listenResearchDone,
  listenResearchError,
  listenResearchQueriesReady,
  listenResearchStreamChunk,
  approveResearchQueries,
  getClipServerStatus,
  type OcrProvider,
} from "./tauri-client";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import { templates, getTemplate } from "./templates";
import {
  formatLintCheckedAt,
} from "./lint-utils";
import type {
  AppOverview,
  AskHistoryItem,
  AskSessionItem,
  AskSessionSearchHitItem,
  AskSessionTurnItem,
  BackendAppMode,
  KnowledgeGraphData,
  KnowledgeGraphLink,
  KnowledgeGraphNode,
  LlmProviderConfig,
  LlmStatus,
  LintIssue,
  LintPatchPreviewItem,
  LogEntry,
  ModuleId,
  ModuleItem,
  ModeId,
  ProgressPayload,
  QueryAnswerResult,
  ResearchTaskItem,
  ResearchTaskStatus,
  SearchConfig,
  IngestPreview,
  WikiTemplate,
  WikiPageDetail,
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
const ASK_SESSION_LIST_MAX = 80;
const ASK_SESSION_SEARCH_LIMIT = 80;
const ingestSupportedFileExtensions = new Set([
  "md",
  "markdown",
  "pdf",
  "docx",
  "pptx",
  "txt",
  "png",
  "jpg",
  "jpeg",
  "webp",
  "bmp",
  "gif",
  "tif",
  "tiff",
]);

export type DroppedIngestPathsResult = {
  accepted: string[];
  rejected: string[];
  duplicateCount: number;
};

export type TemplateInitPreview = {
  dirs: string[];
  files: string[];
};

// RECENT_VAULT_PATHS_STORAGE_KEY 已移到 vault-utils.ts，由顶部 re-export

// 简单的字符串哈希（用于编辑基线校验和）
export const simpleHash = (str: string): string => {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash |= 0;
  }
  return Math.abs(hash).toString(16);
};

const normalizeTemplateDirPath = (dir: string): string => {
  const normalized = dir.trim().replace(/\\/g, "/").replace(/\/+/g, "/").replace(/^\/+/, "");
  if (!normalized) {
    return "";
  }
  if (
    normalized === "wiki"
    || normalized.startsWith("wiki/")
    || normalized === "raw"
    || normalized.startsWith("raw/")
    || normalized === ".app"
    || normalized.startsWith(".app/")
  ) {
    return normalized;
  }
  return `wiki/${normalized}`;
};

/**
 * 构建模板初始化预览：展示会创建的核心目录与文件（相对 vault 根路径）。
 */
export const buildTemplateInitPreview = (template: WikiTemplate): TemplateInitPreview => {
  const dirSet = new Set<string>(["raw", "wiki", ".app"]);
  const fileSet = new Set<string>(["index.md", "log.md", ".app/config.json", ".app/meta.db"]);

  if (template.id !== "general") {
    fileSet.add("wiki/schema.md");
    fileSet.add("wiki/purpose.md");
  }

  for (const dir of template.extraDirs) {
    const normalized = normalizeTemplateDirPath(dir);
    if (normalized) {
      dirSet.add(normalized);
    }
  }

  return {
    dirs: Array.from(dirSet).sort((a, b) => a.localeCompare(b, "zh-CN")),
    files: Array.from(fileSet).sort((a, b) => a.localeCompare(b, "zh-CN")),
  };
};

// normalizeRecentVaultPaths / mergeRecentVaultPaths / readRecentVaultPathsFromStorage / writeRecentVaultPathsToStorage
// 已移到 vault-utils.ts，由顶部 re-export

/**
 * 解析窗口拖拽文件路径：保留受支持扩展名并去重，返回被忽略条目用于提示。
 */
export const parseDroppedIngestPaths = (paths: string[]): DroppedIngestPathsResult => {
  const seen = new Set<string>();
  const accepted: string[] = [];
  const rejected: string[] = [];
  let duplicateCount = 0;

  for (const rawPath of paths) {
    const path = rawPath.trim();
    if (!path) {
      continue;
    }

    // Windows 路径大小写不敏感，统一小写比较去重。
    const normalizedKey = path.replaceAll("\\", "/").toLowerCase();
    if (seen.has(normalizedKey)) {
      duplicateCount += 1;
      continue;
    }
    seen.add(normalizedKey);

    const fileName = path.split(/[/\\]/).pop() ?? "";
    const extension = fileName.includes(".") ? (fileName.split(".").pop() ?? "").toLowerCase() : "";
    if (!extension || !ingestSupportedFileExtensions.has(extension)) {
      rejected.push(path);
      continue;
    }
    accepted.push(path);
  }

  return { accepted, rejected, duplicateCount };
};

const answerStrategyLabels: Record<string, string> = {
  llm: "LLM 合成",
  rule: "规则回退",
  llm_synthesis: "LLM 合成",
  rule_fallback: "规则回退",
};

// getResearchStatusLabel / getResearchStatusColor 已移到 modules/research/ResearchPanel.tsx
export { getResearchStatusLabel, getResearchStatusColor } from "./modules/research/ResearchPanel";

const searchStrategyLabels: Record<string, string> = {
  fts: "FTS 检索",
  scan: "回退扫描",
  empty: "空结果",
  rrf: "RRF 融合检索",
};

const searchRouteLabels: Record<string, string> = {
  fts: "FTS",
  linked: "链接扩展",
  popular: "引用热度",
  embedding: "Embedding",
  scan: "文件扫描",
};

const graphInsightKindLabels: Record<GraphInsightKind, string> = {
  "isolated-node": "孤立页",
  "sparse-group": "稀疏分组",
  "bridge-node": "桥接节点",
  "surprising-link": "异常连接",
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

const formatQuerySearchRouteLabel = (route?: string | null) => {
  const normalizedRoute = route?.trim().toLowerCase();

  if (!normalizedRoute) {
    return "未知路径";
  }

  return searchRouteLabels[normalizedRoute] ?? normalizedRoute;
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

export type WikiLineDiffKind = "unchanged" | "added" | "removed";

export type WikiLineDiffRow = {
  kind: WikiLineDiffKind;
  line: string;
  oldLineNumber?: number;
  newLineNumber?: number;
};

export const buildWikiLineDiff = (currentContent: string, historyContent: string): WikiLineDiffRow[] => {
  const currentLines = currentContent.split(/\r?\n/);
  const historyLines = historyContent.split(/\r?\n/);
  const dp = Array.from({ length: historyLines.length + 1 }, () =>
    Array<number>(currentLines.length + 1).fill(0),
  );

  for (let i = historyLines.length - 1; i >= 0; i -= 1) {
    for (let j = currentLines.length - 1; j >= 0; j -= 1) {
      dp[i][j] = historyLines[i] === currentLines[j]
        ? dp[i + 1][j + 1] + 1
        : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const rows: WikiLineDiffRow[] = [];
  let oldIndex = 0;
  let newIndex = 0;

  while (oldIndex < historyLines.length && newIndex < currentLines.length) {
    if (historyLines[oldIndex] === currentLines[newIndex]) {
      rows.push({
        kind: "unchanged",
        line: historyLines[oldIndex],
        oldLineNumber: oldIndex + 1,
        newLineNumber: newIndex + 1,
      });
      oldIndex += 1;
      newIndex += 1;
    } else if (dp[oldIndex + 1][newIndex] >= dp[oldIndex][newIndex + 1]) {
      rows.push({
        kind: "removed",
        line: historyLines[oldIndex],
        oldLineNumber: oldIndex + 1,
      });
      oldIndex += 1;
    } else {
      rows.push({
        kind: "added",
        line: currentLines[newIndex],
        newLineNumber: newIndex + 1,
      });
      newIndex += 1;
    }
  }

  while (oldIndex < historyLines.length) {
    rows.push({
      kind: "removed",
      line: historyLines[oldIndex],
      oldLineNumber: oldIndex + 1,
    });
    oldIndex += 1;
  }

  while (newIndex < currentLines.length) {
    rows.push({
      kind: "added",
      line: currentLines[newIndex],
      newLineNumber: newIndex + 1,
    });
    newIndex += 1;
  }

  return rows;
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

export type GraphInsightKind = "isolated-node" | "sparse-group" | "bridge-node" | "surprising-link";

export type GraphInsightItem = {
  kind: GraphInsightKind;
  title: string;
  description: string;
  suggestion: string;
  nodeIds: string[];
  group?: string;
  score: number;
  evidence: string[];
};

export type GraphInsightConfig = {
  isolatedMaxDegree: number;
  sparseGroupMinSize: number;
  sparseDensityThreshold: number;
  bridgeMinGroups: number;
  bridgeCandidateLimit: number;
  surprisingMaxJaccard: number;
  surprisingMinConfidence: number;
  surprisingCandidateLimit: number;
};

// ---- 图谱聚合模式类型与常量 ----

/** 大图聚合模式触发阈值（节点数超过此值时可启用） */
export const GRAPH_AGGREGATE_THRESHOLD = 200;

/** 聚合后的超节点 */
export type AggregatedNode = {
  id: string;        // group name 或 page path（ungrouped）
  label: string;     // group name + " (N)" 或原 label
  group: string;
  isAggregate: boolean;  // true = 超节点
  count: number;     // 聚合了多少个原始节点
};

/** 聚合后的边（weight 为折叠的边数） */
export type AggregatedEdge = {
  source: string;
  target: string;
  weight: number;
};

/** 聚合后的图谱数据结构 */
export type AggregatedGraphData = {
  nodes: AggregatedNode[];
  links: AggregatedEdge[];
};

/**
 * 将原始 KnowledgeGraphData 按 group 聚合：
 * - group 内节点数 >= groupMinSize 的节点合并为超节点
 * - 其余节点（ungrouped 或小组）保持原样
 * - 同组内部的边（自环）去除
 * - 跨超节点的多条边合并（weight 累加）
 */
export function buildAggregatedGraphData(
  nodes: KnowledgeGraphNode[],
  links: GraphNormalizedEdge[],
  groupMinSize: number = 2,
): AggregatedGraphData {
  if (nodes.length === 0) {
    return { nodes: [], links: [] };
  }

  // 统计每个 group 的节点数
  const groupCount = new Map<string, number>();
  for (const node of nodes) {
    if (node.group) {
      groupCount.set(node.group, (groupCount.get(node.group) ?? 0) + 1);
    }
  }

  // 判断某个 group 是否需要聚合
  const shouldAggregate = (group: string): boolean =>
    Boolean(group) && (groupCount.get(group) ?? 0) >= groupMinSize;

  // 构建 nodeId -> 超节点 id 的映射
  const nodeToAgg = new Map<string, string>();
  for (const node of nodes) {
    if (shouldAggregate(node.group)) {
      nodeToAgg.set(node.id, node.group);
    } else {
      nodeToAgg.set(node.id, node.id);
    }
  }

  // 构建超节点列表
  const aggNodeMap = new Map<string, AggregatedNode>();
  for (const node of nodes) {
    const aggId = nodeToAgg.get(node.id)!;
    if (!aggNodeMap.has(aggId)) {
      const isAgg = shouldAggregate(node.group);
      aggNodeMap.set(aggId, {
        id: aggId,
        label: isAgg ? `${node.group} (${groupCount.get(node.group)})` : node.label,
        group: node.group,
        isAggregate: isAgg,
        count: isAgg ? (groupCount.get(node.group) ?? 1) : 1,
      });
    }
  }

  // 构建聚合边（去除自环，合并重复边）
  const edgeWeightMap = new Map<string, number>();
  for (const link of links) {
    const srcAgg = nodeToAgg.get(link.sourceId);
    const tgtAgg = nodeToAgg.get(link.targetId);
    if (!srcAgg || !tgtAgg) continue;
    // 去除自环（同组间的边）
    if (srcAgg === tgtAgg) continue;
    const key = `${srcAgg}|||${tgtAgg}`;
    edgeWeightMap.set(key, (edgeWeightMap.get(key) ?? 0) + 1);
  }

  const aggLinks: AggregatedEdge[] = [];
  for (const [key, weight] of edgeWeightMap) {
    const [source, target] = key.split("|||");
    aggLinks.push({ source, target, weight });
  }

  return {
    nodes: Array.from(aggNodeMap.values()),
    links: aggLinks,
  };
}

export type GraphViewMode = "global" | "local";
export type GraphTraversalDirection = "both" | "out" | "in";
export const GRAPH_VIEW_MODE_STORAGE_KEY = "llm_wiki_graph_view_mode_v1";
export const GRAPH_LOCAL_DEPTH_STORAGE_KEY = "llm_wiki_graph_local_depth_v1";
export const GRAPH_LOCAL_DIRECTION_STORAGE_KEY = "llm_wiki_graph_local_direction_v1";
export const GRAPH_INSIGHT_SPARSE_DENSITY_STORAGE_KEY = "llm_wiki_graph_insight_sparse_density_v1";
export const GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_STORAGE_KEY = "llm_wiki_graph_insight_bridge_min_groups_v1";
export const GRAPH_INSIGHT_SURPRISING_JACCARD_STORAGE_KEY = "llm_wiki_graph_insight_surprising_jaccard_v1";
export const GRAPH_INSIGHT_SURPRISING_CONFIDENCE_STORAGE_KEY =
  "llm_wiki_graph_insight_surprising_confidence_v1";
export const GRAPH_LOCAL_DEPTH_MIN = 1;
export const GRAPH_LOCAL_DEPTH_MAX = 3;
export const GRAPH_LOCAL_BACKEND_NODE_THRESHOLD = 1200;
export const GRAPH_LOCAL_BACKEND_LINK_THRESHOLD = 4000;
export const GRAPH_LOCAL_BACKEND_MAX_NODES = 1500;
export const GRAPH_LOCAL_BACKEND_MAX_LINKS = 8000;
export const GRAPH_INSIGHT_SPARSE_DENSITY_MIN = 0.05;
export const GRAPH_INSIGHT_SPARSE_DENSITY_MAX = 0.6;
export const GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN = 2;
export const GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX = 6;
export const GRAPH_INSIGHT_SURPRISING_JACCARD_MIN = 0;
export const GRAPH_INSIGHT_SURPRISING_JACCARD_MAX = 0.6;
export const GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN = 0.3;
export const GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX = 0.95;

export const DEFAULT_GRAPH_INSIGHT_CONFIG: GraphInsightConfig = {
  isolatedMaxDegree: 1,
  sparseGroupMinSize: 3,
  sparseDensityThreshold: 0.15,
  bridgeMinGroups: 3,
  bridgeCandidateLimit: 3,
  surprisingMaxJaccard: 0.1,
  surprisingMinConfidence: 0.55,
  surprisingCandidateLimit: 3,
};

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

export const clampGraphInsightSparseDensity = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_SPARSE_DENSITY_MIN,
    Math.min(
      GRAPH_INSIGHT_SPARSE_DENSITY_MAX,
      Number.isFinite(value) ? Number(value.toFixed(2)) : DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold,
    ),
  );

export const clampGraphInsightBridgeMinGroups = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN,
    Math.min(
      GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX,
      Number.isFinite(value) ? Math.round(value) : DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups,
    ),
  );

export const clampGraphInsightSurprisingJaccard = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_SURPRISING_JACCARD_MIN,
    Math.min(
      GRAPH_INSIGHT_SURPRISING_JACCARD_MAX,
      Number.isFinite(value) ? Number(value.toFixed(2)) : DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard,
    ),
  );

export const clampGraphInsightSurprisingConfidence = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN,
    Math.min(
      GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX,
      Number.isFinite(value) ? Number(value.toFixed(2)) : DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence,
    ),
  );

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

export const readGraphInsightSparseDensityFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_SPARSE_DENSITY_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold;
    }
    const parsed = Number(raw);
    return clampGraphInsightSparseDensity(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold;
  }
};

export const writeGraphInsightSparseDensityToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_SPARSE_DENSITY_STORAGE_KEY,
      String(clampGraphInsightSparseDensity(value)),
    );
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphInsightBridgeMinGroupsFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups;
    }
    const parsed = Number(raw);
    return clampGraphInsightBridgeMinGroups(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups;
  }
};

export const writeGraphInsightBridgeMinGroupsToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_STORAGE_KEY,
      String(clampGraphInsightBridgeMinGroups(value)),
    );
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphInsightSurprisingJaccardFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_SURPRISING_JACCARD_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard;
    }
    const parsed = Number(raw);
    return clampGraphInsightSurprisingJaccard(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard;
  }
};

export const writeGraphInsightSurprisingJaccardToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_SURPRISING_JACCARD_STORAGE_KEY,
      String(clampGraphInsightSurprisingJaccard(value)),
    );
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphInsightSurprisingConfidenceFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_SURPRISING_CONFIDENCE_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence;
    }
    const parsed = Number(raw);
    return clampGraphInsightSurprisingConfidence(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence;
  }
};

export const writeGraphInsightSurprisingConfidenceToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_SURPRISING_CONFIDENCE_STORAGE_KEY,
      String(clampGraphInsightSurprisingConfidence(value)),
    );
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

const GRAPH_STRUCTURAL_PAGE_NAMES = new Set(["index", "log", "overview"]);

const resolveGraphNodeLeafName = (id: string) =>
  id
    .trim()
    .replaceAll("\\", "/")
    .split("/")
    .pop()
    ?.replace(/\.md$/i, "")
    .toLowerCase() ?? "";

const isGraphStructuralNode = (node: KnowledgeGraphNode) =>
  GRAPH_STRUCTURAL_PAGE_NAMES.has(resolveGraphNodeLeafName(node.id));

const buildUndirectedEdgeKey = (left: string, right: string) =>
  left < right ? `${left}:::${right}` : `${right}:::${left}`;

/**
 * 基于当前图谱结构计算可操作洞察：
 * - 孤立页（度数 <= 1）
 * - 稀疏分组（同组密度低于阈值）
 * - 桥接节点（邻居覆盖分组数高于阈值）
 * - 异常连接（跨组且共同邻居相似度低）
 */
export const buildGraphInsights = (
  nodes: KnowledgeGraphNode[],
  edges: GraphNormalizedEdge[],
  limit: number = 8,
  configOverrides?: Partial<GraphInsightConfig>,
  embeddingSim?: Record<string, number>,
): GraphInsightItem[] => {
  if (nodes.length === 0) {
    return [];
  }

  const mergedConfig: GraphInsightConfig = {
    ...DEFAULT_GRAPH_INSIGHT_CONFIG,
    ...(configOverrides ?? {}),
  };
  const config: GraphInsightConfig = {
    isolatedMaxDegree: Math.max(0, Math.round(mergedConfig.isolatedMaxDegree)),
    sparseGroupMinSize: Math.max(2, Math.round(mergedConfig.sparseGroupMinSize)),
    sparseDensityThreshold: clampGraphInsightSparseDensity(mergedConfig.sparseDensityThreshold),
    bridgeMinGroups: clampGraphInsightBridgeMinGroups(mergedConfig.bridgeMinGroups),
    bridgeCandidateLimit: Math.max(1, Math.round(mergedConfig.bridgeCandidateLimit)),
    surprisingMaxJaccard: clampGraphInsightSurprisingJaccard(mergedConfig.surprisingMaxJaccard),
    surprisingMinConfidence: clampGraphInsightSurprisingConfidence(mergedConfig.surprisingMinConfidence),
    surprisingCandidateLimit: Math.max(1, Math.round(mergedConfig.surprisingCandidateLimit)),
  };

  const nodeMap = new Map(nodes.map((node) => [node.id, node]));
  const degreeMap = new Map<string, number>(nodes.map((node) => [node.id, 0]));
  const adjacency = new Map<string, Set<string>>(nodes.map((node) => [node.id, new Set<string>()]));
  const undirectedEdges = new Set<string>();

  for (const edge of edges) {
    const sourceId = edge.sourceId.trim();
    const targetId = edge.targetId.trim();
    if (!nodeMap.has(sourceId) || !nodeMap.has(targetId) || sourceId === targetId) {
      continue;
    }
    degreeMap.set(sourceId, (degreeMap.get(sourceId) ?? 0) + 1);
    degreeMap.set(targetId, (degreeMap.get(targetId) ?? 0) + 1);
    adjacency.get(sourceId)?.add(targetId);
    adjacency.get(targetId)?.add(sourceId);
    undirectedEdges.add(buildUndirectedEdgeKey(sourceId, targetId));
  }

  const insights: GraphInsightItem[] = [];
  const visibleNonStructuralNodes = nodes.filter((node) => !isGraphStructuralNode(node));
  const visibleNonStructuralNodeIds = new Set(visibleNonStructuralNodes.map((node) => node.id));
  const groupLabel = (group: string) => (group === "__ungrouped__" ? "未分组" : group);
  const resolveNodeLabel = (nodeId: string) =>
    nodeMap.get(nodeId)?.label?.trim() || resolveGraphNodeLeafName(nodeId);
  const toTokenSet = (value: string) => {
    const normalized = value
      .toLowerCase()
      .replace(/\.md$/i, "")
      .replace(/[_\-\\/]+/g, " ");
    const tokens = normalized
      .split(/[^a-z0-9\u4e00-\u9fa5]+/i)
      .map((item) => item.trim())
      .filter((item) => item.length >= 2);
    return new Set(tokens);
  };
  const buildNeighborSetWithoutPair = (nodeId: string, excludeId: string) => {
    const neighbors = adjacency.get(nodeId);
    if (!neighbors) {
      return new Set<string>();
    }
    const next = new Set<string>();
    for (const neighborId of neighbors) {
      if (!isSameWikiPagePath(neighborId, excludeId)) {
        next.add(neighborId);
      }
    }
    return next;
  };
  const calcJaccard = (left: Set<string>, right: Set<string>) => {
    if (left.size === 0 && right.size === 0) {
      return 0;
    }
    let intersection = 0;
    for (const item of left) {
      if (right.has(item)) {
        intersection += 1;
      }
    }
    const union = left.size + right.size - intersection;
    return union > 0 ? intersection / union : 0;
  };

  const calcLexicalDistance = (sourceId: string, targetId: string) => {
    const sourceLabel = resolveNodeLabel(sourceId);
    const targetLabel = resolveNodeLabel(targetId);
    const sourceTokens = toTokenSet(`${sourceLabel} ${resolveGraphNodeLeafName(sourceId)}`);
    const targetTokens = toTokenSet(`${targetLabel} ${resolveGraphNodeLeafName(targetId)}`);
    if (sourceTokens.size === 0 && targetTokens.size === 0) {
      return 1;
    }
    const overlap = calcJaccard(sourceTokens, targetTokens);
    return 1 - overlap;
  };

  const isolatedNodes = visibleNonStructuralNodes.filter(
    (node) => (degreeMap.get(node.id) ?? 0) <= config.isolatedMaxDegree,
  );
  if (isolatedNodes.length > 0) {
    const previewLabels = isolatedNodes
      .slice(0, 5)
      .map((node) => node.label || resolveGraphNodeLeafName(node.id));
    insights.push({
      kind: "isolated-node",
      title: `${isolatedNodes.length} 个孤立页面`,
      description:
        previewLabels.join("、") +
        (isolatedNodes.length > 5 ? ` 等 ${isolatedNodes.length} 个页面连接较弱。` : " 连接较弱。"),
      suggestion: "为这些页面补充 [[wiki-link]] 或补充相关引用，提升知识可达性。",
      nodeIds: isolatedNodes.map((node) => node.id),
      score: isolatedNodes.length,
      evidence: [
        `命中条件：度数 ≤ ${config.isolatedMaxDegree}`,
        `当前命中：${isolatedNodes.length} 个页面`,
      ],
    });
  }

  const groupedNodeIds = new Map<string, string[]>();
  for (const node of visibleNonStructuralNodes) {
    const group = node.group.trim();
    if (!group) {
      continue;
    }
    const bucket = groupedNodeIds.get(group) ?? [];
    bucket.push(node.id);
    groupedNodeIds.set(group, bucket);
  }

  for (const [group, memberIds] of groupedNodeIds) {
    if (memberIds.length < config.sparseGroupMinSize) {
      continue;
    }
    let internalEdges = 0;
    for (let left = 0; left < memberIds.length; left += 1) {
      for (let right = left + 1; right < memberIds.length; right += 1) {
        if (undirectedEdges.has(buildUndirectedEdgeKey(memberIds[left], memberIds[right]))) {
          internalEdges += 1;
        }
      }
    }
    const possibleEdges = (memberIds.length * (memberIds.length - 1)) / 2;
    const density = possibleEdges > 0 ? internalEdges / possibleEdges : 0;
    if (density <= config.sparseDensityThreshold) {
      insights.push({
        kind: "sparse-group",
        title: `稀疏分组：${group}`,
        description: `${memberIds.length} 个页面，组内密度 ${density.toFixed(2)}，内部连接偏弱。`,
        suggestion: "优先补齐该分组内部页面互链，减少知识断层。",
        nodeIds: memberIds,
        group,
        score: 1 + (config.sparseDensityThreshold - density),
        evidence: [
          `命中条件：密度 ≤ ${config.sparseDensityThreshold.toFixed(2)}，分组大小 ≥ ${config.sparseGroupMinSize}`,
          `当前密度：${density.toFixed(2)}（${internalEdges}/${possibleEdges}）`,
        ],
      });
    }
  }

  const bridgeCandidates = visibleNonStructuralNodes
    .map((node) => {
      const neighborIds = adjacency.get(node.id);
      if (!neighborIds || neighborIds.size === 0) {
        return null;
      }
      const neighborGroups = new Set<string>();
      for (const neighborId of neighborIds) {
        const neighbor = nodeMap.get(neighborId);
        if (!neighbor) {
          continue;
        }
        const groupName = neighbor.group.trim() || "__ungrouped__";
        neighborGroups.add(groupName);
      }
      if (neighborGroups.size < config.bridgeMinGroups) {
        return null;
      }
      return {
        node,
        groupCount: neighborGroups.size,
        degree: degreeMap.get(node.id) ?? 0,
      };
    })
    .filter((item): item is { node: KnowledgeGraphNode; groupCount: number; degree: number } => item !== null)
    .sort((left, right) => {
      if (left.groupCount !== right.groupCount) {
        return right.groupCount - left.groupCount;
      }
      return right.degree - left.degree;
    })
    .slice(0, config.bridgeCandidateLimit);

  for (const candidate of bridgeCandidates) {
    insights.push({
      kind: "bridge-node",
      title: `关键桥接页：${candidate.node.label || resolveGraphNodeLeafName(candidate.node.id)}`,
      description: `连接 ${candidate.groupCount} 个分组，总连接数 ${candidate.degree}。`,
      suggestion: "该页面是关键通路，建议优先维护并补充上下游引用。",
      nodeIds: [candidate.node.id],
      score: candidate.groupCount,
      evidence: [
        `命中条件：邻居分组数 ≥ ${config.bridgeMinGroups}`,
        `当前分组覆盖：${candidate.groupCount}，节点度数：${candidate.degree}`,
      ],
    });
  }

  const surprisingCandidates: Array<{
    sourceId: string;
    targetId: string;
    sourceGroup: string;
    targetGroup: string;
    sourceDegree: number;
    targetDegree: number;
    jaccard: number;
    lexicalDistance: number;
    crossGroupRarity: number;
    confidence: number;
    score: number;
    embSim: number | null;
  }> = [];
  const visitedSurprisingKeys = new Set<string>();
  const groupPairEdgeCount = new Map<string, number>();
  for (const edge of edges) {
    const sourceId = edge.sourceId.trim();
    const targetId = edge.targetId.trim();
    if (!sourceId || !targetId || sourceId === targetId) {
      continue;
    }
    if (!visibleNonStructuralNodeIds.has(sourceId) || !visibleNonStructuralNodeIds.has(targetId)) {
      continue;
    }
    const sourceNode = nodeMap.get(sourceId);
    const targetNode = nodeMap.get(targetId);
    if (!sourceNode || !targetNode) {
      continue;
    }
    const sourceGroup = sourceNode.group.trim() || "__ungrouped__";
    const targetGroup = targetNode.group.trim() || "__ungrouped__";
    if (sourceGroup === targetGroup) {
      continue;
    }
    const pairKey = buildUndirectedEdgeKey(sourceGroup, targetGroup);
    groupPairEdgeCount.set(pairKey, (groupPairEdgeCount.get(pairKey) ?? 0) + 1);
  }
  for (const edge of edges) {
    const sourceId = edge.sourceId.trim();
    const targetId = edge.targetId.trim();
    if (!sourceId || !targetId || sourceId === targetId) {
      continue;
    }
    if (!visibleNonStructuralNodeIds.has(sourceId) || !visibleNonStructuralNodeIds.has(targetId)) {
      continue;
    }
    const dedupKey = buildUndirectedEdgeKey(sourceId, targetId);
    if (visitedSurprisingKeys.has(dedupKey)) {
      continue;
    }
    visitedSurprisingKeys.add(dedupKey);
    const sourceNode = nodeMap.get(sourceId);
    const targetNode = nodeMap.get(targetId);
    if (!sourceNode || !targetNode) {
      continue;
    }
    const sourceGroup = sourceNode.group.trim() || "__ungrouped__";
    const targetGroup = targetNode.group.trim() || "__ungrouped__";
    if (sourceGroup === targetGroup) {
      continue;
    }
    const sourceNeighbors = buildNeighborSetWithoutPair(sourceId, targetId);
    const targetNeighbors = buildNeighborSetWithoutPair(targetId, sourceId);
    const jaccard = calcJaccard(sourceNeighbors, targetNeighbors);
    if (jaccard > config.surprisingMaxJaccard) {
      continue;
    }
    const sourceDegree = degreeMap.get(sourceId) ?? 0;
    const targetDegree = degreeMap.get(targetId) ?? 0;
    const lexicalDistance = calcLexicalDistance(sourceId, targetId);
    const pairKey = buildUndirectedEdgeKey(sourceGroup, targetGroup);
    const groupPairCount = groupPairEdgeCount.get(pairKey) ?? 1;
    const crossGroupRarity = 1 / Math.sqrt(groupPairCount);
    // 优先使用 embedding 余弦相似度计算语义因子，无外部数据时回退词汇距离。
    const embSimKey =
      sourceId < targetId ? `${sourceId}||${targetId}` : `${targetId}||${sourceId}`;
    const embSim = embeddingSim ? (embeddingSim[embSimKey] ?? null) : null;
    const semanticFactor = embSim !== null ? 1 - embSim : lexicalDistance;
    const confidence = Number(
      (
        (1 - jaccard) * 0.55 +
        semanticFactor * 0.25 +
        crossGroupRarity * 0.2
      ).toFixed(2),
    );
    if (confidence < config.surprisingMinConfidence) {
      continue;
    }
    const score = confidence * 2 + Math.min((sourceDegree + targetDegree) / 8, 1);
    surprisingCandidates.push({
      sourceId,
      targetId,
      sourceGroup,
      targetGroup,
      sourceDegree,
      targetDegree,
      jaccard,
      lexicalDistance,
      crossGroupRarity,
      confidence,
      score,
      embSim,
    });
  }

  surprisingCandidates
    .sort((left, right) => right.score - left.score)
    .slice(0, config.surprisingCandidateLimit)
    .forEach((candidate) => {
      const semanticLabel =
        candidate.embSim !== null
          ? `语义相似度: ${candidate.embSim.toFixed(2)}`
          : `词汇距离=${candidate.lexicalDistance.toFixed(2)}`;
      insights.push({
        kind: "surprising-link",
        title: `异常连接：${resolveNodeLabel(candidate.sourceId)} ↔ ${resolveNodeLabel(candidate.targetId)}`,
        description: `${groupLabel(candidate.sourceGroup)} 与 ${groupLabel(candidate.targetGroup)} 存在低相似跨组连接。`,
        suggestion: "建议核查该连接的引用依据，确认是高价值桥接还是误链。",
        nodeIds: [candidate.sourceId, candidate.targetId],
        score: candidate.score,
        evidence: [
          `命中条件：Jaccard ≤ ${config.surprisingMaxJaccard.toFixed(2)} 且置信度 ≥ ${config.surprisingMinConfidence.toFixed(2)}`,
          `当前：Jaccard=${candidate.jaccard.toFixed(2)}，${semanticLabel}，跨组稀有度=${candidate.crossGroupRarity.toFixed(2)}`,
          `当前置信度：${candidate.confidence.toFixed(2)}，度数：${candidate.sourceDegree}/${candidate.targetDegree}`,
        ],
      });
    });

  return insights
    .sort((left, right) => {
      if (left.kind === right.kind) {
        return right.score - left.score;
      }
      const weight: Record<GraphInsightKind, number> = {
        "surprising-link": 4,
        "bridge-node": 3,
        "isolated-node": 2,
        "sparse-group": 1,
      };
      return weight[right.kind] - weight[left.kind];
    })
    .slice(0, limit);
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
  const hasAutoOcrFallbackHint =
    normalized.includes("已自动 ocr 回退")
    || normalized.includes("自动 ocr 回退")
    || normalized.includes("自动ocr回退")
    || normalized.includes("auto ocr fallback");

  let messagePrefix = "PDF 摄入失败：";
  let friendlyReason = "读取 PDF 失败，请确认文件可访问且内容有效。";
  if (hasAutoOcrFallbackHint) {
    messagePrefix = "PDF 摄入提示：";
    friendlyReason = "检测到解析兼容性问题，已自动 OCR 回退并继续处理。";
  } else if (normalized.includes("tounicode") || normalized.includes("cmap")) {
    friendlyReason = "PDF 字体映射解析失败，建议先用 PDF 工具另存为新文件后重试。";
  } else if (
    normalized.includes("pdftoppm")
    || normalized.includes("poppler")
    || normalized.includes("missing poppler")
    || normalized.includes("未安装 poppler")
  ) {
    friendlyReason = "未检测到 pdftoppm（Poppler），请安装 Poppler 并将其 bin 目录加入 PATH 后重试。";
  } else if (
    normalized.includes("解析器暂不兼容")
    || normalized.includes("结构不兼容")
    || normalized.includes("parser")
  ) {
    friendlyReason = "PDF 文件可打开，但当前解析器暂不兼容该结构，建议先在阅读器中另存为新 PDF 后重试。";
  } else if (
    normalized.includes("未提取到任何文本")
    || normalized.includes("未提取到可用文本")
    || normalized.includes("empty text")
    || normalized.includes("no text")
    || normalized.includes("扫描件")
  ) {
    friendlyReason = "PDF 中没有可提取文本，可能是扫描件或图片型文档，建议先做 OCR。";
  } else if (
    normalized.includes("is not a pdf")
    || normalized.includes("不是 pdf")
    || normalized.includes("扩展名错误")
  ) {
    friendlyReason = "文件类型不是有效的 PDF，请检查路径或文件格式。";
  }

  if (!compactRaw) {
    return `${messagePrefix}${friendlyReason}`;
  }

  // 原始原因仅保留短片段，避免整段底层错误直接透出。
  const rawSnippetMaxLength = 60;
  const rawSnippet = compactRaw.length > rawSnippetMaxLength
    ? `${compactRaw.slice(0, rawSnippetMaxLength)}...`
    : compactRaw;
  return `${messagePrefix}${friendlyReason}（原因：${rawSnippet}）`;
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

type WikiAutocompleteMatch = {
  triggerStart: number;
  query: string;
};

// 解析光标前是否处于 [[... 自动补全上下文
export const resolveWikiAutocompleteMatch = (
  textBeforeCursor: string,
): WikiAutocompleteMatch | null => {
  const match = textBeforeCursor.match(/\[\[([^\]\n]*)$/);
  if (!match) {
    return null;
  }
  const query = match[1] ?? "";
  const triggerStart = textBeforeCursor.length - query.length - 2;
  if (triggerStart < 0) {
    return null;
  }
  return { triggerStart, query };
};

// 将光标前的 [[query 替换为 [[path]]，并返回新光标位置
export const applyWikiAutocompleteSelection = (input: {
  content: string;
  cursor: number;
  path: string;
}) => {
  const textBeforeCursor = input.content.slice(0, input.cursor);
  const match = resolveWikiAutocompleteMatch(textBeforeCursor);
  if (!match) {
    return null;
  }

  const prefix = input.content.slice(0, match.triggerStart);
  const suffix = input.content.slice(input.cursor);
  const replacedBefore = `${prefix}[[${input.path}]]`;
  const nextContent = `${replacedBefore}${suffix}`;
  return {
    content: nextContent,
    cursor: replacedBefore.length,
  };
};

const measureTextareaCaretPosition = (
  textarea: HTMLTextAreaElement,
  cursor: number,
  contentOverride?: string,
) => {
  const value = typeof contentOverride === "string" ? contentOverride : textarea.value;
  const mirror = document.createElement("div");
  const marker = document.createElement("span");
  const computed = window.getComputedStyle(textarea);

  // 复制关键排版属性，确保镜像测量与 textarea 行为一致。
  const mirroredProps = [
    "box-sizing",
    "width",
    "height",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "font-variant",
    "font-stretch",
    "line-height",
    "letter-spacing",
    "text-transform",
    "text-indent",
    "text-align",
    "white-space",
    "word-break",
    "overflow-wrap",
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
  marker.textContent = "\u200b";
  mirror.appendChild(marker);
  document.body.appendChild(mirror);

  const lineHeight = Number.parseFloat(computed.lineHeight) || 20;
  const rawTop = marker.offsetTop - textarea.scrollTop + lineHeight + 6;
  const rawLeft = marker.offsetLeft - textarea.scrollLeft + 4;
  const maxLeft = Math.max(8, textarea.clientWidth - 260);

  document.body.removeChild(mirror);

  return {
    top: textarea.offsetTop + Math.max(8, rawTop),
    left: textarea.offsetLeft + Math.max(8, Math.min(rawLeft, maxLeft)),
  };
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
export const ASK_SEARCH_DEBUG_VISIBLE_STORAGE_KEY = "llm_wiki_ask_search_debug_visible_v1";
export const DROP_MODE_STORAGE_KEY = "llm-wiki-drop-mode";

export type DropMode = "direct" | "queue";

export const isDropMode = (value: string): value is DropMode =>
  value === "direct" || value === "queue";

export const readDropModeFromStorage = (): DropMode => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return "direct";
    const raw = storage.getItem(DROP_MODE_STORAGE_KEY);
    if (!raw) return "direct";
    return isDropMode(raw) ? raw : "direct";
  } catch {
    return "direct";
  }
};

export const writeDropModeToStorage = (mode: DropMode): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return;
    storage.setItem(DROP_MODE_STORAGE_KEY, mode);
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const QUERY_HISTORY_STORAGE_KEY = "llm_wiki_query_history";
export const QUERY_HISTORY_MAX = 30;

export const readAskSearchDebugVisibleFromStorage = (): boolean => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return false;
    }
    return storage.getItem(ASK_SEARCH_DEBUG_VISIBLE_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
};

export const writeAskSearchDebugVisibleToStorage = (visible: boolean): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(ASK_SEARCH_DEBUG_VISIBLE_STORAGE_KEY, visible ? "1" : "0");
  } catch {
    // 本地存储不可用时静默降级，避免影响主流程。
  }
};

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

export const filterAskSessions = (
  sessions: AskSessionItem[],
  keyword: string,
): AskSessionItem[] => {
  const normalizedKeyword = keyword.trim().toLocaleLowerCase("zh-CN");
  if (!normalizedKeyword) {
    return sessions;
  }
  return sessions.filter((item) => {
    const title = item.title?.toLocaleLowerCase("zh-CN") ?? "";
    const preview = item.last_turn_content?.toLocaleLowerCase("zh-CN") ?? "";
    return title.includes(normalizedKeyword) || preview.includes(normalizedKeyword);
  });
};

export const formatAskSessionSearchSnippet = (raw: string): string => {
  const normalized = raw.replace(/\s+/g, " ").trim();
  if (!normalized) {
    return "";
  }
  return normalized.length > 88 ? `${normalized.slice(0, 88)}…` : normalized;
};

export const formatSourceType = (sourceType: string): string => {
  const map: Record<string, string> = {
    file: "本地文件",
    url: "网页链接",
    pdf: "PDF 文档",
    clipboard: "剪贴板",
  };
  return map[sourceType] ?? sourceType;
};

export const buildAskSessionExportMarkdown = (
  session: Pick<AskSessionItem, "title" | "session_id" | "created_at" | "updated_at">,
  turns: Array<Pick<AskSessionTurnItem, "role" | "content" | "created_at">>,
): string => {
  const lines = [
    `# ${session.title || "未命名会话"}`,
    "",
    `- Session ID: \`${session.session_id}\``,
    `- Created At: ${session.created_at || ""}`,
    `- Updated At: ${session.updated_at || ""}`,
    "",
    "## 对话记录",
    "",
  ];

  if (turns.length === 0) {
    lines.push("_暂无对话内容_");
    return lines.join("\n");
  }

  for (const turn of turns) {
    lines.push(`### ${turn.role === "user" ? "用户" : "助手"} · ${turn.created_at || ""}`);
    lines.push("");
    lines.push(turn.content || "");
    lines.push("");
  }
  return lines.join("\n");
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
  { id: "agent", name: "Agent Studio", description: "Agent run 的最小工作台。" },
  { id: "ask", name: "Ask", description: "基于索引与引用证据的问答入口。" },
  { id: "wiki", name: "Wiki", description: "Markdown Vault 的页面编辑与浏览。" },
  { id: "lint", name: "Lint", description: "一致性检查、孤儿页与过期结论扫描。" },
  { id: "graph", name: "图谱", description: "Wiki 页面知识图谱可视化。" },
  { id: "research", name: "研究", description: "多轮检索与研究任务编排。" },
  { id: "inbox", name: "Inbox", description: "收集资料、待处理输入与任务入口。" },
  { id: "operations", name: "运行", description: "摄入队列与运行统计。" },
  { id: "settings", name: "Settings", description: "模式、Provider 与本地配置。" },
];

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
    searchDebug?: import("./types").QuerySearchDebug | null;
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
    getAppOverview(),
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
  const { statusMessage, setStatusMessage } = useToast();
  const { vaultPath, setVaultPath } = useVault();
  const [dropMode, setDropMode] = useState<DropMode>(() => readDropModeFromStorage());
  // Wiki 模块通过 ref 暴露 openPage 方法，供跨模块调用
  const wikiModuleRef = useRef<WikiModuleHandle | null>(null);

  // 当前激活的导航模块（来自 ModeContext）
  const { activeModule, navigateTo: setActiveModule } = useMode();
  // requestedOperationsTab: App 导航到 operations 时请求的 tab（由 OperationsModule 消费后保持自管）
  const [requestedOperationsTab, setRequestedOperationsTab] = useState<"queue" | "stats" | undefined>(undefined);
  // ── 面板拖拽分割 ──────────────────────────────────────────────
  const [sidebarWidth, setSidebarWidth] = useState(220);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const sidebarDragRef = useRef({ active: false, startX: 0, startW: 220 });

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (sidebarDragRef.current.active) {
        const delta = e.clientX - sidebarDragRef.current.startX;
        setSidebarWidth(Math.max(160, Math.min(400, sidebarDragRef.current.startW + delta)));
      }
    };
    const onUp = () => {
      sidebarDragRef.current.active = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      document.body.classList.remove('split-dragging');
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
    return () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
  }, []);

  // 窗口激活/失活时切换 CSS class，使顶部 border-top 颜色跟随 DWM 边框变化
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const win = getCurrentWindow();
    // 初始化：先查询一次当前焦点状态
    void win.isFocused().then((focused) => {
      document.documentElement.classList.toggle('window-focused', focused);
    });
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      document.documentElement.classList.toggle('window-focused', focused);
    });
    return () => { void unlisten.then((fn) => fn()); };
  }, []);
  // ─────────────────────────────────────────────────────────────

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [data, defaultPaths] = await Promise.all([
        loadAppData(),
        fetchDefaultPaths(),
      ]);

      if (!cancelled) {
        setOverview(data.overview);
        setLogs(data.logs);
        setPages(data.pages);
        if (defaultPaths) {
          setVaultPath(defaultPaths.vault_path);
        }
        setLlmStatus(data.llmStatus);
        setLlmStatusLoaded(true);
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const refreshAppData = async (_options?: { includeGraph?: boolean }) => {
    const data = await loadAppData();
    setOverview(data.overview);
    setLogs(data.logs);
    setPages(data.pages);
    setLlmStatus(data.llmStatus);
    setLlmStatusLoaded(true);
  };

  /** 跨模块打开 Wiki 页面（委托给 WikiModule 的 ref handle） */
  const handleOpenWikiPage = async (pagePath: string) => {
    await wikiModuleRef.current?.openPage(pagePath);
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

  const handleCreateLintTargetPage = async (targetTitle: string) => {
    try {
      setStatusMessage(`正在创建页面：${targetTitle}...`);
      await saveWikiPage(targetTitle, `# ${targetTitle}\n`);
      setStatusMessage(`页面 ${targetTitle} 创建成功！`);
      await refreshAppData();
    } catch (error) {
      console.error("创建页面失败:", error);
      setStatusMessage("页面创建失败，请重试。");
    }
  };

  const handleOpenLintPatchPage = async (path: string) => {
    setActiveModule("wiki");
    await handleOpenWikiPage(path);
  };

  const openOperationsModule = (tab: "queue" | "stats") => {
    setRequestedOperationsTab(tab);
    setActiveModule("operations");
  };

  const handleNavModuleSelect = (moduleId: ModuleId) => {
    setActiveModule(moduleId);
  };

  const handleOpenResearchWikiPage = (path: string) => {
    fetchWikiPageDetail(path)
      .then((detail) => {
        if (detail) {
          setActiveModule("wiki");
        }
      })
      .catch(() => {});
  };

  // 侧边栏按"核心 / 运行 / 系统"分组，运行与系统下沉到底。
  const navGroups: Array<{
    id: string;
    title: string;
    items: Array<{ id: ModuleId; icon: string; label: string }>;
    isolated?: boolean;
  }> = [
    {
      id: "core",
      title: "核心",
      items: [
        { id: "agent", icon: "🧠", label: "Agent Studio" },
        { id: "ask", icon: "💬", label: "Ask" },
        { id: "wiki", icon: "📄", label: "Wiki" },
        { id: "lint", icon: "🔍", label: "Lint" },
        { id: "graph", icon: "🕸", label: "图谱" },
        { id: "research", icon: "🔬", label: "研究" },
        { id: "inbox", icon: "⊞", label: "概览" },
      ],
    },
    {
      id: "operations",
      title: "运行",
      items: [
        { id: "operations", icon: "📦", label: "运行" },
      ],
    },
    {
      id: "system",
      title: "系统",
      items: [{ id: "settings", icon: "⚙", label: "设置" }],
    },
  ];

  const handleWindowControl = useCallback(async (action: "minimize" | "toggleMaximize" | "close") => {
    if (!isTauriRuntime()) {
      return;
    }
    try {
      const currentWindow = getCurrentWindow();
      if (action === "minimize") {
        await currentWindow.minimize();
        return;
      }
      if (action === "toggleMaximize") {
        await currentWindow.toggleMaximize();
        return;
      }
      await currentWindow.close();
    } catch (error) {
      console.warn("窗口控制操作失败。", error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`窗口操作失败：${message}`);
    }
  }, []);

  const handleTitlebarMouseDown = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    if (!isTauriRuntime()) {
      return;
    }
    if (event.button !== 0) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (target?.closest(".window-titlebar__actions")) {
      return;
    }
    // 在无边框窗口上显式触发拖动，避免仅依赖 data-tauri-drag-region 的兼容差异。
    void getCurrentWindow().startDragging().catch((error) => {
      console.warn("窗口拖拽启动失败。", error);
    });
  }, []);

  const handleTitlebarDoubleClick = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest(".window-titlebar__actions")) {
      return;
    }
    void handleWindowControl("toggleMaximize");
  }, [handleWindowControl]);

  const tauriRuntime = isTauriRuntime();

  return (
    <div className={`app-root${tauriRuntime ? " app-root--tauri" : ""}`}>
      {tauriRuntime ? (
        <header
          className="window-titlebar"
          onMouseDown={handleTitlebarMouseDown}
          onDoubleClick={handleTitlebarDoubleClick}
        >
          <div
            className="window-titlebar__drag-region"
            data-tauri-drag-region
          >
            <div className="window-titlebar__brand">
              <div className="window-titlebar__logo" aria-hidden="true">
                <img className="window-titlebar__logo-image" src={appLogo} alt="" />
              </div>
              <span className="window-titlebar__title">
                LLM Wiki
              </span>
            </div>
          </div>
          <div className="window-titlebar__drag-spacer" data-tauri-drag-region />
          <div className="window-titlebar__actions">
            <button
              type="button"
              className="window-titlebar__action-btn window-titlebar__action-btn--minimize"
              aria-label="最小化窗口"
              onClick={() => {
                void handleWindowControl("minimize");
              }}
            >
              <span className="window-titlebar__action-glyph" aria-hidden="true">—</span>
            </button>
            <button
              type="button"
              className="window-titlebar__action-btn window-titlebar__action-btn--maximize"
              aria-label="最大化或还原窗口"
              onClick={() => {
                void handleWindowControl("toggleMaximize");
              }}
            >
              <span className="window-titlebar__action-glyph window-titlebar__action-glyph--maximize" aria-hidden="true" />
            </button>
            <button
              type="button"
              className="window-titlebar__action-btn window-titlebar__action-btn--close"
              aria-label="关闭窗口"
              onClick={() => {
                void handleWindowControl("close");
              }}
            >
              <span className="window-titlebar__action-glyph" aria-hidden="true">✕</span>
            </button>
          </div>
        </header>
      ) : null}
      <div className="app-shell">
      {/* 侧边栏导航 */}
      <nav
        className={`sidebar${sidebarCollapsed ? " sidebar--collapsed" : ""}`}
        style={{ width: sidebarCollapsed ? 52 : sidebarWidth }}
      >
        <div className="sidebar__brand">
          <div className="sidebar__brand-logo" aria-hidden="true">
            <img className="sidebar__brand-logo-image" src={appLogo} alt="" />
          </div>
          {!sidebarCollapsed && <span className="sidebar__brand-name">LLM Wiki</span>}
        </div>
        <div className="sidebar__nav">
          {navGroups.map((group) => (
            <section
              key={group.id}
              className={`sidebar__nav-group${group.isolated ? " sidebar__nav-group--isolated" : ""}`}
            >
              {!sidebarCollapsed && <header className="sidebar__nav-group-title">{group.title}</header>}
              <ul className="sidebar__nav-group-list">
                {group.items.map((item) => (
                  <li key={item.id}>
                    <button
                      type="button"
                      className={`sidebar__nav-item${activeModule === item.id ? " sidebar__nav-item--active" : ""}`}
                      title={sidebarCollapsed ? item.label : undefined}
                      onClick={() => {
                        handleNavModuleSelect(item.id);
                      }}
                    >
                      <span className="sidebar__nav-icon">{item.icon}</span>
                      {!sidebarCollapsed && <span className="sidebar__nav-label">{item.label}</span>}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
        <div className="sidebar__footer">
          <button
            type="button"
            className="sidebar__collapse-btn"
            title={sidebarCollapsed ? "展开侧边栏" : "收起侧边栏"}
            onClick={() => setSidebarCollapsed((v) => !v)}
          >
            {sidebarCollapsed ? "▶" : "◀"}
          </button>
          {!sidebarCollapsed && (
            <div className="sidebar__llm-status">
              <span
                className={`sidebar__llm-dot${llmStatus?.available ? " sidebar__llm-dot--ok" : " sidebar__llm-dot--off"}`}
              />
              <span className="sidebar__llm-label">{llmModelText}</span>
            </div>
          )}
        </div>
      </nav>
      {/* 侧边栏 / 主内容 分割拖拽条 */}
      {!sidebarCollapsed && (
        <div
          className="split-handle"
          onMouseDown={(e) => {
            e.preventDefault();
            sidebarDragRef.current = { active: true, startX: e.clientX, startW: sidebarWidth };
            document.body.style.cursor = 'col-resize';
            document.body.style.userSelect = 'none';
            document.body.classList.add('split-dragging');
          }}
        />
      )}

      {/* 主内容区 */}
      <div className="main-content">
        {statusMessage ? (
          <div className="status-bar">
            <span>{statusMessage}</span>
            <button
              type="button"
              className="status-bar__close"
              onClick={() => setStatusMessage("")}
            >
              ✕
            </button>
          </div>
        ) : null}

        <div className={`module-viewport${activeModule === "ask" ? " module-viewport--ask" : ""}${activeModule === "agent" ? " module-viewport--agent" : ""}`}>
          {/* ---- 概览模块 ---- */}
          {activeModule === "inbox" && (
            <InboxModule
              overview={overview}
              pagesCount={pages.length}
              logs={logs}
              dropMode={dropMode}
              onRefreshAppData={refreshAppData}
              navigateTo={setActiveModule}
              llmAvailabilityText={llmAvailabilityText}
              llmModelText={llmModelText}
              llmAddressText={llmAddressText}
              llmHintText={llmHintText}
            />
          )}
          {/* ---- Wiki 模块 ---- */}
          {activeModule === "wiki" && (
            <WikiModule
              ref={wikiModuleRef}
              pages={pages}
              onPagesChange={setPages}
            />
          )}

          {/* ---- Ask 模块 ---- */}
          {activeModule === "ask" && (
            <AskModule onOpenWikiPage={handleOpenWikiPage} />
          )}

          {/* ---- Lint 模块 ---- */}
          {activeModule === "lint" && (
            <LintModule
              onRefreshAppData={refreshAppData}
              onCreateBrokenWikiLinkPage={handleCreateLintTargetPage}
              onOpenPatchPage={handleOpenLintPatchPage}
            />
          )}

          {/* ---- 图谱模块 ---- */}
          {activeModule === "graph" && (
            <GraphModule
              handleOpenWikiPage={handleOpenWikiPage}
            />
          )}

          {/* ---- Settings 模块 ---- */}
          {activeModule === "settings" && (
            <SettingsModule
              onRefreshAppData={refreshAppData}
              dropMode={dropMode}
              onDropModeChange={(mode) => {
                setDropMode(mode);
                writeDropModeToStorage(mode);
              }}
            />
          )}
          {/* ---- 运行模块（队列 + 统计合并） ---- */}
          {activeModule === "operations" && (
            <OperationsModule
              requestedTab={requestedOperationsTab}
              navigateTo={setActiveModule}
            />
          )}
          {/* ---- Deep Research 模块 ---- */}
          {activeModule === "research" && (
            <ResearchModule onOpenWikiPage={handleOpenResearchWikiPage} />
          )}
          {/* ---- Agent Studio 模块 ---- */}
          {activeModule === "agent" && (
            <AgentStudio onOpenWikiPage={handleOpenWikiPage} />
          )}
        </div>
      </div>
    </div>
    </div>
  );
}

// QueuePanel 已提取到 modules/operations/QueuePanel.tsx

// ResearchDialog 已提取到 modules/research/ResearchDialog.tsx


// ResearchPanel 已提取到 modules/research/ResearchPanel.tsx


// SearchConfigPanel 已提取到 modules/lint/SearchConfigPanel.tsx
