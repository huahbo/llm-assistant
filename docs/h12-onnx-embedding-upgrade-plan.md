# H12 — 本地 ONNX Embedding 升级实施计划

> **版本**：2026-05-17（Claude Opus 4.7 规划）
> **目标实施者**：Claude Sonnet 4.6
> **预计阶段**：9 个 Phase，约 600-900 行代码改动
> **测试基线起点**：268 通过（commit `dbdb854` 之后）

---

## 1. 背景与目标

### 1.1 现状
- `LlmProvider::embed()` 通过 HTTP 调用 Ollama `/api/embeddings` 端点
- 默认模型 `nomic-embed-text:latest`（768 维，~280MB Ollama 模型）
- **强依赖**：Ollama daemon 必须在 `localhost:11434` 运行
- 调用点 5 处：`ingest_service.rs:800` / `wiki_service.rs:1239,1524` / `ask_service.rs:225`
- 向量存储：`page_embeddings` 表（SQLite BLOB，无维度元数据）

### 1.2 目标
1. **App 完全独立**：卸载 Ollama 也能跑 wiki 摄入 + 语义检索
2. **内置 ONNX 嵌入引擎**：使用 `ort` crate + `tokenizers` crate，模型文件打包到安装器
3. **保留 Ollama 通路**：作为可选后端，老用户可切换回去
4. **向前兼容**：升级首次启动自动清空旧向量并重建索引
5. **冷启动 ≤ 3 秒**，单文档 embed ≤ 200ms（CPU）
6. **测试基线持续绿色**：268 → 280+（新增 ONNX 相关测试）

### 1.3 非目标
- ❌ GPU/DirectML 加速（首版 CPU only，后续 H12B 再做）
- ❌ 模型微调或本地训练
- ❌ 多模态 embedding（文本 only）
- ❌ 替换 LLM provider（LLM 仍走 OpenAI/Ollama）

---

## 2. 当前代码现状（已勘察）

### 2.1 关键文件清单

| 文件 | 关键函数/行 | 职责 |
|------|------------|------|
| `src-tauri/src/llm/provider.rs` | `LlmProvider::embed()` trait 方法 (line 90) | embed 接口定义 |
| `src-tauri/src/llm/ollama.rs` | `embed()` 实现 (line 357-392) | Ollama HTTP embed 调用 |
| `src-tauri/src/llm/mod.rs` | `pub mod ollama/openai/provider/types` | 模块树 |
| `src-tauri/src/state.rs` | `get_embed_provider()` (line 373-401) | 创建 OllamaProvider 实例 |
| `src-tauri/src/state/config_service.rs` | `embed_ollama_model` / `embed_ollama_base_url` 字段 | Settings 持久化 |
| `src-tauri/src/state/ingest_service.rs` | line 800-823 | 主嵌入入口（摄入流程） |
| `src-tauri/src/state/ask_service.rs` | line 225 | Query embedding |
| `src-tauri/src/state/wiki_service.rs` | line 1239, 1524 | Wiki 重嵌入 / save 后嵌入 |
| `src-tauri/src/db.rs` | `upsert_embedding`/`list_embeddings`/`decode_embedding_blob` (lines 253-322) | SQLite BLOB 读写 |
| `src-tauri/src/db.rs` | `page_embeddings` schema (line 1385-1388) | 向量表（**当前无 dim 列**） |
| `src-tauri/src/search.rs` | `rank_embedding_paths_by_cosine` | 余弦相似度排名 |
| `src-tauri/src/models.rs` | `EmbedConfig` / Settings 类型 | 前后端 type bridge |
| `src-tauri/tauri.conf.json` | `bundle` 字段 | 打包配置（需新增 resources） |
| `src-tauri/Cargo.toml` | 当前 25 行依赖 | 需新增 `ort`/`tokenizers`/`ndarray` |

### 2.2 当前 settings 持久化字段
```rust
embed_ollama_model: Option<String>      // 默认 "nomic-embed-text:latest"
embed_ollama_base_url: Option<String>   // 可为空，回退 ollama_base_url
```

