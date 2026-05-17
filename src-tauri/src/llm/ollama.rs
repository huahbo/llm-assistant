//! Ollama LLM Provider 实现
//!
//! 本模块提供基于 Ollama 本地 LLM 服务的 Provider 实现。
//! Ollama 支持在本地运行各种开源大语言模型。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::provider::{LlmError, LlmProvider};
use super::types::{
    ChatCompletion, ChatMessage, FinishReason, StreamEvent, ToolCall, ToolCallFunction, ToolSchema,
};

/// 默认请求超时时间（秒）
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Ollama Provider 配置
#[derive(Debug, Clone)]
pub struct OllamaConfig {
    /// Ollama 服务地址，默认为 http://localhost:11434
    pub base_url: String,
    /// 使用的模型名称，如 "llama3:8b", "mistral" 等
    pub model: String,
    /// 请求超时时间（秒）
    pub timeout_secs: u64,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "llama3:8b".to_string(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }
}

/// Ollama /api/generate 请求体
#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    /// 模型名称
    model: &'a str,
    /// 提示词
    prompt: &'a str,
    /// 是否流式输出（我们使用非流式）
    stream: bool,
}

/// Ollama /api/embeddings 请求体
#[derive(Debug, Serialize)]
struct EmbedRequest<'a> {
    /// 模型名称
    model: &'a str,
    /// 需要嵌入的文本
    prompt: &'a str,
}

/// Ollama /api/embeddings 响应体
#[derive(Debug, Deserialize)]
struct EmbedResponse {
    /// 生成的嵌入向量
    embedding: Vec<f32>,
}

/// Ollama /api/generate 响应体
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    /// 生成的文本
    response: String,
    /// 是否完成（非流式时始终为 true）
    #[allow(dead_code)]
    done: bool,
}

/// Ollama 流式生成响应体（按行 JSON）
#[derive(Debug, Deserialize)]
struct GenerateStreamResponse {
    /// 本次增量文本
    #[serde(default)]
    response: String,
    /// 是否结束
    #[serde(default)]
    done: bool,
}

/// Ollama /api/version 响应体
#[derive(Debug, Deserialize)]
struct OllamaVersion {
    version: String,
}

/// Ollama /api/chat NDJSON 工具调用函数
#[derive(Debug, Deserialize)]
struct OllamaChatToolCallFunction {
    name: String,
    /// arguments 是 JSON 对象（不是字符串），需转换为字符串
    arguments: serde_json::Value,
}

/// Ollama /api/chat NDJSON 工具调用条目
#[derive(Debug, Deserialize)]
struct OllamaChatToolCall {
    function: OllamaChatToolCallFunction,
}

/// Ollama /api/chat NDJSON 消息体
#[derive(Debug, Deserialize, Default)]
struct OllamaChatMessageBody {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<OllamaChatToolCall>,
}

/// Ollama /api/chat NDJSON 流式响应块
#[derive(Debug, Deserialize)]
struct OllamaChatChunk {
    #[serde(default)]
    message: OllamaChatMessageBody,
    #[serde(default)]
    done: bool,
    done_reason: Option<String>,
}

/// Ollama /api/chat 请求体
#[derive(Debug, Serialize)]
struct OllamaChatRequest<'r> {
    model: &'r str,
    messages: &'r [ChatMessage],
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaToolWire<'r>>>,
}

/// Ollama tools 数组元素（与 OpenAI 格式相同）
#[derive(Debug, Serialize)]
struct OllamaToolWire<'r> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OllamaToolFunctionWire<'r>,
}

#[derive(Debug, Serialize)]
struct OllamaToolFunctionWire<'r> {
    name: &'r str,
    description: &'r str,
    parameters: &'r serde_json::Value,
}

/// 检查 Ollama 版本字符串是否支持工具调用（≥ 0.4.0）
fn ollama_version_supports_tools(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    let major: u32 = parts[0].parse().unwrap_or(0);
    let minor: u32 = parts[1].parse().unwrap_or(0);
    (major, minor) >= (0, 4)
}

/// Ollama /api/tags 响应体（用于健康检查）
#[derive(Debug, Deserialize)]
struct TagsResponse {
    /// 可用模型列表
    models: Vec<ModelInfo>,
}

/// 模型信息
#[derive(Debug, Deserialize)]
struct ModelInfo {
    /// 模型名称
    name: String,
}

