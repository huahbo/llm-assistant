use std::fs;

use crate::{
    agent_loop::{run_agent_task_loop, summarize_agent_task},
    db,
    state::{current_timestamp_ms, AppState},
};

/// 执行 Agent 任务模式主流程（H6-S2）。
pub async fn run_agent_task(
    state: &AppState,
    run_id: i64,
    instruction: String,
    max_iterations: Option<u32>,
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

    let loop_outcome =
        run_agent_task_loop(state, run_id, &instruction, iteration_budget, &wiki_excerpt).await?;

    // 如果有 pending_write，存入 state 等待审批
    if let Some((path, content)) = loop_outcome.pending_write.clone() {
        state.store_pending_agent_write(run_id, path, content);
    }

    let answer = summarize_agent_task(state, &instruction, &wiki_excerpt, &loop_outcome).await?;

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
    // 任务模式产出默认进入 reviewing，交由用户决定后续动作。
    let _ = db::complete_agent_run(&db_path, run_id, "reviewing", &done_at);

    Ok(answer)
}
