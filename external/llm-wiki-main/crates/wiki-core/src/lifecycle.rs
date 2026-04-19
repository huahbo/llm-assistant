//! 置信度、取代链、强化与层级晋升（巩固流水线）。

use crate::model::{Claim, MemoryTier};
use time::OffsetDateTime;

/// 新断言显式取代旧断言：旧条目标记 stale，保留链式 `supersedes`。
pub fn supersede_claim(old: &mut Claim, new_claim: &mut Claim, now: OffsetDateTime) {
    old.stale = true;
    new_claim.supersedes = Some(old.id);
    new_claim.last_reinforced_at = Some(now);
    bump_confidence(new_claim, 0.1, 0.95);
}

/// 访问或新来源确认时调用：强化时间戳与置信度。
pub fn reinforce_claim(claim: &mut Claim, now: OffsetDateTime, delta_confidence: f64) {
    claim.last_reinforced_at = Some(now);
    claim.access_count = claim.access_count.saturating_add(1);
    bump_confidence(claim, delta_confidence, 0.99);
}

/// 仅时间流逝导致的置信度衰减（与检索排序中的 retention 分工：一个偏「相信度」，一个偏「可检索性」）。
pub fn apply_time_decay_to_confidence(claim: &mut Claim, now: OffsetDateTime, half_life_days: f64) {
    let elapsed = (now - claim.created_at).whole_seconds().max(0) as f64 / 86_400.0;
    let lambda = std::f64::consts::LN_2 / half_life_days.max(1e-6);
    let decay = (-lambda * elapsed).exp();
    claim.confidence = (claim.confidence * decay).clamp(0.05, 1.0);
}

/// 将信息沿巩固阶梯上移一格（由引擎在证据足够时调用）。
pub fn advance_tier(claim: &mut Claim) {
    claim.tier = match claim.tier {
        MemoryTier::Working => MemoryTier::Episodic,
        MemoryTier::Episodic => MemoryTier::Semantic,
        MemoryTier::Semantic => MemoryTier::Procedural,
        MemoryTier::Procedural => MemoryTier::Procedural,
    };
}

fn bump_confidence(claim: &mut Claim, d: f64, cap: f64) {
    claim.confidence = (claim.confidence + d).clamp(0.0, cap);
}

/// 当新来源支持同一断言时，按来源数量对置信度做凹函数增长（避免线性爆炸）。
pub fn merge_sources_confidence(claim: &mut Claim, extra_sources: usize) {
    if extra_sources == 0 {
        return;
    }
    let n = claim.source_ids.len() + extra_sources;
    let fused = 1.0 - (-(n as f64) * 0.35).exp();
    claim.confidence = claim.confidence.max(fused).min(0.99);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Scope;

    #[test]
    fn supersession_marks_stale() {
        let mut a = Claim::new("旧结论", Scope::Private { agent_id: "a".into() }, MemoryTier::Semantic);
        let mut b = Claim::new("新结论", Scope::Private { agent_id: "a".into() }, MemoryTier::Semantic);
        let now = OffsetDateTime::now_utc();
        supersede_claim(&mut a, &mut b, now);
        assert!(a.stale);
        assert_eq!(b.supersedes, Some(a.id));
    }
}
