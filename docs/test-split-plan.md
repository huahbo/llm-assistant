# Test Split Refactoring Plan — state.rs → service modules

**Status**: Pending approval  
**Scope**: `src-tauri/src/state.rs` test module (~3565 lines) → 11 service files  
**Constraint**: Zero functional change; `cargo test` must stay green after every batch  
**Date**: 2026-05-16

---

## 1. Current State

`state.rs` contains a single `#[cfg(test)] mod tests` block spanning lines 2039–5603:

| Item | Count |
|------|-------|
| Total test lines | ~3565 |
| Test functions (`#[test]` / `#[tokio::test]`) | 136 |
| Shared test helpers (MockQueryProvider, TempDirGuard, etc.) | 6 items |
| Cross-service integration tests (cannot move) | 2 |

All service files (`state/*.rs`) currently have zero test code.

---

## 2. Goal

| Location | Before | After |
|----------|--------|-------|
| `state.rs` test module | ~3565 lines | ~200 lines |
| `state/test_helpers.rs` | — (new file) | ~120 lines |
| Each service file | 0 | own `mod tests` block |

Total test count stays at **268** (including `agent_chat/db.rs` tests).

---

## 3. New File: `state/test_helpers.rs`

**Step 0 (first)** — create this file before any other moves.

### 3.1 Contents to extract from `state.rs`

| Item | Current location | Rationale |
|------|-----------------|-----------|
| `MockQueryProvider` struct + impls | state.rs:2360–2395 | used by `make_test_state` and several agent/ask tests directly |
| `TempDirGuard` struct + `Drop` | state.rs:4221–4227 | used in ~60 tests across all services |
| `fn make_temp_dir` | state.rs:4229–4238 | universal temp dir creator |
| `fn make_test_state_bare` | state.rs:4240–4276 | base AppState fixture |
| `fn make_test_state` | state.rs:4278–4286 | AppState fixture with MockQueryProvider |
| `fn assert_paths_semantically_equal` | state.rs:4288–4297 | path comparison helper used in wiki tests |

### 3.2 File skeleton

```rust
// state/test_helpers.rs — shared test utilities; compiled only under #[cfg(test)]
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use async_trait::async_trait;
use crate::llm::{LlmError, LlmProvider};
use crate::models::SearchConfig;
use crate::state::{AppState, AppStateData, AppMode, ShellPolicyConfig, QUERY_TOP_K_DEFAULT};

pub(crate) struct MockQueryProvider { ... }
// impl MockQueryProvider + impl LlmProvider for MockQueryProvider

pub(crate) struct TempDirGuard(pub PathBuf);
impl Drop for TempDirGuard { ... }

pub(crate) fn make_temp_dir(prefix: &str) -> PathBuf { ... }
pub(crate) fn make_test_state_bare(vault_dir: &Path) -> AppState { ... }
pub(crate) fn make_test_state(vault_dir: &Path) -> AppState { ... }
pub(crate) fn assert_paths_semantically_equal(expected: &Path, actual: &str) { ... }
```

### 3.3 Declaration in `state/mod.rs`

Add after the existing `pub(crate) mod` list:

```rust
#[cfg(test)]
pub(crate) mod test_helpers;
```

### 3.4 Import pattern in service test modules

```rust
#[cfg(test)]
mod tests {
    use super::*;                               // service's own pub(super) fns
    use crate::state::test_helpers::*;          // TempDirGuard, make_test_state, …
    // service-specific additional imports
}
```

---

## 4. Test Classification

### Category A — Pure function tests (no AppState)

Call only `pub(super)` helper functions; no I/O beyond temp files the function creates itself. These require only `use super::*` and standard imports.

### Category B — AppState single-service tests

Use `make_test_state` / `make_test_state_bare` + `init_vault`. Restricted to one service domain. Require `use crate::state::test_helpers::*`.

### Category C — Cross-service integration tests (stay in state.rs)

Touch ≥2 service domains in a single assertion chain. Keep in state.rs with shortened imports.

---

## 5. Migration Phases

Phases are ordered: pure-only services first (less import churn), then mixed, then AppState-heavy. Run `cargo test` after completing **each phase**; do not batch phases.

---

### Phase 0 — Create `state/test_helpers.rs` ✦ prerequisite

Actions:
1. Create `src-tauri/src/state/test_helpers.rs` with all items from §3.2.
2. In `state/mod.rs`, add `#[cfg(test)] pub(crate) mod test_helpers;`.
3. In `state.rs` test module, replace the inline definitions of all 6 items with `use crate::state::test_helpers::*;`.
4. Run `cargo test` — all 268 tests must pass.

