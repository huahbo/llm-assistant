#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
            let app_handle = app.handle().clone();
            // 注入 AppHandle，供后续 emit 进度事件使用
            app.state::<AppState>().set_app_handle(app_handle.clone());
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
            commands::get_page_embedding_similarities,
            commands::start_research,
            commands::list_research_tasks,
            commands::get_research_task,
            commands::cancel_research_task,
            commands::delete_research_task,
            commands::get_search_config,
            commands::set_search_config,
            commands::save_research_doc,
            commands::approve_research_queries,
            commands::quick_lint_page,
            commands::get_vault_stats,
            commands::create_wiki_page_with_ai,
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}
