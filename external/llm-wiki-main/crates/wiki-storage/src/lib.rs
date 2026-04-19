use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use wiki_core::{AuditRecord, Claim, Entity, RawArtifact, TypedEdge, WikiEvent, WikiPage};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageSnapshot {
    pub sources: Vec<RawArtifact>,
    pub claims: Vec<Claim>,
    pub pages: Vec<WikiPage>,
    pub entities: Vec<Entity>,
    pub edges: Vec<TypedEdge>,
    pub audits: Vec<AuditRecord>,
}

pub trait WikiRepository {
    fn load_snapshot(&self) -> Result<StorageSnapshot, StorageError>;
    fn save_snapshot(&self, snapshot: &StorageSnapshot) -> Result<(), StorageError>;
    fn append_outbox(&self, event: &WikiEvent) -> Result<(), StorageError>;
    fn export_outbox_ndjson(&self) -> Result<String, StorageError>;
    fn export_outbox_ndjson_from_id(&self, last_id: i64) -> Result<String, StorageError>;
    fn mark_outbox_processed(&self, up_to_id: i64, consumer_tag: &str) -> Result<usize, StorageError>;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("db: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct SqliteRepository {
    conn: Connection,
}

impl SqliteRepository {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS wiki_state (
  id INTEGER PRIMARY KEY CHECK (id=1),
  payload_json TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS wiki_outbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  event_json TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  processed_at TEXT,
  consumer_tag TEXT
);
"#,
        )?;
        Ok(Self { conn })
    }
}

impl WikiRepository for SqliteRepository {
    fn load_snapshot(&self) -> Result<StorageSnapshot, StorageError> {
        let row = self.conn.query_row(
            "SELECT payload_json FROM wiki_state WHERE id=1",
            [],
            |r| r.get::<_, String>(0),
        );
        match row {
            Ok(payload) => Ok(serde_json::from_str(&payload)?),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(StorageSnapshot::default()),
            Err(e) => Err(StorageError::Db(e)),
        }
    }

    fn save_snapshot(&self, snapshot: &StorageSnapshot) -> Result<(), StorageError> {
        let payload = serde_json::to_string(snapshot)?;
        self.conn.execute(
            "INSERT INTO wiki_state(id, payload_json) VALUES(1, ?1)
             ON CONFLICT(id) DO UPDATE SET payload_json=excluded.payload_json",
            params![payload],
        )?;
        Ok(())
    }

    fn append_outbox(&self, event: &WikiEvent) -> Result<(), StorageError> {
        let payload = serde_json::to_string(event)?;
        self.conn.execute(
            "INSERT INTO wiki_outbox(event_json) VALUES(?1)",
            params![payload],
        )?;
        Ok(())
    }

    fn export_outbox_ndjson(&self) -> Result<String, StorageError> {
        self.export_outbox_ndjson_from_id(0)
    }

    fn export_outbox_ndjson_from_id(&self, last_id: i64) -> Result<String, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT event_json FROM wiki_outbox WHERE id > ?1 ORDER BY id ASC")?;
        let mut out = String::new();
        let mut rows = stmt.query(params![last_id])?;
        while let Some(r) = rows.next()? {
            let line: String = r.get(0)?;
            out.push_str(&line);
            out.push('\n');
        }
        Ok(out)
    }

    fn mark_outbox_processed(&self, up_to_id: i64, consumer_tag: &str) -> Result<usize, StorageError> {
        let n = self.conn.execute(
            "UPDATE wiki_outbox
             SET processed_at = datetime('now'), consumer_tag = ?2
             WHERE id <= ?1 AND processed_at IS NULL",
            params![up_to_id, consumer_tag],
        )?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wiki_core::{Scope, WikiEvent};

    #[test]
    fn outbox_export_from_id_and_ack() {
        let dir = tempdir().unwrap();
        let db = dir.path().join("wiki.db");
        let repo = SqliteRepository::open(&db).unwrap();

        repo.append_outbox(&WikiEvent::QueryServed {
            query_fingerprint: "q1".into(),
            top_doc_ids: vec!["a".into()],
            at: time::OffsetDateTime::now_utc(),
        })
        .unwrap();
        repo.append_outbox(&WikiEvent::SourceIngested {
            source_id: wiki_core::SourceId(uuid::Uuid::new_v4()),
            redacted: false,
            at: time::OffsetDateTime::now_utc(),
        })
        .unwrap();

        let all = repo.export_outbox_ndjson().unwrap();
        assert!(all.lines().count() >= 2);

        let from1 = repo.export_outbox_ndjson_from_id(1).unwrap();
        assert!(from1.lines().count() >= 1);

        let acked = repo.mark_outbox_processed(1, "t").unwrap();
        assert_eq!(acked, 1);

        // Second ack should not re-ack already processed.
        let acked2 = repo.mark_outbox_processed(1, "t").unwrap();
        assert_eq!(acked2, 0);

        // Make sure schema stays loadable even with extra columns.
        let _snap = repo.load_snapshot().unwrap();
        let _scope = Scope::Private {
            agent_id: "a".into(),
        };
    }
}

