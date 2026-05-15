use super::AppState;
use crate::{
    db,
    llm::{LlmError, LlmProvider},
    models::{
        AskSessionItem, AskSessionSearchHitItem, AskSessionTurnItem, AskSessionTurnMeta,
        LogLevel, OutboxAckResult, OutboxEventItem, QueryAnswerResult, QueryAskOptions,
        QueryCitation, QuerySettings, SaveQueryAnswerInput, SaveQueryAnswerResult,
    },
    vault,
};
use std::{
    path::Path,
    sync::Arc,
};

// ─── Prompt builders (pub(super) so state.rs and tests can call them) ─────────

/// 构建单轮 LLM prompt（无历史）。
pub(super) fn build_query_prompt(question: &str, matches: &[super::WikiMatch]) -> String {
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

/// 构建含历史上下文的 LLM prompt（多轮会话用）。
pub(super) fn build_query_prompt_with_history(
    question: &str,
    matches: &[super::WikiMatch],
    history: &[crate::models::AskTurn],
) -> String {
    let mut lines = vec![
        "你是一个严格本地运行的 Wiki 助手。只能依据下方本地检索结果回答，不能编造。".to_string(),
        "如果证据不足，请明确说明不确定，并给出基于页面内容的保守建议。".to_string(),
    ];

    if !history.is_empty() {
        lines.push("对话历史（供上下文参考）：".to_string());
        for turn in history {
            let prefix = if turn.role == "user" { "用户" } else { "助手" };
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

/// 规则回退回答（无 LLM 时使用）。
pub(super) fn build_query_answer(question: &str, matches: &[super::WikiMatch]) -> String {
    if matches.is_empty() {
        return format!(
            "未在本地 Wiki 中检索到与\u{201c}{}\u{201d}直接相关的页面。建议先导入相关资料后再查询。",
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

// ─── Core LLM generation ──────────────────────────────────────────────────────

/// 使用给定 provider 合成 LLM 回答；provider 为 None 时直接回退到规则回答。
pub(super) async fn generate_query_answer_with_provider(
    state: &AppState,
    question: &str,
    matches: &[super::WikiMatch],
    provider: Option<Arc<dyn LlmProvider>>,
    mut on_chunk: Option<&mut (dyn FnMut(String) + Send)>,
) -> (String, String) {
    let fallback_answer = || build_query_answer(question, matches);

    let provider = match provider {
        Some(provider) => provider,
        None => {
            let fallback = fallback_answer();
            if !fallback.is_empty() {
                if let Some(handler) = on_chunk.as_deref_mut() {
                    handler(fallback.clone());
                }
            }
            state.push_log(
                LogLevel::Warn,
                "本地 LLM Provider 不可用，Query 已回退到规则回答".to_string(),
            );
            return (fallback, "rule".to_string());
        }
    };

    let prompt = build_query_prompt(question, matches);

    let streamed = {
        let on_chunk_ref = &mut on_chunk;
        let mut chunk_forwarder = |chunk: String| {
            if let Some(handler) = on_chunk_ref.as_deref_mut() {
                handler(chunk);
            }
        };
        provider
            .complete_stream(&prompt, &mut chunk_forwarder)
            .await
    };

    match streamed {
        Ok(answer) => {
            let answer = answer.trim().to_string();
            if answer.is_empty() {
                let fallback = fallback_answer();
                if !fallback.is_empty() {
                    if let Some(handler) = on_chunk.as_deref_mut() {
                        handler(fallback.clone());
                    }
                }
                state.push_log(
                    LogLevel::Warn,
                    "本地 LLM 返回空回答，Query 已回退到规则回答".to_string(),
                );
                (fallback, "rule".to_string())
            } else {
                state.push_log(
                    LogLevel::Info,
                    format!(
                        "本地 LLM Query 合成成功，回答长度={}",
                        answer.chars().count()
                    ),
                );
                (answer, "llm".to_string())
            }
        }
        Err(err) => {
            let fallback = fallback_answer();
            if !fallback.is_empty() {
                if let Some(handler) = on_chunk.as_deref_mut() {
                    handler(fallback.clone());
                }
            }
            state.push_log(
                LogLevel::Warn,
                format!("本地 LLM Query 合成失败: {}，已回退到规则回答", err),
            );
            (fallback, "rule".to_string())
        }
    }
}

// ─── Embedding route helper ───────────────────────────────────────────────────

pub(super) async fn query_embedding_route_paths(
    state: &AppState,
    db_path: &Path,
    question: &str,
    limit: usize,
) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }
    let trimmed = question.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let candidate_limit = (limit.saturating_mul(20))
        .max(limit)
        .min(super::QUERY_EMBED_ROUTE_MAX_CANDIDATES);
    let candidates = match db::list_embeddings(db_path, candidate_limit) {
        Ok(items) => items,
        Err(err) => {
            state.push_log(
                LogLevel::Warn,
                format!("读取 embedding 候选失败，已跳过 embedding 召回: {}", err),
            );
            return Vec::new();
        }
    };

    if candidates.is_empty() {
        return Vec::new();
    }

    state.emit_progress("query_progress", "embedding", "正在执行 embedding 召回...");
    match state.get_embed_provider().embed(trimmed).await {
        Ok(query_embedding) => {
            crate::search::rank_embedding_paths_by_cosine(&query_embedding, &candidates, limit)
        }
        Err(err) => {
            let hint = if matches!(
                err,
                LlmError::ModelNotFound(_) | LlmError::ConnectionFailed(_)
            ) {
                "（Ollama 未启动或缺少 nomic-embed-text:latest）"
            } else {
                ""
            };
            state.push_log(
                LogLevel::Warn,
                format!("embedding 召回失败，已跳过{}：{}", hint, err),
            );
            Vec::new()
        }
    }
}

// ─── Public query API ─────────────────────────────────────────────────────────

pub async fn query_ask(state: &AppState, question: String) -> Result<QueryAnswerResult, String> {
    query_ask_with_options(state, question, QueryAskOptions::default()).await
}

pub async fn query_ask_with_options(
    state: &AppState,
    question: String,
    options: QueryAskOptions,
) -> Result<QueryAnswerResult, String> {
    let normalized_question = question.trim().to_string();
    if normalized_question.is_empty() {
        return Err("问题不能为空".to_string());
    }

    let (mode, vault_path, default_top_k) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (guard.mode, guard.vault_path.clone(), guard.query_top_k)
    };

    let vault_path =
        vault_path.ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
    let wiki_dir = vault_path.join("wiki");
    let db_path = vault_path.join(".app").join("meta.db");
    let tokens = super::wiki_service::tokenize_query(&normalized_question);
    let top_k = super::wiki_service::normalize_top_k(options.top_k.or(Some(default_top_k)));

    // 步骤1：多路 RRF 融合检索
    state.emit_progress("query_progress", "searching", "多路 RRF 检索中...");
    let embedding_paths = if tokens.is_empty() {
        Vec::new()
    } else {
        query_embedding_route_paths(state, &db_path, &normalized_question, top_k * 4).await
    };
    let extra_routes = if embedding_paths.is_empty() {
        Vec::new()
    } else {
        vec![("embedding".to_string(), embedding_paths)]
    };
    let (matches, search_strategy, fts_error, search_debug) =
        super::wiki_service::search_wiki_matches_rrf_with_extra_routes(
            &db_path,
            &wiki_dir,
            &tokens,
            &normalized_question,
            top_k,
            &extra_routes,
        )?;

    if let Some(err) = fts_error {
        state.push_log(
            LogLevel::Warn,
            format!("FTS 查询失败，已降级为文件扫描: {}", err),
        );
    }

    let citations = matches
        .iter()
        .map(|item| {
            let display_path =
                super::wiki_service::friendly_display_path(Path::new(&item.page_path));
            QueryCitation {
                page_path: item.page_path.clone(),
                display_path: Some(display_path),
                score: item.score,
                excerpt: item.excerpt.clone(),
            }
        })
        .collect::<Vec<_>>();

    // 步骤2：LLM 合成回答
    state.emit_progress("query_progress", "generating", "正在合成回答（LLM）...");
    let provider = state.get_llm_provider();
    state.emit_progress(
        "query_progress",
        "answer_stream_start",
        "开始流式输出回答...",
    );
    let mut emit_chunk = |chunk: String| {
        if !chunk.is_empty() {
            state.emit_progress("query_progress", "answer_chunk", &chunk);
        }
    };
    let (answer, answer_strategy) = generate_query_answer_with_provider(
        state,
        &normalized_question,
        &matches,
        provider,
        Some(&mut emit_chunk),
    )
    .await;
    state.emit_progress("query_progress", "answer_stream_done", "回答输出完成。");

    let matched_pages = matches
        .iter()
        .map(|item| item.page_path.clone())
        .collect::<Vec<_>>();

    state.push_log(
        LogLevel::Info,
        format!(
            "Query 检索完成: '{}'，命中 {} 页，检索策略={}，回答策略={}，top_k={}",
            normalized_question,
            matched_pages.len(),
            search_strategy,
            answer_strategy,
            top_k
        ),
    );

    state.record_outbox_event(
        "query_answered",
        serde_json::json!({
            "question": normalized_question.clone(),
            "matched_pages": matched_pages.clone(),
            "search_strategy": search_strategy,
            "answer_strategy": answer_strategy.clone(),
            "top_k": top_k,
            "search_debug": search_debug.clone(),
        }),
    );

    Ok(QueryAnswerResult {
        question: normalized_question,
        answer,
        citations,
        matched_pages,
        mode,
        checked_at: super::current_timestamp_ms(),
        search_strategy: search_strategy.to_string(),
        answer_strategy,
        search_debug,
    })
}

pub fn set_query_top_k(state: &AppState, top_k: usize) -> Result<QuerySettings, String> {
    let normalized_top_k = super::wiki_service::normalize_top_k(Some(top_k));
    let (mode, vault_path, expected_snapshot) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (
            guard.mode,
            guard.vault_path.clone(),
            guard.config_snapshot.clone(),
        )
    };

    match super::config_service::persist_config(
        state,
        mode,
        vault_path.as_deref(),
        normalized_top_k,
        expected_snapshot.as_deref(),
    ) {
        Ok(serialized) => {
            let mut guard = state.inner.lock().expect("状态锁已被污染");
            guard.query_top_k = normalized_top_k;
            guard.config_snapshot = Some(serialized);
            guard.push_log(
                LogLevel::Info,
                format!("Query TopK 已更新为 {}", normalized_top_k),
                super::current_timestamp_ms(),
            );
            Ok(QuerySettings {
                top_k: normalized_top_k,
                min_top_k: super::QUERY_TOP_K_MIN,
                max_top_k: super::QUERY_TOP_K_MAX,
            })
        }
        Err(err) => {
            state.push_log(LogLevel::Warn, format!("Query TopK 持久化失败: {}", err));
            Err(err)
        }
    }
}

pub fn save_query_answer(
    state: &AppState,
    input: SaveQueryAnswerInput,
) -> Result<SaveQueryAnswerResult, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };

    match vault::save_query_answer(&vault_path, &input) {
        Ok(result) => {
            state.push_log(
                LogLevel::Info,
                format!("Query 结果已保存: {}", result.wiki_path),
            );
            state.record_outbox_event(
                "query_saved_to_wiki",
                serde_json::json!({
                    "question": input.question.clone(),
                    "page_title": result.page_title.clone(),
                    "wiki_path": result.wiki_path.clone(),
                    "citations": input.citations.len(),
                }),
            );
            Ok(result)
        }
        Err(err) => {
            state.push_log(LogLevel::Warn, format!("保存 Query 结果失败: {}", err));
            Err(err)
        }
    }
}

// ─── Ask history ──────────────────────────────────────────────────────────────

pub fn save_ask_history_impl(state: &AppState, question: &str) -> Result<(), String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return Ok(());
    }
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();
    db::save_ask_history(&db_path, question, &created_at)?;
    Ok(())
}

