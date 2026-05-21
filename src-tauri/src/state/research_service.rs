//! Research service — Deep Research 管线及任务 CRUD。
//! H16 Phase 11 拆分自 state.rs。

use std::{collections::HashSet, fs, sync::Arc};

use tauri::{Emitter, Manager};

use super::{current_timestamp_ms, AppState, PendingResearchReport};
use crate::{
    db,
    llm::LlmError,
    models::{ResearchTaskItem, SearchConfig, WebSearchResult},
    state::search_service,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ResearchOutline {
    title: String,
    sections: Vec<OutlineSection>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OutlineSection {
    heading: String,
    key_questions: Vec<String>,
    search_queries: Vec<String>,
}

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

/// 构造最终报告 Markdown（含 frontmatter + body + References），纯函数。
fn build_final_report_content(
    topic: &str,
    config: &SearchConfig,
    all_results: &[WebSearchResult],
    synthesized: &str,
) -> String {
    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today = date_str.clone();
    let references = all_results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let is_academic = r.source_type == "academic"
                || ["arxiv.org", "doi.org", "pubmed", "scholar.google", "researchgate.net", "semanticscholar"]
                    .iter()
                    .any(|domain| r.url.contains(domain));
            if is_academic {
                format!("[{}] {}. *{}*. <{}>", i + 1, r.title, r.source, r.url)
            } else {
                format!("[{}] [{}]({}). *{}*. Accessed {}.", i + 1, r.title, r.url, r.source, today)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let cleaned = strip_think_tags(synthesized);
    format!(
        "---\ntype: research\ntitle: \"{topic}\"\ncreated: {date}\nupdated: {date}\ndepth: {depth}\nbreadth: {breadth}\nsources: {count}\ntags: [research, deep-research]\n---\n\n{body}\n\n## References\n\n{refs}",
        topic = topic,
        date = date_str,
        depth = config.depth,
        breadth = config.breadth,
        count = all_results.len(),
        body = cleaned,
        refs = references,
    )
}

/// 研究完成后：缓存报告 + 更新 DB 状态为 awaiting_save + 发送 research_complete 事件。
/// 用户后续通过 commit_research_to_wiki / discard_research_report 决定去向。
fn finalize_pending_research(
    db_path: &std::path::Path,
    app_handle: &tauri::AppHandle,
    state: &AppState,
    task_id: i64,
    topic: &str,
    config: &SearchConfig,
    all_results: Vec<WebSearchResult>,
    all_used_queries: Vec<String>,
    learnings: Vec<String>,
    synthesized: String,
) {
    let final_content = build_final_report_content(topic, config, &all_results, &synthesized);

    let report = PendingResearchReport {
        topic: topic.to_string(),
        content: final_content.clone(),
        depth: config.depth,
        breadth: config.breadth,
        all_results: all_results.clone(),
        all_used_queries: all_used_queries.clone(),
        learnings: learnings.clone(),
    };
    state
        .pending_research_reports
        .lock()
        .expect("pending_research_reports lock")
        .insert(task_id, report);

    // 数据库状态：awaiting_save（用户决定前不写 wiki）
    if let Ok(conn) = rusqlite::Connection::open(db_path) {
        let now = current_timestamp_ms();
        let queries_json = serde_json::to_string(&all_used_queries).unwrap_or_default();
        let _ = db::db_update_research_task(
            &conn,
            task_id,
            "awaiting_save",
            &queries_json,
            all_results.len() as i32,
            None,
            None,
            &now,
        );
    }

    let _ = app_handle.emit(
        "research_complete",
        serde_json::json!({
            "task_id": task_id,
            "content": final_content,
            "sources": all_results.len(),
            "learnings": learnings.len(),
        }),
    );
}

/// 用户主动保存：将缓存的报告写到 vault/wiki/research/ 并 ingest，emit research_done。
pub async fn commit_research_to_wiki(state: &AppState, task_id: i64) -> Result<String, String> {
    let report = state
        .pending_research_reports
        .lock()
        .expect("pending_research_reports lock")
        .get(&task_id)
        .cloned()
        .ok_or_else(|| "找不到待保存的研究报告（可能已被丢弃或应用重启）".to_string())?;

    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "Vault 路径丢失".to_string())?
    };

    let date_str = chrono::Local::now().format("%Y-%m-%d").to_string();
    let slug = make_research_slug(&report.topic);
    let filename = format!("research-{}-{}.md", slug, date_str);
    let save_dir = vault_path.join("wiki").join("research");
    fs::create_dir_all(&save_dir).map_err(|e| format!("创建保存目录失败: {}", e))?;
    let save_path = save_dir.join(&filename);

    // 路径越权检查
    let canonical = save_path
        .canonicalize()
        .or_else(|_| save_dir.canonicalize().map(|d| d.join(&filename)))
        .map_err(|e| format!("保存路径解析失败: {}", e))?;
    let canonical_vault = vault_path.canonicalize().unwrap_or(vault_path.clone());
    if !canonical.starts_with(&canonical_vault) {
        return Err("保存路径越权：topic 包含非法路径字符".to_string());
    }

    fs::write(&save_path, &report.content).map_err(|e| format!("写入文件失败: {}", e))?;

    let saved_path_str = save_path.to_string_lossy().to_string();
    {
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
        let now = current_timestamp_ms();
        let queries_json = serde_json::to_string(&report.all_used_queries).unwrap_or_default();
        db::db_update_research_task(
            &conn,
            task_id,
            "done",
            &queries_json,
            report.all_results.len() as i32,
            Some(saved_path_str.as_str()),
            None,
            &now,
        )?;
    }

    let _ = state.ingest_markdown(save_path, None).await;

    // 清理缓存
    state
        .pending_research_reports
        .lock()
        .expect("pending_research_reports lock")
        .remove(&task_id);

    if let Some(handle) = state.get_app_handle() {
        let _ = handle.emit(
            "research_done",
            serde_json::json!({
                "task_id": task_id,
                "saved_path": saved_path_str,
                "sources": report.all_results.len(),
                "learnings": report.learnings.len(),
            }),
        );
    }
    Ok(saved_path_str)
}

/// 用户主动丢弃：清理缓存 + 更新 DB 状态为 discarded。
pub fn discard_research_report(state: &AppState, task_id: i64) -> Result<(), String> {
    let removed = state
        .pending_research_reports
        .lock()
        .expect("pending_research_reports lock")
        .remove(&task_id);
    if removed.is_none() {
        return Err("找不到待保存的研究报告".to_string());
    }

    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    let now = current_timestamp_ms();
    db::db_update_research_task(
        &conn,
        task_id,
        "discarded",
        "[]",
        0,
        None,
        Some("用户丢弃了未保存的报告"),
        &now,
    )?;

    if let Some(handle) = state.get_app_handle() {
        let _ = handle.emit(
            "research_discarded",
            serde_json::json!({ "task_id": task_id }),
        );
    }
    Ok(())
}

/// 查询任务的待保存报告内容（用于对话框重开时恢复正文）。
pub fn get_pending_research_content(state: &AppState, task_id: i64) -> Option<String> {
    state
        .pending_research_reports
        .lock()
        .expect("pending_research_reports lock")
        .get(&task_id)
        .map(|r| r.content.clone())
}

/// 多重策略提取 JSON 字符串：剥离 markdown code fence、剥离首尾说明文字。
fn extract_json_object(text: &str) -> Option<String> {
    let trimmed = text.trim();

    // 策略 1：剥离 ```json ... ``` 或 ``` ... ``` 代码围栏
    if let Some(stripped) = trimmed.strip_prefix("```json").or_else(|| trimmed.strip_prefix("```")) {
        let after_fence = stripped.trim_start_matches('\n').trim_start();
        if let Some(end) = after_fence.rfind("```") {
            let candidate = after_fence[..end].trim();
            if candidate.starts_with('{') {
                return Some(candidate.to_string());
            }
        }
    }

    // 策略 2：括号配对匹配，找第一个完整 JSON 对象（处理嵌套示例 / 杂质前缀）
    let bytes = trimmed.as_bytes();
    let mut start_idx: Option<usize> = None;
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escape = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => {
                if depth == 0 {
                    start_idx = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start_idx {
                        return Some(trimmed[s..=i].to_string());
                    }
                }
            }
            _ => {}
        }
    }

    // 策略 3：最朴素的首尾大括号截取（兜底）
    let first = trimmed.find('{')?;
    let last = trimmed.rfind('}')?;
    if last > first {
        Some(trimmed[first..=last].to_string())
    } else {
        None
    }
}

