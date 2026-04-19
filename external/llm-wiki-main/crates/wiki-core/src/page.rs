//! Wiki 页：人类可读 Markdown；图结构与之并行而非替代。

use crate::model::{PageId, Scope};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiPage {
    pub id: PageId,
    pub title: String,
    pub markdown: String,
    pub scope: Scope,
    pub updated_at: OffsetDateTime,
    pub outbound_page_titles: Vec<String>,
}

impl WikiPage {
    pub fn new(title: impl Into<String>, markdown: impl Into<String>, scope: Scope) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id: PageId(Uuid::new_v4()),
            title: title.into(),
            markdown: markdown.into(),
            scope,
            updated_at: now,
            outbound_page_titles: Vec::new(),
        }
    }

    /// 从 markdown 中提取 `[[Page Title]]` 形式的 wikilink，并写入 `outbound_page_titles`。
    pub fn refresh_outbound_links(&mut self) {
        self.outbound_page_titles = extract_wikilinks(&self.markdown);
    }
}

/// 解析 `[[...]]` 语法，返回去重且保持首次出现顺序的标题。
pub fn extract_wikilinks(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = markdown.as_bytes();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            let start = i + 2;
            let mut j = start;
            while j + 1 < bytes.len() {
                if bytes[j] == b']' && bytes[j + 1] == b']' {
                    let t = markdown[start..j].trim();
                    if !t.is_empty() && !out.iter().any(|x| x == t) {
                        out.push(t.to_string());
                    }
                    i = j + 2;
                    break;
                }
                j += 1;
            }
            if j + 1 >= bytes.len() {
                break;
            }
            continue;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::extract_wikilinks;

    #[test]
    fn extracts_unique_wikilinks() {
        let md = "A [[One]] B [[Two]] [[One]]";
        let got = extract_wikilinks(md);
        assert_eq!(got, vec!["One".to_string(), "Two".to_string()]);
    }
}
