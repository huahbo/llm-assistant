use super::AppState;
use crate::{
    agent_policy::classify_shell_policy_with_config,
    db,
    models::{ShellAuditEvent, ShellResult},
};
use std::path::{Path, PathBuf};
use tokio::io::AsyncBufReadExt;

// ─── Shell 自由函数 ───────────────────────────────────────────────────────────

fn normalize_shell_command_for_executor(command: &str, executor: &str) -> String {
    if executor != "manual" {
        return command.to_string();
    }
    let trimmed = command.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return trimmed.to_string();
    }
    let head = parts[0].to_ascii_lowercase();
    if head == "pwd" {
        return "Get-Location".to_string();
    }
    if head == "ls" {
        if parts.len() == 1 {
            return "Get-ChildItem".to_string();
        }
        let mut show_all = false;
        for arg in parts.iter().skip(1) {
            let normalized = arg.to_ascii_lowercase();
            if matches!(normalized.as_str(), "-a" | "-la" | "-al" | "--all") {
                show_all = true;
                continue;
            }
            return command.to_string();
        }
        return if show_all {
            "Get-ChildItem -Force".to_string()
        } else {
            "Get-ChildItem".to_string()
        };
    }
    command.to_string()
}

fn parse_cd_target(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.eq_ignore_ascii_case("cd") {
        return Some(".".to_string());
    }
    let lowered = trimmed.to_ascii_lowercase();
    let raw = if lowered.starts_with("cd /d ") {
        trimmed[6..].trim()
    } else if lowered.starts_with("cd ") {
        trimmed[3..].trim()
    } else {
        return None;
    };
    if raw.is_empty() {
        return Some(".".to_string());
    }
    Some(raw.trim_matches('"').trim_matches('\'').to_string())
}

fn resolve_cd_target(base: &Path, target: &str) -> Result<PathBuf, String> {
    let target_path = PathBuf::from(target);
    let next = if target_path.is_absolute() {
        target_path
    } else {
        base.join(target_path)
    };
    let canonical = next
        .canonicalize()
        .map_err(|e| format!("切换目录失败: {}", e))?;
    if !canonical.is_dir() {
        return Err(format!("目标不是目录: {}", canonical.to_string_lossy()));
    }
    Ok(canonical)
}

fn rand_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_nanos())
        .unwrap_or(0);
    format!("{:x}", seed & 0xfffff)
}

fn decode_shell_output_chunk(raw: &[u8]) -> String {
    if let Some(text) = decode_utf16_chunk(raw) {
        return text.trim_end_matches(['\r', '\n']).to_string();
    }
    String::from_utf8_lossy(raw)
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

fn decode_utf16_chunk(raw: &[u8]) -> Option<String> {
    if raw.len() < 2 {
        return None;
    }
    if raw.len() % 2 != 0 {
        return None;
    }
    if raw.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    if raw.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    let nul_count = raw.iter().filter(|&&b| b == 0).count();
    if nul_count * 3 < raw.len() {
        return None;
    }
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units))
}

// ─── Shell 私有 helper（需要 AppState 访问）──────────────────────────────────

fn resolve_shell_cwd(state: &AppState, session_id: Option<&str>) -> Result<PathBuf, String> {
    if let Some(id) = session_id.map(str::trim).filter(|v| !v.is_empty()) {
        let map = state.shell_sessions.lock().expect("shell_sessions lock");
        if let Some(s) = map.get(id) {
            return Ok(s.cwd.clone());
        }
        return Err(format!("Shell 会话不存在: {}", id));
    }
    let data = state.inner.lock().unwrap();
    Ok(data.vault_path.clone().unwrap_or_else(std::env::temp_dir))
}

fn update_shell_cwd(state: &AppState, session_id: Option<&str>, cwd: PathBuf) {
    let Some(id) = session_id.map(str::trim).filter(|v| !v.is_empty()) else {
        return;
    };
    let mut map = state.shell_sessions.lock().expect("shell_sessions lock");
    if let Some(s) = map.get_mut(id) {
        s.cwd = cwd;
        s.last_active_at = super::current_timestamp_ms();
    }
}

