use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use crate::{
    db,
    llm::{LlmError, LlmProvider, OllamaConfig, OllamaProvider},
    models::{
        AppConfig, AppMode, AppOverview, DefaultPaths, IngestResult, LintIssue, LintReport,
        LintSeverityStats, LlmStatus, LogEntry, LogLevel, ModeChangeResult, QueryAnswerResult,
        QueryAskOptions, QueryCitation, QuerySettings, SaveQueryAnswerInput, SaveQueryAnswerResult,
        VaultInitResult, WikiPageCitationItem, WikiPageDetail, WikiPageItem,
    },
    vault,
};

const STALE_PENDING_TASK_THRESHOLD_MS: u128 = 24 * 60 * 60 * 1000;
const QUERY_TOP_K_MIN: usize = 1;
const QUERY_TOP_K_MAX: usize = 8;
const QUERY_TOP_K_DEFAULT: usize = 3;

/// 默认摘要最大 token 数量
const LLM_SUMMARY_MAX_TOKENS: usize = 200;

/// 进程内状态。
pub struct AppState {
    inner: Mutex<AppStateData>,
    config_path: PathBuf,
    /// LLM Provider（延迟初始化）
    llm_provider: OnceLock<Option<Arc<OllamaProvider>>>,
}

/// 状态快照。
#[derive(Debug, Clone)]
struct AppStateData {
    mode: AppMode,
    vault_path: Option<PathBuf>,
    query_top_k: usize,
    logs: Vec<LogEntry>,
    next_log_id: u64,
    config_snapshot: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let config_path = Self::default_config_path();
        let (mode, vault_path, query_top_k, config_snapshot) = Self::load_config(&config_path);
        let query_top_k = normalize_top_k(query_top_k);
        let serialized = Self::serialize_config(mode, vault_path.as_deref(), query_top_k);
        let mut runtime_snapshot = config_snapshot.clone();

        if runtime_snapshot.is_none() {
            if let Err(err) = Self::write_config_file(&config_path, &serialized, None) {
                let _ = err;
            } else {
                runtime_snapshot = Some(serialized.clone());
            }
        }

        let logs = vec![
            LogEntry {
                id: 1,
                level: LogLevel::Info,
                message: "应用骨架已启动".to_string(),
                created_at: "2026-04-08T00:00:00+08:00".to_string(),
            },
            LogEntry {
                id: 2,
                level: LogLevel::Info,
                message: format!("模式配置已加载为 {:?}", mode),
                created_at: "2026-04-08T00:00:01+08:00".to_string(),
            },
        ];

