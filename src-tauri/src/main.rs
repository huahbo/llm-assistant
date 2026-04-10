#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod db;
mod models;
mod state;
mod vault;

use state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_overview,
            commands::get_default_paths,
            commands::get_query_settings,
            commands::set_mode,
            commands::get_recent_logs,
            commands::run_lint,
            commands::init_vault,
            commands::ingest_markdown,
            commands::query_ask,
            commands::query_ask_with_options,
            commands::set_query_top_k,
            commands::save_query_answer
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}
