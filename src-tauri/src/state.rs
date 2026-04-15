use std::{
    collections::{BTreeSet, HashSet},
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use tauri::{AppHandle, Emitter};

use crate::{
    db,
    llm::{
        LlmError, LlmProvider, OllamaConfig, OllamaProvider, OpenAiConfig, OpenAiProvider,
        DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL,
    },
    models::{
        AppConfig, AppMode, AppOverview, DefaultPaths, IngestResult, LintIssue,
        LintPatchApplyInput, LintPatchApplyResult, LintPatchBatchApplyItemResult,
        LintPatchBatchApplyResult, LintPatchBatchApplyStatus, LintPatchEventItem, LintPatchPreview,
        LintPatchSuggestion, LintReport, LintSeverityStats, LlmProviderConfig, LlmStatus, LogEntry,
        LogLevel, ModeChangeResult, ProgressPayload, QueryAnswerResult, QueryAskOptions,
        QueryCitation, QuerySettings, SaveQueryAnswerInput, SaveQueryAnswerResult, VaultInitResult,
        WikiPageCitationItem, WikiPageDetail, WikiPageFrontmatter, WikiPageItem,
    },
    vault,
};

const STALE_PENDING_TASK_THRESHOLD_MS: u128 = 24 * 60 * 60 * 1000;
const QUERY_TOP_K_MIN: usize = 1;
const QUERY_TOP_K_MAX: usize = 8;
const QUERY_TOP_K_DEFAULT: usize = 3;

/// 默认摘要最大 token 数量
const LLM_SUMMARY_MAX_TOKENS: usize = 200;

/// 进程内状态。
pub struct AppState {
    inner: Mutex<AppStateData>,
    config_path: PathBuf,
    /// LLM Provider（延迟初始化）
    llm_provider: OnceLock<Option<Arc<OllamaProvider>>>,
    /// Tauri AppHandle（应用启动后由 setup hook 注入，用于 emit 进度事件）
    app_handle: OnceLock<AppHandle>,
}

/// 状态快照。
#[derive(Debug, Clone)]
struct AppStateData {
    mode: AppMode,
    vault_path: Option<PathBuf>,
    query_top_k: usize,
    logs: Vec<LogEntry>,
    next_log_id: u64,
    config_snapshot: Option<String>,
    /// Hybrid 模式下可选云端 API Key（不入仓库）
    cloud_api_key: Option<String>,
    /// Hybrid 模式下使用的云端基础地址
    cloud_base_url: Option<String>,
    /// Hybrid 模式下使用的云端模型名
    cloud_model: Option<String>,
    /// 云端 Provider 名称
    cloud_provider_name: Option<String>,
    /// 当前活跃 Provider
    active_provider: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let config_path = Self::default_config_path();
        let (config, config_snapshot) = Self::load_config(&config_path);
        let mode = config.mode;
        let vault_path = config.vault_path.clone().map(PathBuf::from);
        let query_top_k = config.query_top_k;
        let query_top_k = normalize_top_k(query_top_k);
        // 初始序列化包含所有字段（含云端配置）
        let serialized = Self::serialize_config_full(&AppConfig {
            mode,
            vault_path: vault_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            query_top_k: Some(query_top_k),
            cloud_api_key: config.cloud_api_key.clone(),
            cloud_base_url: config.cloud_base_url.clone(),
            cloud_model: config.cloud_model.clone(),
            cloud_provider_name: config.cloud_provider_name.clone(),
            active_provider: config.active_provider.clone(),
        });
        let mut runtime_snapshot = config_snapshot.clone();

        if runtime_snapshot.is_none() {
            if let Err(err) = Self::write_config_file(&config_path, &serialized, None) {
                let _ = err;
            } else {
                runtime_snapshot = Some(serialized.clone());
            }
        }

        let logs = vec![
            LogEntry {
                id: 1,
                level: LogLevel::Info,
                message: "应用骨架已启动".to_string(),
                created_at: "2026-04-08T00:00:00+08:00".to_string(),
            },
            LogEntry {
                id: 2,
                level: LogLevel::Info,
                message: format!("模式配置已加载为 {:?}", mode),
                created_at: "2026-04-08T00:00:01+08:00".to_string(),
            },
        ];

        Self {
            inner: Mutex::new(AppStateData {
                mode,
                vault_path,
                query_top_k,
                next_log_id: 3,
                logs,
                config_snapshot: runtime_snapshot,
                cloud_api_key: config.cloud_api_key,
                cloud_base_url: config.cloud_base_url,
                cloud_model: config.cloud_model,
                cloud_provider_name: config.cloud_provider_name,
                active_provider: config.active_provider,
            }),
            config_path,
            llm_provider: OnceLock::new(),
            app_handle: OnceLock::new(),
        }
    }

    /// 注入 Tauri AppHandle（在应用 setup hook 中调用一次）。
    pub fn set_app_handle(&self, handle: AppHandle) {
        let _ = self.app_handle.set(handle);
    }

    /// 向前端 emit 进度事件（AppHandle 未注入时静默跳过）。
    fn emit_progress(&self, event: &str, step: &str, message: &str) {
        if let Some(handle) = self.app_handle.get() {
            let _ = handle.emit(
                event,
                ProgressPayload {
                    step: step.to_string(),
                    message: message.to_string(),
                },
            );
        }
    }

    /// 获取 Ollama Provider（延迟初始化）。
    fn get_ollama_provider(&self) -> Option<Arc<OllamaProvider>> {
        self.llm_provider
            .get_or_init(|| {
                // 这里固定使用本地 Ollama
                let config = OllamaConfig::default();
                let provider = OllamaProvider::new(config);
                Some(Arc::new(provider))
            })
            .clone()
    }

    /// 获取 LLM Provider，按模式路由：
    /// - StrictLocal → 仅 Ollama
    /// - Hybrid → 优先使用 active_provider（仅 cloud/ollama），并在无 key 时安全回退到 ollama
    fn get_llm_provider(&self) -> Option<Arc<dyn LlmProvider>> {
        let (
            mode,
            cloud_api_key,
            cloud_base_url,
            cloud_model,
            cloud_provider_name,
            active_provider,
        ) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
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
            AppMode::StrictLocal => {
                // 严格本地模式：禁止云 Provider
                self.get_ollama_provider()
                    .map(|p| p as Arc<dyn LlmProvider>)
            }
            AppMode::Hybrid => {
                // Hybrid 模式：遵循 active_provider，cloud 仅在 key 可用时生效
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
                    let config = OpenAiConfig::with_base_url_and_model(key, base_url, model);
                    Some(Arc::new(OpenAiProvider::new(config)) as Arc<dyn LlmProvider>)
                } else {
                    self.get_ollama_provider()
                        .map(|p| p as Arc<dyn LlmProvider>)
                }
            }
        }
    }

    /// 使用 LLM 生成摘要，失败时回退到截断
    ///
    /// # 参数
    /// - `content`: 需要摘要的原始内容
    ///
    /// # 返回
    /// 生成的摘要文本。如果 LLM 调用失败，则回退到简单截断。
    pub async fn generate_summary(&self, content: &str) -> String {
        // 尝试获取 LLM Provider
        let provider = match self.get_llm_provider() {
            Some(p) => p,
            None => {
                self.push_log(
                    LogLevel::Warn,
                    "LLM Provider 不可用，回退到截断摘要".to_string(),
                );
                return vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS);
            }
        };

        // 尝试使用 LLM 生成摘要
        match provider.summarize(content, LLM_SUMMARY_MAX_TOKENS).await {
            Ok(summary) => {
                let summary = summary.trim().to_string();
                if summary.is_empty() {
                    self.push_log(LogLevel::Warn, "LLM 返回空摘要，回退到截断摘要".to_string());
                    vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS)
                } else {
                    self.push_log(
                        LogLevel::Info,
                        format!("LLM 摘要生成成功，长度={}", summary.chars().count()),
                    );
                    summary
                }
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("LLM 摘要生成失败: {}，回退到截断摘要", err),
                );
                vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS)
            }
        }
    }

    /// 构造 LLM 状态查询输入，避免在异步命令中持有 `State` 借用。
    /// 返回 (mode, cloud_config_if_active, ollama_provider_if_active)
    fn llm_status_input(
        &self,
    ) -> (
        AppMode,
        Option<String>,
        Option<OpenAiConfig>,
        Option<Arc<OllamaProvider>>,
    ) {
        let (
            mode,
            cloud_api_key,
            cloud_base_url,
            cloud_model,
            cloud_provider_name,
            active_provider,
        ) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
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
            AppMode::StrictLocal => (mode, None, None, self.get_ollama_provider()),
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
                    let config = OpenAiConfig::with_base_url_and_model(key, base_url, model);
                    (mode, cloud_provider_name, Some(config), None)
                } else {
                    (mode, None, None, self.get_ollama_provider())
                }
            }
        }
    }

    /// 使用输入快照查询当前活跃 Provider 的健康状态。
    async fn llm_status_from_input(
        mode: AppMode,
        cloud_provider_name: Option<String>,
        cloud_config: Option<OpenAiConfig>,
        ollama_provider: Option<Arc<OllamaProvider>>,
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
            // 使用本地 Ollama
            match ollama_provider {
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
                            "本地 Ollama 健康检查未通过，请确认服务已启动且模型已准备好"
                                .to_string(),
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

    /// 返回可在异步命令中安全等待的 LLM 状态查询 Future。
    pub fn llm_status_future(
        &self,
    ) -> impl std::future::Future<Output = LlmStatus> + Send + 'static {
        let (mode, cloud_provider_name, cloud_config, ollama_provider) = self.llm_status_input();
        async move {
            Self::llm_status_from_input(mode, cloud_provider_name, cloud_config, ollama_provider)
                .await
        }
    }

    /// 获取当前 LLM Provider 配置（供 Settings 页面读取）。
    pub fn get_llm_config(&self) -> LlmProviderConfig {
        let guard = self.inner.lock().expect("状态锁已被污染");
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
        let has_cloud_key = !cloud_api_key.trim().is_empty();
        let active_provider =
            resolve_active_provider(mode, guard.active_provider.as_deref(), has_cloud_key, None);

        LlmProviderConfig {
            cloud_api_key,
            cloud_base_url,
            cloud_model,
            cloud_provider_name,
            active_provider,
        }
    }

    /// 保存 LLM Provider 配置（云端字段持久化）。
    pub fn set_llm_config(&self, config: LlmProviderConfig) -> Result<LlmProviderConfig, String> {
        let (mode, vault_path, query_top_k, expected_snapshot, persisted_active_provider) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
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

        // 先更新 guard，persist_config 会从 guard 读取云端字段
        {
            let mut guard = self.inner.lock().expect("状态锁已被污染");
            guard.cloud_api_key = cloud_api_key;
            guard.cloud_provider_name = cloud_provider_name;
            guard.cloud_base_url = cloud_base_url;
            guard.cloud_model = cloud_model;
            guard.active_provider = Some(active_provider);
        }

        match self.persist_config(
            mode,
            vault_path.as_deref(),
            query_top_k,
            expected_snapshot.as_deref(),
        ) {
            Ok(serialized) => {
                let mut guard = self.inner.lock().expect("状态锁已被污染");
                guard.config_snapshot = Some(serialized);
                guard.push_log(
                    LogLevel::Info,
                    "云端 Provider 配置已保存".to_string(),
                    current_timestamp_ms(),
                );
                drop(guard);
                Ok(self.get_llm_config())
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("云端 Provider 配置持久化失败: {}", err),
                );
                Err(err)
            }
        }
    }

    pub fn set_mode(&self, mode: AppMode) -> ModeChangeResult {
        // 先读取快照，再释放锁；避免 persist_config 内部二次加锁导致死锁。
        let (previous_mode, expected_snapshot, vault_path, query_top_k) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (
                guard.mode,
                guard.config_snapshot.clone(),
                guard.vault_path.clone(),
                guard.query_top_k,
            )
        };

        match self.persist_config(
            mode,
            vault_path.as_deref(),
            query_top_k,
            expected_snapshot.as_deref(),
        ) {
            Ok(serialized) => {
                let mut guard = self.inner.lock().expect("状态锁已被污染");
                guard.mode = mode;
                guard.config_snapshot = Some(serialized);
                guard.push_log(
                    LogLevel::Info,
                    format!("模式切换为 {:?}", mode),
                    current_timestamp_ms(),
                );

                ModeChangeResult {
                    previous_mode,
                    current_mode: mode,
                    strict_local_enabled: matches!(mode, AppMode::StrictLocal),
                }
            }
            Err(err) => {
                let mut guard = self.inner.lock().expect("状态锁已被污染");
                guard.push_log(
                    LogLevel::Warn,
                    format!("模式持久化失败: {}", err),
                    current_timestamp_ms(),
                );

                ModeChangeResult {
                    previous_mode,
                    current_mode: previous_mode,
                    strict_local_enabled: matches!(previous_mode, AppMode::StrictLocal),
                }
            }
        }
    }

    pub fn init_vault(&self, vault_path: PathBuf) -> Result<VaultInitResult, String> {
        let mode = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.mode
        };

        let mut result = match vault::initialize_vault(&vault_path, mode) {
            Ok(result) => result,
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("Vault 初始化失败: {}", err));
                return Err(err);
            }
        };
        let warning = self.set_vault_path(vault_path.clone()).err();

        if let Some(message) = warning {
            self.push_log(
                LogLevel::Warn,
                format!("Vault 初始化完成，但运行配置更新失败: {}", message),
            );
            result.message = format!("Vault 初始化完成，但运行配置更新失败: {}", message);
        } else {
            self.push_log(
                LogLevel::Info,
                format!("Vault 已初始化: {}", vault_path.to_string_lossy()),
            );
        }

        Ok(result)
    }

    pub async fn ingest_markdown(&self, source_path: PathBuf) -> Result<IngestResult, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };

        // 读取源文件内容以生成 LLM 摘要
        let source_content =
            fs::read_to_string(&source_path).map_err(|err| format!("读取源文件失败: {}", err))?;

        // 步骤1：LLM 摘要生成
        self.emit_progress("ingest_progress", "summarizing", "正在生成摘要（LLM）...");
        let llm_summary = self.generate_summary(&source_content).await;

        // 步骤2：LLM 实体提取（写入前完成，确保可持久化到 frontmatter）
        self.emit_progress(
            "ingest_progress",
            "extracting_entities",
            "正在提取关键实体（LLM）...",
        );
        let entities = self.extract_entities(&source_content).await;

        // 步骤3：写入 Wiki 页面（含 frontmatter entities）
        self.emit_progress("ingest_progress", "writing_wiki", "写入 Wiki 页面...");
        let mut result = match vault::ingest_markdown(
            &vault_path,
            &source_path,
            Some(&llm_summary),
            &entities,
        ) {
            Ok(result) => {
                self.push_log(
                    LogLevel::Info,
                    format!(
                        "Markdown 导入成功: {} -> {}",
                        source_path.to_string_lossy(),
                        result.wiki_path
                    ),
                );
                result
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("Markdown 导入失败: {}", err));
                return Err(err);
            }
        };

        // 步骤4：双向链接注入
        self.emit_progress("ingest_progress", "updating_links", "注入双向链接...");
        let db_path = vault_path.join(".app").join("meta.db");
        let wiki_title = PathBuf::from(&result.wiki_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();
        let updated_pages = self
            .update_related_pages_with_link(
                &db_path,
                &vault_path,
                &result.wiki_path,
                &wiki_title,
                &entities,
            )
            .await;

        result.entities = entities;
        result.updated_pages = updated_pages;

        Ok(result)
    }

    /// 读取 PDF 文本后复用现有 Markdown ingest 流程。
    pub async fn ingest_pdf_impl(
        &self,
        source_path: &str,
    ) -> Result<crate::models::IngestResult, String> {
        let source_path_buf = PathBuf::from(source_path);
        validate_pdf_source_path(&source_path_buf)?;

        let extracted_text = extract_text_from_pdf(&source_path_buf)?;
        let tmp_path = std::env::temp_dir().join(format!("llm_wiki_pdf_{}.md", uuid_v4_short()));
        tokio::fs::write(&tmp_path, extracted_text)
            .await
            .map_err(|e| format!("写入临时 Markdown 失败：{e}"))?;

        let mut result = self.ingest_markdown(tmp_path.clone()).await;

        // 无论 ingest 成功或失败都尝试清理临时文件。
        let _ = tokio::fs::remove_file(&tmp_path).await;

        if let Ok(inner) = &mut result {
            inner.source_path = source_path_buf.to_string_lossy().to_string();
            let tmp_display = tmp_path.to_string_lossy().to_string();
            inner.message = inner.message.replace(&tmp_display, source_path);
        }

        result
    }

    /// 拉取 URL 文本内容后走现有 ingest 流程
    pub async fn ingest_url_impl(&self, url: &str) -> Result<crate::models::IngestResult, String> {
        // 1. 用 reqwest 拉取 URL 文本（超时 30s）
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| format!("构建 HTTP 客户端失败：{e}"))?;

        let response = client
            .get(url)
            .header("User-Agent", "llm-wiki/1.0")
            .send()
            .await
            .map_err(|e| format!("拉取 URL 失败：{e}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!("URL 请求失败，HTTP {status}"));
        }

        let text = response
            .text()
            .await
            .map_err(|e| format!("读取响应内容失败：{e}"))?;

        if text.trim().is_empty() {
            return Err("URL 返回内容为空".to_string());
        }

        // 2. 将文本写入临时 Markdown 文件，复用 ingest_markdown
        let tmp_path = std::env::temp_dir().join(format!("llm_wiki_url_{}.md", uuid_v4_short()));
        tokio::fs::write(&tmp_path, &text)
            .await
            .map_err(|e| format!("写入临时文件失败：{e}"))?;

        let result = self.ingest_markdown(tmp_path.clone()).await;

        // 3. 清理临时文件（忽略错误）
        let _ = tokio::fs::remove_file(&tmp_path).await;

        result
    }

    /// 用 LLM 从文档内容中提取关键实体（LLM 不可用时返回空列表）。
    async fn extract_entities(&self, content: &str) -> Vec<String> {
        let provider = match self.get_llm_provider() {
            Some(p) => p,
            None => return Vec::new(),
        };

        // 截断内容，避免超出 token 上限
        let truncated: String = content.chars().take(2000).collect();
        let prompt = format!(
            "请从以下文档中提取关键实体（技术名、概念名、产品名、人名等），\
每行输出一个实体名称，不要编号，不要解释，最多提取10个最重要的实体：\n\n{}",
            truncated
        );

        match provider.complete(&prompt).await {
            Ok(response) => {
                let entities: Vec<String> = response
                    .lines()
                    .map(|line| line.trim().trim_start_matches('-').trim().to_string())
                    .filter(|e| !e.is_empty() && e.len() <= 60)
                    .take(10)
                    .collect();

                if !entities.is_empty() {
                    self.push_log(
                        LogLevel::Info,
                        format!("实体提取完成，共 {} 个实体", entities.len()),
                    );
                }
                entities
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("实体提取失败，跳过: {}", err));
                Vec::new()
            }
        }
    }

    /// Ingest 后扫描相关 Wiki 页面并注入双向 See Also 链接。
    ///
    /// 流程：
    /// 1. 用实体名在 FTS 中搜索相关页面（最多 5 页）。
    /// 2. 向每个相关页面追加指向新页的 See Also 链接。
    /// 3. 向新页追加指向相关页面的 See Also 链接。
    /// 4. 更新受影响页面的 FTS 索引。
    ///
    /// 任何单步失败都记录告警但不中断整体流程。
    async fn update_related_pages_with_link(
        &self,
        db_path: &Path,
        vault_path: &Path,
        new_wiki_abs_path: &str,
        new_wiki_title: &str,
        entities: &[String],
    ) -> Vec<String> {
        if entities.is_empty() {
            return Vec::new();
        }

        // 将实体名称分词，合并去重后送入 FTS
        let mut token_set = std::collections::HashSet::new();
        for entity in entities {
            for token in tokenize_query(entity) {
                token_set.insert(token);
            }
        }
        let tokens: Vec<String> = token_set.into_iter().collect();

        if tokens.is_empty() {
            return Vec::new();
        }

        // FTS 搜索相关页面（最多取 5 个，排除自身）
        let related_paths: Vec<String> = match db::search_fts_page_paths(db_path, &tokens, 10) {
            Ok(paths) => paths
                .into_iter()
                .filter(|p| p != new_wiki_abs_path)
                .take(5)
                .collect(),
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("相关页面 FTS 搜索失败: {}", err));
                return Vec::new();
            }
        };

        if related_paths.is_empty() {
            return Vec::new();
        }

        // 新页面相对于 vault 根的路径（用于写入其他页面的链接）
        let new_wiki_relative = PathBuf::from(new_wiki_abs_path)
            .strip_prefix(vault_path)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| new_wiki_abs_path.to_string());

        let mut updated = Vec::new();

        for related_abs in &related_paths {
            let related_path = PathBuf::from(related_abs);
            if !related_path.exists() {
                continue;
            }

            let related_relative = related_path
                .strip_prefix(vault_path)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| related_abs.clone());

            let related_title = related_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Unknown")
                .to_string();

            // 1. 向相关页面追加指向新页的反向链接
            match vault::append_see_also_link(&related_path, &new_wiki_relative, new_wiki_title) {
                Ok(true) => {
                    updated.push(related_abs.clone());
                    // 更新该相关页面的 FTS 索引
                    if let Ok(content) = fs::read_to_string(&related_path) {
                        let _ = db::upsert_fts_page(
                            db_path,
                            Path::new(related_abs),
                            &related_title,
                            &content,
                        );
                    }
                }
                Ok(false) => {} // 链接已存在，跳过
                Err(err) => {
                    self.push_log(
                        LogLevel::Warn,
                        format!("注入反向链接失败 {}: {}", related_abs, err),
                    );
                }
            }

            // 2. 向新页追加指向相关页面的正向链接（失败不计入 updated）
            let new_path = PathBuf::from(new_wiki_abs_path);
            if let Err(err) =
                vault::append_see_also_link(&new_path, &related_relative, &related_title)
            {
                self.push_log(
                    LogLevel::Warn,
                    format!("注入正向链接失败 {}: {}", related_abs, err),
                );
            }
        }

        // 更新新页的 FTS 索引（包含追加的 See Also 内容）
        if !updated.is_empty() {
            if let Ok(content) = fs::read_to_string(new_wiki_abs_path) {
                let _ = db::upsert_fts_page(
                    db_path,
                    Path::new(new_wiki_abs_path),
                    new_wiki_title,
                    &content,
                );
            }
            self.push_log(
                LogLevel::Info,
                format!("双向链接注入完成，更新了 {} 个相关页面", updated.len()),
            );
        }

        updated
    }

    /// 使用本地 Provider 生成 Query 回答。
    async fn generate_query_answer_with_provider(
        &self,
        question: &str,
        matches: &[WikiMatch],
        provider: Option<Arc<dyn LlmProvider>>,
    ) -> (String, String) {
        let fallback = || (build_query_answer(question, matches), "rule".to_string());

        let provider = match provider {
            Some(provider) => provider,
            None => {
                self.push_log(
                    LogLevel::Warn,
                    "本地 LLM Provider 不可用，Query 已回退到规则回答".to_string(),
                );
                return fallback();
            }
        };

        let prompt = build_query_prompt(question, matches);

        match provider.complete(&prompt).await {
            Ok(answer) => {
                let answer = answer.trim().to_string();
                if answer.is_empty() {
                    self.push_log(
                        LogLevel::Warn,
                        "本地 LLM 返回空回答，Query 已回退到规则回答".to_string(),
                    );
                    fallback()
                } else {
                    self.push_log(
                        LogLevel::Info,
                        format!(
                            "本地 LLM Query 合成成功，回答长度={}",
                            answer.chars().count()
                        ),
                    );
                    (answer, "llm".to_string())
                }
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("本地 LLM Query 合成失败: {}，已回退到规则回答", err),
                );
                fallback()
            }
        }
    }

    pub fn overview(&self) -> AppOverview {
        let guard = self.inner.lock().expect("状态锁已被污染");
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

    pub fn default_paths(&self) -> DefaultPaths {
        let root = Self::project_root();
        DefaultPaths {
            vault_path: root.join("vault").to_string_lossy().to_string(),
            ingest_source_path: Self::default_ingest_source_path(root)
                .to_string_lossy()
                .to_string(),
        }
    }

    fn default_ingest_source_path(_root: PathBuf) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(r"E:\llm-wiki\test-llm.md")
        }

        #[cfg(not(windows))]
        {
            _root.join("test-llm.md")
        }
    }

    pub fn query_settings(&self) -> QuerySettings {
        let guard = self.inner.lock().expect("状态锁已被污染");
        QuerySettings {
            top_k: guard.query_top_k,
            min_top_k: QUERY_TOP_K_MIN,
            max_top_k: QUERY_TOP_K_MAX,
        }
    }

    pub fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        let guard = self.inner.lock().expect("状态锁已被污染");
        guard.logs.iter().rev().take(limit).cloned().collect()
    }

    pub fn recent_wiki_pages(&self, limit: usize) -> Result<Vec<WikiPageItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        let pages = db::list_recent_wiki_pages(&db_path, limit)?;

        Ok(pages
            .into_iter()
            .map(|page| {
                let display_path = friendly_display_path(Path::new(&page.path));
                WikiPageItem {
                    title: page.title,
                    path: page.path,
                    display_path: Some(display_path),
                    summary: page.summary,
                    updated_at: page.updated_at,
                }
            })
            .collect())
    }

    pub fn recent_lint_patch_events(
        &self,
        limit: usize,
    ) -> Result<Vec<LintPatchEventItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        db::list_recent_lint_patch_events(&db_path, limit)
    }

    pub fn search_wiki_pages(
        &self,
        keyword: String,
        limit: usize,
    ) -> Result<Vec<WikiPageItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        let pages = db::search_wiki_pages(&db_path, &keyword, limit)?;

        Ok(pages
            .into_iter()
            .map(|page| {
                let display_path = friendly_display_path(Path::new(&page.path));
                WikiPageItem {
                    title: page.title,
                    path: page.path,
                    display_path: Some(display_path),
                    summary: page.summary,
                    updated_at: page.updated_at,
                }
            })
            .collect())
    }

    pub fn wiki_page_detail(&self, page_path: String) -> Result<WikiPageDetail, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let target_path = resolve_existing_wiki_page_path(&vault_path, &page_path)?;
        if target_path.extension().and_then(|v| v.to_str()) != Some("md") {
            return Err("仅支持读取 Markdown 页面".to_string());
        }

        let content =
            fs::read_to_string(&target_path).map_err(|err| format!("读取页面失败: {}", err))?;
        let title = extract_title_from_markdown(&content, &target_path);
        let frontmatter = parse_wiki_frontmatter(&content);
        let updated_at = file_modified_timestamp_ms(&target_path);

        Ok(WikiPageDetail {
            title,
            path: target_path.to_string_lossy().to_string(),
            display_path: friendly_display_path(&target_path),
            frontmatter,
            content,
            updated_at,
        })
    }

    pub fn wiki_page_citations(
        &self,
        page_path: String,
    ) -> Result<Vec<WikiPageCitationItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let target_path = resolve_existing_wiki_page_path(&vault_path, &page_path)?;
        let target_path_string = target_path.to_string_lossy().to_string();
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        let citations = db::list_citations_for_page(&db_path, &target_path_string)?;

        Ok(citations
            .into_iter()
            .map(|citation| {
                let cited_page_display_path =
                    friendly_display_path(Path::new(&citation.cited_page_path));
                WikiPageCitationItem {
                    cited_page_path: citation.cited_page_path.clone(),
                    cited_page_display_path: Some(cited_page_display_path),
                    score: citation.score,
                    excerpt: citation.excerpt,
                    target_exists: is_existing_wiki_page_target(
                        &vault_path,
                        &citation.cited_page_path,
                    ),
                }
            })
            .collect())
    }

    pub fn lint_report(&self) -> LintReport {
        let (mode, vault_path) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone())
        };
        let mut issues = Vec::new();

        let vault_path = match vault_path.as_ref() {
            Some(path) => path,
            None => {
                issues.push(LintIssue {
                    code: "VAULT_NOT_INITIALIZED".to_string(),
                    severity: "error".to_string(),
                    message: "尚未初始化 Vault".to_string(),
                    path: None,
                    suggestion: "先执行 init_vault 创建本地 Vault".to_string(),
                });
                return build_lint_report(mode, "Vault 未初始化".to_string(), issues);
            }
        };

        let index_path = vault_path.join("index.md");
        let (index_content, index_missing) = match fs::read_to_string(&index_path) {
            Ok(content) => (Some(content), false),
            Err(err) if err.kind() == io::ErrorKind::NotFound => (None, true),
            Err(err) => {
                issues.push(LintIssue {
                    code: "INDEX_READ_FAILED".to_string(),
                    severity: "error".to_string(),
                    message: format!("读取 index.md 失败: {}", err),
                    path: Some(index_path.to_string_lossy().to_string()),
                    suggestion: "检查 index.md 是否可读".to_string(),
                });
                (None, false)
            }
        };

        if index_missing {
            issues.push(LintIssue {
                code: "INDEX_MISSING".to_string(),
                severity: "error".to_string(),
                message: "index.md 缺失".to_string(),
                path: Some(index_path.to_string_lossy().to_string()),
                suggestion: "重新执行 init_vault 或补回 index.md".to_string(),
            });
        }

        let log_path = vault_path.join("log.md");
        if !log_path.exists() {
            issues.push(LintIssue {
                code: "LOG_MISSING".to_string(),
                severity: "error".to_string(),
                message: "log.md 缺失".to_string(),
                path: Some(log_path.to_string_lossy().to_string()),
                suggestion: "重新执行 init_vault 或补回 log.md".to_string(),
            });
        }

        let db_path = vault_path.join(".app").join("meta.db");
        if db_path.exists() {
            if let Err(err) = db::ensure_meta_db(&db_path) {
                issues.push(LintIssue {
                    code: "DB_SCHEMA_UPGRADE_FAILED".to_string(),
                    severity: "warning".to_string(),
                    message: format!("数据库结构校验失败: {}", err),
                    path: Some(db_path.to_string_lossy().to_string()),
                    suggestion: "检查数据库文件权限并重试".to_string(),
                });
            }
        }
        let db_paths = if db_path.exists() {
            match db::list_wiki_page_paths(&db_path) {
                Ok(paths) => Some(paths.into_iter().collect::<BTreeSet<_>>()),
                Err(err) => {
                    issues.push(LintIssue {
                        code: "DB_QUERY_FAILED".to_string(),
                        severity: "warning".to_string(),
                        message: format!("读取 wiki_pages 失败: {}", err),
                        path: Some(db_path.to_string_lossy().to_string()),
                        suggestion: "检查 SQLite 数据库结构是否完整".to_string(),
                    });
                    None
                }
            }
        } else {
            issues.push(LintIssue {
                code: "DB_MISSING".to_string(),
                severity: "error".to_string(),
                message: "meta.db 缺失".to_string(),
                path: Some(db_path.to_string_lossy().to_string()),
                suggestion: "重新执行 init_vault 生成 SQLite 数据库".to_string(),
            });
            None
        };

        if db_path.exists() {
            match db::list_citations(&db_path) {
                Ok(citations) => {
                    for citation in citations {
                        if !Path::new(&citation.page_path).exists() {
                            issues.push(LintIssue {
                                code: "BROKEN_CITING_PAGE".to_string(),
                                severity: "warning".to_string(),
                                message: format!("引用记录所属页面不存在: {}", citation.page_path),
                                path: Some(citation.page_path.clone()),
                                suggestion: "移除失效引用记录或恢复对应页面".to_string(),
                            });
                        }

                        if !Path::new(&citation.cited_page_path).exists() {
                            issues.push(LintIssue {
                                code: "BROKEN_CITATION".to_string(),
                                severity: "warning".to_string(),
                                message: format!(
                                    "引用目标页面不存在: {}",
                                    citation.cited_page_path
                                ),
                                path: Some(citation.cited_page_path.clone()),
                                suggestion: "修复引用路径或补回被引用页面".to_string(),
                            });
                        }
                    }
                }
                Err(err) => {
                    issues.push(LintIssue {
                        code: "CITATION_QUERY_FAILED".to_string(),
                        severity: "warning".to_string(),
                        message: format!("读取 citations 失败: {}", err),
                        path: Some(db_path.to_string_lossy().to_string()),
                        suggestion: "检查 SQLite 数据库结构是否完整".to_string(),
                    });
                }
            }
        }

        let wiki_dir = vault_path.join("wiki");
        let wiki_page_paths = collect_wiki_page_paths(&wiki_dir);

        if let Some(index_content) = index_content.as_ref() {
            let index_page_paths = collect_index_page_paths(index_content, vault_path);

            for path in index_page_paths.difference(&wiki_page_paths) {
                issues.push(LintIssue {
                    code: "MISSING_INDEX_ENTRY".to_string(),
                    severity: "error".to_string(),
                    message: format!("index.md 引用了不存在的页面: {}", path),
                    path: Some(path.clone()),
                    suggestion: "补齐对应的 vault/wiki 页面或修正 index.md 链接".to_string(),
                });
            }

            for path in wiki_page_paths.difference(&index_page_paths) {
                issues.push(LintIssue {
                    code: "ORPHAN_WIKI_PAGE".to_string(),
                    severity: "warning".to_string(),
                    message: format!("wiki 页面未被 index.md 引用: {}", path),
                    path: Some(path.clone()),
                    suggestion: "把页面加入 index.md，或确认该页面是否应保留".to_string(),
                });
            }

            if let Some(db_paths) = db_paths.as_ref() {
                for path in wiki_page_paths.difference(db_paths) {
                    issues.push(LintIssue {
                        code: "DB_MISSING_PAGE_RECORD".to_string(),
                        severity: "warning".to_string(),
                        message: format!("wiki_pages 表缺少页面记录: {}", path),
                        path: Some(path.clone()),
                        suggestion: "重新同步 wiki_pages 表记录".to_string(),
                    });
                }
            }
        }

        if db_path.exists() {
            match db::list_pending_tasks(&db_path) {
                Ok(tasks) => {
                    let checked_at_ms = current_timestamp_ms().parse::<u128>().unwrap_or_default();
                    for task in tasks {
                        if is_stale_pending_task(&task, checked_at_ms) {
                            issues.push(LintIssue {
                                code: "STALE_PENDING_TASK".to_string(),
                                severity: "warning".to_string(),
                                message: format!(
                                    "任务 {}（kind={}）处于 {} 状态且已超过陈旧阈值，raw={}",
                                    task.id, task.kind, task.status, task.raw_path
                                ),
                                path: Some(task.wiki_path.clone()),
                                suggestion: "推进任务状态或清理卡住的任务".to_string(),
                            });
                        }
                    }
                }
                Err(err) => {
                    issues.push(LintIssue {
                        code: "TASK_QUERY_FAILED".to_string(),
                        severity: "warning".to_string(),
                        message: format!("读取 tasks 失败: {}", err),
                        path: Some(db_path.to_string_lossy().to_string()),
                        suggestion: "检查 SQLite 数据库结构是否完整".to_string(),
                    });
                }
            }
        }

        if matches!(mode, AppMode::StrictLocal) {
            issues.push(LintIssue {
                code: "STRICT_LOCAL_GATE".to_string(),
                severity: "info".to_string(),
                message: "严格本地模式处于启用状态".to_string(),
                path: None,
                suggestion: "确保所有 Provider 调用都只走本地路径".to_string(),
            });
        }

        build_lint_report(
            mode,
            format!("已返回 {} 条 lint 问题", issues.len()),
            issues,
        )
    }

    pub fn preview_lint_patches(&self) -> LintPatchPreview {
        let report = self.lint_report();
        let suggestions = report
            .issues
            .iter()
            .map(build_lint_patch_suggestion)
            .collect::<Vec<_>>();

        LintPatchPreview {
            generated_at: current_timestamp_ms(),
            total: suggestions.len(),
            suggestions,
        }
    }

    /// 收集语义 Lint 所需的页面数据（同步，在 State 作用域内完成）。
    ///
    /// 返回 (页面列表[(path, title, summary)], mode)。
    fn collect_semantic_lint_input(&self) -> (Vec<(String, String, String)>, AppMode) {
        let (mode, vault_path) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone())
        };

        let pages = vault_path
            .map(|p| p.join(".app").join("meta.db"))
            .and_then(|db_path| db::list_recent_wiki_pages(&db_path, 20).ok())
            .map(|records| {
                records
                    .into_iter()
                    .map(|r| (r.path, r.title, r.summary))
                    .collect()
            })
            .unwrap_or_default();

        (pages, mode)
    }

    /// 执行 LLM 语义 Lint 分析（矛盾 / 陈旧 / 覆盖度）。
    ///
    /// - LLM 不可用时返回空列表，不报错。
    /// - 最多返回 10 条语义问题。
    async fn run_semantic_lint(
        pages: Vec<(String, String, String)>,
        provider: Option<Arc<OllamaProvider>>,
    ) -> Vec<LintIssue> {
        let provider = match provider {
            Some(p) => p,
            None => return Vec::new(),
        };

        if pages.is_empty() {
            return Vec::new();
        }

        // 构建页面摘要文本（每条摘要截断到 200 字符，控制 token 用量）
        let pages_text = pages
            .iter()
            .map(|(path, title, summary)| {
                let short: String = summary.chars().take(200).collect();
                format!("- [{}] {}\n  摘要: {}", path, title, short)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            "你是 Wiki 内容质量审查员。以下是 Wiki 页面列表（路径+标题+摘要）。\n\
请检查并报告以下3类问题，每行一个，用 | 分隔，严格按格式输出：\n\
CODE|severity|message|path|suggestion\n\
CODE 仅限：SEMANTIC_CONTRADICTION（矛盾陈述）、SEMANTIC_STALE（过时结论）、SEMANTIC_COVERAGE_GAP（缺少重要实体页）\n\
severity 仅限：warning 或 info\n\
path 填相关页面路径，无则留空\n\
若无问题则只输出：NO_ISSUES\n\n\
Wiki 页面：\n{}",
            pages_text
        );

        match provider.complete(&prompt).await {
            Ok(response) => parse_semantic_lint_response(&response),
            Err(_) => Vec::new(),
        }
    }

    /// 返回完整 Lint（规则 + 语义 LLM）的 Future，可在异步命令中安全 await。
    pub fn lint_report_full_future(
        &self,
    ) -> impl std::future::Future<Output = LintReport> + Send + 'static {
        let rules = self.lint_report();
        let (pages, _mode) = self.collect_semantic_lint_input();
        let provider = self.get_ollama_provider();
        async move {
            let semantic = Self::run_semantic_lint(pages, provider).await;
            merge_lint_with_semantic(rules, semantic)
        }
    }

    pub fn apply_lint_patch(
        &self,
        input: LintPatchApplyInput,
    ) -> Result<LintPatchApplyResult, String> {
        let issue_code = input.issue_code.trim().to_string();
        let input_path = input.path.clone();
        if issue_code.is_empty() {
            return Err("issue_code 不能为空".to_string());
        }

        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };

        let (applied, message, touched_paths) = match issue_code.as_str() {
            "MISSING_INDEX_ENTRY" => {
                let path = input_path
                    .as_deref()
                    .ok_or_else(|| "MISSING_INDEX_ENTRY 需要提供 path".to_string())?;
                let page_path = resolve_existing_wiki_page_path(&vault_path, path)?;
                let index_path = vault_path.join("index.md");
                if !index_path.exists() {
                    return Err("index.md 缺失，请先处理 INDEX_MISSING".to_string());
                }

                let link_target = wiki_link_target_from_path(&vault_path, &page_path)?;
                let link_label = wiki_link_label(&page_path);
                let changed = append_index_link_if_missing(&index_path, &link_target, &link_label)?;
                let message = if changed {
                    "已补齐 index.md 引用".to_string()
                } else {
                    "index.md 中已存在该页面引用，未重复写入".to_string()
                };
                let mut touched_paths = vec![index_path.to_string_lossy().to_string()];
                touched_paths.push(page_path.to_string_lossy().to_string());
                (changed, message, touched_paths)
            }
            "ORPHAN_WIKI_PAGE" => {
                let path = input_path
                    .as_deref()
                    .ok_or_else(|| "ORPHAN_WIKI_PAGE 需要提供 path".to_string())?;
                let page_path = resolve_existing_wiki_page_path(&vault_path, path)?;
                let index_path = vault_path.join("index.md");
                if !index_path.exists() {
                    return Err("index.md 缺失，请先处理 INDEX_MISSING".to_string());
                }

                let link_target = wiki_link_target_from_path(&vault_path, &page_path)?;
                let link_label = wiki_link_label(&page_path);
                let changed = append_index_link_if_missing(&index_path, &link_target, &link_label)?;
                let message = if changed {
                    "已将页面加入 index.md".to_string()
                } else {
                    "index.md 中已存在该页面引用，未重复写入".to_string()
                };
                let mut touched_paths = vec![index_path.to_string_lossy().to_string()];
                touched_paths.push(page_path.to_string_lossy().to_string());
                (changed, message, touched_paths)
            }
            "INDEX_MISSING" => {
                let index_path = vault_path.join("index.md");
                let created = if index_path.exists() {
                    false
                } else {
                    fs::write(&index_path, seed_index_content())
                        .map_err(|err| format!("写入 index.md 失败: {}", err))?;
                    true
                };
                let message = if created {
                    "已创建 index.md".to_string()
                } else {
                    "index.md 已存在，未作修改".to_string()
                };
                (
                    created,
                    message,
                    vec![index_path.to_string_lossy().to_string()],
                )
            }
            "LOG_MISSING" => {
                let log_path = vault_path.join("log.md");
                let created = if log_path.exists() {
                    false
                } else {
                    fs::write(&log_path, seed_log_content())
                        .map_err(|err| format!("写入 log.md 失败: {}", err))?;
                    true
                };
                let message = if created {
                    "已创建 log.md".to_string()
                } else {
                    "log.md 已存在，未作修改".to_string()
                };
                (
                    created,
                    message,
                    vec![log_path.to_string_lossy().to_string()],
                )
            }
            _ => {
                return Err("暂不支持自动应用，请手动处理".to_string());
            }
        };

        self.push_log(
            LogLevel::Info,
            format!(
                "Lint 补丁应用完成: issue_code={}, path={}, applied={}, message={}",
                issue_code,
                input_path.as_deref().unwrap_or("无"),
                applied,
                message
            ),
        );

        self.record_lint_patch_event(
            &vault_path,
            &issue_code,
            input_path.as_deref(),
            applied,
            &message,
        );

        Ok(LintPatchApplyResult {
            issue_code,
            path: input_path,
            applied,
            message,
            touched_paths,
        })
    }

    pub fn apply_lint_patches_batch(
        &self,
        inputs: Vec<LintPatchApplyInput>,
    ) -> Result<LintPatchBatchApplyResult, String> {
        let total = inputs.len();
        let mut success = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;
        let mut items = Vec::with_capacity(total);

        for input in inputs {
            let issue_code = input.issue_code.trim().to_string();
            let path = input.path.clone();

            match self.apply_lint_patch(input) {
                Ok(result) => {
                    let status = if result.applied {
                        success += 1;
                        LintPatchBatchApplyStatus::Success
                    } else {
                        skipped += 1;
                        LintPatchBatchApplyStatus::Skipped
                    };

                    items.push(LintPatchBatchApplyItemResult {
                        issue_code: result.issue_code,
                        path: result.path,
                        status,
                        applied: result.applied,
                        message: result.message,
                        touched_paths: result.touched_paths,
                        error: None,
                    });
                }
                Err(error) => {
                    failed += 1;
                    items.push(LintPatchBatchApplyItemResult {
                        issue_code,
                        path,
                        status: LintPatchBatchApplyStatus::Failed,
                        applied: false,
                        message: error.clone(),
                        touched_paths: Vec::new(),
                        error: Some(error),
                    });
                }
            }
        }

        self.push_log(
            LogLevel::Info,
            format!(
                "批量应用 Lint 补丁完成：total={}，success={}，failed={}，skipped={}",
                total, success, failed, skipped
            ),
        );

        Ok(LintPatchBatchApplyResult {
            total,
            success,
            failed,
            skipped,
            items,
        })
    }

    pub async fn query_ask(&self, question: String) -> Result<QueryAnswerResult, String> {
        self.query_ask_with_options(question, QueryAskOptions::default())
            .await
    }

    pub async fn query_ask_with_options(
        &self,
        question: String,
        options: QueryAskOptions,
    ) -> Result<QueryAnswerResult, String> {
        let normalized_question = question.trim().to_string();
        if normalized_question.is_empty() {
            return Err("问题不能为空".to_string());
        }

        let (mode, vault_path, default_top_k) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone(), guard.query_top_k)
        };

        let vault_path =
            vault_path.ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let wiki_dir = vault_path.join("wiki");
        let db_path = vault_path.join(".app").join("meta.db");
        let tokens = tokenize_query(&normalized_question);
        let top_k = normalize_top_k(options.top_k.or(Some(default_top_k)));

        // 步骤1：FTS 检索
        self.emit_progress("query_progress", "searching", "FTS 检索中...");
        let (matches, search_strategy, fts_error) = search_wiki_matches_with_fts(
            &db_path,
            &wiki_dir,
            &tokens,
            &normalized_question,
            top_k,
        )?;

        if let Some(err) = fts_error {
            self.push_log(
                LogLevel::Warn,
                format!("FTS 查询失败，已降级为文件扫描: {}", err),
            );
        }

        let citations = matches
            .iter()
            .map(|item| {
                let display_path = friendly_display_path(Path::new(&item.page_path));
                QueryCitation {
                    page_path: item.page_path.clone(),
                    display_path: Some(display_path),
                    score: item.score,
                    excerpt: item.excerpt.clone(),
                }
            })
            .collect::<Vec<_>>();

        // 步骤2：LLM 合成回答
        self.emit_progress("query_progress", "generating", "正在合成回答（LLM）...");
        let provider = self.get_llm_provider();
        let (answer, answer_strategy) = self
            .generate_query_answer_with_provider(&normalized_question, &matches, provider)
            .await;

        let matched_pages = matches
            .iter()
            .map(|item| item.page_path.clone())
            .collect::<Vec<_>>();

        self.push_log(
            LogLevel::Info,
            format!(
                "Query 检索完成: '{}'，命中 {} 页，检索策略={}，回答策略={}，top_k={}",
                normalized_question,
                matched_pages.len(),
                search_strategy,
                answer_strategy,
                top_k
            ),
        );

        Ok(QueryAnswerResult {
            question: normalized_question,
            answer,
            citations,
            matched_pages,
            mode,
            checked_at: current_timestamp_ms(),
            search_strategy: search_strategy.to_string(),
            answer_strategy,
        })
    }

    pub fn set_query_top_k(&self, top_k: usize) -> Result<QuerySettings, String> {
        let normalized_top_k = normalize_top_k(Some(top_k));
        let (mode, vault_path, expected_snapshot) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (
                guard.mode,
                guard.vault_path.clone(),
                guard.config_snapshot.clone(),
            )
        };

        match self.persist_config(
            mode,
            vault_path.as_deref(),
            normalized_top_k,
            expected_snapshot.as_deref(),
        ) {
            Ok(serialized) => {
                let mut guard = self.inner.lock().expect("状态锁已被污染");
                guard.query_top_k = normalized_top_k;
                guard.config_snapshot = Some(serialized);
                guard.push_log(
                    LogLevel::Info,
                    format!("Query TopK 已更新为 {}", normalized_top_k),
                    current_timestamp_ms(),
                );
                Ok(QuerySettings {
                    top_k: normalized_top_k,
                    min_top_k: QUERY_TOP_K_MIN,
                    max_top_k: QUERY_TOP_K_MAX,
                })
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("Query TopK 持久化失败: {}", err));
                Err(err)
            }
        }
    }

    pub fn save_query_answer(
        &self,
        input: SaveQueryAnswerInput,
    ) -> Result<SaveQueryAnswerResult, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };

        match vault::save_query_answer(&vault_path, &input) {
            Ok(result) => {
                self.push_log(
                    LogLevel::Info,
                    format!("Query 结果已保存: {}", result.wiki_path),
                );
                Ok(result)
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("保存 Query 结果失败: {}", err));
                Err(err)
            }
        }
    }

    /// 将编辑后的内容写回 vault 文件，并同步更新 SQLite FTS 索引。
    pub async fn save_wiki_page_impl(
        &self,
        path: &str,
        content: &str,
    ) -> Result<crate::models::SaveWikiPageResult, String> {
        // 1. 写入文件
        let changed = crate::vault::write_wiki_page(path, content)?;

        if !changed {
            return Ok(crate::models::SaveWikiPageResult {
                path: path.to_string(),
                message: "内容未变化，跳过写入".to_string(),
            });
        }

        // 2. 更新 SQLite FTS 索引（复用已有逻辑）
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };

        if let Some(vault_path) = vault_path {
            let db_path = vault_path.join(".app").join("meta.db");
            if db_path.exists() {
                // 提取标题（取第一个 # 标题，或用文件名）
                let title = content
                    .lines()
                    .find(|l| l.starts_with("# "))
                    .map(|l| l.trim_start_matches("# ").trim().to_string())
                    .unwrap_or_else(|| {
                        std::path::Path::new(path)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(path)
                            .to_string()
                    });
                let body = content.to_string();
                // 更新 FTS 索引（失败时仅记录警告，不阻断主流程）
                if let Err(err) =
                    db::upsert_fts_page(&db_path, std::path::Path::new(path), &title, &body)
                {
                    self.push_log(LogLevel::Warn, format!("FTS 索引更新失败（降级）：{err}"));
                }
            }
        }

        Ok(crate::models::SaveWikiPageResult {
            path: path.to_string(),
            message: format!("已保存并更新索引：{path}"),
        })
    }

    fn set_vault_path(&self, vault_path: PathBuf) -> Result<(), String> {
        let (mode, query_top_k, expected_snapshot) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.query_top_k, guard.config_snapshot.clone())
        };

        {
            let mut guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path = Some(vault_path.clone());
        }

        match self.persist_config(
            mode,
            Some(vault_path.as_path()),
            query_top_k,
            expected_snapshot.as_deref(),
        ) {
            Ok(serialized) => {
                let mut guard = self.inner.lock().expect("状态锁已被污染");
                guard.config_snapshot = Some(serialized);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn default_config_path() -> PathBuf {
        Self::default_config_path_from_root(&Self::project_root())
    }

    fn project_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(manifest_dir)
    }

    fn default_config_path_from_root(root: &Path) -> PathBuf {
        root.join(".runtime").join("app-config.json")
    }

    fn load_config(config_path: &Path) -> (AppConfig, Option<String>) {
        match fs::read_to_string(config_path) {
            Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
                Ok(config) => (config, Some(raw)),
                Err(_) => (AppConfig::default(), Some(raw)),
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => (AppConfig::default(), None),
            Err(_) => (AppConfig::default(), None),
        }
    }

    /// 将运行时配置序列化为新字段格式。
    fn serialize_config_full(config: &AppConfig) -> String {
        serde_json::to_string_pretty(config).expect("配置序列化失败")
    }

    fn persist_config(
        &self,
        mode: AppMode,
        vault_path: Option<&Path>,
        query_top_k: usize,
        expected_snapshot: Option<&str>,
    ) -> Result<String, String> {
        // 从当前 guard 读取云端字段，确保不丢失已保存的配置
        let config = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            AppConfig {
                mode,
                vault_path: vault_path.map(|path| path.to_string_lossy().to_string()),
                query_top_k: Some(query_top_k),
                cloud_api_key: guard.cloud_api_key.clone(),
                cloud_base_url: guard.cloud_base_url.clone(),
                cloud_model: guard.cloud_model.clone(),
                cloud_provider_name: guard.cloud_provider_name.clone(),
                active_provider: guard.active_provider.clone(),
            }
        };
        let serialized = Self::serialize_config_full(&config);

        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("创建配置目录失败: {}", err))?;
        }

        let current_snapshot = match fs::read_to_string(&self.config_path) {
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

        Self::write_config_file(&self.config_path, &serialized, current_snapshot.as_deref())?;
        Ok(serialized)
    }

    fn write_config_file(
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

    fn push_log(&self, level: LogLevel, message: String) {
        let mut guard = self.inner.lock().expect("状态锁已被污染");
        guard.push_log(level, message, current_timestamp_ms());
    }

    fn record_lint_patch_event(
        &self,
        vault_path: &Path,
        issue_code: &str,
        path: Option<&str>,
        applied: bool,
        message: &str,
    ) {
        let db_path = vault_path.join(".app").join("meta.db");
        let timestamp_ms = current_timestamp_ms();

        if let Err(err) =
            db::insert_lint_patch_event(&db_path, issue_code, path, applied, message, &timestamp_ms)
        {
            self.push_log(
                LogLevel::Warn,
                format!("写入 lint_patch_events 失败: {}", err),
            );
        }
    }
}

