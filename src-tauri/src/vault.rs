use std::{
    fs,
    io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    db::{self, IngestTaskInput},
    models::{
        AppConfig, AppMode, IngestResult, QueryCitation, SaveQueryAnswerInput,
        SaveQueryAnswerResult, VaultInitResult,
    },
};

const INDEX_SEED: &str = "# Index\n\n## Imported Pages\n";
const LOG_SEED: &str = "# Log\n\n## Event Log\n";

/// 初始化 Vault 目录与数据库。
pub fn initialize_vault(vault_path: &Path, mode: AppMode) -> Result<VaultInitResult, String> {
    let mut created_paths = Vec::new();

    ensure_directory(vault_path, &mut created_paths)?;

    let raw_dir = vault_path.join("raw");
    ensure_directory(&raw_dir, &mut created_paths)?;

    let wiki_dir = vault_path.join("wiki");
    ensure_directory(&wiki_dir, &mut created_paths)?;

    let app_dir = vault_path.join(".app");
    ensure_directory(&app_dir, &mut created_paths)?;

    let index_path = vault_path.join("index.md");
    create_file_if_missing(&index_path, INDEX_SEED, &mut created_paths)?;

    let log_path = vault_path.join("log.md");
    create_file_if_missing(&log_path, LOG_SEED, &mut created_paths)?;

    let config_path = app_dir.join("config.json");
    let config_content = serde_json::to_string_pretty(&AppConfig {
        mode,
        vault_path: Some(vault_path.to_string_lossy().to_string()),
        query_top_k: Some(3),
        cloud_api_key: None,
        cloud_base_url: None,
        cloud_model: None,
        cloud_provider_name: None,
        active_provider: None,
    })
    .map_err(|err| format!("序列化 Vault 配置失败: {}", err))?;
    create_file_if_missing(&config_path, &config_content, &mut created_paths)?;

    let db_path = app_dir.join("meta.db");
    let db_existed = db_path.exists();
    db::ensure_meta_db(&db_path)?;
    if !db_existed {
        created_paths.push(db_path.to_string_lossy().to_string());
    }

    Ok(VaultInitResult {
        vault_path: vault_path.to_string_lossy().to_string(),
        created_paths,
        message: "Vault 初始化完成".to_string(),
    })
}

