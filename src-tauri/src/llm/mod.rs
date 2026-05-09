//! LLM Provider 模块
//!
//! 本模块提供与大语言模型（LLM）服务交互的抽象层，
//! 支持多种 LLM 后端的统一接口。
//!
//! # 模块结构
//! - `provider`: 定义 LlmProvider trait 和错误类型
//! - `ollama`: Ollama 本地 LLM 服务实现
//! - `openai`: OpenAI 云端 LLM 服务实现（Hybrid 模式）

pub mod ollama;
pub mod openai;
pub mod provider;
pub mod stream_parser;
pub mod types;

// 重导出常用类型，方便外部使用
pub use ollama::{OllamaConfig, OllamaProvider};
pub use openai::{OpenAiConfig, OpenAiProvider, DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL};
pub use provider::{LlmError, LlmProvider};
pub use types::{
    ChatCompletion, ChatMessage, FinishReason, StreamEvent, ToolCall, ToolCallFunction, ToolSchema,
};
