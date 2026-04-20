use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::models::LintPatchEventItem;
/// 已执行过初始化（init_schema）的数据库路径缓存。
static INITIALIZED_DBS: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

/// 检查并标记数据库是否已初始化，用于在进程生命周期内跳过冗余 init_schema 调用。
fn is_db_initialized(path: &Path) -> bool {
    let mutex = INITIALIZED_DBS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = mutex.lock().unwrap();
    if set.contains(path) {
        true
    } else {
        set.insert(path.to_path_buf());
        false
    }
}

/// 数据库连接池（每个路径一个单例连接，由 Mutex 保护）。
static CONNECTION_POOL: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<Connection>>>>> = OnceLock::new();

/// 获取数据库连接（带缓存与初始化逻辑）。
fn get_connection(db_path: &Path) -> Result<Arc<Mutex<Connection>>, String> {
    let pool_mutex = CONNECTION_POOL.get_or_init(|| Mutex::new(HashMap::new()));
    let mut pool = pool_mutex.lock().unwrap();

    if let Some(conn) = pool.get(db_path) {
        return Ok(Arc::clone(conn));
    }

    // 缓存未命中，创建新连接并初始化（如果是第一次）。
    let conn = open_connection_internal(db_path)?;
    if !is_db_initialized(db_path) {
        init_schema(&conn)?;
    }

    let shared_conn = Arc::new(Mutex::new(conn));
    pool.insert(db_path.to_path_buf(), Arc::clone(&shared_conn));
    Ok(shared_conn)
}

fn open_connection_internal(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("创建数据库目录失败: {}", err))?;
    }

    let conn = Connection::open(db_path).map_err(|err| format!("打开数据库失败: {}", err))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|err| format!("启用外键失败: {}", err))?;
    Ok(conn)
}

/// 统计待处理任务数量。

/// Ask 历史最大保留条数，防止无限增长。
pub const ASK_HISTORY_MAX_ENTRIES: usize = 200;

/// 导入任务的基础输入。
pub struct IngestTaskInput<'a> {
    pub source_path: &'a Path,
    pub raw_path: &'a Path,
    pub wiki_path: &'a Path,
    pub content_hash: &'a str,
    pub title: &'a str,
    pub timestamp_ms: &'a str,
}

/// 导入任务创建结果。
pub struct IngestTaskRecord {
    pub source_id: i64,
    pub task_id: i64,
}

/// lint 用的待处理任务记录。
#[derive(Debug, Clone)]
pub struct PendingTaskRecord {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub raw_path: String,
    pub wiki_path: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 已存在的导入结果（用于内容去重）。
#[derive(Debug, Clone)]
pub struct ExistingIngestRecord {
    pub raw_path: String,
    pub wiki_path: String,
}

/// 引用写入输入。
pub struct CitationInput<'a> {
    pub cited_page_path: &'a str,
    pub score: usize,
    pub excerpt: &'a str,
}

/// lint 用的引用记录。
#[derive(Debug, Clone)]
pub struct CitationRecord {
    pub page_path: String,
    pub cited_page_path: String,
    pub score: usize,
    pub excerpt: String,
}

/// wiki_pages 查询记录。
#[derive(Debug, Clone)]
pub struct WikiPageRecord {
    pub title: String,
    pub path: String,
    pub summary: String,
    pub updated_at: String,
    pub score: f64,
}

/// Ask 历史记录。
#[derive(Debug, Clone)]
pub struct AskHistoryRecord {
    pub id: i64,
    pub question: String,
    pub created_at: String,
}

/// Outbox 事件记录。
#[derive(Debug, Clone)]
pub struct OutboxEventRecord {
    pub id: i64,
    pub event_type: String,
    pub payload_json: String,
    pub created_at: String,
    pub processed_at: Option<String>,
    pub consumer_tag: Option<String>,
}

/// 页面 Embedding 记录。
#[derive(Debug, Clone)]
pub struct PageEmbeddingRecord {
    pub page_path: String,
    pub embedding: Vec<f32>,
}

/// 将页面路径及其向量（二进制 BLOB 格式）插入或更新到 `page_embeddings` 表中。
pub fn upsert_embedding(
    db_path: &Path,
    page_path: &str,
    embedding: &[f32],
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;

    // 将 Vec<f32> 转换为 Vec<u8> (little-endian BLOB)
    let mut blob = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        blob.extend_from_slice(&val.to_ne_bytes());
    }

    conn.execute(
        r#"
        INSERT INTO page_embeddings (page_path, embedding_blob)
        VALUES (?1, ?2)
        ON CONFLICT(page_path) DO UPDATE SET
            embedding_blob = excluded.embedding_blob
        "#,
        params![page_path, blob],
    )
    .map_err(|err| format!("写入 page_embeddings 失败: {}", err))?;

    Ok(())
}

/// 读取页面 Embedding 列表（按页面路径排序）。
pub fn list_embeddings(db_path: &Path, limit: usize) -> Result<Vec<PageEmbeddingRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT page_path, embedding_blob
            FROM page_embeddings
            ORDER BY page_path ASC
            LIMIT ?1
            "#,
        )
        .map_err(|err| format!("准备查询 page_embeddings 失败: {}", err))?;

    let rows = stmt
        .query_map(params![limit as i64], |row| {
            let page_path: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((page_path, blob))
        })
        .map_err(|err| format!("执行 page_embeddings 查询失败: {}", err))?;

    let mut result = Vec::new();
    for item in rows {
        let (page_path, blob) = item.map_err(|err| format!("读取 page_embeddings 结果失败: {}", err))?;
        let embedding = decode_embedding_blob(&blob)?;
        result.push(PageEmbeddingRecord { page_path, embedding });
    }
    Ok(result)
}

fn decode_embedding_blob(blob: &[u8]) -> Result<Vec<f32>, String> {
    if blob.is_empty() {
        return Ok(Vec::new());
    }
    if blob.len() % 4 != 0 {
        return Err("embedding_blob 长度非法（不是 4 的倍数）".to_string());
    }
    let mut out = Vec::with_capacity(blob.len() / 4);
    for chunk in blob.chunks_exact(4) {
        let bytes = [chunk[0], chunk[1], chunk[2], chunk[3]];
        out.push(f32::from_ne_bytes(bytes));
    }
    Ok(out)
}

/// 确保元数据库与表结构存在。
pub fn ensure_meta_db(db_path: &Path) -> Result<(), String> {
    let _ = get_connection(db_path)?;
    Ok(())
}

/// 统计待处理任务数量。
pub fn count_pending_tasks(db_path: &Path) -> Result<usize, String> {
    Ok(list_pending_tasks(db_path)?.len())
}

/// 读取所有 wiki 页面路径。
pub fn list_wiki_page_paths(db_path: &Path) -> Result<Vec<String>, String> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare("SELECT path FROM wiki_pages ORDER BY path ASC")
        .map_err(|err| format!("准备查询 wiki_pages 失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|err| format!("读取 wiki_pages 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 wiki_pages 失败: {}", err))
}

/// 读取待处理任务明细。
pub fn list_pending_tasks(db_path: &Path) -> Result<Vec<PendingTaskRecord>, String> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, kind, status, raw_path, wiki_path, created_at, updated_at
            FROM tasks
            WHERE status IN ('queued', 'running', 'reviewing')
            ORDER BY updated_at ASC, id ASC
            "#,
        )
        .map_err(|err| format!("准备查询 tasks 失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(PendingTaskRecord {
                id: row.get(0)?,
                kind: row.get(1)?,
                status: row.get(2)?,
                raw_path: row.get(3)?,
                wiki_path: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|err| format!("读取 tasks 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 tasks 失败: {}", err))
}

