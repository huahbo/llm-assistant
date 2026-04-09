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

/// Lint 报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintReport {
    pub mode: AppMode,
    pub checked_at: String,
    pub summary: String,
    pub issues: Vec<LintIssue>,
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
}

/// Query 运行参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryAskOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
}