/// Ollama LLM Provider
///
/// 通过 Ollama API 与本地运行的 LLM 模型交互。
#[derive(Debug, Clone)]
pub struct OllamaProvider {
    /// Provider 配置
    config: OllamaConfig,
    /// HTTP 客户端
    client: Client,
    /// 版本探测结果缓存（None = 未检测，Some(bool) = 是否支持 tools）
    tool_support_cache: Arc<Mutex<Option<bool>>>,
}

#[allow(dead_code)]
impl OllamaProvider {
    /// 创建新的 Ollama Provider 实例
    pub fn new(config: OllamaConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("创建 HTTP 客户端失败");

        Self {
            config,
            client,
            tool_support_cache: Arc::new(Mutex::new(None)),
        }
    }

    /// 调用 /api/version 检查是否支持工具调用（≥ 0.4.0）
    async fn check_ollama_tool_support(&self) -> bool {
        let url = format!("{}/api/version", self.config.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<OllamaVersion>().await {
                    Ok(v) => ollama_version_supports_tools(&v.version),
                    Err(_) => false,
                }
            }
            _ => false,
        }
    }

    /// 获取当前配置的模型名称
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// 获取服务地址
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// 检查指定模型是否存在
    ///
    /// 调用 /api/tags 获取可用模型列表，检查目标模型是否在其中
    async fn model_exists(&self, model_name: &str) -> Result<bool, LlmError> {
        let url = format!("{}/api/tags", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Self::map_reqwest_error(e))?;

        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("解析响应失败: {}", e)))?;

        // 检查模型是否存在（模型名称可能带有版本标签，如 "llama3.2:latest"）
        let exists = tags
            .models
            .iter()
            .any(|m| m.name == model_name || m.name.starts_with(&format!("{}:", model_name)));

        Ok(exists)
    }

    /// 列出 Ollama 服务上可用的模型名称列表
    pub async fn list_models(&self) -> Result<Vec<String>, LlmError> {
        let url = format!("{}/api/tags", self.config.base_url);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;
        if !response.status().is_success() {
            return Err(LlmError::ConnectionFailed("无法获取模型列表".to_string()));
        }
        let tags: TagsResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("解析响应失败: {}", e)))?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }

    /// 将 reqwest 错误映射为 LlmError
    fn map_reqwest_error(e: reqwest::Error) -> LlmError {
        if e.is_timeout() {
            LlmError::Timeout
        } else if e.is_connect() {
            LlmError::ConnectionFailed(format!("无法连接到 Ollama 服务: {}", e))
        } else {
            LlmError::ConnectionFailed(format!("请求失败: {}", e))
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn model(&self) -> &str {
        &self.config.model
    }

    fn base_url(&self) -> &str {
        &self.config.base_url
    }

    async fn summarize(&self, content: &str, max_tokens: usize) -> Result<String, LlmError> {
        // 构造摘要专用 prompt
        let prompt = format!(
            "请为以下内容生成一个简洁的摘要，摘要长度不超过 {} 个词：\n\n{}\n\n摘要：",
            max_tokens / 4, // 粗略估计：1 token ≈ 4 个字符
            content
        );

        self.complete(&prompt).await
    }

    async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        let url = format!("{}/api/generate", self.config.base_url);

        let request_body = GenerateRequest {
            model: &self.config.model,
            prompt,
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| Self::map_reqwest_error(e))?;

        // 检查 HTTP 状态码
        let status = response.status();
        if status.as_u16() == 404 {
            // 模型不存在
            return Err(LlmError::ModelNotFound(self.config.model.clone()));
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            // 检查是否是模型不存在的错误
            if error_text.contains("model") && error_text.contains("not found") {
                return Err(LlmError::ModelNotFound(self.config.model.clone()));
            }
            return Err(LlmError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let body_text = response
            .text()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("读取响应体失败: {}", e)))?;

        let generate_response: GenerateResponse = serde_json::from_str(&body_text)
            .map_err(|e| LlmError::InvalidResponse(format!("解析响应失败: {}", e)))?;

        Ok(generate_response.response)
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>, LlmError> {
        let url = format!("{}/api/embeddings", self.config.base_url);

        let request_body = EmbedRequest {
            model: &self.config.model,
            prompt: text,
        };

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| Self::map_reqwest_error(e))?;

        let status = response.status();
        if status.as_u16() == 404 {
            return Err(LlmError::ModelNotFound(self.config.model.clone()));
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let embed_response: EmbedResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("解析嵌入响应失败: {}", e)))?;

        Ok(embed_response.embedding)
    }

    async fn complete_stream(
        &self,
        prompt: &str,
        on_chunk: &mut (dyn FnMut(String) + Send),
    ) -> Result<String, LlmError> {
        let url = format!("{}/api/generate", self.config.base_url);
        let request_body = GenerateRequest {
            model: &self.config.model,
            prompt,
            stream: true,
        };

        let mut response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;

        let status = response.status();
        if status.as_u16() == 404 {
            return Err(LlmError::ModelNotFound(self.config.model.clone()));
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if error_text.contains("model") && error_text.contains("not found") {
                return Err(LlmError::ModelNotFound(self.config.model.clone()));
            }
            return Err(LlmError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let mut full_text = String::new();
        let mut pending = String::new();

        while let Some(chunk) = response.chunk().await.map_err(Self::map_reqwest_error)? {
            pending.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_pos) = pending.find('\n') {
                let line = pending[..newline_pos].trim().to_string();
                pending.drain(..=newline_pos);
                if line.is_empty() {
                    continue;
                }

                let parsed: GenerateStreamResponse = serde_json::from_str(&line)
                    .map_err(|e| LlmError::InvalidResponse(format!("解析流式响应失败: {}", e)))?;
                if !parsed.response.is_empty() {
                    full_text.push_str(&parsed.response);
                    on_chunk(parsed.response);
                }
                if parsed.done {
                    return Ok(full_text);
                }
            }
        }

        let tail = pending.trim();
        if !tail.is_empty() {
            let parsed: GenerateStreamResponse = serde_json::from_str(tail)
                .map_err(|e| LlmError::InvalidResponse(format!("解析流式响应失败: {}", e)))?;
            if !parsed.response.is_empty() {
                full_text.push_str(&parsed.response);
                on_chunk(parsed.response);
            }
        }

        Ok(full_text)
    }

    async fn chat_stream(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSchema],
        on_event: &mut (dyn FnMut(StreamEvent) + Send),
    ) -> Result<ChatCompletion, LlmError> {
        // 懒加载版本探测（在 await 前提前释放锁，避免 MutexGuard 跨越 await）
        let needs_check = self.tool_support_cache.lock().unwrap().is_none();
        if needs_check {
            let supported = self.check_ollama_tool_support().await;
            *self.tool_support_cache.lock().unwrap() = Some(supported);
        }

        let url = format!("{}/api/chat", self.config.base_url);

        let tools_wire: Option<Vec<OllamaToolWire>> = if tools.is_empty() {
            None
        } else {
            Some(
                tools
                    .iter()
                    .map(|t| OllamaToolWire {
                        kind: "function",
                        function: OllamaToolFunctionWire {
                            name: &t.name,
                            description: &t.description,
                            parameters: &t.parameters,
                        },
                    })
                    .collect(),
            )
        };

        let request_body = OllamaChatRequest {
            model: &self.config.model,
            messages,
            stream: true,
            tools: tools_wire,
        };

        let mut response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;

        let status = response.status();
        if status.as_u16() == 404 {
            return Err(LlmError::ModelNotFound(self.config.model.clone()));
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let mut content = String::new();
        let mut pending = String::new();

        while let Some(chunk) = response.chunk().await.map_err(Self::map_reqwest_error)? {
            pending.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(nl) = pending.find('\n') {
                let line = pending[..nl].trim().to_string();
                pending.drain(..=nl);
                if line.is_empty() {
                    continue;
                }

                let chat_chunk: OllamaChatChunk =
                    serde_json::from_str(&line).map_err(|e| {
                        LlmError::InvalidResponse(format!(
                            "解析 Ollama NDJSON 失败: {} (line: {})",
                            e,
                            line.chars().take(80).collect::<String>()
                        ))
                    })?;

                if !chat_chunk.message.content.is_empty() {
                    let text = chat_chunk.message.content.clone();
                    content.push_str(&text);
                    on_event(StreamEvent::TextDelta(text));
                }

                if chat_chunk.done {
                    let finish_reason = chat_chunk
                        .done_reason
                        .as_deref()
                        .map(FinishReason::from)
                        .unwrap_or(FinishReason::Stop);

                    // Ollama 在 done 块中一次性返回所有 tool_calls
                    let mut tool_calls = Vec::new();
                    for (idx, tc) in chat_chunk.message.tool_calls.into_iter().enumerate() {
                        let idx = idx as u32;
                        let id = format!("call_{}", idx);
                        let name = tc.function.name;
                        let arguments = serde_json::to_string(&tc.function.arguments)
                            .unwrap_or_else(|_| "{}".to_string());

                        on_event(StreamEvent::ToolCallStart {
                            idx,
                            id: id.clone(),
                            name: name.clone(),
                        });
                        if !arguments.is_empty() {
                            on_event(StreamEvent::ToolCallArgsDelta {
                                idx,
                                args_chunk: arguments.clone(),
                            });
                        }
                        on_event(StreamEvent::ToolCallEnd { idx });

                        tool_calls.push(ToolCall {
                            id,
                            kind: "function".to_string(),
                            function: ToolCallFunction { name, arguments },
                        });
                    }

                    on_event(StreamEvent::FinishReason(finish_reason.clone()));

                    return Ok(ChatCompletion {
                        content,
                        tool_calls,
                        finish_reason,
                        reasoning_content: None,
                    });
                }
            }
        }

        Ok(ChatCompletion::text(content))
    }

    fn supports_tools(&self) -> bool {
        self.tool_support_cache
            .lock()
            .unwrap()
            .unwrap_or(false)
    }

    async fn health_check(&self) -> Result<bool, LlmError> {
        // 调用 /api/tags 检查服务是否可用
        let url = format!("{}/api/tags", self.config.base_url);

        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    // 服务可用，进一步检查模型是否存在
                    self.model_exists(&self.config.model).await
                } else {
                    Ok(false)
                }
            }
            Err(e) => {
                if e.is_connect() {
                    // 连接失败，服务不可用
                    Ok(false)
                } else {
                    Err(Self::map_reqwest_error(e))
                }
            }
        }
    }
}

