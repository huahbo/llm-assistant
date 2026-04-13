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

    // 订阅后端进度事件，实时更新状态栏
    const unlisten = await listenProgress("ingest_progress", (payload) => {
      setStatusMessage(payload.message);
    });

    try {
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
      unlisten();
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

    // 订阅后端进度事件，实时更新状态栏
    const unlisten = await listenProgress("query_progress", (payload) => {
      setStatusMessage(payload.message);
    });

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
      unlisten();
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

    setWikiPageDetailLoading(true);
    setWikiPageCitationsLoading(true);
    setWikiPageDetailError("");
    setWikiPageCitationsError("");
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
    setWikiPageDetail(null);
    setWikiPageCitations([]);
    setWikiPageDetailError("");
    setWikiPageCitationsError("");
    setStatusMessage("已关闭页面预览。");
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
        <div className="runtime-banner" aria-label="LLM 状态">
          <div className="runtime-banner__item">
            <span>LLM</span>
            <strong>{llmAvailabilityText}</strong>
          </div>
          <div className="runtime-banner__item">
            <span>模型</span>
            <strong>{llmModelText}</strong>
          </div>
          <div className="runtime-banner__item">
            <span>地址</span>
            <strong>{llmAddressText}</strong>
          </div>
          <div className="runtime-banner__item">
            <span>提示</span>
            <strong>{llmHintText}</strong>
          </div>
        </div>
        {statusMessage ? <p className="runtime-status">{statusMessage}</p> : null}
        <div className="mode-selector">
          <label className="mode-selector__label" htmlFor="runtime-mode-selector">
            运行策略选择器
          </label>
          <div className="mode-selector__control">
            <select
              id="runtime-mode-selector"
              className="mode-selector__select"
              value={overview ? backendModeToModeId[overview.mode] : "hybrid"}
              onChange={(event) => void handleModeSelect(event.target.value as ModeId)}
              disabled={!isTauriRuntime() || !overview || switchingMode !== null}
            >
              <option value="hybrid">{modeIdLabels.hybrid}</option>
              <option value="strict-local">{modeIdLabels["strict-local"]}</option>
            </select>
            {switchingMode ? <span className="mode-selector__status">切换中...</span> : null}
          </div>
          <p className="mode-selector__hint">
            {overview
              ? modeIdDescriptions[backendModeToModeId[overview.mode]]
              : "浏览器预览模式下不可切换运行策略。"}
          </p>
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
              <span>检索策略：{formatQuerySearchStrategyLabel(queryResult.search_strategy)}</span>
              <span>策略：{formatQueryAnswerStrategyLabel(queryResult.answer_strategy)}</span>
              <span>TopK：{queryTopK}</span>
              <span>命中：{queryResult.matched_pages.length}</span>
              <span>时间：{formatLintCheckedAt(queryResult.checked_at)}</span>
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
        <div className="dev-panel">
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="lint-code-keyword">
              code 关键词
            </label>
            <input
              id="lint-code-keyword"
              className="dev-panel__input"
              type="text"
              value={lintCodeKeyword}
              onChange={(event) => setLintCodeKeyword(event.target.value)}
              placeholder="按 code 关键词筛选"
              spellCheck={false}
            />
          </div>
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="lint-path-keyword">
              path 关键词
            </label>
            <input
              id="lint-path-keyword"
              className="dev-panel__input"
              type="text"
              value={lintPathKeyword}
              onChange={(event) => setLintPathKeyword(event.target.value)}
              placeholder="按 path 关键词筛选"
              spellCheck={false}
            />
          </div>
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="lint-suggestion-keyword">
              suggestion 关键词
            </label>
            <input
              id="lint-suggestion-keyword"
              className="dev-panel__input"
              type="text"
              value={lintSuggestionKeyword}
              onChange={(event) => setLintSuggestionKeyword(event.target.value)}
              placeholder="按 suggestion 关键词筛选"
              spellCheck={false}
            />
          </div>
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
          <div className="runtime-banner__item">
            <span>严重级别</span>
            <strong>
              {`错误 ${lintSeverityStats.error} · 警告 ${lintSeverityStats.warning} · 信息 ${lintSeverityStats.info}`}
            </strong>
          </div>
        </div>
        {lintReport ? (
          <div className="lint-panel__actions" aria-label="lint 严重级别筛选">
            {(["all", "error", "warning", "info"] as LintSeverityFilter[]).map((severity) => {
              const active = lintSeverityFilter === severity;
              const count =
                severity === "all"
                  ? lintIssues.length
                  : severity === "error"
                    ? lintSeverityStats.error
                    : severity === "warning"
                      ? lintSeverityStats.warning
                      : lintSeverityStats.info;

              return (
                <button
                  key={severity}
                  type="button"
                  className={`dev-panel__button ${active ? "dev-panel__button--accent" : ""}`}
                  onClick={() => setLintSeverityFilter(severity)}
                >
                  {lintSeverityFilterLabels[severity]} ({count})
                </button>
              );
            })}
          </div>
        ) : null}
        {lintReport ? (
          filteredLintIssues.length ? (
            <div className="lint-issue-list">
              {filteredLintIssues.map((issue) => {
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
            <p className="empty-state">{lintFilterEmptyText}</p>
          )
        ) : (
          <p className="empty-state">
            {isTauriRuntime()
              ? "尚未运行 Lint。点击按钮后会在此展示报告摘要、检查时间和问题列表。"
              : "浏览器预览模式下不连接后端，无法生成真实 lint 报告。"}
          </p>
        )}
        <div className="dev-panel lint-patch-history">
          <div className="section-head">
            <h3>最近补丁应用记录</h3>
            <span className="section-head__hint">
              {recentLintPatchEvents.length ? `最近 ${recentLintPatchEvents.length} 条` : "暂无记录"}
            </span>
          </div>
          {recentLintPatchEvents.length ? (
            <div className="lint-patch-history__list">
              {recentLintPatchEvents.map((event) => (
                <article
                  key={`${event.issue_code}-${event.path ?? "global"}-${event.created_at}`}
                  className={`lint-patch-history__item ${
                    event.applied ? "lint-patch-history__item--applied" : "lint-patch-history__item--skipped"
                  }`}
                >
                  <div className="lint-patch-history__meta">
                    <span className="lint-patch-history__code">{event.issue_code}</span>
                    <span className={`pill ${event.applied ? "pill--ok" : "pill--danger"}`}>
                      {event.applied ? "已应用" : "未应用"}
                    </span>
                  </div>
                  <div className="lint-patch-history__field">
                    <span>path</span>
                    <code>{event.path ?? "全局"}</code>
                  </div>
                  <div className="lint-patch-history__field">
                    <span>message</span>
                    <p className="lint-patch-history__message">{event.message || "无"}</p>
                  </div>
                  <div className="lint-patch-history__field">
                    <span>created_at</span>
                    <time dateTime={event.created_at}>{formatLintCheckedAt(event.created_at)}</time>
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
        </div>
        <div className="dev-panel">
          <div className="section-head">
            <h3>补丁建议</h3>
            <span className="section-head__hint">
              {lintPatchPreviewItems.length ? `最近 ${lintPatchPreviewItems.length} 项` : "暂无建议"}
            </span>
          </div>
          <div className="lint-patch-panel__actions">
            <button
              type="button"
              className="dev-panel__button dev-panel__button--accent"
              onClick={() => void handleApplyLintPatchesBatch()}
              disabled={!isTauriRuntime() || lintPatchBatchApplying || lintPatchPreviewItems.length === 0}
            >
              {lintPatchBatchApplying ? "批量应用中..." : "批量应用可应用项"}
            </button>
            {lintPatchBatchSummary ? (
              <p className="lint-patch-panel__summary">
                {lintPatchBatchSummary.summary?.trim() ||
                  `成功 ${lintPatchBatchSummary.success_count} · 失败 ${lintPatchBatchSummary.failure_count} · 跳过 ${lintPatchBatchSummary.skipped_count}`}
              </p>
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
              {lintReport ? "点击“生成补丁建议”后在此查看候选补丁预览。" : "请先运行 Lint，再生成补丁建议。"}
            </p>
          )}
        </div>
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
          <h2>Wiki 页面</h2>
          <span className="section-head__hint">{pages.length ? `最近 ${pages.length} 页` : "暂无页面"}</span>
        </div>
        <div className="dev-panel">
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="wiki-keyword">
              关键字
            </label>
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
            {pages.map((page) => (
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
                    onClick={() => void handleOpenWikiPage(page.path)}
                    disabled={!isTauriRuntime() || wikiPageDetailLoading}
                  >
                    查看内容
                  </button>
                </div>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-state">
            {isTauriRuntime()
              ? "当前没有可展示的 wiki 页面。先执行示例摄入或保存 Query 结果。"
              : "浏览器预览模式下不加载后端 wiki 页面列表。"}
          </p>
        )}
        {wikiPageDetail ? (
          <article className="wiki-preview">
            <div className="wiki-preview__head">
              <div className="wiki-preview__title">
                <h3>{wikiPageDetail.title}</h3>
                <p>
                  <code>{resolveDisplayPath(wikiPageDetail)}</code>
                </p>
              </div>
              <div className="wiki-preview__actions">
                <span>{formatLintCheckedAt(wikiPageDetail.updated_at)}</span>
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={handleCloseWikiPreview}
                >
                  关闭预览
                </button>
              </div>
            </div>
            <pre className="wiki-preview__content">{wikiPageDetail.content}</pre>
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
                      <div className="wiki-citation__meta">
                        <span>score: {citation.score}</span>
                      </div>
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
        ) : wikiPageDetailError ? (
          <p className="runtime-status">{wikiPageDetailError}</p>
        ) : wikiPageDetailLoading ? (
          <p className="runtime-hint">正在读取页面内容...</p>
        ) : null}
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

      <section className="panel">
        <div className="section-head">
          <h2>Settings</h2>
          <span className="section-head__hint">
            {isTauriRuntime() ? "模式、Provider 与本地配置" : "浏览器预览"}
          </span>
        </div>
        <div className="dev-panel settings-panel">
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
          <div className="dev-panel__actions settings-panel__presets">
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void handleApplyCloudPreset("deepseek")}
            >
              DeepSeek 预设
            </button>
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void handleApplyCloudPreset("glm")}
            >
              GLM 预设
            </button>
            <button
              type="button"
              className="dev-panel__button"
              onClick={() => void handleApplyCloudPreset("minimax")}
            >
              MiniMax 预设
            </button>
          </div>
          <div className="settings-panel__fields">
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="active-provider">
                活跃 Provider
              </label>
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
              <label className="dev-panel__label" htmlFor="cloud-provider-name">
                云端 Provider 名称
              </label>
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
              <label className="dev-panel__label" htmlFor="cloud-api-key">
                云端 API Key（OpenAI-compatible）
              </label>
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
              <label className="dev-panel__label" htmlFor="cloud-base-url">
                云端 Base URL（OpenAI-compatible）
              </label>
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
              <label className="dev-panel__label" htmlFor="cloud-model">
                云端模型名
              </label>
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
          <div className="dev-panel__actions settings-panel__save">
            <button
              type="button"
              className="dev-panel__button dev-panel__button--accent"
              onClick={() => void handleSaveLlmConfig()}
              disabled={!isTauriRuntime() || llmConfigSaving}
            >
              {llmConfigSaving ? "保存中..." : "保存 LLM 配置"}
            </button>
          </div>
          <p className="dev-panel__hint settings-panel__hint">
            {isTauriRuntime()
              ? "云端配置仅保存在本地配置文件中，不会提交到仓库。可用 DeepSeek、GLM、MiniMax 三家预设，也可自由编辑为任意 OpenAI-compatible Provider。StrictLocal 模式下云 Provider 将被忽略。"
              : "浏览器预览模式下无法保存配置。"}
          </p>
        </div>
      </section>
    </main>
  );
}