### 2.3 当前向量 schema（无维度记录，需迁移）
```sql
CREATE TABLE IF NOT EXISTS page_embeddings (
    page_path     TEXT PRIMARY KEY,
    embedding_blob BLOB NOT NULL
);
```
**问题**：切换 ONNX 后维度从 768 → 384 时，旧 BLOB 无法用余弦相似度对齐，需清空 + 重建。

---

## 3. 架构设计

### 3.1 引入新 trait `EmbedProvider`（与 LlmProvider 解耦）

**为什么不让 ONNX 假装 LlmProvider？**
- `LlmProvider` 有 7 个方法（summarize / complete / complete_stream / chat_stream / health_check / model / base_url），ONNX 只关心 embed
- 让 ONNX 实现 6 个返回 "不支持" 的方法是噪声
- `EmbedProvider` 接口更窄，容易测试

**新 trait（`src-tauri/src/llm/embed_provider.rs`）**：

```rust
#[async_trait]
pub trait EmbedProvider: Send + Sync {
    /// 生成单条文本向量
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError>;

    /// 向量维度（用于 schema 校验）
    fn dimension(&self) -> usize;

    /// 后端标识，用于日志/Settings 显示，例如 "onnx:bge-small-zh-v1.5" / "ollama:nomic-embed-text"
    fn backend_id(&self) -> &str;

    /// 健康检查（启动时探测）
    async fn health_check(&self) -> bool;
}

#[derive(Debug, Clone, Serialize)]
pub enum EmbedError {
    InitFailed(String),
    InferenceFailed(String),
    InputTooLong { tokens: usize, limit: usize },
    Unavailable,
}
```

### 3.2 OllamaProvider 适配 EmbedProvider
保留现有 `LlmProvider::embed` 实现，新增 `impl EmbedProvider for OllamaProvider`，复用内部 HTTP 逻辑（或抽取私有方法）。

### 3.3 OnnxEmbedder 新建
**文件**：`src-tauri/src/llm/onnx_embed.rs`

```rust
pub struct OnnxEmbedder {
    session: ort::Session,         // ONNX Runtime session
    tokenizer: tokenizers::Tokenizer,
    model_dim: usize,              // 384 (e5-small) 或 512 (bge-small-zh)
    backend_id: String,            // "onnx:multilingual-e5-small"
    max_input_tokens: usize,       // 512
}

impl OnnxEmbedder {
    pub fn from_resource_dir(resource_dir: &Path, model_name: &str)
        -> Result<Self, EmbedError>;
}

#[async_trait]
impl EmbedProvider for OnnxEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        // 1. 截断到 max_input_tokens
        // 2. tokenizer.encode -> input_ids / attention_mask
        // 3. ort::inputs! { input_ids, attention_mask } -> session.run
        // 4. 取 last_hidden_state[CLS] 或 mean pooling（按模型选择）
        // 5. L2 归一化
        // 6. 返回 Vec<f32>，长度 == self.model_dim
    }
    fn dimension(&self) -> usize { self.model_dim }
    fn backend_id(&self) -> &str { &self.backend_id }
    async fn health_check(&self) -> bool { /* 跑一次 dummy embed */ }
}
```

### 3.4 state.rs 改动
```rust
pub struct AppState {
    inner: Arc<Mutex<AppStateInner>>,
    embed_provider: OnceLock<Arc<dyn EmbedProvider>>,   // 新增
    // ... existing fields
}

// get_embed_provider 改为返回 EmbedProvider
pub fn get_embed_provider(&self) -> Arc<dyn EmbedProvider> {
    self.embed_provider.get_or_init(|| self.init_embed_provider()).clone()
}

fn init_embed_provider(&self) -> Arc<dyn EmbedProvider> {
    let backend = self.read_embed_backend();  // "onnx" / "ollama" / "disabled"
    match backend.as_str() {
        "onnx" => {
            match OnnxEmbedder::from_resource_dir(&self.resource_dir(), &self.embed_model_name()) {
                Ok(e) => Arc::new(e),
                Err(err) => {
                    self.push_log(LogLevel::Warn,
                        format!("ONNX 加载失败，回退 Ollama: {}", err));
                    Arc::new(self.build_ollama_embed_provider())
                }
            }
        }
        "ollama" => Arc::new(self.build_ollama_embed_provider()),
        "disabled" => Arc::new(NoopEmbedder::new()),
        _ => Arc::new(self.build_ollama_embed_provider()),
    }
}
```

