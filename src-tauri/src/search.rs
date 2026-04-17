use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 页面检索评分结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageScore {
    pub page_path: String,
    pub score: f64,
}

/// RRF（Reciprocal Rank Fusion）多路召回融合。
///
/// 算法：`score = Σ (1 / (k + rank_i))`，此处 `k = 60`。
///
/// # 参数
/// - `fts_results`: 全文检索结果列表
/// - `vector_results`: 向量检索结果列表
///
/// # 返回
/// 融合后的页面评分列表，按分数降序排列
pub fn reciprocal_rank_fusion(
    fts_results: &[PageScore],
    vector_results: &[PageScore],
) -> Vec<PageScore> {
    let k = 60.0;
    let mut acc: HashMap<String, f64> = HashMap::new();

    // 统计 FTS 排名分数
    for (idx, item) in fts_results.iter().enumerate() {
        let rank = (idx + 1) as f64;
        *acc.entry(item.page_path.clone()).or_insert(0.0) += 1.0 / (k + rank);
    }

    // 统计 Vector 排名分数
    for (idx, item) in vector_results.iter().enumerate() {
        let rank = (idx + 1) as f64;
        *acc.entry(item.page_path.clone()).or_insert(0.0) += 1.0 / (k + rank);
    }

    let mut result: Vec<PageScore> = acc
        .into_iter()
        .map(|(page_path, score)| PageScore { page_path, score })
        .collect();

    // 按分数降序排列
    result.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reciprocal_rank_fusion() {
        let fts = vec![
            PageScore { page_path: "p1".into(), score: 0.9 },
            PageScore { page_path: "p2".into(), score: 0.8 },
        ];
        let vec_res = vec![
            PageScore { page_path: "p2".into(), score: 0.9 },
            PageScore { page_path: "p1".into(), score: 0.8 },
        ];

        let fused = reciprocal_rank_fusion(&fts, &vec_res);

        assert_eq!(fused.len(), 2);
        // p1 和 p2 分数应相等（在两者排名分别为 1 和 2 的情况下）
        assert!((fused[0].score - fused[1].score).abs() < f64::EPSILON);
    }
}
