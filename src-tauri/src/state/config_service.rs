use super::AppState;
use crate::{
    db,
    llm::{
        LlmError, LlmProvider, OllamaConfig, OllamaProvider, OpenAiConfig, OpenAiProvider,
        DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL,
    },
    models::{
        AppConfig, AppMode, AppOverview, DefaultPaths, LogEntry, LogLevel, LlmProviderConfig,
        LlmStatus, ModeChangeResult, QuerySettings, ShellPolicyConfig, VaultInitResult,
    },
    vault,
};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

// ─── LLM 状态 helper ─────────────────────────────────────────────────────────

pub(super) fn build_llm_status(
    provider: &str,
    base_url: &str,
    model: &str,
    mode: AppMode,
    healthy: bool,
    message: String,
) -> LlmStatus {
    LlmStatus {
        provider: provider.to_string(),
        base_url: base_url.to_string(),
        model: model.to_string(),
        healthy,
        message,
        mode,
    }
}

pub(super) fn normalize_cloud_provider_name(provider_name: Option<&str>) -> Option<String> {
    let value = provider_name?.trim();
    if value.is_empty() {
        return None;
    }

    let lowered = value.to_ascii_lowercase();
    let canonical = if lowered.contains("deepseek") {
        "deepseek"
    } else if lowered.contains("zhipu") || lowered.contains("glm") {
        "glm"
    } else if lowered.contains("minimax") {
        "minimax"
    } else {
        value
    };

    Some(canonical.to_string())
}

fn normalize_active_provider(active_provider: Option<&str>) -> Option<&'static str> {
    let value = active_provider?.trim();
    if value.is_empty() {
        return None;
    }
    let lowered = value.to_ascii_lowercase();
    match lowered.as_str() {
        "cloud" => Some("cloud"),
        "ollama" => Some("ollama"),
        _ => None,
    }
}

pub(super) fn resolve_active_provider(
    mode: AppMode,
    preferred: Option<&str>,
    has_cloud_key: bool,
    fallback: Option<&str>,
) -> String {
    if matches!(mode, AppMode::StrictLocal) {
        return "ollama".to_string();
    }

    let preferred =
        normalize_active_provider(preferred).or_else(|| normalize_active_provider(fallback));

    match preferred {
        Some("cloud") if has_cloud_key => "cloud".to_string(),
        Some("cloud") => "ollama".to_string(),
        Some("ollama") => "ollama".to_string(),
        _ if has_cloud_key => "cloud".to_string(),
        _ => "ollama".to_string(),
    }
}

pub(super) fn display_cloud_provider_name(provider_name: &str) -> String {
    let trimmed = provider_name.trim();
    let lowered = trimmed.to_ascii_lowercase();
    match lowered.as_str() {
        "deepseek" => "DeepSeek".to_string(),
        "glm" => "GLM".to_string(),
        "minimax" => "MiniMax".to_string(),
        _ => trimmed.to_string(),
    }
}

fn provider_default_base_url(provider_name: Option<&str>) -> Option<String> {
    let lowered = provider_name?.trim().to_ascii_lowercase();
    if lowered.contains("deepseek") {
        Some("https://api.deepseek.com/v1".to_string())
    } else if lowered.contains("zhipu") || lowered.contains("glm") {
        Some("https://open.bigmodel.cn/api/paas/v4".to_string())
    } else if lowered.contains("minimax") {
        Some("https://api.minimax.chat/v1".to_string())
    } else {
        None
    }
}

pub(super) fn normalize_cloud_base_url(
    provider_name: Option<&str>,
    base_url: Option<&str>,
) -> Option<String> {
    if let Some(value) = base_url {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    provider_default_base_url(provider_name)
}

pub(super) fn effective_cloud_base_url(
    provider_name: Option<&str>,
    base_url: Option<&str>,
) -> String {
    normalize_cloud_base_url(provider_name, base_url)
        .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string())
}