### 3.5 调用点改造（5 处）
所有 `state.get_embed_provider().embed(...)` 调用**保持不变**——只是底层返回类型变了，HTTP/ONNX 对调用方透明。

**例外**：`ingest_service.rs:823` 等地方的错误提示文案需要更新，从「Ollama 未启动」改为「Embed 服务不可用」。

### 3.6 维度迁移
**新 schema**：
```sql
CREATE TABLE IF NOT EXISTS page_embeddings (
    page_path      TEXT PRIMARY KEY,
    embedding_blob BLOB NOT NULL,
    backend_id     TEXT NOT NULL DEFAULT 'legacy',  -- 新增
    dim            INTEGER NOT NULL DEFAULT 0       -- 新增
);
```

**迁移逻辑**（在 `db::migrate()` 或类似入口）：
1. ALTER TABLE 加两列
2. 启动时检查 `backend_id` 与当前 provider.backend_id() 是否匹配
3. 不匹配 → 弹通知 "Embedding 后端已切换为 X，需重建索引（耗时 Y）" + 自动后台重建

**重建命令**：`rebuild_embeddings` Tauri command（已存在 `cleanup_orphan_embeddings`，可参考）

---

## 4. 模型选型

### 4.1 候选对比

| 模型 | 维度 | 文件大小 (ONNX FP32) | 输入长度 | 中文质量 | 英文质量 | 备注 |
|------|------|--------------------|---------|---------|---------|------|
| **multilingual-e5-small** ★ | 384 | ~120MB | 512 | 良好 | 良好 | **首选**，HuggingFace 有现成 ONNX |
| bge-small-zh-v1.5 | 512 | ~95MB | 512 | 优秀 | 一般 | 纯中文项目可选 |
| bge-small-en-v1.5 | 384 | ~95MB | 512 | 弱 | 优秀 | 纯英文项目 |
| bge-m3 | 1024 | ~2.2GB | 8192 | 优秀 | 优秀 | **太大**，安装包受影响 |
| multilingual-e5-base | 768 | ~280MB | 512 | 优秀 | 优秀 | 备选（中文 wiki 项目可考虑） |

### 4.2 决策
- **默认**：`intfloat/multilingual-e5-small`（HuggingFace 上的 `Xenova/multilingual-e5-small` 已有 ONNX 转换版）
- **理由**：中英兼顾、384 维体积适中、安装包增量 ~120MB 可接受
- **备选**：在 Settings 中允许切换 `bge-small-zh-v1.5`（用户中文偏多时）

### 4.3 模型下载脚本
**文件**：`scripts/download-embed-models.ps1`（Windows）+ `scripts/download-embed-models.sh`（macOS/Linux）

```powershell
# scripts/download-embed-models.ps1
$ErrorActionPreference = "Stop"
$models = @{
    "multilingual-e5-small" = @{
        repo = "Xenova/multilingual-e5-small"
        files = @("onnx/model.onnx", "tokenizer.json", "config.json")
    }
    # 可扩展 bge-small-zh-v1.5
}

foreach ($name in $models.Keys) {
    $dest = "src-tauri\resources\embed-models\$name"
    New-Item -ItemType Directory -Force $dest | Out-Null
    foreach ($f in $models[$name].files) {
        $url = "https://huggingface.co/$($models[$name].repo)/resolve/main/$f"
        $out = Join-Path $dest (Split-Path $f -Leaf)
        Write-Host "下载 $url -> $out"
        Invoke-WebRequest -Uri $url -OutFile $out
    }
}
```

