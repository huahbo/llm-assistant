import type {
  AppOverview,
  AskHistoryItem,
  AskSessionItem,
  AskSessionSearchHitItem,
  AskSessionTurnItem,
  BackendAppMode,
  DefaultPaths,
  IngestPreview,
  IngestQueueItem,
  IngestResult,
  PageQuickLint,
  KnowledgeGraphData,
  KnowledgeSubgraphData,
  KnowledgeSubgraphRequestParams,
  LlmProviderConfig,
  LlmStatus,
  LintReport,
  LintPatchEvent,
  LintPatchBatchItemInput,
  LintPatchBatchItemResult,
  LintPatchBatchResult,
  LintPatchPreviewItem,
  LintPatchApplyResult,
  LogEntry,
  ModeChangeResult,
  OutboxEventItem,
  ProgressPayload,
  QueryAnswerResult,
  QueryAskOptions,
  QuerySettings,
  ResearchTaskItem,
  SaveQueryAnswerInput,
  SaveQueryAnswerResult,
  SearchConfig,
  DeleteWikiPageResult,
  RenameWikiPageResult,
  SaveWikiPageResult,
  VaultInitResult,
  VaultStats,
  NewPageResult,
  WikiPageDetail,
  WikiPageCitation,
  WikiPageItem,
} from "./types";

