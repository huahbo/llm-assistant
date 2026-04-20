use serde::{Deserialize, Serialize};

/// 运行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    Hybrid,
    StrictLocal,
}

impl Default for AppMode {
    fn default() -> Self {
        Self::Hybrid
    }
}

/// 模式切换结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeChangeResult {
    pub previous_mode: AppMode,
    pub current_mode: AppMode,
    pub strict_local_enabled: bool,
}

/// 本地配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub mode: AppMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vault_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_top_k: Option<usize>,
    /// 云端 API Key（仅存本地，不入仓库）
    #[serde(
        alias = "openai_api_key",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cloud_api_key: Option<String>,
    /// 云端基础地址，兼容 OpenAI / DeepSeek 等 OpenAI-compatible Provider
    #[serde(
        alias = "openai_base_url",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cloud_base_url: Option<String>,
    /// 云端模型名
    #[serde(
        alias = "openai_model",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cloud_model: Option<String>,
    /// 云端 Provider 名称
    #[serde(
        alias = "openai_provider_name",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cloud_provider_name: Option<String>,
    /// 当前活跃 Provider
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider: Option<String>,
    /// 默认 OCR provider（tesseract / paddle），未设置时回退 tesseract
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_ocr_provider: Option<String>,
    /// 本地 Ollama 模型名（手动指定，覆盖默认值）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_model: Option<String>,
    /// 本地 Ollama Base URL（手动指定，覆盖默认值）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ollama_base_url: Option<String>,
    /// Embedding 专用 Ollama 模型（独立于 LLM 模型，默认 nomic-embed-text:latest）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_ollama_model: Option<String>,
    /// Embedding 专用 Ollama Base URL（默认与 ollama_base_url 相同）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_ollama_base_url: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: AppMode::default(),
            vault_path: None,
            query_top_k: None,
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
        }
    }
}

/// LLM Provider 配置（Settings 页面读写接口）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    /// 云端 API Key（空字符串表示未配置）
    #[serde(alias = "openai_api_key", default)]
    pub cloud_api_key: String,
    /// 云端基础地址，兼容 OpenAI / DeepSeek 等 OpenAI-compatible Provider
    #[serde(alias = "openai_base_url", default)]
    pub cloud_base_url: String,
    /// 云端模型名，空字符串时使用默认值
    #[serde(alias = "openai_model", default)]
    pub cloud_model: String,
    /// 云端 Provider 名称，空字符串时使用默认值
    #[serde(alias = "openai_provider_name", default)]
    pub cloud_provider_name: String,
    /// 当前活跃的 provider 类型（"cloud" / "ollama"）
    #[serde(default)]
    pub active_provider: String,
    /// 本地 Ollama 模型名（手动指定）
    #[serde(default)]
    pub ollama_model: String,
    /// 本地 Ollama Base URL（手动指定）
    #[serde(default)]
    pub ollama_base_url: String,
    /// Embedding 专用 Ollama 模型（默认 nomic-embed-text:latest）
    #[serde(default)]
    pub embed_ollama_model: String,
    /// Embedding 专用 Ollama Base URL（默认与 ollama_base_url 相同）
    #[serde(default)]
    pub embed_ollama_base_url: String,
}

/// 应用总览。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppOverview {
    pub app_name: String,
    pub mode: AppMode,
    pub vault_path: String,
    pub recent_log_count: usize,
    pub pending_tasks: usize,
    pub supported_modes: Vec<AppMode>,
}

/// 日志级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// 最近日志项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: u64,
    pub level: LogLevel,
    pub message: String,
    pub created_at: String,
}

/// Lint 问题。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub path: Option<String>,
    pub suggestion: String,
}

/// Lint 严重级别统计。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LintSeverityStats {
    pub error: usize,
    pub warning: usize,
    pub info: usize,
}

fn default_lint_severity_stats() -> LintSeverityStats {
    LintSeverityStats::default()
}