fn validate_pdf_source_path(source_path: &Path) -> Result<(), String> {
    if !source_path.exists() {
        return Err(format!("PDF 文件不存在：{}", source_path.to_string_lossy()));
    }

    if !source_path.is_file() {
        return Err(format!(
            "PDF 路径不是文件：{}",
            source_path.to_string_lossy()
        ));
    }

    let ext = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext != "pdf" {
        return Err(format!(
            "文件扩展名错误，仅支持 .pdf：{}",
            source_path.to_string_lossy()
        ));
    }

    Ok(())
}

fn extract_text_from_pdf(source_path: &Path) -> Result<String, String> {
    let document = lopdf::Document::load(source_path)
        .map_err(|err| format!("读取 PDF 失败，文件内容可能不是有效 PDF：{err}"))?;
    let page_numbers: Vec<u32> = document.get_pages().keys().copied().collect();

    if page_numbers.is_empty() {
        return Err("PDF 不包含可读取页面".to_string());
    }

    let text = document
        .extract_text(&page_numbers)
        .map_err(|err| format!("提取 PDF 文本失败：{err}"))?;
    let normalized = text.replace('\u{0}', "").trim().to_string();

    if normalized.is_empty() {
        return Err("PDF 文本为空，可能是扫描件或图片型 PDF".to_string());
    }

    Ok(normalized)
}