模型放入 `src-tauri/resources/embed-models/multilingual-e5-small/`，通过 `tauri.conf.json` 的 `bundle.resources` 字段打包。

### 4.4 tauri.conf.json 修改
```jsonc
{
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/icon.ico"],
    "resources": ["resources/embed-models/**/*"]   // 新增
  }
}
```

---

## 5. 分阶段实施步骤

> **每个 Phase 结束**：`cargo test` + `npm run typecheck` 双绿；commit 一次。

### Phase 1 — 依赖与基础设施（1 commit）
**目标**：项目能编译 + 模型文件就位 + 不影响现有功能。

1. **新增 Cargo 依赖**（`src-tauri/Cargo.toml`）：
   ```toml
   ort = { version = "2.0.0-rc.10", features = ["load-dynamic"] }
   tokenizers = { version = "0.20", default-features = false, features = ["onig"] }
   ndarray = "0.16"
   ```
   **注意**：
   - `ort` 2.0 仍在 rc 阶段，**先用 `load-dynamic` feature**：不强制下载 onnxruntime 二进制，运行时再找 DLL
   - 或者 `ort = "1.16"` 稳定版（API 不同，需调整代码）。**推荐先尝试 ort 2.0-rc.10**，若构建报错再降级
   - `tokenizers` 去掉默认 features（`http`）以减小依赖体积

2. **下载模型脚本**：写 `scripts/download-embed-models.ps1` + `scripts/download-embed-models.sh`，运行后放入 `src-tauri/resources/embed-models/multilingual-e5-small/`。

3. **添加 `.gitignore`**（避免 ~120MB 模型入仓）：
   ```
   src-tauri/resources/embed-models/
   ```

4. **构建后脚本**（可选，Phase 7 再做）：CI 或 dev 启动前自动跑下载脚本。

5. **验证**：`cargo build` 通过，二进制大小无显著变化（动态库未链接）。

**风险点**：`ort` 2.0 API 在 rc 期可能变动。若超过 1 小时调不通，降级 `ort = "1.16"`。

### Phase 2 — EmbedProvider trait 抽象（1 commit）
**目标**：解耦 embedding 接口，OllamaProvider 仍工作。

1. **新建** `src-tauri/src/llm/embed_provider.rs`：定义 `EmbedProvider` trait + `EmbedError` 枚举（如 §3.1）。
2. **mod.rs 导出**：`pub mod embed_provider; pub use embed_provider::{EmbedProvider, EmbedError};`
3. **OllamaProvider 实现 EmbedProvider**：
   ```rust
   #[async_trait]
   impl EmbedProvider for OllamaProvider {
       async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
           LlmProvider::embed(self, text).await.map_err(|e| match e {
               LlmError::ConnectionFailed(m) => EmbedError::InitFailed(m),
               LlmError::Timeout => EmbedError::InferenceFailed("timeout".into()),
               LlmError::ModelNotFound(m) => EmbedError::InitFailed(format!("模型 {} 未拉取", m)),
               LlmError::InvalidResponse(m) => EmbedError::InferenceFailed(m),
           })
       }
       fn dimension(&self) -> usize { 768 }  // nomic-embed-text 固定 768；如果用户改了模型，可以做 lazy detect
       fn backend_id(&self) -> &str { /* 缓存为 String，return &str */ }
       async fn health_check(&self) -> bool { LlmProvider::health_check(self).await.unwrap_or(false) }
   }
   ```
4. **改 state.rs**：`get_embed_provider() -> Arc<dyn EmbedProvider>`（不再返回 `Arc<dyn LlmProvider>`）。
5. **改 5 处调用点**：方法签名不变（`.embed(&str)`），但错误类型从 `LlmError` 改为 `EmbedError`，更新错误日志文案。
6. **测试**：现有 268 个测试应全部仍绿（仅类型替换，行为不变）。

### Phase 3 — OnnxEmbedder 核心实现（1-2 commit）
**目标**：单元测试覆盖：tokenize → run → pool → normalize → 出向量。

