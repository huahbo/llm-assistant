use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

/// 巩固流水线中的层级：工作记忆 → 情节 → 语义 → 程序性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryTier {
    Working,
    Episodic,
    Semantic,
    Procedural,
}

/// 私域 / 共享范围（多 Agent / 团队）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    Private { agent_id: String },
    Shared { team_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClaimId(pub Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Person,
    Project,
    Library,
    Concept,
    FilePath,
    Decision,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Uses,
    DependsOn,
    Contradicts,
    Caused,
    Fixed,
    Supersedes,
    Related,
}

/// 原子断言：可评分、可取代、可随访问强化与半衰期衰减排序。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: ClaimId,
    pub text: String,
    pub tier: MemoryTier,
    pub scope: Scope,
    /// 综合置信度（来源数量、时间、矛盾处理后的结果）。
    pub confidence: f64,
    pub quality_score: f64,
    pub source_ids: Vec<SourceId>,
    pub supersedes: Option<ClaimId>,
    pub stale: bool,
    pub created_at: OffsetDateTime,
    pub last_reinforced_at: Option<OffsetDateTime>,
    pub access_count: u32,
}

impl Claim {
    pub fn new(text: impl Into<String>, scope: Scope, tier: MemoryTier) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id: ClaimId(Uuid::new_v4()),
            text: text.into(),
            tier,
            scope,
            confidence: 0.5,
            quality_score: 0.5,
            source_ids: Vec::new(),
            supersedes: None,
            stale: false,
            created_at: now,
            last_reinforced_at: None,
            access_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub kind: EntityKind,
    pub label: String,
    pub scope: Scope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedEdge {
    pub from: EntityId,
    pub to: EntityId,
    pub relation: RelationKind,
    pub confidence: f64,
    pub source_ids: Vec<SourceId>,
}
