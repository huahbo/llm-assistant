use tauri::State;

use crate::models::{
    AppMode, AppOverview, DefaultPaths, IngestResult, KnowledgeGraphData,
    KnowledgeGraphDirection, KnowledgeSubgraphData, LintPatchApplyInput, LintPatchApplyResult,
    LintPatchBatchApplyResult, LintPatchEventItem, LintPatchPreview, LintReport,
    LlmProviderConfig, LlmStatus, LogEntry, ModeChangeResult, OutboxAckResult, OutboxEventItem,
    QueryAnswerResult, QueryAskOptions, QuerySettings, ResearchTaskItem, SaveQueryAnswerInput,
    SaveQueryAnswerResult, SearchConfig, VaultInitResult, WikiPageCitationItem, WikiPageDetail,
    WikiPageItem,
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

/// 按查询词搜索所有 wiki 页面路径并进行模糊匹配（忽略大小写）。
#[tauri::command]
pub fn search_wiki_paths(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    state.search_wiki_paths(query)
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
    Ok(state.run_lint_with_outbox().await)
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

/// 返回 LLM Provider 预设模型描述列表。
#[tauri::command]
pub fn get_llm_provider_presets() -> Vec<(String, String, String)> {
    vec![
        ("OpenAI".to_string(), "https://api.openai.com/v1".to_string(), "gpt-4o-mini".to_string()),
        ("DeepSeek".to_string(), "https://api.deepseek.com".to_string(), "deepseek-chat".to_string()),
        ("Moonshot".to_string(), "https://api.moonshot.cn/v1".to_string(), "moonshot-v1-8k".to_string()),
    ]
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

/// 重命名 Wiki 页面（文件系统 + DB path 同步）。
#[tauri::command]
pub async fn rename_wiki_page(
    state: tauri::State<'_, AppState>,
    old_path: String,
    new_name: String,
) -> Result<crate::models::RenameWikiPageResult, String> {
    eprintln!("[rename_wiki_page] old={} new_name={}", old_path, new_name);
    state.rename_wiki_page_impl(&old_path, &new_name).await
}

/// 删除 Wiki 页面（.md 文件 + DB 记录）。
#[tauri::command]
pub async fn delete_wiki_page(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<crate::models::DeleteWikiPageResult, String> {
    eprintln!("[delete_wiki_page] called with path={}", path);
    state.delete_wiki_page_impl(&path).await
}

/// 保存 Ask 问题到历史（answer 暂不存储）。
#[tauri::command]
pub fn save_ask_history(
    state: tauri::State<'_, AppState>,
    question: String,
) -> Result<(), String> {
    state.save_ask_history_impl(&question)
}

/// 读取 Ask 历史（最近 N 条）。
#[tauri::command]
pub fn get_ask_history(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<crate::models::AskHistoryItem>, String> {
    state.get_ask_history_impl(limit.unwrap_or(30))
}

/// 清空 Ask 历史（返回删除条数）。
#[tauri::command]
pub fn clear_ask_history(
    state: tauri::State<'_, AppState>,
) -> Result<usize, String> {
    state.clear_ask_history_impl()
}

/// 按 id 增量读取 outbox 事件。
#[tauri::command]
pub fn get_outbox_events(
    state: tauri::State<'_, AppState>,
    last_id: Option<i64>,
    limit: Option<usize>,
) -> Result<Vec<OutboxEventItem>, String> {
    state.get_outbox_events_impl(last_id.unwrap_or(0), limit.unwrap_or(200))
}

/// 标记 outbox 事件已消费。
#[tauri::command]
pub fn ack_outbox_events(
    state: tauri::State<'_, AppState>,
    up_to_id: i64,
    consumer_tag: String,
) -> Result<OutboxAckResult, String> {
    state.ack_outbox_events_impl(up_to_id, &consumer_tag)
}

/// 多轮会话问答（携带 session_id 和历史上下文）。
#[tauri::command]
pub async fn query_ask_session(
    session_id: String,
    question: String,
    options: Option<QueryAskOptions>,
    state: State<'_, AppState>,
) -> Result<QueryAnswerResult, String> {
    eprintln!("[query_ask_session] session={}, question={}", session_id, question);
    state
        .query_ask_session(session_id, question, options.unwrap_or_default())
        .await
}

/// 取消正在进行的会话查询。
#[tauri::command]
pub fn cancel_ask_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.cancel_ask_session(session_id)
}

/// 清空会话历史（开启新对话）。
#[tauri::command]
pub fn clear_ask_session(
    session_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.clear_ask_session(session_id)
}

/// 设置或取消 Wiki 页面的 stale 标记。
#[tauri::command]
pub fn mark_page_stale(
    page_path: String,
    stale: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.set_page_stale(page_path, stale)
}

/// 获取知识图谱数据（节点 = wiki 页面，边 = 引用关系）。
#[tauri::command]
pub fn get_knowledge_graph(
    state: State<'_, AppState>,
) -> Result<KnowledgeGraphData, String> {
    state.get_knowledge_graph_impl()
}

/// 获取知识子图（中心节点 + hop + 方向 + 返回上限）。
#[tauri::command]
pub fn get_knowledge_subgraph(
    center_page_path: String,
    hop: Option<u8>,
    direction: Option<KnowledgeGraphDirection>,
    limit_nodes: Option<usize>,
    limit_links: Option<usize>,
    state: State<'_, AppState>,
) -> Result<KnowledgeSubgraphData, String> {
    state.get_knowledge_subgraph_impl(
        center_page_path,
        hop.unwrap_or(2),
        direction.unwrap_or_default(),
        limit_nodes.unwrap_or(500),
        limit_links.unwrap_or(2000),
    )
}

/// 将一个 ingest 任务加入持久化队列，立即返回队列 id。
#[tauri::command]
pub fn enqueue_ingest(
    source_type: String,
    source_path: String,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    state.enqueue_ingest(source_type, source_path)
}

/// 列出所有 ingest 队列记录。
#[tauri::command]
pub fn list_ingest_queue(
    state: State<'_, AppState>,
) -> Result<Vec<crate::models::IngestQueueItem>, String> {
    state.list_ingest_queue()
}

/// 取消一条 queued 状态的 ingest 队列记录。
#[tauri::command]
pub fn cancel_ingest_item(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_ingest_item(id)
}

/// 将失败/取消的 ingest 队列记录重置为 queued 以重试。
#[tauri::command]
pub fn retry_ingest_item(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.retry_ingest_item(id)
}

/// 计算给定页面路径列表中存在 embedding 的页面对余弦相似度。
/// 返回 key="pathA||pathB"（字典序），value=相似度（仅返回 >= 0.25 的对，最多 1000 对）。
#[tauri::command]
pub fn get_page_embedding_similarities(
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, f64>, String> {
    state.get_page_embedding_similarities(paths)
}

// ─── Deep Research Commands ───────────────────────────────────────────────────

/// 启动 Deep Research 任务，返回 task_id。
#[tauri::command]
pub async fn start_research(
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
    topic: String,
    depth: i32,
    breadth: i32,
) -> Result<i64, String> {
    state.start_research(app_handle, topic, depth, breadth)
}

/// 列出最近研究任务。
#[tauri::command]
pub fn list_research_tasks(state: State<'_, AppState>) -> Result<Vec<ResearchTaskItem>, String> {
    state.list_research_tasks()
}

/// 取消指定研究任务。
#[tauri::command]
pub fn cancel_research_task(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_research_task(id)
}

/// 获取搜索配置。
#[tauri::command]
pub fn get_search_config(state: State<'_, AppState>) -> SearchConfig {
    state.get_search_config()
}

/// 更新搜索配置。
#[tauri::command]
pub fn set_search_config(state: State<'_, AppState>, config: SearchConfig) -> Result<(), String> {
    state.set_search_config(config)
}
