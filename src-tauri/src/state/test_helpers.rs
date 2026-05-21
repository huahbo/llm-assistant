// 共享测试工具 — 仅在 #[cfg(test)] 下编译，由 state.rs 以 `#[cfg(test)] mod test_helpers` 声明。
// 所有条目对 crate 可见（pub(crate)），以便各 service 子模块的测试直接使用。

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;

use crate::llm::{LlmError, LlmProvider};
use crate::models::{AppMode, ShellPolicyConfig};
use super::{AppState, AppStateData, QUERY_TOP_K_DEFAULT};

// ── Mock LLM Provider ────────────────────────────────────────────────────────

#[derive(Clone)]
pub(crate) struct MockQueryProvider {
    pub(crate) response: String,
    pub(crate) prompt_log: Arc<Mutex<Vec<String>>>,
}

impl MockQueryProvider {
    pub(crate) fn new(response: impl Into<String>, prompt_log: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            response: response.into(),
            prompt_log,
        }
    }
}

#[async_trait]
impl LlmProvider for MockQueryProvider {
    async fn summarize(&self, _content: &str, _max_tokens: usize) -> Result<String, LlmError> {
        Ok(self.response.clone())
    }

    async fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        self.prompt_log
            .lock()
            .expect("记录 prompt 失败")
            .push(prompt.to_string());
        Ok(self.response.clone())
    }

    async fn embed(&self, _text: &str) -> Result<Vec<f32>, LlmError> {
        Ok(vec![])
    }

    async fn health_check(&self) -> Result<bool, LlmError> {
        Ok(true)
    }
}

// ── 临时目录 RAII guard ───────────────────────────────────────────────────────

pub(crate) struct TempDirGuard(pub PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// ── 工厂函数 ──────────────────────────────────────────────────────────────────

pub(crate) fn make_temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
    fs::create_dir_all(&dir).expect("创建临时目录失败");
    dir
}

pub(crate) fn make_test_state_bare(vault_dir: &Path) -> AppState {
    AppState {
        inner: Mutex::new(AppStateData {
            mode: AppMode::Hybrid,
            vault_path: None,
            query_top_k: QUERY_TOP_K_DEFAULT,
            logs: Vec::new(),
            next_log_id: 1,
            config_snapshot: None,
            cloud_api_key: None,
            cloud_base_url: None,
            cloud_model: None,
            cloud_provider_name: None,
            active_provider: None,
            default_ocr_provider: None,
            ollama_model: None,
            ollama_base_url: None,
            embed_ollama_model: None,
            embed_ollama_base_url: None,
            embed_backend: None,
            embed_onnx_model: None,
            shell_policy: ShellPolicyConfig::default(),
            pending_agent_writes: std::collections::HashMap::new(),
        }),
        config_path: vault_dir.join(".runtime").join("app-config.json"),
        llm_provider: OnceLock::new(),
        embed_provider: RwLock::new(Arc::new(super::NoopEmbedder)),
        last_embed_error: Mutex::new(None),
        app_handle: OnceLock::new(),
        ask_sessions: Mutex::new(std::collections::HashMap::new()),
        ask_cancel_flags: Mutex::new(std::collections::HashMap::new()),
        search_config: Mutex::new(crate::models::SearchConfig::default()),
        pending_query_approvals: Mutex::new(std::collections::HashMap::new()),
        pending_outline_approvals: Mutex::new(std::collections::HashMap::new()),
        pending_outline_data: Mutex::new(std::collections::HashMap::new()),
        pending_query_data: Mutex::new(std::collections::HashMap::new()),
        pending_research_reports: Mutex::new(std::collections::HashMap::new()),
        ingest_previews: Mutex::new(std::collections::HashMap::new()),
        shell_sessions: Mutex::new(std::collections::HashMap::new()),
        chat_cancellations: Mutex::new(std::collections::HashMap::new()),
        chat_write_approvals: Mutex::new(std::collections::HashMap::new()),
        chat_shell_pending: Mutex::new(std::collections::HashMap::new()),
        mcp_clients: Mutex::new(std::collections::HashMap::new()),
    }
}

pub(crate) fn make_test_state(vault_dir: &Path) -> AppState {
    let state = make_test_state_bare(vault_dir);
    let _ = state.llm_provider.set(Arc::new(MockQueryProvider::new(
        "Mock Answer",
        Arc::new(Mutex::new(Vec::new())),
    )));
    state
}

// ── 断言辅助 ──────────────────────────────────────────────────────────────────

pub(crate) fn assert_paths_semantically_equal(expected: &Path, actual: &str) {
    let expected_canonical = fs::canonicalize(expected).expect("规范化预期路径失败");
    let actual_canonical = fs::canonicalize(Path::new(actual)).expect("规范化实际路径失败");

    assert_eq!(
        actual_canonical,
        expected_canonical,
        "路径语义不一致：expected={:?}, actual={:?}",
        expected,
        actual
    );
}
