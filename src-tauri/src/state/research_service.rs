//! Research service — Deep Research 管线及任务 CRUD。
//! H16 Phase 11 拆分自 state.rs。

use std::{collections::HashSet, fs, sync::Arc};

use tauri::{Emitter, Manager};

use super::{current_timestamp_ms, AppState};
use crate::{
    db,
    llm::LlmError,
    models::{ResearchTaskItem, SearchConfig, WebSearchResult},
    state::search_service,
};

// ── 内部辅助函数 ────────────────────────────────────────────────────────────

/// 将研究主题转换为 URL 友好的 slug（最多 50 字符）。
pub(super) fn make_research_slug(topic: &str) -> String {
    let raw: String = topic
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut slug = String::new();
    let mut prev_dash = false;
    for c in raw.chars() {
        if c == '-' {
            if !prev_dash {
                slug.push(c);
            }
            prev_dash = true;
        } else {
            slug.push(c);
            prev_dash = false;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    slug.chars().take(50).collect()
}

/// 去除 <think>/<thinking> 标签及其内容。
pub fn strip_think_tags(text: &str) -> String {
    let mut result = text.to_string();
    for tag in &["think", "thinking"] {
        let open = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        while let Some(start) = result.find(&open) {
            if let Some(rel_end) = result[start..].find(&close) {
                let end = start + rel_end + close.len();
                result.replace_range(start..end, "");
            } else {
                result.replace_range(start.., "");
                break;
            }
        }
    }
    result.trim().to_string()
}

/// 将研究任务标记为失败并向前端发送错误事件。
fn report_research_failure(
    db_path: &std::path::Path,
    app_handle: &tauri::AppHandle,
    task_id: i64,
    msg: &str,
) {
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        let now = current_timestamp_ms();
        let _ =
            db::db_update_research_task(&conn, task_id, "failed", "[]", 0, None, Some(msg), &now);
    }
    let _ = app_handle.emit(
        "research_error",
        serde_json::json!({ "task_id": task_id, "error": msg }),
    );
}

/// Phase 4: 将综合报告写入 vault/wiki/research/ 并更新数据库。
/// 返回保存的文件路径（成功）或 Err(()），失败已由 report_research_failure 处理。
async fn save_research_output(
    db_path: &std::path::Path,
    app_handle: &tauri::AppHandle,
    state: &AppState,
    task_id: i64,
    topic: &str,
    config: &SearchConfig,
    all_results: &[WebSearchResult],
    all_used_queries: &[String],
    learnings: &[String],
    synthesized: &str,
) -> Result<String, ()> {
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let references = all_results
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{}. [{}]({})", i + 1, r.title, r.url))
        .collect::<Vec<_>>()
        .join("\n");

    let cleaned = strip_think_tags(synthesized);
    let final_content = format!(
        "---\ntype: research\ntitle: \"{topic}\"\ncreated: {date}\nupdated: {date}\ndepth: {depth}\nbreadth: {breadth}\nsources: {count}\ntags: [research, deep-research]\n---\n\n{body}\n\n## References\n\n{refs}",
        topic = topic,
        date = date_str,
        depth = config.depth,
        breadth = config.breadth,
        count = all_results.len(),
        body = cleaned,
        refs = references,
    );

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁");
        guard.vault_path.clone()
    };
    let vault_path = match vault_path {
        Some(p) => p,
        None => {
            report_research_failure(db_path, app_handle, task_id, "保存阶段：Vault 路径丢失");
            return Err(());
        }
    };

    let slug = make_research_slug(topic);
    let filename = format!("research-{}-{}.md", slug, date_str);
    let save_dir = vault_path.join("wiki").join("research");
    let _ = fs::create_dir_all(&save_dir);
    let save_path = save_dir.join(&filename);

    // 防止 topic 含 ../ 等路径越权
    match save_path
        .canonicalize()
        .or_else(|_| save_dir.canonicalize().map(|d| d.join(&filename)))
    {
        Ok(canonical) => {
            let canonical_vault = vault_path.canonicalize().unwrap_or(vault_path.clone());
            if !canonical.starts_with(&canonical_vault) {
                report_research_failure(
                    db_path,
                    app_handle,
                    task_id,
                    "保存路径越权：topic 包含非法路径字符，已拒绝写入",
                );
                return Err(());
            }
        }
        Err(e) => {
            report_research_failure(
                db_path,
                app_handle,
                task_id,
                &format!("保存路径解析失败: {}", e),
            );
            return Err(());
        }
    }

    if let Err(e) = fs::write(&save_path, &final_content) {
        report_research_failure(
            db_path,
            app_handle,
            task_id,
            &format!("写入文件失败: {}", e),
        );
        return Err(());
    }

    let saved_path_str = save_path.to_string_lossy().to_string();
    {
        let conn = match rusqlite::Connection::open(db_path) {
            Ok(c) => c,
            Err(e) => {
                report_research_failure(
                    db_path,
                    app_handle,
                    task_id,
                    &format!("打开数据库失败: {}", e),
                );
                return Err(());
            }
        };
        let now = current_timestamp_ms();
        let queries_json = serde_json::to_string(all_used_queries).unwrap_or_default();
        let _ = db::db_update_research_task(
            &conn,
            task_id,
            "done",
            &queries_json,
            all_results.len() as i32,
            Some(saved_path_str.as_str()),
            None,
            &now,
        );
    }

    let _ = state.ingest_markdown(save_path, None).await;
    let _ = app_handle.emit(
        "research_done",
        serde_json::json!({
            "task_id": task_id,
            "saved_path": saved_path_str,
            "sources": all_results.len(),
            "learnings": learnings.len(),
        }),
    );
    Ok(saved_path_str)
}

