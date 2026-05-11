//! MCP (Model Context Protocol) client — JSON-RPC 2.0 over stdio transport

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::time::timeout;

// ── Public types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    pub server_name: String,
    pub tool_name: String,
    pub description: String,
    pub parameters_schema: String,
}

// ── McpClient ──────────────────────────────────────────────────────────────────

pub struct McpClient {
    pub server_name: String,
    _child: Child,
    stdin: tokio::io::BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl McpClient {
    /// Spawn an MCP server process and perform the initialize handshake.
    pub async fn spawn(
        server_name: String,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
    ) -> Result<Self, String> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("启动 MCP 服务器 '{server_name}' 失败: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "无法获取子进程 stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "无法获取子进程 stdout".to_string())?;

        let mut client = Self {
            server_name,
            _child: child,
            stdin: tokio::io::BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            next_id: 1,
        };

        client.initialize().await?;
        Ok(client)
    }

    // ── Protocol ────────────────────────────────────────────────────────────────

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut line =
            serde_json::to_string(&request).map_err(|e| format!("序列化请求失败: {e}"))?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("写入 stdin 失败: {e}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("刷新 stdin 失败: {e}"))?;

        // Read lines until we find the matching response (skip notifications).
        let expected_id = json!(id);
        let result = timeout(Duration::from_secs(30), async {
            loop {
                let mut resp_line = String::new();
                let bytes = self.stdout.read_line(&mut resp_line).await?;
                if bytes == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "MCP 服务器已关闭",
                    ));
                }
                let trimmed = resp_line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let v: Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                // Skip JSON-RPC notifications (no "id" field).
                if v.get("id") != Some(&expected_id) {
                    continue;
                }
                return Ok(v);
            }
        })
        .await
        .map_err(|_| "MCP 请求超时（30s）".to_string())?
        .map_err(|e| format!("读取 stdout 失败: {e}"))?;

        if let Some(error) = result.get("error") {
            return Err(format!("MCP 协议错误: {error}"));
        }
        Ok(result.get("result").cloned().unwrap_or(Value::Null))
    }

    async fn send_notification(&mut self, method: &str, params: Value) {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        if let Ok(mut line) = serde_json::to_string(&notification) {
            line.push('\n');
            let _ = self.stdin.write_all(line.as_bytes()).await;
            let _ = self.stdin.flush().await;
        }
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "clientInfo": { "name": "llm-wiki", "version": "0.2.0" },
        });
        self.send_request("initialize", params).await?;
        self.send_notification("notifications/initialized", json!({}))
            .await;
        Ok(())
    }

    // ── Public API ─────────────────────────────────────────────────────────────

    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDef>, String> {
        let result = self.send_request("tools/list", json!({})).await?;

        let tools_arr = result
            .get("tools")
            .and_then(|t| t.as_array())
            .ok_or_else(|| "MCP tools/list 响应格式异常".to_string())?;

        let server_name = self.server_name.clone();
        let tools = tools_arr
            .iter()
            .filter_map(|t| {
                let tool_name = t.get("name")?.as_str()?.to_string();
                let description = t
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let parameters_schema = t
                    .get("inputSchema")
                    .map(|s| {
                        serde_json::to_string(s)
                            .unwrap_or_else(|_| "{}".to_string())
                    })
                    .unwrap_or_else(|| {
                        r#"{"type":"object","properties":{}}"#.to_string()
                    });
                Some(McpToolDef {
                    server_name: server_name.clone(),
                    tool_name,
                    description,
                    parameters_schema,
                })
            })
            .collect();

        Ok(tools)
    }

    pub async fn call_tool(&mut self, tool_name: &str, arguments: Value) -> Result<String, String> {
        let params = json!({
            "name": tool_name,
            "arguments": arguments,
        });
        let result = self.send_request("tools/call", params).await?;

        // MCP tool result: { content: [{ type: "text", text: "..." }], isError?: bool }
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            let parts: Vec<String> = content
                .iter()
                .filter_map(|item| {
                    if item.get("type")?.as_str()? == "text" {
                        item.get("text")?.as_str().map(str::to_owned)
                    } else {
                        None
                    }
                })
                .collect();
            if !parts.is_empty() {
                let text = parts.join("\n");
                if result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return Err(text);
                }
                return Ok(text);
            }
        }

        // Fallback: return JSON
        Ok(serde_json::to_string_pretty(&result).unwrap_or_else(|_| "(empty)".to_string()))
    }
}
