use super::AppState;
use crate::{db, models::LogLevel};
use flate2::read::ZlibDecoder;
use std::{
    collections::HashSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
use tauri::Manager;

// ─── Public instance methods (delegation targets from state.rs impl AppState) ──

pub async fn ingest_markdown(
    state: &AppState,
    source_path: PathBuf,
    display_name: Option<String>,
) -> Result<crate::models::IngestResult, String> {
    let source_path_text = source_path.to_string_lossy().to_string();
    // 对于非 md 文件（PDF/TXT 等），调用方可传入原始文件名以生成有意义的 wiki 页面名称
    let display_path = display_name.unwrap_or_else(|| source_path_text.clone());

    // 记录开始导入事件
    state.record_outbox_event(
        "ingest_started",
        serde_json::json!({
            "source_path": source_path_text.clone(),
            "task_id": super::current_timestamp_ms(),
        }),
    );

    let vault_path_result = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())
    };
    let vault_path = match vault_path_result {
        Ok(path) => path,
        Err(err) => {
            state.record_ingest_failed_event(&source_path_text, &err);
            return Err(err);
        }
    };

    // 读取源文件内容以生成 LLM 摘要
    let source_content = match fs::read_to_string(&source_path) {
        Ok(content) => content,
        Err(err) => {
            let message = format!("读取源文件失败: {}", err);
            state.record_ingest_failed_event(&source_path_text, &message);
            return Err(message);
        }
    };

    // 步骤1：LLM 摘要生成
    state.emit_progress("ingest_progress", "summarizing", "正在生成摘要（LLM）...");
    let llm_summary = state.generate_summary(&source_content).await;

    // 步骤2：LLM 实体提取（写入前完成，确保可持久化到 frontmatter）
    state.emit_progress(
        "ingest_progress",
        "extracting_entities",
        "正在提取关键实体（LLM）...",
    );
    let entities = super::wiki_service::extract_entities(state, &source_content).await;
    complete_ingest_with_precomputed_analysis(
        state,
        &vault_path,
        &source_path,
        &display_path,
        &source_content,
        &llm_summary,
        &entities,
    )
    .await
}

/// 预览摄入分析结果（不写入 Vault，只缓存审批上下文）。
pub async fn preview_ingest_file(
    state: &AppState,
    source_type: &str,
    source_path: &str,
    ocr_provider: Option<&str>,
) -> Result<crate::models::IngestPreview, String> {
    let normalized_source_type = source_type.trim().to_ascii_lowercase();
    if normalized_source_type.is_empty() {
        return Err("source_type 不能为空".to_string());
    }
    let normalized_source_path = source_path.trim().to_string();
    if normalized_source_path.is_empty() {
        return Err("source_path 不能为空".to_string());
    }

    let source_content = load_preview_source_content(
        state,
        normalized_source_type.as_str(),
        normalized_source_path.as_str(),
        ocr_provider,
    )
    .await?;
    if source_content.trim().is_empty() {
        return Err("预览内容为空，无法生成摄入分析".to_string());
    }

    state.emit_progress(
        "ingest_preview_progress",
        "summarizing",
        "正在生成摄入预览摘要（LLM）...",
    );
    let summary = state.generate_summary(&source_content).await;

    state.emit_progress(
        "ingest_preview_progress",
        "extracting_entities",
        "正在提取摄入预览实体（LLM）...",
    );
    let entities = super::wiki_service::extract_entities(state, &source_content).await;

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    let updated_pages = estimate_related_pages_for_preview(state, &db_path, &entities);

    let preview_id = format!("preview_{}_{}", super::current_timestamp_ms(), uuid_v4_short());
    let preview = crate::models::IngestPreview {
        preview_id: preview_id.clone(),
        source_path: normalized_source_path.clone(),
        summary: summary.clone(),
        entities: entities.clone(),
        updated_pages,
    };
    let cached = super::CachedIngestPreview {
        source_type: normalized_source_type,
        source_path: normalized_source_path,
        source_content,
        summary,
        entities,
    };

    state
        .ingest_previews
        .lock()
        .expect("状态锁已被污染")
        .insert(preview_id, cached);
    Ok(preview)
}

/// 应用摄入预览：消费缓存并执行真实落盘。
pub async fn apply_ingest_preview(
    state: &AppState,
    preview_id: &str,
) -> Result<crate::models::IngestResult, String> {
    let normalized_preview_id = preview_id.trim();
    if normalized_preview_id.is_empty() {
        return Err("preview_id 不能为空".to_string());
    }

    let cached = state
        .ingest_previews
        .lock()
        .expect("状态锁已被污染")
        .remove(normalized_preview_id)
        .ok_or_else(|| format!("未找到预览缓存：{}", normalized_preview_id))?;

    state.record_outbox_event(
        "ingest_started",
        serde_json::json!({
            "source_path": cached.source_path.clone(),
            "task_id": super::current_timestamp_ms(),
            "source_type": cached.source_type,
            "from_preview": true,
        }),
    );

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())
    };
    let vault_path = match vault_path {
        Ok(path) => path,
        Err(err) => {
            state.record_ingest_failed_event(&cached.source_path, &err);
            return Err(err);
        }
    };

    // Markdown 源优先走原文件路径，避免 raw 文件名退化为临时文件名。
    let original_path = PathBuf::from(&cached.source_path);
    let use_original_markdown = (cached.source_type == "markdown"
        || cached.source_type == "md"
        || (cached.source_type == "file"
            && matches!(
                original_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|ext| ext.to_ascii_lowercase())
                    .as_deref(),
                Some("md" | "markdown")
            )))
        && original_path.exists();

    if use_original_markdown {
        return complete_ingest_with_precomputed_analysis(
            state,
            &vault_path,
            &original_path,
            &cached.source_path,
            &cached.source_content,
            &cached.summary,
            &cached.entities,
        )
        .await;
    }

    let tmp_path = std::env::temp_dir().join(format!(
        "llm_wiki_preview_apply_{}_{}.md",
        super::current_timestamp_ms(),
        uuid_v4_short()
    ));
    if let Err(err) = tokio::fs::write(&tmp_path, &cached.source_content).await {
        let message = format!("写入预览临时文件失败：{}", err);
        state.record_ingest_failed_event(&cached.source_path, &message);
        return Err(message);
    }

    let result = complete_ingest_with_precomputed_analysis(
        state,
        &vault_path,
        &tmp_path,
        &cached.source_path,
        &cached.source_content,
        &cached.summary,
        &cached.entities,
    )
    .await;
    let _ = tokio::fs::remove_file(&tmp_path).await;
    result
}

/// 导入任意支持格式文件（按扩展名自动路由）。
pub async fn ingest_file_impl(
    state: &AppState,
    source_path: &str,
    ocr_provider: Option<&str>,
) -> Result<crate::models::IngestResult, String> {
    let source_path_buf = PathBuf::from(source_path);
    validate_ingest_source_path(&source_path_buf)?;

    let extension = source_path_buf
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "md" | "markdown" => ingest_markdown(state, source_path_buf, None).await,
        "pdf" => ingest_pdf_impl(state, source_path).await,
        "txt" => {
            let content_bytes = fs::read(&source_path_buf)
                .map_err(|err| format!("读取文本文件失败：{}", err))?;
            let content = String::from_utf8_lossy(&content_bytes).to_string();
            ingest_text_via_temp_markdown(state, &source_path_buf, content, "txt").await
        }
        "docx" => {
            let content = extract_text_from_docx(&source_path_buf)?;
            ingest_text_via_temp_markdown(state, &source_path_buf, content, "docx").await
        }
        "pptx" => {
            let content = extract_text_from_pptx(&source_path_buf)?;
            ingest_text_via_temp_markdown(state, &source_path_buf, content, "pptx").await
        }
        ext if is_supported_image_extension(ext) => {
            let resolved_provider = normalize_ocr_provider(ocr_provider);
            let content = extract_text_from_image_with_fallback(&source_path_buf, resolved_provider)?;
            ingest_text_via_temp_markdown(state, &source_path_buf, content, "ocr").await
        }
        _ => Err(format!(
            "不支持的文件扩展名：{}。当前支持 md/markdown/pdf/docx/pptx/txt/png/jpg/jpeg/bmp/webp/tif/tiff/gif",
            if extension.is_empty() {
                "(无扩展名)"
            } else {
                extension.as_str()
            }
        )),
    }
}

/// 从本地文件提取纯文本，供 Chat 消息上下文使用（不写入 Vault）。
pub fn read_file_for_chat_impl(
    _state: &AppState,
    path: &str,
) -> Result<crate::models::FileChunk, String> {
    let source_path = std::path::Path::new(path.trim());
    let filename = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();

    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let raw_content = match ext.as_str() {
        "txt" | "md" | "markdown" | "csv" | "json" | "yaml" | "yml"
        | "toml" | "xml" | "html" | "htm" | "log" | "rs" | "py"
        | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" => {
            let bytes = std::fs::read(source_path)
                .map_err(|e| format!("读取文件失败: {e}"))?;
            String::from_utf8_lossy(&bytes).to_string()
        }
        "pdf" => {
            extract_text_from_pdf(source_path).or_else(|_| {
                let bytes = std::fs::read(source_path)
                    .map_err(|e| format!("读取 PDF 失败: {e}"))?;
                extract_text_from_pdf_with_pdf_extract(&bytes)
                    .ok_or_else(|| "PDF 文本提取失败，请确认文件包含可选中的文字".to_string())
            })?
        }
        "doc" => extract_text_from_doc(source_path)?,
        "docx" => extract_text_from_docx(source_path)?,
        "pptx" => extract_text_from_pptx(source_path)?,
        other => {
            return Err(format!(
                "不支持 .{other} 类型，支持：txt/md/pdf/doc/docx/pptx/csv/json 及常见代码文件"
            ))
        }
    };

    const MAX_CHARS: usize = 40_000;
    let total = raw_content.chars().count();
    let (content, truncated) = if total > MAX_CHARS {
        (raw_content.chars().take(MAX_CHARS).collect(), true)
    } else {
        (raw_content, false)
    };
    let char_count = content.chars().count();

    Ok(crate::models::FileChunk { filename, content, char_count, truncated })
}

