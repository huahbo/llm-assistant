//! agent_chat DB schema、CRUD 和内置工具种子

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::llm::types::ToolSchema;

// ── Models ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub title: String,
    pub system_prompt: Option<String>,
    pub skill_key: Option<String>,
    pub memory_snapshot: Option<String>,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: Option<String>,
    pub tool_calls_json: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub sequence: i64,
    pub created_at: String,
}

// ── Schema ─────────────────────────────────────────────────────────────────────

/// 创建 agent_chat 所需的 3 张表（幂等，由 db::init_schema 调用）
pub fn ensure_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(r#"
        CREATE TABLE IF NOT EXISTS agent_conversations (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            title         TEXT NOT NULL,
            system_prompt TEXT,
            skill_key     TEXT,
            memory_snapshot TEXT,
            archived      INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS agent_messages (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id INTEGER NOT NULL,
            role            TEXT NOT NULL,
            content         TEXT,
            tool_calls_json TEXT,
            tool_call_id    TEXT,
            tool_name       TEXT,
            sequence        INTEGER NOT NULL,
            created_at      TEXT NOT NULL,
            FOREIGN KEY (conversation_id) REFERENCES agent_conversations(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_msg_conv ON agent_messages(conversation_id, sequence);

        CREATE TABLE IF NOT EXISTS agent_tools (
            name              TEXT PRIMARY KEY,
            description       TEXT NOT NULL,
            parameters_schema TEXT NOT NULL,
            handler_kind      TEXT NOT NULL,
            enabled           INTEGER NOT NULL DEFAULT 1
        );
    "#)
    .map_err(|e| format!("agent_chat schema 创建失败: {}", e))
}

/// 写入 5 个内置工具（INSERT OR IGNORE，幂等）
pub fn seed_builtin_tools(conn: &Connection) -> Result<(), String> {
    let tools: &[(&str, &str, &str, &str)] = &[
        (
            "run_shell",
            "在 vault 所在机器上执行 Shell 命令，返回 stdout / stderr",
            r#"{"type":"object","properties":{"command":{"type":"string","description":"要执行的 Shell 命令"},"working_dir":{"type":"string","description":"工作目录（可选，默认为 vault 根目录）"}},"required":["command"]}"#,
            "builtin",
        ),
        (
            "search_wiki",
            "在 Wiki 中按关键词全文检索，返回最相关页面摘要列表",
            r#"{"type":"object","properties":{"query":{"type":"string","description":"搜索关键词或问题"},"top_k":{"type":"integer","description":"返回结果数量，默认 5"}},"required":["query"]}"#,
            "builtin",
        ),
        (
            "read_wiki",
            "读取指定 Wiki 页面的完整 Markdown 内容",
            r#"{"type":"object","properties":{"path":{"type":"string","description":"Wiki 页面相对路径，如 'docs/index.md'"}},"required":["path"]}"#,
            "builtin",
        ),
        (
            "write_wiki",
            "在 Wiki 中创建新页面（含 frontmatter），需要用户审批后生效",
            r#"{"type":"object","properties":{"path":{"type":"string","description":"目标文件路径"},"content":{"type":"string","description":"页面完整 Markdown 内容（含 frontmatter）"},"summary":{"type":"string","description":"变更摘要，用于审批提示"}},"required":["path","content"]}"#,
            "builtin",
        ),
        (
            "edit_wiki",
            "对现有 Wiki 页面进行局部修改（替换指定文本），需要用户审批后生效",
            r#"{"type":"object","properties":{"path":{"type":"string","description":"目标文件路径"},"old_str":{"type":"string","description":"要替换的原始文本（精确匹配）"},"new_str":{"type":"string","description":"替换后的新文本"},"summary":{"type":"string","description":"变更摘要，用于审批提示"}},"required":["path","old_str","new_str"]}"#,
            "builtin",
        ),
    ];

    for (name, description, parameters_schema, handler_kind) in tools {
        conn.execute(
            "INSERT OR IGNORE INTO agent_tools (name, description, parameters_schema, handler_kind, enabled) VALUES (?1, ?2, ?3, ?4, 1)",
            params![name, description, parameters_schema, handler_kind],
        )
        .map_err(|e| format!("写入内置工具 '{}' 失败: {}", name, e))?;
    }
    Ok(())
}

// ── Connection helper ──────────────────────────────────────────────────────────

fn open_conn(db_path: &Path) -> Result<Connection, String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("打开 agent_chat 数据库失败: {}", e))?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(|e| format!("启用外键约束失败: {}", e))?;
    Ok(conn)
}

// ── Conversation CRUD ──────────────────────────────────────────────────────────

pub fn create_conversation(
    db_path: &Path,
    title: &str,
    system_prompt: Option<&str>,
    skill_key: Option<&str>,
    memory_snapshot: Option<&str>,
    now: &str,
) -> Result<i64, String> {
    let conn = open_conn(db_path)?;
    conn.execute(
        "INSERT INTO agent_conversations (title, system_prompt, skill_key, memory_snapshot, archived, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
        params![title, system_prompt, skill_key, memory_snapshot, now],
    )
    .map_err(|e| format!("创建会话失败: {}", e))?;
    Ok(conn.last_insert_rowid())
}

pub fn list_conversations(db_path: &Path, include_archived: bool) -> Result<Vec<Conversation>, String> {
    let conn = open_conn(db_path)?;
    let sql = if include_archived {
        "SELECT id, title, system_prompt, skill_key, memory_snapshot, archived, created_at, updated_at
         FROM agent_conversations ORDER BY updated_at DESC"
    } else {
        "SELECT id, title, system_prompt, skill_key, memory_snapshot, archived, created_at, updated_at
         FROM agent_conversations WHERE archived = 0 ORDER BY updated_at DESC"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| format!("准备列表查询失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                system_prompt: row.get(2)?,
                skill_key: row.get(3)?,
                memory_snapshot: row.get(4)?,
                archived: row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|e| format!("查询会话列表失败: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取会话行失败: {}", e))
}

pub fn get_conversation(db_path: &Path, id: i64) -> Result<Option<Conversation>, String> {
    let conn = open_conn(db_path)?;
    conn.query_row(
        "SELECT id, title, system_prompt, skill_key, memory_snapshot, archived, created_at, updated_at
         FROM agent_conversations WHERE id = ?1",
        params![id],
        |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                system_prompt: row.get(2)?,
                skill_key: row.get(3)?,
                memory_snapshot: row.get(4)?,
                archived: row.get::<_, i32>(5)? != 0,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(|e| format!("查询会话失败: {}", e))
}

pub fn rename_conversation(db_path: &Path, id: i64, title: &str, now: &str) -> Result<(), String> {
    let conn = open_conn(db_path)?;
    conn.execute(
        "UPDATE agent_conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
        params![title, now, id],
    )
    .map_err(|e| format!("重命名会话失败: {}", e))?;
    Ok(())
}

pub fn archive_conversation(db_path: &Path, id: i64, now: &str) -> Result<(), String> {
    let conn = open_conn(db_path)?;
    conn.execute(
        "UPDATE agent_conversations SET archived = 1, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )
    .map_err(|e| format!("归档会话失败: {}", e))?;
    Ok(())
}

pub fn delete_conversation(db_path: &Path, id: i64) -> Result<(), String> {
    let conn = open_conn(db_path)?;
    conn.execute("DELETE FROM agent_conversations WHERE id = ?1", params![id])
        .map_err(|e| format!("删除会话失败: {}", e))?;
    Ok(())
}

// ── Message CRUD ───────────────────────────────────────────────────────────────

pub fn append_message(
    db_path: &Path,
    conv_id: i64,
    role: &str,
    content: Option<&str>,
    tool_calls_json: Option<&str>,
    tool_call_id: Option<&str>,
    tool_name: Option<&str>,
    now: &str,
) -> Result<i64, String> {
    let conn = open_conn(db_path)?;
    conn.execute(
        r#"INSERT INTO agent_messages
               (conversation_id, role, content, tool_calls_json, tool_call_id, tool_name,
                sequence, created_at)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6,
               COALESCE((SELECT MAX(sequence) FROM agent_messages WHERE conversation_id = ?1), 0) + 1,
               ?7)"#,
        params![conv_id, role, content, tool_calls_json, tool_call_id, tool_name, now],
    )
    .map_err(|e| format!("追加消息失败: {}", e))?;
    // Also bump conversation updated_at
    let _ = conn.execute(
        "UPDATE agent_conversations SET updated_at = ?1 WHERE id = ?2",
        params![now, conv_id],
    );
    Ok(conn.last_insert_rowid())
}

/// 流式完成后更新 assistant 消息的 content 和 tool_calls_json
pub fn update_message_after_stream(
    db_path: &Path,
    message_id: i64,
    content: Option<&str>,
    tool_calls_json: Option<&str>,
) -> Result<(), String> {
    let conn = open_conn(db_path)?;
    conn.execute(
        "UPDATE agent_messages SET content = ?1, tool_calls_json = ?2 WHERE id = ?3",
        params![content, tool_calls_json, message_id],
    )
    .map_err(|e| format!("更新消息失败: {}", e))?;
    Ok(())
}

pub fn list_messages(db_path: &Path, conv_id: i64) -> Result<Vec<Message>, String> {
    let conn = open_conn(db_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, role, content, tool_calls_json, tool_call_id, tool_name, sequence, created_at
             FROM agent_messages WHERE conversation_id = ?1 ORDER BY sequence ASC",
        )
        .map_err(|e| format!("准备消息查询失败: {}", e))?;
    let rows = stmt
        .query_map(params![conv_id], |row| {
            Ok(Message {
                id: row.get(0)?,
                conversation_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls_json: row.get(4)?,
                tool_call_id: row.get(5)?,
                tool_name: row.get(6)?,
                sequence: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| format!("查询消息失败: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取消息行失败: {}", e))
}

// ── Tool CRUD ──────────────────────────────────────────────────────────────────

pub fn list_enabled_tools(db_path: &Path) -> Result<Vec<ToolSchema>, String> {
    let conn = open_conn(db_path)?;
    let mut stmt = conn
        .prepare("SELECT name, description, parameters_schema FROM agent_tools WHERE enabled = 1")
        .map_err(|e| format!("准备工具查询失败: {}", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("查询工具列表失败: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("读取工具行失败: {}", e))?
        .into_iter()
        .map(|(name, description, params_str)| {
            let parameters: serde_json::Value = serde_json::from_str(&params_str)
                .map_err(|e| format!("解析工具 '{}' schema 失败: {}", name, e))?;
            Ok(ToolSchema { name, description, parameters })
        })
        .collect()
}

pub fn upsert_tool(
    db_path: &Path,
    name: &str,
    description: &str,
    parameters_schema: &str,
    handler_kind: &str,
    enabled: bool,
) -> Result<(), String> {
    let conn = open_conn(db_path)?;
    conn.execute(
        "INSERT INTO agent_tools (name, description, parameters_schema, handler_kind, enabled)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(name) DO UPDATE SET
           description = excluded.description,
           parameters_schema = excluded.parameters_schema,
           handler_kind = excluded.handler_kind,
           enabled = excluded.enabled",
        params![name, description, parameters_schema, handler_kind, enabled as i32],
    )
    .map_err(|e| format!("upsert 工具 '{}' 失败: {}", name, e))?;
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::NamedTempFile;

    fn setup_db() -> (NamedTempFile, std::path::PathBuf) {
        let f = NamedTempFile::new().unwrap();
        let path = f.path().to_path_buf();
        let conn = Connection::open(&path).unwrap();
        ensure_schema(&conn).unwrap();
        seed_builtin_tools(&conn).unwrap();
        (f, path)
    }

    #[test]
    fn test_create_and_list_conversation() {
        let (_f, path) = setup_db();
        let id = create_conversation(&path, "Test Conv", None, None, None, "2026-01-01").unwrap();
        assert!(id > 0);
        let convs = list_conversations(&path, false).unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].title, "Test Conv");
        assert!(!convs[0].archived);
    }

    #[test]
    fn test_append_messages_in_order() {
        let (_f, path) = setup_db();
        let cid = create_conversation(&path, "Chat", None, None, None, "2026-01-01").unwrap();
        let m1 = append_message(&path, cid, "user", Some("Hello"), None, None, None, "t1").unwrap();
        let m2 = append_message(&path, cid, "assistant", Some("Hi"), None, None, None, "t2").unwrap();
        let msgs = list_messages(&path, cid).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].id, m1);
        assert_eq!(msgs[1].id, m2);
        assert_eq!(msgs[0].sequence, 1);
        assert_eq!(msgs[1].sequence, 2);
    }

    #[test]
    fn test_archive_excluded_from_default_list() {
        let (_f, path) = setup_db();
        let id = create_conversation(&path, "Old Chat", None, None, None, "2026-01-01").unwrap();
        archive_conversation(&path, id, "2026-01-02").unwrap();
        let default_list = list_conversations(&path, false).unwrap();
        assert!(default_list.is_empty());
        let all_list = list_conversations(&path, true).unwrap();
        assert_eq!(all_list.len(), 1);
        assert!(all_list[0].archived);
    }

    #[test]
    fn test_seed_builtin_tools_idempotent() {
        let (_f, path) = setup_db();
        let conn = Connection::open(&path).unwrap();
        // Calling seed again should not fail or duplicate
        seed_builtin_tools(&conn).unwrap();
        seed_builtin_tools(&conn).unwrap();
        drop(conn);
        let tools = list_enabled_tools(&path).unwrap();
        assert_eq!(tools.len(), 5);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"run_shell"));
        assert!(names.contains(&"search_wiki"));
        assert!(names.contains(&"read_wiki"));
        assert!(names.contains(&"write_wiki"));
        assert!(names.contains(&"edit_wiki"));
    }

    #[test]
    fn test_delete_conversation_cascades_messages() {
        let (_f, path) = setup_db();
        let cid = create_conversation(&path, "Ephemeral", None, None, None, "2026-01-01").unwrap();
        append_message(&path, cid, "user", Some("msg"), None, None, None, "t1").unwrap();
        assert_eq!(list_messages(&path, cid).unwrap().len(), 1);
        delete_conversation(&path, cid).unwrap();
        // After cascade delete, messages should be gone
        assert!(list_messages(&path, cid).unwrap().is_empty());
    }

    #[test]
    fn test_tool_calls_json_roundtrip() {
        let (_f, path) = setup_db();
        let cid = create_conversation(&path, "Tool Chat", None, None, None, "2026-01-01").unwrap();
        let tool_calls_json = r#"[{"id":"call_1","type":"function","function":{"name":"run_shell","arguments":"{\"command\":\"ls\"}"}}]"#;
        let mid = append_message(&path, cid, "assistant", None, Some(tool_calls_json), None, None, "t1").unwrap();
        let msgs = list_messages(&path, cid).unwrap();
        assert_eq!(msgs[0].id, mid);
        assert_eq!(msgs[0].tool_calls_json.as_deref(), Some(tool_calls_json));
        assert!(msgs[0].content.is_none());
    }
}
