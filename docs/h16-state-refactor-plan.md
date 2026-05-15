# H16 — `state.rs` 拆分重构实施方案

> 目标读者：执行 Agent（Sonnet 4.6）
> 起草模型：Opus 4.7
> 日期：2026-05-13

---

## 1. 背景与目标

### 现状
- `src-tauri/src/state.rs`：**11495 行业务逻辑 + 3500+ 行测试**（共 15029 行）
- **125 个 `AppState` 公开方法**、205 个私有方法、153 个模块级自由函数
- 涵盖 11 个不相关业务领域：LLM/Config、Shell、Search、Wiki、Graph、Lint、Ingest、Ask、Agent、Chat、Research

### 痛点
1. 单文件加载慢，IDE 跳转/搜索性能差
2. 并行开发必产生 merge 冲突
3. 测试无法按领域隔离
4. 新功能（H10 MCP / H11 Swarm / H12 ONNX）每条都要改这个文件
5. 跨领域副作用难以追踪

### 目标
拆分为 11 个领域服务模块，`state.rs` 收敛到 **600-900 行**（仅保留 `AppState` 结构定义、共享 helper、薄包装方法）。
**API 表面保持不变** —— `commands.rs` 中所有 `state.method_name(...)` 调用零修改。

---

## 2. 架构决策

### 2.1 模块布局（核心决策）

采用 **submodule 模式**，不引入跨 crate 的服务对象：

```
src-tauri/src/
├── state.rs                  # AppState 结构 + 子模块声明 + 薄包装 + 共享 helper
├── state/                    # 新增目录（11 个领域模块）
│   ├── config_service.rs     # LLM/OCR/Vault 初始化/配置持久化
│   ├── shell_service.rs      # Shell 执行/会话/审计
│   ├── search_service.rs     # 级联 web search（SearXNG/Tavily/Brave/DDG）
│   ├── wiki_service.rs       # Wiki CRUD/历史/搜索/Frontmatter
│   ├── graph_service.rs      # 知识图谱构建
│   ├── lint_service.rs       # Lint 报告/补丁/语义 lint
│   ├── ingest_service.rs     # Markdown/PDF/URL/file 摄入 + 队列 + OCR
│   ├── ask_service.rs        # Query Ask 会话
│   ├── agent_service.rs      # Agent 运行/草稿/记忆/技能（合并现有 agent_service.rs）
│   ├── chat_service.rs       # Chat 取消令牌/写审批/MCP 客户端管理
│   └── research_service.rs   # Deep Research 任务
└── (其他模块: agent_chat/, agent_policy.rs, db.rs, llm.rs, models.rs, search.rs, vault.rs ...)
```

### 2.2 为什么选 submodule 而非 sibling 模块

| 维度 | submodule (`state::wiki_service`) | sibling (`crate::wiki_service`) |
|------|-----------------------------------|--------------------------------|
| 访问 AppState 私有字段 | ✅ 子模块直接可见 | ❌ 需暴露为 `pub(crate)` |
| 命名空间 | 一致 (`state::*`) | 散落顶层 |
| 与现有 `agent_service.rs` 兼容 | ⚠️ 需要移动 | ✅ 不动 |
| API 改动量 | 最小（仅内部组织） | 中等（需加公共字段或 getter） |

**决定**：用 submodule。**现有 `src/agent_service.rs` 一并并入 `state/agent_service.rs`**（保留文件名以减少混淆，只是改路径）。

### 2.3 函数迁移模式

**模式 A —— AppState 方法保留为薄包装（推荐）**

```rust
// state/wiki_service.rs
use super::AppState;
use crate::models::*;

pub fn save_wiki_page(
    state: &AppState,
    title: String,
    content: String,
) -> Result<NewPageResult, String> {
    // 原 save_wiki_page_impl 的完整实现，state.* 调用照旧
}

// state.rs 中保留：
impl AppState {
    pub async fn save_wiki_page_impl(
        &self,
        title: String,
        content: String,
    ) -> Result<NewPageResult, String> {
        wiki_service::save_wiki_page(self, title, content).await
    }
}
```