/// 读取 PDF 文本后复用现有 Markdown ingest 流程。
pub async fn ingest_pdf_impl(
    state: &AppState,
    source_path: &str,
) -> Result<crate::models::IngestResult, String> {
    let source_path_text = source_path.trim().to_string();

    // 记录开始导入事件
    state.record_outbox_event(
        "ingest_started",
        serde_json::json!({
            "source_path": source_path_text.clone(),
            "task_id": super::current_timestamp_ms(),
        }),
    );

    let source_path_buf = PathBuf::from(&source_path_text);
    if let Err(err) = validate_pdf_source_path(&source_path_buf) {
        state.record_ingest_failed_event(&source_path_text, &err);
        return Err(err);
    }

    let extracted_text = match extract_text_from_pdf(&source_path_buf) {
        Ok(text) => text,
        Err(err) => {
            if should_fallback_to_pdf_ocr(&err) {
                let primary_provider = {
                    let guard = state.inner.lock().expect("状态锁已被污染");
                    normalize_ocr_provider(guard.default_ocr_provider.as_deref())
                };

                match extract_text_from_pdf_via_ocr(&source_path_buf, primary_provider) {
                    Ok(ocr_output) => {
                        state.record_outbox_event(
                            "ingest_pdf_ocr_fallback",
                            serde_json::json!({
                                "source_path": source_path_text.clone(),
                                "primary_provider": primary_provider.as_str(),
                                "page_count": ocr_output.page_count,
                            }),
                        );
                        return ingest_text_via_temp_markdown(
                            state,
                            &source_path_buf,
                            ocr_output.text,
                            "pdf_ocr",
                        )
                        .await
                        .map(|mut result| {
                            result.message = format!(
                                "{}（已自动 OCR 回退：provider={}，页数={}）",
                                result.message,
                                primary_provider.as_str(),
                                ocr_output.page_count
                            );
                            result
                        })
                        .map_err(|ingest_err| {
                            state.record_ingest_failed_event(&source_path_text, &ingest_err);
                            ingest_err
                        });
                    }
                    Err(ocr_err) => {
                        let message = build_pdf_ocr_fallback_failure_message(&err, &ocr_err);
                        state.record_ingest_failed_event(&source_path_text, &message);
                        return Err(message);
                    }
                }
            }

            state.record_ingest_failed_event(&source_path_text, &err);
            return Err(err);
        }
    };
    ingest_text_via_temp_markdown(state, &source_path_buf, extracted_text, "pdf")
        .await
        .map_err(|err| {
            state.record_ingest_failed_event(&source_path_text, &err);
            err
        })
}

/// 拉取 URL 文本内容后走现有 ingest 流程
pub async fn ingest_url_impl(
    state: &AppState,
    url: &str,
) -> Result<crate::models::IngestResult, String> {
    let source_url = url.trim().to_string();

    // 记录开始导入事件
    state.record_outbox_event(
        "ingest_started",
        serde_json::json!({
            "source_path": source_url.clone(),
            "task_id": super::current_timestamp_ms(),
        }),
    );

    // 1. 拉取 URL 内容（通过 browser 模块统一抓取）
    let text = fetch_url_content(state, &source_url).await.map_err(|e| {
        state.record_ingest_failed_event(&source_url, &e);
        e
    })?;

    if text.trim().is_empty() {
        let message = "URL 返回内容为空".to_string();
        state.record_ingest_failed_event(&source_url, &message);
        return Err(message);
    }

    // 2. 将纯文本写入临时 Markdown 文件，复用 ingest_markdown
    let tmp_path = std::env::temp_dir().join(format!("llm_wiki_url_{}.md", uuid_v4_short()));
    tokio::fs::write(&tmp_path, &text).await.map_err(|e| {
        let message = format!("写入临时文件失败：{e}");
        state.record_ingest_failed_event(&source_url, &message);
        message
    })?;

    // 从 URL 提取有意义的显示名称（优先路径最后一段，否则用域名）
    let url_display = reqwest::Url::parse(&source_url).ok().and_then(|parsed| {
        let path_seg = parsed
            .path_segments()
            .and_then(|segments| segments.last())
            .filter(|s| !s.is_empty() && s.len() < 50);
        if let Some(seg) = path_seg {
            return Some(seg.to_string());
        }
        parsed
            .host_str()
            .map(|h| h.trim_start_matches("www.").to_string())
    });
    let result = ingest_markdown(state, tmp_path.clone(), url_display).await;

    // 3. 清理临时文件（忽略错误）
    let _ = tokio::fs::remove_file(&tmp_path).await;

    result.map_err(|err| {
        state.record_ingest_failed_event(&source_url, &err);
        err
    })
}

/// 将消息入队到 ingest 队列。
pub fn enqueue_ingest(
    state: &AppState,
    source_type: String,
    source_path: String,
) -> Result<i64, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化，无法入队".to_string())?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    let now = super::current_timestamp_ms();
    let id = db::db_enqueue_ingest(&conn, &source_type, &source_path, &now)?;
    state.record_outbox_event(
        "ingest_queue_enqueued",
        serde_json::json!({ "id": id, "source_type": source_type, "source_path": source_path }),
    );
    Ok(id)
}

/// 列出所有 ingest 队列记录。
pub fn list_ingest_queue(
    state: &AppState,
) -> Result<Vec<crate::models::IngestQueueItem>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    db::db_list_ingest_queue(&conn)
}

/// 取消一条 ingest 队列记录（queued → cancelled）。
pub fn cancel_ingest_item(state: &AppState, id: i64) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    let now = super::current_timestamp_ms();
    db::db_update_ingest_queue_status(&conn, id, "cancelled", None, &now)
}

/// 重试一条失败/取消的 ingest 队列记录（failed/cancelled → queued）。
pub fn retry_ingest_item(state: &AppState, id: i64) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    let now = super::current_timestamp_ms();
    db::db_update_ingest_queue_status(&conn, id, "queued", None, &now)
}

/// 永久删除一条 failed/cancelled/done 的 ingest 队列记录。
pub fn delete_ingest_item(state: &AppState, id: i64) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let conn =
        rusqlite::Connection::open(&db_path).map_err(|e| format!("打开数据库失败: {}", e))?;
    db::db_delete_ingest_queue_item(&conn, id)
}

/// 计算给定页面路径列表中所有存在 embedding 的页面对的余弦相似度。
/// 返回 HashMap，key 为 "pathA||pathB"（路径字典序排列），value 为余弦相似度。
/// 仅返回相似度 >= min_sim（默认 0.25）的对，最多 max_pairs（默认 1000）对。
pub fn get_page_embedding_similarities(
    state: &AppState,
    paths: Vec<String>,
) -> Result<std::collections::HashMap<String, f64>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;

    let all_records = db::list_embeddings(&db_path, 2000)?;
    let path_set: std::collections::HashSet<&str> = paths.iter().map(|s| s.as_str()).collect();

    let records: Vec<_> = all_records
        .iter()
        .filter(|r| path_set.contains(r.page_path.as_str()))
        .collect();

    const MIN_SIM: f64 = 0.25;
    const MAX_PAIRS: usize = 1000;

    let mut pairs = std::collections::HashMap::new();
    'outer: for i in 0..records.len() {
        for j in (i + 1)..records.len() {
            let sim = db::cosine_similarity(&records[i].embedding, &records[j].embedding);
            if sim >= MIN_SIM {
                let (a, b) = if records[i].page_path <= records[j].page_path {
                    (&records[i].page_path, &records[j].page_path)
                } else {
                    (&records[j].page_path, &records[i].page_path)
                };
                pairs.insert(format!("{}||{}", a, b), sim);
                if pairs.len() >= MAX_PAIRS {
                    break 'outer;
                }
            }
        }
    }
    Ok(pairs)
}

