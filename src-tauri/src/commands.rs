use tauri::State;

use crate::models::{
    AppMode, AppOverview, DefaultPaths, IngestResult, LintReport, LogEntry, ModeChangeResult,
    QueryAnswerResult, QueryAskOptions, VaultInitResult,
};
use crate::state::AppState;

const RECENT_LOG_LIMIT: usize = 10;

/// 返回应用总览。
#[tauri::command]
pub fn get_app_overview(state: State<'_, AppState>) -> AppOverview {
    state.overview()
}

/// 返回默认路径。
#[tauri::command]
pub fn get_default_paths(state: State<'_, AppState>) -> DefaultPaths {
    state.default_paths()
}

/// 切换运行模式。
#[tauri::command]
pub fn set_mode(mode: AppMode, state: State<'_, AppState>) -> ModeChangeResult {
    state.set_mode(mode)
}

/// 返回最近日志。
#[tauri::command]
pub fn get_recent_logs(state: State<'_, AppState>) -> Vec<LogEntry> {
    state.recent_logs(RECENT_LOG_LIMIT)
}

/// 返回当前 lint 报告。
#[tauri::command]
pub fn run_lint(state: State<'_, AppState>) -> LintReport {
    state.lint_report()
}

/// 初始化 Vault。
#[tauri::command]
pub fn init_vault(
    vault_path: String,
    state: State<'_, AppState>,
) -> Result<VaultInitResult, String> {
    eprintln!("[init_vault] called with vault_path={}", vault_path);
    state.init_vault(std::path::PathBuf::from(vault_path))
}

/// 导入 Markdown。
#[tauri::command]
pub fn ingest_markdown(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<IngestResult, String> {
    eprintln!("[ingest_markdown] called with source_path={}", source_path);
    state.ingest_markdown(std::path::PathBuf::from(source_path))
}

/// 问答查询。
#[tauri::command]
pub fn query_ask(
    question: String,
    state: State<'_, AppState>,
) -> Result<QueryAnswerResult, String> {
    eprintln!("[query_ask] called with question={}", question);
    state.query_ask(question)
}

/// 问答查询（带参数）。
#[tauri::command]
pub fn query_ask_with_options(
    question: String,
    options: Option<QueryAskOptions>,
    state: State<'_, AppState>,
) -> Result<QueryAnswerResult, String> {
    eprintln!("[query_ask_with_options] called with question={}", question);
    state.query_ask_with_options(question, options.unwrap_or_default())
}
