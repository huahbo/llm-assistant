import { useEffect, useRef, useState } from "react";
import {
  fetchAppOverview,
  fetchDefaultPaths,
  fetchLlmStatus,
  fetchLlmConfig,
  fetchRecentLintPatchEvents,
  fetchQuerySettings,
  fetchRecentLogs,
  fetchRecentWikiPages,
  fetchWikiPageDetail,
  fetchWikiPageCitations,
  initVault,
  ingestMarkdown,
  isTauriRuntime,
  queryAskWithOptions,
  runLint,
  applyLintPatch,
  applyLintPatchesBatch,
  previewLintPatches,
  saveLlmConfig,
  searchWikiPages,
  saveQueryAnswer,
  setBackendMode,
  setQueryTopK as persistQueryTopK,
  formatLlmStatusSummary,
  resolveDisplayPath,
  listenProgress,
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
  BackendAppMode,
  LlmProviderConfig,
  LlmStatus,
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
  const [wikiKeyword, setWikiKeyword] = useState("");
  const [wikiSearching, setWikiSearching] = useState(false);
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
  // LLM Provider 配置（Settings 面板）
  const [llmConfig, setLlmConfig] = useState<LlmProviderConfig | null>(null);
  const [llmConfigCloudApiKey, setLlmConfigCloudApiKey] = useState("");
  const [llmConfigCloudBaseUrl, setLlmConfigCloudBaseUrl] = useState("");
  const [llmConfigCloudModel, setLlmConfigCloudModel] = useState("");
  const [llmConfigCloudProviderName, setLlmConfigCloudProviderName] = useState("");
  const [llmConfigActiveProvider, setLlmConfigActiveProvider] = useState<"cloud" | "ollama">(
    "ollama",
  );
  const [llmConfigSaving, setLlmConfigSaving] = useState(false);
  // 当前激活的导航模块
  const [activeModule, setActiveModule] = useState<ModuleId>("inbox");

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [data, defaultPaths, querySettings, lintPatchEvents, llmConfigResult] =
        await Promise.all([
          loadAppData(),
          fetchDefaultPaths(),
          fetchQuerySettings(),
          fetchRecentLintPatchEvents(),
          fetchLlmConfig(),
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

        setRecentLintPatchEvents(lintPatchEvents);
        const lintFilterState = readLintFilterState();
        setLintSeverityFilter(lintFilterState.severity);
        setLintCodeKeyword(lintFilterState.codeKeyword);
        setLintPathKeyword(lintFilterState.pathKeyword);
        setLintSuggestionKeyword(lintFilterState.suggestionKeyword);
        setLintFilterStateLoaded(true);
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
    if (!statusMessage || !shouldAutoDismissStatusMessage(statusMessage)) {
      return;
    }

    const timerId = globalThis.setTimeout(() => {
      setStatusMessage("");
    }, 4500);

    return () => {
      globalThis.clearTimeout(timerId);
    };
  }, [statusMessage]);

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
    let unlisten: (() => void) | null = null;

    try {
      // 进度订阅失败不应阻塞查询执行，避免按钮持续处于“执行中”。
      try {
        unlisten = await listenProgress("query_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 query 进度事件失败，继续执行查询流程。", error);
      }

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
      if (unlisten) {
        unlisten();
      }
      setQueryRunning(false);
    }
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
          ? `LLM 配置已保存，当前使用 ${result.cloud_provider_name || "云端 Provider"}（${result.cloud_model || defaultCloudModel}）。`
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

  const handleResetWikiPages = async () => {
    setWikiKeyword("");
    await refreshAppData();
    setStatusMessage("已恢复显示最近 Wiki 页面。");
  };

  const handleOpenWikiPage = async (pagePath: string) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法查看 Wiki 页面内容。");
      return;
    }

    setWikiActivePagePath(pagePath);
    setWikiPageDetailLoading(true);
    setWikiPageCitationsLoading(true);
    setWikiPageDetailError("");
    setWikiPageCitationsError("");
    setWikiFrontmatterCopiedKey("");
    setWikiFrontmatterCollapsed(false);
    setWikiDebugInfoVisible(false);
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
      setWikiPageCitations(citations ?? []);
      if (citations === null) {
        setWikiPageCitationsError("当前环境不支持读取页面引用。");
      }
      setStatusMessage(`已打开页面：${detail.title}`);
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

  const handleCloseWikiPreview = () => {
    setWikiActivePagePath("");
    setWikiPageDetail(null);
    setWikiPageCitations([]);
    setWikiPageDetailError("");
    setWikiPageCitationsError("");
    setWikiFrontmatterCopiedKey("");
    setWikiFrontmatterCollapsed(false);
    setWikiDebugInfoVisible(false);
    setStatusMessage("已关闭页面预览。");
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
  const isActiveWikiDetailInList = Boolean(
    wikiActivePagePath
    && pages.some((page) => isSameWikiPagePath(page.path, wikiActivePagePath)),
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
          <button type="button" className="dev-panel__button" onClick={handleCloseWikiPreview}>
            关闭预览
          </button>
        </div>
      </div>
      {wikiFrontmatterDisplay.hasMeta ? (
        <div className="wiki-preview__meta">
          <div className="wiki-preview__meta-head">
            <h4>Frontmatter</h4>
            <div className="wiki-preview__meta-head-actions">
              <span>{wikiFrontmatterDisplay.totalCount} 项</span>
              <button
                type="button"
                className="dev-panel__button wiki-preview__meta-toggle"
                onClick={() => setWikiDebugInfoVisible((value) => !value)}
              >
                {wikiDebugInfoVisible ? "隐藏调试" : "调试信息"}
              </button>
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
      {wikiDebugInfoVisible ? (
        <div className="wiki-preview__debug">
          <div className="wiki-preview__debug-head">
            <h4>调试信息</h4>
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
      <pre className="wiki-preview__content">{wikiPageDetail?.content ?? ""}</pre>
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

        <div className="module-viewport">
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
                  <p className="dev-panel__hint">
                    {isTauriRuntime()
                      ? "按钮会调用本地 Tauri 命令，成功后自动刷新运行概览和最近日志。"
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
                  <span className="section-head__hint">{pages.length ? `最近 ${pages.length} 页` : "暂无页面"}</span>
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
                      placeholder="按标题、摘要、路径搜索"
                      spellCheck={false}
                    />
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
                {pages.length ? (
                  <div className="ask-result__citations">
                    {pages.map((page) => {
                      const isActiveCard = isSameWikiPagePath(page.path, wikiActivePagePath);
                      const isDetailForCard = Boolean(
                        wikiPageDetail && isSameWikiPagePath(page.path, wikiPageDetail.path),
                      );

                      return (
                        <article key={page.path} className="ask-citation">
                          <div className="ask-citation__top">
                            <code>{page.title}</code>
                            <span>{formatLintCheckedAt(page.updated_at)}</span>
                          </div>
                          <p>{page.summary}</p>
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
            <>
              <div className="module-header">
                <h1 className="module-header__title">Ask</h1>
                <p className="module-header__sub">基于索引与引用证据的 LLM 问答</p>
              </div>
              <section className="panel">
                <div className="section-head">
                  <h2>Ask 面板</h2>
                  <span className="section-head__hint">
                    {isTauriRuntime() ? "可调用 query_ask" : "浏览器预览"}
                  </span>
                </div>
                <div className="ask-panel">
                  <div className="dev-panel__field">
                    <label className="dev-panel__label" htmlFor="ask-question">问题</label>
                    <textarea
                      id="ask-question"
                      className="dev-panel__input ask-panel__textarea"
                      value={queryQuestion}
                      onChange={(event) => setQueryQuestion(event.target.value)}
                      placeholder="输入你要检索的问题"
                      rows={3}
                    />
                  </div>
                  <div className="dev-panel__field">
                    <label className="dev-panel__label" htmlFor="ask-top-k">
                      TopK（{queryTopKMin}–{queryTopKMax}）
                    </label>
                    <input
                      id="ask-top-k"
                      className="dev-panel__input"
                      type="number"
                      min={queryTopKMin}
                      max={queryTopKMax}
                      step={1}
                      value={queryTopK}
                      onChange={(event) => setQueryTopK(Number(event.target.value))}
                      style={{ width: "100px" }}
                    />
                  </div>
                  <div className="dev-panel__actions">
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
                      <span className="pill pill--info">模式：{formatBackendMode(queryResult.mode)}</span>
                      <span className="pill pill--lint">检索：{formatQuerySearchStrategyLabel(queryResult.search_strategy)}</span>
                      <span className="pill pill--lint">策略：{formatQueryAnswerStrategyLabel(queryResult.answer_strategy)}</span>
                      <span className="pill">TopK：{queryTopK}</span>
                      <span className="pill">命中：{queryResult.matched_pages.length}</span>
                    </div>
                    <pre className="ask-result__answer">{queryResult.answer}</pre>
                    <div className="ask-result__citations">
                      {queryResult.citations.map((citation) => (
                        <article key={`${citation.page_path}-${citation.score}`} className="ask-citation">
                          <div className="ask-citation__top">
                            <code>{resolveDisplayPath(citation)}</code>
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
                      ? '尚未执行 Query。输入问题后点击\u201c执行 Query\u201d查看本地检索结果。'
                      : "浏览器预览模式下不连接后端，无法生成真实问答结果。"}
                  </p>
                )}
              </section>
            </>
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
                      {filteredLintIssues.map((issue) => {
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
                    {lintPatchPreviewItems.map((item) => (
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
                        </div>
                      </article>
                    ))}
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
                    <button type="button" className="dev-panel__button" onClick={() => void handleApplyCloudPreset("deepseek")}>
                      DeepSeek 预设
                    </button>
                    <button type="button" className="dev-panel__button" onClick={() => void handleApplyCloudPreset("glm")}>
                      GLM 预设
                    </button>
                    <button type="button" className="dev-panel__button" onClick={() => void handleApplyCloudPreset("minimax")}>
                      MiniMax 预设
                    </button>
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
