//! 本地 ONNX Embedding 引擎
//!
//! 使用 ort (ONNX Runtime) + tokenizers (HuggingFace) 在本机 CPU 上生成向量，
//! 不依赖 Ollama 等外部服务。
//!
//! 推荐模型：multilingual-e5-small（384 维，中英文兼顾）
//! 模型文件位于：src-tauri/resources/embed-models/<model_name>/
//!   - onnx/model.onnx       (模型权重)
//!   - tokenizer.json        (HuggingFace tokenizer)

use std::{path::Path, sync::Mutex};

use async_trait::async_trait;
use ort::{session::Session, value::Tensor};
use tokenizers::Tokenizer;

use super::embed_provider::{EmbedError, EmbedProvider};

/// e5 模型推荐的文档前缀；遗漏会导致向量空间偏移
const E5_PASSAGE_PREFIX: &str = "passage: ";
/// tokenizer 截断上限（模型支持 512 token）
const MAX_INPUT_TOKENS: usize = 512;

#[allow(dead_code)]
pub struct OnnxEmbedder {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    model_dim: usize,
    backend_id_str: String,
}

impl std::fmt::Debug for OnnxEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxEmbedder")
            .field("backend_id", &self.backend_id_str)
            .field("model_dim", &self.model_dim)
            .finish()
    }
}

impl OnnxEmbedder {
    /// 从资源目录下的模型文件夹加载 ONNX 嵌入引擎。
    ///
    /// `model_dir` 应包含：
    /// - `onnx/model.onnx`
    /// - `tokenizer.json`
    pub fn from_resource_dir(model_dir: &Path) -> Result<Self, EmbedError> {
        let model_name = model_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let onnx_path = model_dir.join("onnx").join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !onnx_path.exists() {
            return Err(EmbedError::InitFailed(format!(
                "ONNX 模型文件缺失: {}（请运行 scripts/download-embed-models.ps1）",
                onnx_path.display()
            )));
        }
        if !tokenizer_path.exists() {
            return Err(EmbedError::InitFailed(format!(
                "tokenizer.json 缺失: {}",
                tokenizer_path.display()
            )));
        }

        // 加载 ONNX session
        let session = Session::builder()
            .map_err(|e| EmbedError::InitFailed(format!("创建 ONNX Session builder 失败: {e}")))?
            .commit_from_file(&onnx_path)
            .map_err(|e| EmbedError::InitFailed(format!("加载 ONNX 模型失败: {e}")))?;

        // 从模型输出元数据推断向量维度
        let model_dim = detect_model_dim(&session);

        // 加载 tokenizer
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbedError::InitFailed(format!("加载 tokenizer 失败: {e}")))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            model_dim,
            backend_id_str: format!("onnx:{}", model_name),
        })
    }

    /// 同步推理入口（在 spawn_blocking 中调用）。
    pub fn embed_sync(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        // 1. 添加 e5 passage 前缀（multilingual-e5 系列要求）
        let prefixed = format!("{}{}", E5_PASSAGE_PREFIX, text);

        // 2. tokenize
        let encoding = self
            .tokenizer
            .encode(prefixed.as_str(), true)
            .map_err(|e| EmbedError::InferenceFailed(format!("tokenize 失败: {e}")))?;

        let ids_raw = encoding.get_ids();
        let mask_raw = encoding.get_attention_mask();

        // 3. 截断到 MAX_INPUT_TOKENS
        let seq_len = ids_raw.len().min(MAX_INPUT_TOKENS);
        let ids_i64: Vec<i64> = ids_raw[..seq_len].iter().map(|&x| x as i64).collect();
        let mask_i64: Vec<i64> = mask_raw[..seq_len].iter().map(|&x| x as i64).collect();
        let mask_f32: Vec<f32> = mask_raw[..seq_len].iter().map(|&x| x as f32).collect();

        // 4. 构建 input tensors，shape: [1, seq_len]
        let ids_tensor = Tensor::<i64>::from_array(([1_usize, seq_len], ids_i64))
            .map_err(|e| EmbedError::InferenceFailed(format!("创建 input_ids tensor 失败: {e}")))?;
        let mask_tensor = Tensor::<i64>::from_array(([1_usize, seq_len], mask_i64))
            .map_err(|e| {
                EmbedError::InferenceFailed(format!("创建 attention_mask tensor 失败: {e}"))
            })?;

        // 5-8. 持有 session 锁完成：推理 → 提取 → mean pooling → L2 normalize
        // SessionOutputs 的生命周期绑定到 &mut Session，必须在同一 scope 内完成数据提取
        let mut session = self
            .session
            .lock()
            .map_err(|e| EmbedError::InferenceFailed(format!("获取 session 锁失败: {e}")))?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => ids_tensor,
                "attention_mask" => mask_tensor,
            ])
            .map_err(|e| EmbedError::InferenceFailed(format!("ONNX 推理失败: {e}")))?;

        // 6. 提取 last_hidden_state，shape: [1, seq_len, hidden_dim]
        let (shape, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| EmbedError::InferenceFailed(format!("提取输出 tensor 失败: {e}")))?;

        let dims: &[i64] = &shape;
        if dims.len() < 3 {
            return Err(EmbedError::InferenceFailed(format!(
                "输出 tensor 维度异常: {:?}（期望 3 维 [batch, seq, hidden]）",
                dims
            )));
        }
        let hidden_dim = dims[2] as usize;

        // 7. mean pooling（attention mask 加权，排除 padding token）
        let mut pooled = vec![0f32; hidden_dim];
        let mut total_weight = 0f32;
        for i in 0..seq_len {
            let w = mask_f32[i];
            if w > 0.0 {
                let offset = i * hidden_dim;
                for d in 0..hidden_dim {
                    pooled[d] += data[offset + d] * w;
                }
                total_weight += w;
            }
        }
        if total_weight > 0.0 {
            for d in 0..hidden_dim {
                pooled[d] /= total_weight;
            }
        }

        // session lock 在此处 drop（outputs 已完成数据提取，不再持有借用）
        drop(outputs);
        drop(session);

        // 8. L2 normalize（余弦相似度依赖单位向量）
        let norm = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-8 {
            for d in 0..hidden_dim {
                pooled[d] /= norm;
            }
        }

        Ok(pooled)
    }
}