/** 订阅进度事件，返回取消订阅函数。非 Tauri 环境时为空操作。 */
export async function listenProgress(
  event: "ingest_progress" | "query_progress",
  handler: (payload: ProgressPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ProgressPayload>(event, (e) => handler(e.payload));
  return unlisten;
}

export const isTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** 打开文件选择对话框，返回选中路径数组（取消返回 null） */
export async function pickFiles(options: {
  multiple?: boolean;
  filters?: Array<{ name: string; extensions: string[] }>;
}): Promise<string[] | null> {
  if (!isTauriRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({
    multiple: options.multiple ?? false,
    filters: options.filters,
  });
  if (!result) return null;
  if (Array.isArray(result)) return result as string[];
  return [result as string];
}

/** 打开文件夹选择对话框，返回选中路径（取消返回 null） */
export async function pickFolder(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({ directory: true, multiple: false });
  if (!result) return null;
  return Array.isArray(result) ? (result[0] as string) : (result as string);
}

/** 打开保存文件对话框，返回保存路径（取消返回 null） */
export async function pickSaveFile(options: {
  defaultPath?: string;
  filters?: Array<{ name: string; extensions: string[] }>;
}): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const result = await save({
    defaultPath: options.defaultPath,
    filters: options.filters,
  });
  if (!result) return null;
  return result as string;
}

/** 打开确认对话框（Tauri 原生）。浏览器环境回退到 window.confirm。 */
export async function askConfirmDialog(
  message: string,
  options?: {
    title?: string;
    kind?: "info" | "warning" | "error";
    okLabel?: string;
    cancelLabel?: string;
  },
): Promise<boolean> {
  if (!isTauriRuntime()) {
    return globalThis.confirm(message);
  }
  const { confirm } = await import("@tauri-apps/plugin-dialog");
  return await confirm(message, {
    title: options?.title,
    kind: options?.kind,
    okLabel: options?.okLabel,
    cancelLabel: options?.cancelLabel,
  });
}

type DisplayPathSource = {
  display_path?: string | null;
  displayPath?: string | null;
  cited_page_display_path?: string | null;
  path?: string | null;
  page_path?: string | null;
  cited_page_path?: string | null;
  wiki_path?: string | null;
  source_path?: string | null;
  raw_path?: string | null;
};

export const resolveDisplayPath = (source: DisplayPathSource) => {
  // 优先展示后端返回的友好路径，缺失时回退到原始路径。
  const candidates = [
    source.display_path,
    source.displayPath,
    source.cited_page_display_path,
    source.path,
    source.page_path,
    source.cited_page_path,
    source.wiki_path,
    source.source_path,
    source.raw_path,
  ];

  for (const candidate of candidates) {
    const value = candidate?.trim();
    if (value) {
      return value;
    }
  }

  return "";
};

type RawLlmStatus = {
  available?: boolean;
  is_available?: boolean;
  healthy?: boolean;
  model?: string | null;
  model_name?: string | null;
  address?: string | null;
  base_url?: string | null;
  endpoint?: string | null;
  url?: string | null;
  message?: string | null;
  hint?: string | null;
  detail?: string | null;
};

type RawLlmProviderConfig = {
  cloud_api_key?: string | null;
  cloud_base_url?: string | null;
  cloud_model?: string | null;
  cloud_provider_name?: string | null;
  active_provider?: string | null;
  openai_api_key?: string | null;
  openai_base_url?: string | null;
  openai_model?: string | null;
  openai_provider_name?: string | null;
};

type RawLintPatchPreviewResponse =
  | LintPatchPreviewItem[]
  | {
      generated_at?: string | null;
      total?: number | null;
      suggestions?: LintPatchPreviewItem[] | null;
    };

type RawLintPatchApplyResponse =
  | LintPatchApplyResult
  | string
  | null
  | undefined;

type RawLintPatchBatchItemResponse = {
  issue_code?: string | null;
  path?: string | null;
  applied?: boolean | null;
  skipped?: boolean | null;
  status?: string | null;
  message?: string | null;
  touched_paths?: string[] | null;
  error?: string | null;
};

type RawLintPatchBatchEnvelope = {
  summary?: string | null;
  message?: string | null;
  total?: number | null;
  success?: number | null;
  failed?: number | null;
  skipped?: number | null;
  success_count?: number | null;
  failure_count?: number | null;
  skipped_count?: number | null;
  total_count?: number | null;
  applied_count?: number | null;
  results?: Array<RawLintPatchApplyResponse | RawLintPatchBatchItemResponse> | null;
  items?: Array<RawLintPatchApplyResponse | RawLintPatchBatchItemResponse> | null;
};

type RawLintPatchBatchResponse =
  | RawLintPatchBatchEnvelope
  | Array<RawLintPatchApplyResponse | RawLintPatchBatchItemResponse>
  | string
  | null
  | undefined;

type RawLintPatchEvent = {
  issue_code?: string | null;
  path?: string | null;
  applied?: boolean | null;
  message?: string | null;
  created_at?: string | null;
};

export interface LlmStatusSummary {
  availabilityText: string;
  modelText: string;
  addressText: string;
  hintText: string;
}

const pickFirstText = (...values: Array<string | null | undefined>) => {
  for (const candidate of values) {
    const value = candidate?.trim();
    if (value) {
      return value;
    }
  }

  return "";
};

const createUnavailableLlmStatus = (message: string): LlmStatus => ({
  available: false,
  model: "未知模型",
  address: "未知地址",
  message,
});

export const normalizeLlmStatus = (source: RawLlmStatus | null | undefined): LlmStatus | null => {
  if (!source) {
    return null;
  }

  const available =
    typeof source.available === "boolean"
      ? source.available
      : typeof source.is_available === "boolean"
        ? source.is_available
        : typeof source.healthy === "boolean"
          ? source.healthy
        : false;

  return {
    available,
    model: pickFirstText(source.model, source.model_name) || "未知模型",
    address: pickFirstText(source.address, source.base_url, source.endpoint, source.url) || "未知地址",
    message: pickFirstText(source.message, source.hint, source.detail),
  };
};

export const normalizeLlmProviderConfig = (
  source: RawLlmProviderConfig | null | undefined,
): LlmProviderConfig | null => {
  if (!source) {
    return null;
  }

  const cloudApiKey = pickFirstText(source.cloud_api_key, source.openai_api_key);
  const cloudBaseUrl = pickFirstText(source.cloud_base_url, source.openai_base_url);
  const cloudModel = pickFirstText(source.cloud_model, source.openai_model);
  const cloudProviderName = pickFirstText(
    source.cloud_provider_name,
    source.openai_provider_name,
  );

  const normalizedActiveProvider = source.active_provider?.trim();
  const activeProvider =
    normalizedActiveProvider === "openai"
      ? "cloud"
      : normalizedActiveProvider || (cloudApiKey ? "cloud" : "ollama");

  return {
    cloud_api_key: cloudApiKey,
    cloud_base_url: cloudBaseUrl,
    cloud_model: cloudModel,
    cloud_provider_name: cloudProviderName,
    active_provider: activeProvider,
    ollama_model: "",
    ollama_base_url: "",
    embed_ollama_model: "",
    embed_ollama_base_url: "",
  };
};

export const formatLlmStatusSummary = (status: LlmStatus | null): LlmStatusSummary => {
  if (!status) {
    return {
      availabilityText: "LLM 状态未读取",
      modelText: "未知模型",
      addressText: "未知地址",
      hintText: "浏览器预览模式下无法读取 LLM 状态。",
    };
  }

  return {
    availabilityText: status.available ? "LLM 可用" : "LLM 不可用",
    modelText: status.model.trim() || "未知模型",
    addressText: status.address.trim() || "未知地址",
    hintText:
      status.message.trim() ||
      (status.available
        ? "LLM 服务已就绪。"
        : "请检查 Ollama 地址、模型名称或云 Provider 配置。"),
  };
};

export const createVaultInitArgs = (vaultPath: string) => ({
  vaultPath,
  vault_path: vaultPath,
});

export const createIngestMarkdownArgs = (sourcePath: string) => ({
  sourcePath,
  source_path: sourcePath,
});

/** 构造 ingest_pdf 命令参数（用于测试） */
export const createIngestPdfArgs = (sourcePath: string) => ({
  sourcePath,
  source_path: sourcePath,
});

export type OcrProvider = "tesseract" | "paddle";

/** 构造 ingest_file 命令参数（用于测试） */
export const createIngestFileArgs = (sourcePath: string, ocrProvider?: OcrProvider) => {
  const providerArgs = ocrProvider
    ? {
        ocrProvider,
        ocr_provider: ocrProvider,
      }
    : {};

  return {
    sourcePath,
    source_path: sourcePath,
    ...providerArgs,
  };
};

/** 构造 ingest_url 命令参数（用于测试） */
export const createIngestUrlArgs = (url: string) => ({ url });

/** 构造 preview_ingest_file 命令参数（用于测试） */
export const createPreviewIngestFileArgs = (
  sourceType: string,
  sourcePath: string,
  ocrProvider?: OcrProvider,
) => {
  const providerArgs = ocrProvider
    ? {
        ocrProvider,
        ocr_provider: ocrProvider,
      }
    : {};

  return {
    sourceType,
    source_type: sourceType,
    sourcePath,
    source_path: sourcePath,
    ...providerArgs,
  };
};

/** 构造 apply_ingest_preview 命令参数（用于测试） */
export const createApplyIngestPreviewArgs = (previewId: string) => ({
  previewId,
  preview_id: previewId,
});

export const createQueryAskArgs = (question: string) => ({
  question,
});

export const createQueryAskWithOptionsArgs = (
  question: string,
  options?: QueryAskOptions,
) => ({
  question,
  options: options
    ? {
        topK: options.top_k,
        top_k: options.top_k,
      }
    : undefined,
});

export const createSetQueryTopKArgs = (topK: number) => ({
  topK,
  top_k: topK,
});

export const createSaveQueryAnswerArgs = (input: SaveQueryAnswerInput) => ({
  input: {
    question: input.question,
    answer: input.answer,
    citations: input.citations,
    title: input.title,
  },
});

export const createSaveWikiPageArgs = (path: string, content: string) => ({
  path,
  content,
});

export const createSearchWikiPagesArgs = (keyword: string) => ({
  keyword,
});

export const createWikiPageDetailArgs = (pagePath: string) => ({
  pagePath,
  page_path: pagePath,
});

export const createWikiPageCitationsArgs = (pagePath: string) => ({
  pagePath,
  page_path: pagePath,
});

export const createGetKnowledgeSubgraphArgs = (
  params: KnowledgeSubgraphRequestParams,
) => ({
  centerPagePath: params.centerPagePath,
  center_page_path: params.centerPagePath,
  hop: params.hop,
  direction: params.direction,
  limitNodes: params.limitNodes,
  limit_nodes: params.limitNodes,
  limitLinks: params.limitLinks,
  limit_links: params.limitLinks,
});

export const createPreviewLintPatchesArgs = () => ({});

export const createFetchRecentLintPatchEventsArgs = () => ({});

export const createApplyLintPatchArgs = (item: LintPatchPreviewItem) => ({
  issueCode: item.issue_code,
  issue_code: item.issue_code,
  path: item.path ?? null,
});

export const createApplyLintPatchesBatchArgs = (items: LintPatchBatchItemInput[]) => ({
  inputs: items.map((item) => ({
    issueCode: item.issue_code,
    issue_code: item.issue_code,
    path: item.path ?? null,
  })),
  // 兼容旧后端/测试桩，保留 items 字段。
  items: items.map((item) => ({
    issueCode: item.issue_code,
    issue_code: item.issue_code,
    path: item.path ?? null,
  })),
});

export const normalizeLintPatchPreviewResponse = (
  payload: RawLintPatchPreviewResponse | null | undefined,
): LintPatchPreviewItem[] => {
  if (!payload) {
    return [];
  }

  if (Array.isArray(payload)) {
    return payload;
  }

  if (Array.isArray(payload.suggestions)) {
    return payload.suggestions;
  }

  return [];
};

export const normalizeLintPatchApplyResponse = (
  payload: RawLintPatchApplyResponse,
): LintPatchApplyResult | null => {
  if (payload == null) {
    return null;
  }

  if (typeof payload === "string") {
    return { message: payload };
  }

  return {
    issue_code: payload.issue_code ?? undefined,
    path: payload.path ?? undefined,
    applied: payload.applied ?? undefined,
    message: payload.message ?? undefined,
    touched_paths: Array.isArray(payload.touched_paths) ? payload.touched_paths : undefined,
  };
};

const normalizeLintPatchBatchItemResponse = (
  payload: RawLintPatchApplyResponse | RawLintPatchBatchItemResponse,
): LintPatchBatchItemResult => {
  if (typeof payload === "string") {
    return {
      message: payload,
      skipped: true,
      applied: false,
    };
  }

  if (payload == null) {
    return {
      applied: false,
      skipped: true,
    };
  }

  if (typeof payload === "object" && "issue_code" in payload) {
    const batchItem = payload as RawLintPatchBatchItemResponse;
    const status = batchItem.status?.trim().toLowerCase();
    const inferredSkipped =
      typeof batchItem.skipped === "boolean"
        ? batchItem.skipped
        : status === "skipped";
    const inferredApplied =
      typeof batchItem.applied === "boolean"
        ? batchItem.applied
        : status === "success";

    return {
      issue_code: batchItem.issue_code ?? undefined,
      path: batchItem.path ?? undefined,
      applied: inferredApplied,
      skipped: inferredSkipped,
      status: batchItem.status ?? undefined,
      message: batchItem.message ?? undefined,
      touched_paths: Array.isArray(batchItem.touched_paths) ? batchItem.touched_paths : undefined,
      error: batchItem.error ?? undefined,
    };
  }

  return {
    ...(normalizeLintPatchApplyResponse(payload as RawLintPatchApplyResponse) ?? {}),
    skipped: false,
  };
};

export const normalizeLintPatchBatchResponse = (
  payload: RawLintPatchBatchResponse,
): LintPatchBatchResult | null => {
  if (payload == null) {
    return null;
  }

  if (typeof payload === "string") {
    return {
      summary: payload,
      success_count: 0,
      failure_count: 0,
      skipped_count: 0,
      total_count: 0,
      items: [],
    };
  }

  const envelope = !Array.isArray(payload) && typeof payload === "object" ? payload : null;
  const rawItems = Array.isArray(payload)
    ? payload
    : Array.isArray(envelope?.results)
      ? envelope.results
      : Array.isArray(envelope?.items)
        ? envelope.items
        : [];
  const items = rawItems.map((item) => normalizeLintPatchBatchItemResponse(item));

  const countFromEnvelope = (
    value: number | null | undefined,
    fallback: number,
  ) => {
    if (typeof value !== "number" || Number.isNaN(value)) {
      return fallback;
    }
    return value;
  };

  const success_count = envelope
    ? countFromEnvelope(
        envelope.success_count ?? envelope.applied_count ?? envelope.success,
        items.filter((item) => item.applied === true).length,
      )
    : items.filter((item) => item.applied === true).length;
  const failure_count = envelope
    ? countFromEnvelope(
        envelope.failure_count ?? envelope.failed,
        items.filter((item) => item.applied === false && item.skipped !== true).length,
      )
    : items.filter((item) => item.applied === false && item.skipped !== true).length;
  const skipped_count = envelope
    ? countFromEnvelope(
        envelope.skipped_count ?? envelope.skipped,
        items.filter((item) => item.skipped === true).length,
      )
    : items.filter((item) => item.skipped === true).length;
  const total_count = envelope
    ? countFromEnvelope(
        envelope.total_count ?? envelope.total,
        items.length,
      )
    : items.length;

  return {
    summary: envelope ? envelope.summary ?? envelope.message ?? undefined : undefined,
    success_count,
    failure_count,
    skipped_count,
    total_count,
    items,
  };
};

export const normalizeLintPatchEvents = (
  payload: RawLintPatchEvent[] | null | undefined,
): LintPatchEvent[] => {
  if (!Array.isArray(payload)) {
    return [];
  }

  return payload
    .map((item) => ({
      issue_code: item.issue_code?.trim() || "未知",
      path: item.path ?? null,
      applied: Boolean(item.applied),
      message: item.message?.trim() || "",
      created_at: item.created_at?.trim() || "",
    }))
    .filter((item) => item.created_at);
};

/** 摄入操作超时（5分钟）：PDF/文件需要 PDF提取 + LLM摘要 + 实体抽取，耗时长 */
const INGEST_TIMEOUT_MS = 300_000;

const withTimeout = async <T>(promise: Promise<T>, timeoutMs = 15000): Promise<T> => {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const timeoutPromise = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error("命令执行超时，请检查后端日志")), timeoutMs);
  });

  try {
    return await Promise.race([promise, timeoutPromise]);
  } finally {
    if (timer) {
      clearTimeout(timer);
    }
  }
};

