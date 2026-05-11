# H12：本地嵌入向量（ONNX + Ollama 双路）实施计划

> 状态：待实施 | 优先级：中高
> 依赖：Ollama Provider 层 ✅ | page_embeddings 表 ✅ | SQLite FTS5 ✅

---

## 1. 目标

在现有 FTS5 全文索引基础上叠加**语义向量检索**，实现"精确 + 语义"双路召回：

```
Query
  ├─→ FTS5（精确关键词匹配，现有功能）
  └─→ 向量检索（语义相似度召回，本轮新增）
        ↓
  RRF 融合排序（Reciprocal Rank Fusion）
        ↓
  注入 LLM context
```

向量来源支持两路，用户可在设置中选择：
- **Ollama Embed API**（默认，无额外依赖，利用现有 Provider 层）
- **ONNX Runtime**（可选，完全离线，feature-gated，需额外编译约 +163 crates）

---

## 2. 技术背景

### 2.1 已有基础设施（无需重建）

| 组件 | 位置 | 状态 |
|------|------|------|
| `page_embeddings` 表 | `src-tauri/src/db.rs` | ✅ 已存在（`page_path`, `embedding BLOB`） |
| `upsert_embedding()` | `src-tauri/src/db.rs` | ✅ 已实现 |
| Ollama Embed Provider | `src-tauri/src/llm/` | ✅ `get_embed_provider()` |
| `EmbedProvider` trait | `src-tauri/src/llm/` | ✅ 已有 trait 抽象 |

### 2.2 jcode 参考点

- jcode 的 `jcode-embedding` crate 用 ONNX runtime 做本地推理
- **ONNX 是 feature-gated**：`#[cfg(feature = "onnx")]`，关闭时无额外编译负担
- 模型：`all-MiniLM-L6-v2`（英语）或 `multilingual-e5-small`（多语言，适合中文 Wiki）
- 向量维度：384（all-MiniLM-L6-v2）
- 模型大小：~80MB（FP32），~40MB（INT8 量化）

---

## 3. 实施方案

### Phase A：Ollama 向量检索（默认路线，低风险）

#### 3A.1 向量化 Pipeline

```
Wiki 页面写入/更新（ingest）
  ↓
src-tauri/src/state.rs: 写入后异步触发 embed_page(path)
  ↓
embed_page():
  1. 读取页面内容（前 2000 字符，避免超出 embed 模型 token 限制）
  2. 调用 EmbedProvider::embed(content) → Vec<f32>
  3. 序列化为 BLOB（f32 小端字节序）
  4. upsert_embedding(db_path, path, embedding)
```

#### 3A.2 向量相似度检索

在 `src-tauri/src/search.rs` 中新增：

```rust
pub async fn search_by_vector(
    db_path: &Path,
    query: &str,
    limit: usize,
    embed_provider: &dyn EmbedProvider,
) -> Result<Vec<(String, f32)>> {
    let q_vec = embed_provider.embed(query).await?;
    let all = db::load_all_embeddings(db_path)?;
    let mut scored: Vec<(String, f32)> = all
        .into_iter()
        .map(|(path, emb)| (path, cosine_similarity(&q_vec, &emb)))
        .filter(|(_, s)| *s > 0.5)  // 阈值过滤，避免无关结果
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    Ok(scored.into_iter().take(limit).collect())
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() { return 0.0; }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { 0.0 } else { dot / (na * nb) }
}
```

#### 3A.3 双路 RRF 融合

```rust
pub async fn search_hybrid(
    db_path: &Path,
    query: &str,
    limit: usize,
    embed_provider: &dyn EmbedProvider,
) -> Result<Vec<SearchResult>> {
    let (fts_results, vec_results) = tokio::join!(
        search_fts5(db_path, query, limit * 2),
        search_by_vector(db_path, query, limit * 2, embed_provider),
    );
    // RRF: score = Σ 1/(k + rank_i), k=60
    let merged = rrf_merge(fts_results?, vec_results?, 60, limit);
    Ok(merged)
}
```

新增 Tauri 命令 `search_wiki_hybrid(query: String, limit: i64)`。

---

### Phase B：ONNX 本地推理（可选，feature-gated）

#### 3B.1 Cargo feature 设计

```toml
# src-tauri/Cargo.toml
[features]
default = []
local-embed = ["ort", "tokenizers"]  # 约 +163 crates

[dependencies]
ort = { version = "2", optional = true }
tokenizers = { version = "0.15", optional = true }
```

#### 3B.2 OnnxEmbedProvider

新建 `src-tauri/src/llm/onnx_embed.rs`：

