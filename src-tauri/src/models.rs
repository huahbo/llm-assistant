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
    #[serde(alias = "openai_api_key", default, skip_serializing_if = "Option::is_none")]
    pub cloud_api_key: Option<String>,
    /// 云端基础地址，兼容 OpenAI / DeepSeek 等 OpenAI-compatible Provider
    #[serde(alias = "openai_base_url", default, skip_serializing_if = "Option::is_none")]
    pub cloud_base_url: Option<String>,
    /// 云端模型名
    #[serde(alias = "openai_model", default, skip_serializing_if = "Option::is_none")]
    pub cloud_model: Option<String>,
    /// 云端 Provider 名称
    #[serde(alias = "openai_provider_name", default, skip_serializing_if = "Option::is_none")]
    pub cloud_provider_name: Option<String>,
    /// 当前活跃 Provider
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider: Option<String>,
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

/// URL ingest 输入
#[derive(Debug, Clone, serde::Deserialize)]
pub struct IngestUrlInput {
    pub url: String,
}
