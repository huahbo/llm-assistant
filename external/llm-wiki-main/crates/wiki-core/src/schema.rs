//! 「Schema 是真正的产品」：领域类型、关系白名单、质量与巩固策略参数。

use crate::model::{EntityKind, MemoryTier, RelationKind};
use crate::retention::RetentionParams;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum SchemaLoadError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainSchema {
    pub title: String,
    pub allowed_entity_kinds: HashSet<EntityKind>,
    pub allowed_relations: HashSet<RelationKind>,
    pub min_quality_to_crystallize: f64,
    pub min_confidence_to_promote: f64,
    pub default_retention: RetentionParams,
    pub tier_half_life_days: std::collections::HashMap<MemoryTier, f64>,
}

impl DomainSchema {
    pub fn permissive_default() -> Self {
        use std::collections::HashMap;
        let mut tier_half_life_days = HashMap::new();
        tier_half_life_days.insert(MemoryTier::Working, 3.0);
        tier_half_life_days.insert(MemoryTier::Episodic, 14.0);
        tier_half_life_days.insert(MemoryTier::Semantic, 60.0);
        tier_half_life_days.insert(MemoryTier::Procedural, 120.0);

        Self {
            title: "default-permissive".into(),
            allowed_entity_kinds: [
                EntityKind::Person,
                EntityKind::Project,
                EntityKind::Library,
                EntityKind::Concept,
                EntityKind::FilePath,
                EntityKind::Decision,
            ]
            .into_iter()
            .collect(),
            allowed_relations: [
                RelationKind::Uses,
                RelationKind::DependsOn,
                RelationKind::Contradicts,
                RelationKind::Caused,
                RelationKind::Fixed,
                RelationKind::Supersedes,
                RelationKind::Related,
            ]
            .into_iter()
            .collect(),
            min_quality_to_crystallize: 0.55,
            min_confidence_to_promote: 0.62,
            default_retention: RetentionParams::default(),
            tier_half_life_days,
        }
    }

    pub fn relation_allowed(&self, r: &RelationKind) -> bool {
        self.allowed_relations.contains(r)
    }

    pub fn entity_kind_allowed(&self, k: &EntityKind) -> bool {
        self.allowed_entity_kinds.contains(k)
    }

    /// 从 JSON 字节反序列化（与 `serde_json::to_vec` 输出互操作）。
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// 从 UTF-8 文件读取 JSON Schema。
    pub fn from_json_path(path: &Path) -> Result<Self, SchemaLoadError> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_json_slice(&bytes)?)
    }
}
