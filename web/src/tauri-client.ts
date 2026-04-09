import type {
  AppOverview,
  BackendAppMode,
  DefaultPaths,
  IngestResult,
  LintReport,
  LogEntry,
  ModeChangeResult,
  QueryAnswerResult,
  QueryAskOptions,
  VaultInitResult,
} from "./types";

export const isTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

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
