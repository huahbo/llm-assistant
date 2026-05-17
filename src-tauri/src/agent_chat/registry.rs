//! Smithery MCP Registry 客户端（H17）
//!
//! 提供两个 async 函数：
//! - `search_smithery_servers`：搜索服务器列表
//! - `get_smithery_server_detail`：获取服务器详情及安装参数

use crate::models::{SmitheryServer, SmitheryServerDetail};

const REGISTRY_BASE: &str = "https://registry.smithery.ai";
const USER_AGENT: &str = "llm-wiki-desktop/0.2.3";
const TIMEOUT_SECS: u64 = 10;

/// 对查询参数值做 percent-encode（不依赖外部 crate）。
fn percent_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            // RFC 3986 unreserved chars：保留原样
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            // 空格 → +（query string 惯例）
            b' ' => out.push('+'),
            other => {
                out.push('%');
                out.push(char::from_digit((other >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((other & 0xf) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}

/// 对路径段做 percent-encode（`@` 和 `/` 需要编码，供 qualified_name 使用）。
fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => {
                out.push('%');
                out.push(char::from_digit((other >> 4) as u32, 16).unwrap().to_ascii_uppercase());
                out.push(char::from_digit((other & 0xf) as u32, 16).unwrap().to_ascii_uppercase());
            }
        }
    }
    out
}

/// 搜索 Smithery 服务器列表。
///
/// - `query`：关键词（可为空字符串，返回全量前 page_size 条）
/// - `page_size`：每页数量（调用方已 clamp 到合理范围）
pub async fn search_smithery_servers(
    query: &str,
    page_size: u32,
) -> Result<Vec<SmitheryServer>, String> {
    let url = format!(
        "{}/servers?q={}&pageSize={}",
        REGISTRY_BASE,
        percent_encode_query(query),
        page_size
    );

    let client = build_client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "请求超时，请检查网络连接".to_string()
            } else {
                format!("网络请求失败：{e}")
            }
        })?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("网络请求失败：{e}"))?;

    let servers_arr = body
        .get("servers")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut results = Vec::with_capacity(servers_arr.len());
    for item in servers_arr.iter().take(page_size as usize) {
        let qualified_name = item
            .get("qualifiedName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let display_name = item
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(&qualified_name)
            .to_string();
        let description = item
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let verified = item
            .get("verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let use_count = item
            .get("useCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let icon_url = item
            .get("iconUrl")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        results.push(SmitheryServer {
            qualified_name,
            display_name,
            description,
            verified,
            use_count,
            icon_url,
        });
    }

    Ok(results)
}

/// 获取 Smithery 服务器详情，解析安装命令与环境变量。
pub async fn get_smithery_server_detail(
    qualified_name: &str,
) -> Result<SmitheryServerDetail, String> {
    let encoded = percent_encode_path(qualified_name);
    let url = format!("{}/servers/{}", REGISTRY_BASE, encoded);

    let client = build_client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "请求超时，请检查网络连接".to_string()
            } else {
                format!("网络请求失败：{e}")
            }
        })?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("网络请求失败：{e}"))?;

    let display_name = body
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or(qualified_name)
        .to_string();
    let description = body
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 解析 connections[0].configSchema
    let (command, args, required_env_keys) = parse_connection(&body, qualified_name);

    Ok(SmitheryServerDetail {
        qualified_name: qualified_name.to_string(),
        display_name,
        description,
        command,
        args,
        required_env_keys,
    })
}

// ── 内部辅助 ──────────────────────────────────────────────────────────────────

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("网络请求失败：{e}"))
}

/// 从 API 响应中解析 command / args / required_env_keys。
///
/// 策略（宽松，容错）：
/// 1. 找到 `connections` 数组第一个 type == "stdio" 的元素
/// 2. 读其 configSchema
/// 3. 若 configSchema.command.default 有值 → 用作 command
/// 4. 若 configSchema.args.default 是数组 → 用作 args
/// 5. 否则回退 npx -y <qualified_name>
/// 6. 从 configSchema 的属性键中提取 env key（含 KEY/TOKEN/SECRET/API 或在 required 数组里）
fn parse_connection(
    body: &serde_json::Value,
    qualified_name: &str,
) -> (String, Vec<String>, Vec<String>) {
    let fallback = || {
        (
            "npx".to_string(),
            vec!["-y".to_string(), qualified_name.to_string()],
            vec![],
        )
    };

    let connections = match body.get("connections").and_then(|v| v.as_array()) {
        Some(c) if !c.is_empty() => c,
        _ => return fallback(),
    };

    // 优先找 stdio 类型，找不到取第一个
    let conn = connections
        .iter()
        .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("stdio"))
        .or_else(|| connections.first());

    let conn = match conn {
        Some(c) => c,
        None => return fallback(),
    };

    let schema = match conn.get("configSchema") {
        Some(s) => s,
        None => return fallback(),
    };

    // command
    let command = schema
        .get("command")
        .and_then(|c| c.get("default"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

    // args
    let args: Option<Vec<String>> = schema
        .get("args")
        .and_then(|a| a.get("default"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        });

    let (cmd, arg_list) = match (command, args) {
        (Some(c), Some(a)) => (c, a),
        (Some(c), None) => (c, vec![]),
        _ => {
            // 回退 npx
            return (
                "npx".to_string(),
                vec!["-y".to_string(), qualified_name.to_string()],
                extract_env_keys(schema),
            );
        }
    };

    let env_keys = extract_env_keys(schema);
    (cmd, arg_list, env_keys)
}

/// 从 configSchema 中提取环境变量 key 名。
///
/// 规则：
/// - 属性名（大写后）包含 KEY / TOKEN / SECRET / API → 收录
/// - 或该属性名出现在 schema.required 数组中 → 收录
fn extract_env_keys(schema: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();

    let required_arr: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // 遍历 properties
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for key in props.keys() {
            let upper = key.to_uppercase();
            let is_sensitive = upper.contains("KEY")
                || upper.contains("TOKEN")
                || upper.contains("SECRET")
                || upper.contains("API");
            let is_required = required_arr.contains(&key.as_str());

            if is_sensitive || is_required {
                if !keys.contains(key) {
                    keys.push(key.clone());
                }
            }
        }
    }

    // 也尝试顶层 env 字段（有些 schema 直接暴露 env 对象）
    if let Some(env_obj) = schema.get("env").and_then(|e| e.as_object()) {
        for key in env_obj.keys() {
            let upper = key.to_uppercase();
            let is_sensitive = upper.contains("KEY")
                || upper.contains("TOKEN")
                || upper.contains("SECRET")
                || upper.contains("API");
            if is_sensitive && !keys.contains(key) {
                keys.push(key.clone());
            }
        }
    }

    keys
}