No tests move yet; only refactoring the helper location.

---

### Phase 1 — `research_service.rs` (25 tests)

**Pure (18):**

| Test function |
|---------------|
| `parse_learnings_standard_format` |
| `parse_learnings_markdown_bold` |
| `parse_learnings_list_prefix` |
| `parse_learnings_numbered_list` |
| `parse_learnings_lowercase_tag` |
| `parse_learnings_mixed_formats` |
| `parse_learnings_fallback_on_unstructured_output` |
| `parse_learnings_empty_input` |
| `parse_learnings_skips_empty_content_after_tag` |
| `make_research_slug_ascii` |
| `make_research_slug_deduplicates_dashes` |
| `make_research_slug_trims_dashes` |
| `make_research_slug_unicode_becomes_dashes` |
| `make_research_slug_max_50_chars` |
| `strip_think_tags_removes_think_block` |
| `strip_think_tags_removes_thinking_block` |
| `strip_think_tags_unclosed_tag_removes_to_end` |
| `strip_think_tags_no_tags_unchanged` |

**DB helper + DB tests (7 + helper):**

`make_research_db` is a research-specific helper returning `(PathBuf, Connection, impl Drop)`.  
Move it to `research_service.rs` tests (not test_helpers.rs — too specialized).

| Test function |
|---------------|
| `research_task_create_and_list` |
| `research_task_update_to_done` |
| `research_task_cancel_changes_queued_to_cancelled` |
| `research_task_cancel_is_idempotent_on_done` |
| `research_task_delete_removes_row` |
| `research_task_delete_missing_returns_error` |
| `research_task_cancel_is_idempotent_on_already_cancelled` |

**Additional imports needed:**
```rust
use rusqlite::{params, Connection};
use crate::models::ResearchTaskItem;
```

---

### Phase 2 — `search_service.rs` (9 tests)

All pure; no AppState.

| Test function |
|---------------|
| `normalize_searxng_base_url_adds_http_when_missing_scheme` |
| `normalize_searxng_base_url_keeps_https` |
| `searxng_base_root_strips_search_suffix` |
| `detect_query_pref_language_prefers_zh_for_cjk` |
| `detect_query_pref_language_uses_auto_for_latin` |
| `build_searxng_search_params_contains_all_fallback` |
| `parse_unresponsive_engines_supports_string_and_object` |
| `validate_search_config_rejects_missing_searxng_url` |
| `validate_search_config_accepts_valid_searxng_url` |

**Additional imports needed:**
```rust
use crate::models::SearchConfig;
```

---

### Phase 3 — `ingest_service.rs` (25 tests)

**Pure (17):**

| Test function |
|---------------|
| `validate_pdf_source_path_rejects_missing_file` |
| `validate_pdf_source_path_rejects_non_pdf_extension` |
| `validate_pdf_source_path_rejects_directory` |
| `validate_pdf_source_path_accepts_uppercase_pdf_extension` |
| `normalize_ocr_provider_falls_back_to_tesseract_on_invalid_value` |
| `resolve_ocr_provider_order_matches_expected_fallback_sequence` |
| `extract_xml_text_by_tag_reads_docx_minimal_sample` |
| `extract_xml_text_by_tag_reads_pptx_minimal_sample` |
| `format_tesseract_spawn_error_returns_readable_message_when_missing` |
| `extract_text_from_pdf_operations_extracts_simple_text` |
| `extract_text_from_pdf_raw_streams_extracts_text_from_flate_stream` |
| `decode_pdf_stream_candidates_supports_trailing_newline` |
| `should_fallback_to_pdf_ocr_matches_supported_error_patterns` |
| `build_pdf_ocr_fallback_failure_message_contains_install_hints` |
| `find_subsequence_returns_expected_offsets` |
| `test_extract_slide_number_natural_sort` |
| `test_extract_docx_paragraphs_preserves_paragraph_breaks` |

**AppState (8):**

| Test function |
|---------------|
| `validate_pdf_source_path_rejects_directory` *(already above — confirm pure)* |
| `ingest_file_impl_rejects_unsupported_extension` |
| `apply_ingest_preview_returns_error_when_preview_id_missing` |
| `preview_ingest_file_then_apply_succeeds_and_consumes_cache` |
| `ingest_pdf_impl_rejects_invalid_pdf_content_with_readable_error` |
| `default_paths_point_to_project_root_targets` |
| `ingest_queue_stale_running_reset_on_vault_init` |
| `init_vault_with_template_rejects_path_traversal` |
| `init_vault_with_template_rejects_oversized_content` |

