use super::AppState;
use crate::{
    db,
    models::{
        LogLevel, NewPageResult, QuerySearchDebug, QuerySearchRouteDebug,
        WikiPageCitationItem, WikiPageDetail, WikiPageFrontmatter, WikiPageHistoryDetail,
        WikiPageHistoryItem,
    },
    search::reciprocal_rank_fusion,
    vault,
};
use std::{
    collections::HashSet,
    fs,
    io,
    path::{Path, PathBuf},
};

// ─── 常量引用 ────────────────────────────────────────────────────────────────

use super::{
    QUERY_RRF_K, QUERY_ROUTE_DEBUG_TOP_CANDIDATES,
    QUERY_TOP_K_DEFAULT, QUERY_TOP_K_MIN, QUERY_TOP_K_MAX,
};

// ─── Free functions: misc helpers (private) ──────────────────────────────────

/// 跳过 Markdown frontmatter（--- ... ---），返回正文前 `limit` 个字符。
fn extract_content_after_frontmatter(text: &str, limit: usize) -> String {
    let s = text.trim_start();
    let body = if s.starts_with("---") {
        let rest = &s[3..];
        rest.find("\n---")
            .map(|pos| rest[pos + 4..].trim_start_matches('\n'))
            .unwrap_or(s)
    } else {
        s
    };
    body.chars().take(limit).collect()
}

pub(super) fn tokenize_query(question: &str) -> Vec<String> {
    // 轻量混合分词：保留英文 token，同时为连续中文片段生成 2-gram，提升中文命中率。
    let mut tokens = question
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    for segment in extract_cjk_segments(question) {
        let chars = segment.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            continue;
        }
        tokens.push(segment.clone());
        if chars.len() >= 2 {
            for window in chars.windows(2) {
                let gram = window.iter().collect::<String>();
                tokens.push(gram);
            }
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
        .into_iter()
        .filter(|token| !is_stopword(token))
        .collect()
}

fn extract_cjk_segments(input: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if is_cjk(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            segments.push(current.clone());
            current.clear();
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF // CJK Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
    )
}

fn is_stopword(token: &str) -> bool {
    const ZH_STOPWORDS: &[&str] = &[
        "的", "了", "是", "吗", "呢", "和", "与", "及", "在", "对", "把", "将",
    ];
    const EN_STOPWORDS: &[&str] = &["the", "is", "are", "a", "an", "of", "to", "for"];

    ZH_STOPWORDS.contains(&token) || EN_STOPWORDS.contains(&token)
}

pub(super) fn normalize_top_k(top_k: Option<usize>) -> usize {
    top_k
        .unwrap_or(QUERY_TOP_K_DEFAULT)
        .clamp(QUERY_TOP_K_MIN, QUERY_TOP_K_MAX)
}

pub(super) fn search_wiki_matches(
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<Vec<super::WikiMatch>, String> {
    let entries = match fs::read_dir(wiki_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("读取 wiki 目录失败: {}", err)),
    };

    let page_paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    search_wiki_matches_from_paths(&page_paths, tokens, question, limit)
}

pub(super) fn search_wiki_matches_with_fts(
    db_path: &Path,
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<
    (
        Vec<super::WikiMatch>,
        &'static str,
        Option<String>,
        Option<QuerySearchDebug>,
    ),
    String,
> {
    if tokens.is_empty() {
        return Ok((Vec::new(), "empty", None, None));
    }

    match db::search_fts_page_paths(db_path, tokens, 64) {
        Ok(paths) if !paths.is_empty() => {
            let matches = search_wiki_matches_from_paths(&paths, tokens, question, limit)?;
            if !matches.is_empty() {
                let contributed_paths = matches
                    .iter()
                    .map(|item| item.page_path.clone())
                    .collect::<Vec<_>>();
                let debug = QuerySearchDebug {
                    strategy: "fts".to_string(),
                    rrf_k: None,
                    fused_top_paths: contributed_paths.clone(),
                    routes: vec![QuerySearchRouteDebug {
                        route: "fts".to_string(),
                        candidate_count: paths.len(),
                        top_candidates: paths
                            .iter()
                            .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                            .cloned()
                            .collect(),
                        contributed_paths,
                    }],
                };
                return Ok((matches, "fts", None, Some(debug)));
            }
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            let contributed_paths = fallback
                .iter()
                .map(|item| item.page_path.clone())
                .collect::<Vec<_>>();
            let debug = QuerySearchDebug {
                strategy: "scan".to_string(),
                rrf_k: None,
                fused_top_paths: contributed_paths.clone(),
                routes: vec![QuerySearchRouteDebug {
                    route: "scan".to_string(),
                    candidate_count: contributed_paths.len(),
                    top_candidates: contributed_paths
                        .iter()
                        .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                        .cloned()
                        .collect(),
                    contributed_paths,
                }],
            };
            Ok((fallback, "scan", None, Some(debug)))
        }
        Ok(_) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            let contributed_paths = fallback
                .iter()
                .map(|item| item.page_path.clone())
                .collect::<Vec<_>>();
            let debug = QuerySearchDebug {
                strategy: "scan".to_string(),
                rrf_k: None,
                fused_top_paths: contributed_paths.clone(),
                routes: vec![QuerySearchRouteDebug {
                    route: "scan".to_string(),
                    candidate_count: contributed_paths.len(),
                    top_candidates: contributed_paths
                        .iter()
                        .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                        .cloned()
                        .collect(),
                    contributed_paths,
                }],
            };
            Ok((fallback, "scan", None, Some(debug)))
        }
        Err(err) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            let contributed_paths = fallback
                .iter()
                .map(|item| item.page_path.clone())
                .collect::<Vec<_>>();
            let debug = QuerySearchDebug {
                strategy: "scan".to_string(),
                rrf_k: None,
                fused_top_paths: contributed_paths.clone(),
                routes: vec![QuerySearchRouteDebug {
                    route: "scan".to_string(),
                    candidate_count: contributed_paths.len(),
                    top_candidates: contributed_paths
                        .iter()
                        .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                        .cloned()
                        .collect(),
                    contributed_paths,
                }],
            };
            Ok((fallback, "scan", Some(err), Some(debug)))
        }
    }
}

/// 多路 RRF 融合检索：FTS5 + 链接扩展 + Citation 热度 + 可选扩展路径（如 embedding）。
///
/// 若所有路径均为空（如空 vault），自动降级为 `search_wiki_matches_with_fts`。
pub(super) fn search_wiki_matches_rrf_with_extra_routes(
    db_path: &Path,
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
    extra_routes: &[(String, Vec<String>)],
) -> Result<
    (
        Vec<super::WikiMatch>,
        &'static str,
        Option<String>,
        Option<QuerySearchDebug>,
    ),
    String,
> {
    if tokens.is_empty() {
        return Ok((Vec::new(), "empty", None, None));
    }

    // 路径1：FTS5（多取 4x 供融合使用）
    let (fts_paths, fts_error) = match db::search_fts_page_paths(db_path, tokens, limit * 4) {
        Ok(paths) => (paths, None),
        Err(e) => (Vec::new(), Some(e)),
    };

    // 路径2：链接扩展（基于 FTS 结果做一跳扩展）
    let link_paths = if !fts_paths.is_empty() {
        db::query_linked_page_paths(db_path, &fts_paths, limit * 4).unwrap_or_default()
    } else {
        Vec::new()
    };

    // 路径3：Citation 热度
    let popular_paths = db::query_citation_popular_paths(db_path, limit * 4).unwrap_or_default();

    let mut named_routes = vec![
        ("fts".to_string(), fts_paths.clone()),
        ("linked".to_string(), link_paths),
        ("popular".to_string(), popular_paths),
    ];
    for (route_name, route_paths) in extra_routes {
        if !route_paths.is_empty() {
            named_routes.push((route_name.clone(), route_paths.clone()));
        }
    }

    let routes = named_routes
        .iter()
        .map(|(_, paths)| paths.clone())
        .collect::<Vec<_>>();

    // 如果所有路径全空，降级到原有单路逻辑
    if routes.iter().all(|route| route.is_empty()) {
        return search_wiki_matches_with_fts(db_path, wiki_dir, tokens, question, limit);
    }

    // RRF 融合
    let fused = reciprocal_rank_fusion(&routes, QUERY_RRF_K);

    // 取 top-(limit*2) 的路径，再用 search_wiki_matches_from_paths 提取摘要和评分
    let top_paths: Vec<String> = fused
        .into_iter()
        .take(limit * 2)
        .map(|(path, _)| path)
        .collect();

    if top_paths.is_empty() {
        return search_wiki_matches_with_fts(db_path, wiki_dir, tokens, question, limit);
    }

    let matches = search_wiki_matches_from_paths(&top_paths, tokens, question, limit)?;

    // 若 RRF 结果仍为空（页面文件不存在等），降级
    if matches.is_empty() {
        return search_wiki_matches_with_fts(db_path, wiki_dir, tokens, question, limit);
    }

    let matched_set = matches
        .iter()
        .map(|item| item.page_path.clone())
        .collect::<HashSet<_>>();
    let route_debug = named_routes
        .into_iter()
        .map(|(route, route_paths)| {
            let mut contributed_paths = route_paths
                .iter()
                .filter(|path| matched_set.contains(*path))
                .cloned()
                .collect::<Vec<_>>();
            contributed_paths.sort();
            contributed_paths.dedup();
            QuerySearchRouteDebug {
                route,
                candidate_count: route_paths.len(),
                top_candidates: route_paths
                    .iter()
                    .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
                    .cloned()
                    .collect(),
                contributed_paths,
            }
        })
        .collect::<Vec<_>>();

    let search_debug = QuerySearchDebug {
        strategy: "rrf".to_string(),
        rrf_k: Some(QUERY_RRF_K),
        fused_top_paths: top_paths
            .iter()
            .take(QUERY_ROUTE_DEBUG_TOP_CANDIDATES)
            .cloned()
            .collect(),
        routes: route_debug,
    };

    Ok((matches, "rrf", fts_error, Some(search_debug)))
}

/// 三路 RRF 融合检索：FTS5 + 链接扩展 + Citation 热度。
#[allow(dead_code)]
pub(super) fn search_wiki_matches_rrf(
    db_path: &Path,
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<
    (
        Vec<super::WikiMatch>,
        &'static str,
        Option<String>,
        Option<QuerySearchDebug>,
    ),
    String,
> {
    search_wiki_matches_rrf_with_extra_routes(db_path, wiki_dir, tokens, question, limit, &[])
}

pub(super) fn search_wiki_matches_from_paths(
    page_paths: &[String],
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<Vec<super::WikiMatch>, String> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let phrase = question.trim().to_lowercase();
    let mut results = Vec::new();
    let mut seen_paths = HashSet::new();

    for page_path in page_paths {
        let path = PathBuf::from(page_path);
        let canonical = path.to_string_lossy().to_string();
        if !seen_paths.insert(canonical) {
            continue;
        }
        if !path.exists() {
            continue;
        }
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(err) => return Err(format!("读取页面失败 {}: {}", path.to_string_lossy(), err)),
        };
        let lowered = content.to_lowercase();
        let title = extract_title_from_markdown(&content, &path);
        let lowered_title = title.to_lowercase();

        // 综合评分：正文命中 + 标题命中加权 + 短语命中加权。
        let token_hits = tokens
            .iter()
            .map(|token| lowered.matches(token).count())
            .sum::<usize>();
        let title_hits = tokens
            .iter()
            .filter(|token| lowered_title.contains(token.as_str()))
            .count();
        let phrase_hit = usize::from(!phrase.is_empty() && lowered.contains(&phrase));
        let score = token_hits + title_hits * 3 + phrase_hit * 5;
        if score == 0 {
            continue;
        }

        let excerpt = pick_excerpt(&content, tokens);
        results.push(super::WikiMatch {
            page_path: path.to_string_lossy().to_string(),
            score,
            excerpt,
        });
    }

    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.page_path.cmp(&right.page_path))
    });
    if results.len() > limit {
        results.truncate(limit);
    }
    Ok(results)
}