fn emit_shell_stream_chunk(
    state: &AppState,
    stream_id: Option<&str>,
    session_id: Option<&str>,
    chunk: &str,
    stream: &str,
    done: bool,
) {
    use tauri::Emitter;
    let Some(handle) = state.app_handle.get() else {
        return;
    };
    let Some(stream_id) = stream_id.map(str::trim).filter(|v| !v.is_empty()) else {
        return;
    };
    let payload = crate::models::ShellStreamChunk {
        stream_id: stream_id.to_string(),
        session_id: session_id
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string()),
        chunk: chunk.to_string(),
        stream: stream.to_string(),
        done,
    };
    let _ = handle.emit("shell_stream_chunk", payload);
}

// ─── Shell 公共方法实现 ───────────────────────────────────────────────────────

pub async fn run_shell_impl(
    state: &AppState,
    command: String,
    timeout_ms: u64,
    source: Option<String>,
    session_id: Option<String>,
    stream_id: Option<String>,
) -> Result<ShellResult, String> {
    let executor = source
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("manual")
        .to_lowercase();
    let raw_command = command.trim().to_string();
    if raw_command.is_empty() {
        return Err("命令不能为空".to_string());
    }
    let command = normalize_shell_command_for_executor(&raw_command, &executor);
    let shell_policy_config = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.shell_policy.clone()
    };
    let (policy_action, policy_decision, block_reason) =
        classify_shell_policy_with_config(&command, &executor, &shell_policy_config);
    let cwd = resolve_shell_cwd(state, session_id.as_deref())?;
    let audit_db_path = state.outbox_db_path();
    if let Some(reason) = block_reason {
        if let Some(ref db_path) = audit_db_path {
            let audit = ShellAuditEvent {
                id: 0,
                command: raw_command.clone(),
                working_dir: cwd.to_string_lossy().to_string(),
                policy_action: policy_action.to_string(),
                policy_decision: policy_decision.to_string(),
                executor: executor.clone(),
                blocked: true,
                blocked_reason: Some(reason.clone()),
                exit_code: None,
                latency_ms: None,
                session_id: session_id.clone(),
                created_at: super::current_timestamp_ms(),
            };
            let _ = db::insert_shell_audit_event(db_path, &audit);
        }
        return Ok(ShellResult {
            command: raw_command,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: -1,
            working_dir: cwd.to_string_lossy().to_string(),
            blocked: true,
            blocked_reason: Some(reason),
            policy_action: policy_action.to_string(),
            policy_decision: policy_decision.to_string(),
            executor,
        });
    }
    if let Some(target) = parse_cd_target(&raw_command) {
        let resolved = resolve_cd_target(&cwd, &target)?;
        update_shell_cwd(state, session_id.as_deref(), resolved.clone());
        return Ok(ShellResult {
            command: raw_command,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            working_dir: resolved.to_string_lossy().to_string(),
            blocked: false,
            blocked_reason: None,
            policy_action: policy_action.to_string(),
            policy_decision: policy_decision.to_string(),
            executor,
        });
    }
    let exec_start = std::time::Instant::now();
    #[cfg(target_os = "windows")]
    let mut child = tokio::process::Command::new("powershell");
    #[cfg(target_os = "windows")]
    child.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!(
            "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); [Console]::InputEncoding=[System.Text.UTF8Encoding]::new($false); $OutputEncoding=[System.Text.UTF8Encoding]::new($false); {}",
            command
        ),
    ]);
    #[cfg(not(target_os = "windows"))]
    let mut child = tokio::process::Command::new("bash");
    #[cfg(not(target_os = "windows"))]
    child.arg("-c").arg(&command);
    let mut child = child
        .current_dir(&cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("执行失败: {e}"))?;
    let timeout = std::time::Duration::from_millis(timeout_ms.min(120_000));
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut exit_code: Option<i32> = None;
    let deadline = tokio::time::Instant::now() + timeout;

    let mut stdout_reader = child.stdout.take().map(tokio::io::BufReader::new);
    let mut stderr_reader = child.stderr.take().map(tokio::io::BufReader::new);

    loop {
        if tokio::time::Instant::now() >= deadline && exit_code.is_none() {
            let _ = child.kill().await;
            emit_shell_stream_chunk(
                state,
                stream_id.as_deref(),
                session_id.as_deref(),
                "命令执行超时，已中止。",
                "system",
                true,
            );
            return Err(format!("超时（{timeout_ms}ms）"));
        }
        tokio::select! {
            line = async {
                if let Some(reader) = &mut stdout_reader {
                    let mut buf = Vec::new();
                    reader.read_until(b'\n', &mut buf).await.map(|size| {
                        if size == 0 { None } else { Some(buf) }
                    })
                } else {
                    Ok(None)
                }
            }, if !stdout_closed => {
                match line {
                    Ok(Some(raw)) => {
                        let text = decode_shell_output_chunk(&raw);
                        stdout.push_str(&text);
                        if !text.ends_with('\n') { stdout.push('\n'); }
                        emit_shell_stream_chunk(state, stream_id.as_deref(), session_id.as_deref(), &text, "stdout", false);
                    }
                    Ok(None) => { stdout_closed = true; }
                    Err(e) => {
                        stderr.push_str(&format!("读取 stdout 失败: {e}\n"));
                        stdout_closed = true;
                    }
                }
            }
            line = async {
                if let Some(reader) = &mut stderr_reader {
                    let mut buf = Vec::new();
                    reader.read_until(b'\n', &mut buf).await.map(|size| {
                        if size == 0 { None } else { Some(buf) }
                    })
                } else {
                    Ok(None)
                }
            }, if !stderr_closed => {
                match line {
                    Ok(Some(raw)) => {
                        let text = decode_shell_output_chunk(&raw);
                        stderr.push_str(&text);
                        if !text.ends_with('\n') { stderr.push('\n'); }
                        emit_shell_stream_chunk(state, stream_id.as_deref(), session_id.as_deref(), &text, "stderr", false);
                    }
                    Ok(None) => { stderr_closed = true; }
                    Err(e) => {
                        stderr.push_str(&format!("读取 stderr 失败: {e}\n"));
                        stderr_closed = true;
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(18)) => {}
        }
        if exit_code.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => { exit_code = Some(status.code().unwrap_or(-1)); }
                Ok(None) => {}
                Err(e) => return Err(format!("执行失败: {e}")),
            }
        }
        if stdout_closed && stderr_closed && exit_code.is_some() {
            break;
        }
    }
    emit_shell_stream_chunk(state, stream_id.as_deref(), session_id.as_deref(), "", "system", true);
    update_shell_cwd(state, session_id.as_deref(), cwd.clone());
    let elapsed_ms = exec_start.elapsed().as_millis() as i64;
    let final_exit_code = exit_code.unwrap_or(-1);
    if let Some(ref db_path) = audit_db_path {
        let audit = ShellAuditEvent {
            id: 0,
            command: raw_command.clone(),
            working_dir: cwd.to_string_lossy().to_string(),
            policy_action: policy_action.to_string(),
            policy_decision: policy_decision.to_string(),
            executor: executor.clone(),
            blocked: false,
            blocked_reason: None,
            exit_code: Some(final_exit_code),
            latency_ms: Some(elapsed_ms),
            session_id: session_id.clone(),
            created_at: super::current_timestamp_ms(),
        };
        let _ = db::insert_shell_audit_event(db_path, &audit);
    }
    Ok(ShellResult {
        command: raw_command,
        stdout,
        stderr,
        exit_code: final_exit_code,
        working_dir: cwd.to_string_lossy().to_string(),
        blocked: false,
        blocked_reason: None,
        policy_action: policy_action.to_string(),
        policy_decision: policy_decision.to_string(),
        executor,
    })
}