export async function getAppOverview(): Promise<AppOverview | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AppOverview>("get_app_overview");
}

export async function getLlmProviderPresets(): Promise<[string, string, string][]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<[string, string, string][]>("get_llm_provider_presets");
}


export async function fetchDefaultPaths(): Promise<DefaultPaths | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<DefaultPaths>("get_default_paths");
}

export async function fetchRecentLogs(): Promise<LogEntry[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<LogEntry[]>("get_recent_logs");
}

export async function fetchRecentWikiPages(): Promise<WikiPageItem[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await invoke<WikiPageItem[]>("get_recent_wiki_pages");
  } catch {
    return [];
  }
}

export async function fetchLlmStatus(): Promise<LlmStatus | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    const result = await withTimeout(invoke<RawLlmStatus | null>("get_llm_status"));
    const normalized = normalizeLlmStatus(result);
    return normalized ?? createUnavailableLlmStatus("LLM 状态不可用。");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return createUnavailableLlmStatus(`LLM 状态读取失败：${message}`);
  }
}

export async function searchWikiPages(keyword: string): Promise<WikiPageItem[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<WikiPageItem[]>("search_wiki_pages", {
        ...createSearchWikiPagesArgs(keyword),
      }),
    );
  } catch {
    return [];
  }
}

/** 搜索所有 wiki 页面路径（用于链接自动补全） */
export async function searchWikiPaths(query: string): Promise<string[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<string[]>("search_wiki_paths", {
        query,
      }),
    );
  } catch {
    return [];
  }
}

