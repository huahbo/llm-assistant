use tauri::{AppHandle, Manager};
use crate::state::AppState;
use tiny_http::{Server, Response, Header, Method};
use std::thread;
use std::path::PathBuf;
use chrono;
use serde_json;

const PORT: u16 = 19827;

pub fn start_clip_server(app_handle: AppHandle) {
    thread::spawn(move || {
        let server = match Server::http(format!("127.0.0.1:{}", PORT)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[Clip Server] Failed to bind to port {}: {}", PORT, e);
                return;
            }
        };

        println!("[Clip Server] Listening on http://127.0.0.1:{}", PORT);

        for mut request in server.incoming_requests() {
            let cors_headers = vec![
                Header::from_bytes("Access-Control-Allow-Origin", "*").unwrap(),
                Header::from_bytes("Access-Control-Allow-Methods", "GET, POST, OPTIONS").unwrap(),
                Header::from_bytes("Access-Control-Allow-Headers", "Content-Type").unwrap(),
                Header::from_bytes("Content-Type", "application/json").unwrap(),
            ];

            if request.method() == &Method::Options {
                let mut response = Response::from_string("").with_status_code(204);
                for h in &cors_headers {
                    response.add_header(h.clone());
                }
                let _ = request.respond(response);
                continue;
            }

            let url = request.url().to_string();
            match (request.method(), url.as_str()) {
                (&Method::Get, "/status") => {
                    let body = r#"{"ok":true,"version":"0.1.0"}"#;
                    let mut response = Response::from_string(body);
                    for h in &cors_headers {
                        response.add_header(h.clone());
                    }
                    let _ = request.respond(response);
                }
                (&Method::Get, "/project") => {
                    let state = app_handle.state::<AppState>();
                    let overview = state.overview();
                    let body = format!(r#"{{"ok":true,"path":"{}"}}"#, overview.vault_path.replace('\\', "/"));
                    let mut response = Response::from_string(body);
                    for h in &cors_headers { response.add_header(h.clone()); }
                    let _ = request.respond(response);
                }
                (&Method::Get, "/projects") => {
                    let state = app_handle.state::<AppState>();
                    let overview = state.overview();
                    let path = overview.vault_path.replace('\\', "/");
                    let name = path.split('/').last().unwrap_or("Current Project");
                    let body = format!(r#"{{"ok":true,"projects":[{{"name":"{}","path":"{}","current":true}}]}}"#, name, path);
                    let mut response = Response::from_string(body);
                    for h in &cors_headers { response.add_header(h.clone()); }
                    let _ = request.respond(response);
                }
                (&Method::Post, "/clip") => {
                    let mut body = String::new();
                    if let Err(e) = request.as_reader().read_to_string(&mut body) {
                        let err = format!(r#"{{"ok":false,"error":"Read failed: {}"}}"#, e);
                        let mut response = Response::from_string(err).with_status_code(400);
                        for h in &cors_headers { response.add_header(h.clone()); }
                        let _ = request.respond(response);
                        continue;
                    }

                    match handle_clip(&body, &app_handle) {
                        Ok(path) => {
                            let resp_body = format!(r#"{{"ok":true,"path":"{}"}}"#, path);
                            let mut response = Response::from_string(resp_body).with_status_code(200);
                            for h in &cors_headers { response.add_header(h.clone()); }
                            let _ = request.respond(response);
                        }
                        Err(err) => {
                            let resp_body = format!(r#"{{"ok":false,"error":"{}"}}"#, err);
                            let mut response = Response::from_string(resp_body).with_status_code(500);
                            for h in &cors_headers { response.add_header(h.clone()); }
                            let _ = request.respond(response);
                        }
                    }
                }
                _ => {
                    let mut response = Response::from_string(r#"{"ok":false,"error":"Not found"}"#).with_status_code(404);
                    for h in &cors_headers { response.add_header(h.clone()); }
                    let _ = request.respond(response);
                }
            }
        }
    });
}

fn handle_clip(body: &str, app_handle: &AppHandle) -> Result<String, String> {
    let parsed: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let title = parsed["title"].as_str().unwrap_or("Untitled");
    let url = parsed["url"].as_str().unwrap_or("");
    let content = parsed["content"].as_str().unwrap_or("");

    if content.is_empty() {
        return Err("Content is empty".to_string());
    }

    let state = app_handle.state::<AppState>();
    let overview = state.overview();
    let vault_path_str = overview.vault_path.clone();

    if vault_path_str.is_empty() {
        return Err("No vault is currently open".to_string());
    }

    // 如果 extension 指定了 projectPath，校验它与当前 vault 匹配
    if let Some(requested_path) = parsed["projectPath"].as_str() {
        if !requested_path.is_empty() {
            let norm_requested = requested_path.replace('\\', "/").trim_end_matches('/').to_string();
            let norm_vault = vault_path_str.replace('\\', "/").trim_end_matches('/').to_string();
            if norm_requested != norm_vault {
                return Err(format!(
                    "Requested project '{}' does not match active vault '{}'",
                    norm_requested, norm_vault
                ));
            }
        }
    }

    let vault_path = PathBuf::from(&vault_path_str);
    if !vault_path.exists() {
        return Err(format!("Vault path does not exist: {}", vault_path_str));
    }

    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let date_compact = chrono::Local::now().format("%Y%m%d").to_string();

    // Simplified slug generation
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').chars().take(50).collect::<String>();
    
    let base_name = if slug.is_empty() {
        format!("clip-{}", date_compact)
    } else {
        format!("{}-{}", slug, date_compact)
    };

    let clips_dir = vault_path.join("raw").join("clips");
    if !clips_dir.exists() {
        std::fs::create_dir_all(&clips_dir).map_err(|e| format!("Failed to create clips dir: {}", e))?;
    }

    let mut file_path = clips_dir.join(format!("{}.md", base_name));
    let mut counter = 2;
    while file_path.exists() {
        file_path = clips_dir.join(format!("{}-{}.md", base_name, counter));
        counter += 1;
    }

    let markdown = format!(
        "---\ntype: clip\ntitle: \"{}\"\nurl: \"{}\"\nclipped: {}\norigin: clipper\ntags: [clip]\n---\n\n# {}\n\nSource: {}\n\n{}\n",
        title.replace('"', "\\\""),
        url.replace('"', "\\\""),
        date,
        title,
        url,
        content
    );

    // 防止 slug 构造导致路径越权
    let canonical_clips = clips_dir.canonicalize()
        .unwrap_or_else(|_| clips_dir.clone());
    let canonical_file = file_path.canonicalize()
        .unwrap_or_else(|_| {
            // 文件尚未创建时，规范化父目录后拼接文件名
            canonical_clips.join(format!("{}.md", base_name))
        });
    if !canonical_file.starts_with(&canonical_clips) {
        return Err("Clip path traversal detected: title contains illegal path characters".to_string());
    }

    std::fs::write(&file_path, &markdown).map_err(|e| format!("Failed to write clip file: {}", e))?;

    // Asynchronously ingest
    let app_handle_clone = app_handle.clone();
    let file_path_clone = file_path.clone();
    
    tauri::async_runtime::spawn(async move {
        let state = app_handle_clone.state::<AppState>();
        if let Err(e) = state.ingest_markdown(file_path_clone).await {
            eprintln!("[Clip Server] Ingest failed: {}", e);
        }
    });

    let relative_path = file_path
        .strip_prefix(&vault_path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| file_path.to_string_lossy().to_string());

    Ok(relative_path)
}