/// 启动队列 worker（tauri 异步运行时串行消费）。
/// 通过 tauri::AppHandle 获取托管的 AppState，避免另建独立实例。
pub fn start_queue_worker(handle: tauri::AppHandle) {
    const MAX_RETRIES: i64 = 3;

    tauri::async_runtime::spawn(async move {
        // 启动时重置上次崩溃遗留的 running 任务，并将可重试的 failed 任务重入队列
        {
            let state = handle.state::<AppState>();
            if let Some(db_path) = state.outbox_db_path() {
                if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                    let now = super::current_timestamp_ms();
                    let _ = db::db_reset_stale_running(&conn, &now);
                    let _ = db::db_requeue_restartable_items(&conn, MAX_RETRIES, &now);
                }
            }
        }
        loop {
            let state = handle.state::<AppState>();

            // 原子性认领下一条 queued item（queued → running）
            let claimed = {
                let Some(db_path) = state.outbox_db_path() else {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                };
                let conn = match rusqlite::Connection::open(&db_path) {
                    Ok(c) => c,
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        continue;
                    }
                };
                let now = super::current_timestamp_ms();
                match db::db_claim_next_ingest_queue_item(&conn, &now) {
                    Ok(item) => item,
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                        continue;
                    }
                }
            };

            let item = match claimed {
                Some(i) => i,
                None => {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    continue;
                }
            };

            let item_id = item.id;
            let source_type = item.source_type.clone();
            let source_path = item.source_path.clone();

            state.record_outbox_event(
                "ingest_queue_started",
                serde_json::json!({ "id": item_id, "source_type": source_type, "source_path": source_path }),
            );

            // 根据 source_type 调用对应 impl
            let exec_result: Result<crate::models::IngestResult, String> =
                match source_type.as_str() {
                    "file" => ingest_file_impl(&state, &source_path, None).await,
                    "url" => ingest_url_impl(&state, &source_path).await,
                    "markdown" => {
                        ingest_markdown(
                            &state,
                            std::path::PathBuf::from(&source_path),
                            None,
                        )
                        .await
                    }
                    other => Err(format!("未知 source_type: {}", other)),
                };

            let now = super::current_timestamp_ms();
            match exec_result {
                Ok(_) => {
                    if let Some(db_path) = state.outbox_db_path() {
                        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                            let _ = db::db_mark_ingest_queue_item_done(&conn, item_id, &now);
                        }
                    }
                    state.record_outbox_event(
                        "ingest_queue_done",
                        serde_json::json!({ "id": item_id }),
                    );
                }
                Err(err) => {
                    if let Some(db_path) = state.outbox_db_path() {
                        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
                            let _ = db::db_mark_ingest_queue_item_failed(
                                &conn, item_id, err.as_str(), &now,
                            );
                        }
                    }
                    state.record_outbox_event(
                        "ingest_queue_failed",
                        serde_json::json!({ "id": item_id, "error": err }),
                    );
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// 执行摄入落盘（可复用预计算摘要与实体）。
async fn complete_ingest_with_precomputed_analysis(
    state: &AppState,
    vault_path: &Path,
    ingest_source_path: &Path,
    display_source_path: &str,
    source_content: &str,
    llm_summary: &str,
    entities: &[String],
) -> Result<crate::models::IngestResult, String> {
    state.emit_progress("ingest_progress", "writing_wiki", "写入 Wiki 页面...");
    // 从显示路径提取有意义的显示名称，用于 wiki 文件名（而非 ingest-时间戳）
    let display_name = extract_wiki_display_name(display_source_path);
    let mut result = match crate::vault::ingest_markdown(
        vault_path,
        ingest_source_path,
        Some(llm_summary),
        entities,
        display_name.as_deref(),
        Some(display_source_path),
    ) {
        Ok(result) => {
            state.push_log(
                LogLevel::Info,
                format!(
                    "Markdown 导入成功: {} -> {}",
                    display_source_path, result.wiki_path
                ),
            );
            result
        }
        Err(err) => {
            state.push_log(LogLevel::Warn, format!("Markdown 导入失败: {}", err));
            state.record_ingest_failed_event(display_source_path, &err);
            return Err(err);
        }
    };

    state.emit_progress("ingest_progress", "updating_links", "注入双向链接...");
    let db_path = vault_path.join(".app").join("meta.db");
    let wiki_title = PathBuf::from(&result.wiki_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();
    let updated_pages = super::wiki_service::update_related_pages_with_link(
        state,
        &db_path,
        vault_path,
        &result.wiki_path,
        &wiki_title,
        entities,
    )
    .await;

    state.emit_progress(
        "ingest_progress",
        "embedding",
        "正在向量化...",
    );
    let embed_provider = state.get_embed_provider();
    // 截取前 2000 字；multilingual-e5-small 上限 512 token，中文约 1.5 token/字，2000 字 ≈ 1300 token 需截断
    // OnnxEmbedder 内部会按 max_input_tokens 截断，这里取 2000 字作为粗粒度保护
    let embed_content: String = source_content.chars().take(2000).collect();
    let embed_backend_id = embed_provider.backend_id();
    let embed_dim = embed_provider.dimension();
    match embed_provider.embed(&embed_content).await {
        Ok(embedding) => {
            if let Err(err) = db::upsert_embedding(&db_path, &result.wiki_path, &embedding, &embed_backend_id, embed_dim) {
                state.push_log(LogLevel::Warn, format!("写入向量数据库失败: {}", err));
            }
        }
        Err(err) => {
            let hint = if matches!(
                err,
                crate::llm::EmbedError::InitFailed(_) | crate::llm::EmbedError::Unavailable
            ) {
                "（Embed 服务未就绪，请确认 ONNX 模型文件存在或 Ollama 已启动）"
            } else {
                ""
            };
            state.push_log(
                LogLevel::Warn,
                format!("向量化失败，跳过语义检索索引{}：{}", hint, err),
            );
        }
    }

    let ingest_source_path_text = ingest_source_path.to_string_lossy().to_string();
    if ingest_source_path_text != display_source_path {
        result.source_path = display_source_path.to_string();
        result.message = result
            .message
            .replace(&ingest_source_path_text, display_source_path);
    }

    result.entities = entities.to_vec();
    result.updated_pages = updated_pages;
    state.record_outbox_event(
        "ingest_completed",
        serde_json::json!({
            "source_path": result.source_path.clone(),
            "raw_path": result.raw_path.clone(),
            "wiki_path": result.wiki_path.clone(),
            "entities": result.entities.clone(),
            "updated_pages": result.updated_pages.clone(),
        }),
    );
    Ok(result)
}

/// 预估摄入后可能更新的关联页（只读，不写盘）。
fn estimate_related_pages_for_preview(
    state: &AppState,
    db_path: &Path,
    entities: &[String],
) -> Vec<String> {
    if entities.is_empty() || !db_path.exists() {
        return Vec::new();
    }

    let mut token_set = std::collections::HashSet::new();
    for entity in entities {
        for token in tokenize_query(entity) {
            token_set.insert(token);
        }
    }
    let tokens: Vec<String> = token_set.into_iter().collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    match db::search_fts_page_paths(db_path, &tokens, 5) {
        Ok(paths) => paths,
        Err(err) => {
            state.push_log(
                LogLevel::Warn,
                format!("预览阶段估算相关页面失败，已忽略：{}", err),
            );
            Vec::new()
        }
    }
}

/// 预览源内容读取（支持文件和 URL）。
async fn load_preview_source_content(
    state: &AppState,
    source_type: &str,
    source_path: &str,
    ocr_provider: Option<&str>,
) -> Result<String, String> {
    match source_type {
        "url" => fetch_url_content(state, source_path).await,
        "markdown" | "md" => {
            let path = PathBuf::from(source_path);
            validate_ingest_source_path(&path)?;
            fs::read_to_string(&path).map_err(|err| format!("读取 Markdown 文件失败：{}", err))
        }
        "pdf" => {
            let path = PathBuf::from(source_path);
            extract_preview_pdf_text(state, &path)
        }
        "file" | "txt" | "docx" | "pptx" | "image" | "ocr" => {
            let path = PathBuf::from(source_path);
            extract_preview_file_content_by_extension(state, &path, ocr_provider)
        }
        _ => Err(format!(
            "不支持的 source_type：{}（支持 file/markdown/pdf/url）",
            source_type
        )),
    }
}

fn extract_preview_file_content_by_extension(
    state: &AppState,
    source_path: &Path,
    ocr_provider: Option<&str>,
) -> Result<String, String> {
    validate_ingest_source_path(source_path)?;
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "md" | "markdown" => {
            fs::read_to_string(source_path).map_err(|err| format!("读取 Markdown 文件失败：{}", err))
        }
        "pdf" => extract_preview_pdf_text(state, source_path),
        "txt" => {
            let content_bytes = fs::read(source_path).map_err(|err| format!("读取文本文件失败：{}", err))?;
            Ok(String::from_utf8_lossy(&content_bytes).to_string())
        }
        "docx" => extract_text_from_docx(source_path),
        "pptx" => extract_text_from_pptx(source_path),
        ext if is_supported_image_extension(ext) => {
            let resolved_provider = normalize_ocr_provider(ocr_provider);
            extract_text_from_image_with_fallback(source_path, resolved_provider)
        }
        _ => Err(format!(
            "不支持的文件扩展名：{}。当前支持 md/markdown/pdf/docx/pptx/txt/png/jpg/jpeg/bmp/webp/tif/tiff/gif",
            if extension.is_empty() { "(无扩展名)" } else { extension.as_str() }
        )),
    }
}

fn extract_preview_pdf_text(state: &AppState, source_path: &Path) -> Result<String, String> {
    validate_pdf_source_path(source_path)?;
    match extract_text_from_pdf(source_path) {
        Ok(text) => Ok(text),
        Err(err) => {
            if !should_fallback_to_pdf_ocr(&err) {
                return Err(err);
            }
            let primary_provider = {
                let guard = state.inner.lock().expect("状态锁已被污染");
                normalize_ocr_provider(guard.default_ocr_provider.as_deref())
            };
            match extract_text_from_pdf_via_ocr(source_path, primary_provider) {
                Ok(ocr_output) => Ok(ocr_output.text),
                Err(ocr_err) => Err(build_pdf_ocr_fallback_failure_message(&err, &ocr_err)),
            }
        }
    }
}

async fn fetch_url_content(state: &AppState, source_url: &str) -> Result<String, String> {
    let trimmed = source_url.trim();
    if trimmed.is_empty() {
        return Err("URL 不能为空".to_string());
    }

    let result = crate::browser::fetch_url(trimmed, Default::default()).await?;

    state.push_log(
        LogLevel::Info,
        format!(
            "URL 抓取完成：method={} chars={} title={}",
            result.fetch_method,
            result.char_count,
            result.title.chars().take(40).collect::<String>()
        ),
    );

    Ok(result.text)
}

/// 将提取后的纯文本写入临时 Markdown，再复用 ingest_markdown。
async fn ingest_text_via_temp_markdown(
    state: &AppState,
    source_path: &Path,
    content: String,
    route_tag: &str,
) -> Result<crate::models::IngestResult, String> {
    let normalized = content.replace('\u{0}', "").trim().to_string();
    if normalized.is_empty() {
        return Err(format!(
            "{} 提取结果为空，请确认文件内容可读取",
            route_tag.to_uppercase()
        ));
    }

    let tmp_path =
        std::env::temp_dir().join(format!("llm_wiki_{}_{}.md", route_tag, uuid_v4_short()));
    tokio::fs::write(&tmp_path, normalized)
        .await
        .map_err(|e| format!("写入临时 Markdown 失败：{e}"))?;

    // 传入原始源文件名作为 display_name，使 wiki 页面名称有可读性
    let source_stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(route_tag)
        .to_string();
    let mut result = ingest_markdown(state, tmp_path.clone(), Some(source_stem)).await;

    // 无论 ingest 成功或失败都尝试清理临时文件。
    let _ = tokio::fs::remove_file(&tmp_path).await;

    if let Ok(inner) = &mut result {
        let source_display = source_path.to_string_lossy().to_string();
        let tmp_display = tmp_path.to_string_lossy().to_string();
        inner.source_path = source_display.clone();
        inner.message = inner.message.replace(&tmp_display, &source_display);
    }

    result
}

// ─── Free functions (not tied to AppState) ────────────────────────────────────

pub fn validate_ingest_source_path(source_path: &Path) -> Result<(), String> {
    if !source_path.exists() {
        return Err(format!("文件不存在：{}", source_path.to_string_lossy()));
    }

    if !source_path.is_file() {
        return Err(format!("路径不是文件：{}", source_path.to_string_lossy()));
    }

    Ok(())
}

fn is_supported_image_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "bmp" | "webp" | "tif" | "tiff" | "gif"
    )
}

