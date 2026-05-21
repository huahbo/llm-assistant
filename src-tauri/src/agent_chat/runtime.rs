//! agent_chat ReAct 主循环
//!
//! 持久化消息、调用 LLM chat_stream、分发工具调用、emit Tauri 事件。

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use std::path::PathBuf;

use crate::agent_chat::{db as chat_db, tools};
use crate::llm::{types::{ChatMessage, FinishReason, StreamEvent}, LlmProvider};
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

    // 首条消息时异步生成标题
    let existing = chat_db::list_messages(&db_path, conv_id)?;
    if existing.len() == 1 {
        if let Some(p) = state.get_llm_provider() {
            let app2 = app_handle.clone();
            let msg = user_message.clone();
            let db = db_path.clone();
            tokio::spawn(async move {
                generate_title_async(p, &app2, conv_id, msg, &db).await;
            });
        }
    }

    let max_iter: u32 = 12;
    let mut iter: u32 = 0;
    let mut last_message_id: i64 = 0;
    let mut wrap_up_mode: bool = false;

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

        // c. 加载启用的工具（shell_mode=off 时从列表中移除 run_shell）
        let shell_mode = chat_db::get_conv_shell_mode(&db_path, conv_id).unwrap_or_else(|_| "off".to_string());
        let mut tools = chat_db::list_enabled_tools(&db_path)?;
        if shell_mode == "off" {
            tools.retain(|t| t.name != "run_shell");
        }
        let effective_tools = if wrap_up_mode { Vec::new() } else { tools.clone() };

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
            // 取消时静默丢弃所有事件（不 emit）
            if cancel_token.load(Ordering::Relaxed) {
                return;
            }
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

        // 取消检查：流之前先查一次
        if cancel_token.load(Ordering::Relaxed) {
            let _ = app_handle.emit(
                "chat_cancelled",
                json!({ "conversation_id": conv_id, "message_id": message_id }),
            );
            break;
        }

        let completion = match provider
            .chat_stream(&llm_messages, &effective_tools, &mut on_event)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                let _ = app_handle.emit(
                    "chat_error",
                    json!({
                        "conversation_id": conv_id,
                        "message_id": message_id,
                        "error": format!("{e}"),
                    }),
                );
                break;
            }
        };

        // 流结束后再检查一次取消
        if cancel_token.load(Ordering::Relaxed) {
            let _ = app_handle.emit(
                "chat_cancelled",
                json!({ "conversation_id": conv_id, "message_id": message_id }),
            );
            break;
        }

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
        chat_db::update_message_after_stream(
            &db_path, message_id, content_opt,
            tool_calls_json.as_deref(),
            completion.reasoning_content.as_deref(),
        )?;

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

                    let tool_result_content = if let Some(pending_id) = result.awaiting_approval {
                        let _ = app_handle.emit(
                            "chat_awaiting_approval",
                            json!({
                                "conversation_id": conv_id,
                                "message_id": message_id,
                                "call_id": call.id,
                                "pending_id": pending_id,
                                "tool_name": call.function.name,
                                "args": call.function.arguments,
                            }),
                        );
                        // 创建 oneshot channel，挂起循环等待用户审批
                        let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
                        state.register_chat_write_approval(pending_id, tx);

                        match tokio::time::timeout(
                            std::time::Duration::from_secs(600),
                            rx,
                        )
                        .await
                        {
                            Ok(Ok(Ok(msg))) => msg,
                            Ok(Ok(Err(reason))) => format!("写操作被拒绝: {reason}"),
                            Ok(Err(_)) => "审批 channel 关闭".to_string(),
                            Err(_) => "审批超时（10分钟）".to_string(),
                        }
                    } else {
                        result.content
                    };

                    // 持久化工具结果
                    let now3 = current_timestamp_ms();
                    chat_db::append_message(
                        &db_path, conv_id, "tool",
                        Some(&tool_result_content),
                        None,
                        Some(&result.call_id),
                        Some(&call.function.name),
                        &now3,
                    )?;

                    let preview: String = tool_result_content.chars().take(200).collect();
                    let _ = app_handle.emit(
                        "chat_tool_result",
                        json!({
                            "conversation_id": conv_id,
                            "message_id": message_id,
                            "call_id": result.call_id,
                            "ok": true,
                            "content_preview": preview,
                            "latency_ms": result.latency_ms,
                        }),
                    );
                }

                iter += 1;
                if iter >= max_iter {
                    if !wrap_up_mode {
                        // Give model one final text-only call to complete naturally
                        wrap_up_mode = true;
                        continue;
                    } else {
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
        reasoning_content: msg.reasoning_content.clone(),
    })
}

// ── 自动标题生成 ───────────────────────────────────────────────────────────────

async fn generate_title_async(
    provider: std::sync::Arc<dyn LlmProvider>,
    app_handle: &AppHandle,
    conv_id: i64,
    user_message: String,
    db_path: &PathBuf,
) {
    let prompt = format!(
        "请用 4-12 个字概括以下用户问题作为对话标题，只输出标题文字：\n{}",
        user_message.chars().take(200).collect::<String>()
    );
    let Ok(title) = provider.complete(&prompt).await else {
        return;
    };
    let title = title.trim().to_string();
    if title.is_empty() {
        return;
    }
    let now = current_timestamp_ms();
    let _ = chat_db::rename_conversation(db_path, conv_id, &title, &now);
    let _ = app_handle.emit(
        "chat_title_updated",
        json!({ "conversation_id": conv_id, "title": title }),
    );
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_chat::db::Message;

    fn make_msg(role: &str, content: Option<&str>, tool_calls_json: Option<&str>) -> Message {
        let tool_calls_json = tool_calls_json.map(str::to_string);
        let tool_calls = tool_calls_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Message {
            id: 1,
            conversation_id: 1,
            role: role.to_string(),
            content: content.map(str::to_string),
            tool_calls_json,
            tool_calls,
            tool_call_id: None,
            tool_name: None,
            reasoning_content: None,
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
