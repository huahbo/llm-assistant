use super::AppState;
use crate::{
    llm::LlmError,
    models::{SearchConfig, WebSearchResult},
};
use std::{fs, path::PathBuf};

// ─── 搜索 helper 自由函数 ─────────────────────────────────────────────────────

pub(super) fn normalize_searxng_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    }
}

pub(super) fn searxng_base_root(base_url: &str) -> String {
    normalize_searxng_base_url(base_url)
        .trim_end_matches("/search")
        .to_string()
}

#[derive(Clone, Copy)]
pub(super) struct SearxngSearchParams {
    pub(super) language: &'static str,
    pub(super) categories: &'static str,
    pub(super) safesearch: &'static str,
}

pub(super) fn detect_query_pref_language(query: &str) -> &'static str {
    if query.chars().any(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
            || ('\u{3040}'..='\u{30FF}').contains(&c)
            || ('\u{AC00}'..='\u{D7AF}').contains(&c)
    }) {
        "zh-CN"
    } else {
        "auto"
    }
}

pub(super) fn build_searxng_search_params(query: &str) -> Vec<SearxngSearchParams> {
    let preferred_lang = detect_query_pref_language(query);
    let mut params = vec![SearxngSearchParams {
        language: preferred_lang,
        categories: "general",
        safesearch: "0",
    }];
    if preferred_lang != "auto" {
        params.push(SearxngSearchParams {
            language: "auto",
            categories: "general",
            safesearch: "0",
        });
    }
    params.push(SearxngSearchParams {
        language: "all",
        categories: "general,news",
        safesearch: "0",
    });
    params
}

pub(super) fn parse_unresponsive_engines(data: &serde_json::Value) -> Vec<String> {
    let Some(items) = data["unresponsive_engines"].as_array() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if let Some(text) = item.as_str() {
            let text = text.trim();
            if !text.is_empty() {
                out.push(text.to_string());
            }
            continue;
        }
        if let Some(obj) = item.as_object() {
            let name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| obj.get("engine").and_then(|v| v.as_str()))
                .unwrap_or("")
                .trim();
            let reason = obj
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if !name.is_empty() && !reason.is_empty() {
                out.push(format!("{} ({})", name, reason));
                continue;
            }
            if !name.is_empty() {
                out.push(name.to_string());
                continue;
            }
        }
        let fallback = item.to_string();
        if !fallback.is_empty() && fallback != "null" {
            out.push(fallback);
        }
    }
    out
}