/// 从 LLM 输出行中提取 "TAG: content" 结构，容忍 markdown 装饰与大小写变体。
///
/// 支持格式：
/// - `LEARNING: insight`
/// - `- LEARNING: insight`
/// - `**LEARNING:** insight`
/// - `1. FOLLOWUP: query`
/// - `learning: insight`（小写）
fn extract_tagged_content(line: &str, tag: &str) -> Option<String> {
    let mut s = line.trim();
    // 剥离 markdown 列表标记
    if let Some(rest) = s.strip_prefix("- ").or_else(|| s.strip_prefix("* ")) {
        s = rest.trim();
    } else if let Some(dot_idx) = s.find(". ") {
        if s[..dot_idx].chars().all(|c| c.is_ascii_digit()) {
            s = s[dot_idx + 2..].trim();
        }
    }
    // 剥离 markdown 加粗标记
    s = s.trim_start_matches("**");

    let tag_upper = tag.to_ascii_uppercase();
    if s.len() >= tag.len() && s[..tag.len()].to_ascii_uppercase() == tag_upper {
        let after = s[tag.len()..]
            .trim_start_matches(':')
            .trim_start_matches("**")
            .trim();
        Some(after.to_string())
    } else {
        None
    }
}

/// 解析 LLM 输出的结构化学习内容，返回 `(learnings, followup_queries)`。
///
/// 兜底策略：若 LLM 完全不遵循格式，将所有有意义的非空行作为 learnings 返回，
/// 确保管线不会因格式不匹配而静默丢失信息。
pub fn parse_learnings_and_followups(text: &str) -> (Vec<String>, Vec<String>) {
    let mut learnings = Vec::new();
    let mut followups = Vec::new();

    for line in text.lines() {
        if let Some(content) = extract_tagged_content(line, "LEARNING") {
            if !content.is_empty() {
                learnings.push(content);
            }
        } else if let Some(content) = extract_tagged_content(line, "FOLLOWUP") {
            if !content.is_empty() {
                followups.push(content);
            }
        }
    }

    // 兜底：LLM 未输出任何结构化标签时，提取所有有意义的行作为 learnings
    if learnings.is_empty() && followups.is_empty() {
        learnings = text
            .lines()
            .map(|l| {
                l.trim()
                    .trim_start_matches("**")
                    .trim_end_matches("**")
                    .trim_start_matches('-')
                    .trim_start_matches('*')
                    .trim()
            })
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_string())
            .collect();
    }

    (learnings, followups)
}