**保持薄包装的理由**：
- `commands.rs` 中 93 处 `state.xxx_impl(...)` 调用零修改
- 服务层逻辑可独立测试（`wiki_service::save_wiki_page(&state, ...)`）
- 后续可以逐步把 commands.rs 改为直接调用 `wiki_service::*`（非本次目标）

**模式 B —— 私有 helper 与领域强绑定**

私有 helper 函数（如 `search_wiki_matches`、`parse_wiki_frontmatter`）直接整段移到对应服务模块，不留薄包装。

**模式 C —— 跨领域共享 helper**

`current_timestamp_ms`、`md5_simple`、`friendly_display_path` 等多领域使用的工具留在 `state.rs` 顶层或新建 `state/util.rs`。

---

## 3. 分阶段迁移顺序

### 依赖关系图（关键）

```
        config_service ──────────┐
              ↓                  ↓
        shell_service     search_service
              ↓                  ↓
              └──────┬───────────┘
                     ↓
              wiki_service ←── graph_service
                     ↓
              ┌──────┼──────┬──────┬──────┐
              ↓      ↓      ↓      ↓      ↓
        lint_service  ingest  ask  agent  chat
                                  ↓        ↓
                            research_service
```

**迁移原则**：先迁移依赖最少的（叶子节点），最后迁移依赖最多的（汇聚节点）。

### 迁移序列（共 12 个阶段，每阶段独立提交）

| Phase | 模块 | 估计行数 | 依赖前置 | 风险 |
|-------|------|---------|---------|------|
| **0** | 基础设施（建目录、占位 mod.rs） | ~50 | 无 | 极低 |
| **1** | `config_service` | ~700 | Phase 0 | 低 |
| **2** | `shell_service` | ~550 | Phase 1 | 低 |
| **3** | `search_service` | ~580 | Phase 1 | 低 |
| **4** | `wiki_service` | ~1700 | Phase 1 | **中** |
| **5** | `graph_service` | ~240 | Phase 4 | 低 |
| **6** | `lint_service` | ~670 | Phase 4 | 中 |
| **7** | `ingest_service` | ~1400 | Phase 4 | **中** |
| **8** | `ask_service` | ~1150 | Phase 4 + 3 | 中 |
| **9** | `agent_service`（合并现有） | ~700 | Phase 4 | 中 |
| **10** | `chat_service` | ~200 | Phase 9 + 2 | 低 |
| **11** | `research_service` | ~400 | Phase 3 + 4 | 低 |
| **12** | 清理 + state.rs 收口 | -- | 全部 | 极低 |

**每阶段强制验证步骤**（不可跳过）：

```powershell
cd E:\llm-wiki\src-tauri
cargo build 2>&1 | Select-String "error"      # 必须无 error
cargo test 2>&1 | Select-Object -Last 5        # 必须 263 passed (基线)
cd ..\web
npm run typecheck                              # 必须零错误
```

**任一步失败 → 立即停止并回滚当前阶段（`git reset --hard HEAD`），分析根因后重试。**

---

## 4. Phase 详细方案

### Phase 0 — 基础设施

**目标**：建立目录结构和模块声明，验证编译通路。

**操作**：
1. 创建空目录 `src-tauri/src/state/`
2. 编辑 `state.rs`，在文件**顶部 `use` 后**追加：
   ```rust
   // 服务模块（H16 拆分）
   pub mod config_service;
   pub mod shell_service;
   pub mod search_service;
   pub mod wiki_service;
   pub mod graph_service;
   pub mod lint_service;
   pub mod ingest_service;
   pub mod ask_service;
   pub mod agent_service;
   pub mod chat_service;
   pub mod research_service;
   ```
3. 在 `state/` 下创建 11 个空文件，每个含一行：
   ```rust
   //! 占位：H16 Phase N 将填充内容
   ```
4. **验证**：`cargo build` 必须通过（空模块允许）；`cargo test` 必须 263 绿。
5. **commit**：`refactor(state): H16 Phase 0 — 建立服务模块目录骨架`

---

### Phase 1 — `config_service`（LLM + OCR + Vault + Config）

