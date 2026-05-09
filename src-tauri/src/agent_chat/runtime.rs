//! agent_chat ReAct 主循环
//!
//! 持久化消息、调用 LLM chat_stream、分发工具调用、emit Tauri 事件。

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::agent_chat::{db as chat_db, tools};
use crate::llm::types::{ChatMessage, FinishReason, StreamEvent};
use crate::state::{current_timestamp_ms, AppState};

/// 取消令牌（AtomicBool 为 true 时中止当前循环）
pub type CancelToken = Arc<std::sync::atomic::AtomicBool>;

pub fn new_cancel_token() -> CancelToken {
    Arc::new(std::sync::atomic::AtomicBool::new(false))
}

/// 处理一轮用户消息：持久化 → ReAct 循环 → emit 事件
///
/// 返回最终 assistant message_id。
pub async fn process_message_turn(
    state: &AppState,
    app_handle: &AppHandle,
    conv_id: i64,
    user_message: String,
    cancel_token: CancelToken,
) -> Result<i64, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化，无法处理消息".to_string())?;

    let provider = state
        .get_llm_provider()
        .ok_or_else(|| "LLM Provider 未就绪".to_string())?;

    // 1. 持久化 user message
    let now = current_timestamp_ms();
    chat_db::append_message(&db_path, conv_id, "user", Some(&user_message), None, None, None, &now)?;

    let max_iter: u32 = 8;
    let mut iter: u32 = 0;
    let mut last_message_id: i64 = 0;

    loop {
        // 取消检查
        if cancel_token.load(Ordering::Relaxed) {
            break;
        }

        // a. 加载会话 + 消息
        let conv = chat_db::get_conversation(&db_path, conv_id)?
            .ok_or_else(|| format!("会话 #{conv_id} 不存在"))?;
        let db_messages = chat_db::list_messages(&db_path, conv_id)?;

        // b. 构建 LLM messages 数组
        let mut llm_messages: Vec<ChatMessage> = Vec::new();
        if let Some(sp) = &conv.system_prompt {
            if !sp.is_empty() {
                llm_messages.push(ChatMessage::system(sp));
            }
        }
        for msg in &db_messages {
            llm_messages.push(db_msg_to_chat_msg(msg)?);
        }

        // c. 加载启用的工具
        let tools = chat_db::list_enabled_tools(&db_path)?;

        // 预插入 assistant 占位消息，获取 message_id
        let now2 = current_timestamp_ms();
        let message_id = chat_db::append_message(
            &db_path, conv_id, "assistant",
            Some(""),   // 占位，流结束后更新
            None, None, None, &now2,
        )?;
        last_message_id = message_id;

        // 流式事件状态（工具调用 idx → (id, name, args)）
        let mut partial_calls: HashMap<u32, (String, String, String)> = HashMap::new();
        let app = app_handle.clone();

        // d. 调用 chat_stream
        let mut on_event = |event: StreamEvent| {
            match event {
                StreamEvent::TextDelta(chunk) => {
                    let _ = app.emit(
                        "chat_text_delta",
                        json!({
                            "conversation_id": conv_id,
                            "message_id": message_id,
                            "chunk": chunk,
                        }),
                    );
                }
                StreamEvent::ToolCallStart { idx, id, name } => {
                    partial_calls.insert(idx, (id, name, String::new()));
                }
                StreamEvent::ToolCallArgsDelta { idx, args_chunk } => {
                    if let Some((_, _, args)) = partial_calls.get_mut(&idx) {
                        args.push_str(&args_chunk);
                    }
                }
                StreamEvent::ToolCallEnd { idx } => {
                    if let Some((id, name, args)) = partial_calls.get(&idx) {
                        let _ = app.emit(
                            "chat_tool_call",
                            json!({
                                "conversation_id": conv_id,
                                "message_id": message_id,
                                "call_id": id.clone(),
                                "tool_name": name.clone(),
                                "args": args.clone(),
                                "idx": idx,
                            }),
                        );
                    }
                }
                StreamEvent::FinishReason(_) => {}
            }
        };

        let completion = provider
            .chat_stream(&llm_messages, &tools, &mut on_event)
            .await
            .map_err(|e| format!("LLM 调用失败: {e}"))?;

        // e. 更新 assistant 消息
        let tool_calls_json = if completion.tool_calls.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&completion.tool_calls)
                    .unwrap_or_else(|_| "[]".to_string()),
            )
        };
        let content_opt = if completion.content.is_empty() {
            None
        } else {
            Some(completion.content.as_str())
        };
        chat_db::update_message_after_stream(&db_path, message_id, content_opt, tool_calls_json.as_deref())?;

        // f. 根据 finish_reason 决定后续
        match &completion.finish_reason {
            FinishReason::Stop | FinishReason::Other(_) => {
                let _ = app_handle.emit(
                    "chat_message_done",
                    json!({ "conversation_id": conv_id, "message_id": message_id }),
                );
                break;
            }

            FinishReason::ToolCalls => {
                for call in &completion.tool_calls {
                    if cancel_token.load(Ordering::Relaxed) {
                        break;
                    }
                    let result = tools::execute_tool_call(state, conv_id, call).await?;

                    if let Some(pending_id) = result.awaiting_approval {
                        let _ = app_handle.emit(
                            "chat_awaiting_approval",
                            json!({
                                "conversation_id": conv_id,
                                "message_id": message_id,
                                "call_id": call.id,
                                "pending_id": pending_id,
                            }),
                        );
                        return Ok(message_id);
                    }

                    // 持久化工具结果
                    let now3 = current_timestamp_ms();
                    chat_db::append_message(
                        &db_path, conv_id, "tool",
                        Some(&result.content),
                        None,
                        Some(&result.call_id),
                        Some(&call.function.name),
                        &now3,
                    )?;

                    let _ = app_handle.emit(
                        "chat_tool_result",
                        json!({
                            "conversation_id": conv_id,
                            "message_id": message_id,
                            "call_id": result.call_id,
                            "ok": true,
                            "content_preview": result.display_preview,
                            "latency_ms": result.latency_ms,
                        }),
                    );
                }

                iter += 1;
                if iter >= max_iter {
                    let _ = app_handle.emit(
                        "chat_message_done",
                        json!({
                            "conversation_id": conv_id,
                            "message_id": message_id,
                            "note": "max iterations reached",
                        }),
                    );
                    break;
                }
                continue;
            }

            FinishReason::Length => {
                let _ = app_handle.emit(
                    "chat_message_done",
                    json!({
                        "conversation_id": conv_id,
                        "message_id": message_id,
                        "note": "truncated",
                    }),
                );
                break;
            }
        }
    }

    Ok(last_message_id)
}

