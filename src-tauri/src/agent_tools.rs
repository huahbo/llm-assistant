use serde_json::Value;

/// Agent 循环单步决策。
#[derive(Debug)]
pub struct AgentLoopDecision {
    pub done: bool,
    pub final_answer: Option<String>,
    pub note: Option<String>,
    pub action: Option<AgentToolAction>,
}

/// Agent 最小工具动作集合（H6-S2）。
#[derive(Debug)]
pub enum AgentToolAction {
    RunShell { command: String, timeout_ms: u64 },
    SearchWiki { query: String, limit: usize },
    ReadWiki { path: String, max_chars: usize },
    WriteWiki { path: String, content: String },
}

/// 解析单轮 agent 决策（best-effort）。
pub fn parse_agent_loop_decision(raw: &str) -> Option<AgentLoopDecision> {
    let json = parse_json_object(raw)?;
    let done = json.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
    let final_answer = json
        .get("final")
        .or_else(|| json.get("answer"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());
    let note = json
        .get("note")
        .or_else(|| json.get("notes"))
        .and_then(|v| v.as_str())
        .map(|v| v.to_string());

    let mut action = json.get("action").and_then(parse_agent_tool_action);
    if action.is_none() {
        action = json
            .get("actions")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(parse_agent_tool_action);
    }

    Some(AgentLoopDecision {
        done,
        final_answer,
        note,
        action,
    })
}

/// 从 LLM 输出中提取 JSON 对象（best-effort）。
fn parse_json_object(raw: &str) -> Option<Value> {
    let trimmed = raw.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end <= start {
        return None;
    }
    serde_json::from_str::<Value>(&trimmed[start..=end]).ok()
}

/// 解析单个工具动作（best-effort）。
fn parse_agent_tool_action(item: &Value) -> Option<AgentToolAction> {
    let tool = item.get("tool").and_then(|v| v.as_str())?.trim();
    match tool {
        "run_shell" => {
            let command = item
                .get("command")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            let timeout_ms = item
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(30_000)
                .clamp(3_000, 120_000);
            Some(AgentToolAction::RunShell {
                command,
                timeout_ms,
            })
        }
        "search_wiki" => {
            let query = item
                .get("query")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            let limit = item
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(5)
                .clamp(1, 8);
            Some(AgentToolAction::SearchWiki { query, limit })
        }
        "read_wiki" => {
            let path = item
                .get("path")
                .or_else(|| item.get("page_path"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            let max_chars = item
                .get("max_chars")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(1200)
                .clamp(200, 5000);
            Some(AgentToolAction::ReadWiki { path, max_chars })
        }
        "write_wiki" => {
            let path = item
                .get("path")
                .or_else(|| item.get("page_path"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            let content = item
                .get("content")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())?
                .to_string();
            Some(AgentToolAction::WriteWiki { path, content })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_agent_loop_decision, AgentToolAction};

    #[test]
    fn parse_agent_loop_decision_supports_single_action_shape() {
        let raw = r#"{"done":false,"note":"先检索","action":{"tool":"search_wiki","query":"actor","limit":3}}"#;
        let decision = parse_agent_loop_decision(raw).expect("应能解析决策");
        assert!(!decision.done);
        assert_eq!(decision.note.as_deref(), Some("先检索"));
        match decision.action {
            Some(AgentToolAction::SearchWiki { query, limit }) => {
                assert_eq!(query, "actor");
                assert_eq!(limit, 3);
            }
            _ => panic!("应解析为 search_wiki"),
        }
    }

    #[test]
    fn parse_agent_loop_decision_supports_actions_array_fallback() {
        let raw =
            r#"{"done":false,"actions":[{"tool":"read_wiki","path":"wiki/a.md","max_chars":900}]}"#;
        let decision = parse_agent_loop_decision(raw).expect("应能解析决策");
        match decision.action {
            Some(AgentToolAction::ReadWiki { path, max_chars }) => {
                assert_eq!(path, "wiki/a.md");
                assert_eq!(max_chars, 900);
            }
            _ => panic!("应解析为 read_wiki"),
        }
    }

    #[test]
    fn parse_agent_loop_decision_reads_done_and_final_answer() {
        let raw = r#"{"done":true,"final":"已完成"}"#;
        let decision = parse_agent_loop_decision(raw).expect("应能解析决策");
        assert!(decision.done);
        assert_eq!(decision.final_answer.as_deref(), Some("已完成"));
        assert!(decision.action.is_none());
    }

    #[test]
    fn parse_agent_loop_decision_supports_write_wiki() {
        let raw =
            r##"{"done":false,"action":{"tool":"write_wiki","path":"wiki/a.md","content":"# A"}}"##;
        let decision = parse_agent_loop_decision(raw).expect("应能解析决策");
        match decision.action {
            Some(AgentToolAction::WriteWiki { path, content }) => {
                assert_eq!(path, "wiki/a.md");
                assert_eq!(content, "# A");
            }
            _ => panic!("应解析为 write_wiki"),
        }
    }
}
