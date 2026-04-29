use crate::agent_tools::{parse_agent_loop_decision, AgentToolAction, ToolActionResult};
use async_trait::async_trait;

/// Agent 循环主控结果。
pub struct AgentLoopOutcome {
    pub tool_logs: Vec<String>,
    pub planner_notes: Vec<String>,
    pub final_hint: Option<String>,
    pub pending_write: Option<(String, String)>,               // (resolved_path, content)
    pub pending_edit: Option<(String, String, String)>,        // (resolved_path, old_str, new_str)
}

/// Agent 循环运行时接口（由 AppState 实现）。
#[async_trait]
pub trait AgentLoopRuntime {
    async fn complete_prompt(&self, prompt: String) -> Result<String, String>;
    async fn execute_tool_action(&self, idx: u32, action: AgentToolAction) -> ToolActionResult;
    fn append_event(&self, run_id: i64, level: &str, message: String);
}

/// 运行任务模式主循环：多轮决策 + 工具调用。
pub async fn run_agent_task_loop<R: AgentLoopRuntime + Sync>(
    runtime: &R,
    run_id: i64,
    instruction: &str,
    iteration_budget: u32,
    wiki_excerpt: &str,
) -> Result<AgentLoopOutcome, String> {
    let mut tool_logs: Vec<String> = Vec::new();
    let mut planner_notes: Vec<String> = Vec::new();
    let mut final_hint: Option<String> = None;
    let mut pending_write: Option<(String, String)> = None;
    let mut pending_edit: Option<(String, String, String)> = None;

    for idx in 0..iteration_budget {
        let history = if tool_logs.is_empty() {
            "（暂无工具历史）".to_string()
        } else {
            tool_logs.join("\n")
        };
        let loop_prompt = build_loop_prompt(instruction, iteration_budget, wiki_excerpt, &history);
        let planner_raw = runtime.complete_prompt(loop_prompt).await?;
        let Some(decision) = parse_agent_loop_decision(&planner_raw) else {
            runtime.append_event(
                run_id,
                "warn",
                format!("⚠️ 第 {} 轮规划输出无法解析，停止循环", idx + 1),
            );
            break;
        };
        if let Some(note) = decision.note {
            if !note.trim().is_empty() {
                planner_notes.push(note);
            }
        }
        if decision.done {
            final_hint = decision
                .final_answer
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty());
            runtime.append_event(
                run_id,
                "info",
                format!("✅ 任务在第 {} 轮声明完成", idx + 1),
            );
            break;
        }
        let Some(action) = decision.action else {
            runtime.append_event(
                run_id,
                "warn",
                format!("⚠️ 第 {} 轮未返回 action，停止循环", idx + 1),
            );
            break;
        };
        runtime.append_event(run_id, "info", build_tool_start_message(idx, &action));
        let tool_result = runtime.execute_tool_action(idx, action).await;
        tool_logs.push(tool_result.log_line.clone());
        runtime.append_event(
            run_id,
            tool_result.level.as_str(),
            format!("🔧 tool_end #{} {}", idx + 1, tool_result.log_line),
        );
        if tool_result.requires_approval {
            final_hint = Some("触发审批前置，等待人工确认".to_string());
            pending_write = tool_result.pending_write.clone();
            pending_edit = tool_result.pending_edit.clone();
            runtime.append_event(
                run_id,
                "awaiting_approval",
                format!("⏸️ 第 {} 轮触发审批前置，任务循环暂停等待人工确认", idx + 1),
            );
            break;
        }
    }

    Ok(AgentLoopOutcome {
        tool_logs,
        planner_notes,
        final_hint,
        pending_write,
        pending_edit,
    })
}

/// 对工具循环结果做最终总结，输出用户可读答案。
pub async fn summarize_agent_task<R: AgentLoopRuntime + Sync>(
    runtime: &R,
    instruction: &str,
    wiki_excerpt: &str,
    outcome: &AgentLoopOutcome,
) -> Result<String, String> {
    let tool_context = if outcome.tool_logs.is_empty() {
        "（无工具调用）".to_string()
    } else {
        outcome.tool_logs.join("\n")
    };
    let planner_note_text = if outcome.planner_notes.is_empty() {
        "（无）".to_string()
    } else {
        outcome.planner_notes.join("；")
    };
    let final_prompt = build_final_prompt(
        instruction,
        &tool_context,
        &planner_note_text,
        &outcome
            .final_hint
            .clone()
            .unwrap_or_else(|| "（无）".to_string()),
        wiki_excerpt,
    );
    let answer = runtime.complete_prompt(final_prompt).await?;
    let answer = answer.trim().to_string();
    if answer.is_empty() {
        return Err("任务模式返回空内容".to_string());
    }
    Ok(answer)
}