export async function fetchWikiPageDetail(pagePath: string): Promise<WikiPageDetail | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<WikiPageDetail>("get_wiki_page_detail", {
      ...createWikiPageDetailArgs(pagePath),
    }),
  );
}

export async function fetchWikiPageCitations(pagePath: string): Promise<WikiPageCitation[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<WikiPageCitation[]>("get_wiki_page_citations", {
        ...createWikiPageCitationsArgs(pagePath),
      }),
    );
  } catch {
    return null;
  }
}

export async function fetchQuerySettings(): Promise<QuerySettings | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await invoke<QuerySettings>("get_query_settings");
  } catch {
    return null;
  }
}

export async function setBackendMode(mode: BackendAppMode): Promise<ModeChangeResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ModeChangeResult>("set_mode", { mode });
}

export async function initVault(vaultPath: string): Promise<VaultInitResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<VaultInitResult>("init_vault", { vaultPath });
}

export async function initVaultWithTemplate(
  vaultPath: string,
  templateSchema: string,
  templatePurpose: string,
  extraDirs: string[],
): Promise<VaultInitResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<VaultInitResult>("init_vault_with_template", {
    vaultPath,
    templateSchema,
    templatePurpose,
    extraDirs,
  });
}

export async function ingestMarkdown(sourcePath: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_markdown", {
      ...createIngestMarkdownArgs(sourcePath),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function ingestPdf(sourcePath: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_pdf", {
      ...createIngestPdfArgs(sourcePath),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function ingestFile(
  sourcePath: string,
  ocrProvider?: OcrProvider,
): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_file", {
      ...createIngestFileArgs(sourcePath, ocrProvider),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function previewIngestFile(
  sourceType: string,
  sourcePath: string,
  ocrProvider?: OcrProvider,
): Promise<IngestPreview | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestPreview>("preview_ingest_file", {
      ...createPreviewIngestFileArgs(sourceType, sourcePath, ocrProvider),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function applyIngestPreview(previewId: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("apply_ingest_preview", {
      ...createApplyIngestPreviewArgs(previewId),
    }),
    INGEST_TIMEOUT_MS,
  );
}

/** 对单个 Wiki 页面执行快速结构检查（无 LLM，仅文件系统） */
export async function quickLintPage(
  wikiPath: string,
): Promise<PageQuickLint | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<PageQuickLint>("quick_lint_page", { wikiPath });
}

/** 通过 URL 摄入网页内容，与 ingestMarkdown 共用返回结构 */
export async function ingestUrl(url: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_url", {
      ...createIngestUrlArgs(url),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function runLint(): Promise<LintReport | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke<LintReport>("run_lint"));
}

export async function previewLintPatches(): Promise<LintPatchPreviewItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    const result = await withTimeout(
      invoke<RawLintPatchPreviewResponse>("preview_lint_patches", {
        ...createPreviewLintPatchesArgs(),
      }),
    );
    return normalizeLintPatchPreviewResponse(result);
  } catch {
    return null;
  }
}