/// 按内容哈希查找已存在的导入结果。
pub fn find_existing_ingest_by_hash(
    db_path: &Path,
    content_hash: &str,
) -> Result<Option<ExistingIngestRecord>, String> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT sources.raw_path, wiki_pages.path
            FROM sources
            JOIN wiki_pages ON wiki_pages.source_id = sources.id
            WHERE sources.content_hash = ?1
            ORDER BY wiki_pages.updated_at DESC, wiki_pages.id DESC
            LIMIT 1
            "#,
        )
        .map_err(|err| format!("准备查询重复导入记录失败: {}", err))?;

    match stmt.query_row(params![content_hash], |row| {
        Ok(ExistingIngestRecord {
            raw_path: row.get(0)?,
            wiki_path: row.get(1)?,
        })
    }) {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(format!("查询重复导入记录失败: {}", err)),
    }
}

/// 读取所有引用关系。
pub fn list_citations(db_path: &Path) -> Result<Vec<CitationRecord>, String> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT page_path, cited_page_path, score, excerpt
            FROM citations
            ORDER BY id ASC
            "#,
        )
        .map_err(|err| format!("准备查询 citations 失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| {
            let score = row.get::<_, i64>(2)?;
            Ok(CitationRecord {
                page_path: row.get(0)?,
                cited_page_path: row.get(1)?,
                score: usize::try_from(score).unwrap_or_default(),
                excerpt: row.get(3)?,
            })
        })
        .map_err(|err| format!("读取 citations 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 citations 失败: {}", err))
}

/// 读取指定页面的引用关系。
pub fn list_citations_for_page(
    db_path: &Path,
    page_path: &str,
) -> Result<Vec<CitationRecord>, String> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT page_path, cited_page_path, score, excerpt
            FROM citations
            WHERE page_path = ?1
            ORDER BY id ASC
            "#,
        )
        .map_err(|err| format!("准备查询 citations 失败: {}", err))?;
    let rows = stmt
        .query_map(params![page_path], |row| {
            let score = row.get::<_, i64>(2)?;
            Ok(CitationRecord {
                page_path: row.get(0)?,
                cited_page_path: row.get(1)?,
                score: usize::try_from(score).unwrap_or_default(),
                excerpt: row.get(3)?,
            })
        })
        .map_err(|err| format!("读取 citations 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 citations 失败: {}", err))
}

/// 写入 Lint 补丁应用事件。
pub fn insert_lint_patch_event(
    db_path: &Path,
    issue_code: &str,
    path: Option<&str>,
    applied: bool,
    message: &str,
    timestamp_ms: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.execute(
        r#"
        INSERT INTO lint_patch_events (
            issue_code,
            path,
            applied,
            message,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            issue_code,
            path,
            if applied { 1_i64 } else { 0_i64 },
            message,
            timestamp_ms
        ],
    )
    .map_err(|err| format!("写入 lint_patch_events 失败: {}", err))?;
    Ok(())
}

/// 读取最近的 Lint 补丁应用事件。
pub fn list_recent_lint_patch_events(
    db_path: &Path,
    limit: usize,
) -> Result<Vec<LintPatchEventItem>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT issue_code, path, applied, message, created_at
            FROM lint_patch_events
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )
        .map_err(|err| format!("准备查询 lint_patch_events 失败: {}", err))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(LintPatchEventItem {
                issue_code: row.get(0)?,
                path: row.get(1)?,
                applied: row.get::<_, i64>(2)? != 0,
                message: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|err| format!("读取 lint_patch_events 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 lint_patch_events 失败: {}", err))
}

/// 读取最近更新的 wiki 页面。
pub fn list_recent_wiki_pages(db_path: &Path, limit: usize) -> Result<Vec<WikiPageRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT title, path, summary, updated_at
            FROM wiki_pages
            ORDER BY updated_at DESC, id DESC
            LIMIT ?1
            "#,
        )
        .map_err(|err| format!("准备查询 wiki_pages 失败: {}", err))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(WikiPageRecord {
                title: row.get(0)?,
                path: row.get(1)?,
                summary: row.get(2)?,
                updated_at: row.get(3)?,
                score: 0.0,
            })
        })
        .map_err(|err| format!("读取 wiki_pages 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 wiki_pages 失败: {}", err))
}

fn try_fts_search_wiki_pages(
    conn: &Connection,
    fts_query: &str,
    limit: usize,
) -> Result<Vec<WikiPageRecord>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT w.title, w.path, w.summary, w.updated_at,
                   (-bm25(fts_pages)) AS score
            FROM fts_pages
            JOIN wiki_pages w ON w.path = fts_pages.path
            WHERE fts_pages MATCH ?1
            ORDER BY bm25(fts_pages) ASC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备 FTS5 wiki 搜索失败: {}", err))?;
    let rows = stmt
        .query_map(params![fts_query, limit as i64], |row| {
            Ok(WikiPageRecord {
                title: row.get(0)?,
                path: row.get(1)?,
                summary: row.get(2)?,
                updated_at: row.get(3)?,
                score: row.get::<_, f64>(4).unwrap_or(0.0),
            })
        })
        .map_err(|err| format!("FTS5 wiki 搜索失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 FTS5 搜索结果失败: {}", err))
}

/// 按关键字搜索 wiki 页面（标题/摘要/路径）。
pub fn search_wiki_pages(
    db_path: &Path,
    keyword: &str,
    limit: usize,
) -> Result<Vec<WikiPageRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let normalized = keyword.trim();
    if normalized.is_empty() {
        return list_recent_wiki_pages(db_path, limit);
    }
    let conn = open_connection(db_path)?;
    // Try FTS5 first
    let tokens: Vec<String> = normalized.split_whitespace().map(|s| s.to_string()).collect();
    if let Some(fts_query) = build_fts_match_query(&tokens) {
        match try_fts_search_wiki_pages(&conn, &fts_query, limit) {
            Ok(rows) if !rows.is_empty() => return Ok(rows),
            _ => {}
        }
    }
    // Fallback: instr-based with priority score
    let mut stmt = conn
        .prepare(
            r#"
            SELECT title, path, summary, updated_at,
                   CASE
                     WHEN instr(lower(title),   lower(?1)) > 0 THEN 3.0
                     WHEN instr(lower(summary), lower(?1)) > 0 THEN 2.0
                     ELSE 1.0
                   END AS score
            FROM wiki_pages
            WHERE instr(lower(title),   lower(?1)) > 0
               OR instr(lower(summary), lower(?1)) > 0
               OR instr(lower(path),    lower(?1)) > 0
            ORDER BY score DESC, updated_at DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备搜索 wiki_pages 失败: {}", err))?;
    let rows = stmt
        .query_map(params![normalized, limit as i64], |row| {
            Ok(WikiPageRecord {
                title: row.get(0)?,
                path: row.get(1)?,
                summary: row.get(2)?,
                updated_at: row.get(3)?,
                score: row.get::<_, f64>(4).unwrap_or(0.0),
            })
        })
        .map_err(|err| format!("搜索 wiki_pages 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取搜索结果失败: {}", err))
}

/// 创建导入任务的源记录与任务记录。
pub fn begin_ingest_task(
    db_path: &Path,
    input: &IngestTaskInput<'_>,
) -> Result<IngestTaskRecord, String> {
    let mut conn = open_connection(db_path)?;
    init_schema(&conn)?;

    let tx = conn
        .transaction()
        .map_err(|err| format!("开启数据库事务失败: {}", err))?;

    tx.execute(
        r#"
        INSERT OR IGNORE INTO sources (
            content_hash,
            source_path,
            raw_path,
            created_at
        ) VALUES (?1, ?2, ?3, ?4)
        "#,
        params![
            input.content_hash,
            input.source_path.to_string_lossy(),
            input.raw_path.to_string_lossy(),
            input.timestamp_ms
        ],
    )
    .map_err(|err| format!("写入 sources 失败: {}", err))?;

    let source_id: i64 = tx
        .query_row(
            "SELECT id FROM sources WHERE content_hash = ?1",
            params![input.content_hash],
            |row| row.get(0),
        )
        .map_err(|err| format!("读取 sources 失败: {}", err))?;

    tx.execute(
        r#"
        INSERT INTO tasks (
            source_id,
            kind,
            status,
            raw_path,
            wiki_path,
            error,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6)
        "#,
        params![
            source_id,
            "ingest_markdown",
            "running",
            input.raw_path.to_string_lossy(),
            input.wiki_path.to_string_lossy(),
            input.timestamp_ms
        ],
    )
    .map_err(|err| format!("写入 tasks 失败: {}", err))?;

    let task_id = tx.last_insert_rowid();

    tx.execute(
        r#"
        INSERT INTO task_events (
            task_id,
            event_type,
            message,
            created_at
        ) VALUES (?1, ?2, ?3, ?4)
        "#,
        params![
            task_id,
            "running",
            format!("导入任务已创建: {}", input.title),
            input.timestamp_ms
        ],
    )
    .map_err(|err| format!("写入 task_events 失败: {}", err))?;

    tx.commit()
        .map_err(|err| format!("提交导入任务失败: {}", err))?;

    Ok(IngestTaskRecord { source_id, task_id })
}