fn build_tool_start_message(idx: u32, action: &AgentToolAction) -> String {
    match action {
        AgentToolAction::RunShell { command, .. } => format!(
            "🔧 tool_start #{} run_shell: {}",
            idx + 1,
            command.chars().take(100).collect::<String>()
        ),
        AgentToolAction::SearchWiki { query, limit } => format!(
            "🔧 tool_start #{} search_wiki: {} (limit={})",
            idx + 1,
            query.chars().take(80).collect::<String>(),
            limit
        ),
        AgentToolAction::ReadWiki { path, max_chars } => format!(
            "🔧 tool_start #{} read_wiki: {} (max_chars={})",
            idx + 1,
            path.chars().take(80).collect::<String>(),
            max_chars
        ),
        AgentToolAction::WriteWiki { path, content } => format!(
            "🔧 tool_start #{} write_wiki: {} (chars={})",
            idx + 1,
            path.chars().take(80).collect::<String>(),
            content.chars().count()
        ),
        AgentToolAction::EditWiki { path, old_str, new_str } => format!(
            "🔧 tool_start #{} edit_wiki: {} (old={} chars, new={} chars)",
            idx + 1,
            path.chars().take(80).collect::<String>(),
            old_str.chars().count(),
            new_str.chars().count()
        ),
    }
}

/// 构造单轮决策 prompt。
pub fn build_loop_prompt(
    instruction: &str,
    iteration_budget: u32,
    wiki_excerpt: &str,
    history: &str,
) -> String {
    format!(
        "你是 LLM Wiki 的 Agent 执行器。\
你每轮只能返回 1 个 JSON 决策，不要 markdown 代码块，不要额外解释。\n\n\
可用工具：\n\
1) run_shell: {{\"tool\":\"run_shell\",\"command\":\"...\",\"timeout_ms\":30000}}\n\
2) search_wiki: {{\"tool\":\"search_wiki\",\"query\":\"...\",\"limit\":5}}\n\
3) read_wiki: {{\"tool\":\"read_wiki\",\"path\":\"wiki/xxx.md\",\"max_chars\":1200}}\n\
4) write_wiki: {{\"tool\":\"write_wiki\",\"path\":\"wiki/xxx.md\",\"content\":\"...\"}}\n\
5) edit_wiki: {{\"tool\":\"edit_wiki\",\"path\":\"wiki/xxx.md\",\"old_str\":\"...\",\"new_str\":\"...\"}}\n\n\
返回格式（必须是 JSON 对象）：\n\
{{\"done\":false,\"note\":\"...\",\"action\":{{...}}}}\n\
或\n\
{{\"done\":true,\"final\":\"...\"}}\n\n\
规则：\n\
1. 本轮最多 1 个 action；总预算 {iteration_budget} 轮。\n\
2. run_shell 仅允许只读探测命令；可能被策略拒绝。\n\
3. 优先使用 search_wiki/read_wiki 获取知识库证据。\n\
4. write_wiki 为全量覆盖写入，需审批；edit_wiki 为精确替换，需审批（old_str 必须在文件中存在）。\n\
5. 修改现有页面优先用 edit_wiki；创建新页面用 write_wiki。\n\
6. 信息不足时可继续下一轮；若已足够则 done=true。\n\n\
用户任务：\n{instruction}\n\n\
知识库索引摘要：\n{wiki_excerpt}\n\n\
当前工具历史：\n{history}\n"
    )
}

