use std::path::Path;

use rusqlite::{params, Connection};

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
}

/// 确保元数据库与表结构存在。
pub fn ensure_meta_db(db_path: &Path) -> Result<(), String> {
    let conn = open_connection(db_path)?;
    init_schema(&conn)
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
            })
        })
        .map_err(|err| format!("读取 wiki_pages 失败: {}", err))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("读取 wiki_pages 失败: {}", err))
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
    let mut stmt = conn
        .prepare(
            r#"
            SELECT title, path, summary, updated_at
            FROM wiki_pages
            WHERE instr(lower(title), lower(?1)) > 0
               OR instr(lower(summary), lower(?1)) > 0
               OR instr(lower(path), lower(?1)) > 0
            ORDER BY updated_at DESC, id DESC
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

    tx.commit().map_err(|err| format!("提交引用记录失败: {}", err))
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

fn open_connection(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("创建数据库目录失败: {}", err))?;
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

        CREATE INDEX IF NOT EXISTS idx_citations_page_path
            ON citations(page_path);
        "#,
    )
    .map_err(|err| format!("初始化数据库结构失败: {}", err))?;

    // FTS 为增强能力，初始化失败时不阻断主流程，查询侧会自动降级。
    let _ = conn.execute(
        "CREATE VIRTUAL TABLE IF NOT EXISTS fts_pages USING fts5(path UNINDEXED, title, body)",
        [],
    );

    Ok(())
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
        let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
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

        let results = search_fts_page_paths(&db_path, &[String::from("rust")], 5)
            .expect("执行 fts 查询失败");
        assert!(results
            .iter()
            .any(|path| path == &wiki_path.to_string_lossy().to_string()));
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

        let all = search_wiki_pages(&db_path, "   ", 10).expect("读取最近页面失败");
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].title, "Rust 进阶");
    }
}