/// Lint 报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintReport {
    pub mode: AppMode,
    pub checked_at: String,
    pub summary: String,
    pub issues: Vec<LintIssue>,
    #[serde(default = "default_lint_severity_stats")]
    pub severity_stats: LintSeverityStats,
}

/// Lint 补丁建议项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintPatchSuggestion {
    pub issue_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub title: String,
    pub proposed_action: String,
    pub patch_preview: String,
}

/// Lint 补丁预览结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintPatchPreview {
    pub generated_at: String,
    pub total: usize,
    pub suggestions: Vec<LintPatchSuggestion>,
}

/// Lint 补丁应用请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintPatchApplyInput {
    pub issue_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Lint 补丁应用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintPatchApplyResult {
    pub issue_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub applied: bool,
    pub message: String,
    pub touched_paths: Vec<String>,
}

/// Lint 补丁批量应用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LintPatchBatchApplyStatus {
    Success,
    Failed,
    Skipped,
}

/// Lint 补丁批量应用项结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintPatchBatchApplyItemResult {
    pub issue_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub status: LintPatchBatchApplyStatus,
    pub applied: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touched_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Lint 补丁批量应用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintPatchBatchApplyResult {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
    pub items: Vec<LintPatchBatchApplyItemResult>,
}

/// Lint 补丁应用事件项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintPatchEventItem {
    pub issue_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub applied: bool,
    pub message: String,
    pub created_at: String,
}

/// Vault 初始化结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultInitResult {
    pub vault_path: String,
    pub created_paths: Vec<String>,
    pub message: String,
}

/// Markdown 导入结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    pub source_path: String,
    pub raw_path: String,
    pub wiki_path: String,
    pub message: String,
    /// LLM 提取的关键实体列表（P1 复利机制）
    #[serde(default)]
    pub entities: Vec<String>,
    /// 被注入反向链接的相关 Wiki 页面路径（P1 复利机制）
    #[serde(default)]
    pub updated_pages: Vec<String>,
}

/// 默认路径集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultPaths {
    pub vault_path: String,
    pub ingest_source_path: String,
}

/// Query 回答引用项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCitation {
    pub page_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_path: Option<String>,
    pub score: usize,
    pub excerpt: String,
}

/// Query 检索路径调试项（用于解释多路召回贡献）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySearchRouteDebug {
    /// 路径标识，如 fts / linked / popular / embedding。
    pub route: String,
    /// 该路径提供的候选数量。
    pub candidate_count: usize,
    /// 候选路径前若干项（用于快速诊断）。
    #[serde(default)]
    pub top_candidates: Vec<String>,
    /// 最终命中页中由该路径贡献的路径。
    #[serde(default)]
    pub contributed_paths: Vec<String>,
}

/// Query 检索调试信息（用于解释 RRF 各路融合结果）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySearchDebug {
    /// 检索策略（如 rrf / fts / scan / empty）。
    pub strategy: String,
    /// RRF 常量 k；非 RRF 策略为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rrf_k: Option<f64>,
    /// 融合后前若干路径。
    #[serde(default)]
    pub fused_top_paths: Vec<String>,
    /// 各路径贡献详情。
    #[serde(default)]
    pub routes: Vec<QuerySearchRouteDebug>,
}

/// Query 回答结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnswerResult {
    pub question: String,
    pub answer: String,
    pub citations: Vec<QueryCitation>,
    pub matched_pages: Vec<String>,
    pub mode: AppMode,
    pub checked_at: String,
    #[serde(default = "default_query_search_strategy")]
    pub search_strategy: String,
    #[serde(default = "default_query_answer_strategy")]
    pub answer_strategy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_debug: Option<QuerySearchDebug>,
}

fn default_query_search_strategy() -> String {
    "empty".to_string()
}

fn default_query_answer_strategy() -> String {
    "rule".to_string()
}

/// Query 运行参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryAskOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
}

/// Query 参数配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySettings {
    pub top_k: usize,
    pub min_top_k: usize,
    pub max_top_k: usize,
}