**搬迁范围**（按 state.rs 中行号定位，Sonnet 实施时以 grep 校验）：

| AppState 方法 | 大致行号 | 处理方式 |
|--------------|----------|---------|
| `get_llm_config` | 1001-1042 | 移到服务层 + 薄包装 |
| `set_llm_config` | 1042-1141 | 移到服务层 + 薄包装 |
| `get_ocr_config` / `set_ocr_config` | 1141-1191 | 移到服务层 + 薄包装 |
| `get_shell_policy_config` / `set_shell_policy_config` | 1191-1239 | 移到服务层 + 薄包装 |
| `set_mode` | 1239-1290 | 移到服务层 + 薄包装 |
| `init_vault` / `init_vault_with_template` | 1290-1425 | 移到服务层 + 薄包装 |
| `generate_summary` | 560-605 | 移到服务层 + 薄包装 |
| `llm_status_future` | 755-766 | 移到服务层 + 薄包装 |
| `overview` / `default_paths` / `query_settings` | 2478-2532 | 移到服务层 + 薄包装 |
| `recent_logs` | 2532-2537 | 移到服务层 + 薄包装 |

**私有方法 / 自由函数搬迁**：
- `get_ollama_provider` / `get_embed_provider`（430-560）
- `llm_status_input` / `llm_status_from_input`（605-755）
- `load_config` / `serialize_config_full` / `persist_config` / `write_config_file`（6003-6101）
- `default_config_path` / `project_root` / `default_config_path_from_root`（5987-6003）
- `set_vault_path`（5961-5987）
- 自由函数：`build_llm_status`、`normalize_cloud_provider_name`、`normalize_active_provider`、`resolve_active_provider`、`display_cloud_provider_name`、`provider_default_base_url`、`normalize_cloud_base_url`、`effective_cloud_base_url`、`llm_health_error_message`（10102-10231）
- 自由函数：`normalize_top_k`（9385）

**OCR 相关**：
- `OcrProvider` 枚举及其 `impl`（7501-7600+）
- `normalize_ocr_provider` / `resolve_ocr_provider_order`
- `extract_text_from_image_with_fallback` / `_with_provider` / `_with_tesseract` / `_with_paddle`
- `build_tesseract_command_candidates` / `run_tesseract_ocr_command` / `run_paddle_ocr_command`
- `format_tesseract_spawn_error` / `format_paddle_spawn_error`

**验证清单**：
- [ ] `cargo build` 无 error
- [ ] `cargo test` 263 passed
- [ ] `state.rs` 行数减少 ~700
- [ ] `commands.rs` 零修改
- [ ] commit message: `refactor(state): H16 Phase 1 — 抽取 config_service (LLM/OCR/Vault/Mode)`

---

### Phase 2 — `shell_service`

**搬迁范围**：

| AppState 方法 | 行号 |
|--------------|------|
| `run_shell_impl` | 6592-6835 |
| `create_shell_session_impl` | 6835-6868 |
| `close_shell_session_impl` | 6868-6878 |
| `approve_and_run_shell_impl` | 6878-7074 |
| `list_shell_audit_events_impl` | 7074-7084 |

**私有方法**：
- `resolve_shell_cwd` / `update_shell_cwd` / `emit_shell_stream_chunk`（7084-7136）

**自由函数**：
- `normalize_shell_command_for_executor` / `parse_cd_target` / `resolve_cd_target` / `rand_suffix`（7136-7220）
- `decode_shell_output_chunk` / `decode_utf16_chunk`（7220-7263）

**注意**：
- `ShellSessionState` 结构（141-145）保留在 state.rs（因为 `shell_sessions: Mutex<HashMap<String, ShellSessionState>>` 在 AppState 上）
- `classify_shell_policy_with_config` 在 `agent_policy.rs`，**不动**

**验证清单**：
- [ ] cargo build/test 通过
- [ ] state.rs 减少 ~550 行
- [ ] commit: `refactor(state): H16 Phase 2 — 抽取 shell_service`

---

### Phase 3 — `search_service`（Web Search）

**搬迁范围**：

