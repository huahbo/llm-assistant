//! Lint 服务：Vault 结构检查、语义分析、补丁应用。
//! H16 Phase 6 迁移自 state.rs。

use super::AppState;
use crate::{
    db,
    llm::LlmProvider,
    models::{
        AppMode, LintIssue, LintPatchApplyInput, LintPatchApplyResult,
        LintPatchBatchApplyItemResult, LintPatchBatchApplyResult, LintPatchBatchApplyStatus,
        LintPatchPreview, LintPatchSuggestion, LintReport, LintSeverityStats, LogLevel,
    },
    vault,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io,
    path::Path,
    sync::Arc,
};

// ─────────────────────────────────────────────
// Public API
// ─────────────────────────────────────────────

/// 对 Vault 执行完整规则 Lint，返回所有问题列表。
pub fn lint_report(state: &AppState) -> LintReport {
    let (mode, vault_path) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
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
                            message: format!("引用记录所属页面不存在: {}", citation.page_path),
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

    // 1. 扫描失效 wiki-link
    let link_regex = regex::Regex::new(r"\[\[([^|\]]+)(?:\|[^\]]+)?\]\]").unwrap();
    for page_path in &wiki_page_paths {
        if let Ok(content) = fs::read_to_string(page_path) {
            for caps in link_regex.captures_iter(&content) {
                let target_name = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                if target_name.is_empty() {
                    continue;
                }

                if super::resolve_existing_wiki_page_path(vault_path, target_name).is_err() {
                    issues.push(LintIssue {
                        code: "broken_wikilink".to_string(),
                        severity: "warning".to_string(),
                        message: format!(
                            "页面存在失效的 wiki-link：指向不存在的目标 {}",
                            target_name
                        ),
                        path: Some(page_path.clone()),
                        suggestion: "请修复链接名称，或确认该页面已创建。".to_string(),
                    });
                }
            }
        }
    }

    let (_broken_wiki_links, outbound_wiki_links, inbound_wiki_link_counts) =
        collect_wiki_link_graph(vault_path, &wiki_page_paths);

    // 注意：collect_wiki_link_graph 返回的 broken_wiki_links 与我们手动实现的逻辑重叠，建议优先使用其中之一或整合。
    // 为保持一致性，如果手动实现已覆盖需求，可以移除此处 collect_wiki_link_graph 返回的旧逻辑或根据业务需要调整。
    for (source_path, missing_targets) in collect_xref_missing_sources(&outbound_wiki_links) {
        issues.push(LintIssue {
            code: "xref_missing".to_string(),
            severity: "warning".to_string(),
            message: format!(
                "页面缺少反向交叉引用：{} -> {}",
                source_path,
                missing_targets.join(", ")
            ),
            path: Some(source_path),
            suggestion: "应用补丁为目标页面追加指向当前页的 See Also 反向链接".to_string(),
        });
    }

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
            let inbound = inbound_wiki_link_counts.get(path).copied().unwrap_or(0);
            if inbound == 0 {
                issues.push(LintIssue {
                    code: "orphan".to_string(),
                    severity: "warning".to_string(),
                    message: format!("页面未被 index.md 或其他页面引用: {}", path),
                    path: Some(path.clone()),
                    suggestion: "把页面加入 index.md，或在相关页面补齐 wiki-link 引用"
                        .to_string(),
                });
            }
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
                let checked_at_ms =
                    super::current_timestamp_ms().parse::<u128>().unwrap_or_default();
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

    // 检查 wiki 目录下标记为 stale 的页面
    let wiki_dir = vault_path.join("wiki");
    if wiki_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&wiki_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                    continue;
                }
                if let Ok(file_content) = fs::read_to_string(&path) {
                    if let Some(fm) = super::wiki_service::parse_wiki_frontmatter(&file_content) {
                        if fm.stale == Some(true) {
                            issues.push(LintIssue {
                                code: "STALE_PAGE".to_string(),
                                severity: "warning".to_string(),
                                message: format!(
                                    "页面已标记为过时，建议更新或删除: {}",
                                    path.file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("unknown")
                                ),
                                path: Some(path.to_string_lossy().to_string()),
                                suggestion: "更新页面内容后取消 stale 标记，或删除该页面"
                                    .to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    build_lint_report(
        mode,
        format!("已返回 {} 条 lint 问题", issues.len()),
        issues,
    )
}

/// 预览所有 Lint 补丁建议（不执行修改）。
pub fn preview_lint_patches(state: &AppState) -> LintPatchPreview {
    let report = lint_report(state);
    let suggestions = report
        .issues
        .iter()
        .map(build_lint_patch_suggestion)
        .collect::<Vec<_>>();

    LintPatchPreview {
        generated_at: super::current_timestamp_ms(),
        total: suggestions.len(),
        suggestions,
    }
}

/// 返回完整 Lint（规则 + 语义 LLM）的 Future，可在异步命令中安全 await。
pub fn lint_report_full_future(
    state: &AppState,
) -> impl std::future::Future<Output = LintReport> + Send + 'static {
    let rules = lint_report(state);
    let (pages, _mode) = collect_semantic_lint_input(state);
    let provider = state.get_ollama_provider();
    async move {
        let semantic = run_semantic_lint(pages, Some(provider)).await;
        merge_lint_with_semantic(rules, semantic)
    }
}

/// 对单个 Wiki 页面执行快速结构检查（不依赖 LLM）。
/// - 检测 [[wiki-links]] 指向不存在的页面。
/// - 检测 frontmatter entities 中没有对应页面的实体。
pub fn quick_lint_page_impl(
    state: &AppState,
    wiki_path: &str,
) -> Result<crate::models::PageQuickLint, String> {
    // 1. 获取 vault_path
    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard.vault_path.clone()
    };
    let vault_path = vault_path.ok_or_else(|| "Vault 尚未初始化".to_string())?;

    // 2. 解析页面实际路径（兼容绝对路径与相对路径）
    let page_path = {
        let p = Path::new(wiki_path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            vault_path.join(wiki_path)
        }
    };

    // 3. 读取页面内容；文件不存在时返回空结果而非报错
    let content = match fs::read_to_string(&page_path) {
        Ok(c) => c,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(crate::models::PageQuickLint {
                wiki_path: wiki_path.to_string(),
                broken_links: vec![],
                missing_entity_pages: vec![],
                issues_count: 0,
            });
        }
        Err(err) => return Err(format!("读取页面失败: {}", err)),
    };

    // 4. 分离 frontmatter 与正文
    let (frontmatter, body) = split_frontmatter(&content);

    // 5. 检测失效 [[wiki-links]]
    let mut broken_links: Vec<String> = Vec::new();
    let mut seen_links: std::collections::HashSet<String> = std::collections::HashSet::new();
    for target in extract_wiki_link_targets(body) {
        // 将链接目标规范化为 wiki/ 相对路径后检查文件是否存在
        let slug = entity_to_slug(&target);
        let candidate = vault_path.join("wiki").join(format!("{}.md", slug));
        if !candidate.exists() && seen_links.insert(target.clone()) {
            broken_links.push(target);
        }
    }

    // 6. 检测 frontmatter entities 中缺失对应页面的实体
    let mut missing_entity_pages: Vec<String> = Vec::new();
    let mut seen_entities: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entity in parse_frontmatter_entities(frontmatter) {
        let slug = entity_to_slug(&entity);
        let candidate = vault_path.join("wiki").join(format!("{}.md", slug));
        if !candidate.exists() && seen_entities.insert(entity.clone()) {
            missing_entity_pages.push(entity);
        }
    }

    let issues_count = broken_links.len() + missing_entity_pages.len();
    Ok(crate::models::PageQuickLint {
        wiki_path: wiki_path.to_string(),
        broken_links,
        missing_entity_pages,
        issues_count,
    })
}

/// 获取 Vault 统计数据。
pub fn get_vault_stats_impl(state: &AppState) -> Result<crate::models::VaultStats, String> {
    let db_path = state
        .outbox_db_path()
        .ok_or_else(|| "Vault 未初始化".to_string())?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    crate::db::get_vault_stats_from_db(&db_path, now_ms)
}

/// 运行完整 Lint 并写入 outbox。
pub async fn run_lint_with_outbox(state: &AppState) -> LintReport {
    let report = lint_report_full_future(state).await;
    state.record_outbox_event(
        "lint_run_finished",
        serde_json::json!({
            "checked_at": report.checked_at.clone(),
            "issue_count": report.issues.len(),
            "severity_stats": {
                "error": report.severity_stats.error,
                "warning": report.severity_stats.warning,
                "info": report.severity_stats.info,
            },
        }),
    );
    report
}

/// 应用单个 Lint 补丁。
pub fn apply_lint_patch(
    state: &AppState,
    input: LintPatchApplyInput,
) -> Result<LintPatchApplyResult, String> {
    let issue_code = input.issue_code.trim().to_string();
    let input_path = input.path.clone();
    if issue_code.is_empty() {
        return Err("issue_code 不能为空".to_string());
    }

    let vault_path = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        guard
            .vault_path
            .clone()
            .ok_or_else(|| "请先调用 init_vault 初始化 Vault".to_string())?
    };

    let (applied, message, touched_paths) = match issue_code.as_str() {
        "MISSING_INDEX_ENTRY" => {
            let path = input_path
                .as_deref()
                .ok_or_else(|| "MISSING_INDEX_ENTRY 需要提供 path".to_string())?;
            let page_path = super::resolve_existing_wiki_page_path(&vault_path, path)?;
            let index_path = vault_path.join("index.md");
            if !index_path.exists() {
                return Err("index.md 缺失，请先处理 INDEX_MISSING".to_string());
            }

            let link_target = wiki_link_target_from_path(&vault_path, &page_path)?;
            let link_label = wiki_link_label(&page_path);
            let changed = append_index_link_if_missing(&index_path, &link_target, &link_label)?;
            let message = if changed {
                "已补齐 index.md 引用".to_string()
            } else {
                "index.md 中已存在该页面引用，未重复写入".to_string()
            };
            let mut touched_paths = vec![index_path.to_string_lossy().to_string()];
            touched_paths.push(page_path.to_string_lossy().to_string());
            (changed, message, touched_paths)
        }
        "ORPHAN_WIKI_PAGE" | "orphan" => {
            let path = input_path
                .as_deref()
                .ok_or_else(|| format!("{} 需要提供 path", issue_code.as_str()))?;
            let page_path = super::resolve_existing_wiki_page_path(&vault_path, path)?;
            let index_path = vault_path.join("index.md");
            if !index_path.exists() {
                return Err("index.md 缺失，请先处理 INDEX_MISSING".to_string());
            }

            let link_target = wiki_link_target_from_path(&vault_path, &page_path)?;
            let link_label = wiki_link_label(&page_path);
            let changed = append_index_link_if_missing(&index_path, &link_target, &link_label)?;
            let message = if changed {
                "已将页面加入 index.md".to_string()
            } else {
                "index.md 中已存在该页面引用，未重复写入".to_string()
            };
            let mut touched_paths = vec![index_path.to_string_lossy().to_string()];
            touched_paths.push(page_path.to_string_lossy().to_string());
            (changed, message, touched_paths)
        }
        "broken_wikilink" | "BROKEN_WIKILINK" => {
            let path = input_path
                .as_deref()
                .ok_or_else(|| format!("{} 需要提供 path", issue_code.as_str()))?;
            let page_path = super::resolve_existing_wiki_page_path(&vault_path, path)?;
            let replaced = rewrite_broken_wiki_links_in_page(&vault_path, &page_path)?;
            let message = if replaced > 0 {
                format!("已将 {} 个失效 wiki-link 降级为纯文本", replaced)
            } else {
                "页面中未发现可自动修复的失效 wiki-link".to_string()
            };
            (
                replaced > 0,
                message,
                vec![page_path.to_string_lossy().to_string()],
            )
        }
        "xref_missing" | "XREF_MISSING" => {
            let path = input_path
                .as_deref()
                .ok_or_else(|| format!("{} 需要提供 path", issue_code.as_str()))?;
            let source_page = super::resolve_existing_wiki_page_path(&vault_path, path)?;
            let (updated, touched_paths) =
                apply_missing_xref_backlinks(&vault_path, &source_page)?;
            let message = if updated > 0 {
                format!("已补齐 {} 个反向交叉引用", updated)
            } else {
                "未发现需要补齐的反向交叉引用".to_string()
            };
            (updated > 0, message, touched_paths)
        }
        "INDEX_MISSING" => {
            let index_path = vault_path.join("index.md");
            let created = if index_path.exists() {
                false
            } else {
                fs::write(&index_path, seed_index_content())
                    .map_err(|err| format!("写入 index.md 失败: {}", err))?;
                true
            };
            let message = if created {
                "已创建 index.md".to_string()
            } else {
                "index.md 已存在，未作修改".to_string()
            };
            (
                created,
                message,
                vec![index_path.to_string_lossy().to_string()],
            )
        }
        "LOG_MISSING" => {
            let log_path = vault_path.join("log.md");
            let created = if log_path.exists() {
                false
            } else {
                fs::write(&log_path, seed_log_content())
                    .map_err(|err| format!("写入 log.md 失败: {}", err))?;
                true
            };
            let message = if created {
                "已创建 log.md".to_string()
            } else {
                "log.md 已存在，未作修改".to_string()
            };
            (
                created,
                message,
                vec![log_path.to_string_lossy().to_string()],
            )
        }
        _ => {
            return Err("暂不支持自动应用，请手动处理".to_string());
        }
    };

    state.push_log(
        LogLevel::Info,
        format!(
            "Lint 补丁应用完成: issue_code={}, path={}, applied={}, message={}",
            issue_code,
            input_path.as_deref().unwrap_or("无"),
            applied,
            message
        ),
    );

    record_lint_patch_event(
        state,
        &vault_path,
        &issue_code,
        input_path.as_deref(),
        applied,
        &message,
    );

    Ok(LintPatchApplyResult {
        issue_code,
        path: input_path,
        applied,
        message,
        touched_paths,
    })
}