/// 在 Markdown 文件内容中设置或移除 frontmatter 的 `stale` 字段。
/// 如果 stale=true，确保 frontmatter 中有 `stale: true`；
/// 如果 stale=false，移除 `stale:` 行（不写 `stale: false` 以保持简洁）。
pub(super) fn set_frontmatter_stale_field(content: &str, stale: bool) -> String {
    // 定位 frontmatter 块：内容以 "---\n" 开头，找到第二个 "---"
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        // 无 frontmatter：直接返回原内容（不修改）
        return content.to_string();
    }

    let after_first = if content.starts_with("---\r\n") {
        &content[5..]
    } else {
        &content[4..]
    };

    // 找到 frontmatter 结束 "---"
    let end_pos = after_first
        .find("\n---")
        .or_else(|| after_first.find("\r\n---"));

    let Some(rel_end) = end_pos else {
        return content.to_string(); // 格式不对，不改
    };

    let fm_start = if content.starts_with("---\r\n") { 5 } else { 4 };
    let fm_content = &content[fm_start..fm_start + rel_end];
    let after_fm = &content[fm_start + rel_end..]; // 包含 "\n---" 或 "\r\n---" 及其后内容

    // 移除已有 stale: 行
    let cleaned: String = fm_content
        .lines()
        .filter(|line| !line.trim_start().starts_with("stale:"))
        .collect::<Vec<_>>()
        .join("\n");

    // 若 stale=true，追加 stale: true 行
    let new_fm = if stale {
        if cleaned.is_empty() {
            "stale: true".to_string()
        } else {
            format!("{}\nstale: true", cleaned)
        }
    } else {
        cleaned
    };

    format!("---\n{}\n{}", new_fm, after_fm)
}

pub(super) fn parse_wiki_frontmatter(content: &str) -> Option<WikiPageFrontmatter> {
    let block = extract_frontmatter_block(content)?;
    let mut frontmatter = WikiPageFrontmatter {
        title: None,
        source: None,
        raw: None,
        imported_at: None,
        entities: Vec::new(),
        stale: None,
    };
    let mut lines = block.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("title:") {
            frontmatter.title = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("source:") {
            frontmatter.source = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("raw:") {
            frontmatter.raw = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("imported_at:") {
            frontmatter.imported_at = parse_frontmatter_scalar(value);
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("stale:") {
            let v = value.trim().to_ascii_lowercase();
            frontmatter.stale = match v.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            };
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("entities:") {
            let inline = value.trim();
            if inline == "[]" {
                continue;
            }
            if !inline.is_empty() {
                if let Some(entity) = parse_frontmatter_scalar(inline) {
                    if !entity.is_empty() {
                        frontmatter.entities.push(entity);
                    }
                }
                continue;
            }

            while let Some(next_line) = lines.peek().copied() {
                let next_trimmed = next_line.trim();
                if next_trimmed.is_empty() {
                    lines.next();
                    continue;
                }

                if let Some(entity) = next_trimmed.strip_prefix("- ") {
                    if let Some(entity_value) = parse_frontmatter_scalar(entity) {
                        if !entity_value.is_empty() {
                            frontmatter.entities.push(entity_value);
                        }
                    }
                    lines.next();
                    continue;
                }

                break;
            }
        }
    }

    let has_scalar_fields = frontmatter.title.is_some()
        || frontmatter.source.is_some()
        || frontmatter.raw.is_some()
        || frontmatter.imported_at.is_some()
        || frontmatter.stale.is_some();
    if has_scalar_fields || !frontmatter.entities.is_empty() {
        Some(frontmatter)
    } else {
        None
    }
}

/// 读取 .md 文件的 frontmatter entities 作为标签，失败时返回空。
pub(super) fn read_page_tags(path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_wiki_frontmatter(&content)
        .map(|fm| fm.entities)
        .unwrap_or_default()
}

fn extract_frontmatter_block(content: &str) -> Option<String> {
    let normalized = content.replace("\r\n", "\n");
    let mut lines = normalized.lines();

    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut block_lines = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            return Some(block_lines.join("\n"));
        }
        block_lines.push(line.to_string());
    }

    None
}

fn parse_frontmatter_scalar(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        let body = &trimmed[1..trimmed.len() - 1];
        return Some(unescape_yaml_double_quoted(body));
    }

    if trimmed.len() >= 2 && trimmed.starts_with('\'') && trimmed.ends_with('\'') {
        let body = &trimmed[1..trimmed.len() - 1];
        return Some(body.to_string());
    }

    Some(trimmed.to_string())
}

fn unescape_yaml_double_quoted(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                output.push(next);
            } else {
                output.push('\\');
            }
        } else {
            output.push(ch);
        }
    }
    output
}

pub(super) fn extract_title_from_markdown(content: &str, path: &Path) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }

    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

pub(super) fn friendly_display_path(path: &Path) -> String {
    let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let normalized = normalized.to_string_lossy();
    friendly_display_path_str(normalized.as_ref())
}

pub(super) fn friendly_display_path_str(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", stripped);
    }

    if let Some(stripped) = path.strip_prefix(r"\\?\") {
        return stripped.to_string();
    }

    path.to_string()
}

fn pick_excerpt(content: &str, tokens: &[String]) -> String {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lowered = line.to_lowercase();
        if tokens.iter().any(|token| lowered.contains(token)) {
            return trim_excerpt(line, 120);
        }
    }

    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| trim_excerpt(line, 120))
        .unwrap_or_else(|| "(页面无可用内容)".to_string())
}

fn trim_excerpt(input: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for ch in input.chars().take(max_chars) {
        output.push(ch);
    }
    if input.chars().count() > max_chars {
        output.push('…');
    }
    output
}

// ─── Free function helpers: title / slug / graph label ───────────────────────

pub(super) fn extract_markdown_h1_title(content: &str) -> Option<String> {
    content
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_string())
        .filter(|title| !title.is_empty())
}

/// 在 `wiki/` 目录中寻找可用 slug，必要时自动追加序号（`-2` 到 `-99`）。
pub(super) fn resolve_unique_wiki_slug(wiki_dir: &Path, base_slug: &str) -> Result<String, String> {
    let candidate = wiki_dir.join(format!("{}.md", base_slug));
    if !candidate.exists() {
        return Ok(base_slug.to_string());
    }
    for n in 2..=99 {
        let slug = format!("{}-{}", base_slug, n);
        let path = wiki_dir.join(format!("{}.md", slug));
        if !path.exists() {
            return Ok(slug);
        }
    }
    Err(format!("无法为 slug '{}' 找到空闲文件名", base_slug))
}

fn wiki_title_from_content(content: &str, fallback_path: &str) -> String {
    content
        .lines()
        .find(|line| line.starts_with("# "))
        .map(|line| line.trim_start_matches("# ").trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            std::path::Path::new(fallback_path)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(fallback_path)
                .to_string()
        })
}

/// 判断标题是否为原始摄入 ID（格式：`ingest-{纯十进制数字}`）。
pub(super) fn is_raw_ingest_id(title: &str) -> bool {
    match title.strip_prefix("ingest-") {
        Some(rest) => !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// 解析图谱节点的显示标签。
///
/// 当 DB 标题是原始摄入 ID 时（`ingest-{timestamp}`），从 frontmatter 提取更有意义的名称：
/// entities[0]（≥2 字符）> source 文件名 stem（非内部路径）> DB 标题原值。
pub(super) fn resolve_graph_node_label(
    db_title: &str,
    fm: &crate::models::WikiPageFrontmatter,
) -> String {
    if !is_raw_ingest_id(db_title) {
        return db_title.to_string();
    }
    if let Some(entity) = fm.entities.first() {
        let e = entity.trim();
        if e.len() >= 2 {
            return e.to_string();
        }
    }
    if let Some(src) = fm.source.as_deref() {
        if let Some(stem) = std::path::Path::new(src)
            .file_stem()
            .and_then(|s| s.to_str())
        {
            let stem = stem.trim();
            if !stem.is_empty() && !stem.starts_with("llm_wiki_") && stem.len() >= 2 {
                return stem.to_string();
            }
        }
    }
    db_title.to_string()
}


// ─── AppState method implementations ─────────────────────────────────────────

// Private helpers (only used within this module)

pub(super) async fn extract_entities(state: &AppState, content: &str) -> Vec<String> {
    let provider = match state.get_llm_provider() {
        Some(p) => p,
        None => return Vec::new(),
    };

    // 截断内容，避免超出 token 上限
    let truncated: String = content.chars().take(2000).collect();
    let prompt = format!(
        "请从以下文档中提取关键实体（技术名、概念名、产品名、人名等），\
每行输出一个实体名称，不要编号，不要解释，最多提取10个最重要的实体：\n\n{}",
        truncated
    );

    match provider.complete(&prompt).await {
        Ok(response) => {
            let entities: Vec<String> = response
                .lines()
                .map(|line| line.trim().trim_start_matches('-').trim().to_string())
                .filter(|e| !e.is_empty() && e.len() <= 60)
                .take(10)
                .collect();

            if !entities.is_empty() {
                state.push_log(
                    LogLevel::Info,
                    format!("实体提取完成，共 {} 个实体", entities.len()),
                );
            }
            entities
        }
        Err(err) => {
            state.push_log(LogLevel::Warn, format!("实体提取失败，跳过: {}", err));
            Vec::new()
        }
    }
}

/// Ingest 后扫描相关 Wiki 页面并注入双向 See Also 链接。
///
/// 流程：
/// 1. 用实体名在 FTS 中搜索相关页面（最多 5 页）。
/// 2. 向每个相关页面追加指向新页的 See Also 链接。
/// 3. 向新页追加指向相关页面的 See Also 链接。
/// 4. 更新受影响页面的 FTS 索引。
///
/// 任何单步失败都记录告警但不中断整体流程。
pub(super) async fn update_related_pages_with_link(
    state: &AppState,
    db_path: &Path,
    vault_path: &Path,
    new_wiki_abs_path: &str,
    new_wiki_title: &str,
    entities: &[String],
) -> Vec<String> {
    if entities.is_empty() {
        return Vec::new();
    }

    // 将实体名称分词，合并去重后送入 FTS
    let mut token_set = std::collections::HashSet::new();
    for entity in entities {
        for token in tokenize_query(entity) {
            token_set.insert(token);
        }
    }
    let tokens: Vec<String> = token_set.into_iter().collect();

    if tokens.is_empty() {
        return Vec::new();
    }

    // FTS 搜索相关页面（最多取 5 个，排除自身）
    let related_paths: Vec<String> = match db::search_fts_page_paths(db_path, &tokens, 10) {
        Ok(paths) => paths
            .into_iter()
            .filter(|p| p != new_wiki_abs_path)
            .take(5)
            .collect(),
        Err(err) => {
            state.push_log(LogLevel::Warn, format!("相关页面 FTS 搜索失败: {}", err));
            return Vec::new();
        }
    };

    if related_paths.is_empty() {
        return Vec::new();
    }

    // 新页面相对于 vault 根的路径（用于写入其他页面的链接）
    let new_wiki_relative = PathBuf::from(new_wiki_abs_path)
        .strip_prefix(vault_path)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| new_wiki_abs_path.to_string());

    let mut updated = Vec::new();

    for related_abs in &related_paths {
        let related_path = PathBuf::from(related_abs);
        if !related_path.exists() {
            continue;
        }

        let related_relative = related_path
            .strip_prefix(vault_path)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| related_abs.clone());

        let related_title = related_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        // 1. 向相关页面追加指向新页的反向链接
        match vault::append_see_also_link(&related_path, &new_wiki_relative, new_wiki_title) {
            Ok(true) => {
                updated.push(related_abs.clone());
                // 更新该相关页面的 FTS 索引
                if let Ok(content) = fs::read_to_string(&related_path) {
                    let _ = db::upsert_fts_page(
                        db_path,
                        Path::new(related_abs),
                        &related_title,
                        &content,
                    );
                }
            }
            Ok(false) => {} // 链接已存在，跳过
            Err(err) => {
                state.push_log(
                    LogLevel::Warn,
                    format!("注入反向链接失败 {}: {}", related_abs, err),
                );
            }
        }

        // 2. 向新页追加指向相关页面的正向链接（失败不计入 updated）
        let new_path = PathBuf::from(new_wiki_abs_path);
        if let Err(err) =
            vault::append_see_also_link(&new_path, &related_relative, &related_title)
        {
            state.push_log(
                LogLevel::Warn,
                format!("注入正向链接失败 {}: {}", related_abs, err),
            );
        }
    }

    // 更新新页的 FTS 索引（包含追加的 See Also 内容）
    if !updated.is_empty() {
        if let Ok(content) = fs::read_to_string(new_wiki_abs_path) {
            let _ = db::upsert_fts_page(
                db_path,
                Path::new(new_wiki_abs_path),
                new_wiki_title,
                &content,
            );
        }
        state.push_log(
            LogLevel::Info,
            format!("双向链接注入完成，更新了 {} 个相关页面", updated.len()),
        );
    }

    updated
}