async fn generate_research_outline(
    provider: &dyn crate::llm::LlmProvider,
    topic: &str,
    config: &SearchConfig,
) -> Option<ResearchOutline> {
    let section_count = config.breadth.clamp(3, 5);
    let prompt = format!(
        "You are a research planner. Create a structured research outline for the topic below.\n\nTopic: \"{topic}\"\n\nOutput RULES (critical):\n- Output ONLY a single valid JSON object — no markdown code fences, no commentary, no leading or trailing text\n- Must parse with strict JSON (double quotes, no trailing commas)\n- Schema:\n  {{\n    \"title\": \"<topic>\",\n    \"sections\": [\n      {{\n        \"heading\": \"## 1. <Section Title>\",\n        \"key_questions\": [\"<Q1>\", \"<Q2>\"],\n        \"search_queries\": [\"<term1>\", \"<term2>\"]\n      }}\n    ]\n  }}\n\nContent requirements:\n- EXACTLY {n} body sections (do not include Introduction or Conclusion as separate sections)\n- Each section: 2-3 key_questions, 2-3 search_queries\n- Heading format: \"## N. Title\" where N is the section number\n- Write section headings, key_questions, and search_queries in the same language as the topic\n- key_questions are real questions ending with ?; search_queries are short keyword phrases\n\nBegin output now:",
        topic = topic,
        n = section_count,
    );

    let text = provider.complete(&prompt).await.ok()?;
    let cleaned = strip_think_tags(&text);
    let json_str = extract_json_object(&cleaned)?;

    // 直接解析；失败时尝试把单引号替换为双引号再试一次
    if let Ok(outline) = serde_json::from_str::<ResearchOutline>(&json_str) {
        return Some(outline);
    }
    let coerced = json_str.replace('\'', "\"");
    serde_json::from_str::<ResearchOutline>(&coerced).ok()
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
    let emit_section_progress =
        |sec_idx: usize, total: usize, heading: &str, msg: String| {
            let _ = app_handle.emit(
                "research_progress",
                serde_json::json!({
                    "task_id": task_id,
                    "stage": "writing_section",
                    "message": msg,
                    "section_index": sec_idx,
                    "section_title": heading,
                    "total_sections": total,
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
    // 全局源编号（跨轮次不重置）
    let mut source_index: usize = 0;
    // Outline-First 大纲（若生成成功，Phase 3 时用于章节合成；否则 None）
    let mut current_outline: Option<ResearchOutline> = None;

    // ── Phase 1/A: 查询规划（Outline-First 或 H25 兜底） ──────────────────────
    let mut current_queries: Vec<String>;

    emit_progress("planning_outline", "正在规划研究大纲...".to_string());
    let outline_opt = generate_research_outline(&*provider, &topic, &config).await;

    if let Some(outline) = outline_opt {
        // ── Outline-First 路径 ──────────────────────────────────────────────
        let flat_queries: Vec<String> = outline
            .sections
            .iter()
            .flat_map(|s| s.search_queries.iter().cloned())
            .collect();

        let rx_outline = state.register_outline_approval(task_id);
        // 缓存大纲数据，供 ResearchDialog 关闭后重开恢复
        if let Ok(outline_json) = serde_json::to_string(&outline) {
            state.cache_pending_outline(task_id, outline_json);
        }
        let _ = app_handle.emit(
            "research_outline_ready",
            serde_json::json!({
                "task_id": task_id,
                "outline": serde_json::to_value(&outline).unwrap_or_default(),
            }),
        );
        emit_progress(
            "awaiting_outline_approval",
            format!("大纲已生成（{} 章），等待确认...", outline.sections.len()),
        );

        let approved_json = match tokio::time::timeout(
            std::time::Duration::from_secs(300),
            rx_outline,
        )
        .await
        {
            Ok(Ok(json)) => json,
            Ok(Err(_)) => {
                // channel 被关闭 / 任务已取消
                emit_progress(
                    "planning_outline",
                    "审批通道已关闭，使用初始大纲继续".to_string(),
                );
                serde_json::to_string(&outline).unwrap_or_default()
            }
            Err(_) => {
                // 300s 超时
                emit_progress(
                    "planning_outline",
                    "等待大纲确认超时（5 分钟未操作），使用初始大纲自动继续".to_string(),
                );
                serde_json::to_string(&outline).unwrap_or_default()
            }
        };

        let approved = serde_json::from_str::<ResearchOutline>(&approved_json)
            .unwrap_or_else(|_| {
                emit_progress(
                    "planning_outline",
                    "审批返回的大纲 JSON 解析失败，使用初始大纲继续".to_string(),
                );
                outline.clone()
            });

        current_queries = approved
            .sections
            .iter()
            .flat_map(|s| s.search_queries.iter().cloned())
            .collect();
        if current_queries.is_empty() {
            current_queries = flat_queries;
        }
        if current_queries.is_empty() {
            current_queries = vec![topic.clone()];
        }
        current_outline = Some(approved);
    } else {
        // ── H25 兜底路径：子查询分解 + 旧审批机制 ──────────────────────────
        emit_progress(
            "decomposing",
            "未生成有效大纲，降级为标准查询分解流程（章节进度不可用）".to_string(),
        );
        let decompose_prompt = format!(
            "You are an expert researcher. Generate {breadth} specific, diverse search queries to thoroughly investigate: \"{topic}\"\nOutput one query per line, no numbering, no extra text.",
            breadth = config.breadth,
            topic = topic
        );
        current_queries = match provider.complete(&decompose_prompt).await {
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
                    emit_progress(
                        "decomposing",
                        format!("生成了 {} 个研究子查询", qs.len()),
                    );
                    qs
                }
            }
            Err(_) => {
                emit_progress(
                    "decomposing",
                    "查询分解失败，使用原始主题搜索".to_string(),
                );
                vec![topic.clone()]
            }
        };

        let rx = state.register_query_approval(task_id);
        // 缓存子查询，供 ResearchDialog 关闭后重开恢复
        state.cache_pending_queries(task_id, current_queries.clone());
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
    }

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
                    search_service::do_search_multi(&client, &query, &config_clone, breadth_limit).await;
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

        // 构建本轮摘要（每条来源截取 300 字符，避免 token 溢出）；使用全局源编号
        let round_snippets: String = round_results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let snip: String = r.snippet.chars().take(300).collect();
                format!("[{}] {} ({}): {}", source_index + i + 1, r.title, r.source, snip)
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        source_index += round_results.len();
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

    // ── Phase 3/4: 报告合成与保存 ─────────────────────────────────────────────

    if let Some(ref outline) = current_outline {
        // ── Outline-First：按章节合成 ───────────────────────────────────────
        emit_progress(
            "synthesizing",
            format!("按大纲合成报告（{} 章）...", outline.sections.len()),
        );

        // O-7 高质量来源优先：按 quality_score 降序排序后再 truncate，
        //     保证 6000 字符预算优先用于权威来源（arxiv/edu/gov 等）
        let mut ranked_indices: Vec<usize> = (0..all_results.len()).collect();
        ranked_indices.sort_by(|a, b| {
            all_results[*b]
                .quality_score
                .partial_cmp(&all_results[*a].quality_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let sources_block_full: String = ranked_indices
            .iter()
            .map(|&idx| {
                let r = &all_results[idx];
                let snip: String = r.snippet.chars().take(200).collect();
                let quality = if r.quality_score > 0.8 { " [high quality]" } else { "" };
                // 原始全局编号（idx + 1）保留，确保引用编号与 References 一致
                format!("[{}]{} {} ({}): {}", idx + 1, quality, r.title, r.url, snip)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let sources_block: String = sources_block_full.chars().take(6000).collect();

        let total_sections = outline.sections.len();
        let mut section_bodies: Vec<String> = Vec::new();

        for (sec_idx, section) in outline.sections.iter().enumerate() {
            emit_section_progress(
                sec_idx,
                total_sections,
                &section.heading,
                format!("第 {}/{} 章：{}", sec_idx + 1, total_sections, section.heading),
            );
            let questions_text = section.key_questions.join("\n- ");
            let section_prompt = format!(
                "You are writing section \"{heading}\" of a research report on \"{topic}\".\n\nAvailable sources (cite inline as [N]):\n{sources}\n\nKey questions to address:\n- {questions}\n\nRequirements:\n- 500-1000 words\n- EVERY factual claim must have [N] inline citation\n- Do NOT include the heading (it will be added automatically)\n- Write in same language as the topic\n- Output ONLY the section body content",
                heading = section.heading,
                topic = topic,
                sources = sources_block,
                questions = questions_text,
            );

            let section_body = match provider.complete(&section_prompt).await {
                Ok(text) => strip_think_tags(&text),
                Err(e) => {
                    emit_section_progress(
                        sec_idx,
                        total_sections,
                        &section.heading,
                        format!("第 {} 章生成失败: {}", sec_idx + 1, e),
                    );
                    String::new()
                }
            };

            section_bodies.push(format!("{}\n\n{}", section.heading, section_body));
        }

        // 生成摘要与结论（分别生成，确保 Conclusion 拼到正文末尾）
        emit_progress("assembling", "生成摘要与结论...".to_string());
        let sections_overview = outline
            .sections
            .iter()
            .map(|s| format!("- {}", s.heading))
            .collect::<Vec<_>>()
            .join("\n");

        let intro_prompt = format!(
            "Write ONLY the Introduction section (150-250 words) of a research report on \"{topic}\".\n\nThe report will cover these sections:\n{sections}\n\nRequirements:\n- Start with the heading \"## Introduction\"\n- Write in the same language as the topic\n- Output ONLY the Introduction (no Conclusion, no other sections)",
            topic = topic,
            sections = sections_overview,
        );
        let conclusion_prompt = format!(
            "Write ONLY the Conclusion section (150-250 words) of a research report on \"{topic}\".\n\nThe report covered these sections:\n{sections}\n\nRequirements:\n- Start with the heading \"## Conclusion\"\n- Synthesize the main findings; do not introduce new facts\n- Write in the same language as the topic\n- Output ONLY the Conclusion",
            topic = topic,
            sections = sections_overview,
        );

        let introduction = provider
            .complete(&intro_prompt)
            .await
            .map(|t| strip_think_tags(&t))
            .unwrap_or_default();
        let conclusion = provider
            .complete(&conclusion_prompt)
            .await
            .map(|t| strip_think_tags(&t))
            .unwrap_or_default();

        // 拼装：# Title → Introduction → 各 Section bodies → Conclusion
        let mut parts: Vec<String> = Vec::with_capacity(3 + section_bodies.len());
        parts.push(format!("# {}", topic));
        if !introduction.is_empty() {
            parts.push(introduction);
        }
        parts.extend(section_bodies.into_iter());
        if !conclusion.is_empty() {
            parts.push(conclusion);
        }
        let synthesized = parts.join("\n\n");

        emit_progress("awaiting_save", "报告已生成，等待用户决定是否保存到知识库...".to_string());
        finalize_pending_research(
            &db_path,
            &app_handle,
            &state,
            task_id,
            &topic,
            &config,
            all_results,
            all_used_queries,
            learnings,
            synthesized,
        );
    } else {
        // ── H25 兜底：单次综合合成 ──────────────────────────────────────────
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

        let learnings_text = learnings
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{}. {}", i + 1, l))
            .collect::<Vec<_>>()
            .join("\n");
        let context: String = learnings_text.chars().take(8000).collect();

        // C-3：fallback 路径同样按 quality_score 排序、加 [high quality] 标记
        let mut ranked_indices: Vec<usize> = (0..all_results.len()).collect();
        ranked_indices.sort_by(|a, b| {
            all_results[*b]
                .quality_score
                .partial_cmp(&all_results[*a].quality_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let sources_block_full: String = ranked_indices
            .iter()
            .map(|&idx| {
                let r = &all_results[idx];
                let snip: String = r.snippet.chars().take(200).collect();
                let quality = if r.quality_score > 0.8 { " [high quality]" } else { "" };
                format!("[{}]{} {} ({}): {}", idx + 1, quality, r.title, r.url, snip)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let sources_block: String = sources_block_full.chars().take(6000).collect();

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let synth_prompt = format!(
            "You are a professional research analyst. Write a comprehensive, well-structured research report in Markdown.\n\nTopic: {topic}\n\nResearch Sources — cite these inline using [N] notation:\n{sources_block}\n\nKey Research Findings ({count} items):\n{context}\n\nExisting Knowledge Base:\n{wiki}\n\n## Writing Requirements:\n1. Write a complete report with sections: Abstract, Core Findings, Detailed Analysis, Conclusion\n2. EVERY factual claim MUST be supported by an inline citation [N] referencing one of the numbered sources above\n3. The final section MUST be \"## References\" containing properly formatted entries:\n   - For academic papers (URLs containing arxiv.org, doi.org, pubmed, scholar): use format: [N] Author(s) (Year). *Title*. Venue. URL\n   - For web articles: use format: [N] Title. *Site Name*. URL. Accessed {date}\n4. References must be clickable Markdown links where possible\n5. Write in the same language as the topic title (Chinese topic → Chinese report)\n6. Do NOT include reasoning or thinking process in output",
            topic = topic,
            sources_block = sources_block,
            count = learnings.len(),
            context = context,
            wiki = wiki_excerpt,
            date = today,
        );

        let mut synthesized = String::new();
        let mut synth_last_err: Option<LlmError> = None;
        for attempt in 1..=2 {
            let mut char_count = 0usize;
            let mut last_emitted = 0usize;
            let mut buf = String::new();
            let stream_result = provider
                .complete_stream(&synth_prompt, &mut |chunk| {
                    let chunk_str = chunk.clone();
                    buf.push_str(&chunk);
                    char_count += chunk.chars().count();
                    let _ = app_handle.emit(
                        "research_stream_chunk",
                        serde_json::json!({
                            "task_id": task_id,
                            "chunk": chunk_str,
                        }),
                    );
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

        emit_progress("awaiting_save", "报告已生成，等待用户决定是否保存到知识库...".to_string());
        finalize_pending_research(
            &db_path,
            &app_handle,
            &state,
            task_id,
            &topic,
            &config,
            all_results,
            all_used_queries,
            learnings,
            synthesized,
        );
    }
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

    // ─── JSON 提取（大纲生成） ───────────────────────────────────────────────

    #[test]
    fn extract_json_object_plain() {
        let json = extract_json_object("{\"a\":1}").unwrap();
        assert_eq!(json, "{\"a\":1}");
    }

    #[test]
    fn extract_json_object_strips_code_fence() {
        let text = "```json\n{\"a\":1,\"b\":[2,3]}\n```";
        let json = extract_json_object(text).unwrap();
        assert_eq!(json, "{\"a\":1,\"b\":[2,3]}");
    }

    #[test]
    fn extract_json_object_strips_plain_fence() {
        let text = "```\n{\"x\":\"y\"}\n```";
        let json = extract_json_object(text).unwrap();
        assert_eq!(json, "{\"x\":\"y\"}");
    }

    #[test]
    fn extract_json_object_skips_leading_prose() {
        let text = "Here is the outline:\n{\"title\":\"T\"}\nHope this helps.";
        let json = extract_json_object(text).unwrap();
        assert_eq!(json, "{\"title\":\"T\"}");
    }

    #[test]
    fn extract_json_object_handles_nested_braces() {
        let text = "Outline:\n{\"a\":{\"b\":1},\"c\":[{\"d\":2}]}";
        let json = extract_json_object(text).unwrap();
        assert_eq!(json, "{\"a\":{\"b\":1},\"c\":[{\"d\":2}]}");
    }

    #[test]
    fn extract_json_object_ignores_braces_in_strings() {
        let text = "{\"s\":\"contains } brace\",\"n\":1}";
        let json = extract_json_object(text).unwrap();
        assert_eq!(json, text);
    }

    #[test]
    fn extract_json_object_none_when_no_object() {
        assert!(extract_json_object("no json here").is_none());
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