// ── EmbedProvider 实现 ────────────────────────────────────────────────────────

use crate::llm::embed_provider::{EmbedError, EmbedProvider};

#[async_trait]
impl EmbedProvider for OllamaProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        LlmProvider::embed(self, text).await.map_err(|e| match e {
            LlmError::ConnectionFailed(m) => EmbedError::InitFailed(m),
            LlmError::ModelNotFound(m) => {
                EmbedError::InitFailed(format!("Ollama 模型未拉取: {}", m))
            }
            LlmError::Timeout => EmbedError::InferenceFailed("请求超时".to_string()),
            LlmError::InvalidResponse(m) => EmbedError::InferenceFailed(m),
        })
    }

    fn dimension(&self) -> usize {
        // nomic-embed-text 默认 768 维；Phase 4 切换 ONNX 后通过 backend_id 校验维度
        768
    }

    fn backend_id(&self) -> String {
        format!("ollama:{}", self.config.model)
    }

    async fn health_check(&self) -> bool {
        LlmProvider::health_check(self).await.unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OllamaConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.model, "llama3:8b");
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_provider_creation() {
        let provider = OllamaProvider::new(OllamaConfig::default());
        assert_eq!(provider.model(), "llama3:8b");
        assert_eq!(provider.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_custom_config() {
        let config = OllamaConfig {
            base_url: "http://192.168.1.100:11434".to_string(),
            model: "mistral".to_string(),
            timeout_secs: 120,
        };
        let provider = OllamaProvider::new(config);
        assert_eq!(provider.model(), "mistral");
        assert_eq!(provider.base_url(), "http://192.168.1.100:11434");
    }

    #[test]
    fn test_ollama_version_supports_tools() {
        assert!(ollama_version_supports_tools("0.4.0"));
        assert!(ollama_version_supports_tools("0.5.0"));
        assert!(ollama_version_supports_tools("1.0.0"));
        assert!(ollama_version_supports_tools("0.6.1"));
        assert!(!ollama_version_supports_tools("0.3.14"));
        assert!(!ollama_version_supports_tools("0.1.0"));
        assert!(!ollama_version_supports_tools("invalid"));
    }

    #[test]
    fn test_ollama_chat_chunk_text_parse() {
        let line = r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hello"},"done":false}"#;
        let chunk: OllamaChatChunk = serde_json::from_str(line).expect("parse failed");
        assert_eq!(chunk.message.content, "Hello");
        assert!(!chunk.done);
        assert!(chunk.message.tool_calls.is_empty());
    }

    #[test]
    fn test_ollama_chat_chunk_tool_call_parse() {
        let line = r#"{"model":"llama3.2","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"shell_exec","arguments":{"cmd":"ls -la"}}}]},"done":true,"done_reason":"tool_calls"}"#;
        let chunk: OllamaChatChunk = serde_json::from_str(line).expect("parse failed");
        assert!(chunk.done);
        assert_eq!(chunk.done_reason.as_deref(), Some("tool_calls"));
        assert_eq!(chunk.message.tool_calls.len(), 1);
        assert_eq!(chunk.message.tool_calls[0].function.name, "shell_exec");
        // arguments is a JSON object
        assert_eq!(chunk.message.tool_calls[0].function.arguments["cmd"], "ls -la");
    }
}