1. **新建** `src-tauri/src/llm/onnx_embed.rs`：
   - `OnnxEmbedder::from_resource_dir(dir, model_name)`：加载 `model.onnx` + `tokenizer.json`
   - `pub fn embed_sync(&self, text: &str) -> Result<Vec<f32>, EmbedError>`（同步版本）
   - `impl EmbedProvider for OnnxEmbedder`：包裹 `embed_sync` 为 async（`tokio::task::spawn_blocking`）

2. **embedding 流程**（关键代码）：
   ```rust
   // 1. tokenize
   let encoding = self.tokenizer.encode(text, true)
       .map_err(|e| EmbedError::InferenceFailed(format!("tokenize: {}", e)))?;
   let mut ids: Vec<i64> = encoding.get_ids().iter().map(|x| *x as i64).collect();
   let mut mask: Vec<i64> = encoding.get_attention_mask().iter().map(|x| *x as i64).collect();

   // 2. truncate to max_input_tokens
   if ids.len() > self.max_input_tokens {
       ids.truncate(self.max_input_tokens);
       mask.truncate(self.max_input_tokens);
   }
   let seq_len = ids.len();

   // 3. run inference
   let ids_arr = ndarray::Array2::from_shape_vec((1, seq_len), ids)?;
   let mask_arr = ndarray::Array2::from_shape_vec((1, seq_len), mask.clone())?;
   let outputs = self.session.run(ort::inputs![
       "input_ids" => ids_arr.view(),
       "attention_mask" => mask_arr.view(),
   ]?)?;

   // 4. extract last_hidden_state -> shape [1, seq_len, hidden_dim]
   let hidden = outputs[0].try_extract_tensor::<f32>()?;
   let hidden_view = hidden.view();
   let shape = hidden_view.shape();  // [1, seq_len, hidden_dim]
   let hidden_dim = shape[2];

   // 5. mean pooling with mask（multilingual-e5 推荐 mean pooling）
   let mut sum = vec![0f32; hidden_dim];
   let mut cnt = 0f32;
   for i in 0..seq_len {
       if mask[i] == 1 {
           for d in 0..hidden_dim {
               sum[d] += hidden_view[[0, i, d]];
           }
           cnt += 1.0;
       }
   }
   if cnt > 0.0 {
       for d in 0..hidden_dim { sum[d] /= cnt; }
   }

   // 6. L2 normalize
   let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
   if norm > 0.0 {
       for d in 0..hidden_dim { sum[d] /= norm; }
   }

   Ok(sum)
   ```

3. **e5 模型的输入前缀**（关键细节）：
   - multilingual-e5 系列要求查询前加 `"query: "`，文档前加 `"passage: "`
   - 我们的 `embed()` 不区分场景。**决策**：统一加 `"passage: "`（因为大部分是 wiki 文档 embedding；查询少量、影响小）
   - 后续可加 `embed_query()` / `embed_passage()` 区分 API

4. **测试**：
   - `tests/onnx_embed_basic.rs`：用真实模型文件跑一次 embed，断言：
     - 返回向量长度 == 384
     - L2 norm ≈ 1.0
     - 两个不同文本的余弦相似度 < 1.0（不同）
     - 相同文本两次 embed 完全相等
   - **测试前提**：测试运行前模型文件必须存在。在 `tests/` 目录开头 `#[ignore]` 或检测路径不存在时 skip

### Phase 4 — state.rs 集成 + Settings 字段（1 commit）
**目标**：用户可在 Settings 切换 `embed_backend = onnx | ollama | disabled`。

1. **新增 Settings 字段**（`config_service.rs` + `models.rs`）：
   ```rust
   pub embed_backend: String,           // "onnx" | "ollama" | "disabled"，默认 "onnx"
   pub embed_onnx_model: String,         // "multilingual-e5-small"，默认值
   ```

2. **state.rs**：
   - 新增 `embed_provider: OnceLock<Arc<dyn EmbedProvider>>` 字段
   - `init_embed_provider()`：按 §3.4 实现
   - `resource_dir()`：通过 `tauri::AppHandle::path().resource_dir()` 取打包资源目录
   - **关键**：测试模式（不在 Tauri runtime 中）需提供 mock 路径

