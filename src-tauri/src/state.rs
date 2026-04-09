use std::{
    collections::{BTreeSet, HashSet},
    fs,
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use crate::{
    db,
    models::{
        AppConfig, AppMode, AppOverview, DefaultPaths, IngestResult, LintIssue, LintReport,
        LogEntry, LogLevel, ModeChangeResult, QueryAnswerResult, QueryAskOptions, QueryCitation,
        VaultInitResult,
    },
    vault,
};

const STALE_PENDING_TASK_THRESHOLD_MS: u128 = 24 * 60 * 60 * 1000;

/// 进程内状态。
#[derive(Debug)]
pub struct AppState {
    inner: Mutex<AppStateData>,
    config_path: PathBuf,
}

/// 状态快照。
#[derive(Debug, Clone)]
struct AppStateData {
    mode: AppMode,
    vault_path: Option<PathBuf>,
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
        let (mode, vault_path, config_snapshot) = Self::load_config(&config_path);
        let serialized = Self::serialize_config(mode, vault_path.as_deref());
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
                next_log_id: 3,
                logs,
                config_snapshot: runtime_snapshot,
            }),
            config_path,
        }
    }

    pub fn set_mode(&self, mode: AppMode) -> ModeChangeResult {
        let mut guard = self.inner.lock().expect("状态锁已被污染");
        let previous_mode = guard.mode;
        let expected_snapshot = guard.config_snapshot.clone();
        let vault_path = guard.vault_path.clone();

        match self.persist_config(mode, vault_path.as_deref(), expected_snapshot.as_deref()) {
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

    pub fn ingest_markdown(&self, source_path: PathBuf) -> Result<IngestResult, String> {
        let vault_path = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            guard
                .vault_path
                .clone()
                .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
        };

        match vault::ingest_markdown(&vault_path, &source_path) {
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
            ingest_source_path: root.join("README.md").to_string_lossy().to_string(),
        }
    }

    pub fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        let guard = self.inner.lock().expect("状态锁已被污染");
        guard.logs.iter().rev().take(limit).cloned().collect()
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
                return LintReport {
                    mode,
                    checked_at: current_timestamp_ms(),
                    summary: "Vault 未初始化".to_string(),
                    issues,
                };
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

        LintReport {
            mode,
            checked_at: current_timestamp_ms(),
            summary: format!("已返回 {} 条 lint 问题", issues.len()),
            issues,
        }
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

        let (mode, vault_path) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.vault_path.clone())
        };

        let vault_path = vault_path.ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?;
        let wiki_dir = vault_path.join("wiki");
        let db_path = vault_path.join(".app").join("meta.db");
        let tokens = tokenize_query(&normalized_question);
        let top_k = normalize_top_k(options.top_k);
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
            .map(|item| QueryCitation {
                page_path: item.page_path.clone(),
                score: item.score,
                excerpt: item.excerpt.clone(),
            })
            .collect::<Vec<_>>();
        let answer = build_query_answer(&normalized_question, &matches);
        let matched_pages = matches
            .iter()
            .map(|item| item.page_path.clone())
            .collect::<Vec<_>>();

        self.push_log(
            LogLevel::Info,
            format!(
                "Query 检索完成: '{}'，命中 {} 页，策略={}，top_k={}",
                normalized_question,
                matched_pages.len(),
                search_strategy,
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
        })
    }

    fn set_vault_path(&self, vault_path: PathBuf) -> Result<(), String> {
        let (mode, expected_snapshot) = {
            let guard = self.inner.lock().expect("状态锁已被污染");
            (guard.mode, guard.config_snapshot.clone())
        };

        {
            let mut guard = self.inner.lock().expect("状态锁已被污染");
            guard.vault_path = Some(vault_path.clone());
        }

        match self.persist_config(mode, Some(vault_path.as_path()), expected_snapshot.as_deref()) {
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

    fn load_config(config_path: &Path) -> (AppMode, Option<PathBuf>, Option<String>) {
        match fs::read_to_string(config_path) {
            Ok(raw) => match serde_json::from_str::<AppConfig>(&raw) {
                Ok(config) => (
                    config.mode,
                    config.vault_path.map(PathBuf::from),
                    Some(raw),
                ),
                Err(_) => (AppMode::default(), None, Some(raw)),
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => (AppMode::default(), None, None),
            Err(_) => (AppMode::default(), None, None),
        }
    }

    fn serialize_config(mode: AppMode, vault_path: Option<&Path>) -> String {
        serde_json::to_string_pretty(&AppConfig {
            mode,
            vault_path: vault_path.map(|path| path.to_string_lossy().to_string()),
        })
        .expect("配置序列化失败")
    }

    fn persist_config(
        &self,
        mode: AppMode,
        vault_path: Option<&Path>,
        expected_snapshot: Option<&str>,
    ) -> Result<String, String> {
        let serialized = Self::serialize_config(mode, vault_path);

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
    top_k.unwrap_or(3).clamp(1, 8)
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

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };

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
        assert_eq!(
            paths.ingest_source_path,
            root.join("README.md").to_string_lossy()
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
        assert!(result
            .citations
            .iter()
            .any(|item| item.page_path.ends_with("rust-notes.md")));
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

        let result = state
            .query_ask_with_options(
                "这个项目的核心目标是什么".to_string(),
                QueryAskOptions { top_k: Some(99) },
            )
            .expect("query_ask_with_options 应返回成功");
        assert!(result.matched_pages.len() <= 8);
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

        let report = state.lint_report();
        let codes: BTreeSet<_> = report.issues.iter().map(|issue| issue.code.as_str()).collect();

        assert!(codes.contains("MISSING_INDEX_ENTRY"));
        assert!(codes.contains("ORPHAN_WIKI_PAGE"));
        assert!(codes.contains("DB_MISSING_PAGE_RECORD"));
        assert!(!codes.contains("VAULT_NOT_INITIALIZED"));
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
                logs: Vec::new(),
                next_log_id: 1,
                config_snapshot: None,
            }),
            config_path: vault_dir.join(".runtime").join("app-config.json"),
        }
    }
}
