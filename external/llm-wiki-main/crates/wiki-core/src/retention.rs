use crate::model::Claim;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// 遗忘曲线参数：半衰期越长，排序上「旧但仍重要」的权重衰越慢。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RetentionParams {
    pub half_life_days: f64,
}

impl Default for RetentionParams {
    fn default() -> Self {
        Self {
            half_life_days: 30.0,
        }
    }
}

/// 指数衰减 + 访问强化（简化版 Ebbinghaus：每次强化可重置 `last_reinforced_at`）。
pub fn retention_strength(claim: &Claim, now: OffsetDateTime, params: RetentionParams) -> f64 {
    let t0 = claim.last_reinforced_at.unwrap_or(claim.created_at);
    let elapsed = (now - t0).whole_seconds().max(0) as f64 / 86_400.0;
    let lambda = std::f64::consts::LN_2 / params.half_life_days.max(1e-6);
    let decay = (-lambda * elapsed).exp();
    let reinforcement = 1.0 + 0.1 * f64::from(claim.access_count.min(100));
    (decay * reinforcement).clamp(0.0, 1.0)
}
