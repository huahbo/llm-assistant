use crate::{
    agent_loop::AgentLoopRuntime,
    agent_policy::{validate_agent_read_path, validate_agent_write_path},
    agent_tools::{AgentToolAction, ToolActionResult},
    db,
    state::{resolve_existing_wiki_page_path, strip_think_tags, AppState},
};
use std::{fs, path::PathBuf};

#[async_trait::async_trait]
impl AgentLoopRuntime for AppState {
    async fn complete_prompt(&self, prompt: String) -> Result<String, String> {
        let provider = self
            .get_llm_provider()
            .ok_or_else(|| "LLM provider 未初始化".to_string())?;
        let raw = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            provider.complete(&prompt),
        )
        .await
        .map_err(|_| "任务模式调用超时（>90s）".to_string())?
        .map_err(|e| format!("任务模式调用失败: {:?}", e))?;
        Ok(strip_think_tags(&raw).trim().to_string())
    }

    async fn execute_tool_action(&self, idx: u32, action: AgentToolAction) -> ToolActionResult {
        match action {
            AgentToolAction::RunShell {
                command,
                timeout_ms,
            } => execute_shell_action(self, idx, command, timeout_ms).await,
            AgentToolAction::SearchWiki { query, limit } => {
                execute_search_wiki_action(self, idx, query, limit)
            }
            AgentToolAction::ReadWiki { path, max_chars } => {
                execute_read_wiki_action(self, idx, path, max_chars)
            }
            AgentToolAction::WriteWiki { path, content } => {
                execute_write_wiki_action(self, idx, path, content)
            }
        }
    }

    fn append_event(&self, run_id: i64, level: &str, message: String) {
        let Some(db_path) = self.outbox_db_path() else {
            return;
        };
        let _ = db::append_agent_run_event(
            &db_path,
            run_id,
            level,
            &message,
            &crate::state::current_timestamp_ms(),
        );
    }
}

async fn execute_shell_action(
    state: &AppState,
    idx: u32,
    command: String,
    timeout_ms: u64,
) -> ToolActionResult {
    match state
        .run_shell_impl(command.clone(), timeout_ms, Some("agent".to_string()))
        .await
    {
        Ok(result) => {
            let mut content = String::new();
            if result.blocked {
                content.push_str(&format!(
                    "blocked: {}",
                    result
                        .blocked_reason
                        .unwrap_or_else(|| "unknown".to_string())
                ));
            } else {
                let out = if !result.stdout.trim().is_empty() {
                    result.stdout.trim()
                } else {
                    result.stderr.trim()
                };
                content.push_str(out);
                if content.is_empty() {
                    content.push_str(&format!("(exit {})", result.exit_code));
                }
            }
            let preview = truncate_chars(&content, 320);
            if result.blocked || result.exit_code != 0 {
                make_result("warn", format!(
                    "[tool#{}/run_shell] cmd=`{}` action={} decision={} exit={} blocked={} output={}",
                    idx + 1,
                    command,
                    result.policy_action,
                    result.policy_decision,
                    result.exit_code,
                    result.blocked,
                    preview
                ))
            } else {
                make_result("info", format!(
                    "[tool#{}/run_shell] cmd=`{}` action={} decision={} exit={} blocked={} output={}",
                    idx + 1,
                    command,
                    result.policy_action,
                    result.policy_decision,
                    result.exit_code,
                    result.blocked,
                    preview
                ))
            }
        }
        Err(err) => make_result("warn", format!(
            "[tool#{}/run_shell] cmd=`{}` error={}",
            idx + 1,
            command,
            err
        )),
    }
}

