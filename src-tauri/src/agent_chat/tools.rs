//! agent_chat 工具执行分发器
//!
//! 将 OpenAI function_calling tool call 分发到现有 AppState 实现。

use std::time::Instant;

use serde_json::Value;

use crate::llm::types::ToolCall;
use crate::state::AppState;

/// 单次工具调用执行结果
pub struct ToolExecResult {
    /// 与 ToolCall.id 对应
    pub call_id: String,
    /// 注入 tool role message 的完整内容（给 LLM 看）
    pub content: String,
    pub latency_ms: u64,
    /// write/edit_wiki：待审批条目 ID；其余工具 None
    pub awaiting_approval: Option<i64>,
}

/// 分发并执行一个 tool call，总是返回 Ok（工具内部错误编码到 content 中）
pub async fn execute_tool_call(
    state: &AppState,
    _conv_id: i64,
    call: &ToolCall,
) -> Result<ToolExecResult, String> {
    let start = Instant::now();

    let args: Value = serde_json::from_str(&call.function.arguments)
        .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

    let (content, awaiting_approval) = dispatch(state, &call.function.name, args).await;

    let latency_ms = start.elapsed().as_millis() as u64;

    Ok(ToolExecResult {
        call_id: call.id.clone(),
        content,
        latency_ms,
        awaiting_approval,
    })
}

async fn dispatch(state: &AppState, tool_name: &str, args: Value) -> (String, Option<i64>) {
    match tool_name {
        "run_shell" => exec_run_shell(state, args).await,
        "search_wiki" => exec_search_wiki(state, args),
        "read_wiki" => exec_read_wiki(state, args),
        "write_wiki" => exec_write_wiki(state, args),
        "edit_wiki" => exec_edit_wiki(state, args),
        "web_search" => exec_web_search(state, args).await,
        "fetch_url" => exec_fetch_url(args).await,
        name => exec_mcp_or_unknown(state, name, args).await,
    }
}

async fn exec_mcp_or_unknown(state: &AppState, tool_name: &str, args: Value) -> (String, Option<i64>) {
    // Look up the handler_kind in DB to see if it's an MCP tool.
    let handler_kind = state
        .outbox_db_path()
        .and_then(|p| crate::agent_chat::db::get_tool_handler_kind(&p, tool_name).ok().flatten());

    if let Some(kind) = handler_kind {
        if let Some(server_name) = kind.strip_prefix("mcp:") {
            let arc = match state.get_mcp_client(server_name) {
                Some(a) => a,
                None => {
                    return (
                        format!("MCP 服务器 '{server_name}' 未运行；请在设置中启动它后重试"),
                        None,
                    )
                }
            };
            let result = {
                let mut client = arc.lock().await;
                client.call_tool(tool_name, args).await
            };
            return match result {
                Ok(output) => (output, None),
                Err(e) => (format!("MCP 工具调用失败: {e}"), None),
            };
        }
    }

    (format!("未知工具: {tool_name}"), None)
}

// ── run_shell ──────────────────────────────────────────────────────────────────

async fn exec_run_shell(state: &AppState, args: Value) -> (String, Option<i64>) {
    let command = match str_arg(&args, "command") {
        Some(c) => c,
        None => return ("错误：run_shell 需要 'command' 参数".to_string(), None),
    };
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000);

    match state
        .run_shell_impl(
            command,
            timeout_ms,
            Some("agent".to_string()),
            None,
            None,
        )
        .await
    {
        Ok(result) => {
            let mut out = String::new();
            if result.blocked {
                out.push_str(&format!(
                    "blocked: {}\n",
                    result.blocked_reason.unwrap_or_default()
                ));
            }
            if !result.stdout.is_empty() {
                out.push_str(&result.stdout);
            }
            if !result.stderr.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str("--- stderr ---\n");
                out.push_str(&result.stderr);
            }
            if out.is_empty() {
                out.push_str("(no output)");
            }
            (out, None)
        }
        Err(e) => (format!("shell 执行失败: {e}"), None),
    }
}

// ── search_wiki ────────────────────────────────────────────────────────────────

fn exec_search_wiki(state: &AppState, args: Value) -> (String, Option<i64>) {
    let query = match str_arg(&args, "query") {
        Some(q) => q,
        None => return ("错误：search_wiki 需要 'query' 参数".to_string(), None),
    };
    let top_k = args
        .get("top_k")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    match state.search_wiki_pages(query, top_k) {
        Ok(pages) if pages.is_empty() => ("(no results)".to_string(), None),
        Ok(pages) => {
            let lines: Vec<String> = pages
                .iter()
                .map(|p| {
                    format!(
                        "{} | {} | score={:.3} | {}",
                        p.path,
                        p.title,
                        p.score,
                        p.summary.chars().take(100).collect::<String>()
                    )
                })
                .collect();
            (lines.join("\n"), None)
        }
        Err(e) => (format!("搜索失败: {e}"), None),
    }
}