```rust
#[cfg(feature = "local-embed")]
pub struct OnnxEmbedProvider {
    session: ort::Session,
    tokenizer: tokenizers::Tokenizer,
}

#[cfg(feature = "local-embed")]
impl EmbedProvider for OnnxEmbedProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self.tokenizer.encode(text, true)?;
        // ... ort inference → Vec<f32>
    }
}
```

#### 3B.3 模型自动下载

首次使用 `local-embed` feature 时，若模型不存在则：

```rust
pub async fn ensure_model_downloaded(model_dir: &Path) -> Result<()> {
    let model_path = model_dir.join("all-MiniLM-L6-v2.onnx");
    if model_path.exists() { return Ok(()); }
    // 从 HuggingFace Hub 下载（约 80MB）
    // 显示进度到前端（通过 tauri::EventEmitter）
}
```

---

### Phase C：EmbedProvider 工厂 + 设置 UI

#### 3C.1 工厂函数

```rust
pub fn get_embed_provider(config: &Config) -> Box<dyn EmbedProvider> {
    match config.embed_source.as_str() {
        "onnx" => {
            #[cfg(feature = "local-embed")]
            return Box::new(OnnxEmbedProvider::load(...));
            #[cfg(not(feature = "local-embed"))]
            panic!("local-embed feature not compiled");
        }
        _ => Box::new(OllamaEmbedProvider::new(&config.ollama_url)),
    }
}
```

#### 3C.2 Settings UI

在 `web/src/modules/settings/` 中新增"向量检索"配置区：

```
[ 嵌入来源 ]
  ○ Ollama（默认，需 Ollama 运行中）
  ● 本地 ONNX（离线，首次需下载模型）

[ Ollama 模型 ]  nomic-embed-text
[ 相似度阈值 ]   0.5

[ 立即为所有页面生成嵌入 ]  [重新索引]
```

---

## 4. 文件变动清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `src-tauri/src/db.rs` | 修改 | 新增 `load_all_embeddings()` 返回全量向量 |
| `src-tauri/src/search.rs` | 修改 | 新增 `search_by_vector()`, `search_hybrid()`, `rrf_merge()` |
| `src-tauri/src/state.rs` | 修改 | ingest 后异步触发 `embed_page()` |
| `src-tauri/src/commands.rs` | 修改 | 新增 `search_wiki_hybrid` 命令 |
| `src-tauri/src/llm/onnx_embed.rs` | 新建（可选） | `#[cfg(feature="local-embed")]` |
| `src-tauri/src/llm/mod.rs` | 修改 | 注册 onnx_embed 模块 |
| `src-tauri/Cargo.toml` | 修改 | `[features] local-embed = ["ort","tokenizers"]` |
| `web/src/modules/settings/` | 修改 | 向量检索配置 UI |
| `web/src/tauri-client/` | 修改 | 新增 `searchWikiHybrid()` 包装 |

---

## 5. 验收标准

- [ ] 新页面写入后，`page_embeddings` 表中出现对应记录
- [ ] 向量检索可召回语义相关但 FTS5 无法匹配的页面（如"人工智能"匹配含"AI"的页面）
- [ ] 双路 RRF 融合结果综合质量优于单路
- [ ] Ollama 不可用时，向量检索优雅降级（只走 FTS5，不报错）
- [ ] ONNX feature 未启用时，代码编译正常（无 `use` 报错）
- [ ] `cargo test` 全绿；`npm run typecheck` 零错误

---

## 6. 风险与注意事项

1. **大规模 `load_all_embeddings` 性能**：Wiki 页面超过 1000 条时，全量加载向量到内存可能耗时。可考虑只加载前 N 条（按最近访问排序），或实现近似最近邻（HNSW）索引——但对于 llm-wiki 的使用规模（<5000 页），全量加载足够。
2. **Ollama embed 模型选择**：`nomic-embed-text`（768 维，中英文效果好）vs `all-minilm`（384 维，更快）。维度必须与存储时一致，切换模型需要重新索引。
3. **ONNX 编译链**：Windows MSVC 环境下 `ort` crate 需要 ONNX Runtime 动态库（`.dll`），需打包进 Tauri 安装包。这是 Phase B 的主要工程复杂度。
4. **向量维度不一致保护**：`cosine_similarity` 中做 `len != len` 早返回，db 层存储时附带维度信息（或在配置中固定维度）。

---

## 7. 工作量估算

| Phase | 估算 | 关键风险 |
|-------|------|---------|
| A（Ollama + 双路检索） | 2 天 | 全量向量加载性能 |
| B（ONNX feature-gated） | 3-4 天 | Windows MSVC 编译链 |
| C（设置 UI） | 0.5 天 | 低风险 |
| **总计（仅 Phase A + C）** | **~2.5 天** | 推荐先只实现 Phase A |
| **总计（含 Phase B）** | **~6 天** | Phase B 建议独立评估后决策 |