export async function fetchRecentLintPatchEvents(): Promise<LintPatchEvent[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    const result = await withTimeout(
      invoke<RawLintPatchEvent[]>("get_recent_lint_patch_events", {
        ...createFetchRecentLintPatchEventsArgs(),
      }),
    );
    return normalizeLintPatchEvents(result);
  } catch {
    return [];
  }
}

export async function applyLintPatch(
  item: LintPatchPreviewItem,
): Promise<LintPatchApplyResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  const result = await withTimeout(
    invoke<RawLintPatchApplyResponse>("apply_lint_patch", {
      ...createApplyLintPatchArgs(item),
    }),
  );

  return normalizeLintPatchApplyResponse(result);
}

export async function applyLintPatchesBatch(
  items: LintPatchBatchItemInput[],
): Promise<LintPatchBatchResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  if (!items.length) {
    return {
      summary: "暂无可执行项。",
      success_count: 0,
      failure_count: 0,
      skipped_count: 0,
      total_count: 0,
      items: [],
    };
  }

  const { invoke } = await import("@tauri-apps/api/core");
  const result = await withTimeout(
    invoke<RawLintPatchBatchResponse>("apply_lint_patches_batch", {
      ...createApplyLintPatchesBatchArgs(items),
    }),
  );

  return normalizeLintPatchBatchResponse(result);
}

