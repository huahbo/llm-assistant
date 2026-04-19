use std::collections::HashMap;
use wiki_storage::StorageSnapshot;

use wiki_core::{
    Claim, ClaimId, Entity, EntityId, GraphSnapshot, RawArtifact, SourceId, TypedEdge, WikiPage,
    PageId,
};

#[derive(Debug, Default)]
pub struct InMemoryStore {
    pub sources: HashMap<SourceId, RawArtifact>,
    pub claims: HashMap<ClaimId, Claim>,
    pub pages: HashMap<PageId, WikiPage>,
    pub entities: HashMap<EntityId, Entity>,
    pub edges: Vec<TypedEdge>,
}

impl InMemoryStore {
    pub fn graph_snapshot(&self) -> GraphSnapshot {
        GraphSnapshot {
            edges: self
                .edges
                .iter()
                .map(|e| (e.from, e.to, e.relation.clone()))
                .collect(),
        }
    }

    pub fn from_snapshot(s: StorageSnapshot) -> Self {
        Self {
            sources: s.sources.into_iter().map(|x| (x.id, x)).collect(),
            claims: s.claims.into_iter().map(|x| (x.id, x)).collect(),
            pages: s.pages.into_iter().map(|x| (x.id, x)).collect(),
            entities: s.entities.into_iter().map(|x| (x.id, x)).collect(),
            edges: s.edges,
        }
    }

    pub fn to_snapshot(&self, audits: &[wiki_core::AuditRecord]) -> StorageSnapshot {
        StorageSnapshot {
            sources: self.sources.values().cloned().collect(),
            claims: self.claims.values().cloned().collect(),
            pages: self.pages.values().cloned().collect(),
            entities: self.entities.values().cloned().collect(),
            edges: self.edges.clone(),
            audits: audits.to_vec(),
        }
    }
}
