use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    db,
    llm::{EmbedProvider, LlmProvider, OllamaConfig, OllamaProvider, OpenAiConfig, OpenAiProvider, DEFAULT_OPENAI_MODEL},
    models::{
        AgentDraftItem, AgentMemoryItem, AgentRunEventItem, AgentRunItem, AgentSkillItem,
        AppConfig, AppMode, AppOverview, AskSessionItem, AskSessionSearchHitItem,
        AskSessionTurnItem, DefaultPaths, IngestPreview, IngestResult,
        KnowledgeGraphData, KnowledgeGraphDirection,
        KnowledgeSubgraphData, LintPatchApplyInput,
        LintPatchApplyResult, LintPatchBatchApplyResult,
        LintPatchEventItem, LintPatchPreview,
        LintReport, LlmProviderConfig, LlmStatus, LogEntry, LogLevel,
        ModeChangeResult, NewPageResult, OutboxAckResult, OutboxEventItem, ProgressPayload,
        QueryAnswerResult, QueryAskOptions,
        QuerySettings, SaveQueryAnswerInput, SaveQueryAnswerResult, ShellAuditEvent,
        ShellPolicyConfig, ShellResult, VaultInitResult, WikiPageCitationItem, WikiPageDetail,
        WikiPageHistoryDetail, WikiPageHistoryItem, WikiPageItem,
    },
};

// 服务模块（H16 拆分）
pub mod config_service;
pub mod shell_service;
pub mod search_service;
pub mod wiki_service;
pub mod graph_service;
pub mod lint_service;
pub mod ingest_service;
pub mod ask_service;
pub mod agent_service;
pub mod chat_service;
pub mod research_service;
pub use research_service::strip_think_tags;

#[cfg(test)]
mod test_helpers;

/// Noop Embedder：embed_backend = "disabled" 或 ONNX 初始化失败时使用。
/// 所有 embed() 调用返回 EmbedError::Unavailable，应用层已有降级（仅 FTS5）。
struct NoopEmbedder;

#[async_trait::async_trait]
impl EmbedProvider for NoopEmbedder {
    async fn embed(&self, _text: &str) -> Result<Vec<f32>, crate::llm::EmbedError> {
        Err(crate::llm::EmbedError::Unavailable)
    }
    fn dimension(&self) -> usize { 0 }
    fn backend_id(&self) -> String { "noop".to_string() }
    async fn health_check(&self) -> bool { false }
}

const STALE_PENDING_TASK_THRESHOLD_MS: u128 = 24 * 60 * 60 * 1000;
const QUERY_TOP_K_MIN: usize = 1;
const QUERY_TOP_K_MAX: usize = 8;
const QUERY_TOP_K_DEFAULT: usize = 3;
const QUERY_EMBED_ROUTE_MAX_CANDIDATES: usize = 5000;
const QUERY_RRF_K: f64 = 60.0;
const QUERY_ROUTE_DEBUG_TOP_CANDIDATES: usize = 5;

/// 默认摘要最大 token 数量
const LLM_SUMMARY_MAX_TOKENS: usize = 200;
/// AAAK-lite：记忆值总字符数超过此阈值时触发压缩。
const MEMORY_COMPRESS_THRESHOLD_CHARS: usize = 2000;
/// 草稿生成时注入的最大记忆条数（run + global 各自上限）。
const MEMORY_INJECT_LIMIT: usize = 20;

/// 进程内状态。
pub struct AppState {
    inner: Mutex<AppStateData>,
    config_path: PathBuf,
    /// LLM Provider（延迟初始化，存储 trait 对象以支持多后端与 Mock）
    llm_provider: OnceLock<Arc<dyn LlmProvider>>,
    /// Embed Provider（应用启动后由 init_embed_provider 注入，支持 ONNX / Ollama / Noop）
    embed_provider: OnceLock<Arc<dyn EmbedProvider>>,
    /// Tauri AppHandle（应用启动后由 setup hook 注入，用于 emit 进度事件）
    app_handle: OnceLock<AppHandle>,
    /// 会话历史（in-memory，session_id -> 轮次列表）
    ask_sessions: Mutex<std::collections::HashMap<String, Vec<crate::models::AskTurn>>>,
    /// 会话取消标志（session_id -> AtomicBool）
    ask_cancel_flags:
        Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    /// 搜索配置（Deep Research 用）
    search_config: Mutex<crate::models::SearchConfig>,
    /// 等待用户审批子查询的一次性 channel（task_id -> sender）
    pending_query_approvals:
        Mutex<std::collections::HashMap<i64, tokio::sync::oneshot::Sender<Vec<String>>>>,
    /// 摄入预览缓存（preview_id -> 预览上下文）。
    ingest_previews: Mutex<std::collections::HashMap<String, CachedIngestPreview>>,
    /// Shell 会话缓存（session_id -> 会话状态）。
    shell_sessions: Mutex<std::collections::HashMap<String, ShellSessionState>>,
    /// agent_chat 取消令牌（conversation_id -> CancelToken）
    chat_cancellations:
        Mutex<std::collections::HashMap<i64, crate::agent_chat::runtime::CancelToken>>,
    /// agent_chat 写操作审批 channel（pending_id -> oneshot::Sender<Result<String,String>>）
    chat_write_approvals: Mutex<
        std::collections::HashMap<i64, tokio::sync::oneshot::Sender<Result<String, String>>>,
    >,
    /// agent_chat shell 审批待执行信息（pending_id -> (command, timeout_ms)）
    chat_shell_pending: Mutex<std::collections::HashMap<i64, (String, u64)>>,
    /// 活跃 MCP 客户端（server_name -> client）。Arc 允许多个 await 并发持有引用。
    mcp_clients: Mutex<
        std::collections::HashMap<
            String,
            std::sync::Arc<tokio::sync::Mutex<crate::agent_chat::mcp::McpClient>>,
        >,
    >,
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
    /// Embedding 后端（"onnx" | "ollama" | "disabled"）
    embed_backend: Option<String>,
    /// ONNX 模型名（"multilingual-e5-small" | "bge-small-zh-v1.5"）
    embed_onnx_model: Option<String>,
    /// Shell 策略配置（安全与能力平衡）。
    shell_policy: ShellPolicyConfig,
    /// Agent 写入审批队列（run_id -> 待处理条目）
    pending_agent_writes: std::collections::HashMap<i64, crate::models::PendingAgentWrite>,
}

/// 预览审批到落盘之间的一次性缓存。
#[derive(Debug, Clone)]
struct CachedIngestPreview {
    source_type: String,
    source_path: String,
    source_content: String,
    summary: String,
    entities: Vec<String>,
}

