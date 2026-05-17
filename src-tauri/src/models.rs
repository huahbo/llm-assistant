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
    /// Embedding 后端（"onnx" | "ollama" | "disabled"，默认 "onnx"）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_backend: Option<String>,
    /// ONNX 模型名（"multilingual-e5-small" / "bge-small-zh-v1.5"，默认前者）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_onnx_model: Option<String>,
    /// Shell 策略配置（H6-S3：能力最大化与安全收敛）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_policy: Option<ShellPolicyConfig>,
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
            embed_backend: None,
            embed_onnx_model: None,
            shell_policy: None,
        }
    }
}

/// Shell 决策枚举（用于策略配置）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellPolicyDecision {
    AutoAllow,
    RequireApproval,
    Deny,
}

#[allow(dead_code)]
impl ShellPolicyDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShellPolicyDecision::AutoAllow => "auto_allow",
            ShellPolicyDecision::RequireApproval => "require_approval",
            ShellPolicyDecision::Deny => "deny",
        }
    }
}

/// Shell 策略配置（P1：增加 network/script 两个高风险维度）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellPolicyConfig {
    /// `manual` 来源且 action=unknown 时的决策。
    #[serde(default = "default_shell_policy_manual_unknown_decision")]
    pub manual_unknown_decision: ShellPolicyDecision,
    /// `manual` 来源且 action=write 时的决策。
    #[serde(default = "default_shell_policy_manual_write_decision")]
    pub manual_write_decision: ShellPolicyDecision,
    /// `agent` 来源且 action=read 时的决策。
    #[serde(default = "default_shell_policy_agent_read_decision")]
    pub agent_read_decision: ShellPolicyDecision,
    /// `agent` 来源且 action=write 时的决策。
    #[serde(default = "default_shell_policy_agent_write_decision")]
    pub agent_write_decision: ShellPolicyDecision,
    /// `agent` 来源且 action=unknown 时的决策。
    #[serde(default = "default_shell_policy_agent_unknown_decision")]
    pub agent_unknown_decision: ShellPolicyDecision,
    /// 网络类命令（curl/wget/Invoke-WebRequest 等），不区分来源。
    #[serde(default = "default_shell_policy_network_decision")]
    pub network_decision: ShellPolicyDecision,
    /// 脚本类命令（.ps1/.bat/.cmd/.sh），不区分来源。
    #[serde(default = "default_shell_policy_script_decision")]
    pub script_decision: ShellPolicyDecision,
}

fn default_shell_policy_manual_unknown_decision() -> ShellPolicyDecision {
    ShellPolicyDecision::AutoAllow
}

fn default_shell_policy_agent_write_decision() -> ShellPolicyDecision {
    ShellPolicyDecision::RequireApproval
}

fn default_shell_policy_manual_write_decision() -> ShellPolicyDecision {
    ShellPolicyDecision::AutoAllow
}

fn default_shell_policy_agent_read_decision() -> ShellPolicyDecision {
    ShellPolicyDecision::AutoAllow
}

fn default_shell_policy_agent_unknown_decision() -> ShellPolicyDecision {
    ShellPolicyDecision::RequireApproval
}

fn default_shell_policy_network_decision() -> ShellPolicyDecision {
    ShellPolicyDecision::RequireApproval
}

fn default_shell_policy_script_decision() -> ShellPolicyDecision {
    ShellPolicyDecision::RequireApproval
}

impl Default for ShellPolicyConfig {
    fn default() -> Self {
        Self {
            manual_unknown_decision: default_shell_policy_manual_unknown_decision(),
            manual_write_decision: default_shell_policy_manual_write_decision(),
            agent_read_decision: default_shell_policy_agent_read_decision(),
            agent_write_decision: default_shell_policy_agent_write_decision(),
            agent_unknown_decision: default_shell_policy_agent_unknown_decision(),
            network_decision: default_shell_policy_network_decision(),
            script_decision: default_shell_policy_script_decision(),
        }
    }
}

/// Shell 策略档位（用于前端一键切换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum ShellPolicyProfile {
    Strict,
    Balanced,
    PowerUser,
}