pub fn get_ask_history_impl(
    state: &AppState,
    limit: usize,
) -> Result<Vec<crate::models::AskHistoryItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(Vec::new());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    // 读取上限与落库上限一致，避免异常大值导致一次性扫描过多数据。
    let safe_limit = limit.min(db::ASK_HISTORY_MAX_ENTRIES);
    let records = db::list_ask_history(&db_path, safe_limit)?;
    Ok(records
        .into_iter()
        .map(|r| crate::models::AskHistoryItem {
            id: r.id,
            question: r.question,
            created_at: r.created_at,
        })
        .collect())
}

/// 清空 Ask 历史（DB 持久化）。
pub fn clear_ask_history_impl(state: &AppState) -> Result<usize, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(0);
    };
    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return Ok(0);
    }
    db::clear_ask_history(&db_path)
}

// ─── Ask sessions ─────────────────────────────────────────────────────────────

/// 创建 Ask 会话（若已存在则刷新更新时间）。
pub fn create_ask_session_impl(
    state: &AppState,
    session_id: &str,
    title: Option<&str>,
) -> Result<AskSessionItem, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Err("请先调用 init_vault 初始化 Vault".to_string());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    let now = super::current_timestamp_ms();
    let raw_title = title.unwrap_or("新对话");
    let normalized_title = if raw_title.trim().is_empty() {
        "新对话"
    } else {
        raw_title.trim()
    };
    db::create_ask_session(&db_path, session_id, normalized_title, &now)?;
    Ok(AskSessionItem {
        session_id: session_id.trim().to_string(),
        title: normalized_title.to_string(),
        created_at: now.clone(),
        updated_at: now,
        turn_count: 0,
        last_turn_role: None,
        last_turn_content: None,
    })
}

