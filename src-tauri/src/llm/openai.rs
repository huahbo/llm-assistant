//! OpenAI-compatible 云端 LLM Provider 实现
//!
//! 通过 OpenAI Chat Completions API 兼容协议调用云端 LLM，
//! 支持 Hybrid 模式下替换本地 Ollama 处理摘要与问答。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::provider::{LlmError, LlmProvider};

/// 默认请求超时时间（秒）
const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// 默认模型（兼顾性能与成本）
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
/// 默认 API 基础地址（可替换为兼容端点）
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";

/// OpenAI-compatible Provider 配置
#[derive(Debug, Clone)]
pub struct OpenAiConfig {
    /// 云端 API Key
    pub api_key: String,
    /// 使用的模型名称，如 "gpt-4o-mini", "deepseek-chat"
    pub model: String,
    /// API 基础地址，默认为 https://api.openai.com/v1（可替换为兼容端点）
    pub base_url: String,
    /// 请求超时时间（秒）
    pub timeout_secs: u64,
}

impl OpenAiConfig {
    /// 使用 API Key、基础地址和模型创建配置
    pub fn with_base_url_and_model(api_key: String, base_url: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url,
            timeout_secs: DEFAULT_TIMEOUT_SECS,
        }
    }
}

/// Chat Completions 请求中的单条消息
#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

/// Chat Completions 请求体
#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
}

/// Chat Completions 响应中的 choice 项
#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

/// 响应消息体
#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

/// Chat Completions 响应体（仅解析所需字段）
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

/// 流式响应中的 delta 内容
#[derive(Debug, Deserialize)]
struct ChatStreamDelta {
    #[serde(default)]
    content: Option<String>,
}

/// 流式响应中的 choice（仅解析所需字段）
#[derive(Debug, Deserialize)]
struct ChatStreamChoice {
    #[serde(default)]
    delta: Option<ChatStreamDelta>,
}

/// OpenAI-compatible 流式响应体（data: {...}）
#[derive(Debug, Deserialize)]
struct ChatStreamResponse {
    choices: Vec<ChatStreamChoice>,
}

/// OpenAI-compatible LLM Provider
///
/// 通过 OpenAI Chat Completions API 与云端 LLM 交互，
/// 在 Hybrid 模式下作为 Ollama 的替代选项。
#[derive(Debug)]
pub struct OpenAiProvider {
    /// Provider 配置
    config: OpenAiConfig,
    /// HTTP 客户端
    client: Client,
}

impl OpenAiProvider {
    /// 创建新的 OpenAI Provider 实例
    pub fn new(config: OpenAiConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("创建 HTTP 客户端失败");

        Self { config, client }
    }

    /// 将 reqwest 错误映射为 LlmError
    fn map_reqwest_error(e: reqwest::Error) -> LlmError {
        if e.is_timeout() {
            LlmError::Timeout
        } else if e.is_connect() {
            LlmError::ConnectionFailed(format!("无法连接到云端 Provider: {}", e))
        } else {
            LlmError::ConnectionFailed(format!("请求失败: {}", e))
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn model(&self) -> &str {
        &self.config.model
    }

    fn base_url(&self) -> &str {
        &self.config.base_url
    }

    async fn summarize(&self, content: &str, max_tokens: usize) -> Result<String, LlmError> {
        // 构造摘要专用 prompt（英文指令以降低 token 消耗）
        let prompt = format!(
            "Please generate a concise summary of the following content in at most {} words:\n\n{}\n\nSummary:",
            max_tokens / 4,
            content
        );
        self.complete(&prompt).await
    }

    async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        let url = format!("{}/chat/completions", self.config.base_url);

        let request_body = ChatRequest {
            model: &self.config.model,
            messages: vec![ChatMessage {
                role: "user",
                content: prompt,
            }],
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            // Bearer 认证
            .bearer_auth(&self.config.api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;

        let status = response.status();

        // 401 表示 API Key 无效或未授权
        if status.as_u16() == 401 {
            return Err(LlmError::ConnectionFailed(
                "云端 API Key 无效或未授权，请在 Settings 中更新 API Key".to_string(),
            ));
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::InvalidResponse(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("解析响应失败: {}", e)))?;

        // 取第一个 choice 的内容
        chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| LlmError::InvalidResponse("响应中没有可用的 choice".to_string()))
    }

    async fn complete_stream(
        &self,
        prompt: &str,
        on_chunk: &mut (dyn FnMut(String) + Send),
    ) -> Result<String, LlmError> {
        let url = format!("{}/chat/completions", self.config.base_url);
        let request_body = ChatRequest {
            model: &self.config.model,
            messages: vec![ChatMessage {
                role: "user",
                content: prompt,
            }],
            stream: true,
        };

        let mut response = self
            .client
            .post(&url)
            .bearer_auth(&self.config.api_key)
            .json(&request_body)
            .send()
            .await
            .map_err(Self::map_reqwest_error)?;

        let status = response.status();
        if status.as_u16() == 401 {
            return Err(LlmError::ConnectionFailed(
                "云端 API Key 无效或未授权，请在 Settings 中更新 API Key".to_string(),
            ));
        }
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
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

                if let Some(data) = line.strip_prefix("data:") {
                    let payload = data.trim();
                    if payload == "[DONE]" {
                        return Ok(full_text);
                    }
                    let parsed: ChatStreamResponse = serde_json::from_str(payload).map_err(|e| {
                        LlmError::InvalidResponse(format!("解析流式响应失败: {}", e))
                    })?;
                    if let Some(content) = parsed
                        .choices
                        .into_iter()
                        .next()
                        .and_then(|c| c.delta)
                        .and_then(|d| d.content)
                    {
                        if !content.is_empty() {
                            full_text.push_str(&content);
                            on_chunk(content);
                        }
                    }
                }
            }
        }

        Ok(full_text)
    }

    async fn health_check(&self) -> Result<bool, LlmError> {
        // 调用 /models 端点验证 API Key 与网络连通性
        let url = format!("{}/models", self.config.base_url);

        match self
            .client
            .get(&url)
            .bearer_auth(&self.config.api_key)
            .send()
            .await
        {
            Ok(response) => Ok(response.status().is_success()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = OpenAiConfig::with_base_url_and_model(
            "sk-test".to_string(),
            DEFAULT_OPENAI_BASE_URL.to_string(),
            DEFAULT_OPENAI_MODEL.to_string(),
        );
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.model, DEFAULT_OPENAI_MODEL);
        assert_eq!(config.base_url, DEFAULT_OPENAI_BASE_URL);
        assert_eq!(config.timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    #[test]
    fn test_config_with_model() {
        let config = OpenAiConfig::with_base_url_and_model(
            "sk-test".to_string(),
            DEFAULT_OPENAI_BASE_URL.to_string(),
            "gpt-4o".to_string(),
        );
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.base_url, DEFAULT_OPENAI_BASE_URL);
    }

    #[test]
    fn test_provider_creation() {
        let config = OpenAiConfig::with_base_url_and_model(
            "sk-test".to_string(),
            DEFAULT_OPENAI_BASE_URL.to_string(),
            DEFAULT_OPENAI_MODEL.to_string(),
        );
        let provider = OpenAiProvider::new(config.clone());
        assert_eq!(provider.config.model, config.model);
        assert_eq!(provider.config.base_url, config.base_url);
    }
}