| 内容 | 行号 |
|------|------|
| `get_search_config` / `set_search_config` | 311-330 |
| `search_web_cascade` / `_with_source` | 334-410 |
| `load_search_config_from_path` | 298-310 |
| `register_query_approval` / `approve_research_queries` | 390-417 |

**自由函数**：
- `search_tavily`（10231-10282）
- `normalize_searxng_base_url` / `searxng_base_root`（10282-10305）
- `detect_query_pref_language` / `build_searxng_search_params`（10305-10341）
- `parse_unresponsive_engines`（10341-10385）
- `search_searxng_endpoint_with_params` / `search_searxng`（10385-10537）
- `search_brave` / `search_powershell`（10537-10625）
- `url_hostname`（10625-10636）
- `do_search`（10686-10706）
- `normalize_search_provider` / `validate_search_config`（10734-10761）

**注意**：
- `SearxngSearchParams` 结构（10299）随之搬迁
- `search_config: Mutex<SearchConfig>` 字段仍在 AppState 上

**验证清单**：
- [ ] 同前
- [ ] commit: `refactor(state): H16 Phase 3 — 抽取 search_service (web 级联搜索)`

---

### Phase 4 — `wiki_service`（最大模块，分两次 commit）

**风险提示**：1700+ 行 + 大量私有 helper + frontmatter 解析逻辑。**Sonnet 实施时必须分两次 commit**：先方法迁移，再 helper 迁移。

#### Phase 4a — 主方法迁移

| AppState 方法 | 行号 |
|--------------|------|
| `recent_wiki_pages` | 2537-2567 |
| `search_wiki_pages` | 2583-2619 |
| `search_wiki_pages_hybrid` | 2619-2708 |
| `search_wiki_paths` | 2708-2728 |
| `wiki_page_detail` | 2728-2758 |
| `set_page_stale` | 2758-2799 |
| `wiki_page_citations` | 2799-2835 |
| `save_wiki_page_impl` | 3837-3958 |
| `list_wiki_page_history_impl` | 3958-3989 |
| `get_wiki_page_history_entry_impl` | 3989-4015 |
| `restore_wiki_page_from_history_impl` | 4015-4191 |
| `create_wiki_page_with_ai_impl` | 4191-4263 |
| `rename_wiki_page_impl` | 4263-4345 |
| `delete_wiki_page_impl` | 4345-4399 |
| `purge_orphaned_wiki_pages` | 4399-4457 |

**私有方法**：
- `generate_ai_wiki_markdown_draft_impl`（4037-4191）
- `update_related_pages_with_link`（2274-2396）
- `extract_entities`（2227-2274）

**验证**：cargo test 全绿后 commit。

#### Phase 4b — Helper 迁移

| 自由函数 | 行号 |
|---------|------|
| `WikiMatch` struct + 实现 | 9297-9390 |
| `is_stopword` / `normalize_top_k` | 9376-9391 |
| `search_wiki_matches` / `_with_fts` / `_rrf` / `_rrf_with_extra_routes` / `_from_paths` | 9391-9744 |
| `set_frontmatter_stale_field` | 9744-9791 |
| `parse_wiki_frontmatter` / `read_page_tags` / `extract_frontmatter_block` | 9791-9912 |
| `parse_frontmatter_scalar` / `unescape_yaml_double_quoted` | 9912-9948 |
| `extract_title_from_markdown` | 9948-9965 |
| `friendly_display_path` / `_str` | 9965-9983 |
| `pick_excerpt` / `trim_excerpt` | 9983-10014 |
| `resolve_unique_wiki_slug` / `wiki_title_from_content` / `extract_wiki_display_name` | 11374-11448 |
| `is_raw_ingest_id` / `resolve_graph_node_label` | 11448-11484 |
| `extract_markdown_h1_title` | 11365-11374 |

**注意 `normalize_top_k`**：被 `query_top_k` 设置使用，可能需要在 ask_service 中再次访问 —— 用 `pub(super) fn` 暴露给同级模块。

**验证**：
- [ ] state.rs 总共减少 ~1700 行
- [ ] commit: `refactor(state): H16 Phase 4 — 抽取 wiki_service (CRUD + 搜索 + Frontmatter)`