export async function queryAsk(question: string): Promise<QueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<QueryAnswerResult>("query_ask", {
      ...createQueryAskArgs(question),
    }),
  );
}

export async function queryAskWithOptions(
  question: string,
  options?: QueryAskOptions,
): Promise<QueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<QueryAnswerResult>("query_ask_with_options", {
        ...createQueryAskWithOptionsArgs(question, options),
      }),
    );
  } catch {
    // 兼容旧后端：当新命令不可用时回退到 query_ask。
    return queryAsk(question);
  }
}

/** 多轮会话问答（携带 session_id，后端维护历史上下文） */
export async function queryAskSession(
  sessionId: string,
  question: string,
  options?: QueryAskOptions,
): Promise<QueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await invoke<QueryAnswerResult>("query_ask_session", {
      sessionId,
      question,
      options: options ?? null,
    });
  } catch {
    return null;
  }
}

/** 取消正在进行的会话查询 */
export async function cancelAskSession(sessionId: string): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    await invoke("cancel_ask_session", { sessionId });
  } catch {
    // 忽略错误
  }
}

/** 清空会话历史（开启新对话） */
export async function clearAskSession(sessionId: string): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    await invoke("clear_ask_session", { sessionId });
  } catch {
    // 忽略错误
  }
}

/** 创建 Ask 会话（若已存在则刷新更新时间） */
export async function createAskSession(
  sessionId: string,
  title?: string,
): Promise<AskSessionItem | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionItem>("create_ask_session", {
      sessionId,
      title: title ?? null,
    });
  } catch {
    return null;
  }
}

/** 读取 Ask 会话列表 */
export async function listAskSessions(limit = 50): Promise<AskSessionItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionItem[]>("list_ask_sessions", { limit });
  } catch {
    return null;
  }
}

/** 读取指定 Ask 会话轮次（正序） */
export async function fetchAskSessionTurns(
  sessionId: string,
  limit = 400,
): Promise<AskSessionTurnItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionTurnItem[]>("get_ask_session_turns", {
      sessionId,
      limit,
    });
  } catch {
    return null;
  }
}

/** 跨会话检索 Ask 轮次 */
export async function searchAskSessionTurns(
  keyword: string,
  limit = 50,
): Promise<AskSessionSearchHitItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskSessionSearchHitItem[]>("search_ask_session_turns", {
      keyword,
      limit,
    });
  } catch {
    return null;
  }
}

/** 重命名 Ask 会话 */
export async function renameAskSession(sessionId: string, title: string): Promise<boolean> {
  if (!isTauriRuntime()) {
    return false;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke("rename_ask_session", { sessionId, title });
    return true;
  } catch {
    return false;
  }
}

/** 删除 Ask 会话 */
export async function deleteAskSession(sessionId: string): Promise<boolean> {
  if (!isTauriRuntime()) {
    return false;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke("delete_ask_session", { sessionId });
    return true;
  } catch {
    return false;
  }
}

export async function setQueryTopK(topK: number): Promise<QuerySettings | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<QuerySettings>("set_query_top_k", {
        ...createSetQueryTopKArgs(topK),
      }),
    );
  } catch {
    return null;
  }
}

export async function saveQueryAnswer(
  input: SaveQueryAnswerInput,
): Promise<SaveQueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<SaveQueryAnswerResult>("save_query_answer", {
        ...createSaveQueryAnswerArgs(input),
      }),
    );
  } catch {
    return null;
  }
}

