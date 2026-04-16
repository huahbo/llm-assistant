import type {
  AppOverview,
  AskHistoryItem,
  BackendAppMode,
  DefaultPaths,
  IngestResult,
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
  ProgressPayload,
  QueryAnswerResult,
  QueryAskOptions,
  QuerySettings,
  SaveQueryAnswerInput,
  SaveQueryAnswerResult,
  DeleteWikiPageResult,
  RenameWikiPageResult,
  SaveWikiPageResult,
  VaultInitResult,
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

export async function fetchAppOverview(): Promise<AppOverview | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<AppOverview>("get_app_overview");
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
  return withTimeout(
    invoke<VaultInitResult>("init_vault", {
      ...createVaultInitArgs(vaultPath),
    }),
  );
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
  );
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
