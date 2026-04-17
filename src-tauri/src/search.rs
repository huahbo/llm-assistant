//! RRF（Reciprocal Rank Fusion）多路召回融合。
//!
//! 纯函数，无 IO。参考：external/llm-wiki-main/crates/wiki-core/src/search.rs

use std::collections::HashMap;

/// 标准 RRF：`score(d) = Σ_i 1/(k + rank_i(d))`
/// 只出现在部分列表中的 doc，不出现的路径不计入分数（即跳过）。
///
/// # 参数
/// - `rank_lists`: 多路有序 doc id 列表（每路按相关性降序）
/// - `k`: 平滑常数，通常取 60.0
///
/// # 返回
/// 按融合分数降序排列的 `(doc_id, rrf_score)` 列表
pub fn reciprocal_rank_fusion(rank_lists: &[Vec<String>], k: f64) -> Vec<(String, f64)> {
    let mut acc: HashMap<String, f64> = HashMap::new();
    for ranks in rank_lists {
        for (idx, id) in ranks.iter().enumerate() {
            let rank = (idx + 1) as f64;
            *acc.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank);
        }
    }
    let mut v: Vec<(String, f64)> = acc.into_iter().collect();
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_fuses_multiple_lists() {
        let fts = vec!["a".into(), "b".into(), "c".into()];
        let links = vec!["b".into(), "a".into()];
        let popular = vec!["c".into(), "a".into()];
        let out = reciprocal_rank_fusion(&[fts, links, popular], 60.0);
        assert!(!out.is_empty());
        // "a" 出现在全部三路，应排第一
        assert_eq!(out[0].0, "a");
        // 分数应为正数
        assert!(out[0].1 > 0.0);
    }

    #[test]
    fn rrf_single_list_preserves_order() {
        let list = vec!["x".into(), "y".into(), "z".into()];
        let out = reciprocal_rank_fusion(&[list], 60.0);
        assert_eq!(out[0].0, "x");
        assert!(out[0].1 > out[1].1);
    }

    #[test]
    fn rrf_empty_lists_returns_empty() {
        let out = reciprocal_rank_fusion(&[], 60.0);
        assert!(out.is_empty());
        let out2 = reciprocal_rank_fusion(&[vec![], vec![]], 60.0);
        assert!(out2.is_empty());
    }
}
