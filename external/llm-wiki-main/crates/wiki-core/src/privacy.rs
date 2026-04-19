//! 治理：ingest 前脱敏（规则占位，生产可换为 ML/熵检测）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveKind {
    ApiKeyLike,
    BearerToken,
    HighEntropySecret,
    PrivateMarker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionFinding {
    pub kind: SensitiveKind,
    pub placeholder: String,
}

/// 极简脱敏：常见 token / key 行；命中则替换为占位符并记录。
pub fn redact_for_ingest(input: &str) -> (String, Vec<RedactionFinding>) {
    let mut out = String::new();
    let mut findings = Vec::new();
    for line in input.lines() {
        let trimmed = line.trim();
        let mut replaced = line.to_string();
        if trimmed.to_ascii_lowercase().starts_with("authorization:")
            || trimmed.to_ascii_lowercase().starts_with("bearer ")
        {
            replaced = "[REDACTED_BEARER]".into();
            findings.push(RedactionFinding {
                kind: SensitiveKind::BearerToken,
                placeholder: "[REDACTED_BEARER]".into(),
            });
        } else if trimmed.contains("AKIA")
            || (trimmed.contains("api_key") && trimmed.contains('='))
            || trimmed.contains("BEGIN RSA PRIVATE KEY")
        {
            replaced = "[REDACTED_SECRET]".into();
            findings.push(RedactionFinding {
                kind: SensitiveKind::ApiKeyLike,
                placeholder: "[REDACTED_SECRET]".into(),
            });
        } else if trimmed.contains("PRIVATE") && trimmed.contains("DO_NOT_COMMIT") {
            replaced = "[REDACTED_PRIVATE]".into();
            findings.push(RedactionFinding {
                kind: SensitiveKind::PrivateMarker,
                placeholder: "[REDACTED_PRIVATE]".into(),
            });
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&replaced);
    }
    (out, findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_bearer() {
        let (s, f) = redact_for_ingest("Authorization: Bearer abc.def");
        assert!(s.contains("REDACTED"));
        assert!(!f.is_empty());
    }
}