pub(super) fn llm_health_error_message(err: &LlmError) -> String {
    match err {
        LlmError::ConnectionFailed(detail) => {
            format!("无法连接到本地 Ollama 服务：{}", detail)
        }
        LlmError::Timeout => "本地 Ollama 健康检查超时".to_string(),
        LlmError::ModelNotFound(model) => format!("本地 Ollama 未找到模型：{}", model),
        LlmError::InvalidResponse(detail) => {
            format!("本地 Ollama 返回了无效响应：{}", detail)
        }
    }
}

// ─── 配置文件 I/O ─────────────────────────────────────────────────────────────

pub(super) fn project_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(manifest_dir)
}

pub(super) fn default_config_path() -> PathBuf {
    default_config_path_from_root(&project_root())
}

pub(super) fn default_config_path_from_root(root: &Path) -> PathBuf {
    root.join(".runtime").join("app-config.json")
}

pub(super) fn load_config(config_path: &Path) -> (AppConfig, Option<String>) {
    match fs::read_to_string(config_path) {
        Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
            Ok(config) => (config, Some(raw)),
            Err(_) => (AppConfig::default(), Some(raw)),
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => (AppConfig::default(), None),
        Err(_) => (AppConfig::default(), None),
    }
}

pub(super) fn serialize_config_full(config: &AppConfig) -> String {
    serde_json::to_string_pretty(config).expect("配置序列化失败")
}

pub(super) fn write_config_file(
    config_path: &Path,
    serialized: &str,
    expected_snapshot: Option<&str>,
) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建配置目录失败: {}", err))?;
    }

    let current_snapshot = match fs::read_to_string(config_path) {
        Ok(raw) => Some(raw),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("读取配置文件失败: {}", err)),
    };

    if let Some(snapshot) = expected_snapshot {
        if current_snapshot.as_deref() != Some(snapshot) {
            return Err("配置文件已被外部修改".to_string());
        }
    } else if current_snapshot.is_some() {
        return Err("配置文件已被外部创建或修改".to_string());
    }

    fs::write(config_path, serialized).map_err(|err| format!("写入配置文件失败: {}", err))
}

pub(super) fn default_ingest_source_path(_root: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"E:\llm-wiki\test-llm.md")
    }

    #[cfg(not(windows))]
    {
        _root.join("test-llm.md")
    }
}

// ─── 配置持久化（需要 AppState 访问）────────────────────────────────────────

pub(super) fn set_vault_path(state: &AppState, vault_path: PathBuf) -> Result<(), String> {
    let (mode, query_top_k, expected_snapshot) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (guard.mode, guard.query_top_k, guard.config_snapshot.clone())
    };

    {
        let mut guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path = Some(vault_path.clone());
    }

    match persist_config(
        state,
        mode,
        Some(vault_path.as_path()),
        query_top_k,
        expected_snapshot.as_deref(),
    ) {
        Ok(serialized) => {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.config_snapshot = Some(serialized);
            Ok(())
        }
        Err(err) => Err(err),
    }
}

pub(super) fn persist_config(
    state: &AppState,
    mode: AppMode,
    vault_path: Option<&Path>,
    query_top_k: usize,
    expected_snapshot: Option<&str>,
) -> Result<String, String> {
    let config = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        AppConfig {
            mode,
            vault_path: vault_path.map(|path| path.to_string_lossy().to_string()),
            query_top_k: Some(query_top_k),
            cloud_api_key: guard.cloud_api_key.clone(),
            cloud_base_url: guard.cloud_base_url.clone(),
            cloud_model: guard.cloud_model.clone(),
            cloud_provider_name: guard.cloud_provider_name.clone(),
            active_provider: guard.active_provider.clone(),
            default_ocr_provider: guard.default_ocr_provider.clone(),
            ollama_model: guard.ollama_model.clone(),
            ollama_base_url: guard.ollama_base_url.clone(),
            embed_ollama_model: guard.embed_ollama_model.clone(),
            embed_ollama_base_url: guard.embed_ollama_base_url.clone(),
            shell_policy: Some(guard.shell_policy.clone()),
        }
    };
    let serialized = serialize_config_full(&config);

    if let Some(parent) = state.config_path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建配置目录失败: {}", err))?;
    }

    let current_snapshot = match fs::read_to_string(&state.config_path) {
        Ok(raw) => Some(raw),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(format!("读取配置文件失败: {}", err)),
    };

    match expected_snapshot {
        Some(snapshot) => {
            if current_snapshot.as_deref() != Some(snapshot) {
                return Err("配置文件已被外部修改".to_string());
            }
        }
        None => {
            if current_snapshot.is_some() {
                return Err("配置文件已被外部创建或修改".to_string());
            }
        }
    }

    write_config_file(&state.config_path, &serialized, current_snapshot.as_deref())?;
    Ok(serialized)
}