/// 追加任务事件。
pub fn append_task_event(
    db_path: &Path,
    task_id: i64,
    event_type: &str,
    message: &str,
    timestamp_ms: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    conn.execute(
        r#"
        INSERT INTO task_events (
            task_id,
            event_type,
            message,
            created_at
        ) VALUES (?1, ?2, ?3, ?4)
        "#,
        params![task_id, event_type, message, timestamp_ms],
    )
    .map_err(|err| format!("写入 task_events 失败: {}", err))?;
    Ok(())
}

/// 更新任务状态。
pub fn update_task_status(
    db_path: &Path,
    task_id: i64,
    status: &str,
    error: Option<&str>,
    timestamp_ms: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    conn.execute(
        r#"
        UPDATE tasks
        SET status = ?1,
            error = ?2,
            updated_at = ?3
        WHERE id = ?4
        "#,
        params![status, error, timestamp_ms, task_id],
    )
    .map_err(|err| format!("更新 tasks 失败: {}", err))?;
    Ok(())
}

/// 写入 wiki 页面记录。
pub fn record_wiki_page(
    db_path: &Path,
    source_id: i64,
    title: &str,
    wiki_path: &Path,
    summary: &str,
    timestamp_ms: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    conn.execute(
        r#"
        INSERT INTO wiki_pages (
            source_id,
            title,
            path,
            summary,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
        "#,
        params![
            source_id,
            title,
            wiki_path.to_string_lossy(),
            summary,
            timestamp_ms
        ],
    )
    .map_err(|err| format!("写入 wiki_pages 失败: {}", err))?;
    Ok(())
}

/// 写入或更新 AI 生成页面记录。
pub fn upsert_generated_wiki_page(
    db_path: &Path,
    title: &str,
    wiki_path: &Path,
    summary: &str,
    content_hash: &str,
    timestamp_ms: &str,
) -> Result<(), String> {
    let mut conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let tx = conn
        .transaction()
        .map_err(|err| format!("开启数据库事务失败: {}", err))?;

    let source_uri = format!("query://generated/{}", content_hash);
    tx.execute(
        r#"
        INSERT OR IGNORE INTO sources (
            content_hash,
            source_path,
            raw_path,
            created_at
        ) VALUES (?1, ?2, ?3, ?4)
        "#,
        params![
            content_hash,
            source_uri,
            wiki_path.to_string_lossy(),
            timestamp_ms
        ],
    )
    .map_err(|err| format!("写入 sources 失败: {}", err))?;

    let source_id: i64 = tx
        .query_row(
            "SELECT id FROM sources WHERE content_hash = ?1",
            params![content_hash],
            |row| row.get(0),
        )
        .map_err(|err| format!("读取 sources 失败: {}", err))?;

    tx.execute(
        r#"
        INSERT INTO wiki_pages (
            source_id,
            title,
            path,
            summary,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
        ON CONFLICT(path) DO UPDATE SET
            source_id = excluded.source_id,
            title = excluded.title,
            summary = excluded.summary,
            updated_at = excluded.updated_at
        "#,
        params![
            source_id,
            title,
            wiki_path.to_string_lossy(),
            summary,
            timestamp_ms
        ],
    )
    .map_err(|err| format!("写入 wiki_pages 失败: {}", err))?;

    tx.commit()
        .map_err(|err| format!("提交生成页面记录失败: {}", err))
}

/// 用最新结果替换指定页面的引用记录。
pub fn replace_citations_for_page(
    db_path: &Path,
    page_path: &Path,
    citations: &[CitationInput<'_>],
    timestamp_ms: &str,
) -> Result<(), String> {
    let mut conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let tx = conn
        .transaction()
        .map_err(|err| format!("开启数据库事务失败: {}", err))?;

    tx.execute(
        "DELETE FROM citations WHERE page_path = ?1",
        params![page_path.to_string_lossy()],
    )
    .map_err(|err| format!("清理旧引用失败: {}", err))?;

    for citation in citations {
        tx.execute(
            r#"
            INSERT INTO citations (
                page_path,
                cited_page_path,
                score,
                excerpt,
                created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                page_path.to_string_lossy(),
                citation.cited_page_path,
                citation.score as i64,
                citation.excerpt,
                timestamp_ms
            ],
        )
        .map_err(|err| format!("写入 citations 失败: {}", err))?;
    }

    tx.commit()
        .map_err(|err| format!("提交引用记录失败: {}", err))
}

/// 将页面内容写入 FTS 索引。
pub fn upsert_fts_page(
    db_path: &Path,
    wiki_path: &Path,
    title: &str,
    body: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;

    // 先删再插，确保 path 唯一且内容为最新版本。
    conn.execute(
        "DELETE FROM fts_pages WHERE path = ?1",
        params![wiki_path.to_string_lossy()],
    )
    .map_err(|err| format!("删除旧 fts 索引失败: {}", err))?;

    conn.execute(
        "INSERT INTO fts_pages(path, title, body) VALUES (?1, ?2, ?3)",
        params![wiki_path.to_string_lossy(), title, body],
    )
    .map_err(|err| format!("写入 fts 索引失败: {}", err))?;

    Ok(())
}

/// 从数据库中删除 Wiki 页面的所有相关记录（wiki_pages / citations / fts_pages）。
pub fn delete_wiki_page_from_db(db_path: &Path, page_path: &Path) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    let path_str = page_path.to_string_lossy();

    // 删除 FTS 索引
    conn.execute(
        "DELETE FROM fts_pages WHERE path = ?1",
        params![path_str],
    )
    .map_err(|err| format!("删除 fts_pages 记录失败: {}", err))?;

    // 删除引用（作为引用方或被引用方均清理）
    conn.execute(
        "DELETE FROM citations WHERE page_path = ?1 OR cited_page_path = ?1",
        params![path_str],
    )
    .map_err(|err| format!("删除 citations 记录失败: {}", err))?;

    // 删除主记录
    conn.execute(
        "DELETE FROM wiki_pages WHERE path = ?1",
        params![path_str],
    )
    .map_err(|err| format!("删除 wiki_pages 记录失败: {}", err))?;

    Ok(())
}

/// 将数据库中所有引用 old_path 的记录更新为 new_path。
/// 涉及：wiki_pages.path、citations.page_path、citations.cited_page_path、fts_pages.path。
pub fn rename_wiki_page_in_db(
    db_path: &Path,
    old_path: &Path,
    new_path: &Path,
    new_title: &str,
    new_body: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    let old_str = old_path.to_string_lossy();
    let new_str = new_path.to_string_lossy();

    // wiki_pages 主记录
    conn.execute(
        "UPDATE wiki_pages SET path = ?1 WHERE path = ?2",
        params![new_str, old_str],
    )
    .map_err(|err| format!("更新 wiki_pages.path 失败: {}", err))?;

    // citations：作为引用方
    conn.execute(
        "UPDATE citations SET page_path = ?1 WHERE page_path = ?2",
        params![new_str, old_str],
    )
    .map_err(|err| format!("更新 citations.page_path 失败: {}", err))?;

    // citations：作为被引用方
    conn.execute(
        "UPDATE citations SET cited_page_path = ?1 WHERE cited_page_path = ?2",
        params![new_str, old_str],
    )
    .map_err(|err| format!("更新 citations.cited_page_path 失败: {}", err))?;

    // fts_pages：删旧插新
    conn.execute(
        "DELETE FROM fts_pages WHERE path = ?1",
        params![old_str],
    )
    .map_err(|err| format!("删除旧 fts_pages 记录失败: {}", err))?;

    conn.execute(
        "INSERT INTO fts_pages(path, title, body) VALUES (?1, ?2, ?3)",
        params![new_str, new_title, new_body],
    )
    .map_err(|err| format!("写入新 fts_pages 记录失败: {}", err))?;

    Ok(())
}

/// 基于 FTS5 返回候选页面路径。
pub fn search_fts_page_paths(
    db_path: &Path,
    tokens: &[String],
    limit: usize,
) -> Result<Vec<String>, String> {
    if tokens.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let Some(query) = build_fts_match_query(tokens) else {
        return Ok(Vec::new());
    };

    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT path
            FROM fts_pages
            WHERE fts_pages MATCH ?1
            ORDER BY bm25(fts_pages)
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备 FTS 查询失败: {}", err))?;
    let rows = stmt
        .query_map(params![query, limit as i64], |row| row.get::<_, String>(0))
        .map_err(|err| format!("执行 FTS 查询失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 FTS 查询结果失败: {}", err))
}

/// 查找与指定页面集合有引用关系的页面路径（双向：引用方 + 被引用方）。
/// 用于 RRF 链接扩展路径。
pub fn query_linked_page_paths(
    db_path: &Path,
    from_paths: &[String],
    limit: usize,
) -> Result<Vec<String>, String> {
    if from_paths.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    let mut result = std::collections::HashSet::new();

    for path in from_paths {
        // 正向：此页面引用的页面
        let mut stmt = conn.prepare(
            "SELECT cited_page_path FROM citations WHERE page_path = ?1 LIMIT ?2"
        ).map_err(|e| format!("准备正向链接查询失败: {}", e))?;
        let rows = stmt.query_map(params![path, limit as i64], |row| row.get::<_, String>(0))
            .map_err(|e| format!("执行正向链接查询失败: {}", e))?;
        for r in rows.flatten() { result.insert(r); }

        // 反向：引用此页面的页面
        let mut stmt = conn.prepare(
            "SELECT page_path FROM citations WHERE cited_page_path = ?1 LIMIT ?2"
        ).map_err(|e| format!("准备反向链接查询失败: {}", e))?;
        let rows = stmt.query_map(params![path, limit as i64], |row| row.get::<_, String>(0))
            .map_err(|e| format!("执行反向链接查询失败: {}", e))?;
        for r in rows.flatten() { result.insert(r); }

        if result.len() >= limit { break; }
    }

    // 去掉 from_paths 本身（避免循环引用降低 RRF 效果）
    let from_set: std::collections::HashSet<&String> = from_paths.iter().collect();
    Ok(result.into_iter()
        .filter(|p| !from_set.contains(p))
        .take(limit)
        .collect())
}

/// 查找被引用次数最多的页面路径（Citation 热度排序）。
/// 用于 RRF 热度路径。
pub fn query_citation_popular_paths(
    db_path: &Path,
    limit: usize,
) -> Result<Vec<String>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT cited_page_path
            FROM citations
            GROUP BY cited_page_path
            ORDER BY COUNT(*) DESC
            LIMIT ?1
            "#,
        )
        .map_err(|e| format!("准备 citation 热度查询失败: {}", e))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| row.get::<_, String>(0))
        .map_err(|e| format!("执行 citation 热度查询失败: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取 citation 热度结果失败: {}", e))
}

