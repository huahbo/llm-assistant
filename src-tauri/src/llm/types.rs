//! H8 Agent Chat 类型定义
//!
//! 供 ReAct agent 循环、OpenAI function_calling 流式解析使用的共享类型。

use serde::{Deserialize, Serialize};

/// 多轮对话消息（兼容 OpenAI messages 数组格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// "system" | "user" | "assistant" | "tool"
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// tool 角色消息对应的 tool_call_id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// tool 角色消息对应的函数名（部分 provider 要求）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// DeepSeek thinking 模式下的推理内容，必须原样回传给 API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[allow(dead_code)]
impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }
    }

    pub fn assistant_with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: "assistant".into(),
            content: None,
            tool_calls: Some(tool_calls),
            tool_call_id: None,
            name: None,
            reasoning_content: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: Some(content.into()),
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: Some(name.into()),
            reasoning_content: None,
        }
    }
}

/// OpenAI function_calling tool call 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    /// 始终为 "function"
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolCallFunction,
}

/// tool call 函数信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    /// JSON 字符串（流式积累完整后 parse）
    pub arguments: String,
}

/// 工具 schema（对应 OpenAI tools 数组中的 function 描述）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    /// JSON Schema 对象，描述 parameters
    pub parameters: serde_json::Value,
}

/// 流式事件枚举，供 on_event 回调分发
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum StreamEvent {
    /// 普通文本增量
    TextDelta(String),
    /// 某个 tool call 开始（idx 为 choices[0].delta.tool_calls 数组下标）
    ToolCallStart { idx: u32, id: String, name: String },
    /// 某个 tool call 的 arguments 增量
    ToolCallArgsDelta { idx: u32, args_chunk: String },
    /// 某个 tool call 流结束（args 已完整）
    ToolCallEnd { idx: u32 },
    /// 本次生成完成原因
    FinishReason(FinishReason),
}

/// 完成原因
#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    Other(String),
}

impl From<&str> for FinishReason {
    fn from(s: &str) -> Self {
        match s {
            "stop" => FinishReason::Stop,
            "tool_calls" => FinishReason::ToolCalls,
            "length" => FinishReason::Length,
            other => FinishReason::Other(other.to_string()),
        }
    }
}

/// 一次 chat_stream 的完整返回值
#[derive(Debug, Clone)]
pub struct ChatCompletion {
    /// 累积的文本内容（非 tool_calls 时有值）
    pub content: String,
    /// 累积的 tool calls（finish_reason == ToolCalls 时有值）
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    /// DeepSeek thinking 模式下的推理内容，需回传给 API
    pub reasoning_content: Option<String>,
}

#[allow(dead_code)]
impl ChatCompletion {
    pub fn text(content: String) -> Self {
        Self {
            content,
            tool_calls: vec![],
            finish_reason: FinishReason::Stop,
            reasoning_content: None,
        }
    }

    pub fn with_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            content: String::new(),
            tool_calls,
            finish_reason: FinishReason::ToolCalls,
            reasoning_content: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_reason_from_str_known_values() {
        assert_eq!(FinishReason::from("stop"), FinishReason::Stop);
        assert_eq!(FinishReason::from("tool_calls"), FinishReason::ToolCalls);
        assert_eq!(FinishReason::from("length"), FinishReason::Length);
    }

    #[test]
    fn finish_reason_from_str_unknown_falls_back() {
        assert_eq!(
            FinishReason::from("content_filter"),
            FinishReason::Other("content_filter".to_string()),
        );
    }

    #[test]
    fn chat_message_builders_set_correct_roles() {
        assert_eq!(ChatMessage::system("sys").role, "system");
        assert_eq!(ChatMessage::user("u").role, "user");
        assert_eq!(ChatMessage::assistant("a").role, "assistant");
    }

    #[test]
    fn chat_message_tool_result_has_all_fields() {
        let m = ChatMessage::tool_result("cid1", "my_tool", "result body");
        assert_eq!(m.role, "tool");
        assert_eq!(m.tool_call_id.as_deref(), Some("cid1"));
        assert_eq!(m.name.as_deref(), Some("my_tool"));
        assert_eq!(m.content.as_deref(), Some("result body"));
    }

    #[test]
    fn chat_completion_with_tool_calls_has_empty_content() {
        let tc = ToolCall {
            id: "id1".into(),
            kind: "function".into(),
            function: ToolCallFunction { name: "foo".into(), arguments: "{}".into() },
        };
        let c = ChatCompletion::with_tool_calls(vec![tc]);
        assert!(c.content.is_empty());
        assert_eq!(c.finish_reason, FinishReason::ToolCalls);
        assert_eq!(c.tool_calls.len(), 1);
    }
}
