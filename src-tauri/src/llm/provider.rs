//! LLM Provider trait 定义
//!
//! 本模块定义了与 LLM 服务交互的抽象接口，
//! 支持多种 LLM 后端（如 Ollama、OpenAI 等）的统一调用。

use async_trait::async_trait;
use serde::Serialize;

use super::types::{ChatCompletion, ChatMessage, StreamEvent, ToolSchema};

/// LLM 服务错误类型
///
/// 封装与 LLM 服务交互过程中可能出现的各种错误情况。
#[derive(Debug, Clone, Serialize)]
pub enum LlmError {
    /// 连接失败，包含错误详情
    ConnectionFailed(String),
    /// 请求超时
    Timeout,
    /// 指定的模型不存在
    ModelNotFound(String),
    /// 响应格式无效或解析失败
    InvalidResponse(String),
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::ConnectionFailed(msg) => write!(f, "连接失败: {}", msg),
            LlmError::Timeout => write!(f, "请求超时"),
            LlmError::ModelNotFound(model) => write!(f, "模型不存在: {}", model),
            LlmError::InvalidResponse(msg) => write!(f, "无效响应: {}", msg),
        }
    }
}

impl std::error::Error for LlmError {}

/// LLM Provider 抽象接口
///
/// 所有 LLM 后端实现都需要实现此 trait，提供统一的调用方式。
/// 使用 `async_trait` 支持异步方法。
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// 生成文本摘要
    ///
    /// # 参数
    /// - `content`: 需要摘要的原始内容
    /// - `max_tokens`: 摘要的最大 token 数量
    ///
    /// # 返回
    /// - `Ok(String)`: 生成的摘要文本
    /// - `Err(LlmError)`: 生成失败时的错误信息
    async fn summarize(&self, content: &str, max_tokens: usize) -> Result<String, LlmError>;

    /// 通用文本补全
    ///
    /// # 参数
    /// - `prompt`: 输入的提示文本
    ///
    /// # 返回
    /// - `Ok(String)`: LLM 生成的补全文本
    /// - `Err(LlmError)`: 生成失败时的错误信息
    async fn complete(&self, prompt: &str) -> Result<String, LlmError>;

    /// 流式文本补全（阶段2：真流式能力）
    ///
    /// 默认实现会回退到 `complete` 并一次性回调。
    /// 支持流式的 Provider 应覆盖此方法并在收到增量时触发 `on_chunk`。
    async fn complete_stream(
        &self,
        prompt: &str,
        on_chunk: &mut (dyn FnMut(String) + Send),
    ) -> Result<String, LlmError> {
        let answer = self.complete(prompt).await?;
        if !answer.is_empty() {
            on_chunk(answer.clone());
        }
        Ok(answer)
    }

    /// 生成文本嵌入向量
    ///
    /// # 参数
    /// - `text`: 需要转换为向量的文本
    ///
    /// # 返回
    /// - `Ok(Vec<f32>)`: 生成的嵌入向量数组
    /// - `Err(LlmError)`: 生成失败时的错误信息
    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError>;

    /// 检查服务是否可用
    ///
    /// 用于在调用前验证 LLM 服务的连通性和可用性。
    ///
    /// # 返回
    /// - `Ok(true)`: 服务可用
    /// - `Ok(false)`: 服务不可用
    /// - `Err(LlmError)`: 检查过程中出错
    async fn health_check(&self) -> Result<bool, LlmError>;

    /// 多轮对话流式补全（H8 ReAct agent 核心接口）
    ///
    /// 支持 OpenAI function_calling 协议，通过 `on_event` 回调分发流式事件。
    /// 不支持工具调用的 Provider（如 Ollama）可提供降级实现。
    async fn chat_stream(
        &self,
        _messages: &[ChatMessage],
        _tools: &[ToolSchema],
        _on_event: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ChatCompletion, LlmError> {
        Err(LlmError::InvalidResponse(
            "此 Provider 尚未实现 chat_stream".to_string(),
        ))
    }

    /// 是否原生支持 function_calling / tool use
    #[allow(dead_code)]
    fn supports_tools(&self) -> bool {
        false
    }

    /// 获取服务的基础地址（用于状态显示）
    fn base_url(&self) -> &str {
        ""
    }

    /// 获取使用的模型名称（用于状态显示）
    fn model(&self) -> &str {
        "Unknown"
    }
}