// ─── LLM 状态查询（需要 AppState 访问）──────────────────────────────────────

pub(super) fn llm_status_input(
    state: &AppState,
) -> (
    AppMode,
    Option<String>,
    Option<OpenAiConfig>,
    Option<Arc<dyn LlmProvider>>,
) {
    let (
        mode,
        cloud_api_key,
        cloud_base_url,
        cloud_model,
        cloud_provider_name,
        active_provider,
    ) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (
            guard.mode,
            guard.cloud_api_key.clone(),
            guard.cloud_base_url.clone(),
            guard.cloud_model.clone(),
            guard.cloud_provider_name.clone(),
            guard.active_provider.clone(),
        )
    };

    let has_cloud_key = cloud_api_key
        .as_deref()
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false);
    let resolved_provider =
        resolve_active_provider(mode, active_provider.as_deref(), has_cloud_key, None);

    match mode {
        AppMode::StrictLocal => (mode, None, None, Some(state.get_ollama_provider())),
        AppMode::Hybrid => {
            if resolved_provider == "cloud" {
                let key = cloud_api_key
                    .filter(|k| !k.trim().is_empty())
                    .expect("resolved_provider=cloud 时必须存在非空 key");
                let model = cloud_model
                    .filter(|m| !m.trim().is_empty())
                    .unwrap_or_else(|| DEFAULT_OPENAI_MODEL.to_string());
                let base_url = effective_cloud_base_url(
                    cloud_provider_name.as_deref(),
                    cloud_base_url.as_deref(),
                );
                let config = OpenAiConfig::with_base_url_and_model(key, base_url, model, None);
                (mode, cloud_provider_name, Some(config), None)
            } else {
                (mode, None, None, Some(state.get_ollama_provider()))
            }
        }
    }
}

pub(super) async fn llm_status_from_input(
    mode: AppMode,
    cloud_provider_name: Option<String>,
    cloud_config: Option<OpenAiConfig>,
    provider: Option<Arc<dyn LlmProvider>>,
) -> LlmStatus {
    if let Some(config) = cloud_config {
        let provider_name = normalize_cloud_provider_name(cloud_provider_name.as_deref())
            .as_deref()
            .map(display_cloud_provider_name)
            .unwrap_or_else(|| "openai-compatible".to_string());
        let base_url = config.base_url.clone();
        let model = config.model.clone();
        let provider = OpenAiProvider::new(config);
        match provider.health_check().await {
            Ok(true) => build_llm_status(
                &provider_name,
                &base_url,
                &model,
                mode,
                true,
                format!("云端 Provider（OpenAI-compatible）可用：{}", provider_name),
            ),
            Ok(false) => build_llm_status(
                &provider_name,
                &base_url,
                &model,
                mode,
                false,
                "云端 Provider（OpenAI-compatible）健康检查未通过，请确认 API Key、基础地址与网络可达"
                    .to_string(),
            ),
            Err(err) => build_llm_status(
                &provider_name,
                &base_url,
                &model,
                mode,
                false,
                format!("云端 Provider（OpenAI-compatible）状态检查失败: {}", err),
            ),
        }
    } else {
        match provider {
            Some(provider) => {
                let base_url = provider.base_url().to_string();
                let model = provider.model().to_string();

                match provider.health_check().await {
                    Ok(true) => build_llm_status(
                        "ollama",
                        &base_url,
                        &model,
                        mode,
                        true,
                        "本地 Ollama 可用".to_string(),
                    ),
                    Ok(false) => build_llm_status(
                        "ollama",
                        &base_url,
                        &model,
                        mode,
                        false,
                        "本地 Ollama 健康检查未通过，请确认服务已启动且模型已准备好".to_string(),
                    ),
                    Err(err) => build_llm_status(
                        "ollama",
                        &base_url,
                        &model,
                        mode,
                        false,
                        llm_health_error_message(&err),
                    ),
                }
            }
            None => {
                let config = OllamaConfig::default();
                build_llm_status(
                    "ollama",
                    &config.base_url,
                    &config.model,
                    mode,
                    false,
                    "本地 Ollama Provider 初始化失败".to_string(),
                )
            }
        }
    }
}

