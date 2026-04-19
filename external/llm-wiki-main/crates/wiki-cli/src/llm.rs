use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfigFile {
    pub llm: LlmConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub max_retries: u32,
}

fn default_timeout_seconds() -> u64 {
    60
}

pub fn load_llm_config(path: &Path) -> Result<LlmConfig, Box<dyn std::error::Error>> {
    let s = std::fs::read_to_string(path)?;
    let parsed: LlmConfigFile = toml::from_str(&s)?;
    Ok(parsed.llm)
}

pub fn smoke_chat_completion(
    cfg: &LlmConfig,
    prompt: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let url = chat_completions_url(&cfg.base_url);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(cfg.timeout_seconds))
        .build()?;

    let mut last_err: Option<Box<dyn std::error::Error>> = None;
    for _ in 0..=cfg.max_retries {
        match do_chat_once(&client, &url, &cfg.api_key, &cfg.model, prompt) {
            Ok(s) => return Ok(s),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| "unknown llm error".into()))
}

fn chat_completions_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/chat/completions")
    } else {
        format!("{trimmed}/v1/chat/completions")
    }
}

fn do_chat_once(
    client: &reqwest::blocking::Client,
    url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let body = serde_json::json!({
        "model": model,
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "max_tokens": 32,
        "temperature": 0.0
    });

    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&body)
        .send()?;

    let status = resp.status();
    let text = resp.text()?;
    if !status.is_success() {
        return Err(format!("llm http {}: {}", status.as_u16(), text).into());
    }

    let v: serde_json::Value = serde_json::from_str(&text)?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if content.is_empty() {
        return Err(format!("llm response missing content: {text}").into());
    }
    Ok(content)
}

