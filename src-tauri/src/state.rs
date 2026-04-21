use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use flate2::read::ZlibDecoder;

use tauri::{AppHandle, Emitter, Manager};

use crate::{
    db,
    search::reciprocal_rank_fusion,
    llm::{
        LlmError, LlmProvider, OllamaConfig, OllamaProvider, OpenAiConfig, OpenAiProvider,
        DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL,
    },
    models::{
        AppConfig, AppMode, AppOverview, DefaultPaths, IngestResult, KnowledgeGraphData,
        KnowledgeGraphDirection, KnowledgeGraphLink, KnowledgeGraphNode, KnowledgeSubgraphData,
        KnowledgeSubgraphMeta, LintIssue, LintPatchApplyInput, LintPatchApplyResult,
        LintPatchBatchApplyItemResult, LintPatchBatchApplyResult, LintPatchBatchApplyStatus,
        LintPatchEventItem, LintPatchPreview, LintPatchSuggestion, LintReport, LintSeverityStats,
        LlmProviderConfig, LlmStatus, LogEntry, LogLevel, ModeChangeResult, OutboxAckResult,
        OutboxEventItem, ProgressPayload, QueryAnswerResult, QueryAskOptions, QueryCitation,
        QuerySearchDebug, QuerySearchRouteDebug, QuerySettings, SaveQueryAnswerInput,
        SaveQueryAnswerResult, VaultInitResult, WikiPageCitationItem, WikiPageDetail,
        WikiPageFrontmatter, WikiPageItem,
    },
    vault,
};

const STALE_PENDING_TASK_THRESHOLD_MS: u128 = 24 * 60 * 60 * 1000;
const QUERY_TOP_K_MIN: usize = 1;
const QUERY_TOP_K_MAX: usize = 8;
const QUERY_TOP_K_DEFAULT: usize = 3;
const QUERY_EMBED_ROUTE_MAX_CANDIDATES: usize = 5000;
const QUERY_RRF_K: f64 = 60.0;
const QUERY_ROUTE_DEBUG_TOP_CANDIDATES: usize = 5;

/// 默认摘要最大 token 数量
const LLM_SUMMARY_MAX_TOKENS: usize = 200;

