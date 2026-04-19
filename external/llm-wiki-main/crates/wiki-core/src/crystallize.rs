//! 结晶：把一次探索链蒸馏为 Wiki 页草稿 + 可抽取断言文本。

use crate::model::Scope;
use crate::page::WikiPage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCrystallizationInput {
    pub question: String,
    pub findings: Vec<String>,
    pub files_touched: Vec<String>,
    pub lessons: Vec<String>,
    pub scope: Scope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystallizationDraft {
    pub page: WikiPage,
    pub claim_candidates: Vec<String>,
}

pub fn draft_from_session(input: SessionCrystallizationInput) -> CrystallizationDraft {
    let title = format!("结晶: {}", one_line(&input.question));
    let mut md = String::new();
    md.push_str("# 问题\n\n");
    md.push_str(&input.question);
    md.push_str("\n\n## 结论与发现\n\n");
    for f in &input.findings {
        md.push_str(&format!("- {}\n", one_line(f)));
    }
    if !input.files_touched.is_empty() {
        md.push_str("\n## 涉及文件\n\n");
        for p in &input.files_touched {
            md.push_str(&format!("- `{}`\n", p));
        }
    }
    if !input.lessons.is_empty() {
        md.push_str("\n## 可复用教训\n\n");
        for l in &input.lessons {
            md.push_str(&format!("- {}\n", one_line(l)));
        }
    }
    let scope = input.scope.clone();
    let mut page = WikiPage::new(title, md, scope);
    page.outbound_page_titles = Vec::new();
    CrystallizationDraft {
        claim_candidates: input.lessons,
        page,
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