**Additional imports needed:**
```rust
use crate::state::test_helpers::*;
use crate::models::OcrProvider;
use rusqlite::params;
use std::fs;
```

Note: async tests (`ingest_file_impl_*`, `apply_ingest_preview_*`, `preview_ingest_file_*`, `ingest_pdf_impl_*`) use `tokio::runtime::Runtime::new().unwrap().block_on(...)` — copy this pattern from current state.rs code rather than `#[tokio::test]`.

---

### Phase 4 — `lint_service.rs` (17 tests)

**Pure (5):**

| Test function |
|---------------|
| `parse_semantic_lint_response_parses_valid_lines` |
| `parse_semantic_lint_response_rejects_invalid_codes` |
| `parse_semantic_lint_response_handles_no_issues` |
| `parse_semantic_lint_response_caps_at_ten` |
| `merge_lint_with_semantic_updates_stats_and_summary` |

**AppState (12):**

| Test function |
|---------------|
| `lint_report_defaults_severity_stats_when_uninitialized` |
| `preview_lint_patches_returns_uninitialized_vault_suggestion` |
| `apply_lint_patch_supports_orphan_wiki_page_and_writes_log` |
| `apply_lint_patch_supports_missing_index_entry_and_appends_link` |
| `apply_lint_patch_records_event_and_recent_query_returns_latest` |
| `apply_lint_patches_batch_summarizes_success_and_failure` |
| `apply_lint_patch_rejects_unsupported_issue_code` |
| `lint_report_detects_missing_index_entries_orphans_and_db_mismatches` |
| `lint_report_detects_wikilink_level_broken_orphan_and_xref_missing` |
| `apply_lint_patch_supports_broken_wikilink_and_xref_missing` |
| `preview_lint_patches_total_matches_suggestions_for_multiple_issues` |
| `lint_report_flags_stale_pending_tasks` |

**Additional imports needed:**
```rust
use crate::state::test_helpers::*;
use crate::models::{LintIssue, LintPatchBatchApplyItemResult, LintPatchBatchApplyStatus};
use rusqlite::params;
use std::fs;
```

⚠️ `lint_report` and `apply_lint_patch_*` call `state.lint_report()` which is gated `#[cfg(test)]` in `state/mod.rs`. Verify that visibility is `pub(crate)` so service test modules can call it.

---

### Phase 5 — `config_service.rs` (7 tests)

**Pure (4):**

| Test function |
|---------------|
| `llm_health_error_message_maps_known_errors` |
| `build_llm_status_formats_expected_fields` |
| `load_config_compatibly_reads_legacy_openai_fields` |
| `llm_status_input_prefers_ollama_when_active_provider_is_ollama_in_hybrid` |

**AppState (3):**

| Test function |
|---------------|
| `default_config_path_points_to_project_root_runtime_dir` |
| `provider_aliases_are_canonicalized_and_default_urls_are_derived` |
| `set_llm_config_falls_back_to_ollama_when_cloud_selected_without_key` |

**Additional imports needed:**
```rust
use crate::state::test_helpers::*;
use crate::models::AppConfig;
use std::path::Path;
```

---

### Phase 6 — `wiki_service.rs` (38 tests)

This is the largest service batch. Split into two sub-phases for safety:

**Phase 6a — Pure (15 tests):**

| Test function |
|---------------|
| `is_raw_ingest_id_detects_timestamp_pattern` |
| `resolve_graph_node_label_preserves_meaningful_title` |
| `resolve_graph_node_label_uses_first_entity_for_raw_id` |
| `resolve_graph_node_label_falls_back_to_source_stem` |
| `resolve_graph_node_label_skips_internal_source_path` |
| `friendly_display_path_str_strips_windows_verbatim_prefix` |
| `prune_missing_index_links_from_content_removes_only_missing_targets` |
| `search_wiki_matches_with_fts_prefers_fts_strategy` |
| `tokenize_query_supports_cjk_segments_and_bigrams` |
| `tokenize_query_filters_stopwords` |
| `search_wiki_matches_from_paths_applies_phrase_title_boost_and_deduplicates` |
| `search_wiki_matches_rrf_degrades_gracefully_on_empty_vault` |
| `search_wiki_matches_rrf_accepts_embedding_extra_route` |
| `set_frontmatter_stale_field_adds_stale_true` |
| `set_frontmatter_stale_field_removes_stale_on_false` |