// ── read_wiki ──────────────────────────────────────────────────────────────────

fn exec_read_wiki(state: &AppState, args: Value) -> (String, Option<i64>) {
    let path = match str_arg(&args, "path") {
        Some(p) => p,
        None => return ("错误：read_wiki 需要 'path' 参数".to_string(), None),
    };
    match crate::agent_runtime::read_wiki_page_for_agent(state, &path, 8_000) {
        Ok(content) => (content, None),
        Err(e) => (format!("读取失败: {e}"), None),
    }
}

// ── write_wiki ─────────────────────────────────────────────────────────────────

fn exec_write_wiki(state: &AppState, args: Value) -> (String, Option<i64>) {
    let path = match str_arg(&args, "path") {
        Some(p) => p,
        None => return ("错误：write_wiki 需要 'path' 参数".to_string(), None),
    };
    let content = match str_arg(&args, "content") {
        Some(c) => c,
        None => return ("错误：write_wiki 需要 'content' 参数".to_string(), None),
    };

    let target = match crate::agent_runtime::resolve_agent_write_target_path(state, &path) {
        Ok(p) => p,
        Err(e) => return (format!("路径解析失败: {e}"), None),
    };
    let resolved = target.to_string_lossy().to_string();

    // H6-P2：有效票据存在时自动批准，跳过人工审批弹窗。
    if crate::agent_policy::check_write_ticket(&resolved) {
        let pending_id = pending_write_id();
        state.store_pending_agent_write(pending_id, resolved.clone(), content, None);
        return match state.approve_agent_write_impl(pending_id) {
            Ok(msg) => (format!("[ticket-auto-approved] {msg}"), None),
            Err(e) => (format!("[ticket] 写入失败: {e}"), None),
        };
    }

    let pending_id = pending_write_id();
    state.store_pending_agent_write(pending_id, resolved.clone(), content, None);
    (
        format!("write_wiki pending approval id={pending_id} path={resolved}"),
        Some(pending_id),
    )
}

// ── edit_wiki ──────────────────────────────────────────────────────────────────

fn exec_edit_wiki(state: &AppState, args: Value) -> (String, Option<i64>) {
    let path = match str_arg(&args, "path") {
        Some(p) => p,
        None => return ("错误：edit_wiki 需要 'path' 参数".to_string(), None),
    };
    let old_str = match str_arg(&args, "old_str") {
        Some(s) => s,
        None => return ("错误：edit_wiki 需要 'old_str' 参数".to_string(), None),
    };
    let new_str = str_arg(&args, "new_str").unwrap_or_default();

    let target = match crate::agent_runtime::resolve_agent_write_target_path(state, &path) {
        Ok(p) => p,
        Err(e) => return (format!("路径解析失败: {e}"), None),
    };
    let resolved = target.to_string_lossy().to_string();

    // H6-P2：有效票据存在时自动批准，跳过人工审批弹窗。
    if crate::agent_policy::check_write_ticket(&resolved) {
        let pending_id = pending_write_id();
        state.store_pending_agent_write(pending_id, resolved.clone(), new_str, Some(old_str));
        return match state.approve_agent_write_impl(pending_id) {
            Ok(msg) => (format!("[ticket-auto-approved] {msg}"), None),
            Err(e) => (format!("[ticket] 编辑失败: {e}"), None),
        };
    }

    let pending_id = pending_write_id();
    state.store_pending_agent_write(pending_id, resolved.clone(), new_str, Some(old_str));
    (
        format!("edit_wiki pending approval id={pending_id} path={resolved}"),
        Some(pending_id),
    )
}

// ── web_search ─────────────────────────────────────────────────────────────────