/// 查询会话列表（按最近更新时间倒序）。
pub fn list_ask_sessions_impl(state: &AppState, limit: usize) -> Result<Vec<AskSessionItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(Vec::new());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let safe_limit = limit.min(200);
    let records = db::list_ask_sessions(&db_path, safe_limit)?;
    Ok(records
        .into_iter()
        .map(|item| AskSessionItem {
            session_id: item.session_id,
            title: item.title,
            created_at: item.created_at,
            updated_at: item.updated_at,
            turn_count: item.turn_count,
            last_turn_role: item.last_turn_role,
            last_turn_content: item.last_turn_content,
        })
        .collect())
}

/// 查询指定会话的轮次列表（正序）。
pub fn list_ask_session_turns_impl(
    state: &AppState,
    session_id: &str,
    limit: usize,
) -> Result<Vec<AskSessionTurnItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(Vec::new());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let safe_limit = limit.min(400);
    let records = db::list_ask_session_turns(&db_path, session_id, safe_limit)?;
    Ok(records
        .into_iter()
        .map(|item| AskSessionTurnItem {
            id: item.id,
            session_id: item.session_id,
            role: item.role,
            content: item.content,
            created_at: item.created_at,
            citations: serde_json::from_str::<Vec<QueryCitation>>(&item.citations_json)
                .unwrap_or_default(),
            meta: item
                .meta_json
                .as_deref()
                .and_then(|raw| serde_json::from_str::<AskSessionTurnMeta>(raw).ok()),
        })
        .collect())
}