/// 进程内状态。
pub struct AppState {
    inner: Mutex<AppStateData>,
    config_path: PathBuf,
    /// LLM Provider（延迟初始化，存储 trait 对象以支持多后端与 Mock）
    llm_provider: OnceLock<Arc<dyn LlmProvider>>,
    /// Tauri AppHandle（应用启动后由 setup hook 注入，用于 emit 进度事件）
    app_handle: OnceLock<AppHandle>,
    /// 会话历史（in-memory，session_id -> 轮次列表）
    ask_sessions: Mutex<std::collections::HashMap<String, Vec<crate::models::AskTurn>>>,
    /// 会话取消标志（session_id -> AtomicBool）
    ask_cancel_flags: Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// 搜索配置（Deep Research 用）
    search_config: Mutex<crate::models::SearchConfig>,
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
    /// 默认 OCR provider（tesseract / paddle）
    default_ocr_provider: Option<String>,
    /// 本地 Ollama 模型名（手动指定，覆盖默认值）
    ollama_model: Option<String>,
    /// 本地 Ollama Base URL（手动指定，覆盖默认值）
    ollama_base_url: Option<String>,
    /// Embedding 专用 Ollama 模型（独立于 LLM 模型）
    embed_ollama_model: Option<String>,
    /// Embedding 专用 Ollama Base URL
    embed_ollama_base_url: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    /// 仅用于测试：以指定配置路径创建 AppState（不存在时等同于空配置）。
    #[cfg(test)]
    pub fn new_with_path(config_path: PathBuf) -> Self {
        let (config, config_snapshot) = Self::load_config(&config_path);
        let mode = config.mode;
        let vault_path = config.vault_path.clone().map(PathBuf::from);
        let query_top_k = normalize_top_k(config.query_top_k);
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
            default_ocr_provider: config.default_ocr_provider.clone(),
            ollama_model: config.ollama_model.clone(),
            ollama_base_url: config.ollama_base_url.clone(),
            embed_ollama_model: config.embed_ollama_model.clone(),
            embed_ollama_base_url: config.embed_ollama_base_url.clone(),
        });
        let search_config = Self::load_search_config_from_path(&config_path);
        Self {
            inner: Mutex::new(AppStateData {
                mode,
                vault_path,
                query_top_k,
                next_log_id: 1,
                logs: vec![],
                config_snapshot,
                cloud_api_key: config.cloud_api_key,
                cloud_base_url: config.cloud_base_url,
                cloud_model: config.cloud_model,
                cloud_provider_name: config.cloud_provider_name,
                active_provider: config.active_provider,
                default_ocr_provider: config.default_ocr_provider,
                ollama_model: config.ollama_model,
                ollama_base_url: config.ollama_base_url,
                embed_ollama_model: config.embed_ollama_model,
                embed_ollama_base_url: config.embed_ollama_base_url,
            }),
            config_path,
            llm_provider: OnceLock::new(),
            app_handle: OnceLock::new(),
            ask_sessions: Mutex::new(std::collections::HashMap::new()),
            ask_cancel_flags: Mutex::new(std::collections::HashMap::new()),
            search_config: Mutex::new(search_config),
        }
    }

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
            default_ocr_provider: config.default_ocr_provider.clone(),
            ollama_model: config.ollama_model.clone(),
            ollama_base_url: config.ollama_base_url.clone(),
            embed_ollama_model: config.embed_ollama_model.clone(),
            embed_ollama_base_url: config.embed_ollama_base_url.clone(),
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

        let search_config = Self::load_search_config_from_path(&config_path);
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
                default_ocr_provider: config.default_ocr_provider,
                ollama_model: config.ollama_model,
                ollama_base_url: config.ollama_base_url,
                embed_ollama_model: config.embed_ollama_model,
                embed_ollama_base_url: config.embed_ollama_base_url,
            }),
            config_path,
            llm_provider: OnceLock::new(),
            app_handle: OnceLock::new(),
            ask_sessions: Mutex::new(std::collections::HashMap::new()),
            ask_cancel_flags: Mutex::new(std::collections::HashMap::new()),
            search_config: Mutex::new(search_config),
        }
    }

    /// 注入 Tauri AppHandle（在应用 setup hook 中调用一次）。
    pub fn set_app_handle(&self, handle: AppHandle) {
        let _ = self.app_handle.set(handle);
    }

    /// 从 config_path 相邻的 search-config.json 文件加载搜索配置（不存在则返回默认值）。
    fn load_search_config_from_path(config_path: &Path) -> crate::models::SearchConfig {
        let search_config_path = config_path
            .parent()
            .map(|p| p.join("search-config.json"))
            .unwrap_or_else(|| PathBuf::from("search-config.json"));
        if let Ok(content) = fs::read_to_string(&search_config_path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            crate::models::SearchConfig::default()
        }
    }

    /// 获取当前搜索配置。
    pub fn get_search_config(&self) -> crate::models::SearchConfig {
        self.search_config.lock().expect("搜索配置锁已被污染").clone()
    }

    /// 更新搜索配置并持久化到 search-config.json。
    pub fn set_search_config(&self, cfg: crate::models::SearchConfig) -> Result<(), String> {
        let search_config_path = self
            .config_path
            .parent()
            .map(|p| p.join("search-config.json"))
            .unwrap_or_else(|| PathBuf::from("search-config.json"));
        let json = serde_json::to_string_pretty(&cfg)
            .map_err(|e| format!("序列化搜索配置失败: {}", e))?;
        fs::write(&search_config_path, json)
            .map_err(|e| format!("写入搜索配置文件失败: {}", e))?;
        *self.search_config.lock().expect("搜索配置锁已被污染") = cfg;
        Ok(())
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
    fn get_ollama_provider(&self) -> Arc<dyn LlmProvider> {
        self.llm_provider
            .get_or_init(|| {
                // 优先使用本地配置文件中的 Ollama 参数，未配置时回退默认值。
                let mut config = OllamaConfig::default();
                let (custom_base_url, custom_model) = {
                    let guard = self.inner.lock().expect("状态锁已被污染");
                    (guard.ollama_base_url.clone(), guard.ollama_model.clone())
                };
                if let Some(base_url) = custom_base_url {
                    let normalized = base_url.trim();
                    if !normalized.is_empty() {
                        config.base_url = normalized.to_string();
                    }
                }
                if let Some(model) = custom_model {
                    let normalized = model.trim();
                    if !normalized.is_empty() {
                        config.model = normalized.to_string();
                    }
                }
                let provider = OllamaProvider::new(config);
                Arc::new(provider)
            })
            .clone()
    }

    /// 获取 Embedding 专用 Provider：始终使用本地 Ollama embedding 模型，不走云端。
    /// 默认模型：nomic-embed-text:latest；可在 Settings 中配置 embed_ollama_model。
    fn get_embed_provider(&self) -> Arc<dyn LlmProvider> {
        let (embed_model, embed_base_url, fallback_base_url) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (
                guard.embed_ollama_model.clone(),
                guard.embed_ollama_base_url.clone(),
                guard.ollama_base_url.clone(),
            )
        };
        let mut config = OllamaConfig::default();
        // embed 模型优先 embed_ollama_model，未配置时用 nomic-embed-text:latest
        let model = embed_model
            .as_deref()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or("nomic-embed-text:latest");
        config.model = model.to_string();
        // base_url 优先 embed_ollama_base_url，其次 ollama_base_url，最后默认值
        let base_url = embed_base_url
            .as_deref()
            .filter(|u| !u.trim().is_empty())
            .or_else(|| fallback_base_url.as_deref().filter(|u| !u.trim().is_empty()))
            .unwrap_or(&config.base_url);
        config.base_url = base_url.to_string();
        Arc::new(OllamaProvider::new(config))
    }

    /// 获取 LLM Provider，按模式路由：
    /// - StrictLocal → 仅 Ollama
    /// - Hybrid → 优先使用 active_provider（仅 cloud/ollama），并在无 key 时安全回退到 ollama
    fn get_llm_provider(&self) -> Option<Arc<dyn LlmProvider>> {
        // 如果 OnceLock 已经设置（例如测试注入了 Mock），直接返回
        if let Some(p) = self.llm_provider.get() {
            return Some(p.clone());
        }

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
                Some(self.get_ollama_provider())
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
                    let config = OpenAiConfig::with_base_url_and_model(key, base_url, model, None);
                    // 注意：Hybrid 模式下的 OpenAiProvider 目前不进入 OnceLock，以支持 key 的实时更新
                    // 或者，我们可以改进 OnceLock 逻辑使其支持重置
                    Some(Arc::new(OpenAiProvider::new(config)) as Arc<dyn LlmProvider>)
                } else {
                    Some(self.get_ollama_provider())
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
        // 截断到 8000 字符，避免长 PDF 超出云端 LLM token 限制（约 2000 token）
        const MAX_INPUT_CHARS: usize = 8000;
        let truncated_content: String = content.chars().take(MAX_INPUT_CHARS).collect();
        let content = truncated_content.as_str();

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
            AppMode::StrictLocal => (mode, None, None, Some(self.get_ollama_provider())),
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
                    (mode, None, None, Some(self.get_ollama_provider()))
                }
            }
        }
    }

    /// 使用输入快照查询当前活跃 Provider 的健康状态。
    async fn llm_status_from_input(
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
            // 使用本地 Ollama
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

    /// 获取知识图谱数据（所有 wiki 页面节点 + citations 边）。
    pub fn get_knowledge_graph_impl(&self) -> Result<KnowledgeGraphData, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };
        let vault_path = vault_path.ok_or_else(|| "请先初始化 Vault".to_string())?;
        let db_path = vault_path.join(".app").join("meta.db");

        // 构建节点：从 wiki_pages 表获取所有页面
        let page_records = db::list_all_wiki_pages(&db_path)?;

        // 从 wiki 目录读取 frontmatter 以获取 entities（用于分组）
        // 只读取 frontmatter，不读正文，保持高效
        let _wiki_dir = vault_path.join("wiki");
        let nodes: Vec<KnowledgeGraphNode> = page_records
            .iter()
            .map(|record| {
                // 尝试从文件读取第一个 entity 作为分组标签
                let group = std::fs::read_to_string(&record.path)
                    .ok()
                    .and_then(|content| parse_wiki_frontmatter(&content))
                    .and_then(|fm| fm.entities.into_iter().next())
                    .unwrap_or_default();

                KnowledgeGraphNode {
                    id: record.path.clone(),
                    label: record.title.clone(),
                    group,
                }
            })
            .collect();

        // 构建边：从 citations 表获取所有引用关系（去重）
        let citation_records = db::list_citations(&db_path)?;
        let mut seen_links = std::collections::HashSet::new();
        let links: Vec<KnowledgeGraphLink> = citation_records
            .into_iter()
            .filter_map(|c| {
                let key = (c.page_path.clone(), c.cited_page_path.clone());
                if seen_links.insert(key) {
                    Some(KnowledgeGraphLink {
                        source: c.page_path,
                        target: c.cited_page_path,
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(KnowledgeGraphData { nodes, links })
    }

    /// 获取知识子图（基于中心节点 + hop + 方向过滤）。
    pub fn get_knowledge_subgraph_impl(
        &self,
        center_page_path: String,
        hop: u8,
        direction: KnowledgeGraphDirection,
        limit_nodes: usize,
        limit_links: usize,
    ) -> Result<KnowledgeSubgraphData, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };
        let vault_path = vault_path.ok_or_else(|| "请先初始化 Vault".to_string())?;
        let db_path = vault_path.join(".app").join("meta.db");

        let page_records = db::list_all_wiki_pages(&db_path)?;
        let mut node_map = HashMap::<String, KnowledgeGraphNode>::new();
        for record in page_records {
            let group = std::fs::read_to_string(&record.path)
                .ok()
                .and_then(|content| parse_wiki_frontmatter(&content))
                .and_then(|fm| fm.entities.into_iter().next())
                .unwrap_or_default();
            node_map.insert(
                record.path.clone(),
                KnowledgeGraphNode {
                    id: record.path,
                    label: record.title,
                    group,
                },
            );
        }

        let trimmed_center = center_page_path.trim();
        if trimmed_center.is_empty() {
            return Err("中心页面路径不能为空".to_string());
        }
        let center_id = if node_map.contains_key(trimmed_center) {
            trimmed_center.to_string()
        } else {
            let resolved = resolve_existing_wiki_page_path(&vault_path, trimmed_center)?;
            let resolved_str = resolved.to_string_lossy().to_string();
            if !node_map.contains_key(&resolved_str) {
                return Err("中心页面不在图谱中".to_string());
            }
            resolved_str
        };

        let mut seen_links = HashSet::new();
        let mut all_links = Vec::<KnowledgeGraphLink>::new();
        for citation in db::list_citations(&db_path)? {
            if !node_map.contains_key(&citation.page_path) || !node_map.contains_key(&citation.cited_page_path)
            {
                continue;
            }
            let key = (
                citation.page_path.clone(),
                citation.cited_page_path.clone(),
            );
            if !seen_links.insert(key) {
                continue;
            }
            all_links.push(KnowledgeGraphLink {
                source: citation.page_path,
                target: citation.cited_page_path,
            });
        }

        let mut adjacency_both = HashMap::<String, Vec<String>>::new();
        let mut adjacency_out = HashMap::<String, Vec<String>>::new();
        let mut adjacency_in = HashMap::<String, Vec<String>>::new();
        for node_id in node_map.keys() {
            adjacency_both.insert(node_id.clone(), Vec::new());
            adjacency_out.insert(node_id.clone(), Vec::new());
            adjacency_in.insert(node_id.clone(), Vec::new());
        }
        for link in &all_links {
            adjacency_both
                .entry(link.source.clone())
                .or_default()
                .push(link.target.clone());
            adjacency_both
                .entry(link.target.clone())
                .or_default()
                .push(link.source.clone());
            adjacency_out
                .entry(link.source.clone())
                .or_default()
                .push(link.target.clone());
            adjacency_in
                .entry(link.target.clone())
                .or_default()
                .push(link.source.clone());
        }

        let effective_hop = hop.clamp(1, 3);
        let effective_limit_nodes = if limit_nodes == 0 {
            500
        } else {
            limit_nodes.min(5000)
        };
        let effective_limit_links = if limit_links == 0 {
            2000
        } else {
            limit_links.min(20000)
        };

        let mut visited = HashSet::<String>::new();
        visited.insert(center_id.clone());
        let mut queue = VecDeque::<(String, u8)>::new();
        queue.push_back((center_id.clone(), 0));
        let mut truncated = false;

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= effective_hop {
                continue;
            }
            let neighbors = match direction {
                KnowledgeGraphDirection::Both => adjacency_both.get(&current),
                KnowledgeGraphDirection::Out => adjacency_out.get(&current),
                KnowledgeGraphDirection::In => adjacency_in.get(&current),
            };
            let Some(neighbors) = neighbors else {
                continue;
            };
            for neighbor in neighbors {
                if visited.contains(neighbor) {
                    continue;
                }
                if visited.len() >= effective_limit_nodes {
                    truncated = true;
                    break;
                }
                visited.insert(neighbor.clone());
                queue.push_back((neighbor.clone(), depth + 1));
            }
            if truncated {
                break;
            }
        }

        let mut nodes = visited
            .iter()
            .filter_map(|node_id| node_map.get(node_id).cloned())
            .collect::<Vec<_>>();
        nodes.sort_by(|left, right| left.label.cmp(&right.label));

        let mut links = all_links
            .into_iter()
            .filter(|link| visited.contains(&link.source) && visited.contains(&link.target))
            .collect::<Vec<_>>();
        links.sort_by(|left, right| {
            left.source
                .cmp(&right.source)
                .then(left.target.cmp(&right.target))
        });
        if links.len() > effective_limit_links {
            links.truncate(effective_limit_links);
            truncated = true;
        }

        Ok(KnowledgeSubgraphData {
            meta: KnowledgeSubgraphMeta {
                center_page_path: center_id,
                hop: effective_hop,
                direction,
                truncated,
                node_count: nodes.len(),
                link_count: links.len(),
            },
            nodes,
            links,
        })
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

        // 先更新 guard，persist_config 会从 guard 读取云端字段
        {
            let mut guard = self.inner.lock().expect("状态锁已被污染");
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

    /// 读取默认 OCR Provider 配置。
    pub fn get_ocr_config(&self) -> Option<String> {
        let guard = self.inner.lock().expect("状态锁已被污染");
        guard.default_ocr_provider.clone()
    }

    /// 保存默认 OCR Provider 配置并持久化到磁盘。
    pub fn set_ocr_config(&self, provider: Option<String>) -> Result<(), String> {
        let (mode, vault_path, query_top_k, expected_snapshot) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (
                guard.mode,
                guard.vault_path.clone(),
                guard.query_top_k,
                guard.config_snapshot.clone(),
            )
        };

        // 更新内存状态
        {
            let mut guard = self.inner.lock().expect("状态锁已被污染");
            guard.default_ocr_provider = provider;
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
                    "OCR Provider 配置已保存".to_string(),
                    current_timestamp_ms(),
                );
                Ok(())
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("OCR Provider 配置持久化失败: {}", err),
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

        self.record_outbox_event(
            "vault_initialized",
            serde_json::json!({
                "vault_path": result.vault_path.clone(),
                "created_paths": result.created_paths.clone(),
                "message": result.message.clone(),
            }),
        );

        Ok(result)
    }

    /// 使用模板初始化 Vault。
    pub fn init_vault_with_template(
        &self,
        vault_path: PathBuf,
        template_schema: String,
        template_purpose: String,
        extra_dirs: Vec<String>,
    ) -> Result<VaultInitResult, String> {
        let mut result = self.init_vault(vault_path.clone())?;

        // 写入模板文件（通常放在 wiki 目录下）
        let wiki_dir = vault_path.join("wiki");
        if !wiki_dir.exists() {
            fs::create_dir_all(&wiki_dir).map_err(|e| format!("创建 wiki 目录失败: {}", e))?;
        }

        let schema_path = wiki_dir.join("schema.md");
        if !schema_path.exists() {
            fs::write(&schema_path, template_schema)
                .map_err(|e| format!("创建 schema.md 失败: {}", e))?;
            result.created_paths.push(schema_path.to_string_lossy().to_string());
        }

        let purpose_path = wiki_dir.join("purpose.md");
        if !purpose_path.exists() {
            fs::write(&purpose_path, template_purpose)
                .map_err(|e| format!("创建 purpose.md 失败: {}", e))?;
            result.created_paths.push(purpose_path.to_string_lossy().to_string());
        }

        // 创建额外目录
        for dir in extra_dirs {
            let target_dir = if dir.starts_with("wiki/") || dir == "wiki" {
                vault_path.join(&dir)
            } else {
                wiki_dir.join(&dir)
            };

            if !target_dir.exists() {
                fs::create_dir_all(&target_dir)
                    .map_err(|e| format!("创建额外目录 {:?} 失败: {}", target_dir, e))?;
                result.created_paths.push(target_dir.to_string_lossy().to_string());
            }
        }

        self.push_log(
            LogLevel::Info,
            format!("Vault 模板初始化完成: {}", vault_path.to_string_lossy()),
        );

        result.message = format!("Vault 模板初始化已完成：{}", vault_path.to_string_lossy());
        Ok(result)
    }

    pub async fn ingest_markdown(&self, source_path: PathBuf) -> Result<IngestResult, String> {
        let source_path_text = source_path.to_string_lossy().to_string();

        // 记录开始导入事件
        self.record_outbox_event(
            "ingest_started",
            serde_json::json!({
                "source_path": source_path_text.clone(),
                "task_id": current_timestamp_ms(),
            }),
        );

        let vault_path_result = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())
        };
        let vault_path = match vault_path_result {
            Ok(path) => path,
            Err(err) => {
                self.record_ingest_failed_event(&source_path_text, &err);
                return Err(err);
            }
        };

        // 读取源文件内容以生成 LLM 摘要
        let source_content = match fs::read_to_string(&source_path) {
            Ok(content) => content,
            Err(err) => {
                let message = format!("读取源文件失败: {}", err);
                self.record_ingest_failed_event(&source_path_text, &message);
                return Err(message);
            }
        };

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
                self.record_ingest_failed_event(&source_path_text, &err);
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

        // 步骤5：嵌入向量化与持久化（始终走本地 Ollama embed 模型，不走云端）
        self.emit_progress("ingest_progress", "embedding", "正在向量化（本地 Ollama）...");
        let embed_provider = self.get_embed_provider();
        // 截断到 4096 字符以控制 embedding 请求体大小
        let embed_content: String = source_content.chars().take(4096).collect();
        match embed_provider.embed(&embed_content).await {
            Ok(embedding) => {
                if let Err(e) = db::upsert_embedding(&db_path, &result.wiki_path, &embedding) {
                    self.push_log(LogLevel::Warn, format!("写入向量数据库失败: {}", e));
                }
            }
            Err(e) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("向量化失败（跳过，不影响摄入）: {}", e),
                );
            }
        }

        result.entities = entities;
        result.updated_pages = updated_pages;

        self.record_outbox_event(
            "ingest_completed",
            serde_json::json!({
                "source_path": result.source_path.clone(),
                "raw_path": result.raw_path.clone(),
                "wiki_path": result.wiki_path.clone(),
                "entities": result.entities.clone(),
                "updated_pages": result.updated_pages.clone(),
            }),
        );

        Ok(result)
    }

    /// 导入任意支持格式文件（按扩展名自动路由）。
    pub async fn ingest_file_impl(
        &self,
        source_path: &str,
        ocr_provider: Option<&str>,
    ) -> Result<crate::models::IngestResult, String> {
        let source_path_buf = PathBuf::from(source_path);
        validate_ingest_source_path(&source_path_buf)?;

        let extension = source_path_buf
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();

        match extension.as_str() {
            "md" | "markdown" => self.ingest_markdown(source_path_buf).await,
            "pdf" => self.ingest_pdf_impl(source_path).await,
            "txt" => {
                let content_bytes = fs::read(&source_path_buf)
                    .map_err(|err| format!("读取文本文件失败：{}", err))?;
                let content = String::from_utf8_lossy(&content_bytes).to_string();
                self.ingest_text_via_temp_markdown(&source_path_buf, content, "txt")
                    .await
            }
            "docx" => {
                let content = extract_text_from_docx(&source_path_buf)?;
                self.ingest_text_via_temp_markdown(&source_path_buf, content, "docx")
                    .await
            }
            "pptx" => {
                let content = extract_text_from_pptx(&source_path_buf)?;
                self.ingest_text_via_temp_markdown(&source_path_buf, content, "pptx")
                    .await
            }
            ext if is_supported_image_extension(ext) => {
                let resolved_provider = normalize_ocr_provider(ocr_provider);
                let content = extract_text_from_image_with_fallback(
                    &source_path_buf,
                    resolved_provider,
                )?;
                self.ingest_text_via_temp_markdown(&source_path_buf, content, "ocr")
                    .await
            }
            _ => Err(format!(
                "不支持的文件扩展名：{}。当前支持 md/markdown/pdf/docx/pptx/txt/png/jpg/jpeg/bmp/webp/tif/tiff/gif",
                if extension.is_empty() {
                    "(无扩展名)"
                } else {
                    extension.as_str()
                }
            )),
        }
    }

    /// 读取 PDF 文本后复用现有 Markdown ingest 流程。
    pub async fn ingest_pdf_impl(
        &self,
        source_path: &str,
    ) -> Result<crate::models::IngestResult, String> {
        let source_path_text = source_path.trim().to_string();

        // 记录开始导入事件
        self.record_outbox_event(
            "ingest_started",
            serde_json::json!({
                "source_path": source_path_text.clone(),
                "task_id": current_timestamp_ms(),
            }),
        );

        let source_path_buf = PathBuf::from(&source_path_text);
        if let Err(err) = validate_pdf_source_path(&source_path_buf) {
            self.record_ingest_failed_event(&source_path_text, &err);
            return Err(err);
        }

        let extracted_text = match extract_text_from_pdf(&source_path_buf) {
            Ok(text) => text,
            Err(err) => {
                if should_fallback_to_pdf_ocr(&err) {
                    let primary_provider = {
                        let guard = self.inner.lock().expect("状态锁已被污染");
                        normalize_ocr_provider(guard.default_ocr_provider.as_deref())
                    };

                    match extract_text_from_pdf_via_ocr(&source_path_buf, primary_provider) {
                        Ok(ocr_output) => {
                            self.record_outbox_event(
                                "ingest_pdf_ocr_fallback",
                                serde_json::json!({
                                    "source_path": source_path_text.clone(),
                                    "primary_provider": primary_provider.as_str(),
                                    "page_count": ocr_output.page_count,
                                }),
                            );
                            return self
                                .ingest_text_via_temp_markdown(
                                    &source_path_buf,
                                    ocr_output.text,
                                    "pdf_ocr",
                                )
                                .await
                                .map(|mut result| {
                                    result.message = format!(
                                        "{}（已自动 OCR 回退：provider={}，页数={}）",
                                        result.message,
                                        primary_provider.as_str(),
                                        ocr_output.page_count
                                    );
                                    result
                                })
                                .map_err(|ingest_err| {
                                    self.record_ingest_failed_event(&source_path_text, &ingest_err);
                                    ingest_err
                                });
                        }
                        Err(ocr_err) => {
                            let message = build_pdf_ocr_fallback_failure_message(&err, &ocr_err);
                            self.record_ingest_failed_event(&source_path_text, &message);
                            return Err(message);
                        }
                    }
                }

                self.record_ingest_failed_event(&source_path_text, &err);
                return Err(err);
            }
        };
        self.ingest_text_via_temp_markdown(&source_path_buf, extracted_text, "pdf")
            .await
            .map_err(|err| {
                self.record_ingest_failed_event(&source_path_text, &err);
                err
            })
    }

    /// 将提取后的纯文本写入临时 Markdown，再复用 ingest_markdown。
    async fn ingest_text_via_temp_markdown(
        &self,
        source_path: &Path,
        content: String,
        route_tag: &str,
    ) -> Result<crate::models::IngestResult, String> {
        let normalized = content.replace('\u{0}', "").trim().to_string();
        if normalized.is_empty() {
            return Err(format!(
                "{} 提取结果为空，请确认文件内容可读取",
                route_tag.to_uppercase()
            ));
        }

        let tmp_path =
            std::env::temp_dir().join(format!("llm_wiki_{}_{}.md", route_tag, uuid_v4_short()));
        tokio::fs::write(&tmp_path, normalized)
            .await
            .map_err(|e| format!("写入临时 Markdown 失败：{e}"))?;

        let mut result = self.ingest_markdown(tmp_path.clone()).await;

        // 无论 ingest 成功或失败都尝试清理临时文件。
        let _ = tokio::fs::remove_file(&tmp_path).await;

        if let Ok(inner) = &mut result {
            let source_display = source_path.to_string_lossy().to_string();
            let tmp_display = tmp_path.to_string_lossy().to_string();
            inner.source_path = source_display.clone();
            inner.message = inner.message.replace(&tmp_display, &source_display);
        }

        result
    }

    /// 拉取 URL 文本内容后走现有 ingest 流程
    pub async fn ingest_url_impl(&self, url: &str) -> Result<crate::models::IngestResult, String> {
        let source_url = url.trim().to_string();

        // 记录开始导入事件
        self.record_outbox_event(
            "ingest_started",
            serde_json::json!({
                "source_path": source_url.clone(),
                "task_id": current_timestamp_ms(),
            }),
        );

        // 1. 用 reqwest 拉取 URL 文本（超时 30s）
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| {
                let message = format!("构建 HTTP 客户端失败：{e}");
                self.record_ingest_failed_event(&source_url, &message);
                message
            })?;

        let response = client
            .get(&source_url)
            .header("User-Agent", "llm-wiki/1.0")
            .send()
            .await
            .map_err(|e| {
                let message = format!("拉取 URL 失败：{e}");
                self.record_ingest_failed_event(&source_url, &message);
                message
            })?;

        let status = response.status();
        if !status.is_success() {
            let message = format!("URL 请求失败，HTTP {status}");
            self.record_ingest_failed_event(&source_url, &message);
            return Err(message);
        }

        let text = response
            .text()
            .await
            .map_err(|e| {
                let message = format!("读取响应内容失败：{e}");
                self.record_ingest_failed_event(&source_url, &message);
                message
            })?;

        if text.trim().is_empty() {
            let message = "URL 返回内容为空".to_string();
            self.record_ingest_failed_event(&source_url, &message);
            return Err(message);
        }

        // 2. 将文本写入临时 Markdown 文件，复用 ingest_markdown
        let tmp_path = std::env::temp_dir().join(format!("llm_wiki_url_{}.md", uuid_v4_short()));
        tokio::fs::write(&tmp_path, &text)
            .await
            .map_err(|e| {
                let message = format!("写入临时文件失败：{e}");
                self.record_ingest_failed_event(&source_url, &message);
                message
            })?;

        let result = self.ingest_markdown(tmp_path.clone()).await;

        // 3. 清理临时文件（忽略错误）
        let _ = tokio::fs::remove_file(&tmp_path).await;

        result.map_err(|err| {
            self.record_ingest_failed_event(&source_url, &err);
            err
        })
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
        mut on_chunk: Option<&mut (dyn FnMut(String) + Send)>,
    ) -> (String, String) {
        let fallback_answer = || build_query_answer(question, matches);

        let provider = match provider {
            Some(provider) => provider,
            None => {
                let fallback = fallback_answer();
                if !fallback.is_empty() {
                    if let Some(handler) = on_chunk.as_deref_mut() {
                        handler(fallback.clone());
                    }
                }
                self.push_log(
                    LogLevel::Warn,
                    "本地 LLM Provider 不可用，Query 已回退到规则回答".to_string(),
                );
                return (fallback, "rule".to_string());
            }
        };

        let prompt = build_query_prompt(question, matches);

        let streamed = {
            let on_chunk_ref = &mut on_chunk;
            let mut chunk_forwarder = |chunk: String| {
                if let Some(handler) = on_chunk_ref.as_deref_mut() {
                    handler(chunk);
                }
            };
            provider.complete_stream(&prompt, &mut chunk_forwarder).await
        };

        match streamed {
            Ok(answer) => {
                let answer = answer.trim().to_string();
                if answer.is_empty() {
                    let fallback = fallback_answer();
                    if !fallback.is_empty() {
                        if let Some(handler) = on_chunk.as_deref_mut() {
                            handler(fallback.clone());
                        }
                    }
                    self.push_log(
                        LogLevel::Warn,
                        "本地 LLM 返回空回答，Query 已回退到规则回答".to_string(),
                    );
                    (fallback, "rule".to_string())
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
                let fallback = fallback_answer();
                if !fallback.is_empty() {
                    if let Some(handler) = on_chunk.as_deref_mut() {
                        handler(fallback.clone());
                    }
                }
                self.push_log(
                    LogLevel::Warn,
                    format!("本地 LLM Query 合成失败: {}，已回退到规则回答", err),
                );
                (fallback, "rule".to_string())
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
                let tags = read_page_tags(Path::new(&page.path));
                WikiPageItem {
                    title: page.title,
                    path: page.path,
                    display_path: Some(display_path),
                    summary: page.summary,
                    updated_at: page.updated_at,
                    score: 0.0,
                    tags,
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
                let tags = read_page_tags(Path::new(&page.path));
                WikiPageItem {
                    title: page.title,
                    path: page.path,
                    display_path: Some(display_path),
                    summary: page.summary,
                    updated_at: page.updated_at,
                    score: page.score,
                    tags,
                }
            })
            .collect())
    }

    /// 获取所有 wiki 页面路径并根据查询进行模糊匹配（忽略大小写）。
    pub fn search_wiki_paths(
        &self,
        query: String,
    ) -> Result<Vec<String>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        let pages = db::list_all_wiki_pages(&db_path)?;

        let query_lower = query.to_lowercase();
        Ok(pages
            .into_iter()
            .filter(|p| p.path.to_lowercase().contains(&query_lower))
            .map(|p| p.path)
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

    /// 设置或取消 Wiki 页面的 stale 标记（直接修改 frontmatter）。
    pub fn set_page_stale(&self, page_path: String, stale: bool) -> Result<(), String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };
        let vault_path = vault_path.ok_or_else(|| "请先初始化 Vault".to_string())?;

        // 规范化并安全检查路径
        let abs_path = if std::path::Path::new(&page_path).is_absolute() {
            std::path::PathBuf::from(&page_path)
        } else {
            vault_path.join("wiki").join(&page_path)
        };
        let abs_path = abs_path
            .canonicalize()
            .map_err(|e| format!("页面路径无效: {}", e))?;
        let wiki_root = vault_path.join("wiki")
            .canonicalize()
            .map_err(|e| format!("wiki 目录无效: {}", e))?;
        if !abs_path.starts_with(&wiki_root) {
            return Err("禁止操作 wiki 目录之外的文件".to_string());
        }

        let content = fs::read_to_string(&abs_path)
            .map_err(|e| format!("读取页面失败: {}", e))?;

        let updated = set_frontmatter_stale_field(&content, stale);

        fs::write(&abs_path, &updated)
            .map_err(|e| format!("写入页面失败: {}", e))?;

        self.push_log(
            LogLevel::Info,
            format!(
                "页面 stale 标记已{}: {}",
                if stale { "设置" } else { "取消" },
                abs_path.to_string_lossy()
            ),
        );
        Ok(())
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

        // 1. 扫描失效 wiki-link
        let link_regex = regex::Regex::new(r"\[\[([^|\]]+)(?:\|[^\]]+)?\]\]").unwrap();
        for page_path in &wiki_page_paths {
            if let Ok(content) = fs::read_to_string(page_path) {
                for caps in link_regex.captures_iter(&content) {
                    let target_name = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                    if target_name.is_empty() { continue; }

                    if resolve_existing_wiki_page_path(vault_path, target_name).is_err() {
                        issues.push(LintIssue {
                            code: "broken_wikilink".to_string(),
                            severity: "warning".to_string(),
                            message: format!("页面存在失效的 wiki-link：指向不存在的目标 {}", target_name),
                            path: Some(page_path.clone()),
                            suggestion: "请修复链接名称，或确认该页面已创建。".to_string(),
                        });
                    }
                }
            }
        }

        let (_broken_wiki_links, outbound_wiki_links, inbound_wiki_link_counts) =
            collect_wiki_link_graph(vault_path, &wiki_page_paths);

        // 注意：collect_wiki_link_graph 返回的 broken_wiki_links 与我们手动实现的逻辑重叠，建议优先使用其中之一或整合。
        // 为保持一致性，如果手动实现已覆盖需求，可以移除此处 collect_wiki_link_graph 返回的旧逻辑或根据业务需要调整。
        for (source_path, missing_targets) in collect_xref_missing_sources(&outbound_wiki_links) {
            issues.push(LintIssue {
                code: "xref_missing".to_string(),
                severity: "warning".to_string(),
                message: format!(
                    "页面缺少反向交叉引用：{} -> {}",
                    source_path,
                    missing_targets.join(", ")
                ),
                path: Some(source_path),
                suggestion: "应用补丁为目标页面追加指向当前页的 See Also 反向链接".to_string(),
            });
        }

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
                let inbound = inbound_wiki_link_counts.get(path).copied().unwrap_or(0);
                if inbound == 0 {
                    issues.push(LintIssue {
                        code: "orphan".to_string(),
                        severity: "warning".to_string(),
                        message: format!("页面未被 index.md 或其他页面引用: {}", path),
                        path: Some(path.clone()),
                        suggestion: "把页面加入 index.md，或在相关页面补齐 wiki-link 引用".to_string(),
                    });
                }
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

        // 检查 wiki 目录下标记为 stale 的页面
        let wiki_dir = vault_path.join("wiki");
        if wiki_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&wiki_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                        continue;
                    }
                    if let Ok(file_content) = fs::read_to_string(&path) {
                        if let Some(fm) = parse_wiki_frontmatter(&file_content) {
                            if fm.stale == Some(true) {
                                issues.push(LintIssue {
                                    code: "STALE_PAGE".to_string(),
                                    severity: "warning".to_string(),
                                    message: format!(
                                        "页面已标记为过时，建议更新或删除: {}",
                                        path.file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("unknown")
                                    ),
                                    path: Some(path.to_string_lossy().to_string()),
                                    suggestion: "更新页面内容后取消 stale 标记，或删除该页面".to_string(),
                                });
                            }
                        }
                    }
                }
            }
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
        provider: Option<Arc<dyn LlmProvider>>,
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
            let semantic = Self::run_semantic_lint(pages, Some(provider)).await;
            merge_lint_with_semantic(rules, semantic)
        }
    }

    /// 运行完整 Lint 并写入 outbox。
    pub async fn run_lint_with_outbox(&self) -> LintReport {
        let report = self.lint_report_full_future().await;
        self.record_outbox_event(
            "lint_run_finished",
            serde_json::json!({
                "checked_at": report.checked_at.clone(),
                "issue_count": report.issues.len(),
                "severity_stats": {
                    "error": report.severity_stats.error,
                    "warning": report.severity_stats.warning,
                    "info": report.severity_stats.info,
                },
            }),
        );
        report
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
            "ORPHAN_WIKI_PAGE" | "orphan" => {
                let path = input_path
                    .as_deref()
                    .ok_or_else(|| format!("{} 需要提供 path", issue_code.as_str()))?;
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
            "broken_wikilink" | "BROKEN_WIKILINK" => {
                let path = input_path
                    .as_deref()
                    .ok_or_else(|| format!("{} 需要提供 path", issue_code.as_str()))?;
                let page_path = resolve_existing_wiki_page_path(&vault_path, path)?;
                let replaced = rewrite_broken_wiki_links_in_page(&vault_path, &page_path)?;
                let message = if replaced > 0 {
                    format!("已将 {} 个失效 wiki-link 降级为纯文本", replaced)
                } else {
                    "页面中未发现可自动修复的失效 wiki-link".to_string()
                };
                (
                    replaced > 0,
                    message,
                    vec![page_path.to_string_lossy().to_string()],
                )
            }
            "xref_missing" | "XREF_MISSING" => {
                let path = input_path
                    .as_deref()
                    .ok_or_else(|| format!("{} 需要提供 path", issue_code.as_str()))?;
                let source_page = resolve_existing_wiki_page_path(&vault_path, path)?;
                let (updated, touched_paths) =
                    apply_missing_xref_backlinks(&vault_path, &source_page)?;
                let message = if updated > 0 {
                    format!("已补齐 {} 个反向交叉引用", updated)
                } else {
                    "未发现需要补齐的反向交叉引用".to_string()
                };
                (updated > 0, message, touched_paths)
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

    async fn query_embedding_route_paths(
        &self,
        db_path: &Path,
        question: &str,
        limit: usize,
    ) -> Vec<String> {
        if limit == 0 {
            return Vec::new();
        }
        let trimmed = question.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        let candidate_limit = (limit.saturating_mul(20))
            .max(limit)
            .min(QUERY_EMBED_ROUTE_MAX_CANDIDATES);
        let candidates = match db::list_embeddings(db_path, candidate_limit) {
            Ok(items) => items,
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("读取 embedding 候选失败，已跳过 embedding 召回: {}", err),
                );
                return Vec::new();
            }
        };

        if candidates.is_empty() {
            return Vec::new();
        }

        self.emit_progress("query_progress", "embedding", "正在执行 embedding 召回...");
        match self.get_embed_provider().embed(trimmed).await {
            Ok(query_embedding) => {
                crate::search::rank_embedding_paths_by_cosine(&query_embedding, &candidates, limit)
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("embedding 召回失败，已跳过该检索路径: {}", err),
                );
                Vec::new()
            }
        }
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

        // 步骤1：多路 RRF 融合检索
        self.emit_progress("query_progress", "searching", "多路 RRF 检索中...");
        let embedding_paths = if tokens.is_empty() {
            Vec::new()
        } else {
            self.query_embedding_route_paths(&db_path, &normalized_question, top_k * 4)
                .await
        };
        let extra_routes = if embedding_paths.is_empty() {
            Vec::new()
        } else {
            vec![("embedding".to_string(), embedding_paths)]
        };
        let (matches, search_strategy, fts_error, search_debug) = search_wiki_matches_rrf_with_extra_routes(
            &db_path,
            &wiki_dir,
            &tokens,
            &normalized_question,
            top_k,
            &extra_routes,
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
        self.emit_progress("query_progress", "answer_stream_start", "开始流式输出回答...");
        let mut emit_chunk = |chunk: String| {
            if !chunk.is_empty() {
                self.emit_progress("query_progress", "answer_chunk", &chunk);
            }
        };
        let (answer, answer_strategy) = self
            .generate_query_answer_with_provider(
                &normalized_question,
                &matches,
                provider,
                Some(&mut emit_chunk),
            )
            .await;
        self.emit_progress("query_progress", "answer_stream_done", "回答输出完成。");

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

        self.record_outbox_event(
            "query_answered",
            serde_json::json!({
                "question": normalized_question.clone(),
                "matched_pages": matched_pages.clone(),
                "search_strategy": search_strategy,
                "answer_strategy": answer_strategy.clone(),
                "top_k": top_k,
                "search_debug": search_debug.clone(),
            }),
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
            search_debug,
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
                self.record_outbox_event(
                    "query_saved_to_wiki",
                    serde_json::json!({
                        "question": input.question.clone(),
                        "page_title": result.page_title.clone(),
                        "wiki_path": result.wiki_path.clone(),
                        "citations": input.citations.len(),
                    }),
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

        self.record_outbox_event(
            "wiki_page_saved",
            serde_json::json!({
                "path": path,
                "content_length": content.chars().count(),
            }),
        );

        Ok(crate::models::SaveWikiPageResult {
            path: path.to_string(),
            message: format!("已保存并更新索引：{path}"),
        })
    }

    pub async fn rename_wiki_page_impl(
        &self,
        old_path: &str,
        new_name: &str,
    ) -> Result<crate::models::RenameWikiPageResult, String> {
        // 1. 验证新文件名（不允许路径分隔符、不能为空）
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err("新文件名不能为空".to_string());
        }
        if new_name.contains('/') || new_name.contains('\\') {
            return Err("新文件名不能包含路径分隔符".to_string());
        }
        // 确保以 .md 结尾
        let new_name = if new_name.ends_with(".md") {
            new_name.to_string()
        } else {
            format!("{new_name}.md")
        };

        // 2. 计算新路径（与旧文件同目录）
        let old_file = std::path::Path::new(old_path);
        let parent = old_file
            .parent()
            .ok_or_else(|| format!("无法获取父目录：{old_path}"))?;
        let new_file = parent.join(&new_name);
        let new_path_str = new_file.to_string_lossy().to_string();

        if new_file == old_file {
            return Ok(crate::models::RenameWikiPageResult {
                new_path: new_path_str,
                message: "文件名未变化".to_string(),
            });
        }

        if new_file.exists() {
            return Err(format!("目标文件已存在：{new_path_str}"));
        }

        // 3. 重命名文件
        std::fs::rename(old_file, &new_file)
            .map_err(|err| format!("文件重命名失败：{}", err))?;

        // 4. 读取新文件内容以更新 FTS
        let content = std::fs::read_to_string(&new_file).unwrap_or_default();
        let title = content
            .lines()
            .find(|l| l.starts_with("# "))
            .map(|l| l.trim_start_matches("# ").trim().to_string())
            .unwrap_or_else(|| new_name.trim_end_matches(".md").to_string());

        // 5. 更新数据库
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };

        if let Some(vault_path) = vault_path {
            let db_path = vault_path.join(".app").join("meta.db");
            if db_path.exists() {
                if let Err(err) = db::rename_wiki_page_in_db(
                    &db_path,
                    old_file,
                    &new_file,
                    &title,
                    &content,
                ) {
                    self.push_log(
                        LogLevel::Warn,
                        format!("数据库重命名失败（降级）：{err}"),
                    );
                }
            }
        }

        self.record_outbox_event(
            "wiki_page_renamed",
            serde_json::json!({
                "old_path": old_path,
                "new_path": new_path_str.clone(),
                "new_name": new_name,
            }),
        );

        Ok(crate::models::RenameWikiPageResult {
            new_path: new_path_str.clone(),
            message: format!("已重命名：{old_path} → {new_path_str}"),
        })
    }

    pub async fn delete_wiki_page_impl(
        &self,
        path: &str,
    ) -> Result<crate::models::DeleteWikiPageResult, String> {
        // 1. 删除 .md 文件
        let file_path = std::path::Path::new(path);
        if file_path.exists() {
            std::fs::remove_file(file_path)
                .map_err(|err| format!("删除文件失败：{}", err))?;
        }

        // 2. 清理数据库记录
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };

        let mut pruned_index_links = 0usize;
        if let Some(vault_path) = vault_path {
            let db_path = vault_path.join(".app").join("meta.db");
            if db_path.exists() {
                if let Err(err) = db::delete_wiki_page_from_db(&db_path, file_path) {
                    self.push_log(LogLevel::Warn, format!("数据库清理失败（降级）：{err}"));
                }
            }
            match prune_missing_index_links(&vault_path) {
                Ok(removed) => {
                    pruned_index_links = removed;
                }
                Err(err) => {
                    self.push_log(LogLevel::Warn, format!("index.md 清理失败（降级）：{err}"));
                }
            }
        }

        self.record_outbox_event(
            "wiki_page_deleted",
            serde_json::json!({
                "path": path,
                "pruned_index_links": pruned_index_links,
            }),
        );

        Ok(crate::models::DeleteWikiPageResult {
            path: path.to_string(),
            message: if pruned_index_links > 0 {
                format!("已删除：{path}（同步清理 index.md 失效链接 {pruned_index_links} 条）")
            } else {
                format!("已删除：{path}")
            },
        })
    }

    /// 启动时清理孤立 wiki 页面：DB 有记录但文件已不存在的条目。
    /// 在 setup hook 中调用，保证前端首次加载拿到的数据已是干净状态。
    pub fn purge_orphaned_wiki_pages(&self) {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };
        let Some(vault_path) = vault_path else {
            return; // vault 未配置，跳过
        };

        let db_path = vault_path.join(".app").join("meta.db");
        if !db_path.exists() {
            return;
        }

        let paths = match db::list_wiki_page_paths(&db_path) {
            Ok(p) => p,
            Err(err) => {
                eprintln!("[purge_orphaned] 读取 wiki_pages 失败: {err}");
                return;
            }
        };

        let mut purged = 0usize;
        for path_str in &paths {
            let file_path = std::path::Path::new(path_str);
            if !file_path.exists() {
                match db::delete_wiki_page_from_db(&db_path, file_path) {
                    Ok(()) => {
                        eprintln!("[purge_orphaned] 已清理孤立记录: {path_str}");
                        purged += 1;
                    }
                    Err(err) => {
                        eprintln!("[purge_orphaned] 清理失败 {path_str}: {err}");
                    }
                }
            }
        }

        let mut pruned_index_links = 0usize;
        match prune_missing_index_links(&vault_path) {
            Ok(removed) => {
                pruned_index_links = removed;
            }
            Err(err) => {
                eprintln!("[purge_orphaned] index.md 清理失败: {err}");
            }
        }

        if purged > 0 {
            eprintln!("[purge_orphaned] 启动清理完成，共删除 {purged} 条孤立 wiki 页面记录");
        }
        if pruned_index_links > 0 {
            eprintln!("[purge_orphaned] 启动清理完成，index.md 共移除 {pruned_index_links} 条失效链接");
        }
    }

    pub fn save_ask_history_impl(&self, question: &str) -> Result<(), String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };
        let Some(vault_path) = vault_path else {
            return Ok(());
        };
        let db_path = vault_path.join(".app").join("meta.db");
        if !db_path.exists() {
            return Ok(());
        }
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();
        db::save_ask_history(&db_path, question, &created_at)?;
        Ok(())
    }

    pub fn get_ask_history_impl(
        &self,
        limit: usize,
    ) -> Result<Vec<crate::models::AskHistoryItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };
        let Some(vault_path) = vault_path else {
            return Ok(Vec::new());
        };
        let db_path = vault_path.join(".app").join("meta.db");
        if !db_path.exists() {
            return Ok(Vec::new());
        }
        // 读取上限与落库上限一致，避免异常大值导致一次性扫描过多数据。
        let safe_limit = limit.min(db::ASK_HISTORY_MAX_ENTRIES);
        let records = db::list_ask_history(&db_path, safe_limit)?;
        Ok(records
            .into_iter()
            .map(|r| crate::models::AskHistoryItem {
                id: r.id,
                question: r.question,
                created_at: r.created_at,
            })
            .collect())
    }

    /// 清空 Ask 历史（DB 持久化）。
    pub fn clear_ask_history_impl(&self) -> Result<usize, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        };
        let Some(vault_path) = vault_path else {
            return Ok(0);
        };
        let db_path = vault_path.join(".app").join("meta.db");
        if !db_path.exists() {
            return Ok(0);
        }
        db::clear_ask_history(&db_path)
    }

    /// 按 id 增量读取 outbox 事件。
    pub fn get_outbox_events_impl(
        &self,
        last_id: i64,
        limit: usize,
    ) -> Result<Vec<OutboxEventItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let records = db::list_outbox_events_from_id(&db_path, last_id, limit)?;
        Ok(records
            .into_iter()
            .map(|item| OutboxEventItem {
                id: item.id,
                event_type: item.event_type,
                payload_json: item.payload_json,
                created_at: item.created_at,
                processed_at: item.processed_at,
                consumer_tag: item.consumer_tag,
            })
            .collect())
    }

    /// 标记 outbox 事件已消费。
    pub fn ack_outbox_events_impl(
        &self,
        up_to_id: i64,
        consumer_tag: &str,
    ) -> Result<OutboxAckResult, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let acked = db::ack_outbox_events(&db_path, up_to_id, consumer_tag, &current_timestamp_ms())?;
        Ok(OutboxAckResult {
            acked,
            up_to_id,
            consumer_tag: consumer_tag.trim().to_string(),
        })
    }

    /// 多轮会话问答（保留历史上下文 + 支持软取消）
    pub async fn query_ask_session(
        &self,
        session_id: String,
        question: String,
        options: QueryAskOptions,
    ) -> Result<QueryAnswerResult, String> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        const MAX_HISTORY_TURNS: usize = 6; // 最多保留最近 6 轮

        let normalized_question = question.trim().to_string();
        if normalized_question.is_empty() {
            return Err("问题不能为空".to_string());
        }

        let (mode, vault_path, default_top_k) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone(), guard.query_top_k)
        };
        let vault_path = vault_path.ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let wiki_dir = vault_path.join("wiki");
        let db_path = vault_path.join(".app").join("meta.db");
        let tokens = tokenize_query(&normalized_question);
        let top_k = normalize_top_k(options.top_k.or(Some(default_top_k)));

        // 将用户问题加入会话历史
        let user_turn = crate::models::AskTurn {
            role: "user".to_string(),
            content: normalized_question.clone(),
        };
        {
            let mut sessions = self.ask_sessions.lock().expect("sessions 锁已被污染");
            sessions
                .entry(session_id.clone())
                .or_default()
                .push(user_turn);
        }

        // 获取历史上下文（排除刚加入的用户轮）
        let history: Vec<crate::models::AskTurn> = {
            let sessions = self.ask_sessions.lock().expect("sessions 锁已被污染");
            if let Some(turns) = sessions.get(&session_id) {
                let len = turns.len();
                // 排除末尾刚加入的用户轮
                let end = len.saturating_sub(1);
                let start = end.saturating_sub(MAX_HISTORY_TURNS);
                turns[start..end].to_vec()
            } else {
                vec![]
            }
        };

        // 注册取消标志
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut flags = self.ask_cancel_flags.lock().expect("cancel_flags 锁已被污染");
            flags.insert(session_id.clone(), cancel_flag.clone());
        }

        // 多路 RRF 融合检索
        self.emit_progress("query_progress", "searching", "多路 RRF 检索中...");
        let embedding_paths = if tokens.is_empty() {
            Vec::new()
        } else {
            self.query_embedding_route_paths(&db_path, &normalized_question, top_k * 4)
                .await
        };
        let extra_routes = if embedding_paths.is_empty() {
            Vec::new()
        } else {
            vec![("embedding".to_string(), embedding_paths)]
        };
        let (matches, search_strategy, fts_error, search_debug) = search_wiki_matches_rrf_with_extra_routes(
            &db_path,
            &wiki_dir,
            &tokens,
            &normalized_question,
            top_k,
            &extra_routes,
        )
        .map_err(|e| {
            // 清理取消标志
            let mut flags = self.ask_cancel_flags.lock().expect("cancel_flags 锁已被污染");
            flags.remove(&session_id);
            e
        })?;

        if let Some(err) = fts_error {
            self.push_log(
                LogLevel::Warn,
                format!("FTS 查询失败，已降级为文件扫描: {}", err),
            );
        }

        let citations = matches
            .iter()
            .map(|item| {
                let display_path = friendly_display_path(std::path::Path::new(&item.page_path));
                QueryCitation {
                    page_path: item.page_path.clone(),
                    display_path: Some(display_path),
                    score: item.score,
                    excerpt: item.excerpt.clone(),
                }
            })
            .collect::<Vec<_>>();

        // LLM 合成（含历史 context）
        self.emit_progress("query_progress", "generating", "正在合成回答（LLM）...");
        let provider = self.get_llm_provider();
        self.emit_progress("query_progress", "answer_stream_start", "开始流式输出回答...");

        let cancel_for_closure = cancel_flag.clone();
        let mut emit_chunk = |chunk: String| {
            if cancel_for_closure.load(Ordering::Relaxed) {
                return; // 已取消，静默丢弃 chunk
            }
            if !chunk.is_empty() {
                self.emit_progress("query_progress", "answer_chunk", &chunk);
            }
        };

        // 构建含历史的 prompt
        let prompt = build_query_prompt_with_history(&normalized_question, &matches, &history);

        // 直接调用 complete_stream（绕过 generate_query_answer_with_provider 以使用自定义 prompt）
        let answer_strategy;
        let answer = if let Some(p) = provider {
            let streamed = p.complete_stream(&prompt, &mut emit_chunk).await;
            match streamed {
                Ok(raw) => {
                    let trimmed = raw.trim().to_string();
                    if trimmed.is_empty() {
                        answer_strategy = "rule".to_string();
                        build_query_answer(&normalized_question, &matches)
                    } else {
                        answer_strategy = "llm".to_string();
                        trimmed
                    }
                }
                Err(err) => {
                    self.push_log(
                        LogLevel::Warn,
                        format!("LLM 流式生成失败: {}，已回退到规则回答", err),
                    );
                    answer_strategy = "rule".to_string();
                    build_query_answer(&normalized_question, &matches)
                }
            }
        } else {
            answer_strategy = "rule".to_string();
            build_query_answer(&normalized_question, &matches)
        };

        self.emit_progress("query_progress", "answer_stream_done", "回答输出完成。");

        // 检查是否已取消
        let was_cancelled = cancel_flag.load(Ordering::Relaxed);

        // 清理取消标志
        {
            let mut flags = self.ask_cancel_flags.lock().expect("cancel_flags 锁已被污染");
            flags.remove(&session_id);
        }

        if was_cancelled {
            // 移除刚加入的用户轮，避免历史污染
            let mut sessions = self.ask_sessions.lock().expect("sessions 锁已被污染");
            if let Some(turns) = sessions.get_mut(&session_id) {
                turns.pop();
            }
            return Err("查询已取消".to_string());
        }

        // 将助手回答加入会话历史
        {
            let mut sessions = self.ask_sessions.lock().expect("sessions 锁已被污染");
            if let Some(turns) = sessions.get_mut(&session_id) {
                turns.push(crate::models::AskTurn {
                    role: "assistant".to_string(),
                    content: answer.clone(),
                });
            }
        }

        let matched_pages = matches
            .iter()
            .map(|item| item.page_path.clone())
            .collect::<Vec<_>>();

        self.push_log(
            LogLevel::Info,
            format!(
                "会话 Query 完成: session={}, '{}', 命中 {} 页",
                &session_id[..session_id.len().min(8)],
                normalized_question,
                matched_pages.len()
            ),
        );

        self.record_outbox_event(
            "query_session_answered",
            serde_json::json!({
                "session_id": session_id.clone(),
                "question": normalized_question.clone(),
                "matched_pages": matched_pages.clone(),
                "search_strategy": search_strategy,
                "answer_strategy": answer_strategy.clone(),
                "top_k": top_k,
                "search_debug": search_debug.clone(),
            }),
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
            search_debug,
        })
    }

    /// 取消正在进行的会话查询（软取消：停止 emit chunk）
    pub fn cancel_ask_session(&self, session_id: String) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let flags = self.ask_cancel_flags.lock().expect("cancel_flags 锁已被污染");
        if let Some(flag) = flags.get(&session_id) {
            flag.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    /// 清空会话历史（开启新对话）
    pub fn clear_ask_session(&self, session_id: String) -> Result<(), String> {
        let mut sessions = self.ask_sessions.lock().expect("sessions 锁已被污染");
        sessions.remove(&session_id);
        Ok(())
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
        // 从当前 guard 读取云端字段及 OCR 字段，确保不丢失已保存的配置
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
                default_ocr_provider: guard.default_ocr_provider.clone(),
                ollama_model: guard.ollama_model.clone(),
                ollama_base_url: guard.ollama_base_url.clone(),
                embed_ollama_model: guard.embed_ollama_model.clone(),
                embed_ollama_base_url: guard.embed_ollama_base_url.clone(),
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

    /// 记录 ingest 失败事件，供前端结束“处理中”状态并展示失败上下文。
    fn record_ingest_failed_event(&self, source_path: &str, error_message: &str) {
        self.record_outbox_event(
            "ingest_failed",
            serde_json::json!({
                "source_path": source_path,
                "error": error_message,
            }),
        );
    }

    /// 计算 outbox 对应的数据库路径（Vault 未初始化时返回 None）。
    fn outbox_db_path(&self) -> Option<PathBuf> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        }?;
        let db_path = vault_path.join(".app").join("meta.db");
        if db_path.exists() {
            Some(db_path)
        } else {
            None
        }
    }

    /// 追加 outbox 事件，失败仅记录日志，不中断主流程。
    fn record_outbox_event(&self, event_type: &str, payload: serde_json::Value) {
        let Some(db_path) = self.outbox_db_path() else {
            return;
        };
        let payload_json = match serde_json::to_string(&payload) {
            Ok(value) => value,
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("序列化 outbox 事件失败: {}", err),
                );
                return;
            }
        };
        if let Err(err) = db::append_outbox_event(
            &db_path,
            event_type,
            &payload_json,
            &current_timestamp_ms(),
        ) {
            self.push_log(
                LogLevel::Warn,
                format!("写入 outbox 事件失败: {}", err),
            );
        }
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

    /// 同步入队一条 ingest 任务，返回新 id。
    pub fn enqueue_ingest(&self, source_type: String, source_path: String) -> Result<i64, String> {
        let db_path = self.outbox_db_path()
            .ok_or_else(|| "Vault 未初始化，无法入队".to_string())?;
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        let now = current_timestamp_ms();
        let id = db::db_enqueue_ingest(&conn, &source_type, &source_path, &now)?;
        self.record_outbox_event(
            "ingest_queue_enqueued",
            serde_json::json!({ "id": id, "source_type": source_type, "source_path": source_path }),
        );
        Ok(id)
    }

    /// 列出所有 ingest 队列记录。
    pub fn list_ingest_queue(&self) -> Result<Vec<crate::models::IngestQueueItem>, String> {
        let db_path = self.outbox_db_path()
            .ok_or_else(|| "Vault 未初始化".to_string())?;
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        db::db_list_ingest_queue(&conn)
    }

    /// 取消一条 ingest 队列记录（queued → cancelled）。
    pub fn cancel_ingest_item(&self, id: i64) -> Result<(), String> {
        let db_path = self.outbox_db_path()
            .ok_or_else(|| "Vault 未初始化".to_string())?;
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        let now = current_timestamp_ms();
        db::db_update_ingest_queue_status(&conn, id, "cancelled", None, &now)
    }

    /// 重试一条失败/取消的 ingest 队列记录（failed/cancelled → queued）。
    pub fn retry_ingest_item(&self, id: i64) -> Result<(), String> {
        let db_path = self.outbox_db_path()
            .ok_or_else(|| "Vault 未初始化".to_string())?;
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        let now = current_timestamp_ms();
        db::db_update_ingest_queue_status(&conn, id, "queued", None, &now)
    }

    /// 计算给定页面路径列表中所有存在 embedding 的页面对的余弦相似度。
    /// 返回 HashMap，key 为 "pathA||pathB"（路径字典序排列），value 为余弦相似度。
    /// 仅返回相似度 >= min_sim（默认 0.25）的对，最多 max_pairs（默认 1000）对。
    pub fn get_page_embedding_similarities(
        &self,
        paths: Vec<String>,
    ) -> Result<std::collections::HashMap<String, f64>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "Vault 未初始化".to_string())?;

        let all_records = db::list_embeddings(&db_path, 2000)?;
        let path_set: std::collections::HashSet<&str> =
            paths.iter().map(|s| s.as_str()).collect();

        let records: Vec<_> = all_records
            .iter()
            .filter(|r| path_set.contains(r.page_path.as_str()))
            .collect();

        const MIN_SIM: f64 = 0.25;
        const MAX_PAIRS: usize = 1000;

        let mut pairs = std::collections::HashMap::new();
        'outer: for i in 0..records.len() {
            for j in (i + 1)..records.len() {
                let sim = db::cosine_similarity(&records[i].embedding, &records[j].embedding);
                if sim >= MIN_SIM {
                    let (a, b) = if records[i].page_path <= records[j].page_path {
                        (&records[i].page_path, &records[j].page_path)
                    } else {
                        (&records[j].page_path, &records[i].page_path)
                    };
                    pairs.insert(format!("{}||{}", a, b), sim);
                    if pairs.len() >= MAX_PAIRS {
                        break 'outer;
                    }
                }
            }
        }
        Ok(pairs)
    }

    /// 启动队列 worker（tauri 异步运行时串行消费）。
    /// 通过 tauri::AppHandle 获取托管的 AppState，避免另建独立实例。
    pub fn start_queue_worker(handle: tauri::AppHandle) {
        tauri::async_runtime::spawn(async move {
            // 启动时重置上次崩溃遗留的 running 任务
            {
                let state = handle.state::<AppState>();
                if let Some(db_path) = state.outbox_db_path() {
                    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                        let now = current_timestamp_ms();
                        let _ = db::db_reset_stale_running(&conn, &now);
                    }
                }
            }
            loop {
                let state = handle.state::<AppState>();

                // 取下一条 queued item
                let next = {
                    let Some(db_path) = state.outbox_db_path() else {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    };
                    let conn = match rusqlite::Connection::open(&db_path) {
                        Ok(c) => c,
                        Err(_) => {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                    };
                    match db::db_get_next_queued_item(&conn) {
                        Ok(item) => item,
                        Err(_) => {
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                    }
                };

                let item = match next {
                    Some(i) => i,
                    None => {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                let item_id = item.id;
                let source_type = item.source_type.clone();
                let source_path = item.source_path.clone();

                // 更新 status -> running
                {
                    let Some(db_path) = state.outbox_db_path() else { continue; };
                    if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                        let now = current_timestamp_ms();
                        let _ = db::db_update_ingest_queue_status(&conn, item_id, "running", None, &now);
                    }
                }
                state.record_outbox_event(
                    "ingest_queue_started",
                    serde_json::json!({ "id": item_id, "source_type": source_type, "source_path": source_path }),
                );

                // 根据 source_type 调用对应 impl
                let exec_result: Result<crate::models::IngestResult, String> = match source_type.as_str() {
                    "file" => state.ingest_file_impl(&source_path, None).await,
                    "url" => state.ingest_url_impl(&source_path).await,
                    "markdown" => state.ingest_markdown(std::path::PathBuf::from(&source_path)).await,
                    other => Err(format!("未知 source_type: {}", other)),
                };

                let now = current_timestamp_ms();
                match exec_result {
                    Ok(_) => {
                        if let Some(db_path) = state.outbox_db_path() {
                            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                                let _ = db::db_update_ingest_queue_status(&conn, item_id, "done", None, &now);
                            }
                        }
                        state.record_outbox_event(
                            "ingest_queue_done",
                            serde_json::json!({ "id": item_id }),
                        );
                    }
                    Err(err) => {
                        if let Some(db_path) = state.outbox_db_path() {
                            if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                                let _ = db::db_update_ingest_queue_status(&conn, item_id, "failed", Some(err.as_str()), &now);
                            }
                        }
                        state.record_outbox_event(
                            "ingest_queue_failed",
                            serde_json::json!({ "id": item_id, "error": err }),
                        );
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    // ─── Deep Research Public API ────────────────────────────────────────────

    /// 创建研究任务记录并在后台 spawn 研究管线，返回 task_id。
    pub fn start_research(
        &self,
        app_handle: tauri::AppHandle,
        topic: String,
        depth: i32,
        breadth: i32,
    ) -> Result<i64, String> {
        let db_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先初始化 Vault".to_string())?
                .join(".app")
                .join("meta.db")
        };
        // 确保 schema 存在
        db::ensure_meta_db(&db_path)?;

        let now = current_timestamp_ms();
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        let task_id = db::db_create_research_task(&conn, &topic, depth, breadth, &now)?;

        // 构造搜索配置（用传入的 depth/breadth 覆盖 config 中的默认值）
        let mut cfg = self.get_search_config();
        cfg.depth = depth;
        cfg.breadth = breadth;

        tauri::async_runtime::spawn(async move {
            start_research_task(app_handle, task_id, topic, cfg).await;
        });

        Ok(task_id)
    }

    /// 列出最近研究任务。
    pub fn list_research_tasks(&self) -> Result<Vec<crate::models::ResearchTaskItem>, String> {
        let db_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先初始化 Vault".to_string())?
                .join(".app")
                .join("meta.db")
        };
        db::ensure_meta_db(&db_path)?;
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        db::db_list_research_tasks(&conn)
    }

    /// 取消研究任务（将状态设为 cancelled）。
    pub fn cancel_research_task(&self, id: i64) -> Result<(), String> {
        let db_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先初始化 Vault".to_string())?
                .join(".app")
                .join("meta.db")
        };
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        let now = current_timestamp_ms();
        db::db_update_research_task(&conn, id, "cancelled", "[]", 0, None, None, &now)
    }
}

fn validate_ingest_source_path(source_path: &Path) -> Result<(), String> {
    if !source_path.exists() {
        return Err(format!("文件不存在：{}", source_path.to_string_lossy()));
    }

    if !source_path.is_file() {
        return Err(format!("路径不是文件：{}", source_path.to_string_lossy()));
    }

    Ok(())
}

fn is_supported_image_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tif" | "tiff" | "gif"
    )
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

fn extract_text_from_docx(source_path: &Path) -> Result<String, String> {
    let mut archive = open_zip_archive(source_path, "DOCX")?;
    let xml = read_zip_entry_utf8_lossy(&mut archive, "word/document.xml", "DOCX")?
        .ok_or_else(|| "DOCX 缺少 word/document.xml，无法提取正文".to_string())?;

    let text = extract_docx_paragraphs(&xml);
    normalize_extracted_doc_text(text, "DOCX")
}

/// 按段落（<w:p>）提取 DOCX 文本，段落间用空行分隔，段内 <w:t> 拼接。
fn extract_docx_paragraphs(xml: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut offset = 0usize;

    while let Some(para_start_rel) = xml[offset..].find("<w:p") {
        let para_start = offset + para_start_rel;
        // 找到段落结束标记 </w:p>
        let Some(para_end_rel) = xml[para_start..].find("</w:p>") else {
            break;
        };
        let para_end = para_start + para_end_rel + "</w:p>".len();
        let para_xml = &xml[para_start..para_end];

        // 提取段落内所有 <w:t> 文本并拼接（段内不加换行）
        let text = extract_xml_text_by_tag(para_xml, "w:t")
            .lines()
            .collect::<Vec<_>>()
            .join(" ");
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            paragraphs.push(trimmed);
        }
        offset = para_end;
    }

    paragraphs.join("\n\n")
}