        Self {
            inner: Mutex::new(AppStateData {
                mode,
                vault_path,
                query_top_k,
                next_log_id: 3,
                logs,
                config_snapshot: runtime_snapshot,
            }),
            config_path,
            llm_provider: OnceLock::new(),
        }
    }

    /// 获取 Ollama Provider（延迟初始化）。
    fn get_ollama_provider(&self) -> Option<Arc<OllamaProvider>> {
        self.llm_provider
            .get_or_init(|| {
                // 这里固定使用本地 Ollama
                let config = OllamaConfig::default();
                let provider = OllamaProvider::new(config);
                Some(Arc::new(provider))
            })
            .clone()
    }

    /// 获取 LLM Provider（延迟初始化）。
    fn get_llm_provider(&self) -> Option<Arc<dyn LlmProvider>> {
        self.get_ollama_provider().map(|provider| {
            let provider: Arc<dyn LlmProvider> = provider;
            provider
        })
    }

    /// 使用 LLM 生成摘要，失败时回退到截断
    ///
    /// # 参数
    /// - `content`: 需要摘要的原始内容
    ///
    /// # 返回
    /// 生成的摘要文本。如果 LLM 调用失败，则回退到简单截断。
    pub async fn generate_summary(&self, content: &str) -> String {
        // 尝试获取 LLM Provider
        let provider = match self.get_llm_provider() {
            Some(p) => p,
            None => {
                self.push_log(
                    LogLevel::Warn,
                    "LLM Provider 不可用，回退到截断摘要".to_string(),
                );
                return vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS);
            }
        };

        // 尝试使用 LLM 生成摘要
        match provider.summarize(content, LLM_SUMMARY_MAX_TOKENS).await {
            Ok(summary) => {
                let summary = summary.trim().to_string();
                if summary.is_empty() {
                    self.push_log(
                        LogLevel::Warn,
                        "LLM 返回空摘要，回退到截断摘要".to_string(),
                    );
                    vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS)
                } else {
                    self.push_log(
                        LogLevel::Info,
                        format!("LLM 摘要生成成功，长度={}", summary.chars().count()),
                    );
                    summary
                }
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("LLM 摘要生成失败: {}，回退到截断摘要", err),
                );
                vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS)
            }
        }
    }

    /// 构造 LLM 状态查询输入，避免在异步命令中持有 `State` 借用。
    fn llm_status_input(&self) -> (AppMode, Option<Arc<OllamaProvider>>) {
        let mode = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.mode
        };
        let provider = self.get_ollama_provider();
        (mode, provider)
    }

    /// 使用输入快照查询本地 Ollama 健康状态。
    async fn llm_status_from_input(mode: AppMode, provider: Option<Arc<OllamaProvider>>) -> LlmStatus {
        match provider {
            Some(provider) => {
                let base_url = provider.base_url().to_string();
                let model = provider.model().to_string();

                match provider.health_check().await {
                    Ok(true) => build_llm_status(
                        &base_url,
                        &model,
                        mode,
                        true,
                        "本地 Ollama 可用".to_string(),
                    ),
                    Ok(false) => build_llm_status(
                        &base_url,
                        &model,
                        mode,
                        false,
                        "本地 Ollama 健康检查未通过，请确认服务已启动且模型已准备好".to_string(),
                    ),
                    Err(err) => build_llm_status(
                        &base_url,
                        &model,
                        mode,
                        false,
                        llm_health_error_message(&err),
                    ),
                }
            }
            None => {
                let config = OllamaConfig::default();
                build_llm_status(
                    &config.base_url,
                    &config.model,
                    mode,
                    false,
                    "本地 Ollama Provider 初始化失败".to_string(),
                )
            }
        }
    }

    /// 返回可在异步命令中安全等待的 LLM 状态查询 Future。
    pub fn llm_status_future(&self) -> impl std::future::Future<Output = LlmStatus> + Send + 'static {
        let (mode, provider) = self.llm_status_input();
        async move { Self::llm_status_from_input(mode, provider).await }
    }

    /// 查询本地 Ollama 的健康状态。
    pub async fn llm_status(&self) -> LlmStatus {
        let (mode, provider) = self.llm_status_input();
        Self::llm_status_from_input(mode, provider).await
    }

    pub fn set_mode(&self, mode: AppMode) -> ModeChangeResult {
        let mut guard = self.inner.lock().expect("状态锁已被污染");
        let previous_mode = guard.mode;
        let expected_snapshot = guard.config_snapshot.clone();
        let vault_path = guard.vault_path.clone();
        let query_top_k = guard.query_top_k;

        match self.persist_config(
            mode,
            vault_path.as_deref(),
            query_top_k,
            expected_snapshot.as_deref(),
        ) {
            Ok(serialized) => {
                guard.mode = mode;
                guard.config_snapshot = Some(serialized);
                guard.push_log(
                    LogLevel::Info,
                    format!("模式切换为 {:?}", mode),
                    current_timestamp_ms(),
                );

                ModeChangeResult {
                    previous_mode,
                    current_mode: mode,
                    strict_local_enabled: matches!(mode, AppMode::StrictLocal),
                }
            }
            Err(err) => {
                guard.push_log(
                    LogLevel::Warn,
                    format!("模式持久化失败: {}", err),
                    current_timestamp_ms(),
                );

                ModeChangeResult {
                    previous_mode,
                    current_mode: previous_mode,
                    strict_local_enabled: matches!(previous_mode, AppMode::StrictLocal),
                }
            }
        }
    }

    pub fn init_vault(&self, vault_path: PathBuf) -> Result<VaultInitResult, String> {
        let mode = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard.mode
        };

        let mut result = match vault::initialize_vault(&vault_path, mode) {
            Ok(result) => result,
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("Vault 初始化失败: {}", err),
                );
                return Err(err);
            }
        };
        let warning = self.set_vault_path(vault_path.clone()).err();

        if let Some(message) = warning {
            self.push_log(
                LogLevel::Warn,
                format!("Vault 初始化完成，但运行配置更新失败: {}", message),
            );
            result.message = format!("Vault 初始化完成，但运行配置更新失败: {}", message);
        } else {
            self.push_log(
                LogLevel::Info,
                format!("Vault 已初始化: {}", vault_path.to_string_lossy()),
            );
        }

        Ok(result)
    }

    pub async fn ingest_markdown(&self, source_path: PathBuf) -> Result<IngestResult, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };

        // 读取源文件内容以生成 LLM 摘要
        let source_content = fs::read_to_string(&source_path)
            .map_err(|err| format!("读取源文件失败: {}", err))?;

        // 在异步上下文中直接调用 LLM 摘要生成
        let llm_summary = self.generate_summary(&source_content).await;

        match vault::ingest_markdown(&vault_path, &source_path, Some(&llm_summary)) {
            Ok(result) => {
                self.push_log(
                    LogLevel::Info,
                    format!(
                        "Markdown 导入成功: {} -> {}",
                        source_path.to_string_lossy(),
                        result.wiki_path
                    ),
                );
                Ok(result)
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!(
                        "Markdown 导入失败: {}",
                        err
                    ),
                );
                Err(err)
            }
        }
    }

    /// 同步调用 LLM 生成摘要（内部方法）
    ///
    /// 使用 tokio runtime 的 block_on 在同步上下文中调用异步 LLM。
    fn generate_summary_sync(&self, content: &str) -> String {
        // 尝试获取当前 tokio runtime handle
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // 在已有 runtime 中，使用 spawn_blocking + block_on
                // 但由于我们在同步上下文中，需要 block_in_place
                let content = content.to_string();
                let provider = self.get_llm_provider();

                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        self.generate_summary_with_provider(&content, provider).await
                    })
                })
            }
            Err(_) => {
                // 没有 runtime，回退到截断摘要
                self.push_log(
                    LogLevel::Warn,
                    "没有可用的 tokio runtime，回退到截断摘要".to_string(),
                );
                vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS)
            }
        }
    }

    /// 使用 LLM Provider 生成摘要（异步内部方法）
    async fn generate_summary_with_provider(
        &self,
        content: &str,
        provider: Option<Arc<dyn LlmProvider>>,
    ) -> String {
        let provider = match provider {
            Some(p) => p,
            None => {
                self.push_log(
                    LogLevel::Warn,
                    "LLM Provider 不可用，回退到截断摘要".to_string(),
                );
                return vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS);
            }
        };

        match provider.summarize(content, LLM_SUMMARY_MAX_TOKENS).await {
            Ok(summary) => {
                let summary = summary.trim().to_string();
                if summary.is_empty() {
                    self.push_log(
                        LogLevel::Warn,
                        "LLM 返回空摘要，回退到截断摘要".to_string(),
                    );
                    vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS)
                } else {
                    self.push_log(
                        LogLevel::Info,
                        format!("LLM 摘要生成成功，长度={}", summary.chars().count()),
                    );
                    summary
                }
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!("LLM 摘要生成失败: {}，回退到截断摘要", err),
                );
                vault::fallback_summarize(content, LLM_SUMMARY_MAX_TOKENS)
            }
        }
    }

    /// 同步调用本地 LLM 生成 Query 回答。
    ///
    /// 该路径只允许使用本地 Ollama Provider；如果运行时不可用或调用失败，
    /// 会回退到规则回答，保证 StrictLocal 语义不被破坏。
    fn generate_query_answer_sync(
        &self,
        question: &str,
        matches: &[WikiMatch],
    ) -> (String, String) {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let question = question.to_string();
                let matches = matches.to_vec();
                let provider = self.get_llm_provider();

                tokio::task::block_in_place(|| {
                    handle.block_on(async move {
                        self.generate_query_answer_with_provider(&question, &matches, provider)
                            .await
                    })
                })
            }
            Err(_) => {
                self.push_log(
                    LogLevel::Warn,
                    "没有可用的 tokio runtime，Query 已回退到规则回答".to_string(),
                );
                (build_query_answer(question, matches), "rule".to_string())
            }
        }
    }

    /// 使用本地 Provider 生成 Query 回答。
    async fn generate_query_answer_with_provider(
        &self,
        question: &str,
        matches: &[WikiMatch],
        provider: Option<Arc<dyn LlmProvider>>,
    ) -> (String, String) {
        let fallback = || (build_query_answer(question, matches), "rule".to_string());

        let provider = match provider {
            Some(provider) => provider,
            None => {
                self.push_log(
                    LogLevel::Warn,
                    "本地 LLM Provider 不可用，Query 已回退到规则回答".to_string(),
                );
                return fallback();
            }
        };

        let prompt = build_query_prompt(question, matches);

        match provider.complete(&prompt).await {
            Ok(answer) => {
                let answer = answer.trim().to_string();
                if answer.is_empty() {
                    self.push_log(
                        LogLevel::Warn,
                        "本地 LLM 返回空回答，Query 已回退到规则回答".to_string(),
                    );
                    fallback()
                } else {
                    self.push_log(
                        LogLevel::Info,
                        format!(
                            "本地 LLM Query 合成成功，回答长度={}",
                            answer.chars().count()
                        ),
                    );
                    (answer, "llm".to_string())
                }
            }
            Err(err) => {
                self.push_log(
                    LogLevel::Warn,
                    format!(
                        "本地 LLM Query 合成失败: {}，已回退到规则回答",
                        err
                    ),
                );
                fallback()
            }
        }
    }

    pub fn overview(&self) -> AppOverview {
        let guard = self.inner.lock().expect("状态锁已被污染");
        let vault_path = guard
            .vault_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "vault".to_string());
        let pending_tasks = guard
            .vault_path
            .as_ref()
            .and_then(|path| db::count_pending_tasks(&path.join(".app").join("meta.db")).ok())
            .unwrap_or(0);

        AppOverview {
            app_name: "LLM Wiki".to_string(),
            mode: guard.mode,
            vault_path,
            recent_log_count: guard.logs.len(),
            pending_tasks,
            supported_modes: vec![AppMode::Hybrid, AppMode::StrictLocal],
        }
    }

    pub fn default_paths(&self) -> DefaultPaths {
        let root = Self::project_root();
        DefaultPaths {
            vault_path: root.join("vault").to_string_lossy().to_string(),
            ingest_source_path: Self::default_ingest_source_path(root).to_string_lossy().to_string(),
        }
    }

    fn default_ingest_source_path(_root: PathBuf) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(r"E:\llm-wiki\test-llm.md")
        }

        #[cfg(not(windows))]
        {
            _root.join("test-llm.md")
        }
    }

    pub fn query_settings(&self) -> QuerySettings {
        let guard = self.inner.lock().expect("状态锁已被污染");
        QuerySettings {
            top_k: guard.query_top_k,
            min_top_k: QUERY_TOP_K_MIN,
            max_top_k: QUERY_TOP_K_MAX,
        }
    }

    pub fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        let guard = self.inner.lock().expect("状态锁已被污染");
        guard.logs.iter().rev().take(limit).cloned().collect()
    }

    pub fn recent_wiki_pages(&self, limit: usize) -> Result<Vec<WikiPageItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        let pages = db::list_recent_wiki_pages(&db_path, limit)?;

        Ok(pages
            .into_iter()
            .map(|page| {
                let display_path = friendly_display_path(Path::new(&page.path));
                WikiPageItem {
                    title: page.title,
                    path: page.path,
                    display_path: Some(display_path),
                    summary: page.summary,
                    updated_at: page.updated_at,
                }
            })
            .collect())
    }

    pub fn search_wiki_pages(&self, keyword: String, limit: usize) -> Result<Vec<WikiPageItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        let pages = db::search_wiki_pages(&db_path, &keyword, limit)?;

        Ok(pages
            .into_iter()
            .map(|page| {
                let display_path = friendly_display_path(Path::new(&page.path));
                WikiPageItem {
                    title: page.title,
                    path: page.path,
                    display_path: Some(display_path),
                    summary: page.summary,
                    updated_at: page.updated_at,
                }
            })
            .collect())
    }

    pub fn wiki_page_detail(&self, page_path: String) -> Result<WikiPageDetail, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let target_path = resolve_existing_wiki_page_path(&vault_path, &page_path)?;
        if target_path.extension().and_then(|v| v.to_str()) != Some("md") {
            return Err("仅支持读取 Markdown 页面".to_string());
        }

        let content = fs::read_to_string(&target_path)
            .map_err(|err| format!("读取页面失败: {}", err))?;
        let title = extract_title_from_markdown(&content, &target_path);
        let updated_at = file_modified_timestamp_ms(&target_path);

        Ok(WikiPageDetail {
            title,
            path: target_path.to_string_lossy().to_string(),
            display_path: friendly_display_path(&target_path),
            content,
            updated_at,
        })
    }

    pub fn wiki_page_citations(
        &self,
        page_path: String,
    ) -> Result<Vec<WikiPageCitationItem>, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };
        let target_path = resolve_existing_wiki_page_path(&vault_path, &page_path)?;
        let target_path_string = target_path.to_string_lossy().to_string();
        let db_path = vault_path.join(".app").join("meta.db");
        db::ensure_meta_db(&db_path)?;
        let citations = db::list_citations_for_page(&db_path, &target_path_string)?;

        Ok(citations
            .into_iter()
            .map(|citation| {
                let cited_page_display_path =
                    friendly_display_path(Path::new(&citation.cited_page_path));
                WikiPageCitationItem {
                    cited_page_path: citation.cited_page_path.clone(),
                    cited_page_display_path: Some(cited_page_display_path),
                    score: citation.score,
                    excerpt: citation.excerpt,
                    target_exists: is_existing_wiki_page_target(
                        &vault_path,
                        &citation.cited_page_path,
                    ),
                }
            })
            .collect())
    }

    pub fn lint_report(&self) -> LintReport {
        let (mode, vault_path) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone())
        };
        let mut issues = Vec::new();

        let vault_path = match vault_path.as_ref() {
            Some(path) => path,
            None => {
                issues.push(LintIssue {
                    code: "VAULT_NOT_INITIALIZED".to_string(),
                    severity: "error".to_string(),
                    message: "尚未初始化 Vault".to_string(),
                    path: None,
                    suggestion: "先执行 init_vault 创建本地 Vault".to_string(),
                });
                return build_lint_report(mode, "Vault 未初始化".to_string(), issues);
            }
        };

        let index_path = vault_path.join("index.md");
        let (index_content, index_missing) = match fs::read_to_string(&index_path) {
            Ok(content) => (Some(content), false),
            Err(err) if err.kind() == io::ErrorKind::NotFound => (None, true),
            Err(err) => {
                issues.push(LintIssue {
                    code: "INDEX_READ_FAILED".to_string(),
                    severity: "error".to_string(),
                    message: format!("读取 index.md 失败: {}", err),
                    path: Some(index_path.to_string_lossy().to_string()),
                    suggestion: "检查 index.md 是否可读".to_string(),
                });
                (None, false)
            }
        };

        if index_missing {
            issues.push(LintIssue {
                code: "INDEX_MISSING".to_string(),
                severity: "error".to_string(),
                message: "index.md 缺失".to_string(),
                path: Some(index_path.to_string_lossy().to_string()),
                suggestion: "重新执行 init_vault 或补回 index.md".to_string(),
            });
        }

        let log_path = vault_path.join("log.md");
        if !log_path.exists() {
            issues.push(LintIssue {
                code: "LOG_MISSING".to_string(),
                severity: "error".to_string(),
                message: "log.md 缺失".to_string(),
                path: Some(log_path.to_string_lossy().to_string()),
                suggestion: "重新执行 init_vault 或补回 log.md".to_string(),
            });
        }

        let db_path = vault_path.join(".app").join("meta.db");
        if db_path.exists() {
            if let Err(err) = db::ensure_meta_db(&db_path) {
                issues.push(LintIssue {
                    code: "DB_SCHEMA_UPGRADE_FAILED".to_string(),
                    severity: "warning".to_string(),
                    message: format!("数据库结构校验失败: {}", err),
                    path: Some(db_path.to_string_lossy().to_string()),
                    suggestion: "检查数据库文件权限并重试".to_string(),
                });
            }
        }
        let db_paths = if db_path.exists() {
            match db::list_wiki_page_paths(&db_path) {
                Ok(paths) => Some(paths.into_iter().collect::<BTreeSet<_>>()),
                Err(err) => {
                    issues.push(LintIssue {
                        code: "DB_QUERY_FAILED".to_string(),
                        severity: "warning".to_string(),
                        message: format!("读取 wiki_pages 失败: {}", err),
                        path: Some(db_path.to_string_lossy().to_string()),
                        suggestion: "检查 SQLite 数据库结构是否完整".to_string(),
                    });
                    None
                }
            }
        } else {
            issues.push(LintIssue {
                code: "DB_MISSING".to_string(),
                severity: "error".to_string(),
                message: "meta.db 缺失".to_string(),
                path: Some(db_path.to_string_lossy().to_string()),
                suggestion: "重新执行 init_vault 生成 SQLite 数据库".to_string(),
            });
            None
        };

        if db_path.exists() {
            match db::list_citations(&db_path) {
                Ok(citations) => {
                    for citation in citations {
                        if !Path::new(&citation.page_path).exists() {
                            issues.push(LintIssue {
                                code: "BROKEN_CITING_PAGE".to_string(),
                                severity: "warning".to_string(),
                                message: format!(
                                    "引用记录所属页面不存在: {}",
                                    citation.page_path
                                ),
                                path: Some(citation.page_path.clone()),
                                suggestion: "移除失效引用记录或恢复对应页面".to_string(),
                            });
                        }

                        if !Path::new(&citation.cited_page_path).exists() {
                            issues.push(LintIssue {
                                code: "BROKEN_CITATION".to_string(),
                                severity: "warning".to_string(),
                                message: format!(
                                    "引用目标页面不存在: {}",
                                    citation.cited_page_path
                                ),
                                path: Some(citation.cited_page_path.clone()),
                                suggestion: "修复引用路径或补回被引用页面".to_string(),
                            });
                        }
                    }
                }
                Err(err) => {
                    issues.push(LintIssue {
                        code: "CITATION_QUERY_FAILED".to_string(),
                        severity: "warning".to_string(),
                        message: format!("读取 citations 失败: {}", err),
                        path: Some(db_path.to_string_lossy().to_string()),
                        suggestion: "检查 SQLite 数据库结构是否完整".to_string(),
                    });
                }
            }
        }

        let wiki_dir = vault_path.join("wiki");
        let wiki_page_paths = collect_wiki_page_paths(&wiki_dir);

        if let Some(index_content) = index_content.as_ref() {
            let index_page_paths = collect_index_page_paths(index_content, vault_path);

            for path in index_page_paths.difference(&wiki_page_paths) {
                issues.push(LintIssue {
                    code: "MISSING_INDEX_ENTRY".to_string(),
                    severity: "error".to_string(),
                    message: format!("index.md 引用了不存在的页面: {}", path),
                    path: Some(path.clone()),
                    suggestion: "补齐对应的 vault/wiki 页面或修正 index.md 链接".to_string(),
                });
            }

            for path in wiki_page_paths.difference(&index_page_paths) {
                issues.push(LintIssue {
                    code: "ORPHAN_WIKI_PAGE".to_string(),
                    severity: "warning".to_string(),
                    message: format!("wiki 页面未被 index.md 引用: {}", path),
                    path: Some(path.clone()),
                    suggestion: "把页面加入 index.md，或确认该页面是否应保留".to_string(),
                });
            }

            if let Some(db_paths) = db_paths.as_ref() {
                for path in wiki_page_paths.difference(db_paths) {
                    issues.push(LintIssue {
                        code: "DB_MISSING_PAGE_RECORD".to_string(),
                        severity: "warning".to_string(),
                        message: format!("wiki_pages 表缺少页面记录: {}", path),
                        path: Some(path.clone()),
                        suggestion: "重新同步 wiki_pages 表记录".to_string(),
                    });
                }
            }
        }

        if db_path.exists() {
            match db::list_pending_tasks(&db_path) {
                Ok(tasks) => {
                    let checked_at_ms = current_timestamp_ms().parse::<u128>().unwrap_or_default();
                    for task in tasks {
                        if is_stale_pending_task(&task, checked_at_ms) {
                            issues.push(LintIssue {
                                code: "STALE_PENDING_TASK".to_string(),
                                severity: "warning".to_string(),
                                message: format!(
                                    "任务 {}（kind={}）处于 {} 状态且已超过陈旧阈值，raw={}",
                                    task.id, task.kind, task.status, task.raw_path
                                ),
                                path: Some(task.wiki_path.clone()),
                                suggestion: "推进任务状态或清理卡住的任务".to_string(),
                            });
                        }
                    }
                }
                Err(err) => {
                    issues.push(LintIssue {
                        code: "TASK_QUERY_FAILED".to_string(),
                        severity: "warning".to_string(),
                        message: format!("读取 tasks 失败: {}", err),
                        path: Some(db_path.to_string_lossy().to_string()),
                        suggestion: "检查 SQLite 数据库结构是否完整".to_string(),
                    });
                }
            }
        }

        if matches!(mode, AppMode::StrictLocal) {
            issues.push(LintIssue {
                code: "STRICT_LOCAL_GATE".to_string(),
                severity: "info".to_string(),
                message: "严格本地模式处于启用状态".to_string(),
                path: None,
                suggestion: "确保所有 Provider 调用都只走本地路径".to_string(),
            });
        }

        build_lint_report(mode, format!("已返回 {} 条 lint 问题", issues.len()), issues)
    }

    pub fn query_ask(&self, question: String) -> Result<QueryAnswerResult, String> {
        self.query_ask_with_options(question, QueryAskOptions::default())
    }

    pub fn query_ask_with_options(
        &self,
        question: String,
        options: QueryAskOptions,
    ) -> Result<QueryAnswerResult, String> {
        let normalized_question = question.trim().to_string();
        if normalized_question.is_empty() {
            return Err("问题不能为空".to_string());
        }

        let (mode, vault_path, default_top_k) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone(), guard.query_top_k)
        };

        let vault_path = vault_path.ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let wiki_dir = vault_path.join("wiki");
        let db_path = vault_path.join(".app").join("meta.db");
        let tokens = tokenize_query(&normalized_question);
        let top_k = normalize_top_k(options.top_k.or(Some(default_top_k)));
        let (matches, search_strategy, fts_error) =
            search_wiki_matches_with_fts(&db_path, &wiki_dir, &tokens, &normalized_question, top_k)?;

        if let Some(err) = fts_error {
            self.push_log(
                LogLevel::Warn,
                format!("FTS 查询失败，已降级为文件扫描: {}", err),
            );
        }

        let citations = matches
            .iter()
            .map(|item| {
                let display_path = friendly_display_path(Path::new(&item.page_path));
                QueryCitation {
                    page_path: item.page_path.clone(),
                    display_path: Some(display_path),
                    score: item.score,
                    excerpt: item.excerpt.clone(),
                }
            })
            .collect::<Vec<_>>();
        let (answer, answer_strategy) = self.generate_query_answer_sync(&normalized_question, &matches);
        let matched_pages = matches
            .iter()
            .map(|item| item.page_path.clone())
            .collect::<Vec<_>>();

        self.push_log(
            LogLevel::Info,
            format!(
                "Query 检索完成: '{}'，命中 {} 页，检索策略={}，回答策略={}，top_k={}",
                normalized_question,
                matched_pages.len(),
                search_strategy,
                answer_strategy,
                top_k
            ),
        );

        Ok(QueryAnswerResult {
            question: normalized_question,
            answer,
            citations,
            matched_pages,
            mode,
            checked_at: current_timestamp_ms(),
            search_strategy: search_strategy.to_string(),
            answer_strategy,
        })
    }

    pub fn set_query_top_k(&self, top_k: usize) -> Result<QuerySettings, String> {
        let normalized_top_k = normalize_top_k(Some(top_k));
        let (mode, vault_path, expected_snapshot) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone(), guard.config_snapshot.clone())
        };

        match self.persist_config(
            mode,
            vault_path.as_deref(),
            normalized_top_k,
            expected_snapshot.as_deref(),
        ) {
            Ok(serialized) => {
                let mut guard = self.inner.lock().expect("状态锁已被污染");
                guard.query_top_k = normalized_top_k;
                guard.config_snapshot = Some(serialized);
                guard.push_log(
                    LogLevel::Info,
                    format!("Query TopK 已更新为 {}", normalized_top_k),
                    current_timestamp_ms(),
                );
                Ok(QuerySettings {
                    top_k: normalized_top_k,
                    min_top_k: QUERY_TOP_K_MIN,
                    max_top_k: QUERY_TOP_K_MAX,
                })
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("Query TopK 持久化失败: {}", err));
                Err(err)
            }
        }
    }

    pub fn save_query_answer(
        &self,
        input: SaveQueryAnswerInput,
    ) -> Result<SaveQueryAnswerResult, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };

        match vault::save_query_answer(&vault_path, &input) {
            Ok(result) => {
                self.push_log(
                    LogLevel::Info,
                    format!("Query 结果已保存: {}", result.wiki_path),
                );
                Ok(result)
            }
            Err(err) => {
                self.push_log(LogLevel::Warn, format!("保存 Query 结果失败: {}", err));
                Err(err)
            }
        }
    }

    fn set_vault_path(&self, vault_path: PathBuf) -> Result<(), String> {
        let (mode, query_top_k, expected_snapshot) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.query_top_k, guard.config_snapshot.clone())
        };

        {
            let mut guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path = Some(vault_path.clone());
        }

        match self.persist_config(
            mode,
            Some(vault_path.as_path()),
            query_top_k,
            expected_snapshot.as_deref(),
        ) {
            Ok(serialized) => {
                let mut guard = self.inner.lock().expect("状态锁已被污染");
                guard.config_snapshot = Some(serialized);
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn default_config_path() -> PathBuf {
        Self::default_config_path_from_root(&Self::project_root())
    }

    fn project_root() -> PathBuf {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(manifest_dir)
    }

    fn default_config_path_from_root(root: &Path) -> PathBuf {
        root.join(".runtime").join("app-config.json")
    }

    fn load_config(config_path: &Path) -> (AppMode, Option<PathBuf>, Option<usize>, Option<String>) {
        match fs::read_to_string(config_path) {
            Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
                Ok(config) => (
                    config.mode,
                    config.vault_path.map(PathBuf::from),
                    config.query_top_k,
                    Some(raw),
                ),
                Err(_) => (AppMode::default(), None, None, Some(raw)),
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                (AppMode::default(), None, None, None)
            }
            Err(_) => (AppMode::default(), None, None, None),
        }
    }

    fn serialize_config(mode: AppMode, vault_path: Option<&Path>, query_top_k: usize) -> String {
        serde_json::to_string_pretty(&AppConfig {
            mode,
            vault_path: vault_path.map(|path| path.to_string_lossy().to_string()),
            query_top_k: Some(query_top_k),
        })
        .expect("配置序列化失败")
    }

    fn persist_config(
        &self,
        mode: AppMode,
        vault_path: Option<&Path>,
        query_top_k: usize,
        expected_snapshot: Option<&str>,
    ) -> Result<String, String> {
        let serialized = Self::serialize_config(mode, vault_path, query_top_k);

        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("创建配置目录失败: {}", err))?;
        }

        let current_snapshot = match fs::read_to_string(&self.config_path) {
            Ok(raw) => Some(raw),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(format!("读取配置文件失败: {}", err)),
        };

        match expected_snapshot {
            Some(snapshot) => {
                if current_snapshot.as_deref() != Some(snapshot) {
                    return Err("配置文件已被外部修改".to_string());
                }
            }
            None => {
                if current_snapshot.is_some() {
                    return Err("配置文件已被外部创建或修改".to_string());
                }
            }
        }

        Self::write_config_file(&self.config_path, &serialized, current_snapshot.as_deref())?;
        Ok(serialized)
    }

    fn write_config_file(
        config_path: &Path,
        serialized: &str,
        expected_snapshot: Option<&str>,
    ) -> Result<(), String> {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("创建配置目录失败: {}", err))?;
        }

        let current_snapshot = match fs::read_to_string(config_path) {
            Ok(raw) => Some(raw),
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(err) => return Err(format!("读取配置文件失败: {}", err)),
        };

        if let Some(snapshot) = expected_snapshot {
            if current_snapshot.as_deref() != Some(snapshot) {
                return Err("配置文件已被外部修改".to_string());
            }
        } else if current_snapshot.is_some() {
            return Err("配置文件已被外部创建或修改".to_string());
        }

        fs::write(config_path, serialized).map_err(|err| format!("写入配置文件失败: {}", err))
    }

    fn push_log(&self, level: LogLevel, message: String) {
        let mut guard = self.inner.lock().expect("状态锁已被污染");
        guard.push_log(level, message, current_timestamp_ms());
    }
}