---

### Phase 5 — `graph_service`

**搬迁范围**：

| 内容 | 行号 |
|------|------|
| `get_knowledge_graph_impl` | 766-823 |
| `get_knowledge_subgraph_impl` | 823-1001 |

**依赖**：用到 `resolve_graph_node_label`（Phase 4b 已迁移，从 `wiki_service::resolve_graph_node_label` 调用）。

**验证**：
- [ ] commit: `refactor(state): H16 Phase 5 — 抽取 graph_service`

---

### Phase 6 — `lint_service`

**搬迁范围**：

| AppState 方法 | 行号 |
|--------------|------|
| `lint_report` | 2835-3138 |
| `preview_lint_patches` | 3138-3222 |
| `lint_report_full_future` | 3222-3237 |
| `quick_lint_page_impl` | 3237-3308 |
| `get_vault_stats_impl` | 3308-3320 |
| `run_lint_with_outbox` | 3320-3337 |
| `apply_lint_patch` | 3337-3504 |
| `apply_lint_patches_batch` | 3504-3570 |
| `recent_lint_patch_events` | 2567-2583 |

**私有方法**：
- `collect_semantic_lint_input` / `run_semantic_lint`（3156-3222）

**验证**：commit: `refactor(state): H16 Phase 6 — 抽取 lint_service`

---

### Phase 7 — `ingest_service`（含 OCR/Doc 提取）

**风险提示**：1400+ 行，含多种格式处理。**强烈建议分两次 commit**。

#### Phase 7a — 摄入方法主体

| AppState 方法 | 行号 |
|--------------|------|
| `ingest_markdown` | 1425-1491 |
| `preview_ingest_file` | 1491-1565 |
| `apply_ingest_preview` | 1565-1908 |
| `ingest_file_impl` | 1908-1963 |
| `read_file_for_chat_impl` | 1963-2016 |
| `ingest_pdf_impl` | 2016-2142 |
| `ingest_url_impl` | 2142-2227 |
| `enqueue_ingest` / `list_ingest_queue` / `cancel_ingest_item` / `retry_ingest_item` / `delete_ingest_item` | 6266-6326 |
| `get_page_embedding_similarities` | 6326-6367 |
| `start_queue_worker` | 6367-6474 |

**私有方法**：
- `complete_ingest_with_precomputed_analysis`（1657-1764）
- `estimate_related_pages_for_preview`（1764-1796）
- `load_preview_source_content`（1796-1824）
- `extract_preview_file_content_by_extension`（1824-1858）
- `extract_preview_pdf_text`（1858-1878）
- `fetch_preview_url_content`（1878-1908）
- `ingest_text_via_temp_markdown`（2098-2227）
- `default_ingest_source_path`（2511-2523）

**结构**：
- `CachedIngestPreview`（131-138）保留在 state.rs 内（因为 `ingest_previews: Mutex<...>` 字段）
- `PdfOcrFallbackOutput`（7965）随之搬迁

#### Phase 7b — 文档提取 helper

| 自由函数 | 行号 |
|---------|------|
| `validate_ingest_source_path` / `is_supported_image_extension` / `validate_pdf_source_path` | 7263-7311 |
| `extract_text_from_doc` / `is_doc_text_char` / `doc_scan_utf16le` / `doc_scan_ascii` | 7311-7410 |
| `extract_text_from_docx` / `extract_docx_paragraphs` | 7410-7448 |
| `extract_text_from_pptx` / `extract_slide_number` | 7448-7501 |
| `normalize_extracted_doc_text` / `open_zip_archive` | 7723-7800 |

**验证**：commit: `refactor(state): H16 Phase 7 — 抽取 ingest_service (摄入 + 文档解析)`

---

### Phase 8 — `ask_service`

**搬迁范围**：

