//! 质量与矛盾：lint 结果结构；矛盾裁决留给 LLM/人工，此处保留「提示」数据结构。

use crate::model::ClaimId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintFinding {
    pub code: String,
    pub message: String,
    pub severity: LintSeverity,
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionHint {
    pub a: ClaimId,
    pub b: ClaimId,
    pub reason: String,
}
