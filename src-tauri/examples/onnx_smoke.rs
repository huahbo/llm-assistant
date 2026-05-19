//! ONNX 全链路 smoke test：验证 ort+DLL+模型+tokenizer+token_type_ids 完整推理。

use std::path::PathBuf;
use std::time::Instant;
use ort::value::Tensor;
use tokenizers::Tokenizer;

fn main() {
    let dll = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/onnxruntime.dll");
    std::env::set_var("ORT_DYLIB_PATH", &dll);
    eprintln!("=== ONNX full pipeline smoke test ===");

    let model_dir = std::env::var("APPDATA")
        .ok()
        .map(|d| PathBuf::from(d).join("com.llmwiki.desktop/embed-models/multilingual-e5-small"))
        .filter(|p| p.join("onnx/model.onnx").exists())
        .or_else(|| {
            let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/embed-models/multilingual-e5-small");
            if p.join("onnx/model.onnx").exists() { Some(p) } else { None }
        })
        .expect("model dir not found");
    eprintln!("model dir: {}", model_dir.display());

    let t = Instant::now();
    let mut session = ort::session::Session::builder().unwrap()
        .commit_from_file(model_dir.join("onnx/model.onnx")).unwrap();
    eprintln!("[{:.3}s] session loaded", t.elapsed().as_secs_f32());

    // 探测 input 名
    let input_names: Vec<String> = session.inputs().iter().map(|o| o.name().to_string()).collect();
    eprintln!("model inputs: {:?}", input_names);
    let needs_token_type = input_names.iter().any(|n| n == "token_type_ids");
    eprintln!("needs token_type_ids: {}", needs_token_type);

    let t = Instant::now();
    let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json")).unwrap();
    eprintln!("[{:.3}s] tokenizer loaded", t.elapsed().as_secs_f32());

    let text = "passage: 这是一段测试文本，用于验证 ONNX embedding 完整推理链路。";
    let encoding = tokenizer.encode(text, true).unwrap();
    let ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
    let mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&x| x as i64).collect();
    let seq_len = ids.len();
    eprintln!("seq_len: {}", seq_len);

    let ids_t = Tensor::<i64>::from_array(([1usize, seq_len], ids)).unwrap();
    let mask_t = Tensor::<i64>::from_array(([1usize, seq_len], mask)).unwrap();

    let mut inputs: Vec<(&'static str, ort::value::Value)> = vec![
        ("input_ids", ids_t.into()),
        ("attention_mask", mask_t.into()),
    ];
    if needs_token_type {
        let zeros: Vec<i64> = vec![0; seq_len];
        let z_t = Tensor::<i64>::from_array(([1usize, seq_len], zeros)).unwrap();
        inputs.push(("token_type_ids", z_t.into()));
    }

    let t = Instant::now();
    let outputs = session.run(inputs).expect("inference failed");
    eprintln!("[{:.3}s] inference OK, outputs={}", t.elapsed().as_secs_f32(), outputs.len());

    let (shape, _data) = outputs[0].try_extract_tensor::<f32>().unwrap();
    eprintln!("output shape: {:?}", &shape[..]);

    eprintln!("=== PASS ===");
}
