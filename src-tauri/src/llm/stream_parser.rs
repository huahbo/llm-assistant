//! OpenAI SSE 流式响应累积器
//!
//! 将 `data: {...}` 行解析为 StreamEvent 并在流结束时构建 ChatCompletion。

use serde::Deserialize;
use std::collections::HashMap;

use super::types::{ChatCompletion, FinishReason, StreamEvent, ToolCall, ToolCallFunction};

// ── Wire types for parsing SSE deltas ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct StreamToolCallFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamToolCallDelta {
    index: u32,
    id: Option<String>,
    #[serde(default)]
    function: Option<StreamToolCallFunctionDelta>,
}

#[derive(Debug, Deserialize, Default)]
struct StreamDelta {
    content: Option<String>,
    reasoning_content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<StreamToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    #[serde(default)]
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

// ── Accumulator ────────────────────────────────────────────────────────────────

struct ToolCallState {
    id: String,
    name: String,
    args: String,
}

/// 累积 OpenAI SSE deltas，分发 StreamEvent，完成后构建 ChatCompletion
pub struct StreamAccumulator {
    content: String,
    reasoning_content: String,
    tool_call_states: HashMap<u32, ToolCallState>,
    /// 按首次出现顺序保存 idx，保证输出顺序
    seen_indices: Vec<u32>,
    finish_reason: Option<FinishReason>,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            reasoning_content: String::new(),
            tool_call_states: HashMap::new(),
            seen_indices: Vec::new(),
            finish_reason: None,
        }
    }

    /// 处理一条 SSE data 行（`data:` 前缀已去除，`[DONE]` 已过滤）
    ///
    /// 返回本行产生的 StreamEvent 列表，调用者负责 emit。
    pub fn process_line(&mut self, json_payload: &str) -> Result<Vec<StreamEvent>, String> {
        let chunk: StreamChunk = serde_json::from_str(json_payload).map_err(|e| {
            format!(
                "解析流式块失败: {} (payload 前100字符: {})",
                e,
                json_payload.chars().take(100).collect::<String>()
            )
        })?;

        let mut events = Vec::new();

        for choice in chunk.choices {
            let delta = choice.delta;

            // 推理内容增量（DeepSeek thinking 模式，静默累积，不 emit 事件）
            if let Some(rc) = delta.reasoning_content {
                if !rc.is_empty() {
                    self.reasoning_content.push_str(&rc);
                }
            }

            // 文本增量
            if let Some(content) = delta.content {
                if !content.is_empty() {
                    self.content.push_str(&content);
                    events.push(StreamEvent::TextDelta(content));
                }
            }

            // 工具调用增量
            for tc in delta.tool_calls {
                let idx = tc.index;

                if !self.tool_call_states.contains_key(&idx) {
                    // 首次出现：提取 id 和函数名
                    let id = tc.id.unwrap_or_default();
                    let name = tc
                        .function
                        .as_ref()
                        .and_then(|f| f.name.clone())
                        .unwrap_or_default();
                    self.seen_indices.push(idx);
                    self.tool_call_states.insert(
                        idx,
                        ToolCallState {
                            id: id.clone(),
                            name: name.clone(),
                            args: String::new(),
                        },
                    );
                    events.push(StreamEvent::ToolCallStart { idx, id, name });
                }

                // arguments 增量
                if let Some(func) = tc.function {
                    if let Some(args_chunk) = func.arguments {
                        if !args_chunk.is_empty() {
                            if let Some(state) = self.tool_call_states.get_mut(&idx) {
                                state.args.push_str(&args_chunk);
                            }
                            events.push(StreamEvent::ToolCallArgsDelta { idx, args_chunk });
                        }
                    }
                }
            }

            // 完成原因
            if let Some(reason_str) = choice.finish_reason {
                let reason = FinishReason::from(reason_str.as_str());
                // 为所有已开始的 tool call 发出 End 事件
                let mut sorted = self.seen_indices.clone();
                sorted.sort_unstable();
                for idx in sorted {
                    events.push(StreamEvent::ToolCallEnd { idx });
                }
                self.finish_reason = Some(reason.clone());
                events.push(StreamEvent::FinishReason(reason));
            }
        }

        Ok(events)
    }

    /// 将累积状态构建为最终 ChatCompletion
    pub fn into_completion(self) -> ChatCompletion {
        let finish_reason = self.finish_reason.unwrap_or(FinishReason::Stop);

        let tool_calls: Vec<ToolCall> = self
            .seen_indices
            .iter()
            .filter_map(|idx| {
                self.tool_call_states.get(idx).map(|s| ToolCall {
                    id: s.id.clone(),
                    kind: "function".to_string(),
                    function: ToolCallFunction {
                        name: s.name.clone(),
                        arguments: s.args.clone(),
                    },
                })
            })
            .collect();

        ChatCompletion {
            content: self.content,
            tool_calls,
            finish_reason,
            reasoning_content: if self.reasoning_content.is_empty() {
                None
            } else {
                Some(self.reasoning_content)
            },
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(acc: &mut StreamAccumulator, lines: &[&str]) -> Vec<StreamEvent> {
        let mut events = Vec::new();
        for line in lines {
            events.extend(acc.process_line(line).expect("parse error"));
        }
        events
    }

    #[test]
    fn test_pure_text_stream() {
        let mut acc = StreamAccumulator::new();
        let lines = [
            r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":", "},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"world"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"!"},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#,
        ];
        let events = collect(&mut acc, &lines);

        let text_deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| {
                if let StreamEvent::TextDelta(s) = e {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text_deltas, ["Hello", ", ", "world", "!"]);

        let completion = acc.into_completion();
        assert_eq!(completion.content, "Hello, world!");
        assert_eq!(completion.finish_reason, FinishReason::Stop);
        assert!(completion.tool_calls.is_empty());
    }

    #[test]
    fn test_single_tool_call_chunked_args() {
        let mut acc = StreamAccumulator::new();
        let lines = [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_001","type":"function","function":{"name":"shell_exec","arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"cm"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"d\":"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"ls\"}"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        ];
        let events = collect(&mut acc, &lines);

        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolCallStart { idx: 0, .. })));

        let args_deltas: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, StreamEvent::ToolCallArgsDelta { .. }))
            .collect();
        assert_eq!(args_deltas.len(), 3);

        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolCallEnd { idx: 0 })));
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::FinishReason(FinishReason::ToolCalls))));

        let completion = acc.into_completion();
        assert_eq!(completion.tool_calls.len(), 1);
        assert_eq!(completion.tool_calls[0].function.name, "shell_exec");
        assert_eq!(completion.tool_calls[0].function.arguments, r#"{"cmd":"ls"}"#);
        assert_eq!(completion.finish_reason, FinishReason::ToolCalls);
    }

    #[test]
    fn test_text_and_tool_call_mixed() {
        let mut acc = StreamAccumulator::new();
        let lines = [
            r#"{"choices":[{"delta":{"content":"Thinking..."},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_002","type":"function","function":{"name":"get_date","arguments":"{}"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        ];
        let events = collect(&mut acc, &lines);

        assert!(events.iter().any(|e| matches!(e, StreamEvent::TextDelta(_))));
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::ToolCallStart { .. })));

        let completion = acc.into_completion();
        assert_eq!(completion.content, "Thinking...");
        assert_eq!(completion.tool_calls.len(), 1);
        assert_eq!(completion.tool_calls[0].function.name, "get_date");
    }

    #[test]
    fn test_multiple_tool_calls() {
        let mut acc = StreamAccumulator::new();
        let lines = [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"tool_a","arguments":""}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"x\":1}"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_b","type":"function","function":{"name":"tool_b","arguments":"{\"y\":2}"}}]},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
        ];
        let events = collect(&mut acc, &lines);

        let starts: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, StreamEvent::ToolCallStart { .. }))
            .collect();
        assert_eq!(starts.len(), 2);

        let ends: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, StreamEvent::ToolCallEnd { .. }))
            .collect();
        assert_eq!(ends.len(), 2);

        let completion = acc.into_completion();
        assert_eq!(completion.tool_calls.len(), 2);
        assert_eq!(completion.tool_calls[0].function.name, "tool_a");
        assert_eq!(completion.tool_calls[1].function.name, "tool_b");
    }

    #[test]
    fn test_finish_reason_length() {
        let mut acc = StreamAccumulator::new();
        let lines = [
            r#"{"choices":[{"delta":{"content":"truncated text..."},"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#,
        ];
        let events = collect(&mut acc, &lines);

        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::FinishReason(FinishReason::Length))));

        let completion = acc.into_completion();
        assert_eq!(completion.finish_reason, FinishReason::Length);
        assert_eq!(completion.content, "truncated text...");
    }
}
