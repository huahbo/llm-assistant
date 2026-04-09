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
}