// ─── 公共配置方法实现 ─────────────────────────────────────────────────────────

pub fn llm_status_future(
    state: &AppState,
) -> impl std::future::Future<Output = LlmStatus> + Send + 'static {
    let (mode, cloud_provider_name, cloud_config, ollama_provider) = llm_status_input(state);
    async move {
        llm_status_from_input(mode, cloud_provider_name, cloud_config, ollama_provider).await
    }
}

pub async fn generate_summary(state: &AppState, content: &str) -> String {
    const MAX_INPUT_CHARS: usize = 8000;
    let truncated_content: String = content.chars().take(MAX_INPUT_CHARS).collect();
    let content = truncated_content.as_str();

    let provider = match state.get_llm_provider() {
        Some(p) => p,
        None => {
            state.push_log(
                LogLevel::Warn,
                "LLM Provider 不可用，回退到截断摘要".to_string(),
            );
            return vault::fallback_summarize(content, super::LLM_SUMMARY_MAX_TOKENS);
        }
    };

    match provider.summarize(content, super::LLM_SUMMARY_MAX_TOKENS).await {
        Ok(summary) => {
            let summary = summary.trim().to_string();
            if summary.is_empty() {
                state.push_log(LogLevel::Warn, "LLM 返回空摘要，回退到截断摘要".to_string());
                vault::fallback_summarize(content, super::LLM_SUMMARY_MAX_TOKENS)
            } else {
                state.push_log(
                    LogLevel::Info,
                    format!("LLM 摘要生成成功，长度={}", summary.chars().count()),
                );
                summary
            }
        }
        Err(err) => {
            state.push_log(
                LogLevel::Warn,
                format!("LLM 摘要生成失败: {}，回退到截断摘要", err),
            );
            vault::fallback_summarize(content, super::LLM_SUMMARY_MAX_TOKENS)
        }
    }
}

pub fn get_llm_config(state: &AppState) -> LlmProviderConfig {
    let guard = state.inner.lock().expect("状态锁已被污染");
    let mode = guard.mode;
    let cloud_api_key = guard.cloud_api_key.clone().unwrap_or_default();
    let normalized_provider_name =
        normalize_cloud_provider_name(guard.cloud_provider_name.as_deref());
    let cloud_base_url = normalize_cloud_base_url(
        normalized_provider_name.as_deref(),
        guard.cloud_base_url.as_deref(),
    )
    .unwrap_or_default();
    let cloud_model = guard.cloud_model.clone().unwrap_or_default();
    let cloud_provider_name = normalized_provider_name
        .as_deref()
        .map(display_cloud_provider_name)
        .unwrap_or_else(|| "openai-compatible".to_string());
    let ollama_model = guard.ollama_model.clone().unwrap_or_default();
    let ollama_base_url = guard.ollama_base_url.clone().unwrap_or_default();
    let embed_ollama_model = guard
        .embed_ollama_model
        .clone()
        .unwrap_or_else(|| "nomic-embed-text:latest".to_string());
    let embed_ollama_base_url = guard.embed_ollama_base_url.clone().unwrap_or_default();
    let has_cloud_key = !cloud_api_key.trim().is_empty();
    let active_provider =
        resolve_active_provider(mode, guard.active_provider.as_deref(), has_cloud_key, None);

    LlmProviderConfig {
        cloud_api_key,
        cloud_base_url,
        cloud_model,
        cloud_provider_name,
        active_provider,
        ollama_model,
        ollama_base_url,
        embed_ollama_model,
        embed_ollama_base_url,
    }
}