| AppState 方法 | 行号 |
|--------------|------|
| `query_ask` | 3570-3630 |
| `query_ask_with_options` | 3630-3760 |
| `query_ask_session` | 5448-5729 |
| `set_query_top_k` | 3760-3799 |
| `save_query_answer` | 3799-3837 |
| `save_ask_history_impl` | 4457-4478 |
| `get_ask_history_impl` | 4478-4507 |
| `clear_ask_history_impl` | 4507-4523 |
| `create_ask_session_impl` | 4523-4556 |
| `list_ask_sessions_impl` | 4556-4585 |
| `list_ask_session_turns_impl` | 4585-4622 |
| `search_ask_session_turns_impl` | 4622-4664 |
| `rename_ask_session_impl` | 4664-4677 |
| `delete_ask_session_impl` | 4677-4702 |
| `get_outbox_events_impl` / `ack_outbox_events_impl` | 4702-4743 |
| `cancel_ask_session` / `clear_ask_session` | 5915-5961 |

**私有方法**：
- `query_embedding_route_paths`（3575-3630）
- `generate_query_answer_with_provider`（2396-2478）

**自由函数**：
- `build_query_prompt` / `build_query_prompt_with_history` / `build_query_answer`（10014-10102）

**验证**：commit: `refactor(state): H16 Phase 8 — 抽取 ask_service`

---

### Phase 9 — `agent_service`（合并现有 src/agent_service.rs）

**特殊操作**：现有 `src-tauri/src/agent_service.rs` 移动到 `src-tauri/src/state/agent_service.rs`。

**步骤**：
1. `git mv src-tauri/src/agent_service.rs src-tauri/src/state/agent_service.rs`
2. 更新 `main.rs`：删除 `mod agent_service;`（因已成为 state 子模块）
3. 更新 `state.rs`：`pub mod agent_service;` 已在 Phase 0 添加
4. 全 crate 替换：`use crate::agent_service` → `use crate::state::agent_service`
   - 至少需要更新 `state.rs`、`agent_loop.rs`、`commands.rs`

**搬迁 AppState 方法**：

| AppState 方法 | 行号 |
|--------------|------|
| `start_agent_run_impl` | 4743-4752 |
| `append_agent_run_event_impl` | 4752-4765 |
| `list_agent_runs_impl` | 4765-4790 |
| `list_agent_run_events_impl` | 4790-4813 |
| `complete_agent_run_impl` | 4813-4829 |
| `archive_agent_run_impl` | 4829-4854 |
| `restore_agent_run_impl` | 4854-4862 |
| `upsert_agent_memory_impl` | 4862-4884 |
| `list_agent_memories_impl` | 4884-4908 |
| `delete_agent_memory_impl` | 4908-4916 |
| `upsert_agent_skill_impl` | 4916-4937 |
| `list_agent_skills_impl` | 4937-4960 |
| `delete_agent_skill_impl` | 4960-4968 |
| `generate_agent_draft_impl` | 5117-5223 |
| `run_agent_task_impl` | 5223-5235 |
| `list_agent_drafts_impl` | 5235-5260 |
| `check_agent_draft_conflict_impl` | 5260-5298 |
| `approve_agent_draft_impl` | 5298-5373 |
| `rewrite_agent_draft_impl` | 5373-5448 |

**私有方法**：
- `load_and_maybe_compress_memories_impl` / `compress_memories_with_llm` / `extract_and_save_memories_from_draft`（4968-5117）

**自由函数**：
- `format_memories_for_prompt`（11350-11365）

**注意**：
- `store_pending_agent_write`（在 state.rs 其他位置）也要搬迁
- 现有 `agent_loop.rs` 中的 `use crate::agent_service` 改为 `use crate::state::agent_service`

**验证**：
- [ ] `agent_service.rs` 文件已物理移动
- [ ] `cargo test` 263 绿（含原有的 agent 任务测试）
- [ ] commit: `refactor(state): H16 Phase 9 — 合并 agent_service 入 state 子模块 + 抽取 agent 方法`

---

### Phase 10 — `chat_service`（MCP + 聊天审批）

**搬迁范围**：

| AppState 方法 | 行号 |
|--------------|------|
| `store_chat_cancel_token` / `cancel_chat_token` / `remove_chat_cancel_token` | 5729-5761 |
| `register_chat_write_approval` / `approve_chat_write` / `reject_chat_write` | 5761-5804 |
| `register_chat_shell_pending` / `approve_chat_shell_impl` / `reject_chat_shell_impl` | 5804-5873 |
| `spawn_mcp_client` / `stop_mcp_client` / `get_mcp_client` / `list_running_mcp_clients` | 5873-5915 |

