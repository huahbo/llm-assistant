#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod llm;
mod models;
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
        .invoke_handler(tauri::generate_handler![
            commands::get_app_overview,
            commands::get_default_paths,
            commands::get_query_settings,
            commands::set_mode,
            commands::get_recent_logs,
            commands::get_recent_wiki_pages,
            commands::search_wiki_pages,
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
            commands::ingest_url,
            commands::query_ask,
            commands::query_ask_with_options,
            commands::set_query_top_k,
            commands::save_query_answer,
            commands::get_llm_config,
            commands::set_llm_config
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}
