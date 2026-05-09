use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{AskTurn, LintPatchEventItem};
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

/// Wiki 页面历史快照记录。
#[derive(Debug, Clone)]
pub struct WikiPageHistoryRecord {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub content_hash: String,
    pub checksum: String,
    pub created_at: String,
    pub prev_content: Option<String>,
}

/// Ask 历史记录。
#[derive(Debug, Clone)]
pub struct AskHistoryRecord {
    pub id: i64,
    pub question: String,
    pub created_at: String,
}

/// Ask 会话摘要记录。
#[derive(Debug, Clone)]
pub struct AskSessionRecord {
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub turn_count: usize,
    pub last_turn_role: Option<String>,
    pub last_turn_content: Option<String>,
}

/// Ask 会话单轮记录。
#[derive(Debug, Clone)]
pub struct AskSessionTurnRecord {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub citations_json: String,
    pub meta_json: Option<String>,
}

/// Ask 跨会话检索命中记录。
#[derive(Debug, Clone)]
pub struct AskSessionSearchHitRecord {
    pub session_id: String,
    pub session_title: String,
    pub turn_id: i64,
    pub role: String,
    pub snippet: String,
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

/// Agent Run 记录（用于列表展示）。
#[derive(Debug, Clone)]
pub struct AgentRunRecord {
    pub id: i64,
    pub topic: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
    pub archived_at: Option<String>,
}

/// Agent Run 事件记录（用于时间线展示）。
#[derive(Debug, Clone)]
pub struct AgentRunEventRecord {
    pub id: i64,
    pub run_id: i64,
    pub level: String,
    pub message: String,
    pub created_at: String,
}

/// Agent Draft 记录（用于草稿列表与审批）。
#[derive(Debug, Clone)]
pub struct AgentDraftRecord {
    pub id: i64,
    pub run_id: i64,
    pub title: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Agent 记忆数据库记录（H2：记忆增强）。
#[derive(Debug, Clone)]
pub struct AgentMemoryRecord {
    pub id: i64,
    pub run_id: Option<i64>,
    pub memory_key: String,
    pub memory_value: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Agent 技能模板数据库记录（H3：技能化）。
#[derive(Debug, Clone)]
pub struct AgentSkillRecord {
    pub id: i64,
    pub skill_key: String,
    pub prompt_template: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// 页面 Embedding 记录。
#[derive(Debug, Clone)]
pub struct PageEmbeddingRecord {
    pub page_path: String,
    pub embedding: Vec<f32>,
}

/// 将页面路径及其向量（二进制 BLOB 格式）插入或更新到 `page_embeddings` 表中。
pub fn upsert_embedding(db_path: &Path, page_path: &str, embedding: &[f32]) -> Result<(), String> {
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
        let (page_path, blob) =
            item.map_err(|err| format!("读取 page_embeddings 结果失败: {}", err))?;
        let embedding = decode_embedding_blob(&blob)?;
        result.push(PageEmbeddingRecord {
            page_path,
            embedding,
        });
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
    let tokens: Vec<String> = normalized
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
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

/// 更新 wiki_pages 表中指定页面的标题（不涉及文件重命名）。
pub fn update_wiki_page_title(
    db_path: &Path,
    page_path: &Path,
    new_title: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    conn.execute(
        "UPDATE wiki_pages SET title = ?1 WHERE path = ?2",
        params![new_title, page_path.to_string_lossy()],
    )
    .map_err(|err| format!("更新 wiki_pages.title 失败: {}", err))?;
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

/// 保存页面覆盖前的内容快照。
pub fn insert_wiki_page_history(
    db_path: &Path,
    page_path: &Path,
    title: &str,
    content_hash: &str,
    prev_content: &str,
    created_at: &str,
) -> Result<i64, String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.execute(
        r#"
        INSERT INTO wiki_page_history (
            path,
            title,
            content_hash,
            checksum,
            prev_content,
            created_at
        ) VALUES (?1, ?2, ?3, ?3, ?4, ?5)
        "#,
        params![
            page_path.to_string_lossy(),
            title,
            content_hash,
            prev_content,
            created_at
        ],
    )
    .map_err(|err| format!("写入 wiki_page_history 失败: {}", err))?;
    Ok(conn.last_insert_rowid())
}

/// 按页面路径读取历史快照列表，不返回正文以避免列表过大。
pub fn list_wiki_page_history(
    db_path: &Path,
    page_path: &Path,
    limit: usize,
) -> Result<Vec<WikiPageHistoryRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, path, title, content_hash, checksum, created_at
            FROM wiki_page_history
            WHERE path = ?1
            ORDER BY CAST(created_at AS INTEGER) DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备查询 wiki_page_history 失败: {}", err))?;
    let rows = stmt
        .query_map(params![page_path.to_string_lossy(), limit as i64], |row| {
            Ok(WikiPageHistoryRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                title: row.get(2)?,
                content_hash: row.get(3)?,
                checksum: row.get(4)?,
                created_at: row.get(5)?,
                prev_content: None,
            })
        })
        .map_err(|err| format!("读取 wiki_page_history 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 wiki_page_history 失败: {}", err))
}

/// 按 ID 读取单条历史快照，包含覆盖前正文。
pub fn get_wiki_page_history_entry(
    db_path: &Path,
    id: i64,
) -> Result<Option<WikiPageHistoryRecord>, String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, path, title, content_hash, checksum, created_at, prev_content
            FROM wiki_page_history
            WHERE id = ?1
            "#,
        )
        .map_err(|err| format!("准备读取 wiki_page_history 失败: {}", err))?;

    match stmt.query_row(params![id], |row| {
        Ok(WikiPageHistoryRecord {
            id: row.get(0)?,
            path: row.get(1)?,
            title: row.get(2)?,
            content_hash: row.get(3)?,
            checksum: row.get(4)?,
            created_at: row.get(5)?,
            prev_content: Some(row.get(6)?),
        })
    }) {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(format!("读取 wiki_page_history 失败: {}", err)),
    }
}

/// 从数据库中删除 Wiki 页面的所有相关记录（wiki_pages / citations / fts_pages / history）。
pub fn delete_wiki_page_from_db(db_path: &Path, page_path: &Path) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    let path_str = page_path.to_string_lossy();

    // 删除 FTS 索引
    conn.execute("DELETE FROM fts_pages WHERE path = ?1", params![path_str])
        .map_err(|err| format!("删除 fts_pages 记录失败: {}", err))?;

    // 删除引用（作为引用方或被引用方均清理）
    conn.execute(
        "DELETE FROM citations WHERE page_path = ?1 OR cited_page_path = ?1",
        params![path_str],
    )
    .map_err(|err| format!("删除 citations 记录失败: {}", err))?;

    // 删除该页历史，避免删除后仍保留不可见快照。
    conn.execute(
        "DELETE FROM wiki_page_history WHERE path = ?1",
        params![path_str],
    )
    .map_err(|err| format!("删除 wiki_page_history 记录失败: {}", err))?;

    // 删除主记录
    conn.execute("DELETE FROM wiki_pages WHERE path = ?1", params![path_str])
        .map_err(|err| format!("删除 wiki_pages 记录失败: {}", err))?;

    Ok(())
}

/// 将数据库中所有引用 old_path 的记录更新为 new_path。
/// 涉及：wiki_pages.path、citations.page_path、citations.cited_page_path、fts_pages.path、history.path。
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

    // 历史快照跟随页面重命名，保证详情页仍能查到旧版本。
    conn.execute(
        "UPDATE wiki_page_history SET path = ?1 WHERE path = ?2",
        params![new_str, old_str],
    )
    .map_err(|err| format!("更新 wiki_page_history.path 失败: {}", err))?;

    // fts_pages：删旧插新
    conn.execute("DELETE FROM fts_pages WHERE path = ?1", params![old_str])
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
        let mut stmt = conn
            .prepare("SELECT cited_page_path FROM citations WHERE page_path = ?1 LIMIT ?2")
            .map_err(|e| format!("准备正向链接查询失败: {}", e))?;
        let rows = stmt
            .query_map(params![path, limit as i64], |row| row.get::<_, String>(0))
            .map_err(|e| format!("执行正向链接查询失败: {}", e))?;
        for r in rows.flatten() {
            result.insert(r);
        }

        // 反向：引用此页面的页面
        let mut stmt = conn
            .prepare("SELECT page_path FROM citations WHERE cited_page_path = ?1 LIMIT ?2")
            .map_err(|e| format!("准备反向链接查询失败: {}", e))?;
        let rows = stmt
            .query_map(params![path, limit as i64], |row| row.get::<_, String>(0))
            .map_err(|e| format!("执行反向链接查询失败: {}", e))?;
        for r in rows.flatten() {
            result.insert(r);
        }

        if result.len() >= limit {
            break;
        }
    }

    // 去掉 from_paths 本身（避免循环引用降低 RRF 效果）
    let from_set: std::collections::HashSet<&String> = from_paths.iter().collect();
    Ok(result
        .into_iter()
        .filter(|p| !from_set.contains(p))
        .take(limit)
        .collect())
}