fn collect_wiki_page_paths(wiki_dir: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();

    // 只扫描 vault/wiki 顶层 Markdown 页面。
    let Ok(entries) = fs::read_dir(wiki_dir) else {
        return paths;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        paths.insert(path.to_string_lossy().to_string());
    }

    paths
}

fn collect_index_page_paths(index_content: &str, vault_path: &Path) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();

    // 解析 index.md 中的 wiki 与 Markdown 引用。
    for target in extract_wiki_link_targets(index_content) {
        if let Some(path) = resolve_wiki_link_target(vault_path, &target) {
            paths.insert(path);
        }
    }

    for target in extract_markdown_link_targets(index_content) {
        if let Some(path) = resolve_wiki_link_target(vault_path, &target) {
            paths.insert(path);
        }
    }

    paths
}

fn extract_wiki_link_targets(content: &str) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let mut offset = 0;

    while let Some(start) = content[offset..].find("[[") {
        let start = offset + start + 2;
        let Some(end_rel) = content[start..].find("]]") else {
            break;
        };
        let inner = &content[start..start + end_rel];
        if let Some(target) = inner.split('|').next() {
            let target = target.trim();
            if !target.is_empty() {
                targets.insert(target.to_string());
            }
        }
        offset = start + end_rel + 2;
    }

    targets
}