/// Shell 会话状态（仅存储必要上下文）。
#[derive(Debug, Clone)]
struct ShellSessionState {
    cwd: PathBuf,
    last_active_at: String,
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
        let query_top_k = wiki_service::normalize_top_k(config.query_top_k);
        let search_config = search_service::load_search_config_from_path(&config_path);
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
                embed_backend: config.embed_backend,
                embed_onnx_model: config.embed_onnx_model,
                shell_policy: config.shell_policy.unwrap_or_default(),
                pending_agent_writes: std::collections::HashMap::new(),
            }),
            config_path,
            llm_provider: OnceLock::new(),
            embed_provider: OnceLock::new(),
            app_handle: OnceLock::new(),
            ask_sessions: Mutex::new(std::collections::HashMap::new()),
            ask_cancel_flags: Mutex::new(std::collections::HashMap::new()),
            search_config: Mutex::new(search_config),
            pending_query_approvals: Mutex::new(std::collections::HashMap::new()),
            ingest_previews: Mutex::new(std::collections::HashMap::new()),
            shell_sessions: Mutex::new(std::collections::HashMap::new()),
            chat_cancellations: Mutex::new(std::collections::HashMap::new()),
            chat_write_approvals: Mutex::new(std::collections::HashMap::new()),
            chat_shell_pending: Mutex::new(std::collections::HashMap::new()),
            mcp_clients: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn new() -> Self {
        let config_path = Self::default_config_path();
        let (config, config_snapshot) = Self::load_config(&config_path);
        let mode = config.mode;
        let vault_path = config.vault_path.clone().map(PathBuf::from);
        let query_top_k = config.query_top_k;
        let query_top_k = wiki_service::normalize_top_k(query_top_k);
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
            embed_backend: config.embed_backend.clone(),
            embed_onnx_model: config.embed_onnx_model.clone(),
            shell_policy: config.shell_policy.clone(),
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

        let search_config = search_service::load_search_config_from_path(&config_path);
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
                embed_backend: config.embed_backend,
                embed_onnx_model: config.embed_onnx_model,
                shell_policy: config.shell_policy.unwrap_or_default(),
                pending_agent_writes: std::collections::HashMap::new(),
            }),
            config_path,
            llm_provider: OnceLock::new(),
            embed_provider: OnceLock::new(),
            app_handle: OnceLock::new(),
            ask_sessions: Mutex::new(std::collections::HashMap::new()),
            ask_cancel_flags: Mutex::new(std::collections::HashMap::new()),
            search_config: Mutex::new(search_config),
            pending_query_approvals: Mutex::new(std::collections::HashMap::new()),
            ingest_previews: Mutex::new(std::collections::HashMap::new()),
            shell_sessions: Mutex::new(std::collections::HashMap::new()),
            chat_cancellations: Mutex::new(std::collections::HashMap::new()),
            chat_write_approvals: Mutex::new(std::collections::HashMap::new()),
            chat_shell_pending: Mutex::new(std::collections::HashMap::new()),
            mcp_clients: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// 注入 Tauri AppHandle（在应用 setup hook 中调用一次）。
    pub fn set_app_handle(&self, handle: AppHandle) {
        let _ = self.app_handle.set(handle);
    }

    /// 返回 AppHandle 引用（setup 完成后始终 Some）。
    pub fn get_app_handle(&self) -> Option<&AppHandle> {
        self.app_handle.get()
    }

    pub fn get_search_config(&self) -> crate::models::SearchConfig {
        search_service::get_search_config(self)
    }

    pub fn set_search_config(&self, cfg: crate::models::SearchConfig) -> Result<(), String> {
        search_service::set_search_config(self, cfg)
    }

    pub async fn search_web_cascade_with_source(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<(Vec<crate::models::WebSearchResult>, &'static str), String> {
        search_service::search_web_cascade_with_source(self, query, max_results).await
    }

    pub fn register_query_approval(
        &self,
        task_id: i64,
    ) -> tokio::sync::oneshot::Receiver<Vec<String>> {
        search_service::register_query_approval(self, task_id)
    }

    pub fn approve_research_queries(&self, task_id: i64, queries: Vec<String>) -> bool {
        search_service::approve_research_queries(self, task_id, queries)
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

    /// 解析 ONNX 模型目录：打包资源优先，开发时回退 CARGO_MANIFEST_DIR。
    fn resolve_onnx_model_dir(&self, model_name: &str) -> std::path::PathBuf {
        if let Some(handle) = self.app_handle.get() {
            if let Ok(resource_dir) = handle.path().resource_dir() {
                let resource_dir: std::path::PathBuf = resource_dir;
                let bundled = resource_dir.join("embed-models").join(model_name);
                if bundled.join("onnx/model.onnx").exists() {
                    return bundled;
                }
            }
        }
        // 开发期路径（cargo test / dev run）
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/embed-models")
            .join(model_name)
    }

    /// 解析 onnxruntime DLL 路径：打包资源目录优先，开发时回退 CARGO_MANIFEST_DIR。
    fn resolve_ort_dylib_path(&self) -> std::path::PathBuf {
        if let Some(handle) = self.app_handle.get() {
            if let Ok(resource_dir) = handle.path().resource_dir() {
                let resource_dir: std::path::PathBuf = resource_dir;
                let bundled = resource_dir.join("onnxruntime.dll");
                if bundled.exists() {
                    return bundled;
                }
            }
        }
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("onnxruntime.dll")
    }

    /// 初始化 Embed Provider（应用 setup 完成后调用一次）。
    /// 三路选择：onnx → ollama → disabled(noop)。
    pub fn init_embed_provider(&self) {
        let (backend, onnx_model, embed_model, embed_base_url, fallback_base_url) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (
                guard.embed_backend.clone().unwrap_or_else(|| "onnx".to_string()),
                guard.embed_onnx_model.clone().unwrap_or_else(|| "multilingual-e5-small".to_string()),
                guard.embed_ollama_model.clone(),
                guard.embed_ollama_base_url.clone(),
                guard.ollama_base_url.clone(),
            )
        };

        let provider: Arc<dyn EmbedProvider> = match backend.as_str() {
            "onnx" => {
                // 运行时动态加载 ORT DLL（load-dynamic feature）
                // 优先使用已有环境变量，否则自动探测打包资源目录和开发路径
                if std::env::var("ORT_DYLIB_PATH").is_err() {
                    let dll_path = self.resolve_ort_dylib_path();
                    if dll_path.exists() {
                        std::env::set_var("ORT_DYLIB_PATH", &dll_path);
                    }
                }
                let model_dir = self.resolve_onnx_model_dir(&onnx_model);
                match crate::llm::OnnxEmbedder::from_resource_dir(&model_dir) {
                    Ok(embedder) => {
                        self.push_log(LogLevel::Info, format!("ONNX Embed 已加载：{}", onnx_model));
                        Arc::new(embedder) as Arc<dyn EmbedProvider>
                    }
                    Err(e) => {
                        self.push_log(LogLevel::Warn, format!("ONNX 加载失败，已回退 Noop（{}）", e));
                        Arc::new(NoopEmbedder) as Arc<dyn EmbedProvider>
                    }
                }
            }
            "ollama" => {
                let mut config = OllamaConfig::default();
                let model = embed_model.as_deref().filter(|m| !m.trim().is_empty()).unwrap_or("nomic-embed-text:latest");
                config.model = model.to_string();
                let base_url = embed_base_url.as_deref().filter(|u| !u.trim().is_empty())
                    .or_else(|| fallback_base_url.as_deref().filter(|u| !u.trim().is_empty()))
                    .unwrap_or(&config.base_url);
                config.base_url = base_url.to_string();
                self.push_log(LogLevel::Info, format!("Ollama Embed 已配置：{}", model));
                Arc::new(OllamaProvider::new(config)) as Arc<dyn EmbedProvider>
            }
            _ => {
                self.push_log(LogLevel::Info, "Embed 后端已禁用（Noop）".to_string());
                Arc::new(NoopEmbedder) as Arc<dyn EmbedProvider>
            }
        };

        let _ = self.embed_provider.set(provider);
    }

    /// 获取 Embedding 专用 Provider（Phase 4：ONNX / Ollama / Noop 三路选择）。
    pub(crate) fn get_embed_provider(&self) -> Arc<dyn EmbedProvider> {
        self.embed_provider
            .get()
            .cloned()
            .unwrap_or_else(|| Arc::new(NoopEmbedder) as Arc<dyn EmbedProvider>)
    }

    /// 获取 LLM Provider，按模式路由：
    /// - StrictLocal → 仅 Ollama
    /// - Hybrid → 优先使用 active_provider（仅 cloud/ollama），并在无 key 时安全回退到 ollama
    pub(crate) fn get_llm_provider(&self) -> Option<Arc<dyn LlmProvider>> {
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
        let resolved_provider = config_service::resolve_active_provider(
            mode,
            active_provider.as_deref(),
            has_cloud_key,
            None,
        );

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
                    let base_url = config_service::effective_cloud_base_url(
                        cloud_provider_name.as_deref(),
                        cloud_base_url.as_deref(),
                    );
                    let config = OpenAiConfig::with_base_url_and_model(key, base_url, model, None);
                    Some(Arc::new(OpenAiProvider::new(config)) as Arc<dyn LlmProvider>)
                } else {
                    Some(self.get_ollama_provider())
                }
            }
        }
    }

    pub async fn generate_summary(&self, content: &str) -> String {
        config_service::generate_summary(self, content).await
    }

    pub fn llm_status_future(
        &self,
    ) -> impl std::future::Future<Output = LlmStatus> + Send + 'static {
        config_service::llm_status_future(self)
    }

    /// 获取知识图谱数据（所有 wiki 页面节点 + citations 边）。
    pub fn get_knowledge_graph_impl(&self) -> Result<KnowledgeGraphData, String> {
        graph_service::get_knowledge_graph_impl(self)
    }

    pub fn get_knowledge_subgraph_impl(
        &self,
        center_page_path: String,
        hop: u8,
        direction: KnowledgeGraphDirection,
        limit_nodes: usize,
        limit_links: usize,
    ) -> Result<KnowledgeSubgraphData, String> {
        graph_service::get_knowledge_subgraph_impl(self, center_page_path, hop, direction, limit_nodes, limit_links)
    }

    /// 获取当前 LLM Provider 配置（供 Settings 页面读取）。
    pub fn get_llm_config(&self) -> LlmProviderConfig {
        config_service::get_llm_config(self)
    }

    pub fn set_llm_config(&self, config: LlmProviderConfig) -> Result<LlmProviderConfig, String> {
        config_service::set_llm_config(self, config)
    }

    pub fn get_ocr_config(&self) -> Option<String> {
        config_service::get_ocr_config(self)
    }

    pub fn set_ocr_config(&self, provider: Option<String>) -> Result<(), String> {
        config_service::set_ocr_config(self, provider)
    }

    pub fn get_shell_policy_config(&self) -> ShellPolicyConfig {
        config_service::get_shell_policy_config(self)
    }

    pub fn set_shell_policy_config(
        &self,
        config: ShellPolicyConfig,
    ) -> Result<ShellPolicyConfig, String> {
        config_service::set_shell_policy_config(self, config)
    }

    pub fn set_mode(&self, mode: AppMode) -> ModeChangeResult {
        config_service::set_mode(self, mode)
    }

    pub fn init_vault(&self, vault_path: PathBuf) -> Result<VaultInitResult, String> {
        config_service::init_vault(self, vault_path)
    }

    pub fn init_vault_with_template(
        &self,
        vault_path: PathBuf,
        template_schema: String,
        template_purpose: String,
        extra_dirs: Vec<String>,
    ) -> Result<VaultInitResult, String> {
        config_service::init_vault_with_template(self, vault_path, template_schema, template_purpose, extra_dirs)
    }

    pub async fn ingest_markdown(
        &self,
        source_path: PathBuf,
        display_name: Option<String>,
    ) -> Result<IngestResult, String> {
        ingest_service::ingest_markdown(self, source_path, display_name).await
    }

    pub async fn preview_ingest_file(
        &self,
        source_type: &str,
        source_path: &str,
        ocr_provider: Option<&str>,
    ) -> Result<IngestPreview, String> {
        ingest_service::preview_ingest_file(self, source_type, source_path, ocr_provider).await
    }

    pub async fn apply_ingest_preview(&self, preview_id: &str) -> Result<IngestResult, String> {
        ingest_service::apply_ingest_preview(self, preview_id).await
    }

    pub async fn ingest_file_impl(
        &self,
        source_path: &str,
        ocr_provider: Option<&str>,
    ) -> Result<IngestResult, String> {
        ingest_service::ingest_file_impl(self, source_path, ocr_provider).await
    }

    pub fn read_file_for_chat_impl(&self, path: &str) -> Result<crate::models::FileChunk, String> {
        ingest_service::read_file_for_chat_impl(self, path)
    }

    pub async fn ingest_pdf_impl(&self, source_path: &str) -> Result<IngestResult, String> {
        ingest_service::ingest_pdf_impl(self, source_path).await
    }

    pub async fn ingest_url_impl(&self, url: &str) -> Result<crate::models::IngestResult, String> {
        ingest_service::ingest_url_impl(self, url).await
    }


    #[cfg(test)]
    async fn generate_query_answer_with_provider(
        &self,
        question: &str,
        matches: &[WikiMatch],
        provider: Option<Arc<dyn LlmProvider>>,
        on_chunk: Option<&mut (dyn FnMut(String) + Send)>,
    ) -> (String, String) {
        ask_service::generate_query_answer_with_provider(self, question, matches, provider, on_chunk).await
    }

    pub fn overview(&self) -> AppOverview {
        config_service::overview(self)
    }

    pub fn default_paths(&self) -> DefaultPaths {
        config_service::default_paths(self)
    }

    pub fn query_settings(&self) -> QuerySettings {
        config_service::query_settings(self)
    }

    pub fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        config_service::recent_logs(self, limit)
    }

    pub fn recent_wiki_pages(&self, limit: usize) -> Result<Vec<WikiPageItem>, String> {
        wiki_service::recent_wiki_pages(self, limit)
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
        wiki_service::search_wiki_pages(self, keyword, limit)
    }

    pub async fn search_wiki_pages_hybrid(
        &self,
        keyword: String,
        limit: usize,
    ) -> Result<Vec<WikiPageItem>, String> {
        wiki_service::search_wiki_pages_hybrid(self, keyword, limit).await
    }

    pub fn search_wiki_paths(&self, query: String) -> Result<Vec<String>, String> {
        wiki_service::search_wiki_paths(self, query)
    }

    pub async fn ai_assist_wiki_edit(
        &self,
        app_handle: &tauri::AppHandle,
        action: String,
        selected_text: String,
        context: String,
        page_title: String,
    ) -> Result<(), String> {
        wiki_service::ai_assist_wiki_edit(
            self,
            app_handle,
            &action,
            &selected_text,
            &context,
            &page_title,
        )
        .await
    }

    pub fn export_wiki_markdown_zip(&self, dest_path: String) -> Result<u32, String> {
        wiki_service::export_wiki_markdown_zip_impl(self, dest_path)
    }

    pub fn export_wiki_html_zip(&self, dest_path: String) -> Result<u32, String> {
        wiki_service::export_wiki_html_zip_impl(self, dest_path)
    }

    pub fn wiki_page_detail(&self, page_path: String) -> Result<WikiPageDetail, String> {
        wiki_service::wiki_page_detail(self, page_path)
    }

    pub fn set_page_stale(&self, page_path: String, stale: bool) -> Result<(), String> {
        wiki_service::set_page_stale(self, page_path, stale)
    }

    pub fn wiki_page_citations(
        &self,
        page_path: String,
    ) -> Result<Vec<WikiPageCitationItem>, String> {
        wiki_service::wiki_page_citations(self, page_path)
    }

    #[cfg(test)]
    pub fn lint_report(&self) -> LintReport {
        lint_service::lint_report(self)
    }

    pub fn preview_lint_patches(&self) -> LintPatchPreview {
        lint_service::preview_lint_patches(self)
    }

    pub fn quick_lint_page_impl(
        &self,
        wiki_path: &str,
    ) -> Result<crate::models::PageQuickLint, String> {
        lint_service::quick_lint_page_impl(self, wiki_path)
    }

    pub fn get_vault_stats_impl(&self) -> Result<crate::models::VaultStats, String> {
        lint_service::get_vault_stats_impl(self)
    }

    pub async fn run_lint_with_outbox(&self) -> LintReport {
        lint_service::run_lint_with_outbox(self).await
    }

    pub fn apply_lint_patch(
        &self,
        input: LintPatchApplyInput,
    ) -> Result<LintPatchApplyResult, String> {
        lint_service::apply_lint_patch(self, input)
    }

    pub fn apply_lint_patches_batch(
        &self,
        inputs: Vec<LintPatchApplyInput>,
    ) -> Result<LintPatchBatchApplyResult, String> {
        lint_service::apply_lint_patches_batch(self, inputs)
    }

    pub async fn query_ask(&self, question: String) -> Result<QueryAnswerResult, String> {
        ask_service::query_ask(self, question).await
    }

    pub async fn query_ask_with_options(
        &self,
        question: String,
        options: QueryAskOptions,
    ) -> Result<QueryAnswerResult, String> {
        ask_service::query_ask_with_options(self, question, options).await
    }
    pub fn set_query_top_k(&self, top_k: usize) -> Result<QuerySettings, String> {
        ask_service::set_query_top_k(self, top_k)
    }

    pub fn save_query_answer(
        &self,
        input: SaveQueryAnswerInput,
    ) -> Result<SaveQueryAnswerResult, String> {
        ask_service::save_query_answer(self, input)
    }

    /// 将编辑后的内容写回 vault 文件，并同步更新 SQLite FTS 索引。
    /// 可选的 expected_checksum 用于在写入前校验文件未被外部修改（协作安全）。
    pub async fn save_wiki_page_impl(
        &self,
        path: &str,
        content: &str,
        expected_checksum: Option<&str>,
    ) -> Result<crate::models::SaveWikiPageResult, String> {
        wiki_service::save_wiki_page_impl(self, path, content, expected_checksum).await
    }

    pub fn list_wiki_page_history_impl(
        &self,
        path: &str,
        limit: Option<usize>,
    ) -> Result<Vec<WikiPageHistoryItem>, String> {
        wiki_service::list_wiki_page_history_impl(self, path, limit)
    }

    pub fn get_wiki_page_history_entry_impl(
        &self,
        id: i64,
    ) -> Result<WikiPageHistoryDetail, String> {
        wiki_service::get_wiki_page_history_entry_impl(self, id)
    }

    pub async fn restore_wiki_page_from_history_impl(
        &self,
        id: i64,
    ) -> Result<crate::models::SaveWikiPageResult, String> {
        wiki_service::restore_wiki_page_from_history_impl(self, id).await
    }

    async fn generate_ai_wiki_markdown_draft_impl(
        &self,
        db_path: &Path,
        topic: &str,
        memories_context: Option<&str>,
        skill_prompt: Option<&str>,
        research_mode: bool,
        ask_context: Option<&str>,
    ) -> Result<(String, String, String), String> {
        wiki_service::generate_ai_wiki_markdown_draft_impl(self, db_path, topic, memories_context, skill_prompt, research_mode, ask_context).await
    }

    pub async fn create_wiki_page_with_ai_impl(
        &self,
        topic: String,
    ) -> Result<NewPageResult, String> {
        wiki_service::create_wiki_page_with_ai_impl(self, topic).await
    }

    pub async fn rename_wiki_page_impl(
        &self,
        old_path: &str,
        new_name: &str,
    ) -> Result<crate::models::RenameWikiPageResult, String> {
        wiki_service::rename_wiki_page_impl(self, old_path, new_name).await
    }

    pub async fn delete_wiki_page_impl(
        &self,
        path: &str,
    ) -> Result<crate::models::DeleteWikiPageResult, String> {
        wiki_service::delete_wiki_page_impl(self, path).await
    }

    pub fn purge_orphaned_wiki_pages(&self) {
        wiki_service::purge_orphaned_wiki_pages(self)
    }

    pub fn save_ask_history_impl(&self, question: &str) -> Result<(), String> {
        ask_service::save_ask_history_impl(self, question)
    }

    pub fn get_ask_history_impl(
        &self,
        limit: usize,
    ) -> Result<Vec<crate::models::AskHistoryItem>, String> {
        ask_service::get_ask_history_impl(self, limit)
    }

    pub fn clear_ask_history_impl(&self) -> Result<usize, String> {
        ask_service::clear_ask_history_impl(self)
    }

    pub fn create_ask_session_impl(
        &self,
        session_id: &str,
        title: Option<&str>,
    ) -> Result<AskSessionItem, String> {
        ask_service::create_ask_session_impl(self, session_id, title)
    }

    pub fn list_ask_sessions_impl(&self, limit: usize) -> Result<Vec<AskSessionItem>, String> {
        ask_service::list_ask_sessions_impl(self, limit)
    }

    pub fn list_ask_session_turns_impl(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<AskSessionTurnItem>, String> {
        ask_service::list_ask_session_turns_impl(self, session_id, limit)
    }

    pub fn search_ask_session_turns_impl(
        &self,
        keyword: &str,
        limit: usize,
    ) -> Result<Vec<AskSessionSearchHitItem>, String> {
        ask_service::search_ask_session_turns_impl(self, keyword, limit)
    }

    pub fn rename_ask_session_impl(&self, session_id: &str, title: &str) -> Result<(), String> {
        ask_service::rename_ask_session_impl(self, session_id, title)
    }

    pub fn delete_ask_session_impl(&self, session_id: &str) -> Result<usize, String> {
        ask_service::delete_ask_session_impl(self, session_id)
    }

    pub fn get_outbox_events_impl(
        &self,
        last_id: i64,
        limit: usize,
    ) -> Result<Vec<OutboxEventItem>, String> {
        ask_service::get_outbox_events_impl(self, last_id, limit)
    }

    pub fn ack_outbox_events_impl(
        &self,
        up_to_id: i64,
        consumer_tag: &str,
    ) -> Result<OutboxAckResult, String> {
        ask_service::ack_outbox_events_impl(self, up_to_id, consumer_tag)
    }

    /// 创建 Agent Run（H0：最小闭环入口）。
    pub fn start_agent_run_impl(&self, topic: &str) -> Result<i64, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let run_id = db::start_agent_run(&db_path, topic, &current_timestamp_ms())?;
        Ok(run_id)
    }

    /// 追加 Agent Run 事件。
    pub fn append_agent_run_event_impl(
        &self,
        run_id: i64,
        level: &str,
        message: &str,
    ) -> Result<(), String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        db::append_agent_run_event(&db_path, run_id, level, message, &current_timestamp_ms())
    }

    /// 列出最近 Agent Runs。
    pub fn list_agent_runs_impl(
        &self,
        limit: Option<i64>,
        include_archived: Option<bool>,
    ) -> Result<Vec<AgentRunItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let safe_limit = limit.unwrap_or(50).clamp(1, 200) as usize;
        let records = db::list_agent_runs(&db_path, safe_limit, include_archived.unwrap_or(false))?;
        Ok(records
            .into_iter()
            .map(|item| AgentRunItem {
                id: item.id,
                topic: item.topic,
                status: item.status,
                created_at: item.created_at,
                updated_at: item.updated_at,
                completed_at: item.completed_at,
                archived_at: item.archived_at,
            })
            .collect())
    }

    /// 列出指定 Agent Run 的事件。
    pub fn list_agent_run_events_impl(
        &self,
        run_id: i64,
        limit: Option<i64>,
    ) -> Result<Vec<AgentRunEventItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let safe_limit = limit.unwrap_or(200).clamp(1, 1000) as usize;
        let records = db::list_agent_run_events(&db_path, run_id, safe_limit)?;
        Ok(records
            .into_iter()
            .map(|item| AgentRunEventItem {
                id: item.id,
                run_id: item.run_id,
                level: item.level,
                message: item.message,
                created_at: item.created_at,
            })
            .collect())
    }

    /// 将 Agent Run 置为终态。
    pub fn complete_agent_run_impl(&self, run_id: i64, status: &str) -> Result<(), String> {
        let normalized_status = status.trim();
        if normalized_status != "running"
            && normalized_status != "reviewing"
            && normalized_status != "applied"
            && normalized_status != "failed"
        {
            return Err("status 仅支持 running/reviewing/applied/failed".to_string());
        }
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        db::complete_agent_run(&db_path, run_id, normalized_status, &current_timestamp_ms())
    }

    /// 归档 Agent Run（软删除）。
    pub fn archive_agent_run_impl(&self, run_id: i64) -> Result<(), String> {
        {
            let data = self.inner.lock().expect("状态锁已被污染");
            if data.pending_agent_writes.contains_key(&run_id) {
                return Err(format!("run #{} 存在待审批写入，禁止归档", run_id));
            }
        }
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let target = db::get_agent_run_by_id(&db_path, run_id)?
            .ok_or_else(|| format!("Agent Run 不存在: {}", run_id))?;
        let normalized_status = target.status.trim().to_lowercase();
        if normalized_status == "running" || normalized_status == "reviewing" {
            return Err(format!("run #{} 正在进行中，禁止归档", run_id));
        }
        db::archive_agent_run(
            &db_path,
            run_id,
            Some("manual_archive"),
            &current_timestamp_ms(),
        )
    }

    /// 恢复已归档 Agent Run。
    pub fn restore_agent_run_impl(&self, run_id: i64) -> Result<(), String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        db::restore_agent_run(&db_path, run_id, &current_timestamp_ms())
    }

    /// 写入或更新 agent 记忆（H2）。
    pub fn upsert_agent_memory_impl(
        &self,
        run_id: Option<i64>,
        key: &str,
        value: &str,
    ) -> Result<AgentMemoryItem, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let now = current_timestamp_ms();
        let record = db::upsert_agent_memory(&db_path, run_id, key, value, &now)?;
        Ok(AgentMemoryItem {
            id: record.id,
            run_id: record.run_id,
            memory_key: record.memory_key,
            memory_value: record.memory_value,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }

    /// 列出 agent 记忆（H2）。
    pub fn list_agent_memories_impl(
        &self,
        run_id: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<AgentMemoryItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let safe_limit = limit.unwrap_or(50).clamp(1, 200) as usize;
        let records = db::list_agent_memories(&db_path, run_id, safe_limit)?;
        Ok(records
            .into_iter()
            .map(|r| AgentMemoryItem {
                id: r.id,
                run_id: r.run_id,
                memory_key: r.memory_key,
                memory_value: r.memory_value,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    /// 删除单条 agent 记忆（H2）。
    pub fn delete_agent_memory_impl(&self, id: i64) -> Result<(), String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        db::delete_agent_memory(&db_path, id)
    }

    /// 写入或更新 Agent 技能模板（H3）。
    pub fn upsert_agent_skill_impl(
        &self,
        skill_key: &str,
        prompt_template: &str,
    ) -> Result<AgentSkillItem, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let now = current_timestamp_ms();
        let record = db::upsert_agent_skill(&db_path, skill_key, prompt_template, &now)?;
        Ok(AgentSkillItem {
            id: record.id,
            skill_key: record.skill_key,
            prompt_template: record.prompt_template,
            version: record.version,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }

    /// 列出 Agent 技能模板（H3）。
    pub fn list_agent_skills_impl(
        &self,
        limit: Option<i64>,
    ) -> Result<Vec<AgentSkillItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let safe_limit = limit.unwrap_or(50).clamp(1, 200) as usize;
        let records = db::list_agent_skills(&db_path, safe_limit)?;
        Ok(records
            .into_iter()
            .map(|r| AgentSkillItem {
                id: r.id,
                skill_key: r.skill_key,
                prompt_template: r.prompt_template,
                version: r.version,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
            .collect())
    }

    /// 删除单条 Agent 技能模板（H3）。
    pub fn delete_agent_skill_impl(&self, id: i64) -> Result<(), String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        db::delete_agent_skill(&db_path, id)
    }

    /// AAAK-lite：加载记忆，超阈值时用 LLM 压缩，返回格式化的记忆上下文字符串。
    async fn load_and_maybe_compress_memories_impl(
        &self,
        run_id: i64,
        db_path: &std::path::Path,
    ) -> Option<String> {
        let run_mems =
            db::list_agent_memories(db_path, Some(run_id), MEMORY_INJECT_LIMIT).unwrap_or_default();
        let global_mems =
            db::list_agent_memories(db_path, None, MEMORY_INJECT_LIMIT).unwrap_or_default();

        let all_mems: Vec<_> = run_mems.iter().chain(global_mems.iter()).collect();
        if all_mems.is_empty() {
            return None;
        }

        let total_len: usize = all_mems
            .iter()
            .map(|m| m.memory_key.len() + m.memory_value.len())
            .sum();

        // 仅对当前 run 的记忆做压缩（全局记忆不压缩）。
        if total_len > MEMORY_COMPRESS_THRESHOLD_CHARS && !run_mems.is_empty() {
            if let Some(compressed) = self.compress_memories_with_llm(&run_mems, run_id).await {
                // 写回压缩后记忆
                let now = current_timestamp_ms();
                if let Err(e) =
                    db::bulk_replace_agent_memories(db_path, Some(run_id), &compressed, &now)
                {
                    self.push_log(
                        LogLevel::Warn,
                        format!("AAAK-lite 写回压缩记忆失败（降级）: {}", e),
                    );
                } else {
                    // 重新加载压缩后的记忆
                    let refreshed =
                        db::list_agent_memories(db_path, Some(run_id), MEMORY_INJECT_LIMIT)
                            .unwrap_or_default();
                    return Some(format_memories_for_prompt(&refreshed, &global_mems));
                }
            }
        }

        Some(format_memories_for_prompt(&run_mems, &global_mems))
    }

    /// AAAK-lite 压缩：调用 LLM 将记忆压缩为紧凑 key-value 列表。
    async fn compress_memories_with_llm(
        &self,
        memories: &[db::AgentMemoryRecord],
        run_id: i64,
    ) -> Option<Vec<(String, String)>> {
        let provider = self.get_llm_provider()?;
        let memory_text = memories
            .iter()
            .map(|m| format!("{}: {}", m.memory_key, m.memory_value))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "以下是 Agent run #{run_id} 的运行记忆，请精简压缩，保留最重要的信息。\
            每行输出一条，格式为「键: 值」，不要输出任何其他内容。\n\n{memory_text}",
            run_id = run_id,
            memory_text = memory_text,
        );
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            provider.complete(&prompt),
        )
        .await
        .ok()? // Elapsed → None
        .ok()?; // LlmError → None
        let entries: Vec<(String, String)> = result
            .lines()
            .filter_map(|line: &str| {
                let mut parts = line.splitn(2, ':');
                let k = parts.next()?.trim().to_string();
                let v = parts.next()?.trim().to_string();
                if k.is_empty() || v.is_empty() {
                    None
                } else {
                    Some((k, v))
                }
            })
            .collect();
        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }

    /// H5-B: 审批写盘后自动从草稿内容提炼全局记忆（降级不阻塞主流程）。
    async fn extract_and_save_memories_from_draft(
        &self,
        run_id: i64,
        content: &str,
        db_path: &std::path::Path,
    ) {
        let provider = match self.get_llm_provider() {
            Some(p) => p,
            None => return,
        };
        let snippet: String = content.chars().take(3000).collect();
        let prompt = format!(
            "以下是一篇已写入知识库的 Wiki 草稿。\
            请从中提取 3-5 条最有价值的知识点作为长期记忆，\
            每条格式严格为「键: 值」（键为简短标识，值为简要说明），\
            每行一条，不要输出任何其他内容。\n\n{snippet}",
            snippet = snippet,
        );
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(40),
            provider.complete(&prompt),
        )
        .await
        .ok()
        .and_then(|r| r.ok());

        let text = match result {
            Some(t) if !t.trim().is_empty() => t,
            _ => return,
        };

        let now = current_timestamp_ms();
        let mut saved = 0usize;
        for line in text.lines() {
            let mut parts = line.splitn(2, ':');
            let key = match parts.next().map(str::trim) {
                Some(k) if !k.is_empty() => k.to_string(),
                _ => continue,
            };
            let value = match parts.next().map(str::trim) {
                Some(v) if !v.is_empty() => v.to_string(),
                _ => continue,
            };
            if db::upsert_agent_memory(db_path, None, &key, &value, &now).is_ok() {
                saved += 1;
            }
            if saved >= 5 {
                break;
            }
        }
        if saved > 0 {
            let msg = format!("已从 run #{} 草稿自动提炼 {} 条全局记忆", run_id, saved);
            let _ = db::append_agent_run_event(db_path, run_id, "info", &msg, &now);
            self.push_log(LogLevel::Info, msg);
        }
    }

    /// 生成 Agent 草稿（只落库，不直接写入 Wiki 文件）。
    pub async fn generate_agent_draft_impl(
        &self,
        run_id: i64,
        topic: String,
        skill_key: Option<String>,
        research_mode: bool,
        ask_first: bool,
    ) -> Result<AgentDraftItem, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let normalized_topic = topic.trim();
        if normalized_topic.is_empty() {
            return Err("topic 不能为空".to_string());
        }
        let normalized_topic: String = normalized_topic.chars().take(200).collect();

        // AAAK-lite：加载记忆并在超阈值时自动压缩。
        let memories_context = self
            .load_and_maybe_compress_memories_impl(run_id, &db_path)
            .await;

        let skill_key = skill_key
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let skill_prompt = if let Some(ref key) = skill_key {
            db::get_agent_skill_by_key(&db_path, key)?.map(|item| item.prompt_template)
        } else {
            None
        };
        let applied_skill_key = if skill_prompt.is_some() {
            skill_key.clone()
        } else {
            None
        };

        // H5-C: Ask-first — 先对 topic 做一次 Ask 检索，将现有知识库答案注入 draft prompt。
        let ask_answer: Option<String> = if ask_first {
            match self
                .query_ask_with_options(
                    normalized_topic.to_string(),
                    crate::models::QueryAskOptions { top_k: Some(3) },
                )
                .await
            {
                Ok(result) if !result.answer.trim().is_empty() => {
                    let snippet: String = result.answer.chars().take(1200).collect();
                    Some(snippet)
                }
                _ => None,
            }
        } else {
            None
        };

        let (title, _llm_content, markdown_content) = self
            .generate_ai_wiki_markdown_draft_impl(
                &db_path,
                &normalized_topic,
                memories_context.as_deref(),
                skill_prompt.as_deref(),
                research_mode,
                ask_answer.as_deref(),
            )
            .await?;
        let now = current_timestamp_ms();
        let record =
            db::create_agent_draft(&db_path, run_id, &title, &markdown_content, "draft", &now)?;

        let mut tags = String::new();
        if research_mode {
            tags.push_str("，research: on");
        }
        if ask_first {
            tags.push_str("，ask: on");
        }
        let event_msg = if let Some(key) = applied_skill_key {
            format!(
                "已生成草稿 #{}（topic: {}，skill: {}{}）",
                record.id, normalized_topic, key, tags
            )
        } else {
            format!(
                "已生成草稿 #{}（topic: {}{}）",
                record.id, normalized_topic, tags
            )
        };
        if let Err(err) = db::append_agent_run_event(&db_path, run_id, "info", &event_msg, &now) {
            self.push_log(
                LogLevel::Warn,
                format!("写入 Agent 草稿事件失败（降级）: {}", err),
            );
        }

        Ok(AgentDraftItem {
            id: record.id,
            run_id: record.run_id,
            title: record.title,
            content: record.content,
            status: record.status,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }

    /// H6-S2：执行任务模式（多轮决策 + 受控工具调用）。
    pub async fn run_agent_task_impl(
        &self,
        run_id: i64,
        instruction: String,
        max_iterations: Option<u32>,
        memory_context: Option<String>,
    ) -> Result<String, String> {
        agent_service::run_agent_task(self, run_id, instruction, max_iterations, memory_context)
            .await
    }

    /// 列出指定 Run 的 Agent 草稿。
    pub fn list_agent_drafts_impl(
        &self,
        run_id: i64,
        limit: Option<i64>,
    ) -> Result<Vec<AgentDraftItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let safe_limit = limit.unwrap_or(20).clamp(1, 200) as usize;
        let records = db::list_agent_drafts(&db_path, run_id, safe_limit)?;
        Ok(records
            .into_iter()
            .map(|record| AgentDraftItem {
                id: record.id,
                run_id: record.run_id,
                title: record.title,
                content: record.content,
                status: record.status,
                created_at: record.created_at,
                updated_at: record.updated_at,
            })
            .collect())
    }

    /// 检测草稿对应的 Wiki 页面是否已存在（审批前冲突预检）。
    pub fn check_agent_draft_conflict_impl(
        &self,
        draft_id: i64,
    ) -> Result<crate::models::AgentDraftConflictInfo, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        }
        .ok_or_else(|| "请先初始化 Vault".to_string())?;
        let db_path = vault_path.join(".app").join("meta.db");

        let draft = db::get_agent_draft(&db_path, draft_id)?
            .ok_or_else(|| format!("未找到 Agent Draft: {}", draft_id))?;

        let wiki_dir = vault_path.join("wiki");
        let slug = topic_to_slug(&draft.title);
        let candidate = wiki_dir.join(format!("{}.md", slug));

        let conflict = candidate.exists();
        let (existing_path, existing_preview) = if conflict {
            let preview = std::fs::read_to_string(&candidate)
                .ok()
                .map(|c| c.chars().take(300).collect::<String>());
            (Some(candidate.to_string_lossy().to_string()), preview)
        } else {
            (None, None)
        };

        Ok(crate::models::AgentDraftConflictInfo {
            draft_id,
            title: draft.title,
            conflict,
            existing_path,
            existing_preview,
        })
    }

    /// 审批 Agent 草稿并写入 vault/wiki，同时更新 DB 与 FTS。
    pub async fn approve_agent_draft_impl(&self, draft_id: i64) -> Result<NewPageResult, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path.clone()
        }
        .ok_or_else(|| "请先初始化 Vault".to_string())?;
        let db_path = vault_path.join(".app").join("meta.db");

        let draft = db::get_agent_draft(&db_path, draft_id)?
            .ok_or_else(|| format!("未找到 Agent Draft: {}", draft_id))?;
        if draft.status != "draft" {
            return Err(format!("草稿状态不是 draft，当前为: {}", draft.status));
        }
        if draft.content.trim().is_empty() {
            return Err("草稿内容为空，无法审批".to_string());
        }

        let wiki_dir = vault_path.join("wiki");
        std::fs::create_dir_all(&wiki_dir).map_err(|e| format!("创建 wiki 目录失败: {}", e))?;

        let preferred_title = if draft.title.trim().is_empty() {
            wiki_service::extract_markdown_h1_title(&draft.content)
                .unwrap_or_else(|| format!("agent-draft-{}", draft.id))
        } else {
            draft.title.trim().to_string()
        };
        let mut base_slug = topic_to_slug(&preferred_title);
        if base_slug.is_empty() {
            base_slug = format!("agent-draft-{}", draft.id);
        }
        let final_slug = wiki_service::resolve_unique_wiki_slug(&wiki_dir, &base_slug)?;
        let wiki_file_path = wiki_dir.join(format!("{}.md", final_slug));
        std::fs::write(&wiki_file_path, &draft.content)
            .map_err(|e| format!("写入草稿到 wiki 失败: {}", e))?;

        let now = current_timestamp_ms();
        let content_hash = format!("{:x}", wiki_service::md5_simple(&draft.content));
        let content_preview: String = draft.content.chars().take(300).collect();
        db::upsert_generated_wiki_page(
            &db_path,
            &preferred_title,
            &wiki_file_path,
            &content_preview,
            &content_hash,
            &now,
        )?;
        db::upsert_fts_page(&db_path, &wiki_file_path, &preferred_title, &draft.content)?;
        db::update_agent_draft_status(&db_path, draft_id, "applied", &now)?;

        let event_msg = format!(
            "草稿 #{} 已审批写盘：{}",
            draft_id,
            wiki_file_path.to_string_lossy()
        );
        if let Err(err) =
            db::append_agent_run_event(&db_path, draft.run_id, "info", &event_msg, &now)
        {
            self.push_log(
                LogLevel::Warn,
                format!("写入 Agent 审批事件失败（降级）: {}", err),
            );
        }

        // H5-B: 自动提炼全局记忆（降级不阻塞，LLM 不可用时静默跳过）
        self.extract_and_save_memories_from_draft(draft.run_id, &draft.content, &db_path)
            .await;

        Ok(NewPageResult {
            wiki_path: wiki_file_path.to_string_lossy().to_string(),
            title: preferred_title,
            content_preview,
        })
    }

    /// H5-A: 基于批注重写 Agent 草稿，生成新草稿记录（status=draft）。
    pub async fn rewrite_agent_draft_impl(
        &self,
        draft_id: i64,
        comment: String,
    ) -> Result<AgentDraftItem, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let comment = comment.trim().to_string();
        if comment.is_empty() {
            return Err("批注内容不能为空".to_string());
        }
        let draft = db::get_agent_draft(&db_path, draft_id)?
            .ok_or_else(|| format!("未找到草稿: {}", draft_id))?;
        if draft.content.trim().is_empty() {
            return Err("原草稿内容为空，无法重写".to_string());
        }

        let provider = self
            .get_llm_provider()
            .ok_or_else(|| "LLM provider 未初始化".to_string())?;

        let original: String = draft.content.chars().take(6000).collect();
        let prompt = format!(
            "请基于以下草稿和修改意见，生成改进后的完整版本。\n\
            严格保留原 Markdown 格式与 frontmatter 结构（---...---），\
            不要添加任何解释，直接输出改进后的完整 Markdown 内容。\n\n\
            【原始草稿】\n{original}\n\n\
            【修改意见】\n{comment}",
            original = original,
            comment = comment,
        );

        let llm_content = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            provider.complete(&prompt),
        )
        .await
        .map_err(|_| "重写草稿 LLM 超时（>120s）".to_string())?
        .map_err(|e| format!("重写草稿 LLM 失败: {:?}", e))?;

        if llm_content.trim().is_empty() {
            return Err("LLM 返回空内容，重写失败".to_string());
        }

        let now = current_timestamp_ms();
        let record = db::create_agent_draft(
            &db_path,
            draft.run_id,
            &draft.title,
            &llm_content,
            "draft",
            &now,
        )?;

        let event_msg = format!(
            "已基于批注重写草稿 #{}→#{}: {}",
            draft_id,
            record.id,
            comment.chars().take(60).collect::<String>()
        );
        let _ = db::append_agent_run_event(&db_path, draft.run_id, "info", &event_msg, &now);

        Ok(AgentDraftItem {
            id: record.id,
            run_id: record.run_id,
            title: record.title,
            content: record.content,
            status: record.status,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }

    /// 多轮会话问答（保留历史上下文 + 支持软取消）
    pub async fn query_ask_session(
        &self,
        session_id: String,
        question: String,
        options: QueryAskOptions,
    ) -> Result<QueryAnswerResult, String> {
        ask_service::query_ask_session(self, session_id, question, options).await
    }

    pub fn store_chat_cancel_token(
        &self,
        conv_id: i64,
        token: crate::agent_chat::runtime::CancelToken,
    ) {
        chat_service::store_chat_cancel_token(self, conv_id, token)
    }

    pub fn cancel_chat_token(&self, conv_id: i64) {
        chat_service::cancel_chat_token(self, conv_id)
    }

    pub fn remove_chat_cancel_token(&self, conv_id: i64) {
        chat_service::remove_chat_cancel_token(self, conv_id)
    }

    pub fn register_chat_write_approval(
        &self,
        pending_id: i64,
        tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    ) {
        chat_service::register_chat_write_approval(self, pending_id, tx)
    }

    pub fn approve_chat_write(&self, pending_id: i64) -> Result<(), String> {
        chat_service::approve_chat_write(self, pending_id)
    }

    pub fn reject_chat_write(&self, pending_id: i64) -> Result<(), String> {
        chat_service::reject_chat_write(self, pending_id)
    }

    pub fn register_chat_shell_pending(&self, pending_id: i64, command: String, timeout_ms: u64) {
        chat_service::register_chat_shell_pending(self, pending_id, command, timeout_ms)
    }

    pub async fn approve_chat_shell_impl(&self, pending_id: i64) -> Result<(), String> {
        chat_service::approve_chat_shell_impl(self, pending_id).await
    }

    pub fn reject_chat_shell_impl(&self, pending_id: i64) -> Result<(), String> {
        chat_service::reject_chat_shell_impl(self, pending_id)
    }

    pub async fn spawn_mcp_client(
        &self,
        name: String,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        chat_service::spawn_mcp_client(self, name, command, args, env).await
    }

    pub fn stop_mcp_client(&self, name: &str) {
        chat_service::stop_mcp_client(self, name)
    }

    pub fn get_mcp_client(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<tokio::sync::Mutex<crate::agent_chat::mcp::McpClient>>> {
        chat_service::get_mcp_client(self, name)
    }

    pub fn list_running_mcp_clients(&self) -> Vec<String> {
        chat_service::list_running_mcp_clients(self)
    }

    pub fn cancel_ask_session(&self, session_id: String) -> Result<(), String> {
        ask_service::cancel_ask_session(self, session_id)
    }

    pub fn clear_ask_session(&self, session_id: String) -> Result<(), String> {
        ask_service::clear_ask_session(self, session_id)
    }

    // ── 配置 I/O 薄包装（实现在 config_service）─────────────────────────────

    fn load_config(config_path: &Path) -> (AppConfig, Option<String>) {
        config_service::load_config(config_path)
    }

    fn default_config_path() -> PathBuf {
        config_service::default_config_path()
    }

    fn serialize_config_full(config: &AppConfig) -> String {
        config_service::serialize_config_full(config)
    }

    fn write_config_file(
        config_path: &Path,
        serialized: &str,
        expected_snapshot: Option<&str>,
    ) -> Result<(), String> {
        config_service::write_config_file(config_path, serialized, expected_snapshot)
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
    pub(crate) fn outbox_db_path(&self) -> Option<PathBuf> {
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

    pub(crate) fn vault_path_or_err(&self) -> Result<PathBuf, String> {
        let guard = self.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())
    }

    /// 存储 Agent 待审批写入条目。old_str=None 为全量写入，Some 为精确替换。
    pub(crate) fn store_pending_agent_write(
        &self,
        run_id: i64,
        resolved_path: String,
        content: String,
        old_str: Option<String>,
    ) {
        let mut data = self.inner.lock().expect("状态锁已被污染");
        data.pending_agent_writes.insert(
            run_id,
            crate::models::PendingAgentWrite {
                run_id,
                resolved_path,
                content,
                old_str,
                created_at: current_timestamp_ms(),
            },
        );
    }

    /// 取出并移除 Agent 待审批写入条目（审批或拒绝时调用）。
    pub(crate) fn take_pending_agent_write(
        &self,
        run_id: i64,
    ) -> Option<crate::models::PendingAgentWrite> {
        let mut data = self.inner.lock().expect("状态锁已被污染");
        data.pending_agent_writes.remove(&run_id)
    }

    /// 执行审批写入（write_wiki 全量 or edit_wiki patch），可测试。
    pub(crate) fn approve_agent_write_impl(&self, run_id: i64) -> Result<String, String> {
        let pending = self
            .take_pending_agent_write(run_id)
            .ok_or_else(|| format!("run #{run_id} 无待审批写入"))?;

        let vault_path = self.vault_path_or_err()?;
        let target = std::path::PathBuf::from(&pending.resolved_path);
        crate::agent_policy::validate_agent_write_path(&vault_path, &target)?;

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        let op_desc = if let Some(old_str) = &pending.old_str {
            let existing =
                std::fs::read_to_string(&target).map_err(|e| format!("读取文件失败: {e}"))?;
            if !existing.contains(old_str.as_str()) {
                return Err(format!(
                    "文件中未找到待替换内容（old_str 前 80 字符：{}）",
                    old_str.chars().take(80).collect::<String>()
                ));
            }
            let new_content = existing.replacen(old_str.as_str(), &pending.content, 1);
            std::fs::write(&target, new_content).map_err(|e| format!("写盘失败: {e}"))?;
            "编辑"
        } else {
            std::fs::write(&target, &pending.content).map_err(|e| format!("写盘失败: {e}"))?;
            "写入"
        };

        if let Some(db_path) = self.outbox_db_path() {
            let ts = current_timestamp_ms();
            let _ = crate::db::append_agent_run_event(
                &db_path, run_id, "info",
                &format!("✅ 审批通过：已{op_desc} {}", pending.resolved_path), &ts,
            );
            let _ = crate::db::complete_agent_run(&db_path, run_id, "applied", &ts);
        }
        Ok(format!("已{op_desc}: {}", pending.resolved_path))
    }

    /// 拒绝审批写入，不写盘，可测试。
    pub(crate) fn reject_agent_write_impl(&self, run_id: i64) -> Result<String, String> {
        let pending = self
            .take_pending_agent_write(run_id)
            .ok_or_else(|| format!("run #{run_id} 无待审批写入"))?;
        if let Some(db_path) = self.outbox_db_path() {
            let ts = current_timestamp_ms();
            let _ = crate::db::append_agent_run_event(
                &db_path, run_id, "warn",
                &format!("🚫 审批拒绝：已取消写入 {}", pending.resolved_path), &ts,
            );
        }
        Ok(format!("已取消写入: {}", pending.resolved_path))
    }

    /// 追加 outbox 事件，失败仅记录日志，不中断主流程。
    fn record_outbox_event(&self, event_type: &str, payload: serde_json::Value) {
        let Some(db_path) = self.outbox_db_path() else {
            return;
        };
        let payload_json = match serde_json::to_string(&payload) {
            Ok(value) => value,
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("序列化 outbox 事件失败: {}", err));
                return;
            }
        };
        if let Err(err) =
            db::append_outbox_event(&db_path, event_type, &payload_json, &current_timestamp_ms())
        {
            self.push_log(LogLevel::Warn, format!("写入 outbox 事件失败: {}", err));
        }
    }

    /// 同步入队一条 ingest 任务，返回新 id。
    pub fn enqueue_ingest(&self, source_type: String, source_path: String) -> Result<i64, String> {
        ingest_service::enqueue_ingest(self, source_type, source_path)
    }

    pub fn list_ingest_queue(&self) -> Result<Vec<crate::models::IngestQueueItem>, String> {
        ingest_service::list_ingest_queue(self)
    }

    pub fn cancel_ingest_item(&self, id: i64) -> Result<(), String> {
        ingest_service::cancel_ingest_item(self, id)
    }

    pub fn retry_ingest_item(&self, id: i64) -> Result<(), String> {
        ingest_service::retry_ingest_item(self, id)
    }

    pub fn delete_ingest_item(&self, id: i64) -> Result<(), String> {
        ingest_service::delete_ingest_item(self, id)
    }

    pub fn get_page_embedding_similarities(
        &self,
        paths: Vec<String>,
    ) -> Result<std::collections::HashMap<String, f64>, String> {
        ingest_service::get_page_embedding_similarities(self, paths)
    }

    pub async fn get_embed_status(&self) -> crate::models::EmbedStatus {
        let provider = self.get_embed_provider();
        let backend_id = provider.backend_id();
        let dimension = provider.dimension();
        let healthy = provider.health_check().await;
        let indexed_count = self
            .outbox_db_path()
            .and_then(|p| db::count_embeddings(&p).ok())
            .unwrap_or(0);
        crate::models::EmbedStatus {
            backend_id,
            dimension,
            indexed_count,
            healthy,
        }
    }

    pub async fn rebuild_embeddings(&self) -> Result<usize, String> {
        wiki_service::rebuild_embeddings(self).await
    }

    pub fn start_queue_worker(handle: tauri::AppHandle) {
        ingest_service::start_queue_worker(handle)
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
        research_service::start_research(self, app_handle, topic, depth, breadth)
    }

    pub fn list_research_tasks(&self) -> Result<Vec<crate::models::ResearchTaskItem>, String> {
        research_service::list_research_tasks(self)
    }

    pub fn get_research_task(
        &self,
        id: i64,
    ) -> Result<Option<crate::models::ResearchTaskItem>, String> {
        research_service::get_research_task(self, id)
    }

    pub fn cancel_research_task(&self, id: i64) -> Result<(), String> {
        research_service::cancel_research_task(self, id)
    }

    pub async fn delete_research_task(
        &self,
        id: i64,
        delete_saved_wiki: bool,
    ) -> Result<(), String> {
        research_service::delete_research_task(self, id, delete_saved_wiki).await
    }

    /// 执行 Shell 命令（Windows: PowerShell；其他: bash）并返回执行结果。
    /// H6-S1.5：增加最小策略分级（来源 + action + decision），为后续 agent 审批门做准备。
    pub async fn run_shell_impl(
        &self,
        command: String,
        timeout_ms: u64,
        source: Option<String>,
        session_id: Option<String>,
        stream_id: Option<String>,
    ) -> Result<ShellResult, String> {
        shell_service::run_shell_impl(self, command, timeout_ms, source, session_id, stream_id).await
    }

    pub fn create_shell_session_impl(
        &self,
        source: Option<String>,
    ) -> Result<crate::models::ShellSessionInfo, String> {
        shell_service::create_shell_session_impl(self, source)
    }

    pub fn close_shell_session_impl(&self, session_id: String) -> bool {
        shell_service::close_shell_session_impl(self, session_id)
    }

    pub async fn approve_and_run_shell_impl(
        &self,
        command: String,
        timeout_ms: u64,
        session_id: Option<String>,
        stream_id: Option<String>,
    ) -> Result<ShellResult, String> {
        shell_service::approve_and_run_shell_impl(self, command, timeout_ms, session_id, stream_id).await
    }

    pub fn list_shell_audit_events_impl(
        &self,
        limit: i64,
    ) -> Result<Vec<ShellAuditEvent>, String> {
        shell_service::list_shell_audit_events_impl(self, limit)
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

pub(crate) fn current_timestamp_ms() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

#[derive(Debug, Clone)]
pub(crate) struct WikiMatch {
    pub page_path: String,
    pub score: usize,
    pub excerpt: String,
}

pub(crate) fn resolve_existing_wiki_page_path(
    vault_path: &Path,
    raw_path: &str,
) -> Result<PathBuf, String> {
    let wiki_root = vault_path.join("wiki");
    let candidate = wiki_service::resolve_wiki_page_candidate(vault_path, raw_path)?;
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



/// 将 run 记忆 + 全局记忆格式化为 prompt 注入字符串。
fn format_memories_for_prompt(
    run_mems: &[db::AgentMemoryRecord],
    global_mems: &[db::AgentMemoryRecord],
) -> String {
    let mut lines = Vec::new();
    for m in run_mems {
        lines.push(format!("[run] {}: {}", m.memory_key, m.memory_value));
    }
    for m in global_mems {
        lines.push(format!("[global] {}: {}", m.memory_key, m.memory_value));
    }
    lines.join("\n")
}
/// 将主题字符串转换为有效的 wiki 文件 slug（小写、非 ASCII 字母数字转连字符、去重、最长 60 字符）。
pub(crate) fn topic_to_slug(topic: &str) -> String {
    let raw: String = topic
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
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
    slug.chars().take(60).collect()
}


#[cfg(test)]
mod tests {
    use super::*;
    use super::test_helpers::*;
    use std::fs;

    #[test]
    fn create_wiki_page_slug_generation_works() {
        // 中文部分被转为连字符，最终 trim_matches('-') 去掉尾部连字符
        assert_eq!(topic_to_slug("Rust 语言"), "rust");
        assert_eq!(topic_to_slug("  Hello World  "), "hello-world");
        // "C++" 中 '+' 转为 '-', "编程" 转为 '-', trim 后尾部连字符被去掉
        assert_eq!(topic_to_slug("C++ 编程"), "c");
        // slug 超长截断（>60字符）
        let long = "a".repeat(100);
        assert!(topic_to_slug(&long).len() <= 60);
        // 纯 ASCII 字母数字加连字符
        assert_eq!(topic_to_slug("rust-lang"), "rust-lang");
        // 全空白
        assert_eq!(topic_to_slug("   "), "");
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

    // ── NoopEmbedder 行为测试 ────────────────────────────────────────────────────

    #[tokio::test]
    async fn noop_embedder_embed_returns_unavailable_error() {
        let noop = NoopEmbedder;
        let result = noop.embed("hello").await;
        assert!(
            matches!(result, Err(crate::llm::EmbedError::Unavailable)),
            "NoopEmbedder.embed 应返回 Unavailable，实际: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn noop_embedder_health_check_returns_false() {
        let noop = NoopEmbedder;
        assert!(!noop.health_check().await, "NoopEmbedder.health_check 应返回 false");
    }

    #[test]
    fn noop_embedder_dimension_is_zero() {
        let noop = NoopEmbedder;
        assert_eq!(noop.dimension(), 0, "NoopEmbedder.dimension 应为 0");
    }

    #[test]
    fn noop_embedder_backend_id_is_noop() {
        let noop = NoopEmbedder;
        assert_eq!(noop.backend_id(), "noop", "NoopEmbedder.backend_id 应为 \"noop\"");
    }

    // ── init_embed_provider 路由测试 ─────────────────────────────────────────────

    #[test]
    fn init_embed_provider_disabled_backend_uses_noop() {
        let vault_dir = make_temp_dir("embed-disabled");
        let _guard = TempDirGuard(vault_dir.clone());
        let mut state = make_test_state_bare(&vault_dir);
        // 注入 embed_backend = "disabled"
        state.inner.lock().expect("锁失败").embed_backend = Some("disabled".to_string());
        state.init_embed_provider();
        let provider = state.get_embed_provider();
        assert_eq!(provider.backend_id(), "noop", "disabled 后端应路由到 NoopEmbedder");
        assert_eq!(provider.dimension(), 0);
    }

    #[test]
    fn init_embed_provider_onnx_missing_model_falls_back_to_noop() {
        let vault_dir = make_temp_dir("embed-onnx-missing");
        let _guard = TempDirGuard(vault_dir.clone());
        let mut state = make_test_state_bare(&vault_dir);
        {
            let mut guard = state.inner.lock().expect("锁失败");
            guard.embed_backend = Some("onnx".to_string());
            guard.embed_onnx_model = Some("nonexistent-model-xyz".to_string());
        }
        state.init_embed_provider();
        let provider = state.get_embed_provider();
        assert_eq!(provider.backend_id(), "noop", "模型文件缺失时应回退到 NoopEmbedder");
    }

    #[test]
    fn get_embed_provider_returns_noop_when_not_initialized() {
        let vault_dir = make_temp_dir("embed-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        // 未调用 init_embed_provider，expect noop 默认值
        let provider = state.get_embed_provider();
        assert_eq!(provider.backend_id(), "noop");
    }

    // ── get_embed_status 测试 ────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_embed_status_returns_sensible_noop_status() {
        let vault_dir = make_temp_dir("embed-status");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("Vault 初始化失败");
        state.inner.lock().expect("锁失败").embed_backend = Some("disabled".to_string());
        state.init_embed_provider();
        let status = state.get_embed_status().await;
        assert_eq!(status.backend_id, "noop");
        assert_eq!(status.dimension, 0);
        assert!(!status.healthy);
    }

    // ── count_embeddings DB 测试 ─────────────────────────────────────────────────

    #[test]
    fn count_embeddings_empty_db_returns_zero() {
        let vault_dir = make_temp_dir("embed-count");
        let _guard = TempDirGuard(vault_dir.clone());
        let db_path = vault_dir.join("wiki.db");
        let count = crate::db::count_embeddings(&db_path).expect("count_embeddings 失败");
        assert_eq!(count, 0, "空库应返回 0");
    }

    #[test]
    fn count_embeddings_after_upsert_returns_correct_count() {
        let vault_dir = make_temp_dir("embed-count2");
        let _guard = TempDirGuard(vault_dir.clone());
        let db_path = vault_dir.join("wiki.db");
        crate::db::upsert_embedding(&db_path, "wiki/a.md", &[0.1f32, 0.2, 0.3], "test-backend", 3)
            .expect("upsert 失败");
        crate::db::upsert_embedding(&db_path, "wiki/b.md", &[0.4f32, 0.5, 0.6], "test-backend", 3)
            .expect("upsert 失败");
        let count = crate::db::count_embeddings(&db_path).expect("count 失败");
        assert_eq!(count, 2, "两次 upsert 后应为 2");
    }

    #[test]
    fn count_embeddings_upsert_same_path_deduplicates() {
        let vault_dir = make_temp_dir("embed-count3");
        let _guard = TempDirGuard(vault_dir.clone());
        let db_path = vault_dir.join("wiki.db");
        crate::db::upsert_embedding(&db_path, "wiki/a.md", &[0.1f32], "test", 1)
            .expect("upsert 1 失败");
        crate::db::upsert_embedding(&db_path, "wiki/a.md", &[0.2f32], "test", 1)
            .expect("upsert 2 失败");
        let count = crate::db::count_embeddings(&db_path).expect("count 失败");
        assert_eq!(count, 1, "同路径 upsert 两次应去重为 1");
    }

}
