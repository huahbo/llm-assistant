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

#[cfg(test)]
mod tests {
    use crate::state::test_helpers::*;
    use crate::state::current_timestamp_ms;
    use crate::db;
    use std::{fs, sync::{Arc, Mutex}};

    #[test]
    fn agent_run_h0_impl_lifecycle_works() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h0-state");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let run_id = state
            .start_agent_run_impl("Agent Studio H0")
            .expect("创建 run 失败");
        state
            .append_agent_run_event_impl(run_id, "info", "created")
            .expect("写入事件失败");
        state
            .complete_agent_run_impl(run_id, "applied")
            .expect("结束 run 失败");

        let runs = state
            .list_agent_runs_impl(Some(10), Some(false))
            .expect("读取 runs 失败");
        assert!(!runs.is_empty());
        assert_eq!(runs[0].id, run_id);
        assert_eq!(runs[0].status, "applied");

        let events = state
            .list_agent_run_events_impl(run_id, Some(10))
            .expect("读取 events 失败");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, "created");
        assert!(
            events[1].message.contains("系统状态变更"),
            "complete_agent_run 后应自动写系统事件"
        );
    }

    #[test]
    fn agent_draft_generate_and_approve_impl_works() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h1-state");
        let _guard = TempDirGuard(vault_dir.clone());
        // 使用 bare state，避免 OnceLock 被 make_test_state 的默认 mock 抢先占用
        let state = make_test_state_bare(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let prompt_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let _ = state.llm_provider.set(Arc::new(MockQueryProvider::new(
            "# Rust Actor 模块设计\n\n这里是草稿正文。\n",
            prompt_log,
        )));

        let run_id = state
            .start_agent_run_impl("Agent H1")
            .expect("创建 run 失败");
        let runtime = tokio::runtime::Runtime::new().expect("创建 runtime 失败");
        let draft = runtime
            .block_on(state.generate_agent_draft_impl(
                run_id,
                "Rust Actor 模块设计".to_string(),
                None,
                false,
                false,
            ))
            .expect("生成草稿失败");
        assert_eq!(draft.run_id, run_id);
        assert_eq!(draft.status, "draft");
        assert!(draft.content.contains("Rust Actor 模块设计"));

        let drafts = state
            .list_agent_drafts_impl(run_id, Some(10))
            .expect("列出草稿失败");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].id, draft.id);

        let applied = runtime
            .block_on(state.approve_agent_draft_impl(draft.id))
            .expect("审批草稿失败");
        assert!(applied.wiki_path.ends_with(".md"));
        assert_eq!(applied.title, "Rust Actor 模块设计");
        let file_content = fs::read_to_string(&applied.wiki_path).expect("读取写盘文件失败");
        assert!(file_content.contains("这里是草稿正文"));
    }

    #[test]
    fn agent_draft_generate_with_skill_injects_skill_prompt() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h3-skill-generate");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let prompt_log = Arc::new(Mutex::new(Vec::<String>::new()));
        let _ = state.llm_provider.set(Arc::new(MockQueryProvider::new(
            "# 技能化页面\n\n正文。\n",
            prompt_log.clone(),
        )));

        state
            .upsert_agent_skill_impl("writer", "输出语气：客观、结构化、严禁口语")
            .expect("创建技能失败");
        let run_id = state
            .start_agent_run_impl("技能注入测试")
            .expect("创建 run 失败");
        let runtime = tokio::runtime::Runtime::new().expect("创建 runtime 失败");
        let _ = runtime
            .block_on(state.generate_agent_draft_impl(
                run_id,
                "技能注入测试".to_string(),
                Some("writer".to_string()),
                false,
                false,
            ))
            .expect("生成草稿失败");

        let prompts = prompt_log.lock().expect("读取 prompt 失败");
        assert_eq!(prompts.len(), 1);
        assert!(
            prompts[0].contains("当前启用技能模板"),
            "prompt 应包含技能模板注入段"
        );
        assert!(
            prompts[0].contains("输出语气：客观、结构化、严禁口语"),
            "prompt 应包含选中 skill 的模板内容"
        );
    }

    #[test]
    fn agent_skill_crud_impl_works() {
        let vault_dir = make_temp_dir("llm-wiki-agent-h3-skill");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let created = state
            .upsert_agent_skill_impl("writer", "你是一个简洁的知识写作助手")
            .expect("创建技能失败");
        assert_eq!(created.skill_key, "writer");
        assert_eq!(created.version, 1);

        let updated = state
            .upsert_agent_skill_impl("writer", "你是一个结构化的知识写作助手")
            .expect("更新技能失败");
        assert_eq!(updated.id, created.id);
        assert_eq!(updated.version, 2);

        let list = state
            .list_agent_skills_impl(Some(10))
            .expect("查询技能失败");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, created.id);

        state
            .delete_agent_skill_impl(created.id)
            .expect("删除技能失败");
        let empty = state
            .list_agent_skills_impl(Some(10))
            .expect("查询技能失败");
        assert!(empty.is_empty());
    }

    #[test]
    fn check_agent_draft_conflict_returns_no_conflict_when_page_absent() {
        let vault_dir = make_temp_dir("llm-wiki-h1-conflict");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let run_id = state
            .start_agent_run_impl("冲突检测测试")
            .expect("创建 run 失败");
        let now = current_timestamp_ms();
        let db_path = vault_dir.join(".app").join("meta.db");
        let draft = db::create_agent_draft(
            &db_path,
            run_id,
            "唯一不存在的页面标题",
            "draft content",
            "draft",
            &now,
        )
        .expect("创建草稿失败");

        let info = state
            .check_agent_draft_conflict_impl(draft.id)
            .expect("冲突检测失败");
        assert_eq!(info.draft_id, draft.id);
        assert_eq!(info.title, "唯一不存在的页面标题");
        assert!(!info.conflict);
        assert!(info.existing_path.is_none());
        assert!(info.existing_preview.is_none());
    }

    // ── archive / restore agent run 约束测试 ─────────────────────────────

    #[test]
    fn archive_agent_run_rejects_running_status() {
        let vault_dir = make_temp_dir("llm-wiki-archive-running");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        // start_agent_run_impl 默认 status=running
        let run_id = state.start_agent_run_impl("running 归档测试").expect("创建 run");
        let result = state.archive_agent_run_impl(run_id);
        assert!(result.is_err(), "running 状态应禁止归档");
        assert!(result.unwrap_err().contains("正在进行中"));
    }

    #[test]
    fn archive_agent_run_rejects_when_pending_write_exists() {
        let vault_dir = make_temp_dir("llm-wiki-archive-pending");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let run_id = state.start_agent_run_impl("pending 写入归档测试").expect("创建 run");
        state.complete_agent_run_impl(run_id, "applied").expect("完成 run");

        state.store_pending_agent_write(
            run_id,
            vault_dir.join("wiki").join("block.md").to_string_lossy().to_string(),
            "内容".to_string(),
            None,
        );

        let result = state.archive_agent_run_impl(run_id);
        assert!(result.is_err(), "存在 pending write 时应禁止归档");
        assert!(result.unwrap_err().contains("待审批写入"));
    }

    #[test]
    fn archive_and_restore_agent_run_round_trip() {
        let vault_dir = make_temp_dir("llm-wiki-archive-restore");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let run_id = state.start_agent_run_impl("归档恢复测试").expect("创建 run");
        state.complete_agent_run_impl(run_id, "applied").expect("完成 run");

        // 归档
        let archive_result = state.archive_agent_run_impl(run_id);
        assert!(archive_result.is_ok(), "done 状态应允许归档: {archive_result:?}");

        // 归档后重复归档应失败
        let double_archive = state.archive_agent_run_impl(run_id);
        assert!(double_archive.is_err(), "已归档的 run 不能再次归档");

        // 恢复
        let restore_result = state.restore_agent_run_impl(run_id);
        assert!(restore_result.is_ok(), "已归档 run 应可恢复: {restore_result:?}");

        // 恢复后重复恢复应失败
        let double_restore = state.restore_agent_run_impl(run_id);
        assert!(double_restore.is_err(), "未归档的 run 不能再次恢复");
    }

    // ── approve/reject agent write 审批链路 ──────────────────────────────

    #[test]
    fn approve_agent_write_full_write_creates_file() {
        let vault_dir = make_temp_dir("llm-wiki-approve-write");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let wiki_dir = vault_dir.join("wiki");
        let target = wiki_dir.join("test-approve.md");
        let run_id = 9001_i64;

        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "# 审批写入测试\n\n内容正文。\n".to_string(),
            None, // write_wiki 全量写入
        );

        let result = state.approve_agent_write_impl(run_id);
        assert!(result.is_ok(), "approve 应成功，实际: {result:?}");
        assert!(target.exists(), "文件应被写入");
        let content = fs::read_to_string(&target).expect("读文件");
        assert!(content.contains("审批写入测试"));
        // 写入后 pending 应被消耗
        assert!(state.take_pending_agent_write(run_id).is_none());
    }

    #[test]
    fn reject_agent_write_does_not_create_file() {
        let vault_dir = make_temp_dir("llm-wiki-reject-write");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let target = vault_dir.join("wiki").join("should-not-exist.md");
        let run_id = 9002_i64;

        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "不应被写入的内容".to_string(),
            None,
        );

        let result = state.reject_agent_write_impl(run_id);
        assert!(result.is_ok(), "reject 应成功，实际: {result:?}");
        assert!(!target.exists(), "文件不应被创建");
        assert!(state.take_pending_agent_write(run_id).is_none());
    }

    #[test]
    fn approve_agent_write_patch_replaces_content() {
        let vault_dir = make_temp_dir("llm-wiki-approve-patch");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let wiki_dir = vault_dir.join("wiki");
        let target = wiki_dir.join("patch-target.md");
        fs::write(&target, "# 标题\n\n旧内容段落。\n\n其他部分。\n").expect("初始文件");

        let run_id = 9003_i64;
        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "新内容段落。".to_string(),
            Some("旧内容段落。".to_string()), // edit_wiki patch
        );

        let result = state.approve_agent_write_impl(run_id);
        assert!(result.is_ok(), "patch approve 应成功: {result:?}");
        let content = fs::read_to_string(&target).expect("读文件");
        assert!(content.contains("新内容段落。"), "新内容应存在");
        assert!(!content.contains("旧内容段落。"), "旧内容应被替换");
        assert!(content.contains("其他部分。"), "其他内容应保留");
    }

    #[test]
    fn approve_agent_write_patch_fails_when_old_str_not_found() {
        let vault_dir = make_temp_dir("llm-wiki-approve-patch-fail");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state_bare(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("init vault");

        let wiki_dir = vault_dir.join("wiki");
        let target = wiki_dir.join("patch-fail.md");
        fs::write(&target, "# 标题\n\n实际内容。\n").expect("初始文件");

        let run_id = 9004_i64;
        state.store_pending_agent_write(
            run_id,
            target.to_string_lossy().to_string(),
            "替换后内容".to_string(),
            Some("不存在的旧内容".to_string()),
        );

        let result = state.approve_agent_write_impl(run_id);
        assert!(result.is_err(), "old_str 不存在时应返回 Err");
        assert!(result.unwrap_err().contains("未找到待替换内容"));
        // 文件内容不应变化
        let content = fs::read_to_string(&target).expect("读文件");
        assert!(content.contains("实际内容。"));
    }
}