/// 查找被引用次数最多的页面路径（Citation 热度排序）。
/// 用于 RRF 热度路径。
pub fn query_citation_popular_paths(db_path: &Path, limit: usize) -> Result<Vec<String>, String> {
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

        CREATE TABLE IF NOT EXISTS wiki_page_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL,
            title TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            checksum TEXT NOT NULL,
            prev_content TEXT NOT NULL,
            created_at TEXT NOT NULL
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

        CREATE INDEX IF NOT EXISTS idx_wiki_page_history_path_created_at
            ON wiki_page_history(path, created_at);

        CREATE INDEX IF NOT EXISTS idx_wiki_page_history_created_at
            ON wiki_page_history(created_at);

        CREATE TABLE IF NOT EXISTS ask_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            question TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ask_history_created_at
            ON ask_history(created_at);

        CREATE TABLE IF NOT EXISTS ask_sessions (
            session_id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_ask_sessions_updated_at
            ON ask_sessions(updated_at);

        CREATE TABLE IF NOT EXISTS ask_session_turns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            citations_json TEXT NOT NULL DEFAULT '[]',
            meta_json TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(session_id) REFERENCES ask_sessions(session_id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_ask_session_turns_session_id
            ON ask_session_turns(session_id, id);

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
            depth             INTEGER NOT NULL DEFAULT 1,
            breadth           INTEGER NOT NULL DEFAULT 3,
            saved_path        TEXT,
            error             TEXT,
            created_at        TEXT NOT NULL,
            updated_at        TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS agent_runs (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            topic        TEXT NOT NULL,
            status       TEXT NOT NULL DEFAULT 'running',
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL,
            completed_at TEXT,
            archived_at  TEXT,
            archived_reason TEXT
        );

        CREATE TABLE IF NOT EXISTS agent_run_events (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id      INTEGER NOT NULL,
            level       TEXT NOT NULL,
            message     TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            FOREIGN KEY(run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS agent_drafts (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id       INTEGER NOT NULL,
            title        TEXT NOT NULL DEFAULT '',
            content      TEXT NOT NULL DEFAULT '',
            status       TEXT NOT NULL DEFAULT 'draft',
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL,
            FOREIGN KEY(run_id) REFERENCES agent_runs(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS agent_memories (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id       INTEGER,
            memory_key   TEXT NOT NULL,
            memory_value TEXT NOT NULL,
            created_at   TEXT NOT NULL,
            updated_at   TEXT NOT NULL,
            FOREIGN KEY(run_id) REFERENCES agent_runs(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS agent_skills (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            skill_key       TEXT NOT NULL UNIQUE,
            prompt_template TEXT NOT NULL,
            version         INTEGER NOT NULL DEFAULT 1,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_agent_runs_updated_at
            ON agent_runs(updated_at, id);

        CREATE INDEX IF NOT EXISTS idx_agent_run_events_run_id_id
            ON agent_run_events(run_id, id);

        CREATE INDEX IF NOT EXISTS idx_agent_drafts_run_id_updated_at
            ON agent_drafts(run_id, updated_at, id);

        CREATE INDEX IF NOT EXISTS idx_agent_memories_run_id_updated_at
            ON agent_memories(run_id, updated_at, id);

        CREATE INDEX IF NOT EXISTS idx_agent_skills_updated_at
            ON agent_skills(updated_at, id);

        CREATE TABLE IF NOT EXISTS shell_audit_events (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            command         TEXT NOT NULL,
            working_dir     TEXT NOT NULL DEFAULT '',
            policy_action   TEXT NOT NULL,
            policy_decision TEXT NOT NULL,
            executor        TEXT NOT NULL,
            blocked         INTEGER NOT NULL DEFAULT 0,
            blocked_reason  TEXT,
            exit_code       INTEGER,
            latency_ms      INTEGER,
            session_id      TEXT,
            created_at      TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_shell_audit_created
            ON shell_audit_events(created_at DESC);
        "#,
    )
    .map_err(|err| format!("初始化数据库结构失败: {}", err))?;

    // FTS 为增强能力，初始化失败时不阻断主流程，查询侧会自动降级。
    let _ = conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS fts_pages USING fts5(path UNINDEXED, title, body)",
        [],
    );

    // FTS for ask session turns（触发器自动维护）
    let _ = conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS fts_ask_turns USING fts5(session_id UNINDEXED, content, tokenize=\"unicode61\")",
        [],
    );
    let _ = conn.execute(
        r#"CREATE TRIGGER IF NOT EXISTS trg_ask_turns_fts_insert
    AFTER INSERT ON ask_session_turns BEGIN
        INSERT INTO fts_ask_turns(rowid, session_id, content)
        VALUES (NEW.id, NEW.session_id, NEW.content);
    END"#,
        [],
    );
    let _ = conn.execute(
        r#"CREATE TRIGGER IF NOT EXISTS trg_ask_turns_fts_delete
    AFTER DELETE ON ask_session_turns BEGIN
        DELETE FROM fts_ask_turns WHERE rowid = OLD.id;
    END"#,
        [],
    );

    ensure_ask_history_quality(conn)?;
    ensure_ask_session_turns_quality(conn)?;
    ensure_agent_runs_quality(conn)?;
    ensure_ingest_queue_quality(conn)?;

    // agent_chat 表与内置工具种子（幂等）
    crate::agent_chat::db::ensure_schema(conn)?;
    crate::agent_chat::db::seed_builtin_tools(conn)?;

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
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|err| format!("读取 ask_history 历史数据失败: {}", err))?;

        for row in rows {
            let (id, question) =
                row.map_err(|err| format!("读取 ask_history 历史数据失败: {}", err))?;
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

/// Ask 会话轮次表质量保障：补齐结构化元信息字段。
fn ensure_ask_session_turns_quality(conn: &Connection) -> Result<(), String> {
    let mut has_citations_json = false;
    let mut has_meta_json = false;
    let mut stmt = conn
        .prepare("PRAGMA table_info(ask_session_turns)")
        .map_err(|err| format!("读取 ask_session_turns 表结构失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| format!("读取 ask_session_turns 字段失败: {}", err))?;
    for row in rows {
        let name = row.map_err(|err| format!("读取 ask_session_turns 字段失败: {}", err))?;
        if name == "citations_json" {
            has_citations_json = true;
        } else if name == "meta_json" {
            has_meta_json = true;
        }
    }

    if !has_citations_json {
        conn.execute(
            "ALTER TABLE ask_session_turns ADD COLUMN citations_json TEXT NOT NULL DEFAULT '[]'",
            [],
        )
        .map_err(|err| format!("补齐 ask_session_turns.citations_json 失败: {}", err))?;
    }
    if !has_meta_json {
        conn.execute(
            "ALTER TABLE ask_session_turns ADD COLUMN meta_json TEXT",
            [],
        )
        .map_err(|err| format!("补齐 ask_session_turns.meta_json 失败: {}", err))?;
    }

    // 回填历史数据到 FTS（升级迁移：fts 为空但 turns 有数据时）
    let fts_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM fts_ask_turns", [], |row| row.get(0))
        .unwrap_or(0);
    if fts_count == 0 {
        let turns_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM ask_session_turns", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        if turns_count > 0 {
            let _ = conn.execute(
                "INSERT INTO fts_ask_turns(rowid, session_id, content) SELECT id, session_id, content FROM ask_session_turns",
                [],
            );
        }
    }

    Ok(())
}

/// Agent run 表质量保障：补齐归档字段。
fn ensure_agent_runs_quality(conn: &Connection) -> Result<(), String> {
    let mut has_archived_at = false;
    let mut has_archived_reason = false;
    let mut stmt = conn
        .prepare("PRAGMA table_info(agent_runs)")
        .map_err(|err| format!("读取 agent_runs 表结构失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| format!("读取 agent_runs 字段失败: {}", err))?;
    for row in rows {
        let name = row.map_err(|err| format!("读取 agent_runs 字段失败: {}", err))?;
        if name == "archived_at" {
            has_archived_at = true;
        } else if name == "archived_reason" {
            has_archived_reason = true;
        }
    }
    if !has_archived_at {
        conn.execute("ALTER TABLE agent_runs ADD COLUMN archived_at TEXT", [])
            .map_err(|err| format!("补齐 agent_runs.archived_at 失败: {}", err))?;
    }
    if !has_archived_reason {
        conn.execute("ALTER TABLE agent_runs ADD COLUMN archived_reason TEXT", [])
            .map_err(|err| format!("补齐 agent_runs.archived_reason 失败: {}", err))?;
    }
    Ok(())
}

/// ingest_queue_items 表质量保障：补齐 started_at / completed_at / retry_count 字段。
fn ensure_ingest_queue_quality(conn: &Connection) -> Result<(), String> {
    let mut has_started_at = false;
    let mut has_completed_at = false;
    let mut has_retry_count = false;
    let mut stmt = conn
        .prepare("PRAGMA table_info(ingest_queue_items)")
        .map_err(|err| format!("读取 ingest_queue_items 表结构失败: {}", err))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|err| format!("读取 ingest_queue_items 字段失败: {}", err))?;
    for row in rows {
        let name = row.map_err(|err| format!("读取 ingest_queue_items 字段失败: {}", err))?;
        match name.as_str() {
            "started_at" => has_started_at = true,
            "completed_at" => has_completed_at = true,
            "retry_count" => has_retry_count = true,
            _ => {}
        }
    }
    if !has_started_at {
        conn.execute("ALTER TABLE ingest_queue_items ADD COLUMN started_at TEXT", [])
            .map_err(|err| format!("补齐 ingest_queue_items.started_at 失败: {}", err))?;
    }
    if !has_completed_at {
        conn.execute("ALTER TABLE ingest_queue_items ADD COLUMN completed_at TEXT", [])
            .map_err(|err| format!("补齐 ingest_queue_items.completed_at 失败: {}", err))?;
    }
    if !has_retry_count {
        conn.execute(
            "ALTER TABLE ingest_queue_items ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|err| format!("补齐 ingest_queue_items.retry_count 失败: {}", err))?;
    }
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

fn normalize_ask_session_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        "新对话".to_string()
    } else {
        trimmed.chars().take(60).collect()
    }
}

fn build_auto_session_title(content: &str) -> Option<String> {
    let first_line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let mut title: String = first_line.chars().take(36).collect();
    if first_line.chars().count() > 36 {
        title.push('…');
    }
    let normalized = normalize_ask_session_title(&title);
    if normalized == "新对话" {
        None
    } else {
        Some(normalized)
    }
}

/// 创建会话（若已存在则仅刷新 updated_at）。
pub fn create_ask_session(
    db_path: &Path,
    session_id: &str,
    title: &str,
    now: &str,
) -> Result<(), String> {
    let normalized_session_id = session_id.trim();
    if normalized_session_id.is_empty() {
        return Err("session_id 不能为空".to_string());
    }
    let normalized_title = normalize_ask_session_title(title);

    let shared_conn = get_connection(db_path)?;
    let conn = shared_conn.lock().unwrap();
    conn.execute(
        r#"
        INSERT INTO ask_sessions (session_id, title, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?3)
        ON CONFLICT(session_id) DO UPDATE SET
            updated_at = excluded.updated_at
        "#,
        params![normalized_session_id, normalized_title, now],
    )
    .map_err(|err| format!("创建 ask_session 失败: {}", err))?;
    Ok(())
}

/// 追加会话单轮，返回新增 turn id。
pub fn append_ask_session_turn(
    db_path: &Path,
    session_id: &str,
    role: &str,
    content: &str,
    created_at: &str,
    citations_json: Option<&str>,
    meta_json: Option<&str>,
) -> Result<i64, String> {
    let normalized_session_id = session_id.trim();
    if normalized_session_id.is_empty() {
        return Err("session_id 不能为空".to_string());
    }
    let normalized_role = role.trim();
    if normalized_role != "user" && normalized_role != "assistant" {
        return Err("role 必须是 user 或 assistant".to_string());
    }
    let normalized_content = content.trim();
    if normalized_content.is_empty() {
        return Err("content 不能为空".to_string());
    }
    let normalized_citations_json = citations_json.unwrap_or("[]").trim();
    let normalized_citations_json = if normalized_citations_json.is_empty() {
        "[]"
    } else {
        normalized_citations_json
    };
    let normalized_meta_json = meta_json.map(str::trim).filter(|value| !value.is_empty());

    let shared_conn = get_connection(db_path)?;
    let mut conn = shared_conn.lock().unwrap();
    let tx = conn
        .transaction()
        .map_err(|err| format!("开启 ask_session_turn 事务失败: {}", err))?;

    tx.execute(
        r#"
        INSERT INTO ask_sessions (session_id, title, created_at, updated_at)
        VALUES (?1, '新对话', ?2, ?2)
        ON CONFLICT(session_id) DO UPDATE SET
            updated_at = excluded.updated_at
        "#,
        params![normalized_session_id, created_at],
    )
    .map_err(|err| format!("确保 ask_session 存在失败: {}", err))?;

    tx.execute(
        r#"
        INSERT INTO ask_session_turns (session_id, role, content, citations_json, meta_json, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            normalized_session_id,
            normalized_role,
            normalized_content,
            normalized_citations_json,
            normalized_meta_json,
            created_at
        ],
    )
    .map_err(|err| format!("写入 ask_session_turn 失败: {}", err))?;
    let turn_id = tx.last_insert_rowid();

    if normalized_role == "user" {
        if let Some(auto_title) = build_auto_session_title(normalized_content) {
            tx.execute(
                r#"
                UPDATE ask_sessions
                SET title = ?1
                WHERE session_id = ?2
                  AND (title = '新对话' OR trim(title) = '')
                "#,
                params![auto_title, normalized_session_id],
            )
            .map_err(|err| format!("自动更新 ask_session 标题失败: {}", err))?;
        }
    }

    tx.execute(
        "UPDATE ask_sessions SET updated_at = ?1 WHERE session_id = ?2",
        params![created_at, normalized_session_id],
    )
    .map_err(|err| format!("刷新 ask_session 更新时间失败: {}", err))?;

    tx.commit()
        .map_err(|err| format!("提交 ask_session_turn 事务失败: {}", err))?;
    Ok(turn_id)
}

/// 删除一条会话单轮（用于取消时回滚刚追加的 user turn）。
pub fn delete_ask_session_turn_by_id(db_path: &Path, turn_id: i64) -> Result<(), String> {
    let shared_conn = get_connection(db_path)?;
    let conn = shared_conn.lock().unwrap();
    conn.execute(
        "DELETE FROM ask_session_turns WHERE id = ?1",
        params![turn_id],
    )
    .map_err(|err| format!("删除 ask_session_turn 失败: {}", err))?;
    Ok(())
}

/// 查询会话列表（按最近更新时间倒序）。
pub fn list_ask_sessions(db_path: &Path, limit: usize) -> Result<Vec<AskSessionRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            WITH latest_turn_ids AS (
                SELECT session_id, MAX(id) AS max_id
                FROM ask_session_turns
                GROUP BY session_id
            )
            SELECT
                s.session_id,
                s.title,
                s.created_at,
                s.updated_at,
                COUNT(t.id) AS turn_count,
                lt.role       AS last_turn_role,
                lt.content    AS last_turn_content
            FROM ask_sessions s
            LEFT JOIN ask_session_turns t   ON t.session_id = s.session_id
            LEFT JOIN latest_turn_ids lti   ON lti.session_id = s.session_id
            LEFT JOIN ask_session_turns lt  ON lt.id = lti.max_id
            GROUP BY s.session_id, s.title, s.created_at, s.updated_at
            ORDER BY CAST(s.updated_at AS INTEGER) DESC, s.session_id DESC
            LIMIT ?1
            "#,
        )
        .map_err(|err| format!("准备查询 ask_sessions 失败: {}", err))?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(AskSessionRecord {
                session_id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                turn_count: row.get::<_, i64>(4)? as usize,
                last_turn_role: row.get(5)?,
                last_turn_content: row.get(6)?,
            })
        })
        .map_err(|err| format!("执行查询 ask_sessions 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 ask_sessions 失败: {}", err))
}

/// 查询指定会话全部轮次（按时间正序）。
pub fn list_ask_session_turns(
    db_path: &Path,
    session_id: &str,
    limit: usize,
) -> Result<Vec<AskSessionTurnRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let normalized_session_id = session_id.trim();
    if normalized_session_id.is_empty() {
        return Ok(Vec::new());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, session_id, role, content, created_at, citations_json, meta_json
            FROM ask_session_turns
            WHERE session_id = ?1
            ORDER BY id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备查询 ask_session_turns 失败: {}", err))?;
    let rows = stmt
        .query_map(params![normalized_session_id, limit as i64], |row| {
            Ok(AskSessionTurnRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                citations_json: row.get(5)?,
                meta_json: row.get(6)?,
            })
        })
        .map_err(|err| format!("执行查询 ask_session_turns 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 ask_session_turns 失败: {}", err))
}

/// 查询指定会话最近 N 轮（按时间正序）用于构建上下文。
pub fn list_recent_ask_session_turns(
    db_path: &Path,
    session_id: &str,
    limit: usize,
) -> Result<Vec<AskTurn>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let normalized_session_id = session_id.trim();
    if normalized_session_id.is_empty() {
        return Ok(Vec::new());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT role, content
            FROM ask_session_turns
            WHERE session_id = ?1
            ORDER BY id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备查询最近 ask_session_turns 失败: {}", err))?;
    let rows = stmt
        .query_map(params![normalized_session_id, limit as i64], |row| {
            Ok(AskTurn {
                role: row.get(0)?,
                content: row.get(1)?,
            })
        })
        .map_err(|err| format!("执行查询最近 ask_session_turns 失败: {}", err))?;

    let mut turns = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取最近 ask_session_turns 失败: {}", err))?;
    turns.reverse();
    Ok(turns)
}

/// 跨会话检索轮次内容（按时间倒序）。
pub fn search_ask_session_turns(
    db_path: &Path,
    keyword: &str,
    limit: usize,
) -> Result<Vec<AskSessionSearchHitRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let normalized_keyword = keyword.trim();
    if normalized_keyword.is_empty() {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;

    // FTS5 需要转义特殊字符；用双引号包裹以做精确短语搜索
    let fts_term = format!("\"{}\"", normalized_keyword.replace('"', "\"\""));
    let like_pattern = format!("%{}%", normalized_keyword.to_lowercase());

    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                t.session_id,
                s.title,
                t.id,
                t.role,
                t.content,
                t.created_at
            FROM ask_session_turns t
            INNER JOIN ask_sessions s
                ON s.session_id = t.session_id
            WHERE t.id IN (
                SELECT rowid FROM fts_ask_turns WHERE fts_ask_turns MATCH ?1
            )
               OR lower(s.title) LIKE ?2
            ORDER BY CAST(t.created_at AS INTEGER) DESC, t.id DESC
            LIMIT ?3
            "#,
        )
        .map_err(|err| format!("准备检索 ask_session_turns 失败: {}", err))?;
    let rows = stmt
        .query_map(params![fts_term, like_pattern, limit as i64], |row| {
            Ok(AskSessionSearchHitRecord {
                session_id: row.get(0)?,
                session_title: row.get(1)?,
                turn_id: row.get(2)?,
                role: row.get(3)?,
                snippet: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|err| format!("执行检索 ask_session_turns 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 ask_session_turns 检索结果失败: {}", err))
}

/// 重命名会话标题。
pub fn rename_ask_session(
    db_path: &Path,
    session_id: &str,
    title: &str,
    updated_at: &str,
) -> Result<(), String> {
    let normalized_session_id = session_id.trim();
    if normalized_session_id.is_empty() {
        return Err("session_id 不能为空".to_string());
    }
    let normalized_title = normalize_ask_session_title(title);

    let shared_conn = get_connection(db_path)?;
    let conn = shared_conn.lock().unwrap();
    let affected = conn
        .execute(
            "UPDATE ask_sessions SET title = ?1, updated_at = ?2 WHERE session_id = ?3",
            params![normalized_title, updated_at, normalized_session_id],
        )
        .map_err(|err| format!("重命名 ask_session 失败: {}", err))?;
    if affected == 0 {
        return Err("会话不存在".to_string());
    }
    Ok(())
}

/// 清空指定会话全部轮次（会话实体保留）。
pub fn clear_ask_session_turns(
    db_path: &Path,
    session_id: &str,
    updated_at: &str,
) -> Result<(), String> {
    let normalized_session_id = session_id.trim();
    if normalized_session_id.is_empty() {
        return Ok(());
    }
    let shared_conn = get_connection(db_path)?;
    let mut conn = shared_conn.lock().unwrap();
    let tx = conn
        .transaction()
        .map_err(|err| format!("开启 clear_ask_session_turns 事务失败: {}", err))?;
    tx.execute(
        "DELETE FROM ask_session_turns WHERE session_id = ?1",
        params![normalized_session_id],
    )
    .map_err(|err| format!("清空 ask_session_turns 失败: {}", err))?;
    tx.execute(
        "UPDATE ask_sessions SET updated_at = ?1 WHERE session_id = ?2",
        params![updated_at, normalized_session_id],
    )
    .map_err(|err| format!("更新 ask_session 更新时间失败: {}", err))?;
    tx.commit()
        .map_err(|err| format!("提交 clear_ask_session_turns 事务失败: {}", err))?;
    Ok(())
}

/// 删除会话（同时删除关联轮次，依赖外键级联）。
pub fn delete_ask_session(db_path: &Path, session_id: &str) -> Result<usize, String> {
    let normalized_session_id = session_id.trim();
    if normalized_session_id.is_empty() {
        return Ok(0);
    }

    let shared_conn = get_connection(db_path)?;
    let conn = shared_conn.lock().unwrap();
    let affected = conn
        .execute(
            "DELETE FROM ask_sessions WHERE session_id = ?1",
            params![normalized_session_id],
        )
        .map_err(|err| format!("删除 ask_session 失败: {}", err))?;
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

/// 创建一条 Agent Run，默认 status=running。
pub fn start_agent_run(db_path: &Path, topic: &str, now: &str) -> Result<i64, String> {
    let normalized_topic = topic.trim();
    if normalized_topic.is_empty() {
        return Err("topic 不能为空".to_string());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.execute(
        r#"
        INSERT INTO agent_runs (topic, status, created_at, updated_at)
        VALUES (?1, 'running', ?2, ?2)
        "#,
        params![normalized_topic, now],
    )
    .map_err(|err| format!("写入 agent_runs 失败: {}", err))?;
    Ok(conn.last_insert_rowid())
}

/// 追加一条 Agent Run 事件。
pub fn append_agent_run_event(
    db_path: &Path,
    run_id: i64,
    level: &str,
    message: &str,
    now: &str,
) -> Result<(), String> {
    let normalized_level = level.trim();
    let normalized_message = message.trim();
    if normalized_level.is_empty() {
        return Err("level 不能为空".to_string());
    }
    if normalized_message.is_empty() {
        return Err("message 不能为空".to_string());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.execute(
        r#"
        INSERT INTO agent_run_events (run_id, level, message, created_at)
        VALUES (?1, ?2, ?3, ?4)
        "#,
        params![run_id, normalized_level, normalized_message, now],
    )
    .map_err(|err| format!("写入 agent_run_events 失败: {}", err))?;
    conn.execute(
        "UPDATE agent_runs SET updated_at = ?1 WHERE id = ?2",
        params![now, run_id],
    )
    .map_err(|err| format!("更新 agent_runs 时间失败: {}", err))?;
    Ok(())
}

/// 按更新时间倒序列出 Agent Runs。
pub fn list_agent_runs(
    db_path: &Path,
    limit: usize,
    include_archived: bool,
) -> Result<Vec<AgentRunRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, topic, status, created_at, updated_at, completed_at, archived_at
            FROM agent_runs
            WHERE (?1 = 1 OR archived_at IS NULL)
            ORDER BY CAST(updated_at AS INTEGER) DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备查询 agent_runs 失败: {}", err))?;
    let rows = stmt
        .query_map(
            params![if include_archived { 1 } else { 0 }, limit as i64],
            |row| {
                Ok(AgentRunRecord {
                    id: row.get(0)?,
                    topic: row.get(1)?,
                    status: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    completed_at: row.get(5)?,
                    archived_at: row.get(6)?,
                })
            },
        )
        .map_err(|err| format!("执行查询 agent_runs 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 agent_runs 失败: {}", err))
}

/// 按 ID 查询单条 Agent Run（含已归档）。
pub fn get_agent_run_by_id(db_path: &Path, run_id: i64) -> Result<Option<AgentRunRecord>, String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, topic, status, created_at, updated_at, completed_at, archived_at
            FROM agent_runs
            WHERE id = ?1
            LIMIT 1
            "#,
        )
        .map_err(|err| format!("准备查询 agent_run 失败: {}", err))?;
    let mut rows = stmt
        .query_map(params![run_id], |row| {
            Ok(AgentRunRecord {
                id: row.get(0)?,
                topic: row.get(1)?,
                status: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                completed_at: row.get(5)?,
                archived_at: row.get(6)?,
            })
        })
        .map_err(|err| format!("执行查询 agent_run 失败: {}", err))?;
    match rows.next() {
        Some(Ok(record)) => Ok(Some(record)),
        Some(Err(err)) => Err(format!("读取 agent_run 失败: {}", err)),
        None => Ok(None),
    }
}

/// 归档 Agent Run（软删除）。
pub fn archive_agent_run(
    db_path: &Path,
    run_id: i64,
    archived_reason: Option<&str>,
    now: &str,
) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let reason = archived_reason
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let affected = conn
        .execute(
            r#"
            UPDATE agent_runs
            SET archived_at = ?1,
                archived_reason = ?2,
                updated_at = ?1
            WHERE id = ?3 AND archived_at IS NULL
            "#,
            params![now, reason, run_id],
        )
        .map_err(|err| format!("归档 agent_runs 失败: {}", err))?;
    if affected == 0 {
        return Err(format!("Agent Run 不存在或已归档: {}", run_id));
    }
    conn.execute(
        r#"
        INSERT INTO agent_run_events (run_id, level, message, created_at)
        VALUES (?1, 'info', ?2, ?3)
        "#,
        params![run_id, "系统状态变更：run 已归档（软删除）", now],
    )
    .map_err(|err| format!("写入归档事件失败: {}", err))?;
    Ok(())
}

/// 恢复已归档 Agent Run。
pub fn restore_agent_run(db_path: &Path, run_id: i64, now: &str) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let affected = conn
        .execute(
            r#"
            UPDATE agent_runs
            SET archived_at = NULL,
                archived_reason = NULL,
                updated_at = ?1
            WHERE id = ?2 AND archived_at IS NOT NULL
            "#,
            params![now, run_id],
        )
        .map_err(|err| format!("恢复 agent_runs 失败: {}", err))?;
    if affected == 0 {
        return Err(format!("Agent Run 不存在或未归档: {}", run_id));
    }
    conn.execute(
        r#"
        INSERT INTO agent_run_events (run_id, level, message, created_at)
        VALUES (?1, 'info', ?2, ?3)
        "#,
        params![run_id, "系统状态变更：run 已恢复", now],
    )
    .map_err(|err| format!("写入恢复事件失败: {}", err))?;
    Ok(())
}

/// 按 id 正序列出指定 Agent Run 的事件。
pub fn list_agent_run_events(
    db_path: &Path,
    run_id: i64,
    limit: usize,
) -> Result<Vec<AgentRunEventRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, run_id, level, message, created_at
            FROM agent_run_events
            WHERE run_id = ?1
            ORDER BY id ASC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备查询 agent_run_events 失败: {}", err))?;
    let rows = stmt
        .query_map(params![run_id, limit as i64], |row| {
            Ok(AgentRunEventRecord {
                id: row.get(0)?,
                run_id: row.get(1)?,
                level: row.get(2)?,
                message: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|err| format!("执行查询 agent_run_events 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 agent_run_events 失败: {}", err))
}

/// 更新 Agent Run 状态；仅 applied/failed 视为终态并写入完成时间。
pub fn complete_agent_run(
    db_path: &Path,
    run_id: i64,
    status: &str,
    now: &str,
) -> Result<(), String> {
    let normalized_status = status.trim();
    if normalized_status.is_empty() {
        return Err("status 不能为空".to_string());
    }
    let mut conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let tx = conn
        .transaction()
        .map_err(|err| format!("开启结束 run 事务失败: {}", err))?;
    let completed_at = if normalized_status == "running" || normalized_status == "reviewing" {
        None
    } else {
        Some(now)
    };
    let affected = tx
        .execute(
            r#"
            UPDATE agent_runs
            SET status = ?1, updated_at = ?2, completed_at = ?3
            WHERE id = ?4
            "#,
            params![normalized_status, now, completed_at, run_id],
        )
        .map_err(|err| format!("更新 agent_runs 状态失败: {}", err))?;
    if affected == 0 {
        return Err(format!("Agent Run 不存在: {}", run_id));
    }
    // 结束 run 时自动补一条系统事件，避免事件面板空白。
    let system_message = format!("系统状态变更：{} -> {}", run_id, normalized_status);
    tx.execute(
        r#"
        INSERT INTO agent_run_events (run_id, level, message, created_at)
        VALUES (?1, 'info', ?2, ?3)
        "#,
        params![run_id, system_message, now],
    )
    .map_err(|err| format!("写入 run 系统事件失败: {}", err))?;

    tx.commit()
        .map_err(|err| format!("提交结束 run 事务失败: {}", err))?;
    Ok(())
}

/// 写入一条 Agent Draft 草稿记录。
pub fn create_agent_draft(
    db_path: &Path,
    run_id: i64,
    title: &str,
    content: &str,
    status: &str,
    now: &str,
) -> Result<AgentDraftRecord, String> {
    let normalized_title = title.trim();
    let normalized_content = content.trim();
    let normalized_status = status.trim();
    if normalized_content.is_empty() {
        return Err("draft content 不能为空".to_string());
    }
    if normalized_status.is_empty() {
        return Err("draft status 不能为空".to_string());
    }

    let conn = open_connection(db_path)?;
    init_schema(&conn)?;

    let run_exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM agent_runs WHERE id = ?1",
            params![run_id],
            |row| row.get(0),
        )
        .map_err(|err| format!("查询 agent_runs 失败: {}", err))?;
    if run_exists == 0 {
        return Err(format!("Agent Run 不存在: {}", run_id));
    }

    conn.execute(
        r#"
        INSERT INTO agent_drafts (run_id, title, content, status, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?5)
        "#,
        params![
            run_id,
            normalized_title,
            normalized_content,
            normalized_status,
            now
        ],
    )
    .map_err(|err| format!("写入 agent_drafts 失败: {}", err))?;
    let draft_id = conn.last_insert_rowid();

    let draft = conn
        .query_row(
            r#"
            SELECT id, run_id, title, content, status, created_at, updated_at
            FROM agent_drafts
            WHERE id = ?1
            "#,
            params![draft_id],
            |row| {
                Ok(AgentDraftRecord {
                    id: row.get(0)?,
                    run_id: row.get(1)?,
                    title: row.get(2)?,
                    content: row.get(3)?,
                    status: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|err| format!("读取新建 agent_draft 失败: {}", err))?;
    Ok(draft)
}

/// 列出指定 Run 的草稿（按更新时间倒序）。
pub fn list_agent_drafts(
    db_path: &Path,
    run_id: i64,
    limit: usize,
) -> Result<Vec<AgentDraftRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, run_id, title, content, status, created_at, updated_at
            FROM agent_drafts
            WHERE run_id = ?1
            ORDER BY CAST(updated_at AS INTEGER) DESC, id DESC
            LIMIT ?2
            "#,
        )
        .map_err(|err| format!("准备查询 agent_drafts 失败: {}", err))?;
    let rows = stmt
        .query_map(params![run_id, limit as i64], |row| {
            Ok(AgentDraftRecord {
                id: row.get(0)?,
                run_id: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|err| format!("执行查询 agent_drafts 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 agent_drafts 失败: {}", err))
}

/// 读取指定草稿。
pub fn get_agent_draft(db_path: &Path, draft_id: i64) -> Result<Option<AgentDraftRecord>, String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.query_row(
        r#"
        SELECT id, run_id, title, content, status, created_at, updated_at
        FROM agent_drafts
        WHERE id = ?1
        "#,
        params![draft_id],
        |row| {
            Ok(AgentDraftRecord {
                id: row.get(0)?,
                run_id: row.get(1)?,
                title: row.get(2)?,
                content: row.get(3)?,
                status: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(|err| format!("读取 agent_draft 失败: {}", err))
}

/// 更新草稿状态（例如 draft -> applied）。
pub fn update_agent_draft_status(
    db_path: &Path,
    draft_id: i64,
    status: &str,
    now: &str,
) -> Result<(), String> {
    let normalized_status = status.trim();
    if normalized_status.is_empty() {
        return Err("draft status 不能为空".to_string());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let affected = conn
        .execute(
            r#"
            UPDATE agent_drafts
            SET status = ?1, updated_at = ?2
            WHERE id = ?3
            "#,
            params![normalized_status, now, draft_id],
        )
        .map_err(|err| format!("更新 agent_draft 状态失败: {}", err))?;
    if affected == 0 {
        return Err(format!("Agent Draft 不存在: {}", draft_id));
    }
    Ok(())
}

/// 写入或更新 agent 记忆（按 run_id + memory_key upsert）。
pub fn upsert_agent_memory(
    db_path: &Path,
    run_id: Option<i64>,
    key: &str,
    value: &str,
    now: &str,
) -> Result<AgentMemoryRecord, String> {
    let normalized_key = key.trim();
    let normalized_value = value.trim();
    if normalized_key.is_empty() {
        return Err("memory_key 不能为空".to_string());
    }
    if normalized_value.is_empty() {
        return Err("memory_value 不能为空".to_string());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let existing_id: Option<i64> = if let Some(rid) = run_id {
        conn.query_row(
            "SELECT id FROM agent_memories WHERE run_id = ?1 AND memory_key = ?2",
            params![rid, normalized_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("查询 agent_memories 失败: {}", e))?
    } else {
        conn.query_row(
            "SELECT id FROM agent_memories WHERE run_id IS NULL AND memory_key = ?1",
            params![normalized_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("查询 agent_memories 失败: {}", e))?
    };
    let id = if let Some(eid) = existing_id {
        conn.execute(
            "UPDATE agent_memories SET memory_value = ?1, updated_at = ?2 WHERE id = ?3",
            params![normalized_value, now, eid],
        )
        .map_err(|e| format!("更新 agent_memories 失败: {}", e))?;
        eid
    } else {
        conn.execute(
            r#"INSERT INTO agent_memories (run_id, memory_key, memory_value, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?4)"#,
            params![run_id, normalized_key, normalized_value, now],
        )
        .map_err(|e| format!("写入 agent_memories 失败: {}", e))?;
        conn.last_insert_rowid()
    };
    conn.query_row(
        "SELECT id, run_id, memory_key, memory_value, created_at, updated_at FROM agent_memories WHERE id = ?1",
        params![id],
        |row| {
            Ok(AgentMemoryRecord {
                id: row.get(0)?,
                run_id: row.get(1)?,
                memory_key: row.get(2)?,
                memory_value: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .map_err(|e| format!("读取新建 agent_memory 失败: {}", e))
}

/// 列出 agent 记忆（None = 全局记忆，Some(id) = 指定 run 的记忆）。
pub fn list_agent_memories(
    db_path: &Path,
    run_id: Option<i64>,
    limit: usize,
) -> Result<Vec<AgentMemoryRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let to_record = |row: &rusqlite::Row<'_>| {
        Ok(AgentMemoryRecord {
            id: row.get(0)?,
            run_id: row.get(1)?,
            memory_key: row.get(2)?,
            memory_value: row.get(3)?,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    };
    if let Some(rid) = run_id {
        let mut stmt = conn
            .prepare(
                r#"SELECT id, run_id, memory_key, memory_value, created_at, updated_at
                   FROM agent_memories WHERE run_id = ?1
                   ORDER BY CAST(updated_at AS INTEGER) DESC, id DESC LIMIT ?2"#,
            )
            .map_err(|e| format!("准备查询 agent_memories 失败: {}", e))?;
        let mapped = stmt
            .query_map(params![rid, limit as i64], to_record)
            .map_err(|e| format!("执行查询 agent_memories 失败: {}", e))?;
        let rows: Result<Vec<_>, _> = mapped.collect();
        rows.map_err(|e| format!("读取 agent_memories 失败: {}", e))
    } else {
        let mut stmt = conn
            .prepare(
                r#"SELECT id, run_id, memory_key, memory_value, created_at, updated_at
                   FROM agent_memories WHERE run_id IS NULL
                   ORDER BY CAST(updated_at AS INTEGER) DESC, id DESC LIMIT ?1"#,
            )
            .map_err(|e| format!("准备查询 agent_memories 失败: {}", e))?;
        let mapped = stmt
            .query_map(params![limit as i64], to_record)
            .map_err(|e| format!("执行查询 agent_memories 失败: {}", e))?;
        let rows: Result<Vec<_>, _> = mapped.collect();
        rows.map_err(|e| format!("读取 agent_memories 失败: {}", e))
    }
}

/// 删除单条 agent 记忆。
pub fn delete_agent_memory(db_path: &Path, id: i64) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let affected = conn
        .execute("DELETE FROM agent_memories WHERE id = ?1", params![id])
        .map_err(|e| format!("删除 agent_memory 失败: {}", e))?;
    if affected == 0 {
        return Err(format!("agent_memory 不存在: {}", id));
    }
    Ok(())
}

/// 批量替换 agent 记忆（AAAK-lite 压缩后写回）。
pub fn bulk_replace_agent_memories(
    db_path: &Path,
    run_id: Option<i64>,
    entries: &[(String, String)],
    now: &str,
) -> Result<(), String> {
    let mut conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let tx = conn
        .transaction()
        .map_err(|e| format!("开启 bulk_replace_memories 事务失败: {}", e))?;
    if let Some(rid) = run_id {
        tx.execute("DELETE FROM agent_memories WHERE run_id = ?1", params![rid])
            .map_err(|e| format!("清空 agent_memories 失败: {}", e))?;
    } else {
        tx.execute("DELETE FROM agent_memories WHERE run_id IS NULL", [])
            .map_err(|e| format!("清空全局 agent_memories 失败: {}", e))?;
    }
    for (key, value) in entries {
        let k = key.trim();
        let v = value.trim();
        if k.is_empty() || v.is_empty() {
            continue;
        }
        tx.execute(
            r#"INSERT INTO agent_memories (run_id, memory_key, memory_value, created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?4)"#,
            params![run_id, k, v, now],
        )
        .map_err(|e| format!("写入压缩记忆失败: {}", e))?;
    }
    tx.commit()
        .map_err(|e| format!("提交 bulk_replace_memories 事务失败: {}", e))?;
    Ok(())
}

/// 写入或更新 Agent 技能模板（同 key 更新内容并递增版本号）。
pub fn upsert_agent_skill(
    db_path: &Path,
    skill_key: &str,
    prompt_template: &str,
    now: &str,
) -> Result<AgentSkillRecord, String> {
    let normalized_key = skill_key.trim();
    let normalized_prompt = prompt_template.trim();
    if normalized_key.is_empty() {
        return Err("skill_key 不能为空".to_string());
    }
    if normalized_prompt.is_empty() {
        return Err("prompt_template 不能为空".to_string());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;

    let existing: Option<(i64, i64, String)> = conn
        .query_row(
            "SELECT id, version, created_at FROM agent_skills WHERE skill_key = ?1",
            params![normalized_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|e| format!("查询 agent_skills 失败: {}", e))?;

    let id = if let Some((existing_id, existing_version, _)) = existing {
        let next_version = existing_version.saturating_add(1);
        conn.execute(
            r#"
            UPDATE agent_skills
            SET prompt_template = ?1, version = ?2, updated_at = ?3
            WHERE id = ?4
            "#,
            params![normalized_prompt, next_version, now, existing_id],
        )
        .map_err(|e| format!("更新 agent_skills 失败: {}", e))?;
        existing_id
    } else {
        conn.execute(
            r#"
            INSERT INTO agent_skills (skill_key, prompt_template, version, created_at, updated_at)
            VALUES (?1, ?2, 1, ?3, ?3)
            "#,
            params![normalized_key, normalized_prompt, now],
        )
        .map_err(|e| format!("写入 agent_skills 失败: {}", e))?;
        conn.last_insert_rowid()
    };

    conn.query_row(
        r#"
        SELECT id, skill_key, prompt_template, version, created_at, updated_at
        FROM agent_skills
        WHERE id = ?1
        "#,
        params![id],
        |row| {
            Ok(AgentSkillRecord {
                id: row.get(0)?,
                skill_key: row.get(1)?,
                prompt_template: row.get(2)?,
                version: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .map_err(|e| format!("读取 agent_skill 失败: {}", e))
}

/// 列出 Agent 技能模板（按更新时间倒序）。
pub fn list_agent_skills(db_path: &Path, limit: usize) -> Result<Vec<AgentSkillRecord>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, skill_key, prompt_template, version, created_at, updated_at
            FROM agent_skills
            ORDER BY CAST(updated_at AS INTEGER) DESC, id DESC
            LIMIT ?1
            "#,
        )
        .map_err(|e| format!("准备查询 agent_skills 失败: {}", e))?;
    let mapped = stmt
        .query_map(params![limit as i64], |row| {
            Ok(AgentSkillRecord {
                id: row.get(0)?,
                skill_key: row.get(1)?,
                prompt_template: row.get(2)?,
                version: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("执行查询 agent_skills 失败: {}", e))?;
    let rows: Result<Vec<_>, _> = mapped.collect();
    rows.map_err(|e| format!("读取 agent_skills 失败: {}", e))
}

/// 按 skill_key 读取单条 Agent 技能模板。
pub fn get_agent_skill_by_key(
    db_path: &Path,
    skill_key: &str,
) -> Result<Option<AgentSkillRecord>, String> {
    let normalized_key = skill_key.trim();
    if normalized_key.is_empty() {
        return Ok(None);
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.query_row(
        r#"
        SELECT id, skill_key, prompt_template, version, created_at, updated_at
        FROM agent_skills
        WHERE skill_key = ?1
        "#,
        params![normalized_key],
        |row| {
            Ok(AgentSkillRecord {
                id: row.get(0)?,
                skill_key: row.get(1)?,
                prompt_template: row.get(2)?,
                version: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("读取 agent_skill 失败: {}", e))
}

/// 删除单条 Agent 技能模板。
pub fn delete_agent_skill(db_path: &Path, id: i64) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let affected = conn
        .execute("DELETE FROM agent_skills WHERE id = ?1", params![id])
        .map_err(|e| format!("删除 agent_skill 失败: {}", e))?;
    if affected == 0 {
        return Err(format!("agent_skill 不存在: {}", id));
    }
    Ok(())
}

/// 插入一条 queued 记录，返回新 id。
pub fn db_enqueue_ingest(
    conn: &Connection,
    source_type: &str,
    source_path: &str,
    now: &str,
) -> Result<i64, String> {
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
pub fn db_list_ingest_queue(
    conn: &Connection,
) -> Result<Vec<crate::models::IngestQueueItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, source_type, source_path, status, error, created_at, updated_at,
                   started_at, completed_at, COALESCE(retry_count, 0)
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
                started_at: row.get(7)?,
                completed_at: row.get(8)?,
                retry_count: row.get(9)?,
            })
        })
        .map_err(|err| format!("查询 ingest_queue_items 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 ingest_queue_items 失败: {}", err))
}

/// 取最旧一条 queued 记录。
#[allow(dead_code)]
pub fn db_get_next_queued_item(
    conn: &Connection,
) -> Result<Option<crate::models::IngestQueueItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, source_type, source_path, status, error, created_at, updated_at,
                   started_at, completed_at, COALESCE(retry_count, 0)
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
            started_at: row.get(7)?,
            completed_at: row.get(8)?,
            retry_count: row.get(9)?,
        })
    }) {
        Ok(item) => Ok(Some(item)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(format!("查询 ingest_queue_items 失败: {}", err)),
    }
}

/// 原子性认领下一个待处理 item（queued → running），返回被认领的 item。
pub fn db_claim_next_ingest_queue_item(
    conn: &Connection,
    now: &str,
) -> Result<Option<crate::models::IngestQueueItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            UPDATE ingest_queue_items
            SET status = 'running', started_at = ?1, updated_at = ?1
            WHERE id = (
                SELECT id FROM ingest_queue_items
                WHERE status = 'queued'
                ORDER BY created_at ASC, id ASC
                LIMIT 1
            )
            RETURNING id, source_type, source_path, status, error, created_at, updated_at,
                      started_at, completed_at, COALESCE(retry_count, 0)
            "#,
        )
        .map_err(|err| format!("准备认领 ingest_queue_items 失败: {}", err))?;
    match stmt.query_row(params![now], |row| {
        Ok(crate::models::IngestQueueItem {
            id: row.get(0)?,
            source_type: row.get(1)?,
            source_path: row.get(2)?,
            status: row.get(3)?,
            error: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            started_at: row.get(7)?,
            completed_at: row.get(8)?,
            retry_count: row.get(9)?,
        })
    }) {
        Ok(item) => Ok(Some(item)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(format!("认领 ingest_queue_items 失败: {}", err)),
    }
}

/// 标记 ingest item 处理完成（running → done）。
pub fn db_mark_ingest_queue_item_done(
    conn: &Connection,
    id: i64,
    completed_at: &str,
) -> Result<(), String> {
    conn.execute(
        r#"
        UPDATE ingest_queue_items
        SET status = 'done', completed_at = ?1, updated_at = ?1
        WHERE id = ?2
        "#,
        params![completed_at, id],
    )
    .map_err(|err| format!("标记 ingest_queue_items done 失败: {}", err))?;
    Ok(())
}

/// 标记 ingest item 处理失败（running → failed），递增 retry_count。
pub fn db_mark_ingest_queue_item_failed(
    conn: &Connection,
    id: i64,
    error: &str,
    completed_at: &str,
) -> Result<(), String> {
    conn.execute(
        r#"
        UPDATE ingest_queue_items
        SET status = 'failed', error = ?1, completed_at = ?2, updated_at = ?2,
            retry_count = COALESCE(retry_count, 0) + 1
        WHERE id = ?3
        "#,
        params![error, completed_at, id],
    )
    .map_err(|err| format!("标记 ingest_queue_items failed 失败: {}", err))?;
    Ok(())
}

/// 将 retry_count < max_retries 的 failed item 重置为 queued（app 重启时调用），返回影响行数。
pub fn db_requeue_restartable_items(
    conn: &Connection,
    max_retries: i64,
    now: &str,
) -> Result<i64, String> {
    let affected = conn
        .execute(
            r#"
            UPDATE ingest_queue_items
            SET status = 'queued', error = NULL, started_at = NULL, completed_at = NULL,
                updated_at = ?1
            WHERE status = 'failed' AND COALESCE(retry_count, 0) < ?2
            "#,
            params![now, max_retries],
        )
        .map_err(|err| format!("重置可重试 ingest_queue_items 失败: {}", err))?;
    Ok(affected as i64)
}

/// 更新 ingest_queue_items 的 status + error + updated_at。
pub fn db_update_ingest_queue_status(
    conn: &Connection,
    id: i64,
    status: &str,
    error: Option<&str>,
    now: &str,
) -> Result<(), String> {
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

/// 永久删除一条 ingest 队列记录（仅允许 failed / cancelled 状态）。
pub fn db_delete_ingest_queue_item(conn: &Connection, id: i64) -> Result<(), String> {
    let affected = conn
        .execute(
            "DELETE FROM ingest_queue_items WHERE id = ?1 AND status IN ('failed', 'cancelled', 'done')",
            params![id],
        )
        .map_err(|e| format!("删除 ingest_queue_items 失败: {}", e))?;
    if affected == 0 {
        return Err("任务不存在或状态不允许删除（仅 failed/cancelled/done 可删）".to_string());
    }
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
pub fn db_create_research_task(
    conn: &Connection,
    topic: &str,
    depth: i32,
    breadth: i32,
    now: &str,
) -> Result<i64, String> {
    conn.execute(
        r#"
        INSERT INTO research_tasks (topic, depth, breadth, status, sub_queries, web_results_count, created_at, updated_at)
        VALUES (?1, ?2, ?3, 'queued', '[]', 0, ?4, ?4)
        "#,
        params![topic, depth, breadth, now],
    )
    .map_err(|err| format!("写入 research_tasks 失败: {}", err))?;
    Ok(conn.last_insert_rowid())
}

/// 查询最近 100 条 research_tasks，按 created_at DESC。
pub fn db_list_research_tasks(
    conn: &Connection,
) -> Result<Vec<crate::models::ResearchTaskItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, topic, status, sub_queries, web_results_count, depth, breadth, saved_path, error, created_at, updated_at
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
                row.get::<_, i32>(5)?,
                row.get::<_, i32>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(|err| format!("查询 research_tasks 失败: {}", err))?;

    let mut result = Vec::new();
    for row in rows {
        let (
            id,
            topic,
            status,
            sub_queries_json,
            web_results_count,
            depth,
            breadth,
            saved_path,
            error,
            created_at,
            updated_at,
        ) = row.map_err(|err| format!("读取 research_tasks 失败: {}", err))?;
        let sub_queries: Vec<String> = serde_json::from_str(&sub_queries_json).unwrap_or_default();
        result.push(crate::models::ResearchTaskItem {
            id,
            topic,
            status,
            sub_queries,
            web_results_count,
            depth,
            breadth,
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

/// 仅更新任务状态为 cancelled，不重置其他字段。
/// 幂等：已 done/failed/cancelled 的任务不受影响。
pub fn db_cancel_research_task(conn: &Connection, id: i64, now: &str) -> Result<(), String> {
    conn.execute(
        r#"UPDATE research_tasks SET status = 'cancelled', updated_at = ?1
           WHERE id = ?2 AND status NOT IN ('done', 'failed', 'cancelled')"#,
        params![now, id],
    )
    .map_err(|err| format!("取消 research_tasks 失败: {}", err))?;
    Ok(())
}

/// 删除指定 research_tasks 记录。
pub fn db_delete_research_task(conn: &Connection, id: i64) -> Result<(), String> {
    let changed = conn
        .execute("DELETE FROM research_tasks WHERE id = ?1", params![id])
        .map_err(|err| format!("删除 research_tasks 失败: {}", err))?;
    if changed == 0 {
        return Err(format!("研究任务不存在: {}", id));
    }
    Ok(())
}

/// 按 id 查询单条 research_tasks 记录（供未来命令扩展使用）。
#[allow(dead_code)]
pub fn db_get_research_task(
    conn: &Connection,
    id: i64,
) -> Result<Option<crate::models::ResearchTaskItem>, String> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, topic, status, sub_queries, web_results_count, depth, breadth, saved_path, error, created_at, updated_at
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
            row.get::<_, i32>(5)?,
            row.get::<_, i32>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
        ))
    }) {
        Ok((
            id,
            topic,
            status,
            sub_queries_json,
            web_results_count,
            depth,
            breadth,
            saved_path,
            error,
            created_at,
            updated_at,
        )) => {
            let sub_queries: Vec<String> =
                serde_json::from_str(&sub_queries_json).unwrap_or_default();
            Ok(Some(crate::models::ResearchTaskItem {
                id,
                topic,
                status,
                sub_queries,
                web_results_count,
                depth,
                breadth,
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

/// 一次性获取 Vault 知识库统计数据。
pub fn get_vault_stats_from_db(
    db_path: &Path,
    now_ms: i64,
) -> Result<crate::models::VaultStats, String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;

    let ms_7d = now_ms - 7 * 24 * 3600 * 1000_i64;
    let ms_30d = now_ms - 30 * 24 * 3600 * 1000_i64;

    let total_pages: usize = conn
        .query_row("SELECT COUNT(*) FROM wiki_pages", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0) as usize;

    let pages_last_7_days: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM wiki_pages WHERE CAST(updated_at AS INTEGER) > ?1",
            params![ms_7d],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;

    let pages_last_30_days: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM wiki_pages WHERE CAST(updated_at AS INTEGER) > ?1",
            params![ms_30d],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;

    let orphan_pages: usize = conn
        .query_row(
            r#"SELECT COUNT(*) FROM wiki_pages
               WHERE path NOT IN (
                   SELECT DISTINCT cited_page_path FROM citations
                   WHERE cited_page_path IS NOT NULL AND cited_page_path != ''
               )"#,
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;

    let total_citations: usize = conn
        .query_row("SELECT COUNT(*) FROM citations", [], |r| r.get::<_, i64>(0))
        .unwrap_or(0) as usize;

    // 被引用最多的页面（TOP 10）
    let mut top_stmt = conn
        .prepare(
            r#"SELECT c.cited_page_path,
                      COALESCE(w.title, c.cited_page_path) AS title,
                      COUNT(*) AS cnt
               FROM citations c
               LEFT JOIN wiki_pages w ON w.path = c.cited_page_path
               WHERE c.cited_page_path IS NOT NULL AND c.cited_page_path != ''
               GROUP BY c.cited_page_path
               ORDER BY cnt DESC
               LIMIT 10"#,
        )
        .map_err(|e| format!("准备 top_cited 查询失败: {}", e))?;
    let top_cited_pages = top_stmt
        .query_map([], |row| {
            Ok(crate::models::CitedPageStat {
                path: row.get(0)?,
                title: row.get(1)?,
                citation_count: row.get::<_, i64>(2)? as usize,
            })
        })
        .map_err(|e| format!("执行 top_cited 查询失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    // 摄入来源分布（来自 ingest_queue_items）
    let mut src_stmt = conn
        .prepare(
            r#"SELECT source_type, COUNT(*) AS cnt
               FROM ingest_queue_items
               GROUP BY source_type
               ORDER BY cnt DESC"#,
        )
        .map_err(|e| format!("准备 source_counts 查询失败: {}", e))?;
    let ingest_source_counts = src_stmt
        .query_map([], |row| {
            Ok(crate::models::IngestSourceCount {
                source_type: row.get(0)?,
                count: row.get::<_, i64>(1)? as usize,
            })
        })
        .map_err(|e| format!("执行 source_counts 查询失败: {}", e))?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

    Ok(crate::models::VaultStats {
        total_pages,
        pages_last_7_days,
        pages_last_30_days,
        orphan_pages,
        total_citations,
        top_cited_pages,
        ingest_source_counts,
    })
}

// ─── Shell 审计日志（H6-P3） ───────────────────────────────────────────────────

/// 写入一条 Shell 审计事件，返回新行 id。
pub fn insert_shell_audit_event(
    db_path: &Path,
    event: &crate::models::ShellAuditEvent,
) -> Result<i64, String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    conn.execute(
        r#"
        INSERT INTO shell_audit_events (
            command, working_dir, policy_action, policy_decision,
            executor, blocked, blocked_reason, exit_code, latency_ms,
            session_id, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            event.command,
            event.working_dir,
            event.policy_action,
            event.policy_decision,
            event.executor,
            if event.blocked { 1_i64 } else { 0_i64 },
            event.blocked_reason,
            event.exit_code,
            event.latency_ms,
            event.session_id,
            event.created_at,
        ],
    )
    .map_err(|err| format!("写入 shell_audit_events 失败: {}", err))?;
    Ok(conn.last_insert_rowid())
}

/// 读取最近的 Shell 审计事件（按创建时间倒序）。
pub fn list_shell_audit_events(
    db_path: &Path,
    limit: i64,
) -> Result<Vec<crate::models::ShellAuditEvent>, String> {
    if limit <= 0 {
        return Ok(Vec::new());
    }
    let conn = open_connection(db_path)?;
    init_schema(&conn)?;
    let mut stmt = conn
        .prepare(
            r#"
            SELECT id, command, working_dir, policy_action, policy_decision,
                   executor, blocked, blocked_reason, exit_code, latency_ms,
                   session_id, created_at
            FROM shell_audit_events
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )
        .map_err(|err| format!("准备查询 shell_audit_events 失败: {}", err))?;
    let rows = stmt
        .query_map(params![limit], |row| {
            Ok(crate::models::ShellAuditEvent {
                id: row.get(0)?,
                command: row.get(1)?,
                working_dir: row.get(2)?,
                policy_action: row.get(3)?,
                policy_decision: row.get(4)?,
                executor: row.get(5)?,
                blocked: row.get::<_, i64>(6)? != 0,
                blocked_reason: row.get(7)?,
                exit_code: row.get(8)?,
                latency_ms: row.get(9)?,
                session_id: row.get(10)?,
                created_at: row.get(11)?,
            })
        })
        .map_err(|err| format!("读取 shell_audit_events 失败: {}", err))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 shell_audit_events 结果失败: {}", err))
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
        let second_id =
            append_outbox_event(&db_path, "query_answered", r#"{"question":"rust"}"#, "200")
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
        upsert_fts_page(
            &db_path,
            std::path::Path::new("wiki/target.md"),
            "目标页面",
            "内容",
        )
        .expect("写入 fts_pages 失败");
        upsert_fts_page(
            &db_path,
            std::path::Path::new("wiki/other.md"),
            "其他页面",
            "内容2",
        )
        .expect("写入 fts_pages 失败");
        insert_wiki_page_history(
            &db_path,
            std::path::Path::new("wiki/target.md"),
            "目标页面",
            "history-hash",
            "旧内容",
            "3",
        )
        .expect("写入 wiki_page_history 失败");

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
        let fts_paths =
            search_fts_page_paths(&db_path, &["其他".to_string()], 10).expect("FTS 查询失败");
        assert!(!fts_paths.is_empty());
        let fts_target =
            search_fts_page_paths(&db_path, &["目标".to_string()], 10).expect("FTS 查询失败");
        assert!(fts_target.is_empty());

        let history_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wiki_page_history", [], |r| r.get(0))
            .expect("查询 wiki_page_history 失败");
        assert_eq!(history_count, 0);
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
        upsert_fts_page(
            &db_path,
            std::path::Path::new("wiki/old.md"),
            "旧标题",
            "旧内容",
        )
        .expect("写入 fts_pages 失败");
        insert_wiki_page_history(
            &db_path,
            std::path::Path::new("wiki/old.md"),
            "旧标题",
            "history-hash",
            "旧版本内容",
            "3",
        )
        .expect("写入 wiki_page_history 失败");

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
        let found_new =
            search_fts_page_paths(&db_path, &["新内容".to_string()], 10).expect("FTS 查询失败");
        assert!(!found_new.is_empty());
        let found_old =
            search_fts_page_paths(&db_path, &["旧内容".to_string()], 10).expect("FTS 查询失败");
        assert!(found_old.is_empty());

        let history_path: String = conn
            .query_row("SELECT path FROM wiki_page_history", [], |r| r.get(0))
            .expect("查询 wiki_page_history 失败");
        assert_eq!(history_path, "wiki/new.md");
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
        assert_eq!(
            rows.last().map(|r| r.question.as_str()),
            Some("question-15")
        );
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
    fn ask_sessions_create_append_and_list_turns_work() {
        let dir = make_temp_dir("llm-wiki-db-ask-session-basic");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        create_ask_session(&db_path, "sess-1", "新对话", "100").expect("创建会话失败");
        append_ask_session_turn(
            &db_path,
            "sess-1",
            "user",
            "Rust 是什么？",
            "101",
            None,
            None,
        )
        .expect("写入用户轮失败");
        append_ask_session_turn(
            &db_path,
            "sess-1",
            "assistant",
            "Rust 是系统编程语言。",
            "102",
            Some(
                r#"[{"page_path":"wiki/rust.md","score":12,"excerpt":"Rust 简介","display_path":"rust"}]"#,
            ),
            Some(
                r#"{"mode":"Hybrid","search_strategy":"rrf","answer_strategy":"llm","top_k":3,"matched_pages":1}"#,
            ),
        )
        .expect("写入助手轮失败");

        let sessions = list_ask_sessions(&db_path, 20).expect("读取会话失败");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "sess-1");
        assert_eq!(sessions[0].turn_count, 2);
        assert_eq!(sessions[0].last_turn_role.as_deref(), Some("assistant"));

        let turns = list_ask_session_turns(&db_path, "sess-1", 20).expect("读取会话轮次失败");
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, "user");
        assert_eq!(turns[1].role, "assistant");
        assert_eq!(turns[0].citations_json, "[]");
        assert!(turns[0].meta_json.is_none());
        assert!(turns[1].citations_json.contains("wiki/rust.md"));
        assert!(turns[1]
            .meta_json
            .as_deref()
            .unwrap_or_default()
            .contains("\"search_strategy\":\"rrf\""));
    }

    #[test]
    fn ask_session_rename_and_delete_work() {
        let dir = make_temp_dir("llm-wiki-db-ask-session-manage");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        create_ask_session(&db_path, "sess-2", "新对话", "100").expect("创建会话失败");
        append_ask_session_turn(&db_path, "sess-2", "user", "第一问", "101", None, None)
            .expect("写入用户轮失败");

        rename_ask_session(&db_path, "sess-2", "我的会话", "102").expect("重命名失败");
        let sessions = list_ask_sessions(&db_path, 20).expect("读取会话失败");
        assert_eq!(sessions[0].title, "我的会话");

        let affected = delete_ask_session(&db_path, "sess-2").expect("删除会话失败");
        assert_eq!(affected, 1);
        let sessions_after_delete = list_ask_sessions(&db_path, 20).expect("读取会话失败");
        assert!(sessions_after_delete.is_empty());
        let turns_after_delete =
            list_ask_session_turns(&db_path, "sess-2", 20).expect("读取会话轮次失败");
        assert!(turns_after_delete.is_empty());
    }

    #[test]
    fn search_ask_session_turns_matches_across_sessions() {
        let dir = make_temp_dir("llm-wiki-db-ask-session-search");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        create_ask_session(&db_path, "sess-a", "Rust 会话", "100").expect("创建会话失败");
        append_ask_session_turn(
            &db_path,
            "sess-a",
            "assistant",
            "Rust 的所有权系统可以减少内存错误。",
            "101",
            None,
            None,
        )
        .expect("写入会话 A 失败");

        create_ask_session(&db_path, "sess-b", "SQLite 会话", "100").expect("创建会话失败");
        append_ask_session_turn(
            &db_path,
            "sess-b",
            "assistant",
            "SQLite FTS5 适合本地检索。",
            "102",
            None,
            None,
        )
        .expect("写入会话 B 失败");

        let hits = search_ask_session_turns(&db_path, "rust", 20).expect("检索失败");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "sess-a");
        assert!(hits[0].snippet.contains("Rust"));
    }

    #[test]
    fn search_ask_session_turns_fts_cleaned_on_delete() {
        let dir = make_temp_dir("llm-wiki-db-fts-cleanup");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        create_ask_session(&db_path, "sess-del", "待删会话", "100").expect("创建会话失败");
        append_ask_session_turn(
            &db_path,
            "sess-del",
            "user",
            "这条内容应被清理",
            "101",
            None,
            None,
        )
        .expect("写入轮次失败");

        // 删除会话，FK CASCADE + 触发器应清理 FTS
        delete_ask_session(&db_path, "sess-del").expect("删除会话失败");

        let hits = search_ask_session_turns(&db_path, "这条内容应被清理", 20).expect("检索失败");
        assert!(hits.is_empty(), "删除会话后 FTS 索引应已清理，不应搜到结果");
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
        let count =
            db_reset_stale_running(&conn, "2026-01-01T00:01:00Z").expect("重置 stale running 失败");
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

    #[test]
    fn agent_run_lifecycle_and_event_listing_work() {
        let dir = make_temp_dir("llm-wiki-db-agent-run");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        let run_id = start_agent_run(&db_path, "Rust Agent H0", "100").expect("创建 run 失败");
        append_agent_run_event(&db_path, run_id, "info", "start", "110").expect("写入事件失败");
        append_agent_run_event(&db_path, run_id, "info", "phase-1", "120").expect("写入事件失败");
        complete_agent_run(&db_path, run_id, "applied", "130").expect("结束 run 失败");

        let runs = list_agent_runs(&db_path, 10, false).expect("查询 runs 失败");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, run_id);
        assert_eq!(runs[0].topic, "Rust Agent H0");
        assert_eq!(runs[0].status, "applied");
        assert_eq!(runs[0].completed_at.as_deref(), Some("130"));

        let events = list_agent_run_events(&db_path, run_id, 10).expect("查询事件失败");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].message, "start");
        assert_eq!(events[1].message, "phase-1");
        assert!(
            events[2].message.contains("系统状态变更"),
            "应自动写入系统状态事件"
        );
    }

    #[test]
    fn agent_auxiliary_tables_exist_after_init() {
        let dir = make_temp_dir("llm-wiki-db-agent-tables");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");
        let conn = Connection::open(&db_path).expect("打开数据库失败");

        let names = [
            "agent_runs",
            "agent_run_events",
            "agent_drafts",
            "agent_memories",
            "agent_skills",
        ];
        for name in names {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    params![name],
                    |row| row.get(0),
                )
                .expect("查询 sqlite_master 失败");
            assert_eq!(count, 1, "表应存在: {}", name);
        }
    }

    #[test]
    fn agent_run_running_status_keeps_completed_at_empty() {
        let dir = make_temp_dir("llm-wiki-db-agent-running");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        let run_id = start_agent_run(&db_path, "H0 running", "10").expect("创建 run 失败");
        complete_agent_run(&db_path, run_id, "running", "20").expect("更新 run 失败");
        let runs = list_agent_runs(&db_path, 5, false).expect("查询 runs 失败");
        assert_eq!(runs[0].id, run_id);
        assert_eq!(runs[0].status, "running");
        assert!(runs[0].completed_at.is_none());
    }

    #[test]
    fn agent_draft_create_list_and_apply_work() {
        let dir = make_temp_dir("llm-wiki-db-agent-draft");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        let run_id = start_agent_run(&db_path, "H1 Draft", "100").expect("创建 run 失败");
        let draft_1 = create_agent_draft(
            &db_path,
            run_id,
            "Draft A",
            "# Draft A\n\ncontent A",
            "draft",
            "110",
        )
        .expect("创建草稿 1 失败");
        let draft_2 = create_agent_draft(
            &db_path,
            run_id,
            "Draft B",
            "# Draft B\n\ncontent B",
            "draft",
            "120",
        )
        .expect("创建草稿 2 失败");

        let drafts = list_agent_drafts(&db_path, run_id, 10).expect("查询草稿失败");
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].id, draft_2.id, "应按更新时间倒序");
        assert_eq!(drafts[1].id, draft_1.id);

        update_agent_draft_status(&db_path, draft_1.id, "applied", "130")
            .expect("更新草稿状态失败");
        let applied = get_agent_draft(&db_path, draft_1.id)
            .expect("读取草稿失败")
            .expect("草稿应存在");
        assert_eq!(applied.status, "applied");
        assert_eq!(applied.updated_at, "130");
    }

    #[test]
    fn agent_skill_upsert_list_delete_work() {
        let dir = make_temp_dir("llm-wiki-db-agent-skill");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        let first =
            upsert_agent_skill(&db_path, "writer", "你是写作助手", "100").expect("创建技能失败");
        assert_eq!(first.skill_key, "writer");
        assert_eq!(first.version, 1);

        let second = upsert_agent_skill(&db_path, "writer", "你是高级写作助手", "120")
            .expect("更新技能失败");
        assert_eq!(second.id, first.id);
        assert_eq!(second.version, 2);
        assert_eq!(second.prompt_template, "你是高级写作助手");
        assert_eq!(second.created_at, "100");
        assert_eq!(second.updated_at, "120");

        let list = list_agent_skills(&db_path, 10).expect("查询技能失败");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, first.id);

        delete_agent_skill(&db_path, first.id).expect("删除技能失败");
        let empty = list_agent_skills(&db_path, 10).expect("查询技能失败");
        assert!(empty.is_empty());
    }

    #[test]
    fn get_vault_stats_returns_correct_counts() {
        let dir = make_temp_dir("llm-wiki-db-vault-stats");
        let _guard = TempDirGuard(dir.clone());
        let db_path = dir.join("meta.db");
        ensure_meta_db(&db_path).expect("初始化数据库失败");

        // 写入 2 个 wiki 页面
        upsert_generated_wiki_page(
            &db_path,
            "页面 A",
            &dir.join("wiki/a.md"),
            "摘要 A",
            "hash-a",
            "100",
        )
        .expect("写入 wiki/a.md 失败");
        upsert_generated_wiki_page(
            &db_path,
            "页面 B",
            &dir.join("wiki/b.md"),
            "摘要 B",
            "hash-b",
            "200",
        )
        .expect("写入 wiki/b.md 失败");

        // a → b 引用：使用 wiki/b.md 的规范路径（与 wiki_pages.path 匹配）
        let b_path_str = dir.join("wiki/b.md").to_string_lossy().to_string();
        replace_citations_for_page(
            &db_path,
            &dir.join("wiki/a.md"),
            &[CitationInput {
                cited_page_path: &b_path_str,
                score: 1,
                excerpt: "excerpt",
            }],
            "300",
        )
        .expect("写入 citations 失败");

        let now_ms = 400_i64;
        let stats = get_vault_stats_from_db(&db_path, now_ms).expect("获取统计数据失败");

        assert_eq!(stats.total_pages, 2);
        assert_eq!(stats.total_citations, 1);
        // wiki/a.md 没有被引用 → orphan；wiki/b.md 被引用 → 非 orphan
        assert_eq!(stats.orphan_pages, 1);
        assert_eq!(stats.top_cited_pages.len(), 1);
        assert_eq!(stats.top_cited_pages[0].path, b_path_str);
        assert_eq!(stats.top_cited_pages[0].citation_count, 1);
    }
}
