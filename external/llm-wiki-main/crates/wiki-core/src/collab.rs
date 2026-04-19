//! 轻量协作：谁在做什么（非完整任务系统）。

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkState {
    Todo,
    InProgress,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: Uuid,
    pub topic: String,
    pub owner: String,
    pub state: WorkState,
    pub updated_at: OffsetDateTime,
}

impl WorkItem {
    pub fn new(topic: impl Into<String>, owner: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            topic: topic.into(),
            owner: owner.into(),
            state: WorkState::Todo,
            updated_at: OffsetDateTime::now_utc(),
        }
    }
}
