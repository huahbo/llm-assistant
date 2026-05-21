#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_chat;
mod agent_loop;
mod browser;
mod agent_policy;
mod agent_runtime;
mod agent_tools;
mod clip_server;
mod commands;
mod db;
mod llm;
mod models;
mod search;
mod state;
mod vault;

use state::AppState;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            // decorations=false 时 Windows 任务栏图标需手动注入
            if let Some(window) = app.get_webview_window("main") {
                if let Some(icon) = app.default_window_icon() {
                    let _ = window.set_icon(icon.clone());
                }
            }
            let app_handle = app.handle().clone();
            // 注入 AppHandle，供后续 emit 进度事件使用
            app.state::<AppState>().set_app_handle(app_handle.clone());
            // 初始化 Embed Provider（后台线程，ONNX 加载最长数秒，不阻塞窗口渲染）
            let handle_for_embed = app_handle.clone();
            std::thread::spawn(move || {
                handle_for_embed.state::<AppState>().init_embed_provider();
            });
            // 启动时清理孤立 wiki 页面（文件已删但 DB 记录残留）
            app.state::<AppState>().purge_orphaned_wiki_pages();
            // 启动持久化 ingest 队列 worker
            crate::state::AppState::start_queue_worker(app_handle.clone());
            // 启动本地剪藏服务器
            clip_server::start_clip_server(app_handle);
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_overview,
            commands::get_default_paths,
            commands::get_query_settings,
            commands::set_mode,
            commands::get_recent_logs,
            commands::get_recent_wiki_pages,
            commands::search_wiki_pages,
            commands::search_wiki_pages_hybrid,
            commands::search_wiki_paths,
            commands::get_wiki_page_detail,
            commands::get_wiki_page_citations,
            commands::run_lint,
            commands::preview_lint_patches,
            commands::apply_lint_patch,
            commands::apply_lint_patches_batch,
            commands::get_recent_lint_patch_events,
            commands::get_llm_status,
            commands::get_llm_provider_presets,
            commands::init_vault,
            commands::init_vault_with_template,
            commands::ingest_markdown,
            commands::ingest_file,
            commands::preview_ingest_file,
            commands::apply_ingest_preview,
            commands::ingest_pdf,
            commands::ingest_url,
            commands::query_ask,
            commands::query_ask_with_options,
            commands::set_query_top_k,
            commands::save_query_answer,
            commands::get_llm_config,
            commands::set_llm_config,
            commands::get_ocr_config,
            commands::set_ocr_config,
            commands::get_shell_policy_config,
            commands::set_shell_policy_config,
            commands::get_clip_server_status,
            commands::save_wiki_page,
            commands::list_wiki_page_history,
            commands::get_wiki_page_history_entry,
            commands::restore_wiki_page_from_history,
            commands::rename_wiki_page,
            commands::delete_wiki_page,
            commands::save_ask_history,
            commands::get_ask_history,
            commands::clear_ask_history,
            commands::create_ask_session,
            commands::list_ask_sessions,
            commands::get_ask_session_turns,
            commands::search_ask_session_turns,
            commands::rename_ask_session,
            commands::delete_ask_session,
            commands::get_outbox_events,
            commands::ack_outbox_events,
            commands::start_agent_run,
            commands::append_agent_run_event,
            commands::list_agent_runs,
            commands::list_agent_run_events,
            commands::complete_agent_run,
            commands::archive_agent_run,
            commands::restore_agent_run,
            commands::generate_agent_draft,
            commands::run_agent_task,
            commands::rewrite_agent_draft,
            commands::list_agent_drafts,
            commands::check_agent_draft_conflict,
            commands::approve_agent_draft,
            commands::upsert_agent_memory,
            commands::list_agent_memories,
            commands::delete_agent_memory,
            commands::upsert_agent_skill,
            commands::list_agent_skills,
            commands::delete_agent_skill,
            commands::query_ask_session,
            commands::cancel_ask_session,
            commands::clear_ask_session,
            commands::mark_page_stale,
            commands::get_knowledge_graph,
            commands::get_knowledge_subgraph,
            commands::enqueue_ingest,
            commands::list_ingest_queue,
            commands::cancel_ingest_item,
            commands::retry_ingest_item,
            commands::delete_ingest_item,
            commands::get_page_embedding_similarities,
            commands::start_research,
            commands::list_research_tasks,
            commands::get_research_task,
            commands::cancel_research_task,
            commands::delete_research_task,
            commands::get_search_config,
            commands::set_search_config,
            commands::save_research_doc,
            commands::research_md_to_html,
            commands::approve_research_queries,
            commands::quick_lint_page,
            commands::get_vault_stats,
            commands::ai_assist_wiki_edit,
            commands::export_wiki_markdown_zip,
            commands::export_wiki_html_zip,
            commands::create_wiki_page_with_ai,
            commands::create_shell_session,
            commands::close_shell_session,
            commands::run_shell,
            commands::approve_agent_write,
            commands::reject_agent_write,
            commands::approve_and_run_shell,
            commands::list_shell_audit_events,
            commands::grant_write_ticket,
            commands::read_file_for_chat,
            commands::open_external_url,
            commands::install_skill_from_url,
            commands::get_embed_status,
            commands::rebuild_embeddings,
            agent_chat::commands::create_conversation,
            agent_chat::commands::list_conversations,
            agent_chat::commands::search_chat_messages,
            agent_chat::commands::list_child_conversations,
            agent_chat::commands::export_conversation_markdown,
            agent_chat::commands::rename_conversation,
            agent_chat::commands::archive_conversation,
            agent_chat::commands::delete_conversation,
            agent_chat::commands::list_chat_messages,
            agent_chat::commands::send_chat_message,
            agent_chat::commands::cancel_chat_message,
            agent_chat::commands::approve_chat_write,
            agent_chat::commands::reject_chat_write,
            agent_chat::commands::get_conv_shell_mode,
            agent_chat::commands::set_conv_shell_mode,
            agent_chat::commands::approve_chat_shell,
            agent_chat::commands::reject_chat_shell,
            agent_chat::commands::list_mcp_servers,
            agent_chat::commands::upsert_mcp_server,
            agent_chat::commands::delete_mcp_server,
            agent_chat::commands::reload_mcp_server_tools,
            agent_chat::commands::search_mcp_registry,
            agent_chat::commands::get_mcp_registry_server,
            commands::fetch_url_context,
        ])
        .build(tauri::generate_context!())
        .expect("应用启动失败")
        .run(|_app_handle, event| {
            // 退出请求时强制终止进程：std::thread::spawn 启动的后台线程
            // （clip_server / queue_worker / embed init 等）是非守护线程，
            // 默认会阻止进程退出。这里在 ExitRequested 后用 std::process::exit
            // 立即退出，让 `cargo tauri dev` 终端能正常释放。
            if let tauri::RunEvent::ExitRequested { .. } = event {
                std::process::exit(0);
            }
        });
}