/// 导入 Markdown 文件到 Vault。
///
/// # 参数
/// - `vault_path`: Vault 根目录路径
/// - `source_path`: 源 Markdown 文件路径
/// - `llm_summary`: LLM 生成的摘要（可选），如果为 None 则使用截断摘要
pub fn ingest_markdown(
    vault_path: &Path,
    source_path: &Path,
    llm_summary: Option<&str>,
) -> Result<IngestResult, String> {
    if !vault_path.exists() {
        return Err("Vault 不存在，请先执行 init_vault".to_string());
    }

    let source_content = fs::read_to_string(source_path)
        .map_err(|err| format!("读取源 Markdown 失败: {}", err))?;

    let content_hash = stable_hash_hex(source_content.as_bytes());
    let timestamp_ms = current_timestamp_ms();
    let timestamp_ns = current_timestamp_ns();
    let source_stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("source");
    let raw_file_name = normalize_raw_filename(source_stem, &content_hash);
    let raw_path = vault_path.join("raw").join(&raw_file_name);
    let wiki_file_name = format!("ingest-{}.md", timestamp_ns);
    let wiki_path = vault_path.join("wiki").join(&wiki_file_name);
    let wiki_title = wiki_file_name.trim_end_matches(".md").to_string();
    // 优先使用 LLM 摘要，否则回退到截断摘要
    let summary = llm_summary
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback_summarize(&source_content, 200));
    let wiki_body = build_wiki_summary(
        &wiki_title,
        source_path,
        &raw_path,
        &summary,
        &timestamp_ms,
    );

    let db_path = vault_path.join(".app").join("meta.db");
    db::ensure_meta_db(&db_path)?;
    if let Some(existing) = db::find_existing_ingest_by_hash(&db_path, &content_hash)? {
        let existing_wiki_path = PathBuf::from(&existing.wiki_path);
        if existing_wiki_path.exists() {
            return Ok(IngestResult {
                source_path: source_path.to_string_lossy().to_string(),
                raw_path: existing.raw_path,
                wiki_path: existing.wiki_path,
                message: "检测到重复内容，已复用既有 Wiki 页面".to_string(),
                entities: Vec::new(),
                updated_pages: Vec::new(),
            });
        }
    }
    let task_input = IngestTaskInput {
        source_path,
        raw_path: &raw_path,
        wiki_path: &wiki_path,
        content_hash: &content_hash,
        title: &wiki_title,
        timestamp_ms: &timestamp_ms,
    };
    let task = db::begin_ingest_task(&db_path, &task_input)?;

    if let Err(err) = write_or_verify_same(&raw_path, &source_content) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }
    if let Err(err) = db::append_task_event(
        &db_path,
        task.task_id,
        "raw_copied",
        "原始 Markdown 已复制到 raw 目录",
        &timestamp_ms,
    ) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }

    if let Err(err) = write_or_verify_same(&wiki_path, &wiki_body) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }
    if let Err(err) = db::append_task_event(
        &db_path,
        task.task_id,
        "wiki_generated",
        "摘要页已生成",
        &timestamp_ms,
    ) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }

    let index_path = vault_path.join("index.md");
    // index 中使用较短的摘要（截取前 80 字符）
    let short_summary = llm_summary
        .map(|s| fallback_summarize(s, 80))
        .unwrap_or_else(|| fallback_summarize(&source_content, 80));
    let index_entry = format!(
        "- [[wiki/{}|{}]]\n  - Source file: `{}`\n  - Summary: {}\n",
        wiki_file_name,
        wiki_title,
        raw_file_name,
        short_summary
    );
    if let Err(err) = append_markdown_entry(&index_path, INDEX_SEED, &index_entry) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }

    let log_path = vault_path.join("log.md");
    let log_entry = format!(
        "## {}\n- Event: Markdown ingest\n- Source: `{}`\n- Raw: `{}`\n- Wiki: `{}`\n- Status: Success\n",
        timestamp_ms,
        source_path.to_string_lossy(),
        raw_path.to_string_lossy(),
        wiki_path.to_string_lossy()
    );
    if let Err(err) = append_markdown_entry(&log_path, LOG_SEED, &log_entry) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }
    if let Err(err) = db::append_task_event(
        &db_path,
        task.task_id,
        "docs_synced",
        "index.md and log.md updated",
        &timestamp_ms,
    ) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }

    if let Err(err) = db::record_wiki_page(
        &db_path,
        task.source_id,
        &wiki_title,
        &wiki_path,
        &summary,
        &timestamp_ms,
    ) {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }

    // FTS 属于检索增强能力，失败时保留主流程成功并记录告警事件。
    if let Err(err) = db::upsert_fts_page(&db_path, &wiki_path, &wiki_title, &wiki_body) {
        let _ = db::append_task_event(
            &db_path,
            task.task_id,
            "fts_warning",
            &format!("fts 索引更新失败，将降级为文件扫描检索: {}", err),
            &timestamp_ms,
        );
    }

    if let Err(err) = db::update_task_status(&db_path, task.task_id, "applied", None, &timestamp_ms)
    {
        finalize_failed_task(&db_path, task.task_id, &timestamp_ms, &err);
        return Err(err);
    }

    let message = format!(
        "已导入 {}，raw={}，wiki={}",
        source_path.to_string_lossy(),
        raw_path.to_string_lossy(),
        wiki_path.to_string_lossy()
    );

    Ok(IngestResult {
        source_path: source_path.to_string_lossy().to_string(),
        raw_path: raw_path.to_string_lossy().to_string(),
        wiki_path: wiki_path.to_string_lossy().to_string(),
        message,
        entities: Vec::new(),
        updated_pages: Vec::new(),
    })
}