/// 生成基于微秒时间戳的短唯一字符串，用于临时文件命名。
fn uuid_v4_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros().to_string())
        .unwrap_or_else(|_| "tmp".to_string())
}

fn collect_wiki_page_paths(wiki_dir: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();

    // 只扫描 vault/wiki 顶层 Markdown 页面。
    let Ok(entries) = fs::read_dir(wiki_dir) else {
        return paths;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        paths.insert(path.to_string_lossy().to_string());
    }

    paths
}

fn collect_index_page_paths(index_content: &str, vault_path: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();

    // 解析 index.md 中的 wiki 与 Markdown 引用。
    for target in extract_wiki_link_targets(index_content) {
        if let Some(path) = resolve_wiki_link_target(vault_path, &target) {
            paths.insert(path);
        }
    }

    for target in extract_markdown_link_targets(index_content) {
        if let Some(path) = resolve_wiki_link_target(vault_path, &target) {
            paths.insert(path);
        }
    }

    paths
}

fn extract_wiki_link_targets(content: &str) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let mut offset = 0;

    while let Some(start) = content[offset..].find("[[") {
        let start = offset + start + 2;
        let Some(end_rel) = content[start..].find("]]") else {
            break;
        };
        let inner = &content[start..start + end_rel];
        if let Some(target) = inner.split('|').next() {
            let target = target.trim();
            if !target.is_empty() {
                targets.insert(target.to_string());
            }
        }
        offset = start + end_rel + 2;
    }

    targets
}

