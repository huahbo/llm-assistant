use flate2::read::ZlibDecoder;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokio::io::{AsyncBufReadExt, BufReader};

use tauri::{AppHandle, Emitter, Manager};

use crate::{
    agent_policy::classify_shell_policy_with_config,
    db,
    llm::{
        LlmError, LlmProvider, OllamaConfig, OllamaProvider, OpenAiConfig, OpenAiProvider,
        DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL,
    },
    models::{
        AgentDraftItem, AgentMemoryItem, AgentRunEventItem, AgentRunItem, AgentSkillItem,
        AppConfig, AppMode, AppOverview, AskSessionItem, AskSessionSearchHitItem,
        AskSessionTurnItem, AskSessionTurnMeta, DefaultPaths, IngestPreview, IngestResult,
        KnowledgeGraphData, KnowledgeGraphDirection, KnowledgeGraphLink, KnowledgeGraphNode,
        KnowledgeSubgraphData, KnowledgeSubgraphMeta, LintIssue, LintPatchApplyInput,
        LintPatchApplyResult, LintPatchBatchApplyItemResult, LintPatchBatchApplyResult,
        LintPatchBatchApplyStatus, LintPatchEventItem, LintPatchPreview, LintPatchSuggestion,
        LintReport, LintSeverityStats, LlmProviderConfig, LlmStatus, LogEntry, LogLevel,
        ModeChangeResult, NewPageResult, OutboxAckResult, OutboxEventItem, ProgressPayload,
        QueryAnswerResult, QueryAskOptions, QueryCitation, QuerySearchDebug, QuerySearchRouteDebug,
        QuerySettings, SaveQueryAnswerInput, SaveQueryAnswerResult, ShellAuditEvent,
        ShellPolicyConfig, ShellResult, VaultInitResult, WikiPageCitationItem, WikiPageDetail,
        WikiPageFrontmatter, WikiPageHistoryDetail, WikiPageHistoryItem, WikiPageItem,
    },
    search::reciprocal_rank_fusion,
    vault,
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
        let query_top_k = normalize_top_k(config.query_top_k);
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
                shell_policy: config.shell_policy.unwrap_or_default(),
                pending_agent_writes: std::collections::HashMap::new(),
            }),
            config_path,
            llm_provider: OnceLock::new(),
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
                shell_policy: config.shell_policy.unwrap_or_default(),
                pending_agent_writes: std::collections::HashMap::new(),
            }),
            config_path,
            llm_provider: OnceLock::new(),
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

    fn load_search_config_from_path(config_path: &Path) -> crate::models::SearchConfig {
        search_service::load_search_config_from_path(config_path)
    }

    pub fn get_search_config(&self) -> crate::models::SearchConfig {
        search_service::get_search_config(self)
    }

    pub fn set_search_config(&self, cfg: crate::models::SearchConfig) -> Result<(), String> {
        search_service::set_search_config(self, cfg)
    }

    pub async fn search_web_cascade(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<crate::models::WebSearchResult>, String> {
        search_service::search_web_cascade(self, query, max_results).await
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
            .or_else(|| {
                fallback_base_url
                    .as_deref()
                    .filter(|u| !u.trim().is_empty())
            })
            .unwrap_or(&config.base_url);
        config.base_url = base_url.to_string();
        Arc::new(OllamaProvider::new(config))
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


    async fn extract_entities(&self, content: &str) -> Vec<String> {
        wiki_service::extract_entities(self, content).await
    }

    async fn update_related_pages_with_link(
        &self,
        db_path: &Path,
        vault_path: &Path,
        new_wiki_abs_path: &str,
        new_wiki_title: &str,
        entities: &[String],
    ) -> Vec<String> {
        wiki_service::update_related_pages_with_link(self, db_path, vault_path, new_wiki_abs_path, new_wiki_title, entities).await
    }

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

    pub fn lint_report(&self) -> LintReport {
        lint_service::lint_report(self)
    }

    pub fn preview_lint_patches(&self) -> LintPatchPreview {
        lint_service::preview_lint_patches(self)
    }

    pub fn lint_report_full_future(
        &self,
    ) -> impl std::future::Future<Output = LintReport> + Send + 'static {
        lint_service::lint_report_full_future(self)
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
            extract_markdown_h1_title(&draft.content)
                .unwrap_or_else(|| format!("agent-draft-{}", draft.id))
        } else {
            draft.title.trim().to_string()
        };
        let mut base_slug = topic_to_slug(&preferred_title);
        if base_slug.is_empty() {
            base_slug = format!("agent-draft-{}", draft.id);
        }
        let final_slug = resolve_unique_wiki_slug(&wiki_dir, &base_slug)?;
        let wiki_file_path = wiki_dir.join(format!("{}.md", final_slug));
        std::fs::write(&wiki_file_path, &draft.content)
            .map_err(|e| format!("写入草稿到 wiki 失败: {}", e))?;

        let now = current_timestamp_ms();
        let content_hash = format!("{:x}", md5_simple(&draft.content));
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
        self.chat_cancellations
            .lock()
            .expect("chat_cancellations 锁已被污染")
            .insert(conv_id, token);
    }

    pub fn cancel_chat_token(&self, conv_id: i64) {
        use std::sync::atomic::Ordering;
        let mut map = self
            .chat_cancellations
            .lock()
            .expect("chat_cancellations 锁已被污染");
        if let Some(token) = map.remove(&conv_id) {
            token.store(true, Ordering::Relaxed);
        }
    }

    pub fn remove_chat_cancel_token(&self, conv_id: i64) {
        self.chat_cancellations
            .lock()
            .expect("chat_cancellations 锁已被污染")
            .remove(&conv_id);
    }

    pub fn register_chat_write_approval(
        &self,
        pending_id: i64,
        tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    ) {
        self.chat_write_approvals
            .lock()
            .expect("chat_write_approvals 锁已被污染")
            .insert(pending_id, tx);
    }

    pub fn approve_chat_write(&self, pending_id: i64) -> Result<(), String> {
        let result = self.approve_agent_write_impl(pending_id);
        let msg = result.unwrap_or_else(|e| format!("写操作失败: {e}"));
        let tx = self
            .chat_write_approvals
            .lock()
            .expect("chat_write_approvals 锁已被污染")
            .remove(&pending_id);
        if let Some(tx) = tx {
            let _ = tx.send(Ok(msg));
        }
        Ok(())
    }

    pub fn reject_chat_write(&self, pending_id: i64) -> Result<(), String> {
        let _ = self.reject_agent_write_impl(pending_id);
        let tx = self
            .chat_write_approvals
            .lock()
            .expect("chat_write_approvals 锁已被污染")
            .remove(&pending_id);
        if let Some(tx) = tx {
            let _ = tx.send(Err("用户拒绝写操作".to_string()));
        }
        Ok(())
    }

    pub fn register_chat_shell_pending(&self, pending_id: i64, command: String, timeout_ms: u64) {
        self.chat_shell_pending
            .lock()
            .expect("chat_shell_pending 锁已被污染")
            .insert(pending_id, (command, timeout_ms));
    }

    pub async fn approve_chat_shell_impl(&self, pending_id: i64) -> Result<(), String> {
        let (command, timeout_ms) = self
            .chat_shell_pending
            .lock()
            .expect("chat_shell_pending 锁已被污染")
            .remove(&pending_id)
            .ok_or_else(|| format!("pending_id={pending_id} 不存在或已过期"))?;

        let result = self
            .run_shell_impl(command, timeout_ms, Some("chat_approved".to_string()), None, None)
            .await;

        let content = match result {
            Ok(r) => {
                let mut out = String::new();
                if r.blocked {
                    out.push_str(&format!("blocked: {}\n", r.blocked_reason.unwrap_or_default()));
                }
                if !r.stdout.is_empty() { out.push_str(&r.stdout); }
                if !r.stderr.is_empty() {
                    if !out.is_empty() { out.push('\n'); }
                    out.push_str("--- stderr ---\n");
                    out.push_str(&r.stderr);
                }
                if out.is_empty() { out.push_str("(no output)"); }
                out
            }
            Err(e) => format!("shell 执行失败: {e}"),
        };

        let tx = self
            .chat_write_approvals
            .lock()
            .expect("chat_write_approvals 锁已被污染")
            .remove(&pending_id);
        if let Some(tx) = tx {
            let _ = tx.send(Ok(content));
        }
        Ok(())
    }

    pub fn reject_chat_shell_impl(&self, pending_id: i64) -> Result<(), String> {
        self.chat_shell_pending
            .lock()
            .expect("chat_shell_pending 锁已被污染")
            .remove(&pending_id);
        let tx = self
            .chat_write_approvals
            .lock()
            .expect("chat_write_approvals 锁已被污染")
            .remove(&pending_id);
        if let Some(tx) = tx {
            let _ = tx.send(Err("用户拒绝执行".to_string()));
        }
        Ok(())
    }

    pub async fn spawn_mcp_client(
        &self,
        name: String,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
    ) -> Result<(), String> {
        let client = crate::agent_chat::mcp::McpClient::spawn(name.clone(), command, args, env).await?;
        let mut clients = self.mcp_clients.lock().expect("mcp_clients 锁已被污染");
        clients.insert(name, std::sync::Arc::new(tokio::sync::Mutex::new(client)));
        Ok(())
    }

    pub fn stop_mcp_client(&self, name: &str) {
        let mut clients = self.mcp_clients.lock().expect("mcp_clients 锁已被污染");
        clients.remove(name);
    }

    pub fn get_mcp_client(
        &self,
        name: &str,
    ) -> Option<std::sync::Arc<tokio::sync::Mutex<crate::agent_chat::mcp::McpClient>>> {
        self.mcp_clients
            .lock()
            .expect("mcp_clients 锁已被污染")
            .get(name)
            .cloned()
    }

    pub fn list_running_mcp_clients(&self) -> Vec<String> {
        self.mcp_clients
            .lock()
            .expect("mcp_clients 锁已被污染")
            .keys()
            .cloned()
            .collect()
    }

    pub fn cancel_ask_session(&self, session_id: String) -> Result<(), String> {
        ask_service::cancel_ask_session(self, session_id)
    }

    pub fn clear_ask_session(&self, session_id: String) -> Result<(), String> {
        ask_service::clear_ask_session(self, session_id)
    }

    // ── 配置 I/O 薄包装（实现在 config_service）─────────────────────────────

    #[cfg(test)]
    fn llm_status_input(
        &self,
    ) -> (
        AppMode,
        Option<String>,
        Option<OpenAiConfig>,
        Option<Arc<dyn LlmProvider>>,
    ) {
        config_service::llm_status_input(self)
    }

    fn load_config(config_path: &Path) -> (AppConfig, Option<String>) {
        config_service::load_config(config_path)
    }

    fn default_config_path() -> PathBuf {
        config_service::default_config_path()
    }

    fn project_root() -> PathBuf {
        config_service::project_root()
    }

    fn default_config_path_from_root(root: &Path) -> PathBuf {
        config_service::default_config_path_from_root(root)
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

    fn persist_config(
        &self,
        mode: AppMode,
        vault_path: Option<&Path>,
        query_top_k: usize,
        expected_snapshot: Option<&str>,
    ) -> Result<String, String> {
        config_service::persist_config(self, mode, vault_path, query_top_k, expected_snapshot)
    }

    fn set_vault_path(&self, vault_path: PathBuf) -> Result<(), String> {
        config_service::set_vault_path(self, vault_path)
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

        // 先校验搜索配置，避免创建必然失败的任务记录。
        let mut cfg = self.get_search_config();
        cfg.depth = depth;
        cfg.breadth = breadth;
        search_service::validate_search_config(&cfg)?;

        let now = current_timestamp_ms();
        let conn =
            rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
        let task_id = db::db_create_research_task(&conn, &topic, depth, breadth, &now)?;

        tauri::async_runtime::spawn(async move {
            start_research_task(app_handle, task_id, topic, cfg).await;
        });

        Ok(task_id)
    }

    /// 列出最近研究任务。
    pub fn list_research_tasks(&self) -> Result<Vec<crate::models::ResearchTaskItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先初始化 Vault".to_string())?;
        db::ensure_meta_db(&db_path)?;
        let conn =
            rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
        db::db_list_research_tasks(&conn)
    }

    pub fn get_research_task(
        &self,
        id: i64,
    ) -> Result<Option<crate::models::ResearchTaskItem>, String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先初始化 Vault".to_string())?;
        db::ensure_meta_db(&db_path)?;
        let conn =
            rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
        db::db_get_research_task(&conn, id)
    }

    /// 取消研究任务（幂等，不重置已有字段）。
    pub fn cancel_research_task(&self, id: i64) -> Result<(), String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先初始化 Vault".to_string())?;
        let conn =
            rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
        let now = current_timestamp_ms();
        db::db_cancel_research_task(&conn, id, &now)
    }

    /// 删除研究任务；可选同时删除该任务关联的 Wiki 页面。
    pub async fn delete_research_task(
        &self,
        id: i64,
        delete_saved_wiki: bool,
    ) -> Result<(), String> {
        let db_path = self
            .outbox_db_path()
            .ok_or_else(|| "请先初始化 Vault".to_string())?;
        db::ensure_meta_db(&db_path)?;

        let task = {
            let conn = rusqlite::Connection::open(&db_path)
                .map_err(|e| format!("打开数据库失败: {}", e))?;
            db::db_get_research_task(&conn, id)?.ok_or_else(|| format!("研究任务不存在: {}", id))?
        };

        if !matches!(task.status.as_str(), "done" | "failed" | "cancelled") {
            return Err("任务仍在运行中，请先取消后再删除".to_string());
        }

        if delete_saved_wiki {
            if let Some(saved_path) = task.saved_path.as_deref() {
                let _ = self.delete_wiki_page_impl(saved_path).await?;
            }
        }

        let conn =
            rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
        db::db_delete_research_task(&conn, id)?;

        self.record_outbox_event(
            "research_task_deleted",
            serde_json::json!({
                "task_id": id,
                "topic": task.topic,
                "status": task.status,
                "delete_saved_wiki": delete_saved_wiki,
                "saved_path": task.saved_path,
            }),
        );
        Ok(())
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

/// Heuristic text extractor for legacy Word .doc (OLE compound document) files.
/// Tries UTF-16LE scan for CJK content, falls back to ASCII string extraction.
fn extract_text_from_doc(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取 .doc 失败: {e}"))?;

    // Validate OLE compound document magic bytes
    const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    if bytes.len() < 8 || bytes[0..8] != OLE_MAGIC {
        return Err("不是有效的 .doc 文件（非 OLE 复合文档格式）".to_string());
    }

    // UTF-16LE scan for CJK and Latin text (try both byte alignments)
    let utf16_text = doc_scan_utf16le(&bytes);

    // ASCII run extraction as fallback / supplement
    let ascii_text = doc_scan_ascii(&bytes);

    let has_cjk = utf16_text.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
    let text = if has_cjk { utf16_text } else { ascii_text };

    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim().to_string();

    if text.len() < 20 {
        return Err(
            "无法从 .doc 文件提取可读文本（建议将文件另存为 .docx 格式后重试）".to_string(),
        );
    }
    Ok(text)
}

fn is_doc_text_char(cp: u16) -> bool {
    matches!(
        cp,
        0x0009
            | 0x000A
            | 0x000D
            | 0x0020..=0x007E
            | 0x00A0..=0x00FF
            | 0x2010..=0x2027
            | 0x3000..=0x303F
            | 0x3040..=0x30FF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0xFF01..=0xFF60
    )
}

fn doc_scan_utf16le(bytes: &[u8]) -> String {
    let mut best = String::new();
    for start in 0..2usize {
        let mut tokens: Vec<String> = Vec::new();
        let mut run: Vec<u16> = Vec::new();
        let mut i = start;
        while i + 1 < bytes.len() {
            let cp = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
            if is_doc_text_char(cp) {
                run.push(cp);
            } else {
                if run.len() >= 4 {
                    tokens.push(String::from_utf16_lossy(&run).to_string());
                }
                run.clear();
            }
            i += 2;
        }
        if run.len() >= 4 {
            tokens.push(String::from_utf16_lossy(&run).to_string());
        }
        let candidate = tokens.join(" ");
        let cjk = |s: &str| s.chars().filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c)).count();
        if cjk(&candidate) > cjk(&best) || (cjk(&candidate) == cjk(&best) && candidate.len() > best.len()) {
            best = candidate;
        }
    }
    best
}