/// 复用主动新建页面的 prompt 逻辑，生成带 frontmatter 的 Markdown 草稿。
pub(super) async fn generate_ai_wiki_markdown_draft_impl(
    state: &AppState,
    db_path: &Path,
    topic: &str,
    memories_context: Option<&str>,
    skill_prompt: Option<&str>,
    research_mode: bool,
    ask_context: Option<&str>,
) -> Result<(String, String, String), String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let search_limit = if research_mode { 8 } else { 5 };
    let related_pages = if db_path.exists() {
        db::search_wiki_pages(db_path, topic, search_limit).unwrap_or_default()
    } else {
        Vec::new()
    };

    let related_context = if related_pages.is_empty() {
        "（暂无相关页面）".to_string()
    } else {
        related_pages
            .iter()
            .map(|p| {
                let snippet = if research_mode {
                    std::fs::read_to_string(&p.path)
                        .ok()
                        .map(|t| extract_content_after_frontmatter(&t, 400))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| {
                            if p.summary.is_empty() {
                                "（无摘要）".to_string()
                            } else {
                                p.summary.chars().take(400).collect()
                            }
                        })
                } else {
                    if p.summary.is_empty() {
                        "（无摘要）".to_string()
                    } else {
                        p.summary.chars().take(120).collect()
                    }
                };
                format!("- {}: {}", p.title, snippet)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let memories_section = if let Some(ctx) = memories_context {
        if ctx.is_empty() {
            String::new()
        } else {
            format!(
                "\n已记录的上下文记忆（请结合这些信息生成内容）：\n{}\n",
                ctx
            )
        }
    } else {
        String::new()
    };

    let skill_section = if let Some(skill) = skill_prompt {
        let normalized = skill.trim();
        if normalized.is_empty() {
            String::new()
        } else {
            // H5-D: 替换模板变量 {{topic}} {{memories}}
            let memories_text = memories_context.unwrap_or("").trim().to_string();
            let expanded = normalized
                .replace("{{topic}}", topic)
                .replace("{{memories}}", &memories_text);
            format!("\n当前启用技能模板（请优先遵循）：\n{}\n", expanded)
        }
    } else {
        String::new()
    };

    let ask_section = if let Some(ctx) = ask_context {
        let trimmed = ctx.trim();
        if trimmed.is_empty() {
            String::new()
        } else {
            format!(
                "\n知识库已有答案（Ask 检索结果，请参考但不要照抄）：\n{}\n",
                trimmed
            )
        }
    } else {
        String::new()
    };

    let prompt = format!(
        "你是一个专业的知识管理助手，负责为个人 Wiki 创建结构化页面。\n\
        \n\
        请为以下主题创建一个结构化的 Wiki 页面初稿。\n\
        \n\
        要创建的主题：{topic}\n\
        {memories_section}\
        {skill_section}\
        {ask_section}\
        知识库中已有的相关页面：\n\
        {related_context}\n\
        \n\
        输出要求（不要有任何前缀，只输出 Markdown 内容）：\n\
        \n\
        # {{页面标题}}\n\
        \n\
        {{2-4 段正文，每段 3-5 句，与相关页面形成知识链接，使用 [[相关页面]] 格式引用}}\n\
        \n\
        ## 参考\n\
        {{如有相关已有页面，用 [[页面标题]] 格式列出}}",
        topic = topic,
        memories_section = memories_section,
        skill_section = skill_section,
        ask_section = ask_section,
        related_context = related_context,
    );

    let provider = state.get_llm_provider();
    let llm_content = if let Some(p) = provider {
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(60), p.complete(&prompt))
                .await
                .map_err(|_| "LLM 调用超时（60s）".to_string())?
                .map_err(|e| format!("LLM 调用失败: {}", e))?;
        result.trim().to_string()
    } else {
        return Err("LLM 未配置，无法生成页面内容".to_string());
    };

    if llm_content.is_empty() {
        return Err("LLM 返回了空内容，请检查模型配置".to_string());
    }

    let page_title = extract_markdown_h1_title(&llm_content)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| topic.to_string());

    // frontmatter 与已有 create_wiki_page_with_ai 保持一致，便于后续审批直写。
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    let frontmatter = format!(
        "---\ntitle: '{}'\nsource: 'ai_generated'\ncreated_at: '{}'\nentities: []\n---\n",
        page_title.replace('\'', "''"),
        now_ms,
    );
    let full_content = format!("{}{}", frontmatter, llm_content);
    Ok((page_title, llm_content, full_content))
}

// Public methods delegated from AppState (called via state.method() in state.rs)

pub fn recent_wiki_pages(
    state: &AppState,
    limit: usize,
) -> Result<Vec<crate::models::WikiPageItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    db::ensure_meta_db(&db_path)?;
    let pages = db::list_recent_wiki_pages(&db_path, limit)?;

    Ok(pages
        .into_iter()
        .map(|page| {
            let display_path = friendly_display_path(Path::new(&page.path));
            let tags = read_page_tags(Path::new(&page.path));
            crate::models::WikiPageItem {
                title: page.title,
                path: page.path,
                display_path: Some(display_path),
                summary: page.summary,
                updated_at: page.updated_at,
                score: 0.0,
                tags,
            }
        })
        .collect())
}

pub fn search_wiki_pages(
    state: &AppState,
    keyword: String,
    limit: usize,
) -> Result<Vec<crate::models::WikiPageItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    db::ensure_meta_db(&db_path)?;
    let pages = db::search_wiki_pages(&db_path, &keyword, limit)?;

    Ok(pages
        .into_iter()
        .map(|page| {
            let display_path = friendly_display_path(Path::new(&page.path));
            let tags = read_page_tags(Path::new(&page.path));
            crate::models::WikiPageItem {
                title: page.title,
                path: page.path,
                display_path: Some(display_path),
                summary: page.summary,
                updated_at: page.updated_at,
                score: page.score,
                tags,
            }
        })
        .collect())
}

/// Wiki 混合搜索：FTS5 关键词 + 向量语义双路召回，RRF 融合排序。
/// Ollama 不可用时自动降级为纯 FTS5。
pub async fn search_wiki_pages_hybrid(
    state: &AppState,
    keyword: String,
    limit: usize,
) -> Result<Vec<crate::models::WikiPageItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    db::ensure_meta_db(&db_path)?;

    // FTS5 召回（keyword 为空时返回最近页面）
    let fts_pages = db::search_wiki_pages(&db_path, &keyword, limit * 3)?;
    let fts_paths: Vec<String> = fts_pages.iter().map(|p| p.path.clone()).collect();

    // 向量召回（best-effort，失败时静默降级）
    let embed_paths: Vec<String> = if !keyword.trim().is_empty() {
        match state.get_embed_provider().embed(&keyword).await {
            Ok(query_vec) => {
                let candidates = db::list_embeddings(&db_path, 5000).unwrap_or_default();
                crate::search::rank_embedding_paths_by_cosine(&query_vec, &candidates, limit * 3)
            }
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // RRF 融合（单路时退化为原路排名）
    let merged_paths: Vec<String> = if embed_paths.is_empty() {
        fts_paths.into_iter().take(limit).collect()
    } else {
        crate::search::reciprocal_rank_fusion(&[fts_paths, embed_paths], 60.0)
            .into_iter()
            .take(limit)
            .map(|(p, _)| p)
            .collect()
    };

    // 解析为 WikiPageItem（从 FTS 结果中查找 title/summary，找不到则用路径名）
    let fts_map: std::collections::HashMap<String, crate::db::WikiPageRecord> =
        db::search_wiki_pages(&db_path, "", limit * 3)
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.path.clone(), r))
            .collect();

    let items = merged_paths
        .into_iter()
        .map(|path| {
            let display_path = friendly_display_path(Path::new(&path));
            let tags = read_page_tags(Path::new(&path));
            if let Some(record) = fts_map.get(&path) {
                crate::models::WikiPageItem {
                    title: record.title.clone(),
                    path: path.clone(),
                    display_path: Some(display_path),
                    summary: record.summary.clone(),
                    updated_at: record.updated_at.clone(),
                    score: record.score,
                    tags,
                }
            } else {
                let title = Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&path)
                    .to_string();
                crate::models::WikiPageItem {
                    title,
                    path: path.clone(),
                    display_path: Some(display_path),
                    summary: String::new(),
                    updated_at: String::new(),
                    score: 0.0,
                    tags,
                }
            }
        })
        .collect();

    Ok(items)
}

/// 获取所有 wiki 页面路径并根据查询进行模糊匹配（忽略大小写）。
pub fn search_wiki_paths(state: &AppState, query: String) -> Result<Vec<String>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    db::ensure_meta_db(&db_path)?;
    let pages = db::list_all_wiki_pages(&db_path)?;

    let query_lower = query.to_lowercase();
    Ok(pages
        .into_iter()
        .filter(|p| p.path.to_lowercase().contains(&query_lower))
        .map(|p| p.path)
        .collect())
}

pub fn wiki_page_detail(state: &AppState, page_path: String) -> Result<WikiPageDetail, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let target_path = super::resolve_existing_wiki_page_path(&vault_path, &page_path)?;
    if target_path.extension().and_then(|v| v.to_str()) != Some("md") {
        return Err("仅支持读取 Markdown 页面".to_string());
    }

    let content =
        fs::read_to_string(&target_path).map_err(|err| format!("读取页面失败: {}", err))?;
    let title = extract_title_from_markdown(&content, &target_path);
    let frontmatter = parse_wiki_frontmatter(&content);
    let updated_at = file_modified_timestamp_ms(&target_path);

    Ok(WikiPageDetail {
        title,
        path: target_path.to_string_lossy().to_string(),
        display_path: friendly_display_path(&target_path),
        frontmatter,
        content,
        updated_at,
    })
}