fn extract_markdown_link_targets(content: &str) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let mut offset = 0;

    while let Some(start) = content[offset..].find("](") {
        let start = offset + start + 2;
        let Some(end_rel) = content[start..].find(')') else {
            break;
        };
        let target = content[start..start + end_rel].trim();
        if !target.is_empty() {
            targets.insert(target.to_string());
        }
        offset = start + end_rel + 1;
    }

    targets
}

fn resolve_wiki_link_target(vault_path: &Path, raw_target: &str) -> Option<String> {
    let target = raw_target
        .split('|')
        .next()
        .unwrap_or(raw_target)
        .split('#')
        .next()
        .unwrap_or(raw_target)
        .split('^')
        .next()
        .unwrap_or(raw_target)
        .trim();

    let relative = target
        .strip_prefix("wiki/")
        .or_else(|| target.strip_prefix("wiki\\"))
        .or_else(|| target.strip_prefix("./wiki/"))
        .or_else(|| target.strip_prefix("./wiki\\"))?;

    let relative = if relative.ends_with(".md") {
        relative.to_string()
    } else {
        format!("{}.md", relative)
    };

    Some(
        vault_path
            .join("wiki")
            .join(relative)
            .to_string_lossy()
            .to_string(),
    )
}

fn is_stale_pending_task(task: &db::PendingTaskRecord, checked_at_ms: u128) -> bool {
    let updated_at_ms = task
        .updated_at
        .parse::<u128>()
        .ok()
        .or_else(|| task.created_at.parse::<u128>().ok());

    // 以更新时间判断是否已经卡住。
    match updated_at_ms {
        Some(value) => checked_at_ms.saturating_sub(value) > STALE_PENDING_TASK_THRESHOLD_MS,
        None => true,
    }
}

impl AppStateData {
    fn push_log(&mut self, level: LogLevel, message: String, created_at: String) {
        let entry = LogEntry {
            id: self.next_log_id,
            level,
            message,
            created_at,
        };
        self.next_log_id += 1;
        self.logs.push(entry);
    }
}

fn current_timestamp_ms() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

/// LLM 语义 Lint 合法 code 列表。
const SEMANTIC_LINT_CODES: &[&str] = &[
    "SEMANTIC_CONTRADICTION",
    "SEMANTIC_STALE",
    "SEMANTIC_COVERAGE_GAP",
];

/// 解析 LLM 返回的语义 Lint 文本为 LintIssue 列表。
///
/// 格式要求：每行 `CODE|severity|message|path|suggestion`，
/// 非法行静默跳过，最多返回 10 条。
fn parse_semantic_lint_response(response: &str) -> Vec<LintIssue> {
    response
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.eq_ignore_ascii_case("NO_ISSUES") {
                return None;
            }
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() < 5 {
                return None;
            }
            let code = parts[0].trim();
            let severity = parts[1].trim();
            let message = parts[2].trim();
            let path = parts[3].trim();
            let suggestion = parts[4].trim();

            if !SEMANTIC_LINT_CODES.contains(&code) {
                return None;
            }
            if severity != "warning" && severity != "info" {
                return None;
            }
            if message.is_empty() || suggestion.is_empty() {
                return None;
            }

            Some(LintIssue {
                code: code.to_string(),
                severity: severity.to_string(),
                message: message.to_string(),
                path: if path.is_empty() {
                    None
                } else {
                    Some(path.to_string())
                },
                suggestion: suggestion.to_string(),
            })
        })
        .take(10)
        .collect()
}

/// 将语义问题合并进规则 Lint 报告，更新统计与摘要。
fn merge_lint_with_semantic(mut rules: LintReport, semantic: Vec<LintIssue>) -> LintReport {
    if semantic.is_empty() {
        return rules;
    }
    for issue in &semantic {
        match issue.severity.as_str() {
            "error" => rules.severity_stats.error += 1,
            "warning" => rules.severity_stats.warning += 1,
            "info" => rules.severity_stats.info += 1,
            _ => {}
        }
    }
    rules.issues.extend(semantic);
    let total = rules.issues.len();
    rules.summary = format!("共发现 {} 个问题（规则 + 语义分析）", total);
    rules
}

fn build_lint_report(mode: AppMode, summary: String, issues: Vec<LintIssue>) -> LintReport {
    let severity_stats = count_lint_severity_stats(&issues);

    LintReport {
        mode,
        checked_at: current_timestamp_ms(),
        summary,
        issues,
        severity_stats,
    }
}

fn count_lint_severity_stats(issues: &[LintIssue]) -> LintSeverityStats {
    let mut stats = LintSeverityStats::default();

    for issue in issues {
        match issue.severity.to_ascii_lowercase().as_str() {
            "error" => stats.error += 1,
            "warning" => stats.warning += 1,
            "info" => stats.info += 1,
            _ => {}
        }
    }

    stats
}

fn lint_patch_link_target(path: Option<&str>) -> String {
    let file_name = path
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("xxx.md");
    format!("wiki/{}", file_name)
}

