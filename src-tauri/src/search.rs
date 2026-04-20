use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::db::PageEmbeddingRecord;

/// 页面检索评分结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageScore {
    pub page_path: String,
    pub score: f64,
}

/// RRF（Reciprocal Rank Fusion）多路召回融合。
///
/// 算法：`score = Σ (1 / (k + rank_i))`。
///
/// # 参数
/// - `results_list`: 多路检索结果（每路仅包含页面路径列表，按原始排名排列）
/// - `k`: 平滑常数，通常取 60.0
///
/// # 返回
/// 融合后的页面评分列表，按分数降序排列
pub fn reciprocal_rank_fusion(
    results_list: &[Vec<String>],
    k: f64,
) -> Vec<(String, f64)> {
    let mut acc: HashMap<String, f64> = HashMap::new();

    for list in results_list {
        for (idx, page_path) in list.iter().enumerate() {
            let rank = (idx + 1) as f64;
            *acc.entry(page_path.clone()).or_insert(0.0) += 1.0 / (k + rank);
        }
    }

    let mut result: Vec<(String, f64)> = acc.into_iter().collect();

    // 按分数降序排列
    result.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    result
}

/// 基于余弦相似度对 Embedding 候选页面排序。
///
/// - 仅保留与 query 维度一致且相似度 > 0 的候选。
/// - 返回按相似度降序后的页面路径列表。
pub fn rank_embedding_paths_by_cosine(
    query_embedding: &[f32],
    candidates: &[PageEmbeddingRecord],
    limit: usize,
) -> Vec<String> {
    if query_embedding.is_empty() || candidates.is_empty() || limit == 0 {
        return Vec::new();
    }

    let mut scored: Vec<(&str, f64)> = candidates
        .iter()
        .filter_map(|item| {
            if item.embedding.len() != query_embedding.len() {
                return None;
            }
            let score = crate::db::cosine_similarity(query_embedding, &item.embedding);
            if score <= 0.0 {
                None
            } else {
                Some((item.page_path.as_str(), score))
            }
        })
        .collect();

    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(path, _)| path.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PageEmbeddingRecord;

    #[test]
    fn test_reciprocal_rank_fusion() {
        let fts = vec!["p1".into(), "p2".into()];
        let vec_res = vec!["p2".into(), "p1".into()];

        let fused = reciprocal_rank_fusion(&[fts, vec_res], 60.0);

        assert_eq!(fused.len(), 2);
        // p1 和 p2 分数应相等（在两者排名分别为 1 和 2 的情况下）
        assert!((fused[0].1 - fused[1].1).abs() < f64::EPSILON);
    }

    #[test]
    fn rank_embedding_paths_by_cosine_orders_by_similarity() {
        let query = vec![1.0_f32, 0.0, 0.0];
        let candidates = vec![
            PageEmbeddingRecord {
                page_path: "wiki/a.md".to_string(),
                embedding: vec![1.0, 0.0, 0.0],
            },
            PageEmbeddingRecord {
                page_path: "wiki/b.md".to_string(),
                embedding: vec![0.8, 0.2, 0.0],
            },
            PageEmbeddingRecord {
                page_path: "wiki/c.md".to_string(),
                embedding: vec![0.0, 1.0, 0.0],
            },
        ];

        let ranked = rank_embedding_paths_by_cosine(&query, &candidates, 3);
        assert_eq!(ranked, vec!["wiki/a.md", "wiki/b.md"]);
    }
}