/// 将 Query 结果保存为 wiki 页面，并同步 index/log/SQLite。
pub fn save_query_answer(
    vault_path: &Path,
    input: &SaveQueryAnswerInput,
) -> Result<SaveQueryAnswerResult, String> {
    if !vault_path.exists() {
        return Err("Vault 不存在，请先执行 init_vault".to_string());
    }

    let question = input.question.trim();
    if question.is_empty() {
        return Err("问题不能为空".to_string());
    }

    let answer = input.answer.trim();
    if answer.is_empty() {
        return Err("回答不能为空".to_string());
    }

    let timestamp_ms = current_timestamp_ms();
    let timestamp_ns = current_timestamp_ns();
    let page_title = build_query_page_title(input.title.as_deref(), question);
    let wiki_file_name = format!("query-{}.md", timestamp_ns);
    let wiki_path = vault_path.join("wiki").join(&wiki_file_name);
    let body = build_query_wiki_body(&page_title, question, answer, &timestamp_ms, &input.citations);
    write_or_verify_same(&wiki_path, &body)?;

    let index_path = vault_path.join("index.md");
    let index_entry = format!(
        "- [[wiki/{}|{}]]\n  - Source file: `query://{}`\n  - Summary: {}\n",
        wiki_file_name,
        page_title,
        timestamp_ms,
        fallback_summarize(answer, 80)
    );
    append_markdown_entry(&index_path, INDEX_SEED, &index_entry)?;

    let log_path = vault_path.join("log.md");
    let log_entry = format!(
        "## {}\n- Event: Query answer saved\n- Question: `{}`\n- Wiki: `{}`\n- Status: Success\n",
        timestamp_ms,
        question,
        wiki_path.to_string_lossy()
    );
    append_markdown_entry(&log_path, LOG_SEED, &log_entry)?;

    let db_path = vault_path.join(".app").join("meta.db");
    db::ensure_meta_db(&db_path)?;
    let content_hash = stable_hash_hex(body.as_bytes());
    db::upsert_generated_wiki_page(
        &db_path,
        &page_title,
        &wiki_path,
        &fallback_summarize(answer, 200),
        &content_hash,
        &timestamp_ms,
    )?;
    let citation_inputs = input
        .citations
        .iter()
        .map(|citation| db::CitationInput {
            cited_page_path: citation.page_path.as_str(),
            score: citation.score,
            excerpt: citation.excerpt.as_str(),
        })
        .collect::<Vec<_>>();
    db::replace_citations_for_page(&db_path, &wiki_path, &citation_inputs, &timestamp_ms)?;
    db::upsert_fts_page(&db_path, &wiki_path, &page_title, &body)?;

    Ok(SaveQueryAnswerResult {
        wiki_path: wiki_path.to_string_lossy().to_string(),
        page_title,
        message: "Query 结果已保存到 Wiki".to_string(),
    })
}

/// 在指定 Wiki 页面末尾追加 See Also 链接（幂等：链接已存在则跳过）。
///
/// # 参数
/// - `page_path`: 目标页面绝对路径
/// - `link_target`: 链接目标（相对于 vault 根，如 `wiki/ingest-123.md`）
/// - `link_title`: 链接显示标题
///
/// # 返回
/// - `Ok(true)`: 已成功追加
/// - `Ok(false)`: 链接已存在，跳过
pub fn append_see_also_link(
    page_path: &Path,
    link_target: &str,
    link_title: &str,
) -> Result<bool, String> {
    let content = fs::read_to_string(page_path)
        .map_err(|err| format!("读取页面失败 {}: {}", page_path.to_string_lossy(), err))?;

    // 幂等检查：链接目标已在内容中则跳过
    if content.contains(link_target) {
        return Ok(false);
    }

    let link_line = format!("- [[{}|{}]]", link_target, link_title);

    let new_content = if content.contains("## See Also") {
        // 追加到已有 See Also 节末尾
        format!("{}\n{}", content.trim_end(), link_line)
    } else {
        // 新建 See Also 节
        format!("{}\n\n## See Also\n{}\n", content.trim_end(), link_line)
    };

    fs::write(page_path, &new_content)
        .map_err(|err| format!("写入页面失败 {}: {}", page_path.to_string_lossy(), err))?;

    Ok(true)
}