fn lint_patch_link_label(path: Option<&str>) -> String {
    path.and_then(|value| Path::new(value).file_stem())
        .and_then(|value| value.to_str())
        .unwrap_or("xxx")
        .to_string()
}

fn build_lint_patch_suggestion(issue: &LintIssue) -> LintPatchSuggestion {
    let (title, proposed_action, patch_preview) = match issue.code.as_str() {
        "VAULT_NOT_INITIALIZED" => (
            "初始化 Vault".to_string(),
            "先执行 init_vault 创建本地 Vault".to_string(),
            "```text\n执行 init_vault 后，系统会生成 vault/index.md、vault/log.md 和 .app/meta.db。\n```"
                .to_string(),
        ),
        "INDEX_READ_FAILED" => (
            "检查 index.md 读取".to_string(),
            "确认 index.md 可读并修复文件权限或编码问题".to_string(),
            format!(
                "```text\n检查文件：{}\n若文件可读性异常，修复后重新运行 lint。\n```",
                issue.path.as_deref().unwrap_or("index.md")
            ),
        ),
        "INDEX_MISSING" => (
            "补回 index.md".to_string(),
            "重新执行 init_vault 或补回 index.md".to_string(),
            "```text\n# Index\n\n## Imported Pages\n- [[wiki/xxx.md|xxx]]\n```".to_string(),
        ),
        "LOG_MISSING" => (
            "补回 log.md".to_string(),
            "重新执行 init_vault 或补回 log.md".to_string(),
            "```text\n# Log\n\n## 事件日志\n```".to_string(),
        ),
        "DB_SCHEMA_UPGRADE_FAILED" | "DB_MISSING" | "DB_QUERY_FAILED" => (
            "检查 meta.db".to_string(),
            "确认 SQLite 数据库可用并重试结构校验".to_string(),
            "```text\n确认 .app/meta.db 可读写，并检查数据库结构是否完整。\n```"
                .to_string(),
        ),
        "CITATION_QUERY_FAILED" => (
            "检查 citations 查询".to_string(),
            "确认 citations 表可查询并修复数据库结构".to_string(),
            "```text\n检查 citations 表与相关索引是否存在。\n```".to_string(),
        ),
        "BROKEN_CITING_PAGE" => (
            "处理失效引用所属页面".to_string(),
            "恢复对应页面或移除失效引用记录".to_string(),
            format!(
                "```text\n引用所属页面不存在：{}\n建议恢复页面或清理引用记录。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "BROKEN_CITATION" => (
            "修复引用目标页面".to_string(),
            "补回被引用页面或修正引用路径".to_string(),
            format!(
                "```text\n引用目标缺失：{}\n建议修复引用路径或补回页面。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "MISSING_INDEX_ENTRY" => (
            "补齐 index 引用".to_string(),
            "把缺失页面加入 index.md".to_string(),
            format!(
                "```text\n- [[{}|{}]]\n```",
                lint_patch_link_target(issue.path.as_deref()),
                lint_patch_link_label(issue.path.as_deref())
            ),
        ),
        "ORPHAN_WIKI_PAGE" => (
            "把页面挂回 index.md".to_string(),
            "将该页面加入 index.md，或确认其应保留为孤页".to_string(),
            format!(
                "```text\n- [[{}|{}]]\n```",
                lint_patch_link_target(issue.path.as_deref()),
                lint_patch_link_label(issue.path.as_deref())
            ),
        ),
        "DB_MISSING_PAGE_RECORD" => (
            "同步 wiki_pages 记录".to_string(),
            "重新同步 wiki_pages 表记录".to_string(),
            format!(
                "```text\n补写 wiki_pages 记录以匹配页面：{}\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "STALE_PENDING_TASK" => (
            "推进卡住的任务".to_string(),
            "更新任务状态或清理陈旧任务".to_string(),
            format!(
                "```text\n任务路径：{}\n建议推进状态到 applied/failed，或清理过期任务。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "TASK_QUERY_FAILED" => (
            "检查任务查询".to_string(),
            "确认 tasks 表可查询并修复数据库结构".to_string(),
            "```text\n检查 tasks 表与数据库可读性。\n```".to_string(),
        ),
        "STRICT_LOCAL_GATE" => (
            "严格本地模式提示".to_string(),
            "无需修改；仅确认当前运行在严格本地模式".to_string(),
            "```text\n该项为信息提示，无需应用补丁。\n```".to_string(),
        ),
        _ => (
            "检查问题".to_string(),
            "根据 lint 结果进行人工确认".to_string(),
            format!(
                "```text\n问题代码：{}\n路径：{}\n```",
                issue.code,
                issue.path.as_deref().unwrap_or("全局")
            ),
        ),
    };

    LintPatchSuggestion {
        issue_code: issue.code.clone(),
        path: issue.path.clone(),
        title,
        proposed_action,
        patch_preview,
    }
}

fn file_modified_timestamp_ms(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|dur| dur.as_millis().to_string())
        .unwrap_or_else(current_timestamp_ms)
}

fn resolve_wiki_page_candidate(vault_path: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let wiki_root = vault_path.join("wiki");
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("页面路径不能为空".to_string());
    }

    let input_path = PathBuf::from(trimmed);
    Ok(if input_path.is_absolute() {
        input_path
    } else if trimmed.starts_with("wiki/")
        || trimmed.starts_with("wiki\\")
        || trimmed.starts_with("./wiki/")
        || trimmed.starts_with("./wiki\\")
    {
        vault_path.join(trimmed)
    } else {
        wiki_root.join(input_path)
    })
}

fn resolve_existing_wiki_page_path(vault_path: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let wiki_root = vault_path.join("wiki");
    let candidate = resolve_wiki_page_candidate(vault_path, raw_path)?;
    if !candidate.exists() {
        return Err("页面不存在".to_string());
    }

    // 安全约束：只允许读取当前 vault/wiki 目录内文件，禁止越界访问。
    let canonical_root =
        fs::canonicalize(&wiki_root).map_err(|err| format!("解析 wiki 根目录失败: {}", err))?;
    let canonical_target =
        fs::canonicalize(&candidate).map_err(|err| format!("解析页面路径失败: {}", err))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err("只允许读取 vault/wiki 目录下的页面".to_string());
    }

    Ok(canonical_target)
}

fn is_existing_wiki_page_target(vault_path: &Path, raw_path: &str) -> bool {
    let Ok(candidate) = resolve_wiki_page_candidate(vault_path, raw_path) else {
        return false;
    };
    if !candidate.exists() {
        return false;
    }

    let wiki_root = vault_path.join("wiki");
    let Ok(canonical_root) = fs::canonicalize(&wiki_root) else {
        return false;
    };
    let Ok(canonical_target) = fs::canonicalize(&candidate) else {
        return false;
    };

    canonical_target.starts_with(&canonical_root)
}

fn wiki_link_target_from_path(vault_path: &Path, page_path: &Path) -> Result<String, String> {
    let wiki_root = fs::canonicalize(vault_path.join("wiki"))
        .map_err(|err| format!("解析 wiki 根目录失败: {}", err))?;
    let canonical_page =
        fs::canonicalize(page_path).map_err(|err| format!("解析页面路径失败: {}", err))?;
    let relative = canonical_page
        .strip_prefix(&wiki_root)
        .map_err(|_| "页面不在 vault/wiki 目录下".to_string())?;

    Ok(format!(
        "wiki/{}",
        relative.to_string_lossy().replace('\\', "/")
    ))
}

fn wiki_link_label(page_path: &Path) -> String {
    page_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("xxx")
        .to_string()
}

fn append_index_link_if_missing(
    index_path: &Path,
    link_target: &str,
    label: &str,
) -> Result<bool, String> {
    let existing =
        fs::read_to_string(index_path).map_err(|err| format!("读取 index.md 失败: {}", err))?;
    let link = format!("[[{}|{}]]", link_target, label);
    if existing.contains(&link) {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&format!("- {}\n", link));
    fs::write(index_path, updated).map_err(|err| format!("写入 index.md 失败: {}", err))?;
    Ok(true)
}

fn seed_index_content() -> &'static str {
    "# Index\n\n## Imported Pages\n"
}

fn seed_log_content() -> &'static str {
    "# Log\n\n## Event Log\n"
}

#[derive(Debug, Clone)]
struct WikiMatch {
    page_path: String,
    score: usize,
    excerpt: String,
}

fn tokenize_query(question: &str) -> Vec<String> {
    // 轻量混合分词：保留英文 token，同时为连续中文片段生成 2-gram，提升中文命中率。
    let mut tokens = question
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    for segment in extract_cjk_segments(question) {
        let chars = segment.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            continue;
        }
        tokens.push(segment.clone());
        if chars.len() >= 2 {
            for window in chars.windows(2) {
                let gram = window.iter().collect::<String>();
                tokens.push(gram);
            }
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
        .into_iter()
        .filter(|token| !is_stopword(token))
        .collect()
}

fn extract_cjk_segments(input: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if is_cjk(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            segments.push(current.clone());
            current.clear();
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF // CJK Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
    )
}

fn is_stopword(token: &str) -> bool {
    const ZH_STOPWORDS: &[&str] = &[
        "的", "了", "是", "吗", "呢", "和", "与", "及", "在", "对", "把", "将",
    ];
    const EN_STOPWORDS: &[&str] = &["the", "is", "are", "a", "an", "of", "to", "for"];

    ZH_STOPWORDS.contains(&token) || EN_STOPWORDS.contains(&token)
}

fn normalize_top_k(top_k: Option<usize>) -> usize {
    top_k
        .unwrap_or(QUERY_TOP_K_DEFAULT)
        .clamp(QUERY_TOP_K_MIN, QUERY_TOP_K_MAX)
}

fn search_wiki_matches(
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<Vec<WikiMatch>, String> {
    let entries = match fs::read_dir(wiki_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("读取 wiki 目录失败: {}", err)),
    };

    let page_paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    search_wiki_matches_from_paths(&page_paths, tokens, question, limit)
}

fn search_wiki_matches_with_fts(
    db_path: &Path,
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<(Vec<WikiMatch>, &'static str, Option<String>), String> {
    if tokens.is_empty() {
        return Ok((Vec::new(), "empty", None));
    }

    match db::search_fts_page_paths(db_path, tokens, 64) {
        Ok(paths) if !paths.is_empty() => {
            let matches = search_wiki_matches_from_paths(&paths, tokens, question, limit)?;
            if !matches.is_empty() {
                return Ok((matches, "fts", None));
            }
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            Ok((fallback, "scan", None))
        }
        Ok(_) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            Ok((fallback, "scan", None))
        }
        Err(err) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            Ok((fallback, "scan", Some(err)))
        }
    }
}

fn search_wiki_matches_from_paths(
    page_paths: &[String],
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<Vec<WikiMatch>, String> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let phrase = question.trim().to_lowercase();
    let mut results = Vec::new();
    let mut seen_paths = HashSet::new();

    for page_path in page_paths {
        let path = PathBuf::from(page_path);
        let canonical = path.to_string_lossy().to_string();
        if !seen_paths.insert(canonical) {
            continue;
        }
        if !path.exists() {
            continue;
        }
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(err) => return Err(format!("读取页面失败 {}: {}", path.to_string_lossy(), err)),
        };
        let lowered = content.to_lowercase();
        let title = extract_title_from_markdown(&content, &path);
        let lowered_title = title.to_lowercase();

        // 综合评分：正文命中 + 标题命中加权 + 短语命中加权。
        let token_hits = tokens
            .iter()
            .map(|token| lowered.matches(token).count())
            .sum::<usize>();
        let title_hits = tokens
            .iter()
            .filter(|token| lowered_title.contains(token.as_str()))
            .count();
        let phrase_hit = usize::from(!phrase.is_empty() && lowered.contains(&phrase));
        let score = token_hits + title_hits * 3 + phrase_hit * 5;
        if score == 0 {
            continue;
        }

        let excerpt = pick_excerpt(&content, tokens);
        results.push(WikiMatch {
            page_path: path.to_string_lossy().to_string(),
            score,
            excerpt,
        });
    }

    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.page_path.cmp(&right.page_path))
    });
    if results.len() > limit {
        results.truncate(limit);
    }
    Ok(results)
}

fn parse_wiki_frontmatter(content: &str) -> Option<WikiPageFrontmatter> {
    let block = extract_frontmatter_block(content)?;
    let mut frontmatter = WikiPageFrontmatter {
        title: None,
        source: None,
        raw: None,
        imported_at: None,
        entities: Vec::new(),
    };
    let mut lines = block.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("title:") {
            frontmatter.title = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("source:") {
            frontmatter.source = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("raw:") {
            frontmatter.raw = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("imported_at:") {
            frontmatter.imported_at = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("entities:") {
            let inline = value.trim();
            if inline == "[]" {
                continue;
            }
            if !inline.is_empty() {
                if let Some(entity) = parse_frontmatter_scalar(inline) {
                    if !entity.is_empty() {
                        frontmatter.entities.push(entity);
                    }
                }
                continue;
            }

            while let Some(next_line) = lines.peek().copied() {
                let next_trimmed = next_line.trim();
                if next_trimmed.is_empty() {
                    lines.next();
                    continue;
                }

                if let Some(entity) = next_trimmed.strip_prefix("- ") {
                    if let Some(entity_value) = parse_frontmatter_scalar(entity) {
                        if !entity_value.is_empty() {
                            frontmatter.entities.push(entity_value);
                        }
                    }
                    lines.next();
                    continue;
                }

                break;
            }
        }
    }

    let has_scalar_fields = frontmatter.title.is_some()
        || frontmatter.source.is_some()
        || frontmatter.raw.is_some()
        || frontmatter.imported_at.is_some();
    if has_scalar_fields || !frontmatter.entities.is_empty() {
        Some(frontmatter)
    } else {
        None
    }
}

fn extract_frontmatter_block(content: &str) -> Option<String> {
    let normalized = content.replace("\r\n", "\n");
    let mut lines = normalized.lines();

    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut block_lines = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(block_lines.join("\n"));
        }
        block_lines.push(line.to_string());
    }

    None
}

fn parse_frontmatter_scalar(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let body = &trimmed[1..trimmed.len() - 1];
        return Some(unescape_yaml_double_quoted(body));
    }

    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        let body = &trimmed[1..trimmed.len() - 1];
        return Some(body.to_string());
    }

    Some(trimmed.to_string())
}

fn unescape_yaml_double_quoted(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                output.push(next);
            } else {
                output.push('\\');
            }
        } else {
            output.push(ch);
        }
    }
    output
}

fn extract_title_from_markdown(content: &str, path: &Path) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }

    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn friendly_display_path(path: &Path) -> String {
    let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let normalized = normalized.to_string_lossy();
    friendly_display_path_str(normalized.as_ref())
}

fn friendly_display_path_str(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", stripped);
    }

    if let Some(stripped) = path.strip_prefix(r"\\?\") {
        return stripped.to_string();
    }

    path.to_string()
}

fn pick_excerpt(content: &str, tokens: &[String]) -> String {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lowered = line.to_lowercase();
        if tokens.iter().any(|token| lowered.contains(token)) {
            return trim_excerpt(line, 120);
        }
    }

    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| trim_excerpt(line, 120))
        .unwrap_or_else(|| "(页面无可用内容)".to_string())
}

