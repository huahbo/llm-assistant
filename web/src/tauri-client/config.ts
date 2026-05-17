import type {
  AppOverview,
  BackendAppMode,
  DefaultPaths,
  EmbedStatus,
  LlmProviderConfig,
  LlmStatus,
  LogEntry,
  ModeChangeResult,
  SearchConfig,
  VaultInitResult,
  VaultStats,
} from "../types";
import { isTauriRuntime, withTimeout } from "./base";

// ── Raw response types (private) ─────────────────────────────────────────────

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
  ollama_model?: string | null;
  ollama_base_url?: string | null;
  embed_ollama_model?: string | null;
  embed_ollama_base_url?: string | null;
  embed_backend?: string | null;
  embed_onnx_model?: string | null;
};

// ── LlmStatusSummary interface ────────────────────────────────────────────────

export interface LlmStatusSummary {
  availabilityText: string;
  modelText: string;
  addressText: string;
  hintText: string;
}

// ── Internal helpers ──────────────────────────────────────────────────────────

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

const defaultSearchConfig: SearchConfig = {
  search_provider: "none",
  tavily_api_key: "",
  searxng_url: "http://localhost:8080",
  brave_api_key: "",
  breadth: 3,
  depth: 1,
};

// ── Args builders ─────────────────────────────────────────────────────────────

export const createVaultInitArgs = (vaultPath: string) => ({
  vaultPath,
  vault_path: vaultPath,
});

/** 构造 set_ocr_config 参数（用于测试） */
export const createSetOcrConfigArgs = (provider: string | null) => ({ provider });

// ── Normalize helpers ─────────────────────────────────────────────────────────

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
    ollama_model: source.ollama_model?.trim() ?? "",
    ollama_base_url: source.ollama_base_url?.trim() ?? "",
    embed_ollama_model: source.embed_ollama_model?.trim() ?? "",
    embed_ollama_base_url: source.embed_ollama_base_url?.trim() ?? "",
    embed_backend: source.embed_backend?.trim() ?? "onnx",
    embed_onnx_model: source.embed_onnx_model?.trim() ?? "multilingual-e5-small",
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

// ── App overview & defaults ───────────────────────────────────────────────────

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

// ── LLM config ────────────────────────────────────────────────────────────────

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

// ── OCR config ────────────────────────────────────────────────────────────────

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

// ── Backend mode ──────────────────────────────────────────────────────────────

export async function setBackendMode(mode: BackendAppMode): Promise<ModeChangeResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ModeChangeResult>("set_mode", { mode });
}

// ── Vault init ────────────────────────────────────────────────────────────────

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

// ── Search config ─────────────────────────────────────────────────────────────

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

// ── Vault stats & clip server ─────────────────────────────────────────────────

/** 获取 Vault 统计数据（页面数、引用数、摄入来源分布等）。非 Tauri 环境返回 null。 */
export async function getVaultStats(): Promise<VaultStats | null> {
  if (!isTauriRuntime()) { return null; }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<VaultStats>("get_vault_stats");
}

/** 获取 Web Clipper HTTP 服务状态字符串（如 "running:19827" 或 "stopped"）。非 Tauri 环境返回空字符串。 */
export async function getClipServerStatus(): Promise<string> {
  if (!isTauriRuntime()) return "";
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("get_clip_server_status");
}

// ── Embedding ─────────────────────────────────────────────────────────────────

/** 获取 Embedding 后端状态（backend_id、维度、已索引页数、健康状态）。 */
export async function getEmbedStatus(): Promise<EmbedStatus | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<EmbedStatus>("get_embed_status");
  } catch {
    return null;
  }
}

/** 触发全量重建向量索引。返回已处理页数，失败时抛出。 */
export async function rebuildEmbeddings(): Promise<number> {
  if (!isTauriRuntime()) return 0;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<number>("rebuild_embeddings");
}
