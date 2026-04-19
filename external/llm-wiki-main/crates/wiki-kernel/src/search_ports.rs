//! 三路检索端口：BM25 / 向量 / 图 由外部实现；内存 stub 供开发与单测。

use crate::memory::InMemoryStore;
use wiki_core::{ClaimId, EntityId, PageId};

/// 混合检索的三路有序 doc id（约定：`claim:`、`page:`、`entity:` 前缀）。
pub trait SearchPorts {
    fn bm25_ranked_ids(&self, query: &str, limit: usize) -> Vec<String>;
    fn vector_ranked_ids(&self, query: &str, limit: usize) -> Vec<String>;
    fn graph_ranked_ids(&self, query: &str, limit: usize) -> Vec<String>;
}

/// 空三路（用于只测 RRF  plumbing）。
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptySearchPorts;

impl SearchPorts for EmptySearchPorts {
    fn bm25_ranked_ids(&self, _query: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }

    fn vector_ranked_ids(&self, _query: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }

    fn graph_ranked_ids(&self, _query: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
}

fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 1)
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn score_text_lc(haystack_lc: &str, tokens: &[String]) -> usize {
    tokens.iter().filter(|t| haystack_lc.contains(t.as_str())).count()
}

/// 基于子串/token 重叠的内存 stub：BM25 与 vector 两路顺序刻意不同以检验 RRF。
#[derive(Debug, Clone, Copy)]
pub struct InMemorySearchPorts<'a> {
    pub store: &'a InMemoryStore,
}

impl SearchPorts for InMemorySearchPorts<'_> {
    fn bm25_ranked_ids(&self, query: &str, limit: usize) -> Vec<String> {
        let tokens = query_tokens(query);
        let mut scored: Vec<(String, usize)> = self.collect_doc_scores(&tokens);
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored.into_iter().map(|(id, _)| id).take(limit).collect()
    }

    fn vector_ranked_ids(&self, query: &str, limit: usize) -> Vec<String> {
        let tokens = query_tokens(query);
        let mut scored: Vec<(String, usize)> = self.collect_doc_scores(&tokens);
        // 同分按 id 逆序，模拟与 BM25 不同的重排
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        scored.into_iter().map(|(id, _)| id).take(limit).collect()
    }

    fn graph_ranked_ids(&self, query: &str, limit: usize) -> Vec<String> {
        let tokens = query_tokens(query);
        let mut scored: Vec<(String, usize)> = Vec::new();
        for e in self.store.entities.values() {
            let label_lc = e.label.to_ascii_lowercase();
            let s = score_text_lc(&label_lc, &tokens);
            if s > 0 {
                scored.push((format_entity_doc_id(e.id), s));
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        scored.into_iter().map(|(id, _)| id).take(limit).collect()
    }
}

impl InMemorySearchPorts<'_> {
    fn collect_doc_scores(&self, tokens: &[String]) -> Vec<(String, usize)> {
        let mut scored: Vec<(String, usize)> = Vec::new();
        for c in self.store.claims.values() {
            if c.stale {
                continue;
            }
            let t = c.text.to_ascii_lowercase();
            let s = score_text_lc(&t, tokens);
            if s > 0 {
                scored.push((format_claim_doc_id(c.id), s));
            }
        }
        for p in self.store.pages.values() {
            let blob = format!("{} {}", p.title, p.markdown).to_ascii_lowercase();
            let s = score_text_lc(&blob, tokens);
            if s > 0 {
                scored.push((format_page_doc_id(p.id), s));
            }
        }
        scored
    }
}

pub fn format_claim_doc_id(id: ClaimId) -> String {
    format!("claim:{}", id.0)
}

pub fn format_page_doc_id(id: PageId) -> String {
    format!("page:{}", id.0)
}

pub fn format_entity_doc_id(id: EntityId) -> String {
    format!("entity:{}", id.0)
}