pub fn create_shell_session_impl(
    state: &AppState,
    source: Option<String>,
) -> Result<crate::models::ShellSessionInfo, String> {
    let executor = source
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("manual")
        .to_lowercase();
    let now = super::current_timestamp_ms();
    let session_id = format!("sh-{}-{}", now, rand_suffix());
    let cwd = {
        let data = state.inner.lock().unwrap();
        data.vault_path.clone().unwrap_or_else(std::env::temp_dir)
    };
    let session_state = super::ShellSessionState {
        cwd: cwd.clone(),
        last_active_at: now.clone(),
    };
    state
        .shell_sessions
        .lock()
        .expect("shell_sessions lock")
        .insert(session_id.clone(), session_state);
    Ok(crate::models::ShellSessionInfo {
        session_id,
        working_dir: cwd.to_string_lossy().to_string(),
        executor,
        created_at: now,
    })
}

pub fn close_shell_session_impl(state: &AppState, session_id: String) -> bool {
    state
        .shell_sessions
        .lock()
        .expect("shell_sessions lock")
        .remove(session_id.trim())
        .is_some()
}

pub async fn approve_and_run_shell_impl(
    state: &AppState,
    command: String,
    timeout_ms: u64,
    session_id: Option<String>,
    stream_id: Option<String>,
) -> Result<ShellResult, String> {
    let raw_command = command.trim().to_string();
    if raw_command.is_empty() {
        return Err("命令不能为空".to_string());
    }
    let executor = "manual_approved".to_string();
    let normalized = normalize_shell_command_for_executor(&raw_command, &executor);
    let cwd = resolve_shell_cwd(state, session_id.as_deref())?;
    let audit_db_path = state.outbox_db_path();

    if let Some(target) = parse_cd_target(&raw_command) {
        let resolved = resolve_cd_target(&cwd, &target)?;
        update_shell_cwd(state, session_id.as_deref(), resolved.clone());
        if let Some(ref db_path) = audit_db_path {
            let audit = ShellAuditEvent {
                id: 0,
                command: raw_command.clone(),
                working_dir: resolved.to_string_lossy().to_string(),
                policy_action: "builtin_cd".to_string(),
                policy_decision: "approved_by_user".to_string(),
                executor: executor.clone(),
                blocked: false,
                blocked_reason: None,
                exit_code: Some(0),
                latency_ms: Some(0),
                session_id: session_id.clone(),
                created_at: super::current_timestamp_ms(),
            };
            let _ = db::insert_shell_audit_event(db_path, &audit);
        }
        return Ok(ShellResult {
            command: raw_command,
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
            working_dir: resolved.to_string_lossy().to_string(),
            blocked: false,
            blocked_reason: None,
            policy_action: "builtin_cd".to_string(),
            policy_decision: "approved_by_user".to_string(),
            executor,
        });
    }

    let exec_start = std::time::Instant::now();
    #[cfg(target_os = "windows")]
    let mut child = tokio::process::Command::new("powershell");
    #[cfg(target_os = "windows")]
    child.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!(
            "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); [Console]::InputEncoding=[System.Text.UTF8Encoding]::new($false); $OutputEncoding=[System.Text.UTF8Encoding]::new($false); {}",
            normalized
        ),
    ]);
    #[cfg(not(target_os = "windows"))]
    let mut child = tokio::process::Command::new("bash");
    #[cfg(not(target_os = "windows"))]
    child.arg("-c").arg(&normalized);
    let mut child = child
        .current_dir(&cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("执行失败: {e}"))?;
    let timeout = std::time::Duration::from_millis(timeout_ms.min(120_000));
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut stdout_closed = false;
    let mut stderr_closed = false;
    let mut exit_code: Option<i32> = None;
    let deadline = tokio::time::Instant::now() + timeout;

    let mut stdout_reader = child.stdout.take().map(tokio::io::BufReader::new);
    let mut stderr_reader = child.stderr.take().map(tokio::io::BufReader::new);

    loop {
        if tokio::time::Instant::now() >= deadline && exit_code.is_none() {
            let _ = child.kill().await;
            emit_shell_stream_chunk(
                state,
                stream_id.as_deref(),
                session_id.as_deref(),
                "命令执行超时，已中止。",
                "system",
                true,
            );
            return Err(format!("超时（{timeout_ms}ms）"));
        }
        tokio::select! {
            line = async {
                if let Some(reader) = &mut stdout_reader {
                    let mut buf = Vec::new();
                    reader.read_until(b'\n', &mut buf).await.map(|size| {
                        if size == 0 { None } else { Some(buf) }
                    })
                } else {
                    Ok(None)
                }
            }, if !stdout_closed => {
                match line {
                    Ok(Some(raw)) => {
                        let text = decode_shell_output_chunk(&raw);
                        stdout.push_str(&text);
                        if !text.ends_with('\n') { stdout.push('\n'); }
                        emit_shell_stream_chunk(state, stream_id.as_deref(), session_id.as_deref(), &text, "stdout", false);
                    }
                    Ok(None) => { stdout_closed = true; }
                    Err(e) => {
                        stderr.push_str(&format!("读取 stdout 失败: {e}\n"));
                        stdout_closed = true;
                    }
                }
            }
            line = async {
                if let Some(reader) = &mut stderr_reader {
                    let mut buf = Vec::new();
                    reader.read_until(b'\n', &mut buf).await.map(|size| {
                        if size == 0 { None } else { Some(buf) }
                    })
                } else {
                    Ok(None)
                }
            }, if !stderr_closed => {
                match line {
                    Ok(Some(raw)) => {
                        let text = decode_shell_output_chunk(&raw);
                        stderr.push_str(&text);
                        if !text.ends_with('\n') { stderr.push('\n'); }
                        emit_shell_stream_chunk(state, stream_id.as_deref(), session_id.as_deref(), &text, "stderr", false);
                    }
                    Ok(None) => { stderr_closed = true; }
                    Err(e) => {
                        stderr.push_str(&format!("读取 stderr 失败: {e}\n"));
                        stderr_closed = true;
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(18)) => {}
        }
        if exit_code.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => { exit_code = Some(status.code().unwrap_or(-1)); }
                Ok(None) => {}
                Err(e) => return Err(format!("执行失败: {e}")),
            }
        }
        if stdout_closed && stderr_closed && exit_code.is_some() {
            break;
        }
    }
    emit_shell_stream_chunk(state, stream_id.as_deref(), session_id.as_deref(), "", "system", true);
    update_shell_cwd(state, session_id.as_deref(), cwd.clone());
    let elapsed_ms = exec_start.elapsed().as_millis() as i64;
    let final_exit_code = exit_code.unwrap_or(-1);
    if let Some(ref db_path) = audit_db_path {
        let audit = ShellAuditEvent {
            id: 0,
            command: raw_command.clone(),
            working_dir: cwd.to_string_lossy().to_string(),
            policy_action: "approved".to_string(),
            policy_decision: "approved_by_user".to_string(),
            executor: executor.clone(),
            blocked: false,
            blocked_reason: None,
            exit_code: Some(final_exit_code),
            latency_ms: Some(elapsed_ms),
            session_id: session_id.clone(),
            created_at: super::current_timestamp_ms(),
        };
        let _ = db::insert_shell_audit_event(db_path, &audit);
    }
    Ok(ShellResult {
        command: raw_command,
        stdout,
        stderr,
        exit_code: final_exit_code,
        working_dir: cwd.to_string_lossy().to_string(),
        blocked: false,
        blocked_reason: None,
        policy_action: "approved".to_string(),
        policy_decision: "approved_by_user".to_string(),
        executor,
    })
}

pub fn list_shell_audit_events_impl(
    state: &AppState,
    limit: i64,
) -> Result<Vec<ShellAuditEvent>, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "请先初始化 Vault".to_string())?;
    db::list_shell_audit_events(&db_path, limit)
}