/// 构造最终总结 prompt。
pub fn build_final_prompt(
    instruction: &str,
    tool_context: &str,
    planner_note_text: &str,
    final_hint: &str,
    wiki_excerpt: &str,
) -> String {
    format!(
        "你是 LLM Wiki 的 Agent 任务模式总结器。\
基于用户任务与工具输出，给出可执行结论。\n\n\
输出格式（必须包含以下四节）：\n\
## 任务理解\n\
## 执行计划\n\
## 风险与护栏\n\
## 当前结论\n\n\
用户任务：\n{instruction}\n\n\
工具输出：\n{tool_context}\n\n\
规划备注：\n{planner_note_text}\n\n\
Agent 自述完成信号：\n{final_hint}\n\n\
知识库索引摘要：\n{wiki_excerpt}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_final_prompt, build_loop_prompt, run_agent_task_loop, summarize_agent_task,
        AgentLoopOutcome, AgentLoopRuntime,
    };
    use crate::agent_tools::{AgentToolAction, ToolActionResult};
    use async_trait::async_trait;

    struct MockRuntime {
        output: String,
    }

    #[async_trait]
    impl AgentLoopRuntime for MockRuntime {
        async fn complete_prompt(&self, _prompt: String) -> Result<String, String> {
            Ok(self.output.clone())
        }
        async fn execute_tool_action(
            &self,
            _idx: u32,
            _action: AgentToolAction,
        ) -> ToolActionResult {
            ToolActionResult {
                level: "info".to_string(),
                log_line: String::new(),
                requires_approval: false,
                pending_write: None,
                pending_edit: None,
            }
        }
        fn append_event(&self, _run_id: i64, _level: &str, _message: String) {}
    }

    #[test]
    fn loop_prompt_contains_tool_contract() {
        let p = build_loop_prompt("任务", 3, "index", "history");
        assert!(p.contains("run_shell"));
        assert!(p.contains("search_wiki"));
        assert!(p.contains("read_wiki"));
        assert!(p.contains("write_wiki"));
        assert!(p.contains("edit_wiki"));
        assert!(p.contains("总预算 3 轮"));
    }

    #[test]
    fn final_prompt_contains_required_sections() {
        let p = build_final_prompt("任务", "工具", "备注", "完成", "索引");
        assert!(p.contains("## 任务理解"));
        assert!(p.contains("## 执行计划"));
        assert!(p.contains("## 风险与护栏"));
        assert!(p.contains("## 当前结论"));
    }

    #[tokio::test]
    async fn summarize_agent_task_uses_runtime_response() {
        let runtime = MockRuntime {
            output: "最终答案".to_string(),
        };
        let outcome = AgentLoopOutcome {
            tool_logs: vec!["tool-log".to_string()],
            planner_notes: vec!["note".to_string()],
            final_hint: Some("hint".to_string()),
            pending_write: None,
            pending_edit: None,
        };
        let answer = summarize_agent_task(&runtime, "任务", "索引", &outcome)
            .await
            .expect("总结应成功");
        assert_eq!(answer, "最终答案");
    }

    struct ApprovalRuntime;

    #[async_trait]
    impl AgentLoopRuntime for ApprovalRuntime {
        async fn complete_prompt(&self, _prompt: String) -> Result<String, String> {
            Ok(
                r##"{"done":false,"action":{"tool":"write_wiki","path":"wiki/a.md","content":"# A"}}"##
                    .to_string(),
            )
        }

        async fn execute_tool_action(
            &self,
            _idx: u32,
            _action: AgentToolAction,
        ) -> ToolActionResult {
            ToolActionResult {
                level: "warn".to_string(),
                log_line: "[tool#1/write_wiki] decision=require_approval".to_string(),
                requires_approval: true,
                pending_write: Some(("vault/wiki/a.md".to_string(), "# A".to_string())),
                pending_edit: None,
            }
        }

        fn append_event(&self, _run_id: i64, _level: &str, _message: String) {}
    }

    #[tokio::test]
    async fn run_agent_task_loop_stops_on_approval_required() {
        let runtime = ApprovalRuntime;
        let outcome = run_agent_task_loop(&runtime, 1, "任务", 4, "index")
            .await
            .expect("循环应成功返回");
        assert_eq!(outcome.tool_logs.len(), 1);
        assert_eq!(
            outcome.final_hint.as_deref(),
            Some("触发审批前置，等待人工确认")
        );
        assert!(outcome.pending_write.is_some());
        assert_eq!(outcome.pending_write.as_ref().unwrap().0, "vault/wiki/a.md");
    }
}
