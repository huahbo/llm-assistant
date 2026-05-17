//! Embedding 专用 Provider trait
//!
//! 比 LlmProvider 接口更窄，仅关注向量化能力。
//! OllamaProvider 和 OnnxEmbedder 都实现此 trait。

use async_trait::async_trait;
use serde::Serialize;

/// Embedding 专用错误类型
#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub enum EmbedError {
    /// 初始化/连接失败（模型文件缺失、Ollama 未启动等）
    InitFailed(String),
    /// 推理失败（运行时错误、格式解析失败等）
    InferenceFailed(String),
    /// 输入超出模型 token 上限
    InputTooLong { tokens: usize, limit: usize },
    /// 后端已被用户禁用
    Unavailable,
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbedError::InitFailed(m) => write!(f, "Embed 初始化失败: {}", m),
            EmbedError::InferenceFailed(m) => write!(f, "Embed 推理失败: {}", m),
            EmbedError::InputTooLong { tokens, limit } => {
                write!(f, "输入过长: {} tokens（上限 {}）", tokens, limit)
            }
            EmbedError::Unavailable => write!(f, "Embed 服务不可用"),
        }
    }
}

impl std::error::Error for EmbedError {}

/// Embedding Provider 抽象接口
#[async_trait]
#[allow(dead_code)]
pub trait EmbedProvider: Send + Sync {
    /// 生成单条文本向量（长度 == dimension()）
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError>;

    /// 向量维度（用于 schema 校验与迁移检测）
    fn dimension(&self) -> usize;

    /// 后端标识，用于日志与 Settings 显示
    /// 例如 "onnx:multilingual-e5-small" / "ollama:nomic-embed-text:latest"
    fn backend_id(&self) -> String;

    /// 健康检查（启动时探测，失败不 panic）
    async fn health_check(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_error_display() {
        assert!(EmbedError::InitFailed("test".into()).to_string().contains("初始化失败"));
        assert!(EmbedError::InferenceFailed("x".into()).to_string().contains("推理失败"));
        assert!(EmbedError::InputTooLong { tokens: 600, limit: 512 }
            .to_string()
            .contains("600"));
        assert_eq!(EmbedError::Unavailable.to_string(), "Embed 服务不可用");
    }
}
