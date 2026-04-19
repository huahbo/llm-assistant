//! 混合检索：三路有序 doc id 列表 → Reciprocal Rank Fusion（RRF）。

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RankedDoc {
    pub id: String,
    pub rrf_score: f64,
}

/// 标准 RRF：`score(d) = sum_i 1/(k + rank_i(d))`，仅出现在某些流中的 rank 可视为缺失（此处省略该项）。
pub fn reciprocal_rank_fusion(rank_lists: &[Vec<String>], k: f64) -> Vec<RankedDoc> {
    let mut acc: HashMap<String, f64> = HashMap::new();
    for ranks in rank_lists {
        for (idx, id) in ranks.iter().enumerate() {
            let rank = (idx + 1) as f64;
            *acc.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank);
        }
    }
    let mut v: Vec<RankedDoc> = acc
        .into_iter()
        .map(|(id, rrf_score)| RankedDoc { id, rrf_score })
        .collect();
    v.sort_by(|a, b| {
        b.rrf_score
            .partial_cmp(&a.rrf_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_orders_by_fusion() {
        let bm25 = vec!["a".into(), "b".into(), "c".into()];
        let vector = vec!["b".into(), "a".into()];
        let graph = vec!["c".into(), "a".into()];
        let out = reciprocal_rank_fusion(&[bm25, vector, graph], 60.0);
        assert!(!out.is_empty());
        // "a" appears in all three → should be strong
        assert_eq!(out[0].id, "a");
    }
}