pub(super) fn url_hostname(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

fn normalize_search_provider(provider: &str) -> &str {
    match provider.trim().to_ascii_lowercase().as_str() {
        "tavily" => "tavily",
        "searxng" => "searxng",
        "none" => "none",
        _ => provider.trim(),
    }
}

pub(super) fn validate_search_config(config: &SearchConfig) -> Result<(), String> {
    match normalize_search_provider(&config.search_provider) {
        "tavily" if config.tavily_api_key.trim().is_empty() => {
            Err("搜索配置错误：已选择 Tavily，但未填写 API Key".to_string())
        }
        "searxng" if config.searxng_url.trim().is_empty() => {
            Err("搜索配置错误：已选择 SearXNG，但未填写地址".to_string())
        }
        "none" => Err("搜索配置错误：未启用搜索 Provider".to_string()),
        "tavily" | "searxng" => Ok(()),
        other => Err(format!("搜索配置错误：不支持的搜索 Provider `{}`", other)),
    }
}

// research_service 也会使用这些 error helper
pub(super) fn compact_error_message(err: &str, max_chars: usize) -> String {
    let compact = err.replace('\n', " ").replace('\r', " ");
    let mut short: String = compact.chars().take(max_chars).collect();
    if compact.chars().count() > max_chars {
        short.push_str("...");
    }
    short
}

pub(super) fn compact_llm_error(err: &LlmError, max_chars: usize) -> String {
    compact_error_message(&err.to_string(), max_chars)
}

pub(super) fn summarize_round_errors(errors: &[String], max_items: usize) -> String {
    if errors.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = errors
        .iter()
        .take(max_items)
        .map(|err| compact_error_message(err, 140))
        .collect();
    if errors.len() > max_items {
        parts.push(format!("其余 {} 条错误已省略", errors.len() - max_items));
    }
    parts.join(" | ")
}

// ─── 搜索 provider 实现 ───────────────────────────────────────────────────────

async fn search_tavily(
    client: &reqwest::Client,
    query: &str,
    api_key: &str,
    max_results: usize,
) -> Result<Vec<WebSearchResult>, String> {
    let body = serde_json::json!({
        "api_key": api_key,
        "query": query,
        "max_results": max_results,
        "search_depth": "advanced",
        "include_raw_content": true
    });
    let resp = client
        .post("https://api.tavily.com/search")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Tavily 请求失败: {}", e))?;
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Tavily 响应解析失败: {}", e))?;
    let results = data["results"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for item in results.iter().take(max_results) {
        let title = item["title"].as_str().unwrap_or("").to_string();
        let url = item["url"].as_str().unwrap_or("").to_string();
        let snippet = item["raw_content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| item["content"].as_str())
            .unwrap_or("")
            .chars()
            .take(300)
            .collect();
        let source = url_hostname(&url);
        out.push(WebSearchResult { title, url, snippet, source });
    }
    Ok(out)
}

async fn search_searxng_endpoint_with_params(
    client: &reqwest::Client,
    endpoint: &str,
    query: &str,
    max_results: usize,
    params: SearxngSearchParams,
) -> Result<(Vec<WebSearchResult>, Vec<String>), String> {
    let target_count = max_results.max(1).min(10).to_string();
    let req_params = vec![
        ("q", query.to_string()),
        ("format", "json".to_string()),
        ("language", params.language.to_string()),
        ("categories", params.categories.to_string()),
        ("safesearch", params.safesearch.to_string()),
        ("pageno", "1".to_string()),
        ("count", target_count),
    ];
    let resp = client
        .get(endpoint)
        .query(&req_params)
        .header("Accept", "application/json")
        .header("User-Agent", "Mozilla/5.0 llm-wiki-searxng-client")
        .header("X-Forwarded-For", "127.0.0.1")
        .header("X-Real-IP", "127.0.0.1")
        .header("X-Forwarded-Proto", "http")
        .send()
        .await
        .map_err(|e| format!("SearXNG 请求失败: {}", e))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let preview: String = body.chars().take(120).collect();
        return Err(format!("SearXNG HTTP {}: {}", status.as_u16(), preview));
    }
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("SearXNG 响应解析失败: {}", e))?;
    let unresponsive_engines = parse_unresponsive_engines(&data);
    let results = data["results"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for item in results.iter().take(max_results.max(1)) {
        let title = item["title"].as_str().unwrap_or("").to_string();
        let url_str = item["url"].as_str().unwrap_or("").to_string();
        let snippet = item["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| item["title"].as_str())
            .unwrap_or("")
            .chars()
            .take(300)
            .collect();
        let source = url_hostname(&url_str);
        out.push(WebSearchResult { title, url: url_str, snippet, source });
    }
    Ok((out, unresponsive_engines))
}

async fn search_searxng(
    client: &reqwest::Client,
    query: &str,
    base_url: &str,
    max_results: usize,
) -> Result<Vec<WebSearchResult>, String> {
    let base_root = searxng_base_root(base_url);
    let target_count = max_results.max(1);
    let endpoint_candidates = [format!("{}/search", base_root), base_root];
    let query_params = build_searxng_search_params(query);

    let mut merged_results: Vec<WebSearchResult> = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();
    let mut unresponsive_hints: Vec<String> = Vec::new();
    let mut seen_hints = std::collections::HashSet::new();
    let mut errors = Vec::new();

    for endpoint in endpoint_candidates {
        for params in &query_params {
            match search_searxng_endpoint_with_params(client, &endpoint, query, target_count, *params).await {
                Ok((results, hints)) => {
                    for hint in hints {
                        if seen_hints.insert(hint.clone()) {
                            unresponsive_hints.push(hint);
                        }
                    }
                    for result in results {
                        if seen_urls.insert(result.url.clone()) {
                            merged_results.push(result);
                        }
                    }
                    if merged_results.len() >= target_count {
                        merged_results.truncate(target_count);
                        return Ok(merged_results);
                    }
                }
                Err(err) => {
                    errors.push(format!(
                        "{} (language={}, categories={}): {}",
                        endpoint, params.language, params.categories, err
                    ));
                }
            }
        }
        if !merged_results.is_empty() {
            break;
        }
    }

    if !merged_results.is_empty() {
        merged_results.truncate(target_count);
        return Ok(merged_results);
    }

    if !errors.is_empty() {
        let summary = summarize_round_errors(&errors, 3);
        if !unresponsive_hints.is_empty() {
            return Err(format!(
                "SearXNG 搜索无结果。错误摘要：{}。不可用引擎：{}",
                summary,
                unresponsive_hints.join("; ")
            ));
        }
        return Err(format!("SearXNG 搜索无结果。错误摘要：{}", summary));
    }

    if !unresponsive_hints.is_empty() {
        return Err(format!(
            "SearXNG 返回 0 条结果。不可用引擎：{}",
            unresponsive_hints.join("; ")
        ));
    }

    Err("SearXNG 返回 0 条结果，请检查 engines 配置或网络连通性".to_string())
}

async fn search_brave(
    client: &reqwest::Client,
    query: &str,
    api_key: &str,
    max_results: usize,
) -> Result<Vec<WebSearchResult>, String> {
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        query.replace(' ', "+"),
        max_results.min(10)
    );
    let resp = client
        .get(&url)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip")
        .header("X-Subscription-Token", api_key)
        .send()
        .await
        .map_err(|e| format!("Brave 请求失败: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Brave API 错误 {status}: {body}"));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| format!("Brave 响应解析失败: {e}"))?;
    let mut out = Vec::new();
    if let Some(results) = json.get("web").and_then(|w| w.get("results")).and_then(|r| r.as_array()) {
        for item in results.iter().take(max_results) {
            let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let snippet = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if url.is_empty() { continue; }
            let source = url_hostname(&url);
            out.push(WebSearchResult { title, url, snippet, source });
        }
    }
    Ok(out)
}

async fn search_powershell(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
) -> Result<Vec<WebSearchResult>, String> {
    let url = format!("https://lite.duckduckgo.com/lite/?q={}", query.replace(' ', "+"));
    let resp = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await
        .map_err(|e| format!("DuckDuckGo 请求失败: {e}"))?;
    let html = resp.text().await.map_err(|e| format!("DuckDuckGo 响应读取失败: {e}"))?;
    use scraper::{Html, Selector};
    let document = Html::parse_document(&html);
    let row_sel = Selector::parse("tr").unwrap();
    let link_sel = Selector::parse("a.result-link").unwrap();
    let snippet_sel = Selector::parse("td.result-snippet").unwrap();
    let mut out = Vec::new();
    let rows: Vec<_> = document.select(&row_sel).collect();
    let mut i = 0;
    while i < rows.len() && out.len() < max_results {
        if let Some(a) = rows[i].select(&link_sel).next() {
            let title = a.text().collect::<String>().trim().to_string();
            let href = a.value().attr("href").unwrap_or("").to_string();
            let snippet = if i + 1 < rows.len() {
                rows[i + 1].select(&snippet_sel).next()
                    .map(|s| s.text().collect::<String>().trim().to_string())
                    .unwrap_or_default()
            } else {
                String::new()
            };
            if !href.is_empty() && !title.is_empty() {
                let source = url_hostname(&href);
                out.push(WebSearchResult { title, url: href, snippet, source });
            }
        }
        i += 1;
    }
    Ok(out)
}

pub(super) async fn do_search(
    client: &reqwest::Client,
    query: &str,
    config: &SearchConfig,
    max_results: usize,
) -> Result<Vec<WebSearchResult>, String> {
    match normalize_search_provider(&config.search_provider) {
        "tavily" if !config.tavily_api_key.is_empty() => {
            search_tavily(client, query, &config.tavily_api_key, max_results).await
        }
        "tavily" => Err("搜索配置错误：已选择 Tavily，但未填写 API Key".to_string()),
        "searxng" if !config.searxng_url.trim().is_empty() => {
            search_searxng(client, query, &config.searxng_url, max_results).await
        }
        "searxng" => Err("搜索配置错误：已选择 SearXNG，但未填写地址".to_string()),
        "none" => Err("搜索配置错误：未启用搜索 Provider".to_string()),
        other => Err(format!("搜索配置错误：不支持的搜索 Provider `{}`", other)),
    }
}

// ─── 公共方法实现 ─────────────────────────────────────────────────────────────

pub fn load_search_config_from_path(config_path: &std::path::Path) -> SearchConfig {
    let search_config_path = config_path
        .parent()
        .map(|p| p.join("search-config.json"))
        .unwrap_or_else(|| PathBuf::from("search-config.json"));
    if let Ok(content) = fs::read_to_string(&search_config_path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        SearchConfig::default()
    }
}

pub fn get_search_config(state: &AppState) -> SearchConfig {
    state
        .search_config
        .lock()
        .expect("搜索配置锁已被污染")
        .clone()
}

pub fn set_search_config(state: &AppState, cfg: SearchConfig) -> Result<(), String> {
    let search_config_path = state
        .config_path
        .parent()
        .map(|p| p.join("search-config.json"))
        .unwrap_or_else(|| PathBuf::from("search-config.json"));
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| format!("序列化搜索配置失败: {}", e))?;
    fs::write(&search_config_path, json).map_err(|e| format!("写入搜索配置文件失败: {}", e))?;
    *state.search_config.lock().expect("搜索配置锁已被污染") = cfg;
    Ok(())
}