fn extract_text_from_pptx(source_path: &Path) -> Result<String, String> {
    let mut archive = open_zip_archive(source_path, "PPTX")?;
    let mut slide_entries: Vec<(String, String)> = Vec::new();

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| format!("读取 PPTX 条目失败：{}", err))?;
        let name = file.name().to_string();
        if !is_pptx_slide_xml_entry(&name) {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|err| format!("读取 PPTX 页面内容失败：{}", err))?;
        slide_entries.push((name, String::from_utf8_lossy(&bytes).to_string()));
    }

    if slide_entries.is_empty() {
        return Err("PPTX 未检测到可读取幻灯片页面".to_string());
    }

    slide_entries.sort_by_key(|(name, _)| extract_slide_number(name));
    let mut pages = Vec::new();
    for (_, xml) in slide_entries {
        let text = extract_xml_text_by_tag(&xml, "a:t");
        if !text.is_empty() {
            pages.push(text);
        }
    }

    normalize_extracted_doc_text(pages.join("\n\n"), "PPTX")
}

/// 从路径中提取幻灯片编号数字，用于自然数排序。
/// 例如 "ppt/slides/slide12.xml" -> 12，未匹配时返回 0。
fn extract_slide_number(name: &str) -> u32 {
    // 匹配 "slide" 后面的数字，例如 "ppt/slides/slide12.xml" -> 12
    name.rfind("slide")
        .and_then(|pos| {
            let after = &name[pos + 5..]; // "slide" 长度 = 5
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u32>().ok()
        })
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OcrProvider {
    Tesseract,
    Paddle,
}

impl OcrProvider {
    fn as_str(self) -> &'static str {
        match self {
            OcrProvider::Tesseract => "tesseract",
            OcrProvider::Paddle => "paddle",
        }
    }
}

