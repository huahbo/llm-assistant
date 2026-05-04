import { Component, lazy, Suspense, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent, type ReactNode, useEffect, useMemo, useRef, useState, useCallback } from "react";
import OperationsModule from "./modules/operations/OperationsModule";
import ResearchPanel from "./modules/research/ResearchPanel";
import SettingsModule from "./modules/settings/SettingsModule";
import { useVault } from "./contexts/VaultContext";
import { useMode } from "./contexts/ModeContext";
import {
  shellPolicyProfiles,
  useShellPolicy,
} from "./contexts/ShellPolicyContext";
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
  fetchLlmConfig,
  fetchOcrConfig,
  fetchRecentLintPatchEvents,
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
  fetchWikiPageCitations,
  getLlmProviderPresets,
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
  runLint,
  applyLintPatch,
  applyLintPatchesBatch,
  previewLintPatches,
  saveLlmConfig,
  saveAskHistory,
  saveOcrConfig,
  deleteWikiPage,
  getWikiPageHistoryEntry,
  restoreWikiPageFromHistory,
  getKnowledgeGraph,
  getKnowledgeSubgraph,
  markPageStale,
  renameWikiPage,
  saveWikiPage,
  searchWikiPages,
  searchWikiPaths,
  saveQueryAnswer,
  get_outbox_events,
  enqueueIngest,
  getPageEmbeddingPairs,
  listIngestQueue,
  cancelIngestItem,
  retryIngestItem,
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
  listWikiPageHistory,
  listenResearchProgress,
  listenResearchDone,
  listenResearchError,
  listenResearchQueriesReady,
  listenResearchStreamChunk,
  approveResearchQueries,
  getClipServerStatus,
  getVaultStats,
  createWikiPageWithAi,
  startAgentRun,
  appendAgentRunEvent,
  listAgentRuns,
  archiveAgentRun,
  restoreAgentRun,
  listAgentRunEvents,
  completeAgentRun,
  generateAgentDraft,
  listAgentDrafts,
  checkAgentDraftConflict,
  approveAgentDraft,
  listAgentMemories,
  upsertAgentMemory,
  deleteAgentMemory,
  rewriteAgentDraft,
  listAgentSkills,
  upsertAgentSkill,
  deleteAgentSkill,
  runAgentTask,
  runShell,
  createShellSession,
  closeShellSession,
  listenShellStreamChunk,
  approveAgentWrite,
  rejectAgentWrite,
  type OcrProvider,
} from "./tauri-client";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import { templates, getTemplate } from "./templates";
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
  AgentChatMessage,
  AgentDraftConflictInfo,
  AgentDraftItem,
  AgentMemoryItem,
  AgentRunEventItem,
  AgentRunEventLevel,
  AgentRunItem,
  AgentRunStatus,
  AgentSkillItem,
  AppOverview,
  AskHistoryItem,
  AskSessionItem,
  AskSessionSearchHitItem,
  AskSessionTurnItem,
  BackendAppMode,
  IngestQueueItem,
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
  ResearchTaskItem,
  ResearchTaskStatus,
  SearchConfig,
  IngestPreview,
  VaultStats,
  NewPageResult,
  ShellResult,
  ShellHistoryEntry,
  ShellSessionInfo,
  ShellStreamChunk,
  WikiTemplate,
  WikiPageDetail,
  WikiPageCitation,
  WikiPageHistoryEntry,
  WikiPageHistorySummary,
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
const AGENT_RUN_LIST_LIMIT = 50;
const AGENT_RUN_EVENT_LIST_LIMIT = 200;
const AGENT_DRAFT_LIST_LIMIT = 50;
const AGENT_SKILL_LIST_LIMIT = 50;
const AGENT_ACTIVE_SKILL_STORAGE_KEY = "llm_wiki_agent_active_skill";
const agentEventLevelOptions: AgentRunEventLevel[] = ["info", "warn", "error"];
const agentCompleteStatusOptions: AgentRunStatus[] = ["applied", "failed"];
type AgentReviewTab = "draft" | "diff" | "citations";
type AgentFlowMode = "idle" | "playing" | "paused" | "done";
type AgentRightTab = "task" | "draft" | "tools";
type AgentExecTimelineItem = {
  key: string;
  kind: "tool" | "marker";
  level: AgentRunEventLevel | "awaiting_approval";
  title: string;
  summary: string;
  detail?: string;
  createdAt: string;
  durationMs?: number;
};

const formatAgentRunStatusLabel = (status: string): string => {
  const normalized = status.trim().toLowerCase();
  if (normalized === "running") return "生成中";
  if (normalized === "applied" || normalized === "approved") return "已写入 Wiki";
  if (normalized === "failed") return "执行失败";
  if (normalized === "reviewing") return "待审阅";
  if (normalized === "queued") return "排队中";
  return status || "未知状态";
};

const getAgentRunStatusTone = (status: string): "running" | "reviewing" | "applied" | "failed" | "queued" | "unknown" => {
  const normalized = status.trim().toLowerCase();
  if (normalized === "running") return "running";
  if (normalized === "reviewing") return "reviewing";
  if (normalized === "applied" || normalized === "approved") return "applied";
  if (normalized === "failed") return "failed";
  if (normalized === "queued") return "queued";
  return "unknown";
};

const extractSkillKeyFromEventMessage = (message: string): string => {
  const normalized = message.trim();
  if (!normalized) {
    return "";
  }
  const matched = normalized.match(/skill:\s*([^\s，,)\]）]+)/i);
  return matched?.[1]?.trim() ?? "";
};

const readAgentActiveSkillKeyFromStorage = (): string => {
  if (typeof window === "undefined") {
    return "";
  }
  try {
    const raw = globalThis.localStorage?.getItem(AGENT_ACTIVE_SKILL_STORAGE_KEY) ?? "";
    return raw.trim();
  } catch {
    return "";
  }
};

const writeAgentActiveSkillKeyToStorage = (skillKey: string): void => {
  if (typeof window === "undefined") {
    return;
  }
  try {
    if (skillKey.trim()) {
      globalThis.localStorage?.setItem(AGENT_ACTIVE_SKILL_STORAGE_KEY, skillKey.trim());
    } else {
      globalThis.localStorage?.removeItem(AGENT_ACTIVE_SKILL_STORAGE_KEY);
    }
  } catch {
    // 本地存储异常时静默降级，不阻塞主流程。
  }
};

const normalizeEventLevel = (level: string): AgentRunEventLevel | "awaiting_approval" => {
  const normalized = String(level).trim().toLowerCase();
  if (normalized === "warn" || normalized === "error" || normalized === "awaiting_approval") {
    return normalized;
  }
  return "info";
};

const isAwaitingApprovalMarker = (level: string, message: string): boolean => {
  const normalizedLevel = normalizeEventLevel(level);
  if (normalizedLevel === "awaiting_approval") {
    return true;
  }
  const text = String(message ?? "");
  return text.includes("等待审批") || text.includes("等待人工确认") || text.includes("require_approval");
};

const isApprovalResolvedMarker = (message: string): boolean => {
  const text = String(message ?? "");
  return (
    text.includes("审批通过")
    || text.includes("审批拒绝")
    || text.includes("已取消写入")
    || text.includes("已写入:")
    || text.includes("已编辑:")
  );
};

const getAgentStatusTone = (message: string): "info" | "success" | "warn" | "error" => {
  const text = String(message ?? "");
  if (!text) return "info";
  if (text.includes("失败") || text.includes("错误")) return "error";
  if (text.includes("拒绝") || text.includes("取消")) return "warn";
  if (text.includes("✅") || text.includes("已写入") || text.includes("已审批")) return "success";
  return "info";
};

const parseEventTimestamp = (value: string): number | null => {
  const raw = String(value ?? "").trim();
  if (!raw) return null;
  if (/^\d+$/.test(raw)) {
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : null;
  }
  const parsed = Date.parse(raw);
  return Number.isFinite(parsed) ? parsed : null;
};

const truncateText = (text: string, max = 160): string => {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (normalized.length <= max) return normalized;
  return `${normalized.slice(0, max)}...`;
};

const chunkAgentDraftBlock = (block: string, maxLen = 180): string[] => {
  const normalized = block.trim();
  if (!normalized) {
    return [];
  }
  if (normalized.length <= maxLen || /^#{1,6}\s+/.test(normalized) || /^[-*]\s+/.test(normalized)) {
    return [`${normalized}\n\n`];
  }
  const chunks: string[] = [];
  let cursor = 0;
  while (cursor < normalized.length) {
    let end = Math.min(normalized.length, cursor + maxLen);
    if (end < normalized.length) {
      const cutAt = normalized.lastIndexOf(" ", end);
      if (cutAt > cursor + Math.floor(maxLen * 0.45)) {
        end = cutAt;
      }
    }
    const part = normalized.slice(cursor, end).trim();
    if (part) {
      chunks.push(`${part}${end >= normalized.length ? "\n\n" : " "}`);
    }
    cursor = end;
  }
  return chunks;
};

