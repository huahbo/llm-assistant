#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
            // 注入 AppHandle，供后续 emit 进度事件使用
            app.state::<AppState>().set_app_handle(app.handle().clone());
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
            commands::init_vault,
            commands::ingest_markdown,
            commands::ingest_file,
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
            commands::save_wiki_page,
            commands::rename_wiki_page,
            commands::delete_wiki_page,
            commands::save_ask_history,
            commands::get_ask_history,
            commands::clear_ask_history,
            commands::get_outbox_events,
            commands::ack_outbox_events,
            commands::query_ask_session,
            commands::cancel_ask_session,
            commands::clear_ask_session,
            commands::mark_page_stale,
            commands::get_knowledge_graph,
            commands::get_knowledge_subgraph
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}