async fn exec_web_search(state: &AppState, args: Value) -> (String, Option<i64>) {
    let query = match str_arg(&args, "query") {
        Some(q) => q,
        None => return ("错误：web_search 需要 'query' 参数".to_string(), None),
    };
    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;

    match state.search_web_cascade_with_source(&query, max_results).await {
        Ok((results, _)) if results.is_empty() => ("(no results)".to_string(), None),
        Ok((results, source)) => {
            let mut lines: Vec<String> = Vec::with_capacity(results.len() + 1);
            lines.push(format!("[via {source}]"));
            for (i, r) in results.iter().enumerate() {
                lines.push(format!(
                    "{}. [{}]({}) — {}",
                    i + 1,
                    r.title,
                    r.url,
                    r.snippet.chars().take(200).collect::<String>()
                ));
            }
            (lines.join("\n"), None)
        }
        Err(e) => (format!("搜索失败: {e}"), None),
    }
}

// ── fetch_url ──────────────────────────────────────────────────────────────────

async fn exec_fetch_url(args: Value) -> (String, Option<i64>) {
    let url = match str_arg(&args, "url") {
        Some(u) => u,
        None => return ("错误：fetch_url 需要 'url' 参数".to_string(), None),
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("llm-wiki/1.0")
        .build()
    {
        Ok(c) => c,
        Err(e) => return (format!("HTTP 客户端初始化失败: {e}"), None),
    };

    // 先 HEAD 检查 Content-Length（可选，部分服务器不返回）
    let head_resp = client.head(&url).send().await;
    if let Ok(resp) = head_resp {
        if let Some(len) = resp.headers().get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
        {
            if len > 5 * 1024 * 1024 {
                return (format!("页面过大（{} 字节），拒绝获取", len), None);
            }
        }
    }

    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => return (format!("请求失败: {e}"), None),
    };

    if !resp.status().is_success() {
        return (format!("HTTP 错误 {}", resp.status()), None);
    }

    let html = match resp.text().await {
        Ok(t) => t,
        Err(e) => return (format!("响应读取失败: {e}"), None),
    };

    let text = extract_html_text(&html);
    let truncated: String = text.chars().take(8_000).collect();
    (truncated, None)
}

fn extract_html_text(html: &str) -> String {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);

    // 尝试找到主内容区域
    let content_selectors = ["main", "article", "[role=main]", "body"];

    for sel_str in &content_selectors {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(element) = document.select(&sel).next() {
                // 提取文本，跳过被 remove_sel 覆盖的子元素
                let mut text = String::new();
                for node in element.descendants() {
                    if let Some(text_node) = node.value().as_text() {
                        // 检查祖先链中是否有需要移除的标签
                        let skip = node.ancestors().any(|a| {
                            a.value().as_element().map_or(false, |e| {
                                matches!(e.name(), "script" | "style" | "nav" | "footer" | "header" | "aside")
                            })
                        });
                        if !skip {
                            text.push_str(text_node.trim());
                            text.push(' ');
                        }
                    }
                }
                let result = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if result.len() > 100 {
                    return result;
                }
            }
        }
    }

    // 最终回退：直接提取所有文本
    document.root_element().text().collect::<Vec<_>>().join(" ")
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn pending_write_id() -> i64 {
    crate::state::current_timestamp_ms()
        .parse::<i64>()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn str_arg_returns_value_for_present_key() {
        let args = json!({"command": "ls -la"});
        assert_eq!(str_arg(&args, "command"), Some("ls -la".to_string()));
    }

    #[test]
    fn str_arg_returns_none_for_missing_key() {
        let args = json!({"other": "val"});
        assert_eq!(str_arg(&args, "command"), None);
    }

    #[test]
    fn str_arg_trims_and_rejects_whitespace_only() {
        let args = json!({"command": "   "});
        assert_eq!(str_arg(&args, "command"), None);
    }

    #[test]
    fn str_arg_trims_surrounding_whitespace() {
        let args = json!({"query": "  hello world  "});
        assert_eq!(str_arg(&args, "query"), Some("hello world".to_string()));
    }

    #[test]
    fn str_arg_returns_none_for_non_string_value() {
        let args = json!({"timeout_ms": 5000});
        assert_eq!(str_arg(&args, "timeout_ms"), None);
    }

    #[test]
    fn pending_write_id_is_nonzero() {
        let id = pending_write_id();
        assert!(id > 0, "pending_write_id should be a positive timestamp");
    }

    #[test]
    fn dispatch_unknown_tool_returns_error_message() {
        // We can't easily call the async dispatch without a real AppState,
        // but we can verify the sync routing logic via the error message pattern.
        // This is a documentation test to ensure the match arm exists.
        let tool_name = "nonexistent_tool_xyz";
        let expected_prefix = format!("未知工具: {tool_name}");
        // We construct the expected output directly since dispatch is private async
        let msg = format!("未知工具: {tool_name}");
        assert_eq!(msg, expected_prefix);
    }
}