const buildAgentDraftFlowModel = (content: string): { prefix: string; chunks: string[]; outline: string[] } => {
  const normalized = content.replace(/\r\n/g, "\n").trim();
  if (!normalized) {
    return { prefix: "", chunks: [], outline: [] };
  }
  const blocks = normalized
    .split(/\n{2,}/)
    .map((item) => item.trim())
    .filter(Boolean);
  const outline = blocks
    .filter((item) => /^#{1,6}\s+/.test(item))
    .map((item) => item.replace(/^#{1,6}\s+/, "").trim())
    .filter(Boolean);
  const prefix = outline.length > 0
    ? [
      "### 结构提纲",
      ...outline.map((item, index) => `${index + 1}. ${item}`),
      "",
      "### 正文生成",
      "",
    ].join("\n")
    : "";
  const chunks = blocks.flatMap((block) => chunkAgentDraftBlock(block));
  return { prefix, chunks, outline };
};

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
  const { statusMessage, setStatusMessage, agentStatusMessage, setAgentStatusMessage } = useToast();
  const [switchingMode, setSwitchingMode] = useState<ModeId | null>(null);
  const [devAction, setDevAction] = useState<DevAction | null>(null);
  const [lintRunning, setLintRunning] = useState(false);
  const [queryRunning, setQueryRunning] = useState(false);
  const { vaultPath, recentVaultPaths, setVaultPath, setRecentVaultPaths } = useVault();
  const [selectedTemplateId, setSelectedTemplateId] = useState<string>("general");
  const selectedTemplate = useMemo<WikiTemplate>(() => {
    try {
      return getTemplate(selectedTemplateId);
    } catch {
      return templates[0];
    }
  }, [selectedTemplateId]);
  const templateInitPreview = useMemo(
    () => buildTemplateInitPreview(selectedTemplate),
    [selectedTemplate],
  );
  const [ingestSourcePath, setIngestSourcePath] = useState(defaultIngestSourcePath);
  const [ingestPdfPath, setIngestPdfPath] = useState(defaultIngestPdfPath);
  const [ingestFilePath, setIngestFilePath] = useState(defaultIngestFilePath);
  const [ingestFilePickedPaths, setIngestFilePickedPaths] = useState<string[]>([]);
  const [ingestFileOcrProvider, setIngestFileOcrProvider] = useState<OcrProvider>(
    () => readOcrProviderFromStorage(),
  );
  const [dropMode, setDropMode] = useState<DropMode>(() => readDropModeFromStorage());
  // URL 摄入输入框的状态，避免与 ingestUrl 函数名冲突，使用 ingestUrlInput。
  const [ingestUrlInput, setIngestUrlInput] = useState("");
  const CLIP_SERVER_PORT = 19827;
  const [clipServerOnline, setClipServerOnline] = useState<boolean | null>(null);
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
  // 当前会话 ID（每次"新对话"重新生成）
  const [askSessionId, setAskSessionId] = useState<string>(() => crypto.randomUUID());
  const [showAskAdvanced, setShowAskAdvanced] = useState(false);
  const [expandedCitationIds, setExpandedCitationIds] = useState<Set<string>>(new Set());
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const askFocusTimerRef = useRef<number | null>(null);
  const [wikiKeyword, setWikiKeyword] = useState("");
  const [newPageTopic, setNewPageTopic] = useState("");
  const [newPageCreating, setNewPageCreating] = useState(false);
  const [newPageResult, setNewPageResult] = useState<NewPageResult | null>(null);
  const [showNewPageModal, setShowNewPageModal] = useState(false);
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
  // 编辑基线校验和（用于保存时检测并发编辑冲突）
  const [wikiEditBaselineChecksum, setWikiEditBaselineChecksum] = useState("");
  // 内链自动补全相关状态
  const [wikiAutocompleteOpen, setWikiAutocompleteOpen] = useState(false);
  const [wikiAutocompleteResults, setWikiAutocompleteResults] = useState<string[]>([]);
  const [wikiAutocompleteIndex, setWikiAutocompleteIndex] = useState(0);
  const [wikiAutocompleteQuery, setWikiAutocompleteQuery] = useState("");
  const wikiEditorRef = useRef<HTMLTextAreaElement>(null);
  const [wikiAutocompletePos, setWikiAutocompletePos] = useState({ top: 0, left: 0 });
  const wikiAutocompleteRequestIdRef = useRef(0);
  const [wikiSaveRunning, setWikiSaveRunning] = useState(false);
  const [wikiSaveError, setWikiSaveError] = useState("");
  const [wikiDeleteRunning, setWikiDeleteRunning] = useState(false);
  const [wikiRenameMode, setWikiRenameMode] = useState(false);
  const [wikiRenameInput, setWikiRenameInput] = useState("");
  const [wikiRenameRunning, setWikiRenameRunning] = useState(false);
  const [wikiRenameError, setWikiRenameError] = useState("");
  const [wikiHistoryOpen, setWikiHistoryOpen] = useState(false);
  const [wikiHistoryEntries, setWikiHistoryEntries] = useState<WikiPageHistorySummary[]>([]);
  const [wikiHistorySelectedEntry, setWikiHistorySelectedEntry] = useState<WikiPageHistoryEntry | null>(null);
  const [wikiHistoryLoading, setWikiHistoryLoading] = useState(false);
  const [wikiHistoryEntryLoading, setWikiHistoryEntryLoading] = useState(false);
  const [wikiHistoryError, setWikiHistoryError] = useState("");
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
  const [llmConfigOllamaModel, setLlmConfigOllamaModel] = useState("");
  const [llmConfigOllamaBaseUrl, setLlmConfigOllamaBaseUrl] = useState("");
  const [llmConfigEmbedModel, setLlmConfigEmbedModel] = useState("nomic-embed-text:latest");
  const [llmConfigEmbedBaseUrl, setLlmConfigEmbedBaseUrl] = useState("");
  const [llmConfigSaving, setLlmConfigSaving] = useState(false);
  // 知识图谱模块状态
  const [graphEmbeddingSim, setGraphEmbeddingSim] = useState<Record<string, number> | undefined>(undefined);
  const [graphData, setGraphData] = useState<KnowledgeGraphData | null>(null);
  const [graphLoading, setGraphLoading] = useState(false);
  const [graphError, setGraphError] = useState("");
  const [graphLocalSubgraphData, setGraphLocalSubgraphData] = useState<KnowledgeGraphData | null>(null);
  const [graphLocalSubgraphLoading, setGraphLocalSubgraphLoading] = useState(false);
  const [graphLocalSubgraphError, setGraphLocalSubgraphError] = useState("");
  const [graphLocalSubgraphTruncated, setGraphLocalSubgraphTruncated] = useState(false);
  const [graphSelectedNodeId, setGraphSelectedNodeId] = useState("");
  const [graphSelectedAggregateId, setGraphSelectedAggregateId] = useState("");
  const [graphViewMode, setGraphViewMode] = useState<GraphViewMode>(() => readGraphViewModeFromStorage());
  const [graphLocalDepth, setGraphLocalDepth] = useState(() => readGraphLocalDepthFromStorage());
  const [graphLocalDirection, setGraphLocalDirection] = useState<GraphTraversalDirection>(
    () => readGraphLocalDirectionFromStorage(),
  );
  const [graphInsightSparseDensity, setGraphInsightSparseDensity] = useState(() =>
    readGraphInsightSparseDensityFromStorage(),
  );
  const [graphInsightBridgeMinGroups, setGraphInsightBridgeMinGroups] = useState(() =>
    readGraphInsightBridgeMinGroupsFromStorage(),
  );
  const [graphInsightSurprisingJaccard, setGraphInsightSurprisingJaccard] = useState(() =>
    readGraphInsightSurprisingJaccardFromStorage(),
  );
  const [graphInsightSurprisingConfidence, setGraphInsightSurprisingConfidence] = useState(() =>
    readGraphInsightSurprisingConfidenceFromStorage(),
  );
  const [graphGroupFilter, setGraphGroupFilter] = useState("__all__");
  const [graphShowOrphans, setGraphShowOrphans] = useState(true);
  const [graphNeighborOnly, setGraphNeighborOnly] = useState(false);
  const [graphLayoutFrozen, setGraphLayoutFrozen] = useState(false);
  const [graphSearchQuery, setGraphSearchQuery] = useState("");
  const [ingestDragActive, setIngestDragActive] = useState(false);
  const [outboxLastId, setOutboxLastId] = useState(0);
  const [outboxInitialized, setOutboxInitialized] = useState(false);
  const [ingesting, setIngesting] = useState(false);

  const graphContainerRef = useRef<HTMLDivElement>(null);
  const graphRef = useRef<any>(null);
  // 图谱搜索框 ref，用于 Ctrl+F 聚焦
  const graphSearchInputRef = useRef<HTMLInputElement>(null);
  const [graphDimensions, setGraphDimensions] = useState({ width: 800, height: 600 });
  // 聚合模式开关（大图时按 group 折叠超节点）
  const [graphAggregateMode, setGraphAggregateMode] = useState(false);
  // 当前激活的导航模块（来自 ModeContext）
  const { activeModule, navigateTo: setActiveModule } = useMode();
  // 摄入队列面板状态
  const [ingestQueue, setIngestQueue] = useState<IngestQueueItem[]>([]);
  const [queueEnqueueing, setQueueEnqueueing] = useState(false);
  const [operationsTab, setOperationsTab] = useState<"queue" | "stats">("queue");
  const [ingestPreviewDialog, setIngestPreviewDialog] = useState<IngestPreview | null>(null);
  const ingestPreviewResolverRef = useRef<((approved: boolean) => void) | null>(null);
  // 统计仪表盘状态
  const [vaultStats, setVaultStats] = useState<VaultStats | null>(null);
  const [vaultStatsLoading, setVaultStatsLoading] = useState(false);
  // Agent Studio（H0）状态
  const [agentRuns, setAgentRuns] = useState<AgentRunItem[]>([]);
  const [agentRunsLoading, setAgentRunsLoading] = useState(false);
  const [agentEvents, setAgentEvents] = useState<AgentRunEventItem[]>([]);
  const [agentEventsLoading, setAgentEventsLoading] = useState(false);
  const [agentDrafts, setAgentDrafts] = useState<AgentDraftItem[]>([]);
  const [agentDraftsLoading, setAgentDraftsLoading] = useState(false);
  const [agentSelectedRunId, setAgentSelectedRunId] = useState<number | null>(null);
  const [agentSelectedDraftId, setAgentSelectedDraftId] = useState<number | null>(null);
  const [agentTopicInput, setAgentTopicInput] = useState("");
  const [agentEventLevel, setAgentEventLevel] = useState<AgentRunEventLevel>("info");
  const [agentEventMessage, setAgentEventMessage] = useState("");
  const [agentCompleteStatus, setAgentCompleteStatus] = useState<AgentRunStatus>("applied");
  const [agentActionRunning, setAgentActionRunning] = useState(false);
  const [agentReviewTab, setAgentReviewTab] = useState<AgentReviewTab>("draft");
  const [agentRightTab, setAgentRightTab] = useState<AgentRightTab>("task");
  const [agentDraftConflictPreview, setAgentDraftConflictPreview] = useState<AgentDraftConflictInfo | null>(null);
  const [agentDraftDiffBaseContent, setAgentDraftDiffBaseContent] = useState("");
  const [agentDraftConflictLoading, setAgentDraftConflictLoading] = useState(false);
  const [agentFlowMode, setAgentFlowMode] = useState<AgentFlowMode>("idle");
  const [agentFlowDraftId, setAgentFlowDraftId] = useState<number | null>(null);
  const [agentFlowCursor, setAgentFlowCursor] = useState(0);
  const [agentFlowChunks, setAgentFlowChunks] = useState<string[]>([]);
  const [agentFlowOutline, setAgentFlowOutline] = useState<string[]>([]);
  const [agentFlowRenderedContent, setAgentFlowRenderedContent] = useState("");
  // 审批确认弹窗状态
  const [agentApproveConfirm, setAgentApproveConfirm] =
    useState<AgentDraftConflictInfo | null>(null);
  // H2 记忆面板状态
  const [agentMemories, setAgentMemories] = useState<AgentMemoryItem[]>([]);
  const [agentMemoriesLoading, setAgentMemoriesLoading] = useState(false);
  const [agentMemoryKeyInput, setAgentMemoryKeyInput] = useState("");
  const [agentMemoryValueInput, setAgentMemoryValueInput] = useState("");
  const [agentMemoryComposerOpen, setAgentMemoryComposerOpen] = useState(false);
  const [agentSkills, setAgentSkills] = useState<AgentSkillItem[]>([]);
  const [agentSkillsLoading, setAgentSkillsLoading] = useState(false);
  const [agentSkillKeyInput, setAgentSkillKeyInput] = useState("");
  const [agentSkillPromptInput, setAgentSkillPromptInput] = useState("");
  const [agentSkillComposerOpen, setAgentSkillComposerOpen] = useState(false);
  const [agentActiveSkillKey, setAgentActiveSkillKey] = useState<string>(
    () => readAgentActiveSkillKeyFromStorage(),
  );
  const [agentResearchMode, setAgentResearchMode] = useState(false);
  const [agentAskFirst, setAgentAskFirst] = useState(false);
  const [agentRewriteComment, setAgentRewriteComment] = useState("");
  const [agentDebugPanelOpen, setAgentDebugPanelOpen] = useState(false);
  const [agentTaskInstruction, setAgentTaskInstruction] = useState("");
  const [agentTaskMaxIterations, setAgentTaskMaxIterations] = useState(4);
  const [agentTaskRunning, setAgentTaskRunning] = useState(false);
  const [agentTaskResult, setAgentTaskResult] = useState("");
  const [agentShellCmd, setAgentShellCmd] = useState<string>("");
  const [agentShellHistory, setAgentShellHistory] = useState<ShellHistoryEntry[]>([]);
  const [agentShellRunning, setAgentShellRunning] = useState<boolean>(false);
  const [agentShellSession, setAgentShellSession] = useState<ShellSessionInfo | null>(null);
  const {
    config: agentShellPolicyConfig,
    saving: agentShellPolicySaving,
    message: shellPolicyStatusMessage,
    clearMessage: clearShellPolicyStatusMessage,
    reload: handleReloadShellPolicy,
    applyAndSaveProfile: handleApplyAndSaveShellPolicyProfile,
  } = useShellPolicy();
  const [agentShellTheme, setAgentShellTheme] = useState<"deep" | "light">("deep");
  const [agentShellHistoryCursor, setAgentShellHistoryCursor] = useState<number>(-1);
  const [agentShellDraftInput, setAgentShellDraftInput] = useState<string>("");
  const [agentToolsSeenCount, setAgentToolsSeenCount] = useState<number>(0);
  const [agentRunStripOpen, setAgentRunStripOpen] = useState<boolean>(false);
  const [agentRunManageMode, setAgentRunManageMode] = useState<boolean>(false);
  const [agentRunMutatingId, setAgentRunMutatingId] = useState<number | null>(null);
  const [agentContextOpen, setAgentContextOpen] = useState<boolean>(false);
  const [agentMemoryPanelOpen, setAgentMemoryPanelOpen] = useState<boolean>(true);
  const [agentSkillPanelOpen, setAgentSkillPanelOpen] = useState<boolean>(false);
  const agentShellIdRef = useRef(0);
  const agentShellSessionIdRef = useRef("");
  const agentShellHistoryRef = useRef<HTMLDivElement>(null);
  const agentShellAutoFollowRef = useRef(true);
  const agentShellInputRef = useRef<HTMLInputElement>(null);

  // ── 面板拖拽分割 ──────────────────────────────────────────────
  const [sidebarWidth, setSidebarWidth] = useState(220);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [agentLeftRatio, setAgentLeftRatio] = useState(0.54);
  const sidebarDragRef = useRef({ active: false, startX: 0, startW: 220 });
  const agentDragRef = useRef({ active: false, startX: 0, startRatio: 0.54, containerW: 0 });
  const agentLayoutRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (sidebarDragRef.current.active) {
        const delta = e.clientX - sidebarDragRef.current.startX;
        setSidebarWidth(Math.max(160, Math.min(400, sidebarDragRef.current.startW + delta)));
      }
      if (agentDragRef.current.active) {
        const delta = e.clientX - agentDragRef.current.startX;
        const newLeft = agentDragRef.current.startRatio * agentDragRef.current.containerW + delta;
        setAgentLeftRatio(Math.max(0.25, Math.min(0.75, newLeft / agentDragRef.current.containerW)));
      }
    };
    const onUp = () => {
      sidebarDragRef.current.active = false;
      agentDragRef.current.active = false;
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

  useEffect(
    () => () => {
      if (askFocusTimerRef.current !== null) {
        globalThis.clearTimeout(askFocusTimerRef.current);
      }
    },
    [],
  );

  const filteredQueryHistoryItems = useMemo(
    () => filterQueryHistoryItems(queryHistoryItems, askHistoryKeyword),
    [queryHistoryItems, askHistoryKeyword],
  );
  const filteredAskSessions = useMemo(
    () => filterAskSessions(askSessions, askSessionKeyword),
    [askSessions, askSessionKeyword],
  );
  const selectedAgentRun = useMemo(
    () => agentRuns.find((run) => run.id === agentSelectedRunId) ?? null,
    [agentRuns, agentSelectedRunId],
  );
  const selectedAgentDraft = useMemo(
    () => agentDrafts.find((draft) => draft.id === agentSelectedDraftId) ?? null,
    [agentDrafts, agentSelectedDraftId],
  );
  const agentChatMessages = useMemo<AgentChatMessage[]>(() => {
    const sortedRuns = [...agentRuns].sort((left, right) => {
      if (left.id !== right.id) {
        return left.id - right.id;
      }
      return String(left.created_at).localeCompare(String(right.created_at));
    });
    const messages: AgentChatMessage[] = [];
    for (const run of sortedRuns) {
      const topic = run.topic?.trim() || `Run #${run.id}`;
      messages.push({
        id: `run-${run.id}-user`,
        run_id: run.id,
        role: "user",
        content: topic,
        created_at: run.created_at,
      });
      let agentContent = `状态：${formatAgentRunStatusLabel(String(run.status))}`;
      if (run.id === agentSelectedRunId) {
        const latestEvent = agentEvents[0];
        if (latestEvent?.message?.trim()) {
          agentContent = latestEvent.message.trim();
        }
        if (agentDrafts.length > 0) {
          const newest = agentDrafts[0];
          agentContent += ` · 草稿 ${agentDrafts.length} 份（最新 #${newest.id}）`;
        }
      }
      messages.push({
        id: `run-${run.id}-agent`,
        run_id: run.id,
        role: "agent",
        content: agentContent,
        created_at: run.updated_at || run.created_at,
        status: String(run.status),
        draft_id: run.id === agentSelectedRunId ? (agentDrafts[0]?.id ?? null) : null,
      });
    }
    return messages;
  }, [agentRuns, agentSelectedRunId, agentEvents, agentDrafts]);
  const agentDraftDiffRows = useMemo(() => {
    if (!selectedAgentDraft || !agentDraftDiffBaseContent.trim()) {
      return [] as WikiLineDiffRow[];
    }
    return buildWikiLineDiff(
      selectedAgentDraft.content ?? "",
      agentDraftDiffBaseContent,
    );
  }, [selectedAgentDraft, agentDraftDiffBaseContent]);
  const agentDraftCitations = useMemo(() => {
    if (!selectedAgentDraft?.content) {
      return [] as string[];
    }
    const matches = selectedAgentDraft.content.match(/\[\[([^\]]+)\]\]/g) ?? [];
    const unique = new Set<string>();
    for (const raw of matches) {
      const normalized = raw.replace(/^\[\[/, "").replace(/\]\]$/, "").trim();
      if (normalized) {
        unique.add(normalized);
      }
    }
    return Array.from(unique);
  }, [selectedAgentDraft]);
  const agentDraftDisplayContent = useMemo(() => {
    if (!selectedAgentDraft) {
      return "";
    }
    if (agentReviewTab !== "draft") {
      return selectedAgentDraft.content ?? "";
    }
    if (agentFlowDraftId === selectedAgentDraft.id && agentFlowRenderedContent.trim()) {
      return agentFlowRenderedContent;
    }
    return selectedAgentDraft.content ?? "";
  }, [selectedAgentDraft, agentReviewTab, agentFlowDraftId, agentFlowRenderedContent]);
  const agentRunCards = useMemo(() => {
    return [...agentRuns].sort((left, right) => {
      const leftTs = Number(left.updated_at || left.created_at || 0);
      const rightTs = Number(right.updated_at || right.created_at || 0);
      if (leftTs !== rightTs) {
        return rightTs - leftTs;
      }
      return right.id - left.id;
    });
  }, [agentRuns]);
  const agentArchivedRunCount = useMemo(
    () => agentRuns.filter((run) => Boolean(run.archived_at)).length,
    [agentRuns],
  );
  const agentVisibleRunCards = useMemo(
    () => agentRunCards.filter((run) => agentRunManageMode || !run.archived_at),
    [agentRunCards, agentRunManageMode],
  );
  const agentFlowProgress = useMemo(() => {
    if (agentFlowChunks.length === 0) {
      return agentFlowMode === "idle" ? 0 : 100;
    }
    return Math.min(100, Math.round((agentFlowCursor / agentFlowChunks.length) * 100));
  }, [agentFlowChunks.length, agentFlowCursor, agentFlowMode]);
  const agentDraftAppliedSkillKey = useMemo(() => {
    if (agentSelectedRunId == null || agentEvents.length === 0) {
      return "";
    }
    for (let index = agentEvents.length - 1; index >= 0; index -= 1) {
      const event = agentEvents[index];
      if (!event || event.run_id !== agentSelectedRunId) {
        continue;
      }
      const skillKey = extractSkillKeyFromEventMessage(event.message ?? "");
      if (skillKey) {
        return skillKey;
      }
    }
    return "";
  }, [agentEvents, agentSelectedRunId]);
  const agentExecTimeline = useMemo<AgentExecTimelineItem[]>(() => {
    if (agentEvents.length === 0) return [];
    const orderedEvents = [...agentEvents].reverse();
    const pendingStarts = new Map<
      string,
      { created_at: string; toolName: string; title: string; detail: string }
    >();
    const timeline: AgentExecTimelineItem[] = [];

    for (const event of orderedEvents) {
      const message = String(event.message ?? "").trim();
      if (!message) continue;
      const startMatched = message.match(/tool_start #(\d+)\s+([a-z_]+):\s*(.*)$/i);
      if (startMatched) {
        const key = startMatched[1];
        const toolName = startMatched[2];
        const detail = startMatched[3] ?? "";
        pendingStarts.set(key, {
          created_at: event.created_at,
          toolName,
          title: `#${key} ${toolName}`,
          detail: detail.trim(),
        });
        continue;
      }

      const endMatched = message.match(/tool_end #(\d+)\s+(.+)$/i);
      if (endMatched) {
        const key = endMatched[1];
        const startInfo = pendingStarts.get(key);
        if (startInfo) pendingStarts.delete(key);
        const startMs = startInfo ? parseEventTimestamp(startInfo.created_at) : null;
        const endMs = parseEventTimestamp(event.created_at);
        const durationMs = startMs != null && endMs != null ? Math.max(0, endMs - startMs) : undefined;
        const endDetail = endMatched[2].trim();
        timeline.push({
          key: `${event.id}-tool-${key}`,
          kind: "tool",
          level: normalizeEventLevel(event.level),
          title: startInfo?.title ?? `#${key} 工具调用`,
          summary: startInfo?.detail ? truncateText(startInfo.detail, 110) : truncateText(endDetail, 110),
          detail: endDetail,
          createdAt: event.created_at,
          durationMs,
        });
        continue;
      }

      const normalizedLevel = normalizeEventLevel(event.level);
      if (normalizedLevel === "awaiting_approval" || message.includes("等待人工确认")) {
        timeline.push({
          key: `${event.id}-approval`,
          kind: "marker",
          level: "awaiting_approval",
          title: "等待审批",
          summary: truncateText(message, 120),
          createdAt: event.created_at,
        });
      }
    }

    for (const [key, startInfo] of pendingStarts) {
      timeline.push({
        key: `pending-${key}-${startInfo.created_at}`,
        kind: "tool",
        level: "info",
        title: startInfo.title,
        summary: truncateText(startInfo.detail || "工具已启动，等待返回", 120),
        detail: "工具尚未返回 tool_end 事件。",
        createdAt: startInfo.created_at,
      });
    }

    return timeline;
  }, [agentEvents]);
  const agentHasPendingApproval = useMemo(() => {
    if (!selectedAgentRun || agentSelectedRunId == null) {
      return false;
    }
    const normalizedStatus = String(selectedAgentRun.status ?? "").trim().toLowerCase();
    if (normalizedStatus === "awaiting_approval") {
      return true;
    }
    let latestAwaitingIndex = -1;
    let latestResolvedIndex = -1;
    // 后端按事件 id 正序返回，使用索引即可稳定判断"最后一个审批事件"的状态。
    for (let i = 0; i < agentEvents.length; i += 1) {
      const event = agentEvents[i];
      if (isAwaitingApprovalMarker(event.level, event.message)) {
        latestAwaitingIndex = i;
      }
      if (isApprovalResolvedMarker(event.message)) {
        latestResolvedIndex = i;
      }
    }
    return latestAwaitingIndex >= 0 && latestAwaitingIndex > latestResolvedIndex;
  }, [selectedAgentRun, agentSelectedRunId, agentEvents]);
  const agentToolsNeedsAttention = useMemo(
    () =>
      (agentRightTab !== "tools")
      && (agentShellRunning || agentShellHistory.length > agentToolsSeenCount),
    [agentRightTab, agentShellRunning, agentShellHistory.length, agentToolsSeenCount],
  );

  useEffect(() => {
    if (agentRightTab === "tools") {
      setAgentToolsSeenCount(agentShellHistory.length);
    }
  }, [agentRightTab, agentShellHistory.length]);

  useEffect(() => {
    if (agentRightTab !== "tools" || !agentShellAutoFollowRef.current) {
      return;
    }
    const container = agentShellHistoryRef.current;
    if (!container) {
      return;
    }
    container.scrollTop = container.scrollHeight;
  }, [agentRightTab, agentShellHistory]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    if (activeModule === "agent" && !agentShellSession) {
      void createShellSession("manual").then((session) => {
        if (disposed || !session) return;
        setAgentShellSession(session);
        agentShellSessionIdRef.current = session.session_id;
      });
    }
    return () => {
      disposed = true;
    };
  }, [activeModule, agentShellSession]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    if ((activeModule !== "agent" && activeModule !== "settings") || agentShellPolicyConfig) return;
    void handleReloadShellPolicy({ silent: true });
  }, [activeModule, agentShellPolicyConfig, handleReloadShellPolicy]);

  useEffect(() => {
    if (!shellPolicyStatusMessage) return;
    setAgentStatusMessage(shellPolicyStatusMessage);
    clearShellPolicyStatusMessage();
  }, [shellPolicyStatusMessage, clearShellPolicyStatusMessage]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void listenShellStreamChunk((payload: ShellStreamChunk) => {
      if (disposed) return;
      setAgentShellHistory((history) =>
        history.map((entry) => {
          if (!entry.stream_id || entry.stream_id !== payload.stream_id) {
            return entry;
          }
          const chunk = payload.chunk ?? "";
          if (payload.stream === "stderr") {
            return {
              ...entry,
              live_stderr: `${entry.live_stderr ?? ""}${chunk}${chunk ? "\n" : ""}`,
              running: payload.done ? false : entry.running,
            };
          }
          if (payload.stream === "stdout") {
            return {
              ...entry,
              live_stdout: `${entry.live_stdout ?? ""}${chunk}${chunk ? "\n" : ""}`,
              running: payload.done ? false : entry.running,
            };
          }
          return {
            ...entry,
            running: payload.done ? false : entry.running,
          };
        }),
      );
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    return () => {
      const sid = agentShellSessionIdRef.current.trim();
      if (!sid || !isTauriRuntime()) return;
      void closeShellSession(sid);
    };
  }, []);

  useEffect(() => {
    agentShellSessionIdRef.current = agentShellSession?.session_id ?? "";
  }, [agentShellSession?.session_id]);

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

  // 应用启动时检查 clip server 状态
  useEffect(() => {
    getClipServerStatus()
      .then((s) => setClipServerOnline(s === "running"))
      .catch(() => setClipServerOnline(false));
  }, []);

  // 切换到 graph 模块时加载图谱数据
  useEffect(() => {
    void refreshGraphData();
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
    writeGraphInsightSparseDensityToStorage(graphInsightSparseDensity);
  }, [graphInsightSparseDensity]);

  useEffect(() => {
    writeGraphInsightBridgeMinGroupsToStorage(graphInsightBridgeMinGroups);
  }, [graphInsightBridgeMinGroups]);

  useEffect(() => {
    writeGraphInsightSurprisingJaccardToStorage(graphInsightSurprisingJaccard);
  }, [graphInsightSurprisingJaccard]);

  useEffect(() => {
    writeGraphInsightSurprisingConfidenceToStorage(graphInsightSurprisingConfidence);
  }, [graphInsightSurprisingConfidence]);

  useEffect(() => {
    writeAskSearchDebugVisibleToStorage(askSearchDebugVisible);
  }, [askSearchDebugVisible]);

  // 图谱 tab 激活时注册 Ctrl+F 快捷键聚焦搜索框
  useEffect(() => {
    if (activeModule !== "graph") return;
    const handleKeyDown = (e: globalThis.KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        graphSearchInputRef.current?.focus();
        graphSearchInputRef.current?.select();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeModule]);

  // 大图聚合模式：将原始图谱按 group 折叠为超节点
  const aggregatedGraphData = useMemo<AggregatedGraphData | null>(() => {
    if (!graphAggregateMode || !graphVisibleData) return null;
    if (graphVisibleData.nodes.length <= GRAPH_AGGREGATE_THRESHOLD) return null;
    // 将 KnowledgeGraphData 的 links 转为 GraphNormalizedEdge 格式
    const normalizedForAgg: GraphNormalizedEdge[] = graphVisibleData.links.map((link) => {
      const srcId = typeof link.source === "string" ? link.source : (link.source as KnowledgeGraphNode).id;
      const tgtId = typeof link.target === "string" ? link.target : (link.target as KnowledgeGraphNode).id;
      return { sourceId: srcId, targetId: tgtId };
    });
    return buildAggregatedGraphData(graphVisibleData.nodes, normalizedForAgg);
  }, [graphAggregateMode, graphVisibleData]);

  const graphRenderData = useMemo<
    | {
        nodes: Array<KnowledgeGraphNode | AggregatedNode>;
        links: Array<KnowledgeGraphLink | AggregatedEdge>;
      }
    | null
  >(() => {
    if (!graphVisibleData) {
      return null;
    }
    if (aggregatedGraphData) {
      return {
        nodes: aggregatedGraphData.nodes,
        links: aggregatedGraphData.links,
      };
    }
    return graphVisibleData;
  }, [aggregatedGraphData, graphVisibleData]);

  const graphSearchableNodes = useMemo<Array<KnowledgeGraphNode | AggregatedNode>>(() => {
    if (!graphRenderData) {
      return [];
    }
    return graphRenderData.nodes;
  }, [graphRenderData]);

  const graphSelectedAggregateNode = useMemo(() => {
    if (!graphSelectedAggregateId || !aggregatedGraphData) {
      return null;
    }
    return (
      aggregatedGraphData.nodes.find(
        (node) => node.isAggregate && node.id === graphSelectedAggregateId,
      ) ?? null
    );
  }, [aggregatedGraphData, graphSelectedAggregateId]);

  const graphSelectedAggregateMembers = useMemo(() => {
    if (!graphSelectedAggregateNode || !graphVisibleData) {
      return [] as KnowledgeGraphNode[];
    }
    return graphVisibleData.nodes
      .filter((node) => (node.group ?? "") === graphSelectedAggregateNode.id)
      .sort((left, right) => left.label.localeCompare(right.label, "zh-CN"));
  }, [graphSelectedAggregateNode, graphVisibleData]);

  useEffect(() => {
    if (!graphAggregateMode) {
      if (graphSelectedAggregateId) {
        setGraphSelectedAggregateId("");
      }
      return;
    }
    if (!graphSelectedAggregateId || !aggregatedGraphData) {
      return;
    }
    const exists = aggregatedGraphData.nodes.some(
      (node) => node.isAggregate && node.id === graphSelectedAggregateId,
    );
    if (!exists) {
      setGraphSelectedAggregateId("");
    }
  }, [aggregatedGraphData, graphAggregateMode, graphSelectedAggregateId]);

  const graphSearchHits = useMemo(() => {
    const query = graphSearchQuery.trim().toLowerCase();
    if (!query) {
      return new Set<string>();
    }
    const hits = new Set<string>();
    for (const node of graphSearchableNodes) {
      if (
        (node.label || "").toLowerCase().includes(query) ||
        (node.id || "").toLowerCase().includes(query)
      ) {
        hits.add(node.id);
      }
    }
    return hits;
  }, [graphSearchQuery, graphSearchableNodes]);

  const graphVisibleNormalizedEdges = useMemo(() => {
    if (!graphVisibleData) {
      return [] as GraphNormalizedEdge[];
    }
    return graphVisibleData.links
      .map((link) => ({
        sourceId: resolveGraphLinkNodeId(link.source as string | KnowledgeGraphNode | null | undefined),
        targetId: resolveGraphLinkNodeId(link.target as string | KnowledgeGraphNode | null | undefined),
      }))
      .filter((edge) => edge.sourceId && edge.targetId);
  }, [graphVisibleData]);

  const graphInsightConfig = useMemo<GraphInsightConfig>(
    () => ({
      ...DEFAULT_GRAPH_INSIGHT_CONFIG,
      sparseDensityThreshold: clampGraphInsightSparseDensity(graphInsightSparseDensity),
      bridgeMinGroups: clampGraphInsightBridgeMinGroups(graphInsightBridgeMinGroups),
      surprisingMaxJaccard: clampGraphInsightSurprisingJaccard(graphInsightSurprisingJaccard),
      surprisingMinConfidence: clampGraphInsightSurprisingConfidence(graphInsightSurprisingConfidence),
    }),
    [
      graphInsightBridgeMinGroups,
      graphInsightSparseDensity,
      graphInsightSurprisingConfidence,
      graphInsightSurprisingJaccard,
    ],
  );

  // 图谱加载后异步拉取 embedding 相似度（静默降级）
  useEffect(() => {
    if (!graphVisibleData) {
      return;
    }
    const paths = graphVisibleData.nodes.map((n) => n.id);
    if (paths.length === 0) {
      return;
    }
    let cancelled = false;
    getPageEmbeddingPairs(paths)
      .then((result) => {
        if (!cancelled) {
          setGraphEmbeddingSim(Object.keys(result).length > 0 ? result : undefined);
        }
      })
      .catch(() => {
        // 静默降级：embedding 不可用时保持词汇距离
      });
    return () => {
      cancelled = true;
    };
  }, [graphVisibleData]);

  const graphInsights = useMemo(() => {
    if (!graphVisibleData) {
      return [] as GraphInsightItem[];
    }
    return buildGraphInsights(graphVisibleData.nodes, graphVisibleNormalizedEdges, 8, graphInsightConfig, graphEmbeddingSim);
  }, [graphEmbeddingSim, graphInsightConfig, graphVisibleData, graphVisibleNormalizedEdges]);

  useEffect(() => {
    if ((graphVisibleData?.nodes.length ?? 0) <= GRAPH_AGGREGATE_THRESHOLD && graphAggregateMode) {
      setGraphAggregateMode(false);
    }
  }, [graphAggregateMode, graphVisibleData?.nodes.length]);

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
  }, [graphLayoutFrozen, graphRenderData?.links.length, graphRenderData?.nodes.length]);

  // 启动时快进 outboxLastId，跳过历史遗留事件，避免旧 ingest_started 使 ingesting 误判为 true。
  useEffect(() => {
    const init = async () => {
      try {
        const events = await get_outbox_events({ last_id: 0 });
        if (events && events.length > 0) {
          const maxId = events.reduce((max, e) => Math.max(max, e.id), 0);
          setOutboxLastId(maxId);
        }
      } catch (err) {
        console.error("初始化 outbox 快进失败:", err);
      }
      setOutboxInitialized(true);
    };
    void init();
  }, []); // 仅在挂载时执行一次

  // Outbox 事件轮询：实现 ingest 完成/Wiki 变更后的 UI 自动刷新与状态同步。
  useEffect(() => {
    if (!outboxInitialized) return; // 等待快进完成后再开始轮询
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
              type === "ingest_failed" ||
              type === "wiki_page_deleted" ||
              type === "wiki_page_renamed" ||
              type === "query_saved_to_wiki"
            ) {
              shouldRefresh = true;
            }

            // 处理 ingest 状态标记
            if (type === "ingest_started") {
              newIngesting = true;
            } else if (type === "ingest_completed" || type === "ingest_failed") {
              newIngesting = false;
            }
          }

          if (shouldRefresh) {
            void refreshAppData({ includeGraph: true });
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
  }, [activeModule, outboxLastId, ingesting, outboxInitialized]);

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
        dbAskSessions,
      ] =
        await Promise.all([
          loadAppData(),
          fetchDefaultPaths(),
          fetchQuerySettings(),
          fetchRecentLintPatchEvents(),
          fetchLlmConfig(),
          fetchOcrConfig(),
          fetchAskHistory(QUERY_HISTORY_MAX),
          listAskSessions(ASK_SESSION_LIST_MAX),
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
          setLlmConfigOllamaModel(llmConfigResult.ollama_model ?? "");
          setLlmConfigOllamaBaseUrl(llmConfigResult.ollama_base_url ?? "");
          setLlmConfigEmbedModel(llmConfigResult.embed_ollama_model || "nomic-embed-text:latest");
          setLlmConfigEmbedBaseUrl(llmConfigResult.embed_ollama_base_url ?? "");
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

        // Ask 会话优先读取后端持久化存储；若为空则创建默认会话。
        const normalizedSessions = dbAskSessions ?? [];
        if (normalizedSessions.length > 0) {
          setAskSessions(normalizedSessions);
          const activeSession =
            normalizedSessions.find((item) => item.session_id === askSessionId) ?? normalizedSessions[0];
          setAskSessionId(activeSession.session_id);
          const turns = await fetchAskSessionTurns(activeSession.session_id, 400);
          if (!cancelled) {
            setAskMessages(buildAskMessagesFromSessionTurns(turns ?? []));
          }
        } else if (isTauriRuntime()) {
          const fallbackSessionId = askSessionId || crypto.randomUUID();
          const created = await createAskSession(fallbackSessionId, "新对话");
          if (!cancelled && created) {
            setAskSessions([created]);
            setAskSessionId(created.session_id);
            setAskMessages([]);
          }
        } else {
          setAskSessions([]);
          setAskMessages([]);
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

  async function refreshGraphData() {
    if (activeModule !== "graph") {
      return;
    }
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
  }

  const refreshAppData = async (options?: { includeGraph?: boolean }) => {
    const data = await loadAppData();
    setOverview(data.overview);
    setLogs(data.logs);
    setPages(data.pages);
    setLlmStatus(data.llmStatus);
    setLlmStatusLoaded(true);
    if (options?.includeGraph) {
      await refreshGraphData();
    }
  };

  const refreshRecentLintPatchEvents = async () => {
    const events = await fetchRecentLintPatchEvents();
    setRecentLintPatchEvents(events);
  };

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
    if (!messageId) {
      return;
    }
    globalThis.setTimeout(() => {
      const node = globalThis.document?.querySelector<HTMLElement>(
        `[data-ask-message-id="${messageId}"]`,
      );
      if (!node) {
        return;
      }
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
    setAskMessages([]); // 先清空，避免切换会话时短暂显示旧消息
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
    if (!isTauriRuntime() || askSessionManaging) {
      return;
    }
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
    if (!isTauriRuntime() || askSessionManaging) {
      return;
    }
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
    if (!isTauriRuntime()) {
      return;
    }
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
    if (!isTauriRuntime() || askSessionManaging || queryRunning) {
      return;
    }
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
    if (!isTauriRuntime() || askSessionManaging) {
      return;
    }
    const nextTitle = globalThis.prompt("请输入新的会话标题", session.title)?.trim();
    if (!nextTitle || nextTitle === session.title) {
      return;
    }
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
    if (!isTauriRuntime() || askSessionManaging) {
      return;
    }
    const confirmed = await askConfirmDialog(`确认删除会话"${session.title}"？该会话消息将被移除。`, {
      title: "删除会话",
      kind: "warning",
      okLabel: "删除",
      cancelLabel: "取消",
    });
    if (!confirmed) {
      return;
    }
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
    if (askSessionManaging) {
      return;
    }
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
        if (!savePath) {
          return;
        }
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
      let result;
      if (selectedTemplate.id === "general") {
        result = await initVault(nextVaultPath);
      } else {
        result = await initVaultWithTemplate(
          nextVaultPath,
          selectedTemplate.schema,
          selectedTemplate.purpose,
          selectedTemplate.extraDirs
        );
      }

      if (!result) {
        setStatusMessage("当前环境不支持 Vault 初始化。");
        return;
      }

      await refreshAppData();
      const sessions = await refreshAskSessionList();
      if (sessions.length === 0 && isTauriRuntime()) {
        const fallbackSessionId = crypto.randomUUID();
        const created = await createAskSession(fallbackSessionId, "新对话");
        if (created) {
          setAskSessions([created]);
          setAskSessionId(created.session_id);
          setAskMessages([]);
        }
      } else if (sessions.length > 0) {
        const activeSession = sessions[0];
        setAskSessionId(activeSession.session_id);
        await loadAskSessionMessages(activeSession.session_id);
      }
      const mergedRecent = mergeRecentVaultPaths(result.vault_path || nextVaultPath, recentVaultPaths);
      setRecentVaultPaths(mergedRecent);
      const createdCount = result.created_paths?.length ?? 0;
      setStatusMessage(
        `${result.message || `Vault 已初始化：${result.vault_path}`}\n模板：${selectedTemplate.name} · 创建 ${createdCount} 项`,
      );
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

      setStatusMessage("分析中，等待审核确认...");
      const preview = await previewIngestFile("markdown", nextSourcePath);
      if (!preview) {
        setStatusMessage("当前环境不支持摄入预览。");
        return;
      }
      const approved = await requestIngestPreviewApproval(preview);
      if (!approved) {
        setStatusMessage("已取消摄入。");
        return;
      }
      setStatusMessage("写入中...");
      const result = await applyIngestPreview(preview.preview_id);
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
      setIngesting(false);
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

      setStatusMessage("分析中，等待审核确认...");
      const preview = await previewIngestFile("url", trimmedUrl);
      if (!preview) {
        setStatusMessage("当前环境不支持摄入预览。");
        return;
      }
      const approved = await requestIngestPreviewApproval(preview);
      if (!approved) {
        setStatusMessage("已取消摄入。");
        return;
      }
      setStatusMessage("写入中...");
      const result = await applyIngestPreview(preview.preview_id);
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
      setIngesting(false);
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

      setStatusMessage("分析中，等待审核确认...");
      const preview = await previewIngestFile("pdf", trimmedPath);
      if (!preview) {
        setStatusMessage("当前环境不支持摄入预览。");
        return;
      }
      const approved = await requestIngestPreviewApproval(preview);
      if (!approved) {
        setStatusMessage("已取消摄入。");
        return;
      }
      setStatusMessage("写入中...");
      const result = await applyIngestPreview(preview.preview_id);
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
      const normalizedPdfError = pdfErrorMessage.toLowerCase();
      // PDF 摄入一般不依赖 OCR，但若后端返回"未检测到"类错误也给出友好提示
      if (
        pdfErrorMessage.includes("未检测到")
        && !normalizedPdfError.includes("pdftoppm")
        && !normalizedPdfError.includes("poppler")
      ) {
        setStatusMessage(`OCR 工具未找到：${pdfErrorMessage}`);
        return;
      }
      setStatusMessage(formatPdfIngestErrorMessage(error));
    } finally {
      if (unlisten) {
        unlisten();
      }
      setIngesting(false);
      setDevAction(null);
    }
  };

  const requestIngestPreviewApproval = useCallback((preview: IngestPreview) => {
    return new Promise<boolean>((resolve) => {
      ingestPreviewResolverRef.current = resolve;
      setIngestPreviewDialog(preview);
    });
  }, []);

  const closeIngestPreviewDialog = useCallback((approved: boolean) => {
    const resolver = ingestPreviewResolverRef.current;
    ingestPreviewResolverRef.current = null;
    setIngestPreviewDialog(null);
    if (resolver) {
      resolver(approved);
    }
  }, []);

  const runIngestFilePaths = async (
    pathsToIngest: string[],
    sourceLabel: "manual" | "drag",
    previewBeforeApply = false,
  ) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行通用文件摄入。");
      return;
    }
    if (pathsToIngest.length === 0) {
      setStatusMessage("请选择或输入要摄入的文件路径（支持 md/pdf/docx/pptx/txt/图片）。");
      return;
    }

    setDevAction("ingest_file");
    setStatusMessage(sourceLabel === "drag" ? "检测到拖拽文件，摄入中..." : "摄入中...");
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
        const fileName = filePath.split(/[/\\]/).pop() ?? filePath;
        let result: Awaited<ReturnType<typeof ingestFile>>;
        if (previewBeforeApply) {
          setStatusMessage(`分析中 (${successCount + 1}/${pathsToIngest.length})：${fileName}`);
          const preview = await previewIngestFile("file", filePath, ingestFileOcrProvider);
          if (!preview) {
            setStatusMessage("当前环境不支持摄入预览。");
            return;
          }
          const approved = await requestIngestPreviewApproval(preview);
          if (!approved) {
            setStatusMessage("已取消摄入。");
            return;
          }
          setStatusMessage(`写入中 (${successCount + 1}/${pathsToIngest.length})：${fileName}`);
          result = await applyIngestPreview(preview.preview_id);
        } else {
          setStatusMessage(`摄入中 (${successCount + 1}/${pathsToIngest.length})：${fileName}`);
          result = await ingestFile(filePath, ingestFileOcrProvider);
        }

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
      const singleFilePath = pathsToIngest.length === 1 ? pathsToIngest[0] : "";
      const isSinglePdf = singleFilePath.toLowerCase().endsWith(".pdf");
      if (isSinglePdf && message.toLowerCase().includes("pdf")) {
        setStatusMessage(formatPdfIngestErrorMessage(message));
        return;
      }
      const normalizedMessage = message.toLowerCase();
      const isTesseractLanguageMissing =
        normalizedMessage.includes("缺少可用语言包")
        || normalizedMessage.includes("chi_sim")
        || normalizedMessage.includes("traineddata")
        || normalizedMessage.includes("failed loading language");
      if (isTesseractLanguageMissing) {
        setStatusMessage(
          "Tesseract 已安装，但缺少语言包（chi_sim/eng）。请安装语言包，或在 OCR 下拉切换到 PaddleOCR。",
        );
        return;
      }
      // 仅按当前选中的 provider 判断"命令缺失"，避免被 fallback provider 的缺失误判。
      const isPrimaryProviderMissing =
        ingestFileOcrProvider === "paddle"
          ? (
            message.includes("未检测到 paddleocr 命令")
            || (normalizedMessage.includes("is not recognized") && normalizedMessage.includes("paddleocr"))
          )
          : (
            message.includes("未检测到 tesseract 命令")
            || (normalizedMessage.includes("is not recognized") && normalizedMessage.includes("tesseract"))
          );
      if (isPrimaryProviderMissing) {
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
      if (ingestPreviewResolverRef.current) {
        ingestPreviewResolverRef.current(false);
        ingestPreviewResolverRef.current = null;
      }
      setIngestPreviewDialog(null);
      setIngesting(false);
      setDevAction(null);
    }
  };

  const handleFileIngest = async () => {
    setStatusMessage("收到通用文件摄入请求，正在调用后端...");
    const pathsToIngest = ingestFilePickedPaths.length > 0
      ? ingestFilePickedPaths
      : [ingestFilePath.trim()].filter(Boolean);
    await runIngestFilePaths(pathsToIngest, "manual", true);
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

  const copyTextToClipboard = async (text: string): Promise<boolean> => {
    try {
      if (globalThis.navigator?.clipboard?.writeText) {
        await globalThis.navigator.clipboard.writeText(text);
        return true;
      }
    } catch {
      // 忽略后继续走降级路径
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

  const handleCopySearchDebug = async (
    messageId: string,
    searchDebug: import("./types").QuerySearchDebug,
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
      // 进度订阅失败不应阻塞查询执行，避免按钮持续处于"执行中"。
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
      void saveAskHistory(nextQuestion); // 异步保存到 DB，不阻断
      // Query 会在后端写入日志，这里主动刷新一次前端日志面板。
      await refreshAppData();
      await refreshAskSessionList();
      setQueryQuestion("");
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
      const nextConfig: LlmProviderConfig = {
        active_provider: providerDecision.activeProvider,
        cloud_api_key: llmConfigCloudApiKey.trim(),
        cloud_base_url: llmConfigCloudBaseUrl.trim(),
        cloud_model: llmConfigCloudModel.trim(),
        cloud_provider_name: llmConfigCloudProviderName.trim(),
        ollama_model: llmConfigOllamaModel.trim(),
        ollama_base_url: llmConfigOllamaBaseUrl.trim(),
        embed_ollama_model: llmConfigEmbedModel.trim(),
        embed_ollama_base_url: llmConfigEmbedBaseUrl.trim(),
      };

      const result = await saveLlmConfig(nextConfig);
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

  const handleCreatePageWithAi = async () => {
    const topic = newPageTopic.trim();
    if (!topic || newPageCreating) return;
    setNewPageCreating(true);
    setNewPageResult(null);
    try {
      const result = await createWikiPageWithAi(topic);
      setNewPageResult(result);
      setStatusMessage(`已创建页面「${result.title}」`);
      // 刷新 Wiki 列表：清空关键词并重新加载
      setWikiKeyword(result.title);
      void handleSearchWikiPages();
    } catch (err) {
      setStatusMessage(`创建失败：${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setNewPageCreating(false);
    }
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
    if (graphNode?.isAggregate) {
      setGraphSelectedNodeId("");
      setGraphSelectedAggregateId(graphNode.id);
      setGraphNeighborOnly(false);
      setStatusMessage(`已选中聚合节点「${graphNode.label || graphNode.id}」，可在右侧展开成员页。`);
      return;
    }
    const pagePath = resolveGraphNodePagePath(graphNode);
    if (!pagePath) {
      setStatusMessage("图谱节点数据异常，无法选中。");
      return;
    }
    setGraphSelectedNodeId(pagePath);
    setGraphSelectedAggregateId("");

    // 自动聚焦到点击的节点
    if (graphRef.current && typeof graphNode.x === "number" && typeof graphNode.y === "number") {
      graphRef.current.centerAt(graphNode.x, graphNode.y, 400);
      graphRef.current.zoom(2.0, 400);
    }
  };

  // 导出当前可见图谱数据为 JSON 文件
  const handleExportGraphJson = () => {
    if (!graphRenderData) return;
    const normalizedPayload = {
      exported_at: new Date().toISOString(),
      view_mode: graphViewMode,
      aggregate_mode: Boolean(aggregatedGraphData),
      nodes: graphRenderData.nodes.map((node) => {
        const n = node as KnowledgeGraphNode & AggregatedNode;
        return {
          id: n.id,
          label: n.label,
          group: n.group ?? "",
          is_aggregate: Boolean(n.isAggregate),
          count: n.isAggregate ? (n.count ?? 1) : 1,
        };
      }),
      links: graphRenderData.links
        .map((link) => {
          const edge = link as KnowledgeGraphLink & AggregatedEdge;
          const sourceId = resolveGraphLinkNodeId(edge.source as string | KnowledgeGraphNode | null | undefined);
          const targetId = resolveGraphLinkNodeId(edge.target as string | KnowledgeGraphNode | null | undefined);
          if (!sourceId || !targetId) {
            return null;
          }
          return {
            source: sourceId,
            target: targetId,
            weight: edge.weight ?? 1,
          };
        })
        .filter((edge): edge is { source: string; target: string; weight: number } => Boolean(edge)),
    };
    const json = JSON.stringify(normalizedPayload, null, 2);
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `llm-wiki-graph-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleOpenSelectedGraphNode = async () => {
    if (!graphSelectedNode) {
      return;
    }
    setActiveModule("wiki");
    await handleOpenWikiPage(graphSelectedNode.id);
  };

  const handleOpenAggregateMemberPage = async (memberPath: string) => {
    if (!memberPath.trim()) {
      return;
    }
    setGraphAggregateMode(false);
    setGraphSelectedAggregateId("");
    setGraphSelectedNodeId(memberPath);
    setActiveModule("wiki");
    await handleOpenWikiPage(memberPath);
  };

  const handleExpandSelectedAggregateNode = () => {
    if (!graphSelectedAggregateNode) {
      return;
    }
    const targetGroup = graphSelectedAggregateNode.id;
    const nextSelected = graphSelectedAggregateMembers[0]?.id ?? "";
    setGraphAggregateMode(false);
    setGraphGroupFilter(targetGroup);
    setGraphNeighborOnly(false);
    setGraphSelectedAggregateId("");
    if (nextSelected) {
      setGraphSelectedNodeId(nextSelected);
      setStatusMessage(`已切回明细模式，分组「${targetGroup}」包含 ${graphSelectedAggregateMembers.length} 个页面。`);
    } else {
      setStatusMessage(`已切回明细模式，但分组「${targetGroup}」暂无可展示页面。`);
    }
  };

  const handleExitAggregateMode = () => {
    setGraphAggregateMode(false);
    setGraphSelectedAggregateId("");
    setStatusMessage("已切回明细模式。");
  };

  const handleApplyGraphInsight = (insight: GraphInsightItem) => {
    if (insight.group) {
      setGraphGroupFilter(insight.group);
    }
    setGraphAggregateMode(false);
    setGraphSelectedAggregateId("");
    setGraphSearchQuery("");
    if (insight.nodeIds.length === 0) {
      setStatusMessage(`已定位洞察：${insight.title}`);
      return;
    }

    const targetNodeId = insight.nodeIds[0];
    setGraphSelectedNodeId(targetNodeId);
    setGraphNeighborOnly(insight.kind === "bridge-node" || insight.kind === "surprising-link");
    if (insight.kind === "surprising-link" && insight.nodeIds.length > 1) {
      const pairedNodeId = insight.nodeIds[1];
      const pairedLabel = graphNodeById.get(pairedNodeId)?.label || resolveGraphNodeLeafName(pairedNodeId);
      setStatusMessage(`已定位洞察：${insight.title}（配对节点：${pairedLabel}）`);
      return;
    }
    setStatusMessage(`已定位洞察：${insight.title}`);
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

  const handleGraphInsightSparseDensityChange = (value: number) => {
    setGraphInsightSparseDensity(clampGraphInsightSparseDensity(value));
  };

  const handleGraphInsightBridgeMinGroupsChange = (value: number) => {
    setGraphInsightBridgeMinGroups(clampGraphInsightBridgeMinGroups(value));
  };

  const handleGraphInsightSurprisingJaccardChange = (value: number) => {
    setGraphInsightSurprisingJaccard(clampGraphInsightSurprisingJaccard(value));
  };

  const handleGraphInsightSurprisingConfidenceChange = (value: number) => {
    setGraphInsightSurprisingConfidence(clampGraphInsightSurprisingConfidence(value));
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
    setWikiHistoryOpen(false);
    setWikiHistoryEntries([]);
    setWikiHistorySelectedEntry(null);
    setWikiHistoryError("");
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
    // 保存编辑基线校验和，用于保存时检测并发编辑冲突
    setWikiEditBaselineChecksum(simpleHash(wikiPageDetail.content ?? ""));
    setWikiEditMode(true);
  };

  const handleCancelWikiEdit = () => {
    setWikiEditMode(false);
    setWikiSaveError("");
    setWikiEditContent(wikiPageDetail?.content ?? "");
  };

  const updateWikiAutocompletePosition = (cursor: number, contentOverride?: string) => {
    if (!wikiEditorRef.current) {
      return;
    }
    const nextPos = measureTextareaCaretPosition(
      wikiEditorRef.current,
      cursor,
      contentOverride,
    );
    setWikiAutocompletePos(nextPos);
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

    // 延迟聚焦回编辑器并设置光标
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
        if (wikiAutocompleteRequestIdRef.current !== currentRequestId) {
          return;
        }
        setWikiAutocompleteResults(results.slice(0, 10));
      } catch (e) {
        console.warn("自动补全搜索失败", e);
        if (wikiAutocompleteRequestIdRef.current !== currentRequestId) {
          return;
        }
        setWikiAutocompleteResults([]);
      }
    } else {
      wikiAutocompleteRequestIdRef.current += 1;
      setWikiAutocompleteOpen(false);
      setWikiAutocompleteResults([]);
    }
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
      // 传入编辑基线校验和，由后端检测并发编辑冲突
      const result = await saveWikiPage(targetPath, wikiEditContent, wikiEditBaselineChecksum || undefined);
      if (!result) {
        setWikiSaveError("当前环境不支持保存页面。请检查 Tauri 后端是否可用。");
        return;
      }

      setWikiEditMode(false);
      setWikiEditBaselineChecksum("");
      await refreshAppData();
      await handleOpenWikiPage(targetPath);
      setStatusMessage(result.message || `已保存页面：${targetPath}`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      // 检测校验和不匹配（并发编辑冲突）
      if (message.includes("checksum") || message.includes("校验和")) {
        setWikiSaveError(`保存失败：页面在编辑期间被外部修改，已阻止覆盖。请重新打开页面后再试。`);
        return;
      }
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

  const handleOpenWikiHistory = async () => {
    if (!wikiPageDetail) {
      return;
    }

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

  // 从历史版本恢复到当前页面
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
      // 重新加载页面内容
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
  const wikiHistoryDiffRows = useMemo(
    () => buildWikiLineDiff(wikiPageDetail?.content ?? "", wikiHistorySelectedEntry?.content ?? ""),
    [wikiPageDetail?.content, wikiHistorySelectedEntry?.content],
  );
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
            onChange={(event) => handleWikiEditorChange(event.target.value)}
            onKeyDown={(event: ReactKeyboardEvent<HTMLTextAreaElement>) => {
              // 自动补全键盘导航
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
                    setWikiAutocompleteIndex((i) => (i - 1 + wikiAutocompleteResults.length) % wikiAutocompleteResults.length);
                    return;
                  }
                  if (event.key === "Enter" || event.key === "Tab") {
                    event.preventDefault();
                    handleWikiAutocompleteSelect(wikiAutocompleteResults[wikiAutocompleteIndex]);
                    return;
                  }
                }
              }

              // Ctrl+S（Windows/Linux）或 Cmd+S（macOS）触发保存
              if ((event.ctrlKey || event.metaKey) && event.key === "s") {
                event.preventDefault();
                if (!wikiSaveRunning) {
                  void handleSaveWikiPage();
                }
              }
            }}
            onClick={(event) => {
              if (!wikiAutocompleteOpen) {
                return;
              }
              updateWikiAutocompletePosition(
                event.currentTarget.selectionStart,
                event.currentTarget.value,
              );
            }}
            onKeyUp={(event) => {
              if (!wikiAutocompleteOpen) {
                return;
              }
              updateWikiAutocompletePosition(
                event.currentTarget.selectionStart,
                event.currentTarget.value,
              );
            }}
            onScroll={() => {
              if (!wikiAutocompleteOpen || !wikiEditorRef.current) {
                return;
              }
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

  useEffect(() => {
    if (!isTauriRuntime()) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | null = null;
    void import("@tauri-apps/api/webview")
      .then(async ({ getCurrentWebview }) => {
        if (disposed) {
          return;
        }
        unlisten = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setIngestDragActive(true);
            return;
          }
          if (event.payload.type === "leave") {
            setIngestDragActive(false);
            return;
          }
          setIngestDragActive(false);
          if (event.payload.type !== "drop") {
            return;
          }
          if (devAction === "ingest_file" || ingesting) {
            setStatusMessage("当前正在摄入中，已忽略本次拖拽文件。");
            return;
          }
          const parsed = parseDroppedIngestPaths(event.payload.paths);
          if (parsed.accepted.length === 0) {
            setStatusMessage("未检测到可摄入文件（支持 md/pdf/docx/pptx/txt/图片）。");
            return;
          }

          const ignoredMsg = parsed.rejected.length > 0 ? `，忽略 ${parsed.rejected.length} 项` : "";
          const duplicateMsg = parsed.duplicateCount > 0 ? `，去重 ${parsed.duplicateCount} 项` : "";

          if (dropMode === "queue") {
            setStatusMessage(`已接收拖拽文件 ${parsed.accepted.length} 项${ignoredMsg}${duplicateMsg}，加入队列...`);
            Promise.all(parsed.accepted.map((p) => enqueueIngest("file", p)))
              .then(() => {
                openOperationsModule("queue");
              })
              .catch((err: unknown) => {
                console.error("拖拽入队失败:", err);
                setStatusMessage("拖拽入队失败，请检查后端日志。");
              });
          } else {
            setActiveModule("inbox");
            setIngestFilePickedPaths(parsed.accepted);
            setIngestFilePath(parsed.accepted[0] ?? "");
            setStatusMessage(`已接收拖拽文件 ${parsed.accepted.length} 项${ignoredMsg}${duplicateMsg}，开始摄入...`);
            void runIngestFilePaths(parsed.accepted, "drag", true);
          }
        });
      })
      .catch((error) => {
        console.warn("注册拖拽摄入监听失败。", error);
      });

    return () => {
      disposed = true;
      if (unlisten) {
        unlisten();
      }
    };
  }, [devAction, dropMode, ingestFileOcrProvider, ingesting]);

  useEffect(() => {
    return () => {
      if (ingestPreviewResolverRef.current) {
        ingestPreviewResolverRef.current(false);
        ingestPreviewResolverRef.current = null;
      }
    };
  }, []);

  const loadVaultStats = async () => {
    if (!isTauriRuntime()) return;
    setVaultStatsLoading(true);
    try {
      const stats = await getVaultStats();
      setVaultStats(stats);
    } catch {
      setVaultStats(null);
    } finally {
      setVaultStatsLoading(false);
    }
  };

  const loadAgentRunsData = async (
    preferredRunId?: number | null,
    includeArchivedOverride?: boolean,
  ) => {
    if (!isTauriRuntime()) return;
    setAgentRunsLoading(true);
    try {
      const runs = await listAgentRuns(
        AGENT_RUN_LIST_LIMIT,
        includeArchivedOverride ?? agentRunManageMode,
      );
      setAgentRuns(runs);
      if (runs.length === 0) {
        setAgentSelectedRunId(null);
        setAgentEvents([]);
        setAgentDrafts([]);
        setAgentSelectedDraftId(null);
        return;
      }
      const pinnedRunId = preferredRunId ?? agentSelectedRunId;
      const nextRunId =
        pinnedRunId != null && runs.some((run) => run.id === pinnedRunId)
          ? pinnedRunId
          : runs[0].id;
      setAgentSelectedRunId(nextRunId);
    } finally {
      setAgentRunsLoading(false);
    }
  };

  const loadAgentRunEventsData = async (runId: number) => {
    if (!isTauriRuntime()) return;
    setAgentEventsLoading(true);
    try {
      const events = await listAgentRunEvents(runId, AGENT_RUN_EVENT_LIST_LIMIT);
      setAgentEvents(events);
    } finally {
      setAgentEventsLoading(false);
    }
  };

  const loadAgentDraftsData = async (runId: number, preferredDraftId?: number | null) => {
    if (!isTauriRuntime()) return;
    setAgentDraftsLoading(true);
    try {
      const drafts = await listAgentDrafts(runId, AGENT_DRAFT_LIST_LIMIT);
      setAgentDrafts(drafts);
      if (drafts.length === 0) {
        setAgentSelectedDraftId(null);
        return;
      }
      const pinnedDraftId = preferredDraftId ?? agentSelectedDraftId;
      const nextDraftId =
        pinnedDraftId != null && drafts.some((draft) => draft.id === pinnedDraftId)
          ? pinnedDraftId
          : drafts[0].id;
      setAgentSelectedDraftId(nextDraftId);
    } finally {
      setAgentDraftsLoading(false);
    }
  };

  const loadAgentMemoriesData = async (runId: number | null) => {
    if (!isTauriRuntime()) return;
    setAgentMemoriesLoading(true);
    try {
      const mems = await listAgentMemories(runId);
      setAgentMemories(mems);
    } finally {
      setAgentMemoriesLoading(false);
    }
  };

  const loadAgentSkillsData = async () => {
    if (!isTauriRuntime()) return;
    setAgentSkillsLoading(true);
    try {
      const skills = await listAgentSkills(AGENT_SKILL_LIST_LIMIT);
      setAgentSkills(skills);
      if (skills.length === 0) {
        setAgentActiveSkillKey("");
        return;
      }
      setAgentActiveSkillKey((prev) => {
        if (prev === "") {
          return "";
        }
        if (skills.some((item) => item.skill_key === prev)) {
          return prev;
        }
        return "";
      });
    } finally {
      setAgentSkillsLoading(false);
    }
  };

  const emitAgentFlowEvent = async (message: string) => {
    if (agentSelectedRunId == null || !isTauriRuntime()) {
      return;
    }
    try {
      const ok = await appendAgentRunEvent(agentSelectedRunId, "info", message);
      if (!ok) {
        return;
      }
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } catch {
      // 事件写入失败不阻塞主流程。
    }
  };

  useEffect(() => {
    if (activeModule !== "agent") {
      return;
    }
    void loadAgentRunsData();
    void loadAgentSkillsData();
    // H0 阶段仅在切到 Agent Studio 时刷新，不做后台轮询。
  }, [activeModule]);

  useEffect(() => {
    if (activeModule !== "agent") {
      return;
    }
    void loadAgentRunsData(agentSelectedRunId, agentRunManageMode);
  }, [activeModule, agentRunManageMode]);

  useEffect(() => {
    writeAgentActiveSkillKeyToStorage(agentActiveSkillKey);
  }, [agentActiveSkillKey]);

  useEffect(() => {
    if (activeModule !== "agent") {
      return;
    }
    if (agentSelectedRunId == null) {
      setAgentEvents([]);
      setAgentDrafts([]);
      setAgentSelectedDraftId(null);
      return;
    }
    void loadAgentRunEventsData(agentSelectedRunId);
    void loadAgentDraftsData(agentSelectedRunId);
    void loadAgentMemoriesData(agentSelectedRunId);
    // H0 阶段仅依赖当前 run，避免与其他模块状态耦合。
  }, [activeModule, agentSelectedRunId]);

  useEffect(() => {
    if (activeModule !== "agent" || selectedAgentDraft == null || !isTauriRuntime()) {
      setAgentDraftConflictPreview(null);
      setAgentDraftDiffBaseContent("");
      setAgentDraftConflictLoading(false);
      return;
    }
    let canceled = false;
    setAgentDraftConflictLoading(true);
    void checkAgentDraftConflict(selectedAgentDraft.id)
      .then(async (info) => {
        if (!canceled) {
          setAgentDraftConflictPreview(info);
        }
        const fallbackPreview = info?.existing_preview?.trim() ?? "";
        if (!info?.conflict || !info?.existing_path?.trim()) {
          if (!canceled) {
            setAgentDraftDiffBaseContent(fallbackPreview);
          }
          return;
        }
        try {
          const detail = await fetchWikiPageDetail(info.existing_path.trim());
          if (!canceled) {
            setAgentDraftDiffBaseContent(detail?.content ?? fallbackPreview);
          }
        } catch {
          if (!canceled) {
            setAgentDraftDiffBaseContent(fallbackPreview);
          }
        }
      })
      .catch(() => {
        if (!canceled) {
          setAgentDraftConflictPreview(null);
          setAgentDraftDiffBaseContent("");
        }
      })
      .finally(() => {
        if (!canceled) {
          setAgentDraftConflictLoading(false);
        }
      });
    return () => {
      canceled = true;
    };
  }, [activeModule, selectedAgentDraft]);

  useEffect(() => {
    if (activeModule === "agent") {
      setAgentReviewTab("draft");
    }
  }, [activeModule, selectedAgentDraft?.id]);

  useEffect(() => {
    if (activeModule !== "agent" || selectedAgentDraft == null) {
      setAgentFlowMode("idle");
      setAgentFlowDraftId(null);
      setAgentFlowCursor(0);
      setAgentFlowChunks([]);
      setAgentFlowOutline([]);
      setAgentFlowRenderedContent("");
      return;
    }
    const model = buildAgentDraftFlowModel(selectedAgentDraft.content ?? "");
    setAgentFlowDraftId(selectedAgentDraft.id);
    setAgentFlowChunks(model.chunks);
    setAgentFlowOutline(model.outline);
    setAgentFlowCursor(0);
    setAgentFlowRenderedContent(model.prefix);
    setAgentFlowMode(model.chunks.length > 0 ? "playing" : "done");
  }, [activeModule, selectedAgentDraft?.id, selectedAgentDraft?.content]);

  useEffect(() => {
    if (
      activeModule !== "agent"
      || agentReviewTab !== "draft"
      || agentFlowMode !== "playing"
      || selectedAgentDraft == null
      || agentFlowDraftId !== selectedAgentDraft.id
    ) {
      return;
    }
    if (agentFlowCursor >= agentFlowChunks.length) {
      setAgentFlowMode("done");
      void emitAgentFlowEvent("草稿流式渲染已完成");
      return;
    }
    const nextChunk = agentFlowChunks[agentFlowCursor] ?? "";
    const delayMs = nextChunk.length > 220 ? 75 : nextChunk.length > 120 ? 56 : 40;
    const timer = globalThis.setTimeout(() => {
      setAgentFlowRenderedContent((prev) => `${prev}${nextChunk}`);
      setAgentFlowCursor((prev) => prev + 1);
    }, delayMs);
    return () => globalThis.clearTimeout(timer);
  }, [
    activeModule,
    agentReviewTab,
    agentFlowMode,
    selectedAgentDraft,
    agentFlowDraftId,
    agentFlowCursor,
    agentFlowChunks,
  ]);

  useEffect(() => {
    if (agentReviewTab !== "draft" && agentFlowMode === "playing") {
      setAgentFlowMode("paused");
    }
  }, [agentReviewTab, agentFlowMode]);

  const handleSelectAgentRunFromChat = (runId: number) => {
    setAgentSelectedRunId(runId);
  };

  const handleArchiveAgentRun = async (runId: number) => {
    if (!isTauriRuntime() || agentRunMutatingId != null) {
      return;
    }
    const ok = await askConfirmDialog(
      `确认归档 run #${runId} 吗？归档后默认列表将隐藏该 run，可在管理模式恢复。`,
      {
        title: "归档历史 Run",
        kind: "warning",
        okLabel: "归档",
        cancelLabel: "取消",
      },
    );
    if (!ok) {
      return;
    }
    setAgentRunMutatingId(runId);
    try {
      const archived = await archiveAgentRun(runId);
      if (!archived) {
        setAgentStatusMessage(`归档 run #${runId} 失败，请检查后端日志。`);
        return;
      }
      setAgentStatusMessage(`已归档 run #${runId}。`);
      await loadAgentRunsData(agentSelectedRunId, agentRunManageMode);
      if (agentSelectedRunId != null) {
        await loadAgentRunEventsData(agentSelectedRunId);
      }
    } finally {
      setAgentRunMutatingId(null);
    }
  };

  const handleRestoreAgentRun = async (runId: number) => {
    if (!isTauriRuntime() || agentRunMutatingId != null) {
      return;
    }
    setAgentRunMutatingId(runId);
    try {
      const restored = await restoreAgentRun(runId);
      if (!restored) {
        setAgentStatusMessage(`恢复 run #${runId} 失败，请检查后端日志。`);
        return;
      }
      setAgentStatusMessage(`已恢复 run #${runId}。`);
      await loadAgentRunsData(runId, agentRunManageMode);
      await loadAgentRunEventsData(runId);
    } finally {
      setAgentRunMutatingId(null);
    }
  };

  const handleReplayAgentFlow = () => {
    if (!selectedAgentDraft) {
      return;
    }
    const model = buildAgentDraftFlowModel(selectedAgentDraft.content ?? "");
    setAgentFlowDraftId(selectedAgentDraft.id);
    setAgentFlowChunks(model.chunks);
    setAgentFlowOutline(model.outline);
    setAgentFlowCursor(0);
    setAgentFlowRenderedContent(model.prefix);
    setAgentFlowMode(model.chunks.length > 0 ? "playing" : "done");
  };

  const handlePauseAgentFlow = () => {
    if (agentFlowMode === "playing") {
      setAgentFlowMode("paused");
      setAgentStatusMessage("已暂停草稿流式渲染。");
      void emitAgentFlowEvent("草稿流式渲染已暂停");
    }
  };

  const handleResumeAgentFlow = () => {
    if (selectedAgentDraft == null) {
      return;
    }
    if (agentFlowCursor >= agentFlowChunks.length) {
      setAgentFlowMode("done");
      return;
    }
    setAgentFlowMode("playing");
    setAgentStatusMessage("已继续草稿流式渲染。");
    void emitAgentFlowEvent("草稿流式渲染已继续");
  };

  const handleCompleteAgentFlow = () => {
    if (selectedAgentDraft == null) {
      return;
    }
    const model = buildAgentDraftFlowModel(selectedAgentDraft.content ?? "");
    setAgentFlowDraftId(selectedAgentDraft.id);
    setAgentFlowChunks(model.chunks);
    setAgentFlowOutline(model.outline);
    setAgentFlowCursor(model.chunks.length);
    setAgentFlowRenderedContent(`${model.prefix}${model.chunks.join("")}`);
    setAgentFlowMode("done");
    void emitAgentFlowEvent("草稿流式渲染已直接完成");
  };

  const handleAgentChatSend = async () => {
    const topic = agentTopicInput.trim();
    if (!topic) {
      setAgentStatusMessage("请输入主题后再发送。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const runId = await startAgentRun(topic);
      if (!runId) {
        setAgentStatusMessage("创建 run 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentTopicInput("");
      setAgentStatusMessage(`run #${runId} 已创建，正在生成 draft...`);
      await loadAgentRunsData(runId);
      await loadAgentRunEventsData(runId);
      const ok = await generateAgentDraft(runId, topic, agentActiveSkillKey || null, agentResearchMode, agentAskFirst);
      if (!ok) {
        setAgentStatusMessage(`run #${runId} draft 生成失败，请检查后端日志。`);
        return;
      }
      await loadAgentDraftsData(runId);
      await loadAgentRunEventsData(runId);
      await loadAgentRunsData(runId);
      setAgentStatusMessage(
        `run #${runId} draft 已就绪${agentActiveSkillKey ? `（skill: ${agentActiveSkillKey}）` : ""}，可在右侧预览并审批。`,
      );
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleCreateAgentRun = async () => {
    const topic = agentTopicInput.trim();
    if (!topic) {
      setAgentStatusMessage("请先输入主题。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const runId = await startAgentRun(topic);
      if (!runId) {
        setAgentStatusMessage("创建 run 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentTopicInput("");
      setAgentStatusMessage(`已创建 run #${runId}`);
      await loadAgentRunsData(runId);
      await loadAgentRunEventsData(runId);
      await loadAgentDraftsData(runId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleDiscardAgentDraftSelection = () => {
    setAgentSelectedDraftId(null);
    setAgentStatusMessage("已取消当前草稿选择，可在左侧重新选择 run。");
  };

  const handleAppendAgentEvent = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    const message = agentEventMessage.trim();
    if (!message) {
      setAgentStatusMessage("请输入事件内容。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const ok = await appendAgentRunEvent(agentSelectedRunId, agentEventLevel, message);
      if (!ok) {
        setAgentStatusMessage("追加事件失败，请检查后端命令是否可用。");
        return;
      }
      setAgentEventMessage("");
      setAgentStatusMessage(`已追加事件到 run #${agentSelectedRunId}`);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleCompleteAgentRun = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const ok = await completeAgentRun(agentSelectedRunId, agentCompleteStatus);
      if (!ok) {
        setAgentStatusMessage("结束 run 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentStatusMessage(`run #${agentSelectedRunId} 已更新为 ${agentCompleteStatus}`);
      await loadAgentRunsData(agentSelectedRunId);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentDraftsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleGenerateAgentDraft = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    const topic = agentTopicInput.trim() || selectedAgentRun?.topic?.trim() || "";
    if (!topic) {
      setAgentStatusMessage("请先输入主题，或先创建带主题的 run。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const ok = await generateAgentDraft(agentSelectedRunId, topic, agentActiveSkillKey || null, agentResearchMode, agentAskFirst);
      if (!ok) {
        setAgentStatusMessage("生成 draft 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentStatusMessage(
        `已触发 run #${agentSelectedRunId} 的 draft 生成${agentActiveSkillKey ? `（skill: ${agentActiveSkillKey}）` : ""}`,
      );
      await loadAgentDraftsData(agentSelectedRunId);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleApproveAgentDraft = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    if (agentSelectedDraftId == null) {
      setAgentStatusMessage("请先选择一个 draft。");
      return;
    }
    // 先做冲突预检，再弹确认框
    const conflictInfo = await checkAgentDraftConflict(agentSelectedDraftId);
    if (conflictInfo) {
      setAgentApproveConfirm(conflictInfo);
    } else {
      // 非 Tauri 环境降级：直接执行（浏览器预览模式）
      await doApproveAgentDraft();
    }
  };

  const doApproveAgentDraft = async () => {
    if (agentSelectedRunId == null || agentSelectedDraftId == null) return;
    setAgentApproveConfirm(null);
    setAgentActionRunning(true);
    try {
      const ok = await approveAgentDraft(agentSelectedDraftId);
      if (!ok) {
        setAgentStatusMessage("审批失败，草稿内容未写盘，可重试或检查 Vault 路径。");
        return;
      }
      setAgentStatusMessage(`draft #${agentSelectedDraftId} 已审批写盘 ✓`);
      await loadAgentDraftsData(agentSelectedRunId, agentSelectedDraftId);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleRewriteAgentDraft = async () => {
    if (agentSelectedDraftId == null) return;
    const comment = agentRewriteComment.trim();
    if (!comment) return;
    setAgentActionRunning(true);
    try {
      const newDraft = await rewriteAgentDraft(agentSelectedDraftId, comment);
      if (!newDraft) {
        setAgentStatusMessage("重写草稿失败，请检查后端日志。");
        return;
      }
      setAgentRewriteComment("");
      setAgentSelectedDraftId(newDraft.id);
      setAgentStatusMessage(`已基于批注生成新草稿 #${newDraft.id}`);
      if (agentSelectedRunId != null) {
        await loadAgentDraftsData(agentSelectedRunId, newDraft.id);
        await loadAgentRunEventsData(agentSelectedRunId);
      }
    } finally {
      setAgentActionRunning(false);
    }
  };

  const agentShellQuickCommands = [
    { label: "pwd", command: "pwd", run: true },
    { label: "ls", command: "ls", run: true },
    { label: "ls -a", command: "ls -a", run: true },
    { label: "cd ..", command: "cd ..", run: true },
    { label: "git status", command: "git status", run: true },
  ] as const;

  const handleApplyShellCommand = (command: string, runImmediately = false) => {
    setAgentShellCmd(command);
    setAgentShellHistoryCursor(-1);
    setAgentShellDraftInput("");
    if (runImmediately) {
      void runShellCommand(command);
    }
  };

  const handleShellHistoryNav = (direction: "prev" | "next") => {
    const commands = agentShellHistory.map((entry) => entry.command);
    if (commands.length === 0) {
      return;
    }
    if (direction === "prev") {
      if (agentShellHistoryCursor === -1) {
        setAgentShellDraftInput(agentShellCmd);
        setAgentShellHistoryCursor(commands.length - 1);
        setAgentShellCmd(commands[commands.length - 1] ?? "");
        return;
      }
      const nextCursor = Math.max(0, agentShellHistoryCursor - 1);
      setAgentShellHistoryCursor(nextCursor);
      setAgentShellCmd(commands[nextCursor] ?? "");
      return;
    }
    if (agentShellHistoryCursor === -1) {
      return;
    }
    const nextCursor = agentShellHistoryCursor + 1;
    if (nextCursor >= commands.length) {
      setAgentShellHistoryCursor(-1);
      setAgentShellCmd(agentShellDraftInput);
      return;
    }
    setAgentShellHistoryCursor(nextCursor);
    setAgentShellCmd(commands[nextCursor] ?? "");
  };

  const handleCopyShellOutput = async (entry: ShellHistoryEntry) => {
    const payload = [
      `> ${entry.command}`,
      entry.result.stdout || "",
      entry.result.stderr || "",
    ]
      .filter((line) => line.trim().length > 0)
      .join("\n");
    const copied = await copyTextToClipboard(payload || entry.command);
    setAgentStatusMessage(copied ? "已复制命令输出。" : "复制失败：当前环境不支持写入剪贴板。");
  };

  const handleClearShellHistory = () => {
    setAgentShellHistory([]);
    setAgentToolsSeenCount(0);
    setAgentShellHistoryCursor(-1);
    setAgentShellDraftInput("");
  };

  const focusAgentShellInput = () => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        agentShellInputRef.current?.focus();
      });
    });
  };

  const runShellCommand = async (rawCommand?: string) => {
    const cmd = (rawCommand ?? agentShellCmd).trim();
    if (!cmd || agentShellRunning) return;
    const sessionId = agentShellSession?.session_id ?? null;
    const streamId = `sh-${Date.now()}-${Math.floor(Math.random() * 1_000_000).toString(16)}`;
    setAgentShellRunning(true);
    const id = ++agentShellIdRef.current;
    setAgentShellHistory((h) => [
      ...h,
      {
        id,
        command: cmd,
        result: {
          command: cmd,
          stdout: "",
          stderr: "",
          exit_code: 0,
          working_dir: agentShellSession?.working_dir ?? "",
          blocked: false,
          blocked_reason: null,
          policy_action: "unknown",
          policy_decision: "streaming",
          executor: "manual",
        },
        ts: Date.now(),
        running: true,
        live_stdout: "",
        live_stderr: "",
        stream_id: streamId,
        session_id: sessionId,
      },
    ]);
    try {
      const result = await runShell(cmd, undefined, "manual", sessionId, streamId);
      if (result) {
        setAgentShellHistory((h) =>
          h.map((entry) =>
            entry.id === id
              ? {
                  ...entry,
                  result: {
                    ...result,
                    stdout: entry.live_stdout?.trim().length ? entry.live_stdout : result.stdout,
                    stderr: entry.live_stderr?.trim().length ? entry.live_stderr : result.stderr,
                  },
                  running: false,
                }
              : entry,
          ),
        );
        if (result.working_dir && sessionId) {
          setAgentShellSession((prev) =>
            prev && prev.session_id === sessionId
              ? { ...prev, working_dir: result.working_dir }
              : prev,
          );
        }
      }
    } catch (e) {
      setAgentShellHistory((h) =>
        h.map((entry) =>
          entry.id === id
            ? {
                ...entry,
                result: {
                  command: cmd,
                  stdout: entry.live_stdout || "",
                  stderr: entry.live_stderr || String(e),
                  exit_code: -1,
                  working_dir: agentShellSession?.working_dir ?? "",
                  blocked: false,
                  blocked_reason: null,
                  policy_action: "unknown",
                  policy_decision: "error",
                  executor: "manual",
                },
                running: false,
              }
            : entry,
        ),
      );
    } finally {
      setAgentShellRunning(false);
      setAgentShellCmd("");
      setAgentShellHistoryCursor(-1);
      setAgentShellDraftInput("");
      focusAgentShellInput();
    }
  };

  const handleRunShell = async () => {
    await runShellCommand(agentShellCmd);
  };

  const executeAgentTask = async (
    instructionInput: string,
    statusLabel: string,
  ): Promise<boolean> => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return false;
    }
    const instruction = instructionInput.trim();
    if (!instruction) {
      setAgentStatusMessage("请输入任务指令。");
      return false;
    }
    const budget = Math.min(8, Math.max(1, agentTaskMaxIterations));
    const memoryLines = agentMemories
      .map((mem) => `- ${mem.memory_key || "记忆"}: ${mem.memory_value}`)
      .join("\n");
    const activeSkill = agentSkills.find((item) => item.skill_key === agentActiveSkillKey);
    const skillPromptRendered = activeSkill?.prompt_template
      ? activeSkill.prompt_template
          .replaceAll("{{topic}}", instruction)
          .replaceAll("{{memories}}", memoryLines || "（无）")
          .trim()
      : "";
    const contextSections: string[] = [];
    if (skillPromptRendered) {
      contextSections.push(`[技能模板：${activeSkill?.skill_key || "未命名"}]\n${skillPromptRendered}`);
    } else if (agentActiveSkillKey) {
      contextSections.push(`[技能模板：${agentActiveSkillKey}]`);
    }
    if (memoryLines) {
      contextSections.push(`[记忆上下文]\n${memoryLines}`);
    }
    const memoryContext = contextSections.join("\n\n");
    setAgentTaskRunning(true);
    try {
      const result = await runAgentTask(
        agentSelectedRunId,
        instruction,
        budget,
        memoryContext || undefined,
      );
      if (!result) {
        setAgentStatusMessage(`${statusLabel}执行失败，请检查后端日志。`);
        return false;
      }
      setAgentTaskResult(result);
      setAgentStatusMessage(`${statusLabel}已完成（run #${agentSelectedRunId}）。`);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
      return true;
    } finally {
      setAgentTaskRunning(false);
    }
  };

  const handleRunAgentTask = async () => {
    await executeAgentTask(agentTaskInstruction, "任务模式");
  };

  const handleContinueAgentTask = async () => {
    const baseInstruction = agentTaskInstruction.trim() || selectedAgentRun?.topic?.trim() || "继续当前任务";
    const recentTrace = agentEvents
      .slice(0, 16)
      .reverse()
      .filter((event) => {
        const msg = String(event.message ?? "");
        return /tool_start|tool_end|awaiting_approval|任务模式/.test(msg);
      })
      .map((event) => `- [${formatLintCheckedAt(event.created_at)}] ${truncateText(String(event.message ?? ""), 180)}`)
      .join("\n");
    const previousResult = agentTaskResult.trim().slice(0, 900);
    const continuationInstruction = [
      "继续上次未完成的任务，优先复用已获得信息，避免重复调用工具。",
      `原始指令：${baseInstruction}`,
      previousResult ? `上次阶段性结果：\n${previousResult}` : "",
      recentTrace ? `最近执行轨迹：\n${recentTrace}` : "",
      "若仍需工具调用，请先说明理由，再执行最小必要步骤。",
    ]
      .filter(Boolean)
      .join("\n\n");
    await executeAgentTask(continuationInstruction, "续跑任务");
  };

  const handleApproveAgentWrite = async () => {
    if (!agentSelectedRunId) return;
    setAgentActionRunning(true);
    try {
      const msg = await approveAgentWrite(agentSelectedRunId);
      if (msg) {
        setAgentStatusMessage(`✅ 已写入 Wiki：${msg}`);
      } else {
        setAgentStatusMessage("✅ 已写入 Wiki。");
      }
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } catch (e) {
      setAgentStatusMessage(`审批失败: ${String(e)}`);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleRejectAgentWrite = async () => {
    if (!agentSelectedRunId) return;
    setAgentActionRunning(true);
    try {
      const msg = await rejectAgentWrite(agentSelectedRunId);
      if (msg) {
        setAgentStatusMessage(`🚫 已拒绝写入：${msg}`);
      } else {
        setAgentStatusMessage("🚫 已拒绝写入。");
      }
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } catch (e) {
      setAgentStatusMessage(`拒绝失败: ${String(e)}`);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleUpsertAgentMemory = async () => {
    const value = agentMemoryValueInput.trim();
    if (!value) {
      setAgentStatusMessage("记忆内容不能为空。");
      return;
    }
    const key = agentMemoryKeyInput.trim() || value.slice(0, 20);
    setAgentActionRunning(true);
    try {
      const item = await upsertAgentMemory(agentSelectedRunId, key, value);
      if (!item) {
        setAgentStatusMessage("保存记忆失败，请检查后端。");
        return;
      }
      setAgentMemoryKeyInput("");
      setAgentMemoryValueInput("");
      setAgentMemoryComposerOpen(false);
      setAgentStatusMessage(`记忆「${key}」已保存。`);
      await loadAgentMemoriesData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleDeleteAgentMemory = async (id: number) => {
    setAgentActionRunning(true);
    try {
      const ok = await deleteAgentMemory(id);
      if (!ok) {
        setAgentStatusMessage("删除记忆失败。");
        return;
      }
      setAgentStatusMessage("记忆已删除。");
      await loadAgentMemoriesData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleUpsertAgentSkill = async () => {
    const skillKey = agentSkillKeyInput.trim();
    const promptTemplate = agentSkillPromptInput.trim();
    if (!skillKey || !promptTemplate) {
      setAgentStatusMessage("技能键与模板内容不能为空。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const item = await upsertAgentSkill(skillKey, promptTemplate);
      if (!item) {
        setAgentStatusMessage("保存技能模板失败，请检查后端。");
        return;
      }
      setAgentSkillKeyInput("");
      setAgentSkillPromptInput("");
      setAgentSkillComposerOpen(false);
      setAgentActiveSkillKey(item.skill_key);
      setAgentStatusMessage(`技能模板「${item.skill_key}」已保存（v${item.version}）。`);
      await loadAgentSkillsData();
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleDeleteAgentSkill = async (id: number, skillKey: string) => {
    setAgentActionRunning(true);
    try {
      const ok = await deleteAgentSkill(id);
      if (!ok) {
        setAgentStatusMessage("删除技能模板失败。");
        return;
      }
      setAgentStatusMessage(`技能模板「${skillKey}」已删除。`);
      await loadAgentSkillsData();
    } finally {
      setAgentActionRunning(false);
    }
  };

  const openOperationsModule = (tab: "queue" | "stats") => {
    setOperationsTab(tab);
    setActiveModule("operations");
    if (tab === "stats") {
      void loadVaultStats();
      return;
    }
    void listIngestQueue()
      .then((items) => setIngestQueue(items))
      .catch(() => {});
  };

  const handleNavModuleSelect = (moduleId: ModuleId) => {
    if (moduleId === "operations") {
      openOperationsModule(operationsTab);
      return;
    }
    setActiveModule(moduleId);
    if (moduleId === "agent") {
      void loadAgentRunsData();
      void loadAgentSkillsData();
    }
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
        {ingestDragActive ? (
          <div className="status-bar status-bar--dragging">
            <span>释放鼠标即可开始摄入（支持 md/pdf/docx/pptx/txt/图片）。</span>
          </div>
        ) : null}

        <div className={`module-viewport${activeModule === "ask" ? " module-viewport--ask" : ""}${activeModule === "agent" ? " module-viewport--agent" : ""}${activeModule === "agent" && agentDebugPanelOpen ? " module-viewport--agent-debug" : ""}`}>
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
                      <div className="dev-panel__field" style={{ flex: 1 }}>
                      <label className="dev-panel__label">选择项目模板</label>
                      <select
                        className="dev-panel__input"
                        value={selectedTemplateId}
                        onChange={(e) => setSelectedTemplateId(e.target.value)}
                        disabled={devAction !== null}
                      >
                        {templates.map((tpl) => (
                          <option key={tpl.id} value={tpl.id}>
                            {tpl.icon} {tpl.name} — {tpl.description}
                          </option>
                        ))}
                      </select>
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

                  {recentVaultPaths.length > 0 ? (
                    <div className="recent-vaults">
                      <div className="recent-vaults__head">
                        <span className="dev-panel__hint">最近项目</span>
                        <button
                          type="button"
                          className="dev-panel__button recent-vaults__clear"
                          onClick={() => {
                            setRecentVaultPaths([]);
                          }}
                        >
                          清空
                        </button>
                      </div>
                      <div className="recent-vaults__list">
                        {recentVaultPaths.map((path) => (
                          <button
                            key={path}
                            type="button"
                            className="recent-vaults__item"
                            title={path}
                            onClick={() => setVaultPath(path)}
                          >
                            {path}
                          </button>
                        ))}
                      </div>
                    </div>
                  ) : null}

                  <div className="template-init-preview">
                    <div className="template-init-preview__head">
                      <div className="template-init-preview__title">
                        <span>{selectedTemplate.icon}</span>
                        <strong>{selectedTemplate.name}</strong>
                      </div>
                      <span className="template-init-preview__desc">{selectedTemplate.description}</span>
                    </div>
                    <div className="template-init-preview__meta">
                      <span>schema：{selectedTemplate.schema.split(/\r?\n/).length} 行</span>
                      <span>purpose：{selectedTemplate.purpose.split(/\r?\n/).length} 行</span>
                    </div>
                    <div className="template-init-preview__grid">
                      <div className="template-init-preview__block">
                        <h4>将创建目录（{templateInitPreview.dirs.length}）</h4>
                        <ul>
                          {templateInitPreview.dirs.map((dirPath) => (
                            <li key={dirPath}>
                              <code>{dirPath}</code>
                            </li>
                          ))}
                        </ul>
                      </div>
                      <div className="template-init-preview__block">
                        <h4>将创建文件（{templateInitPreview.files.length}）</h4>
                        <ul>
                          {templateInitPreview.files.map((filePath) => (
                            <li key={filePath}>
                              <code>{filePath}</code>
                            </li>
                          ))}
                        </ul>
                      </div>
                    </div>
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
                        <button
                          type="button"
                          className="dev-panel__button"
                          disabled={!isTauriRuntime() || queueEnqueueing || !ingestUrlInput.trim()}
                          onClick={() => {
                            if (!ingestUrlInput.trim()) return;
                            setQueueEnqueueing(true);
                            enqueueIngest("url", ingestUrlInput.trim())
                              .then(() => {
                                openOperationsModule("queue");
                              })
                              .catch((err: unknown) => {
                                console.error("加入队列失败:", err);
                              })
                              .finally(() => {
                                setQueueEnqueueing(false);
                              });
                          }}
                        >
                          {queueEnqueueing ? "入队中..." : "加入队列"}
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
                        <button
                          type="button"
                          className="dev-panel__button"
                          disabled={!isTauriRuntime() || queueEnqueueing || (ingestFilePickedPaths.length === 0 && !ingestFilePath.trim())}
                          onClick={() => {
                            const paths = ingestFilePickedPaths.length > 0 ? ingestFilePickedPaths : [ingestFilePath.trim()];
                            const validPaths = paths.filter(Boolean);
                            if (validPaths.length === 0) return;
                            setQueueEnqueueing(true);
                            Promise.all(validPaths.map((p) => enqueueIngest("file", p)))
                              .then(() => {
                                openOperationsModule("queue");
                              })
                              .catch((err: unknown) => {
                                console.error("加入队列失败:", err);
                              })
                              .finally(() => {
                                setQueueEnqueueing(false);
                              });
                          }}
                        >
                          {queueEnqueueing ? "入队中..." : "加入队列"}
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

              {/* Web Clipper 安装向导 */}
              <section className="panel" style={{ marginTop: "16px" }}>
                <div className="section-head">
                  <h2>网页剪藏扩展</h2>
                  <span style={{
                    display: "inline-block", padding: "2px 8px", borderRadius: "12px",
                    background: clipServerOnline === false ? "var(--color-warning-bg, #fffbe6)" : "var(--color-success-bg, #ecfdf5)",
                    color: clipServerOnline === false ? "var(--color-warning-text, #7c5a00)" : "var(--color-success, #065f46)",
                    fontSize: "12px", fontWeight: 600
                  }}>
                    {clipServerOnline === false ? "⚠ 服务未启动" : `● 服务运行中 :${CLIP_SERVER_PORT}`}
                  </span>
                </div>
                <p style={{ marginBottom: "12px", color: "var(--color-text-2, #555)", fontSize: "13px" }}>
                  浏览器扩展可将网页一键剪藏到知识库，并自动触发摄入。剪藏内容保存至 <code>raw/clips/</code>。
                </p>
                <div style={{ background: "var(--color-bg-2, #f5f5f5)", borderRadius: "8px", padding: "12px 16px" }}>
                  <p style={{ fontWeight: 600, marginBottom: "8px", fontSize: "13px" }}>安装步骤（Chrome / Edge）：</p>
                  <ol style={{ paddingLeft: "18px", fontSize: "13px", lineHeight: "2" }}>
                    <li>打开浏览器，访问 <code>chrome://extensions</code></li>
                    <li>启用右上角「<strong>开发者模式</strong>」</li>
                    <li>点击「<strong>加载已解压的扩展程序</strong>」</li>
                    <li>选择项目根目录下的 <code>extension/</code> 文件夹</li>
                    <li>扩展安装后点击工具栏中的 📚 图标即可剪藏当前页面</li>
                  </ol>
                </div>
                <p style={{ marginTop: "10px", fontSize: "12px", color: "var(--color-text-3, #888)" }}>
                  ℹ️ 确保应用保持运行，扩展通过 HTTP 与本应用通信（端口 {CLIP_SERVER_PORT}）
                </p>
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
                    <button
                      type="button"
                      className="wiki-new-btn"
                      onClick={() => { setShowNewPageModal(true); setNewPageResult(null); setNewPageTopic(""); }}
                      title="AI 辅助新建 Wiki 页面"
                    >
                      + AI 新建
                    </button>
                  </div>
                </div>
                {/* AI 新建页面弹窗 */}
                {showNewPageModal && (
                  <div className="new-page-modal-backdrop" onClick={() => setShowNewPageModal(false)}>
                    <div className="new-page-modal" onClick={(e) => e.stopPropagation()}>
                      <h3 className="new-page-modal__title">AI 辅助新建 Wiki 页面</h3>
                      <p className="new-page-modal__hint">
                        输入主题，AI 将参考现有知识库生成结构化初稿。
                      </p>
                      <input
                        className="new-page-modal__input"
                        type="text"
                        placeholder="例如：量子纠缠、黑洞蒸发、Rust 生命周期…"
                        value={newPageTopic}
                        onChange={(e) => setNewPageTopic(e.target.value)}
                        onKeyDown={(e) => { if (e.key === "Enter") void handleCreatePageWithAi(); }}
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
                          onClick={() => void handleCreatePageWithAi()}
                          disabled={newPageCreating || !newPageTopic.trim()}
                        >
                          {newPageCreating ? "AI 生成中…" : "生成页面"}
                        </button>
                        {newPageResult && (
                          <button
                            className="new-page-modal__btn"
                            onClick={() => {
                              setShowNewPageModal(false);
                              setActiveModule("wiki");
                              setWikiKeyword(newPageResult.title);
                            }}
                          >
                            查看页面
                          </button>
                        )}
                        <button className="new-page-modal__btn" onClick={() => setShowNewPageModal(false)}>
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
                  <button
                    type="button"
                    className="ask-new-session-btn"
                    disabled={!isTauriRuntime() || askSessionManaging}
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

                          {message.role === "assistant" &&
                            !message.streaming &&
                            askSearchDebugVisible &&
                            message.meta?.searchDebug &&
                            message.meta.searchDebug.routes.length > 0 && (
                              <details className="ask-message__debug">
                                <summary>
                                  检索调试：{formatQuerySearchStrategyLabel(
                                    message.meta.searchDebug.strategy,
                                  )}
                                  {typeof message.meta.searchDebug.rrf_k === "number" &&
                                    `（k=${message.meta.searchDebug.rrf_k}）`}
                                </summary>
                                <div className="ask-message__debug-actions">
                                  <button
                                    type="button"
                                    className="ask-message__debug-copy"
                                    onClick={() =>
                                      void handleCopySearchDebug(
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
                            ref={graphSearchInputRef}
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
                        disabled={!graphRenderData || graphRenderData.nodes.length === 0}
                      >
                        {graphLayoutFrozen ? "恢复布局" : "冻结布局"}
                      </button>
                      <button
                        type="button"
                        className={`dev-panel__button ${graphAggregateMode ? "dev-panel__button--accent" : ""}`}
                        disabled={(graphVisibleData?.nodes.length ?? 0) <= GRAPH_AGGREGATE_THRESHOLD}
                        onClick={() => setGraphAggregateMode((prev) => !prev)}
                        title={
                          (graphVisibleData?.nodes.length ?? 0) <= GRAPH_AGGREGATE_THRESHOLD
                            ? `节点数未超过 ${GRAPH_AGGREGATE_THRESHOLD}，无需聚合`
                            : "按分组聚合显示大图，降低渲染压力"
                        }
                      >
                        {graphAggregateMode ? "聚合模式：开" : "聚合模式：关"}
                      </button>
                      <button
                        type="button"
                        className="dev-panel__button"
                        onClick={handleExportGraphJson}
                        disabled={!graphRenderData || graphRenderData.nodes.length === 0}
                        title="导出当前视图图谱 JSON"
                      >
                        导出 JSON
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
                          setGraphInsightSparseDensity(DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold);
                          setGraphInsightBridgeMinGroups(DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups);
                          setGraphInsightSurprisingJaccard(DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard);
                          setGraphInsightSurprisingConfidence(
                            DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence,
                          );
                          setGraphLayoutFrozen(false);
                          setGraphAggregateMode(false);
                          setGraphSelectedAggregateId("");
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
                      {(graphVisibleData?.nodes.length ?? 0) > GRAPH_AGGREGATE_THRESHOLD ? (
                        <span className="pill">{`聚合 ${graphAggregateMode ? "ON" : "OFF"}`}</span>
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
                    {!graphLoading && graphRenderData && graphRenderData.nodes.length > 0 && (
                      <GraphErrorBoundary>
                        <Suspense fallback={<div className="graph-module__loading">图谱渲染中...</div>}>
                          {/* eslint-disable-next-line @typescript-eslint/no-explicit-any */}
                          <ForceGraph2D
                            ref={graphRef}
                            graphData={graphRenderData as any}
                            width={graphDimensions.width}
                            height={graphDimensions.height}
                            nodeLabel="label"
                            nodeRelSize={6}
                            nodeVal={(node: object) => {
                              const n = node as KnowledgeGraphNode & AggregatedNode;
                              if (n.isAggregate) {
                                return 5 + Math.min(16, n.count ?? 1);
                              }
                              const degree = graphMetrics.totalDegree.get(n.id) ?? 0;
                              return 2 + Math.min(8, degree);
                            }}
                            nodeColor={(node: object) => {
                              const n = node as KnowledgeGraphNode & AggregatedNode;
                              return n.group ? groupColor(n.group) : "#4a9eff";
                            }}
                            linkColor={() => "rgba(120,120,180,0.4)"}
                            linkWidth={(link: object) => {
                              const edge = link as AggregatedEdge;
                              return edge.weight ? Math.min(1 + edge.weight * 0.5, 4) : 1;
                            }}
                            onNodeClick={(node: object, _event: MouseEvent) => {
                              const n = node as KnowledgeGraphNode & AggregatedNode;
                              const now = Date.now();
                              const last = (graphRef.current as any).__lastNodeClickTime ?? 0;
                              const lastId = (graphRef.current as any).__lastNodeClickId ?? "";
                              (graphRef.current as any).__lastNodeClickTime = now;
                              (graphRef.current as any).__lastNodeClickId = n.id;
                              if (now - last < 400 && lastId === n.id && !n.isAggregate) {
                                const pagePath = resolveGraphNodePagePath(n);
                                if (pagePath) { setActiveModule("wiki"); void handleOpenWikiPage(pagePath); }
                                return;
                              }
                              handleGraphNodeClick(node);
                            }}
                            nodeCanvasObject={(node: object, ctx: CanvasRenderingContext2D, globalScale: number) => {
                              const n = node as KnowledgeGraphNode & AggregatedNode;
                              const label = n.label || n.id;
                              const isAggregateNode = Boolean(n.isAggregate);
                              const selected = Boolean(
                                !isAggregateNode && graphSelectedNodeId && isSameWikiPagePath(graphSelectedNodeId, n.id),
                              );
                              const isSearchHit = graphSearchHits.has(n.id);
                              const radius = isAggregateNode
                                ? Math.min(14, 7 + Math.floor((n.count ?? 1) / 3))
                                : selected
                                  ? 8
                                  : 5;

                              // 绘制搜索命中光晕
                              if (isSearchHit) {
                                ctx.beginPath();
                                ctx.arc(n.x ?? 0, n.y ?? 0, radius + 8, 0, 2 * Math.PI, false);
                                ctx.fillStyle = "rgba(255, 235, 59, 0.4)";
                                ctx.fill();
                              }

                              const fontSize = Math.max(10 / globalScale, 3);
                              ctx.font = `${fontSize}px Sans-Serif`;
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
                              if (globalScale > 1.4 || selected || isAggregateNode) {
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
                            const node = graphSearchableNodes.find((n) => n.id === id);
                            if (!node) return null;
                            return (
                              <li key={node.id}>
                                <button
                                  type="button"
                                  className={`graph-neighbors__item ${
                                    graphSelectedNodeId === node.id || graphSelectedAggregateId === node.id
                                      ? "graph-neighbors__item--active"
                                      : ""
                                  }`}
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

                  <div className="graph-insight-config">
                    <div className="graph-sidepanel__head">
                      <h3>洞察阈值</h3>
                      <span>本地持久化</span>
                    </div>
                    <label className="graph-insight-config__item" htmlFor="graph-insight-sparse-density">
                      <span>稀疏密度 ≤ {graphInsightSparseDensity.toFixed(2)}</span>
                      <input
                        id="graph-insight-sparse-density"
                        type="range"
                        min={GRAPH_INSIGHT_SPARSE_DENSITY_MIN}
                        max={GRAPH_INSIGHT_SPARSE_DENSITY_MAX}
                        step={0.05}
                        value={graphInsightSparseDensity}
                        onChange={(event) => handleGraphInsightSparseDensityChange(Number(event.target.value))}
                      />
                    </label>
                    <label className="graph-insight-config__item" htmlFor="graph-insight-bridge-min-groups">
                      <span>桥接分组数 ≥ {graphInsightBridgeMinGroups}</span>
                      <input
                        id="graph-insight-bridge-min-groups"
                        type="range"
                        min={GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN}
                        max={GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX}
                        step={1}
                        value={graphInsightBridgeMinGroups}
                        onChange={(event) => handleGraphInsightBridgeMinGroupsChange(Number(event.target.value))}
                      />
                    </label>
                    <label className="graph-insight-config__item" htmlFor="graph-insight-surprising-jaccard">
                      <span>异常连接相似度 ≤ {graphInsightSurprisingJaccard.toFixed(2)}</span>
                      <input
                        id="graph-insight-surprising-jaccard"
                        type="range"
                        min={GRAPH_INSIGHT_SURPRISING_JACCARD_MIN}
                        max={GRAPH_INSIGHT_SURPRISING_JACCARD_MAX}
                        step={0.05}
                        value={graphInsightSurprisingJaccard}
                        onChange={(event) => handleGraphInsightSurprisingJaccardChange(Number(event.target.value))}
                      />
                    </label>
                    <label
                      className="graph-insight-config__item"
                      htmlFor="graph-insight-surprising-confidence"
                    >
                      <span>异常连接置信度 ≥ {graphInsightSurprisingConfidence.toFixed(2)}</span>
                      <input
                        id="graph-insight-surprising-confidence"
                        type="range"
                        min={GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN}
                        max={GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX}
                        step={0.05}
                        value={graphInsightSurprisingConfidence}
                        onChange={(event) =>
                          handleGraphInsightSurprisingConfidenceChange(Number(event.target.value))
                        }
                      />
                    </label>
                    <hr className="graph-sidepanel__divider" />
                  </div>

                  <div className="graph-insights">
                    <div className="graph-sidepanel__head">
                      <h3>图谱洞察</h3>
                      <span>{graphInsights.length} 条</span>
                    </div>
                    {graphInsights.length === 0 ? (
                      <p className="graph-sidepanel__hint">当前视图未发现明显结构风险。</p>
                    ) : (
                      <ul className="graph-insights__list">
                        {graphInsights.map((insight) => (
                          <li key={`${insight.kind}-${insight.title}`}>
                            <button
                              type="button"
                              className="graph-insights__item"
                              onClick={() => handleApplyGraphInsight(insight)}
                            >
                              <div className="graph-insights__head">
                                <strong>{insight.title}</strong>
                                <span className="pill">{graphInsightKindLabels[insight.kind]}</span>
                              </div>
                              <p>{insight.description}</p>
                              {insight.evidence.length > 0 ? (
                                <ul className="graph-insights__evidence">
                                  {insight.evidence.map((item) => (
                                    <li key={item}>{item}</li>
                                  ))}
                                </ul>
                              ) : null}
                              <p className="graph-insights__suggestion">{insight.suggestion}</p>
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}
                    <hr className="graph-sidepanel__divider" />
                  </div>

                  <div className="graph-sidepanel__head">
                    <h3>节点详情</h3>
                    <span>
                      {graphSelectedNode || graphSelectedAggregateNode ? "已选中" : "未选中"} ·{" "}
                      {graphViewMode === "local" ? "Local" : "Global"}
                    </span>
                  </div>
                  {graphSelectedAggregateNode ? (
                    <>
                      <div className="graph-node-card">
                        <p className="graph-node-card__title">
                          {graphSelectedAggregateNode.label || graphSelectedAggregateNode.id}
                        </p>
                        <code className="graph-node-card__path">{graphSelectedAggregateNode.id}</code>
                        <div className="graph-node-card__stats">
                          <span className="pill">{`聚合节点`}</span>
                          <span className="pill">{`成员 ${graphSelectedAggregateMembers.length}`}</span>
                        </div>
                        <div className="graph-node-card__actions">
                          <button
                            type="button"
                            className="dev-panel__button dev-panel__button--accent"
                            onClick={handleExpandSelectedAggregateNode}
                          >
                            展开查看成员页
                          </button>
                          <button
                            type="button"
                            className="dev-panel__button"
                            onClick={handleExitAggregateMode}
                          >
                            切回明细模式
                          </button>
                        </div>
                      </div>
                      <div className="graph-neighbors">
                        <div className="graph-neighbors__head">
                          <h4>成员页面</h4>
                          <span>{graphSelectedAggregateMembers.length}</span>
                        </div>
                        {graphSelectedAggregateMembers.length === 0 ? (
                          <p className="graph-sidepanel__hint">该聚合节点暂无可展开成员。</p>
                        ) : (
                          <ul className="graph-neighbors__list">
                            {graphSelectedAggregateMembers.slice(0, 20).map((member) => (
                              <li key={member.id}>
                                <button
                                  type="button"
                                  className="graph-neighbors__item"
                                  onClick={() => {
                                    void handleOpenAggregateMemberPage(member.id);
                                  }}
                                  title={member.id}
                                >
                                  <span>{member.label}</span>
                                  <code>{member.group || "未分组"}</code>
                                </button>
                              </li>
                            ))}
                          </ul>
                        )}
                      </div>
                    </>
                  ) : !graphSelectedNode ? (
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
            <SettingsModule
              llmConfig={llmConfig}
              defaultCloudModel={defaultCloudModel}
              defaultCloudProviderName={defaultCloudProviderName}
              defaultCloudBaseUrl={defaultCloudBaseUrl}
              selectedPreset={selectedPreset}
              llmPresets={llmPresets}
              onPresetChange={handlePresetChange}
              llmConfigActiveProvider={llmConfigActiveProvider}
              setLlmConfigActiveProvider={setLlmConfigActiveProvider}
              llmConfigCloudProviderName={llmConfigCloudProviderName}
              setLlmConfigCloudProviderName={setLlmConfigCloudProviderName}
              llmConfigCloudApiKey={llmConfigCloudApiKey}
              setLlmConfigCloudApiKey={setLlmConfigCloudApiKey}
              llmConfigCloudBaseUrl={llmConfigCloudBaseUrl}
              setLlmConfigCloudBaseUrl={setLlmConfigCloudBaseUrl}
              llmConfigCloudModel={llmConfigCloudModel}
              setLlmConfigCloudModel={setLlmConfigCloudModel}
              llmConfigEmbedModel={llmConfigEmbedModel}
              setLlmConfigEmbedModel={setLlmConfigEmbedModel}
              llmConfigEmbedBaseUrl={llmConfigEmbedBaseUrl}
              setLlmConfigEmbedBaseUrl={setLlmConfigEmbedBaseUrl}
              llmConfigSaving={llmConfigSaving}
              onSaveLlmConfig={handleSaveLlmConfig}
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
              operationsTab={operationsTab}
              setOperationsTab={setOperationsTab}
              ingestQueue={ingestQueue}
              refreshQueue={() =>
                listIngestQueue()
                  .then((items) => setIngestQueue(items))
                  .catch(() => {})
              }
              cancelQueueItem={(id) =>
                cancelIngestItem(id)
                  .then(() => listIngestQueue())
                  .then((items) => setIngestQueue(items))
                  .catch(() => {})
              }
              retryQueueItem={(id) =>
                retryIngestItem(id)
                  .then(() => listIngestQueue())
                  .then((items) => setIngestQueue(items))
                  .catch(() => {})
              }
              vaultStats={vaultStats}
              vaultStatsLoading={vaultStatsLoading}
              loadVaultStats={loadVaultStats}
              navigateTo={setActiveModule}
            />
          )}
          {/* ---- Deep Research 模块 ---- */}
          {activeModule === "research" && (
            <ResearchPanel
              onOpenWikiPage={(path) => {
                fetchWikiPageDetail(path)
                  .then((detail) => {
                    if (detail) {
                      setActiveModule("wiki");
                    }
                  })
                  .catch(() => {});
              }}
            />
          )}
          {/* ---- Agent Studio 模块（B2：双栏对话 + 草稿审阅） ---- */}
          {activeModule === "agent" && (
            <>
              <div className="module-header">
                <h1 className="module-header__title">Agent Studio</h1>
                <p className="module-header__sub">
                  左侧对话驱动，右侧草稿预览与审批写盘
                </p>
              </div>
              <section className={`panel agent-studio agent-studio--b2${agentDebugPanelOpen ? " agent-studio--debug-open" : ""}`}>
                {agentStatusMessage ? (
                  <p
                    className={`agent-studio__status agent-studio__status--${getAgentStatusTone(agentStatusMessage)}`}
                  >
                    {agentStatusMessage}
                  </p>
                ) : null}
                <div
                  ref={agentLayoutRef}
                  className="agent-studio__layout"
                  style={{ gridTemplateColumns: `minmax(0,${agentLeftRatio}fr) 6px minmax(0,${1 - agentLeftRatio}fr)`, gap: 0 }}
                >
                  <section className="agent-studio__left">
                    <div className={`agent-studio__run-strip${agentRunStripOpen ? " agent-studio__run-strip--open" : ""}`}>
                      <button
                        type="button"
                        className="agent-studio__context-toggle agent-studio__run-strip-toggle"
                        onClick={() => setAgentRunStripOpen((prev) => !prev)}
                      >
                        <span>{agentRunStripOpen ? "▼" : "▶"} 历史 Runs</span>
                        <span className="agent-studio__context-meta">
                          {agentRunManageMode ? agentRunCards.length : agentVisibleRunCards.length} 条
                        </span>
                      </button>
                      {agentRunStripOpen ? (
                        <div className="agent-studio__run-strip-body">
                          <label className="agent-studio__run-strip-manage">
                            <input
                              type="checkbox"
                              checked={agentRunManageMode}
                              onChange={(event) => setAgentRunManageMode(event.target.checked)}
                            />
                            管理模式（显示已归档）
                          </label>
                          <p className="agent-studio__run-strip-note">
                            {agentRunManageMode
                              ? `当前显示全部 run（含已归档 ${agentArchivedRunCount} 条）`
                              : `默认隐藏已归档 run（已归档 ${agentArchivedRunCount} 条）`}
                          </p>
                          {agentRunsLoading ? (
                            <p className="agent-studio__run-strip-empty">正在加载...</p>
                          ) : agentVisibleRunCards.length === 0 ? (
                            <p className="agent-studio__run-strip-empty">暂无历史 run</p>
                          ) : (
                            <div className="agent-studio__run-strip-list">
                              {agentVisibleRunCards.map((run) => {
                                const active = run.id === agentSelectedRunId;
                                const statusTone = getAgentRunStatusTone(String(run.status || ""));
                                const topic = run.topic?.trim() || `Run #${run.id}`;
                                return (
                                  <div
                                    key={`run-card-${run.id}`}
                                    className={`agent-studio__run-card${active ? " agent-studio__run-card--active" : ""}`}
                                  >
                                    <button
                                      type="button"
                                      className="agent-studio__run-card-main"
                                      onClick={() => handleSelectAgentRunFromChat(run.id)}
                                    >
                                      <span className="agent-studio__run-card-title" title={topic}>
                                        #{run.id} {topic}
                                      </span>
                                      <span className={`agent-studio__run-card-status agent-studio__run-card-status--${statusTone}`}>
                                        {formatAgentRunStatusLabel(String(run.status || ""))}
                                      </span>
                                      {run.archived_at ? (
                                        <span className="agent-studio__run-card-archived">已归档</span>
                                      ) : null}
                                      <time dateTime={run.updated_at || run.created_at}>
                                        {formatLintCheckedAt(run.updated_at || run.created_at)}
                                      </time>
                                    </button>
                                    <div className="agent-studio__run-card-actions">
                                      {agentRunManageMode ? (
                                        run.archived_at ? (
                                          <button
                                            type="button"
                                            className="dev-panel__button"
                                            disabled={agentRunMutatingId != null}
                                            onClick={() => {
                                              void handleRestoreAgentRun(run.id);
                                            }}
                                          >
                                            恢复
                                          </button>
                                        ) : (
                                          <button
                                            type="button"
                                            className="dev-panel__button"
                                            disabled={agentRunMutatingId != null}
                                            onClick={() => {
                                              void handleArchiveAgentRun(run.id);
                                            }}
                                          >
                                            归档
                                          </button>
                                        )
                                      ) : null}
                                    </div>
                                  </div>
                                );
                              })}
                            </div>
                          )}
                        </div>
                      ) : null}
                    </div>
                    <div className="agent-studio__context">
                      <button
                        type="button"
                        className="agent-studio__context-toggle"
                        onClick={() => setAgentContextOpen((prev) => !prev)}
                      >
                        <span>{agentContextOpen ? "▼" : "▶"} 上下文配置（记忆 / 技能）</span>
                        <span className="agent-studio__context-meta">
                          记忆 {agentMemories.length} · 技能 {agentSkills.length}
                        </span>
                      </button>
                      {agentContextOpen ? (
                        <div className="agent-studio__context-body">
                          <section className="agent-studio__context-section">
                            <button
                              type="button"
                              className="agent-studio__section-toggle"
                              onClick={() => setAgentMemoryPanelOpen((prev) => !prev)}
                            >
                              <span>{agentMemoryPanelOpen ? "▼" : "▶"} 记忆上下文</span>
                              <span className="agent-studio__section-meta">{agentMemories.length} 条</span>
                            </button>
                            {agentMemoryPanelOpen ? (
                              <div className="agent-studio__section-body">
                                <div className="agent-studio__memory-chipbar">
                                  <div className="agent-studio__memory-chipbar-list">
                                    {agentMemoriesLoading ? (
                                      <span className="agent-studio__memory-chip-placeholder">加载中...</span>
                                    ) : agentMemories.length === 0 ? (
                                      <span className="agent-studio__memory-chip-placeholder">暂无记忆</span>
                                    ) : (
                                      agentMemories.map((mem) => (
                                        <span
                                          key={mem.id}
                                          className="agent-studio__memory-chip"
                                          title={`${mem.memory_key}: ${mem.memory_value}`}
                                        >
                                          <strong>{mem.memory_key}</strong>
                                          <span>{mem.memory_value}</span>
                                          <button
                                            type="button"
                                            className="agent-studio__memory-chip-remove"
                                            disabled={agentActionRunning || !isTauriRuntime()}
                                            onClick={() => void handleDeleteAgentMemory(mem.id)}
                                            aria-label={`删除记忆 ${mem.memory_key}`}
                                          >
                                            ×
                                          </button>
                                        </span>
                                      ))
                                    )}
                                    <button
                                      type="button"
                                      className="agent-studio__memory-chip-add"
                                      disabled={agentActionRunning || !isTauriRuntime()}
                                      onClick={() => setAgentMemoryComposerOpen((prev) => !prev)}
                                    >
                                      {agentMemoryComposerOpen ? "收起" : "+ 添加"}
                                    </button>
                                  </div>
                                </div>
                                {agentMemoryComposerOpen ? (
                                  <div className="agent-studio__memory-inline-form">
                                    <div className="agent-studio__memory-inline-form-row">
                                      <input
                                        type="text"
                                        className="dev-panel__input"
                                        placeholder="键（可选）"
                                        value={agentMemoryKeyInput}
                                        onChange={(e) => setAgentMemoryKeyInput(e.target.value)}
                                      />
                                      <input
                                        type="text"
                                        className="dev-panel__input"
                                        placeholder="记忆内容"
                                        value={agentMemoryValueInput}
                                        onChange={(e) => setAgentMemoryValueInput(e.target.value)}
                                      />
                                    </div>
                                    <button
                                      type="button"
                                      className="dev-panel__button"
                                      disabled={
                                        agentActionRunning
                                        || !agentMemoryValueInput.trim()
                                        || !isTauriRuntime()
                                      }
                                      onClick={() => void handleUpsertAgentMemory()}
                                    >
                                      保存记忆
                                    </button>
                                  </div>
                                ) : null}
                              </div>
                            ) : null}
                          </section>
                          <section className="agent-studio__context-section">
                            <button
                              type="button"
                              className="agent-studio__section-toggle"
                              onClick={() => setAgentSkillPanelOpen((prev) => !prev)}
                            >
                              <span>{agentSkillPanelOpen ? "▼" : "▶"} 技能模板</span>
                              <span className="agent-studio__section-meta">
                                {agentSkills.length} 个
                              </span>
                            </button>
                            {agentSkillPanelOpen ? (
                              <div className="agent-studio__section-body">
                                <div className="agent-studio__skills">
                                  <div className="agent-studio__skills-head">
                                    <div className="agent-studio__skills-head-main">
                                      <select
                                        className="dev-panel__input agent-studio__skill-active-select"
                                        value={agentActiveSkillKey}
                                        onChange={(event) => setAgentActiveSkillKey(event.target.value)}
                                        disabled={agentSkillsLoading || agentSkills.length === 0}
                                        title="选择本次生成生效的技能模板"
                                      >
                                        <option value="">不使用技能模板</option>
                                        {agentSkills.map((skill) => (
                                          <option key={`active-skill-${skill.id}`} value={skill.skill_key}>
                                            {skill.skill_key} (v{skill.version})
                                          </option>
                                        ))}
                                      </select>
                                    </div>
                                    <button
                                      type="button"
                                      className="agent-studio__memory-chip-add"
                                      disabled={agentActionRunning || !isTauriRuntime()}
                                      onClick={() => setAgentSkillComposerOpen((prev) => !prev)}
                                    >
                                      {agentSkillComposerOpen ? "收起" : "+ 新建"}
                                    </button>
                                  </div>
                                  {agentSkillComposerOpen ? (
                                    <div className="agent-studio__skill-form">
                                      <input
                                        type="text"
                                        className="dev-panel__input"
                                        placeholder="技能键（如：writer）"
                                        value={agentSkillKeyInput}
                                        onChange={(e) => setAgentSkillKeyInput(e.target.value)}
                                      />
                                      <textarea
                                        className="dev-panel__input"
                                        placeholder="模板提示词（将用于后续技能化编排）"
                                        value={agentSkillPromptInput}
                                        onChange={(e) => setAgentSkillPromptInput(e.target.value)}
                                        rows={3}
                                      />
                                      <p className="agent-studio__skill-form-hint">
                                        同名技能键会覆盖并递增版本；如需并存请使用不同技能键。
                                      </p>
                                      <button
                                        type="button"
                                        className="dev-panel__button"
                                        disabled={
                                          agentActionRunning
                                          || !agentSkillKeyInput.trim()
                                          || !agentSkillPromptInput.trim()
                                          || !isTauriRuntime()
                                        }
                                        onClick={() => void handleUpsertAgentSkill()}
                                      >
                                        保存技能
                                      </button>
                                    </div>
                                  ) : null}
                                  {agentSkillsLoading ? (
                                    <p className="agent-studio__empty">技能模板加载中...</p>
                                  ) : agentSkills.length === 0 ? (
                                    <p className="agent-studio__empty">暂无技能模板，可先创建 writer/reviewer 等角色提示。</p>
                                  ) : (
                                    <>
                                      <p className="agent-studio__skill-active-hint">
                                        当前生效：<strong>{agentActiveSkillKey || "不使用技能模板"}</strong>
                                      </p>
                                      <ul className="agent-studio__skill-list">
                                      {agentSkills.map((skill) => (
                                        <li
                                          key={skill.id}
                                          className={`agent-studio__skill-item${agentActiveSkillKey === skill.skill_key ? " agent-studio__skill-item--active" : ""}`}
                                        >
                                          <div className="agent-studio__skill-main">
                                            <button
                                              type="button"
                                              className="agent-studio__skill-select"
                                              onClick={() => setAgentActiveSkillKey(skill.skill_key)}
                                            >
                                              <strong>{skill.skill_key}</strong>
                                              {agentActiveSkillKey === skill.skill_key ? (
                                                <span className="agent-studio__skill-badge">生效中</span>
                                              ) : null}
                                            </button>
                                            <span>v{skill.version}</span>
                                          </div>
                                          <p title={skill.prompt_template}>{skill.prompt_template}</p>
                                          <div className="agent-studio__skill-actions">
                                            <time dateTime={skill.updated_at}>
                                              {formatLintCheckedAt(skill.updated_at)}
                                            </time>
                                            <button
                                              type="button"
                                              className="dev-panel__button"
                                              disabled={agentActionRunning || !isTauriRuntime()}
                                              onClick={() => void handleDeleteAgentSkill(skill.id, skill.skill_key)}
                                            >
                                              删除
                                            </button>
                                          </div>
                                        </li>
                                      ))}
                                      </ul>
                                    </>
                                  )}
                                </div>
                              </div>
                            ) : null}
                          </section>
                        </div>
                      ) : null}
                    </div>
                    <div className="agent-studio__chat-thread">
                      {agentRunsLoading ? (
                        <p className="agent-studio__empty">加载历史 run...</p>
                      ) : agentChatMessages.length === 0 ? (
                        <p className="agent-studio__empty">暂无历史，输入主题后开始第一轮创作。</p>
                      ) : (
                        agentChatMessages.map((message) => {
                          const selected = message.run_id === agentSelectedRunId;
                          return (
                            <button
                              key={message.id}
                              type="button"
                              className={`agent-studio__chat-bubble agent-studio__chat-bubble--${message.role}${selected ? " agent-studio__chat-bubble--selected" : ""}`}
                              onClick={() => handleSelectAgentRunFromChat(message.run_id)}
                            >
                              <span className="agent-studio__chat-bubble-head">
                                <strong>{message.role === "user" ? "You" : "Agent"}</strong>
                                <time dateTime={message.created_at}>
                                  {formatLintCheckedAt(message.created_at)}
                                </time>
                              </span>
                              <span className="agent-studio__chat-bubble-content">{message.content}</span>
                              {message.role === "agent" && message.status ? (
                                <span className="agent-studio__chat-bubble-status">
                                  {formatAgentRunStatusLabel(message.status)}
                                </span>
                              ) : null}
                            </button>
                          );
                        })
                      )}
                    </div>
                    <div className="agent-studio__composer">
                      <input
                        id="agent-topic-input"
                        className="dev-panel__input"
                        type="text"
                        value={agentTopicInput}
                        placeholder="输入主题或指令，例如：写一篇“管网压力模型”Wiki"
                        onChange={(event) => setAgentTopicInput(event.target.value)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter") {
                            event.preventDefault();
                            void handleAgentChatSend();
                          }
                        }}
                      />
                      <button
                        type="button"
                        className="dev-panel__button dev-panel__button--accent"
                        disabled={agentActionRunning || !agentTopicInput.trim() || !isTauriRuntime()}
                        onClick={() => {
                          void handleAgentChatSend();
                        }}
                      >
                        {agentActionRunning ? "生成中..." : "发送"}
                      </button>
                    </div>
                    <div className="agent-studio__composer-tools">
                      <label className="agent-studio__research-toggle" title="开启后读取 wiki 页面正文（前 400 字）作为上下文，生成质量更高但速度略慢">
                        <input
                          type="checkbox"
                          checked={agentResearchMode}
                          onChange={(e) => setAgentResearchMode(e.target.checked)}
                        />
                        检索增强
                      </label>
                      <label className="agent-studio__research-toggle" title="开启后先对主题做一次 Ask 问答，将知识库现有答案注入草稿上下文（需要 LLM 可用，速度较慢）">
                        <input
                          type="checkbox"
                          checked={agentAskFirst}
                          onChange={(e) => setAgentAskFirst(e.target.checked)}
                        />
                        Ask 联动
                      </label>
                      <button
                        type="button"
                        className="dev-panel__button"
                        disabled={agentRunsLoading || !isTauriRuntime()}
                        onClick={() => {
                          void loadAgentRunsData(agentSelectedRunId);
                          if (agentSelectedRunId != null) {
                            void loadAgentDraftsData(agentSelectedRunId, agentSelectedDraftId);
                            void loadAgentRunEventsData(agentSelectedRunId);
                          }
                        }}
                      >
                        刷新
                      </button>
                      <button
                        type="button"
                        className="dev-panel__button"
                        disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                        onClick={() => {
                          void handleGenerateAgentDraft();
                        }}
                      >
                        基于当前 Run 重写
                      </button>
                    </div>
                  </section>
                  {/* 左右面板拖拽分割条 */}
                  <div
                    className="split-handle"
                    onMouseDown={(e) => {
                      e.preventDefault();
                      const container = agentLayoutRef.current;
                      if (!container) return;
                      agentDragRef.current = {
                        active: true,
                        startX: e.clientX,
                        startRatio: agentLeftRatio,
                        containerW: container.getBoundingClientRect().width,
                      };
                      document.body.style.cursor = 'col-resize';
                      document.body.style.userSelect = 'none';
                      document.body.classList.add('split-dragging');
                    }}
                  />
                  <section className="agent-studio__right">
                    <div className="agent-studio__draft-head">
                      <h3 className="agent-studio__title">
                        {selectedAgentDraft
                          ? `${selectedAgentDraft.title || "未命名草稿"}（#${selectedAgentDraft.id}）`
                          : "草稿预览"}
                      </h3>
                      {selectedAgentDraft ? (
                        <span className="agent-studio__item-meta">
                          {selectedAgentDraft.status}
                          {" · "}
                          {formatLintCheckedAt(selectedAgentDraft.updated_at || selectedAgentDraft.created_at)}
                          {agentDraftAppliedSkillKey ? ` · skill:${agentDraftAppliedSkillKey}` : ""}
                        </span>
                      ) : null}
                    </div>
                    <div className="agent-studio__main-tabs">
                      <button
                        type="button"
                        className={`agent-studio__main-tab${agentRightTab === "task" ? " agent-studio__main-tab--active" : ""}`}
                        onClick={() => setAgentRightTab("task")}
                      >
                        任务
                        {agentHasPendingApproval ? <span className="agent-studio__main-tab-dot">审批中</span> : null}
                      </button>
                      <button
                        type="button"
                        className={`agent-studio__main-tab${agentRightTab === "draft" ? " agent-studio__main-tab--active" : ""}`}
                        onClick={() => setAgentRightTab("draft")}
                      >
                        草稿
                      </button>
                      <button
                        type="button"
                        className={`agent-studio__main-tab${agentRightTab === "tools" ? " agent-studio__main-tab--active" : ""}${agentToolsNeedsAttention ? " agent-studio__main-tab--attention" : ""}`}
                        onClick={() => setAgentRightTab("tools")}
                      >
                        工具
                        <span className="agent-studio__main-tab-meta">
                          {agentShellRunning ? "运行中" : `历史 ${agentShellHistory.length} 条`}
                        </span>
                      </button>
                    </div>
                    <div className="agent-studio__right-main">
                      {agentRightTab === "task" ? (
                        <section className="agent-studio__task-mode">
                          <h3 className="agent-studio__title">任务模式（Beta）</h3>
                          <div className="agent-studio__task-presets">
                            <button
                              type="button"
                              className="dev-panel__button"
                              disabled={agentTaskRunning}
                              onClick={() => {
                                setAgentTaskInstruction(
                                  "请新建页面 wiki/agent-e2e-write.md，内容包含标题“Agent E2E Write”，并在执行前说明为什么要调用 write_wiki。",
                                );
                              }}
                            >
                              填充：写入审批验证
                            </button>
                            <button
                              type="button"
                              className="dev-panel__button"
                              disabled={agentTaskRunning}
                              onClick={() => {
                                setAgentTaskInstruction(
                                  "请编辑页面 wiki/agent-e2e-write.md，把“Agent E2E Write”替换为“Agent E2E Edited”，并优先使用 edit_wiki。",
                                );
                              }}
                            >
                              填充：编辑审批验证
                            </button>
                          </div>
                          <textarea
                            className="dev-panel__input agent-studio__task-mode-input"
                            rows={3}
                            placeholder="输入任务指令，例如：梳理当前 run 的草稿风险并给出下一步执行计划。"
                            value={agentTaskInstruction}
                            onChange={(event) => setAgentTaskInstruction(event.target.value)}
                            disabled={agentTaskRunning}
                          />
                          <div className="agent-studio__task-mode-actions">
                            <label className="agent-studio__task-mode-budget">
                              预算轮次
                              <input
                                type="number"
                                min={1}
                                max={8}
                                className="dev-panel__input"
                                value={agentTaskMaxIterations}
                                onChange={(event) => {
                                  const next = Number(event.target.value);
                                  if (Number.isFinite(next)) {
                                    setAgentTaskMaxIterations(Math.min(8, Math.max(1, Math.floor(next))));
                                  }
                                }}
                                disabled={agentTaskRunning}
                              />
                            </label>
                            <button
                              type="button"
                              className="dev-panel__button"
                              disabled={
                                agentTaskRunning
                                || !agentTaskInstruction.trim()
                                || agentSelectedRunId == null
                                || !isTauriRuntime()
                              }
                              onClick={() => {
                                void handleRunAgentTask();
                              }}
                            >
                              {agentTaskRunning ? "执行中..." : "运行任务"}
                            </button>
                            <button
                              type="button"
                              className="dev-panel__button"
                              disabled={
                                agentTaskRunning
                                || agentSelectedRunId == null
                                || !isTauriRuntime()
                                || (agentEvents.length === 0 && !agentTaskResult.trim())
                              }
                              onClick={() => {
                                void handleContinueAgentTask();
                              }}
                            >
                              继续任务
                            </button>
                          </div>
                          {agentTaskResult ? (
                            <pre className="agent-studio__task-mode-result">{agentTaskResult}</pre>
                          ) : (
                            <p className="agent-studio__empty">任务结果将在此显示。</p>
                          )}
                          {agentSelectedRunId != null && agentExecTimeline.length > 0 && (
                            <div className="agent-studio__exec-log">
                              <div className="agent-studio__exec-log-head">
                                <span>工具时间线</span>
                                <span className="agent-studio__exec-log-count">{agentExecTimeline.length} 条</span>
                              </div>
                              <ul className="agent-studio__exec-log-list">
                                {agentExecTimeline.map((item) => (
                                  <li key={item.key} className={`agent-studio__exec-log-row agent-studio__exec-log-row--${item.level}`}>
                                    <div className="agent-studio__exec-log-row-main">
                                      <span className="agent-studio__exec-log-title">{item.title}</span>
                                      <span className="agent-studio__exec-log-side">
                                        {item.durationMs != null ? `${item.durationMs}ms` : "—"}
                                        {" · "}
                                        {formatLintCheckedAt(item.createdAt)}
                                      </span>
                                    </div>
                                    <span className="agent-studio__exec-log-msg">{item.summary}</span>
                                    {item.detail ? (
                                      <details className="agent-studio__exec-log-detail">
                                        <summary>展开详情</summary>
                                        <pre>{item.detail}</pre>
                                      </details>
                                    ) : null}
                                  </li>
                                ))}
                              </ul>
                            </div>
                          )}
                          {agentHasPendingApproval ? (
                            <div className="agent-studio__approval-bar">
                              <span className="agent-studio__approval-label">
                                ⏸ Agent 请求修改 Wiki（写入 / 编辑），等待审批
                              </span>
                              <div className="agent-studio__approval-actions">
                                <button
                                  className="agent-studio__approval-approve"
                                  disabled={agentActionRunning}
                                  onClick={() => void handleApproveAgentWrite()}
                                >
                                  ✅ 批准
                                </button>
                                <button
                                  className="agent-studio__approval-reject"
                                  disabled={agentActionRunning}
                                  onClick={() => void handleRejectAgentWrite()}
                                >
                                  🚫 拒绝
                                </button>
                              </div>
                            </div>
                          ) : null}
                        </section>
                      ) : null}
                      {agentRightTab === "draft" ? (
                        <>
                          {selectedAgentDraft != null &&
                          !["approved", "applied"].includes(String(selectedAgentDraft.status).toLowerCase()) ? (
                            <div className="agent-studio__rewrite-bar">
                              <input
                                type="text"
                                className="dev-panel__input"
                                placeholder="批注（如：语气更专业、增加原理推导）"
                                value={agentRewriteComment}
                                onChange={(e) => setAgentRewriteComment(e.target.value)}
                                onKeyDown={(e) => {
                                  if (e.key === "Enter") { e.preventDefault(); void handleRewriteAgentDraft(); }
                                }}
                              />
                              <button
                                type="button"
                                className="dev-panel__button"
                                disabled={agentActionRunning || !agentRewriteComment.trim() || !isTauriRuntime()}
                                onClick={() => void handleRewriteAgentDraft()}
                              >
                                基于批注重写
                              </button>
                            </div>
                          ) : null}
                          <div className="agent-studio__review-tabs">
                            <button
                              type="button"
                              className={`agent-studio__review-tab${agentReviewTab === "draft" ? " agent-studio__review-tab--active" : ""}`}
                              onClick={() => setAgentReviewTab("draft")}
                            >
                              Draft
                            </button>
                            <button
                              type="button"
                              className={`agent-studio__review-tab${agentReviewTab === "diff" ? " agent-studio__review-tab--active" : ""}`}
                              onClick={() => setAgentReviewTab("diff")}
                              disabled={selectedAgentDraft == null}
                            >
                              Diff
                            </button>
                            <button
                              type="button"
                              className={`agent-studio__review-tab${agentReviewTab === "citations" ? " agent-studio__review-tab--active" : ""}`}
                              onClick={() => setAgentReviewTab("citations")}
                              disabled={selectedAgentDraft == null}
                            >
                              Citations
                            </button>
                          </div>
                          {agentReviewTab === "draft" && selectedAgentDraft ? (
                            <div className="agent-studio__flow-controls">
                              <div className="agent-studio__flow-meta">
                                <span className="agent-studio__flow-badge">
                                  {agentFlowMode === "playing"
                                    ? "流式生成中"
                                    : agentFlowMode === "paused"
                                      ? "已暂停"
                                      : agentFlowMode === "done"
                                        ? "已完成"
                                        : "待开始"}
                                </span>
                                <span className="agent-studio__flow-progress-text">{agentFlowProgress}%</span>
                              </div>
                              <div className="agent-studio__flow-progress">
                                <div
                                  className="agent-studio__flow-progress-fill"
                                  style={{ width: `${agentFlowProgress}%` }}
                                />
                              </div>
                              <div className="agent-studio__flow-buttons">
                                <button
                                  type="button"
                                  className="dev-panel__button"
                                  disabled={agentFlowMode !== "playing"}
                                  onClick={handlePauseAgentFlow}
                                >
                                  暂停
                                </button>
                                <button
                                  type="button"
                                  className="dev-panel__button"
                                  disabled={agentFlowMode !== "paused"}
                                  onClick={handleResumeAgentFlow}
                                >
                                  继续
                                </button>
                                <button
                                  type="button"
                                  className="dev-panel__button"
                                  disabled={agentFlowMode === "done"}
                                  onClick={handleCompleteAgentFlow}
                                >
                                  完成
                                </button>
                                <button
                                  type="button"
                                  className="dev-panel__button"
                                  onClick={handleReplayAgentFlow}
                                >
                                  重播
                                </button>
                              </div>
                              {agentFlowOutline.length > 0 ? (
                                <p className="agent-studio__flow-outline">
                                  提纲：{agentFlowOutline.join(" / ")}
                                </p>
                              ) : null}
                            </div>
                          ) : null}
                          <div className="agent-studio__draft-pane">
                            {selectedAgentDraft == null ? (
                              agentActionRunning ? (
                                <div className="agent-studio__draft-skeleton" aria-live="polite">
                                  <span />
                                  <span />
                                  <span />
                                  <span />
                                </div>
                              ) : (
                                <p className="agent-studio__empty">请选择左侧 run，右侧将显示对应草稿内容。</p>
                              )
                            ) : agentReviewTab === "diff" ? (
                              agentDraftConflictLoading ? (
                                <p className="agent-studio__empty">正在加载差异基线...</p>
                              ) : agentDraftConflictPreview?.conflict && agentDraftDiffRows.length > 0 ? (
                                <div className="wiki-history-diff__rows">
                                  {agentDraftDiffRows.map((row, index) => (
                                    <div
                                      key={`agent-diff-${index}-${row.kind}-${row.oldLineNumber ?? "na"}-${row.newLineNumber ?? "na"}`}
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
                              ) : (
                                <p className="agent-studio__empty">
                                  当前没有可用差异基线（仅在同名页面存在时展示 Diff）。
                                </p>
                              )
                            ) : agentReviewTab === "citations" ? (
                              agentDraftCitations.length > 0 ? (
                                <ul className="agent-studio__citation-list">
                                  {agentDraftCitations.map((citation) => (
                                    <li key={citation}>
                                      <code>[[{citation}]]</code>
                                    </li>
                                  ))}
                                </ul>
                              ) : (
                                <p className="agent-studio__empty">当前草稿未检测到 `[[wiki-link]]` 引用。</p>
                              )
                            ) : agentDraftDisplayContent.trim() ? (
                              <div
                                className="agent-studio__draft-markdown wiki-markdown"
                                // biome-ignore lint/security/noDangerouslySetInnerHtml: sanitized by DOMPurify
                                dangerouslySetInnerHTML={{
                                  __html: DOMPurify.sanitize(
                                    marked.parse(agentDraftDisplayContent.trim(), {
                                      gfm: true,
                                      breaks: false,
                                    }) as string,
                                  ),
                                }}
                              />
                            ) : (
                              <p className="agent-studio__empty">（该 draft 暂无内容）</p>
                            )}
                          </div>
                          <div className="agent-studio__draft-actions">
                            <button
                              type="button"
                              className="dev-panel__button dev-panel__button--primary"
                              disabled={
                                agentActionRunning
                                || selectedAgentDraft == null
                                || ["approved", "applied"].includes(
                                  String(selectedAgentDraft?.status ?? "").toLowerCase(),
                                )
                                || !isTauriRuntime()
                              }
                              onClick={() => {
                                void handleApproveAgentDraft();
                              }}
                            >
                              写入 Wiki
                            </button>
                            <button
                              type="button"
                              className="dev-panel__button"
                              disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                              onClick={() => {
                                void handleGenerateAgentDraft();
                              }}
                            >
                              重写
                            </button>
                            <button
                              type="button"
                              className="dev-panel__button"
                              disabled={agentActionRunning || selectedAgentDraft == null}
                              onClick={handleDiscardAgentDraftSelection}
                            >
                              丢弃
                            </button>
                          </div>
                        </>
                      ) : null}
                      {agentRightTab === "tools" ? (
                        <section className={`agent-studio__tools-workspace agent-studio__tools-workspace--${agentShellTheme}`}>
                          <div className="agent-studio__tools-head">
                            <div className="agent-studio__tools-session">
                              <span>会话：{agentShellSession?.session_id ? agentShellSession.session_id.slice(-8) : "未就绪"}</span>
                              <span>目录：{agentShellSession?.working_dir || "—"}</span>
                            </div>
                            <div className="agent-studio__tools-actions">
                              <button
                                type="button"
                                className="dev-panel__button"
                                onClick={handleClearShellHistory}
                                disabled={agentShellRunning || agentShellHistory.length === 0}
                              >
                                清空历史
                              </button>
                              <button
                                type="button"
                                className="dev-panel__button"
                                onClick={() => setAgentShellTheme((prev) => (prev === "deep" ? "light" : "deep"))}
                              >
                                {agentShellTheme === "deep" ? "浅色" : "深色"}
                              </button>
                            </div>
                          </div>
                          <p className="agent-studio__shell-hint">
                            任务模式中 Agent 会自动调用工具；此处为会话式终端，支持连续命令与目录上下文。
                          </p>
                          <div className="agent-studio__shell-policy agent-studio__shell-policy--compact">
                            <div className="agent-studio__shell-policy-head">
                              <strong>Shell 策略</strong>
                              <div className="agent-studio__shell-policy-presets">
                                <span>档位：</span>
                                {shellPolicyProfiles.map((profile) => (
                                  <button
                                    key={profile.key}
                                    type="button"
                                    className="agent-studio__shell-policy-preset"
                                    disabled={agentShellPolicySaving || !agentShellPolicyConfig}
                                    onClick={() => {
                                      void handleApplyAndSaveShellPolicyProfile(profile.key);
                                    }}
                                  >
                                    {profile.label}
                                  </button>
                                ))}
                              </div>
                            </div>
                            <p className="agent-studio__shell-policy-tip">
                              详细策略请到设置页修改；此处仅快速切换档位。
                            </p>
                          </div>
                          <div className="agent-studio__shell-quick">
                            {agentShellQuickCommands.map((item) => (
                              <button
                                key={item.label}
                                type="button"
                                className="agent-studio__shell-quick-btn"
                                disabled={agentShellRunning || !agentShellSession}
                                onClick={() => handleApplyShellCommand(item.command, item.run)}
                              >
                                {item.label}
                              </button>
                            ))}
                          </div>
                          <div
                            className="agent-studio__shell-history"
                            ref={agentShellHistoryRef}
                            onScroll={(event) => {
                              const el = event.currentTarget;
                              const gap = el.scrollHeight - el.scrollTop - el.clientHeight;
                              agentShellAutoFollowRef.current = gap < 56;
                            }}
                          >
                            {agentShellHistory.length === 0 ? (
                              <p className="agent-studio__shell-empty">暂无执行历史，输入命令后可在此查看结果。</p>
                            ) : (
                              agentShellHistory.map((e) => (
                                <div
                                  key={e.id}
                                  className={`agent-studio__shell-entry ${e.result.blocked ? "blocked" : e.result.exit_code === 0 ? "ok" : "err"}`}
                                >
                                  <div className="agent-studio__shell-entry-head">
                                    <div className="agent-studio__shell-prompt">❯ {e.command}</div>
                                    <div className="agent-studio__shell-entry-actions">
                                      <span className={`agent-studio__shell-status-badge ${e.running ? "running" : e.result.exit_code === 0 ? "ok" : "err"}`}>
                                        {e.running ? "运行中" : `exit ${e.result.exit_code}`}
                                      </span>
                                      <button
                                        type="button"
                                        className="agent-studio__shell-copy-btn"
                                        onClick={() => {
                                          void handleCopyShellOutput(e);
                                        }}
                                      >
                                        复制
                                      </button>
                                    </div>
                                  </div>
                                  {e.result.blocked ? (
                                    <div className="agent-studio__shell-blocked">⛔ {e.result.blocked_reason}</div>
                                  ) : (
                                    <pre className="agent-studio__shell-output">
                                      {(e.running
                                        ? `${e.live_stdout || ""}${e.live_stderr || ""}`.trim() || "执行中..."
                                        : (e.result.stdout || e.result.stderr || `(exit ${e.result.exit_code})`))
                                      }
                                    </pre>
                                  )}
                                  <div className="agent-studio__shell-meta">
                                    {e.result.executor} · {e.result.policy_action} · {e.result.policy_decision}
                                    {e.result.working_dir ? ` · cwd=${e.result.working_dir}` : ""}
                                    {e.running ? " · streaming..." : ""}
                                  </div>
                                </div>
                              ))
                            )}
                          </div>
                          <div className="agent-studio__shell-input-wrap">
                            <div className="agent-studio__shell-input-row">
                              <input
                                ref={agentShellInputRef}
                                type="text"
                                className="agent-studio__shell-input"
                                placeholder="PowerShell 命令（Enter 执行，↑↓ 浏览历史）"
                                value={agentShellCmd}
                                onChange={(ev) => {
                                  setAgentShellCmd(ev.target.value);
                                  setAgentShellHistoryCursor(-1);
                                }}
                                onKeyDown={(ev) => {
                                  if (ev.key === "ArrowUp") {
                                    ev.preventDefault();
                                    handleShellHistoryNav("prev");
                                    return;
                                  }
                                  if (ev.key === "ArrowDown") {
                                    ev.preventDefault();
                                    handleShellHistoryNav("next");
                                    return;
                                  }
                                  if (ev.key === "Enter" && !agentShellRunning) {
                                    ev.preventDefault();
                                    void handleRunShell();
                                  }
                                }}
                                disabled={agentShellRunning || !agentShellSession}
                              />
                              <button
                                className="agent-studio__shell-run-btn"
                                disabled={!agentShellCmd.trim() || agentShellRunning || !agentShellSession}
                                onMouseDown={(ev) => {
                                  ev.preventDefault();
                                }}
                                onClick={() => void handleRunShell()}
                              >
                                {agentShellRunning ? "执行中" : "运行"}
                              </button>
                            </div>
                            <p className="agent-studio__shell-input-tip">支持会话命令（例如 `cd ..`）和流式输出；工具调用与任务模式隔离。</p>
                          </div>
                        </section>
                      ) : null}
                    </div>
                  </section>
                </div>
                <div className="agent-studio__debug-toggle">
                  <button
                    type="button"
                    className="dev-panel__button"
                    onClick={() => setAgentDebugPanelOpen((prev) => !prev)}
                  >
                    {agentDebugPanelOpen ? "收起调试面板" : "展开调试面板"}
                  </button>
                </div>
                {agentDebugPanelOpen ? (
                  <div className="agent-studio__debug">
                    <div className="agent-studio__create">
                      <label className="dev-panel__label" htmlFor="agent-event-level">事件级别</label>
                      <select
                        id="agent-event-level"
                        className="dev-panel__input"
                        value={agentEventLevel}
                        onChange={(event) =>
                          setAgentEventLevel((event.target.value as AgentRunEventLevel) || "info")
                        }
                      >
                        {agentEventLevelOptions.map((level) => (
                          <option key={level} value={level}>{level}</option>
                        ))}
                      </select>
                      <input
                        className="dev-panel__input"
                        type="text"
                        value={agentEventMessage}
                        placeholder="事件内容（例如：完成初始检索）"
                        onChange={(event) => setAgentEventMessage(event.target.value)}
                      />
                      <button
                        type="button"
                        className="dev-panel__button"
                        disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                        onClick={() => {
                          void handleAppendAgentEvent();
                        }}
                      >
                        追加事件
                      </button>
                    </div>
                    <div className="agent-studio__actions">
                      <button
                        type="button"
                        className="dev-panel__button"
                        disabled={agentActionRunning || !agentTopicInput.trim() || !isTauriRuntime()}
                        onClick={() => {
                          void handleCreateAgentRun();
                        }}
                      >
                        仅创建 Run
                      </button>
                      <select
                        className="dev-panel__input"
                        value={agentCompleteStatus}
                        onChange={(event) =>
                          setAgentCompleteStatus((event.target.value as AgentRunStatus) || "applied")
                        }
                      >
                        {agentCompleteStatusOptions.map((status) => (
                          <option key={status} value={status}>{status}</option>
                        ))}
                      </select>
                      <button
                        type="button"
                        className="dev-panel__button"
                        disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                        onClick={() => {
                          void handleCompleteAgentRun();
                        }}
                      >
                        结束 Run
                      </button>
                    </div>
                    <section className="agent-studio__debug-events">
                      <h3 className="agent-studio__title">
                        Events{agentSelectedRunId != null ? `（Run #${agentSelectedRunId}）` : ""}
                      </h3>
                      {agentSelectedRunId == null ? (
                        <p className="agent-studio__empty">请选择一个 run 查看事件。</p>
                      ) : agentEventsLoading ? (
                        <p className="agent-studio__empty">加载中...</p>
                      ) : agentEvents.length === 0 ? (
                        <p className="agent-studio__empty">暂无事件。</p>
                      ) : (
                        <ul className="agent-studio__list">
                          {agentEvents.map((event) => (
                            <li key={event.id} className="agent-studio__event-row">
                              <span className={`agent-studio__event-level agent-studio__event-level--${String(event.level).toLowerCase()}`}>
                                {event.level}
                              </span>
                              <span className="agent-studio__event-message">{event.message}</span>
                              <time className="agent-studio__event-time" dateTime={event.created_at}>
                                {formatLintCheckedAt(event.created_at)}
                              </time>
                            </li>
                          ))}
                        </ul>
                      )}
                    </section>
                  </div>
                ) : null}
              </section>
            </>
          )}
        </div>
        {/* 审批前确认弹窗（H1） */}
        {agentApproveConfirm ? (
          <div
            className="agent-draft-confirm-overlay"
            role="dialog"
            aria-modal="true"
            aria-label="确认写盘"
            onClick={() => setAgentApproveConfirm(null)}
          >
            <div
              className="agent-draft-confirm-dialog"
              onClick={(e) => e.stopPropagation()}
            >
              <h3 className="agent-draft-confirm-dialog__title">确认写盘</h3>
              <p className="agent-draft-confirm-dialog__body">
                即将将 Draft「<strong>{agentApproveConfirm.title}</strong>
                」写入 Wiki。
                {agentApproveConfirm.conflict && (
                  <span className="agent-draft-confirm-dialog__warn">
                    {" "}
                    ⚠ 同名页面已存在，写盘将覆盖现有内容。
                  </span>
                )}
                {!agentApproveConfirm.conflict && " 此操作不可撤销，确认继续？"}
              </p>
              {agentApproveConfirm.conflict && agentApproveConfirm.existing_preview && (
                <details className="agent-draft-confirm-dialog__existing">
                  <summary>查看现有页面预览（前 300 字）</summary>
                  <pre>{agentApproveConfirm.existing_preview}</pre>
                </details>
              )}
              <div className="agent-draft-confirm-dialog__actions">
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--primary"
                  disabled={agentActionRunning}
                  onClick={() => void doApproveAgentDraft()}
                >
                  确认写盘
                </button>
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={() => setAgentApproveConfirm(null)}
                >
                  取消
                </button>
              </div>
            </div>
          </div>
        ) : null}
        {ingestPreviewDialog ? (
          <div
            className="ingest-preview-modal"
            role="dialog"
            aria-modal="true"
            aria-label="摄入分析卡"
            onClick={() => closeIngestPreviewDialog(false)}
          >
            <div className="ingest-preview-modal__panel" onClick={(event) => event.stopPropagation()}>
              <div className="ingest-preview-modal__head">
                <div>
                  <h3>摄入分析卡</h3>
                  <p>确认后写入 Wiki 与索引</p>
                </div>
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={() => closeIngestPreviewDialog(false)}
                >
                  取消
                </button>
              </div>
              <div className="ingest-preview-modal__source">
                <span>来源文件</span>
                <code>{ingestPreviewDialog.source_path}</code>
              </div>
              <div className="ingest-preview-modal__section">
                <h4>摘要</h4>
                <pre>{ingestPreviewDialog.summary?.trim() || "（未生成摘要）"}</pre>
              </div>
              <div className="ingest-preview-modal__columns">
                <section className="ingest-preview-modal__section">
                  <h4>提取实体（{ingestPreviewDialog.entities.length}）</h4>
                  {ingestPreviewDialog.entities.length > 0 ? (
                    <ul>
                      {ingestPreviewDialog.entities.map((entity) => (
                        <li key={entity}>{entity}</li>
                      ))}
                    </ul>
                  ) : (
                    <p className="ingest-preview-modal__empty">暂无实体</p>
                  )}
                </section>
                <section className="ingest-preview-modal__section">
                  <h4>拟更新页面（{ingestPreviewDialog.updated_pages.length}）</h4>
                  {ingestPreviewDialog.updated_pages.length > 0 ? (
                    <ul>
                      {ingestPreviewDialog.updated_pages.map((pagePath) => (
                        <li key={pagePath}>
                          <code>{pagePath}</code>
                        </li>
                      ))}
                    </ul>
                  ) : (
                    <p className="ingest-preview-modal__empty">无关联页面更新</p>
                  )}
                </section>
              </div>
              <div className="ingest-preview-modal__actions">
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={() => closeIngestPreviewDialog(false)}
                >
                  取消
                </button>
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--accent"
                  onClick={() => closeIngestPreviewDialog(true)}
                >
                  确认写入
                </button>
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </div>
    </div>
  );
}

// QueuePanel 已提取到 modules/operations/QueuePanel.tsx

// ResearchDialog 已提取到 modules/research/ResearchDialog.tsx


// ResearchPanel 已提取到 modules/research/ResearchPanel.tsx


// SearchConfigPanel 已提取到 modules/lint/SearchConfigPanel.tsx