pub fn set_llm_config(
    state: &AppState,
    config: LlmProviderConfig,
) -> Result<LlmProviderConfig, String> {
    let (mode, vault_path, query_top_k, expected_snapshot, persisted_active_provider) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (
            guard.mode,
            guard.vault_path.clone(),
            guard.query_top_k,
            guard.config_snapshot.clone(),
            guard.active_provider.clone(),
        )
    };

    let cloud_api_key = if config.cloud_api_key.trim().is_empty() {
        None
    } else {
        Some(config.cloud_api_key.trim().to_string())
    };
    let cloud_provider_name =
        normalize_cloud_provider_name(Some(config.cloud_provider_name.as_str()));
    let cloud_base_url = normalize_cloud_base_url(
        cloud_provider_name.as_deref(),
        Some(config.cloud_base_url.as_str()),
    );
    let cloud_model = if config.cloud_model.trim().is_empty() {
        None
    } else {
        Some(config.cloud_model.trim().to_string())
    };
    let ollama_model = if config.ollama_model.trim().is_empty() {
        None
    } else {
        Some(config.ollama_model.trim().to_string())
    };
    let ollama_base_url = if config.ollama_base_url.trim().is_empty() {
        None
    } else {
        Some(config.ollama_base_url.trim().to_string())
    };
    let has_cloud_key = cloud_api_key
        .as_deref()
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false);
    let active_provider = resolve_active_provider(
        mode,
        Some(config.active_provider.as_str()),
        has_cloud_key,
        persisted_active_provider.as_deref(),
    );

    {
        let mut guard = state.inner.lock().expect("状态锁已被污染");
        guard.cloud_api_key = cloud_api_key;
        guard.cloud_provider_name = cloud_provider_name;
        guard.cloud_base_url = cloud_base_url;
        guard.cloud_model = cloud_model;
        guard.active_provider = Some(active_provider);
        guard.ollama_model = ollama_model;
        guard.ollama_base_url = ollama_base_url;
        guard.embed_ollama_model = if config.embed_ollama_model.trim().is_empty() {
            None
        } else {
            Some(config.embed_ollama_model.trim().to_string())
        };
        guard.embed_ollama_base_url = if config.embed_ollama_base_url.trim().is_empty() {
            None
        } else {
            Some(config.embed_ollama_base_url.trim().to_string())
        };
    }

    match persist_config(
        state,
        mode,
        vault_path.as_deref(),
        query_top_k,
        expected_snapshot.as_deref(),
    ) {
        Ok(serialized) => {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.config_snapshot = Some(serialized);
            guard.push_log(
                LogLevel::Info,
                "云端 Provider 配置已保存".to_string(),
                super::current_timestamp_ms(),
            );
            drop(guard);
            Ok(get_llm_config(state))
        }
        Err(err) => {
            state.push_log(
                LogLevel::Warn,
                format!("云端 Provider 配置持久化失败: {}", err),
            );
            Err(err)
        }
    }
}

pub fn get_ocr_config(state: &AppState) -> Option<String> {
    let guard = state.inner.lock().expect("状态锁已被污染");
    guard.default_ocr_provider.clone()
}

pub fn set_ocr_config(state: &AppState, provider: Option<String>) -> Result<(), String> {
    let (mode, vault_path, query_top_k, expected_snapshot) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (
            guard.mode,
            guard.vault_path.clone(),
            guard.query_top_k,
            guard.config_snapshot.clone(),
        )
    };

    {
        let mut guard = state.inner.lock().expect("状态锁已被污染");
        guard.default_ocr_provider = provider;
    }

    match persist_config(
        state,
        mode,
        vault_path.as_deref(),
        query_top_k,
        expected_snapshot.as_deref(),
    ) {
        Ok(serialized) => {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.config_snapshot = Some(serialized);
            guard.push_log(
                LogLevel::Info,
                "OCR Provider 配置已保存".to_string(),
                super::current_timestamp_ms(),
            );
            Ok(())
        }
        Err(err) => {
            state.push_log(
                LogLevel::Warn,
                format!("OCR Provider 配置持久化失败: {}", err),
            );
            Err(err)
        }
    }
}