**注意**：依赖 `agent_chat::mcp::McpClient` 和 `agent_chat::runtime::CancelToken`，**不要动这些上游模块**。

**验证**：commit: `refactor(state): H16 Phase 10 — 抽取 chat_service (MCP + 审批令牌)`

---

### Phase 11 — `research_service`

**搬迁范围**：

| AppState 方法 | 行号 |
|--------------|------|
| `start_research` | 6474-6512 |
| `list_research_tasks` | 6512-6522 |
| `get_research_task` | 6522-6536 |
| `cancel_research_task` | 6536-6547 |
| `delete_research_task` | 6547-6592 |

**自由函数**：
- `make_research_slug`（10636-10686）
- `compact_error_message` / `compact_llm_error` / `summarize_round_errors`（10706-10734）
- `save_research_output` / `report_research_failure`（10761-10915）
- `extract_tagged_content`（10915-10982）
- `start_research_task`（10982-11350）

**验证**：commit: `refactor(state): H16 Phase 11 — 抽取 research_service`

---

### Phase 12 — 收口清理

**目标**：state.rs 应只剩下：
1. `use` 语句
2. 常量定义（`STALE_PENDING_TASK_THRESHOLD_MS` 等）
3. 子模块声明（`pub mod ...`）
4. `AppState` struct + `AppStateData` struct
5. `CachedIngestPreview` / `ShellSessionState` 等 state 字段绑定的私有结构
6. `impl AppState { new(), new_with_path(), set_app_handle(), get_app_handle() }`
7. `impl Default for AppState`
8. `impl AppStateData { push_log }`
9. 跨领域共享 helper：`current_timestamp_ms`、`md5_simple`、`emit_progress`
10. 私有日志方法：`push_log`、`record_ingest_failed_event`、`record_outbox_event`、`record_lint_patch_event`（可选下沉到 `state/util.rs`）
11. 测试模块（`mod tests { ... }`）—— 暂时**全部保留**，后续可拆但本次不做

**预期最终行数**：state.rs 控制在 **600-900 行**（业务部分），加测试约 4000 行。

**操作清单**：
- [ ] 检查 state.rs 是否还有未迁移的方法：`grep "^    pub fn\|^    pub async fn" state.rs | wc -l` 应等于薄包装数量（即 Phase 1-11 中保留的）
- [ ] 检查 state.rs 自由函数：仅保留共享 helper
- [ ] 运行完整测试 `cargo test`：263 passed
- [ ] 运行 `cargo clippy`：无新增 warning
- [ ] `npm run typecheck`：零错误
- [ ] commit: `refactor(state): H16 Phase 12 — state.rs 收口清理（11495→<900 行）`

---

## 5. 风险控制

### 5.1 阶段独立性

每个 Phase 都必须满足：
- 单独编译通过（不依赖未完成的后续 phase）
- 单独 `cargo test` 全绿
- 单独 commit，可回退

### 5.2 公共可见性原则

- 服务模块对外公开的函数用 `pub` (在 state 命名空间内可见)
- 服务模块之间需要共享的 helper 用 `pub(super)`（只在 state 子树内可见）
- 不可暴露任何 AppState 私有字段为 `pub` —— 仅 `pub(crate)` 或保持私有

### 5.3 测试基线

**当前基线**：263 tests passed（已确认）。
**每个 Phase 后必须维持 263 绿**。任何测试数量减少或失败 → 必须修复或回滚。

### 5.4 边界情况预案

| 情况 | 处理 |
|------|------|
| 某方法跨多个领域（如同时操作 wiki 和 agent） | 优先放在"主导"领域，跨域调用通过 `&state` 走另一服务的 pub 函数 |
| `AppStateData` 字段需要被多个服务访问 | 不动字段定义，通过 `state.inner.lock()` 在各服务中直接访问 |
| 测试中引用了被搬迁的私有函数 | 用 `pub(crate)` 或暴露 `#[cfg(test)] pub` 适配 |
| 循环依赖（A 服务 → B → A） | 把循环边界的函数下沉到 `state/util.rs` |
| 自由函数被多个服务共享 | 放在 `state/util.rs` 或保留在 `state.rs` 顶层 |