/// Deep Research 主管线（后台 task）。
/// 算法参考 dzhng/deep-research：迭代式学习提取 + 并发搜索 + 知识缺口驱动的下一轮查询生成。
async fn start_research_task(
    app_handle: tauri::AppHandle,
    task_id: i64,
    topic: String,
    config: SearchConfig,
) {
    let state = app_handle.state::<AppState>();
    let db_path = match state.outbox_db_path() {
        Some(p) => p,
        None => {
            let _ = app_handle.emit(
                "research_error",
                serde_json::json!({ "task_id": task_id, "error": "Vault 未初始化" }),
            );
            return;
        }
    };

    let emit_progress = |stage: &str, msg: String| {
        let _ = app_handle.emit(
            "research_progress",
            serde_json::json!({
                "task_id": task_id, "stage": stage, "message": msg
            }),
        );
    };

    if let Err(err) = search_service::validate_search_config(&config) {
        report_research_failure(&db_path, &app_handle, task_id, &err);
        return;
    }

    let provider = match state.get_llm_provider() {
        Some(p) => p,
        None => {
            report_research_failure(&db_path, &app_handle, task_id, "LLM Provider 不可用");
            return;
        }
    };

    // 共享 HTTP 客户端，统一设置 30s 超时
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => Arc::new(c),
        Err(e) => {
            report_research_failure(
                &db_path,
                &app_handle,
                task_id,
                &format!("HTTP 客户端初始化失败: {}", e),
            );
            return;
        }
    };

    let mut all_results: Vec<WebSearchResult> = Vec::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    // 累积的结构化学习成果（参考 dzhng/deep-research 的 learnings 模型）
    let mut learnings: Vec<String> = Vec::new();
    let mut all_used_queries: Vec<String> = Vec::new();

    // ── Phase 1: 初始查询分解 ─────────────────────────────────────────────────
    emit_progress("decomposing", "正在规划研究路径...".to_string());
    let decompose_prompt = format!(
        "You are an expert researcher. Generate {breadth} specific, diverse search queries to thoroughly investigate: \"{topic}\"\nOutput one query per line, no numbering, no extra text.",
        breadth = config.breadth,
        topic = topic
    );
    let mut current_queries: Vec<String> = match provider.complete(&decompose_prompt).await {
        Ok(text) => {
            let qs: Vec<String> = strip_think_tags(&text)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .take(config.breadth as usize)
                .collect();
            if qs.is_empty() {
                emit_progress(
                    "decomposing",
                    "LLM 未生成子查询，使用原始主题搜索".to_string(),
                );
                vec![topic.clone()]
            } else {
                emit_progress("decomposing", format!("生成了 {} 个研究子查询", qs.len()));
                qs
            }
        }
        Err(_) => {
            emit_progress("decomposing", "查询分解失败，使用原始主题搜索".to_string());
            vec![topic.clone()]
        }
    };

    // 暂停：通知前端子查询已就绪，等待用户审批（最多 5 分钟）
    let rx = state.register_query_approval(task_id);
    let _ = app_handle.emit(
        "research_queries_ready",
        serde_json::json!({
            "task_id": task_id,
            "queries": current_queries,
        }),
    );
    emit_progress(
        "awaiting_approval",
        format!("已分解为 {} 个研究方向，等待确认...", current_queries.len()),
    );

    current_queries = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or_else(|| current_queries.clone());

    // ── Phase 2: 迭代式并发搜索 + 学习提取 ──────────────────────────────────
    let max_depth = config.depth.clamp(1, 5);
    for depth in 1..=max_depth {
        emit_progress(
            "searching",
            format!(
                "第 {}/{} 轮：{} 个查询并发搜索中...",
                depth,
                max_depth,
                current_queries.len()
            ),
        );

        // 并发执行本轮所有查询
        let breadth_limit = config.breadth as usize;
        let mut handles = Vec::new();
        for query in &current_queries {
            let client = Arc::clone(&client);
            let query = query.clone();
            let config_clone = config.clone();
            handles.push(tokio::task::spawn(async move {
                let result =
                    search_service::do_search(&client, &query, &config_clone, breadth_limit).await;
                (query, result)
            }));
        }

        let mut round_results: Vec<WebSearchResult> = Vec::new();
        let mut round_errors: Vec<String> = Vec::new();
        for handle in handles {
            match handle.await {
                Ok((query, Ok(results))) => {
                    emit_progress("searching", format!("\u{2713} {}（{} 条）", query, results.len()));
                    for r in results {
                        if seen_urls.insert(r.url.clone()) {
                            round_results.push(r);
                        }
                    }
                }
                Ok((query, Err(err))) => {
                    emit_progress(
                        "searching",
                        format!(
                            "\u{2717} {}（{}）",
                            query,
                            search_service::compact_error_message(&err, 80)
                        ),
                    );
                    round_errors.push(format!("{}: {}", query, err));
                }
                Err(join_err) => {
                    let err = format!("查询任务并发执行失败: {}", join_err);
                    emit_progress(
                        "searching",
                        format!(
                            "\u{2717} {}",
                            search_service::compact_error_message(&err, 90)
                        ),
                    );
                    round_errors.push(err);
                }
            }
        }
        all_used_queries.extend(current_queries.iter().cloned());

        if round_results.is_empty() {
            if depth == 1 && !round_errors.is_empty() {
                let summary = search_service::summarize_round_errors(&round_errors, 3);
                report_research_failure(
                    &db_path,
                    &app_handle,
                    task_id,
                    &format!("搜索阶段失败：{}", summary),
                );
                return;
            }
            if depth > 1 {
                if round_errors.is_empty() {
                    emit_progress("searching", "本轮未发现新资料，提前结束搜索。".to_string());
                } else {
                    emit_progress(
                        "searching",
                        format!(
                            "本轮无新资料，且存在 {} 条搜索错误，提前结束。",
                            round_errors.len()
                        ),
                    );
                }
            }
            break;
        }

        // 构建本轮摘要（每条来源截取 300 字符，避免 token 溢出）
        let round_snippets: String = round_results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let snip: String = r.snippet.chars().take(300).collect();
                format!("[{}] {} ({}): {}", i + 1, r.title, r.source, snip)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        all_results.extend(round_results);

        if depth < max_depth {
            // 提取结构化学习 + 生成知识缺口查询（参考 dzhng/deep-research processSerpResult）
            emit_progress(
                "synthesizing",
                format!("第 {} 轮：提取关键发现并识别知识缺口...", depth),
            );
            let extract_prompt = format!(
                "Research topic: {topic}\n\nSearch results (round {depth}):\n{snippets}\n\nExisting learnings:\n{existing}\n\nTask:\n1. Extract 3-5 NEW key learnings not already in existing learnings (format: LEARNING: <insight>)\n2. Generate {next_breadth} follow-up search queries to fill knowledge gaps (format: FOLLOWUP: <query>)\n\nOutput only LEARNING: and FOLLOWUP: lines, nothing else.",
                topic = topic,
                depth = depth,
                snippets = round_snippets,
                existing = if learnings.is_empty() {
                    "none yet".to_string()
                } else {
                    learnings.join("\n")
                },
                next_breadth = (config.breadth as usize).min(3),
            );

            match provider.complete(&extract_prompt).await {
                Ok(text) => {
                    let (new_learnings, next_queries) =
                        parse_learnings_and_followups(&strip_think_tags(&text));
                    learnings.extend(new_learnings);
                    if next_queries.is_empty() {
                        emit_progress("searching", "无新知识缺口，提前结束迭代。".to_string());
                        break;
                    }
                    current_queries = next_queries;
                }
                Err(_) => break,
            }
        } else {
            // 最后一轮：直接提取学习成果，不再生成新查询
            let final_extract_prompt = format!(
                "Research topic: {topic}\n\nFinal round search results:\n{snippets}\n\nExtract 5-8 key learnings. Format: LEARNING: <insight>",
                topic = topic,
                snippets = round_snippets,
            );
            if let Ok(text) = provider.complete(&final_extract_prompt).await {
                let (new_learnings, _) = parse_learnings_and_followups(&strip_think_tags(&text));
                learnings.extend(new_learnings);
            }
        }
    }

    // ── Phase 3: 最终综合报告 ────────────────────────────────────────────────
    emit_progress(
        "synthesizing",
        format!("综合 {} 条关键发现，撰写研究报告...", learnings.len()),
    );

    let wiki_index = {
        let guard = state.inner.lock().expect("状态锁");
        guard
            .vault_path
            .as_ref()
            .and_then(|vp| fs::read_to_string(vp.join("wiki").join("index.md")).ok())
            .unwrap_or_default()
    };
    let wiki_excerpt: String = wiki_index.chars().take(800).collect();

    // 截断学习成果到 8000 字符防止 LLM token 溢出
    let learnings_text = learnings
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}. {}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n");
    let context: String = learnings_text.chars().take(8000).collect();

    let synth_prompt = format!(
        "You are a professional research analyst. Write a comprehensive, well-structured Markdown wiki page based on the research findings.\n\nTopic: {topic}\n\nKey Learnings ({count} findings):\n{context}\n\nExisting Knowledge Base Overview:\n{wiki}\n\nRequirements:\n1. Sections: Abstract, Core Findings, Detailed Analysis, Conclusion\n2. Cite sources inline as [N] referencing the numbered learnings\n3. Suggest cross-references to existing wiki pages where relevant\n4. Write in the same language as the topic title\n5. Do not include reasoning or thinking process",
        topic = topic,
        count = learnings.len(),
        context = context,
        wiki = wiki_excerpt,
    );

    let mut synthesized = String::new();
    let mut synth_last_err: Option<LlmError> = None;
    for attempt in 1..=2 {
        let mut char_count = 0usize;
        let mut last_emitted = 0usize;
        let mut buf = String::new();
        let stream_result = provider
            .complete_stream(&synth_prompt, &mut |chunk| {
                let chunk_str = chunk.clone(); // 给 emit 用
                buf.push_str(&chunk);
                char_count += chunk.chars().count();
                let _ = app_handle.emit(
                    "research_stream_chunk",
                    serde_json::json!({
                        "task_id": task_id,
                        "chunk": chunk_str,
                    }),
                );
                // 每增加 150 字触发一次进度更新，避免事件风暴
                if char_count.saturating_sub(last_emitted) >= 150 {
                    last_emitted = char_count;
                    let _ = app_handle.emit(
                        "research_progress",
                        serde_json::json!({
                            "task_id": task_id,
                            "stage": "synthesizing",
                            "message": format!("正在生成综合报告... 已生成 {} 字", char_count),
                        }),
                    );
                }
            })
            .await;
        match stream_result {
            Ok(_) => {
                synthesized = buf;
                synth_last_err = None;
                break;
            }
            Err(err) => {
                synth_last_err = Some(err.clone());
                if attempt == 1 {
                    emit_progress(
                        "synthesizing",
                        format!(
                            "综合报告生成失败，正在重试一次：{}",
                            search_service::compact_llm_error(&err, 120)
                        ),
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                }
            }
        }
    }
    if let Some(err) = synth_last_err {
        report_research_failure(
            &db_path,
            &app_handle,
            task_id,
            &format!("综合报告生成失败: {}", err),
        );
        return;
    }

    // ── Phase 4: 保存 ────────────────────────────────────────────────────────
    emit_progress("saving", "正在保存到知识库...".to_string());
    let _ = save_research_output(
        &db_path,
        &app_handle,
        &state,
        task_id,
        &topic,
        &config,
        &all_results,
        &all_used_queries,
        &learnings,
        &synthesized,
    )
    .await;
}