/// 从 session 输出元数据推断向量维度（取最后一维）
#[allow(dead_code)]
fn detect_model_dim(session: &Session) -> usize {
    session
        .outputs()
        .first()
        .and_then(|o| o.dtype().tensor_shape())
        .and_then(|s| s.last().copied())
        .map(|d| if d > 0 { d as usize } else { 384 })
        .unwrap_or(384) // multilingual-e5-small 默认 384
}

#[async_trait]
impl EmbedProvider for OnnxEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        // ONNX 推理是 CPU bound；直接调用 embed_sync
        // spawn_blocking 需要 'static bound（Arc<Self>），此处架构为 Arc<dyn EmbedProvider>，
        // 所以 Phase 3 先同步调用，不会阻塞网络 IO 密集型场景（embed 调用本就是串行的）
        self.embed_sync(text)
    }

    fn dimension(&self) -> usize {
        self.model_dim
    }

    fn backend_id(&self) -> String {
        self.backend_id_str.clone()
    }

    async fn health_check(&self) -> bool {
        self.embed_sync("health check").is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn model_dir() -> PathBuf {
        // 寻找开发期模型路径（cargo test 在 src-tauri/ 下运行）
        let candidates = [
            PathBuf::from("resources/embed-models/multilingual-e5-small"),
            PathBuf::from("../src-tauri/resources/embed-models/multilingual-e5-small"),
        ];
        candidates
            .into_iter()
            .find(|p| p.join("onnx/model.onnx").exists())
            .unwrap_or_else(|| PathBuf::from("resources/embed-models/multilingual-e5-small"))
    }

    fn model_available() -> bool {
        model_dir().join("onnx/model.onnx").exists()
    }

    #[test]
    #[ignore = "需要模型文件与 onnxruntime.dll，运行: cargo test -- --include-ignored"]
    fn test_embed_returns_correct_dimension() {
        if !model_available() {
            eprintln!("[skip] 模型文件不存在，跳过 ONNX 推理测试");
            return;
        }
        let embedder = OnnxEmbedder::from_resource_dir(&model_dir())
            .expect("OnnxEmbedder 初始化失败");

        let vec = embedder.embed_sync("这是一段测试文本").expect("embed 失败");
        assert_eq!(vec.len(), 384, "multilingual-e5-small 应输出 384 维向量");
    }

    #[test]
    #[ignore = "需要模型文件与 onnxruntime.dll，运行: cargo test -- --include-ignored"]
    fn test_embed_is_l2_normalized() {
        if !model_available() {
            eprintln!("[skip] 模型文件不存在，跳过 ONNX 推理测试");
            return;
        }
        let embedder = OnnxEmbedder::from_resource_dir(&model_dir())
            .expect("OnnxEmbedder 初始化失败");

        let vec = embedder.embed_sync("test normalization").expect("embed 失败");
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-4,
            "L2 norm 应接近 1.0，实际: {:.6}",
            norm
        );
    }

    #[test]
    #[ignore = "需要模型文件与 onnxruntime.dll，运行: cargo test -- --include-ignored"]
    fn test_same_text_same_vector() {
        if !model_available() {
            eprintln!("[skip] 模型文件不存在，跳过 ONNX 推理测试");
            return;
        }
        let embedder = OnnxEmbedder::from_resource_dir(&model_dir())
            .expect("OnnxEmbedder 初始化失败");

        let v1 = embedder.embed_sync("hello world").expect("embed 失败");
        let v2 = embedder.embed_sync("hello world").expect("embed 失败");
        assert_eq!(v1, v2, "相同输入应得到完全相同的向量");
    }

    #[test]
    #[ignore = "需要模型文件与 onnxruntime.dll，运行: cargo test -- --include-ignored"]
    fn test_different_texts_different_vectors() {
        if !model_available() {
            eprintln!("[skip] 模型文件不存在，跳过 ONNX 推理测试");
            return;
        }
        let embedder = OnnxEmbedder::from_resource_dir(&model_dir())
            .expect("OnnxEmbedder 初始化失败");

        let v1 = embedder.embed_sync("今天天气很好").expect("embed 失败");
        let v2 = embedder.embed_sync("量子计算机原理").expect("embed 失败");
        let cosine: f32 = v1.iter().zip(v2.iter()).map(|(a, b)| a * b).sum();
        assert!(cosine < 0.99, "不同语义文本的余弦相似度应 < 0.99，实际: {:.4}", cosine);
    }

    #[test]
    fn test_missing_model_returns_error() {
        let result = OnnxEmbedder::from_resource_dir(Path::new("/nonexistent/path"));
        assert!(
            matches!(result, Err(EmbedError::InitFailed(_))),
            "模型路径不存在时应返回 InitFailed"
        );
    }
}