fn finalize_failed_task(db_path: &Path, task_id: i64, timestamp_ms: &str, error: &str) {
    let _ = db::append_task_event(db_path, task_id, "failed", error, timestamp_ms);
    let _ = db::update_task_status(db_path, task_id, "failed", Some(error), timestamp_ms);
}

fn build_wiki_summary(
    wiki_title: &str,
    source_path: &Path,
    raw_path: &Path,
    summary: &str,
    timestamp_ms: &str,
) -> String {
    format!(
        "# {}\n\n- Source: `{}`\n- Raw: `{}`\n- Imported at: {}\n\n## Summary\n\n{}\n",
        wiki_title,
        source_path.to_string_lossy(),
        raw_path.to_string_lossy(),
        timestamp_ms,
        summary
    )
}

fn build_query_page_title(input_title: Option<&str>, question: &str) -> String {
    if let Some(value) = input_title {
        let title = value.trim();
        if !title.is_empty() {
            return title.to_string();
        }
    }

    let mut title = String::from("问答-");
    title.push_str(&fallback_summarize(question, 24));
    title
}

fn build_query_wiki_body(
    page_title: &str,
    question: &str,
    answer: &str,
    timestamp_ms: &str,
    citations: &[QueryCitation],
) -> String {
    let mut body = String::new();
    body.push_str("# ");
    body.push_str(page_title);
    body.push_str("\n\n");
    body.push_str("- Question: ");
    body.push_str(question);
    body.push('\n');
    body.push_str("- Saved at: ");
    body.push_str(timestamp_ms);
    body.push_str("\n\n## Answer\n\n");
    body.push_str(answer);
    body.push_str("\n\n## Citations\n\n");

    if citations.is_empty() {
        body.push_str("- (none)\n");
        return body;
    }

    for citation in citations {
        body.push_str("- `");
        body.push_str(&citation.page_path);
        body.push_str("` (score: ");
        body.push_str(&citation.score.to_string());
        body.push_str(")\n");
        if !citation.excerpt.trim().is_empty() {
            body.push_str("  - ");
            body.push_str(&trim_excerpt(citation.excerpt.trim(), 160));
            body.push('\n');
        }
    }

    body
}

fn normalize_raw_filename(source_stem: &str, content_hash: &str) -> String {
    let mut cleaned = String::new();

    for ch in source_stem.chars() {
        let mapped = match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c if c.is_whitespace() => '_',
            c => c,
        };
        cleaned.push(mapped);
    }

    let cleaned = cleaned.trim_matches('_');
    let cleaned = if cleaned.is_empty() { "source" } else { cleaned };
    let hash_prefix_len = content_hash.len().min(8);
    let hash_prefix = &content_hash[..hash_prefix_len];
    format!("{}-{}.md", cleaned, hash_prefix)
}