// ── DB Message → LLM ChatMessage conversion ───────────────────────────────────

fn db_msg_to_chat_msg(msg: &crate::agent_chat::db::Message) -> Result<ChatMessage, String> {
    let tool_calls = if let Some(json_str) = &msg.tool_calls_json {
        let calls: Vec<crate::llm::types::ToolCall> = serde_json::from_str(json_str)
            .map_err(|e| format!("解析 tool_calls_json 失败: {e}"))?;
        Some(calls)
    } else {
        None
    };

    Ok(ChatMessage {
        role: msg.role.clone(),
        content: msg.content.clone(),
        tool_calls,
        tool_call_id: msg.tool_call_id.clone(),
        name: msg.tool_name.clone(),
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_chat::db::Message;

    fn make_msg(role: &str, content: Option<&str>, tool_calls_json: Option<&str>) -> Message {
        Message {
            id: 1,
            conversation_id: 1,
            role: role.to_string(),
            content: content.map(str::to_string),
            tool_calls_json: tool_calls_json.map(str::to_string),
            tool_call_id: None,
            tool_name: None,
            sequence: 1,
            created_at: "t".to_string(),
        }
    }

    #[test]
    fn test_db_msg_to_chat_msg_user() {
        let msg = make_msg("user", Some("hello"), None);
        let cm = db_msg_to_chat_msg(&msg).unwrap();
        assert_eq!(cm.role, "user");
        assert_eq!(cm.content.as_deref(), Some("hello"));
        assert!(cm.tool_calls.is_none());
    }

    #[test]
    fn test_db_msg_to_chat_msg_tool_calls_roundtrip() {
        let tc_json = r#"[{"id":"c1","type":"function","function":{"name":"run_shell","arguments":"{\"command\":\"ls\"}"}}]"#;
        let msg = make_msg("assistant", None, Some(tc_json));
        let cm = db_msg_to_chat_msg(&msg).unwrap();
        assert_eq!(cm.role, "assistant");
        let calls = cm.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "run_shell");
    }

    #[test]
    fn test_db_msg_to_chat_msg_invalid_tool_calls_json() {
        let msg = make_msg("assistant", None, Some("not valid json"));
        assert!(db_msg_to_chat_msg(&msg).is_err());
    }

    #[test]
    fn test_new_cancel_token_starts_false() {
        let token = new_cancel_token();
        assert!(!token.load(Ordering::Relaxed));
        token.store(true, Ordering::Relaxed);
        assert!(token.load(Ordering::Relaxed));
    }
}