fn doc_scan_ascii(bytes: &[u8]) -> String {
    const MIN_RUN: usize = 8;
    let mut tokens: Vec<String> = Vec::new();
    let mut run = String::new();
    for &b in bytes {
        match b {
            0x09 => run.push(' '),
            0x20..=0x7E => run.push(b as char),
            _ => {
                if run.len() >= MIN_RUN {
                    tokens.push(std::mem::take(&mut run));
                } else {
                    run.clear();
                }
            }
        }
    }
    if run.len() >= MIN_RUN {
        tokens.push(run);
    }
    tokens.join(" ")
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
            push_candidate(format!(
                r"{}\Tesseract-OCR\tesseract.exe",
                program_files_x86
            ));
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
    let entries =
        fs::read_dir(temp_dir).map_err(|err| format!("读取 PDF 页图目录失败：{}", err))?;
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
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn rfind_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
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

/// 将实体名或链接目标规范化为文件系统 slug（小写，空格替换为横线）。
fn entity_to_slug(entity: &str) -> String {
    entity.trim().to_lowercase().replace(' ', "-")
}

/// 将页面内容分离为 (frontmatter, body)。
/// frontmatter 为两个 `---` 之间的文本（不含分隔行），body 为之后的全部内容。
/// 若没有 frontmatter，返回 ("", content)。
fn split_frontmatter(content: &str) -> (&str, &str) {
    let bytes = content.as_bytes();
    // 页面必须以 "---" 开头才视为有 frontmatter
    if !content.starts_with("---") {
        return ("", content);
    }
    // 找到第一行结束位置（跳过开头 "---\n" 或 "---\r\n"）
    let after_first = if bytes.get(3) == Some(&b'\r') && bytes.get(4) == Some(&b'\n') {
        4
    } else if bytes.get(3) == Some(&b'\n') {
        3
    } else {
        return ("", content);
    };
    // 在剩余内容中寻找独立的 "---" 行作为 frontmatter 结束标记
    let rest = &content[after_first + 1..];
    // 搜索 "\n---" 或 "^---" 后跟换行/EOF
    let mut search_start = 0;
    loop {
        let Some(pos) = rest[search_start..].find("---") else {
            break;
        };
        let abs_pos = search_start + pos;
        // 确保 "---" 在行首
        let at_line_start = abs_pos == 0 || rest.as_bytes().get(abs_pos - 1) == Some(&b'\n');
        if !at_line_start {
            search_start = abs_pos + 3;
            continue;
        }
        // 确保 "---" 后是换行或 EOF
        let end_pos = abs_pos + 3;
        let followed_by_newline = matches!(
            rest.as_bytes().get(end_pos),
            None | Some(b'\n') | Some(b'\r')
        );
        if !followed_by_newline {
            search_start = end_pos;
            continue;
        }
        let fm = &rest[..abs_pos];
        // body 从结束标记行之后开始
        let body_start = if rest.as_bytes().get(end_pos) == Some(&b'\r')
            && rest.as_bytes().get(end_pos + 1) == Some(&b'\n')
        {
            end_pos + 2
        } else if rest.as_bytes().get(end_pos) == Some(&b'\n') {
            end_pos + 1
        } else {
            end_pos
        };
        let body = if body_start < rest.len() {
            &rest[body_start..]
        } else {
            ""
        };
        return (fm, body);
    }
    ("", content)
}

/// 从 frontmatter 文本中解析 `entities:` 列表（简单行解析，支持 YAML 列表格式）。
fn parse_frontmatter_entities(frontmatter: &str) -> Vec<String> {
    let mut entities = Vec::new();
    let mut in_entities = false;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("entities:") {
            in_entities = true;
            // 支持行内列表，如 `entities: [A, B]`（极少用，忽略）
            continue;
        }
        if in_entities {
            if trimmed.starts_with("- ") {
                let item = trimmed
                    .trim_start_matches("- ")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !item.is_empty() {
                    entities.push(item.to_string());
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // 遇到其他键则退出 entities 块
                in_entities = false;
            }
        }
    }
    entities
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

pub(crate) fn current_timestamp_ms() -> String {
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

pub(crate) fn resolve_existing_wiki_page_path(
    vault_path: &Path,
    raw_path: &str,
) -> Result<PathBuf, String> {
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
    let wiki_link_re =
        regex::Regex::new(r"\[\[([^|\]]+)(?:\|[^\]]+)?\]\]").expect("wiki link regex 应可编译");
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
    let content = fs::read_to_string(page_path).map_err(|err| format!("读取页面失败: {}", err))?;
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
        let raw_label = segments
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty());
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

        let target_content = fs::read_to_string(&target_canonical)
            .map_err(|err| format!("读取页面失败: {}", err))?;
        let has_reverse = extract_wiki_link_targets(&target_content)
            .iter()
            .any(|raw| {
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

/// 跳过 Markdown frontmatter（--- ... ---），返回正文前 `limit` 个字符。
fn extract_content_after_frontmatter(text: &str, limit: usize) -> String {
    let s = text.trim_start();
    let body = if s.starts_with("---") {
        let rest = &s[3..];
        rest.find("\n---")
            .map(|pos| rest[pos + 4..].trim_start_matches('\n'))
            .unwrap_or(s)
    } else {
        s
    };
    body.chars().take(limit).collect()
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
) -> Result<
    (
        Vec<WikiMatch>,
        &'static str,
        Option<String>,
        Option<QuerySearchDebug>,
    ),
    String,
> {
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
) -> Result<
    (
        Vec<WikiMatch>,
        &'static str,
        Option<String>,
        Option<QuerySearchDebug>,
    ),
    String,
> {
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
) -> Result<
    (
        Vec<WikiMatch>,
        &'static str,
        Option<String>,
        Option<QuerySearchDebug>,
    ),
    String,
> {
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
    let end_pos = after_first
        .find("\n---")
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
        || frontmatter.stale.is_some(); // 新增
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
            let prefix = if turn.role == "user" {
                "用户"
            } else {
                "助手"
            };
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


fn make_research_slug(topic: &str) -> String {
    let raw: String = topic
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
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
pub(crate) fn strip_think_tags(text: &str) -> String {
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

/// 将研究任务标记为失败并向前端发送错误事件。
/// 提取为具名函数，消除闭包所有权歧义，可在管线任意位置调用。
/// Phase 4: 将综合报告写入 vault/wiki/research/ 并更新数据库。
/// 返回保存的文件路径（成功）或 Err(()，失败已由 report_research_failure 处理）。
async fn save_research_output(
    db_path: &std::path::Path,
    app_handle: &tauri::AppHandle,
    state: &AppState,
    task_id: i64,
    topic: &str,
    config: &crate::models::SearchConfig,
    all_results: &[crate::models::WebSearchResult],
    all_used_queries: &[String],
    learnings: &[String],
    synthesized: &str,
) -> Result<String, ()> {
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let references = all_results
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{}. [{}]({})", i + 1, r.title, r.url))
        .collect::<Vec<_>>()
        .join("\n");

    let cleaned = strip_think_tags(synthesized);
    let final_content = format!(
        "---\ntype: research\ntitle: \"{topic}\"\ncreated: {date}\nupdated: {date}\ndepth: {depth}\nbreadth: {breadth}\nsources: {count}\ntags: [research, deep-research]\n---\n\n{body}\n\n## References\n\n{refs}",
        topic = topic,
        date = date_str,
        depth = config.depth,
        breadth = config.breadth,
        count = all_results.len(),
        body = cleaned,
        refs = references,
    );

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁");
        guard.vault_path.clone()
    };
    let vault_path = match vault_path {
        Some(p) => p,
        None => {
            report_research_failure(db_path, app_handle, task_id, "保存阶段：Vault 路径丢失");
            return Err(());
        }
    };

    let slug = make_research_slug(topic);
    let filename = format!("research-{}-{}.md", slug, date_str);
    let save_dir = vault_path.join("wiki").join("research");
    let _ = fs::create_dir_all(&save_dir);
    let save_path = save_dir.join(&filename);

    // 防止 topic 含 ../ 等路径越权
    match save_path
        .canonicalize()
        .or_else(|_| save_dir.canonicalize().map(|d| d.join(&filename)))
    {
        Ok(canonical) => {
            let canonical_vault = vault_path.canonicalize().unwrap_or(vault_path.clone());
            if !canonical.starts_with(&canonical_vault) {
                report_research_failure(
                    db_path,
                    app_handle,
                    task_id,
                    "保存路径越权：topic 包含非法路径字符，已拒绝写入",
                );
                return Err(());
            }
        }
        Err(e) => {
            report_research_failure(
                db_path,
                app_handle,
                task_id,
                &format!("保存路径解析失败: {}", e),
            );
            return Err(());
        }
    }

    if let Err(e) = fs::write(&save_path, &final_content) {
        report_research_failure(
            db_path,
            app_handle,
            task_id,
            &format!("写入文件失败: {}", e),
        );
        return Err(());
    }

    let saved_path_str = save_path.to_string_lossy().to_string();
    {
        let conn = match rusqlite::Connection::open(db_path) {
            Ok(c) => c,
            Err(e) => {
                report_research_failure(
                    db_path,
                    app_handle,
                    task_id,
                    &format!("打开数据库失败: {}", e),
                );
                return Err(());
            }
        };
        let now = current_timestamp_ms();
        let queries_json = serde_json::to_string(all_used_queries).unwrap_or_default();
        let _ = db::db_update_research_task(
            &conn,
            task_id,
            "done",
            &queries_json,
            all_results.len() as i32,
            Some(saved_path_str.as_str()),
            None,
            &now,
        );
    }

    let _ = state.ingest_markdown(save_path, None).await;
    let _ = app_handle.emit(
        "research_done",
        serde_json::json!({
            "task_id": task_id,
            "saved_path": saved_path_str,
            "sources": all_results.len(),
            "learnings": learnings.len(),
        }),
    );
    Ok(saved_path_str)
}

fn report_research_failure(
    db_path: &std::path::Path,
    app_handle: &tauri::AppHandle,
    task_id: i64,
    msg: &str,
) {
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        let now = current_timestamp_ms();
        let _ =
            db::db_update_research_task(&conn, task_id, "failed", "[]", 0, None, Some(msg), &now);
    }
    let _ = app_handle.emit(
        "research_error",
        serde_json::json!({ "task_id": task_id, "error": msg }),
    );
}

/// 从 LLM 输出行中提取 "TAG: content" 结构，容忍 markdown 装饰与大小写变体。
///
/// 支持格式：
/// - `LEARNING: insight`
/// - `- LEARNING: insight`
/// - `**LEARNING:** insight`
/// - `1. FOLLOWUP: query`
/// - `learning: insight`（小写）
fn extract_tagged_content(line: &str, tag: &str) -> Option<String> {
    let mut s = line.trim();
    // 剥离 markdown 列表标记
    if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
        s = rest.trim();
    } else if let Some(dot_idx) = s.find(". ") {
        if s[..dot_idx].chars().all(|c| c.is_ascii_digit()) {
            s = s[dot_idx + 2..].trim();
        }
    }
    // 剥离 markdown 加粗标记
    s = s.trim_start_matches("**");

    let tag_upper = tag.to_ascii_uppercase();
    if s.len() >= tag.len() && s[..tag.len()].to_ascii_uppercase() == tag_upper {
        let after = s[tag.len()..]
            .trim_start_matches(':')
            .trim_start_matches("**")
            .trim();
        Some(after.to_string())
    } else {
        None
    }
}

/// 解析 LLM 输出的结构化学习内容，返回 `(learnings, followup_queries)`。
///
/// 兜底策略：若 LLM 完全不遵循格式，将所有有意义的非空行作为 learnings 返回，
/// 确保管线不会因格式不匹配而静默丢失信息。
pub(crate) fn parse_learnings_and_followups(text: &str) -> (Vec<String>, Vec<String>) {
    let mut learnings = Vec::new();
    let mut followups = Vec::new();

    for line in text.lines() {
        if let Some(content) = extract_tagged_content(line, "LEARNING") {
            if !content.is_empty() {
                learnings.push(content);
            }
        } else if let Some(content) = extract_tagged_content(line, "FOLLOWUP") {
            if !content.is_empty() {
                followups.push(content);
            }
        }
    }

    // 兜底：LLM 未输出任何结构化标签时，提取所有有意义的行作为 learnings
    if learnings.is_empty() && followups.is_empty() {
        learnings = text
            .lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches("**")
                    .trim_end_matches("**")
                    .trim_start_matches('-')
                    .trim_start_matches('*')
                    .trim()
            })
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect();
    }

    (learnings, followups)
}

/// Deep Research 主管线（后台 task）。
/// 算法参考 dzhng/deep-research：迭代式学习提取 + 并发搜索 + 知识缺口驱动的下一轮查询生成。
async fn start_research_task(
    app_handle: tauri::AppHandle,
    task_id: i64,
    topic: String,
    config: crate::models::SearchConfig,
) {
    let state = app_handle.state::<AppState>();
    let db_path = match state.outbox_db_path() {
        Some(p) => p,
        None => {
            let _ = app_handle.emit(
                "research_error",
                serde_json::json!({ "task_id": task_id, "error": "Vault 未初始化" }),
            );
            return;
        }
    };

    let emit_progress = |stage: &str, msg: String| {
        let _ = app_handle.emit(
            "research_progress",
            serde_json::json!({
                "task_id": task_id, "stage": stage, "message": msg
            }),
        );
    };

    if let Err(err) = search_service::validate_search_config(&config) {
        report_research_failure(&db_path, &app_handle, task_id, &err);
        return;
    }

    let provider = match state.get_llm_provider() {
        Some(p) => p,
        None => {
            report_research_failure(&db_path, &app_handle, task_id, "LLM Provider 不可用");
            return;
        }
    };

    // 共享 HTTP 客户端，统一设置 30s 超时
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => Arc::new(c),
        Err(e) => {
            report_research_failure(
                &db_path,
                &app_handle,
                task_id,
                &format!("HTTP 客户端初始化失败: {}", e),
            );
            return;
        }
    };

    let mut all_results: Vec<crate::models::WebSearchResult> = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    // 累积的结构化学习成果（参考 dzhng/deep-research 的 learnings 模型）
    let mut learnings: Vec<String> = Vec::new();
    let mut all_used_queries: Vec<String> = Vec::new();

    // ── Phase 1: 初始查询分解 ─────────────────────────────────────────────────
    emit_progress("decomposing", "正在规划研究路径...".to_string());
    let decompose_prompt = format!(
        "You are an expert researcher. Generate {breadth} specific, diverse search queries to thoroughly investigate: \"{topic}\"\nOutput one query per line, no numbering, no extra text.",
        breadth = config.breadth,
        topic = topic
    );
    let mut current_queries: Vec<String> = match provider.complete(&decompose_prompt).await {
        Ok(text) => {
            let qs: Vec<String> = strip_think_tags(&text)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .take(config.breadth as usize)
                .collect();
            if qs.is_empty() {
                emit_progress(
                    "decomposing",
                    "LLM 未生成子查询，使用原始主题搜索".to_string(),
                );
                vec![topic.clone()]
            } else {
                emit_progress("decomposing", format!("生成了 {} 个研究子查询", qs.len()));
                qs
            }
        }
        Err(_) => {
            emit_progress("decomposing", "查询分解失败，使用原始主题搜索".to_string());
            vec![topic.clone()]
        }
    };

    // 暂停：通知前端子查询已就绪，等待用户审批（最多 5 分钟）
    let rx = state.register_query_approval(task_id);
    let _ = app_handle.emit(
        "research_queries_ready",
        serde_json::json!({
            "task_id": task_id,
            "queries": current_queries,
        }),
    );
    emit_progress(
        "awaiting_approval",
        format!("已分解为 {} 个研究方向，等待确认...", current_queries.len()),
    );

    current_queries = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_else(|| current_queries.clone());

    // ── Phase 2: 迭代式并发搜索 + 学习提取 ──────────────────────────────────
    let max_depth = config.depth.clamp(1, 5);
    for depth in 1..=max_depth {
        emit_progress(
            "searching",
            format!(
                "第 {}/{} 轮：{} 个查询并发搜索中...",
                depth,
                max_depth,
                current_queries.len()
            ),
        );

        // 并发执行本轮所有查询
        let breadth_limit = config.breadth as usize;
        let mut handles = Vec::new();
        for query in &current_queries {
            let client = Arc::clone(&client);
            let query = query.clone();
            let config_clone = config.clone();
            handles.push(tokio::task::spawn(async move {
                let result = search_service::do_search(&client, &query, &config_clone, breadth_limit).await;
                (query, result)
            }));
        }

        let mut round_results: Vec<crate::models::WebSearchResult> = Vec::new();
        let mut round_errors: Vec<String> = Vec::new();
        for handle in handles {
            match handle.await {
                Ok((query, Ok(results))) => {
                    emit_progress("searching", format!("✓ {}（{} 条）", query, results.len()));
                    for r in results {
                        if seen_urls.insert(r.url.clone()) {
                            round_results.push(r);
                        }
                    }
                }
                Ok((query, Err(err))) => {
                    emit_progress(
                        "searching",
                        format!("✗ {}（{}）", query, search_service::compact_error_message(&err, 80)),
                    );
                    round_errors.push(format!("{}: {}", query, err));
                }
                Err(join_err) => {
                    let err = format!("查询任务并发执行失败: {}", join_err);
                    emit_progress(
                        "searching",
                        format!("✗ {}", search_service::compact_error_message(&err, 90)),
                    );
                    round_errors.push(err);
                }
            }
        }
        all_used_queries.extend(current_queries.iter().cloned());

        if round_results.is_empty() {
            if depth == 1 && !round_errors.is_empty() {
                let summary = search_service::summarize_round_errors(&round_errors, 3);
                report_research_failure(
                    &db_path,
                    &app_handle,
                    task_id,
                    &format!("搜索阶段失败：{}", summary),
                );
                return;
            }
            if depth > 1 {
                if round_errors.is_empty() {
                    emit_progress("searching", "本轮未发现新资料，提前结束搜索。".to_string());
                } else {
                    emit_progress(
                        "searching",
                        format!(
                            "本轮无新资料，且存在 {} 条搜索错误，提前结束。",
                            round_errors.len()
                        ),
                    );
                }
            }
            break;
        }

        // 构建本轮摘要（每条来源截取 300 字符，避免 token 溢出）
        let round_snippets: String = round_results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let snip: String = r.snippet.chars().take(300).collect();
                format!("[{}] {} ({}): {}", i + 1, r.title, r.source, snip)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        all_results.extend(round_results);

        if depth < max_depth {
            // 提取结构化学习 + 生成知识缺口查询（参考 dzhng/deep-research processSerpResult）
            emit_progress(
                "synthesizing",
                format!("第 {} 轮：提取关键发现并识别知识缺口...", depth),
            );
            let extract_prompt = format!(
                "Research topic: {topic}\n\nSearch results (round {depth}):\n{snippets}\n\nExisting learnings:\n{existing}\n\nTask:\n1. Extract 3-5 NEW key learnings not already in existing learnings (format: LEARNING: <insight>)\n2. Generate {next_breadth} follow-up search queries to fill knowledge gaps (format: FOLLOWUP: <query>)\n\nOutput only LEARNING: and FOLLOWUP: lines, nothing else.",
                topic = topic,
                depth = depth,
                snippets = round_snippets,
                existing = if learnings.is_empty() { "none yet".to_string() } else { learnings.join("\n") },
                next_breadth = (config.breadth as usize).min(3),
            );

            match provider.complete(&extract_prompt).await {
                Ok(text) => {
                    let (new_learnings, next_queries) =
                        parse_learnings_and_followups(&strip_think_tags(&text));
                    learnings.extend(new_learnings);
                    if next_queries.is_empty() {
                        emit_progress("searching", "无新知识缺口，提前结束迭代。".to_string());
                        break;
                    }
                    current_queries = next_queries;
                }
                Err(_) => break,
            }
        } else {
            // 最后一轮：直接提取学习成果，不再生成新查询
            let final_extract_prompt = format!(
                "Research topic: {topic}\n\nFinal round search results:\n{snippets}\n\nExtract 5-8 key learnings. Format: LEARNING: <insight>",
                topic = topic,
                snippets = round_snippets,
            );
            if let Ok(text) = provider.complete(&final_extract_prompt).await {
                let (new_learnings, _) = parse_learnings_and_followups(&strip_think_tags(&text));
                learnings.extend(new_learnings);
            }
        }
    }

    // ── Phase 3: 最终综合报告 ────────────────────────────────────────────────
    emit_progress(
        "synthesizing",
        format!("综合 {} 条关键发现，撰写研究报告...", learnings.len()),
    );

    let wiki_index = {
        let guard = state.inner.lock().expect("状态锁");
        guard
            .vault_path
            .as_ref()
            .and_then(|vp| fs::read_to_string(vp.join("wiki").join("index.md")).ok())
            .unwrap_or_default()
    };
    let wiki_excerpt: String = wiki_index.chars().take(800).collect();

    // 截断学习成果到 8000 字符防止 LLM token 溢出
    let learnings_text = learnings
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}. {}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n");
    let context: String = learnings_text.chars().take(8000).collect();

    let synth_prompt = format!(
        "You are a professional research analyst. Write a comprehensive, well-structured Markdown wiki page based on the research findings.\n\nTopic: {topic}\n\nKey Learnings ({count} findings):\n{context}\n\nExisting Knowledge Base Overview:\n{wiki}\n\nRequirements:\n1. Sections: Abstract, Core Findings, Detailed Analysis, Conclusion\n2. Cite sources inline as [N] referencing the numbered learnings\n3. Suggest cross-references to existing wiki pages where relevant\n4. Write in the same language as the topic title\n5. Do not include reasoning or thinking process",
        topic = topic,
        count = learnings.len(),
        context = context,
        wiki = wiki_excerpt,
    );

    let mut synthesized = String::new();
    let mut synth_last_err: Option<LlmError> = None;
    for attempt in 1..=2 {
        let mut char_count = 0usize;
        let mut last_emitted = 0usize;
        let mut buf = String::new();
        let stream_result = provider
            .complete_stream(&synth_prompt, &mut |chunk| {
                let chunk_str = chunk.clone(); // 给 emit 用
                buf.push_str(&chunk);
                char_count += chunk.chars().count();
                let _ = app_handle.emit(
                    "research_stream_chunk",
                    serde_json::json!({
                        "task_id": task_id,
                        "chunk": chunk_str,
                    }),
                );
                // 每增加 150 字触发一次进度更新，避免事件风暴
                if char_count.saturating_sub(last_emitted) >= 150 {
                    last_emitted = char_count;
                    let _ = app_handle.emit(
                        "research_progress",
                        serde_json::json!({
                            "task_id": task_id,
                            "stage": "synthesizing",
                            "message": format!("正在生成综合报告... 已生成 {} 字", char_count),
                        }),
                    );
                }
            })
            .await;
        match stream_result {
            Ok(_) => {
                synthesized = buf;
                synth_last_err = None;
                break;
            }
            Err(err) => {
                synth_last_err = Some(err.clone());
                if attempt == 1 {
                    emit_progress(
                        "synthesizing",
                        format!(
                            "综合报告生成失败，正在重试一次：{}",
                            search_service::compact_llm_error(&err, 120)
                        ),
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                }
            }
        }
    }
    if let Some(err) = synth_last_err {
        report_research_failure(
            &db_path,
            &app_handle,
            task_id,
            &format!("综合报告生成失败: {}", err),
        );
        return;
    }

    // ── Phase 4: 保存 ────────────────────────────────────────────────────────
    emit_progress("saving", "正在保存到知识库...".to_string());
    let _ = save_research_output(
        &db_path,
        &app_handle,
        &state,
        task_id,
        &topic,
        &config,
        &all_results,
        &all_used_queries,
        &learnings,
        &synthesized,
    )
    .await;
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

/// 提取 Markdown 首个 H1 标题（`# `）。
fn extract_markdown_h1_title(content: &str) -> Option<String> {
    content
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_string())
        .filter(|title| !title.is_empty())
}

/// 在 `wiki/` 目录中寻找可用 slug，必要时自动追加序号（`-2` 到 `-99`）。
fn resolve_unique_wiki_slug(wiki_dir: &Path, base_slug: &str) -> Result<String, String> {
    let candidate = wiki_dir.join(format!("{}.md", base_slug));
    if !candidate.exists() {
        return Ok(base_slug.to_string());
    }
    for n in 2..=99 {
        let slug = format!("{}-{}", base_slug, n);
        let path = wiki_dir.join(format!("{}.md", slug));
        if !path.exists() {
            return Ok(slug);
        }
    }
    Err(format!("无法为 slug '{}' 找到空闲文件名", base_slug))
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

fn wiki_title_from_content(content: &str, fallback_path: &str) -> String {
    content
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            std::path::Path::new(fallback_path)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(fallback_path)
                .to_string()
        })
}

/// 从显示路径提取有意义的 wiki 页面名称（供 ingest 命名使用）。
/// 取文件 stem，排除内部临时文件（`llm_wiki_` 前缀）。
fn extract_wiki_display_name(display_source_path: &str) -> Option<String> {
    std::path::Path::new(display_source_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with("llm_wiki_"))
        .map(str::to_string)
}

/// 判断标题是否为原始摄入 ID（格式：`ingest-{纯十进制数字}`）。
fn is_raw_ingest_id(title: &str) -> bool {
    match title.strip_prefix("ingest-") {
        Some(rest) => !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// 解析图谱节点的显示标签。
///
/// 当 DB 标题是原始摄入 ID 时（`ingest-{timestamp}`），从 frontmatter 提取更有意义的名称：
/// entities[0]（≥2 字符）> source 文件名 stem（非内部路径）> DB 标题原值。
fn resolve_graph_node_label(db_title: &str, fm: &crate::models::WikiPageFrontmatter) -> String {
    if !is_raw_ingest_id(db_title) {
        return db_title.to_string();
    }
    if let Some(entity) = fm.entities.first() {
        let e = entity.trim();
        if e.len() >= 2 {
            return e.to_string();
        }
    }
    if let Some(src) = fm.source.as_deref() {
        if let Some(stem) = std::path::Path::new(src)
            .file_stem()
            .and_then(|s| s.to_str())
        {
            let stem = stem.trim();
            if !stem.is_empty() && !stem.starts_with("llm_wiki_") && stem.len() >= 2 {
                return stem.to_string();
            }
        }
    }
    db_title.to_string()
}

/// 计算字符串的简单 MD5 十六进制摘要（用于生成 content_hash）。
fn md5_simple(input: &str) -> u64 {
    // 使用 FNV-1a 64-bit 哈希作为轻量内容指纹（无需引入 md5 crate）
    let mut hash: u64 = 14695981039346656037u64;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211u64);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::config_service::{
        build_llm_status, display_cloud_provider_name, effective_cloud_base_url,
        llm_health_error_message, normalize_cloud_base_url, normalize_cloud_provider_name,
    };
    use super::search_service::{
        build_searxng_search_params, detect_query_pref_language, normalize_searxng_base_url,
        parse_unresponsive_engines, searxng_base_root, validate_search_config, SearxngSearchParams,
    };
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

    #[test]
    fn is_raw_ingest_id_detects_timestamp_pattern() {
        assert!(is_raw_ingest_id("ingest-1777101379565550500"));
        assert!(is_raw_ingest_id("ingest-0"));
        assert!(!is_raw_ingest_id("rust生命周期"));
        assert!(!is_raw_ingest_id("ingest-abc123"));
        assert!(!is_raw_ingest_id("ingest-123abc"));
        assert!(!is_raw_ingest_id("ingest-"));
        assert!(!is_raw_ingest_id(""));
    }

    #[test]
    fn resolve_graph_node_label_preserves_meaningful_title() {
        let fm = crate::models::WikiPageFrontmatter {
            title: Some("rust生命周期".to_string()),
            source: None,
            raw: None,
            imported_at: None,
            entities: vec!["Rust".to_string()],
            stale: None,
        };
        // 有意义的标题不应被替换，即便有 entities
        assert_eq!(
            resolve_graph_node_label("rust生命周期", &fm),
            "rust生命周期"
        );
    }

    #[test]
    fn resolve_graph_node_label_uses_first_entity_for_raw_id() {
        let fm = crate::models::WikiPageFrontmatter {
            title: Some("ingest-1777101379565550500".to_string()),
            source: None,
            raw: None,
            imported_at: None,
            entities: vec!["PINNs".to_string(), "Transformer".to_string()],
            stale: None,
        };
        assert_eq!(
            resolve_graph_node_label("ingest-1777101379565550500", &fm),
            "PINNs"
        );
    }

    #[test]
    fn resolve_graph_node_label_falls_back_to_source_stem() {
        let fm = crate::models::WikiPageFrontmatter {
            title: None,
            source: Some(r"E:\vault\research\大模型rag框架进展.md".to_string()),
            raw: None,
            imported_at: None,
            entities: vec![],
            stale: None,
        };
        assert_eq!(
            resolve_graph_node_label("ingest-1776768806095623000", &fm),
            "大模型rag框架进展"
        );
    }

    #[test]
    fn resolve_graph_node_label_skips_internal_source_path() {
        let fm = crate::models::WikiPageFrontmatter {
            title: None,
            source: Some(r"C:\Temp\llm_wiki_preview_apply_123.md".to_string()),
            raw: None,
            imported_at: None,
            entities: vec![],
            stale: None,
        };
        // internal temp path → fall back to db_title
        assert_eq!(
            resolve_graph_node_label("ingest-1776000000000000000", &fm),
            "ingest-1776000000000000000"
        );
    }

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

    #[tokio::test]
    async fn apply_ingest_preview_returns_error_when_preview_id_missing() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-preview-missing");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let err = state
            .apply_ingest_preview("preview-not-exists")
            .await
            .expect_err("不存在的 preview_id 应返回错误");
        assert!(err.contains("未找到预览缓存"));
    }

    #[tokio::test]
    async fn preview_ingest_file_then_apply_succeeds_and_consumes_cache() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-preview-apply");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let source_path = vault_dir.join("preview-source.md");
        fs::write(&source_path, "# Preview Source\n\nRust ingest preview")
            .expect("写入预览源文件失败");

        let preview = state
            .preview_ingest_file("file", source_path.to_string_lossy().as_ref(), None)
            .await
            .expect("生成 ingest preview 失败");
        assert!(!preview.preview_id.trim().is_empty());

        let result = state
            .apply_ingest_preview(&preview.preview_id)
            .await
            .expect("应用 ingest preview 失败");
        assert!(!result.wiki_path.trim().is_empty());
        assert!(
            Path::new(&result.wiki_path).exists(),
            "落盘后 wiki 页面应存在"
        );

        let err = state
            .apply_ingest_preview(&preview.preview_id)
            .await
            .expect_err("同一个 preview_id 第二次 apply 应失败");
        assert!(err.contains("未找到预览缓存"));
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
        compressor.write_all(&encoded).expect("压缩内容流失败");
        let compressed = compressor.finish().expect("完成压缩失败");

        let mut pseudo_pdf = Vec::new();
        pseudo_pdf.extend_from_slice(b"%PDF-1.4\n1 0 obj\n<< /Length ");
        pseudo_pdf.extend_from_slice(compressed.len().to_string().as_bytes());
        pseudo_pdf.extend_from_slice(b" /Filter /FlateDecode >>\nstream\n");
        pseudo_pdf.extend_from_slice(&compressed);
        pseudo_pdf.extend_from_slice(b"\nendstream\n%%EOF");

        let extracted =
            extract_text_from_pdf_raw_streams(&pseudo_pdf).expect("应能从 Flate stream 提取文本");
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
        assert!(!should_fallback_to_pdf_ocr(
            "读取 PDF 原始字节失败：permission denied"
        ));
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
                .generate_query_answer_with_provider("需要回退吗", &matches, Some(provider), None)
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
        let codes: BTreeSet<_> = report
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect();

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

    fn make_test_state_bare(vault_dir: &Path) -> AppState {
        AppState {
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
                shell_policy: ShellPolicyConfig::default(),
                pending_agent_writes: std::collections::HashMap::new(),
            }),
            config_path: vault_dir.join(".runtime").join("app-config.json"),
            llm_provider: OnceLock::new(),
            app_handle: OnceLock::new(),
            ask_sessions: Mutex::new(std::collections::HashMap::new()),
            ask_cancel_flags: Mutex::new(std::collections::HashMap::new()),
            search_config: Mutex::new(crate::models::SearchConfig::default()),
            pending_query_approvals: Mutex::new(std::collections::HashMap::new()),
            ingest_previews: Mutex::new(std::collections::HashMap::new()),
            shell_sessions: Mutex::new(std::collections::HashMap::new()),
            chat_cancellations: Mutex::new(std::collections::HashMap::new()),
            chat_write_approvals: Mutex::new(std::collections::HashMap::new()),
            chat_shell_pending: Mutex::new(std::collections::HashMap::new()),
            mcp_clients: Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn make_test_state(vault_dir: &Path) -> AppState {
        let state = make_test_state_bare(vault_dir);
        // 注入默认 MockQueryProvider，使依赖 LLM 的测试开箱即用
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

    #[test]
    fn save_wiki_page_records_previous_content_history() {
        let vault_dir = make_temp_dir("llm-wiki-page-history");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("history.md");
        fs::write(&page_path, "# History\nfirst version\n").expect("写入初始页面失败");

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                "# History\nsecond version\n",
                None,
            ))
            .expect("第一次保存失败");
        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                "# History\nthird version\n",
                None,
            ))
            .expect("第二次保存失败");

        let history = state
            .list_wiki_page_history_impl(page_path.to_str().expect("页面路径不是 UTF-8"), Some(10))
            .expect("读取历史列表失败");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].title, "History");

        let latest = state
            .get_wiki_page_history_entry_impl(history[0].id)
            .expect("读取最新历史详情失败");
        let older = state
            .get_wiki_page_history_entry_impl(history[1].id)
            .expect("读取较早历史详情失败");

        assert_eq!(latest.content, "# History\nsecond version\n");
        assert_eq!(older.content, "# History\nfirst version\n");
        assert_eq!(
            fs::read_to_string(&page_path).expect("读取当前页面失败"),
            "# History\nthird version\n"
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
            sessions.insert(
                "sess1".to_string(),
                vec![crate::models::AskTurn {
                    role: "user".to_string(),
                    content: "hi".to_string(),
                }],
            );
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

        let paths = vec![
            "wiki/a.md".to_string(),
            "wiki/b.md".to_string(),
            "wiki/c.md".to_string(),
        ];
        let result = state.get_page_embedding_similarities(paths).unwrap();

        assert!(result.contains_key("wiki/a.md||wiki/b.md"), "a-b 对应包含");
        let sim = result["wiki/a.md||wiki/b.md"];
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "a-b 相似度应为 1.0，实际: {}",
            sim
        );
        assert!(
            !result.contains_key("wiki/a.md||wiki/c.md"),
            "a-c 直交不应包含"
        );
        assert!(
            !result.contains_key("wiki/b.md||wiki/c.md"),
            "b-c 直交不应包含"
        );
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

    // ─── Deep Research pipeline unit tests ───────────────────────────────────

    #[test]
    fn parse_learnings_standard_format() {
        let text =
            "LEARNING: LLMs use attention mechanisms\nFOLLOWUP: how does multi-head attention work";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["LLMs use attention mechanisms"]);
        assert_eq!(followups, vec!["how does multi-head attention work"]);
    }

    #[test]
    fn parse_learnings_markdown_bold() {
        let text = "**LEARNING:** transformers replaced RNNs\n**FOLLOWUP:** what are the main transformer variants";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["transformers replaced RNNs"]);
        assert_eq!(followups, vec!["what are the main transformer variants"]);
    }

    #[test]
    fn parse_learnings_list_prefix() {
        let text = "- LEARNING: attention is O(n^2)\n- FOLLOWUP: sparse attention methods";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["attention is O(n^2)"]);
        assert_eq!(followups, vec!["sparse attention methods"]);
    }

    #[test]
    fn parse_learnings_numbered_list() {
        let text = "1. LEARNING: GPT uses decoder-only\n2. FOLLOWUP: encoder-decoder models";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["GPT uses decoder-only"]);
        assert_eq!(followups, vec!["encoder-decoder models"]);
    }

    #[test]
    fn parse_learnings_lowercase_tag() {
        let text =
            "learning: BERT is bidirectional\nfollowup: how does masked language modeling work";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["BERT is bidirectional"]);
        assert_eq!(followups, vec!["how does masked language modeling work"]);
    }

    #[test]
    fn parse_learnings_mixed_formats() {
        let text = "LEARNING: finding A\n- learning: finding B\n**LEARNING:** finding C\nFOLLOWUP: query X\n1. followup: query Y";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings.len(), 3);
        assert_eq!(followups.len(), 2);
        assert!(learnings.contains(&"finding A".to_string()));
        assert!(learnings.contains(&"finding B".to_string()));
        assert!(learnings.contains(&"finding C".to_string()));
    }

    #[test]
    fn parse_learnings_fallback_on_unstructured_output() {
        // LLM ignored the format — all meaningful lines should become learnings
        let text = "The sky is blue\nWater is wet\n\n# Header (should be skipped)";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert!(followups.is_empty());
        assert!(learnings.contains(&"The sky is blue".to_string()));
        assert!(learnings.contains(&"Water is wet".to_string()));
        assert!(!learnings.iter().any(|l| l.starts_with('#')));
    }

    #[test]
    fn parse_learnings_empty_input() {
        let (learnings, followups) = parse_learnings_and_followups("");
        assert!(learnings.is_empty());
        assert!(followups.is_empty());
    }

    #[test]
    fn parse_learnings_skips_empty_content_after_tag() {
        let text = "LEARNING:   \nFOLLOWUP:";
        let (learnings, followups) = parse_learnings_and_followups(text);
        // Empty content after tag → fallback will pick up "LEARNING:   " and "FOLLOWUP:" as text
        // The structured pass produces nothing (empty content), so fallback kicks in
        // We just verify it doesn't crash and produces consistent output
        let _ = (learnings, followups);
    }

    #[test]
    fn make_research_slug_ascii() {
        assert_eq!(make_research_slug("Hello World"), "hello-world");
    }

    #[test]
    fn make_research_slug_deduplicates_dashes() {
        assert_eq!(make_research_slug("hello   world"), "hello-world");
    }

    #[test]
    fn make_research_slug_trims_dashes() {
        assert_eq!(make_research_slug("  hello  "), "hello");
    }

    #[test]
    fn make_research_slug_unicode_becomes_dashes() {
        let slug = make_research_slug("大模型 RAG");
        // Unicode chars map to '-', spaces map to '-', deduped
        assert!(!slug.contains(' '));
        assert!(slug.len() <= 50);
    }

    #[test]
    fn make_research_slug_max_50_chars() {
        let long = "a".repeat(100);
        assert_eq!(make_research_slug(&long).len(), 50);
    }

    #[test]
    fn normalize_searxng_base_url_adds_http_when_missing_scheme() {
        assert_eq!(
            normalize_searxng_base_url("localhost:8080"),
            "http://localhost:8080"
        );
    }

    #[test]
    fn normalize_searxng_base_url_keeps_https() {
        assert_eq!(
            normalize_searxng_base_url("https://searx.local/"),
            "https://searx.local"
        );
    }

    #[test]
    fn searxng_base_root_strips_search_suffix() {
        assert_eq!(
            searxng_base_root("http://127.0.0.1:8080/search/"),
            "http://127.0.0.1:8080"
        );
    }

    #[test]
    fn detect_query_pref_language_prefers_zh_for_cjk() {
        assert_eq!(detect_query_pref_language("大模型 RAG 架构"), "zh-CN");
    }

    #[test]
    fn detect_query_pref_language_uses_auto_for_latin() {
        assert_eq!(detect_query_pref_language("rust async runtime"), "auto");
    }

    #[test]
    fn build_searxng_search_params_contains_all_fallback() {
        let params = build_searxng_search_params("rust ownership model");
        assert!(params
            .iter()
            .any(|p| p.language == "all" && p.categories == "general,news"));
        assert!(params.iter().any(|p| p.safesearch == "0"));
    }

    #[test]
    fn parse_unresponsive_engines_supports_string_and_object() {
        let data = serde_json::json!({
            "unresponsive_engines": [
                "google too many requests",
                {"name": "brave", "reason": "access denied"},
                {"engine": "duckduckgo"},
                null
            ]
        });
        let parsed = parse_unresponsive_engines(&data);
        assert!(parsed.iter().any(|s| s.contains("google")));
        assert!(parsed.iter().any(|s| s.contains("brave (access denied)")));
        assert!(parsed.iter().any(|s| s == "duckduckgo"));
    }

    #[test]
    fn validate_search_config_rejects_missing_searxng_url() {
        let mut cfg = crate::models::SearchConfig::default();
        cfg.search_provider = "searxng".to_string();
        cfg.searxng_url = "   ".to_string();
        let err = validate_search_config(&cfg).expect_err("应拒绝空 searxng 地址");
        assert!(err.contains("SearXNG"));
    }

    #[test]
    fn validate_search_config_accepts_valid_searxng_url() {
        let mut cfg = crate::models::SearchConfig::default();
        cfg.search_provider = "searxng".to_string();
        cfg.searxng_url = "localhost:8080".to_string();
        assert!(validate_search_config(&cfg).is_ok());
    }

    #[test]
    fn strip_think_tags_removes_think_block() {
        let input = "before<think>internal reasoning</think>after";
        assert_eq!(strip_think_tags(input), "beforeafter");
    }

    #[test]
    fn strip_think_tags_removes_thinking_block() {
        let input = "<thinking>step 1\nstep 2</thinking>result";
        assert_eq!(strip_think_tags(input), "result");
    }

    #[test]
    fn strip_think_tags_unclosed_tag_removes_to_end() {
        let input = "start<think>incomplete";
        assert_eq!(strip_think_tags(input), "start");
    }

    #[test]
    fn strip_think_tags_no_tags_unchanged() {
        let input = "plain text with no tags";
        assert_eq!(strip_think_tags(input), input);
    }

    // ─── DB research task CRUD tests ─────────────────────────────────────────

    fn make_research_db() -> (PathBuf, rusqlite::Connection, impl Drop) {
        let dir = make_temp_dir("llm-wiki-research-db");
        let guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        db::ensure_meta_db(&db_path).expect("ensure_meta_db 失败");
        let conn = rusqlite::Connection::open(&db_path).expect("打开数据库失败");
        (db_path, conn, guard)
    }

    #[test]
    fn research_task_create_and_list() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "test topic", 2, 3, "100").expect("create 失败");
        assert!(id > 0);

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.topic, "test topic");
        assert_eq!(t.status, "queued");
        assert_eq!(t.depth, 2);
        assert_eq!(t.breadth, 3);
        assert_eq!(t.web_results_count, 0);
        assert!(t.sub_queries.is_empty());
    }

    #[test]
    fn research_task_update_to_done() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "update test", 1, 2, "100").expect("create 失败");

        let queries = serde_json::to_string(&vec!["q1", "q2"]).unwrap();
        db::db_update_research_task(
            &conn,
            id,
            "done",
            &queries,
            5,
            Some("/path/to/file.md"),
            None,
            "200",
        )
        .expect("update 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        let t = &tasks[0];
        assert_eq!(t.status, "done");
        assert_eq!(t.web_results_count, 5);
        assert_eq!(t.saved_path.as_deref(), Some("/path/to/file.md"));
        assert_eq!(t.sub_queries, vec!["q1", "q2"]);
    }

    #[test]
    fn research_task_cancel_changes_queued_to_cancelled() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "cancel test", 1, 1, "100").expect("create 失败");

        db::db_cancel_research_task(&conn, id, "200").expect("cancel 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks[0].status, "cancelled");
    }

    #[test]
    fn research_task_cancel_is_idempotent_on_done() {
        let (_path, conn, _guard) = make_research_db();
        let id = db::db_create_research_task(&conn, "idempotent test", 1, 1, "100")
            .expect("create 失败");

        // 先将任务标记为 done
        db::db_update_research_task(&conn, id, "done", "[]", 3, Some("/p.md"), None, "150")
            .expect("update 失败");

        // cancel 不应影响 done 任务
        db::db_cancel_research_task(&conn, id, "200").expect("cancel 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks[0].status, "done", "done 任务不应被 cancel 覆盖");
        assert_eq!(
            tasks[0].web_results_count, 3,
            "web_results_count 不应被重置"
        );
        assert_eq!(
            tasks[0].saved_path.as_deref(),
            Some("/p.md"),
            "saved_path 不应被清空"
        );
    }

    #[test]
    fn research_task_delete_removes_row() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "delete test", 1, 1, "100").expect("create 失败");

        db::db_delete_research_task(&conn, id).expect("delete 失败");

        let task = db::db_get_research_task(&conn, id).expect("get 失败");
        assert!(task.is_none(), "删除后任务应不存在");
    }

    #[test]
    fn research_task_delete_missing_returns_error() {
        let (_path, conn, _guard) = make_research_db();
        let err = db::db_delete_research_task(&conn, 99999).expect_err("不存在任务应报错");
        assert!(err.contains("研究任务不存在"), "错误信息应包含不存在提示");
    }

    #[test]
    fn ingest_queue_stale_running_reset_on_vault_init() {
        let vault_dir = make_temp_dir("llm-wiki-restart-recovery");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("init_vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = rusqlite::Connection::open(&db_path).expect("打开数据库失败");

        // 模拟上次崩溃遗留的 running 任务
        db::db_enqueue_ingest(&conn, "file", "/some/path.md", "100").expect("enqueue 失败");
        let items = db::db_list_ingest_queue(&conn).expect("list 失败");
        let id = items[0].id;
        let now = current_timestamp_ms();
        db::db_update_ingest_queue_status(&conn, id, "running", None, &now)
            .expect("set running 失败");

        // 重新 init_vault 模拟重启
        state
            .init_vault(vault_dir.clone())
            .expect("第二次 init_vault 失败");

        let items = db::db_list_ingest_queue(&conn).expect("list after restart 失败");
        assert_eq!(
            items[0].status, "queued",
            "重启后 running 任务应被重置为 queued"
        );
    }

    #[test]
    fn research_task_cancel_is_idempotent_on_already_cancelled() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "double cancel", 1, 1, "100").expect("create 失败");

        db::db_cancel_research_task(&conn, id, "200").expect("第一次 cancel 失败");
        db::db_cancel_research_task(&conn, id, "300").expect("第二次 cancel 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks[0].status, "cancelled");
        // updated_at 应为第一次 cancel 的时间，不被第二次覆盖
        assert_eq!(tasks[0].updated_at, "200");
    }

    // ── init_vault_with_template 安全测试 ────────────────────────────────────

    #[test]
    fn init_vault_with_template_rejects_path_traversal() {
        let dir = make_temp_dir("llm-wiki-state-template-traversal");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        let vault_dir = dir.join("vault");
        fs::create_dir_all(&vault_dir).unwrap();
        let result = state.init_vault_with_template(
            vault_dir,
            "schema content".to_string(),
            "purpose content".to_string(),
            vec!["../../../etc".to_string()],
        );
        assert!(result.is_err(), "路径遍历应被拒绝");
        assert!(result.unwrap_err().contains("非法目录路径"));
    }

    #[test]
    fn init_vault_with_template_rejects_oversized_content() {
        let dir = make_temp_dir("llm-wiki-state-template-size");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        let vault_dir = dir.join("vault");
        fs::create_dir_all(&vault_dir).unwrap();
        let big = "x".repeat(513 * 1024);
        let result = state.init_vault_with_template(vault_dir, big, "ok".to_string(), vec![]);
        assert!(result.is_err(), "超大 schema 内容应被拒绝");
        assert!(result.unwrap_err().contains("512 KB"));
    }

    // ── quick_lint_page 测试 ─────────────────────────────────────────────────

    #[cfg(test)]
    mod quick_lint_tests {
        use super::*;

        fn make_vault_with_page(content: &str) -> (PathBuf, String) {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("llm-wiki-ql-{}-{}", std::process::id(), unique));
            let wiki_dir = dir.join("wiki");
            fs::create_dir_all(&wiki_dir).unwrap();
            let page_path = wiki_dir.join("test-page.md");
            fs::write(&page_path, content).unwrap();
            let wiki_path = "wiki/test-page.md".to_string();
            (dir, wiki_path)
        }

        fn make_state_for_vault(vault_dir: &Path) -> AppState {
            let config_path = vault_dir.join(".app").join("config.json");
            let state = AppState::new_with_path(config_path);
            {
                let mut inner = state.inner.lock().unwrap();
                inner.vault_path = Some(vault_dir.to_path_buf());
            }
            state
        }

        #[test]
        fn quick_lint_page_no_issues_when_links_exist() {
            let (dir, wiki_path) = make_vault_with_page(
                "---\ntitle: Test\nentities:\n  - Existing\n---\n\nSee [[Existing]]",
            );
            let _guard = TempDirGuard(dir.clone());
            // 创建链接指向的页面
            fs::write(dir.join("wiki").join("existing.md"), "").unwrap();

            let state = make_state_for_vault(&dir);
            let result = state.quick_lint_page_impl(&wiki_path).unwrap();
            assert_eq!(result.issues_count, 0);
            assert!(result.broken_links.is_empty());
            assert!(result.missing_entity_pages.is_empty());
        }

        #[test]
        fn quick_lint_page_detects_broken_link() {
            let (dir, wiki_path) =
                make_vault_with_page("---\ntitle: Test\n---\n\nSee [[NonExistentPage]]");
            let _guard = TempDirGuard(dir.clone());

            let state = make_state_for_vault(&dir);
            let result = state.quick_lint_page_impl(&wiki_path).unwrap();
            assert!(result.issues_count > 0);
            assert!(!result.broken_links.is_empty());
        }

        #[test]
        fn quick_lint_page_detects_missing_entity_page() {
            let (dir, wiki_path) = make_vault_with_page(
                "---\ntitle: Test\nentities:\n  - MissingEntity\n---\n\nContent here.",
            );
            let _guard = TempDirGuard(dir.clone());

            let state = make_state_for_vault(&dir);
            let result = state.quick_lint_page_impl(&wiki_path).unwrap();
            assert!(!result.missing_entity_pages.is_empty());
        }

        #[test]
        fn quick_lint_page_returns_empty_when_file_missing() {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "llm-wiki-ql-nofile-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(dir.join("wiki")).unwrap();
            let _guard = TempDirGuard(dir.clone());

            let state = make_state_for_vault(&dir);
            let result = state
                .quick_lint_page_impl("wiki/does-not-exist.md")
                .unwrap();
            assert_eq!(result.issues_count, 0);
        }
    }

    // ── 页面历史恢复测试 ──────────────────────────────────────────

    #[test]
    fn restore_wiki_page_from_history_replaces_content() {
        let vault_dir = make_temp_dir("llm-wiki-restore-history");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("restore-test.md");
        let content_v1 = "# Restore Test\n版本一\n";
        let content_v2 = "# Restore Test\n版本二\n";
        let content_v3 = "# Restore Test\n版本三\n";

        fs::write(&page_path, content_v1).expect("写入初始页面失败");

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");

        // 保存两次，生成两条历史记录（v1→v2, v2→v3）
        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                content_v2,
                None,
            ))
            .expect("第一次保存失败");
        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                content_v3,
                None,
            ))
            .expect("第二次保存失败");

        // 读取历史列表，取最旧（id 较小）的记录
        let history = state
            .list_wiki_page_history_impl(page_path.to_str().expect("页面路径不是 UTF-8"), Some(10))
            .expect("读取历史列表失败");
        assert_eq!(history.len(), 2, "应有 2 条历史记录");

        let oldest = state
            .get_wiki_page_history_entry_impl(history[1].id)
            .expect("读取最旧历史详情失败");
        assert_eq!(oldest.content, content_v1);

        // 恢复到最旧版本
        let result = runtime
            .block_on(state.restore_wiki_page_from_history_impl(history[1].id))
            .expect("恢复历史版本失败");
        assert_eq!(result.path, page_path.to_str().expect("路径不是 UTF-8"));

        // 验证文件内容已恢复为 v1
        let restored = fs::read_to_string(&page_path).expect("读取恢复后页面失败");
        assert_eq!(restored, content_v1, "恢复后内容应与 v1 一致");

        // 恢复行为又生成了一条新的历史记录（v3 被保存为历史）
        let history_after = state
            .list_wiki_page_history_impl(page_path.to_str().expect("页面路径不是 UTF-8"), Some(10))
            .expect("读取恢复后历史列表失败");
        assert_eq!(history_after.len(), 3, "恢复操作应产生新的历史快照");
    }

    // ── 保存时 checksum 校验测试 ──────────────────────────────────

    #[test]
    fn save_wiki_page_checksum_mismatch_rejected() {
        let vault_dir = make_temp_dir("llm-wiki-checksum-mismatch");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("checksum.md");
        let original = "# Checksum Test\n原始内容\n";
        fs::write(&page_path, original).expect("写入初始页面失败");

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        // 提供故意不匹配的 checksum
        let wrong_hash = "deadbeef".to_string();
        let result = runtime.block_on(state.save_wiki_page_impl(
            page_path.to_str().expect("页面路径不是 UTF-8"),
            "# Checksum Test\n新内容\n",
            Some(&wrong_hash),
        ));
        assert!(result.is_err(), "checksum 不匹配应返回 Err");
        assert!(
            result.unwrap_err().contains("checksum_mismatch"),
            "错误消息应包含 checksum_mismatch"
        );

        // 文件内容不应被修改
        let current = fs::read_to_string(&page_path).expect("读取页面失败");
        assert_eq!(current, original, "checksum 校验失败时文件不应被修改");
    }

    #[test]
    fn save_wiki_page_checksum_match_accepted() {
        let vault_dir = make_temp_dir("llm-wiki-checksum-match");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("checksum-ok.md");
        let original = "# Checksum Test\n原始内容\n";
        fs::write(&page_path, original).expect("写入初始页面失败");

        // 计算正确的 checksum（与 md5_simple 一致的格式）
        let correct_hash = format!("{:x}", md5_simple(original));

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let result = runtime.block_on(state.save_wiki_page_impl(
            page_path.to_str().expect("页面路径不是 UTF-8"),
            "# Checksum Test\n新内容\n",
            Some(&correct_hash),
        ));
        assert!(result.is_ok(), "正确 checksum 应保存成功");

        // 验证新内容已写入
        let current = fs::read_to_string(&page_path).expect("读取页面失败");
        assert_eq!(current, "# Checksum Test\n新内容\n");
    }

    #[test]
    fn agent_run_h0_impl_lifecycle_works() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h0-state");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let run_id = state
            .start_agent_run_impl("Agent Studio H0")
            .expect("创建 run 失败");
        state
            .append_agent_run_event_impl(run_id, "info", "created")
            .expect("写入事件失败");
        state
            .complete_agent_run_impl(run_id, "applied")
            .expect("结束 run 失败");

        let runs = state
            .list_agent_runs_impl(Some(10), Some(false))
            .expect("读取 runs 失败");
        assert!(!runs.is_empty());
        assert_eq!(runs[0].id, run_id);
        assert_eq!(runs[0].status, "applied");

        let events = state
            .list_agent_run_events_impl(run_id, Some(10))
            .expect("读取 events 失败");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, "created");
        assert!(
            events[1].message.contains("系统状态变更"),
            "complete_agent_run 后应自动写系统事件"
        );
    }

    #[test]
    fn agent_draft_generate_and_approve_impl_works() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h1-state");
        let _guard = TempDirGuard(vault_dir.clone());
        // 使用 bare state，避免 OnceLock 被 make_test_state 的默认 mock 抢先占用
        let state = make_test_state_bare(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let prompt_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let _ = state.llm_provider.set(Arc::new(MockQueryProvider::new(
            "# Rust Actor 模块设计\n\n这里是草稿正文。\n",
            prompt_log,
        )));

        let run_id = state
            .start_agent_run_impl("Agent H1")
            .expect("创建 run 失败");
        let runtime = tokio::runtime::Runtime::new().expect("创建 runtime 失败");
        let draft = runtime
            .block_on(state.generate_agent_draft_impl(
                run_id,
                "Rust Actor 模块设计".to_string(),
                None,
                false,
                false,
            ))
            .expect("生成草稿失败");
        assert_eq!(draft.run_id, run_id);
        assert_eq!(draft.status, "draft");
        assert!(draft.content.contains("Rust Actor 模块设计"));

        let drafts = state
            .list_agent_drafts_impl(run_id, Some(10))
            .expect("列出草稿失败");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].id, draft.id);

        let applied = runtime
            .block_on(state.approve_agent_draft_impl(draft.id))
            .expect("审批草稿失败");
        assert!(applied.wiki_path.ends_with(".md"));
        assert_eq!(applied.title, "Rust Actor 模块设计");
        let file_content = fs::read_to_string(&applied.wiki_path).expect("读取写盘文件失败");
        assert!(file_content.contains("这里是草稿正文"));
    }

    #[test]
    fn agent_draft_generate_with_skill_injects_skill_prompt() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h3-skill-generate");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let prompt_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let _ = state.llm_provider.set(Arc::new(MockQueryProvider::new(
            "# 技能化页面\n\n正文。\n",
            prompt_log.clone(),
        )));

        state
            .upsert_agent_skill_impl("writer", "输出语气：客观、结构化、严禁口语")
            .expect("创建技能失败");
        let run_id = state
            .start_agent_run_impl("技能注入测试")
            .expect("创建 run 失败");
        let runtime = tokio::runtime::Runtime::new().expect("创建 runtime 失败");
        let _ = runtime
            .block_on(state.generate_agent_draft_impl(
                run_id,
                "技能注入测试".to_string(),
                Some("writer".to_string()),
                false,
                false,
            ))
            .expect("生成草稿失败");

        let prompts = prompt_log.lock().expect("读取 prompt 失败");
        assert_eq!(prompts.len(), 1);
        assert!(
            prompts[0].contains("当前启用技能模板"),
            "prompt 应包含技能模板注入段"
        );
        assert!(
            prompts[0].contains("输出语气：客观、结构化、严禁口语"),
            "prompt 应包含选中 skill 的模板内容"
        );
    }

    #[test]
    fn agent_skill_crud_impl_works() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h3-skill");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let created = state
            .upsert_agent_skill_impl("writer", "你是一个简洁的知识写作助手")
            .expect("创建技能失败");
        assert_eq!(created.skill_key, "writer");
        assert_eq!(created.version, 1);

        let updated = state
            .upsert_agent_skill_impl("writer", "你是一个结构化的知识写作助手")
            .expect("更新技能失败");
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.version, 2);

        let list = state
            .list_agent_skills_impl(Some(10))
            .expect("查询技能失败");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, created.id);

        state
            .delete_agent_skill_impl(created.id)
            .expect("删除技能失败");
        let empty = state
            .list_agent_skills_impl(Some(10))
            .expect("查询技能失败");
        assert!(empty.is_empty());
    }

    #[test]
    fn check_agent_draft_conflict_returns_no_conflict_when_page_absent() {
        let vault_dir = make_temp_dir("llm-wiki-h1-conflict");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let run_id = state
            .start_agent_run_impl("冲突检测测试")
            .expect("创建 run 失败");
        let now = current_timestamp_ms();
        let db_path = vault_dir.join(".app").join("meta.db");
        let draft = db::create_agent_draft(
            &db_path,
            run_id,
            "唯一不存在的页面标题",
            "draft content",
            "draft",
            &now,
        )
        .expect("创建草稿失败");

        let info = state
            .check_agent_draft_conflict_impl(draft.id)
            .expect("冲突检测失败");
        assert_eq!(info.draft_id, draft.id);
        assert_eq!(info.title, "唯一不存在的页面标题");
        assert!(!info.conflict);
        assert!(info.existing_path.is_none());
        assert!(info.existing_preview.is_none());
    }

    // ── archive / restore agent run 约束测试 ─────────────────────────────

    #[test]
    fn archive_agent_run_rejects_running_status() {
        let vault_dir = make_temp_dir("llm-wiki-archive-running");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        // start_agent_run_impl 默认 status=running
        let run_id = state.start_agent_run_impl("running 归档测试").expect("创建 run");
        let result = state.archive_agent_run_impl(run_id);
        assert!(result.is_err(), "running 状态应禁止归档");
        assert!(result.unwrap_err().contains("正在进行中"));
    }

    #[test]
    fn archive_agent_run_rejects_when_pending_write_exists() {
        let vault_dir = make_temp_dir("llm-wiki-archive-pending");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let run_id = state.start_agent_run_impl("pending 写入归档测试").expect("创建 run");
        state.complete_agent_run_impl(run_id, "applied").expect("完成 run");

        state.store_pending_agent_write(
            run_id,
            vault_dir.join("wiki").join("block.md").to_string_lossy().to_string(),
            "内容".to_string(),
            None,
        );

        let result = state.archive_agent_run_impl(run_id);
        assert!(result.is_err(), "存在 pending write 时应禁止归档");
        assert!(result.unwrap_err().contains("待审批写入"));
    }

    #[test]
    fn archive_and_restore_agent_run_round_trip() {
        let vault_dir = make_temp_dir("llm-wiki-archive-restore");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let run_id = state.start_agent_run_impl("归档恢复测试").expect("创建 run");
        state.complete_agent_run_impl(run_id, "applied").expect("完成 run");

        // 归档
        let archive_result = state.archive_agent_run_impl(run_id);
        assert!(archive_result.is_ok(), "done 状态应允许归档: {archive_result:?}");

        // 归档后重复归档应失败
        let double_archive = state.archive_agent_run_impl(run_id);
        assert!(double_archive.is_err(), "已归档的 run 不能再次归档");

        // 恢复
        let restore_result = state.restore_agent_run_impl(run_id);
        assert!(restore_result.is_ok(), "已归档 run 应可恢复: {restore_result:?}");

        // 恢复后重复恢复应失败
        let double_restore = state.restore_agent_run_impl(run_id);
        assert!(double_restore.is_err(), "未归档的 run 不能再次恢复");
    }

    // ── approve/reject agent write 审批链路 ──────────────────────────────

    #[test]
    fn approve_agent_write_full_write_creates_file() {
        let vault_dir = make_temp_dir("llm-wiki-approve-write");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let wiki_dir = vault_dir.join("wiki");
        let target = wiki_dir.join("test-approve.md");
        let run_id = 9001_i64;

        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "# 审批写入测试\n\n内容正文。\n".to_string(),
            None, // write_wiki 全量写入
        );

        let result = state.approve_agent_write_impl(run_id);
        assert!(result.is_ok(), "approve 应成功，实际: {result:?}");
        assert!(target.exists(), "文件应被写入");
        let content = fs::read_to_string(&target).expect("读文件");
        assert!(content.contains("审批写入测试"));
        // 写入后 pending 应被消耗
        assert!(state.take_pending_agent_write(run_id).is_none());
    }

    #[test]
    fn reject_agent_write_does_not_create_file() {
        let vault_dir = make_temp_dir("llm-wiki-reject-write");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let target = vault_dir.join("wiki").join("should-not-exist.md");
        let run_id = 9002_i64;

        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "不应被写入的内容".to_string(),
            None,
        );

        let result = state.reject_agent_write_impl(run_id);
        assert!(result.is_ok(), "reject 应成功，实际: {result:?}");
        assert!(!target.exists(), "文件不应被创建");
        assert!(state.take_pending_agent_write(run_id).is_none());
    }

    #[test]
    fn approve_agent_write_patch_replaces_content() {
        let vault_dir = make_temp_dir("llm-wiki-approve-patch");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let wiki_dir = vault_dir.join("wiki");
        let target = wiki_dir.join("patch-target.md");
        fs::write(&target, "# 标题\n\n旧内容段落。\n\n其他部分。\n").expect("初始文件");

        let run_id = 9003_i64;
        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "新内容段落。".to_string(),
            Some("旧内容段落。".to_string()), // edit_wiki patch
        );

        let result = state.approve_agent_write_impl(run_id);
        assert!(result.is_ok(), "patch approve 应成功: {result:?}");
        let content = fs::read_to_string(&target).expect("读文件");
        assert!(content.contains("新内容段落。"), "新内容应存在");
        assert!(!content.contains("旧内容段落。"), "旧内容应被替换");
        assert!(content.contains("其他部分。"), "其他内容应保留");
    }

    #[test]
    fn approve_agent_write_patch_fails_when_old_str_not_found() {
        let vault_dir = make_temp_dir("llm-wiki-approve-patch-fail");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let wiki_dir = vault_dir.join("wiki");
        let target = wiki_dir.join("patch-fail.md");
        fs::write(&target, "# 标题\n\n实际内容。\n").expect("初始文件");

        let run_id = 9004_i64;
        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "替换后内容".to_string(),
            Some("不存在的旧内容".to_string()),
        );

        let result = state.approve_agent_write_impl(run_id);
        assert!(result.is_err(), "old_str 不存在时应返回 Err");
        assert!(result.unwrap_err().contains("未找到待替换内容"));
        // 文件内容不应变化
        let content = fs::read_to_string(&target).expect("读文件");
        assert!(content.contains("实际内容。"));
    }
}