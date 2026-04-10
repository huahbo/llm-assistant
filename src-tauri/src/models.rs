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
}

/// Wiki 页面详情。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageDetail {
    pub title: String,
    pub path: String,
    pub display_path: String,
    pub content: String,
    pub updated_at: String,
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
