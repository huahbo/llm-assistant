use tauri::{AppHandle, Manager, State};

use crate::models::{
    AgentDraftConflictInfo, AgentDraftItem, AgentMemoryItem, AgentRunEventItem, AgentRunItem,
    AgentSkillItem, AppMode, AppOverview, AskSessionItem, AskSessionSearchHitItem,
    AskSessionTurnItem, DefaultPaths, IngestPreview, IngestResult, KnowledgeGraphData,
    KnowledgeGraphDirection, KnowledgeSubgraphData, LintPatchApplyInput, LintPatchApplyResult,
    LintPatchBatchApplyResult, LintPatchEventItem, LintPatchPreview, LintReport, LlmProviderConfig,
    LlmStatus, LogEntry, ModeChangeResult, NewPageResult, OutboxAckResult, OutboxEventItem,
    EmbedStatus, PageQuickLint, QueryAnswerResult, QueryAskOptions, QuerySettings, ResearchTaskItem,
    SaveQueryAnswerInput, SaveQueryAnswerResult, SearchConfig, ShellAuditEvent, ShellPolicyConfig,
    ShellResult, ShellSessionInfo, VaultInitResult, VaultStats, WikiPageCitationItem,
    WikiPageDetail, WikiPageHistoryDetail, WikiPageHistoryItem, WikiPageItem,
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

/// FTS5 + 向量双路召回 wiki 页面（RRF 融合）；Ollama 不可用时自动降级为纯 FTS5。
#[tauri::command]
pub async fn search_wiki_pages_hybrid(
    keyword: String,
    state: State<'_, AppState>,
) -> Result<Vec<WikiPageItem>, String> {
    state.search_wiki_pages_hybrid(keyword, SEARCH_WIKI_LIMIT).await
}

/// 按查询词搜索所有 wiki 页面路径并进行模糊匹配（忽略大小写）。
#[tauri::command]
pub fn search_wiki_paths(query: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
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

/// 读取指定 Wiki 页面的历史快照列表。
#[tauri::command]
pub fn list_wiki_page_history(
    path: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<WikiPageHistoryItem>, String> {
    state.list_wiki_page_history_impl(&path, limit)
}

/// 读取单条 Wiki 页面历史快照正文。
#[tauri::command]
pub fn get_wiki_page_history_entry(
    id: i64,
    state: State<'_, AppState>,
) -> Result<WikiPageHistoryDetail, String> {
    state.get_wiki_page_history_entry_impl(id)
}

/// 从历史快照恢复 Wiki 页面内容（"一键恢复到此版本"）。
#[tauri::command]
pub async fn restore_wiki_page_from_history(
    id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<crate::models::SaveWikiPageResult, String> {
    state.restore_wiki_page_from_history_impl(id).await
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
        (
            "OpenAI".to_string(),
            "https://api.openai.com/v1".to_string(),
            "gpt-4o-mini".to_string(),
        ),
        (
            "DeepSeek".to_string(),
            "https://api.deepseek.com".to_string(),
            "deepseek-v4-flash".to_string(),
        ),
        (
            "Moonshot".to_string(),
            "https://api.moonshot.cn/v1".to_string(),
            "moonshot-v1-8k".to_string(),
        ),
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

/// 使用模板初始化 Vault。
#[tauri::command]
pub fn init_vault_with_template(
    vault_path: String,
    template_schema: String,
    template_purpose: String,
    extra_dirs: Vec<String>,
    state: State<'_, AppState>,
) -> Result<VaultInitResult, String> {
    eprintln!(
        "[init_vault_with_template] called with vault_path={}",
        vault_path
    );
    state.init_vault_with_template(
        std::path::PathBuf::from(vault_path),
        template_schema,
        template_purpose,
        extra_dirs,
    )
}

#[tauri::command]
pub fn get_clip_server_status() -> String {
    if crate::clip_server::is_clip_server_running() {
        "running".to_string()
    } else {
        "stopped".to_string()
    }
}

/// 导入 Markdown。
#[tauri::command]
pub async fn ingest_markdown(
    source_path: String,
    state: State<'_, AppState>,
) -> Result<IngestResult, String> {
    eprintln!("[ingest_markdown] called with source_path={}", source_path);
    state
        .ingest_markdown(std::path::PathBuf::from(source_path), None)
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

/// 预览摄入分析（不写入 Vault）。
#[tauri::command]
pub async fn preview_ingest_file(
    source_type: String,
    source_path: String,
    ocr_provider: Option<String>,
    state: State<'_, AppState>,
) -> Result<IngestPreview, String> {
    eprintln!(
        "[preview_ingest_file] source_type={}, source_path={}, ocr_provider={}",
        source_type,
        source_path,
        ocr_provider.as_deref().unwrap_or("tesseract")
    );
    state
        .preview_ingest_file(&source_type, &source_path, ocr_provider.as_deref())
        .await
}

/// 应用已审批的摄入预览并真正落盘。
#[tauri::command]
pub async fn apply_ingest_preview(
    preview_id: String,
    state: State<'_, AppState>,
) -> Result<IngestResult, String> {
    eprintln!("[apply_ingest_preview] preview_id={}", preview_id);
    state.apply_ingest_preview(&preview_id).await
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

/// 保存云端 Provider 配置，并在后台非阻塞地重新初始化 Embed Provider。
#[tauri::command]
pub async fn set_llm_config(
    config: LlmProviderConfig,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<LlmProviderConfig, String> {
    eprintln!(
        "[set_llm_config] called with active_provider={}, cloud_provider_name={}",
        config.active_provider, config.cloud_provider_name
    );
    let result = state.set_llm_config(config)?;
    // embed 配置变更后在后台重新初始化（ONNX 加载可能耗时，不阻塞命令返回）
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || {
        let s: State<'_, AppState> = app2.state();
        s.init_embed_provider();
    });
    Ok(result)
}

/// 读取默认 OCR Provider 配置。
#[tauri::command]
pub fn get_ocr_config(state: State<'_, AppState>) -> Option<String> {
    state.get_ocr_config()
}

/// 保存默认 OCR Provider 配置。
#[tauri::command]
pub fn set_ocr_config(state: State<'_, AppState>, provider: Option<String>) -> Result<(), String> {
    eprintln!(
        "[set_ocr_config] called with provider={}",
        provider.as_deref().unwrap_or("None")
    );
    state.set_ocr_config(provider)
}

/// 读取 Shell 策略配置。
#[tauri::command]
pub fn get_shell_policy_config(state: State<'_, AppState>) -> ShellPolicyConfig {
    state.get_shell_policy_config()
}

/// 保存 Shell 策略配置。
#[tauri::command]
pub fn set_shell_policy_config(
    config: ShellPolicyConfig,
    state: State<'_, AppState>,
) -> Result<ShellPolicyConfig, String> {
    state.set_shell_policy_config(config)
}

/// 将编辑后的 Markdown 内容写回 vault 文件并更新 FTS 索引。
/// 可选的 expected_checksum 用于在写入前校验文件未被外部修改（协作安全）。
#[tauri::command]
pub async fn save_wiki_page(
    state: tauri::State<'_, AppState>,
    path: String,
    content: String,
    expected_checksum: Option<String>,
) -> Result<crate::models::SaveWikiPageResult, String> {
    eprintln!("[save_wiki_page] called with path={}", path);
    state
        .save_wiki_page_impl(&path, &content, expected_checksum.as_deref())
        .await
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
pub fn save_ask_history(state: tauri::State<'_, AppState>, question: String) -> Result<(), String> {
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
pub fn clear_ask_history(state: tauri::State<'_, AppState>) -> Result<usize, String> {
    state.clear_ask_history_impl()
}

/// 创建 Ask 会话（若已存在则刷新更新时间）。
#[tauri::command]
pub fn create_ask_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
    title: Option<String>,
) -> Result<AskSessionItem, String> {
    state.create_ask_session_impl(&session_id, title.as_deref())
}

/// 读取 Ask 会话列表（最近 N 条）。
#[tauri::command]
pub fn list_ask_sessions(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<AskSessionItem>, String> {
    state.list_ask_sessions_impl(limit.unwrap_or(50))
}

/// 读取指定 Ask 会话轮次（正序）。
#[tauri::command]
pub fn get_ask_session_turns(
    state: tauri::State<'_, AppState>,
    session_id: String,
    limit: Option<usize>,
) -> Result<Vec<AskSessionTurnItem>, String> {
    state.list_ask_session_turns_impl(&session_id, limit.unwrap_or(400))
}

/// 跨会话检索 Ask 轮次内容（按时间倒序）。
#[tauri::command]
pub fn search_ask_session_turns(
    state: tauri::State<'_, AppState>,
    keyword: String,
    limit: Option<usize>,
) -> Result<Vec<AskSessionSearchHitItem>, String> {
    state.search_ask_session_turns_impl(&keyword, limit.unwrap_or(50))
}

/// 重命名 Ask 会话标题。
#[tauri::command]
pub fn rename_ask_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
    title: String,
) -> Result<(), String> {
    state.rename_ask_session_impl(&session_id, &title)
}

/// 删除 Ask 会话（含会话轮次）。
#[tauri::command]
pub fn delete_ask_session(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Result<usize, String> {
    state.delete_ask_session_impl(&session_id)
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

/// 创建 Agent Run（H0）。
#[tauri::command]
pub fn start_agent_run(topic: String, state: State<'_, AppState>) -> Result<i64, String> {
    state.start_agent_run_impl(&topic)
}

/// 追加 Agent Run 事件（H0）。
#[tauri::command]
pub fn append_agent_run_event(
    run_id: i64,
    level: String,
    message: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.append_agent_run_event_impl(run_id, &level, &message)
}

/// 列出 Agent Runs（H0）。
#[tauri::command]
pub fn list_agent_runs(
    limit: Option<i64>,
    include_archived: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<AgentRunItem>, String> {
    state.list_agent_runs_impl(limit, include_archived)
}

/// 列出指定 Agent Run 的事件（H0）。
#[tauri::command]
pub fn list_agent_run_events(
    run_id: i64,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<AgentRunEventItem>, String> {
    state.list_agent_run_events_impl(run_id, limit)
}

/// 结束 Agent Run（H0）。
#[tauri::command]
pub fn complete_agent_run(
    run_id: i64,
    status: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.complete_agent_run_impl(run_id, &status)
}

/// 归档 Agent Run（软删除）。
#[tauri::command]
pub fn archive_agent_run(run_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.archive_agent_run_impl(run_id)
}

/// 恢复已归档 Agent Run。
#[tauri::command]
pub fn restore_agent_run(run_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.restore_agent_run_impl(run_id)
}

/// 生成 Agent Draft（H1）。
#[tauri::command]
pub async fn generate_agent_draft(
    run_id: i64,
    topic: String,
    skill_key: Option<String>,
    research_mode: Option<bool>,
    ask_first: Option<bool>,
    state: State<'_, AppState>,
) -> Result<AgentDraftItem, String> {
    state
        .generate_agent_draft_impl(
            run_id,
            topic,
            skill_key,
            research_mode.unwrap_or(false),
            ask_first.unwrap_or(false),
        )
        .await
}

/// 执行 Agent 任务模式（H6-S2 skeleton）。
#[tauri::command]
pub async fn run_agent_task(
    run_id: i64,
    instruction: String,
    max_iterations: Option<u32>,
    memory_context: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .run_agent_task_impl(run_id, instruction, max_iterations, memory_context)
        .await
}

/// 基于批注重写 Agent Draft（H5-A）。
#[tauri::command]
pub async fn rewrite_agent_draft(
    draft_id: i64,
    comment: String,
    state: State<'_, AppState>,
) -> Result<AgentDraftItem, String> {
    state.rewrite_agent_draft_impl(draft_id, comment).await
}

/// 列出指定 Run 的草稿（H1）。
#[tauri::command]
pub fn list_agent_drafts(
    run_id: i64,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<AgentDraftItem>, String> {
    state.list_agent_drafts_impl(run_id, limit)
}

/// 审批前冲突预检：检测同名 Wiki 页面是否已存在（H1）。
#[tauri::command]
pub fn check_agent_draft_conflict(
    draft_id: i64,
    state: State<'_, AppState>,
) -> Result<AgentDraftConflictInfo, String> {
    state.check_agent_draft_conflict_impl(draft_id)
}

/// 审批并写盘草稿（H1）。
#[tauri::command]
pub async fn approve_agent_draft(
    draft_id: i64,
    state: State<'_, AppState>,
) -> Result<NewPageResult, String> {
    state.approve_agent_draft_impl(draft_id).await
}

/// 写入或更新 agent 记忆（H2）。
#[tauri::command]
pub fn upsert_agent_memory(
    run_id: Option<i64>,
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<AgentMemoryItem, String> {
    state.upsert_agent_memory_impl(run_id, &key, &value)
}

/// 列出 agent 记忆（H2）。
#[tauri::command]
pub fn list_agent_memories(
    run_id: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<AgentMemoryItem>, String> {
    state.list_agent_memories_impl(run_id, limit)
}

/// 删除单条 agent 记忆（H2）。
#[tauri::command]
pub fn delete_agent_memory(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.delete_agent_memory_impl(id)
}

/// 写入或更新 agent 技能模板（H3）。
#[tauri::command]
pub fn upsert_agent_skill(
    skill_key: String,
    prompt_template: String,
    state: State<'_, AppState>,
) -> Result<AgentSkillItem, String> {
    state.upsert_agent_skill_impl(&skill_key, &prompt_template)
}

/// 列出 agent 技能模板（H3）。
#[tauri::command]
pub fn list_agent_skills(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<AgentSkillItem>, String> {
    state.list_agent_skills_impl(limit)
}

/// 删除单条 agent 技能模板（H3）。
#[tauri::command]
pub fn delete_agent_skill(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.delete_agent_skill_impl(id)
}

/// 多轮会话问答（携带 session_id 和历史上下文）。
#[tauri::command]
pub async fn query_ask_session(
    session_id: String,
    question: String,
    options: Option<QueryAskOptions>,
    state: State<'_, AppState>,
) -> Result<QueryAnswerResult, String> {
    eprintln!(
        "[query_ask_session] session={}, question={}",
        session_id, question
    );
    state
        .query_ask_session(session_id, question, options.unwrap_or_default())
        .await
}

/// 取消正在进行的会话查询。
#[tauri::command]
pub fn cancel_ask_session(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_ask_session(session_id)
}

/// 清空会话历史（开启新对话）。
#[tauri::command]
pub fn clear_ask_session(session_id: String, state: State<'_, AppState>) -> Result<(), String> {
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
pub fn get_knowledge_graph(state: State<'_, AppState>) -> Result<KnowledgeGraphData, String> {
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

#[tauri::command]
pub fn delete_ingest_item(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.delete_ingest_item(id)
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

/// 获取单条研究任务。
#[tauri::command]
pub fn get_research_task(
    id: i64,
    state: State<'_, AppState>,
) -> Result<Option<ResearchTaskItem>, String> {
    state.get_research_task(id)
}

/// 取消指定研究任务。
#[tauri::command]
pub fn cancel_research_task(id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_research_task(id)
}

/// 删除指定研究任务；可选同时删除该任务关联 Wiki 页面。
#[tauri::command]
pub async fn delete_research_task(
    state: State<'_, AppState>,
    payload: serde_json::Value,
) -> Result<(), String> {
    let id = payload
        .get("id")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            payload
                .get("input")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_i64())
        })
        .ok_or_else(|| "delete_research_task 缺少有效 id".to_string())?;

    let delete_saved_wiki = payload
        .get("delete_saved_wiki")
        .and_then(|v| v.as_bool())
        .or_else(|| payload.get("deleteSavedWiki").and_then(|v| v.as_bool()))
        .or_else(|| {
            payload
                .get("input")
                .and_then(|v| v.get("delete_saved_wiki"))
                .and_then(|v| v.as_bool())
        })
        .or_else(|| {
            payload
                .get("input")
                .and_then(|v| v.get("deleteSavedWiki"))
                .and_then(|v| v.as_bool())
        })
        .unwrap_or(false);

    state.delete_research_task(id, delete_saved_wiki).await
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

/// 用户审批 Deep Research 子查询列表。
#[tauri::command]
pub async fn approve_research_queries(
    state: tauri::State<'_, crate::state::AppState>,
    task_id: i64,
    queries: Vec<String>,
) -> Result<(), String> {
    let queries: Vec<String> = queries
        .into_iter()
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty())
        .collect();
    if queries.is_empty() {
        return Err("至少需要一个研究方向".to_string());
    }
    if !state.approve_research_queries(task_id, queries) {
        return Err("任务不存在或已超时，请重新开始研究".to_string());
    }
    Ok(())
}

/// 用户审批研究大纲（Outline-First 模式）。
#[tauri::command]
pub async fn approve_research_outline(
    state: tauri::State<'_, crate::state::AppState>,
    task_id: i64,
    outline_json: String,
) -> Result<(), String> {
    if outline_json.trim().is_empty() {
        return Err("大纲 JSON 不能为空".to_string());
    }
    if !state.approve_research_outline(task_id, outline_json) {
        return Err("任务不存在或已超时，请重新开始研究".to_string());
    }
    Ok(())
}

/// 查询任务当前待审批的大纲数据（用于关闭对话框后重新打开恢复状态）。
/// 返回 outline JSON 字符串，无待审批时返回 None。
#[tauri::command]
pub fn get_pending_research_outline(
    state: tauri::State<'_, crate::state::AppState>,
    task_id: i64,
) -> Option<String> {
    state.get_pending_outline(task_id)
}

/// 查询任务当前待审批的子查询（用于关闭对话框后重新打开恢复状态）。
#[tauri::command]
pub fn get_pending_research_queries(
    state: tauri::State<'_, crate::state::AppState>,
    task_id: i64,
) -> Option<Vec<String>> {
    state.get_pending_queries(task_id)
}

/// 对单个 Wiki 页面执行快速结构检查（仅文件系统，不依赖 LLM）。
#[tauri::command]
pub fn quick_lint_page(
    wiki_path: String,
    state: State<'_, AppState>,
) -> Result<PageQuickLint, String> {
    state.quick_lint_page_impl(&wiki_path)
}

/// 将已保存的研究报告 .md 文件转换为独立 HTML，返回 HTML 字符串供前端写入用户选择的路径。
#[tauri::command]
pub fn research_md_to_html(md_path: String) -> Result<String, String> {
    let path = std::path::PathBuf::from(md_path.trim());
    let md = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取研究报告失败: {}", e))?;

    // Markdown → HTML
    use pulldown_cmark::{html, Options, Parser};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_FOOTNOTES);
    let parser = Parser::new_ext(&md, opts);
    let mut body_html = String::new();
    html::push_html(&mut body_html, parser);

    // 从 h2/h3 标题提取目录
    let toc = build_html_toc(&md);

    let title = md
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").trim())
        .unwrap_or("研究报告")
        .to_string();

    let html_out = format!(
        r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{title}</title>
<style>
  :root {{
    --accent: #7c3aed;
    --text: #1e1b2e;
    --text-muted: #6b7280;
    --border: #e5e7eb;
    --bg: #fafafa;
    --code-bg: #f3f4f6;
  }}
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{
    font-family: "Noto Serif SC", Georgia, "Times New Roman", serif;
    font-size: 16px;
    line-height: 1.8;
    color: var(--text);
    background: var(--bg);
    display: flex;
    min-height: 100vh;
  }}
  #toc {{
    width: 220px;
    flex-shrink: 0;
    position: sticky;
    top: 0;
    height: 100vh;
    overflow-y: auto;
    padding: 28px 16px;
    border-right: 1px solid var(--border);
    font-size: 13px;
    font-family: system-ui, sans-serif;
    background: #fff;
  }}
  #toc h3 {{
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    margin-bottom: 12px;
  }}
  #toc ul {{ list-style: none; padding: 0; }}
  #toc li {{ margin: 4px 0; }}
  #toc a {{
    color: var(--text-muted);
    text-decoration: none;
    display: block;
    padding: 2px 6px;
    border-radius: 4px;
    transition: background 0.12s, color 0.12s;
  }}
  #toc a:hover {{ background: #ede9fe; color: var(--accent); }}
  #toc .toc-h3 {{ padding-left: 14px; font-size: 12px; }}
  main {{
    flex: 1;
    max-width: 820px;
    margin: 0 auto;
    padding: 48px 40px;
  }}
  h1 {{ font-size: 2em; font-weight: 700; margin-bottom: 8px; color: var(--text); line-height: 1.3; }}
  h2 {{
    font-size: 1.4em; font-weight: 700;
    margin-top: 2.5em; margin-bottom: 0.8em;
    padding-bottom: 8px;
    border-bottom: 2px solid var(--accent);
    color: var(--text);
  }}
  h3 {{ font-size: 1.15em; font-weight: 600; margin-top: 1.8em; margin-bottom: 0.5em; color: #374151; }}
  p {{ margin: 0.9em 0; }}
  ul, ol {{ padding-left: 1.6em; margin: 0.8em 0; }}
  li {{ margin: 0.3em 0; }}
  blockquote {{
    border-left: 4px solid var(--accent);
    padding: 8px 16px;
    margin: 1em 0;
    background: #ede9fe20;
    color: #4b5563;
    border-radius: 0 6px 6px 0;
  }}
  code {{
    background: var(--code-bg);
    padding: 1px 5px;
    border-radius: 4px;
    font-family: "Cascadia Code", "Fira Code", monospace;
    font-size: 0.88em;
  }}
  pre {{
    background: #1e1b2e;
    color: #e2e8f0;
    padding: 16px 20px;
    border-radius: 8px;
    overflow-x: auto;
    margin: 1.2em 0;
    font-size: 0.9em;
    line-height: 1.5;
  }}
  pre code {{ background: none; padding: 0; color: inherit; }}
  table {{
    width: 100%;
    border-collapse: collapse;
    margin: 1.2em 0;
    font-size: 14px;
  }}
  th, td {{
    text-align: left;
    padding: 8px 12px;
    border: 1px solid var(--border);
  }}
  th {{ background: #f9fafb; font-weight: 600; }}
  tr:nth-child(even) td {{ background: #fafafa; }}
  a {{ color: var(--accent); text-decoration: underline dotted; }}
  a:hover {{ text-decoration: underline; }}
  hr {{ border: none; border-top: 1px solid var(--border); margin: 2em 0; }}
  sup {{ font-size: 0.75em; vertical-align: super; }}
  @media (max-width: 768px) {{
    #toc {{ display: none; }}
    main {{ padding: 24px 20px; }}
  }}
</style>
</head>
<body>
<nav id="toc">
  <h3>目录</h3>
  {toc}
</nav>
<main>
{body_html}
</main>
</body>
</html>"#,
        title = title,
        toc = toc,
        body_html = body_html,
    );

    Ok(html_out)
}

/// 从 Markdown 文本提取 h2/h3 标题生成 HTML 目录列表。
fn build_html_toc(md: &str) -> String {
    let mut items = Vec::new();
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            let text = rest.trim();
            let anchor = heading_to_anchor(text);
            items.push(format!("<li><a href=\"#{}\">{}</a></li>", anchor, text));
        } else if let Some(rest) = line.strip_prefix("### ") {
            let text = rest.trim();
            let anchor = heading_to_anchor(text);
            items.push(format!("<li class=\"toc-h3\"><a href=\"#{}\">{}</a></li>", anchor, text));
        }
    }
    if items.is_empty() {
        return "<ul><li><em>无目录</em></li></ul>".to_string();
    }
    format!("<ul>{}</ul>", items.join(""))
}

/// 将标题文字转换为 URL 锚点（小写 + 非字母数字替换为 -）。
fn heading_to_anchor(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// 保存 Deep Research 导出文件（.md 或其他格式）到用户指定路径。
#[tauri::command]
pub fn save_research_doc(path: String, content: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("保存路径不能为空".to_string());
    }
    let target = std::path::PathBuf::from(trimmed);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    std::fs::write(&target, content).map_err(|e| format!("写入文件失败: {}", e))?;
    Ok(())
}

/// 获取 Vault 知识库统计数据（供仪表盘展示）。
#[tauri::command]
pub fn get_vault_stats(state: State<'_, AppState>) -> Result<VaultStats, String> {
    state.get_vault_stats_impl()
}

/// Wiki 内联 AI 辅助编辑（续写/改写/扩写），通过 ai_assist_chunk 事件流式回传。
#[tauri::command]
pub async fn ai_assist_wiki_edit(
    action: String,
    selected_text: String,
    context: String,
    page_title: String,
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    state
        .ai_assist_wiki_edit(&app_handle, action, selected_text, context, page_title)
        .await
}

/// 导出全部 Wiki 为 Markdown ZIP 包，返回导出页面数。
#[tauri::command]
pub fn export_wiki_markdown_zip(
    dest_path: String,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    state.export_wiki_markdown_zip(dest_path)
}

/// 导出全部 Wiki 为静态 HTML ZIP 包，返回导出页面数。
#[tauri::command]
pub fn export_wiki_html_zip(
    dest_path: String,
    state: State<'_, AppState>,
) -> Result<u32, String> {
    state.export_wiki_html_zip(dest_path)
}

/// AI 辅助新建 Wiki 页面（根据主题查找相关页面、调用 LLM 生成初稿并写入 vault）。
#[tauri::command]
pub async fn create_wiki_page_with_ai(
    topic: String,
    state: State<'_, AppState>,
) -> Result<NewPageResult, String> {
    state.create_wiki_page_with_ai_impl(topic).await
}

/// 执行 Shell 命令（H6-S1：Agent Studio PowerShell 执行能力）。
#[tauri::command]
pub async fn run_shell(
    command: String,
    timeout_ms: Option<u64>,
    source: Option<String>,
    session_id: Option<String>,
    stream_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ShellResult, String> {
    state
        .run_shell_impl(
            command,
            timeout_ms.unwrap_or(30_000),
            source,
            session_id,
            stream_id,
        )
        .await
}

/// 创建 Shell 会话（会话内 cwd 持久化，支持终端式连续交互）。
#[tauri::command]
pub fn create_shell_session(
    source: Option<String>,
    state: State<'_, AppState>,
) -> Result<ShellSessionInfo, String> {
    state.create_shell_session_impl(source)
}

/// 关闭 Shell 会话。
#[tauri::command]
pub fn close_shell_session(session_id: String, state: State<'_, AppState>) -> bool {
    state.close_shell_session_impl(session_id)
}

/// 批准 Agent write_wiki 写盘（H6-S2 审批流）。
#[tauri::command]
pub async fn approve_agent_write(
    run_id: i64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.approve_agent_write_impl(run_id)
}

/// 拒绝 Agent write_wiki 写盘（H6-S2 审批流）。
#[tauri::command]
pub async fn reject_agent_write(
    run_id: i64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    state.reject_agent_write_impl(run_id)
}

/// 用户主动确认并执行被拦截的 Shell 命令（H6-P2 简化版）。
#[tauri::command]
pub async fn approve_and_run_shell(
    command: String,
    timeout_ms: Option<u64>,
    session_id: Option<String>,
    stream_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<ShellResult, String> {
    state
        .approve_and_run_shell_impl(
            command,
            timeout_ms.unwrap_or(30_000),
            session_id,
            stream_id,
        )
        .await
}

/// 读取 Shell 审计日志（H6-P3）。
#[tauri::command]
pub fn list_shell_audit_events(
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<ShellAuditEvent>, String> {
    state.list_shell_audit_events_impl(limit.unwrap_or(100))
}

/// 颁发写操作审批票据（H6-P2）。
/// 用户勾选"记住 5 分钟"后前端调用，此后同路径写操作无需再弹审批。
#[tauri::command]
pub fn grant_write_ticket(path: String) {
    crate::agent_policy::grant_write_ticket(&path);
}

/// 从本地文件读取纯文本内容，供 Chat 消息附件上下文使用（不写入 Vault）。
#[tauri::command]
pub fn read_file_for_chat(
    path: String,
    state: State<'_, AppState>,
) -> Result<crate::models::FileChunk, String> {
    state.read_file_for_chat_impl(&path)
}

/// 在系统默认浏览器中打开外部 URL，防止 WebView 直接导航替换 App 界面。
/// 仅允许 http/https URL。
#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("只允许 http/https URL".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开链接失败: {e}"))
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开链接失败: {e}"))
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("打开链接失败: {e}"))
    }
}

/// 从 URL 下载 Skill JSON 并安装到技能库。
/// JSON 格式：{ "name": "...", "system_prompt": "...", "description"?: "..." }
#[tauri::command]
pub async fn install_skill_from_url(
    url: String,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("llm-wiki/1.0")
        .build()
        .map_err(|e| format!("HTTP client 创建失败: {e}"))?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}: 无法获取 {url}", resp.status().as_u16()));
    }

    let text = resp.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("JSON 解析失败: {e}"))?;

    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Skill JSON 缺少有效的 'name' 字段".to_string())?
        .to_string();

    let system_prompt = value
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Skill JSON 缺少有效的 'system_prompt' 字段".to_string())?
        .to_string();

    let skill = state.upsert_agent_skill_impl(&name, &system_prompt)?;
    Ok(skill.id)
}

/// 返回 Embed 后端当前状态（后端 ID、维度、索引数量、健康状态）。
#[tauri::command]
pub async fn get_embed_status(state: State<'_, AppState>) -> Result<EmbedStatus, String> {
    Ok(state.get_embed_status().await)
}

/// 重建所有页面的 Embedding 索引（切换后端后调用）。
#[tauri::command]
pub async fn rebuild_embeddings(state: State<'_, AppState>) -> Result<usize, String> {
    state.rebuild_embeddings().await
}

// ── 无头浏览器 URL 上下文抓取 ────────────────────────────────────────────────

/// Agent 对话中 URL 粘贴时抓取页面摘要，返回给前端展示卡片。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlContextCard {
    pub title: String,
    pub summary: String,
    pub url: String,
    pub domain: String,
    pub char_count: usize,
    pub fetch_method: String,
}

#[tauri::command]
pub async fn fetch_url_context(url: String) -> Result<UrlContextCard, String> {
    if url.trim().is_empty() {
        return Err("URL 不能为空".to_string());
    }
    let result = crate::browser::fetch_url(&url, Default::default()).await?;
    let summary: String = result.markdown.chars().take(3000).collect();
    Ok(UrlContextCard {
        title: result.title,
        summary,
        url: result.url,
        domain: result.domain,
        char_count: result.char_count,
        fetch_method: result.fetch_method,
    })
}