3. **NoopEmbedder**：当 `embed_backend = "disabled"`，返回 `Err(EmbedError::Unavailable)`，调用方已有降级（跳过 embed 召回，仅 FTS5）

4. **前端 Settings UI**（`web/src/modules/settings/`）：
   - 新增「向量后端」单选：内置 ONNX / Ollama / 禁用
   - 选 ONNX 时显示模型下拉（multilingual-e5-small / bge-small-zh-v1.5）
   - 选 Ollama 时显示现有 `embed_ollama_model` 输入框
   - 切换时弹确认对话框 "需要重建嵌入索引（约 X 分钟），是否继续？"

### Phase 5 — 维度迁移 + rebuild_embeddings 命令（1 commit）
**目标**：切换后端时自动/手动清空旧向量并重建。

1. **DB schema 迁移**（`db.rs`）：
   ```rust
   // 在 init_schema 或 migrate 加：
   conn.execute("ALTER TABLE page_embeddings ADD COLUMN backend_id TEXT NOT NULL DEFAULT 'legacy'", [])?;
   conn.execute("ALTER TABLE page_embeddings ADD COLUMN dim INTEGER NOT NULL DEFAULT 0", [])?;
   ```
   **注意**：`ALTER TABLE ADD COLUMN` 在 SQLite 是幂等的（重复执行报错），需 try/catch 或预查 `PRAGMA table_info`。

2. **upsert_embedding 改签名**：
   ```rust
   pub fn upsert_embedding(db_path: &Path, page_path: &str,
                            embedding: &[f32], backend_id: &str, dim: usize) -> Result<(), String>
   ```

3. **新命令 `rebuild_embeddings`**（`commands.rs` + `wiki_service.rs`）：
   - 遍历 `wiki_pages` 表，对每个 page 重新生成 embedding
   - emit `rebuild_progress { current, total }` 事件
   - 异步任务，可取消

4. **启动时自动检测**：
   - state 初始化后查 `SELECT DISTINCT backend_id FROM page_embeddings`
   - 若与当前 `embed_provider.backend_id()` 不一致 → emit 通知 `embed_backend_changed`
   - 前端弹卡片 "Embedding 后端已变更，建议重建索引" + "立即重建"按钮

5. **测试**：
   - `db::tests::upsert_embedding_with_backend_and_dim`
   - `state::tests::detect_backend_change` (mock)

### Phase 6 — Settings UI + 重建进度 UI（1 commit）
**目标**：前端体验闭环。

1. **`web/src/modules/settings/EmbeddingPanel.tsx`**：
   - 后端单选 + 模型下拉
   - 「重建索引」按钮 + 进度条
   - 当前状态卡片：「当前后端：ONNX (multilingual-e5-small, 384 维)；已索引页面：N」

2. **`tauri-client/embed.ts`**：
   - `rebuildEmbeddings()`: withTimeout(invoke, 600_000)（10 分钟兜底）
   - `listenRebuildProgress(handler)`: tauri event listener

3. **状态查询命令** `get_embed_status() -> EmbedStatus`：
   ```rust
   pub struct EmbedStatus {
       backend_id: String,
       dimension: usize,
       indexed_count: usize,
       healthy: bool,
   }
   ```

### Phase 7 — Tauri resource 打包 + 首次启动检测（1 commit）
**目标**：`npm run tauri:build` 生成的 .msi 包含模型文件。

1. **tauri.conf.json**：加 `bundle.resources: ["resources/embed-models/**/*"]`

2. **state.rs resource_dir()**：
   ```rust
   fn resolve_model_path(&self, app: &AppHandle, model_name: &str) -> PathBuf {
       app.path()
          .resource_dir()
          .expect("无法定位资源目录")
          .join("resources/embed-models")
          .join(model_name)
   }
   ```

3. **首次启动检测**：
   - `OnnxEmbedder::from_resource_dir` 若 `model.onnx` 不存在 → 返回 `EmbedError::InitFailed("模型文件缺失...")`
   - 自动回退 Ollama（已在 §3.4 处理）
   - 前端启动横幅提示 "未发现 ONNX 模型，使用 Ollama 后备"