/// 截断文本生成摘要（回退方案）
///
/// 当 LLM 不可用时，简单截断原始文本作为摘要。
pub fn fallback_summarize(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

fn trim_excerpt(input: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for ch in input.chars().take(max_chars) {
        output.push(ch);
    }
    if input.chars().count() > max_chars {
        output.push('…');
    }
    output
}

fn stable_hash_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

fn ensure_directory(path: &Path, created_paths: &mut Vec<String>) -> Result<(), String> {
    if path.exists() {
        if !path.is_dir() {
            return Err(format!("路径已存在但不是目录: {}", path.to_string_lossy()));
        }
        return Ok(());
    }

    fs::create_dir_all(path).map_err(|err| format!("创建目录失败: {}", err))?;
    created_paths.push(path.to_string_lossy().to_string());
    Ok(())
}

fn create_file_if_missing(
    path: &Path,
    content: &str,
    created_paths: &mut Vec<String>,
) -> Result<(), String> {
    if path.exists() {
        if path.is_dir() {
            return Err(format!("路径已存在但不是文件: {}", path.to_string_lossy()));
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建父目录失败: {}", err))?;
    }

    let current = read_snapshot(path)?;
    if current.is_some() {
        return Err(format!("文件已被外部创建: {}", path.to_string_lossy()));
    }

    fs::write(path, content).map_err(|err| format!("写入文件失败: {}", err))?;
    created_paths.push(path.to_string_lossy().to_string());
    Ok(())
}

fn write_or_verify_same(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建父目录失败: {}", err))?;
    }

    let snapshot = read_snapshot(path)?;
    match snapshot.as_deref() {
        Some(existing) => {
            if existing != content {
                Err(format!("文件已存在且内容不同: {}", path.to_string_lossy()))
            } else {
                Ok(())
            }
        }
        None => {
            let current = read_snapshot(path)?;
            if current.is_some() {
                return Err(format!("文件已被外部创建: {}", path.to_string_lossy()));
            }
            fs::write(path, content).map_err(|err| format!("写入文件失败: {}", err))
        }
    }
}

fn append_markdown_entry(path: &Path, base_content: &str, addition: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| format!("创建父目录失败: {}", err))?;
    }

    let snapshot = read_snapshot(path)?;
    let current = snapshot
        .as_deref()
        .unwrap_or(base_content)
        .trim_end()
        .to_string();
    let mut next = String::new();
    if !current.is_empty() {
        next.push_str(&current);
        next.push_str("\n\n");
    }
    next.push_str(addition.trim_start());
    if !next.ends_with('\n') {
        next.push('\n');
    }

    let current_now = read_snapshot(path)?;
    if current_now != snapshot {
        return Err(format!("文件已被外部修改: {}", path.to_string_lossy()));
    }

    fs::write(path, next).map_err(|err| format!("写入文件失败: {}", err))
}

fn read_snapshot(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(format!("读取文件失败: {}", err)),
    }
}

