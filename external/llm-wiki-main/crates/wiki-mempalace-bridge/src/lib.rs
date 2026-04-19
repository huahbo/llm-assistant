//! 与 `rust-mempalace` 的对接面：这里保留较细的 `MempalaceWikiSink`；更自然的集成是
//! 在引入 `wiki-kernel` 后实现其中的 `WikiHook` trait，在 `on_event` 里把 `WikiEvent` 映射到
//! Palace 的 `drawers` / `kg_facts` / 向量索引等 API。
//!
//! 若本机尚未 clone `rust-mempalace`，可只依赖本 crate 的 trait 边界，或在本 crate 增加 `impl` 与 `path` 依赖。
//!
//! 建议路径：`/Users/brzhang/Projects/rust-mempalace`。在本 crate 的 `Cargo.toml` 中加入
//! `rust-mempalace = { path = "../../../../Projects/rust-mempalace" }`（相对 `OpenClawWorkSpace/llm-wiki/crates/wiki-mempalace-bridge` 时约为 `../../../../Projects/rust-mempalace`，请按实际目录调整），
//! 然后为 `MempalaceWikiSink` 提供具体类型。

use wiki_core::{Claim, ClaimId, Scope, SourceId};
use wiki_core::WikiEvent;

/// 写入外部「记忆宫殿」引擎的最小事件面（ingest / reinforce / 淘汰）。
pub trait MempalaceWikiSink: Send + Sync {
    fn on_claim_upserted(&self, claim: &Claim) -> Result<(), MempalaceError>;
    fn on_claim_event(&self, claim_id: ClaimId) -> Result<(), MempalaceError>;
    fn on_claim_superseded(&self, old: ClaimId, new: ClaimId) -> Result<(), MempalaceError>;
    fn on_source_linked(&self, source_id: SourceId, claim_id: ClaimId) -> Result<(), MempalaceError>;
    fn scope_filter(&self, scope: &Scope) -> bool;
}

#[derive(Debug, thiserror::Error)]
pub enum MempalaceError {
    #[error("external memory backend error: {0}")]
    Backend(String),
}

/// 默认无操作，便于内核单测与不启用 mempalace 时编译通过。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopMempalace;

impl MempalaceWikiSink for NoopMempalace {
    fn on_claim_event(&self, _claim_id: ClaimId) -> Result<(), MempalaceError> {
        Ok(())
    }

    fn on_claim_superseded(&self, _old: ClaimId, _new: ClaimId) -> Result<(), MempalaceError> {
        Ok(())
    }

    fn on_claim_upserted(&self, _claim: &Claim) -> Result<(), MempalaceError> {
        Ok(())
    }

    fn on_source_linked(&self, _source_id: SourceId, _claim_id: ClaimId) -> Result<(), MempalaceError> {
        Ok(())
    }

    fn scope_filter(&self, _scope: &Scope) -> bool {
        true
    }
}

pub fn consume_outbox_ndjson<S: MempalaceWikiSink>(
    sink: &S,
    ndjson: &str,
) -> Result<usize, MempalaceError> {
    let mut count = 0usize;
    for line in ndjson.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let event: WikiEvent = serde_json::from_str(line)
            .map_err(|e| MempalaceError::Backend(format!("invalid event json: {e}")))?;
        match event {
            WikiEvent::ClaimUpserted { claim_id, .. } => {
                sink.on_claim_event(claim_id)?;
                count += 1;
            }
            WikiEvent::ClaimSuperseded { old, new, .. } => {
                sink.on_claim_superseded(old, new)?;
                count += 1;
            }
            _ => {}
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use wiki_core::WikiEvent;

    #[derive(Clone, Default)]
    struct CountingSink {
        upserted: Arc<AtomicUsize>,
        superseded: Arc<AtomicUsize>,
    }

    impl MempalaceWikiSink for CountingSink {
        fn on_claim_upserted(&self, _claim: &Claim) -> Result<(), MempalaceError> {
            Ok(())
        }

        fn on_claim_event(&self, _claim_id: ClaimId) -> Result<(), MempalaceError> {
            self.upserted.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn on_claim_superseded(&self, _old: ClaimId, _new: ClaimId) -> Result<(), MempalaceError> {
            self.superseded.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn on_source_linked(&self, _source_id: SourceId, _claim_id: ClaimId) -> Result<(), MempalaceError> {
            Ok(())
        }

        fn scope_filter(&self, _scope: &Scope) -> bool {
            true
        }
    }

    #[test]
    fn consumes_ndjson_and_dispatches_claim_events() {
        let sink = CountingSink::default();
        let a = ClaimId(uuid::Uuid::new_v4());
        let b = ClaimId(uuid::Uuid::new_v4());

        let lines = vec![
            serde_json::to_string(&WikiEvent::ClaimUpserted {
                claim_id: a,
                at: time::OffsetDateTime::now_utc(),
            })
            .unwrap(),
            serde_json::to_string(&WikiEvent::ClaimSuperseded {
                old: a,
                new: b,
                at: time::OffsetDateTime::now_utc(),
            })
            .unwrap(),
        ]
        .join("\n");

        let n = consume_outbox_ndjson(&sink, &lines).unwrap();
        assert_eq!(n, 2);
        assert_eq!(sink.upserted.load(Ordering::SeqCst), 1);
        assert_eq!(sink.superseded.load(Ordering::SeqCst), 1);
    }
}