4. **dev 模式**：脚本下载后放本地路径，state 应同时检测 `target/debug/resources/...` 与开发路径

### Phase 8 — 性能基线 + 集成测试（1 commit）
**目标**：建立性能 SLO；端到端测试覆盖。

1. **性能基线脚本**（`benches/embed_benchmark.rs` 或简单的 `examples/embed_perf.rs`）：
   - 单文档（500 字）embed：目标 < 200ms
   - 批量 100 文档：目标 < 15s
   - 模型加载冷启动：目标 < 3s

2. **集成测试**（`tests/integration_embed_e2e.rs`）：
   - 初始化 mock vault
   - 摄入 3 个 markdown 文件
   - 查询语义相似的 query
   - 断言 RRF 排名靠前的是预期文件

3. **基线提升**：268 → 280+

### Phase 9 — 文档更新 + memory 更新（1 commit）
**目标**：交接文档完整。

1. **更新文件**：
   - `docs/dev-status.md`：H12 状态 完成 → ONNX 升级完成（v2）
   - `docs/实施过程记录.md`：追加一条 H12-ONNX 实施记录
   - `README.md`（如有）：去 Ollama 依赖说明
   - `MEMORY.md` + `project_dev_status.md`：基线 280+，下一步 P22

2. **新增 `docs/embedding-architecture.md`**（可选）：架构图 + 模型对比 + 切换指南

---

## 6. 风险与降级方案

| 风险 | 等级 | 缓解策略 |
|------|------|---------|
| `ort` 2.0 rc API 不稳 | 高 | Phase 1 卡住超 1h 降级 ort 1.16；锁版本号 |
| onnxruntime DLL Windows 找不到 | 中 | `load-dynamic` + 启动时检测，自动从资源目录加载；MSI 打包包含 onnxruntime.dll |
| 模型文件 ~120MB 让 MSI 变大 | 中 | 选 multilingual-e5-small（最小），不嵌入 bge-m3 |
| 中文分词精度下降 | 中 | 默认 multilingual-e5-small 多语 token；选项可切 bge-small-zh-v1.5 |
| 用户已有 768 维向量失效 | 低 | rebuild_embeddings 命令 + UI 引导 |
| ONNX 首次加载慢（cold start 5s+） | 低 | OnceLock lazy init，第一次 embed 阻塞 + UI loading 指示 |
| 安全：onnxruntime CVE | 低 | 版本钉死、跟随上游更新 |

**总体降级链**：
```
OnnxEmbedder 初始化失败
   ↓ 回退
OllamaProvider 可用（Ollama daemon 在跑）
   ↓ Ollama 不可用回退
NoopEmbedder（embed 调用全部返回 EmbedError::Unavailable）
   ↓ 应用层降级
检索流程跳过 embedding 召回，仅用 FTS5
```

---

## 7. 验收清单

### 7.1 功能验收
- [ ] 卸载 Ollama 后，App 摄入 markdown 文件正常，能用语义检索查到
- [ ] Settings 切换后端，自动提示重建并跑通
- [ ] 切换到 `disabled`，仍可用 FTS5 检索（无报错）
- [ ] 首次启动若模型缺失，提示并回退 Ollama
- [ ] DeepSeek 调用 + ONNX embed 同时工作（LLM 与 Embed 完全独立）

### 7.2 性能验收
- [ ] 单文档 embed < 200ms（i5+ CPU）
- [ ] 重建 100 页索引 < 30 秒
- [ ] 冷启动到首次 embed 完成 < 3 秒

### 7.3 测试基线
- [ ] `cargo test`: 280+ 通过，0 失败
- [ ] `npm run typecheck`: 零错误
- [ ] 集成测试覆盖 e2e: 摄入 → embed → 检索

### 7.4 打包验收
- [ ] `npm run tauri:build` 成功
- [ ] .msi 安装包包含 onnxruntime.dll + 模型文件
- [ ] 安装到干净 Windows 机器，无 Ollama 也可用

