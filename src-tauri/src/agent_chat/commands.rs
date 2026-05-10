//! agent_chat Tauri 命令层

use tauri::{AppHandle, State};

use crate::agent_chat::{
    db as chat_db,
    runtime::{new_cancel_token, process_message_turn},
    Conversation, Message,
};
use crate::state::{current_timestamp_ms, AppState};
use chrono::Local;

// ── 会话管理 ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn create_conversation(
    title: String,
    skill_key: Option<String>,
    inject_memories: Option<bool>,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;

    let system_prompt = build_system_prompt(&state, skill_key.as_deref(), inject_memories);
    let now = current_timestamp_ms();
    chat_db::create_conversation(
        &db_path,
        &title,
        system_prompt.as_deref(),
        skill_key.as_deref(),
        None,
        &now,
    )
}

#[tauri::command]
pub async fn list_conversations(
    include_archived: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<Conversation>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    chat_db::list_conversations(&db_path, include_archived.unwrap_or(false))
}

#[tauri::command]
pub async fn rename_conversation(
    conversation_id: i64,
    new_title: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let now = current_timestamp_ms();
    chat_db::rename_conversation(&db_path, conversation_id, &new_title, &now)
}

#[tauri::command]
pub async fn archive_conversation(
    conversation_id: i64,
    archived: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let now = current_timestamp_ms();
    chat_db::archive_conversation(&db_path, conversation_id, archived, &now)
}

#[tauri::command]
pub async fn delete_conversation(
    conversation_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    chat_db::delete_conversation(&db_path, conversation_id)
}

// ── 消息 ───────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_chat_messages(
    conversation_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Message>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    chat_db::list_messages(&db_path, conversation_id)
}

#[tauri::command]
pub async fn send_chat_message(
    conversation_id: i64,
    user_message: String,
    state: State<'_, AppState>,
    app_handle: AppHandle,
) -> Result<i64, String> {
    let token = new_cancel_token();
    state.store_chat_cancel_token(conversation_id, token.clone());

    let result =
        process_message_turn(&state, &app_handle, conversation_id, user_message, token).await;

    state.remove_chat_cancel_token(conversation_id);
    result
}

#[tauri::command]
pub async fn cancel_chat_message(
    conversation_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.cancel_chat_token(conversation_id);
    Ok(())
}

#[tauri::command]
pub async fn approve_chat_write(
    pending_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.approve_chat_write(pending_id)
}

#[tauri::command]
pub async fn reject_chat_write(
    pending_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.reject_chat_write(pending_id)
}

// ── 内部：system prompt 构建 ───────────────────────────────────────────────────

fn build_system_prompt(
    state: &AppState,
    skill_key: Option<&str>,
    inject_memories: Option<bool>,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    // 环境基线：Shell 环境 + 当前日期
    let today = Local::now().format("%Y年%m月%d日").to_string();
    parts.push(format!(
        "你运行在 Windows 系统上。使用 run_shell 工具时，执行的是 PowerShell 命令，\
不能使用 Linux/Unix 命令（如 ls -la、grep、cat）。\
请使用等效的 PowerShell 命令（如 Get-ChildItem、Select-String、Get-Content）。\n\
当前日期：{today}。使用 web_search 工具搜索时，请在关键词中加入年份或最新等时间限定词，以获取最新信息。"
    ));

    if let Some(key) = skill_key {
        if let Ok(skills) = state.list_agent_skills_impl(None) {
            if let Some(skill) = skills.iter().find(|s| s.skill_key == key) {
                if !skill.prompt_template.is_empty() {
                    parts.push(skill.prompt_template.clone());
                }
            }
        }
    }

    if inject_memories.unwrap_or(false) {
        if let Ok(memories) = state.list_agent_memories_impl(None, None) {
            if !memories.is_empty() {
                let mem_lines: Vec<String> = memories
                    .iter()
                    .map(|m| format!("- {}: {}", m.memory_key, m.memory_value))
                    .collect();
                parts.push(format!("## 记忆\n{}", mem_lines.join("\n")));
            }
        }
    }

    Some(parts.join("\n\n"))
}