/// Query 结果保存请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveQueryAnswerInput {
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub citations: Vec<QueryCitation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Query 结果保存结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveQueryAnswerResult {
    pub wiki_path: String,
    pub page_title: String,
    pub message: String,
}

/// Wiki 页面保存结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SaveWikiPageResult {
    pub path: String,
    pub message: String,
}

/// Wiki 页面删除结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeleteWikiPageResult {
    pub path: String,
    pub message: String,
}

/// Wiki 页面重命名结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RenameWikiPageResult {
    /// 重命名后的新路径
    pub new_path: String,
    pub message: String,
}

/// 长时间操作的进度事件载荷（Tauri emit 用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    /// 当前步骤标识，如 "summarizing" / "extracting_entities" / "done"
    pub step: String,
    /// 面向用户的进度描述
    pub message: String,
}

/// LLM 状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmStatus {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub healthy: bool,
    pub message: String,
    pub mode: AppMode,
}

/// Wiki 页面摘要项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageItem {
    pub title: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_path: Option<String>,
    pub summary: String,
    pub updated_at: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Ask 历史单条记录（前端显示用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct AskHistoryItem {
    pub id: i64,
    pub question: String,
    pub created_at: String,
}

/// Outbox 事件项（增量导出与消费确认用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxEventItem {
    pub id: i64,
    pub event_type: String,
    pub payload_json: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumer_tag: Option<String>,
}

/// Outbox ack 结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboxAckResult {
    pub acked: usize,
    pub up_to_id: i64,
    pub consumer_tag: String,
}

/// Ask 会话单轮记录（多轮对话用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskTurn {
    /// "user" 或 "assistant"
    pub role: String,
    pub content: String,
}

/// Wiki 页面详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageDetail {
    pub title: String,
    pub path: String,
    pub display_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<WikiPageFrontmatter>,
    pub content: String,
    pub updated_at: String,
}

/// Wiki 页面 frontmatter 结构化信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageFrontmatter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<String>,
    /// 是否已过时（由用户或 lint 标记）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale: Option<bool>,
}

/// Wiki 页面引用项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageCitationItem {
    pub cited_page_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cited_page_display_path: Option<String>,
    pub score: usize,
    pub excerpt: String,
    pub target_exists: bool,
}


/// 知识图谱节点（对应一个 Wiki 页面）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraphNode {
    /// 节点唯一 ID（页面绝对路径）
    pub id: String,
    /// 显示标签（页面标题）
    pub label: String,
    /// 分组标识（第一个 entity 标签，或空字符串）
    pub group: String,
}

/// 知识图谱边（页面间引用关系）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraphLink {
    /// 来源页面路径
    pub source: String,
    /// 目标页面路径
    pub target: String,
}

/// 知识图谱完整数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeGraphData {
    pub nodes: Vec<KnowledgeGraphNode>,
    pub links: Vec<KnowledgeGraphLink>,
}

/// 知识子图方向过滤。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeGraphDirection {
    Both,
    Out,
    In,
}

impl Default for KnowledgeGraphDirection {
    fn default() -> Self {
        Self::Both
    }
}

/// 知识子图元信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSubgraphMeta {
    pub center_page_path: String,
    pub hop: u8,
    pub direction: KnowledgeGraphDirection,
    pub truncated: bool,
    pub node_count: usize,
    pub link_count: usize,
}

/// 知识子图返回结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSubgraphData {
    pub nodes: Vec<KnowledgeGraphNode>,
    pub links: Vec<KnowledgeGraphLink>,
    pub meta: KnowledgeSubgraphMeta,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum IngestQueueStatus {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl std::fmt::Display for IngestQueueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::Running => write!(f, "running"),
            Self::Done => write!(f, "done"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for IngestQueueStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("unknown status: {}", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestQueueItem {
    pub id: i64,
    pub source_type: String,  // "file" | "url" | "markdown"
    pub source_path: String,
    pub status: String,       // queued/running/done/failed/cancelled
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
