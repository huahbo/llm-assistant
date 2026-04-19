use time::OffsetDateTime;

use crate::hooks::{NoopWikiHook, WikiHook};
use crate::memory::InMemoryStore;
use std::path::Path;
use wiki_storage::{StorageError, WikiRepository};

use wiki_core::{
    advance_tier, apply_time_decay_to_confidence, draft_from_session, redact_for_ingest,
    reinforce_claim, supersede_claim,
    walk_entities, AuditOperation, AuditRecord, Claim, ClaimId, ContradictionHint,
    CrystallizationDraft, DomainSchema, Entity, EntityId, GraphWalkOptions, LintFinding,
    LintSeverity, MemoryTier, QueryContext, RawArtifact, RelationKind, SchemaLoadError,
    Scope, SessionCrystallizationInput, SourceId, TypedEdge, WikiEvent,
    merge_sources_confidence, reciprocal_rank_fusion, retention_strength, RankedDoc,
};
use crate::search_ports::SearchPorts;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Schema(#[from] SchemaLoadError),
    #[error("claim not found: {0:?}")]
    ClaimNotFound(ClaimId),
    #[error("schema rejected relation: {0:?}")]
    RelationNotAllowed(RelationKind),
    #[error("schema rejected entity kind")]
    EntityKindNotAllowed,
    #[error("promotion threshold not met")]
    PromotionDenied,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// 编排 ingest / 写入 / 取代 / 结晶 / lint / 混合排序 的内存参考实现。
pub struct LlmWikiEngine<H: WikiHook = NoopWikiHook> {
    pub schema: DomainSchema,
    pub store: InMemoryStore,
    pub audits: Vec<AuditRecord>,
    pub outbox: Vec<WikiEvent>,
    hooks: H,
}

impl LlmWikiEngine<NoopWikiHook> {
    pub fn new(schema: DomainSchema) -> Self {
        Self::with_hooks(schema, NoopWikiHook)
    }

    /// 从 JSON 文件加载 [`DomainSchema`] 并创建引擎。
    pub fn from_schema_json_path(path: &Path) -> Result<Self, EngineError> {
        let schema = DomainSchema::from_json_path(path)?;
        Ok(Self::new(schema))
    }
}

impl<H: WikiHook> LlmWikiEngine<H> {
    pub fn with_hooks(schema: DomainSchema, hooks: H) -> Self {
        Self {
            schema,
            store: InMemoryStore::default(),
            audits: Vec::new(),
            outbox: Vec::new(),
            hooks,
        }
    }

    fn emit(&mut self, e: WikiEvent) {
        self.outbox.push(e.clone());
        self.hooks.on_event(&e);
    }

    fn audit(&mut self, op: AuditOperation, actor: &str, summary: impl Into<String>) {
        self.audits
            .push(AuditRecord::new(op, actor.to_string(), summary));
    }

    /// 原始层 ingest：先脱敏，再入库，写审计与事件。
    pub fn ingest_raw(
        &mut self,
        uri: impl Into<String>,
        body: &str,
        scope: Scope,
        actor: &str,
    ) -> SourceId {
        let (clean, findings) = redact_for_ingest(body);
        let art = RawArtifact::new(uri, clean, scope);
        let id = art.id;
        self.store.sources.insert(id, art);
        self.audit(
            AuditOperation::IngestSource,
            actor,
            format!("ingested {} redactions={}", id.0, findings.len()),
        );
        self.emit(WikiEvent::SourceIngested {
            source_id: id,
            redacted: !findings.is_empty(),
            at: OffsetDateTime::now_utc(),
        });
        if !findings.is_empty() {
            self.audit(
                AuditOperation::RedactSensitive,
                actor,
                format!("redaction findings={}", findings.len()),
            );
        }
        id
    }

    pub fn file_claim(
        &mut self,
        text: impl Into<String>,
        scope: Scope,
        tier: MemoryTier,
        actor: &str,
    ) -> ClaimId {
        let c = Claim::new(text, scope, tier);
        let id = c.id;
        self.store.claims.insert(id, c);
        self.audit(
            AuditOperation::WriteClaim,
            actor,
            format!("claim {}", id.0),
        );
        self.emit(WikiEvent::ClaimUpserted {
            claim_id: id,
            at: OffsetDateTime::now_utc(),
        });
        id
    }

    pub fn attach_sources(&mut self, claim_id: ClaimId, sources: &[SourceId]) -> Result<(), EngineError> {
        let claim = self
            .store
            .claims
            .get_mut(&claim_id)
            .ok_or(EngineError::ClaimNotFound(claim_id))?;
        for s in sources {
            if !claim.source_ids.contains(s) {
                claim.source_ids.push(*s);
            }
        }
        merge_sources_confidence(claim, sources.len());
        reinforce_claim(claim, OffsetDateTime::now_utc(), 0.03);
        Ok(())
    }

    pub fn supersede(
        &mut self,
        old_id: ClaimId,
        new_text: impl Into<String>,
        scope: Scope,
        tier: MemoryTier,
        actor: &str,
    ) -> Result<ClaimId, EngineError> {
        let old = self
            .store
            .claims
            .get_mut(&old_id)
            .ok_or(EngineError::ClaimNotFound(old_id))?
            .clone();
        let mut new_c = Claim::new(new_text, scope, tier);
        let now = OffsetDateTime::now_utc();
        if let Some(old_mut) = self.store.claims.get_mut(&old_id) {
            supersede_claim(old_mut, &mut new_c, now);
        }
        let new_id = new_c.id;
        self.store.claims.insert(new_id, new_c);
        self.audit(
            AuditOperation::SupersedeClaim,
            actor,
            format!("{} supersedes {}", new_id.0, old.id.0),
        );
        self.emit(WikiEvent::ClaimSuperseded {
            old: old_id,
            new: new_id,
            at: now,
        });
        Ok(new_id)
    }

    pub fn add_entity(&mut self, entity: Entity) -> Result<(), EngineError> {
        if !self.schema.entity_kind_allowed(&entity.kind) {
            return Err(EngineError::EntityKindNotAllowed);
        }
        self.store.entities.insert(entity.id, entity);
        Ok(())
    }

    pub fn add_edge(&mut self, edge: TypedEdge) -> Result<(), EngineError> {
        if !self.schema.relation_allowed(&edge.relation) {
            return Err(EngineError::RelationNotAllowed(edge.relation.clone()));
        }
        self.store.edges.push(edge);
        Ok(())
    }

    /// 若质量与置信度达到 Schema 阈值，则沿巩固阶梯晋升一级。
    pub fn promote_if_qualified(&mut self, claim_id: ClaimId, actor: &str) -> Result<(), EngineError> {
        let claim = self
            .store
            .claims
            .get(&claim_id)
            .ok_or(EngineError::ClaimNotFound(claim_id))?;
        if claim.confidence < self.schema.min_confidence_to_promote
            || claim.quality_score < self.schema.min_quality_to_crystallize
        {
            return Err(EngineError::PromotionDenied);
        }
        let claim = self.store.claims.get_mut(&claim_id).unwrap();
        advance_tier(claim);
        self.audit(
            AuditOperation::WriteClaim,
            actor,
            format!("promoted claim {}", claim_id.0),
        );
        Ok(())
    }

    pub fn set_claim_quality(&mut self, claim_id: ClaimId, q: f64) -> Result<(), EngineError> {
        let c = self
            .store
            .claims
            .get_mut(&claim_id)
            .ok_or(EngineError::ClaimNotFound(claim_id))?;
        c.quality_score = q.clamp(0.0, 1.0);
        Ok(())
    }

    /// 结晶：生成 `WikiPage` 草稿与候选断言文本。
    pub fn crystallize(
        &mut self,
        input: SessionCrystallizationInput,
        actor: &str,
    ) -> Result<CrystallizationDraft, EngineError> {
        let draft = draft_from_session(input);
        let page_id = draft.page.id;
        self.store.pages.insert(page_id, draft.page.clone());
        self.audit(
            AuditOperation::CrystallizeSession,
            actor,
            format!("crystallized page {}", page_id.0),
        );
        self.emit(WikiEvent::SessionCrystallized {
            page_id,
            at: OffsetDateTime::now_utc(),
        });
        Ok(draft)
    }

    pub fn hybrid_rrf(
        &self,
        bm25_ids: Vec<String>,
        vector_ids: Vec<String>,
        graph_ids: Vec<String>,
        k: f64,
    ) -> Vec<RankedDoc> {
        reciprocal_rank_fusion(&[bm25_ids, vector_ids, graph_ids], k)
    }

    /// 内置内存三路 stub → RRF → 保留强度加权（不写审计）。
    pub fn query_ranked_memory(&self, ctx: &QueryContext<'_>, now: OffsetDateTime) -> Vec<(String, f64)> {
        let ports = crate::InMemorySearchPorts {
            store: &self.store,
        };
        query_ranked_with_ports(&self.schema, &self.store, ctx, &ports, now)
    }

    /// 三路端口召回 → RRF →（对 `claim:`）保留强度加权；并写审计与 `QueryServed` 事件。
    pub fn query_pipeline_memory(
        &mut self,
        ctx: &QueryContext<'_>,
        now: OffsetDateTime,
        actor: &str,
    ) -> Vec<(String, f64)> {
        let ranked = self.query_ranked_memory(ctx, now);
        let top: Vec<String> = ranked.iter().take(24).map(|(id, _)| id.clone()).collect();
        self.record_query(ctx.query, top, actor);
        ranked
    }

    /// 与 [`query_ranked_with_ports`] 相同，但接收外部实现的检索端口。
    pub fn query_pipeline_with_ports<P: SearchPorts>(
        &mut self,
        ctx: &QueryContext<'_>,
        ports: &P,
        now: OffsetDateTime,
        actor: &str,
    ) -> Vec<(String, f64)> {
        let ranked = query_ranked_with_ports(&self.schema, &self.store, ctx, ports, now);
        let top: Vec<String> = ranked.iter().take(24).map(|(id, _)| id.clone()).collect();
        self.record_query(ctx.query, top, actor);
        ranked
    }

    pub fn expand_graph(&self, seeds: &[EntityId], opts: &GraphWalkOptions) -> Vec<EntityId> {
        let snap = self.store.graph_snapshot();
        walk_entities(&snap, seeds, opts)
    }

    /// 结合 RRF 与保留强度：仅对 `claim:` doc 二次加权；`page:` / `entity:` 乘子为 1。
    pub fn rank_claims_with_retention(
        &self,
        fused: &[RankedDoc],
        now: OffsetDateTime,
    ) -> Vec<(String, f64)> {
        self.rank_docs_with_retention(fused, now)
    }

    fn rank_docs_with_retention(&self, fused: &[RankedDoc], now: OffsetDateTime) -> Vec<(String, f64)> {
        rank_fused_with_retention(&self.schema, &self.store, fused, now)
    }

    pub fn run_basic_lint(&mut self, actor: &str) -> Vec<LintFinding> {
        let mut findings = Vec::new();
        for c in self.store.claims.values() {
            if c.quality_score < 0.35 {
                findings.push(LintFinding {
                    code: "quality.low".into(),
                    message: "claim quality below threshold".into(),
                    severity: LintSeverity::Warn,
                    subject: Some(c.id.0.to_string()),
                });
            }
            if c.stale {
                findings.push(LintFinding {
                    code: "lifecycle.stale".into(),
                    message: "stale claim retained for audit".into(),
                    severity: LintSeverity::Info,
                    subject: Some(c.id.0.to_string()),
                });
            }
        }
        let titles: std::collections::HashSet<String> = self
            .store
            .pages
            .values()
            .map(|p| p.title.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        for p in self.store.pages.values_mut() {
            p.refresh_outbound_links();
            if p.title.trim().is_empty() {
                findings.push(LintFinding {
                    code: "page.empty_title".into(),
                    message: "wiki page has empty title".into(),
                    severity: LintSeverity::Error,
                    subject: Some(p.id.0.to_string()),
                });
            }
            for link in &p.outbound_page_titles {
                if !titles.contains(link) {
                    findings.push(LintFinding {
                        code: "page.broken_wikilink".into(),
                        message: format!("broken wikilink: {link}"),
                        severity: LintSeverity::Warn,
                        subject: Some(p.id.0.to_string()),
                    });
                }
            }
        }
        let mut inbound_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for p in self.store.pages.values() {
            for link in &p.outbound_page_titles {
                *inbound_count.entry(link.clone()).or_insert(0) += 1;
            }
        }
        for p in self.store.pages.values() {
            if inbound_count.get(&p.title).copied().unwrap_or(0) == 0 {
                findings.push(LintFinding {
                    code: "page.orphan".into(),
                    message: "page has no inbound wikilinks".into(),
                    severity: LintSeverity::Info,
                    subject: Some(p.id.0.to_string()),
                });
            }
        }
        let mut page_text = String::new();
        for p in self.store.pages.values() {
            page_text.push_str(&p.markdown.to_ascii_lowercase());
            page_text.push('\n');
        }
        for c in self.store.claims.values() {
            if c.stale {
                findings.push(LintFinding {
                    code: "claim.stale".into(),
                    message: "stale claim should be reconciled in wiki pages".into(),
                    severity: LintSeverity::Warn,
                    subject: Some(c.id.0.to_string()),
                });
            } else if !claim_has_page_reference(&c.text, &page_text) {
                findings.push(LintFinding {
                    code: "xref.missing".into(),
                    message: "claim keywords are not referenced in current pages".into(),
                    severity: LintSeverity::Info,
                    subject: Some(c.id.0.to_string()),
                });
            }
        }
        self.audit(
            AuditOperation::RunLint,
            actor,
            format!("lint findings={}", findings.len()),
        );
        self.emit(WikiEvent::LintRunFinished {
            findings: findings.len(),
            at: OffsetDateTime::now_utc(),
        });
        findings
    }

    pub fn naive_contradiction_pairs(&self) -> Vec<ContradictionHint> {
        let mut hints = Vec::new();
        let ids: Vec<ClaimId> = self.store.claims.keys().copied().collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let a = &self.store.claims[&ids[i]];
                let b = &self.store.claims[&ids[j]];
                if a.stale || b.stale {
                    continue;
                }
                if contradicts_heuristic(&a.text, &b.text) {
                    hints.push(ContradictionHint {
                        a: a.id,
                        b: b.id,
                        reason: "heuristic negation / mismatch".into(),
                    });
                }
            }
        }
        hints
    }

    /// 批处理：对所有断言按统一半衰期衰减置信度（遗忘曲线的时间分量）。
    pub fn apply_confidence_decay_all(&mut self, now: OffsetDateTime, half_life_days: f64) {
        for c in self.store.claims.values_mut() {
            apply_time_decay_to_confidence(c, now, half_life_days);
        }
    }

    pub fn record_query(
        &mut self,
        query_fingerprint: impl Into<String>,
        top_doc_ids: Vec<String>,
        actor: &str,
    ) {
        let fp = query_fingerprint.into();
        self.audit(
            AuditOperation::RunQuery,
            actor,
            format!("query fp={fp}"),
        );
        self.emit(WikiEvent::QueryServed {
            query_fingerprint: fp,
            top_doc_ids,
            at: OffsetDateTime::now_utc(),
        });
    }

    pub fn load_from_repo<R: WikiRepository>(
        schema: DomainSchema,
        repo: &R,
        hooks: H,
    ) -> Result<Self, EngineError> {
        let snap = repo.load_snapshot()?;
        Ok(Self {
            schema,
            store: InMemoryStore::from_snapshot(snap.clone()),
            audits: snap.audits,
            outbox: Vec::new(),
            hooks,
        })
    }

    pub fn save_to_repo<R: WikiRepository>(&self, repo: &R) -> Result<(), EngineError> {
        let snap = self.store.to_snapshot(&self.audits);
        repo.save_snapshot(&snap)?;
        Ok(())
    }

    pub fn flush_outbox_to_repo<R: WikiRepository>(
        &mut self,
        repo: &R,
    ) -> Result<usize, EngineError> {
        self.flush_outbox_to_repo_with_policy(repo, 128, 3)
    }

    pub fn flush_outbox_to_repo_with_policy<R: WikiRepository>(
        &mut self,
        repo: &R,
        batch_size: usize,
        retry_count: usize,
    ) -> Result<usize, EngineError> {
        let mut n = 0usize;
        let size = batch_size.max(1);
        let mut start = 0usize;
        while start < self.outbox.len() {
            let end = usize::min(start + size, self.outbox.len());
            for event in &self.outbox[start..end] {
                let mut last_err: Option<EngineError> = None;
                for _ in 0..=retry_count {
                    match repo.append_outbox(event) {
                        Ok(()) => {
                            last_err = None;
                            break;
                        }
                        Err(err) => {
                            last_err = Some(err.into());
                        }
                    }
                }
                if let Some(err) = last_err {
                    return Err(err);
                }
                n += 1;
            }
            start = end;
        }
        self.outbox.clear();
        Ok(n)
    }
}

