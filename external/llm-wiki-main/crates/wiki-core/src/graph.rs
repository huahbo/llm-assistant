//! 类型化知识图：邻接快照上的受控遍历（供「图检索」与混合排序的种子扩展）。

use crate::model::{EntityId, RelationKind};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphSnapshot {
    pub edges: Vec<(EntityId, EntityId, RelationKind)>,
}

#[derive(Debug, Clone)]
pub struct GraphWalkOptions {
    pub max_depth: u32,
    pub allowed_relations: Option<HashSet<RelationKind>>,
}

impl Default for GraphWalkOptions {
    fn default() -> Self {
        Self {
            max_depth: 3,
            allowed_relations: None,
        }
    }
}

/// 从种子实体出发 BFS，返回访问到的实体（含种子），边过滤可选。
pub fn walk_entities(graph: &GraphSnapshot, seeds: &[EntityId], opts: &GraphWalkOptions) -> Vec<EntityId> {
    let mut adj: HashMap<EntityId, Vec<(EntityId, RelationKind)>> = HashMap::new();
    for (a, b, r) in &graph.edges {
        adj.entry(*a).or_default().push((*b, r.clone()));
    }
    let mut seen: HashSet<EntityId> = HashSet::new();
    let mut q: VecDeque<(EntityId, u32)> = VecDeque::new();
    for s in seeds {
        if seen.insert(*s) {
            q.push_back((*s, 0));
        }
    }
    let mut order = Vec::new();
    while let Some((node, d)) = q.pop_front() {
        order.push(node);
        if d >= opts.max_depth {
            continue;
        }
        if let Some(nbrs) = adj.get(&node) {
            for (next, rel) in nbrs {
                if let Some(allow) = &opts.allowed_relations {
                    if !allow.contains(rel) {
                        continue;
                    }
                }
                if seen.insert(*next) {
                    q.push_back((*next, d + 1));
                }
            }
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EntityId, RelationKind};
    use uuid::Uuid;

    #[test]
    fn bfs_respects_depth() {
        let a = EntityId(Uuid::new_v4());
        let b = EntityId(Uuid::new_v4());
        let c = EntityId(Uuid::new_v4());
        let g = GraphSnapshot {
            edges: vec![
                (a, b, RelationKind::DependsOn),
                (b, c, RelationKind::Uses),
            ],
        };
        let out = walk_entities(
            &g,
            &[a],
            &GraphWalkOptions {
                max_depth: 1,
                allowed_relations: None,
            },
        );
        assert!(out.contains(&a));
        assert!(out.contains(&b));
        assert!(!out.contains(&c));
    }
}