fn execute_search_wiki_action(
    state: &AppState,
    idx: u32,
    query: String,
    limit: usize,
) -> ToolActionResult {
    match state.search_wiki_pages(query.clone(), limit) {
        Ok(pages) => {
            let lines = pages
                .iter()
                .take(limit)
                .map(|p| {
                    format!(
                        "{} | {} | score={:.3} | {}",
                        p.path,
                        p.title,
                        p.score,
                        p.summary.chars().take(80).collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join(" ; ");
            let summary = if lines.is_empty() {
                "(no hits)".to_string()
            } else {
                lines
            };
            make_result("info", format!(
                "[tool#{}/search_wiki] query=`{}` limit={} hits={} output={}",
                idx + 1,
                query,
                limit,
                pages.len(),
                summary
            ))
        }
        Err(err) => make_result("warn", format!(
            "[tool#{}/search_wiki] query=`{}` limit={} error={}",
            idx + 1,
            query,
            limit,
            err
        )),
    }
}

fn execute_read_wiki_action(
    state: &AppState,
    idx: u32,
    path: String,
    max_chars: usize,
) -> ToolActionResult {
    match read_wiki_page_for_agent(state, &path, max_chars) {
        Ok(content) => make_result("info", format!(
            "[tool#{}/read_wiki] path=`{}` max_chars={} output={}",
            idx + 1,
            path,
            max_chars,
            truncate_chars(&content, 320)
        )),
        Err(err) => make_result("warn", format!(
            "[tool#{}/read_wiki] path=`{}` max_chars={} error={}",
            idx + 1,
            path,
            max_chars,
            err
        )),
    }
}

fn execute_write_wiki_action(
    state: &AppState,
    idx: u32,
    path: String,
    content: String,
) -> ToolActionResult {
    let preview = truncate_chars(&content, 200);
    let content_chars = content.chars().count();
    let target = match resolve_agent_write_target_path(state, &path) {
        Ok(p) => p,
        Err(err) => {
            return make_result("warn", format!(
                "[tool#{}/write_wiki] path=`{}` chars={} error={}",
                idx + 1,
                path,
                content_chars,
                err
            ));
        }
    };
    let resolved = target.to_string_lossy().to_string();
    ToolActionResult {
        level: "warn".to_string(),
        log_line: format!(
            "[tool#{}/write_wiki] path=`{}` resolved=`{}` chars={} decision=require_approval preview={}",
            idx + 1, path, resolved, content_chars, preview
        ),
        requires_approval: true,
        pending_write: Some((resolved, content)),
    }
}

fn resolve_agent_write_target_path(state: &AppState, path: &str) -> Result<PathBuf, String> {
    let vault_path = state.vault_path_or_err()?;
    let raw = PathBuf::from(path.trim());
    let target_path = if raw.is_absolute() {
        raw
    } else {
        vault_path.join(&raw)
    };
    validate_agent_write_path(&vault_path, &target_path)?;
    Ok(target_path)
}

fn read_wiki_page_for_agent(
    state: &AppState,
    page_path: &str,
    max_chars: usize,
) -> Result<String, String> {
    let vault_path = state.vault_path_or_err()?;
    let target_path = resolve_existing_wiki_page_path(&vault_path, page_path)?;
    validate_agent_read_path(&vault_path, &target_path)?;
    let content =
        fs::read_to_string(&target_path).map_err(|err| format!("读取页面失败: {}", err))?;
    let preview: String = content.chars().take(max_chars).collect();
    Ok(format!(
        "path={} chars={} content={}",
        target_path.to_string_lossy(),
        preview.chars().count(),
        preview
    ))
}

fn truncate_chars(content: &str, max: usize) -> String {
    content.chars().take(max).collect::<String>()
}

fn make_result(level: &str, line: String) -> ToolActionResult {
    ToolActionResult {
        level: level.to_string(),
        log_line: line,
        requires_approval: false,
        pending_write: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{read_wiki_page_for_agent, resolve_agent_write_target_path};
    use crate::state::AppState;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct TempDirGuard(PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    #[test]
    fn read_wiki_page_for_agent_allows_wiki_markdown() {
        let vault_dir = make_temp_dir("llm-wiki-agent-runtime-allow");
        let _guard = TempDirGuard(vault_dir.clone());
        let config_path = vault_dir.join(".app").join("config.json");
        let state = AppState::new_with_path(config_path);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 vault 失败");

        let page = vault_dir.join("wiki").join("a.md");
        fs::write(&page, "# title\n\nbody").expect("写入 wiki 页面失败");
        let result = read_wiki_page_for_agent(&state, "wiki/a.md", 200).expect("读取应成功");
        assert!(result.contains("path="));
        assert!(result.contains("content="));
    }

    #[test]
    fn read_wiki_page_for_agent_rejects_outside_wiki() {
        let vault_dir = make_temp_dir("llm-wiki-agent-runtime-deny");
        let _guard = TempDirGuard(vault_dir.clone());
        let config_path = vault_dir.join(".app").join("config.json");
        let state = AppState::new_with_path(config_path);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 vault 失败");

        let outsider = vault_dir.join("outside.md");
        fs::write(&outsider, "outside").expect("写入外部文件失败");
        let err = read_wiki_page_for_agent(&state, &outsider.to_string_lossy(), 200)
            .expect_err("wiki 外路径应拒绝");
        assert!(
            err.contains("只允许读取 vault/wiki")
                || err.contains("PathGuard 拒绝")
                || err.contains("页面不存在"),
            "错误信息应体现路径受限，实际: {err}"
        );
    }

    #[test]
    fn resolve_agent_write_target_path_allows_wiki_markdown() {
        let vault_dir = make_temp_dir("llm-wiki-agent-runtime-write-allow");
        let _guard = TempDirGuard(vault_dir.clone());
        let config_path = vault_dir.join(".app").join("config.json");
        let state = AppState::new_with_path(config_path);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 vault 失败");

        let resolved =
            resolve_agent_write_target_path(&state, "wiki/new-page.md").expect("应允许写路径");
        assert!(resolved.to_string_lossy().ends_with("wiki/new-page.md"));
    }

    #[test]
    fn resolve_agent_write_target_path_rejects_non_markdown() {
        let vault_dir = make_temp_dir("llm-wiki-agent-runtime-write-deny");
        let _guard = TempDirGuard(vault_dir.clone());
        let config_path = vault_dir.join(".app").join("config.json");
        let state = AppState::new_with_path(config_path);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 vault 失败");

        let err = resolve_agent_write_target_path(&state, "wiki/new-page.txt")
            .expect_err("非 markdown 路径应拒绝");
        assert!(
            err.contains("仅允许写入 Markdown 页面") || err.contains("PathGuard 拒绝"),
            "错误信息应体现扩展名受限，实际: {err}"
        );
    }
}
