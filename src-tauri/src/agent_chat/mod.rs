//! agent_chat 模块：多轮对话 agent，OpenAI function_calling 协议

pub mod commands;
pub mod db;
pub mod mcp;
pub mod runtime;
pub mod tools;

pub use db::{Conversation, Message};