/// 自由函数：三路召回 + RRF + 保留强度（不写审计），避免 `SearchPorts` 与 `&mut self` 交错借用。
pub fn query_ranked_with_ports<P: SearchPorts>(
    schema: &DomainSchema,
    store: &InMemoryStore,
    ctx: &QueryContext<'_>,
    ports: &P,
    now: OffsetDateTime,
) -> Vec<(String, f64)> {
    let lim = ctx.per_stream_limit;
    let bm25 = ports.bm25_ranked_ids(ctx.query, lim);
    let vector = ports.vector_ranked_ids(ctx.query, lim);
    let graph = ports.graph_ranked_ids(ctx.query, lim);
    let fused = reciprocal_rank_fusion(&[bm25, vector, graph], ctx.rrf_k);
    rank_fused_with_retention(schema, store, &fused, now)
}

fn rank_fused_with_retention(
    schema: &DomainSchema,
    store: &InMemoryStore,
    fused: &[RankedDoc],
    now: OffsetDateTime,
) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = fused
        .iter()
        .map(|d| {
            let bonus = if let Some(cid) = parse_claim_doc_id(&d.id) {
                store
                    .claims
                    .get(&cid)
                    .map(|c| {
                        let hl = schema
                            .tier_half_life_days
                            .get(&c.tier)
                            .copied()
                            .unwrap_or(schema.default_retention.half_life_days);
                        let rp = wiki_core::RetentionParams {
                            half_life_days: hl,
                        };
                        retention_strength(c, now, rp)
                    })
                    .unwrap_or(1.0)
            } else {
                1.0
            };
            (d.id.clone(), d.rrf_score * bonus)
        })
        .collect();
    out.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    out
}