/// 归一化 OCR provider：非法值统一回退到 tesseract。
fn normalize_ocr_provider(provider: Option<&str>) -> OcrProvider {
    let normalized = provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("tesseract")
        .to_ascii_lowercase();

    match normalized.as_str() {
        "paddle" => OcrProvider::Paddle,
        _ => OcrProvider::Tesseract,
    }
}

/// 根据首选 provider 生成执行顺序（主用失败后自动回退）。
fn resolve_ocr_provider_order(primary: OcrProvider) -> [OcrProvider; 2] {
    match primary {
        OcrProvider::Tesseract => [OcrProvider::Tesseract, OcrProvider::Paddle],
        OcrProvider::Paddle => [OcrProvider::Paddle, OcrProvider::Tesseract],
    }
}

fn extract_text_from_image_with_fallback(
    source_path: &Path,
    primary_provider: OcrProvider,
) -> Result<String, String> {
    let provider_order = resolve_ocr_provider_order(primary_provider);
    let mut provider_errors = Vec::new();

    for provider in provider_order {
        match extract_text_from_image_with_provider(source_path, provider) {
            Ok(text) => return Ok(text),
            Err(err) => {
                provider_errors.push(format!(
                    "{}：{}",
                    provider.as_str(),
                    shorten_error_snippet(err.trim(), 60)
                ));
            }
        }
    }

    Err(format!(
        "图片 OCR 失败，已按顺序尝试 {} -> {}。{}",
        provider_order[0].as_str(),
        provider_order[1].as_str(),
        provider_errors.join("；")
    ))
}

fn extract_text_from_image_with_provider(
    source_path: &Path,
    provider: OcrProvider,
) -> Result<String, String> {
    match provider {
        OcrProvider::Tesseract => extract_text_from_image_with_tesseract(source_path),
        OcrProvider::Paddle => extract_text_from_image_with_paddle(source_path),
    }
}

fn extract_text_from_image_with_tesseract(source_path: &Path) -> Result<String, String> {
    let output = run_tesseract_ocr_command(source_path)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lower = stderr.to_lowercase();
        if stderr_lower.contains("failed loading language")
            || stderr_lower.contains("chi_sim")
            || stderr_lower.contains("traineddata")
        {
            return Err(
                "Tesseract 已安装，但缺少可用语言包（chi_sim/eng）。请安装对应 traineddata 或在设置中改用 PaddleOCR"
                    .to_string(),
            );
        }
        let short_reason = shorten_error_snippet(stderr.trim(), 80);
        if short_reason.is_empty() {
            return Err("Tesseract OCR 失败，请确认图片可读且已安装中文/英文语言包".to_string());
        }
        return Err(format!("Tesseract OCR 失败：{}", short_reason));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    normalize_extracted_doc_text(text, "图片 OCR")
}

fn extract_text_from_image_with_paddle(source_path: &Path) -> Result<String, String> {
    let output = run_paddle_ocr_command(source_path)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let short_reason = shorten_error_snippet(stderr.trim(), 80);
        if short_reason.is_empty() {
            return Err("Paddle OCR 失败，请确认命令可用且模型已安装".to_string());
        }
        return Err(format!("Paddle OCR 失败：{}", short_reason));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let extracted_text = parse_paddle_ocr_stdout(&stdout);
    normalize_extracted_doc_text(extracted_text, "图片 OCR")
}

/// 生成 tesseract 命令候选列表：
/// - 先尝试 PATH 中的 `tesseract`
/// - 再尝试 Windows 常见安装目录，避免 PATH 尚未刷新的场景
fn build_tesseract_command_candidates() -> Vec<String> {
    let mut candidates = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    let mut push_candidate = |value: String| {
        let normalized = value.trim().to_lowercase();
        if normalized.is_empty() || !seen.insert(normalized) {
            return;
        }
        candidates.push(value);
    };

    push_candidate("tesseract".to_string());

    if cfg!(target_os = "windows") {
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            push_candidate(format!(r"{}\Tesseract-OCR\tesseract.exe", program_files));
        }
        if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
            push_candidate(format!(r"{}\Tesseract-OCR\tesseract.exe", program_files_x86));
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            push_candidate(format!(
                r"{}\Programs\Tesseract-OCR\tesseract.exe",
                local_app_data
            ));
        }
        // 环境变量缺失时的硬编码兜底路径（Windows 默认安装位置）
        push_candidate(r"C:\Program Files\Tesseract-OCR\tesseract.exe".to_string());
        push_candidate(r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe".to_string());
    }

    candidates
}

