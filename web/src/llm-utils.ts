// ── LLM provider configuration utilities ─────────────────────────────────────

export const defaultCloudProviderName = "DeepSeek";
export const defaultCloudBaseUrl = "https://api.deepseek.com/v1";
export const defaultCloudModel = "deepseek-v4-flash";

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
    model: "deepseek-v4-flash",
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

  return buildLlmProviderConfig({
    activeProvider,
    cloudApiKey: existingApiKey,
    cloudBaseUrl: preset.baseUrl,
    cloudModel: preset.model,
    cloudProviderName: preset.providerName,
  });
};

const answerStrategyLabels: Record<string, string> = {
  llm: "LLM 合成",
  rule: "规则回退",
  llm_synthesis: "LLM 合成",
  rule_fallback: "规则回退",
};

const searchStrategyLabels: Record<string, string> = {
  fts: "FTS 检索",
  scan: "回退扫描",
  empty: "空结果",
  rrf: "RRF 融合检索",
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
