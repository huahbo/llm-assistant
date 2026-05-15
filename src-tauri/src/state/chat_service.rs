use super::AppState;
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn store_chat_cancel_token(
    state: &AppState,
    conv_id: i64,
    token: crate::agent_chat::runtime::CancelToken,
) {
    state
        .chat_cancellations
        .lock()
        .expect("chat_cancellations 锁已被污染")
        .insert(conv_id, token);
}

pub fn cancel_chat_token(state: &AppState, conv_id: i64) {
    use std::sync::atomic::Ordering;
    let mut map = state
        .chat_cancellations
        .lock()
        .expect("chat_cancellations 锁已被污染");
    if let Some(token) = map.remove(&conv_id) {
        token.store(true, Ordering::Relaxed);
    }
}

pub fn remove_chat_cancel_token(state: &AppState, conv_id: i64) {
    state
        .chat_cancellations
        .lock()
        .expect("chat_cancellations 锁已被污染")
        .remove(&conv_id);
}

pub fn register_chat_write_approval(
    state: &AppState,
    pending_id: i64,
    tx: tokio::sync::oneshot::Sender<Result<String, String>>,
) {
    state
        .chat_write_approvals
        .lock()
        .expect("chat_write_approvals 锁已被污染")
        .insert(pending_id, tx);
}

pub fn approve_chat_write(state: &AppState, pending_id: i64) -> Result<(), String> {
    let result = state.approve_agent_write_impl(pending_id);
    let msg = result.unwrap_or_else(|e| format!("写操作失败: {e}"));
    let tx = state
        .chat_write_approvals
        .lock()
        .expect("chat_write_approvals 锁已被污染")
        .remove(&pending_id);
    if let Some(tx) = tx {
        let _ = tx.send(Ok(msg));
    }
    Ok(())
}

pub fn reject_chat_write(state: &AppState, pending_id: i64) -> Result<(), String> {
    let _ = state.reject_agent_write_impl(pending_id);
    let tx = state
        .chat_write_approvals
        .lock()
        .expect("chat_write_approvals 锁已被污染")
        .remove(&pending_id);
    if let Some(tx) = tx {
        let _ = tx.send(Err("用户拒绝写操作".to_string()));
    }
    Ok(())
}

pub fn register_chat_shell_pending(
    state: &AppState,
    pending_id: i64,
    command: String,
    timeout_ms: u64,
) {
    state
        .chat_shell_pending
        .lock()
        .expect("chat_shell_pending 锁已被污染")
        .insert(pending_id, (command, timeout_ms));
}

pub async fn approve_chat_shell_impl(state: &AppState, pending_id: i64) -> Result<(), String> {
    let (command, timeout_ms) = state
        .chat_shell_pending
        .lock()
        .expect("chat_shell_pending 锁已被污染")
        .remove(&pending_id)
        .ok_or_else(|| format!("pending_id={pending_id} 不存在或已过期"))?;

    let result = state
        .run_shell_impl(command, timeout_ms, Some("chat_approved".to_string()), None, None)
        .await;

    let content = match result {
        Ok(r) => {
            let mut out = String::new();
            if r.blocked {
                out.push_str(&format!("blocked: {}\n", r.blocked_reason.unwrap_or_default()));
            }
            if !r.stdout.is_empty() {
                out.push_str(&r.stdout);
            }
            if !r.stderr.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str("--- stderr ---\n");
                out.push_str(&r.stderr);
            }
            if out.is_empty() {
                out.push_str("(no output)");
            }
            out
        }
        Err(e) => format!("shell 执行失败: {e}"),
    };

    let tx = state
        .chat_write_approvals
        .lock()
        .expect("chat_write_approvals 锁已被污染")
        .remove(&pending_id);
    if let Some(tx) = tx {
        let _ = tx.send(Ok(content));
    }
    Ok(())
}

pub fn reject_chat_shell_impl(state: &AppState, pending_id: i64) -> Result<(), String> {
    state
        .chat_shell_pending
        .lock()
        .expect("chat_shell_pending 锁已被污染")
        .remove(&pending_id);
    let tx = state
        .chat_write_approvals
        .lock()
        .expect("chat_write_approvals 锁已被污染")
        .remove(&pending_id);
    if let Some(tx) = tx {
        let _ = tx.send(Err("用户拒绝执行".to_string()));
    }
    Ok(())
}

pub async fn spawn_mcp_client(
    state: &AppState,
    name: String,
    command: &str,
    args: &[String],
    env: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let client =
        crate::agent_chat::mcp::McpClient::spawn(name.clone(), command, args, env).await?;
    let mut clients = state.mcp_clients.lock().expect("mcp_clients 锁已被污染");
    clients.insert(name, Arc::new(Mutex::new(client)));
    Ok(())
}

pub fn stop_mcp_client(state: &AppState, name: &str) {
    state
        .mcp_clients
        .lock()
        .expect("mcp_clients 锁已被污染")
        .remove(name);
}

pub fn get_mcp_client(
    state: &AppState,
    name: &str,
) -> Option<Arc<Mutex<crate::agent_chat::mcp::McpClient>>> {
    state
        .mcp_clients
        .lock()
        .expect("mcp_clients 锁已被污染")
        .get(name)
        .cloned()
}

pub fn list_running_mcp_clients(state: &AppState) -> Vec<String> {
    state
        .mcp_clients
        .lock()
        .expect("mcp_clients 锁已被污染")
        .keys()
        .cloned()
        .collect()
}