fn run_tesseract_ocr_command(source_path: &Path) -> Result<std::process::Output, String> {
    let candidates = build_tesseract_command_candidates();
    let mut last_not_found: Option<io::Error> = None;

    for candidate in &candidates {
        match std::process::Command::new(candidate)
            .arg(source_path)
            .arg("stdout")
            .arg("-l")
            .arg("chi_sim+eng")
            .output()
        {
            Ok(output) => return Ok(output),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                last_not_found = Some(err);
            }
            Err(err) => {
                return Err(format!(
                    "调用 tesseract 失败（{}）：{}",
                    candidate,
                    shorten_error_snippet(&err.to_string(), 80)
                ));
            }
        }
    }

    let base_message = match last_not_found.as_ref() {
        Some(err) => format_tesseract_spawn_error(err),
        None => "调用 tesseract 失败：未知错误".to_string(),
    };
    Err(format!(
        "{}；已尝试命令：{}",
        base_message,
        candidates.join(" | ")
    ))
}

fn run_paddle_ocr_command(source_path: &Path) -> Result<std::process::Output, String> {
    std::process::Command::new("paddleocr")
        .arg("--image_dir")
        .arg(source_path)
        .arg("--use_angle_cls")
        .arg("true")
        .arg("--lang")
        .arg("ch")
        .output()
        .map_err(|err| format_paddle_spawn_error(&err))
}

fn format_tesseract_spawn_error(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "未检测到 tesseract 命令，请先安装 Tesseract OCR 并加入 PATH".to_string()
    } else {
        format!(
            "调用 tesseract 失败：{}",
            shorten_error_snippet(&err.to_string(), 60)
        )
    }
}

fn format_paddle_spawn_error(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "未检测到 paddleocr 命令，请先安装 PaddleOCR 并加入 PATH".to_string()
    } else {
        format!(
            "调用 paddleocr 失败：{}",
            shorten_error_snippet(&err.to_string(), 60)
        )
    }
}

fn normalize_extracted_doc_text(text: String, source_type: &str) -> Result<String, String> {
    let normalized = text.replace('\u{0}', "").trim().to_string();
    if normalized.is_empty() {
        Err(format!(
            "{} 提取结果为空，可能是扫描件、图片型内容或文档受保护",
            source_type
        ))
    } else {
        Ok(normalized)
    }
}

fn open_zip_archive(
    source_path: &Path,
    source_type: &str,
) -> Result<zip::ZipArchive<fs::File>, String> {
    let file = fs::File::open(source_path)
        .map_err(|err| format!("打开 {} 文件失败：{}", source_type, err))?;
    zip::ZipArchive::new(file).map_err(|err| format!("解析 {} 压缩结构失败：{}", source_type, err))
}

fn read_zip_entry_utf8_lossy<R: io::Read + io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    entry_name: &str,
    source_type: &str,
) -> Result<Option<String>, String> {
    match archive.by_name(entry_name) {
        Ok(mut file) => {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(|err| {
                format!("读取 {} 条目失败（{}）：{}", source_type, entry_name, err)
            })?;
            Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(err) => Err(format!(
            "读取 {} 条目失败（{}）：{}",
            source_type, entry_name, err
        )),
    }
}

fn is_pptx_slide_xml_entry(entry_name: &str) -> bool {
    entry_name.starts_with("ppt/slides/slide") && entry_name.ends_with(".xml")
}

/// 提取 PaddleOCR 输出中的识别文本，优先抓取引号内容。
fn parse_paddle_ocr_stdout(stdout: &str) -> String {
    let mut collected = Vec::new();

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let mut quoted_values = extract_quoted_segments(line);
        quoted_values.retain(|value| {
            let trimmed = value.trim();
            !trimmed.is_empty()
                && trimmed != "OCR"
                && trimmed != "result"
                && !trimmed.starts_with("http://")
                && !trimmed.starts_with("https://")
        });

        if !quoted_values.is_empty() {
            collected.extend(quoted_values);
            continue;
        }

        if line.starts_with('[') || line.contains("INFO") || line.contains("DEBUG") {
            continue;
        }
        collected.push(line.to_string());
    }

    collected.join("\n")
}

fn extract_quoted_segments(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    let mut results = Vec::new();

    while index < chars.len() {
        let quote = chars[index];
        if quote != '\'' && quote != '"' {
            index += 1;
            continue;
        }

        let mut end = index + 1;
        while end < chars.len() {
            if chars[end] == quote && chars[end.saturating_sub(1)] != '\\' {
                let value: String = chars[index + 1..end].iter().collect();
                if value
                    .chars()
                    .any(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch))
                {
                    results.push(value);
                }
                break;
            }
            end += 1;
        }

        index = end.saturating_add(1);
    }

    results
}

fn extract_xml_text_by_tag(xml: &str, tag_name: &str) -> String {
    let open_tag = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);
    let mut values = Vec::new();
    let mut offset = 0usize;

    while let Some(start_rel) = xml[offset..].find(&open_tag) {
        let start_idx = offset + start_rel;
        let Some(open_end_rel) = xml[start_idx..].find('>') else {
            break;
        };
        let content_start = start_idx + open_end_rel + 1;
        let Some(close_rel) = xml[content_start..].find(&close_tag) else {
            break;
        };
        let content_end = content_start + close_rel;
        let decoded = decode_xml_entities(&xml[content_start..content_end]);
        let trimmed = decoded.trim();
        if !trimmed.is_empty() {
            values.push(trimmed.to_string());
        }
        offset = content_end + close_tag.len();
    }

    values.join("\n")
}

fn decode_xml_entities(raw_text: &str) -> String {
    let chars: Vec<char> = raw_text.chars().collect();
    let mut decoded = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '&' {
            let mut end = index + 1;
            while end < chars.len() && chars[end] != ';' {
                end += 1;
            }

            if end < chars.len() && chars[end] == ';' {
                let entity: String = chars[index + 1..end].iter().collect();
                if let Some(value) = decode_xml_entity(&entity) {
                    decoded.push_str(&value);
                    index = end + 1;
                    continue;
                }
            }
        }

        decoded.push(chars[index]);
        index += 1;
    }

    decoded
}

fn decode_xml_entity(entity: &str) -> Option<String> {
    let decoded = match entity {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            let code = u32::from_str_radix(&entity[2..], 16).ok()?;
            char::from_u32(code)?.to_string()
        }
        _ if entity.starts_with('#') => {
            let code: u32 = entity[1..].parse().ok()?;
            char::from_u32(code)?.to_string()
        }
        _ => return None,
    };
    Some(decoded)
}

fn shorten_error_snippet(message: &str, max_chars: usize) -> String {
    message.chars().take(max_chars).collect()
}

fn extract_text_from_pdf(source_path: &Path) -> Result<String, String> {
    let file_bytes =
        fs::read(source_path).map_err(|err| format!("读取 PDF 原始字节失败：{}", err))?;
    let mut parse_error: Option<String> = None;

    match load_pdf_document_with_fallback(source_path, &file_bytes) {
        Ok(document) => {
            let pages = document.get_pages();
            let page_numbers: Vec<u32> = pages.keys().copied().collect();

            if !page_numbers.is_empty() {
                // 优先使用 lopdf 内置提取；失败时再走操作符降级解析。
                if let Ok(text) = document.extract_text(&page_numbers) {
                    if let Some(normalized) = normalize_extracted_pdf_text(&text) {
                        return Ok(normalized);
                    }
                }

                if let Some(text) = extract_text_from_pdf_fallback_ops(&document, &pages) {
                    return Ok(text);
                }
            }
        }
        Err(err) => {
            parse_error = Some(err);
        }
    }

    // 兼容兜底：尝试使用独立解析实现（pdf-extract）提取文本。
    if let Some(text) = extract_text_from_pdf_with_pdf_extract(&file_bytes) {
        return Ok(text);
    }

    // 当结构化解析失败时，尝试直接扫描 stream 并解压文本内容流。
    if let Some(text) = extract_text_from_pdf_raw_streams(&file_bytes) {
        return Ok(text);
    }

    if let Some(error_message) = parse_error {
        return Err(format!(
            "{}；并且未从原始流中提取到可用文本，可能是扫描件或字体编码不兼容",
            error_message
        ));
    }

    Err("提取 PDF 文本失败：未识别到可用文本，可能是扫描件或字体编码不兼容".to_string())
}

#[derive(Debug)]
struct PdfOcrFallbackOutput {
    text: String,
    page_count: usize,
}

/// 仅在“解析兼容/无文本/扫描件”类场景启用 PDF OCR 回退。
fn should_fallback_to_pdf_ocr(error_message: &str) -> bool {
    [
        "解析器暂不兼容",
        "未识别到可用文本",
        "扫描件",
        "字体编码不兼容",
        "未从原始流中提取到可用文本",
    ]
    .iter()
    .any(|keyword| error_message.contains(keyword))
}

/// 拼装 PDF OCR 回退失败文案，统一包含安装指引。
fn build_pdf_ocr_fallback_failure_message(parse_error: &str, ocr_error: &str) -> String {
    format!(
        "PDF 文本解析失败：{}；自动 OCR 回退失败：{}。安装指引：请安装 Poppler（确保 `pdftoppm` 可执行并已加入 PATH）；并安装 tesseract 或 paddleocr（至少一种）并加入 PATH。",
        shorten_error_snippet(parse_error.trim(), 160),
        shorten_error_snippet(ocr_error.trim(), 200)
    )
}

fn extract_text_from_pdf_via_ocr(
    source_path: &Path,
    primary_provider: OcrProvider,
) -> Result<PdfOcrFallbackOutput, String> {
    let temp_dir = std::env::temp_dir().join(format!("llm_wiki_pdf_ocr_{}", uuid_v4_short()));
    fs::create_dir_all(&temp_dir).map_err(|err| format!("创建 PDF OCR 临时目录失败：{}", err))?;

    let run_result = (|| {
        let output_prefix = temp_dir.join("page");
        let command_output = run_pdftoppm_png_command(source_path, &output_prefix)?;
        if !command_output.status.success() {
            let stderr = String::from_utf8_lossy(&command_output.stderr);
            let short_reason = shorten_error_snippet(stderr.trim(), 160);
            return Err(format!(
                "pdftoppm 转图失败：{}",
                if short_reason.is_empty() {
                    "请确认 PDF 文件可读且 Poppler 已正确安装"
                } else {
                    short_reason.as_str()
                }
            ));
        }

        let page_images = collect_pdftoppm_generated_pngs(&temp_dir, "page")?;
        let mut page_texts = Vec::new();
        let mut page_errors = Vec::new();

        for (page_number, image_path) in page_images.iter() {
            match extract_text_from_image_with_fallback(image_path, primary_provider) {
                Ok(text) => page_texts.push(format!("[第 {} 页]\n{}", page_number, text)),
                Err(err) => page_errors.push(format!(
                    "第 {} 页：{}",
                    page_number,
                    shorten_error_snippet(err.trim(), 120)
                )),
            }
        }

        if page_texts.is_empty() {
            let mut details = page_errors.join("；");
            if details.is_empty() {
                details = "未提取到任何页面文本".to_string();
            }
            return Err(format!("PDF OCR 未提取到可用文本：{}", details));
        }

        Ok(PdfOcrFallbackOutput {
            text: page_texts.join("\n\n"),
            page_count: page_images.len(),
        })
    })();

    let _ = fs::remove_dir_all(&temp_dir);
    run_result
}

fn run_pdftoppm_png_command(
    source_path: &Path,
    output_prefix: &Path,
) -> Result<std::process::Output, String> {
    std::process::Command::new("pdftoppm")
        .arg("-png")
        .arg("-r")
        .arg("220")
        .arg(source_path)
        .arg(output_prefix)
        .output()
        .map_err(|err| format_pdftoppm_spawn_error(&err))
}

fn format_pdftoppm_spawn_error(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "未检测到 pdftoppm 命令，请先安装 Poppler 并将 pdftoppm 加入 PATH".to_string()
    } else {
        format!(
            "调用 pdftoppm 失败：{}",
            shorten_error_snippet(&err.to_string(), 80)
        )
    }
}

fn collect_pdftoppm_generated_pngs(
    temp_dir: &Path,
    output_prefix: &str,
) -> Result<Vec<(usize, PathBuf)>, String> {
    let mut pages = Vec::new();
    let entries = fs::read_dir(temp_dir).map_err(|err| format!("读取 PDF 页图目录失败：{}", err))?;
    let expected_prefix = format!("{}-", output_prefix);

    for entry in entries {
        let entry = entry.map_err(|err| format!("读取 PDF 页图条目失败：{}", err))?;
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("png") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|v| v.to_str()) else {
            continue;
        };
        let Some(page_number_raw) = stem.strip_prefix(&expected_prefix) else {
            continue;
        };
        let Ok(page_number) = page_number_raw.parse::<usize>() else {
            continue;
        };
        pages.push((page_number, path));
    }

    pages.sort_by_key(|(page_number, _)| *page_number);
    if pages.is_empty() {
        return Err("pdftoppm 未生成可识别的 PNG 页面文件".to_string());
    }
    Ok(pages)
}

/// 兼容加载 PDF：标准加载失败后，尝试对二进制做轻量修复再重试。
/// 目的：避免“阅读器可打开但解析器严格失败”的误判。
fn load_pdf_document_with_fallback(
    source_path: &Path,
    file_bytes: &[u8],
) -> Result<lopdf::Document, String> {
    let primary_error = match lopdf::Document::load(source_path) {
        Ok(document) => return Ok(document),
        Err(err) => err,
    };
    let mut attempt_reasons = vec![format!(
        "标准加载失败：{}",
        shorten_error_snippet(&primary_error.to_string(), 80)
    )];

    if let Ok(document) = lopdf::Document::load_mem(file_bytes) {
        return Ok(document);
    }
    attempt_reasons.push("内存直读失败".to_string());

    let pdf_header_offset = find_subsequence(file_bytes, b"%PDF-");
    let pdf_eof_end = rfind_subsequence(file_bytes, b"%%EOF").map(|idx| idx + 5);

    if let Some(start) = pdf_header_offset.filter(|offset| *offset > 0) {
        if let Ok(document) = lopdf::Document::load_mem(&file_bytes[start..]) {
            return Ok(document);
        }
        attempt_reasons.push(format!("头偏移修复失败(start={})", start));
    }

    if let Some(end) = pdf_eof_end.filter(|offset| *offset < file_bytes.len()) {
        if let Ok(document) = lopdf::Document::load_mem(&file_bytes[..end]) {
            return Ok(document);
        }
        attempt_reasons.push(format!("EOF 截断修复失败(end={})", end));
    }

    if let (Some(start), Some(end)) = (pdf_header_offset, pdf_eof_end) {
        if start < end && (start > 0 || end < file_bytes.len()) {
            if let Ok(document) = lopdf::Document::load_mem(&file_bytes[start..end]) {
                return Ok(document);
            }
            attempt_reasons.push(format!("头尾联合修复失败(range={}..{})", start, end));
        }
    }

    Err(format!(
        "读取 PDF 失败：当前解析器暂不兼容该文件结构。可在阅读器中“另存为”后重试，或转图片后使用 OCR。{}",
        attempt_reasons.join("；")
    ))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn rfind_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    haystack.windows(needle.len()).rposition(|window| window == needle)
}

/// 无法建模 PDF 结构时，直接从原始 stream 中提取可解码文本。
fn extract_text_from_pdf_raw_streams(file_bytes: &[u8]) -> Option<String> {
    const STREAM_MARKER: &[u8] = b"stream";
    const ENDSTREAM_MARKER: &[u8] = b"endstream";

    let mut extracted_chunks = Vec::new();
    let mut cursor = 0usize;

    while cursor < file_bytes.len() {
        let Some(stream_rel) = find_subsequence(&file_bytes[cursor..], STREAM_MARKER) else {
            break;
        };
        let stream_marker_index = cursor + stream_rel;
        let mut stream_content_start = stream_marker_index + STREAM_MARKER.len();
        if stream_content_start >= file_bytes.len() {
            break;
        }

        // PDF stream 正文前通常紧跟换行。
        if file_bytes[stream_content_start..].starts_with(b"\r\n") {
            stream_content_start += 2;
        } else if file_bytes[stream_content_start..].starts_with(b"\n")
            || file_bytes[stream_content_start..].starts_with(b"\r")
        {
            stream_content_start += 1;
        }

        let Some(endstream_rel) =
            find_subsequence(&file_bytes[stream_content_start..], ENDSTREAM_MARKER)
        else {
            break;
        };
        let stream_content_end = stream_content_start + endstream_rel;
        if stream_content_end <= stream_content_start {
            cursor = stream_content_end.saturating_add(ENDSTREAM_MARKER.len());
            continue;
        }

        let stream_bytes = &file_bytes[stream_content_start..stream_content_end];
        let candidates = decode_pdf_stream_candidates(stream_bytes);
        for candidate in candidates {
            let Ok(content) = lopdf::content::Content::decode(&candidate) else {
                continue;
            };
            let decoded_text = extract_text_from_pdf_operations(&content.operations);
            let Some(normalized_text) = normalize_extracted_pdf_text(&decoded_text) else {
                continue;
            };
            if is_likely_meaningful_pdf_text(&normalized_text) {
                extracted_chunks.push(normalized_text);
            }
        }

        cursor = stream_content_end + ENDSTREAM_MARKER.len();
    }

    if extracted_chunks.is_empty() {
        None
    } else {
        normalize_extracted_pdf_text(&extracted_chunks.join("\n\n"))
    }
}

/// 为每个 stream 提供原始字节与 Flate 解压字节两个候选。
fn decode_pdf_stream_candidates(stream_bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut candidates = Vec::with_capacity(2);
    if !stream_bytes.is_empty() {
        candidates.push(stream_bytes.to_vec());
    }

    let mut decoded = Vec::new();
    let mut decoder = ZlibDecoder::new(stream_bytes);
    if decoder.read_to_end(&mut decoded).is_ok() && !decoded.is_empty() {
        candidates.push(decoded);
    } else {
        let mut trimmed_len = stream_bytes.len();
        while trimmed_len > 0
            && (stream_bytes[trimmed_len - 1] == b'\r' || stream_bytes[trimmed_len - 1] == b'\n')
        {
            trimmed_len -= 1;
        }
        let mut decoded_trimmed = Vec::new();
        let mut decoder_trimmed = ZlibDecoder::new(&stream_bytes[..trimmed_len]);
        if decoder_trimmed.read_to_end(&mut decoded_trimmed).is_ok() && !decoded_trimmed.is_empty()
        {
            candidates.push(decoded_trimmed);
        }
    }

    candidates
}

fn is_likely_meaningful_pdf_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.chars().count() < 12 {
        return false;
    }

    let ascii_letters = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();
    let cjk_chars = trimmed
        .chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .count();
    let meaningful_chars = ascii_letters + cjk_chars;
    let total_chars = trimmed.chars().count();

    meaningful_chars * 100 / total_chars >= 18
}