/// 设置或取消 Wiki 页面的 stale 标记（直接修改 frontmatter）。
pub fn set_page_stale(state: &AppState, page_path: String, stale: bool) -> Result<(), String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let vault_path = vault_path.ok_or_else(|| "请先初始化 Vault".to_string())?;

    // 规范化并安全检查路径
    let abs_path = if std::path::Path::new(&page_path).is_absolute() {
        std::path::PathBuf::from(&page_path)
    } else {
        vault_path.join("wiki").join(&page_path)
    };
    let abs_path = abs_path
        .canonicalize()
        .map_err(|e| format!("页面路径无效: {}", e))?;
    let wiki_root = vault_path
        .join("wiki")
        .canonicalize()
        .map_err(|e| format!("wiki 目录无效: {}", e))?;
    if !abs_path.starts_with(&wiki_root) {
        return Err("禁止操作 wiki 目录之外的文件".to_string());
    }

    let content = fs::read_to_string(&abs_path).map_err(|e| format!("读取页面失败: {}", e))?;

    let updated = set_frontmatter_stale_field(&content, stale);

    fs::write(&abs_path, &updated).map_err(|e| format!("写入页面失败: {}", e))?;

    state.push_log(
        LogLevel::Info,
        format!(
            "页面 stale 标记已{}: {}",
            if stale { "设置" } else { "取消" },
            abs_path.to_string_lossy()
        ),
    );
    Ok(())
}

pub fn wiki_page_citations(
    state: &AppState,
    page_path: String,
) -> Result<Vec<WikiPageCitationItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let target_path = super::resolve_existing_wiki_page_path(&vault_path, &page_path)?;
    let target_path_string = target_path.to_string_lossy().to_string();
    let db_path = vault_path.join(".app").join("meta.db");
    db::ensure_meta_db(&db_path)?;
    let citations = db::list_citations_for_page(&db_path, &target_path_string)?;

    Ok(citations
        .into_iter()
        .map(|citation| {
            let cited_page_display_path =
                friendly_display_path(Path::new(&citation.cited_page_path));
            WikiPageCitationItem {
                cited_page_path: citation.cited_page_path.clone(),
                cited_page_display_path: Some(cited_page_display_path),
                score: citation.score,
                excerpt: citation.excerpt,
                target_exists: is_existing_wiki_page_target(
                    &vault_path,
                    &citation.cited_page_path,
                ),
            }
        })
        .collect())
}

pub async fn save_wiki_page_impl(
    state: &AppState,
    path: &str,
    content: &str,
    expected_checksum: Option<&str>,
) -> Result<crate::models::SaveWikiPageResult, String> {
    let path_buf = std::path::Path::new(path);
    match path_buf.extension().and_then(|ext| ext.to_str()) {
        Some("md") => {}
        _ => return Err(format!("路径必须是 .md 文件：{path}")),
    }
    if let Some(parent) = path_buf.parent() {
        if !parent.exists() {
            return Err(format!("父目录不存在：{}", parent.display()));
        }
    }

    // 校验 checksum（写入前防止静默覆盖外部编辑）
    let previous_content = if let Some(expected) = expected_checksum {
        if path_buf.exists() {
            let current_raw = std::fs::read_to_string(path_buf)
                .map_err(|err| format!("读取现有文件失败：{}", err))?;
            let current_hash = format!("{:x}", md5_simple(&current_raw));
            if current_hash != expected {
                return Err("checksum_mismatch: 文件已被外部修改，请刷新后再编辑。".to_string());
            }
            Some(current_raw)
        } else {
            // 文件尚不存在，跳过 checksum 校验
            None
        }
    } else {
        if path_buf.exists() {
            Some(
                std::fs::read_to_string(path_buf)
                    .map_err(|err| format!("读取现有文件失败：{}", err))?,
            )
        } else {
            None
        }
    };

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let db_path = vault_path
        .as_ref()
        .map(|vault_path| vault_path.join(".app").join("meta.db"));

    if let (Some(prev_content), Some(db_path)) = (previous_content.as_deref(), db_path.as_ref())
    {
        if prev_content != content && db_path.exists() {
            let previous_hash = format!("{:x}", md5_simple(prev_content));
            let previous_title = wiki_title_from_content(prev_content, path);
            db::insert_wiki_page_history(
                db_path,
                path_buf,
                &previous_title,
                &previous_hash,
                prev_content,
                &super::current_timestamp_ms(),
            )?;
        }
    }

    // 1. 写入文件
    let changed = crate::vault::write_wiki_page(path, content)?;

    if !changed {
        return Ok(crate::models::SaveWikiPageResult {
            path: path.to_string(),
            message: "内容未变化，跳过写入".to_string(),
        });
    }

    // 2. 更新 SQLite FTS 索引 + wiki_pages.title（复用已有逻辑）
    if let Some(db_path) = db_path {
        if db_path.exists() {
            let title = wiki_title_from_content(content, path);
            let body = content.to_string();
            // 更新 FTS 索引（失败时仅记录警告，不阻断主流程）
            if let Err(err) = db::upsert_fts_page(&db_path, path_buf, &title, &body) {
                state.push_log(LogLevel::Warn, format!("FTS 索引更新失败（降级）：{err}"));
            }
            // 同步 wiki_pages.title 到 DB，确保图谱/检索显示正确标题
            if let Err(err) = db::update_wiki_page_title(&db_path, path_buf, &title) {
                state.push_log(LogLevel::Warn, format!("更新 wiki_pages.title 失败：{err}"));
            }
            // 异步更新向量索引（不阻塞主流程；Ollama 不可用时静默跳过）
            let embed_provider = state.get_embed_provider();
            let embed_db_path = db_path.clone();
            let embed_path = path.to_string();
            let embed_content: String = content.chars().take(2000).collect();
            tokio::spawn(async move {
                match embed_provider.embed(&embed_content).await {
                    Ok(embedding) => {
                        if let Err(e) =
                            db::upsert_embedding(&embed_db_path, &embed_path, &embedding)
                        {
                            eprintln!("[embed] 向量索引写入失败（忽略）: {e}");
                        }
                    }
                    Err(_) => {} // Ollama 不可用时静默跳过
                }
            });
        }
    }

    state.record_outbox_event(
        "wiki_page_saved",
        serde_json::json!({
            "path": path,
            "content_length": content.chars().count(),
        }),
    );

    Ok(crate::models::SaveWikiPageResult {
        path: path.to_string(),
        message: format!("已保存并更新索引：{path}"),
    })
}

pub fn list_wiki_page_history_impl(
    state: &AppState,
    path: &str,
    limit: Option<usize>,
) -> Result<Vec<WikiPageHistoryItem>, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    let normalized_limit = limit.unwrap_or(20).clamp(1, 100);
    db::list_wiki_page_history(&db_path, std::path::Path::new(path), normalized_limit).map(
        |records| {
            records
                .into_iter()
                .map(|record| WikiPageHistoryItem {
                    id: record.id,
                    path: record.path,
                    title: record.title,
                    content_hash: record.content_hash,
                    checksum: record.checksum,
                    created_at: record.created_at,
                })
                .collect()
        },
    )
}

pub fn get_wiki_page_history_entry_impl(
    state: &AppState,
    id: i64,
) -> Result<WikiPageHistoryDetail, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    let record = db::get_wiki_page_history_entry(&db_path, id)?
        .ok_or_else(|| format!("未找到页面历史记录：{}", id))?;
    Ok(WikiPageHistoryDetail {
        id: record.id,
        path: record.path,
        title: record.title,
        content_hash: record.content_hash,
        checksum: record.checksum,
        created_at: record.created_at,
        content: record.prev_content.unwrap_or_default(),
    })
}

/// 从历史快照恢复 Wiki 页面内容（"一键恢复到此版本"）。
pub async fn restore_wiki_page_from_history_impl(
    state: &AppState,
    id: i64,
) -> Result<crate::models::SaveWikiPageResult, String> {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };
    let db_path = vault_path.join(".app").join("meta.db");
    let record = db::get_wiki_page_history_entry(&db_path, id)?
        .ok_or_else(|| format!("未找到页面历史记录：{}", id))?;
    let content = record
        .prev_content
        .ok_or_else(|| "历史记录中无内容可恢复".to_string())?;
    let path = record.path;
    save_wiki_page_impl(state, &path, &content, None).await
}

pub async fn create_wiki_page_with_ai_impl(
    state: &AppState,
    topic: String,
) -> Result<NewPageResult, String> {
    // a. 检查 Vault 是否已初始化
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let vault_path = vault_path.ok_or_else(|| "请先初始化 Vault".to_string())?;

    // b. 清理主题字符串
    let topic = topic.trim().to_string();
    if topic.is_empty() {
        return Err("主题名称不能为空".to_string());
    }
    let topic: String = topic.chars().take(200).collect();

    let db_path = vault_path.join(".app").join("meta.db");

    // c. 生成 slug
    let base_slug = super::topic_to_slug(&topic);
    if base_slug.is_empty() {
        return Err("无法从主题生成有效文件名，请使用英文或数字".to_string());
    }

    // d. 找到不冲突的文件路径
    let wiki_dir = vault_path.join("wiki");
    std::fs::create_dir_all(&wiki_dir).map_err(|e| format!("创建 wiki 目录失败: {}", e))?;
    let final_slug = resolve_unique_wiki_slug(&wiki_dir, &base_slug)?;
    let wiki_file_path = wiki_dir.join(format!("{}.md", final_slug));
    let wiki_path_str = wiki_file_path.to_string_lossy().to_string();

    // e. 生成草稿内容（与 Agent Draft 逻辑共用）。
    let (page_title, llm_content, full_content) =
        generate_ai_wiki_markdown_draft_impl(state, &db_path, &topic, None, None, false, None)
            .await?;
    let now_ms = super::current_timestamp_ms();

    // f. 写入文件
    std::fs::write(&wiki_file_path, &full_content)
        .map_err(|e| format!("写入 wiki 文件失败: {}", e))?;

    // g. 更新 DB
    if db_path.exists() || true {
        let content_hash = format!("{:x}", md5_simple(&full_content));
        if let Err(e) = db::upsert_generated_wiki_page(
            &db_path,
            &page_title,
            &wiki_file_path,
            &llm_content.chars().take(300).collect::<String>(),
            &content_hash,
            &now_ms,
        ) {
            state.push_log(LogLevel::Warn, format!("DB 更新失败（降级）: {}", e));
        }
    }

    // h. 更新 FTS
    if let Err(e) = db::upsert_fts_page(&db_path, &wiki_file_path, &page_title, &full_content) {
        state.push_log(LogLevel::Warn, format!("FTS 索引更新失败（降级）: {}", e));
    }

    // i. 返回结果
    let content_preview: String = llm_content.chars().take(300).collect();
    Ok(NewPageResult {
        wiki_path: wiki_path_str,
        title: page_title,
        content_preview,
    })
}

pub async fn rename_wiki_page_impl(
    state: &AppState,
    old_path: &str,
    new_name: &str,
) -> Result<crate::models::RenameWikiPageResult, String> {
    // 1. 验证新文件名（不允许路径分隔符、不能为空）
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("新文件名不能为空".to_string());
    }
    if new_name.contains('/') || new_name.contains('\\') {
        return Err("新文件名不能包含路径分隔符".to_string());
    }
    // 确保以 .md 结尾
    let new_name = if new_name.ends_with(".md") {
        new_name.to_string()
    } else {
        format!("{new_name}.md")
    };

    // 2. 计算新路径（与旧文件同目录）
    let old_file = std::path::Path::new(old_path);
    let parent = old_file
        .parent()
        .ok_or_else(|| format!("无法获取父目录：{old_path}"))?;
    let new_file = parent.join(&new_name);
    let new_path_str = new_file.to_string_lossy().to_string();

    if new_file == old_file {
        return Ok(crate::models::RenameWikiPageResult {
            new_path: new_path_str,
            message: "文件名未变化".to_string(),
        });
    }

    if new_file.exists() {
        return Err(format!("目标文件已存在：{new_path_str}"));
    }

    // 3. 重命名文件
    std::fs::rename(old_file, &new_file).map_err(|err| format!("文件重命名失败：{}", err))?;

    // 4. 读取新文件内容以更新 FTS
    let content = std::fs::read_to_string(&new_file).unwrap_or_default();
    let title = content
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").trim().to_string())
        .unwrap_or_else(|| new_name.trim_end_matches(".md").to_string());

    // 5. 更新数据库
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };

    if let Some(vault_path) = vault_path {
        let db_path = vault_path.join(".app").join("meta.db");
        if db_path.exists() {
            if let Err(err) =
                db::rename_wiki_page_in_db(&db_path, old_file, &new_file, &title, &content)
            {
                state.push_log(LogLevel::Warn, format!("数据库重命名失败（降级）：{err}"));
            }
        }
    }

    state.record_outbox_event(
        "wiki_page_renamed",
        serde_json::json!({
            "old_path": old_path,
            "new_path": new_path_str.clone(),
            "new_name": new_name,
        }),
    );

    Ok(crate::models::RenameWikiPageResult {
        new_path: new_path_str.clone(),
        message: format!("已重命名：{old_path} → {new_path_str}"),
    })
}