fn open_connection(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("创建数据库目录失败: {}", err))?;
    }

    let conn = Connection::open(db_path).map_err(|err| format!("打开数据库失败: {}", err))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|err| format!("启用外键失败: {}", err))?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content_hash TEXT NOT NULL UNIQUE,
            source_path TEXT NOT NULL,
            raw_path TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id INTEGER NOT NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            raw_path TEXT NOT NULL,
            wiki_path TEXT NOT NULL,
            error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(source_id) REFERENCES sources(id)
        );

        CREATE TABLE IF NOT EXISTS task_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY(task_id) REFERENCES tasks(id)
        );

        CREATE TABLE IF NOT EXISTS wiki_pages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            path TEXT NOT NULL UNIQUE,
            summary TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY(source_id) REFERENCES sources(id)
        );

        CREATE TABLE IF NOT EXISTS citations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            page_path TEXT NOT NULL,
            cited_page_path TEXT NOT NULL,
            score INTEGER NOT NULL,
            excerpt TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS lint_patch_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            issue_code TEXT NOT NULL,
            path TEXT,
            applied INTEGER NOT NULL,
            message TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS page_embeddings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            page_path TEXT NOT NULL UNIQUE,
            embedding_blob BLOB NOT NULL
        );

        CREATE TABLE IF NOT EXISTS wiki_outbox (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            processed_at TEXT,
            consumer_tag TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_citations_page_path
            ON citations(page_path);

        CREATE INDEX IF NOT EXISTS idx_lint_patch_events_created_at
            ON lint_patch_events(created_at);

        CREATE INDEX IF NOT EXISTS idx_wiki_outbox_created_at
            ON wiki_outbox(created_at);

        CREATE INDEX IF NOT EXISTS idx_wiki_outbox_processed_at
            ON wiki_outbox(processed_at);

        CREATE TABLE IF NOT EXISTS ask_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            question TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ask_history_created_at
            ON ask_history(created_at);

        CREATE TABLE IF NOT EXISTS ingest_queue_items (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            source_type TEXT NOT NULL,
            source_path TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'queued',
            error       TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS research_tasks (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            topic             TEXT NOT NULL,
            status            TEXT NOT NULL DEFAULT 'queued',
            sub_queries       TEXT NOT NULL DEFAULT '[]',
            web_results_count INTEGER NOT NULL DEFAULT 0,
            saved_path        TEXT,
            error             TEXT,
            created_at        TEXT NOT NULL,
            updated_at        TEXT NOT NULL
        );
        "#,
    )
    .map_err(|err| format!("初始化数据库结构失败: {}", err))?;

    // FTS 为增强能力，初始化失败时不阻断主流程，查询侧会自动降级。
    let _ = conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS fts_pages USING fts5(path UNINDEXED, title, body)",
        [],
    );

    ensure_ask_history_quality(conn)?;

    Ok(())
}