fn extract_markdown_link_targets(content: &str) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    let mut offset = 0;

    while let Some(start) = content[offset..].find("](") {
        let start = offset + start + 2;
        let Some(end_rel) = content[start..].find(')') else {
            break;
        };
        let target = content[start..start + end_rel].trim();
        if !target.is_empty() {
            targets.insert(target.to_string());
        }
        offset = start + end_rel + 1;
    }

    targets
}

fn resolve_wiki_link_target(vault_path: &Path, raw_target: &str) -> Option<String> {
    let target = raw_target
        .split('|')
        .next()
        .unwrap_or(raw_target)
        .split('#')
        .next()
        .unwrap_or(raw_target)
        .split('^')
        .next()
        .unwrap_or(raw_target)
        .trim();

    let relative = target
        .strip_prefix("wiki/")
        .or_else(|| target.strip_prefix("wiki\\"))
        .or_else(|| target.strip_prefix("./wiki/"))
        .or_else(|| target.strip_prefix("./wiki\\"))?;

    let relative = if relative.ends_with(".md") {
        relative.to_string()
    } else {
        format!("{}.md", relative)
    };

    Some(vault_path.join("wiki").join(relative).to_string_lossy().to_string())
}

fn is_stale_pending_task(task: &db::PendingTaskRecord, checked_at_ms: u128) -> bool {
    let updated_at_ms = task
        .updated_at
        .parse::<u128>()
        .ok()
        .or_else(|| task.created_at.parse::<u128>().ok());

    // 以更新时间判断是否已经卡住。
    match updated_at_ms {
        Some(value) => checked_at_ms.saturating_sub(value) > STALE_PENDING_TASK_THRESHOLD_MS,
        None => true,
    }
}

