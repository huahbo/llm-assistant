use clap::{Parser, Subcommand};
use std::path::PathBuf;
use time::OffsetDateTime;
use wiki_core::{DomainSchema, MemoryTier, QueryContext, Scope, SessionCrystallizationInput, WikiPage};
use wiki_kernel::{write_lint_report, write_projection, LlmWikiEngine, NoopWikiHook};
use wiki_mempalace_bridge::{consume_outbox_ndjson, MempalaceError, MempalaceWikiSink};
use wiki_storage::{SqliteRepository, WikiRepository};

mod banner;
mod llm;

#[derive(Parser)]
#[command(name = "wiki")]
#[command(
    about = "rust-llm-wiki — LLM Wiki v2 CLI (Rust kernel, outbox, MemPalace bridge)",
    long_about = None
)]
struct Cli {
    #[arg(long, default_value = "wiki.db")]
    db: PathBuf,
    #[arg(long)]
    schema: Option<PathBuf>,
    #[arg(long)]
    wiki_dir: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    sync_wiki: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Ingest {
        uri: String,
        body: String,
        #[arg(long, default_value = "private:cli")]
        scope: String,
    },
    FileClaim {
        text: String,
        #[arg(long, default_value = "private:cli")]
        scope: String,
        #[arg(long, default_value = "working")]
        tier: String,
    },
    SupersedeClaim {
        old_claim_id: String,
        new_text: String,
        #[arg(long, default_value = "private:cli")]
        scope: String,
        #[arg(long, default_value = "working")]
        tier: String,
    },
    Query {
        query: String,
        #[arg(long, default_value_t = 60.0)]
        rrf_k: f64,
        #[arg(long, default_value_t = 50)]
        per_stream_limit: usize,
        #[arg(long, default_value_t = false)]
        write_page: bool,
        #[arg(long)]
        page_title: Option<String>,
    },
    Lint,
    Promote {
        claim_id: String,
    },
    Crystallize {
        question: String,
        #[arg(long = "finding")]
        findings: Vec<String>,
        #[arg(long = "file")]
        files: Vec<String>,
        #[arg(long = "lesson")]
        lessons: Vec<String>,
    },
    ExportOutboxNdjson,
    ExportOutboxNdjsonFrom {
        #[arg(long, default_value_t = 0)]
        last_id: i64,
    },
    AckOutbox {
        #[arg(long)]
        up_to_id: i64,
        #[arg(long)]
        consumer_tag: String,
    },
    ConsumeToMempalace {
        #[arg(long, default_value_t = 0)]
        last_id: i64,
    },
    LlmSmoke {
        #[arg(long, default_value = "llm-config.toml")]
        config: PathBuf,
        #[arg(long, default_value = "Say 'ok' only.")]
        prompt: String,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    banner::print_startup_banner();
    let cli = Cli::parse();
    let wiki_root = cli.wiki_dir.clone();
    let sync_wiki = cli.sync_wiki;
    let repo = SqliteRepository::open(&cli.db)?;
    let schema = if let Some(path) = &cli.schema {
        DomainSchema::from_json_path(path)?
    } else {
        DomainSchema::permissive_default()
    };
    let mut eng = LlmWikiEngine::load_from_repo(schema, &repo, NoopWikiHook)?;

    match cli.cmd {
        Cmd::Ingest { uri, body, scope } => {
            let sid = eng.ingest_raw(uri, &body, parse_scope(&scope), "cli");
            eng.save_to_repo(&repo)?;
            eng.flush_outbox_to_repo_with_policy(&repo, 128, 3)?;
            maybe_sync_projection(sync_wiki, wiki_root.as_deref(), &eng)?;
            println!("ingested source={}", sid.0);
        }
        Cmd::FileClaim { text, scope, tier } => {
            let tier = parse_tier(&tier)?;
            let cid = eng.file_claim(text, parse_scope(&scope), tier, "cli");
            eng.save_to_repo(&repo)?;
            eng.flush_outbox_to_repo_with_policy(&repo, 128, 3)?;
            maybe_sync_projection(sync_wiki, wiki_root.as_deref(), &eng)?;
            println!("claim_id={}", cid.0);
        }
        Cmd::SupersedeClaim {
            old_claim_id,
            new_text,
            scope,
            tier,
        } => {
            let old = wiki_core::ClaimId(uuid::Uuid::parse_str(&old_claim_id)?);
            let tier = parse_tier(&tier)?;
            let new_id = eng.supersede(old, new_text, parse_scope(&scope), tier, "cli")?;
            eng.save_to_repo(&repo)?;
            eng.flush_outbox_to_repo_with_policy(&repo, 128, 3)?;
            maybe_sync_projection(sync_wiki, wiki_root.as_deref(), &eng)?;
            println!("new_claim_id={}", new_id.0);
        }
        Cmd::Query {
            query,
            rrf_k,
            per_stream_limit,
            write_page,
            page_title,
        } => {
            let ctx = QueryContext::new(&query)
                .with_rrf_k(rrf_k)
                .with_per_stream_limit(per_stream_limit);
            let ranked = eng.query_pipeline_memory(&ctx, OffsetDateTime::now_utc(), "cli");
            if write_page {
                let title = page_title.unwrap_or_else(|| format!("query-{}", timestamp_slug()));
                let page = query_to_page(&title, &query, &ranked);
                eng.store.pages.insert(page.id, page);
            }
            eng.save_to_repo(&repo)?;
            eng.flush_outbox_to_repo_with_policy(&repo, 128, 3)?;
            maybe_sync_projection(sync_wiki, wiki_root.as_deref(), &eng)?;
            for (id, score) in ranked.into_iter().take(20) {
                println!("{score:.6}\t{id}");
            }
        }
        Cmd::Lint => {
            let findings = eng.run_basic_lint("cli");
            eng.save_to_repo(&repo)?;
            eng.flush_outbox_to_repo_with_policy(&repo, 128, 3)?;
            if let Some(root) = wiki_root.as_deref() {
                let report = write_lint_report(root, &format!("lint-{}", timestamp_slug()), &findings)?;
                println!("lint_report={}", report.display());
            }
            maybe_sync_projection(sync_wiki, wiki_root.as_deref(), &eng)?;
            for f in findings {
                println!("{:?}\t{}\t{}", f.severity, f.code, f.message);
            }
        }
        Cmd::Promote { claim_id } => {
            let cid = wiki_core::ClaimId(uuid::Uuid::parse_str(&claim_id)?);
            eng.promote_if_qualified(cid, "cli")?;
            eng.save_to_repo(&repo)?;
            eng.flush_outbox_to_repo_with_policy(&repo, 128, 3)?;
            maybe_sync_projection(sync_wiki, wiki_root.as_deref(), &eng)?;
            println!("promoted {claim_id}");
        }
        Cmd::Crystallize {
            question,
            findings,
            files,
            lessons,
        } => {
            let draft = eng.crystallize(
                SessionCrystallizationInput {
                    question,
                    findings,
                    files_touched: files,
                    lessons,
                    scope: Scope::Private {
                        agent_id: "cli".into(),
                    },
                },
                "cli",
            )?;
            eng.save_to_repo(&repo)?;
            eng.flush_outbox_to_repo_with_policy(&repo, 128, 3)?;
            maybe_sync_projection(sync_wiki, wiki_root.as_deref(), &eng)?;
            println!("page={} claims={}", draft.page.id.0, draft.claim_candidates.len());
        }
        Cmd::ExportOutboxNdjson => {
            print!("{}", repo.export_outbox_ndjson()?);
        }
        Cmd::ExportOutboxNdjsonFrom { last_id } => {
            print!("{}", repo.export_outbox_ndjson_from_id(last_id)?);
        }
        Cmd::AckOutbox {
            up_to_id,
            consumer_tag,
        } => {
            let n = repo.mark_outbox_processed(up_to_id, &consumer_tag)?;
            println!("acked={n}");
        }
        Cmd::ConsumeToMempalace { last_id } => {
            let ndjson = repo.export_outbox_ndjson_from_id(last_id)?;
            let n = consume_outbox_ndjson(&CliMempalaceSink, &ndjson)?;
            println!("consumed={n}");
        }
        Cmd::LlmSmoke { config, prompt } => {
            let cfg = llm::load_llm_config(&config)?;
            let out = llm::smoke_chat_completion(&cfg, &prompt)?;
            println!("{out}");
        }
    }
    Ok(())
}

fn parse_scope(s: &str) -> Scope {
    if let Some(x) = s.strip_prefix("shared:") {
        Scope::Shared {
            team_id: x.to_string(),
        }
    } else if let Some(x) = s.strip_prefix("private:") {
        Scope::Private {
            agent_id: x.to_string(),
        }
    } else {
        Scope::Private {
            agent_id: s.to_string(),
        }
    }
}

fn parse_tier(s: &str) -> Result<MemoryTier, Box<dyn std::error::Error>> {
    let x = s.trim().to_ascii_lowercase();
    match x.as_str() {
        "working" => Ok(MemoryTier::Working),
        "episodic" => Ok(MemoryTier::Episodic),
        "semantic" => Ok(MemoryTier::Semantic),
        "procedural" => Ok(MemoryTier::Procedural),
        _ => Err(format!("unknown tier: {s}").into()),
    }
}

fn maybe_sync_projection(
    sync_wiki: bool,
    wiki_root: Option<&std::path::Path>,
    eng: &LlmWikiEngine<NoopWikiHook>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !sync_wiki {
        return Ok(());
    }
    if let Some(root) = wiki_root {
        let stats = write_projection(root, &eng.store, &eng.audits)?;
        println!(
            "projection pages={} claims={} sources={}",
            stats.pages_written, stats.claims_written, stats.sources_written
        );
    }
    Ok(())
}

fn query_to_page(title: &str, query: &str, ranked: &[(String, f64)]) -> WikiPage {
    let mut md = format!("# {title}\n\n## Query\n\n{query}\n\n## Top Results\n\n");
    for (doc, score) in ranked.iter().take(20) {
        md.push_str(&format!("- `{doc}` score={score:.6}\n"));
    }
    WikiPage::new(
        title,
        md,
        Scope::Private {
            agent_id: "cli".into(),
        },
    )
}

fn timestamp_slug() -> String {
    let now = OffsetDateTime::now_utc();
    now.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "now".to_string())
        .replace(':', "-")
}

struct CliMempalaceSink;

impl MempalaceWikiSink for CliMempalaceSink {
    fn on_claim_upserted(&self, _claim: &wiki_core::Claim) -> Result<(), MempalaceError> {
        Ok(())
    }

    fn on_claim_event(&self, claim_id: wiki_core::ClaimId) -> Result<(), MempalaceError> {
        println!("mempalace claim_upserted {}", claim_id.0);
        Ok(())
    }

    fn on_claim_superseded(
        &self,
        old: wiki_core::ClaimId,
        new: wiki_core::ClaimId,
    ) -> Result<(), MempalaceError> {
        println!("mempalace claim_superseded {} -> {}", old.0, new.0);
        Ok(())
    }

    fn on_source_linked(
        &self,
        source_id: wiki_core::SourceId,
        claim_id: wiki_core::ClaimId,
    ) -> Result<(), MempalaceError> {
        println!("mempalace source_linked {} -> {}", source_id.0, claim_id.0);
        Ok(())
    }

    fn scope_filter(&self, _scope: &Scope) -> bool {
        true
    }
}