pub async fn search_web_cascade(
    state: &AppState,
    query: &str,
    max_results: usize,
) -> Result<Vec<WebSearchResult>, String> {
    Ok(search_web_cascade_with_source(state, query, max_results).await?.0)
}

pub async fn search_web_cascade_with_source(
    state: &AppState,
    query: &str,
    max_results: usize,
) -> Result<(Vec<WebSearchResult>, &'static str), String> {
    let config = get_search_config(state);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("llm-wiki/1.0")
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {e}"))?;

    if !config.searxng_url.trim().is_empty() {
        match search_searxng(&client, query, &config.searxng_url, max_results).await {
            Ok(results) if !results.is_empty() => return Ok((results, "SearXNG")),
            _ => {}
        }
    }

    if !config.tavily_api_key.trim().is_empty() {
        match search_tavily(&client, query, &config.tavily_api_key, max_results).await {
            Ok(results) if !results.is_empty() => return Ok((results, "Tavily")),
            _ => {}
        }
    }

    if !config.brave_api_key.trim().is_empty() {
        match search_brave(&client, query, &config.brave_api_key, max_results).await {
            Ok(results) if !results.is_empty() => return Ok((results, "Brave")),
            _ => {}
        }
    }

    match search_powershell(&client, query, max_results).await {
        Ok(results) if !results.is_empty() => return Ok((results, "DuckDuckGo")),
        _ => {}
    }

    Err("搜索服务不可用：所有 provider 均无响应或无结果".to_string())
}

pub fn register_query_approval(
    state: &AppState,
    task_id: i64,
) -> tokio::sync::oneshot::Receiver<Vec<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    state
        .pending_query_approvals
        .lock()
        .expect("pending_query_approvals lock")
        .insert(task_id, tx);
    rx
}

pub fn approve_research_queries(state: &AppState, task_id: i64, queries: Vec<String>) -> bool {
    if let Some(tx) = state
        .pending_query_approvals
        .lock()
        .expect("pending_query_approvals lock")
        .remove(&task_id)
    {
        tx.send(queries).is_ok()
    } else {
        false
    }
}