// ── AppState 方法的自由函数版本 ──────────────────────────────────────────────

/// 创建研究任务并在后台启动研究管线。
pub fn start_research(
    state: &AppState,
    app_handle: tauri::AppHandle,
    topic: String,
    depth: i32,
    breadth: i32,
) -> Result<i64, String> {
    let db_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先初始化 Vault".to_string())?
            .join(".app")
            .join("meta.db")
    };
    // 确保 schema 存在
    db::ensure_meta_db(&db_path)?;

    // 先校验搜索配置，避免创建必然失败的任务记录。
    let mut cfg = state.get_search_config();
    cfg.depth = depth;
    cfg.breadth = breadth;
    search_service::validate_search_config(&cfg)?;

    let now = current_timestamp_ms();
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    let task_id = db::db_create_research_task(&conn, &topic, depth, breadth, &now)?;

    tauri::async_runtime::spawn(async move {
        start_research_task(app_handle, task_id, topic, cfg).await;
    });

    Ok(task_id)
}

/// 列出最近研究任务。
pub fn list_research_tasks(state: &AppState) -> Result<Vec<ResearchTaskItem>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先初始化 Vault".to_string())?;
    db::ensure_meta_db(&db_path)?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    db::db_list_research_tasks(&conn)
}

/// 获取单个研究任务详情。
pub fn get_research_task(
    state: &AppState,
    id: i64,
) -> Result<Option<ResearchTaskItem>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先初始化 Vault".to_string())?;
    db::ensure_meta_db(&db_path)?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    db::db_get_research_task(&conn, id)
}