fn parse_claim_doc_id(s: &str) -> Option<ClaimId> {
    let rest = s.strip_prefix("claim:")?;
    let u = uuid::Uuid::parse_str(rest).ok()?;
    Some(ClaimId(u))
}

fn contradicts_heuristic(a: &str, b: &str) -> bool {
    let la = a.to_ascii_lowercase();
    let lb = b.to_ascii_lowercase();
    (la.contains("不是") && lb.contains("是"))
        || (lb.contains("不是") && la.contains("是"))
        || (la.contains("cannot") && lb.contains("can "))
        || (lb.contains("cannot") && la.contains("can "))
}

fn claim_has_page_reference(claim_text: &str, page_text: &str) -> bool {
    let keys: Vec<String> = claim_text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|x| x.trim().to_ascii_lowercase())
        .filter(|x| x.len() >= 4)
        .collect();
    if keys.is_empty() {
        return true;
    }
    keys.iter().any(|k| page_text.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiki_core::WikiPage;
    use wiki_storage::{SqliteRepository, WikiRepository};

    #[test]
    fn ingest_and_file_claim_flow() {
        let mut eng = LlmWikiEngine::new(DomainSchema::permissive_default());
        let sid = eng.ingest_raw(
            "file:///tmp/note.md",
            "项目使用 Redis\nAuthorization: Bearer secret",
            Scope::Private {
                agent_id: "a1".into(),
            },
            "tester",
        );
        let cid = eng.file_claim(
            "项目使用 Redis",
            Scope::Private {
                agent_id: "a1".into(),
            },
            MemoryTier::Working,
            "tester",
        );
        eng.attach_sources(cid, &[sid]).unwrap();
        assert!(eng.store.sources[&sid].body.contains("REDACTED"));
        assert!(!eng.store.claims[&cid].source_ids.is_empty());
    }

    #[test]
    fn loads_schema_from_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schema.json");
        let s = DomainSchema::permissive_default();
        std::fs::write(&path, serde_json::to_vec(&s).unwrap()).unwrap();
        let eng = LlmWikiEngine::from_schema_json_path(&path).unwrap();
        assert_eq!(eng.schema.title, "default-permissive");
    }

    #[test]
    fn query_pipeline_finds_claims() {
        let mut eng = LlmWikiEngine::new(DomainSchema::permissive_default());
        eng.file_claim(
            "Redis caching for API",
            Scope::Private {
                agent_id: "a".into(),
            },
            MemoryTier::Semantic,
            "t",
        );
        let ctx = QueryContext::new("Redis API").with_per_stream_limit(20);
        let now = OffsetDateTime::now_utc();
        let ranked = eng.query_pipeline_memory(&ctx, now, "t");
        assert!(!ranked.is_empty());
        assert!(ranked[0].0.starts_with("claim:"));
    }

    #[test]
    fn supersede_chain() {
        let mut eng = LlmWikiEngine::new(DomainSchema::permissive_default());
        let old = eng.file_claim(
            "v1",
            Scope::Shared {
                team_id: "t1".into(),
            },
            MemoryTier::Semantic,
            "u",
        );
        let new = eng.supersede(old, "v2", Scope::Shared { team_id: "t1".into() }, MemoryTier::Semantic, "u").unwrap();
        assert!(eng.store.claims[&old].stale);
        assert_eq!(eng.store.claims[&new].supersedes, Some(old));
    }

    #[test]
    fn lint_reports_broken_wikilink() {
        let mut eng = LlmWikiEngine::new(DomainSchema::permissive_default());
        let p = WikiPage::new(
            "Alpha",
            "Link to [[MissingPage]]",
            Scope::Private {
                agent_id: "a".into(),
            },
        );
        eng.store.pages.insert(p.id, p);
        let findings = eng.run_basic_lint("tester");
        assert!(findings.iter().any(|f| f.code == "page.broken_wikilink"));
    }

    #[test]
    fn persist_and_reload_snapshot_and_outbox() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("wiki.db");
        let repo = SqliteRepository::open(&db).unwrap();

        let mut eng = LlmWikiEngine::new(DomainSchema::permissive_default());
        let c1 = eng.file_claim(
            "Postgres is default DB",
            Scope::Shared {
                team_id: "t".into(),
            },
            MemoryTier::Semantic,
            "tester",
        );
        let _c2 = eng
            .supersede(
                c1,
                "SQLite is default DB",
                Scope::Shared { team_id: "t".into() },
                MemoryTier::Semantic,
                "tester",
            )
            .unwrap();
        let _ = eng.query_pipeline_memory(&QueryContext::new("Postgres"), OffsetDateTime::now_utc(), "tester");
        eng.save_to_repo(&repo).unwrap();
        let n = eng.flush_outbox_to_repo(&repo).unwrap();
        assert!(n > 0);

        let reloaded = LlmWikiEngine::load_from_repo(
            DomainSchema::permissive_default(),
            &repo,
            NoopWikiHook,
        )
        .unwrap();
        assert_eq!(reloaded.store.claims.len(), 2);

        let ndjson = repo.export_outbox_ndjson().unwrap();
        assert!(ndjson.contains("query_served"));
        assert!(ndjson.contains("claim_upserted"));
        assert!(ndjson.contains("claim_superseded"));
    }
}