pub(super) fn validate_pdf_source_path(source_path: &Path) -> Result<(), String> {
    if !source_path.exists() {
        return Err(format!("PDF 文件不存在：{}", source_path.to_string_lossy()));
    }

    if !source_path.is_file() {
        return Err(format!(
            "PDF 路径不是文件：{}",
            source_path.to_string_lossy()
        ));
    }

    let ext = source_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if ext != "pdf" {
        return Err(format!(
            "文件扩展名错误，仅支持 .pdf：{}",
            source_path.to_string_lossy()
        ));
    }

    Ok(())
}

/// Heuristic text extractor for legacy Word .doc (OLE compound document) files.
/// Tries UTF-16LE scan for CJK content, falls back to ASCII string extraction.
fn extract_text_from_doc(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("读取 .doc 失败: {e}"))?;

    // Validate OLE compound document magic bytes
    const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    if bytes.len() < 8 || bytes[0..8] != OLE_MAGIC {
        return Err("不是有效的 .doc 文件（非 OLE 复合文档格式）".to_string());
    }

    // UTF-16LE scan for CJK and Latin text (try both byte alignments)
    let utf16_text = doc_scan_utf16le(&bytes);

    // ASCII run extraction as fallback / supplement
    let ascii_text = doc_scan_ascii(&bytes);

    let has_cjk = utf16_text.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
    let text = if has_cjk { utf16_text } else { ascii_text };

    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim().to_string();

    if text.len() < 20 {
        return Err(
            "无法从 .doc 文件提取可读文本（建议将文件另存为 .docx 格式后重试）".to_string(),
        );
    }
    Ok(text)
}

fn is_doc_text_char(cp: u16) -> bool {
    matches!(
        cp,
        0x0009
            | 0x000A
            | 0x000D
            | 0x0020..=0x007E
            | 0x00A0..=0x00FF
            | 0x2010..=0x2027
            | 0x3000..=0x303F
            | 0x3040..=0x30FF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0xFF01..=0xFF60
    )
}

fn doc_scan_utf16le(bytes: &[u8]) -> String {
    let mut best = String::new();
    for start in 0..2usize {
        let mut tokens: Vec<String> = Vec::new();
        let mut run: Vec<u16> = Vec::new();
        let mut i = start;
        while i + 1 < bytes.len() {
            let cp = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
            if is_doc_text_char(cp) {
                run.push(cp);
            } else {
                if run.len() >= 4 {
                    tokens.push(String::from_utf16_lossy(&run).to_string());
                }
                run.clear();
            }
            i += 2;
        }
        if run.len() >= 4 {
            tokens.push(String::from_utf16_lossy(&run).to_string());
        }
        let candidate = tokens.join(" ");
        let cjk = |s: &str| s.chars().filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c)).count();
        if cjk(&candidate) > cjk(&best) || (cjk(&candidate) == cjk(&best) && candidate.len() > best.len()) {
            best = candidate;
        }
    }
    best
}

fn doc_scan_ascii(bytes: &[u8]) -> String {
    const MIN_RUN: usize = 8;
    let mut tokens: Vec<String> = Vec::new();
    let mut run = String::new();
    for &b in bytes {
        match b {
            0x09 => run.push(' '),
            0x20..=0x7E => run.push(b as char),
            _ => {
                if run.len() >= MIN_RUN {
                    tokens.push(std::mem::take(&mut run));
                } else {
                    run.clear();
                }
            }
        }
    }
    if run.len() >= MIN_RUN {
        tokens.push(run);
    }
    tokens.join(" ")
}

fn extract_text_from_docx(source_path: &Path) -> Result<String, String> {
    let mut archive = open_zip_archive(source_path, "DOCX")?;
    let xml = read_zip_entry_utf8_lossy(&mut archive, "word/document.xml", "DOCX")?
        .ok_or_else(|| "DOCX 缺少 word/document.xml，无法提取正文".to_string())?;

    let text = extract_docx_paragraphs(&xml);
    normalize_extracted_doc_text(text, "DOCX")
}

/// 按段落（<w:p>）提取 DOCX 文本，段落间用空行分隔，段内 <w:t> 拼接。
pub(super) fn extract_docx_paragraphs(xml: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut offset = 0usize;

    while let Some(para_start_rel) = xml[offset..].find("<w:p") {
        let para_start = offset + para_start_rel;
        // 找到段落结束标记 </w:p>
        let Some(para_end_rel) = xml[para_start..].find("</w:p>") else {
            break;
        };
        let para_end = para_start + para_end_rel + "</w:p>".len();
        let para_xml = &xml[para_start..para_end];

        // 提取段落内所有 <w:t> 文本并拼接（段内不加换行）
        let text = extract_xml_text_by_tag(para_xml, "w:t")
            .lines()
            .collect::<Vec<_>>()
            .join(" ");
        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            paragraphs.push(trimmed);
        }
        offset = para_end;
    }

    paragraphs.join("\n\n")
}

fn extract_text_from_pptx(source_path: &Path) -> Result<String, String> {
    let mut archive = open_zip_archive(source_path, "PPTX")?;
    let mut slide_entries: Vec<(String, String)> = Vec::new();

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| format!("读取 PPTX 条目失败：{}", err))?;
        let name = file.name().to_string();
        if !is_pptx_slide_xml_entry(&name) {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|err| format!("读取 PPTX 页面内容失败：{}", err))?;
        slide_entries.push((name, String::from_utf8_lossy(&bytes).to_string()));
    }

    if slide_entries.is_empty() {
        return Err("PPTX 未检测到可读取幻灯片页面".to_string());
    }

    slide_entries.sort_by_key(|(name, _)| extract_slide_number(name));
    let mut pages = Vec::new();
    for (_, xml) in slide_entries {
        let text = extract_xml_text_by_tag(&xml, "a:t");
        if !text.is_empty() {
            pages.push(text);
        }
    }

    normalize_extracted_doc_text(pages.join("\n\n"), "PPTX")
}

/// 从路径中提取幻灯片编号数字，用于自然数排序。
/// 例如 "ppt/slides/slide12.xml" -> 12，未匹配时返回 0。
pub(super) fn extract_slide_number(name: &str) -> u32 {
    // 匹配 "slide" 后面的数字，例如 "ppt/slides/slide12.xml" -> 12
    name.rfind("slide")
        .and_then(|pos| {
            let after = &name[pos + 5..]; // "slide" 长度 = 5
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u32>().ok()
        })
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OcrProvider {
    Tesseract,
    Paddle,
}

impl OcrProvider {
    fn as_str(self) -> &'static str {
        match self {
            OcrProvider::Tesseract => "tesseract",
            OcrProvider::Paddle => "paddle",
        }
    }
}

/// 归一化 OCR provider：非法值统一回退到 tesseract。
pub(super) fn normalize_ocr_provider(provider: Option<&str>) -> OcrProvider {
    let normalized = provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("tesseract")
        .to_ascii_lowercase();

    match normalized.as_str() {
        "paddle" => OcrProvider::Paddle,
        _ => OcrProvider::Tesseract,
    }
}

/// 根据首选 provider 生成执行顺序（主用失败后自动回退）。
pub(super) fn resolve_ocr_provider_order(primary: OcrProvider) -> [OcrProvider; 2] {
    match primary {
        OcrProvider::Tesseract => [OcrProvider::Tesseract, OcrProvider::Paddle],
        OcrProvider::Paddle => [OcrProvider::Paddle, OcrProvider::Tesseract],
    }
}

fn extract_text_from_image_with_fallback(
    source_path: &Path,
    primary_provider: OcrProvider,
) -> Result<String, String> {
    let provider_order = resolve_ocr_provider_order(primary_provider);
    let mut provider_errors = Vec::new();

    for provider in provider_order {
        match extract_text_from_image_with_provider(source_path, provider) {
            Ok(text) => return Ok(text),
            Err(err) => {
                provider_errors.push(format!(
                    "{}：{}",
                    provider.as_str(),
                    shorten_error_snippet(err.trim(), 60)
                ));
            }
        }
    }

    Err(format!(
        "图片 OCR 失败，已按顺序尝试 {} -> {}。{}",
        provider_order[0].as_str(),
        provider_order[1].as_str(),
        provider_errors.join("；")
    ))
}

fn extract_text_from_image_with_provider(
    source_path: &Path,
    provider: OcrProvider,
) -> Result<String, String> {
    match provider {
        OcrProvider::Tesseract => extract_text_from_image_with_tesseract(source_path),
        OcrProvider::Paddle => extract_text_from_image_with_paddle(source_path),
    }
}

fn extract_text_from_image_with_tesseract(source_path: &Path) -> Result<String, String> {
    let output = run_tesseract_ocr_command(source_path)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_lower = stderr.to_lowercase();
        if stderr_lower.contains("failed loading language")
            || stderr_lower.contains("chi_sim")
            || stderr_lower.contains("traineddata")
        {
            return Err(
                "Tesseract 已安装，但缺少可用语言包（chi_sim/eng）。请安装对应 traineddata 或在设置中改用 PaddleOCR"
                    .to_string(),
            );
        }
        let short_reason = shorten_error_snippet(stderr.trim(), 80);
        if short_reason.is_empty() {
            return Err("Tesseract OCR 失败，请确认图片可读且已安装中文/英文语言包".to_string());
        }
        return Err(format!("Tesseract OCR 失败：{}", short_reason));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    normalize_extracted_doc_text(text, "图片 OCR")
}

fn extract_text_from_image_with_paddle(source_path: &Path) -> Result<String, String> {
    let output = run_paddle_ocr_command(source_path)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let short_reason = shorten_error_snippet(stderr.trim(), 80);
        if short_reason.is_empty() {
            return Err("Paddle OCR 失败，请确认命令可用且模型已安装".to_string());
        }
        return Err(format!("Paddle OCR 失败：{}", short_reason));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let extracted_text = parse_paddle_ocr_stdout(&stdout);
    normalize_extracted_doc_text(extracted_text, "图片 OCR")
}

