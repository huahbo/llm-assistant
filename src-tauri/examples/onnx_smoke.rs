//! ONNX 加载冒烟测试 — 验证 ort + onnxruntime.dll + 模型文件配套是否正常。
//!
//! 用法：
//!   cargo run --no-default-features --example onnx_smoke
//!
//! 关键背景：ort 2.0.0-rc.12 默认启用 api-24，但官方 ONNX Runtime 1.20.x DLL
//! 只支持到 API 版本 20。版本不匹配时 `ort::api()` 会在 OnceLock 死锁。
//! 修复：Cargo.toml 中设 `default-features = false, features = ["api-20", ...]`。

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let dll = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/onnxruntime.dll");
    std::env::set_var("ORT_DYLIB_PATH", &dll);
    eprintln!("=== ONNX runtime smoke test ===");
    eprintln!("DLL: {}", dll.display());

    let t = Instant::now();
    let _api = ort::api();
    eprintln!("[{:.3}s] ort::api() OK", t.elapsed().as_secs_f32());

    let t = Instant::now();
    ort::environment::Environment::current().expect("Environment::current FAIL");
    eprintln!("[{:.3}s] Environment::current OK", t.elapsed().as_secs_f32());

    let model_path = std::env::var("APPDATA")
        .ok()
        .map(|d| PathBuf::from(d).join("com.llmwiki.desktop/embed-models/multilingual-e5-small/onnx/model.onnx"))
        .filter(|p| p.exists())
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/embed-models/multilingual-e5-small/onnx/model.onnx");
            if p.exists() { Some(p) } else { None }
        })
        .expect("model.onnx not found in AppData or dev path");
    eprintln!("model: {}", model_path.display());

    let t = Instant::now();
    let _session = ort::session::Session::builder()
        .and_then(|mut b| b.commit_from_file(&model_path))
        .expect("Session creation FAIL");
    eprintln!("[{:.3}s] Session OK", t.elapsed().as_secs_f32());

    eprintln!("=== PASS ===");
}
