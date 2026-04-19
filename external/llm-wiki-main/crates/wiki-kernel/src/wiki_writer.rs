use crate::InMemoryStore;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use wiki_core::{AuditRecord, LintFinding, LintSeverity};

#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectionStats {
    pub pages_written: usize,
    pub claims_written: usize,
    pub sources_written: usize,
}

pub fn write_projection(
    wiki_root: &Path,
    store: &InMemoryStore,
    audits: &[AuditRecord],
) -> io::Result<ProjectionStats> {
    let pages_dir = wiki_root.join("pages");
    let concepts_dir = wiki_root.join("concepts");
    let sources_dir = wiki_root.join("sources");
    let analyses_dir = wiki_root.join("analyses");
    fs::create_dir_all(&pages_dir)?;
    fs::create_dir_all(&concepts_dir)?;
    fs::create_dir_all(&sources_dir)?;
    fs::create_dir_all(&analyses_dir)?;

    let mut stats = ProjectionStats::default();
    let mut page_rows = Vec::new();
    let mut claim_rows = Vec::new();
    let mut source_rows = Vec::new();

    let mut pages: Vec<_> = store.pages.values().collect();
    pages.sort_by(|a, b| a.title.cmp(&b.title).then_with(|| a.id.0.cmp(&b.id.0)));
    for page in pages {
        let slug = slugify(&page.title, &page.id.0.to_string());
        let path = pages_dir.join(format!("{slug}.md"));
        fs::write(&path, &page.markdown)?;
        stats.pages_written += 1;
        page_rows.push(format!(
            "- [{}](pages/{}.md) | updated: {}",
            page.title,
            slug,
            page.updated_at.date()
        ));
    }

    let mut claims: Vec<_> = store.claims.values().collect();
    claims.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    for claim in claims {
        let short = claim.id.0.to_string();
        let id_short = &short[..8];
        let file = format!("{id_short}.md");
        let path = concepts_dir.join(&file);
        let md = format!(
            "# Claim {id_short}\n\n- id: `{}`\n- tier: `{:?}`\n- confidence: `{:.3}`\n- quality: `{:.3}`\n- stale: `{}`\n- sources: `{}`\n\n## Text\n\n{}\n",
            claim.id.0,
            claim.tier,
            claim.confidence,
            claim.quality_score,
            claim.stale,
            claim.source_ids.len(),
            claim.text
        );
        fs::write(path, md)?;
        stats.claims_written += 1;
        claim_rows.push(format!(
            "- [claim:{}](concepts/{}) | tier={:?} stale={}",
            id_short, file, claim.tier, claim.stale
        ));
    }

    let mut sources: Vec<_> = store.sources.values().collect();
    sources.sort_by(|a, b| a.ingested_at.cmp(&b.ingested_at));
    for source in sources {
        let short = source.id.0.to_string();
        let id_short = &short[..8];
        let file = format!("{id_short}.md");
        let path = sources_dir.join(&file);
        let preview = preview_text(&source.body, 2000);
        let md = format!(
            "# Source {id_short}\n\n- id: `{}`\n- uri: `{}`\n- ingested_at: `{}`\n\n## Preview\n\n{}\n",
            source.id.0, source.uri, source.ingested_at, preview
        );
        fs::write(path, md)?;
        stats.sources_written += 1;
        source_rows.push(format!("- [source:{}](sources/{}) | {}", id_short, file, source.uri));
    }

    fs::write(
        wiki_root.join("index.md"),
        render_index(&page_rows, &claim_rows, &source_rows),
    )?;
    fs::write(wiki_root.join("log.md"), render_log(audits))?;
    Ok(stats)
}

