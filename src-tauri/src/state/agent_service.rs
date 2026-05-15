use std::{fs, time::Duration};

use super::{current_timestamp_ms, AppState};
use crate::{agent_loop::{run_agent_task_loop, summarize_agent_task}, db};

const AGENT_WALL_CLOCK_TIMEOUT: Duration = Duration::from_secs(600);

pub async fn run_agent_task(
    state: &AppState,
    run_id: i64,
    instruction: String,
    max_iterations: Option<u32>,
    memory_context: Option<String>,
) -> Result<String, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;

    let instruction = instruction.trim().to_string();
    if instruction.is_empty() {
        return Err("任务指令不能为空".to_string());
    }

    let iteration_budget = max_iterations.unwrap_or(4).clamp(1, 8);
    let now = current_timestamp_ms();
    db::append_agent_run_event(
        &db_path,
        run_id,
        "info",
        &format!(
            "任务模式已启动（beta，预算 {iteration_budget} 轮）：{}",
            instruction.chars().take(80).collect::<String>()
        ),
        &now,
    )?;

    let vault_path = state.vault_path_or_err()?;
    let wiki_excerpt = fs::read_to_string(vault_path.join("wiki").join("index.md"))
        .unwrap_or_default()
        .chars()
        .take(1200)
        .collect::<String>();
    let memory_context = memory_context
        .unwrap_or_default()
        .trim()
        .chars()
        .take(2400)
        .collect::<String>();

    let inner = async {
        let loop_outcome = run_agent_task_loop(
            state,
            run_id,
            &instruction,
            iteration_budget,
            &wiki_excerpt,
            &memory_context,
        )
        .await?;

        if let Some((path, content)) = loop_outcome.pending_write.clone() {
            state.store_pending_agent_write(run_id, path, content, None);
        } else if let Some((path, old_str, new_str)) = loop_outcome.pending_edit.clone() {
            state.store_pending_agent_write(run_id, path, new_str, Some(old_str));
        }

        let answer = summarize_agent_task(state, &instruction, &wiki_excerpt, &loop_outcome).await?;
        Ok::<_, String>((loop_outcome, answer))
    };

    let (loop_outcome, answer) = tokio::time::timeout(AGENT_WALL_CLOCK_TIMEOUT, inner)
        .await
        .map_err(|_| "Agent 任务超时（超过 10 分钟），已自动终止".to_string())??;

    let done_at = current_timestamp_ms();
    let preview: String = answer.chars().take(120).collect();
    let _ = db::append_agent_run_event(
        &db_path,
        run_id,
        "info",
        &format!(
            "任务模式已完成（beta，多轮工具调用 {} 次）：{}",
            loop_outcome.tool_logs.len(),
            preview
        ),
        &done_at,
    );
    let _ = db::complete_agent_run(&db_path, run_id, "reviewing", &done_at);

    Ok(answer)
}