#[allow(dead_code)]
impl ShellPolicyConfig {
    pub fn from_profile(profile: ShellPolicyProfile) -> Self {
        match profile {
            ShellPolicyProfile::Strict => Self {
                manual_unknown_decision: ShellPolicyDecision::Deny,
                manual_write_decision: ShellPolicyDecision::RequireApproval,
                agent_read_decision: ShellPolicyDecision::RequireApproval,
                agent_write_decision: ShellPolicyDecision::Deny,
                agent_unknown_decision: ShellPolicyDecision::Deny,
                network_decision: ShellPolicyDecision::Deny,
                script_decision: ShellPolicyDecision::Deny,
            },
            ShellPolicyProfile::Balanced => Self::default(),
            ShellPolicyProfile::PowerUser => Self {
                manual_unknown_decision: ShellPolicyDecision::AutoAllow,
                manual_write_decision: ShellPolicyDecision::AutoAllow,
                agent_read_decision: ShellPolicyDecision::AutoAllow,
                agent_write_decision: ShellPolicyDecision::AutoAllow,
                agent_unknown_decision: ShellPolicyDecision::AutoAllow,
                network_decision: ShellPolicyDecision::AutoAllow,
                script_decision: ShellPolicyDecision::RequireApproval,
            },
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
    /// Embedding 后端（"onnx" | "ollama" | "disabled"）
    #[serde(default)]
    pub embed_backend: String,
    /// ONNX 模型名（"multilingual-e5-small" | "bge-small-zh-v1.5"）
    #[serde(default)]
    pub embed_onnx_model: String,
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

/// 页面快速结构检查结果（无需 LLM，仅文件系统）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageQuickLint {
    pub wiki_path: String,
    /// 内容中 [[wiki-links]] 找不到对应页面的链接列表
    #[serde(default)]
    pub broken_links: Vec<String>,
    /// frontmatter entities 中没有对应 wiki 页面的实体名列表
    #[serde(default)]
    pub missing_entity_pages: Vec<String>,
    /// 总问题数 = broken_links.len() + missing_entity_pages.len()
    pub issues_count: usize,
}

/// 摄入预览信息（仅分析，不落盘）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestPreview {
    pub preview_id: String,
    pub source_path: String,
    pub summary: String,
    #[serde(default)]
    pub entities: Vec<String>,
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

/// Wiki 页面历史列表项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageHistoryItem {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub content_hash: String,
    pub checksum: String,
    pub created_at: String,
}

/// Wiki 页面历史详情，包含覆盖前正文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPageHistoryDetail {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub content_hash: String,
    pub checksum: String,
    pub created_at: String,
    pub content: String,
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

/// Ask 会话摘要（会话管理列表用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskSessionItem {
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub turn_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_turn_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_turn_content: Option<String>,
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

/// Agent Run 摘要项（H0 后端脚手架）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunItem {
    pub id: i64,
    pub topic: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
}

/// Agent Run 事件项（H0 后端脚手架）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunEventItem {
    pub id: i64,
    pub run_id: i64,
    pub level: String,
    pub message: String,
    pub created_at: String,
}

/// Agent Draft 草稿项（H1：草稿生成与审批）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDraftItem {
    pub id: i64,
    pub run_id: i64,
    pub title: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 审批前冲突检测结果（H1 审批弹窗用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDraftConflictInfo {
    pub draft_id: i64,
    /// 草稿标题
    pub title: String,
    /// true = vault/wiki 下已存在同名页面
    pub conflict: bool,
    /// 已有页面的文件路径（仅冲突时有值）
    pub existing_path: Option<String>,
    /// 已有页面前 300 字预览（仅冲突时有值）
    pub existing_preview: Option<String>,
}

/// Ask 会话单轮记录（多轮对话用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskTurn {
    /// "user" 或 "assistant"
    pub role: String,
    pub content: String,
}

/// Ask 会话单轮（附带时间，供前端会话管理展示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskSessionTurnItem {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<QueryCitation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<AskSessionTurnMeta>,
}

/// Ask 会话助手轮次元信息（用于恢复会话时展示引用与检索策略）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskSessionTurnMeta {
    pub mode: AppMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_strategy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_strategy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_pages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_debug: Option<QuerySearchDebug>,
}

/// 跨会话检索命中项（按会话轮次返回）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskSessionSearchHitItem {
    pub session_id: String,
    pub session_title: String,
    pub turn_id: i64,
    pub role: String,
    pub snippet: String,
    pub created_at: String,
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

/// Agent 记忆项（H2：记忆增强，前端传输用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryItem {
    pub id: i64,
    pub run_id: Option<i64>,
    pub memory_key: String,
    pub memory_value: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Agent 技能模板项（H3：技能化基础资产）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkillItem {
    pub id: i64,
    pub skill_key: String,
    pub prompt_template: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
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
    pub source_type: String, // "file" | "url" | "markdown"
    pub source_path: String,
    pub status: String, // queued/running/done/failed/cancelled
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub retry_count: i64,
}

/// Deep Research 任务状态（Display/FromStr 供序列化使用，枚举供类型安全扩展）。
#[allow(dead_code)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum ResearchTaskStatus {
    Queued,
    Decomposing,
    Searching,
    Synthesizing,
    Saving,
    Done,
    Failed,
    Cancelled,
}