/// 归一化问题文本：裁剪首尾空白、压缩连续空白，并统一小写用于去重。
fn normalize_ask_question(question: &str) -> String {
    question
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Ask 历史表质量保障：补列、回填、去重、索引。
fn ensure_ask_history_quality(conn: &Connection) -> Result<(), String> {
    let mut has_question_norm = false;
    let mut stmt = conn
        .prepare("PRAGMA table_info(ask_history)")
        .map_err(|err| format!("读取 ask_history 表结构失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| format!("读取 ask_history 字段失败: {}", err))?;
    for row in rows {
        let name = row.map_err(|err| format!("读取 ask_history 字段失败: {}", err))?;
        if name == "question_norm" {
            has_question_norm = true;
            break;
        }
    }

    if has_question_norm {
        // 如果已经有 question_norm，说明结构已经升级并清洗过，跳过重度操作以提升性能。
        return Ok(());
    }

    conn.execute("ALTER TABLE ask_history ADD COLUMN question_norm TEXT", [])
        .map_err(|err| format!("升级 ask_history 结构失败: {}", err))?;

    // 清洗旧数据，避免空白问题和历史重复占用容量。
    conn.execute(
        "UPDATE ask_history SET question = trim(question) WHERE question <> trim(question)",
        [],
    )
    .map_err(|err| format!("清洗 ask_history 问题文本失败: {}", err))?;

    {
        let mut select_stmt = conn
            .prepare("SELECT id, question FROM ask_history")
            .map_err(|err| format!("读取 ask_history 历史数据失败: {}", err))?;
        let rows = select_stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
            .map_err(|err| format!("读取 ask_history 历史数据失败: {}", err))?;

        for row in rows {
            let (id, question) = row.map_err(|err| format!("读取 ask_history 历史数据失败: {}", err))?;
            let norm = normalize_ask_question(question.trim());
            conn.execute(
                "UPDATE ask_history SET question_norm = ?1 WHERE id = ?2",
                params![norm, id],
            )
            .map_err(|err| format!("回填 ask_history 归一化字段失败: {}", err))?;
        }
    }

    conn.execute(
        "DELETE FROM ask_history WHERE question_norm IS NULL OR question_norm = ''",
        [],
    )
    .map_err(|err| format!("清理 ask_history 空问题失败: {}", err))?;

    conn.execute(
        r#"
        DELETE FROM ask_history
        WHERE id NOT IN (
            SELECT MAX(id)
            FROM ask_history
            GROUP BY question_norm
        )
        "#,
        [],
    )
    .map_err(|err| format!("清理 ask_history 重复问题失败: {}", err))?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_ask_history_question_norm_unique ON ask_history(question_norm)",
        [],
    )
    .map_err(|err| format!("创建 ask_history 去重索引失败: {}", err))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_ask_history_created_at_id ON ask_history(created_at, id)",
        [],
    )
    .map_err(|err| format!("创建 ask_history 时间索引失败: {}", err))?;

    Ok(())
}

/// 裁剪 Ask 历史总量，按时间和 id 保留最新 N 条。
fn prune_ask_history(conn: &Connection, max_entries: usize) -> Result<(), String> {
    conn.execute(
        r#"
        DELETE FROM ask_history
        WHERE id IN (
            SELECT id
            FROM ask_history
            ORDER BY CAST(created_at AS INTEGER) DESC, id DESC
            LIMIT -1 OFFSET ?1
        )
        "#,
        params![max_entries as i64],
    )
    .map_err(|err| format!("裁剪 ask_history 失败: {}", err))?;
    Ok(())
}

/// 保存一条 Ask 历史问题，返回插入的 id。
pub fn save_ask_history(db_path: &Path, question: &str, created_at: &str) -> Result<i64, String> {
    let cleaned_question = question.trim();
    if cleaned_question.is_empty() {
        return Ok(0);
    }
    let question_norm = normalize_ask_question(cleaned_question);

    let shared_conn = get_connection(db_path)?;
    let mut conn = shared_conn.lock().unwrap();
    let tx = conn
        .transaction()
        .map_err(|err| format!("开启 ask_history 事务失败: {}", err))?;

    let existing_id = tx
        .query_row(
            "SELECT id FROM ask_history WHERE question_norm = ?1 ORDER BY id DESC LIMIT 1",
            params![question_norm],
            |row| row.get::<_, i64>(0),
        )
        .ok();

    let final_id = if let Some(id) = existing_id {
        tx.execute(
            "UPDATE ask_history SET question = ?1, created_at = ?2, question_norm = ?3 WHERE id = ?4",
            params![cleaned_question, created_at, question_norm, id],
        )
        .map_err(|err| format!("更新 ask_history 失败: {}", err))?;
        tx.execute(
            "DELETE FROM ask_history WHERE question_norm = ?1 AND id <> ?2",
            params![question_norm, id],
        )
        .map_err(|err| format!("清理 ask_history 重复行失败: {}", err))?;
        id
    } else {
        tx.execute(
            "INSERT INTO ask_history (question, question_norm, created_at) VALUES (?1, ?2, ?3)",
            params![cleaned_question, question_norm, created_at],
        )
        .map_err(|err| format!("写入 ask_history 失败: {}", err))?;
        tx.last_insert_rowid()
    };

    prune_ask_history(&tx, ASK_HISTORY_MAX_ENTRIES)?;
    tx.commit()
        .map_err(|err| format!("提交 ask_history 事务失败: {}", err))?;
    Ok(final_id)
}

/// 读取最近的 Ask 历史，按时间倒序。
pub fn list_ask_history(db_path: &Path, limit: usize) -> Result<Vec<AskHistoryRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, question, created_at FROM ask_history ORDER BY CAST(created_at AS INTEGER) DESC, id DESC LIMIT ?1",
        )
        .map_err(|err| format!("准备查询 ask_history 失败: {}", err))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(AskHistoryRecord {
                id: row.get(0)?,
                question: row.get(1)?,
                created_at: row.get(2)?,
            })
        })
        .map_err(|err| format!("执行查询 ask_history 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 ask_history 失败: {}", err))
}

/// 清空 Ask 历史，返回删除条数。
pub fn clear_ask_history(db_path: &Path) -> Result<usize, String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let affected = conn
        .execute("DELETE FROM ask_history", [])
        .map_err(|err| format!("清空 ask_history 失败: {}", err))?;
    Ok(affected)
}

/// 追加一条 outbox 事件，返回事件 id。
pub fn append_outbox_event(
    db_path: &Path,
    event_type: &str,
    payload_json: &str,
    created_at: &str,
) -> Result<i64, String> {
    let normalized_event_type = event_type.trim();
    if normalized_event_type.is_empty() {
        return Err("event_type 不能为空".to_string());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.execute(
        r#"
        INSERT INTO wiki_outbox (
            event_type,
            payload_json,
            created_at
        ) VALUES (?1, ?2, ?3)
        "#,
        params![normalized_event_type, payload_json, created_at],
    )
    .map_err(|err| format!("写入 wiki_outbox 失败: {}", err))?;
    Ok(conn.last_insert_rowid())
}

/// 按事件 id 增量读取 outbox。
pub fn list_outbox_events_from_id(
    db_path: &Path,
    last_id: i64,
    limit: usize,
) -> Result<Vec<OutboxEventRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, event_type, payload_json, created_at, processed_at, consumer_tag
            FROM wiki_outbox
            WHERE id > ?1
            ORDER BY id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备查询 wiki_outbox 失败: {}", err))?;
    let rows = stmt
        .query_map(params![last_id, limit as i64], |row| {
            Ok(OutboxEventRecord {
                id: row.get(0)?,
                event_type: row.get(1)?,
                payload_json: row.get(2)?,
                created_at: row.get(3)?,
                processed_at: row.get(4)?,
                consumer_tag: row.get(5)?,
            })
        })
        .map_err(|err| format!("查询 wiki_outbox 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 wiki_outbox 失败: {}", err))
}

/// 标记 outbox 事件已消费，返回本次 ack 数量。
pub fn ack_outbox_events(
    db_path: &Path,
    up_to_id: i64,
    consumer_tag: &str,
    processed_at: &str,
) -> Result<usize, String> {
    let normalized_consumer_tag = consumer_tag.trim();
    if normalized_consumer_tag.is_empty() {
        return Err("consumer_tag 不能为空".to_string());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let affected = conn
        .execute(
            r#"
            UPDATE wiki_outbox
            SET processed_at = ?1,
                consumer_tag = ?2
            WHERE id <= ?3 AND processed_at IS NULL
            "#,
            params![processed_at, normalized_consumer_tag, up_to_id],
        )
        .map_err(|err| format!("更新 wiki_outbox ack 失败: {}", err))?;
    Ok(affected)
}

/// 插入一条 queued 记录，返回新 id。
pub fn db_enqueue_ingest(conn: &Connection, source_type: &str, source_path: &str, now: &str) -> Result<i64, String> {
    conn.execute(
        r#"
        INSERT INTO ingest_queue_items (source_type, source_path, status, created_at, updated_at)
        VALUES (?1, ?2, 'queued', ?3, ?3)
        "#,
        params![source_type, source_path, now],
    )
    .map_err(|err| format!("写入 ingest_queue_items 失败: {}", err))?;
    Ok(conn.last_insert_rowid())
}

/// 查询全部 ingest_queue_items，按 created_at DESC。
pub fn db_list_ingest_queue(conn: &Connection) -> Result<Vec<crate::models::IngestQueueItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, source_type, source_path, status, error, created_at, updated_at
            FROM ingest_queue_items
            ORDER BY created_at DESC
            "#,
        )
        .map_err(|err| format!("准备查询 ingest_queue_items 失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(crate::models::IngestQueueItem {
                id: row.get(0)?,
                source_type: row.get(1)?,
                source_path: row.get(2)?,
                status: row.get(3)?,
                error: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|err| format!("查询 ingest_queue_items 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 ingest_queue_items 失败: {}", err))
}