pub async fn delete_wiki_page_impl(
    state: &AppState,
    path: &str,
) -> Result<crate::models::DeleteWikiPageResult, String> {
    // 1. 删除 .md 文件
    let file_path = std::path::Path::new(path);
    if file_path.exists() {
        std::fs::remove_file(file_path).map_err(|err| format!("删除文件失败：{}", err))?;
    }

    // 2. 清理数据库记录
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };

    let mut pruned_index_links = 0usize;
    if let Some(vault_path) = vault_path {
        let db_path = vault_path.join(".app").join("meta.db");
        if db_path.exists() {
            if let Err(err) = db::delete_wiki_page_from_db(&db_path, file_path) {
                state.push_log(LogLevel::Warn, format!("数据库清理失败（降级）：{err}"));
            }
        }
        match prune_missing_index_links(&vault_path) {
            Ok(removed) => {
                pruned_index_links = removed;
            }
            Err(err) => {
                state.push_log(LogLevel::Warn, format!("index.md 清理失败（降级）：{err}"));
            }
        }
    }

    state.record_outbox_event(
        "wiki_page_deleted",
        serde_json::json!({
            "path": path,
            "pruned_index_links": pruned_index_links,
        }),
    );

    Ok(crate::models::DeleteWikiPageResult {
        path: path.to_string(),
        message: if pruned_index_links > 0 {
            format!("已删除：{path}（同步清理 index.md 失效链接 {pruned_index_links} 条）")
        } else {
            format!("已删除：{path}")
        },
    })
}

/// 启动时清理孤立 wiki 页面：DB 有记录但文件已不存在的条目。
/// 在 setup hook 中调用，保证前端首次加载拿到的数据已是干净状态。
pub fn purge_orphaned_wiki_pages(state: &AppState) {
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let Some(vault_path) = vault_path else {
        return; // vault 未配置，跳过
    };

    let db_path = vault_path.join(".app").join("meta.db");
    if !db_path.exists() {
        return;
    }

    let paths = match db::list_wiki_page_paths(&db_path) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("[purge_orphaned] 读取 wiki_pages 失败: {err}");
            return;
        }
    };

    let mut purged = 0usize;
    for path_str in &paths {
        let file_path = std::path::Path::new(path_str);
        if !file_path.exists() {
            match db::delete_wiki_page_from_db(&db_path, file_path) {
                Ok(()) => {
                    eprintln!("[purge_orphaned] 已清理孤立记录: {path_str}");
                    purged += 1;
                }
                Err(err) => {
                    eprintln!("[purge_orphaned] 清理失败 {path_str}: {err}");
                }
            }
        }
    }

    let mut pruned_index_links = 0usize;
    match prune_missing_index_links(&vault_path) {
        Ok(removed) => {
            pruned_index_links = removed;
        }
        Err(err) => {
            eprintln!("[purge_orphaned] index.md 清理失败: {err}");
        }
    }

    if purged > 0 {
        eprintln!("[purge_orphaned] 启动清理完成，共删除 {purged} 条孤立 wiki 页面记录");
    }
    if pruned_index_links > 0 {
        eprintln!(
            "[purge_orphaned] 启动清理完成，index.md 共移除 {pruned_index_links} 条失效链接"
        );
    }
}

// ─── Private free function helpers used by methods above ─────────────────────

fn file_modified_timestamp_ms(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|dur| dur.as_millis().to_string())
        .unwrap_or_else(super::current_timestamp_ms)
}

fn is_existing_wiki_page_target(vault_path: &Path, raw_path: &str) -> bool {
    let Ok(candidate) = resolve_wiki_page_candidate(vault_path, raw_path) else {
        return false;
    };
    if !candidate.exists() {
        return false;
    }

    let wiki_root = vault_path.join("wiki");
    let Ok(canonical_root) = fs::canonicalize(&wiki_root) else {
        return false;
    };
    let Ok(canonical_target) = fs::canonicalize(&candidate) else {
        return false;
    };

    canonical_target.starts_with(&canonical_root)
}

pub(super) fn resolve_wiki_page_candidate(vault_path: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let wiki_root = vault_path.join("wiki");
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("页面路径不能为空".to_string());
    }
    // 外部 URL 不是 wiki 页面路径，直接拒绝
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("ftp://")
        || trimmed.starts_with("mailto:")
    {
        return Err("外部 URL 不是 wiki 页面路径".to_string());
    }

    let input_path = PathBuf::from(trimmed);
    Ok(if input_path.is_absolute() {
        input_path
    } else if trimmed.starts_with("wiki/")
        || trimmed.starts_with("wiki\\")
        || trimmed.starts_with("./wiki/")
        || trimmed.starts_with("./wiki\\")
    {
        vault_path.join(trimmed)
    } else {
        wiki_root.join(input_path)
    })
}

pub(super) fn prune_missing_index_links(vault_path: &Path) -> Result<usize, String> {
    let index_path = vault_path.join("index.md");
    if !index_path.exists() {
        return Ok(0);
    }

    let content =
        fs::read_to_string(&index_path).map_err(|err| format!("读取 index.md 失败: {}", err))?;
    let (updated, removed) = prune_missing_index_links_from_content(vault_path, &content);
    if removed > 0 {
        fs::write(&index_path, updated).map_err(|err| format!("写入 index.md 失败: {}", err))?;
    }
    Ok(removed)
}

pub(super) fn prune_missing_index_links_from_content(vault_path: &Path, content: &str) -> (String, usize) {
    let wiki_link_re =
        regex::Regex::new(r"\[\[([^|\]]+)(?:\|[^\]]+)?\]\]").expect("wiki link regex 应可编译");
    let markdown_link_re =
        regex::Regex::new(r"\[[^\]]+\]\(([^)]+)\)").expect("markdown link regex 应可编译");
    let mut kept_lines = Vec::new();
    let mut removed = 0usize;

    for line in content.lines() {
        let mut should_remove_line = false;
        for capture in wiki_link_re.captures_iter(line) {
            let raw_target = capture.get(1).map(|m| m.as_str().trim()).unwrap_or("");
            if raw_target.is_empty() {
                continue;
            }
            if !is_existing_wiki_page_target(vault_path, raw_target)
                && resolve_wiki_page_candidate(vault_path, raw_target).is_ok()
            {
                should_remove_line = true;
                break;
            }
        }

        if !should_remove_line {
            for capture in markdown_link_re.captures_iter(line) {
                let raw_target = capture.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                if raw_target.is_empty() {
                    continue;
                }
                if !is_existing_wiki_page_target(vault_path, raw_target)
                    && resolve_wiki_page_candidate(vault_path, raw_target).is_ok()
                {
                    should_remove_line = true;
                    break;
                }
            }
        }

        if should_remove_line {
            removed += 1;
        } else {
            kept_lines.push(line);
        }
    }

    let updated = if content.ends_with('\n') {
        format!("{}\n", kept_lines.join("\n"))
    } else {
        kept_lines.join("\n")
    };
    (updated, removed)
}

/// 计算字符串的简单哈希（FNV-1a 64-bit，用于生成 content_hash）。
pub(super) fn md5_simple(input: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037u64;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(1099511628211u64);
    }
    hash
}

// ─── Wiki 内联 AI 辅助编辑 ───────────────────────────────────────────────────

/// Wiki 内联 AI 辅助编辑：续写 / 改写 / 扩写，流式 emit `ai_assist_chunk` 事件。
pub async fn ai_assist_wiki_edit(
    state: &AppState,
    app_handle: &tauri::AppHandle,
    action: &str,
    selected_text: &str,
    context: &str,
    page_title: &str,
) -> Result<(), String> {
    use tauri::Emitter;

    let provider = state
        .get_llm_provider()
        .ok_or_else(|| "LLM Provider 未配置，请先在设置中配置模型".to_string())?;

    let prompt = build_assist_prompt(action, selected_text, context, page_title)?;

    let app = app_handle.clone();
    let result = provider
        .complete_stream(&prompt, &mut |chunk| {
            let _ = app.emit("ai_assist_chunk", serde_json::json!({ "chunk": chunk }));
        })
        .await;

    match result {
        Ok(_) => {
            let _ = app_handle.emit("ai_assist_done", serde_json::json!({}));
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = app_handle.emit("ai_assist_error", serde_json::json!({ "error": &msg }));
            Err(msg)
        }
    }
}

fn build_assist_prompt(
    action: &str,
    selected_text: &str,
    context: &str,
    page_title: &str,
) -> Result<String, String> {
    let prompt = match action {
        "continue" => format!(
            "You are a writing assistant helping to continue text in a document titled '{title}'.\n\
             Here is the document context:\n\n{ctx}\n\n\
             Continue writing naturally after the last sentence. \
             Output only the continuation text, no preamble or explanation. \
             Write in the same language and style as the existing text.",
            title = page_title,
            ctx = context.chars().take(2000).collect::<String>(),
        ),
        "rewrite" => format!(
            "Rewrite the following text to be clearer, more concise, and better structured. \
             Output only the rewritten text with no preamble or explanation. \
             Preserve the original meaning and language:\n\n{text}",
            text = selected_text,
        ),
        "expand" => format!(
            "Expand the following text/bullet points into a well-written, detailed paragraph or section. \
             Output only the expanded text with no preamble or explanation. \
             Write in the same language as the input:\n\n{text}",
            text = selected_text,
        ),
        other => return Err(format!("未知操作类型: {other}")),
    };
    Ok(prompt)
}

// ─── Wiki 知识导出 ────────────────────────────────────────────────────────────

struct ExportPage {
    abs_path: PathBuf,
    /// 相对于 wiki/ 根的 zip 内路径，如 "notes/topic.md"
    zip_rel: String,
    title: String,
}

/// 递归收集 wiki 目录下所有 .md 文件。
fn collect_export_pages(base: &Path, dir: &Path) -> Result<Vec<ExportPage>, String> {
    let mut pages = Vec::new();
    collect_export_pages_inner(base, dir, &mut pages)?;
    Ok(pages)
}

fn collect_export_pages_inner(
    base: &Path,
    dir: &Path,
    out: &mut Vec<ExportPage>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("读取目录失败: {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("目录条目读取失败: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_export_pages_inner(base, &path, out)?;
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            let rel = path
                .strip_prefix(base)
                .map_err(|e| format!("路径处理失败: {e}"))?;
            let zip_rel = rel.to_string_lossy().replace('\\', "/");
            let title = path
                .file_stem()
                .map(|s| s.to_string_lossy().replace(['-', '_'], " "))
                .unwrap_or_else(|| "Untitled".into());
            out.push(ExportPage { abs_path: path, zip_rel, title });
        }
    }
    Ok(())
}