pub fn get_shell_policy_config(state: &AppState) -> ShellPolicyConfig {
    let guard = state.inner.lock().expect("状态锁已被污染");
    guard.shell_policy.clone()
}

pub fn set_shell_policy_config(
    state: &AppState,
    config: ShellPolicyConfig,
) -> Result<ShellPolicyConfig, String> {
    let (mode, vault_path, query_top_k, expected_snapshot) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (
            guard.mode,
            guard.vault_path.clone(),
            guard.query_top_k,
            guard.config_snapshot.clone(),
        )
    };

    {
        let mut guard = state.inner.lock().expect("状态锁已被污染");
        guard.shell_policy = config;
    }

    match persist_config(
        state,
        mode,
        vault_path.as_deref(),
        query_top_k,
        expected_snapshot.as_deref(),
    ) {
        Ok(serialized) => {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.config_snapshot = Some(serialized);
            guard.push_log(
                LogLevel::Info,
                "Shell 策略配置已保存".to_string(),
                super::current_timestamp_ms(),
            );
            Ok(guard.shell_policy.clone())
        }
        Err(err) => {
            state.push_log(
                LogLevel::Warn,
                format!("Shell 策略配置持久化失败: {}", err),
            );
            Err(err)
        }
    }
}

pub fn set_mode(state: &AppState, mode: AppMode) -> ModeChangeResult {
    let (previous_mode, expected_snapshot, vault_path, query_top_k) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (
            guard.mode,
            guard.config_snapshot.clone(),
            guard.vault_path.clone(),
            guard.query_top_k,
        )
    };

    match persist_config(
        state,
        mode,
        vault_path.as_deref(),
        query_top_k,
        expected_snapshot.as_deref(),
    ) {
        Ok(serialized) => {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.mode = mode;
            guard.config_snapshot = Some(serialized);
            guard.push_log(
                LogLevel::Info,
                format!("模式切换为 {:?}", mode),
                super::current_timestamp_ms(),
            );

            ModeChangeResult {
                previous_mode,
                current_mode: mode,
                strict_local_enabled: matches!(mode, AppMode::StrictLocal),
            }
        }
        Err(err) => {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.push_log(
                LogLevel::Warn,
                format!("模式持久化失败: {}", err),
                super::current_timestamp_ms(),
            );

            ModeChangeResult {
                previous_mode,
                current_mode: previous_mode,
                strict_local_enabled: matches!(previous_mode, AppMode::StrictLocal),
            }
        }
    }
}

pub fn init_vault(state: &AppState, vault_path: PathBuf) -> Result<VaultInitResult, String> {
    let mode = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.mode
    };

    let mut result = match vault::initialize_vault(&vault_path, mode) {
        Ok(result) => result,
        Err(err) => {
            state.push_log(LogLevel::Warn, format!("Vault 初始化失败: {}", err));
            return Err(err);
        }
    };
    let warning = set_vault_path(state, vault_path.clone()).err();

    if let Some(message) = warning {
        state.push_log(
            LogLevel::Warn,
            format!("Vault 初始化完成，但运行配置更新失败: {}", message),
        );
        result.message = format!("Vault 初始化完成，但运行配置更新失败: {}", message);
    } else {
        state.push_log(
            LogLevel::Info,
            format!("Vault 已初始化: {}", vault_path.to_string_lossy()),
        );
    }

    state.record_outbox_event(
        "vault_initialized",
        serde_json::json!({
            "vault_path": result.vault_path.clone(),
            "created_paths": result.created_paths.clone(),
            "message": result.message.clone(),
        }),
    );

    if let Some(db_path) = state.outbox_db_path() {
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            let now = super::current_timestamp_ms();
            match db::db_reset_stale_running(&conn, &now) {
                Ok(n) if n > 0 => {
                    state.push_log(
                        LogLevel::Info,
                        format!("重启恢复：已将 {} 条中断的 ingest 任务重新排队", n),
                    );
                }
                _ => {}
            }
        }
    }

    Ok(result)
}