/// 生成 tesseract 命令候选列表：
/// - 先尝试 PATH 中的 `tesseract`
/// - 再尝试 Windows 常见安装目录，避免 PATH 尚未刷新的场景
fn build_tesseract_command_candidates() -> Vec<String> {
    let mut candidates = Vec::<String>::new();
    let mut seen = HashSet::<String>::new();

    let mut push_candidate = |value: String| {
        let normalized = value.trim().to_lowercase();
        if normalized.is_empty() || !seen.insert(normalized) {
            return;
        }
        candidates.push(value);
    };

    push_candidate("tesseract".to_string());

    if cfg!(target_os = "windows") {
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            push_candidate(format!(r"{}\Tesseract-OCR\tesseract.exe", program_files));
        }
        if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
            push_candidate(format!(
                r"{}\Tesseract-OCR\tesseract.exe",
                program_files_x86
            ));
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            push_candidate(format!(
                r"{}\Programs\Tesseract-OCR\tesseract.exe",
                local_app_data
            ));
        }
        // 环境变量缺失时的硬编码兜底路径（Windows 默认安装位置）
        push_candidate(r"C:\Program Files\Tesseract-OCR\tesseract.exe".to_string());
        push_candidate(r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe".to_string());
    }

    candidates
}

fn run_tesseract_ocr_command(source_path: &Path) -> Result<std::process::Output, String> {
    let candidates = build_tesseract_command_candidates();
    let mut last_not_found: Option<io::Error> = None;

    for candidate in &candidates {
        match std::process::Command::new(candidate)
            .arg(source_path)
            .arg("stdout")
            .arg("-l")
            .arg("chi_sim+eng")
            .output()
        {
            Ok(output) => return Ok(output),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                last_not_found = Some(err);
            }
            Err(err) => {
                return Err(format!(
                    "调用 tesseract 失败（{}）：{}",
                    candidate,
                    shorten_error_snippet(&err.to_string(), 80)
                ));
            }
        }
    }

    let base_message = match last_not_found.as_ref() {
        Some(err) => format_tesseract_spawn_error(err),
        None => "调用 tesseract 失败：未知错误".to_string(),
    };
    Err(format!(
        "{}；已尝试命令：{}",
        base_message,
        candidates.join(" | ")
    ))
}

fn run_paddle_ocr_command(source_path: &Path) -> Result<std::process::Output, String> {
    std::process::Command::new("paddleocr")
        .arg("--image_dir")
        .arg(source_path)
        .arg("--use_angle_cls")
        .arg("true")
        .arg("--lang")
        .arg("ch")
        .output()
        .map_err(|err| format_paddle_spawn_error(&err))
}

pub(super) fn format_tesseract_spawn_error(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "未检测到 tesseract 命令，请先安装 Tesseract OCR 并加入 PATH".to_string()
    } else {
        format!(
            "调用 tesseract 失败：{}",
            shorten_error_snippet(&err.to_string(), 60)
        )
    }
}

fn format_paddle_spawn_error(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "未检测到 paddleocr 命令，请先安装 PaddleOCR 并加入 PATH".to_string()
    } else {
        format!(
            "调用 paddleocr 失败：{}",
            shorten_error_snippet(&err.to_string(), 60)
        )
    }
}

fn normalize_extracted_doc_text(text: String, source_type: &str) -> Result<String, String> {
    let normalized = text.replace('\u{0}', "").trim().to_string();
    if normalized.is_empty() {
        Err(format!(
            "{} 提取结果为空，可能是扫描件、图片型内容或文档受保护",
            source_type
        ))
    } else {
        Ok(normalized)
    }
}

fn open_zip_archive(
    source_path: &Path,
    source_type: &str,
) -> Result<zip::ZipArchive<fs::File>, String> {
    let file = fs::File::open(source_path)
        .map_err(|err| format!("打开 {} 文件失败：{}", source_type, err))?;
    zip::ZipArchive::new(file).map_err(|err| format!("解析 {} 压缩结构失败：{}", source_type, err))
}

fn read_zip_entry_utf8_lossy<R: io::Read + io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    entry_name: &str,
    source_type: &str,
) -> Result<Option<String>, String> {
    match archive.by_name(entry_name) {
        Ok(mut file) => {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(|err| {
                format!("读取 {} 条目失败（{}）：{}", source_type, entry_name, err)
            })?;
            Ok(Some(String::from_utf8_lossy(&bytes).to_string()))
        }
        Err(zip::result::ZipError::FileNotFound) => Ok(None),
        Err(err) => Err(format!(
            "读取 {} 条目失败（{}）：{}",
            source_type, entry_name, err
        )),
    }
}

fn is_pptx_slide_xml_entry(entry_name: &str) -> bool {
    entry_name.starts_with("ppt/slides/slide") && entry_name.ends_with(".xml")
}

/// 提取 PaddleOCR 输出中的识别文本，优先抓取引号内容。
fn parse_paddle_ocr_stdout(stdout: &str) -> String {
    let mut collected = Vec::new();

    for raw_line in stdout.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let mut quoted_values = extract_quoted_segments(line);
        quoted_values.retain(|value| {
            let trimmed = value.trim();
            !trimmed.is_empty()
                && trimmed != "OCR"
                && trimmed != "result"
                && !trimmed.starts_with("http://")
                && !trimmed.starts_with("https://")
        });

        if !quoted_values.is_empty() {
            collected.extend(quoted_values);
            continue;
        }

        if line.starts_with('[') || line.contains("INFO") || line.contains("DEBUG") {
            continue;
        }
        collected.push(line.to_string());
    }

    collected.join("\n")
}

fn extract_quoted_segments(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    let mut results = Vec::new();

    while index < chars.len() {
        let quote = chars[index];
        if quote != '\'' && quote != '"' {
            index += 1;
            continue;
        }

        let mut end = index + 1;
        while end < chars.len() {
            if chars[end] == quote && chars[end.saturating_sub(1)] != '\\' {
                let value: String = chars[index + 1..end].iter().collect();
                if value
                    .chars()
                    .any(|ch| ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch))
                {
                    results.push(value);
                }
                break;
            }
            end += 1;
        }

        index = end.saturating_add(1);
    }

    results
}

pub(super) fn extract_xml_text_by_tag(xml: &str, tag_name: &str) -> String {
    let open_tag = format!("<{}", tag_name);
    let close_tag = format!("</{}>", tag_name);
    let mut values = Vec::new();
    let mut offset = 0usize;

    while let Some(start_rel) = xml[offset..].find(&open_tag) {
        let start_idx = offset + start_rel;
        let Some(open_end_rel) = xml[start_idx..].find('>') else {
            break;
        };
        let content_start = start_idx + open_end_rel + 1;
        let Some(close_rel) = xml[content_start..].find(&close_tag) else {
            break;
        };
        let content_end = content_start + close_rel;
        let decoded = decode_xml_entities(&xml[content_start..content_end]);
        let trimmed = decoded.trim();
        if !trimmed.is_empty() {
            values.push(trimmed.to_string());
        }
        offset = content_end + close_tag.len();
    }

    values.join("\n")
}

fn decode_xml_entities(raw_text: &str) -> String {
    let chars: Vec<char> = raw_text.chars().collect();
    let mut decoded = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '&' {
            let mut end = index + 1;
            while end < chars.len() && chars[end] != ';' {
                end += 1;
            }

            if end < chars.len() && chars[end] == ';' {
                let entity: String = chars[index + 1..end].iter().collect();
                if let Some(value) = decode_xml_entity(&entity) {
                    decoded.push_str(&value);
                    index = end + 1;
                    continue;
                }
            }
        }

        decoded.push(chars[index]);
        index += 1;
    }

    decoded
}

fn decode_xml_entity(entity: &str) -> Option<String> {
    let decoded = match entity {
        "amp" => "&".to_string(),
        "lt" => "<".to_string(),
        "gt" => ">".to_string(),
        "quot" => "\"".to_string(),
        "apos" => "'".to_string(),
        _ if entity.starts_with("#x") || entity.starts_with("#X") => {
            let code = u32::from_str_radix(&entity[2..], 16).ok()?;
            char::from_u32(code)?.to_string()
        }
        _ if entity.starts_with('#') => {
            let code: u32 = entity[1..].parse().ok()?;
            char::from_u32(code)?.to_string()
        }
        _ => return None,
    };
    Some(decoded)
}

fn shorten_error_snippet(message: &str, max_chars: usize) -> String {
    message.chars().take(max_chars).collect()
}

fn extract_text_from_pdf(source_path: &Path) -> Result<String, String> {
    let file_bytes =
        fs::read(source_path).map_err(|err| format!("读取 PDF 原始字节失败：{}", err))?;
    let mut parse_error: Option<String> = None;

    match load_pdf_document_with_fallback(source_path, &file_bytes) {
        Ok(document) => {
            let pages = document.get_pages();
            let page_numbers: Vec<u32> = pages.keys().copied().collect();

            if !page_numbers.is_empty() {
                // 优先使用 lopdf 内置提取；失败时再走操作符降级解析。
                if let Ok(text) = document.extract_text(&page_numbers) {
                    if let Some(normalized) = normalize_extracted_pdf_text(&text) {
                        return Ok(normalized);
                    }
                }

                if let Some(text) = extract_text_from_pdf_fallback_ops(&document, &pages) {
                    return Ok(text);
                }
            }
        }
        Err(err) => {
            parse_error = Some(err);
        }
    }

    // 兼容兜底：尝试使用独立解析实现（pdf-extract）提取文本。
    if let Some(text) = extract_text_from_pdf_with_pdf_extract(&file_bytes) {
        return Ok(text);
    }

    // 当结构化解析失败时，尝试直接扫描 stream 并解压文本内容流。
    if let Some(text) = extract_text_from_pdf_raw_streams(&file_bytes) {
        return Ok(text);
    }

    if let Some(error_message) = parse_error {
        return Err(format!(
            "{}；并且未从原始流中提取到可用文本，可能是扫描件或字体编码不兼容",
            error_message
        ));
    }

    Err("提取 PDF 文本失败：未识别到可用文本，可能是扫描件或字体编码不兼容".to_string())
}