/// 批量应用 Lint 补丁。
pub fn apply_lint_patches_batch(
    state: &AppState,
    inputs: Vec<LintPatchApplyInput>,
) -> Result<LintPatchBatchApplyResult, String> {
    let total = inputs.len();
    let mut success = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut items = Vec::with_capacity(total);

    for input in inputs {
        let issue_code = input.issue_code.trim().to_string();
        let path = input.path.clone();

        match apply_lint_patch(state, input) {
            Ok(result) => {
                let status = if result.applied {
                    success += 1;
                    LintPatchBatchApplyStatus::Success
                } else {
                    skipped += 1;
                    LintPatchBatchApplyStatus::Skipped
                };

                items.push(LintPatchBatchApplyItemResult {
                    issue_code: result.issue_code,
                    path: result.path,
                    status,
                    applied: result.applied,
                    message: result.message,
                    touched_paths: result.touched_paths,
                    error: None,
                });
            }
            Err(error) => {
                failed += 1;
                items.push(LintPatchBatchApplyItemResult {
                    issue_code,
                    path,
                    status: LintPatchBatchApplyStatus::Failed,
                    applied: false,
                    message: error.clone(),
                    touched_paths: Vec::new(),
                    error: Some(error),
                });
            }
        }
    }

    state.push_log(
        LogLevel::Info,
        format!(
            "批量应用 Lint 补丁完成：total={}，success={}，failed={}，skipped={}",
            total, success, failed, skipped
        ),
    );

    Ok(LintPatchBatchApplyResult {
        total,
        success,
        failed,
        skipped,
        items,
    })
}