/// 使用 pdf-extract 作为解析器独立兜底，提升异常结构 PDF 兼容性。
fn extract_text_from_pdf_with_pdf_extract(file_bytes: &[u8]) -> Option<String> {
    let extracted = pdf_extract::extract_text_from_mem(file_bytes).ok()?;
    let normalized = normalize_extracted_pdf_text(&extracted)?;
    if is_likely_meaningful_pdf_text(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_extracted_pdf_text(text: &str) -> Option<String> {
    let normalized = text.replace('\u{0}', "").trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_text_from_pdf_fallback_ops(
    document: &lopdf::Document,
    pages: &std::collections::BTreeMap<u32, lopdf::ObjectId>,
) -> Option<String> {
    let mut all_pages_text = Vec::new();

    for page_id in pages.values() {
        let Ok(content_bytes) = document.get_page_content(*page_id) else {
            continue;
        };
        let Ok(content) = lopdf::content::Content::decode(&content_bytes) else {
            continue;
        };
        let page_text = extract_text_from_pdf_operations(&content.operations);
        if let Some(normalized) = normalize_extracted_pdf_text(&page_text) {
            all_pages_text.push(normalized);
        }
    }

    if all_pages_text.is_empty() {
        None
    } else {
        normalize_extracted_pdf_text(&all_pages_text.join("\n\n"))
    }
}

fn extract_text_from_pdf_operations(operations: &[lopdf::content::Operation]) -> String {
    let mut extracted = String::new();

    for operation in operations {
        match operation.operator.as_str() {
            "Tj" | "'" => {
                if let Some(value) = operation.operands.first() {
                    append_pdf_text_object(value, &mut extracted);
                    extracted.push('\n');
                }
            }
            "\"" => {
                if let Some(value) = operation.operands.last() {
                    append_pdf_text_object(value, &mut extracted);
                    extracted.push('\n');
                }
            }
            "TJ" => {
                if let Some(lopdf::Object::Array(items)) = operation.operands.first() {
                    for item in items {
                        append_pdf_text_object(item, &mut extracted);
                    }
                    extracted.push('\n');
                }
            }
            "T*" => extracted.push('\n'),
            _ => {}
        }
    }

    extracted
}

fn append_pdf_text_object(value: &lopdf::Object, output: &mut String) {
    if let lopdf::Object::String(bytes, _) = value {
        output.push_str(&String::from_utf8_lossy(bytes));
    }
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

fn collect_wiki_link_graph(
    vault_path: &Path,
    wiki_page_paths: &BTreeSet<String>,
) -> (
    Vec<(String, String)>,
    BTreeMap<String, BTreeSet<String>>,
    BTreeMap<String, usize>,
) {
    let mut broken_links = Vec::new();
    let mut outbound_links = BTreeMap::new();
    let mut inbound_counts = wiki_page_paths
        .iter()
        .map(|path| (path.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();

    for source_path in wiki_page_paths {
        let Ok(content) = fs::read_to_string(source_path) else {
            continue;
        };
        let mut existing_targets = BTreeSet::new();

        for raw_target in extract_wiki_link_targets(&content) {
            let Some(target_path) = resolve_wiki_link_target(vault_path, &raw_target) else {
                continue;
            };
            if Path::new(&target_path).exists() {
                if target_path != *source_path {
                    existing_targets.insert(target_path);
                }
            } else {
                broken_links.push((source_path.clone(), raw_target));
            }
        }

        if !existing_targets.is_empty() {
            for target in &existing_targets {
                *inbound_counts.entry(target.clone()).or_insert(0) += 1;
            }
            outbound_links.insert(source_path.clone(), existing_targets);
        }
    }

    (broken_links, outbound_links, inbound_counts)
}

fn collect_xref_missing_sources(
    outbound_links: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, Vec<String>> {
    let mut missing = BTreeMap::new();

    for (source, targets) in outbound_links {
        for target in targets {
            let has_reverse = outbound_links
                .get(target)
                .map(|reverse_targets| reverse_targets.contains(source))
                .unwrap_or(false);
            if !has_reverse {
                missing
                    .entry(source.clone())
                    .or_insert_with(Vec::new)
                    .push(target.clone());
            }
        }
    }

    missing
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
        "broken_wikilink" | "BROKEN_WIKILINK" => (
            "修复失效 wiki-link".to_string(),
            "应用补丁可将失效 wiki-link 自动降级为纯文本，后续再补正确链接".to_string(),
            format!(
                "```text\n页面：{}\n将失效 [[wiki-link]] 转成可读纯文本，避免继续指向不存在页面。\n```",
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
        "ORPHAN_WIKI_PAGE" | "orphan" => (
            "把页面挂回 index.md".to_string(),
            "将该页面加入 index.md，或确认其应保留为孤页".to_string(),
            format!(
                "```text\n- [[{}|{}]]\n```",
                lint_patch_link_target(issue.path.as_deref()),
                lint_patch_link_label(issue.path.as_deref())
            ),
        ),
        "xref_missing" | "XREF_MISSING" => (
            "补齐反向交叉引用".to_string(),
            "应用补丁会向目标页面追加 See Also 反向链接".to_string(),
            format!(
                "```text\n来源页面：{}\n为其已引用页面补充反向链接（See Also）。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
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
    // 外部 URL 不是 wiki 页面路径，直接拒绝
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("ftp://")
        || trimmed.starts_with("mailto:")
    {
        return Err("外部 URL 不是 wiki 页面路径".to_string());
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

/// 清理 index.md 中指向不存在 wiki 页面的链接行，避免 lint 持续报 MISSING_INDEX_ENTRY。
fn prune_missing_index_links(vault_path: &Path) -> Result<usize, String> {
    let index_path = vault_path.join("index.md");
    if !index_path.exists() {
        return Ok(0);
    }

    let content =
        fs::read_to_string(&index_path).map_err(|err| format!("读取 index.md 失败: {}", err))?;
    let (updated, removed) = prune_missing_index_links_from_content(vault_path, &content);
    if removed > 0 {
        fs::write(&index_path, updated).map_err(|err| format!("写入 index.md 失败: {}", err))?;
    }
    Ok(removed)
}

fn prune_missing_index_links_from_content(vault_path: &Path, content: &str) -> (String, usize) {
    let wiki_link_re = regex::Regex::new(r"\[\[([^|\]]+)(?:\|[^\]]+)?\]\]")
        .expect("wiki link regex 应可编译");
    let markdown_link_re =
        regex::Regex::new(r"\[[^\]]+\]\(([^)]+)\)").expect("markdown link regex 应可编译");
    let mut kept_lines = Vec::new();
    let mut removed = 0usize;

    for line in content.lines() {
        let mut should_remove_line = false;
        for capture in wiki_link_re.captures_iter(line) {
            let raw_target = capture.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            if raw_target.is_empty() {
                continue;
            }
            if !is_existing_wiki_page_target(vault_path, raw_target)
                && resolve_wiki_page_candidate(vault_path, raw_target).is_ok()
            {
                should_remove_line = true;
                break;
            }
        }

        if !should_remove_line {
            for capture in markdown_link_re.captures_iter(line) {
                let raw_target = capture.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                if raw_target.is_empty() {
                    continue;
                }
                if !is_existing_wiki_page_target(vault_path, raw_target)
                    && resolve_wiki_page_candidate(vault_path, raw_target).is_ok()
                {
                    should_remove_line = true;
                    break;
                }
            }
        }

        if should_remove_line {
            removed += 1;
        } else {
            kept_lines.push(line);
        }
    }

    let updated = if content.ends_with('\n') {
        format!("{}\n", kept_lines.join("\n"))
    } else {
        kept_lines.join("\n")
    };
    (updated, removed)
}

fn rewrite_broken_wiki_links_in_page(vault_path: &Path, page_path: &Path) -> Result<usize, String> {
    let content =
        fs::read_to_string(page_path).map_err(|err| format!("读取页面失败: {}", err))?;
    let (updated, replaced) = rewrite_broken_wiki_links(&content, vault_path);
    if replaced > 0 {
        fs::write(page_path, updated).map_err(|err| format!("写入页面失败: {}", err))?;
    }
    Ok(replaced)
}

fn rewrite_broken_wiki_links(content: &str, vault_path: &Path) -> (String, usize) {
    let mut updated = String::with_capacity(content.len());
    let mut offset = 0usize;
    let mut replaced = 0usize;

    while let Some(start_rel) = content[offset..].find("[[") {
        let start = offset + start_rel;
        updated.push_str(&content[offset..start]);

        let inner_start = start + 2;
        let Some(end_rel) = content[inner_start..].find("]]") else {
            updated.push_str(&content[start..]);
            offset = content.len();
            break;
        };
        let inner_end = inner_start + end_rel;
        let original = &content[start..inner_end + 2];
        let inner = &content[inner_start..inner_end];

        let mut segments = inner.splitn(2, '|');
        let raw_target = segments.next().unwrap_or("").trim();
        let raw_label = segments.next().map(str::trim).filter(|value| !value.is_empty());
        let replacement = raw_label
            .map(|value| value.to_string())
            .unwrap_or_else(|| fallback_wiki_link_label(raw_target));

        let should_replace = resolve_wiki_link_target(vault_path, raw_target)
            .map(|target_path| !Path::new(&target_path).exists())
            .unwrap_or(false);

        if should_replace {
            updated.push_str(&replacement);
            replaced += 1;
        } else {
            updated.push_str(original);
        }

        offset = inner_end + 2;
    }

    if offset < content.len() {
        updated.push_str(&content[offset..]);
    }

    if replaced == 0 {
        (content.to_string(), 0)
    } else {
        (updated, replaced)
    }
}

fn fallback_wiki_link_label(raw_target: &str) -> String {
    let normalized = raw_target
        .split('#')
        .next()
        .unwrap_or(raw_target)
        .split('^')
        .next()
        .unwrap_or(raw_target)
        .trim();
    let stem = normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized)
        .trim_end_matches(".md")
        .trim();
    if stem.is_empty() {
        "未命名链接".to_string()
    } else {
        stem.to_string()
    }
}

fn apply_missing_xref_backlinks(
    vault_path: &Path,
    source_page: &Path,
) -> Result<(usize, Vec<String>), String> {
    let source_content =
        fs::read_to_string(source_page).map_err(|err| format!("读取页面失败: {}", err))?;
    let source_link_target = wiki_link_target_from_path(vault_path, source_page)?;
    let source_title = wiki_link_label(source_page);
    let source_canonical =
        fs::canonicalize(source_page).map_err(|err| format!("解析页面路径失败: {}", err))?;
    let source_canonical_str = source_canonical.to_string_lossy().to_string();

    let mut updated = 0usize;
    let mut touched_paths = vec![source_page.to_string_lossy().to_string()];
    let mut unique_targets = BTreeSet::new();

    for raw_target in extract_wiki_link_targets(&source_content) {
        let Some(target_path) = resolve_wiki_link_target(vault_path, &raw_target) else {
            continue;
        };
        if !Path::new(&target_path).exists() {
            continue;
        }
        unique_targets.insert(target_path);
    }

    for target_path in unique_targets {
        let target_canonical = fs::canonicalize(&target_path)
            .map_err(|err| format!("解析目标页面路径失败: {}", err))?;
        if target_canonical == source_canonical {
            continue;
        }

        let target_content =
            fs::read_to_string(&target_canonical).map_err(|err| format!("读取页面失败: {}", err))?;
        let has_reverse = extract_wiki_link_targets(&target_content).iter().any(|raw| {
            resolve_wiki_link_target(vault_path, raw)
                .and_then(|path| fs::canonicalize(path).ok())
                .map(|path| path.to_string_lossy().to_string() == source_canonical_str)
                .unwrap_or(false)
        });
        if has_reverse {
            continue;
        }

        let changed =
            vault::append_see_also_link(&target_canonical, &source_link_target, &source_title)
                .map_err(|err| format!("写入反向链接失败: {}", err))?;
        if changed {
            updated += 1;
            touched_paths.push(target_canonical.to_string_lossy().to_string());
        }
    }

    touched_paths.sort();
    touched_paths.dedup();
    Ok((updated, touched_paths))
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
) -> Result<(Vec<WikiMatch>, &'static str, Option<String>, Option<QuerySearchDebug>), String> {
    if tokens.is_empty() {
        return Ok((Vec::new(), "empty", None, None));
    }

    match db::search_fts_page_paths(db_path, tokens, 64) {
        Ok(paths) if !paths.is_empty() => {
            let matches = search_wiki_matches_from_paths(&paths, tokens, question, limit)?;
            if !matches.is_empty() {
                let contributed_paths = matches
                    .iter()
                    .map(|item| item.page_path.clone())
                    .collect::<Vec<_>>();
                let debug = QuerySearchDebug {
                    strategy: "fts".to_string(),
                    rrf_k: None,
                    fused_top_paths: contributed_paths.clone(),
                    routes: vec![QuerySearchRouteDebug {
                        route: "fts".to_string(),
                        candidate_count: paths.len(),
                        top_candidates: paths
                            .iter()
                            .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                            .cloned()
                            .collect(),
                        contributed_paths,
                    }],
                };
                return Ok((matches, "fts", None, Some(debug)));
            }
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            let contributed_paths = fallback
                .iter()
                .map(|item| item.page_path.clone())
                .collect::<Vec<_>>();
            let debug = QuerySearchDebug {
                strategy: "scan".to_string(),
                rrf_k: None,
                fused_top_paths: contributed_paths.clone(),
                routes: vec![QuerySearchRouteDebug {
                    route: "scan".to_string(),
                    candidate_count: contributed_paths.len(),
                    top_candidates: contributed_paths
                        .iter()
                        .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                        .cloned()
                        .collect(),
                    contributed_paths,
                }],
            };
            Ok((fallback, "scan", None, Some(debug)))
        }
        Ok(_) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            let contributed_paths = fallback
                .iter()
                .map(|item| item.page_path.clone())
                .collect::<Vec<_>>();
            let debug = QuerySearchDebug {
                strategy: "scan".to_string(),
                rrf_k: None,
                fused_top_paths: contributed_paths.clone(),
                routes: vec![QuerySearchRouteDebug {
                    route: "scan".to_string(),
                    candidate_count: contributed_paths.len(),
                    top_candidates: contributed_paths
                        .iter()
                        .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                        .cloned()
                        .collect(),
                    contributed_paths,
                }],
            };
            Ok((fallback, "scan", None, Some(debug)))
        }
        Err(err) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            let contributed_paths = fallback
                .iter()
                .map(|item| item.page_path.clone())
                .collect::<Vec<_>>();
            let debug = QuerySearchDebug {
                strategy: "scan".to_string(),
                rrf_k: None,
                fused_top_paths: contributed_paths.clone(),
                routes: vec![QuerySearchRouteDebug {
                    route: "scan".to_string(),
                    candidate_count: contributed_paths.len(),
                    top_candidates: contributed_paths
                        .iter()
                        .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                        .cloned()
                        .collect(),
                    contributed_paths,
                }],
            };
            Ok((fallback, "scan", Some(err), Some(debug)))
        }
    }
}

/// 多路 RRF 融合检索：FTS5 + 链接扩展 + Citation 热度 + 可选扩展路径（如 embedding）。
///
/// 若所有路径均为空（如空 vault），自动降级为 `search_wiki_matches_with_fts`。
fn search_wiki_matches_rrf_with_extra_routes(
    db_path: &Path,
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
    extra_routes: &[(String, Vec<String>)],
) -> Result<(Vec<WikiMatch>, &'static str, Option<String>, Option<QuerySearchDebug>), String> {
    if tokens.is_empty() {
        return Ok((Vec::new(), "empty", None, None));
    }

    // 路径1：FTS5（多取 4x 供融合使用）
    let (fts_paths, fts_error) = match db::search_fts_page_paths(db_path, tokens, limit * 4) {
        Ok(paths) => (paths, None),
        Err(e) => (Vec::new(), Some(e)),
    };

    // 路径2：链接扩展（基于 FTS 结果做一跳扩展）
    let link_paths = if !fts_paths.is_empty() {
        db::query_linked_page_paths(db_path, &fts_paths, limit * 4).unwrap_or_default()
    } else {
        Vec::new()
    };

    // 路径3：Citation 热度
    let popular_paths = db::query_citation_popular_paths(db_path, limit * 4).unwrap_or_default();

    let mut named_routes = vec![
        ("fts".to_string(), fts_paths.clone()),
        ("linked".to_string(), link_paths),
        ("popular".to_string(), popular_paths),
    ];
    for (route_name, route_paths) in extra_routes {
        if !route_paths.is_empty() {
            named_routes.push((route_name.clone(), route_paths.clone()));
        }
    }

    let routes = named_routes
        .iter()
        .map(|(_, paths)| paths.clone())
        .collect::<Vec<_>>();

    // 如果所有路径全空，降级到原有单路逻辑
    if routes.iter().all(|route| route.is_empty()) {
        return search_wiki_matches_with_fts(db_path, wiki_dir, tokens, question, limit);
    }

    // RRF 融合
    let fused = reciprocal_rank_fusion(&routes, QUERY_RRF_K);

    // 取 top-(limit*2) 的路径，再用 search_wiki_matches_from_paths 提取摘要和评分
    let top_paths: Vec<String> = fused
        .into_iter()
        .take(limit * 2)
        .map(|(path, _)| path)
        .collect();

    if top_paths.is_empty() {
        return search_wiki_matches_with_fts(db_path, wiki_dir, tokens, question, limit);
    }

    let matches = search_wiki_matches_from_paths(&top_paths, tokens, question, limit)?;

    // 若 RRF 结果仍为空（页面文件不存在等），降级
    if matches.is_empty() {
        return search_wiki_matches_with_fts(db_path, wiki_dir, tokens, question, limit);
    }

    let matched_set = matches
        .iter()
        .map(|item| item.page_path.clone())
        .collect::<HashSet<_>>();
    let route_debug = named_routes
        .into_iter()
        .map(|(route, route_paths)| {
            let mut contributed_paths = route_paths
                .iter()
                .filter(|path| matched_set.contains(*path))
                .cloned()
                .collect::<Vec<_>>();
            contributed_paths.sort();
            contributed_paths.dedup();
            QuerySearchRouteDebug {
                route,
                candidate_count: route_paths.len(),
                top_candidates: route_paths
                    .iter()
                    .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                    .cloned()
                    .collect(),
                contributed_paths,
            }
        })
        .collect::<Vec<_>>();

    let search_debug = QuerySearchDebug {
        strategy: "rrf".to_string(),
        rrf_k: Some(QUERY_RRF_K),
        fused_top_paths: top_paths
            .iter()
            .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
            .cloned()
            .collect(),
        routes: route_debug,
    };

    Ok((matches, "rrf", fts_error, Some(search_debug)))
}

/// 三路 RRF 融合检索：FTS5 + 链接扩展 + Citation 热度。
#[allow(dead_code)]
fn search_wiki_matches_rrf(
    db_path: &Path,
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<(Vec<WikiMatch>, &'static str, Option<String>, Option<QuerySearchDebug>), String> {
    search_wiki_matches_rrf_with_extra_routes(db_path, wiki_dir, tokens, question, limit, &[])
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

/// 在 Markdown 文件内容中设置或移除 frontmatter 的 `stale` 字段。
/// 如果 stale=true，确保 frontmatter 中有 `stale: true`；
/// 如果 stale=false，移除 `stale:` 行（不写 `stale: false` 以保持简洁）。
fn set_frontmatter_stale_field(content: &str, stale: bool) -> String {
    // 定位 frontmatter 块：内容以 "---\n" 开头，找到第二个 "---"
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        // 无 frontmatter：直接返回原内容（不修改）
        return content.to_string();
    }

    let after_first = if content.starts_with("---\r\n") {
        &content[5..]
    } else {
        &content[4..]
    };

    // 找到 frontmatter 结束 "---"
    let end_pos = after_first.find("\n---")
        .or_else(|| after_first.find("\r\n---"));

    let Some(rel_end) = end_pos else {
        return content.to_string(); // 格式不对，不改
    };

    let fm_start = if content.starts_with("---\r\n") { 5 } else { 4 };
    let fm_content = &content[fm_start..fm_start + rel_end];
    let after_fm = &content[fm_start + rel_end..]; // 包含 "\n---" 或 "\r\n---" 及其后内容

    // 移除已有 stale: 行
    let cleaned: String = fm_content
        .lines()
        .filter(|line| !line.trim_start().starts_with("stale:"))
        .collect::<Vec<_>>()
        .join("\n");

    // 若 stale=true，追加 stale: true 行
    let new_fm = if stale {
        if cleaned.is_empty() {
            "stale: true".to_string()
        } else {
            format!("{}\nstale: true", cleaned)
        }
    } else {
        cleaned
    };

    format!("---\n{}\n{}", new_fm, after_fm)
}

fn parse_wiki_frontmatter(content: &str) -> Option<WikiPageFrontmatter> {
    let block = extract_frontmatter_block(content)?;
    let mut frontmatter = WikiPageFrontmatter {
        title: None,
        source: None,
        raw: None,
        imported_at: None,
        entities: Vec::new(),
        stale: None,
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
        if let Some(value) = trimmed.strip_prefix("stale:") {
            let v = value.trim().to_ascii_lowercase();
            frontmatter.stale = match v.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
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
        || frontmatter.imported_at.is_some()
        || frontmatter.stale.is_some();  // 新增
    if has_scalar_fields || !frontmatter.entities.is_empty() {
        Some(frontmatter)
    } else {
        None
    }
}

/// 读取 .md 文件的 frontmatter entities 作为标签，失败时返回空。
fn read_page_tags(path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_wiki_frontmatter(&content)
        .map(|fm| fm.entities)
        .unwrap_or_default()
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

/// 构建含历史上下文的 LLM prompt（多轮会话用）
fn build_query_prompt_with_history(
    question: &str,
    matches: &[WikiMatch],
    history: &[crate::models::AskTurn],
) -> String {
    let mut lines = vec![
        "你是一个严格本地运行的 Wiki 助手。只能依据下方本地检索结果回答，不能编造。".to_string(),
        "如果证据不足，请明确说明不确定，并给出基于页面内容的保守建议。".to_string(),
    ];

    if !history.is_empty() {
        lines.push("对话历史（供上下文参考）：".to_string());
        for turn in history {
            let prefix = if turn.role == "user" { "用户" } else { "助手" };
            lines.push(format!("{}: {}", prefix, turn.content));
        }
    }

    lines.push(format!("当前问题：{}", question));
    lines.push("本地检索结果：".to_string());

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

// ─── Deep Research Pipeline ──────────────────────────────────────────────────

/// Tavily 搜索。
async fn search_tavily(
    query: &str,
    api_key: &str,
    max_results: usize,
) -> Result<Vec<crate::models::WebSearchResult>, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "api_key": api_key,
        "query": query,
        "max_results": max_results,
        "search_depth": "advanced",
        "include_raw_content": true
    });
    let resp = client
        .post("https://api.tavily.com/search")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Tavily 请求失败: {}", e))?;
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Tavily 响应解析失败: {}", e))?;
    let results = data["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for item in results.iter().take(max_results) {
        let title = item["title"].as_str().unwrap_or("").to_string();
        let url = item["url"].as_str().unwrap_or("").to_string();
        let snippet = item["content"].as_str().unwrap_or("").to_string();
        let source = url_hostname(&url);
        out.push(crate::models::WebSearchResult {
            title,
            url,
            snippet,
            source,
        });
    }
    Ok(out)
}

/// SearXNG 搜索。
async fn search_searxng(
    query: &str,
    base_url: &str,
    max_results: usize,
) -> Result<Vec<crate::models::WebSearchResult>, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/search", base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .query(&[("q", query), ("format", "json")])
        .send()
        .await
        .map_err(|e| format!("SearXNG 请求失败: {}", e))?;
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("SearXNG 响应解析失败: {}", e))?;
    let results = data["results"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    for item in results.iter().take(max_results) {
        let title = item["title"].as_str().unwrap_or("").to_string();
        let url_str = item["url"].as_str().unwrap_or("").to_string();
        let snippet = item["content"].as_str().unwrap_or("").to_string();
        let source = url_hostname(&url_str);
        out.push(crate::models::WebSearchResult {
            title,
            url: url_str,
            snippet,
            source,
        });
    }
    Ok(out)
}

/// 从 URL 提取 hostname。
fn url_hostname(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

/// 生成 slug（小写字母数字 + 横线，最多 50 字符）。
fn make_research_slug(topic: &str) -> String {
    let raw: String = topic
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    // 去重连续横线
    let mut slug = String::new();
    let mut prev_dash = false;
    for c in raw.chars() {
        if c == '-' {
            if !prev_dash {
                slug.push(c);
            }
            prev_dash = true;
        } else {
            slug.push(c);
            prev_dash = false;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    slug.chars().take(50).collect()
}

/// 去除 <think>/<thinking> 标签及其内容。
fn strip_think_tags(text: &str) -> String {
    let mut result = text.to_string();
    for tag in &["think", "thinking"] {
        let open = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        while let Some(start) = result.find(&open) {
            if let Some(rel_end) = result[start..].find(&close) {
                let end = start + rel_end + close.len();
                result.replace_range(start..end, "");
            } else {
                result.replace_range(start.., "");
                break;
            }
        }
    }
    result.trim().to_string()
}

/// 执行搜索（按 provider 路由）。
async fn do_search(
    query: &str,
    config: &crate::models::SearchConfig,
    max_results: usize,
) -> Vec<crate::models::WebSearchResult> {
    match config.search_provider.as_str() {
        "tavily" if !config.tavily_api_key.is_empty() => {
            search_tavily(query, &config.tavily_api_key, max_results)
                .await
                .unwrap_or_default()
        }
        "searxng" => {
            search_searxng(query, &config.searxng_url, max_results)
                .await
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// Deep Research 主管线（后台 task）。
async fn start_research_task(
    app_handle: tauri::AppHandle,
    task_id: i64,
    topic: String,
    config: crate::models::SearchConfig,
) {
    let state = app_handle.state::<AppState>();
    let db_path_opt = state.outbox_db_path();
    let db_path = match db_path_opt {
        Some(p) => p,
        None => {
            let _ = app_handle.emit("research_error", serde_json::json!({ "task_id": task_id, "error": "Vault 未初始化" }));
            return;
        }
    };

    let fail = |msg: String| {
        let conn_r = rusqlite::Connection::open(&db_path);
        if let Ok(conn) = conn_r {
            let now = current_timestamp_ms();
            let _ = db::db_update_research_task(&conn, task_id, "failed", "[]", 0, None, Some(msg.as_str()), &now);
        }
        let _ = app_handle.emit("research_error", serde_json::json!({ "task_id": task_id, "error": msg }));
    };

    let report_step = |step: &str, msg: String| {
        let _ = app_handle.emit("research_progress", serde_json::json!({ "task_id": task_id, "stage": step, "message": msg }));
    };

    // 获取 LLM
    let provider = match state.get_llm_provider() {
        Some(p) => p,
        None => { fail("LLM Provider 不可用".to_string()); return; }
    };

    let mut all_results: Vec<crate::models::WebSearchResult> = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut current_sub_queries: Vec<String> = Vec::new();
    let mut research_context = String::new(); // 累积的研究上下文

    // ── Phase 1: Initial Decomposition ────────────────────────────────────────
    report_step("decomposing", "正在规划研究路径...".to_string());
    let decompose_prompt = format!(
        "你是一个资深研究员。请针对主题「{}」生成 {} 个深入研究的具体搜索查询。一行一条，不要编号，不要废话。",
        topic, config.breadth
    );
    if let Ok(text) = provider.complete(&decompose_prompt).await {
        current_sub_queries = text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).take(config.breadth as usize).collect();
    }
    if current_sub_queries.is_empty() { current_sub_queries.push(topic.clone()); }

    // ── Iterative Research Loop ──────────────────────────────────────────────
    let max_depth = config.depth.clamp(1, 5);
    for current_depth in 1..=max_depth {
        report_step("searching", format!("第 {}/{} 轮：正在执行搜索...", current_depth, max_depth));
        
        let mut new_round_results = Vec::new();
        for query in &current_sub_queries {
            report_step("searching", format!("正在搜索：{}", query));
            let results = do_search(query, &config, config.breadth as usize).await;
            for r in results {
                if seen_urls.insert(r.url.clone()) {
                    new_round_results.push(r);
                }
            }
        }

        if new_round_results.is_empty() && current_depth > 1 {
            report_step("searching", "本轮未发现新资料，提前结束搜索。".to_string());
            break;
        }

        // 提取本轮事实（如果 Tavily 给了 Raw Content，优先使用）
        let round_facts = new_round_results.iter().map(|r| {
            format!("来源: {} ({})\n内容: {}", r.title, r.source, r.snippet)
        }).collect::<Vec<_>>().join("\n\n");
        
        research_context.push_str(&format!("\n\n### 第 {} 轮搜索事实\n{}", current_depth, round_facts));
        all_results.extend(new_round_results);

        // 如果还没到最后一轮，评估下一步
        if current_depth < max_depth {
            report_step("synthesizing", format!("正在评估第 {} 轮结果并寻找知识缺口...", current_depth));
            let gap_prompt = format!(
                "已知研究背景：\n{}\n\n当前已收集事实：\n{}\n\n请分析以上内容，指出 2-3 个仍需进一步挖掘的深层问题或知识盲点。一行一条，仅输出搜索查询。",
                topic, research_context
            );
            if let Ok(gap_text) = provider.complete(&gap_prompt).await {
                current_sub_queries = gap_text.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).take(3).collect();
                if current_sub_queries.is_empty() { break; }
            } else {
                break;
            }
        }
    }

    // ── Step 3: Final Synthesis ─────────────────────────────────────────────
    report_step("synthesizing", "正在撰写最终研究报告...".to_string());
    let wiki_index = {
        let guard = state.inner.lock().expect("状态锁");
        guard.vault_path.as_ref().and_then(|vp| fs::read_to_string(vp.join("wiki").join("index.md")).ok()).unwrap_or_default()
    };
    let wiki_index_excerpt: String = wiki_index.chars().take(1000).collect();

    let synth_prompt = format!(
        "你是一个专业的研究助理。请将以下深度研究收集的所有资料综合成一篇高质量的 Markdown Wiki 页面。\n\n## 研究主题\n{}\n\n## 收集的事实\n{}\n\n## 现有知识库概览\n{}\n\n要求：\n1. 结构严谨，包含摘要、核心发现、详细分析和结论。\n2. 在正文中使用 [N] 标注来源。\n3. 如果可以，请在结尾处加入针对现有知识库的交叉引用建议。\n4. 使用中文撰写，不要输出思考过程。",
        topic, research_context, wiki_index_excerpt
    );

    let synthesized = match provider.complete(&synth_prompt).await {
        Ok(t) => t,
        Err(e) => { fail(format!("最终综合失败: {:?}", e)); return; }
    };

    // ── Step 4: Saving ───────────────────────────────────────────────────────
    report_step("saving", "正在保存到知识库...".to_string());
    let cleaned = strip_think_tags(&synthesized);
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    
    let references = all_results.iter().enumerate().map(|(i, r)| {
        format!("{}. [{}]({})", i + 1, r.title, r.url)
    }).collect::<Vec<_>>().join("\n");

    let final_content = format!(
        "---\ntype: research\ntitle: \"深度研究：{}\"\ncreated: {}\nupdated: {}\ndepth: {}\nbreadth: {}\ntags: [research, deep-research]\n---\n\n{}\n\n## 参考文献\n{}",
        topic, date_str, date_str, config.depth, config.breadth, cleaned, references
    );

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁");
        guard.vault_path.clone()
    };
    let vault_path = match vault_path {
        Some(p) => p,
        None => { fail("保存阶段：Vault 路径丢失".to_string()); return; }
    };

    let slug = make_research_slug(&topic);
    let filename = format!("research-{}-{}.md", slug, date_str);
    let save_dir = vault_path.join("wiki").join("research");
    let _ = fs::create_dir_all(&save_dir);
    let save_path = save_dir.join(&filename);

    if let Err(e) = fs::write(&save_path, &final_content) {
        fail(format!("写入文件失败: {}", e));
        return;
    }

    let saved_path_str = save_path.to_string_lossy().to_string();
    {
        let conn = match rusqlite::Connection::open(&db_path) {
            Ok(c) => c,
            Err(e) => { fail(format!("打开数据库失败: {}", e)); return; }
        };
        let now = current_timestamp_ms();
        // 存储本轮实际使用的子查询（current_sub_queries 在最后一轮结束后保持为最后一组）
        let sub_queries_json = serde_json::to_string(&current_sub_queries).unwrap_or_default();
        let _ = db::db_update_research_task(&conn, task_id, "done", &sub_queries_json, all_results.len() as i32, Some(saved_path_str.as_str()), None, &now);
    }

    // 触发后续管线
    let _ = state.ingest_markdown(save_path).await;
    let _ = app_handle.emit("research_done", serde_json::json!({ "task_id": task_id, "saved_path": saved_path_str }));
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

    #[test]
    fn get_knowledge_graph_returns_err_when_no_vault() {
        // 使用一个不存在的配置文件路径，确保 vault_path 为 None
        let tmp = std::env::temp_dir().join(format!(
            "llm-wiki-kg-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let config_path = tmp.join("app-config.json");
        let state = AppState::new_with_path(config_path);
        // vault 未初始化，应返回 Err
        let result = state.get_knowledge_graph_impl();
        assert!(result.is_err());
    }

    #[test]
    fn get_knowledge_subgraph_returns_err_when_no_vault() {
        let tmp = std::env::temp_dir().join(format!(
            "llm-wiki-subgraph-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let config_path = tmp.join("app-config.json");
        let state = AppState::new_with_path(config_path);
        let result = state.get_knowledge_subgraph_impl(
            "wiki/a.md".to_string(),
            1,
            KnowledgeGraphDirection::Both,
            10,
            10,
        );
        assert!(result.is_err());
    }

    #[test]
    fn get_knowledge_subgraph_respects_direction_and_hop() {
        let vault_dir = make_temp_dir("llm-wiki-subgraph-direction");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let wiki_dir = vault_dir.join("wiki");
        let page_a = wiki_dir.join("a.md");
        let page_b = wiki_dir.join("b.md");
        let page_c = wiki_dir.join("c.md");
        let page_d = wiki_dir.join("d.md");
        fs::write(&page_a, "# A").expect("写入 A 页面失败");
        fs::write(&page_b, "# B").expect("写入 B 页面失败");
        fs::write(&page_c, "# C").expect("写入 C 页面失败");
        fs::write(&page_d, "# D").expect("写入 D 页面失败");

        let page_a = fs::canonicalize(&page_a).expect("规范化 A 页面失败");
        let page_b = fs::canonicalize(&page_b).expect("规范化 B 页面失败");
        let page_c = fs::canonicalize(&page_c).expect("规范化 C 页面失败");
        let page_d = fs::canonicalize(&page_d).expect("规范化 D 页面失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        for (idx, (title, path)) in [
            ("A", &page_a),
            ("B", &page_b),
            ("C", &page_c),
            ("D", &page_d),
        ]
        .into_iter()
        .enumerate()
        {
            conn.execute(
                "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    format!("test-hash-{}", idx),
                    format!("source://{}", idx),
                    path.to_string_lossy().to_string(),
                    "1"
                ],
            )
            .expect("写入 sources 失败");
            let source_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    source_id,
                    title,
                    path.to_string_lossy().to_string(),
                    format!("summary {}", title),
                    "1",
                    "1"
                ],
            )
            .expect("写入 wiki_pages 失败");
        }

        // A -> B -> C，且 D -> B
        for (source, target) in [(&page_a, &page_b), (&page_b, &page_c), (&page_d, &page_b)] {
            conn.execute(
                "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    source.to_string_lossy().to_string(),
                    target.to_string_lossy().to_string(),
                    1_i64,
                    "edge",
                    "1"
                ],
            )
            .expect("写入 citations 失败");
        }

        let out_graph = state
            .get_knowledge_subgraph_impl(
                page_b.to_string_lossy().to_string(),
                1,
                KnowledgeGraphDirection::Out,
                50,
                50,
            )
            .expect("查询 out 子图失败");
        let out_nodes = out_graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            out_nodes,
            BTreeSet::from([
                page_b.to_string_lossy().to_string(),
                page_c.to_string_lossy().to_string(),
            ])
        );

        let in_graph = state
            .get_knowledge_subgraph_impl(
                page_b.to_string_lossy().to_string(),
                1,
                KnowledgeGraphDirection::In,
                50,
                50,
            )
            .expect("查询 in 子图失败");
        let in_nodes = in_graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            in_nodes,
            BTreeSet::from([
                page_a.to_string_lossy().to_string(),
                page_b.to_string_lossy().to_string(),
                page_d.to_string_lossy().to_string(),
            ])
        );

        let both_graph = state
            .get_knowledge_subgraph_impl(
                page_b.to_string_lossy().to_string(),
                2,
                KnowledgeGraphDirection::Both,
                50,
                50,
            )
            .expect("查询 both 子图失败");
        let both_nodes = both_graph
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            both_nodes,
            BTreeSet::from([
                page_a.to_string_lossy().to_string(),
                page_b.to_string_lossy().to_string(),
                page_c.to_string_lossy().to_string(),
                page_d.to_string_lossy().to_string(),
            ])
        );
    }

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

        async fn embed(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
            Ok(vec![])
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
    async fn ingest_file_impl_rejects_unsupported_extension() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-file-unsupported");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let unsupported = vault_dir.join("sample.xyz");
        fs::write(&unsupported, "unsupported").expect("写入测试文件失败");

        let err = state
            .ingest_file_impl(unsupported.to_string_lossy().as_ref(), None)
            .await
            .expect_err("不支持扩展名应返回错误");
        assert!(err.contains("不支持的文件扩展名"));
        assert!(err.contains("md/markdown/pdf/docx/pptx/txt"));
    }

    #[test]
    fn normalize_ocr_provider_falls_back_to_tesseract_on_invalid_value() {
        assert_eq!(normalize_ocr_provider(None), OcrProvider::Tesseract);
        assert_eq!(normalize_ocr_provider(Some("")), OcrProvider::Tesseract);
        assert_eq!(
            normalize_ocr_provider(Some("invalid-provider")),
            OcrProvider::Tesseract
        );
        assert_eq!(normalize_ocr_provider(Some("paddle")), OcrProvider::Paddle);
        assert_eq!(
            normalize_ocr_provider(Some(" TESSERACT ")),
            OcrProvider::Tesseract
        );
    }

    #[test]
    fn resolve_ocr_provider_order_matches_expected_fallback_sequence() {
        assert_eq!(
            resolve_ocr_provider_order(OcrProvider::Tesseract),
            [OcrProvider::Tesseract, OcrProvider::Paddle]
        );
        assert_eq!(
            resolve_ocr_provider_order(OcrProvider::Paddle),
            [OcrProvider::Paddle, OcrProvider::Tesseract]
        );
    }

    #[test]
    fn extract_xml_text_by_tag_reads_docx_minimal_sample() {
        let xml = r#"
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>你好&amp;Rust</w:t></w:r></w:p>
    <w:p><w:r><w:t xml:space="preserve">  Wiki  </w:t></w:r></w:p>
  </w:body>
</w:document>
"#;

        let text = extract_xml_text_by_tag(xml, "w:t");
        assert!(text.contains("你好&Rust"));
        assert!(text.contains("Wiki"));
    }

    #[test]
    fn extract_xml_text_by_tag_reads_pptx_minimal_sample() {
        let xml = r#"
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld>
    <a:t>Hello</a:t>
    <a:t>Slide&#32;1</a:t>
  </p:cSld>
</p:sld>
"#;

        let text = extract_xml_text_by_tag(xml, "a:t");
        assert!(text.contains("Hello"));
        assert!(text.contains("Slide 1"));
    }

    #[test]
    fn format_tesseract_spawn_error_returns_readable_message_when_missing() {
        let error = io::Error::new(io::ErrorKind::NotFound, "not found");
        let message = format_tesseract_spawn_error(&error);
        assert!(message.contains("未检测到 tesseract"));
        assert!(message.contains("PATH"));
    }

    #[test]
    fn extract_text_from_pdf_operations_extracts_simple_text() {
        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new(
                    "Tj",
                    vec![lopdf::Object::String(
                        b"Hello PDF".to_vec(),
                        lopdf::StringFormat::Literal,
                    )],
                ),
                lopdf::content::Operation::new(
                    "TJ",
                    vec![lopdf::Object::Array(vec![
                        lopdf::Object::String(b"Fallback ".to_vec(), lopdf::StringFormat::Literal),
                        lopdf::Object::Integer(-120),
                        lopdf::Object::String(b"Works".to_vec(), lopdf::StringFormat::Literal),
                    ])],
                ),
                lopdf::content::Operation::new("ET", vec![]),
            ],
        };
        let encoded = content.encode().expect("编码 PDF 操作失败");
        let decoded = lopdf::content::Content::decode(&encoded).expect("解码 PDF 操作失败");

        let extracted = extract_text_from_pdf_operations(&decoded.operations);
        assert!(extracted.contains("Hello PDF"));
        assert!(extracted.contains("Fallback Works"));
    }

    #[test]
    fn extract_text_from_pdf_raw_streams_extracts_text_from_flate_stream() {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new(
                    "Tj",
                    vec![lopdf::Object::String(
                        b"Gradient Tensor".to_vec(),
                        lopdf::StringFormat::Literal,
                    )],
                ),
                lopdf::content::Operation::new("ET", vec![]),
            ],
        };
        let encoded = content.encode().expect("编码内容流失败");
        let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
        compressor
            .write_all(&encoded)
            .expect("压缩内容流失败");
        let compressed = compressor.finish().expect("完成压缩失败");

        let mut pseudo_pdf = Vec::new();
        pseudo_pdf.extend_from_slice(b"%PDF-1.4\n1 0 obj\n<< /Length ");
        pseudo_pdf.extend_from_slice(compressed.len().to_string().as_bytes());
        pseudo_pdf.extend_from_slice(b" /Filter /FlateDecode >>\nstream\n");
        pseudo_pdf.extend_from_slice(&compressed);
        pseudo_pdf.extend_from_slice(b"\nendstream\n%%EOF");

        let extracted = extract_text_from_pdf_raw_streams(&pseudo_pdf)
            .expect("应能从 Flate stream 提取文本");
        assert!(extracted.contains("Gradient Tensor"));
    }

    #[test]
    fn decode_pdf_stream_candidates_supports_trailing_newline() {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        let payload = b"BT\n(Hello Stream)\nTj\nET";
        let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
        compressor.write_all(payload).expect("压缩 payload 失败");
        let mut compressed = compressor.finish().expect("完成压缩失败");
        compressed.extend_from_slice(b"\r\n");

        let candidates = decode_pdf_stream_candidates(&compressed);
        assert!(candidates.iter().any(|candidate| {
            let text = String::from_utf8_lossy(candidate);
            text.contains("Hello Stream")
        }));
    }

    #[test]
    fn should_fallback_to_pdf_ocr_matches_supported_error_patterns() {
        assert!(should_fallback_to_pdf_ocr(
            "提取 PDF 文本失败：未识别到可用文本，可能是扫描件或字体编码不兼容"
        ));
        assert!(should_fallback_to_pdf_ocr(
            "读取 PDF 失败：当前解析器暂不兼容该文件结构"
        ));
        assert!(!should_fallback_to_pdf_ocr("读取 PDF 原始字节失败：permission denied"));
    }

    #[test]
    fn build_pdf_ocr_fallback_failure_message_contains_install_hints() {
        let message = build_pdf_ocr_fallback_failure_message(
            "读取 PDF 失败：当前解析器暂不兼容该文件结构",
            "未检测到 pdftoppm 命令",
        );
        assert!(message.contains("自动 OCR 回退失败"));
        assert!(message.contains("Poppler"));
        assert!(message.contains("pdftoppm"));
        assert!(message.contains("tesseract"));
        assert!(message.contains("paddleocr"));
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
        assert!(err.contains("解析器暂不兼容"));
    }

    #[test]
    fn find_subsequence_returns_expected_offsets() {
        let bytes = b"abc%PDF-1.4...%%EOFtail";
        assert_eq!(find_subsequence(bytes, b"%PDF-"), Some(3));
        assert_eq!(rfind_subsequence(bytes, b"%%EOF"), Some(14));
        assert_eq!(find_subsequence(bytes, b"not-found"), None);
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
    fn prune_missing_index_links_from_content_removes_only_missing_targets() {
        let vault_dir = make_temp_dir("llm-wiki-prune-index-content");
        let _guard = TempDirGuard(vault_dir.clone());
        fs::create_dir_all(vault_dir.join("wiki")).expect("创建 wiki 目录失败");
        fs::write(vault_dir.join("wiki").join("kept.md"), "# kept").expect("写入 kept 页面失败");

        let content = [
            "# Index",
            "- [[wiki/kept.md|kept]]",
            "- [[wiki/missing.md|missing]]",
            "- [missing-md](wiki/missing2.md)",
            "- [external](https://example.com)",
        ]
        .join("\n");

        let (updated, removed) = prune_missing_index_links_from_content(&vault_dir, &content);
        assert_eq!(removed, 2);
        assert!(updated.contains("[[wiki/kept.md|kept]]"));
        assert!(!updated.contains("[[wiki/missing.md|missing]]"));
        assert!(!updated.contains("wiki/missing2.md"));
        assert!(updated.contains("https://example.com"));
    }

    #[test]
    fn prune_missing_index_links_updates_file_and_returns_removed_count() {
        let vault_dir = make_temp_dir("llm-wiki-prune-index-file");
        let _guard = TempDirGuard(vault_dir.clone());
        fs::create_dir_all(vault_dir.join("wiki")).expect("创建 wiki 目录失败");
        fs::write(vault_dir.join("wiki").join("exists.md"), "# exists").expect("写入页面失败");
        fs::write(
            vault_dir.join("index.md"),
            "- [[wiki/exists.md|exists]]\n- [[wiki/gone.md|gone]]\n",
        )
        .expect("写入 index 失败");

        let removed = prune_missing_index_links(&vault_dir).expect("清理 index 链接失败");
        assert_eq!(removed, 1);

        let updated = fs::read_to_string(vault_dir.join("index.md")).expect("读取 index 失败");
        assert!(updated.contains("[[wiki/exists.md|exists]]"));
        assert!(!updated.contains("[[wiki/gone.md|gone]]"));
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
                .generate_query_answer_with_provider(
                    "核心目标是什么",
                    &matches,
                    Some(provider),
                    None,
                )
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
                .generate_query_answer_with_provider(
                    "需要回退吗",
                    &matches,
                    Some(provider),
                    None,
                )
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
                ollama_model: "".to_string(),
                ollama_base_url: "".to_string(),
                embed_ollama_model: "".to_string(),
                embed_ollama_base_url: "".to_string(),
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
        let (matches, strategy, fts_error, search_debug) =
            search_wiki_matches_with_fts(&db_path, &wiki_dir, &tokens, "Rust backend", 3)
                .expect("执行检索失败");

        assert!(fts_error.is_none());
        assert_eq!(strategy, "fts");
        assert!(search_debug.is_some());
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
        assert_eq!(result.search_strategy, "rrf");
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
        assert_eq!(result.search_strategy, "rrf");
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
        assert_eq!(result.search_strategy, "rrf");
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
        assert!(codes.contains("orphan"));
        assert!(codes.contains("DB_MISSING_PAGE_RECORD"));
        assert!(codes.contains("BROKEN_CITATION"));
        assert!(!codes.contains("VAULT_NOT_INITIALIZED"));
        assert_eq!(report.severity_stats.error, 1);
        assert_eq!(report.severity_stats.warning, 3);
        assert_eq!(report.severity_stats.info, 0);
    }

    #[test]
    fn lint_report_detects_wikilink_level_broken_orphan_and_xref_missing() {
        let vault_dir = make_temp_dir("llm-wiki-lint-wikilink-level");
        let _guard = TempDirGuard(vault_dir.clone());

        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_a = vault_dir.join("wiki").join("a.md");
        let page_b = vault_dir.join("wiki").join("b.md");
        let page_orphan = vault_dir.join("wiki").join("orphan.md");

        fs::write(
            &page_a,
            "# A\n\n[[wiki/missing.md|missing]]\n[[wiki/b.md|B]]\n",
        )
        .expect("写入 a.md 失败");
        fs::write(&page_b, "# B\n\n页面 B 内容。\n").expect("写入 b.md 失败");
        fs::write(&page_orphan, "# Orphan\n\n孤页内容。\n").expect("写入 orphan.md 失败");

        fs::write(
            vault_dir.join("index.md"),
            "# Index\n\n## Imported Pages\n- [[wiki/a.md|a]]\n- [[wiki/b.md|b]]\n",
        )
        .expect("写入 index.md 失败");

        let report = state.lint_report();
        let codes: BTreeSet<_> = report.issues.iter().map(|issue| issue.code.as_str()).collect();

        assert!(codes.contains("broken_wikilink"));
        assert!(codes.contains("xref_missing"));
        assert!(codes.contains("orphan"));
    }

    #[test]
    fn apply_lint_patch_supports_broken_wikilink_and_xref_missing() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-wikilink-level");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_a = vault_dir.join("wiki").join("a.md");
        let page_b = vault_dir.join("wiki").join("b.md");
        fs::write(
            &page_a,
            "# A\n\n[[wiki/missing.md|缺失页]]\n[[wiki/b.md|B]]\n",
        )
        .expect("写入 a.md 失败");
        fs::write(&page_b, "# B\n\n页面 B 内容。\n").expect("写入 b.md 失败");

        let broken_result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "broken_wikilink".to_string(),
                path: Some(page_a.to_string_lossy().to_string()),
            })
            .expect("应用 broken_wikilink 补丁失败");
        assert!(broken_result.applied);

        let page_a_content = fs::read_to_string(&page_a).expect("读取 a.md 失败");
        assert!(!page_a_content.contains("[[wiki/missing.md|缺失页]]"));
        assert!(page_a_content.contains("缺失页"));

        let xref_result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "xref_missing".to_string(),
                path: Some(page_a.to_string_lossy().to_string()),
            })
            .expect("应用 xref_missing 补丁失败");
        assert!(xref_result.applied);

        let page_b_content = fs::read_to_string(&page_b).expect("读取 b.md 失败");
        assert!(page_b_content.contains("[[wiki/a.md|a]]"));
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
        let state = AppState {
            inner: Mutex::new(AppStateData {
                mode: AppMode::Hybrid,
                vault_path: None,
                query_top_k: QUERY_TOP_K_DEFAULT,
                logs: Vec::new(),
                next_log_id: 1,
                config_snapshot: None,
                cloud_api_key: None,
                cloud_base_url: None,
                cloud_model: None,
                cloud_provider_name: None,
                active_provider: None,
                default_ocr_provider: None,
                ollama_model: None,
                ollama_base_url: None,
                embed_ollama_model: None,
                embed_ollama_base_url: None,
            }),
            config_path: vault_dir.join(".runtime").join("app-config.json"),
            llm_provider: OnceLock::new(),
            app_handle: OnceLock::new(),
            ask_sessions: Mutex::new(std::collections::HashMap::new()),
            ask_cancel_flags: Mutex::new(std::collections::HashMap::new()),
            search_config: Mutex::new(crate::models::SearchConfig::default()),
        };
        // 注入已有的 MockQueryProvider
        let _ = state.llm_provider.set(Arc::new(MockQueryProvider::new(
            "Mock Answer",
            Arc::new(Mutex::new(Vec::new())),
        )));
        state
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

    #[test]
    fn test_extract_slide_number_natural_sort() {
        assert_eq!(extract_slide_number("ppt/slides/slide1.xml"), 1);
        assert_eq!(extract_slide_number("ppt/slides/slide10.xml"), 10);
        assert_eq!(extract_slide_number("ppt/slides/slide2.xml"), 2);
        // 确保 slide10 > slide2（自然数顺序）
        assert!(
            extract_slide_number("ppt/slides/slide10.xml")
                > extract_slide_number("ppt/slides/slide2.xml")
        );
    }

    #[test]
    fn test_extract_docx_paragraphs_preserves_paragraph_breaks() {
        let xml = r#"<w:body>
        <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
        <w:p><w:r><w:t>第二段</w:t></w:r><w:r><w:t>续文</w:t></w:r></w:p>
        <w:p></w:p>
    </w:body>"#;
        let result = extract_docx_paragraphs(xml);
        assert!(result.contains("第一段"));
        assert!(result.contains("第二段"));
        // 两段之间应有段落分隔
        assert!(result.contains("\n\n") || result.lines().count() >= 2);
    }

    /// 验证 WikiPageDetail.content 字段可被 serde_json 正确序列化/反序列化（round-trip）
    #[test]
    fn test_wiki_page_detail_content_field_roundtrip() {
        let detail = crate::models::WikiPageDetail {
            title: "测试页面".to_string(),
            path: "vault/test.md".to_string(),
            display_path: "test.md".to_string(),
            frontmatter: None,
            content: "# Hello\n\nWorld".to_string(),
            updated_at: "1000000".to_string(),
        };
        let json = serde_json::to_string(&detail).unwrap();
        // 确认原始内容已被序列化
        assert!(json.contains("Hello"));
        let restored: crate::models::WikiPageDetail = serde_json::from_str(&json).unwrap();
        // 确认反序列化后内容与原始一致
        assert_eq!(restored.content, "# Hello\n\nWorld");
    }

    #[test]
    fn clear_ask_session_removes_history() {
        let state = AppState::new();
        {
            let mut sessions = state.ask_sessions.lock().unwrap();
            sessions.insert("sess1".to_string(), vec![
                crate::models::AskTurn { role: "user".to_string(), content: "hi".to_string() }
            ]);
        }
        state.clear_ask_session("sess1".to_string()).unwrap();
        let sessions = state.ask_sessions.lock().unwrap();
        assert!(sessions.get("sess1").map(|v| v.is_empty()).unwrap_or(true));
    }

    #[test]
    fn cancel_ask_session_noop_when_no_flag() {
        let state = AppState::new();
        // 无 flag 时不 panic，返回 Ok
        let result = state.cancel_ask_session("nonexistent".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn search_wiki_matches_rrf_degrades_gracefully_on_empty_vault() {
        let tmp = make_temp_dir("llm-wiki-rrf-degrade");
        let _guard = TempDirGuard(tmp.clone());
        let db_path = tmp.join("meta.db");
        let wiki_dir = tmp.join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();
        let tokens = vec!["test".to_string()];
        // 应该不 panic，返回空结果
        let result = search_wiki_matches_rrf(&db_path, &wiki_dir, &tokens, "test", 3);
        assert!(result.is_ok());
        let (matches, _, _, _) = result.unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn search_wiki_matches_rrf_accepts_embedding_extra_route() {
        let tmp = make_temp_dir("llm-wiki-rrf-extra-route");
        let _guard = TempDirGuard(tmp.clone());
        let db_path = tmp.join("meta.db");
        let wiki_dir = tmp.join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let page_path = wiki_dir.join("embedding-only.md");
        fs::write(&page_path, "# Embedding\n\nRust embedding recall path").unwrap();

        let tokens = vec!["rust".to_string()];
        let extra_routes = vec![(
            "embedding".to_string(),
            vec![page_path.to_string_lossy().to_string()],
        )];
        let result = search_wiki_matches_rrf_with_extra_routes(
            &db_path,
            &wiki_dir,
            &tokens,
            "rust",
            3,
            &extra_routes,
        )
        .expect("执行含 embedding 扩展路径的 RRF 失败");

        assert_eq!(result.1, "rrf");
        assert_eq!(result.0.len(), 1);
        assert!(result.0[0].page_path.ends_with("embedding-only.md"));
        let debug = result.3.expect("应返回检索调试信息");
        assert!(debug.routes.iter().any(|item| item.route == "embedding"));
    }

    #[test]
    fn set_frontmatter_stale_field_adds_stale_true() {
        let content = "---\ntitle: 'Test'\nimported_at: '2026-01-01'\n---\n# Body\n";
        let result = set_frontmatter_stale_field(content, true);
        assert!(result.contains("stale: true"), "应包含 stale: true");
        assert!(result.contains("title:"), "不应丢失 title");
    }

    #[test]
    fn set_frontmatter_stale_field_removes_stale_on_false() {
        let content = "---\ntitle: 'Test'\nstale: true\n---\n# Body\n";
        let result = set_frontmatter_stale_field(content, false);
        assert!(!result.contains("stale:"), "应移除 stale 字段");
        assert!(result.contains("title:"), "不应丢失 title");
    }

    #[test]
    fn get_page_embedding_similarities_returns_high_sim_pairs() {
        let dir = make_temp_dir("llm-wiki-emb-sim");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        state.init_vault(dir.clone()).unwrap();

        let db_path = dir.join(".app").join("meta.db");
        db::upsert_embedding(&db_path, "wiki/a.md", &[1.0_f32, 0.0, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/b.md", &[1.0_f32, 0.0, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/c.md", &[0.0_f32, 1.0, 0.0]).unwrap();

        let paths = vec!["wiki/a.md".to_string(), "wiki/b.md".to_string(), "wiki/c.md".to_string()];
        let result = state.get_page_embedding_similarities(paths).unwrap();

        assert!(result.contains_key("wiki/a.md||wiki/b.md"), "a-b 对应包含");
        let sim = result["wiki/a.md||wiki/b.md"];
        assert!((sim - 1.0).abs() < 1e-6, "a-b 相似度应为 1.0，实际: {}", sim);
        assert!(!result.contains_key("wiki/a.md||wiki/c.md"), "a-c 直交不应包含");
        assert!(!result.contains_key("wiki/b.md||wiki/c.md"), "b-c 直交不应包含");
    }

    #[test]
    fn get_page_embedding_similarities_filters_to_requested_paths() {
        let dir = make_temp_dir("llm-wiki-emb-sim-filter");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        state.init_vault(dir.clone()).unwrap();

        let db_path = dir.join(".app").join("meta.db");
        db::upsert_embedding(&db_path, "wiki/a.md", &[1.0_f32, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/b.md", &[1.0_f32, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/c.md", &[1.0_f32, 0.0]).unwrap();

        // 只请求 a 和 b，c 不在路径列表中
        let paths = vec!["wiki/a.md".to_string(), "wiki/b.md".to_string()];
        let result = state.get_page_embedding_similarities(paths).unwrap();
        assert!(result.contains_key("wiki/a.md||wiki/b.md"));
        assert!(!result.keys().any(|k| k.contains("wiki/c.md")));
    }
}
