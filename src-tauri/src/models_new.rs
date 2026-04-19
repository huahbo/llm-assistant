// src/models.rs 中间层类型定义补充
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct TagStats {
    pub tag: String,
    pub count: usize,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct SemanticHealthReport {
    pub total_pages: usize,
    pub pages_with_entities: usize,
    pub average_entities_per_page: f64,
    pub stale_pages_count: usize,
}
