//! Ollama LLM Provider 实现
//!
//! 本模块提供基于 Ollama 本地 LLM 服务的 Provider 实现。
//! Ollama 支持在本地运行各种开源大语言模型。

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::provider::{LlmError, LlmProvider};

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

/// Ollama /api/generate 响应体
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    /// 生成的文本
    response: String,
    /// 是否完成（非流式时始终为 true）
    #[allow(dead_code)]
    done: bool,
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
}

impl OllamaProvider {
    /// 创建新的 Ollama Provider 实例
    ///
    /// # 参数
    /// - `config`: Ollama 配置
    pub fn new(config: OllamaConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("创建 HTTP 客户端失败");

        Self { config, client }
    }

    /// 使用默认配置创建 Provider
    pub fn with_defaults() -> Self {
        Self::new(OllamaConfig::default())
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
        let exists = tags.models.iter().any(|m| {
            m.name == model_name || m.name.starts_with(&format!("{}:", model_name))
        });

        Ok(exists)
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

        let generate_response: GenerateResponse = response
            .json()
            .await
            .map_err(|e| LlmError::InvalidResponse(format!("解析响应失败: {}", e)))?;

        Ok(generate_response.response)
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
        let provider = OllamaProvider::with_defaults();
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
}