impl AppStateData {
    fn push_log(&mut self, level: LogLevel, message: String, created_at: String) {
        let entry = LogEntry {
            id: self.next_log_id,
            level,
            message,
            created_at,
        };
        self.next_log_id += 1;
        self.logs.push(entry);
    }
}

fn current_timestamp_ms() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

fn build_lint_report(mode: AppMode, summary: String, issues: Vec<LintIssue>) -> LintReport {
    let severity_stats = count_lint_severity_stats(&issues);

    LintReport {
        mode,
        checked_at: current_timestamp_ms(),
        summary,
        issues,
        severity_stats,
    }
}

fn count_lint_severity_stats(issues: &[LintIssue]) -> LintSeverityStats {
    let mut stats = LintSeverityStats::default();

    for issue in issues {
        match issue.severity.to_ascii_lowercase().as_str() {
            "error" => stats.error += 1,
            "warning" => stats.warning += 1,
            "info" => stats.info += 1,
            _ => {}
        }
    }

    stats
}

fn file_modified_timestamp_ms(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|dur| dur.as_millis().to_string())
        .unwrap_or_else(current_timestamp_ms)
}

fn resolve_wiki_page_candidate(vault_path: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let wiki_root = vault_path.join("wiki");
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("页面路径不能为空".to_string());
    }

    let input_path = PathBuf::from(trimmed);
    Ok(if input_path.is_absolute() {
        input_path
    } else if trimmed.starts_with("wiki/")
        || trimmed.starts_with("wiki\\")
        || trimmed.starts_with("./wiki/")
        || trimmed.starts_with("./wiki\\")
    {
        vault_path.join(trimmed)
    } else {
        wiki_root.join(input_path)
    })
}

fn resolve_existing_wiki_page_path(vault_path: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let wiki_root = vault_path.join("wiki");
    let candidate = resolve_wiki_page_candidate(vault_path, raw_path)?;
    if !candidate.exists() {
        return Err("页面不存在".to_string());
    }

    // 安全约束：只允许读取当前 vault/wiki 目录内文件，禁止越界访问。
    let canonical_root =
        fs::canonicalize(&wiki_root).map_err(|err| format!("解析 wiki 根目录失败: {}", err))?;
    let canonical_target =
        fs::canonicalize(&candidate).map_err(|err| format!("解析页面路径失败: {}", err))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err("只允许读取 vault/wiki 目录下的页面".to_string());
    }

    Ok(canonical_target)
}

fn is_existing_wiki_page_target(vault_path: &Path, raw_path: &str) -> bool {
    let Ok(candidate) = resolve_wiki_page_candidate(vault_path, raw_path) else {
        return false;
    };
    if !candidate.exists() {
        return false;
    }

    let wiki_root = vault_path.join("wiki");
    let Ok(canonical_root) = fs::canonicalize(&wiki_root) else {
        return false;
    };
    let Ok(canonical_target) = fs::canonicalize(&candidate) else {
        return false;
    };

    canonical_target.starts_with(&canonical_root)
}

#[derive(Debug, Clone)]
struct WikiMatch {
    page_path: String,
    score: usize,
    excerpt: String,
}

fn tokenize_query(question: &str) -> Vec<String> {
    // 轻量混合分词：保留英文 token，同时为连续中文片段生成 2-gram，提升中文命中率。
    let mut tokens = question
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();

    for segment in extract_cjk_segments(question) {
        let chars = segment.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            continue;
        }
        tokens.push(segment.clone());
        if chars.len() >= 2 {
            for window in chars.windows(2) {
                let gram = window.iter().collect::<String>();
                tokens.push(gram);
            }
        }
    }

    tokens.sort();
    tokens.dedup();
    tokens
        .into_iter()
        .filter(|token| !is_stopword(token))
        .collect()
}

fn extract_cjk_segments(input: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in input.chars() {
        if is_cjk(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            segments.push(current.clone());
            current.clear();
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF // CJK Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
    )
}

fn is_stopword(token: &str) -> bool {
    const ZH_STOPWORDS: &[&str] = &["的", "了", "是", "吗", "呢", "和", "与", "及", "在", "对", "把", "将"];
    const EN_STOPWORDS: &[&str] = &["the", "is", "are", "a", "an", "of", "to", "for"];

    ZH_STOPWORDS.contains(&token) || EN_STOPWORDS.contains(&token)
}

fn normalize_top_k(top_k: Option<usize>) -> usize {
    top_k
        .unwrap_or(QUERY_TOP_K_DEFAULT)
        .clamp(QUERY_TOP_K_MIN, QUERY_TOP_K_MAX)
}

fn search_wiki_matches(
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<Vec<WikiMatch>, String> {
    let entries = match fs::read_dir(wiki_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("读取 wiki 目录失败: {}", err)),
    };

    let page_paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("md"))
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();

    search_wiki_matches_from_paths(&page_paths, tokens, question, limit)
}

fn search_wiki_matches_with_fts(
    db_path: &Path,
    wiki_dir: &Path,
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<(Vec<WikiMatch>, &'static str, Option<String>), String> {
    if tokens.is_empty() {
        return Ok((Vec::new(), "empty", None));
    }

    match db::search_fts_page_paths(db_path, tokens, 64) {
        Ok(paths) if !paths.is_empty() => {
            let matches = search_wiki_matches_from_paths(&paths, tokens, question, limit)?;
            if !matches.is_empty() {
                return Ok((matches, "fts", None));
            }
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            Ok((fallback, "scan", None))
        }
        Ok(_) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            Ok((fallback, "scan", None))
        }
        Err(err) => {
            let fallback = search_wiki_matches(wiki_dir, tokens, question, limit)?;
            Ok((fallback, "scan", Some(err)))
        }
    }
}

fn search_wiki_matches_from_paths(
    page_paths: &[String],
    tokens: &[String],
    question: &str,
    limit: usize,
) -> Result<Vec<WikiMatch>, String> {
    if tokens.is_empty() {
        return Ok(Vec::new());
    }

    let phrase = question.trim().to_lowercase();
    let mut results = Vec::new();
    let mut seen_paths = HashSet::new();

    for page_path in page_paths {
        let path = PathBuf::from(page_path);
        let canonical = path.to_string_lossy().to_string();
        if !seen_paths.insert(canonical) {
            continue;
        }
        if !path.exists() {
            continue;
        }
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(err) => return Err(format!("读取页面失败 {}: {}", path.to_string_lossy(), err)),
        };
        let lowered = content.to_lowercase();
        let title = extract_title_from_markdown(&content, &path);
        let lowered_title = title.to_lowercase();

        // 综合评分：正文命中 + 标题命中加权 + 短语命中加权。
        let token_hits = tokens
            .iter()
            .map(|token| lowered.matches(token).count())
            .sum::<usize>();
        let title_hits = tokens
            .iter()
            .filter(|token| lowered_title.contains(token.as_str()))
            .count();
        let phrase_hit = usize::from(!phrase.is_empty() && lowered.contains(&phrase));
        let score = token_hits + title_hits * 3 + phrase_hit * 5;
        if score == 0 {
            continue;
        }

        let excerpt = pick_excerpt(&content, tokens);
        results.push(WikiMatch {
            page_path: path.to_string_lossy().to_string(),
            score,
            excerpt,
        });
    }

    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.page_path.cmp(&right.page_path))
    });
    if results.len() > limit {
        results.truncate(limit);
    }
    Ok(results)
}

fn extract_title_from_markdown(content: &str, path: &Path) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            let title = title.trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
    }

    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn friendly_display_path(path: &Path) -> String {
    let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let normalized = normalized.to_string_lossy();
    friendly_display_path_str(normalized.as_ref())
}

fn friendly_display_path_str(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{}", stripped);
    }

    if let Some(stripped) = path.strip_prefix(r"\\?\") {
        return stripped.to_string();
    }

    path.to_string()
}

fn pick_excerpt(content: &str, tokens: &[String]) -> String {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let lowered = line.to_lowercase();
        if tokens.iter().any(|token| lowered.contains(token)) {
            return trim_excerpt(line, 120);
        }
    }

    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| trim_excerpt(line, 120))
        .unwrap_or_else(|| "(页面无可用内容)".to_string())
}

fn trim_excerpt(input: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for ch in input.chars().take(max_chars) {
        output.push(ch);
    }
    if input.chars().count() > max_chars {
        output.push('…');
    }
    output
}

fn build_query_prompt(question: &str, matches: &[WikiMatch]) -> String {
    let mut lines = vec![
        "你是一个严格本地运行的 Wiki 助手。只能依据下方本地检索结果回答，不能编造。".to_string(),
        "如果证据不足，请明确说明不确定，并给出基于页面内容的保守建议。".to_string(),
        format!("问题：{}", question),
        "本地检索结果：".to_string(),
    ];

    if matches.is_empty() {
        lines.push("(未命中任何本地页面)".to_string());
    } else {
        for (idx, item) in matches.iter().enumerate() {
            lines.push(format!("{}. 页面：{}", idx + 1, item.page_path));
            lines.push(format!("   相关度：{}", item.score));
            lines.push(format!("   证据：{}", item.excerpt));
        }
    }

    lines.push("回答要求：".to_string());
    lines.push("- 使用中文简洁回答。".to_string());
    lines.push("- 优先引用页面路径和检索证据。".to_string());
    lines.push("- 如果无法确认答案，请直接说明。".to_string());
    lines.join("\n")
}