---

## 8. 给实施 Agent 的关键提示

### 8.1 必读上下文
- `src-tauri/src/llm/ollama.rs` line 357-392（参考 embed 实现）
- `src-tauri/src/state.rs` line 373-401（get_embed_provider 现状）
- `src-tauri/src/db.rs` line 253-322（embedding BLOB 读写）
- `agents.md §18`（项目交接惯例）

### 8.2 防坑提醒
1. **ort 2.0 vs 1.x API 差异巨大**——若 Phase 1 卡 1 小时，立即换 1.16，不要硬刚
2. **e5 模型需要 `"passage: "` 前缀**——遗漏会导致向量空间错位、检索质量下降
3. **mean pooling 必须按 mask 加权**——否则 padding token 污染语义
4. **L2 normalize**——余弦相似度依赖单位向量；遗漏会让余弦变成内积，排序错乱
5. **SQLite `ALTER TABLE ADD COLUMN` 不幂等**——必须先查 `PRAGMA table_info`
6. **OnceLock + 测试模式**——测试不在 Tauri runtime，需提供 mock resource_dir 注入点
7. **tokio::task::spawn_blocking**——ONNX 推理是 CPU bound，必须用 spawn_blocking 避免阻塞 tokio runtime
8. **大模型文件不要入 git**——`.gitignore` 必加 `src-tauri/resources/embed-models/`

### 8.3 每个 Phase 结束动作
1. `cd src-tauri && cargo test`
2. `cd web && npm run typecheck`
3. `git add -A && git commit -m "feat(embed): Phase N — <描述>"`
4. **不要跨 Phase 提交**——失败时回退困难

### 8.4 实施过程中允许的自主决策
- ✅ 调整 trait 内部签名（保持公共行为）
- ✅ 拆分 Phase（如 Phase 3 太大可拆 3a/3b）
- ✅ 选择更优的 ndarray API
- ✅ 修复发现的 lint warning

### 8.5 必须征求用户确认的决策
- ❌ 改默认模型（multilingual-e5-small 是定的）
- ❌ 删除 Ollama 通路
- ❌ 跳过维度迁移
- ❌ 改测试基线门槛
- ❌ 用 web 下载替代 Tauri resources 打包

---

## 9. 时间预估

| Phase | 预估时长 | 累计 |
|-------|---------|------|
| 1 依赖 | 1-2h | 1-2h |
| 2 trait 抽象 | 1h | 2-3h |
| 3 OnnxEmbedder | 2-3h | 4-6h |
| 4 state 集成 | 1-2h | 5-8h |
| 5 维度迁移 | 1-2h | 6-10h |
| 6 Settings UI | 1-2h | 7-12h |
| 7 资源打包 | 1h | 8-13h |
| 8 性能 + e2e | 1-2h | 9-15h |
| 9 文档 | 0.5h | 9-16h |

**总计**：约 1-2 个工作日（Sonnet 4.6 实施）

---

## 10. 决策日志

| 日期 | 决策 | 理由 |
|------|------|------|
| 2026-05-17 | 默认模型 multilingual-e5-small | 中英兼顾 + 体积适中 |
| 2026-05-17 | ort 2.0-rc.10 优先，1.16 备选 | 拥抱新版 API + 风险可控 |
| 2026-05-17 | EmbedProvider trait 与 LlmProvider 分离 | 接口窄、ONNX 不需假装是 LLM |
| 2026-05-17 | Tauri resource 打包模型 | 用户体验好（首次启动可用），安装包大可接受 |
| 2026-05-17 | 保留 Ollama 通路 | 升级平滑，老用户不被迫迁移 |
| 2026-05-17 | rebuild_embeddings 自动触发 | 切换后端必须重建，否则维度不匹配崩溃 |
| 2026-05-17 | mean pooling + L2 normalize | e5 模型官方推荐 |
| 2026-05-17 | "passage: " 前缀 | e5 模型强制要求，否则向量分布偏移 |
