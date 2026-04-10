import type {
  AppOverview,
  BackendAppMode,
  DefaultPaths,
  IngestResult,
  LlmStatus,
  LintReport,
  LogEntry,
  ModeChangeResult,
  QueryAnswerResult,
  QueryAskOptions,
  QuerySettings,
  SaveQueryAnswerInput,
  SaveQueryAnswerResult,
  VaultInitResult,
  WikiPageDetail,
  WikiPageCitation,
  WikiPageItem,
} from "./types";

export const isTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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

export async function runLint(): Promise<LintReport | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke<LintReport>("run_lint"));
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