Run `cargo test` after 6a before proceeding.

**Phase 6b — AppState (23 tests):**

| Test function |
|---------------|
| `prune_missing_index_links_updates_file_and_returns_removed_count` |
| `recent_wiki_pages_requires_initialized_vault` |
| `recent_wiki_pages_returns_db_rows` |
| `search_wiki_pages_filters_rows` |
| `wiki_page_detail_requires_initialized_vault` |
| `wiki_page_detail_reads_markdown_content` |
| `wiki_page_detail_parses_frontmatter_fields` |
| `wiki_page_detail_accepts_wiki_relative_path` |
| `wiki_page_detail_rejects_outside_wiki_root` |
| `wiki_page_citations_requires_initialized_vault` |
| `wiki_page_citations_returns_rows_and_target_existence_flags` |
| `wiki_page_citations_rejects_outside_wiki_root` |
| `save_query_answer_requires_initialized_vault` |
| `save_query_answer_writes_wiki_file_and_updates_db` |
| `get_page_embedding_similarities_returns_high_sim_pairs` |
| `get_page_embedding_similarities_filters_to_requested_paths` |
| `test_wiki_page_detail_content_field_roundtrip` |
| `save_wiki_page_records_previous_content_history` |
| `restore_wiki_page_from_history_replaces_content` |
| `save_wiki_page_checksum_mismatch_rejected` |
| `save_wiki_page_checksum_match_accepted` |

**Additional imports needed:**
```rust
use crate::state::test_helpers::*;
use crate::models::{WikiPageFrontmatter, QueryCitation, WikiPageDetail};
use rusqlite::{params, Connection};
use std::{fs, path::{Path, PathBuf}};
```

---

### Phase 7 — `ask_service.rs` (8 tests)

| Test function | Category |
|---------------|----------|
| `query_answer_result_defaults_missing_search_strategy` | Pure |
| `query_ask_rejects_empty_question` | AppState |
| `query_ask_requires_initialized_vault` | AppState |
| `query_ask_with_options_applies_top_k_clamp` | AppState async |
| `set_query_top_k_persists_to_runtime_config` | AppState |
| `query_ask_with_options_uses_persisted_default_top_k` | AppState async |
| `generate_query_answer_with_provider_uses_llm_strategy_and_prompt` | AppState (uses MockQueryProvider directly) |
| `generate_query_answer_with_provider_falls_back_to_rule_on_empty_response` | AppState |
| `clear_ask_session_removes_history` | AppState |
| `cancel_ask_session_noop_when_no_flag` | AppState |

**Note:** `generate_query_answer_with_provider` is gated `#[cfg(test)]` in state.rs. Verify it is `pub(crate)` so ask_service tests can call it.

**Additional imports needed:**
```rust
use crate::state::test_helpers::*;
use crate::models::{AskTurn, QueryAnswerResult};
use std::sync::{Arc, Mutex};
```

---

### Phase 8 — `graph_service.rs` (3 tests)

| Test function |
|---------------|
| `get_knowledge_graph_returns_err_when_no_vault` |
| `get_knowledge_subgraph_returns_err_when_no_vault` |
| `get_knowledge_subgraph_respects_direction_and_hop` |

**Additional imports needed:**
```rust
use crate::state::test_helpers::*;
use crate::state::KnowledgeGraphDirection;
use rusqlite::params;
use std::{fs, time::{SystemTime, UNIX_EPOCH}};
```

---

### Phase 9 — `agent_service.rs` (12 tests)

| Test function |
|---------------|
| `agent_run_h0_impl_lifecycle_works` |
| `agent_draft_generate_and_approve_impl_works` |
| `agent_draft_generate_with_skill_injects_skill_prompt` |
| `agent_skill_crud_impl_works` |
| `check_agent_draft_conflict_returns_no_conflict_when_page_absent` |
| `archive_agent_run_rejects_running_status` |
| `archive_agent_run_rejects_when_pending_write_exists` |
| `archive_and_restore_agent_run_round_trip` |
| `approve_agent_write_full_write_creates_file` |
| `reject_agent_write_does_not_create_file` |
| `approve_agent_write_patch_replaces_content` |
| `approve_agent_write_patch_fails_when_old_str_not_found` |