fn current_timestamp_ms() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn current_timestamp_ns() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    #[test]
    fn initialize_vault_creates_expected_files_and_db() {
        let vault_dir = make_temp_dir("llm-wiki-init");
        let _guard = TempDirGuard(vault_dir.clone());

        let result = initialize_vault(&vault_dir, AppMode::Hybrid).expect("初始化 Vault 失败");

        assert_eq!(result.vault_path, vault_dir.to_string_lossy().to_string());
        assert!(vault_dir.join("raw").is_dir());
        assert!(vault_dir.join("wiki").is_dir());
        assert!(vault_dir.join(".app").is_dir());
        assert!(vault_dir.join("index.md").is_file());
        assert!(vault_dir.join("log.md").is_file());
        assert!(vault_dir.join(".app").join("config.json").is_file());
        assert!(vault_dir.join(".app").join("meta.db").is_file());

        let index_content = fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        let log_content = fs::read_to_string(vault_dir.join("log.md")).expect("读取 log.md 失败");
        assert!(index_content.contains("## Imported Pages"));
        assert!(log_content.contains("## Event Log"));
    }

    #[test]
    fn ingest_markdown_writes_raw_wiki_index_log_and_db_records() {
        let vault_dir = make_temp_dir("llm-wiki-ingest");
        let _guard = TempDirGuard(vault_dir.clone());
        initialize_vault(&vault_dir, AppMode::Hybrid).expect("初始化 Vault 失败");

        let source_path = vault_dir.join("source.md");
        let source_content = "# Source Title\n\nA short note for ingest.";
        fs::write(&source_path, source_content).expect("写入源文件失败");

        let result = ingest_markdown(&vault_dir, &source_path, None).expect("导入 Markdown 失败");

        assert!(Path::new(&result.raw_path).is_file());
        assert!(Path::new(&result.wiki_path).is_file());

        let raw_content = fs::read_to_string(&result.raw_path).expect("读取 raw 文件失败");
        let wiki_content = fs::read_to_string(&result.wiki_path).expect("读取 wiki 文件失败");
        let index_content = fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        let log_content = fs::read_to_string(vault_dir.join("log.md")).expect("读取 log.md 失败");

        assert_eq!(raw_content, source_content);
        assert!(wiki_content.contains("## Summary"));
        assert!(index_content.contains("Source file:"));
        assert!(log_content.contains("Event: Markdown ingest"));

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        let source_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))
            .expect("查询 sources 失败");
        let task_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .expect("查询 tasks 失败");
        let wiki_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiki_pages", [], |row| row.get(0))
            .expect("查询 wiki_pages 失败");
        let status: String = conn
            .query_row("SELECT status FROM tasks LIMIT 1", [], |row| row.get(0))
            .expect("读取任务状态失败");
        let task_event_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM task_events", [], |row| row.get(0))
            .expect("查询 task_events 失败");

        assert_eq!(source_count, 1);
        assert_eq!(task_count, 1);
        assert_eq!(wiki_count, 1);
        assert_eq!(status, "applied");
        assert!(task_event_count >= 4);
    }

    #[test]
    fn ingest_markdown_deduplicates_same_content_by_hash() {
        let vault_dir = make_temp_dir("llm-wiki-ingest-dedup");
        let _guard = TempDirGuard(vault_dir.clone());
        initialize_vault(&vault_dir, AppMode::Hybrid).expect("初始化 Vault 失败");

        let source_path = vault_dir.join("source.md");
        let source_content = "# Source Title\n\nDuplicate note content.";
        fs::write(&source_path, source_content).expect("写入源文件失败");

        let first = ingest_markdown(&vault_dir, &source_path, None).expect("第一次导入失败");
        let second = ingest_markdown(&vault_dir, &source_path, None).expect("第二次导入失败");

        assert_eq!(first.wiki_path, second.wiki_path);
        assert_eq!(first.raw_path, second.raw_path);
        assert!(second.message.contains("重复内容"));

        let index_content = fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        let link_count = index_content.matches(&format!("[[wiki/{}|", Path::new(&first.wiki_path).file_name().and_then(|x| x.to_str()).unwrap_or_default())).count();
        assert_eq!(link_count, 1);

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        let task_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .expect("查询 tasks 失败");
        let wiki_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiki_pages", [], |row| row.get(0))
            .expect("查询 wiki_pages 失败");
        assert_eq!(task_count, 1);
        assert_eq!(wiki_count, 1);
    }

    #[test]
    fn save_query_answer_writes_wiki_index_log_and_db_records() {
        let vault_dir = make_temp_dir("llm-wiki-save-query");
        let _guard = TempDirGuard(vault_dir.clone());
        initialize_vault(&vault_dir, AppMode::Hybrid).expect("初始化 Vault 失败");

        let input = SaveQueryAnswerInput {
            question: "这个项目的核心目标是什么？".to_string(),
            answer: "目标是构建 Windows 优先的个人 Wiki 桌面应用。".to_string(),
            citations: vec![QueryCitation {
                page_path: vault_dir
                    .join("wiki")
                    .join("ingest-1.md")
                    .to_string_lossy()
                    .to_string(),
                display_path: None,
                score: 3,
                excerpt: "本项目用于实现一个 Windows 优先的个人 Wiki 桌面应用。".to_string(),
            }],
            title: Some("问答-核心目标".to_string()),
        };

        let result = save_query_answer(&vault_dir, &input).expect("保存 Query 结果失败");

        assert!(Path::new(&result.wiki_path).is_file());
        let wiki_content = fs::read_to_string(&result.wiki_path).expect("读取 wiki 文件失败");
        let index_content = fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        let log_content = fs::read_to_string(vault_dir.join("log.md")).expect("读取 log.md 失败");
        assert!(wiki_content.contains("## Answer"));
        assert!(wiki_content.contains("## Citations"));
        assert!(index_content.contains("问答-核心目标"));
        assert!(log_content.contains("Event: Query answer saved"));

        let db_path = vault_dir.join(".app").join("meta.db");
        let page_paths = db::list_wiki_page_paths(&db_path).expect("读取 wiki_pages 失败");
        assert!(page_paths.iter().any(|path| path == &result.wiki_path));
        let citations = db::list_citations(&db_path).expect("读取 citations 失败");
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].page_path, result.wiki_path);
    }
}