fn build_query_answer(question: &str, matches: &[WikiMatch]) -> String {
    if matches.is_empty() {
        return format!(
            "未在本地 Wiki 中检索到与“{}”直接相关的页面。建议先导入相关资料后再查询。",
            question
        );
    }

    let mut lines = vec![
        format!("问题：{}", question),
        "基于本地检索，以下页面与问题最相关：".to_string(),
    ];

    for item in matches {
        lines.push(format!(
            "- {}（相关度：{}）",
            item.page_path, item.score
        ));
    }
    lines.push("以上为本地规则检索结果（未调用云模型）。".to_string());
    lines.join("\n")
}

fn build_llm_status(
    base_url: &str,
    model: &str,
    mode: AppMode,
    healthy: bool,
    message: String,
) -> LlmStatus {
    LlmStatus {
        provider: "ollama".to_string(),
        base_url: base_url.to_string(),
        model: model.to_string(),
        healthy,
        message,
        mode,
    }
}

fn llm_health_error_message(err: &LlmError) -> String {
    match err {
        LlmError::ConnectionFailed(detail) => {
            format!("无法连接到本地 Ollama 服务：{}", detail)
        }
        LlmError::Timeout => "本地 Ollama 健康检查超时".to_string(),
        LlmError::ModelNotFound(model) => format!("本地 Ollama 未找到模型：{}", model),
        LlmError::InvalidResponse(detail) => {
            format!("本地 Ollama 返回了无效响应：{}", detail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rusqlite::{params, Connection};
    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        sync::{Arc, Mutex, OnceLock},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Clone)]
    struct MockQueryProvider {
        response: String,
        prompt_log: Arc<Mutex<Vec<String>>>,
    }

    impl MockQueryProvider {
        fn new(response: impl Into<String>, prompt_log: Arc<Mutex<Vec<String>>>) -> Self {
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

        async fn health_check(&self) -> Result<bool, LlmError> {
            Ok(true)
        }
    }

    #[test]
    fn default_config_path_points_to_project_root_runtime_dir() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
        let path = AppState::default_config_path();

        assert_eq!(path, root.join(".runtime").join("app-config.json"));
    }

    #[test]
    fn default_paths_point_to_project_root_targets() {
        let vault_dir = make_temp_dir("llm-wiki-default-paths");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));

        let paths = state.default_paths();
        assert_eq!(paths.vault_path, root.join("vault").to_string_lossy());
        #[cfg(windows)]
        assert_eq!(paths.ingest_source_path, r"E:\llm-wiki\test-llm.md");

        #[cfg(not(windows))]
        assert_eq!(
            paths.ingest_source_path,
            root.join("test-llm.md").to_string_lossy()
        );
    }

    #[test]
    fn query_ask_rejects_empty_question() {
        let vault_dir = make_temp_dir("llm-wiki-query-empty");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.query_ask("   ".to_string());
        assert!(result.is_err());
        assert_eq!(result.err(), Some("问题不能为空".to_string()));
    }

    #[test]
    fn lint_report_defaults_severity_stats_when_uninitialized() {
        let vault_dir = make_temp_dir("llm-wiki-lint-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let report = state.lint_report();
        assert_eq!(report.summary, "Vault 未初始化");
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.severity_stats.error, 1);
        assert_eq!(report.severity_stats.warning, 0);
        assert_eq!(report.severity_stats.info, 0);
    }

    #[test]
    fn query_answer_result_defaults_missing_search_strategy() {
        let value = serde_json::json!({
            "question": "Q",
            "answer": "A",
            "citations": [],
            "matched_pages": [],
            "mode": "Hybrid",
            "checked_at": "1",
            "answer_strategy": "rule"
        });

        let result: QueryAnswerResult =
            serde_json::from_value(value).expect("反序列化 QueryAnswerResult 失败");

        assert_eq!(result.search_strategy, "empty");
        assert_eq!(result.answer_strategy, "rule");
    }

    #[test]
    fn query_ask_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-query-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.query_ask("rust wiki".to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn recent_wiki_pages_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-recent-wiki-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.recent_wiki_pages(10);
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn recent_wiki_pages_returns_db_rows() {
        let vault_dir = make_temp_dir("llm-wiki-recent-wiki");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        // 创建实际的 wiki 文件（assert_paths_semantically_equal 需要文件存在）
        let wiki_a_path = vault_dir.join("wiki").join("a.md");
        let wiki_b_path = vault_dir.join("wiki").join("b.md");
        fs::write(&wiki_a_path, "# Wiki A\nContent A").expect("创建 wiki a 失败");
        fs::write(&wiki_b_path, "# Wiki B\nContent B").expect("创建 wiki b 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-wiki", "source", "raw", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Wiki A",
                wiki_a_path.to_string_lossy().to_string(),
                "summary a",
                "1",
                "2"
            ],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Wiki B",
                wiki_b_path.to_string_lossy().to_string(),
                "summary b",
                "1",
                "3"
            ],
        )
        .expect("写入 wiki_pages 失败");

        let pages = state.recent_wiki_pages(10).expect("读取 recent wiki 失败");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].title, "Wiki B");
        assert_eq!(pages[1].title, "Wiki A");
        assert_paths_semantically_equal(
            &wiki_b_path,
            pages[0].display_path.as_deref().expect("缺少显示路径"),
        );
        assert_paths_semantically_equal(
            &wiki_a_path,
            pages[1].display_path.as_deref().expect("缺少显示路径"),
        );
    }

    #[test]
    fn search_wiki_pages_filters_rows() {
        let vault_dir = make_temp_dir("llm-wiki-search-wiki");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params!["hash-search", "source", "raw", "1"],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Rust Wiki",
                vault_dir.join("wiki").join("rust.md").to_string_lossy().to_string(),
                "rust topic",
                "1",
                "2"
            ],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "Tauri Wiki",
                vault_dir.join("wiki").join("tauri.md").to_string_lossy().to_string(),
                "desktop topic",
                "1",
                "3"
            ],
        )
        .expect("写入 wiki_pages 失败");

        let rust_pages = state
            .search_wiki_pages("rust".to_string(), 10)
            .expect("搜索 wiki 页面失败");
        assert_eq!(rust_pages.len(), 1);
        assert_eq!(rust_pages[0].title, "Rust Wiki");

        let all_pages = state
            .search_wiki_pages("   ".to_string(), 10)
            .expect("读取最近 wiki 页面失败");
        assert_eq!(all_pages.len(), 2);
    }

    #[test]
    fn wiki_page_detail_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.wiki_page_detail("wiki/a.md".to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn wiki_page_detail_reads_markdown_content() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("detail.md");
        fs::write(&page_path, "# Detail Title\n\n正文内容。").expect("写入页面失败");

        let detail = state
            .wiki_page_detail(page_path.to_string_lossy().to_string())
            .expect("读取页面详情失败");
        assert_eq!(detail.title, "Detail Title");
        assert!(detail.content.contains("正文内容"));
        assert_paths_semantically_equal(&page_path, &detail.path);
        assert_paths_semantically_equal(&page_path, &detail.display_path);
    }

    #[test]
    fn friendly_display_path_str_strips_windows_verbatim_prefix() {
        assert_eq!(
            friendly_display_path_str(r"\\?\C:\llm-wiki\vault\wiki\detail.md"),
            r"C:\llm-wiki\vault\wiki\detail.md"
        );
        assert_eq!(
            friendly_display_path_str(r"\\?\UNC\server\share\vault\wiki\detail.md"),
            r"\\server\share\vault\wiki\detail.md"
        );
    }

    #[test]
    fn wiki_page_detail_accepts_wiki_relative_path() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-relative");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("relative.md");
        fs::write(&page_path, "# Relative Title\n\n相对路径页面。").expect("写入页面失败");

        let detail = state
            .wiki_page_detail("wiki/relative.md".to_string())
            .expect("读取页面详情失败");
        assert_eq!(detail.title, "Relative Title");
        assert!(detail.content.contains("相对路径页面"));
    }

    #[test]
    fn wiki_page_detail_rejects_outside_wiki_root() {
        let vault_dir = make_temp_dir("llm-wiki-page-detail-safe");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let outside_path = vault_dir.join("outside.md");
        fs::write(&outside_path, "# Outside").expect("写入外部文件失败");

        let result = state.wiki_page_detail(outside_path.to_string_lossy().to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("只允许读取 vault/wiki 目录下的页面".to_string())
        );
    }

    #[test]
    fn wiki_page_citations_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let result = state.wiki_page_citations("wiki/a.md".to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn wiki_page_citations_returns_rows_and_target_existence_flags() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let target_path = vault_dir.join("wiki").join("detail.md");
        fs::write(&target_path, "# Detail\n\n页面正文。").expect("写入页面失败");
        let inside_cited_path = vault_dir.join("wiki").join("cited.md");
        fs::write(&inside_cited_path, "# Cited\n\n被引用页面。").expect("写入引用页失败");
        let outside_cited_path = vault_dir.join("outside-cited.md");
        fs::write(&outside_cited_path, "# Outside\n\n外部页面。").expect("写入外部引用页失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                fs::canonicalize(&target_path)
                    .expect("规范化目标页失败")
                    .to_string_lossy()
                    .to_string(),
                fs::canonicalize(&inside_cited_path)
                    .expect("规范化引用页失败")
                    .to_string_lossy()
                    .to_string(),
                7_i64,
                "命中本地页面",
                "1"
            ],
        )
        .expect("写入 citations 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                fs::canonicalize(&target_path)
                    .expect("规范化目标页失败")
                    .to_string_lossy()
                    .to_string(),
                outside_cited_path.to_string_lossy().to_string(),
                4_i64,
                "越界引用",
                "2"
            ],
        )
        .expect("写入 citations 失败");

        let citations = state
            .wiki_page_citations(target_path.to_string_lossy().to_string())
            .expect("读取页面引用失败");
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].score, 7);
        assert_eq!(citations[0].excerpt, "命中本地页面");
        assert!(citations[0].target_exists);
        assert_paths_semantically_equal(
            &inside_cited_path,
            citations[0]
                .cited_page_display_path
                .as_deref()
                .expect("缺少显示路径"),
        );
        assert_eq!(
            citations[0].cited_page_path,
            fs::canonicalize(&inside_cited_path)
                .expect("规范化引用页失败")
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(citations[1].score, 4);
        assert_eq!(citations[1].excerpt, "越界引用");
        assert!(!citations[1].target_exists);
        assert_paths_semantically_equal(
            &outside_cited_path,
            citations[1]
                .cited_page_display_path
                .as_deref()
                .expect("缺少显示路径"),
        );
        assert_eq!(citations[1].cited_page_path, outside_cited_path.to_string_lossy());
    }

    #[test]
    fn wiki_page_citations_rejects_outside_wiki_root() {
        let vault_dir = make_temp_dir("llm-wiki-page-citations-safe");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let outside_path = vault_dir.join("outside.md");
        fs::write(&outside_path, "# Outside").expect("写入外部文件失败");

        let result = state.wiki_page_citations(outside_path.to_string_lossy().to_string());
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("只允许读取 vault/wiki 目录下的页面".to_string())
        );
    }

    #[test]
    fn query_ask_returns_matches_with_citations() {
        let vault_dir = make_temp_dir("llm-wiki-query-hit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let page_a = vault_dir.join("wiki").join("rust-notes.md");
        let page_b = vault_dir.join("wiki").join("tauri-notes.md");
        fs::write(
            &page_a,
            "# Rust Notes\nRust ownership and borrow checker basics.\n",
        )
        .expect("写入 rust-notes 失败");
        fs::write(
            &page_b,
            "# Tauri Notes\nTauri integrates Rust backend and WebView UI.\n",
        )
        .expect("写入 tauri-notes 失败");

        let result = state
            .query_ask("Rust backend".to_string())
            .expect("query_ask 应返回成功");

        assert_eq!(result.question, "Rust backend");
        assert!(!result.matched_pages.is_empty());
        assert!(!result.citations.is_empty());
        assert_eq!(result.mode, AppMode::Hybrid);
        assert_eq!(result.search_strategy, "scan");
        assert_eq!(result.answer_strategy, "rule");
        assert!(result
            .citations
            .iter()
            .any(|item| item.page_path.ends_with("rust-notes.md")));
        let rust_citation = result
            .citations
            .iter()
            .find(|item| item.page_path.ends_with("rust-notes.md"))
            .expect("缺少 rust-notes 引用");
        assert_paths_semantically_equal(
            &page_a,
            rust_citation.display_path.as_deref().expect("缺少显示路径"),
        );
    }

    #[test]
    fn generate_query_answer_with_provider_uses_llm_strategy_and_prompt() {
        let vault_dir = make_temp_dir("llm-wiki-query-llm");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let prompt_log = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn LlmProvider> =
            Arc::new(MockQueryProvider::new("本地 LLM 合成回答", prompt_log.clone()));
        let matches = vec![WikiMatch {
            page_path: vault_dir.join("wiki").join("prompt.md").to_string_lossy().to_string(),
            score: 7,
            excerpt: "核心证据片段".to_string(),
        }];

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let (answer, strategy) = runtime.block_on(async {
            state
                .generate_query_answer_with_provider("核心目标是什么", &matches, Some(provider))
                .await
        });

        assert_eq!(strategy, "llm");
        assert_eq!(answer, "本地 LLM 合成回答");

        let prompts = prompt_log.lock().expect("读取 prompt 失败");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("核心目标是什么"));
        assert!(prompts[0].contains("prompt.md"));
        assert!(prompts[0].contains("核心证据片段"));
    }

    #[test]
    fn generate_query_answer_with_provider_falls_back_to_rule_on_empty_response() {
        let vault_dir = make_temp_dir("llm-wiki-query-fallback");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let prompt_log = Arc::new(Mutex::new(Vec::new()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockQueryProvider::new("   ", prompt_log.clone()));
        let matches = vec![WikiMatch {
            page_path: vault_dir.join("wiki").join("fallback.md").to_string_lossy().to_string(),
            score: 3,
            excerpt: "回退证据".to_string(),
        }];
        let expected = build_query_answer("需要回退吗", &matches);

        let runtime = tokio::runtime::Runtime::new().expect("创建 tokio runtime 失败");
        let (answer, strategy) = runtime.block_on(async {
            state
                .generate_query_answer_with_provider("需要回退吗", &matches, Some(provider))
                .await
        });

        assert_eq!(strategy, "rule");
        assert_eq!(answer, expected);

        let prompts = prompt_log.lock().expect("读取 prompt 失败");
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("fallback.md"));
    }

    #[test]
    fn llm_health_error_message_maps_known_errors() {
        assert_eq!(
            llm_health_error_message(&LlmError::Timeout),
            "本地 Ollama 健康检查超时"
        );
        assert_eq!(
            llm_health_error_message(&LlmError::ModelNotFound("mistral".to_string())),
            "本地 Ollama 未找到模型：mistral"
        );
    }

    #[test]
    fn build_llm_status_formats_expected_fields() {
        let status = build_llm_status(
            "http://localhost:11434",
            "llama3:8b",
            AppMode::StrictLocal,
            true,
            "本地 Ollama 可用".to_string(),
        );

        assert_eq!(status.provider, "ollama");
        assert_eq!(status.base_url, "http://localhost:11434");
        assert_eq!(status.model, "llama3:8b");
        assert!(status.healthy);
        assert_eq!(status.message, "本地 Ollama 可用");
        assert_eq!(status.mode, AppMode::StrictLocal);
    }

    #[test]
    fn search_wiki_matches_with_fts_prefers_fts_strategy() {
        let vault_dir = make_temp_dir("llm-wiki-query-fts");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let wiki_dir = vault_dir.join("wiki");
        let wiki_path = wiki_dir.join("fts-hit.md");
        let content = "# FTS Hit\nRust backend with tauri integration.\n";
        fs::write(&wiki_path, content).expect("写入 fts-hit 失败");
        db::upsert_fts_page(&db_path, &wiki_path, "fts-hit", content).expect("写入 fts 索引失败");

        let tokens = tokenize_query("Rust backend");
        let (matches, strategy, fts_error) = search_wiki_matches_with_fts(
            &db_path,
            &wiki_dir,
            &tokens,
            "Rust backend",
            3,
        )
        .expect("执行检索失败");

        assert!(fts_error.is_none());
        assert_eq!(strategy, "fts");
        assert!(!matches.is_empty());
        assert!(matches
            .iter()
            .any(|item| item.page_path.ends_with("fts-hit.md")));
    }

    #[test]
    fn tokenize_query_supports_cjk_segments_and_bigrams() {
        let tokens = tokenize_query("这个项目的核心目标是什么？query v1");

        assert!(tokens.iter().any(|item| item == "query"));
        assert!(tokens.iter().any(|item| item == "这个项目的核心目标是什么"));
        assert!(tokens.iter().any(|item| item == "核心"));
        assert!(tokens.iter().any(|item| item == "目标"));
    }

    #[test]
    fn tokenize_query_filters_stopwords() {
        let tokens = tokenize_query("the 这个 项目 的 核心 目标 是 什么");

        assert!(!tokens.iter().any(|item| item == "the"));
        assert!(!tokens.iter().any(|item| item == "的"));
        assert!(!tokens.iter().any(|item| item == "是"));
        assert!(tokens.iter().any(|item| item == "项目"));
        assert!(tokens.iter().any(|item| item == "核心"));
    }

    #[test]
    fn query_ask_with_options_applies_top_k_clamp() {
        let vault_dir = make_temp_dir("llm-wiki-query-topk");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        for idx in 0..4 {
            let page_path = vault_dir.join("wiki").join(format!("page-{}.md", idx));
            fs::write(
                &page_path,
                format!("# 页面{}\n这个项目的核心目标是什么。\n", idx),
            )
            .expect("写入测试页面失败");
            let db_path = vault_dir.join(".app").join("meta.db");
            db::upsert_fts_page(&db_path, &page_path, &format!("page-{}", idx), "这个项目的核心目标是什么。")
                .expect("写入 fts 索引失败");
        }

        let result = state
            .query_ask_with_options(
                "这个项目的核心目标是什么".to_string(),
                QueryAskOptions { top_k: Some(1) },
            )
            .expect("query_ask_with_options 应返回成功");
        assert_eq!(result.matched_pages.len(), 1);
        assert_eq!(result.search_strategy, "fts");
        assert!(result.citations.iter().all(|item| item.display_path.is_some()));

        let result = state
            .query_ask_with_options(
                "这个项目的核心目标是什么".to_string(),
                QueryAskOptions { top_k: Some(99) },
            )
            .expect("query_ask_with_options 应返回成功");
        assert!(result.matched_pages.len() <= QUERY_TOP_K_MAX);
        assert_eq!(result.search_strategy, "fts");
        assert!(result.citations.iter().all(|item| item.display_path.is_some()));
    }

    #[test]
    fn set_query_top_k_persists_to_runtime_config() {
        let vault_dir = make_temp_dir("llm-wiki-query-settings");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let settings = state.set_query_top_k(6).expect("设置 top_k 失败");
        assert_eq!(settings.top_k, 6);

        let config_raw = fs::read_to_string(vault_dir.join(".runtime").join("app-config.json"))
            .expect("读取运行时配置失败");
        let config: AppConfig = serde_json::from_str(&config_raw).expect("解析运行时配置失败");
        assert_eq!(config.query_top_k, Some(6));
    }

    #[test]
    fn query_ask_with_options_uses_persisted_default_top_k() {
        let vault_dir = make_temp_dir("llm-wiki-query-default-topk");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");
        state.set_query_top_k(2).expect("设置 top_k 失败");

        for idx in 0..4 {
            let page_path = vault_dir.join("wiki").join(format!("topk-default-{}.md", idx));
            fs::write(&page_path, format!("# 页面{}\nquery default topk 测试。\n", idx))
                .expect("写入测试页面失败");
            let db_path = vault_dir.join(".app").join("meta.db");
            db::upsert_fts_page(
                &db_path,
                &page_path,
                &format!("topk-default-{}", idx),
                "query default topk 测试。",
            )
            .expect("写入 fts 索引失败");
        }

        let result = state
            .query_ask_with_options(
                "query default topk".to_string(),
                QueryAskOptions::default(),
            )
            .expect("query_ask_with_options 应返回成功");
        assert_eq!(result.matched_pages.len(), 2);
        assert_eq!(result.search_strategy, "fts");
    }

    #[test]
    fn save_query_answer_requires_initialized_vault() {
        let vault_dir = make_temp_dir("llm-wiki-save-query-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        let input = SaveQueryAnswerInput {
            question: "q".to_string(),
            answer: "a".to_string(),
            citations: Vec::new(),
            title: None,
        };

        let result = state.save_query_answer(input);
        assert!(result.is_err());
        assert_eq!(
            result.err(),
            Some("请先调用 init_vault 初始化 Vault".to_string())
        );
    }

    #[test]
    fn save_query_answer_writes_wiki_file_and_updates_db() {
        let vault_dir = make_temp_dir("llm-wiki-save-query");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state
            .init_vault(vault_dir.clone())
            .expect("初始化 Vault 失败");

        let input = SaveQueryAnswerInput {
            question: "这个项目的核心目标是什么？".to_string(),
            answer: "目标是构建 Windows 优先的个人 Wiki 桌面应用。".to_string(),
            citations: vec![QueryCitation {
                page_path: vault_dir
                    .join("wiki")
                    .join("ingest-1.md")
                    .to_string_lossy()
                    .to_string(),
                display_path: None,
                score: 3,
                excerpt: "本项目用于实现一个 Windows 优先的个人 Wiki 桌面应用".to_string(),
            }],
            title: Some("问答-核心目标".to_string()),
        };
        let result = state.save_query_answer(input).expect("保存 Query 结果失败");

        assert!(PathBuf::from(&result.wiki_path).exists());
        let index_content =
            fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        assert!(index_content.contains("问答-核心目标"));

        let db_path = vault_dir.join(".app").join("meta.db");
        let page_paths = db::list_wiki_page_paths(&db_path).expect("读取 wiki_pages 失败");
        assert!(page_paths.iter().any(|path| path == &result.wiki_path));
    }

    #[test]
    fn search_wiki_matches_from_paths_applies_phrase_title_boost_and_deduplicates() {
        let vault_dir = make_temp_dir("llm-wiki-query-rank");
        let _guard = TempDirGuard(vault_dir.clone());
        let wiki_dir = vault_dir.join("wiki");
        fs::create_dir_all(&wiki_dir).expect("创建 wiki 目录失败");

        let high_path = wiki_dir.join("high.md");
        let low_path = wiki_dir.join("low.md");
        fs::write(
            &high_path,
            "# 核心目标\n这个项目的核心目标是什么 这是完整短语。\n",
        )
        .expect("写入 high.md 失败");
        fs::write(
            &low_path,
            "# 其他说明\n这个 项目 核心 目标 分散出现。\n",
        )
        .expect("写入 low.md 失败");

        let page_paths = vec![
            high_path.to_string_lossy().to_string(),
            high_path.to_string_lossy().to_string(),
            low_path.to_string_lossy().to_string(),
        ];
        let question = "这个项目的核心目标是什么";
        let tokens = tokenize_query(question);
        let matches = search_wiki_matches_from_paths(&page_paths, &tokens, question, 5)
            .expect("执行页面匹配失败");

        assert_eq!(matches.len(), 2);
        assert!(matches[0].page_path.ends_with("high.md"));
        assert!(matches[0].score >= matches[1].score);
    }

    #[test]
    fn lint_report_detects_missing_index_entries_orphans_and_db_mismatches() {
        let vault_dir = make_temp_dir("llm-wiki-lint-pages");
        let _guard = TempDirGuard(vault_dir.clone());

        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let present_path = vault_dir.join("wiki").join("present.md");
        let orphan_path = vault_dir.join("wiki").join("orphan.md");
        fs::write(&present_path, "# present\n").expect("写入 present 失败");
        fs::write(&orphan_path, "# orphan\n").expect("写入 orphan 失败");

        fs::write(
            vault_dir.join("index.md"),
            "# Index\n\n## Imported Pages\n- [[wiki/present.md|present]]\n- [[wiki/missing.md|missing]]\n",
        )
        .expect("写入 index.md 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "hash-1",
                vault_dir.join("source.md").to_string_lossy().to_string(),
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                "1"
            ],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                source_id,
                "present",
                present_path.to_string_lossy().to_string(),
                "present summary",
                "1",
                "1"
            ],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                present_path.to_string_lossy().to_string(),
                vault_dir.join("wiki").join("missing-cited.md").to_string_lossy().to_string(),
                3_i64,
                "missing citation",
                "1"
            ],
        )
        .expect("写入 citations 失败");

        let report = state.lint_report();
        let codes: BTreeSet<_> = report.issues.iter().map(|issue| issue.code.as_str()).collect();

        assert!(codes.contains("MISSING_INDEX_ENTRY"));
        assert!(codes.contains("ORPHAN_WIKI_PAGE"));
        assert!(codes.contains("DB_MISSING_PAGE_RECORD"));
        assert!(codes.contains("BROKEN_CITATION"));
        assert!(!codes.contains("VAULT_NOT_INITIALIZED"));
        assert_eq!(report.severity_stats.error, 1);
        assert_eq!(report.severity_stats.warning, 3);
        assert_eq!(report.severity_stats.info, 0);
    }

    #[test]
    fn lint_report_flags_stale_pending_tasks() {
        let vault_dir = make_temp_dir("llm-wiki-lint-tasks");
        let _guard = TempDirGuard(vault_dir.clone());

        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let db_path = vault_dir.join(".app").join("meta.db");
        let conn = Connection::open(&db_path).expect("打开数据库失败");
        conn.execute(
            "INSERT INTO sources (content_hash, source_path, raw_path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                "hash-2",
                vault_dir.join("source.md").to_string_lossy().to_string(),
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                "1"
            ],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tasks (source_id, kind, status, raw_path, wiki_path, error, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7)",
            params![
                source_id,
                "ingest_markdown",
                "queued",
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                vault_dir.join("wiki").join("stale.md").to_string_lossy().to_string(),
                "1",
                "1"
            ],
        )
        .expect("写入 tasks 失败");

        let report = state.lint_report();

        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "STALE_PENDING_TASK"));
        assert_eq!(report.severity_stats.error, 0);
        assert_eq!(report.severity_stats.warning, 1);
        assert_eq!(report.severity_stats.info, 0);
    }

    struct TempDirGuard(PathBuf);

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    fn make_test_state(vault_dir: &Path) -> AppState {
        AppState {
            inner: Mutex::new(AppStateData {
                mode: AppMode::Hybrid,
                vault_path: None,
                query_top_k: QUERY_TOP_K_DEFAULT,
                logs: Vec::new(),
                next_log_id: 1,
                config_snapshot: None,
            }),
            config_path: vault_dir.join(".runtime").join("app-config.json"),
            llm_provider: OnceLock::new(),
        }
    }

    fn assert_paths_semantically_equal(expected: &Path, actual: &str) {
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
}