/// 取消研究任务（幂等，不重置已有字段）。
pub fn cancel_research_task(state: &AppState, id: i64) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先初始化 Vault".to_string())?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    let now = current_timestamp_ms();
    db::db_cancel_research_task(&conn, id, &now)
}

/// 删除研究任务；可选同时删除该任务关联的 Wiki 页面。
pub async fn delete_research_task(
    state: &AppState,
    id: i64,
    delete_saved_wiki: bool,
) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先初始化 Vault".to_string())?;
    db::ensure_meta_db(&db_path)?;

    let task = {
        let conn = rusqlite::Connection::open(&db_path)
            .map_err(|e| format!("打开数据库失败: {}", e))?;
        db::db_get_research_task(&conn, id)?.ok_or_else(|| format!("研究任务不存在: {}", id))?
    };

    if !matches!(task.status.as_str(), "done" | "failed" | "cancelled") {
        return Err("任务仍在运行中，请先取消后再删除".to_string());
    }

    if delete_saved_wiki {
        if let Some(saved_path) = task.saved_path.as_deref() {
            let _ = state.delete_wiki_page_impl(saved_path).await?;
        }
    }

    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    db::db_delete_research_task(&conn, id)?;

    state.record_outbox_event(
        "research_task_deleted",
        serde_json::json!({
            "task_id": id,
            "topic": task.topic,
            "status": task.status,
            "delete_saved_wiki": delete_saved_wiki,
            "saved_path": task.saved_path,
        }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, state::test_helpers::*};
    use std::path::PathBuf;

    // ─── Deep Research pipeline unit tests ───────────────────────────────────

    #[test]
    fn parse_learnings_standard_format() {
        let text =
            "LEARNING: LLMs use attention mechanisms\nFOLLOWUP: how does multi-head attention work";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["LLMs use attention mechanisms"]);
        assert_eq!(followups, vec!["how does multi-head attention work"]);
    }

    #[test]
    fn parse_learnings_markdown_bold() {
        let text = "**LEARNING:** transformers replaced RNNs\n**FOLLOWUP:** what are the main transformer variants";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["transformers replaced RNNs"]);
        assert_eq!(followups, vec!["what are the main transformer variants"]);
    }

    #[test]
    fn parse_learnings_list_prefix() {
        let text = "- LEARNING: attention is O(n^2)\n- FOLLOWUP: sparse attention methods";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["attention is O(n^2)"]);
        assert_eq!(followups, vec!["sparse attention methods"]);
    }

    #[test]
    fn parse_learnings_numbered_list() {
        let text = "1. LEARNING: GPT uses decoder-only\n2. FOLLOWUP: encoder-decoder models";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["GPT uses decoder-only"]);
        assert_eq!(followups, vec!["encoder-decoder models"]);
    }

    #[test]
    fn parse_learnings_lowercase_tag() {
        let text =
            "learning: BERT is bidirectional\nfollowup: how does masked language modeling work";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings, vec!["BERT is bidirectional"]);
        assert_eq!(followups, vec!["how does masked language modeling work"]);
    }

    #[test]
    fn parse_learnings_mixed_formats() {
        let text = "LEARNING: finding A\n- learning: finding B\n**LEARNING:** finding C\nFOLLOWUP: query X\n1. followup: query Y";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert_eq!(learnings.len(), 3);
        assert_eq!(followups.len(), 2);
        assert!(learnings.contains(&"finding A".to_string()));
        assert!(learnings.contains(&"finding B".to_string()));
        assert!(learnings.contains(&"finding C".to_string()));
    }

    #[test]
    fn parse_learnings_fallback_on_unstructured_output() {
        let text = "The sky is blue\nWater is wet\n\n# Header (should be skipped)";
        let (learnings, followups) = parse_learnings_and_followups(text);
        assert!(followups.is_empty());
        assert!(learnings.contains(&"The sky is blue".to_string()));
        assert!(learnings.contains(&"Water is wet".to_string()));
        assert!(!learnings.iter().any(|l| l.starts_with('#')));
    }

    #[test]
    fn parse_learnings_empty_input() {
        let (learnings, followups) = parse_learnings_and_followups("");
        assert!(learnings.is_empty());
        assert!(followups.is_empty());
    }

    #[test]
    fn parse_learnings_skips_empty_content_after_tag() {
        let text = "LEARNING:   \nFOLLOWUP:";
        let (learnings, followups) = parse_learnings_and_followups(text);
        let _ = (learnings, followups);
    }

    #[test]
    fn make_research_slug_ascii() {
        assert_eq!(make_research_slug("Hello World"), "hello-world");
    }

    #[test]
    fn make_research_slug_deduplicates_dashes() {
        assert_eq!(make_research_slug("hello   world"), "hello-world");
    }

    #[test]
    fn make_research_slug_trims_dashes() {
        assert_eq!(make_research_slug("  hello  "), "hello");
    }

    #[test]
    fn make_research_slug_unicode_becomes_dashes() {
        let slug = make_research_slug("大模型 RAG");
        assert!(!slug.contains(' '));
        assert!(slug.len() <= 50);
    }

    #[test]
    fn make_research_slug_max_50_chars() {
        let long = "a".repeat(100);
        assert_eq!(make_research_slug(&long).len(), 50);
    }

    #[test]
    fn strip_think_tags_removes_think_block() {
        let input = "before<think>internal reasoning</think>after";
        assert_eq!(strip_think_tags(input), "beforeafter");
    }

    #[test]
    fn strip_think_tags_removes_thinking_block() {
        let input = "<thinking>step 1\nstep 2</thinking>result";
        assert_eq!(strip_think_tags(input), "result");
    }

    #[test]
    fn strip_think_tags_unclosed_tag_removes_to_end() {
        let input = "start<think>incomplete";
        assert_eq!(strip_think_tags(input), "start");
    }

    #[test]
    fn strip_think_tags_no_tags_unchanged() {
        let input = "plain text with no tags";
        assert_eq!(strip_think_tags(input), input);
    }

    // ─── DB research task CRUD tests ─────────────────────────────────────────

    fn make_research_db() -> (PathBuf, rusqlite::Connection, impl Drop) {
        let dir = make_temp_dir("llm-wiki-research-db");
        let guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        db::ensure_meta_db(&db_path).expect("ensure_meta_db 失败");
        let conn = rusqlite::Connection::open(&db_path).expect("打开数据库失败");
        (db_path, conn, guard)
    }

    #[test]
    fn research_task_create_and_list() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "test topic", 2, 3, "100").expect("create 失败");
        assert!(id > 0);

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks.len(), 1);
        let t = &tasks[0];
        assert_eq!(t.topic, "test topic");
        assert_eq!(t.status, "queued");
        assert_eq!(t.depth, 2);
        assert_eq!(t.breadth, 3);
        assert_eq!(t.web_results_count, 0);
        assert!(t.sub_queries.is_empty());
    }

    #[test]
    fn research_task_update_to_done() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "update test", 1, 2, "100").expect("create 失败");

        let queries = serde_json::to_string(&vec!["q1", "q2"]).unwrap();
        db::db_update_research_task(
            &conn,
            id,
            "done",
            &queries,
            5,
            Some("/path/to/file.md"),
            None,
            "200",
        )
        .expect("update 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        let t = &tasks[0];
        assert_eq!(t.status, "done");
        assert_eq!(t.web_results_count, 5);
        assert_eq!(t.saved_path.as_deref(), Some("/path/to/file.md"));
        assert_eq!(t.sub_queries, vec!["q1", "q2"]);
    }

    #[test]
    fn research_task_cancel_changes_queued_to_cancelled() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "cancel test", 1, 1, "100").expect("create 失败");

        db::db_cancel_research_task(&conn, id, "200").expect("cancel 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks[0].status, "cancelled");
    }

    #[test]
    fn research_task_cancel_is_idempotent_on_done() {
        let (_path, conn, _guard) = make_research_db();
        let id = db::db_create_research_task(&conn, "idempotent test", 1, 1, "100")
            .expect("create 失败");

        db::db_update_research_task(&conn, id, "done", "[]", 3, Some("/p.md"), None, "150")
            .expect("update 失败");

        db::db_cancel_research_task(&conn, id, "200").expect("cancel 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks[0].status, "done", "done 任务不应被 cancel 覆盖");
        assert_eq!(tasks[0].web_results_count, 3, "web_results_count 不应被重置");
        assert_eq!(tasks[0].saved_path.as_deref(), Some("/p.md"), "saved_path 不应被清空");
    }

    #[test]
    fn research_task_delete_removes_row() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "delete test", 1, 1, "100").expect("create 失败");

        db::db_delete_research_task(&conn, id).expect("delete 失败");

        let task = db::db_get_research_task(&conn, id).expect("get 失败");
        assert!(task.is_none(), "删除后任务应不存在");
    }

    #[test]
    fn research_task_delete_missing_returns_error() {
        let (_path, conn, _guard) = make_research_db();
        let err = db::db_delete_research_task(&conn, 99999).expect_err("不存在任务应报错");
        assert!(err.contains("研究任务不存在"), "错误信息应包含不存在提示");
    }

    #[test]
    fn research_task_cancel_is_idempotent_on_already_cancelled() {
        let (_path, conn, _guard) = make_research_db();
        let id =
            db::db_create_research_task(&conn, "double cancel", 1, 1, "100").expect("create 失败");

        db::db_cancel_research_task(&conn, id, "200").expect("第一次 cancel 失败");
        db::db_cancel_research_task(&conn, id, "300").expect("第二次 cancel 失败");

        let tasks = db::db_list_research_tasks(&conn).expect("list 失败");
        assert_eq!(tasks[0].status, "cancelled");
        assert_eq!(tasks[0].updated_at, "200");
    }
}
