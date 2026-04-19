//! 原始资料层：verbatim + 元数据，供 ingest 与结晶回溯。

use crate::model::{Scope, SourceId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawArtifact {
    pub id: SourceId,
    pub uri: String,
    pub body: String,
    pub scope: Scope,
    pub ingested_at: OffsetDateTime,
}

impl RawArtifact {
    pub fn new(uri: impl Into<String>, body: impl Into<String>, scope: Scope) -> Self {
        Self {
            id: SourceId(Uuid::new_v4()),
            uri: uri.into(),
            body: body.into(),
            scope,
            ingested_at: OffsetDateTime::now_utc(),
        }
    }
}