fn trim_excerpt(input: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for ch in input.chars().take(max_chars) {
        output.push(ch);
    }
    if input.chars().count() > max_chars {
        output.push('…');
    }
    output
}

fn build_query_prompt(question: &str, matches: &[WikiMatch]) -> String {
    let mut lines = vec![
        "你是一个严格本地运行的 Wiki 助手。只能依据下方本地检索结果回答，不能编造。".to_string(),
        "如果证据不足，请明确说明不确定，并给出基于页面内容的保守建议。".to_string(),
        format!("问题：{}", question),
        "本地检索结果：".to_string(),
    ];

    if matches.is_empty() {
        lines.push("(未命中任何本地页面)".to_string());
    } else {
        for (idx, item) in matches.iter().enumerate() {
            lines.push(format!("{}. 页面：{}", idx + 1, item.page_path));
            lines.push(format!("   相关度：{}", item.score));
            lines.push(format!("   证据：{}", item.excerpt));
        }
    }

    lines.push("回答要求：".to_string());
    lines.push("- 使用中文简洁回答。".to_string());
    lines.push("- 优先引用页面路径和检索证据。".to_string());
    lines.push("- 如果无法确认答案，请直接说明。".to_string());
    lines.join("\n")
}

fn build_query_answer(question: &str, matches: &[WikiMatch]) -> String {
    if matches.is_empty() {
        return format!(
            "未在本地 Wiki 中检索到与“{}”直接相关的页面。建议先导入相关资料后再查询。",
            question
        );
    }

    let mut lines = vec![
        format!("问题：{}", question),
        "基于本地检索，以下页面与问题最相关：".to_string(),
    ];

    for item in matches {
        lines.push(format!("- {}（相关度：{}）", item.page_path, item.score));
    }
    lines.push("以上为本地规则检索结果（未调用云模型）。".to_string());
    lines.join("\n")
}