export async function saveWikiPage(
  path: string,
  content: string,
): Promise<SaveWikiPageResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<SaveWikiPageResult>("save_wiki_page", {
      ...createSaveWikiPageArgs(path, content),
    }),
  );
}

export async function saveAskHistory(question: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke("save_ask_history", { question });
  } catch {
    // 历史保存失败不阻断主流程
  }
}

export async function fetchAskHistory(limit = 30): Promise<AskHistoryItem[] | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AskHistoryItem[]>("get_ask_history", { limit });
  } catch {
    return null;
  }
}

export async function clearAskHistory(): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke<number>("clear_ask_history");
    return true;
  } catch {
    return false;
  }
}

export async function renameWikiPage(
  oldPath: string,
  newName: string,
): Promise<RenameWikiPageResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<RenameWikiPageResult>("rename_wiki_page", { oldPath, newName }),
  );
}

export async function deleteWikiPage(path: string): Promise<DeleteWikiPageResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke<DeleteWikiPageResult>("delete_wiki_page", { path }));
}

/** 读取 LLM Provider 配置（Settings 页面初始化时调用） */
export async function fetchLlmConfig(): Promise<LlmProviderConfig | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    const result = await invoke<RawLlmProviderConfig | null>("get_llm_config");
    return normalizeLlmProviderConfig(result);
  } catch {
    return null;
  }
}

/** 保存 LLM Provider 配置（Settings 页面点击保存时调用） */
export async function saveLlmConfig(
  config: LlmProviderConfig,
): Promise<LlmProviderConfig | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<LlmProviderConfig>("set_llm_config", { config }),
    );
  } catch {
    return null;
  }
}

/** 从后端读取默认 OCR provider（null 表示未配置） */
export const fetchOcrConfig = async (): Promise<string | null> => {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<string | null>("get_ocr_config");
  } catch {
    return null;
  }
};

/** 保存默认 OCR provider 到后端配置文件 */
export const saveOcrConfig = async (provider: string | null): Promise<void> => {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke<void>("set_ocr_config", { provider });
  } catch (e) {
    console.warn("保存 OCR 配置失败：", e);
  }
};

/** 构造 set_ocr_config 参数（用于测试） */
export const createSetOcrConfigArgs = (provider: string | null) => ({ provider });

/** 设置或取消 Wiki 页面的 stale 标记 */
export async function markPageStale(
  pagePath: string,
  stale: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("mark_page_stale", { pagePath, stale });
}

/** 获取知识图谱数据（节点 = wiki 页面，边 = 引用关系） */
export async function getKnowledgeGraph(): Promise<KnowledgeGraphData | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return await invoke<KnowledgeGraphData>("get_knowledge_graph");
}

/** 获取知识子图（以中心页面为起点，按 hop/方向裁剪） */
export async function getKnowledgeSubgraph(
  params: KnowledgeSubgraphRequestParams,
): Promise<KnowledgeSubgraphData | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return await withTimeout(
    invoke<KnowledgeSubgraphData>("get_knowledge_subgraph", {
      ...createGetKnowledgeSubgraphArgs(params),
    }),
  );
}

/** 获取 Outbox 事件（增量轮询） */
export async function get_outbox_events(options: {
  last_id?: number;
  limit?: number;
}): Promise<OutboxEventItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<OutboxEventItem[]>("get_outbox_events", {
      lastId: options.last_id,
      last_id: options.last_id,
      limit: options.limit,
    });
  } catch {
    return [];
  }
}

export async function enqueueIngest(sourceType: string, sourcePath: string): Promise<number> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<number>("enqueue_ingest", { sourceType, sourcePath });
}

/**
 * 获取页面对之间的 embedding 余弦相似度。
 * key 格式："pathA||pathB"（路径顺序无关，内部已规范化）。
 * 非 Tauri 环境或后端命令不可用时返回空对象（静默降级）。
 */
export async function getPageEmbeddingPairs(paths: string[]): Promise<Record<string, number>> {
  if (!isTauriRuntime()) return {};
  if (paths.length === 0) return {};
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    const result = await invoke<Record<string, number>>("get_page_embedding_similarities", { paths });
    return result ?? {};
  } catch {
    // 后端命令尚未实现时静默降级，不影响前端已有功能。
    return {};
  }
}

export async function listIngestQueue(): Promise<IngestQueueItem[]> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<IngestQueueItem[]>("list_ingest_queue");
}

export async function cancelIngestItem(id: number): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("cancel_ingest_item", { id });
}

export async function retryIngestItem(id: number): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("retry_ingest_item", { id });
}

// ---- Deep Research API ----

/** 研究进度事件载荷 */
export interface ResearchProgressPayload {
  task_id: number;
  stage: string;
  message: string;
}