/// 跨会话检索 Ask 轮次内容（用于历史定位）。
pub fn search_ask_session_turns_impl(
    state: &AppState,
    keyword: &str,
    limit: usize,
) -> Result<Vec<AskSessionSearchHitItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(Vec::new());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return Ok(Vec::new());
    }
    let safe_limit = limit.min(200);
    let records = db::search_ask_session_turns(&db_path, keyword, safe_limit)?;
    Ok(records
        .into_iter()
        .map(|item| {
            let snippet = item
                .snippet
                .replace('\n', " ")
                .replace('\r', " ")
                .trim()
                .chars()
                .take(120)
                .collect::<String>();
            AskSessionSearchHitItem {
                session_id: item.session_id,
                session_title: item.session_title,
                turn_id: item.turn_id,
                role: item.role,
                snippet,
                created_at: item.created_at,
            }
        })
        .collect())
}

/// 重命名 Ask 会话标题。
pub fn rename_ask_session_impl(state: &AppState, session_id: &str, title: &str) -> Result<(), String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Err("请先调用 init_vault 初始化 Vault".to_string());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    db::rename_ask_session(&db_path, session_id, title, &super::current_timestamp_ms())
}

/// 删除 Ask 会话（含全部轮次）。
pub fn delete_ask_session_impl(state: &AppState, session_id: &str) -> Result<usize, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(0);
    };
    let db_path = vault_path.join(".app").join("meta.db");
    let affected = db::delete_ask_session(&db_path, session_id)?;
    {
        let mut sessions = state.ask_sessions.lock().expect("sessions 锁已被污染");
        sessions.remove(session_id);
    }
    {
        let mut flags = state
            .ask_cancel_flags
            .lock()
            .expect("cancel_flags 锁已被污染");
        flags.remove(session_id);
    }
    Ok(affected)
}

