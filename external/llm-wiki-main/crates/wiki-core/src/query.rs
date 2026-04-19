//! 查询上下文：自然语言查询与 RRF 参数（三路召回由 `SearchPorts` 在 kernel 注入）。

/// 单次查询的公共参数。
#[derive(Debug, Clone)]
pub struct QueryContext<'a> {
    pub query: &'a str,
    /// RRF 常数 `k`（常见取 60）。
    pub rrf_k: f64,
    /// 每一路召回的最大条数（传给各 SearchPorts 实现）。
    pub per_stream_limit: usize,
}

impl<'a> QueryContext<'a> {
    pub fn new(query: &'a str) -> Self {
        Self {
            query,
            rrf_k: 60.0,
            per_stream_limit: 50,
        }
    }

    pub fn with_rrf_k(mut self, k: f64) -> Self {
        self.rrf_k = k;
        self
    }

    pub fn with_per_stream_limit(mut self, n: usize) -> Self {
        self.per_stream_limit = n;
        self
    }
}