impl std::fmt::Display for ResearchTaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::Decomposing => write!(f, "decomposing"),
            Self::Searching => write!(f, "searching"),
            Self::Synthesizing => write!(f, "synthesizing"),
            Self::Saving => write!(f, "saving"),
            Self::Done => write!(f, "done"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for ResearchTaskStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "decomposing" => Ok(Self::Decomposing),
            "searching" => Ok(Self::Searching),
            "synthesizing" => Ok(Self::Synthesizing),
            "saving" => Ok(Self::Saving),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("unknown research status: {}", other)),
        }
    }
}

/// Deep Research 任务记录（前端显示用）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResearchTaskItem {
    pub id: i64,
    pub topic: String,
    pub status: String,
    pub sub_queries: Vec<String>,
    pub web_results_count: i32,
    pub depth: i32,
    pub breadth: i32,
    pub saved_path: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 搜索配置。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchConfig {
    pub search_provider: String,
    pub tavily_api_key: String,
    pub searxng_url: String,
    pub brave_api_key: String,
    pub breadth: i32,
    pub depth: i32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            search_provider: "none".into(),
            tavily_api_key: "".into(),
            searxng_url: "http://localhost:8080".into(),
            brave_api_key: "".into(),
            breadth: 3,
            depth: 1,
        }
    }
}

/// Web 搜索结果（内部使用）。
pub struct WebSearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub source: String,
}

/// 被引用次数统计（单条记录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitedPageStat {
    pub path: String,
    pub title: String,
    pub citation_count: usize,
}

/// 摄入来源分布（单条记录）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSourceCount {
    pub source_type: String,
    pub count: usize,
}

/// Vault 知识库统计数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_pages: usize,
    pub pages_last_7_days: usize,
    pub pages_last_30_days: usize,
    pub orphan_pages: usize,
    pub total_citations: usize,
    pub top_cited_pages: Vec<CitedPageStat>,
    pub ingest_source_counts: Vec<IngestSourceCount>,
}

/// AI 辅助新建 Wiki 页面的返回结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPageResult {
    pub wiki_path: String,
    pub title: String,
    pub content_preview: String, // 前 300 字符
}

/// Agent 写入审批队列中的待处理条目（H6-S2）。
/// old_str=None 表示全量覆盖写入；old_str=Some 表示精确替换（edit_wiki）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingAgentWrite {
    pub run_id: i64,
    pub resolved_path: String,
    pub content: String,
    pub old_str: Option<String>,
    pub created_at: String,
}

/// Shell 命令执行结果（H6-S1：Agent Studio PowerShell 执行能力）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellResult {
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub working_dir: String,
    pub blocked: bool,
    pub blocked_reason: Option<String>,
    pub policy_action: String,
    pub policy_decision: String,
    pub executor: String,
}

/// Shell 审计事件（H6-P3：每次命令执行均落库）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAuditEvent {
    pub id: i64,
    pub command: String,
    pub working_dir: String,
    pub policy_action: String,
    pub policy_decision: String,
    pub executor: String,
    pub blocked: bool,
    pub blocked_reason: Option<String>,
    pub exit_code: Option<i32>,
    pub latency_ms: Option<i64>,
    pub session_id: Option<String>,
    pub created_at: String,
}

/// Shell 会话元信息（H6-S3：会话型终端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellSessionInfo {
    pub session_id: String,
    pub working_dir: String,
    pub executor: String,
    pub created_at: String,
}

/// Shell 流式输出事件载荷（前端通过 tauri event 订阅）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellStreamChunk {
    pub stream_id: String,
    pub session_id: Option<String>,
    pub chunk: String,
    pub stream: String, // stdout | stderr | system
    pub done: bool,
}

/// 文件附件块——从本地文件提取的纯文本，供 Chat 消息上下文使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunk {
    pub filename: String,
    pub content: String,
    pub char_count: usize,
    pub truncated: bool,
}

/// Embedding 后端状态（Phase 5 — Settings UI 用）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmbedStatus {
    /// 后端标识，如 "onnx:multilingual-e5-small" / "ollama:nomic-embed-text" / "noop"
    pub backend_id: String,
    /// 向量维度（0 = noop 或未知）
    pub dimension: usize,
    /// 已索引页面数量
    pub indexed_count: usize,
    /// 后端是否健康可用
    pub healthy: bool,
}

/// Smithery MCP Registry 服务器列表项（H17）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SmitheryServer {
    pub qualified_name: String,
    pub display_name: String,
    pub description: String,
    pub verified: bool,
    pub use_count: u64,
    pub icon_url: Option<String>,
}

/// Smithery MCP Registry 服务器详情（H17）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SmitheryServerDetail {
    pub qualified_name: String,
    pub display_name: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    pub required_env_keys: Vec<String>,
}