pub fn write_lint_report(
    wiki_root: &Path,
    report_name: &str,
    findings: &[LintFinding],
) -> io::Result<std::path::PathBuf> {
    let reports_dir = wiki_root.join("reports");
    fs::create_dir_all(&reports_dir)?;
    let filename = if report_name.ends_with(".md") {
        report_name.to_string()
    } else {
        format!("{report_name}.md")
    };
    let out = reports_dir.join(filename);
    let mut grouped: BTreeMap<&'static str, Vec<&LintFinding>> = BTreeMap::new();
    for f in findings {
        let key = match f.severity {
            LintSeverity::Error => "error",
            LintSeverity::Warn => "warn",
            LintSeverity::Info => "info",
        };
        grouped.entry(key).or_default().push(f);
    }
    let mut md = String::from("# Lint Report\n\n");
    md.push_str(&format!("- total findings: `{}`\n\n", findings.len()));
    for (k, items) in grouped {
        md.push_str(&format!("## {}\n\n", k));
        for item in items {
            md.push_str(&format!(
                "- `{}` {}{}\n",
                item.code,
                item.message,
                item.subject
                    .as_ref()
                    .map(|s| format!(" (subject={s})"))
                    .unwrap_or_default()
            ));
        }
        md.push('\n');
    }
    fs::write(&out, md)?;
    Ok(out)
}

fn render_index(pages: &[String], claims: &[String], sources: &[String]) -> String {
    let mut md = String::from("# index\n\n");
    md.push_str("## pages\n\n");
    if pages.is_empty() {
        md.push_str("- (empty)\n");
    } else {
        for l in pages {
            md.push_str(l);
            md.push('\n');
        }
    }
    md.push_str("\n## concepts\n\n");
    if claims.is_empty() {
        md.push_str("- (empty)\n");
    } else {
        for l in claims {
            md.push_str(l);
            md.push('\n');
        }
    }
    md.push_str("\n## sources\n\n");
    if sources.is_empty() {
        md.push_str("- (empty)\n");
    } else {
        for l in sources {
            md.push_str(l);
            md.push('\n');
        }
    }
    md
}

fn render_log(audits: &[AuditRecord]) -> String {
    let mut lines = String::from("# log\n\n");
    let mut rows: Vec<_> = audits.iter().collect();
    rows.sort_by(|a, b| a.at.cmp(&b.at));
    for a in rows {
        lines.push_str(&format!(
            "## [{}] {:?} | {}\n- actor: `{}`\n- summary: {}\n\n",
            a.at, a.op, a.id, a.actor, a.summary
        ));
    }
    lines
}

fn slugify(title: &str, fallback: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii_whitespace() || c == '-' || c == '_' {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        fallback.chars().take(8).collect()
    } else {
        out
    }
}

fn preview_text(body: &str, max_len: usize) -> String {
    let mut s = body.to_string();
    if s.len() > max_len {
        s.truncate(max_len);
        s.push_str("\n\n...truncated...");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wiki_core::{AuditOperation, AuditRecord, Claim, MemoryTier, RawArtifact, Scope, WikiPage};

    #[test]
    fn projection_writes_index_log_and_dirs() {
        let dir = tempdir().unwrap();
        let wiki_root = dir.path();

        let mut store = InMemoryStore::default();
        let scope = Scope::Private {
            agent_id: "t".into(),
        };

        let src = RawArtifact::new("file:///a.md", "hello world", scope.clone());
        store.sources.insert(src.id, src);

        let claim = Claim::new("Redis caching for API", scope.clone(), MemoryTier::Semantic);
        store.claims.insert(claim.id, claim);

        let page = WikiPage::new("Alpha Page", "Link to [[Beta Page]]", scope.clone());
        store.pages.insert(page.id, page);

        let audits = vec![AuditRecord::new(AuditOperation::IngestSource, "t", "x")];
        let stats = write_projection(wiki_root, &store, &audits).unwrap();
        assert!(stats.pages_written >= 1);
        assert!(wiki_root.join("index.md").exists());
        assert!(wiki_root.join("log.md").exists());
        assert!(wiki_root.join("pages").is_dir());
        assert!(wiki_root.join("concepts").is_dir());
        assert!(wiki_root.join("sources").is_dir());
    }
}
