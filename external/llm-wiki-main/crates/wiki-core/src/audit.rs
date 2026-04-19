//! 审计：ingest / edit / delete / query 的可追责记录（append-only 模型）。

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOperation {
    IngestSource,
    WriteClaim,
    SupersedeClaim,
    WritePage,
    RunLint,
    RunQuery,
    CrystallizeSession,
    RedactSensitive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub id: Uuid,
    pub op: AuditOperation,
    pub actor: String,
    pub summary: String,
    pub at: OffsetDateTime,
}

impl AuditRecord {
    pub fn new(op: AuditOperation, actor: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            op,
            actor: actor.into(),
            summary: summary.into(),
            at: OffsetDateTime::now_utc(),
        }
    }
}