/// 将 Markdown 渲染为 HTML 页面（含 wiki 链接转换）。
fn render_page_html(md: &str, title: &str, depth: usize) -> String {
    use pulldown_cmark::{html, Options, Parser};
    use regex::Regex;

    let wiki_link_re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let md_linked = wiki_link_re.replace_all(md, |caps: &regex::Captures| {
        let link = &caps[1];
        let href = format!("{}.html", link.replace(' ', "%20"));
        format!("[{link}]({href})")
    });

    let mut html_body = String::new();
    html::push_html(&mut html_body, Parser::new_ext(&md_linked, Options::all()));

    let back = "../".repeat(depth) + "index.html";
    format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head><meta charset="utf-8"><title>{t}</title>
<style>
body{{font-family:system-ui,sans-serif;max-width:820px;margin:2rem auto;padding:0 1.2rem;line-height:1.7;color:#1e293b;}}
pre{{background:#f5f5f5;padding:1rem;overflow:auto;border-radius:4px;}}
code{{background:#f0f0f0;padding:.1em .3em;border-radius:3px;font-size:.9em;}}
a{{color:#6366f1;}}blockquote{{border-left:3px solid #ccc;margin:0;padding-left:1rem;color:#666;}}
.back{{font-size:.85em;margin-bottom:1.5rem;}}
</style></head>
<body>
<p class="back"><a href="{back}">← 返回目录</a></p>
{body}
</body></html>"#,
        t = html_escape(title),
        back = back,
        body = html_body,
    )
}

fn build_wiki_index_html(pages: &[ExportPage]) -> String {
    let rows: String = pages
        .iter()
        .map(|p| {
            let html_path = p.zip_rel.replace(".md", ".html");
            format!(
                r#"    <li><a href="{}">{}</a></li>"#,
                html_escape(&html_path),
                html_escape(&p.title)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head><meta charset="utf-8"><title>LLM Wiki 知识库</title>
<style>body{{font-family:system-ui,sans-serif;max-width:820px;margin:2rem auto;padding:0 1.2rem;}}
a{{color:#6366f1;}}li{{margin:.3rem 0;}}h1{{margin-bottom:.5rem;}}p{{color:#64748b;margin-bottom:1.5rem;}}
</style></head>
<body>
<h1>LLM Wiki 知识库</h1>
<p>共 {count} 个页面</p>
<ul>
{rows}
</ul>
</body></html>"#,
        count = pages.len(),
        rows = rows,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 导出全部 Wiki 为 Markdown ZIP 包，返回导出页面数。
pub fn export_wiki_markdown_zip_impl(state: &AppState, dest_path: String) -> Result<u32, String> {
    use std::io::Write;

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone().ok_or_else(|| "请先初始化 Vault".to_string())?
    };
    let wiki_dir = vault_path.join("wiki");
    if !wiki_dir.exists() {
        return Ok(0);
    }

    let pages = collect_export_pages(&wiki_dir, &wiki_dir)?;

    let file =
        fs::File::create(&dest_path).map_err(|e| format!("创建 ZIP 文件失败: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    for page in &pages {
        let content = fs::read(&page.abs_path).map_err(|e| format!("读取文件失败: {e}"))?;
        zip.start_file(&page.zip_rel, opts)
            .map_err(|e| format!("写入 {} 失败: {e}", page.zip_rel))?;
        zip.write_all(&content)
            .map_err(|e| format!("写入 {} 内容失败: {e}", page.zip_rel))?;
    }

    zip.finish().map_err(|e| format!("完成 ZIP 失败: {e}"))?;
    Ok(pages.len() as u32)
}

/// 导出全部 Wiki 为静态 HTML ZIP 包（含 index.html），返回导出页面数。
pub fn export_wiki_html_zip_impl(state: &AppState, dest_path: String) -> Result<u32, String> {
    use std::io::Write;

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone().ok_or_else(|| "请先初始化 Vault".to_string())?
    };
    let wiki_dir = vault_path.join("wiki");
    if !wiki_dir.exists() {
        return Ok(0);
    }

    let pages = collect_export_pages(&wiki_dir, &wiki_dir)?;

    let file =
        fs::File::create(&dest_path).map_err(|e| format!("创建 ZIP 文件失败: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // index.html
    let index_html = build_wiki_index_html(&pages);
    zip.start_file("index.html", opts)
        .map_err(|e| format!("写入 index.html 失败: {e}"))?;
    zip.write_all(index_html.as_bytes())
        .map_err(|e| format!("写入 index.html 内容失败: {e}"))?;

    // 每个页面的 HTML
    for page in &pages {
        let md = fs::read_to_string(&page.abs_path).unwrap_or_default();
        let depth = page.zip_rel.matches('/').count();
        let html = render_page_html(&md, &page.title, depth);
        let html_path = page.zip_rel.replace(".md", ".html");
        zip.start_file(&html_path, opts)
            .map_err(|e| format!("写入 {} 失败: {e}", html_path))?;
        zip.write_all(html.as_bytes())
            .map_err(|e| format!("写入 {} 内容失败: {e}", html_path))?;
    }

    zip.finish().map_err(|e| format!("完成 ZIP 失败: {e}"))?;
    Ok(pages.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, state::test_helpers::*};
    use crate::models::{QueryCitation, SaveQueryAnswerInput};
    use rusqlite::{params, Connection};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    // ── Phase 6a: Pure tests ──────────────────────────────────────────────────

    #[test]
    fn is_raw_ingest_id_detects_timestamp_pattern() {
        assert!(is_raw_ingest_id("ingest-1777101379565550500"));
        assert!(is_raw_ingest_id("ingest-0"));
        assert!(!is_raw_ingest_id("rust生命周期"));
        assert!(!is_raw_ingest_id("ingest-abc123"));
        assert!(!is_raw_ingest_id("ingest-123abc"));
        assert!(!is_raw_ingest_id("ingest-"));
        assert!(!is_raw_ingest_id(""));
    }

    #[test]
    fn resolve_graph_node_label_preserves_meaningful_title() {
        let fm = crate::models::WikiPageFrontmatter {
            title: Some("rust生命周期".to_string()),
            source: None,
            raw: None,
            imported_at: None,
            entities: vec!["Rust".to_string()],
            stale: None,
        };
        assert_eq!(
            resolve_graph_node_label("rust生命周期", &fm),
            "rust生命周期"
        );
    }

    #[test]
    fn resolve_graph_node_label_uses_first_entity_for_raw_id() {
        let fm = crate::models::WikiPageFrontmatter {
            title: Some("ingest-1777101379565550500".to_string()),
            source: None,
            raw: None,
            imported_at: None,
            entities: vec!["PINNs".to_string(), "Transformer".to_string()],
            stale: None,
        };
        assert_eq!(
            resolve_graph_node_label("ingest-1777101379565550500", &fm),
            "PINNs"
        );
    }

    #[test]
    fn resolve_graph_node_label_falls_back_to_source_stem() {
        let fm = crate::models::WikiPageFrontmatter {
            title: None,
            source: Some(r"E:\vault\research\大模型rag框架进展.md".to_string()),
            raw: None,
            imported_at: None,
            entities: vec![],
            stale: None,
        };
        assert_eq!(
            resolve_graph_node_label("ingest-1776768806095623000", &fm),
            "大模型rag框架进展"
        );
    }

    #[test]
    fn resolve_graph_node_label_skips_internal_source_path() {
        let fm = crate::models::WikiPageFrontmatter {
            title: None,
            source: Some(r"C:\Temp\llm_wiki_preview_apply_123.md".to_string()),
            raw: None,
            imported_at: None,
            entities: vec![],
            stale: None,
        };
        assert_eq!(
            resolve_graph_node_label("ingest-1776000000000000000", &fm),
            "ingest-1776000000000000000"
        );
    }

    #[test]
    fn friendly_display_path_str_strips_windows_verbatim_prefix() {
        assert_eq!(
            friendly_display_path_str(r"\\?\C:\llm-wiki\vault\wiki\detail.md"),
            r"C:\llm-wiki\vault\wiki\detail.md"
        );
        assert_eq!(
            friendly_display_path_str(r"\\?\UNC\server\share\vault\wiki\detail.md"),
            r"\\server\share\vault\wiki\detail.md"
        );
    }

    #[test]
    fn prune_missing_index_links_from_content_removes_only_missing_targets() {
        let vault_dir = make_temp_dir("llm-wiki-prune-index-content");
        let _guard = TempDirGuard(vault_dir.clone());
        fs::create_dir_all(vault_dir.join("wiki")).expect("创建 wiki 目录失败");
        fs::write(vault_dir.join("wiki").join("kept.md"), "# kept").expect("写入 kept 页面失败");

        let content = [
            "# Index",
            "- [[wiki/kept.md|kept]]",
            "- [[wiki/missing.md|missing]]",
            "- [missing-md](wiki/missing2.md)",
            "- [external](https://example.com)",
        ]
        .join("\n");

        let (updated, removed) = prune_missing_index_links_from_content(&vault_dir, &content);
        assert_eq!(removed, 2);
        assert!(updated.contains("[[wiki/kept.md|kept]]"));
        assert!(!updated.contains("[[wiki/missing.md|missing]]"));
        assert!(!updated.contains("wiki/missing2.md"));
        assert!(updated.contains("https://example.com"));
    }

    #[test]
    fn search_wiki_matches_with_fts_prefers_fts_strategy() {
        let vault_dir = make_temp_dir("llm-wiki-query-fts");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let wiki_dir = vault_dir.join("wiki");
        let wiki_path = wiki_dir.join("fts-hit.md");
        let content = "# FTS Hit\nRust backend with tauri integration.\n";
        fs::write(&wiki_path, content).expect("写入 fts-hit 失败");
        db::upsert_fts_page(&db_path, &wiki_path, "fts-hit", content).expect("写入 fts 索引失败");

        let tokens = tokenize_query("Rust backend");
        let (matches, strategy, fts_error, search_debug) =
            search_wiki_matches_with_fts(&db_path, &wiki_dir, &tokens, "Rust backend", 3)
                .expect("执行检索失败");

        assert!(fts_error.is_none());
        assert_eq!(strategy, "fts");
        assert!(search_debug.is_some());
        assert!(!matches.is_empty());
        assert!(matches.iter().any(|item| item.page_path.ends_with("fts-hit.md")));
    }

    #[test]
    fn tokenize_query_supports_cjk_segments_and_bigrams() {
        let tokens = tokenize_query("这个项目的核心目标是什么？query v1");

        assert!(tokens.iter().any(|item| item == "query"));
        assert!(tokens.iter().any(|item| item == "这个项目的核心目标是什么"));
        assert!(tokens.iter().any(|item| item == "核心"));
        assert!(tokens.iter().any(|item| item == "目标"));
    }

    #[test]
    fn tokenize_query_filters_stopwords() {
        let tokens = tokenize_query("the 这个 项目 的 核心 目标 是 什么");

        assert!(!tokens.iter().any(|item| item == "the"));
        assert!(!tokens.iter().any(|item| item == "的"));
        assert!(!tokens.iter().any(|item| item == "是"));
        assert!(tokens.iter().any(|item| item == "项目"));
        assert!(tokens.iter().any(|item| item == "核心"));
    }

    #[test]
    fn search_wiki_matches_from_paths_applies_phrase_title_boost_and_deduplicates() {
        let vault_dir = make_temp_dir("llm-wiki-query-rank");
        let _guard = TempDirGuard(vault_dir.clone());
        let wiki_dir = vault_dir.join("wiki");
        fs::create_dir_all(&wiki_dir).expect("创建 wiki 目录失败");

        let high_path = wiki_dir.join("high.md");
        let low_path = wiki_dir.join("low.md");
        fs::write(
            &high_path,
            "# 核心目标\n这个项目的核心目标是什么 这是完整短语。\n",
        )
        .expect("写入 high.md 失败");
        fs::write(&low_path, "# 其他说明\n这个 项目 核心 目标 分散出现。\n")
            .expect("写入 low.md 失败");

        let page_paths = vec![
            high_path.to_string_lossy().to_string(),
            high_path.to_string_lossy().to_string(),
            low_path.to_string_lossy().to_string(),
        ];
        let question = "这个项目的核心目标是什么";
        let tokens = tokenize_query(question);
        let matches = search_wiki_matches_from_paths(&page_paths, &tokens, question, 5)
            .expect("执行页面匹配失败");

        assert_eq!(matches.len(), 2);
        assert!(matches[0].page_path.ends_with("high.md"));
        assert!(matches[0].score >= matches[1].score);
    }

    #[test]
    fn search_wiki_matches_rrf_degrades_gracefully_on_empty_vault() {
        let tmp = make_temp_dir("llm-wiki-rrf-degrade");
        let _guard = TempDirGuard(tmp.clone());
        let db_path = tmp.join("meta.db");
        let wiki_dir = tmp.join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();
        let tokens = vec!["test".to_string()];
        let result = search_wiki_matches_rrf(&db_path, &wiki_dir, &tokens, "test", 3);
        assert!(result.is_ok());
        let (matches, _, _, _) = result.unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn search_wiki_matches_rrf_accepts_embedding_extra_route() {
        let tmp = make_temp_dir("llm-wiki-rrf-extra-route");
        let _guard = TempDirGuard(tmp.clone());
        let db_path = tmp.join("meta.db");
        let wiki_dir = tmp.join("wiki");
        fs::create_dir_all(&wiki_dir).unwrap();

        let page_path = wiki_dir.join("embedding-only.md");
        fs::write(&page_path, "# Embedding\n\nRust embedding recall path").unwrap();

        let tokens = vec!["rust".to_string()];
        let extra_routes = vec![(
            "embedding".to_string(),
            vec![page_path.to_string_lossy().to_string()],
        )];
        let result = search_wiki_matches_rrf_with_extra_routes(
            &db_path,
            &wiki_dir,
            &tokens,
            "rust",
            3,
            &extra_routes,
        )
        .expect("执行含 embedding 扩展路径的 RRF 失败");

        assert_eq!(result.1, "rrf");
        assert_eq!(result.0.len(), 1);
        assert!(result.0[0].page_path.ends_with("embedding-only.md"));
        let debug = result.3.expect("应返回检索调试信息");
        assert!(debug.routes.iter().any(|item| item.route == "embedding"));
    }

    #[test]
    fn set_frontmatter_stale_field_adds_stale_true() {
        let content = "---\ntitle: 'Test'\nimported_at: '2026-01-01'\n---\n# Body\n";
        let result = set_frontmatter_stale_field(content, true);
        assert!(result.contains("stale: true"), "应包含 stale: true");
        assert!(result.contains("title:"), "不应丢失 title");
    }

    #[test]
    fn set_frontmatter_stale_field_removes_stale_on_false() {
        let content = "---\ntitle: 'Test'\nstale: true\n---\n# Body\n";
        let result = set_frontmatter_stale_field(content, false);
        assert!(!result.contains("stale:"), "应移除 stale 字段");
        assert!(result.contains("title:"), "不应丢失 title");
    }

    // ── Phase 6b: AppState tests ──────────────────────────────────────────────

    #[test]
    fn prune_missing_index_links_updates_file_and_returns_removed_count() {
        let vault_dir = make_temp_dir("llm-wiki-prune-index-file");
        let _guard = TempDirGuard(vault_dir.clone());
        fs::create_dir_all(vault_dir.join("wiki")).expect("创建 wiki 目录失败");
        fs::write(vault_dir.join("wiki").join("exists.md"), "# exists").expect("写入页面失败");
        fs::write(
            vault_dir.join("index.md"),
            "- [[wiki/exists.md|exists]]\n- [[wiki/gone.md|gone]]\n",
        )
        .expect("写入 index 失败");

        let removed = prune_missing_index_links(&vault_dir).expect("清理 index 链接失败");
        assert_eq!(removed, 1);

        let updated = fs::read_to_string(vault_dir.join("index.md")).expect("读取 index 失败");
        assert!(updated.contains("[[wiki/exists.md|exists]]"));
        assert!(!updated.contains("[[wiki/gone.md|gone]]"));
    }

    #[test]
    fn recent_wiki_pages_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-recent-wiki-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.recent_wiki_pages(10);
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn recent_wiki_pages_returns_db_rows() {
        let vault_dir = make_temp_dir("llm-wiki-recent-wiki");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let wiki_a_path = vault_dir.join("wiki").join("a.md");
        let wiki_b_path = vault_dir.join("wiki").join("b.md");
        fs::write(&wiki_a_path, "# Wiki A\nContent A").expect("创建 wiki a 失败");
        fs::write(&wiki_b_path, "# Wiki B\nContent B").expect("创建 wiki b 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-wiki", "source", "raw", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "Wiki A", wiki_a_path.to_string_lossy().to_string(), "summary a", "1", "2"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "Wiki B", wiki_b_path.to_string_lossy().to_string(), "summary b", "1", "3"],
        )
        .expect("写入 wiki_pages 失败");

        let pages = state.recent_wiki_pages(10).expect("读取 recent wiki 失败");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].title, "Wiki B");
        assert_eq!(pages[1].title, "Wiki A");
        assert_paths_semantically_equal(
            &wiki_b_path,
            pages[0].display_path.as_deref().expect("缺少显示路径"),
        );
        assert_paths_semantically_equal(
            &wiki_a_path,
            pages[1].display_path.as_deref().expect("缺少显示路径"),
        );
    }

    #[test]
    fn search_wiki_pages_filters_rows() {
        let vault_dir = make_temp_dir("llm-wiki-search-wiki");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-search", "source", "raw", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "Rust Wiki", vault_dir.join("wiki").join("rust.md").to_string_lossy().to_string(), "rust topic", "1", "2"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "Tauri Wiki", vault_dir.join("wiki").join("tauri.md").to_string_lossy().to_string(), "desktop topic", "1", "3"],
        )
        .expect("写入 wiki_pages 失败");

        let rust_pages = state.search_wiki_pages("rust".to_string(), 10).expect("搜索 wiki 页面失败");
        assert_eq!(rust_pages.len(), 1);
        assert_eq!(rust_pages[0].title, "Rust Wiki");

        let all_pages = state.search_wiki_pages("   ".to_string(), 10).expect("读取最近 wiki 页面失败");
        assert_eq!(all_pages.len(), 2);
    }

    #[test]
    fn wiki_page_detail_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.wiki_page_detail("wiki/a.md".to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn wiki_page_detail_reads_markdown_content() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("detail.md");
        fs::write(&page_path, "# Detail Title\n\n正文内容。").expect("写入页面失败");

        let detail = state
            .wiki_page_detail(page_path.to_string_lossy().to_string())
            .expect("读取页面详情失败");
        assert_eq!(detail.title, "Detail Title");
        assert!(detail.content.contains("正文内容"));
        assert!(detail.frontmatter.is_none());
        assert_paths_semantically_equal(&page_path, &detail.path);
        assert_paths_semantically_equal(&page_path, &detail.display_path);
    }

    #[test]
    fn wiki_page_detail_parses_frontmatter_fields() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-frontmatter");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("detail-frontmatter.md");
        fs::write(
            &page_path,
            r#"---
title: "Detail Title"
source: "E:\\llm-wiki\\source\\detail.md"
raw: "raw/detail.md"
imported_at: "2026-04-13T18:00:00+08:00"
entities:
  - "Rust"
  - "SQLite"
---
# Detail Title

正文内容。
"#,
        )
        .expect("写入页面失败");

        let detail = state
            .wiki_page_detail(page_path.to_string_lossy().to_string())
            .expect("读取页面详情失败");
        let frontmatter = detail.frontmatter.expect("未解析到 frontmatter");
        assert_eq!(frontmatter.title.as_deref(), Some("Detail Title"));
        assert_eq!(frontmatter.source.as_deref(), Some(r"E:\llm-wiki\source\detail.md"));
        assert_eq!(frontmatter.raw.as_deref(), Some("raw/detail.md"));
        assert_eq!(frontmatter.imported_at.as_deref(), Some("2026-04-13T18:00:00+08:00"));
        assert_eq!(
            frontmatter.entities,
            vec!["Rust".to_string(), "SQLite".to_string()]
        );
    }

    #[test]
    fn wiki_page_detail_accepts_wiki_relative_path() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-relative");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("relative.md");
        fs::write(&page_path, "# Relative Title\n\n相对路径页面。").expect("写入页面失败");

        let detail = state
            .wiki_page_detail("wiki/relative.md".to_string())
            .expect("读取页面详情失败");
        assert_eq!(detail.title, "Relative Title");
        assert!(detail.content.contains("相对路径页面"));
    }

    #[test]
    fn wiki_page_detail_rejects_outside_wiki_root() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-safe");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let outside_path = vault_dir.join("outside.md");
        fs::write(&outside_path, "# Outside").expect("写入外部文件失败");

        let result = state.wiki_page_detail(outside_path.to_string_lossy().to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("只允许读取 vault/wiki 目录下的页面".to_string())
        );
    }

    #[test]
    fn wiki_page_citations_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.wiki_page_citations("wiki/a.md".to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn wiki_page_citations_returns_rows_and_target_existence_flags() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let target_path = vault_dir.join("wiki").join("detail.md");
        fs::write(&target_path, "# Detail\n\n页面正文。").expect("写入页面失败");
        let inside_cited_path = vault_dir.join("wiki").join("cited.md");
        fs::write(&inside_cited_path, "# Cited\n\n被引用页面。").expect("写入引用页失败");
        let outside_cited_path = vault_dir.join("outside-cited.md");
        fs::write(&outside_cited_path, "# Outside\n\n外部页面。").expect("写入外部引用页失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                fs::canonicalize(&target_path).expect("规范化目标页失败").to_string_lossy().to_string(),
                fs::canonicalize(&inside_cited_path).expect("规范化引用页失败").to_string_lossy().to_string(),
                7_i64, "命中本地页面", "1"
            ],
        )
        .expect("写入 citations 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                fs::canonicalize(&target_path).expect("规范化目标页失败").to_string_lossy().to_string(),
                outside_cited_path.to_string_lossy().to_string(),
                4_i64, "越界引用", "2"
            ],
        )
        .expect("写入 citations 失败");

        let citations = state
            .wiki_page_citations(target_path.to_string_lossy().to_string())
            .expect("读取页面引用失败");
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].score, 7);
        assert_eq!(citations[0].excerpt, "命中本地页面");
        assert!(citations[0].target_exists);
        assert_paths_semantically_equal(
            &inside_cited_path,
            citations[0].cited_page_display_path.as_deref().expect("缺少显示路径"),
        );
        assert_eq!(
            citations[0].cited_page_path,
            fs::canonicalize(&inside_cited_path).expect("规范化引用页失败").to_string_lossy().to_string()
        );
        assert_eq!(citations[1].score, 4);
        assert_eq!(citations[1].excerpt, "越界引用");
        assert!(!citations[1].target_exists);
        assert_paths_semantically_equal(
            &outside_cited_path,
            citations[1].cited_page_display_path.as_deref().expect("缺少显示路径"),
        );
        assert_eq!(citations[1].cited_page_path, outside_cited_path.to_string_lossy());
    }

    #[test]
    fn wiki_page_citations_rejects_outside_wiki_root() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations-safe");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let outside_path = vault_dir.join("outside.md");
        fs::write(&outside_path, "# Outside").expect("写入外部文件失败");

        let result = state.wiki_page_citations(outside_path.to_string_lossy().to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("只允许读取 vault/wiki 目录下的页面".to_string())
        );
    }

    #[test]
    fn save_query_answer_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-save-query-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let input = SaveQueryAnswerInput {
            question: "q".to_string(),
            answer: "a".to_string(),
            citations: Vec::new(),
            title: None,
        };

        let result = state.save_query_answer(input);
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn save_query_answer_writes_wiki_file_and_updates_db() {
        let vault_dir = make_temp_dir("llm-wiki-save-query");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let input = SaveQueryAnswerInput {
            question: "这个项目的核心目标是什么？".to_string(),
            answer: "目标是构建 Windows 优先的个人 Wiki 桌面应用。".to_string(),
            citations: vec![QueryCitation {
                page_path: vault_dir.join("wiki").join("ingest-1.md").to_string_lossy().to_string(),
                display_path: None,
                score: 3,
                excerpt: "本项目用于实现一个 Windows 优先的个人 Wiki 桌面应用".to_string(),
            }],
            title: Some("问答-核心目标".to_string()),
        };
        let result = state.save_query_answer(input).expect("保存 Query 结果失败");

        assert!(PathBuf::from(&result.wiki_path).exists());
        let index_content = fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        assert!(index_content.contains("问答-核心目标"));

        let db_path = vault_dir.join(".app").join("meta.db");
        let page_paths = db::list_wiki_page_paths(&db_path).expect("读取 wiki_pages 失败");
        assert!(page_paths.iter().any(|path| path == &result.wiki_path));
    }

    #[test]
    fn get_page_embedding_similarities_returns_high_sim_pairs() {
        let dir = make_temp_dir("llm-wiki-emb-sim");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        state.init_vault(dir.clone()).unwrap();

        let db_path = dir.join(".app").join("meta.db");
        db::upsert_embedding(&db_path, "wiki/a.md", &[1.0_f32, 0.0, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/b.md", &[1.0_f32, 0.0, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/c.md", &[0.0_f32, 1.0, 0.0]).unwrap();

        let paths = vec![
            "wiki/a.md".to_string(),
            "wiki/b.md".to_string(),
            "wiki/c.md".to_string(),
        ];
        let result = state.get_page_embedding_similarities(paths).unwrap();

        assert!(result.contains_key("wiki/a.md||wiki/b.md"), "a-b 对应包含");
        let sim = result["wiki/a.md||wiki/b.md"];
        assert!((sim - 1.0).abs() < 1e-6, "a-b 相似度应为 1.0，实际: {}", sim);
        assert!(!result.contains_key("wiki/a.md||wiki/c.md"), "a-c 直交不应包含");
        assert!(!result.contains_key("wiki/b.md||wiki/c.md"), "b-c 直交不应包含");
    }

    #[test]
    fn get_page_embedding_similarities_filters_to_requested_paths() {
        let dir = make_temp_dir("llm-wiki-emb-sim-filter");
        let _guard = TempDirGuard(dir.clone());
        let state = make_test_state(&dir);
        state.init_vault(dir.clone()).unwrap();

        let db_path = dir.join(".app").join("meta.db");
        db::upsert_embedding(&db_path, "wiki/a.md", &[1.0_f32, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/b.md", &[1.0_f32, 0.0]).unwrap();
        db::upsert_embedding(&db_path, "wiki/c.md", &[1.0_f32, 0.0]).unwrap();

        let paths = vec!["wiki/a.md".to_string(), "wiki/b.md".to_string()];
        let result = state.get_page_embedding_similarities(paths).unwrap();
        assert!(result.contains_key("wiki/a.md||wiki/b.md"));
        assert!(!result.keys().any(|k| k.contains("wiki/c.md")));
    }

    /// 验证 WikiPageDetail.content 字段可被 serde_json 正确序列化/反序列化（round-trip）
    #[test]
    fn test_wiki_page_detail_content_field_roundtrip() {
        let detail = crate::models::WikiPageDetail {
            title: "测试页面".to_string(),
            path: "vault/test.md".to_string(),
            display_path: "test.md".to_string(),
            frontmatter: None,
            content: "# Hello\n\nWorld".to_string(),
            updated_at: "1000000".to_string(),
        };
        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("Hello"));
        let restored: crate::models::WikiPageDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.content, "# Hello\n\nWorld");
    }

    #[test]
    fn save_wiki_page_records_previous_content_history() {
        let vault_dir = make_temp_dir("llm-wiki-page-history");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("history.md");
        fs::write(&page_path, "# History\nfirst version\n").expect("写入初始页面失败");

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                "# History\nsecond version\n",
                None,
            ))
            .expect("第一次保存失败");
        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                "# History\nthird version\n",
                None,
            ))
            .expect("第二次保存失败");

        let history = state
            .list_wiki_page_history_impl(page_path.to_str().expect("页面路径不是 UTF-8"), Some(10))
            .expect("读取历史列表失败");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].title, "History");

        let latest = state
            .get_wiki_page_history_entry_impl(history[0].id)
            .expect("读取最新历史详情失败");
        let older = state
            .get_wiki_page_history_entry_impl(history[1].id)
            .expect("读取较早历史详情失败");

        assert_eq!(latest.content, "# History\nsecond version\n");
        assert_eq!(older.content, "# History\nfirst version\n");
        assert_eq!(
            fs::read_to_string(&page_path).expect("读取当前页面失败"),
            "# History\nthird version\n"
        );
    }

    #[test]
    fn restore_wiki_page_from_history_replaces_content() {
        let vault_dir = make_temp_dir("llm-wiki-restore-history");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("restore-test.md");
        let content_v1 = "# Restore Test\n版本一\n";
        let content_v2 = "# Restore Test\n版本二\n";
        let content_v3 = "# Restore Test\n版本三\n";

        fs::write(&page_path, content_v1).expect("写入初始页面失败");

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");

        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                content_v2,
                None,
            ))
            .expect("第一次保存失败");
        runtime
            .block_on(state.save_wiki_page_impl(
                page_path.to_str().expect("页面路径不是 UTF-8"),
                content_v3,
                None,
            ))
            .expect("第二次保存失败");

        let history = state
            .list_wiki_page_history_impl(page_path.to_str().expect("页面路径不是 UTF-8"), Some(10))
            .expect("读取历史列表失败");
        assert_eq!(history.len(), 2, "应有 2 条历史记录");

        let oldest = state
            .get_wiki_page_history_entry_impl(history[1].id)
            .expect("读取最旧历史详情失败");
        assert_eq!(oldest.content, content_v1);

        let result = runtime
            .block_on(state.restore_wiki_page_from_history_impl(history[1].id))
            .expect("恢复历史版本失败");
        assert_eq!(result.path, page_path.to_str().expect("路径不是 UTF-8"));

        let restored = fs::read_to_string(&page_path).expect("读取恢复后页面失败");
        assert_eq!(restored, content_v1, "恢复后内容应与 v1 一致");

        let history_after = state
            .list_wiki_page_history_impl(page_path.to_str().expect("页面路径不是 UTF-8"), Some(10))
            .expect("读取恢复后历史列表失败");
        assert_eq!(history_after.len(), 3, "恢复操作应产生新的历史快照");
    }

    #[test]
    fn save_wiki_page_checksum_mismatch_rejected() {
        let vault_dir = make_temp_dir("llm-wiki-checksum-mismatch");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("checksum.md");
        let original = "# Checksum Test\n原始内容\n";
        fs::write(&page_path, original).expect("写入初始页面失败");

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let wrong_hash = "deadbeef".to_string();
        let result = runtime.block_on(state.save_wiki_page_impl(
            page_path.to_str().expect("页面路径不是 UTF-8"),
            "# Checksum Test\n新内容\n",
            Some(&wrong_hash),
        ));
        assert!(result.is_err(), "checksum 不匹配应返回 Err");
        assert!(
            result.unwrap_err().contains("checksum_mismatch"),
            "错误消息应包含 checksum_mismatch"
        );

        let current = fs::read_to_string(&page_path).expect("读取页面失败");
        assert_eq!(current, original, "checksum 校验失败时文件不应被修改");
    }

    #[test]
    fn save_wiki_page_checksum_match_accepted() {
        let vault_dir = make_temp_dir("llm-wiki-checksum-match");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("checksum-ok.md");
        let original = "# Checksum Test\n原始内容\n";
        fs::write(&page_path, original).expect("写入初始页面失败");

        let correct_hash = format!("{:x}", md5_simple(original));

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let result = runtime.block_on(state.save_wiki_page_impl(
            page_path.to_str().expect("页面路径不是 UTF-8"),
            "# Checksum Test\n新内容\n",
            Some(&correct_hash),
        ));
        assert!(result.is_ok(), "正确 checksum 应保存成功");

        let current = fs::read_to_string(&page_path).expect("读取页面失败");
        assert_eq!(current, "# Checksum Test\n新内容\n");
    }

    // ── quick_lint_page 测试 ──────────────────────────────────────────────────

    #[cfg(test)]
    mod quick_lint_tests {
        use super::*;

        fn make_vault_with_page(content: &str) -> (PathBuf, String) {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir()
                .join(format!("llm-wiki-ql-{}-{}", std::process::id(), unique));
            let wiki_dir = dir.join("wiki");
            fs::create_dir_all(&wiki_dir).unwrap();
            let page_path = wiki_dir.join("test-page.md");
            fs::write(&page_path, content).unwrap();
            let wiki_path = "wiki/test-page.md".to_string();
            (dir, wiki_path)
        }

        fn make_state_for_vault(vault_dir: &Path) -> crate::state::AppState {
            let config_path = vault_dir.join(".app").join("config.json");
            let state = crate::state::AppState::new_with_path(config_path);
            {
                let mut inner = state.inner.lock().unwrap();
                inner.vault_path = Some(vault_dir.to_path_buf());
            }
            state
        }

        #[test]
        fn quick_lint_page_no_issues_when_links_exist() {
            let (dir, wiki_path) = make_vault_with_page(
                "---\ntitle: Test\nentities:\n  - Existing\n---\n\nSee [[Existing]]",
            );
            let _guard = TempDirGuard(dir.clone());
            fs::write(dir.join("wiki").join("existing.md"), "").unwrap();

            let state = make_state_for_vault(&dir);
            let result = state.quick_lint_page_impl(&wiki_path).unwrap();
            assert_eq!(result.issues_count, 0);
            assert!(result.broken_links.is_empty());
            assert!(result.missing_entity_pages.is_empty());
        }

        #[test]
        fn quick_lint_page_detects_broken_link() {
            let (dir, wiki_path) =
                make_vault_with_page("---\ntitle: Test\n---\n\nSee [[NonExistentPage]]");
            let _guard = TempDirGuard(dir.clone());

            let state = make_state_for_vault(&dir);
            let result = state.quick_lint_page_impl(&wiki_path).unwrap();
            assert!(result.issues_count > 0);
            assert!(!result.broken_links.is_empty());
        }

        #[test]
        fn quick_lint_page_detects_missing_entity_page() {
            let (dir, wiki_path) = make_vault_with_page(
                "---\ntitle: Test\nentities:\n  - MissingEntity\n---\n\nContent here.",
            );
            let _guard = TempDirGuard(dir.clone());

            let state = make_state_for_vault(&dir);
            let result = state.quick_lint_page_impl(&wiki_path).unwrap();
            assert!(!result.missing_entity_pages.is_empty());
        }

        #[test]
        fn quick_lint_page_returns_empty_when_file_missing() {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let dir = std::env::temp_dir().join(format!(
                "llm-wiki-ql-nofile-{}-{}",
                std::process::id(),
                unique
            ));
            fs::create_dir_all(dir.join("wiki")).unwrap();
            let _guard = TempDirGuard(dir.clone());

            let state = make_state_for_vault(&dir);
            let result = state.quick_lint_page_impl("wiki/does-not-exist.md").unwrap();
            assert_eq!(result.issues_count, 0);
        }
    }
}