// ─── Outbox events ────────────────────────────────────────────────────────────

/// 按 id 增量读取 outbox 事件。
pub fn get_outbox_events_impl(
    state: &AppState,
    last_id: i64,
    limit: usize,
) -> Result<Vec<OutboxEventItem>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
    let records = db::list_outbox_events_from_id(&db_path, last_id, limit)?;
    Ok(records
        .into_iter()
        .map(|item| OutboxEventItem {
            id: item.id,
            event_type: item.event_type,
            payload_json: item.payload_json,
            created_at: item.created_at,
            processed_at: item.processed_at,
            consumer_tag: item.consumer_tag,
        })
        .collect())
}

/// 标记 outbox 事件已消费。
pub fn ack_outbox_events_impl(
    state: &AppState,
    up_to_id: i64,
    consumer_tag: &str,
) -> Result<OutboxAckResult, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
    let acked =
        db::ack_outbox_events(&db_path, up_to_id, consumer_tag, &super::current_timestamp_ms())?;
    Ok(OutboxAckResult {
        acked,
        up_to_id,
        consumer_tag: consumer_tag.trim().to_string(),
    })
}

// ─── Session query (multi-turn) ───────────────────────────────────────────────

pub async fn query_ask_session(
    state: &AppState,
    session_id: String,
    question: String,
    options: QueryAskOptions,
) -> Result<QueryAnswerResult, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    const MAX_HISTORY_TURNS: usize = 6; // 最多保留最近 6 轮

    let normalized_session_id = session_id.trim().to_string();
    if normalized_session_id.is_empty() {
        return Err("session_id 不能为空".to_string());
    }

    let normalized_question = question.trim().to_string();
    if normalized_question.is_empty() {
        return Err("问题不能为空".to_string());
    }

    let (mode, vault_path, default_top_k) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (guard.mode, guard.vault_path.clone(), guard.query_top_k)
    };
    let vault_path =
        vault_path.ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
    let wiki_dir = vault_path.join("wiki");
    let db_path = vault_path.join(".app").join("meta.db");
    let tokens = super::wiki_service::tokenize_query(&normalized_question);
    let top_k = super::wiki_service::normalize_top_k(options.top_k.or(Some(default_top_k)));
    let now = super::current_timestamp_ms();
    db::create_ask_session(&db_path, &normalized_session_id, "新对话", &now)?;
    let history =
        db::list_recent_ask_session_turns(&db_path, &normalized_session_id, MAX_HISTORY_TURNS)?;

    // 将用户问题加入会话历史（内存 + 持久化）。
    let user_turn = crate::models::AskTurn {
        role: "user".to_string(),
        content: normalized_question.clone(),
    };
    let user_turn_db_id = db::append_ask_session_turn(
        &db_path,
        &normalized_session_id,
        "user",
        &normalized_question,
        &now,
        None,
        None,
    )?;
    {
        let mut sessions = state.ask_sessions.lock().expect("sessions 锁已被污染");
        let turns = sessions.entry(normalized_session_id.clone()).or_default();
        if turns.is_empty() && !history.is_empty() {
            turns.extend(history.iter().cloned());
        }
        turns.push(user_turn);
    }

    // 注册取消标志
    let cancel_flag = Arc::new(AtomicBool::new(false));
    {
        let mut flags = state
            .ask_cancel_flags
            .lock()
            .expect("cancel_flags 锁已被污染");
        flags.insert(normalized_session_id.clone(), cancel_flag.clone());
    }

    // 多路 RRF 融合检索
    state.emit_progress("query_progress", "searching", "多路 RRF 检索中...");
    let embedding_paths = if tokens.is_empty() {
        Vec::new()
    } else {
        query_embedding_route_paths(state, &db_path, &normalized_question, top_k * 4).await
    };
    let extra_routes = if embedding_paths.is_empty() {
        Vec::new()
    } else {
        vec![("embedding".to_string(), embedding_paths)]
    };
    let (matches, search_strategy, fts_error, search_debug) =
        match super::wiki_service::search_wiki_matches_rrf_with_extra_routes(
            &db_path,
            &wiki_dir,
            &tokens,
            &normalized_question,
            top_k,
            &extra_routes,
        ) {
            Ok(result) => result,
            Err(err) => {
                {
                    let mut flags = state
                        .ask_cancel_flags
                        .lock()
                        .expect("cancel_flags 锁已被污染");
                    flags.remove(&normalized_session_id);
                }
                {
                    let mut sessions = state.ask_sessions.lock().expect("sessions 锁已被污染");
                    if let Some(turns) = sessions.get_mut(&normalized_session_id) {
                        turns.pop();
                    }
                }
                let _ = db::delete_ask_session_turn_by_id(&db_path, user_turn_db_id);
                return Err(err);
            }
        };

    if let Some(err) = fts_error {
        state.push_log(
            LogLevel::Warn,
            format!("FTS 查询失败，已降级为文件扫描: {}", err),
        );
    }

    let citations = matches
        .iter()
        .map(|item| {
            let display_path =
                super::wiki_service::friendly_display_path(std::path::Path::new(&item.page_path));
            QueryCitation {
                page_path: item.page_path.clone(),
                display_path: Some(display_path),
                score: item.score,
                excerpt: item.excerpt.clone(),
            }
        })
        .collect::<Vec<_>>();

    // LLM 合成（含历史 context）
    state.emit_progress("query_progress", "generating", "正在合成回答（LLM）...");
    let provider = state.get_llm_provider();
    state.emit_progress(
        "query_progress",
        "answer_stream_start",
        "开始流式输出回答...",
    );

    let cancel_for_closure = cancel_flag.clone();
    let mut emit_chunk = |chunk: String| {
        if cancel_for_closure.load(Ordering::Relaxed) {
            return; // 已取消，静默丢弃 chunk
        }
        if !chunk.is_empty() {
            state.emit_progress("query_progress", "answer_chunk", &chunk);
        }
    };

    // 构建含历史的 prompt
    let prompt = build_query_prompt_with_history(&normalized_question, &matches, &history);

    // 直接调用 complete_stream（绕过 generate_query_answer_with_provider 以使用自定义 prompt）
    let answer_strategy;
    let answer = if let Some(p) = provider {
        let streamed = p.complete_stream(&prompt, &mut emit_chunk).await;
        match streamed {
            Ok(raw) => {
                let trimmed = raw.trim().to_string();
                if trimmed.is_empty() {
                    answer_strategy = "rule".to_string();
                    build_query_answer(&normalized_question, &matches)
                } else {
                    answer_strategy = "llm".to_string();
                    trimmed
                }
            }
            Err(err) => {
                state.push_log(
                    LogLevel::Warn,
                    format!("LLM 流式生成失败: {}，已回退到规则回答", err),
                );
                answer_strategy = "rule".to_string();
                build_query_answer(&normalized_question, &matches)
            }
        }
    } else {
        answer_strategy = "rule".to_string();
        build_query_answer(&normalized_question, &matches)
    };

    state.emit_progress("query_progress", "answer_stream_done", "回答输出完成。");

    // 检查是否已取消
    let was_cancelled = cancel_flag.load(Ordering::Relaxed);

    // 清理取消标志
    {
        let mut flags = state
            .ask_cancel_flags
            .lock()
            .expect("cancel_flags 锁已被污染");
        flags.remove(&normalized_session_id);
    }

    if was_cancelled {
        // 移除刚加入的用户轮，避免历史污染
        let mut sessions = state.ask_sessions.lock().expect("sessions 锁已被污染");
        if let Some(turns) = sessions.get_mut(&normalized_session_id) {
            turns.pop();
        }
        let _ = db::delete_ask_session_turn_by_id(&db_path, user_turn_db_id);
        return Err("查询已取消".to_string());
    }

    // 将助手回答加入会话历史
    {
        let mut sessions = state.ask_sessions.lock().expect("sessions 锁已被污染");
        if let Some(turns) = sessions.get_mut(&normalized_session_id) {
            turns.push(crate::models::AskTurn {
                role: "assistant".to_string(),
                content: answer.clone(),
            });
        }
    }
    let assistant_turn_citations_json =
        serde_json::to_string(&citations).unwrap_or_else(|_| "[]".to_string());
    let assistant_turn_meta_json = serde_json::to_string(&AskSessionTurnMeta {
        mode,
        search_strategy: Some(search_strategy.to_string()),
        answer_strategy: Some(answer_strategy.clone()),
        top_k: Some(top_k),
        matched_pages: Some(matches.len()),
        search_debug: search_debug.clone(),
    })
    .unwrap_or_else(|_| "{}".to_string());
    if let Err(err) = db::append_ask_session_turn(
        &db_path,
        &normalized_session_id,
        "assistant",
        &answer,
        &super::current_timestamp_ms(),
        Some(&assistant_turn_citations_json),
        Some(&assistant_turn_meta_json),
    ) {
        state.push_log(LogLevel::Warn, format!("持久化助手会话轮次失败: {}", err));
    }

    let matched_pages = matches
        .iter()
        .map(|item| item.page_path.clone())
        .collect::<Vec<_>>();

    state.push_log(
        LogLevel::Info,
        format!(
            "会话 Query 完成: session={}, '{}', 命中 {} 页",
            &normalized_session_id[..normalized_session_id.len().min(8)],
            normalized_question,
            matched_pages.len()
        ),
    );

    state.record_outbox_event(
        "query_session_answered",
        serde_json::json!({
            "session_id": normalized_session_id.clone(),
            "question": normalized_question.clone(),
            "matched_pages": matched_pages.clone(),
            "search_strategy": search_strategy,
            "answer_strategy": answer_strategy.clone(),
            "top_k": top_k,
            "search_debug": search_debug.clone(),
        }),
    );

    Ok(QueryAnswerResult {
        question: normalized_question,
        answer,
        citations,
        matched_pages,
        mode,
        checked_at: super::current_timestamp_ms(),
        search_strategy: search_strategy.to_string(),
        answer_strategy,
        search_debug,
    })
}

/// 取消正在进行的会话查询（软取消：停止 emit chunk）。
pub fn cancel_ask_session(state: &AppState, session_id: String) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let flags = state
        .ask_cancel_flags
        .lock()
        .expect("cancel_flags 锁已被污染");
    if let Some(flag) = flags.get(&session_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// 清空会话历史（开启新对话）。
pub fn clear_ask_session(state: &AppState, session_id: String) -> Result<(), String> {
    let normalized_session_id = session_id.trim().to_string();
    if normalized_session_id.is_empty() {
        return Ok(());
    }

    {
        let mut sessions = state.ask_sessions.lock().expect("sessions 锁已被污染");
        sessions.remove(&normalized_session_id);
    }
    {
        let mut flags = state
            .ask_cancel_flags
            .lock()
            .expect("cancel_flags 锁已被污染");
        flags.remove(&normalized_session_id);
    }

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return Ok(());
    };
    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return Ok(());
    }
    db::clear_ask_session_turns(&db_path, &normalized_session_id, &super::current_timestamp_ms())?;
    Ok(())
}