/** 研究完成事件载荷 */
export interface ResearchDonePayload {
  task_id: number;
  saved_path: string;
}

/** 研究错误事件载荷 */
export interface ResearchErrorPayload {
  task_id: number;
  error: string;
}

/** 监听研究进度事件（research_progress / research_done / research_error）。非 Tauri 环境为空操作。 */
export async function listenResearchProgress(
  handler: (payload: ResearchProgressPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchProgressPayload>("research_progress", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchDone(
  handler: (payload: ResearchDonePayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchDonePayload>("research_done", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchError(
  handler: (payload: ResearchErrorPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchErrorPayload>("research_error", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

/** 子查询就绪事件载荷（用于用户审批） */
export interface ResearchQueriesReadyPayload {
  task_id: number;
  queries: string[];
}

/** 综合报告流式块载荷 */
export interface ResearchStreamChunkPayload {
  task_id: number;
  chunk: string;
}

export async function listenResearchQueriesReady(
  handler: (payload: ResearchQueriesReadyPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchQueriesReadyPayload>("research_queries_ready", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function listenResearchStreamChunk(
  handler: (payload: ResearchStreamChunkPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ResearchStreamChunkPayload>("research_stream_chunk", (e) =>
    handler(e.payload),
  );
  return unlisten;
}

export async function approveResearchQueries(taskId: number, queries: string[]): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke("approve_research_queries", { taskId, queries });
}

/** 启动研究任务，返回 task_id。非 Tauri 环境返回 -1。 */
export async function startResearch(
  topic: string,
  depth: number,
  breadth: number,
): Promise<number> {
  if (!isTauriRuntime()) return -1;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<number>("start_research", { topic, depth, breadth });
}

/** 列出历史研究任务（最多 100 条，倒序）。非 Tauri 环境返回 []。 */
export async function listResearchTasks(): Promise<ResearchTaskItem[]> {
  if (!isTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<ResearchTaskItem[]>("list_research_tasks");
  } catch {
    return [];
  }
}

/** 获取单条研究任务的最新状态。非 Tauri 环境返回 null。 */
export async function getResearchTask(id: number): Promise<ResearchTaskItem | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<ResearchTaskItem | null>("get_research_task", { id });
  } catch {
    return null;
  }
}

/** 取消研究任务（queued 状态）。非 Tauri 环境静默忽略。 */
export async function cancelResearchTask(id: number): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke<void>("cancel_research_task", { id });
  } catch {
    // 静默忽略
  }
}

/** 删除研究任务；可选同时删除关联的 Wiki 页面。 */
export async function deleteResearchTask(
  id: number,
  deleteSavedWiki: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("delete_research_task", {
    payload: {
      id,
      deleteSavedWiki,
      delete_saved_wiki: deleteSavedWiki,
    },
  });
}

const defaultSearchConfig: SearchConfig = {
  search_provider: "none",
  tavily_api_key: "",
  searxng_url: "http://localhost:8080",
  breadth: 3,
  depth: 1,
};

/** 获取搜索配置。非 Tauri 环境返回默认值。 */
export async function getSearchConfig(): Promise<SearchConfig> {
  if (!isTauriRuntime()) return { ...defaultSearchConfig };
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<SearchConfig>("get_search_config");
  } catch {
    return { ...defaultSearchConfig };
  }
}

/** 保存搜索配置。非 Tauri 环境静默忽略。 */
export async function setSearchConfig(config: SearchConfig): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke<void>("set_search_config", { config });
  } catch {
    // 静默忽略
  }
}

/** 保存 Deep Research 导出文件（.md 或其他格式）到用户指定路径。 */
export async function saveResearchDoc(path: string, content: string): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("save_research_doc", { path, content });
}

/** 获取 Web Clipper HTTP 服务状态字符串（如 "running:19827" 或 "stopped"）。非 Tauri 环境返回空字符串。 */
export async function getClipServerStatus(): Promise<string> {
  if (!isTauriRuntime()) return "";
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("get_clip_server_status");
}

/** 获取 Vault 统计数据（页面数、引用数、摄入来源分布等）。非 Tauri 环境返回 null。 */
export async function getVaultStats(): Promise<VaultStats | null> {
  if (!isTauriRuntime()) { return null; }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<VaultStats>("get_vault_stats");
}

/** AI 辅助新建 Wiki 页面。非 Tauri 环境抛出错误。超时 90 秒。 */
export async function createWikiPageWithAi(topic: string): Promise<NewPageResult> {
  if (!isTauriRuntime()) {
    throw new Error("仅在 Tauri 环境下可用");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<NewPageResult>("create_wiki_page_with_ai", { topic }),
    90_000,
  );
}
