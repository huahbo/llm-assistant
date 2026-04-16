use tauri::State;

use crate::models::{
    AppMode, AppOverview, DefaultPaths, IngestResult, LintPatchApplyInput, LintPatchApplyResult,
    LintPatchBatchApplyResult, LintPatchEventItem, LintPatchPreview, LintReport, LlmProviderConfig,
    LlmStatus, LogEntry, ModeChangeResult, QueryAnswerResult, QueryAskOptions, QuerySettings,
    SaveQueryAnswerInput, SaveQueryAnswerResult, VaultInitResult, WikiPageCitationItem,
    WikiPageDetail, WikiPageItem,
};
use crate::state::AppState;

const RECENT_LOG_LIMIT: usize = 10;
const RECENT_WIKI_LIMIT: usize = 20;
const RECENT_LINT_PATCH_EVENT_LIMIT: usize = 20;
const SEARCH_WIKI_LIMIT: usize = 50;

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

/// 返回 Query 参数配置。
#[tauri::command]
pub fn get_query_settings(state: State<'_, AppState>) -> QuerySettings {
    state.query_settings()
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

/// 返回最近更新的 wiki 页面。
#[tauri::command]
pub fn get_recent_wiki_pages(state: State<'_, AppState>) -> Result<Vec<WikiPageItem>, String> {
    state.recent_wiki_pages(RECENT_WIKI_LIMIT)
}

/// 按关键字搜索 wiki 页面。
#[tauri::command]
pub fn search_wiki_pages(
    keyword: String,
    state: State<'_, AppState>,
) -> Result<Vec<WikiPageItem>, String> {
    state.search_wiki_pages(keyword, SEARCH_WIKI_LIMIT)
}

/// 读取指定 wiki 页面详情。
#[tauri::command]
pub fn get_wiki_page_detail(
    page_path: String,
    state: State<'_, AppState>,
) -> Result<WikiPageDetail, String> {
    state.wiki_page_detail(page_path)
}

/// 读取指定 wiki 页面被哪些页面引用。
#[tauri::command]
pub fn get_wiki_page_citations(
    page_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<WikiPageCitationItem>, String> {
    state.wiki_page_citations(page_path)
}

/// 返回当前 lint 报告（规则检查 + LLM 语义分析）。
#[tauri::command]
pub async fn run_lint(state: State<'_, AppState>) -> Result<LintReport, String> {
    let future = state.lint_report_full_future();
    drop(state);
    Ok(future.await)
}

/// 预览 Lint 建议补丁。
#[tauri::command]
pub fn preview_lint_patches(state: State<'_, AppState>) -> LintPatchPreview {
    state.preview_lint_patches()
}

/// 手动应用 Lint 补丁。
#[tauri::command]
pub fn apply_lint_patch(
    input: LintPatchApplyInput,
    state: State<'_, AppState>,
) -> Result<LintPatchApplyResult, String> {
    state.apply_lint_patch(input)
}

/// 批量应用 Lint 补丁。
#[tauri::command]
pub fn apply_lint_patches_batch(
    inputs: Vec<LintPatchApplyInput>,
    state: State<'_, AppState>,
) -> Result<LintPatchBatchApplyResult, String> {
    state.apply_lint_patches_batch(inputs)
}

/// 返回最近的 Lint 补丁应用事件。
#[tauri::command]
pub fn get_recent_lint_patch_events(
    state: State<'_, AppState>,
) -> Result<Vec<LintPatchEventItem>, String> {
    state.recent_lint_patch_events(RECENT_LINT_PATCH_EVENT_LIMIT)
}

/// 返回 LLM 状态。
#[tauri::command]
pub async fn get_llm_status(state: State<'_, AppState>) -> Result<LlmStatus, String> {
    let future = state.llm_status_future();
    drop(state);
    Ok(future.await)
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
pub async fn ingest_markdown(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<IngestResult, String> {
    eprintln!("[ingest_markdown] called with source_path={}", source_path);
    state
        .ingest_markdown(std::path::PathBuf::from(source_path))
        .await
}

/// 导入任意支持格式文件（按扩展名自动路由）。
#[tauri::command]
pub async fn ingest_file(
    source_path: String,
    ocr_provider: Option<String>,
    state: State<'_, AppState>,
) -> Result<IngestResult, String> {
    eprintln!(
        "[ingest_file] called with source_path={}, ocr_provider={}",
        source_path,
        ocr_provider.as_deref().unwrap_or("tesseract")
    );
    state
        .ingest_file_impl(&source_path, ocr_provider.as_deref())
        .await
}

/// 导入 PDF（提取文本后复用 Markdown ingest 流程）。
#[tauri::command]
pub async fn ingest_pdf(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<IngestResult, String> {
    eprintln!("[ingest_pdf] called with source_path={}", source_path);
    state.ingest_pdf_impl(&source_path).await
}

/// 问答查询。
#[tauri::command]
pub async fn query_ask(
    question: String,
    state: State<'_, AppState>,
) -> Result<QueryAnswerResult, String> {
    eprintln!("[query_ask] called with question={}", question);
    state.query_ask(question).await
}

/// 问答查询（带参数）。
#[tauri::command]
pub async fn query_ask_with_options(
    question: String,
    options: Option<QueryAskOptions>,
    state: State<'_, AppState>,
) -> Result<QueryAnswerResult, String> {
    eprintln!("[query_ask_with_options] called with question={}", question);
    state
        .query_ask_with_options(question, options.unwrap_or_default())
        .await
}

/// 保存 Query TopK 配置。
#[tauri::command]
pub fn set_query_top_k(top_k: usize, state: State<'_, AppState>) -> Result<QuerySettings, String> {
    eprintln!("[set_query_top_k] called with top_k={}", top_k);
    state.set_query_top_k(top_k)
}

/// 保存 Query 结果到 Wiki。
#[tauri::command]
pub fn save_query_answer(
    input: SaveQueryAnswerInput,
    state: State<'_, AppState>,
) -> Result<SaveQueryAnswerResult, String> {
    eprintln!("[save_query_answer] called");
    state.save_query_answer(input)
}

/// 拉取 URL 内容并执行 ingest，与 ingest_markdown 共享返回结构。
#[tauri::command]
pub async fn ingest_url(
    state: tauri::State<'_, AppState>,
    url: String,
) -> Result<crate::models::IngestResult, String> {
    eprintln!("[ingest_url] called with url={}", url);
    state.ingest_url_impl(&url).await
}

/// 读取云端 Provider 配置（Settings 页面用）。
#[tauri::command]
pub fn get_llm_config(state: State<'_, AppState>) -> LlmProviderConfig {
    state.get_llm_config()
}

/// 保存云端 Provider 配置。
#[tauri::command]
pub fn set_llm_config(
    config: LlmProviderConfig,
    state: State<'_, AppState>,
) -> Result<LlmProviderConfig, String> {
    eprintln!(
        "[set_llm_config] called with active_provider={}, cloud_provider_name={}",
        config.active_provider, config.cloud_provider_name
    );
    state.set_llm_config(config)
}

/// 读取默认 OCR Provider 配置。
#[tauri::command]
pub fn get_ocr_config(state: State<'_, AppState>) -> Option<String> {
    state.get_ocr_config()
}

/// 保存默认 OCR Provider 配置。
#[tauri::command]
pub fn set_ocr_config(
    state: State<'_, AppState>,
    provider: Option<String>,
) -> Result<(), String> {
    eprintln!(
        "[set_ocr_config] called with provider={}",
        provider.as_deref().unwrap_or("None")
    );
    state.set_ocr_config(provider)
}

/// 将编辑后的 Markdown 内容写回 vault 文件并更新 FTS 索引。
#[tauri::command]
pub async fn save_wiki_page(
    state: tauri::State<'_, AppState>,
    path: String,
    content: String,
) -> Result<crate::models::SaveWikiPageResult, String> {
    eprintln!("[save_wiki_page] called with path={}", path);
    state.save_wiki_page_impl(&path, &content).await
}