### 5.5 回滚策略

任一 Phase 失败：
```powershell
git reset --hard HEAD~1     # 回退当前 Phase 的 commit
# 分析失败原因，调整方案后重新执行
```

**禁止跨 Phase 修复**：发现 Phase N 有问题就在 Phase N 修，不要带着隐患进入 Phase N+1。

---

## 6. Sonnet 执行指引

### 6.1 进入工作前确认

- [ ] 读完本文档全文
- [ ] 当前 git 状态 clean（`git status` 无未提交修改）
- [ ] 已运行基线测试 `cd src-tauri && cargo test`，确认 263 passed
- [ ] 已记下基线测试数量（用于每 Phase 校验）

### 6.2 每 Phase 操作模板

```
1. 读取本文档对应 Phase 章节
2. 用 Grep 确认 state.rs 中目标方法/函数的当前行号（行号会随之前 Phase 漂移）
3. 创建新服务文件 / 移动代码
4. 在 state.rs 中保留薄包装方法（如 Phase 描述）
5. 运行 cargo build —— 必须无 error
6. 运行 cargo test —— 必须 263 passed
7. 运行 npm run typecheck —— 必须零错误
8. git add + commit（消息严格按 Phase 描述）
9. 进入下一 Phase
```

### 6.3 行号漂移处理

由于 Phase 之间会移动大块代码，**Phase N+1 开始前必须重新 grep 实际行号**，不要直接套用本文档行号。

行号校验命令模板：
```powershell
Select-String -Path "src-tauri/src/state.rs" -Pattern "pub fn 方法名"
```

### 6.4 卡住时

如果某 Phase 出现：
- 编译错误难以解决
- 测试失败原因不明
- 跨服务循环依赖

**立即 `git reset --hard HEAD`** 回到 Phase 起点，把问题写到 `docs/h16-issues.md` 等待用户决策。**不要**：
- 强行编辑测试代码绕过失败
- 用 `#[allow]` 屏蔽 warning
- 跳过当前 Phase 进入下一 Phase

---

## 7. 完成后产出

执行完毕后应满足：

| 指标 | 期望值 |
|------|--------|
| state.rs 业务部分行数 | < 900 行 |
| 新增服务模块数 | 11 个 |
| 单文件最大行数 | < 2000 行（wiki_service 估计最大） |
| cargo test 通过数 | 263（与基线一致）|
| typecheck 错误 | 0 |
| commit 数量 | 12-14（每 Phase 1-2 个）|
| 顶层 `agent_service.rs` 文件 | 已移动至 `state/agent_service.rs` |

**最终汇报格式**：

```markdown
# H16 state.rs 拆分完成报告

- state.rs: 11495 → XXX 行
- 新增模块: 11 个，合计 XXXX 行
- 测试基线: 263/263 ✅
- 已 push 至 origin/main: ✅
- commits: <range>

## 风险残留
- (如有)

## 后续建议
- (如 H17 commands.rs 收编建议、服务层单元测试规划)
```

---

## 8. 不在本次范围内的事项

明确**不做**的事：
- ❌ 修改 `commands.rs` 任何代码（保留 API 表面）
- ❌ 修改 `agent_chat/` 目录下任何代码
- ❌ 修改 `agent_loop.rs` / `agent_policy.rs` / `agent_runtime.rs` / `agent_tools.rs` 业务逻辑（除非纯路径修正）
- ❌ 修改 `db.rs` / `models.rs` / `llm.rs` / `search.rs` / `vault.rs`
- ❌ 任何前端代码改动（仅 typecheck 验证）
- ❌ 引入新依赖（Cargo.toml 不动）
- ❌ 拆分测试代码（保留在 state.rs `mod tests` 内）

如执行中发现这些范围外的修改必要，**停下来汇报**。
