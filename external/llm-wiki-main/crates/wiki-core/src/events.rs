//! 事件驱动自动化：ingest / session / query / 定时任务 的钩子载荷。

use crate::model::{ClaimId, EntityId, PageId, SourceId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WikiEvent {
    SourceIngested {
        source_id: SourceId,
        redacted: bool,
        at: OffsetDateTime,
    },
    ClaimUpserted {
        claim_id: ClaimId,
        at: OffsetDateTime,
    },
    ClaimSuperseded {
        old: ClaimId,
        new: ClaimId,
        at: OffsetDateTime,
    },
    PageWritten {
        page_id: PageId,
        at: OffsetDateTime,
    },
    QueryServed {
        query_fingerprint: String,
        top_doc_ids: Vec<String>,
        at: OffsetDateTime,
    },
    SessionCrystallized {
        page_id: PageId,
        at: OffsetDateTime,
    },
    GraphExpanded {
        seeds: Vec<EntityId>,
        visited: Vec<EntityId>,
        at: OffsetDateTime,
    },
    LintRunFinished {
        findings: usize,
        at: OffsetDateTime,
    },
}