/// 取最旧一条 queued 记录。
pub fn db_get_next_queued_item(conn: &Connection) -> Result<Option<crate::models::IngestQueueItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, source_type, source_path, status, error, created_at, updated_at
            FROM ingest_queue_items
            WHERE status = 'queued'
            ORDER BY created_at ASC, id ASC
            LIMIT 1
            "#,
        )
        .map_err(|err| format!("准备查询 ingest_queue_items 失败: {}", err))?;
    match stmt.query_row([], |row| {
        Ok(crate::models::IngestQueueItem {
            id: row.get(0)?,
            source_type: row.get(1)?,
            source_path: row.get(2)?,
            status: row.get(3)?,
            error: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    }) {
        Ok(item) => Ok(Some(item)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(format!("查询 ingest_queue_items 失败: {}", err)),
    }
}

/// 更新 ingest_queue_items 的 status + error + updated_at。
pub fn db_update_ingest_queue_status(conn: &Connection, id: i64, status: &str, error: Option<&str>, now: &str) -> Result<(), String> {
    conn.execute(
        r#"
        UPDATE ingest_queue_items
        SET status = ?1, error = ?2, updated_at = ?3
        WHERE id = ?4
        "#,
        params![status, error, now, id],
    )
    .map_err(|err| format!("更新 ingest_queue_items 失败: {}", err))?;
    Ok(())
}

/// 把所有 running → queued（启动恢复用），返回影响行数。
pub fn db_reset_stale_running(conn: &Connection, now: &str) -> Result<usize, String> {
    let affected = conn
        .execute(
            r#"
            UPDATE ingest_queue_items
            SET status = 'queued', updated_at = ?1
            WHERE status = 'running'
            "#,
            params![now],
        )
        .map_err(|err| format!("重置 running 记录失败: {}", err))?;
    Ok(affected)
}

fn build_fts_match_query(tokens: &[String]) -> Option<String> {
    let terms = tokens
        .iter()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"*", token.replace('\"', "")))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// 获取所有 Wiki 页面（用于知识图谱节点构建）。
pub fn list_all_wiki_pages(db_path: &Path) -> Result<Vec<WikiPageRecord>, String> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT title, path, summary, updated_at, 0.0 AS score
            FROM wiki_pages
            ORDER BY updated_at DESC
            "#,
        )
        .map_err(|err| format!("准备查询所有页面失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(WikiPageRecord {
                title: row.get(0)?,
                path: row.get(1)?,
                summary: row.get(2)?,
                updated_at: row.get(3)?,
                score: row.get::<_, f64>(4).unwrap_or(0.0),
            })
        })
        .map_err(|err| format!("查询所有页面失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取所有页面结果失败: {}", err))
}

/// 计算两个向量（f32数组）的余弦相似度。
/// 兜底方案：在 Rust 中进行计算，通过 rusqlite 自定义函数注册到 SQLite。
pub fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f64 {
    let dot: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
    let mag1: f32 = v1.iter().map(|a| a * a).sum::<f32>().sqrt();
    let mag2: f32 = v2.iter().map(|a| a * a).sum::<f32>().sqrt();
    if mag1 == 0.0 || mag2 == 0.0 {
        0.0
    } else {
        (dot / (mag1 * mag2)) as f64
    }
}

// ─── Research Tasks ───────────────────────────────────────────────────────────

/// 创建一条 research_tasks 记录，返回新记录 id。
pub fn db_create_research_task(conn: &Connection, topic: &str, now: &str) -> Result<i64, String> {
    conn.execute(
        r#"
        INSERT INTO research_tasks (topic, status, sub_queries, web_results_count, created_at, updated_at)
        VALUES (?1, 'queued', '[]', 0, ?2, ?2)
        "#,
        params![topic, now],
    )
    .map_err(|err| format!("写入 research_tasks 失败: {}", err))?;
    Ok(conn.last_insert_rowid())
}