#[derive(Debug)]
struct PdfOcrFallbackOutput {
    text: String,
    page_count: usize,
}

/// 仅在"解析兼容/无文本/扫描件"类场景启用 PDF OCR 回退。
pub(super) fn should_fallback_to_pdf_ocr(error_message: &str) -> bool {
    [
        "解析器暂不兼容",
        "未识别到可用文本",
        "扫描件",
        "字体编码不兼容",
        "未从原始流中提取到可用文本",
    ]
    .iter()
    .any(|keyword| error_message.contains(keyword))
}

/// 拼装 PDF OCR 回退失败文案，统一包含安装指引。
pub(super) fn build_pdf_ocr_fallback_failure_message(parse_error: &str, ocr_error: &str) -> String {
    format!(
        "PDF 文本解析失败：{}；自动 OCR 回退失败：{}。安装指引：请安装 Poppler（确保 `pdftoppm` 可执行并已加入 PATH）；并安装 tesseract 或 paddleocr（至少一种）并加入 PATH。",
        shorten_error_snippet(parse_error.trim(), 160),
        shorten_error_snippet(ocr_error.trim(), 200)
    )
}

fn extract_text_from_pdf_via_ocr(
    source_path: &Path,
    primary_provider: OcrProvider,
) -> Result<PdfOcrFallbackOutput, String> {
    let temp_dir = std::env::temp_dir().join(format!("llm_wiki_pdf_ocr_{}", uuid_v4_short()));
    fs::create_dir_all(&temp_dir).map_err(|err| format!("创建 PDF OCR 临时目录失败：{}", err))?;

    let run_result = (|| {
        let output_prefix = temp_dir.join("page");
        let command_output = run_pdftoppm_png_command(source_path, &output_prefix)?;
        if !command_output.status.success() {
            let stderr = String::from_utf8_lossy(&command_output.stderr);
            let short_reason = shorten_error_snippet(stderr.trim(), 160);
            return Err(format!(
                "pdftoppm 转图失败：{}",
                if short_reason.is_empty() {
                    "请确认 PDF 文件可读且 Poppler 已正确安装"
                } else {
                    short_reason.as_str()
                }
            ));
        }

        let page_images = collect_pdftoppm_generated_pngs(&temp_dir, "page")?;
        let mut page_texts = Vec::new();
        let mut page_errors = Vec::new();

        for (page_number, image_path) in page_images.iter() {
            match extract_text_from_image_with_fallback(image_path, primary_provider) {
                Ok(text) => page_texts.push(format!("[第 {} 页]\n{}", page_number, text)),
                Err(err) => page_errors.push(format!(
                    "第 {} 页：{}",
                    page_number,
                    shorten_error_snippet(err.trim(), 120)
                )),
            }
        }

        if page_texts.is_empty() {
            let mut details = page_errors.join("；");
            if details.is_empty() {
                details = "未提取到任何页面文本".to_string();
            }
            return Err(format!("PDF OCR 未提取到可用文本：{}", details));
        }

        Ok(PdfOcrFallbackOutput {
            text: page_texts.join("\n\n"),
            page_count: page_images.len(),
        })
    })();

    let _ = fs::remove_dir_all(&temp_dir);
    run_result
}

fn run_pdftoppm_png_command(
    source_path: &Path,
    output_prefix: &Path,
) -> Result<std::process::Output, String> {
    std::process::Command::new("pdftoppm")
        .arg("-png")
        .arg("-r")
        .arg("220")
        .arg(source_path)
        .arg(output_prefix)
        .output()
        .map_err(|err| format_pdftoppm_spawn_error(&err))
}

fn format_pdftoppm_spawn_error(err: &io::Error) -> String {
    if err.kind() == io::ErrorKind::NotFound {
        "未检测到 pdftoppm 命令，请先安装 Poppler 并将 pdftoppm 加入 PATH".to_string()
    } else {
        format!(
            "调用 pdftoppm 失败：{}",
            shorten_error_snippet(&err.to_string(), 80)
        )
    }
}

fn collect_pdftoppm_generated_pngs(
    temp_dir: &Path,
    output_prefix: &str,
) -> Result<Vec<(usize, PathBuf)>, String> {
    let mut pages = Vec::new();
    let entries =
        fs::read_dir(temp_dir).map_err(|err| format!("读取 PDF 页图目录失败：{}", err))?;
    let expected_prefix = format!("{}-", output_prefix);

    for entry in entries {
        let entry = entry.map_err(|err| format!("读取 PDF 页图条目失败：{}", err))?;
        let path = entry.path();
        if path.extension().and_then(|v| v.to_str()) != Some("png") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|v| v.to_str()) else {
            continue;
        };
        let Some(page_number_raw) = stem.strip_prefix(&expected_prefix) else {
            continue;
        };
        let Ok(page_number) = page_number_raw.parse::<usize>() else {
            continue;
        };
        pages.push((page_number, path));
    }

    pages.sort_by_key(|(page_number, _)| *page_number);
    if pages.is_empty() {
        return Err("pdftoppm 未生成可识别的 PNG 页面文件".to_string());
    }
    Ok(pages)
}

/// 兼容加载 PDF：标准加载失败后，尝试对二进制做轻量修复再重试。
/// 目的：避免"阅读器可打开但解析器严格失败"的误判。
fn load_pdf_document_with_fallback(
    source_path: &Path,
    file_bytes: &[u8],
) -> Result<lopdf::Document, String> {
    let primary_error = match lopdf::Document::load(source_path) {
        Ok(document) => return Ok(document),
        Err(err) => err,
    };
    let mut attempt_reasons = vec![format!(
        "标准加载失败：{}",
        shorten_error_snippet(&primary_error.to_string(), 80)
    )];

    if let Ok(document) = lopdf::Document::load_mem(file_bytes) {
        return Ok(document);
    }
    attempt_reasons.push("内存直读失败".to_string());

    let pdf_header_offset = find_subsequence(file_bytes, b"%PDF-");
    let pdf_eof_end = rfind_subsequence(file_bytes, b"%%EOF").map(|idx| idx + 5);

    if let Some(start) = pdf_header_offset.filter(|offset| *offset > 0) {
        if let Ok(document) = lopdf::Document::load_mem(&file_bytes[start..]) {
            return Ok(document);
        }
        attempt_reasons.push(format!("头偏移修复失败(start={})", start));
    }

    if let Some(end) = pdf_eof_end.filter(|offset| *offset < file_bytes.len()) {
        if let Ok(document) = lopdf::Document::load_mem(&file_bytes[..end]) {
            return Ok(document);
        }
        attempt_reasons.push(format!("EOF 截断修复失败(end={})", end));
    }

    if let (Some(start), Some(end)) = (pdf_header_offset, pdf_eof_end) {
        if start < end && (start > 0 || end < file_bytes.len()) {
            if let Ok(document) = lopdf::Document::load_mem(&file_bytes[start..end]) {
                return Ok(document);
            }
            attempt_reasons.push(format!("头尾联合修复失败(range={}..{})", start, end));
        }
    }

    Err(format!(
        "读取 PDF 失败：当前解析器暂不兼容该文件结构。可在阅读器中\u{201c}另存为\u{201d}后重试，或转图片后使用 OCR。{}",
        attempt_reasons.join("；")
    ))
}

pub(super) fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(super) fn rfind_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

/// 无法建模 PDF 结构时，直接从原始 stream 中提取可解码文本。
pub(super) fn extract_text_from_pdf_raw_streams(file_bytes: &[u8]) -> Option<String> {
    const STREAM_MARKER: &[u8] = b"stream";
    const ENDSTREAM_MARKER: &[u8] = b"endstream";

    let mut extracted_chunks = Vec::new();
    let mut cursor = 0usize;

    while cursor < file_bytes.len() {
        let Some(stream_rel) = find_subsequence(&file_bytes[cursor..], STREAM_MARKER) else {
            break;
        };
        let stream_marker_index = cursor + stream_rel;
        let mut stream_content_start = stream_marker_index + STREAM_MARKER.len();
        if stream_content_start >= file_bytes.len() {
            break;
        }

        // PDF stream 正文前通常紧跟换行。
        if file_bytes[stream_content_start..].starts_with(b"\r\n") {
            stream_content_start += 2;
        } else if file_bytes[stream_content_start..].starts_with(b"\n")
            || file_bytes[stream_content_start..].starts_with(b"\r")
        {
            stream_content_start += 1;
        }

        let Some(endstream_rel) =
            find_subsequence(&file_bytes[stream_content_start..], ENDSTREAM_MARKER)
        else {
            break;
        };
        let stream_content_end = stream_content_start + endstream_rel;
        if stream_content_end <= stream_content_start {
            cursor = stream_content_end.saturating_add(ENDSTREAM_MARKER.len());
            continue;
        }

        let stream_bytes = &file_bytes[stream_content_start..stream_content_end];
        let candidates = decode_pdf_stream_candidates(stream_bytes);
        for candidate in candidates {
            let Ok(content) = lopdf::content::Content::decode(&candidate) else {
                continue;
            };
            let decoded_text = extract_text_from_pdf_operations(&content.operations);
            let Some(normalized_text) = normalize_extracted_pdf_text(&decoded_text) else {
                continue;
            };
            if is_likely_meaningful_pdf_text(&normalized_text) {
                extracted_chunks.push(normalized_text);
            }
        }

        cursor = stream_content_end + ENDSTREAM_MARKER.len();
    }

    if extracted_chunks.is_empty() {
        None
    } else {
        normalize_extracted_pdf_text(&extracted_chunks.join("\n\n"))
    }
}