fn build_llm_status(
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

fn normalize_cloud_provider_name(provider_name: Option<&str>) -> Option<String> {
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

fn resolve_active_provider(
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

fn display_cloud_provider_name(provider_name: &str) -> String {
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

fn normalize_cloud_base_url(provider_name: Option<&str>, base_url: Option<&str>) -> Option<String> {
    if let Some(value) = base_url {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    provider_default_base_url(provider_name)
}

fn effective_cloud_base_url(provider_name: Option<&str>, base_url: Option<&str>) -> String {
    normalize_cloud_base_url(provider_name, base_url)
        .unwrap_or_else(|| DEFAULT_OPENAI_BASE_URL.to_string())
}

fn llm_health_error_message(err: &LlmError) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rusqlite::{params, Connection};
    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        sync::{Arc, Mutex, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Clone)]
    struct MockQueryProvider {
        response: String,
        prompt_log: Arc<Mutex<Vec<String>>>,
    }

    impl MockQueryProvider {
        fn new(response: impl Into<String>, prompt_log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                response: response.into(),
                prompt_log,
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockQueryProvider {
        async fn summarize(&self, _content: &str, _max_tokens: usize) -> Result<String, LlmError> {
            Ok(self.response.clone())
        }

        async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
            self.prompt_log
                .lock()
                .expect("记录 prompt 失败")
                .push(prompt.to_string());
            Ok(self.response.clone())
        }

        async fn health_check(&self) -> Result<bool, LlmError> {
            Ok(true)
        }
    }

    #[test]
    fn default_config_path_points_to_project_root_runtime_dir() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let path = AppState::default_config_path();

        assert_eq!(path, root.join(".runtime").join("app-config.json"));
    }

    #[test]
    fn validate_pdf_source_path_rejects_missing_file() {
        let missing = std::env::temp_dir().join(format!("missing-{}.pdf", uuid_v4_short()));
        let err = validate_pdf_source_path(&missing).expect_err("缺失文件应报错");
        assert!(err.contains("PDF 文件不存在"));
    }

    #[test]
    fn validate_pdf_source_path_rejects_non_pdf_extension() {
        let dir = make_temp_dir("llm-wiki-pdf-validate-ext");
        let _guard = TempDirGuard(dir.clone());
        let txt_path = dir.join("note.txt");
        fs::write(&txt_path, "plain text").expect("写入测试文件失败");

        let err = validate_pdf_source_path(&txt_path).expect_err("非 PDF 扩展名应报错");
        assert!(err.contains("仅支持 .pdf"));
    }

    #[test]
    fn validate_pdf_source_path_rejects_directory() {
        let dir = make_temp_dir("llm-wiki-pdf-validate-dir");
        let _guard = TempDirGuard(dir.clone());
        let err = validate_pdf_source_path(&dir).expect_err("目录路径应报错");
        assert!(err.contains("不是文件"));
    }

    #[test]
    fn validate_pdf_source_path_accepts_uppercase_pdf_extension() {
        let dir = make_temp_dir("llm-wiki-pdf-validate-uppercase");
        let _guard = TempDirGuard(dir.clone());
        let upper_pdf_path = dir.join("note.PDF");
        fs::write(&upper_pdf_path, "placeholder").expect("写入测试文件失败");

        let result = validate_pdf_source_path(&upper_pdf_path);
        assert!(result.is_ok(), "大写扩展名 .PDF 应通过校验");
    }

    #[tokio::test]
    async fn ingest_pdf_impl_rejects_invalid_pdf_content_with_readable_error() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-pdf-invalid");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let invalid_pdf_path = vault_dir.join("invalid.pdf");
        fs::write(&invalid_pdf_path, "this is not a real pdf").expect("写入伪 PDF 文件失败");

        let err = state
            .ingest_pdf_impl(invalid_pdf_path.to_string_lossy().as_ref())
            .await
            .expect_err("非法 PDF 内容应返回错误");
        assert!(err.contains("读取 PDF 失败"));
        assert!(err.contains("有效 PDF"));
    }

    #[test]
    fn default_paths_point_to_project_root_targets() {
        let vault_dir = make_temp_dir("llm-wiki-default-paths");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));

        let paths = state.default_paths();
        assert_eq!(paths.vault_path, root.join("vault").to_string_lossy());
        #[cfg(windows)]
        assert_eq!(paths.ingest_source_path, r"E:\llm-wiki\test-llm.md");

        #[cfg(not(windows))]
        assert_eq!(
            paths.ingest_source_path,
            root.join("test-llm.md").to_string_lossy()
        );
    }

    #[tokio::test]
    async fn query_ask_rejects_empty_question() {
        let vault_dir = make_temp_dir("llm-wiki-query-empty");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.query_ask("   ".to_string()).await;
        assert!(result.is_err());
        assert_eq!(result.err(), Some("问题不能为空".to_string()));
    }

    #[test]
    fn lint_report_defaults_severity_stats_when_uninitialized() {
        let vault_dir = make_temp_dir("llm-wiki-lint-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let report = state.lint_report();
        assert_eq!(report.summary, "Vault 未初始化");
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.severity_stats.error, 1);
        assert_eq!(report.severity_stats.warning, 0);
        assert_eq!(report.severity_stats.info, 0);
    }

    #[test]
    fn preview_lint_patches_returns_uninitialized_vault_suggestion() {
        let vault_dir = make_temp_dir("llm-wiki-lint-preview-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let preview = state.preview_lint_patches();
        assert_eq!(preview.total, 1);
        assert_eq!(preview.suggestions.len(), 1);
        let suggestion = &preview.suggestions[0];
        assert_eq!(suggestion.issue_code, "VAULT_NOT_INITIALIZED");
        assert_eq!(suggestion.title, "初始化 Vault");
        assert!(suggestion.patch_preview.contains("init_vault"));
    }

    #[test]
    fn apply_lint_patch_supports_orphan_wiki_page_and_writes_log() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-orphan");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let orphan_path = vault_dir.join("wiki").join("orphan.md");
        let orphan_path_str = orphan_path.to_string_lossy().to_string();
        fs::write(&orphan_path, "# Orphan\n\n孤页内容。").expect("写入 orphan 页面失败");

        let result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "ORPHAN_WIKI_PAGE".to_string(),
                path: Some(orphan_path.to_string_lossy().to_string()),
            })
            .expect("应用 lint 补丁失败");

        assert!(result.applied);
        assert_eq!(result.issue_code, "ORPHAN_WIKI_PAGE");
        assert_eq!(result.path.as_deref(), Some(orphan_path_str.as_str()));
        assert!(result
            .touched_paths
            .iter()
            .any(|path| path.ends_with("index.md")));

        let index_content =
            fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        assert!(index_content.contains("[[wiki/orphan.md|orphan]]"));

        let recent_log = state.recent_logs(1);
        assert_eq!(recent_log.len(), 1);
        assert!(recent_log[0].message.contains("ORPHAN_WIKI_PAGE"));
    }

    #[test]
    fn apply_lint_patch_supports_missing_index_entry_and_appends_link() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-missing-index");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("standalone.md");
        fs::write(&page_path, "# Standalone\n\n页面内容。").expect("写入页面失败");

        let result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "MISSING_INDEX_ENTRY".to_string(),
                path: Some(page_path.to_string_lossy().to_string()),
            })
            .expect("应用 lint 补丁失败");

        assert!(result.applied);
        assert_eq!(result.issue_code, "MISSING_INDEX_ENTRY");
        assert!(result
            .touched_paths
            .iter()
            .any(|path| path.ends_with("index.md")));

        let index_content =
            fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        assert!(index_content.contains("[[wiki/standalone.md|standalone]]"));
    }

    #[test]
    fn apply_lint_patch_records_event_and_recent_query_returns_latest() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-event");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let orphan_path = vault_dir.join("wiki").join("event-note.md");
        let orphan_path_str = orphan_path.to_string_lossy().to_string();
        fs::write(&orphan_path, "# Event Note\n\n页面内容。").expect("写入页面失败");

        let result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "ORPHAN_WIKI_PAGE".to_string(),
                path: Some(orphan_path.to_string_lossy().to_string()),
            })
            .expect("应用 lint 补丁失败");

        assert!(result.applied);

        let events = state
            .recent_lint_patch_events(10)
            .expect("读取 lint 补丁事件失败");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].issue_code, "ORPHAN_WIKI_PAGE");
        assert_eq!(events[0].path.as_deref(), Some(orphan_path_str.as_str()));
        assert!(events[0].applied);
        assert!(events[0].message.contains("已将页面加入 index.md"));
        assert!(!events[0].created_at.is_empty());
    }

    #[test]
    fn apply_lint_patches_batch_summarizes_success_and_failure() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-batch");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let success_path = vault_dir.join("wiki").join("batch-note.md");
        fs::write(&success_path, "# Batch Note\n\n页面内容。").expect("写入页面失败");

        let result = state
            .apply_lint_patches_batch(vec![
                LintPatchApplyInput {
                    issue_code: "ORPHAN_WIKI_PAGE".to_string(),
                    path: Some(success_path.to_string_lossy().to_string()),
                },
                LintPatchApplyInput {
                    issue_code: "TASK_QUERY_FAILED".to_string(),
                    path: None,
                },
            ])
            .expect("批量应用 lint 补丁失败");

        assert_eq!(result.total, 2);
        assert_eq!(result.success, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.items.len(), 2);
        assert!(matches!(
            result.items[0].status,
            LintPatchBatchApplyStatus::Success
        ));
        assert!(result.items[0].applied);
        assert!(matches!(
            result.items[1].status,
            LintPatchBatchApplyStatus::Failed
        ));
        assert!(!result.items[1].applied);
        assert!(result.items[1].error.is_some());

        let recent_log = state.recent_logs(1);
        assert_eq!(recent_log.len(), 1);
        assert!(recent_log[0].message.contains("批量应用 Lint 补丁完成"));
        assert!(recent_log[0].message.contains("total=2"));
        assert!(recent_log[0].message.contains("success=1"));
        assert!(recent_log[0].message.contains("failed=1"));
        assert!(recent_log[0].message.contains("skipped=0"));
    }

    #[test]
    fn apply_lint_patch_rejects_unsupported_issue_code() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-unsupported");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let result = state.apply_lint_patch(LintPatchApplyInput {
            issue_code: "TASK_QUERY_FAILED".to_string(),
            path: None,
        });

        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("暂不支持自动应用，请手动处理".to_string())
        );
    }

    #[test]
    fn query_answer_result_defaults_missing_search_strategy() {
        let value = serde_json::json!({
            "question": "Q",
            "answer": "A",
            "citations": [],
            "matched_pages": [],
            "mode": "Hybrid",
            "checked_at": "1",
            "answer_strategy": "rule"
        });

        let result: QueryAnswerResult =
            serde_json::from_value(value).expect("反序列化 QueryAnswerResult 失败");

        assert_eq!(result.search_strategy, "empty");
        assert_eq!(result.answer_strategy, "rule");
    }

    #[tokio::test]
    async fn query_ask_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-query-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.query_ask("rust wiki".to_string()).await;
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn recent_wiki_pages_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-recent-wiki-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.recent_wiki_pages(10);
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn recent_wiki_pages_returns_db_rows() {
        let vault_dir = make_temp_dir("llm-wiki-recent-wiki");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        // 创建实际的 wiki 文件（assert_paths_semantically_equal 需要文件存在）
        let wiki_a_path = vault_dir.join("wiki").join("a.md");
        let wiki_b_path = vault_dir.join("wiki").join("b.md");
        fs::write(&wiki_a_path, "# Wiki A\nContent A").expect("创建 wiki a 失败");
        fs::write(&wiki_b_path, "# Wiki B\nContent B").expect("创建 wiki b 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-wiki", "source", "raw", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Wiki A",
                wiki_a_path.to_string_lossy().to_string(),
                "summary a",
                "1",
                "2"
            ],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Wiki B",
                wiki_b_path.to_string_lossy().to_string(),
                "summary b",
                "1",
                "3"
            ],
        )
        .expect("写入 wiki_pages 失败");

        let pages = state.recent_wiki_pages(10).expect("读取 recent wiki 失败");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].title, "Wiki B");
        assert_eq!(pages[1].title, "Wiki A");
        assert_paths_semantically_equal(
            &wiki_b_path,
            pages[0].display_path.as_deref().expect("缺少显示路径"),
        );
        assert_paths_semantically_equal(
            &wiki_a_path,
            pages[1].display_path.as_deref().expect("缺少显示路径"),
        );
    }

    #[test]
    fn search_wiki_pages_filters_rows() {
        let vault_dir = make_temp_dir("llm-wiki-search-wiki");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-search", "source", "raw", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Rust Wiki",
                vault_dir.join("wiki").join("rust.md").to_string_lossy().to_string(),
                "rust topic",
                "1",
                "2"
            ],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Tauri Wiki",
                vault_dir.join("wiki").join("tauri.md").to_string_lossy().to_string(),
                "desktop topic",
                "1",
                "3"
            ],
        )
        .expect("写入 wiki_pages 失败");

        let rust_pages = state
            .search_wiki_pages("rust".to_string(), 10)
            .expect("搜索 wiki 页面失败");
        assert_eq!(rust_pages.len(), 1);
        assert_eq!(rust_pages[0].title, "Rust Wiki");

        let all_pages = state
            .search_wiki_pages("   ".to_string(), 10)
            .expect("读取最近 wiki 页面失败");
        assert_eq!(all_pages.len(), 2);
    }

    #[test]
    fn wiki_page_detail_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.wiki_page_detail("wiki/a.md".to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn wiki_page_detail_reads_markdown_content() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("detail.md");
        fs::write(&page_path, "# Detail Title\n\n正文内容。").expect("写入页面失败");

        let detail = state
            .wiki_page_detail(page_path.to_string_lossy().to_string())
            .expect("读取页面详情失败");
        assert_eq!(detail.title, "Detail Title");
        assert!(detail.content.contains("正文内容"));
        assert!(detail.frontmatter.is_none());
        assert_paths_semantically_equal(&page_path, &detail.path);
        assert_paths_semantically_equal(&page_path, &detail.display_path);
    }

    #[test]
    fn wiki_page_detail_parses_frontmatter_fields() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-frontmatter");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("detail-frontmatter.md");
        fs::write(
            &page_path,
            r#"---
title: "Detail Title"
source: "E:\\llm-wiki\\source\\detail.md"
raw: "raw/detail.md"
imported_at: "2026-04-13T18:00:00+08:00"
entities:
  - "Rust"
  - "SQLite"
---
# Detail Title

正文内容。
"#,
        )
        .expect("写入页面失败");

        let detail = state
            .wiki_page_detail(page_path.to_string_lossy().to_string())
            .expect("读取页面详情失败");
        let frontmatter = detail.frontmatter.expect("未解析到 frontmatter");
        assert_eq!(frontmatter.title.as_deref(), Some("Detail Title"));
        assert_eq!(
            frontmatter.source.as_deref(),
            Some(r"E:\llm-wiki\source\detail.md")
        );
        assert_eq!(frontmatter.raw.as_deref(), Some("raw/detail.md"));
        assert_eq!(
            frontmatter.imported_at.as_deref(),
            Some("2026-04-13T18:00:00+08:00")
        );
        assert_eq!(
            frontmatter.entities,
            vec!["Rust".to_string(), "SQLite".to_string()]
        );
    }

    #[test]
    fn friendly_display_path_str_strips_windows_verbatim_prefix() {
        assert_eq!(
            friendly_display_path_str(r"\\?\C:\llm-wiki\vault\wiki\detail.md"),
            r"C:\llm-wiki\vault\wiki\detail.md"
        );
        assert_eq!(
            friendly_display_path_str(r"\\?\UNC\server\share\vault\wiki\detail.md"),
            r"\\server\share\vault\wiki\detail.md"
        );
    }

    #[test]
    fn wiki_page_detail_accepts_wiki_relative_path() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-relative");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("relative.md");
        fs::write(&page_path, "# Relative Title\n\n相对路径页面。").expect("写入页面失败");

        let detail = state
            .wiki_page_detail("wiki/relative.md".to_string())
            .expect("读取页面详情失败");
        assert_eq!(detail.title, "Relative Title");
        assert!(detail.content.contains("相对路径页面"));
    }

    #[test]
    fn wiki_page_detail_rejects_outside_wiki_root() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-safe");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let outside_path = vault_dir.join("outside.md");
        fs::write(&outside_path, "# Outside").expect("写入外部文件失败");

        let result = state.wiki_page_detail(outside_path.to_string_lossy().to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("只允许读取 vault/wiki 目录下的页面".to_string())
        );
    }

    #[test]
    fn wiki_page_citations_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.wiki_page_citations("wiki/a.md".to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn wiki_page_citations_returns_rows_and_target_existence_flags() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let target_path = vault_dir.join("wiki").join("detail.md");
        fs::write(&target_path, "# Detail\n\n页面正文。").expect("写入页面失败");
        let inside_cited_path = vault_dir.join("wiki").join("cited.md");
        fs::write(&inside_cited_path, "# Cited\n\n被引用页面。").expect("写入引用页失败");
        let outside_cited_path = vault_dir.join("outside-cited.md");
        fs::write(&outside_cited_path, "# Outside\n\n外部页面。").expect("写入外部引用页失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                fs::canonicalize(&target_path)
                    .expect("规范化目标页失败")
                    .to_string_lossy()
                    .to_string(),
                fs::canonicalize(&inside_cited_path)
                    .expect("规范化引用页失败")
                    .to_string_lossy()
                    .to_string(),
                7_i64,
                "命中本地页面",
                "1"
            ],
        )
        .expect("写入 citations 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                fs::canonicalize(&target_path)
                    .expect("规范化目标页失败")
                    .to_string_lossy()
                    .to_string(),
                outside_cited_path.to_string_lossy().to_string(),
                4_i64,
                "越界引用",
                "2"
            ],
        )
        .expect("写入 citations 失败");

        let citations = state
            .wiki_page_citations(target_path.to_string_lossy().to_string())
            .expect("读取页面引用失败");
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].score, 7);
        assert_eq!(citations[0].excerpt, "命中本地页面");
        assert!(citations[0].target_exists);
        assert_paths_semantically_equal(
            &inside_cited_path,
            citations[0]
                .cited_page_display_path
                .as_deref()
                .expect("缺少显示路径"),
        );
        assert_eq!(
            citations[0].cited_page_path,
            fs::canonicalize(&inside_cited_path)
                .expect("规范化引用页失败")
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(citations[1].score, 4);
        assert_eq!(citations[1].excerpt, "越界引用");
        assert!(!citations[1].target_exists);
        assert_paths_semantically_equal(
            &outside_cited_path,
            citations[1]
                .cited_page_display_path
                .as_deref()
                .expect("缺少显示路径"),
        );
        assert_eq!(
            citations[1].cited_page_path,
            outside_cited_path.to_string_lossy()
        );
    }

    #[test]
    fn wiki_page_citations_rejects_outside_wiki_root() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations-safe");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let outside_path = vault_dir.join("outside.md");
        fs::write(&outside_path, "# Outside").expect("写入外部文件失败");

        let result = state.wiki_page_citations(outside_path.to_string_lossy().to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("只允许读取 vault/wiki 目录下的页面".to_string())
        );
    }

    #[tokio::test]
    async fn query_ask_returns_matches_with_citations() {
        let vault_dir = make_temp_dir("llm-wiki-query-hit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_a = vault_dir.join("wiki").join("rust-notes.md");
        let page_b = vault_dir.join("wiki").join("tauri-notes.md");
        fs::write(
            &page_a,
            "# Rust Notes\nRust ownership and borrow checker basics.\n",
        )
        .expect("写入 rust-notes 失败");
        fs::write(
            &page_b,
            "# Tauri Notes\nTauri integrates Rust backend and WebView UI.\n",
        )
        .expect("写入 tauri-notes 失败");

        let result = state
            .query_ask("Rust backend".to_string())
            .await
            .expect("query_ask 应返回成功");

        assert_eq!(result.question, "Rust backend");
        assert!(!result.matched_pages.is_empty());
        assert!(!result.citations.is_empty());
        assert_eq!(result.mode, AppMode::Hybrid);
        assert_eq!(result.search_strategy, "scan");
        // answer_strategy 取决于 Ollama 是否可用：可用时为 "llm"，不可用时回退 "rule"
        assert!(result.answer_strategy == "llm" || result.answer_strategy == "rule");
        assert!(result
            .citations
            .iter()
            .any(|item| item.page_path.ends_with("rust-notes.md")));
        let rust_citation = result
            .citations
            .iter()
            .find(|item| item.page_path.ends_with("rust-notes.md"))
            .expect("缺少 rust-notes 引用");
        assert_paths_semantically_equal(
            &page_a,
            rust_citation.display_path.as_deref().expect("缺少显示路径"),
        );
    }

    #[test]
    fn generate_query_answer_with_provider_uses_llm_strategy_and_prompt() {
        let vault_dir = make_temp_dir("llm-wiki-query-llm");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let prompt_log = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockQueryProvider::new(
            "本地 LLM 合成回答",
            prompt_log.clone(),
        ));
        let matches = vec![WikiMatch {
            page_path: vault_dir
                .join("wiki")
                .join("prompt.md")
                .to_string_lossy()
                .to_string(),
            score: 7,
            excerpt: "核心证据片段".to_string(),
        }];

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let (answer, strategy) = runtime.block_on(async {
            state
                .generate_query_answer_with_provider("核心目标是什么", &matches, Some(provider))
                .await
        });

        assert_eq!(strategy, "llm");
        assert_eq!(answer, "本地 LLM 合成回答");

        let prompts = prompt_log.lock().expect("读取 prompt 失败");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("核心目标是什么"));
        assert!(prompts[0].contains("prompt.md"));
        assert!(prompts[0].contains("核心证据片段"));
    }

    #[test]
    fn generate_query_answer_with_provider_falls_back_to_rule_on_empty_response() {
        let vault_dir = make_temp_dir("llm-wiki-query-fallback");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let prompt_log = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn LlmProvider> =
            Arc::new(MockQueryProvider::new("   ", prompt_log.clone()));
        let matches = vec![WikiMatch {
            page_path: vault_dir
                .join("wiki")
                .join("fallback.md")
                .to_string_lossy()
                .to_string(),
            score: 3,
            excerpt: "回退证据".to_string(),
        }];
        let expected = build_query_answer("需要回退吗", &matches);

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let (answer, strategy) = runtime.block_on(async {
            state
                .generate_query_answer_with_provider("需要回退吗", &matches, Some(provider))
                .await
        });

        assert_eq!(strategy, "rule");
        assert_eq!(answer, expected);

        let prompts = prompt_log.lock().expect("读取 prompt 失败");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("fallback.md"));
    }

    #[test]
    fn llm_health_error_message_maps_known_errors() {
        assert_eq!(
            llm_health_error_message(&LlmError::Timeout),
            "本地 Ollama 健康检查超时"
        );
        assert_eq!(
            llm_health_error_message(&LlmError::ModelNotFound("mistral".to_string())),
            "本地 Ollama 未找到模型：mistral"
        );
    }

    #[test]
    fn build_llm_status_formats_expected_fields() {
        let status = build_llm_status(
            "ollama",
            "http://localhost:11434",
            "llama3:8b",
            AppMode::StrictLocal,
            true,
            "本地 Ollama 可用".to_string(),
        );

        assert_eq!(status.provider, "ollama");
        assert_eq!(status.base_url, "http://localhost:11434");
        assert_eq!(status.model, "llama3:8b");
        assert!(status.healthy);
        assert_eq!(status.message, "本地 Ollama 可用");
        assert_eq!(status.mode, AppMode::StrictLocal);
    }

    #[test]
    fn load_config_compatibly_reads_legacy_openai_fields() {
        let dir = make_temp_dir("llm-wiki-config-legacy");
        let _guard = TempDirGuard(dir.clone());
        let config_path = dir.join("app-config.json");
        fs::write(
            &config_path,
            r#"{
  "mode": "Hybrid",
  "vault_path": "C:/wiki",
  "query_top_k": 5,
  "openai_api_key": "sk-legacy",
  "openai_provider_name": "DeepSeek",
  "openai_model": "deepseek-chat"
}"#,
        )
        .expect("写入旧配置失败");

        let (config, snapshot) = AppState::load_config(&config_path);

        assert_eq!(
            snapshot.as_deref().map(str::trim),
            Some(
                r#"{
  "mode": "Hybrid",
  "vault_path": "C:/wiki",
  "query_top_k": 5,
  "openai_api_key": "sk-legacy",
  "openai_provider_name": "DeepSeek",
  "openai_model": "deepseek-chat"
}"#
            )
        );
        assert_eq!(config.mode, AppMode::Hybrid);
        assert_eq!(config.vault_path.as_deref(), Some("C:/wiki"));
        assert_eq!(config.query_top_k, Some(5));
        assert_eq!(config.cloud_api_key.as_deref(), Some("sk-legacy"));
        assert_eq!(config.cloud_model.as_deref(), Some("deepseek-chat"));
        assert_eq!(config.cloud_provider_name.as_deref(), Some("DeepSeek"));
        assert!(config.cloud_base_url.is_none());
        assert!(config.active_provider.is_none());
    }

    #[test]
    fn provider_aliases_are_canonicalized_and_default_urls_are_derived() {
        assert_eq!(
            normalize_cloud_provider_name(Some("  DeepSeek Chat  ")).as_deref(),
            Some("deepseek")
        );
        assert_eq!(
            normalize_cloud_provider_name(Some("GLM-4")).as_deref(),
            Some("glm")
        );
        assert_eq!(
            normalize_cloud_provider_name(Some("zhipu ai")).as_deref(),
            Some("glm")
        );
        assert_eq!(
            normalize_cloud_provider_name(Some("MiniMax abab6.5")).as_deref(),
            Some("minimax")
        );
        assert_eq!(display_cloud_provider_name("deepseek"), "DeepSeek");
        assert_eq!(display_cloud_provider_name("glm"), "GLM");
        assert_eq!(display_cloud_provider_name("minimax"), "MiniMax");

        let cases = [
            ("deepseek", Some("https://api.deepseek.com/v1")),
            ("glm", Some("https://open.bigmodel.cn/api/paas/v4")),
            ("zhipu", Some("https://open.bigmodel.cn/api/paas/v4")),
            ("minimax", Some("https://api.minimax.chat/v1")),
        ];

        for (provider_name, expected_base_url) in cases {
            assert_eq!(
                normalize_cloud_base_url(Some(provider_name), None).as_deref(),
                expected_base_url
            );
            assert_eq!(
                effective_cloud_base_url(Some(provider_name), None),
                expected_base_url.unwrap()
            );
        }
    }

    #[test]
    fn llm_status_input_prefers_ollama_when_active_provider_is_ollama_in_hybrid() {
        let vault_dir = make_temp_dir("llm-wiki-provider-route-ollama");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.mode = AppMode::Hybrid;
            guard.cloud_api_key = Some("sk-test".to_string());
            guard.cloud_model = Some("gpt-4o-mini".to_string());
            guard.active_provider = Some("ollama".to_string());
        }

        let (_mode, _cloud_provider_name, cloud_config, ollama_provider) = state.llm_status_input();
        assert!(cloud_config.is_none());
        assert!(ollama_provider.is_some());
    }

    #[test]
    fn set_llm_config_falls_back_to_ollama_when_cloud_selected_without_key() {
        let vault_dir = make_temp_dir("llm-wiki-provider-fallback");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let saved = state
            .set_llm_config(LlmProviderConfig {
                cloud_api_key: "".to_string(),
                cloud_base_url: "".to_string(),
                cloud_model: "gpt-4o-mini".to_string(),
                cloud_provider_name: "DeepSeek".to_string(),
                active_provider: "cloud".to_string(),
            })
            .expect("保存 LLM 配置失败");

        assert_eq!(saved.active_provider, "ollama");
    }

    #[test]
    fn search_wiki_matches_with_fts_prefers_fts_strategy() {
        let vault_dir = make_temp_dir("llm-wiki-query-fts");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let wiki_dir = vault_dir.join("wiki");
        let wiki_path = wiki_dir.join("fts-hit.md");
        let content = "# FTS Hit\nRust backend with tauri integration.\n";
        fs::write(&wiki_path, content).expect("写入 fts-hit 失败");
        db::upsert_fts_page(&db_path, &wiki_path, "fts-hit", content).expect("写入 fts 索引失败");

        let tokens = tokenize_query("Rust backend");
        let (matches, strategy, fts_error) =
            search_wiki_matches_with_fts(&db_path, &wiki_dir, &tokens, "Rust backend", 3)
                .expect("执行检索失败");

        assert!(fts_error.is_none());
        assert_eq!(strategy, "fts");
        assert!(!matches.is_empty());
        assert!(matches
            .iter()
            .any(|item| item.page_path.ends_with("fts-hit.md")));
    }

    #[test]
    fn tokenize_query_supports_cjk_segments_and_bigrams() {
        let tokens = tokenize_query("这个项目的核心目标是什么？query v1");

        assert!(tokens.iter().any(|item| item == "query"));
        assert!(tokens.iter().any(|item| item == "这个项目的核心目标是什么"));
        assert!(tokens.iter().any(|item| item == "核心"));
        assert!(tokens.iter().any(|item| item == "目标"));
    }

    #[test]
    fn tokenize_query_filters_stopwords() {
        let tokens = tokenize_query("the 这个 项目 的 核心 目标 是 什么");

        assert!(!tokens.iter().any(|item| item == "the"));
        assert!(!tokens.iter().any(|item| item == "的"));
        assert!(!tokens.iter().any(|item| item == "是"));
        assert!(tokens.iter().any(|item| item == "项目"));
        assert!(tokens.iter().any(|item| item == "核心"));
    }

    #[tokio::test]
    async fn query_ask_with_options_applies_top_k_clamp() {
        let vault_dir = make_temp_dir("llm-wiki-query-topk");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        for idx in 0..4 {
            let page_path = vault_dir.join("wiki").join(format!("page-{}.md", idx));
            fs::write(
                &page_path,
                format!("# 页面{}\n这个项目的核心目标是什么。\n", idx),
            )
            .expect("写入测试页面失败");
            let db_path = vault_dir.join(".app").join("meta.db");
            db::upsert_fts_page(
                &db_path,
                &page_path,
                &format!("page-{}", idx),
                "这个项目的核心目标是什么。",
            )
            .expect("写入 fts 索引失败");
        }

        let result = state
            .query_ask_with_options(
                "这个项目的核心目标是什么".to_string(),
                QueryAskOptions { top_k: Some(1) },
            )
            .await
            .expect("query_ask_with_options 应返回成功");
        assert_eq!(result.matched_pages.len(), 1);
        assert_eq!(result.search_strategy, "fts");
        assert!(result
            .citations
            .iter()
            .all(|item| item.display_path.is_some()));

        let result = state
            .query_ask_with_options(
                "这个项目的核心目标是什么".to_string(),
                QueryAskOptions { top_k: Some(99) },
            )
            .await
            .expect("query_ask_with_options 应返回成功");
        assert!(result.matched_pages.len() <= QUERY_TOP_K_MAX);
        assert_eq!(result.search_strategy, "fts");
        assert!(result
            .citations
            .iter()
            .all(|item| item.display_path.is_some()));
    }

    #[test]
    fn set_query_top_k_persists_to_runtime_config() {
        let vault_dir = make_temp_dir("llm-wiki-query-settings");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let settings = state.set_query_top_k(6).expect("设置 top_k 失败");
        assert_eq!(settings.top_k, 6);

        let config_raw = fs::read_to_string(vault_dir.join(".runtime").join("app-config.json"))
            .expect("读取运行时配置失败");
        let config: AppConfig = serde_json::from_str(&config_raw).expect("解析运行时配置失败");
        assert_eq!(config.query_top_k, Some(6));
    }

    #[tokio::test]
    async fn query_ask_with_options_uses_persisted_default_top_k() {
        let vault_dir = make_temp_dir("llm-wiki-query-default-topk");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");
        state.set_query_top_k(2).expect("设置 top_k 失败");

        for idx in 0..4 {
            let page_path = vault_dir
                .join("wiki")
                .join(format!("topk-default-{}.md", idx));
            fs::write(
                &page_path,
                format!("# 页面{}\nquery default topk 测试。\n", idx),
            )
            .expect("写入测试页面失败");
            let db_path = vault_dir.join(".app").join("meta.db");
            db::upsert_fts_page(
                &db_path,
                &page_path,
                &format!("topk-default-{}", idx),
                "query default topk 测试。",
            )
            .expect("写入 fts 索引失败");
        }

        let result = state
            .query_ask_with_options("query default topk".to_string(), QueryAskOptions::default())
            .await
            .expect("query_ask_with_options 应返回成功");
        assert_eq!(result.matched_pages.len(), 2);
        assert_eq!(result.search_strategy, "fts");
    }

    #[test]
    fn save_query_answer_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-save-query-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let input = SaveQueryAnswerInput {
            question: "q".to_string(),
            answer: "a".to_string(),
            citations: Vec::new(),
            title: None,
        };

        let result = state.save_query_answer(input);
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn save_query_answer_writes_wiki_file_and_updates_db() {
        let vault_dir = make_temp_dir("llm-wiki-save-query");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let input = SaveQueryAnswerInput {
            question: "这个项目的核心目标是什么？".to_string(),
            answer: "目标是构建 Windows 优先的个人 Wiki 桌面应用。".to_string(),
            citations: vec![QueryCitation {
                page_path: vault_dir
                    .join("wiki")
                    .join("ingest-1.md")
                    .to_string_lossy()
                    .to_string(),
                display_path: None,
                score: 3,
                excerpt: "本项目用于实现一个 Windows 优先的个人 Wiki 桌面应用".to_string(),
            }],
            title: Some("问答-核心目标".to_string()),
        };
        let result = state.save_query_answer(input).expect("保存 Query 结果失败");

        assert!(PathBuf::from(&result.wiki_path).exists());
        let index_content =
            fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        assert!(index_content.contains("问答-核心目标"));

        let db_path = vault_dir.join(".app").join("meta.db");
        let page_paths = db::list_wiki_page_paths(&db_path).expect("读取 wiki_pages 失败");
        assert!(page_paths.iter().any(|path| path == &result.wiki_path));
    }

    #[test]
    fn search_wiki_matches_from_paths_applies_phrase_title_boost_and_deduplicates() {
        let vault_dir = make_temp_dir("llm-wiki-query-rank");
        let _guard = TempDirGuard(vault_dir.clone());
        let wiki_dir = vault_dir.join("wiki");
        fs::create_dir_all(&wiki_dir).expect("创建 wiki 目录失败");

        let high_path = wiki_dir.join("high.md");
        let low_path = wiki_dir.join("low.md");
        fs::write(
            &high_path,
            "# 核心目标\n这个项目的核心目标是什么 这是完整短语。\n",
        )
        .expect("写入 high.md 失败");
        fs::write(&low_path, "# 其他说明\n这个 项目 核心 目标 分散出现。\n")
            .expect("写入 low.md 失败");

        let page_paths = vec![
            high_path.to_string_lossy().to_string(),
            high_path.to_string_lossy().to_string(),
            low_path.to_string_lossy().to_string(),
        ];
        let question = "这个项目的核心目标是什么";
        let tokens = tokenize_query(question);
        let matches = search_wiki_matches_from_paths(&page_paths, &tokens, question, 5)
            .expect("执行页面匹配失败");

        assert_eq!(matches.len(), 2);
        assert!(matches[0].page_path.ends_with("high.md"));
        assert!(matches[0].score >= matches[1].score);
    }

    #[test]
    fn lint_report_detects_missing_index_entries_orphans_and_db_mismatches() {
        let vault_dir = make_temp_dir("llm-wiki-lint-pages");
        let _guard = TempDirGuard(vault_dir.clone());

        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let present_path = vault_dir.join("wiki").join("present.md");
        let orphan_path = vault_dir.join("wiki").join("orphan.md");
        fs::write(&present_path, "# present\n").expect("写入 present 失败");
        fs::write(&orphan_path, "# orphan\n").expect("写入 orphan 失败");

        fs::write(
            vault_dir.join("index.md"),
            "# Index\n\n## Imported Pages\n- [[wiki/present.md|present]]\n- [[wiki/missing.md|missing]]\n",
        )
        .expect("写入 index.md 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "hash-1",
                vault_dir.join("source.md").to_string_lossy().to_string(),
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                "1"
            ],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "present",
                present_path.to_string_lossy().to_string(),
                "present summary",
                "1",
                "1"
            ],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                present_path.to_string_lossy().to_string(),
                vault_dir.join("wiki").join("missing-cited.md").to_string_lossy().to_string(),
                3_i64,
                "missing citation",
                "1"
            ],
        )
        .expect("写入 citations 失败");

        let report = state.lint_report();
        let codes: BTreeSet<_> = report
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect();

        assert!(codes.contains("MISSING_INDEX_ENTRY"));
        assert!(codes.contains("ORPHAN_WIKI_PAGE"));
        assert!(codes.contains("DB_MISSING_PAGE_RECORD"));
        assert!(codes.contains("BROKEN_CITATION"));
        assert!(!codes.contains("VAULT_NOT_INITIALIZED"));
        assert_eq!(report.severity_stats.error, 1);
        assert_eq!(report.severity_stats.warning, 3);
        assert_eq!(report.severity_stats.info, 0);
    }

    #[test]
    fn preview_lint_patches_total_matches_suggestions_for_multiple_issues() {
        let vault_dir = make_temp_dir("llm-wiki-lint-preview-multi");
        let _guard = TempDirGuard(vault_dir.clone());

        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let present_path = vault_dir.join("wiki").join("present.md");
        let orphan_path = vault_dir.join("wiki").join("orphan.md");
        fs::write(&present_path, "# present\n").expect("写入 present 失败");
        fs::write(&orphan_path, "# orphan\n").expect("写入 orphan 失败");

        fs::write(
            vault_dir.join("index.md"),
            "# Index\n\n## Imported Pages\n- [[wiki/present.md|present]]\n- [[wiki/missing.md|missing]]\n",
        )
        .expect("写入 index.md 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "hash-preview",
                vault_dir.join("source.md").to_string_lossy().to_string(),
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                "1"
            ],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "present",
                present_path.to_string_lossy().to_string(),
                "present summary",
                "1",
                "1"
            ],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                present_path.to_string_lossy().to_string(),
                vault_dir.join("wiki").join("missing-cited.md").to_string_lossy().to_string(),
                3_i64,
                "missing citation",
                "1"
            ],
        )
        .expect("写入 citations 失败");

        let report = state.lint_report();
        let preview = state.preview_lint_patches();

        assert_eq!(preview.total, preview.suggestions.len());
        assert_eq!(preview.total, report.issues.len());
        assert!(preview
            .suggestions
            .iter()
            .any(|item| item.issue_code == "BROKEN_CITATION"));
        assert!(preview
            .suggestions
            .iter()
            .any(|item| item.issue_code == "MISSING_INDEX_ENTRY"));
    }

    #[test]
    fn lint_report_flags_stale_pending_tasks() {
        let vault_dir = make_temp_dir("llm-wiki-lint-tasks");
        let _guard = TempDirGuard(vault_dir.clone());

        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "hash-2",
                vault_dir.join("source.md").to_string_lossy().to_string(),
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                "1"
            ],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tasks (source_id, kind, status, raw_path, wiki_path, error, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7)",
            params![
                source_id,
                "ingest_markdown",
                "queued",
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                vault_dir.join("wiki").join("stale.md").to_string_lossy().to_string(),
                "1",
                "1"
            ],
        )
        .expect("写入 tasks 失败");

        let report = state.lint_report();

        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "STALE_PENDING_TASK"));
        assert_eq!(report.severity_stats.error, 0);
        assert_eq!(report.severity_stats.warning, 1);
        assert_eq!(report.severity_stats.info, 0);
    }

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    fn make_test_state(vault_dir: &Path) -> AppState {
        AppState {
            inner: Mutex::new(AppStateData {
                mode: AppMode::Hybrid,
                vault_path: None,
                query_top_k: QUERY_TOP_K_DEFAULT,
                logs: Vec::new(),
                next_log_id: 1,
                config_snapshot: None,
                // 测试环境不配置云端 Provider，以确保回退到 Ollama
                cloud_api_key: None,
                cloud_base_url: None,
                cloud_model: None,
                cloud_provider_name: None,
                active_provider: None,
            }),
            config_path: vault_dir.join(".runtime").join("app-config.json"),
            llm_provider: OnceLock::new(),
            // 测试环境不注入 AppHandle，emit_progress 静默跳过
            app_handle: OnceLock::new(),
        }
    }

    fn assert_paths_semantically_equal(expected: &Path, actual: &str) {
        let expected_canonical = fs::canonicalize(expected).expect("规范化预期路径失败");
        let actual_canonical = fs::canonicalize(Path::new(actual)).expect("规范化实际路径失败");

        assert_eq!(
            actual_canonical, expected_canonical,
            "路径语义不一致：expected={:?}, actual={:?}",
            expected, actual
        );
    }

    // ── 语义 Lint 解析测试 ──────────────────────────────────────────

    #[test]
    fn parse_semantic_lint_response_parses_valid_lines() {
        let input = "SEMANTIC_CONTRADICTION|warning|page A 与 page B 矛盾|wiki/a.md|对齐两页内容\n\
                     SEMANTIC_STALE|info|结论可能已过时||更新至最新信息\n\
                     NO_ISSUES";
        let issues = parse_semantic_lint_response(input);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].code, "SEMANTIC_CONTRADICTION");
        assert_eq!(issues[0].severity, "warning");
        assert_eq!(issues[0].path, Some("wiki/a.md".to_string()));
        assert_eq!(issues[1].code, "SEMANTIC_STALE");
        assert_eq!(issues[1].severity, "info");
        assert_eq!(issues[1].path, None);
    }

    #[test]
    fn parse_semantic_lint_response_rejects_invalid_codes() {
        // 非法 code 行应被跳过
        let input = "INVALID_CODE|warning|some message|wiki/a.md|fix it\n\
                     SEMANTIC_COVERAGE_GAP|info|缺少 Rust 语言页面||新建 wiki/rust.md";
        let issues = parse_semantic_lint_response(input);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "SEMANTIC_COVERAGE_GAP");
    }

    #[test]
    fn parse_semantic_lint_response_handles_no_issues() {
        let issues = parse_semantic_lint_response("NO_ISSUES");
        assert!(issues.is_empty());

        let issues2 = parse_semantic_lint_response("");
        assert!(issues2.is_empty());
    }

    #[test]
    fn parse_semantic_lint_response_caps_at_ten() {
        // 超过 10 条时只返回前 10 条
        let line = "SEMANTIC_STALE|info|old conclusion||update it\n";
        let input = line.repeat(15);
        let issues = parse_semantic_lint_response(&input);
        assert_eq!(issues.len(), 10);
    }

    #[test]
    fn merge_lint_with_semantic_updates_stats_and_summary() {
        use crate::models::LintSeverityStats;
        let rules = LintReport {
            mode: AppMode::Hybrid,
            checked_at: "0".to_string(),
            summary: "初始".to_string(),
            issues: vec![],
            severity_stats: LintSeverityStats {
                error: 0,
                warning: 1,
                info: 0,
            },
        };
        let semantic = vec![LintIssue {
            code: "SEMANTIC_STALE".to_string(),
            severity: "warning".to_string(),
            message: "过时".to_string(),
            path: None,
            suggestion: "更新".to_string(),
        }];
        let merged = merge_lint_with_semantic(rules, semantic);
        assert_eq!(merged.issues.len(), 1);
        assert_eq!(merged.severity_stats.warning, 2); // 原1 + 新增1
        assert!(merged.summary.contains("1 个问题"));
    }
}
