use super::AppState;
use crate::{
    db,
    models::{
        KnowledgeGraphData, KnowledgeGraphDirection, KnowledgeGraphLink, KnowledgeGraphNode,
        KnowledgeSubgraphData, KnowledgeSubgraphMeta,
    },
};
use std::collections::{HashMap, HashSet, VecDeque};

pub fn get_knowledge_graph_impl(state: &AppState) -> Result<KnowledgeGraphData, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let vault_path = vault_path.ok_or_else(|| "请先初始化 Vault".to_string())?;
    let db_path = vault_path.join(".app").join("meta.db");

    let page_records = db::list_all_wiki_pages(&db_path)?;

    let nodes: Vec<KnowledgeGraphNode> = page_records
        .iter()
        .map(|record| {
            let (group, label) = std::fs::read_to_string(&record.path)
                .ok()
                .and_then(|content| super::wiki_service::parse_wiki_frontmatter(&content))
                .map(|fm| {
                    let label = super::wiki_service::resolve_graph_node_label(&record.title, &fm);
                    let group = fm.entities.into_iter().next().unwrap_or_default();
                    (group, label)
                })
                .unwrap_or_else(|| (String::new(), record.title.clone()));

            KnowledgeGraphNode {
                id: record.path.clone(),
                label,
                group,
            }
        })
        .collect();

    let citation_records = db::list_citations(&db_path)?;
    let mut seen_links = HashSet::new();
    let links: Vec<KnowledgeGraphLink> = citation_records
        .into_iter()
        .filter_map(|c| {
            let key = (c.page_path.clone(), c.cited_page_path.clone());
            if seen_links.insert(key) {
                Some(KnowledgeGraphLink {
                    source: c.page_path,
                    target: c.cited_page_path,
                })
            } else {
                None
            }
        })
        .collect();

    Ok(KnowledgeGraphData { nodes, links })
}

pub fn get_knowledge_subgraph_impl(
    state: &AppState,
    center_page_path: String,
    hop: u8,
    direction: KnowledgeGraphDirection,
    limit_nodes: usize,
    limit_links: usize,
) -> Result<KnowledgeSubgraphData, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let vault_path = vault_path.ok_or_else(|| "请先初始化 Vault".to_string())?;
    let db_path = vault_path.join(".app").join("meta.db");

    let page_records = db::list_all_wiki_pages(&db_path)?;
    let mut node_map = HashMap::<String, KnowledgeGraphNode>::new();
    for record in page_records {
        let (group, label) = std::fs::read_to_string(&record.path)
            .ok()
            .and_then(|content| super::wiki_service::parse_wiki_frontmatter(&content))
            .map(|fm| {
                let label = super::wiki_service::resolve_graph_node_label(&record.title, &fm);
                let group = fm.entities.into_iter().next().unwrap_or_default();
                (group, label)
            })
            .unwrap_or_else(|| (String::new(), record.title.clone()));
        node_map.insert(
            record.path.clone(),
            KnowledgeGraphNode {
                id: record.path,
                label,
                group,
            },
        );
    }

    let trimmed_center = center_page_path.trim();
    if trimmed_center.is_empty() {
        return Err("中心页面路径不能为空".to_string());
    }
    let center_id = if node_map.contains_key(trimmed_center) {
        trimmed_center.to_string()
    } else {
        let resolved = super::resolve_existing_wiki_page_path(&vault_path, trimmed_center)?;
        let resolved_str = resolved.to_string_lossy().to_string();
        if !node_map.contains_key(&resolved_str) {
            return Err("中心页面不在图谱中".to_string());
        }
        resolved_str
    };

    let mut seen_links = HashSet::new();
    let mut all_links = Vec::<KnowledgeGraphLink>::new();
    for citation in db::list_citations(&db_path)? {
        if !node_map.contains_key(&citation.page_path)
            || !node_map.contains_key(&citation.cited_page_path)
        {
            continue;
        }
        let key = (citation.page_path.clone(), citation.cited_page_path.clone());
        if !seen_links.insert(key) {
            continue;
        }
        all_links.push(KnowledgeGraphLink {
            source: citation.page_path,
            target: citation.cited_page_path,
        });
    }

    let mut adjacency_both = HashMap::<String, Vec<String>>::new();
    let mut adjacency_out = HashMap::<String, Vec<String>>::new();
    let mut adjacency_in = HashMap::<String, Vec<String>>::new();
    for node_id in node_map.keys() {
        adjacency_both.insert(node_id.clone(), Vec::new());
        adjacency_out.insert(node_id.clone(), Vec::new());
        adjacency_in.insert(node_id.clone(), Vec::new());
    }
    for link in &all_links {
        adjacency_both
            .entry(link.source.clone())
            .or_default()
            .push(link.target.clone());
        adjacency_both
            .entry(link.target.clone())
            .or_default()
            .push(link.source.clone());
        adjacency_out
            .entry(link.source.clone())
            .or_default()
            .push(link.target.clone());
        adjacency_in
            .entry(link.target.clone())
            .or_default()
            .push(link.source.clone());
    }

    let effective_hop = hop.clamp(1, 3);
    let effective_limit_nodes = if limit_nodes == 0 { 500 } else { limit_nodes.min(5000) };
    let effective_limit_links = if limit_links == 0 { 2000 } else { limit_links.min(20000) };

    let mut visited = HashSet::<String>::new();
    visited.insert(center_id.clone());
    let mut queue = VecDeque::<(String, u8)>::new();
    queue.push_back((center_id.clone(), 0));
    let mut truncated = false;

    while let Some((current, depth)) = queue.pop_front() {
        if depth >= effective_hop {
            continue;
        }
        let neighbors = match direction {
            KnowledgeGraphDirection::Both => adjacency_both.get(&current),
            KnowledgeGraphDirection::Out => adjacency_out.get(&current),
            KnowledgeGraphDirection::In => adjacency_in.get(&current),
        };
        let Some(neighbors) = neighbors else {
            continue;
        };
        for neighbor in neighbors {
            if visited.contains(neighbor) {
                continue;
            }
            if visited.len() >= effective_limit_nodes {
                truncated = true;
                break;
            }
            visited.insert(neighbor.clone());
            queue.push_back((neighbor.clone(), depth + 1));
        }
        if truncated {
            break;
        }
    }

    let mut nodes = visited
        .iter()
        .filter_map(|node_id| node_map.get(node_id).cloned())
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| left.label.cmp(&right.label));

    let mut links = all_links
        .into_iter()
        .filter(|link| visited.contains(&link.source) && visited.contains(&link.target))
        .collect::<Vec<_>>();
    links.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.target.cmp(&right.target))
    });
    if links.len() > effective_limit_links {
        links.truncate(effective_limit_links);
        truncated = true;
    }

    Ok(KnowledgeSubgraphData {
        meta: KnowledgeSubgraphMeta {
            center_page_path: center_id,
            hop: effective_hop,
            direction,
            truncated,
            node_count: nodes.len(),
            link_count: links.len(),
        },
        nodes,
        links,
    })
}