/// 为每个 stream 提供原始字节与 Flate 解压字节两个候选。
pub(super) fn decode_pdf_stream_candidates(stream_bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut candidates = Vec::with_capacity(2);
    if !stream_bytes.is_empty() {
        candidates.push(stream_bytes.to_vec());
    }

    let mut decoded = Vec::new();
    let mut decoder = ZlibDecoder::new(stream_bytes);
    if decoder.read_to_end(&mut decoded).is_ok() && !decoded.is_empty() {
        candidates.push(decoded);
    } else {
        let mut trimmed_len = stream_bytes.len();
        while trimmed_len > 0
            && (stream_bytes[trimmed_len - 1] == b'\r' || stream_bytes[trimmed_len - 1] == b'\n')
        {
            trimmed_len -= 1;
        }
        let mut decoded_trimmed = Vec::new();
        let mut decoder_trimmed = ZlibDecoder::new(&stream_bytes[..trimmed_len]);
        if decoder_trimmed.read_to_end(&mut decoded_trimmed).is_ok() && !decoded_trimmed.is_empty()
        {
            candidates.push(decoded_trimmed);
        }
    }

    candidates
}

fn is_likely_meaningful_pdf_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.chars().count() < 12 {
        return false;
    }

    let ascii_letters = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .count();
    let cjk_chars = trimmed
        .chars()
        .filter(|ch| ('\u{4e00}'..='\u{9fff}').contains(ch))
        .count();
    let meaningful_chars = ascii_letters + cjk_chars;
    let total_chars = trimmed.chars().count();

    meaningful_chars * 100 / total_chars >= 18
}