These tests use `MockQueryProvider` directly (not just via `make_test_state`), so they need `MockQueryProvider` exported from test_helpers as `pub(crate)`.

**Additional imports needed:**
```rust
use crate::state::test_helpers::*;
use std::sync::{Arc, Mutex};
```

---

### Phase 10 — `state.rs` — Keep (2 tests)

| Test function | Why kept |
|---------------|----------|
| `create_wiki_page_slug_generation_works` | Tests `topic_to_slug` defined in `state.rs` (line 2007), not a service function |
| `query_ask_returns_matches_with_citations` | Exercises ask + wiki + search in a single assertion chain; cross-service integration |

After all phases, the state.rs test module becomes:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::test_helpers::*;
    use crate::models::QueryCitation;
    use std::{fs, path::PathBuf};

    #[test]
    fn create_wiki_page_slug_generation_works() { ... }

    #[test]
    async fn query_ask_returns_matches_with_citations() { ... }
}
```

---

## 6. Visibility Audit (before Phase 0)

Before touching any test code, verify these items have the right visibility for cross-module test access:

| Item | Current visibility | Required | Location |
|------|--------------------|----------|----------|
| `lint_report()` | `pub(crate)` + `#[cfg(test)]` | `pub(crate)` | `state/mod.rs` |
| `generate_query_answer_with_provider()` | `pub(crate)` + `#[cfg(test)]` | `pub(crate)` | `state/mod.rs` |
| All `pub(super)` service fns | `pub(super)` | OK — tests in same module | service files |

If `lint_report` or `generate_query_answer_with_provider` are only `pub(super)`, elevate to `pub(crate)` in Phase 0.

---

## 7. Execution Checklist

```
[ ] Phase 0: Create test_helpers.rs, declare in mod.rs, update state.rs imports
    → cargo test (268 pass)
[ ] Phase 1: Move research_service tests
    → cargo test
[ ] Phase 2: Move search_service tests
    → cargo test
[ ] Phase 3: Move ingest_service tests
    → cargo test
[ ] Phase 4: Move lint_service tests
    → cargo test
[ ] Phase 5: Move config_service tests
    → cargo test
[ ] Phase 6a: Move wiki_service pure tests
    → cargo test
[ ] Phase 6b: Move wiki_service AppState tests
    → cargo test
[ ] Phase 7: Move ask_service tests
    → cargo test
[ ] Phase 8: Move graph_service tests
    → cargo test
[ ] Phase 9: Move agent_service tests
    → cargo test
[ ] Phase 10: Trim state.rs test module to 2 remaining tests
    → cargo test (268 pass)
[ ] npm run typecheck (0 errors expected — backend-only change)
[ ] git commit
```

---

## 8. Risk Register

| Risk | Mitigation |
|------|-----------|
| `pub(super)` function not visible from service's own test module | Already in same module — no issue |
| `#[cfg(test)]`-gated items not visible across modules | Use `pub(crate)` on test-only items in mod.rs |
| tokio runtime: service tests call async fns | Use same `tokio::runtime::Runtime::new().block_on(...)` pattern as current code; no `#[tokio::test]` attribute needed |
| Windows path separator in path assertions | `assert_paths_semantically_equal` handles this — copy it to test_helpers |
| Merge conflicts if other work happens concurrently | Plan is self-contained to test modules; no production code changes |

---

## 9. Expected Final State

```
src-tauri/src/state/
├── mod.rs                  — AppState + #[cfg(test)] pub(crate) mod test_helpers
├── test_helpers.rs         — NEW: MockQueryProvider, TempDirGuard, make_temp_dir, make_test_state*
├── agent_service.rs        — +12 tests
├── ask_service.rs          — +10 tests
├── config_service.rs       — +7 tests
├── graph_service.rs        — +3 tests
├── ingest_service.rs       — +25 tests
├── lint_service.rs         — +17 tests
├── research_service.rs     — +25 tests (incl. make_research_db helper)
├── search_service.rs       — +9 tests
├── shell_service.rs        — 0 tests (no tests for this service currently)
├── chat_service.rs         — 0 tests (no tests for this service currently)
└── wiki_service.rs         — +38 tests

src-tauri/src/state.rs (if flat) OR mod.rs:
└── mod tests: 2 remaining cross-service integration tests (~50 lines)
```

**state.rs test module line count**: ~3565 → ~50 lines  
**Total new test infrastructure**: ~120 lines (test_helpers.rs)  
**Net line reduction in state.rs**: ~3400 lines