pub fn init_vault_with_template(
    state: &AppState,
    vault_path: PathBuf,
    template_schema: String,
    template_purpose: String,
    extra_dirs: Vec<String>,
) -> Result<VaultInitResult, String> {
    for dir in &extra_dirs {
        let clean = dir.trim().replace('\\', "/");
        if clean.contains("..") || clean.starts_with('/') {
            return Err(format!("非法目录路径（不允许路径遍历或绝对路径）: {}", dir));
        }
    }
    const MAX_TEMPLATE_BYTES: usize = 512 * 1024;
    if template_schema.len() > MAX_TEMPLATE_BYTES {
        return Err("模板 schema 内容超过 512 KB 限制".to_string());
    }
    if template_purpose.len() > MAX_TEMPLATE_BYTES {
        return Err("模板 purpose 内容超过 512 KB 限制".to_string());
    }

    let mut result = init_vault(state, vault_path.clone())?;

    let wiki_dir = vault_path.join("wiki");
    if !wiki_dir.exists() {
        fs::create_dir_all(&wiki_dir).map_err(|e| format!("创建 wiki 目录失败: {}", e))?;
    }

    let schema_path = wiki_dir.join("schema.md");
    if !schema_path.exists() {
        fs::write(&schema_path, template_schema)
            .map_err(|e| format!("创建 schema.md 失败: {}", e))?;
        result
            .created_paths
            .push(schema_path.to_string_lossy().to_string());
    }

    let purpose_path = wiki_dir.join("purpose.md");
    if !purpose_path.exists() {
        fs::write(&purpose_path, template_purpose)
            .map_err(|e| format!("创建 purpose.md 失败: {}", e))?;
        result
            .created_paths
            .push(purpose_path.to_string_lossy().to_string());
    }

    for dir in extra_dirs {
        let target_dir = if dir.starts_with("wiki/") || dir == "wiki" {
            vault_path.join(&dir)
        } else {
            wiki_dir.join(&dir)
        };

        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)
                .map_err(|e| format!("创建额外目录 {:?} 失败: {}", target_dir, e))?;
            result
                .created_paths
                .push(target_dir.to_string_lossy().to_string());
        }
    }

    state.push_log(
        LogLevel::Info,
        format!("Vault 模板初始化完成: {}", vault_path.to_string_lossy()),
    );

    result.message = format!("Vault 模板初始化已完成：{}", vault_path.to_string_lossy());
    Ok(result)
}

pub fn overview(state: &AppState) -> AppOverview {
    let guard = state.inner.lock().expect("状态锁已被污染");
    let vault_path = guard
        .vault_path
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "vault".to_string());
    let pending_tasks = guard
        .vault_path
        .as_ref()
        .and_then(|path| db::count_pending_tasks(&path.join(".app").join("meta.db")).ok())
        .unwrap_or(0);

    AppOverview {
        app_name: "LLM Wiki".to_string(),
        mode: guard.mode,
        vault_path,
        recent_log_count: guard.logs.len(),
        pending_tasks,
        supported_modes: vec![AppMode::Hybrid, AppMode::StrictLocal],
    }
}

pub fn default_paths(state: &AppState) -> DefaultPaths {
    let _ = state;
    let root = project_root();
    DefaultPaths {
        vault_path: root.join("vault").to_string_lossy().to_string(),
        ingest_source_path: default_ingest_source_path(root).to_string_lossy().to_string(),
    }
}

pub fn query_settings(state: &AppState) -> QuerySettings {
    let guard = state.inner.lock().expect("状态锁已被污染");
    QuerySettings {
        top_k: guard.query_top_k,
        min_top_k: super::QUERY_TOP_K_MIN,
        max_top_k: super::QUERY_TOP_K_MAX,
    }
}

pub fn recent_logs(state: &AppState, limit: usize) -> Vec<LogEntry> {
    let guard = state.inner.lock().expect("状态锁已被污染");
    guard.logs.iter().rev().take(limit).cloned().collect()
}