/// 查询最近 100 条 research_tasks，按 created_at DESC。
pub fn db_list_research_tasks(conn: &Connection) -> Result<Vec<crate::models::ResearchTaskItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, topic, status, sub_queries, web_results_count, saved_path, error, created_at, updated_at
            FROM research_tasks
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )
        .map_err(|err| format!("准备查询 research_tasks 失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| {
            let sub_queries_json: String = row.get(3)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                sub_queries_json,
                row.get::<_, i32>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(|err| format!("查询 research_tasks 失败: {}", err))?;

    let mut result = Vec::new();
    for row in rows {
        let (id, topic, status, sub_queries_json, web_results_count, saved_path, error, created_at, updated_at) =
            row.map_err(|err| format!("读取 research_tasks 失败: {}", err))?;
        let sub_queries: Vec<String> =
            serde_json::from_str(&sub_queries_json).unwrap_or_default();
        result.push(crate::models::ResearchTaskItem {
            id,
            topic,
            status,
            sub_queries,
            web_results_count,
            saved_path,
            error,
            created_at,
            updated_at,
        });
    }
    Ok(result)
}

/// 更新 research_tasks 记录状态及相关字段。
#[allow(clippy::too_many_arguments)]
pub fn db_update_research_task(
    conn: &Connection,
    id: i64,
    status: &str,
    sub_queries_json: &str,
    web_results_count: i32,
    saved_path: Option<&str>,
    error: Option<&str>,
    now: &str,
) -> Result<(), String> {
    conn.execute(
        r#"
        UPDATE research_tasks
        SET status = ?1, sub_queries = ?2, web_results_count = ?3, saved_path = ?4, error = ?5, updated_at = ?6
        WHERE id = ?7
        "#,
        params![status, sub_queries_json, web_results_count, saved_path, error, now, id],
    )
    .map_err(|err| format!("更新 research_tasks 失败: {}", err))?;
    Ok(())
}

/// 按 id 查询单条 research_tasks 记录。
pub fn db_get_research_task(conn: &Connection, id: i64) -> Result<Option<crate::models::ResearchTaskItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, topic, status, sub_queries, web_results_count, saved_path, error, created_at, updated_at
            FROM research_tasks
            WHERE id = ?1
            "#,
        )
        .map_err(|err| format!("准备查询 research_tasks 失败: {}", err))?;
    match stmt.query_row(params![id], |row| {
        let sub_queries_json: String = row.get(3)?;
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            sub_queries_json,
            row.get::<_, i32>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
        ))
    }) {
        Ok((id, topic, status, sub_queries_json, web_results_count, saved_path, error, created_at, updated_at)) => {
            let sub_queries: Vec<String> =
                serde_json::from_str(&sub_queries_json).unwrap_or_default();
            Ok(Some(crate::models::ResearchTaskItem {
                id,
                topic,
                status,
                sub_queries,
                web_results_count,
                saved_path,
                error,
                created_at,
                updated_at,
            }))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(format!("查询 research_tasks 失败: {}", err)),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
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
        let dir =
            std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    #[test]
    fn upsert_and_search_fts_page_paths_work() {
        let dir = make_temp_dir("llm-wiki-db-fts");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        let wiki_path = dir.join("rust-note.md");
        upsert_fts_page(
            &db_path,
            &wiki_path,
            "rust-note",
            "Rust backend and tauri app integration",
        )
        .expect("写入 fts 索引失败");

        let results =
            search_fts_page_paths(&db_path, &[String::from("rust")], 5).expect("执行 fts 查询失败");
        assert!(results
            .iter()
            .any(|path| path == &wiki_path.to_string_lossy().to_string()));
    }

    #[test]
    fn upsert_and_list_embeddings_work() {
        let dir = make_temp_dir("llm-wiki-db-embeddings");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        upsert_embedding(&db_path, "wiki/a.md", &[0.1, 0.2, 0.3]).expect("写入 embedding 失败");
        upsert_embedding(&db_path, "wiki/b.md", &[0.4, 0.5, 0.6]).expect("写入 embedding 失败");

        let items = list_embeddings(&db_path, 10).expect("读取 embedding 列表失败");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].page_path, "wiki/a.md");
        assert_eq!(items[1].page_path, "wiki/b.md");
        assert_eq!(items[0].embedding.len(), 3);
        assert!((items[0].embedding[0] - 0.1).abs() < f32::EPSILON);
        assert!((items[1].embedding[2] - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn decode_embedding_blob_rejects_invalid_length() {
        let err = decode_embedding_blob(&[1, 2, 3]).expect_err("应返回非法长度错误");
        assert!(err.contains("4 的倍数"));
    }

    #[test]
    fn replace_citations_for_page_replaces_rows() {
        let dir = make_temp_dir("llm-wiki-db-citations");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let page_path = dir.join("wiki").join("query.md");

        replace_citations_for_page(
            &db_path,
            &page_path,
            &[CitationInput {
                cited_page_path: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
                score: 3,
                excerpt: "excerpt-1",
            }],
            "1",
        )
        .expect("第一次写入引用失败");
        replace_citations_for_page(
            &db_path,
            &page_path,
            &[CitationInput {
                cited_page_path: "E:\\llm-wiki\\vault\\wiki\\ingest-2.md",
                score: 2,
                excerpt: "excerpt-2",
            }],
            "2",
        )
        .expect("第二次写入引用失败");

        let citations = list_citations(&db_path).expect("读取引用失败");
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].page_path, page_path.to_string_lossy());
        assert_eq!(
            citations[0].cited_page_path,
            "E:\\llm-wiki\\vault\\wiki\\ingest-2.md"
        );
        assert_eq!(citations[0].score, 2);
        assert_eq!(citations[0].excerpt, "excerpt-2");
    }

    #[test]
    fn find_existing_ingest_by_hash_returns_latest_page() {
        let dir = make_temp_dir("llm-wiki-db-dedup");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-1", "source-1", "raw-1.md", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "t1", "wiki-a.md", "s1", "1", "1"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "t2", "wiki-b.md", "s2", "2", "2"],
        )
        .expect("写入 wiki_pages 失败");

        let existing = find_existing_ingest_by_hash(&db_path, "hash-1")
            .expect("查询重复导入失败")
            .expect("应返回结果");
        assert_eq!(existing.raw_path, "raw-1.md");
        assert_eq!(existing.wiki_path, "wiki-b.md");
    }

    #[test]
    fn list_citations_for_page_returns_page_rows() {
        let dir = make_temp_dir("llm-wiki-db-citations-for-page");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["wiki/a.md", "wiki/b.md", 3_i64, "excerpt-a", "1"],
        )
        .expect("写入 citations 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["wiki/a.md", "wiki/c.md", 2_i64, "excerpt-b", "2"],
        )
        .expect("写入 citations 失败");

        let rows = list_citations_for_page(&db_path, "wiki/a.md").expect("读取 citations 失败");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].cited_page_path, "wiki/b.md");
        assert_eq!(rows[1].cited_page_path, "wiki/c.md");
    }

    #[test]
    fn search_wiki_pages_matches_keyword_and_orders_by_updated_at() {
        let dir = make_temp_dir("llm-wiki-db-search-pages");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-s", "source-s", "raw-s.md", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "Rust 页面", "wiki/rust-a.md", "rust summary a", "1", "2"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "Tauri 页面", "wiki/tauri.md", "desktop app", "1", "3"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "Rust 进阶", "wiki/rust-b.md", "rust summary b", "1", "4"],
        )
        .expect("写入 wiki_pages 失败");

        let matches = search_wiki_pages(&db_path, "rust", 10).expect("搜索 wiki 页面失败");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].title, "Rust 进阶");
        assert_eq!(matches[1].title, "Rust 页面");
        assert!(matches[0].score >= 0.0);

        let all = search_wiki_pages(&db_path, "   ", 10).expect("读取最近页面失败");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].title, "Rust 进阶");
    }

    #[test]
    fn lint_patch_events_insert_and_list_recent_work() {
        let dir = make_temp_dir("llm-wiki-db-lint-patch-events");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        insert_lint_patch_event(
            &db_path,
            "ORPHAN_WIKI_PAGE",
            Some("wiki/a.md"),
            true,
            "已将页面加入 index.md",
            "1",
        )
        .expect("第一次写入 lint_patch_events 失败");
        insert_lint_patch_event(
            &db_path,
            "MISSING_INDEX_ENTRY",
            None,
            false,
            "index.md 中已存在该页面引用，未重复写入",
            "2",
        )
        .expect("第二次写入 lint_patch_events 失败");

        let events =
            list_recent_lint_patch_events(&db_path, 10).expect("读取 lint_patch_events 失败");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].issue_code, "MISSING_INDEX_ENTRY");
        assert_eq!(events[0].path, None);
        assert!(!events[0].applied);
        assert_eq!(events[1].issue_code, "ORPHAN_WIKI_PAGE");
        assert_eq!(events[1].path.as_deref(), Some("wiki/a.md"));
        assert!(events[1].applied);
    }

    #[test]
    fn outbox_append_export_and_ack_work() {
        let dir = make_temp_dir("llm-wiki-db-outbox");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        let first_id = append_outbox_event(
            &db_path,
            "ingest_completed",
            r#"{"wiki_path":"wiki/a.md"}"#,
            "100",
        )
        .expect("写入第一条 outbox 失败");
        let second_id = append_outbox_event(
            &db_path,
            "query_answered",
            r#"{"question":"rust"}"#,
            "200",
        )
        .expect("写入第二条 outbox 失败");
        assert!(second_id > first_id);

        let all = list_outbox_events_from_id(&db_path, 0, 20).expect("读取 outbox 全量失败");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].event_type, "ingest_completed");
        assert_eq!(all[1].event_type, "query_answered");
        assert!(all[0].processed_at.is_none());

        let incremental =
            list_outbox_events_from_id(&db_path, first_id, 20).expect("读取 outbox 增量失败");
        assert_eq!(incremental.len(), 1);
        assert_eq!(incremental[0].id, second_id);

        let acked = ack_outbox_events(&db_path, first_id, "test-consumer", "300")
            .expect("执行 outbox ack 失败");
        assert_eq!(acked, 1);

        let acked_again = ack_outbox_events(&db_path, first_id, "test-consumer", "301")
            .expect("重复 outbox ack 失败");
        assert_eq!(acked_again, 0);

        let after_ack =
            list_outbox_events_from_id(&db_path, 0, 20).expect("读取 ack 后 outbox 失败");
        assert_eq!(after_ack.len(), 2);
        assert_eq!(after_ack[0].processed_at.as_deref(), Some("300"));
        assert_eq!(after_ack[0].consumer_tag.as_deref(), Some("test-consumer"));
        assert!(after_ack[1].processed_at.is_none());
    }

    #[test]
    fn delete_wiki_page_from_db_removes_all_related_records() {
        let dir = make_temp_dir("llm-wiki-db-delete-wiki-page");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        // 插入 sources 记录（wiki_pages.source_id 有外键约束）
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-target", "src/target.md", "raw/target.md", "1"],
        )
        .expect("写入 sources 失败");
        let source_id_target = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-other", "src/other.md", "raw/other.md", "2"],
        )
        .expect("写入 sources 失败");
        let source_id_other = conn.last_insert_rowid();

        // 插入 wiki_pages 记录
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id_target, "目标页面", "wiki/target.md", "摘要", "1", "1"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id_other, "其他页面", "wiki/other.md", "摘要2", "2", "2"],
        )
        .expect("写入 wiki_pages 失败");

        // 插入 citations 记录（target 作为引用方和被引用方各一条）
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["wiki/target.md", "wiki/other.md", 1, "引用1", "1"],
        )
        .expect("写入 citations 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["wiki/other.md", "wiki/target.md", 2, "引用2", "2"],
        )
        .expect("写入 citations 失败");

        // 插入 fts_pages 记录
        upsert_fts_page(&db_path, std::path::Path::new("wiki/target.md"), "目标页面", "内容")
            .expect("写入 fts_pages 失败");
        upsert_fts_page(&db_path, std::path::Path::new("wiki/other.md"), "其他页面", "内容2")
            .expect("写入 fts_pages 失败");

        // 执行删除
        delete_wiki_page_from_db(&db_path, std::path::Path::new("wiki/target.md"))
            .expect("删除 wiki 页面失败");

        // 验证 wiki_pages：target 已删，other 保留
        let page_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiki_pages", [], |r| r.get(0))
            .expect("查询 wiki_pages 失败");
        assert_eq!(page_count, 1);
        let remaining_path: String = conn
            .query_row("SELECT path FROM wiki_pages", [], |r| r.get(0))
            .expect("查询 wiki_pages 失败");
        assert_eq!(remaining_path, "wiki/other.md");

        // 验证 citations：两条均删除（target 参与的全部清除）
        let citation_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM citations", [], |r| r.get(0))
            .expect("查询 citations 失败");
        assert_eq!(citation_count, 0);

        // 验证 fts_pages：target 已删，other 保留
        let fts_paths = search_fts_page_paths(&db_path, &["其他".to_string()], 10)
            .expect("FTS 查询失败");
        assert!(!fts_paths.is_empty());
        let fts_target = search_fts_page_paths(&db_path, &["目标".to_string()], 10)
            .expect("FTS 查询失败");
        assert!(fts_target.is_empty());
    }

    #[test]
    fn rename_wiki_page_in_db_updates_all_tables() {
        let dir = make_temp_dir("llm-wiki-db-rename-wiki-page");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        // 插入 sources
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-a", "src/a.md", "raw/a.md", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();

        // 插入 wiki_pages：old + other
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "旧标题", "wiki/old.md", "摘要", "1", "1"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-b", "src/b.md", "raw/b.md", "2"],
        )
        .expect("写入 sources 失败");
        let source_id_b = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id_b, "其他页面", "wiki/other.md", "摘要2", "2", "2"],
        )
        .expect("写入 wiki_pages 失败");

        // 插入 citations（old 作为引用方和被引用方各一条）
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["wiki/old.md", "wiki/other.md", 1, "excerpt1", "1"],
        )
        .expect("写入 citations 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["wiki/other.md", "wiki/old.md", 2, "excerpt2", "2"],
        )
        .expect("写入 citations 失败");

        // 插入 fts_pages
        upsert_fts_page(&db_path, std::path::Path::new("wiki/old.md"), "旧标题", "旧内容")
            .expect("写入 fts_pages 失败");

        // 执行重命名
        rename_wiki_page_in_db(
            &db_path,
            std::path::Path::new("wiki/old.md"),
            std::path::Path::new("wiki/new.md"),
            "新标题",
            "新内容",
        )
        .expect("重命名数据库记录失败");

        // 验证 wiki_pages
        let new_path: String = conn
            .query_row(
                "SELECT path FROM wiki_pages WHERE path = 'wiki/new.md'",
                [],
                |r| r.get(0),
            )
            .expect("未找到新路径");
        assert_eq!(new_path, "wiki/new.md");
        let old_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM wiki_pages WHERE path = 'wiki/old.md'",
                [],
                |r| r.get(0),
            )
            .expect("查询失败");
        assert_eq!(old_count, 0);

        // 验证 citations
        let c1_page: String = conn
            .query_row(
                "SELECT page_path FROM citations WHERE cited_page_path = 'wiki/other.md'",
                [],
                |r| r.get(0),
            )
            .expect("未找到 citations 记录");
        assert_eq!(c1_page, "wiki/new.md");

        let c2_cited: String = conn
            .query_row(
                "SELECT cited_page_path FROM citations WHERE page_path = 'wiki/other.md'",
                [],
                |r| r.get(0),
            )
            .expect("未找到 citations 记录");
        assert_eq!(c2_cited, "wiki/new.md");

        // 验证 fts_pages：新路径可搜到，旧路径搜不到
        let found_new = search_fts_page_paths(&db_path, &["新内容".to_string()], 10)
            .expect("FTS 查询失败");
        assert!(!found_new.is_empty());
        let found_old = search_fts_page_paths(&db_path, &["旧内容".to_string()], 10)
            .expect("FTS 查询失败");
        assert!(found_old.is_empty());
    }

    #[test]
    fn save_ask_history_deduplicates_and_refreshes_timestamp() {
        let dir = make_temp_dir("llm-wiki-db-ask-history-dedup");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        save_ask_history(&db_path, "  Rust FTS5 是什么  ", "100").expect("首次写入失败");
        save_ask_history(&db_path, "rust   fts5   是什么", "200").expect("重复写入失败");

        let rows = list_ask_history(&db_path, 10).expect("读取 Ask 历史失败");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].question, "rust   fts5   是什么");
        assert_eq!(rows[0].created_at, "200");
    }

    #[test]
    fn save_ask_history_prunes_to_max_entries() {
        let dir = make_temp_dir("llm-wiki-db-ask-history-prune");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        let total = ASK_HISTORY_MAX_ENTRIES + 15;
        for i in 0..total {
            let question = format!("question-{i}");
            let created_at = format!("{i}");
            save_ask_history(&db_path, &question, &created_at).expect("写入 Ask 历史失败");
        }

        let rows = list_ask_history(&db_path, total).expect("读取 Ask 历史失败");
        assert_eq!(rows.len(), ASK_HISTORY_MAX_ENTRIES);
        assert_eq!(rows[0].question, format!("question-{}", total - 1));
        assert_eq!(rows.last().map(|r| r.question.as_str()), Some("question-15"));
    }

    #[test]
    fn clear_ask_history_removes_all_rows() {
        let dir = make_temp_dir("llm-wiki-db-ask-history-clear");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        save_ask_history(&db_path, "问题 1", "100").expect("写入历史失败");
        save_ask_history(&db_path, "问题 2", "200").expect("写入历史失败");

        let removed = clear_ask_history(&db_path).expect("清空历史失败");
        assert_eq!(removed, 2);

        let rows = list_ask_history(&db_path, 10).expect("读取 Ask 历史失败");
        assert!(rows.is_empty());
    }

    #[test]
    fn enqueue_and_list_ingest_queue_works() {
        let dir = make_temp_dir("llm-wiki-db-ingest-queue-enqueue");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        let id = db_enqueue_ingest(&conn, "file", "/tmp/a.md", "2026-01-01T00:00:00Z")
            .expect("入队失败");
        assert!(id > 0, "返回的 id 应大于 0");

        let items = db_list_ingest_queue(&conn).expect("读取队列失败");
        assert_eq!(items.len(), 1, "应有 1 条记录");
        assert_eq!(items[0].source_type, "file");
        assert_eq!(items[0].source_path, "/tmp/a.md");
        assert_eq!(items[0].status, "queued");
        assert!(items[0].error.is_none());
    }

    #[test]
    fn reset_stale_running_resets_to_queued() {
        let dir = make_temp_dir("llm-wiki-db-ingest-queue-reset");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        // 入队一条，再手动设为 running
        let id = db_enqueue_ingest(&conn, "url", "https://example.com", "2026-01-01T00:00:00Z")
            .expect("入队失败");
        conn.execute(
            "UPDATE ingest_queue_items SET status = 'running' WHERE id = ?1",
            params![id],
        )
        .expect("手动更新为 running 失败");

        // 重置 stale running → queued
        let count = db_reset_stale_running(&conn, "2026-01-01T00:01:00Z")
            .expect("重置 stale running 失败");
        assert_eq!(count, 1, "应重置 1 条记录");

        // 再次 list，验证 status 已恢复 queued
        let items = db_list_ingest_queue(&conn).expect("读取队列失败");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, "queued");
    }

    #[test]
    fn cancel_ingest_item_sets_cancelled() {
        let dir = make_temp_dir("llm-wiki-db-ingest-queue-cancel");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        let id = db_enqueue_ingest(&conn, "markdown", "/tmp/note.md", "2026-01-01T00:00:00Z")
            .expect("入队失败");

        db_update_ingest_queue_status(&conn, id, "cancelled", None, "2026-01-01T00:02:00Z")
            .expect("更新状态为 cancelled 失败");

        let items = db_list_ingest_queue(&conn).expect("读取队列失败");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, "cancelled");
        assert!(items[0].error.is_none());
    }
}