/// 使用 pdf-extract 作为解析器独立兜底，提升异常结构 PDF 兼容性。
fn extract_text_from_pdf_with_pdf_extract(file_bytes: &[u8]) -> Option<String> {
    let extracted = pdf_extract::extract_text_from_mem(file_bytes).ok()?;
    let normalized = normalize_extracted_pdf_text(&extracted)?;
    if is_likely_meaningful_pdf_text(&normalized) {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_extracted_pdf_text(text: &str) -> Option<String> {
    let normalized = text.replace('\u{0}', "").trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn extract_text_from_pdf_fallback_ops(
    document: &lopdf::Document,
    pages: &std::collections::BTreeMap<u32, lopdf::ObjectId>,
) -> Option<String> {
    let mut all_pages_text = Vec::new();

    for page_id in pages.values() {
        let Ok(content_bytes) = document.get_page_content(*page_id) else {
            continue;
        };
        let Ok(content) = lopdf::content::Content::decode(&content_bytes) else {
            continue;
        };
        let page_text = extract_text_from_pdf_operations(&content.operations);
        if let Some(normalized) = normalize_extracted_pdf_text(&page_text) {
            all_pages_text.push(normalized);
        }
    }

    if all_pages_text.is_empty() {
        None
    } else {
        normalize_extracted_pdf_text(&all_pages_text.join("\n\n"))
    }
}

pub(super) fn extract_text_from_pdf_operations(operations: &[lopdf::content::Operation]) -> String {
    let mut extracted = String::new();

    for operation in operations {
        match operation.operator.as_str() {
            "Tj" | "'" => {
                if let Some(value) = operation.operands.first() {
                    append_pdf_text_object(value, &mut extracted);
                    extracted.push('\n');
                }
            }
            "\"" => {
                if let Some(value) = operation.operands.last() {
                    append_pdf_text_object(value, &mut extracted);
                    extracted.push('\n');
                }
            }
            "TJ" => {
                if let Some(lopdf::Object::Array(items)) = operation.operands.first() {
                    for item in items {
                        append_pdf_text_object(item, &mut extracted);
                    }
                    extracted.push('\n');
                }
            }
            "T*" => extracted.push('\n'),
            _ => {}
        }
    }

    extracted
}

fn append_pdf_text_object(value: &lopdf::Object, output: &mut String) {
    if let lopdf::Object::String(bytes, _) = value {
        output.push_str(&String::from_utf8_lossy(bytes));
    }
}

/// 生成基于微秒时间戳的短唯一字符串，用于临时文件命名。
pub(super) fn uuid_v4_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros().to_string())
        .unwrap_or_else(|_| "tmp".to_string())
}

// ─── Local copies of private state.rs helpers ─────────────────────────────────
// tokenize_query, extract_wiki_display_name and their dependencies are private
// free functions in state.rs; we duplicate them here so this submodule compiles
// without requiring visibility changes to the parent module.

fn extract_wiki_display_name(display_source_path: &str) -> Option<String> {
    std::path::Path::new(display_source_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with("llm_wiki_"))
        .map(str::to_string)
}

fn tokenize_query(question: &str) -> Vec<String> {
    // 轻量混合分词：保留英文 token，同时为连续中文片段生成 2-gram，提升中文命中率。
    let mut tokens = question
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    for segment in extract_cjk_segments(question) {
        let chars = segment.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            continue;
        }
        tokens.push(segment.clone());
        if chars.len() >= 2 {
            for window in chars.windows(2) {
                let gram = window.iter().collect::<String>();
                tokens.push(gram);
            }
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
        .into_iter()
        .filter(|token| !is_stopword(token))
        .collect()
}

fn extract_cjk_segments(input: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if is_cjk(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            segments.push(current.clone());
            current.clear();
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF // CJK Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
    )
}

fn is_stopword(token: &str) -> bool {
    const ZH_STOPWORDS: &[&str] = &[
        "的", "了", "是", "吗", "呢", "和", "与", "及", "在", "对", "把", "将",
    ];
    const EN_STOPWORDS: &[&str] = &["the", "is", "are", "a", "an", "of", "to", "for"];

    ZH_STOPWORDS.contains(&token) || EN_STOPWORDS.contains(&token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, state::test_helpers::*};
    use std::{
        fs,
        io,
        path::{Path, PathBuf},
    };

    // ── validate_pdf_source_path ──────────────────────────────────────────────

    #[test]
    fn validate_pdf_source_path_rejects_missing_file() {
        let missing = std::env::temp_dir().join(format!("missing-{}.pdf", uuid_v4_short()));
        let err = validate_pdf_source_path(&missing).expect_err("缺失文件应报错");
        assert!(err.contains("PDF 文件不存在"));
    }

    #[test]
    fn validate_pdf_source_path_rejects_non_pdf_extension() {
        let dir = make_temp_dir("llm-wiki-pdf-validate-ext");
        let _guard = TempDirGuard(dir.clone());
        let txt_path = dir.join("note.txt");
        fs::write(&txt_path, "plain text").expect("写入测试文件失败");
        let err = validate_pdf_source_path(&txt_path).expect_err("非 PDF 扩展名应报错");
        assert!(err.contains("仅支持 .pdf"));
    }

    #[test]
    fn validate_pdf_source_path_rejects_directory() {
        let dir = make_temp_dir("llm-wiki-pdf-validate-dir");
        let _guard = TempDirGuard(dir.clone());
        let err = validate_pdf_source_path(&dir).expect_err("目录路径应报错");
        assert!(err.contains("不是文件"));
    }

    #[test]
    fn validate_pdf_source_path_accepts_uppercase_pdf_extension() {
        let dir = make_temp_dir("llm-wiki-pdf-validate-uppercase");
        let _guard = TempDirGuard(dir.clone());
        let upper_pdf_path = dir.join("note.PDF");
        fs::write(&upper_pdf_path, "placeholder").expect("写入测试文件失败");
        let result = validate_pdf_source_path(&upper_pdf_path);
        assert!(result.is_ok(), "大写扩展名 .PDF 应通过校验");
    }

    // ── OcrProvider ──────────────────────────────────────────────────────────

    #[test]
    fn normalize_ocr_provider_falls_back_to_tesseract_on_invalid_value() {
        assert_eq!(normalize_ocr_provider(None), OcrProvider::Tesseract);
        assert_eq!(normalize_ocr_provider(Some("")), OcrProvider::Tesseract);
        assert_eq!(normalize_ocr_provider(Some("invalid-provider")), OcrProvider::Tesseract);
        assert_eq!(normalize_ocr_provider(Some("paddle")), OcrProvider::Paddle);
        assert_eq!(normalize_ocr_provider(Some(" TESSERACT ")), OcrProvider::Tesseract);
    }

    #[test]
    fn resolve_ocr_provider_order_matches_expected_fallback_sequence() {
        assert_eq!(
            resolve_ocr_provider_order(OcrProvider::Tesseract),
            [OcrProvider::Tesseract, OcrProvider::Paddle]
        );
        assert_eq!(
            resolve_ocr_provider_order(OcrProvider::Paddle),
            [OcrProvider::Paddle, OcrProvider::Tesseract]
        );
    }

    // ── XML extraction ────────────────────────────────────────────────────────

    #[test]
    fn extract_xml_text_by_tag_reads_docx_minimal_sample() {
        let xml = r#"
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>你好&amp;Rust</w:t></w:r></w:p>
    <w:p><w:r><w:t xml:space="preserve">  Wiki  </w:t></w:r></w:p>
  </w:body>
</w:document>
"#;
        let text = extract_xml_text_by_tag(xml, "w:t");
        assert!(text.contains("你好&Rust"));
        assert!(text.contains("Wiki"));
    }

    #[test]
    fn extract_xml_text_by_tag_reads_pptx_minimal_sample() {
        let xml = r#"
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld>
    <a:t>Hello</a:t>
    <a:t>Slide&#32;1</a:t>
  </p:cSld>
</p:sld>
"#;
        let text = extract_xml_text_by_tag(xml, "a:t");
        assert!(text.contains("Hello"));
        assert!(text.contains("Slide 1"));
    }

    #[test]
    fn test_extract_slide_number_natural_sort() {
        assert_eq!(extract_slide_number("ppt/slides/slide1.xml"), 1);
        assert_eq!(extract_slide_number("ppt/slides/slide10.xml"), 10);
        assert_eq!(extract_slide_number("ppt/slides/slide2.xml"), 2);
        assert!(
            extract_slide_number("ppt/slides/slide10.xml")
                > extract_slide_number("ppt/slides/slide2.xml")
        );
    }

    #[test]
    fn test_extract_docx_paragraphs_preserves_paragraph_breaks() {
        let xml = r#"<w:body>
        <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
        <w:p><w:r><w:t>第二段</w:t></w:r><w:r><w:t>续文</w:t></w:r></w:p>
        <w:p></w:p>
    </w:body>"#;
        let result = extract_docx_paragraphs(xml);
        assert!(result.contains("第一段"));
        assert!(result.contains("第二段"));
        assert!(result.contains("\n\n") || result.lines().count() >= 2);
    }

    // ── Tesseract error formatting ────────────────────────────────────────────

    #[test]
    fn format_tesseract_spawn_error_returns_readable_message_when_missing() {
        let error = io::Error::new(io::ErrorKind::NotFound, "not found");
        let message = format_tesseract_spawn_error(&error);
        assert!(message.contains("未检测到 tesseract"));
        assert!(message.contains("PATH"));
    }

    // ── PDF text extraction ───────────────────────────────────────────────────

    #[test]
    fn extract_text_from_pdf_operations_extracts_simple_text() {
        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new(
                    "Tj",
                    vec![lopdf::Object::String(
                        b"Hello PDF".to_vec(),
                        lopdf::StringFormat::Literal,
                    )],
                ),
                lopdf::content::Operation::new(
                    "TJ",
                    vec![lopdf::Object::Array(vec![
                        lopdf::Object::String(b"Fallback ".to_vec(), lopdf::StringFormat::Literal),
                        lopdf::Object::Integer(-120),
                        lopdf::Object::String(b"Works".to_vec(), lopdf::StringFormat::Literal),
                    ])],
                ),
                lopdf::content::Operation::new("ET", vec![]),
            ],
        };
        let encoded = content.encode().expect("编码 PDF 操作失败");
        let decoded = lopdf::content::Content::decode(&encoded).expect("解码 PDF 操作失败");
        let extracted = extract_text_from_pdf_operations(&decoded.operations);
        assert!(extracted.contains("Hello PDF"));
        assert!(extracted.contains("Fallback Works"));
    }

    #[test]
    fn extract_text_from_pdf_raw_streams_extracts_text_from_flate_stream() {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        let content = lopdf::content::Content {
            operations: vec![
                lopdf::content::Operation::new("BT", vec![]),
                lopdf::content::Operation::new(
                    "Tj",
                    vec![lopdf::Object::String(
                        b"Gradient Tensor".to_vec(),
                        lopdf::StringFormat::Literal,
                    )],
                ),
                lopdf::content::Operation::new("ET", vec![]),
            ],
        };
        let encoded = content.encode().expect("编码内容流失败");
        let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
        compressor.write_all(&encoded).expect("压缩内容流失败");
        let compressed = compressor.finish().expect("完成压缩失败");

        let mut pseudo_pdf = Vec::new();
        pseudo_pdf.extend_from_slice(b"%PDF-1.4\n1 0 obj\n<< /Length ");
        pseudo_pdf.extend_from_slice(compressed.len().to_string().as_bytes());
        pseudo_pdf.extend_from_slice(b" /Filter /FlateDecode >>\nstream\n");
        pseudo_pdf.extend_from_slice(&compressed);
        pseudo_pdf.extend_from_slice(b"\nendstream\n%%EOF");

        let extracted =
            extract_text_from_pdf_raw_streams(&pseudo_pdf).expect("应能从 Flate stream 提取文本");
        assert!(extracted.contains("Gradient Tensor"));
    }

    #[test]
    fn decode_pdf_stream_candidates_supports_trailing_newline() {
        use flate2::{write::ZlibEncoder, Compression};
        use std::io::Write;

        let payload = b"BT\n(Hello Stream)\nTj\nET";
        let mut compressor = ZlibEncoder::new(Vec::new(), Compression::default());
        compressor.write_all(payload).expect("压缩 payload 失败");
        let mut compressed = compressor.finish().expect("完成压缩失败");
        compressed.extend_from_slice(b"\r\n");

        let candidates = decode_pdf_stream_candidates(&compressed);
        assert!(candidates.iter().any(|candidate| {
            let text = String::from_utf8_lossy(candidate);
            text.contains("Hello Stream")
        }));
    }

    #[test]
    fn should_fallback_to_pdf_ocr_matches_supported_error_patterns() {
        assert!(should_fallback_to_pdf_ocr(
            "提取 PDF 文本失败：未识别到可用文本，可能是扫描件或字体编码不兼容"
        ));
        assert!(should_fallback_to_pdf_ocr(
            "读取 PDF 失败：当前解析器暂不兼容该文件结构"
        ));
        assert!(!should_fallback_to_pdf_ocr(
            "读取 PDF 原始字节失败：permission denied"
        ));
    }

    #[test]
    fn build_pdf_ocr_fallback_failure_message_contains_install_hints() {
        let message = build_pdf_ocr_fallback_failure_message(
            "读取 PDF 失败：当前解析器暂不兼容该文件结构",
            "未检测到 pdftoppm 命令",
        );
        assert!(message.contains("自动 OCR 回退失败"));
        assert!(message.contains("Poppler"));
        assert!(message.contains("pdftoppm"));
        assert!(message.contains("tesseract"));
        assert!(message.contains("paddleocr"));
    }

    #[test]
    fn find_subsequence_returns_expected_offsets() {
        let bytes = b"abc%PDF-1.4...%%EOFtail";
        assert_eq!(find_subsequence(bytes, b"%PDF-"), Some(3));
        assert_eq!(rfind_subsequence(bytes, b"%%EOF"), Some(14));
        assert_eq!(find_subsequence(bytes, b"not-found"), None);
    }

    // ── AppState integration tests ────────────────────────────────────────────

    #[tokio::test]
    async fn ingest_file_impl_rejects_unsupported_extension() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-file-unsupported");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let unsupported = vault_dir.join("sample.xyz");
        fs::write(&unsupported, "unsupported").expect("写入测试文件失败");

        let err = state
            .ingest_file_impl(unsupported.to_string_lossy().as_ref(), None)
            .await
            .expect_err("不支持扩展名应返回错误");
        assert!(err.contains("不支持的文件扩展名"));
        assert!(err.contains("md/markdown/pdf/docx/pptx/txt"));
    }

    #[tokio::test]
    async fn apply_ingest_preview_returns_error_when_preview_id_missing() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-preview-missing");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let err = state
            .apply_ingest_preview("preview-not-exists")
            .await
            .expect_err("不存在的 preview_id 应返回错误");
        assert!(err.contains("未找到预览缓存"));
    }

    #[tokio::test]
    async fn preview_ingest_file_then_apply_succeeds_and_consumes_cache() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-preview-apply");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let source_path = vault_dir.join("preview-source.md");
        fs::write(&source_path, "# Preview Source\n\nRust ingest preview")
            .expect("写入预览源文件失败");

        let preview = state
            .preview_ingest_file("file", source_path.to_string_lossy().as_ref(), None)
            .await
            .expect("生成 ingest preview 失败");
        assert!(!preview.preview_id.trim().is_empty());

        let result = state
            .apply_ingest_preview(&preview.preview_id)
            .await
            .expect("应用 ingest preview 失败");
        assert!(!result.wiki_path.trim().is_empty());
        assert!(
            Path::new(&result.wiki_path).exists(),
            "落盘后 wiki 页面应存在"
        );

        let err = state
            .apply_ingest_preview(&preview.preview_id)
            .await
            .expect_err("同一个 preview_id 第二次 apply 应失败");
        assert!(err.contains("未找到预览缓存"));
    }

    #[tokio::test]
    async fn ingest_pdf_impl_rejects_invalid_pdf_content_with_readable_error() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-pdf-invalid");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let invalid_pdf_path = vault_dir.join("invalid.pdf");
        fs::write(&invalid_pdf_path, "this is not a real pdf").expect("写入伪 PDF 文件失败");

        let err = state
            .ingest_pdf_impl(invalid_pdf_path.to_string_lossy().as_ref())
            .await
            .expect_err("非法 PDF 内容应返回错误");
        assert!(err.contains("读取 PDF 失败"));
        assert!(err.contains("解析器暂不兼容"));
    }

    #[test]
    fn default_paths_point_to_project_root_targets() {
        let vault_dir = make_temp_dir("llm-wiki-default-paths");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));

        let paths = state.default_paths();
        assert_eq!(paths.vault_path, root.join("vault").to_string_lossy());
        #[cfg(windows)]
        assert_eq!(paths.ingest_source_path, r"E:\llm-wiki\test-llm.md");
        #[cfg(not(windows))]
        assert_eq!(
            paths.ingest_source_path,
            root.join("test-llm.md").to_string_lossy()
        );
    }

    #[test]
    fn ingest_queue_stale_running_reset_on_vault_init() {
        let vault_dir = make_temp_dir("llm-wiki-restart-recovery");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init_vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = rusqlite::Connection::open(&db_path).expect("打开数据库失败");

        db::db_enqueue_ingest(&conn, "file", "/some/path.md", "100").expect("enqueue 失败");
        let items = db::db_list_ingest_queue(&conn).expect("list 失败");
        let id = items[0].id;
        let now = crate::state::current_timestamp_ms();
        db::db_update_ingest_queue_status(&conn, id, "running", None, &now)
            .expect("set running 失败");

        state.init_vault(vault_dir.clone()).expect("第二次 init_vault 失败");

        let items = db::db_list_ingest_queue(&conn).expect("list after restart 失败");
        assert_eq!(items[0].status, "queued", "重启后 running 任务应被重置为 queued");
    }

    #[test]
    fn init_vault_with_template_rejects_path_traversal() {
        let dir = make_temp_dir("llm-wiki-state-template-traversal");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        let vault_dir = dir.join("vault");
        fs::create_dir_all(&vault_dir).unwrap();
        let result = state.init_vault_with_template(
            vault_dir,
            "schema content".to_string(),
            "purpose content".to_string(),
            vec!["../../../etc".to_string()],
        );
        assert!(result.is_err(), "路径遍历应被拒绝");
        assert!(result.unwrap_err().contains("非法目录路径"));
    }

    #[test]
    fn init_vault_with_template_rejects_oversized_content() {
        let dir = make_temp_dir("llm-wiki-state-template-size");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        let vault_dir = dir.join("vault");
        fs::create_dir_all(&vault_dir).unwrap();
        let big = "x".repeat(513 * 1024);
        let result = state.init_vault_with_template(vault_dir, big, "ok".to_string(), vec![]);
        assert!(result.is_err(), "超大 schema 内容应被拒绝");
        assert!(result.unwrap_err().contains("512 KB"));
    }
}
