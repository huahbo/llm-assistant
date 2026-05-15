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
        None,
        0,
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
pub async fn search_chat_messages(
    query: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<chat_db::MessageSearchHit>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    chat_db::search_messages(&db_path, &query, limit.unwrap_or(20).max(1).min(100) as usize)
}

#[tauri::command]
pub async fn list_child_conversations(
    parent_conv_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Conversation>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    chat_db::list_child_conversations(&db_path, parent_conv_id)
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

// ── 对话导出 ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn export_conversation_markdown(
    conversation_id: i64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;

    let conv = chat_db::get_conversation(&db_path, conversation_id)?
        .ok_or_else(|| format!("会话 #{conversation_id} 不存在"))?;
    let messages = chat_db::list_messages(&db_path, conversation_id)?;

    let mut out = format!("# {}\n\n", conv.title);

    for msg in &messages {
        match msg.role.as_str() {
            "user" => {
                let content = msg.content.as_deref().unwrap_or("").trim();
                if !content.is_empty() {
                    out.push_str(&format!("**用户**\n\n{content}\n\n---\n\n"));
                }
            }
            "assistant" => {
                let content = msg.content.as_deref().unwrap_or("").trim();
                if !content.is_empty() {
                    out.push_str(&format!("**AI**\n\n{content}\n\n---\n\n"));
                } else if msg.tool_calls_json.is_some() {
                    // Assistant message that only has tool calls — skip body
                }
            }
            _ => {} // skip system / tool result messages
        }
    }

    Ok(out.trim_end_matches("---\n\n").to_string())
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

#[tauri::command]
pub async fn get_conv_shell_mode(
    conversation_id: i64,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let db_path = state.outbox_db_path().ok_or_else(|| "Vault 未初始化".to_string())?;
    chat_db::get_conv_shell_mode(&db_path, conversation_id)
}

#[tauri::command]
pub async fn set_conv_shell_mode(
    conversation_id: i64,
    mode: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if !["off", "yolo", "approval"].contains(&mode.as_str()) {
        return Err(format!("无效 shell_mode: {mode}，必须为 off / yolo / approval"));
    }
    let db_path = state.outbox_db_path().ok_or_else(|| "Vault 未初始化".to_string())?;
    chat_db::set_conv_shell_mode(&db_path, conversation_id, &mode)
}

#[tauri::command]
pub async fn approve_chat_shell(
    pending_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.approve_chat_shell_impl(pending_id).await
}

#[tauri::command]
pub async fn reject_chat_shell(
    pending_id: i64,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.reject_chat_shell_impl(pending_id)
}

// ── MCP 服务器管理 ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_mcp_servers(
    state: State<'_, AppState>,
) -> Result<Vec<crate::agent_chat::mcp::McpServerConfig>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let mut configs = chat_db::list_mcp_servers(&db_path)?;
    let running = state.list_running_mcp_clients();
    for cfg in &mut configs {
        cfg.enabled = running.contains(&cfg.name);
    }
    Ok(configs)
}

#[tauri::command]
pub async fn upsert_mcp_server(
    name: String,
    command: String,
    args: Vec<String>,
    env: std::collections::HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let args_json = serde_json::to_string(&args).unwrap_or_else(|_| "[]".to_string());
    let env_json = serde_json::to_string(&env).unwrap_or_else(|_| "{}".to_string());
    let now = current_timestamp_ms();
    chat_db::upsert_mcp_server(&db_path, &name, &command, &args_json, &env_json, true, &now)
}

#[tauri::command]
pub async fn delete_mcp_server(
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    state.stop_mcp_client(&name);
    chat_db::delete_mcp_server(&db_path, &name)
}

/// Spawn the MCP server, call tools/list, and sync tools into agent_tools.
#[tauri::command]
pub async fn reload_mcp_server_tools(
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;

    let cfg = chat_db::get_mcp_server(&db_path, &name)?
        .ok_or_else(|| format!("MCP 服务器 '{name}' 不存在"))?;

    // (Re)spawn the client
    state.stop_mcp_client(&name);
    state.spawn_mcp_client(name.clone(), &cfg.command, &cfg.args, &cfg.env).await?;

    let arc = state
        .get_mcp_client(&name)
        .ok_or_else(|| "客户端未启动".to_string())?;
    let tools = {
        let mut client = arc.lock().await;
        client.list_tools().await?
    };

    let tool_names: Vec<String> = tools.iter().map(|t| t.tool_name.clone()).collect();
    chat_db::sync_mcp_tools(&db_path, &name, &tools)?;

    Ok(tool_names)
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