// ─────────────────────────────────────────────
// Private helpers — semantic lint
// ─────────────────────────────────────────────

/// 收集语义 Lint 所需的页面数据（同步，在 State 作用域内完成）。
///
/// 返回 (页面列表[(path, title, summary)], mode)。
fn collect_semantic_lint_input(state: &AppState) -> (Vec<(String, String, String)>, AppMode) {
    let (mode, vault_path) = {
        let guard = state.inner.lock().expect("状态锁已被污染");
        (guard.mode, guard.vault_path.clone())
    };

    let pages = vault_path
        .map(|p| p.join(".app").join("meta.db"))
        .and_then(|db_path| db::list_recent_wiki_pages(&db_path, 20).ok())
        .map(|records| {
            records
                .into_iter()
                .map(|r| (r.path, r.title, r.summary))
                .collect()
        })
        .unwrap_or_default();

    (pages, mode)
}

/// 执行 LLM 语义 Lint 分析（矛盾 / 陈旧 / 覆盖度）。
///
/// - LLM 不可用时返回空列表，不报错。
/// - 最多返回 10 条语义问题。
async fn run_semantic_lint(
    pages: Vec<(String, String, String)>,
    provider: Option<Arc<dyn LlmProvider>>,
) -> Vec<LintIssue> {
    let provider = match provider {
        Some(p) => p,
        None => return Vec::new(),
    };

    if pages.is_empty() {
        return Vec::new();
    }

    // 构建页面摘要文本（每条摘要截断到 200 字符，控制 token 用量）
    let pages_text = pages
        .iter()
        .map(|(path, title, summary)| {
            let short: String = summary.chars().take(200).collect();
            format!("- [{}] {}\n  摘要: {}", path, title, short)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "你是 Wiki 内容质量审查员。以下是 Wiki 页面列表（路径+标题+摘要）。\n\
请检查并报告以下3类问题，每行一个，用 | 分隔，严格按格式输出：\n\
CODE|severity|message|path|suggestion\n\
CODE 仅限：SEMANTIC_CONTRADICTION（矛盾陈述）、SEMANTIC_STALE（过时结论）、SEMANTIC_COVERAGE_GAP（缺少重要实体页）\n\
severity 仅限：warning 或 info\n\
path 填相关页面路径，无则留空\n\
若无问题则只输出：NO_ISSUES\n\n\
Wiki 页面：\n{}",
        pages_text
    );

    match provider.complete(&prompt).await {
        Ok(response) => parse_semantic_lint_response(&response),
        Err(_) => Vec::new(),
    }
}

// ─────────────────────────────────────────────
// Private helpers — lint report building
// ─────────────────────────────────────────────

fn build_lint_report(mode: AppMode, summary: String, issues: Vec<LintIssue>) -> LintReport {
    let severity_stats = count_lint_severity_stats(&issues);

    LintReport {
        mode,
        checked_at: super::current_timestamp_ms(),
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

/// LLM 语义 Lint 合法 code 列表。
const SEMANTIC_LINT_CODES: &[&str] = &[
    "SEMANTIC_CONTRADICTION",
    "SEMANTIC_STALE",
    "SEMANTIC_COVERAGE_GAP",
];

/// 解析 LLM 返回的语义 Lint 文本为 LintIssue 列表。
///
/// 格式要求：每行 `CODE|severity|message|path|suggestion`，
/// 非法行静默跳过，最多返回 10 条。
pub(super) fn parse_semantic_lint_response(response: &str) -> Vec<LintIssue> {
    response
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.eq_ignore_ascii_case("NO_ISSUES") {
                return None;
            }
            let parts: Vec<&str> = line.splitn(5, '|').collect();
            if parts.len() < 5 {
                return None;
            }
            let code = parts[0].trim();
            let severity = parts[1].trim();
            let message = parts[2].trim();
            let path = parts[3].trim();
            let suggestion = parts[4].trim();

            if !SEMANTIC_LINT_CODES.contains(&code) {
                return None;
            }
            if severity != "warning" && severity != "info" {
                return None;
            }
            if message.is_empty() || suggestion.is_empty() {
                return None;
            }

            Some(LintIssue {
                code: code.to_string(),
                severity: severity.to_string(),
                message: message.to_string(),
                path: if path.is_empty() {
                    None
                } else {
                    Some(path.to_string())
                },
                suggestion: suggestion.to_string(),
            })
        })
        .take(10)
        .collect()
}

/// 将语义问题合并进规则 Lint 报告，更新统计与摘要。
pub(super) fn merge_lint_with_semantic(mut rules: LintReport, semantic: Vec<LintIssue>) -> LintReport {
    if semantic.is_empty() {
        return rules;
    }
    for issue in &semantic {
        match issue.severity.as_str() {
            "error" => rules.severity_stats.error += 1,
            "warning" => rules.severity_stats.warning += 1,
            "info" => rules.severity_stats.info += 1,
            _ => {}
        }
    }
    rules.issues.extend(semantic);
    let total = rules.issues.len();
    rules.summary = format!("共发现 {} 个问题（规则 + 语义分析）", total);
    rules
}

// ─────────────────────────────────────────────
// Private helpers — patch suggestion building
// ─────────────────────────────────────────────

fn lint_patch_link_target(path: Option<&str>) -> String {
    let file_name = path
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("xxx.md");
    format!("wiki/{}", file_name)
}

fn lint_patch_link_label(path: Option<&str>) -> String {
    path.and_then(|value| Path::new(value).file_stem())
        .and_then(|value| value.to_str())
        .unwrap_or("xxx")
        .to_string()
}

fn build_lint_patch_suggestion(issue: &LintIssue) -> LintPatchSuggestion {
    let (title, proposed_action, patch_preview) = match issue.code.as_str() {
        "VAULT_NOT_INITIALIZED" => (
            "初始化 Vault".to_string(),
            "先执行 init_vault 创建本地 Vault".to_string(),
            "```text\n执行 init_vault 后，系统会生成 vault/index.md、vault/log.md 和 .app/meta.db。\n```"
                .to_string(),
        ),
        "INDEX_READ_FAILED" => (
            "检查 index.md 读取".to_string(),
            "确认 index.md 可读并修复文件权限或编码问题".to_string(),
            format!(
                "```text\n检查文件：{}\n若文件可读性异常，修复后重新运行 lint。\n```",
                issue.path.as_deref().unwrap_or("index.md")
            ),
        ),
        "INDEX_MISSING" => (
            "补回 index.md".to_string(),
            "重新执行 init_vault 或补回 index.md".to_string(),
            "```text\n# Index\n\n## Imported Pages\n- [[wiki/xxx.md|xxx]]\n```".to_string(),
        ),
        "LOG_MISSING" => (
            "补回 log.md".to_string(),
            "重新执行 init_vault 或补回 log.md".to_string(),
            "```text\n# Log\n\n## 事件日志\n```".to_string(),
        ),
        "DB_SCHEMA_UPGRADE_FAILED" | "DB_MISSING" | "DB_QUERY_FAILED" => (
            "检查 meta.db".to_string(),
            "确认 SQLite 数据库可用并重试结构校验".to_string(),
            "```text\n确认 .app/meta.db 可读写，并检查数据库结构是否完整。\n```".to_string(),
        ),
        "CITATION_QUERY_FAILED" => (
            "检查 citations 查询".to_string(),
            "确认 citations 表可查询并修复数据库结构".to_string(),
            "```text\n检查 citations 表与相关索引是否存在。\n```".to_string(),
        ),
        "BROKEN_CITING_PAGE" => (
            "处理失效引用所属页面".to_string(),
            "恢复对应页面或移除失效引用记录".to_string(),
            format!(
                "```text\n引用所属页面不存在：{}\n建议恢复页面或清理引用记录。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "BROKEN_CITATION" => (
            "修复引用目标页面".to_string(),
            "补回被引用页面或修正引用路径".to_string(),
            format!(
                "```text\n引用目标缺失：{}\n建议修复引用路径或补回页面。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "broken_wikilink" | "BROKEN_WIKILINK" => (
            "修复失效 wiki-link".to_string(),
            "应用补丁可将失效 wiki-link 自动降级为纯文本，后续再补正确链接".to_string(),
            format!(
                "```text\n页面：{}\n将失效 [[wiki-link]] 转成可读纯文本，避免继续指向不存在页面。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "MISSING_INDEX_ENTRY" => (
            "补齐 index 引用".to_string(),
            "把缺失页面加入 index.md".to_string(),
            format!(
                "```text\n- [[{}|{}]]\n```",
                lint_patch_link_target(issue.path.as_deref()),
                lint_patch_link_label(issue.path.as_deref())
            ),
        ),
        "ORPHAN_WIKI_PAGE" | "orphan" => (
            "把页面挂回 index.md".to_string(),
            "将该页面加入 index.md，或确认其应保留为孤页".to_string(),
            format!(
                "```text\n- [[{}|{}]]\n```",
                lint_patch_link_target(issue.path.as_deref()),
                lint_patch_link_label(issue.path.as_deref())
            ),
        ),
        "xref_missing" | "XREF_MISSING" => (
            "补齐反向交叉引用".to_string(),
            "应用补丁会向目标页面追加 See Also 反向链接".to_string(),
            format!(
                "```text\n来源页面：{}\n为其已引用页面补充反向链接（See Also）。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "DB_MISSING_PAGE_RECORD" => (
            "同步 wiki_pages 记录".to_string(),
            "重新同步 wiki_pages 表记录".to_string(),
            format!(
                "```text\n补写 wiki_pages 记录以匹配页面：{}\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "STALE_PENDING_TASK" => (
            "推进卡住的任务".to_string(),
            "更新任务状态或清理陈旧任务".to_string(),
            format!(
                "```text\n任务路径：{}\n建议推进状态到 applied/failed，或清理过期任务。\n```",
                issue.path.as_deref().unwrap_or("未知路径")
            ),
        ),
        "TASK_QUERY_FAILED" => (
            "检查任务查询".to_string(),
            "确认 tasks 表可查询并修复数据库结构".to_string(),
            "```text\n检查 tasks 表与数据库可读性。\n```".to_string(),
        ),
        "STRICT_LOCAL_GATE" => (
            "严格本地模式提示".to_string(),
            "无需修改；仅确认当前运行在严格本地模式".to_string(),
            "```text\n该项为信息提示，无需应用补丁。\n```".to_string(),
        ),
        _ => (
            "检查问题".to_string(),
            "根据 lint 结果进行人工确认".to_string(),
            format!(
                "```text\n问题代码：{}\n路径：{}\n```",
                issue.code,
                issue.path.as_deref().unwrap_or("全局")
            ),
        ),
    };

    LintPatchSuggestion {
        issue_code: issue.code.clone(),
        path: issue.path.clone(),
        title,
        proposed_action,
        patch_preview,
    }
}

// ─────────────────────────────────────────────
// Private helpers — wiki graph / path helpers
// ─────────────────────────────────────────────

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

fn collect_wiki_link_graph(
    vault_path: &Path,
    wiki_page_paths: &BTreeSet<String>,
) -> (
    Vec<(String, String)>,
    BTreeMap<String, BTreeSet<String>>,
    BTreeMap<String, usize>,
) {
    let mut broken_links = Vec::new();
    let mut outbound_links = BTreeMap::new();
    let mut inbound_counts = wiki_page_paths
        .iter()
        .map(|path| (path.clone(), 0usize))
        .collect::<BTreeMap<_, _>>();

    for source_path in wiki_page_paths {
        let Ok(content) = fs::read_to_string(source_path) else {
            continue;
        };
        let mut existing_targets = BTreeSet::new();

        for raw_target in extract_wiki_link_targets(&content) {
            let Some(target_path) = resolve_wiki_link_target(vault_path, &raw_target) else {
                continue;
            };
            if Path::new(&target_path).exists() {
                if target_path != *source_path {
                    existing_targets.insert(target_path);
                }
            } else {
                broken_links.push((source_path.clone(), raw_target));
            }
        }

        if !existing_targets.is_empty() {
            for target in &existing_targets {
                *inbound_counts.entry(target.clone()).or_insert(0) += 1;
            }
            outbound_links.insert(source_path.clone(), existing_targets);
        }
    }

    (broken_links, outbound_links, inbound_counts)
}

fn collect_xref_missing_sources(
    outbound_links: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, Vec<String>> {
    let mut missing = BTreeMap::new();

    for (source, targets) in outbound_links {
        for target in targets {
            let has_reverse = outbound_links
                .get(target)
                .map(|reverse_targets| reverse_targets.contains(source))
                .unwrap_or(false);
            if !has_reverse {
                missing
                    .entry(source.clone())
                    .or_insert_with(Vec::new)
                    .push(target.clone());
            }
        }
    }

    missing
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

/// 将实体名或链接目标规范化为文件系统 slug（小写，空格替换为横线）。
fn entity_to_slug(entity: &str) -> String {
    entity.trim().to_lowercase().replace(' ', "-")
}

/// 将页面内容分离为 (frontmatter, body)。
/// frontmatter 为两个 `---` 之间的文本（不含分隔行），body 为之后的全部内容。
/// 若没有 frontmatter，返回 ("", content)。
fn split_frontmatter(content: &str) -> (&str, &str) {
    let bytes = content.as_bytes();
    // 页面必须以 "---" 开头才视为有 frontmatter
    if !content.starts_with("---") {
        return ("", content);
    }
    // 找到第一行结束位置（跳过开头 "---\n" 或 "---\r\n"）
    let after_first = if bytes.get(3) == Some(&b'\r') && bytes.get(4) == Some(&b'\n') {
        4
    } else if bytes.get(3) == Some(&b'\n') {
        3
    } else {
        return ("", content);
    };
    // 在剩余内容中寻找独立的 "---" 行作为 frontmatter 结束标记
    let rest = &content[after_first + 1..];
    // 搜索 "\n---" 或 "^---" 后跟换行/EOF
    let mut search_start = 0;
    loop {
        let Some(pos) = rest[search_start..].find("---") else {
            break;
        };
        let abs_pos = search_start + pos;
        // 确保 "---" 在行首
        let at_line_start = abs_pos == 0 || rest.as_bytes().get(abs_pos - 1) == Some(&b'\n');
        if !at_line_start {
            search_start = abs_pos + 3;
            continue;
        }
        // 确保 "---" 后是换行或 EOF
        let end_pos = abs_pos + 3;
        let followed_by_newline = matches!(
            rest.as_bytes().get(end_pos),
            None | Some(b'\n') | Some(b'\r')
        );
        if !followed_by_newline {
            search_start = end_pos;
            continue;
        }
        let fm = &rest[..abs_pos];
        // body 从结束标记行之后开始
        let body_start = if rest.as_bytes().get(end_pos) == Some(&b'\r')
            && rest.as_bytes().get(end_pos + 1) == Some(&b'\n')
        {
            end_pos + 2
        } else if rest.as_bytes().get(end_pos) == Some(&b'\n') {
            end_pos + 1
        } else {
            end_pos
        };
        let body = if body_start < rest.len() {
            &rest[body_start..]
        } else {
            ""
        };
        return (fm, body);
    }
    ("", content)
}

/// 从 frontmatter 文本中解析 `entities:` 列表（简单行解析，支持 YAML 列表格式）。
fn parse_frontmatter_entities(frontmatter: &str) -> Vec<String> {
    let mut entities = Vec::new();
    let mut in_entities = false;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("entities:") {
            in_entities = true;
            // 支持行内列表，如 `entities: [A, B]`（极少用，忽略）
            continue;
        }
        if in_entities {
            if trimmed.starts_with("- ") {
                let item = trimmed
                    .trim_start_matches("- ")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !item.is_empty() {
                    entities.push(item.to_string());
                }
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                // 遇到其他键则退出 entities 块
                in_entities = false;
            }
        }
    }
    entities
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

    Some(
        vault_path
            .join("wiki")
            .join(relative)
            .to_string_lossy()
            .to_string(),
    )
}

fn is_stale_pending_task(task: &db::PendingTaskRecord, checked_at_ms: u128) -> bool {
    let updated_at_ms = task
        .updated_at
        .parse::<u128>()
        .ok()
        .or_else(|| task.created_at.parse::<u128>().ok());

    // 以更新时间判断是否已经卡住。
    match updated_at_ms {
        Some(value) => {
            checked_at_ms.saturating_sub(value) > super::STALE_PENDING_TASK_THRESHOLD_MS
        }
        None => true,
    }
}

// ─────────────────────────────────────────────
// Private helpers — patch apply
// ─────────────────────────────────────────────

fn wiki_link_target_from_path(vault_path: &Path, page_path: &Path) -> Result<String, String> {
    let wiki_root = fs::canonicalize(vault_path.join("wiki"))
        .map_err(|err| format!("解析 wiki 根目录失败: {}", err))?;
    let canonical_page =
        fs::canonicalize(page_path).map_err(|err| format!("解析页面路径失败: {}", err))?;
    let relative = canonical_page
        .strip_prefix(&wiki_root)
        .map_err(|_| "页面不在 vault/wiki 目录下".to_string())?;

    Ok(format!(
        "wiki/{}",
        relative.to_string_lossy().replace('\\', "/")
    ))
}

fn wiki_link_label(page_path: &Path) -> String {
    page_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("xxx")
        .to_string()
}

fn append_index_link_if_missing(
    index_path: &Path,
    link_target: &str,
    label: &str,
) -> Result<bool, String> {
    let existing =
        fs::read_to_string(index_path).map_err(|err| format!("读取 index.md 失败: {}", err))?;
    let link = format!("[[{}|{}]]", link_target, label);
    if existing.contains(&link) {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&format!("- {}\n", link));
    fs::write(index_path, updated).map_err(|err| format!("写入 index.md 失败: {}", err))?;
    Ok(true)
}

fn rewrite_broken_wiki_links_in_page(vault_path: &Path, page_path: &Path) -> Result<usize, String> {
    let content =
        fs::read_to_string(page_path).map_err(|err| format!("读取页面失败: {}", err))?;
    let (updated, replaced) = rewrite_broken_wiki_links(&content, vault_path);
    if replaced > 0 {
        fs::write(page_path, updated).map_err(|err| format!("写入页面失败: {}", err))?;
    }
    Ok(replaced)
}

fn rewrite_broken_wiki_links(content: &str, vault_path: &Path) -> (String, usize) {
    let mut updated = String::with_capacity(content.len());
    let mut offset = 0usize;
    let mut replaced = 0usize;

    while let Some(start_rel) = content[offset..].find("[[") {
        let start = offset + start_rel;
        updated.push_str(&content[offset..start]);

        let inner_start = start + 2;
        let Some(end_rel) = content[inner_start..].find("]]") else {
            updated.push_str(&content[start..]);
            offset = content.len();
            break;
        };
        let inner_end = inner_start + end_rel;
        let original = &content[start..inner_end + 2];
        let inner = &content[inner_start..inner_end];

        let mut segments = inner.splitn(2, '|');
        let raw_target = segments.next().unwrap_or("").trim();
        let raw_label = segments
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let replacement = raw_label
            .map(|value| value.to_string())
            .unwrap_or_else(|| fallback_wiki_link_label(raw_target));

        let should_replace = resolve_wiki_link_target(vault_path, raw_target)
            .map(|target_path| !Path::new(&target_path).exists())
            .unwrap_or(false);

        if should_replace {
            updated.push_str(&replacement);
            replaced += 1;
        } else {
            updated.push_str(original);
        }

        offset = inner_end + 2;
    }

    if offset < content.len() {
        updated.push_str(&content[offset..]);
    }

    if replaced == 0 {
        (content.to_string(), 0)
    } else {
        (updated, replaced)
    }
}

fn fallback_wiki_link_label(raw_target: &str) -> String {
    let normalized = raw_target
        .split('#')
        .next()
        .unwrap_or(raw_target)
        .split('^')
        .next()
        .unwrap_or(raw_target)
        .trim();
    let stem = normalized
        .rsplit('/')
        .next()
        .unwrap_or(normalized)
        .trim_end_matches(".md")
        .trim();
    if stem.is_empty() {
        "未命名链接".to_string()
    } else {
        stem.to_string()
    }
}

fn apply_missing_xref_backlinks(
    vault_path: &Path,
    source_page: &Path,
) -> Result<(usize, Vec<String>), String> {
    let source_content =
        fs::read_to_string(source_page).map_err(|err| format!("读取页面失败: {}", err))?;
    let source_link_target = wiki_link_target_from_path(vault_path, source_page)?;
    let source_title = wiki_link_label(source_page);
    let source_canonical =
        fs::canonicalize(source_page).map_err(|err| format!("解析页面路径失败: {}", err))?;
    let source_canonical_str = source_canonical.to_string_lossy().to_string();

    let mut updated = 0usize;
    let mut touched_paths = vec![source_page.to_string_lossy().to_string()];
    let mut unique_targets = BTreeSet::new();

    for raw_target in extract_wiki_link_targets(&source_content) {
        let Some(target_path) = resolve_wiki_link_target(vault_path, &raw_target) else {
            continue;
        };
        if !Path::new(&target_path).exists() {
            continue;
        }
        unique_targets.insert(target_path);
    }

    for target_path in unique_targets {
        let target_canonical = fs::canonicalize(&target_path)
            .map_err(|err| format!("解析目标页面路径失败: {}", err))?;
        if target_canonical == source_canonical {
            continue;
        }

        let target_content = fs::read_to_string(&target_canonical)
            .map_err(|err| format!("读取页面失败: {}", err))?;
        let has_reverse = extract_wiki_link_targets(&target_content)
            .iter()
            .any(|raw| {
                resolve_wiki_link_target(vault_path, raw)
                    .and_then(|path| fs::canonicalize(path).ok())
                    .map(|path| path.to_string_lossy().to_string() == source_canonical_str)
                    .unwrap_or(false)
            });
        if has_reverse {
            continue;
        }

        let changed =
            vault::append_see_also_link(&target_canonical, &source_link_target, &source_title)
                .map_err(|err| format!("写入反向链接失败: {}", err))?;
        if changed {
            updated += 1;
            touched_paths.push(target_canonical.to_string_lossy().to_string());
        }
    }

    touched_paths.sort();
    touched_paths.dedup();
    Ok((updated, touched_paths))
}

fn seed_index_content() -> &'static str {
    "# Index\n\n## Imported Pages\n"
}

fn seed_log_content() -> &'static str {
    "# Log\n\n## Event Log\n"
}

// ─────────────────────────────────────────────
// Private helpers — outbox / event recording
// ─────────────────────────────────────────────

fn record_lint_patch_event(
    state: &AppState,
    vault_path: &Path,
    issue_code: &str,
    path: Option<&str>,
    applied: bool,
    message: &str,
) {
    let db_path = vault_path.join(".app").join("meta.db");
    let timestamp_ms = super::current_timestamp_ms();

    if let Err(err) =
        db::insert_lint_patch_event(&db_path, issue_code, path, applied, message, &timestamp_ms)
    {
        state.push_log(
            LogLevel::Warn,
            format!("写入 lint_patch_events 失败: {}", err),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::test_helpers::*;
    use crate::models::{
        AppMode, LintIssue, LintPatchApplyInput, LintPatchBatchApplyStatus, LintReport,
        LintSeverityStats,
    };
    use rusqlite::{params, Connection};
    use std::{collections::BTreeSet, fs, path::PathBuf};

    // ── AppState lint tests ───────────────────────────────────────────────────

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
    fn preview_lint_patches_returns_uninitialized_vault_suggestion() {
        let vault_dir = make_temp_dir("llm-wiki-lint-preview-uninit");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);

        let preview = state.preview_lint_patches();
        assert_eq!(preview.total, 1);
        assert_eq!(preview.suggestions.len(), 1);
        let suggestion = &preview.suggestions[0];
        assert_eq!(suggestion.issue_code, "VAULT_NOT_INITIALIZED");
        assert_eq!(suggestion.title, "初始化 Vault");
        assert!(suggestion.patch_preview.contains("init_vault"));
    }

    #[test]
    fn apply_lint_patch_supports_orphan_wiki_page_and_writes_log() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-orphan");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let orphan_path = vault_dir.join("wiki").join("orphan.md");
        let orphan_path_str = orphan_path.to_string_lossy().to_string();
        fs::write(&orphan_path, "# Orphan\n\n孤页内容。").expect("写入 orphan 页面失败");

        let result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "ORPHAN_WIKI_PAGE".to_string(),
                path: Some(orphan_path.to_string_lossy().to_string()),
            })
            .expect("应用 lint 补丁失败");

        assert!(result.applied);
        assert_eq!(result.issue_code, "ORPHAN_WIKI_PAGE");
        assert_eq!(result.path.as_deref(), Some(orphan_path_str.as_str()));
        assert!(result.touched_paths.iter().any(|path| path.ends_with("index.md")));

        let index_content =
            fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        assert!(index_content.contains("[[wiki/orphan.md|orphan]]"));

        let recent_log = state.recent_logs(1);
        assert_eq!(recent_log.len(), 1);
        assert!(recent_log[0].message.contains("ORPHAN_WIKI_PAGE"));
    }

    #[test]
    fn apply_lint_patch_supports_missing_index_entry_and_appends_link() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-missing-index");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_path = vault_dir.join("wiki").join("standalone.md");
        fs::write(&page_path, "# Standalone\n\n页面内容。").expect("写入页面失败");

        let result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "MISSING_INDEX_ENTRY".to_string(),
                path: Some(page_path.to_string_lossy().to_string()),
            })
            .expect("应用 lint 补丁失败");

        assert!(result.applied);
        assert_eq!(result.issue_code, "MISSING_INDEX_ENTRY");
        assert!(result.touched_paths.iter().any(|path| path.ends_with("index.md")));

        let index_content =
            fs::read_to_string(vault_dir.join("index.md")).expect("读取 index.md 失败");
        assert!(index_content.contains("[[wiki/standalone.md|standalone]]"));
    }

    #[test]
    fn apply_lint_patch_records_event_and_recent_query_returns_latest() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-event");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let orphan_path = vault_dir.join("wiki").join("event-note.md");
        let orphan_path_str = orphan_path.to_string_lossy().to_string();
        fs::write(&orphan_path, "# Event Note\n\n页面内容。").expect("写入页面失败");

        let result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "ORPHAN_WIKI_PAGE".to_string(),
                path: Some(orphan_path.to_string_lossy().to_string()),
            })
            .expect("应用 lint 补丁失败");
        assert!(result.applied);

        let events = state
            .recent_lint_patch_events(10)
            .expect("读取 lint 补丁事件失败");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].issue_code, "ORPHAN_WIKI_PAGE");
        assert_eq!(events[0].path.as_deref(), Some(orphan_path_str.as_str()));
        assert!(events[0].applied);
        assert!(events[0].message.contains("已将页面加入 index.md"));
        assert!(!events[0].created_at.is_empty());
    }

    #[test]
    fn apply_lint_patches_batch_summarizes_success_and_failure() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-batch");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let success_path = vault_dir.join("wiki").join("batch-note.md");
        fs::write(&success_path, "# Batch Note\n\n页面内容。").expect("写入页面失败");

        let result = state
            .apply_lint_patches_batch(vec![
                LintPatchApplyInput {
                    issue_code: "ORPHAN_WIKI_PAGE".to_string(),
                    path: Some(success_path.to_string_lossy().to_string()),
                },
                LintPatchApplyInput {
                    issue_code: "TASK_QUERY_FAILED".to_string(),
                    path: None,
                },
            ])
            .expect("批量应用 lint 补丁失败");

        assert_eq!(result.total, 2);
        assert_eq!(result.success, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.items.len(), 2);
        assert!(matches!(result.items[0].status, LintPatchBatchApplyStatus::Success));
        assert!(result.items[0].applied);
        assert!(matches!(result.items[1].status, LintPatchBatchApplyStatus::Failed));
        assert!(!result.items[1].applied);
        assert!(result.items[1].error.is_some());

        let recent_log = state.recent_logs(1);
        assert_eq!(recent_log.len(), 1);
        assert!(recent_log[0].message.contains("批量应用 Lint 补丁完成"));
        assert!(recent_log[0].message.contains("total=2"));
        assert!(recent_log[0].message.contains("success=1"));
        assert!(recent_log[0].message.contains("failed=1"));
        assert!(recent_log[0].message.contains("skipped=0"));
    }

    #[test]
    fn apply_lint_patch_rejects_unsupported_issue_code() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-unsupported");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let result = state.apply_lint_patch(LintPatchApplyInput {
            issue_code: "TASK_QUERY_FAILED".to_string(),
            path: None,
        });

        assert!(result.is_err());
        assert_eq!(result.err(), Some("暂不支持自动应用，请手动处理".to_string()));
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
            params![source_id, "present", present_path.to_string_lossy().to_string(), "present summary", "1", "1"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                present_path.to_string_lossy().to_string(),
                vault_dir.join("wiki").join("missing-cited.md").to_string_lossy().to_string(),
                3_i64, "missing citation", "1"
            ],
        )
        .expect("写入 citations 失败");

        let report = state.lint_report();
        let codes: BTreeSet<_> = report.issues.iter().map(|issue| issue.code.as_str()).collect();

        assert!(codes.contains("MISSING_INDEX_ENTRY"));
        assert!(codes.contains("orphan"));
        assert!(codes.contains("DB_MISSING_PAGE_RECORD"));
        assert!(codes.contains("BROKEN_CITATION"));
        assert!(!codes.contains("VAULT_NOT_INITIALIZED"));
        assert_eq!(report.severity_stats.error, 1);
        assert_eq!(report.severity_stats.warning, 3);
        assert_eq!(report.severity_stats.info, 0);
    }

    #[test]
    fn lint_report_detects_wikilink_level_broken_orphan_and_xref_missing() {
        let vault_dir = make_temp_dir("llm-wiki-lint-wikilink-level");
        let _guard = TempDirGuard(vault_dir.clone());

        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_a = vault_dir.join("wiki").join("a.md");
        let page_b = vault_dir.join("wiki").join("b.md");
        let page_orphan = vault_dir.join("wiki").join("orphan.md");

        fs::write(&page_a, "# A\n\n[[wiki/missing.md|missing]]\n[[wiki/b.md|B]]\n")
            .expect("写入 a.md 失败");
        fs::write(&page_b, "# B\n\n页面 B 内容。\n").expect("写入 b.md 失败");
        fs::write(&page_orphan, "# Orphan\n\n孤页内容。\n").expect("写入 orphan.md 失败");

        fs::write(
            vault_dir.join("index.md"),
            "# Index\n\n## Imported Pages\n- [[wiki/a.md|a]]\n- [[wiki/b.md|b]]\n",
        )
        .expect("写入 index.md 失败");

        let report = state.lint_report();
        let codes: BTreeSet<_> = report.issues.iter().map(|issue| issue.code.as_str()).collect();

        assert!(codes.contains("broken_wikilink"));
        assert!(codes.contains("xref_missing"));
        assert!(codes.contains("orphan"));
    }

    #[test]
    fn apply_lint_patch_supports_broken_wikilink_and_xref_missing() {
        let vault_dir = make_temp_dir("llm-wiki-lint-apply-wikilink-level");
        let _guard = TempDirGuard(vault_dir.clone());
        let state = make_test_state(&vault_dir);
        state.init_vault(vault_dir.clone()).expect("初始化 Vault 失败");

        let page_a = vault_dir.join("wiki").join("a.md");
        let page_b = vault_dir.join("wiki").join("b.md");
        fs::write(&page_a, "# A\n\n[[wiki/missing.md|缺失页]]\n[[wiki/b.md|B]]\n")
            .expect("写入 a.md 失败");
        fs::write(&page_b, "# B\n\n页面 B 内容。\n").expect("写入 b.md 失败");

        let broken_result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "broken_wikilink".to_string(),
                path: Some(page_a.to_string_lossy().to_string()),
            })
            .expect("应用 broken_wikilink 补丁失败");
        assert!(broken_result.applied);

        let page_a_content = fs::read_to_string(&page_a).expect("读取 a.md 失败");
        assert!(!page_a_content.contains("[[wiki/missing.md|缺失页]]"));
        assert!(page_a_content.contains("缺失页"));

        let xref_result = state
            .apply_lint_patch(LintPatchApplyInput {
                issue_code: "xref_missing".to_string(),
                path: Some(page_a.to_string_lossy().to_string()),
            })
            .expect("应用 xref_missing 补丁失败");
        assert!(xref_result.applied);

        let page_b_content = fs::read_to_string(&page_b).expect("读取 b.md 失败");
        assert!(page_b_content.contains("[[wiki/a.md|a]]"));
    }

    #[test]
    fn preview_lint_patches_total_matches_suggestions_for_multiple_issues() {
        let vault_dir = make_temp_dir("llm-wiki-lint-preview-multi");
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
                "hash-preview",
                vault_dir.join("source.md").to_string_lossy().to_string(),
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                "1"
            ],
        )
        .expect("写入 sources 失败");
        let source_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO wiki_pages (source_id, title, path, summary, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![source_id, "present", present_path.to_string_lossy().to_string(), "present summary", "1", "1"],
        )
        .expect("写入 wiki_pages 失败");
        conn.execute(
            "INSERT INTO citations (page_path, cited_page_path, score, excerpt, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                present_path.to_string_lossy().to_string(),
                vault_dir.join("wiki").join("missing-cited.md").to_string_lossy().to_string(),
                3_i64, "missing citation", "1"
            ],
        )
        .expect("写入 citations 失败");

        let report = state.lint_report();
        let preview = state.preview_lint_patches();

        assert_eq!(preview.total, preview.suggestions.len());
        assert_eq!(preview.total, report.issues.len());
        assert!(preview.suggestions.iter().any(|item| item.issue_code == "BROKEN_CITATION"));
        assert!(preview.suggestions.iter().any(|item| item.issue_code == "MISSING_INDEX_ENTRY"));
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
                source_id, "ingest_markdown", "queued",
                vault_dir.join("raw").join("source.md").to_string_lossy().to_string(),
                vault_dir.join("wiki").join("stale.md").to_string_lossy().to_string(),
                "1", "1"
            ],
        )
        .expect("写入 tasks 失败");

        let report = state.lint_report();

        assert!(report.issues.iter().any(|issue| issue.code == "STALE_PENDING_TASK"));
        assert_eq!(report.severity_stats.error, 0);
        assert_eq!(report.severity_stats.warning, 1);
        assert_eq!(report.severity_stats.info, 0);
    }

    // ── 语义 Lint 解析测试 ────────────────────────────────────────────────────

    #[test]
    fn parse_semantic_lint_response_parses_valid_lines() {
        let input = "SEMANTIC_CONTRADICTION|warning|page A 与 page B 矛盾|wiki/a.md|对齐两页内容\n\
                     SEMANTIC_STALE|info|结论可能已过时||更新至最新信息\n\
                     NO_ISSUES";
        let issues = parse_semantic_lint_response(input);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].code, "SEMANTIC_CONTRADICTION");
        assert_eq!(issues[0].severity, "warning");
        assert_eq!(issues[0].path, Some("wiki/a.md".to_string()));
        assert_eq!(issues[1].code, "SEMANTIC_STALE");
        assert_eq!(issues[1].severity, "info");
        assert_eq!(issues[1].path, None);
    }

    #[test]
    fn parse_semantic_lint_response_rejects_invalid_codes() {
        let input = "INVALID_CODE|warning|some message|wiki/a.md|fix it\n\
                     SEMANTIC_COVERAGE_GAP|info|缺少 Rust 语言页面||新建 wiki/rust.md";
        let issues = parse_semantic_lint_response(input);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "SEMANTIC_COVERAGE_GAP");
    }

    #[test]
    fn parse_semantic_lint_response_handles_no_issues() {
        let issues = parse_semantic_lint_response("NO_ISSUES");
        assert!(issues.is_empty());

        let issues2 = parse_semantic_lint_response("");
        assert!(issues2.is_empty());
    }

    #[test]
    fn parse_semantic_lint_response_caps_at_ten() {
        let line = "SEMANTIC_STALE|info|old conclusion||update it\n";
        let input = line.repeat(15);
        let issues = parse_semantic_lint_response(&input);
        assert_eq!(issues.len(), 10);
    }

    #[test]
    fn merge_lint_with_semantic_updates_stats_and_summary() {
        let rules = LintReport {
            mode: AppMode::Hybrid,
            checked_at: "0".to_string(),
            summary: "初始".to_string(),
            issues: vec![],
            severity_stats: LintSeverityStats {
                error: 0,
                warning: 1,
                info: 0,
            },
        };
        let semantic = vec![LintIssue {
            code: "SEMANTIC_STALE".to_string(),
            severity: "warning".to_string(),
            message: "过时".to_string(),
            path: None,
            suggestion: "更新".to_string(),
        }];
        let merged = merge_lint_with_semantic(rules, semantic);
        assert_eq!(merged.issues.len(), 1);
        assert_eq!(merged.severity_stats.warning, 2);
        assert!(merged.summary.contains("1 个问题"));
    }
}
