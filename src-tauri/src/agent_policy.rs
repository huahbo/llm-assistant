use crate::models::{ShellPolicyConfig, ShellPolicyDecision};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ── Write Ticket Cache (H6-P2) ────────────────────────────────────────────────

const TICKET_TTL: Duration = Duration::from_secs(300);

static TICKET_CACHE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn ticket_cache() -> &'static Mutex<HashMap<String, Instant>> {
    TICKET_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 检查某路径是否有有效的写操作审批票据（TTL 5 分钟）。
pub fn check_write_ticket(path: &str) -> bool {
    let cache = ticket_cache().lock().unwrap();
    cache.get(path).is_some_and(|&t| t.elapsed() < TICKET_TTL)
}

/// 为路径颁发写操作审批票据（TTL 5 分钟），同时清理过期条目。
pub fn grant_write_ticket(path: &str) {
    let mut cache = ticket_cache().lock().unwrap();
    cache.retain(|_, v| v.elapsed() < TICKET_TTL);
    cache.insert(path.to_string(), Instant::now());
}

/// Shell 命令最小策略分类（H6-S1.5/S2）。
/// 返回：(action, decision, block_reason)。
#[allow(dead_code)]
pub fn classify_shell_policy(
    command: &str,
    executor: &str,
) -> (&'static str, &'static str, Option<String>) {
    classify_shell_policy_with_config(command, executor, &ShellPolicyConfig::default())
}

/// Shell 命令策略分类（可配置版，P1：新增 network/script 两个维度）。
/// 分类优先级：destructive > script > network > read/write/unknown。
/// 返回：(action, decision, block_reason)。
pub fn classify_shell_policy_with_config(
    command: &str,
    executor: &str,
    config: &ShellPolicyConfig,
) -> (&'static str, &'static str, Option<String>) {
    const DESTRUCTIVE_PATTERNS: &[&str] = &[
        "rm -rf",
        "rm -r",
        "format ",
        "reg delete",
        "reg add",
        "shutdown",
        "del /f",
        "rmdir /s",
        "remove-item -recurse",
        "clear-recyclebin",
        "diskpart",
        "bcdedit",
        "mkfs",
        "dd if=",
        ":(){ :|:& };:",
        "stop-computer",
        "restart-computer",
    ];
    const NETWORK_COMMANDS: &[&str] = &[
        "curl",
        "wget",
        "invoke-webrequest",
        "iwr",
        "irm",
        "invoke-restmethod",
        "ping",
        "nslookup",
        "tracert",
        "traceroute",
        "netstat",
        "ssh",
        "scp",
        "sftp",
        "ftp",
        "telnet",
    ];
    const READ_COMMANDS: &[&str] = &[
        "ls", "dir", "pwd", "cd", "cat", "type", "echo", "whoami", "hostname", "date", "get-date",
        "rg", "grep", "find", "git", "cargo", "npm",
    ];
    const WRITE_COMMANDS: &[&str] = &[
        "cp",
        "copy",
        "mv",
        "move",
        "mkdir",
        "md",
        "touch",
        "ni",
        "new-item",
        "set-content",
        "add-content",
        "sc",
        "ac",
        "write-output",
    ];

    let lower = command.to_lowercase();

    // 1. destructive — 永远 deny，不可通过 profile 降级
    for pat in DESTRUCTIVE_PATTERNS {
        if lower.contains(pat) {
            return (
                "destructive",
                "deny",
                Some(format!("策略拒绝：命令包含高危模式 `{pat}`")),
            );
        }
    }

    // 2. script — .ps1/.bat/.cmd/.sh 或 powershell -file
    if is_script_command(&lower) {
        return decision_tuple("script", config.script_decision, "策略命中：脚本类命令");
    }

    // 3. network — curl/wget/Invoke-WebRequest 等
    let first = lower.split_whitespace().next().unwrap_or("");
    if NETWORK_COMMANDS.contains(&first) {
        return decision_tuple("network", config.network_decision, "策略命中：网络类命令");
    }

    // 4. read/write/unknown — 按来源执行器决策
    let action = if READ_COMMANDS.contains(&first) {
        "read"
    } else if WRITE_COMMANDS.contains(&first) {
        "write"
    } else {
        "unknown"
    };

    match (executor, action) {
        ("agent", "read")    => decision_tuple(action, config.agent_read_decision,    "策略命中：agent 来源只读命令"),
        ("agent", "write")   => decision_tuple(action, config.agent_write_decision,   "策略命中：agent 来源写入命令"),
        ("agent", _)         => decision_tuple(action, config.agent_unknown_decision,  "策略命中：agent 来源未知命令"),
        ("manual", "write")   => decision_tuple(action, config.manual_write_decision,  "策略命中：manual 来源写入命令"),
        ("manual", "unknown") => decision_tuple(action, config.manual_unknown_decision, "策略命中：manual 来源未知命令"),
        _ => (action, "auto_allow", None),
    }
}

fn is_script_command(lower: &str) -> bool {
    let mut tokens = lower.split_whitespace();
    let first = tokens.next().unwrap_or("");
    let stripped = first.trim_start_matches("./").trim_start_matches(".\\");
    for ext in &[".ps1", ".bat", ".cmd", ".sh"] {
        if stripped.ends_with(ext) {
            return true;
        }
    }
    // `bash script.sh` / `sh script.sh`
    if first == "bash" || first == "sh" {
        if let Some(arg) = tokens.next() {
            if arg.ends_with(".sh") || arg.ends_with(".bash") {
                return true;
            }
        }
    }
    // `powershell -File script.ps1` — only flag if the next token after -file is the script
    if let Some(file_idx) = lower.find("-file ") {
        let after = lower[file_idx + 6..].trim_start();
        let filename = after.split_whitespace().next().unwrap_or("");
        if filename.ends_with(".ps1") || filename.ends_with(".bat") {
            return true;
        }
    }
    false
}

fn decision_tuple(
    action: &'static str,
    decision: ShellPolicyDecision,
    reason_prefix: &str,
) -> (&'static str, &'static str, Option<String>) {
    match decision {
        ShellPolicyDecision::AutoAllow => (action, "auto_allow", None),
        ShellPolicyDecision::RequireApproval => (
            action,
            "require_approval",
            Some(format!("{reason_prefix}，需人工确认")),
        ),
        ShellPolicyDecision::Deny => (
            action,
            "deny",
            Some(format!("{reason_prefix}，策略拒绝执行")),
        ),
    }
}

/// PathGuard（最小版）：限制 agent 的 read_wiki 只能读取当前 vault/wiki 下的 markdown 文件。
pub fn validate_agent_read_path(vault_path: &Path, target_path: &Path) -> Result<(), String> {
    let wiki_root = vault_path.join("wiki");
    let wiki_root = wiki_root
        .canonicalize()
        .map_err(|err| format!("PathGuard 失败：无法解析 wiki 根目录: {}", err))?;
    let target = target_path
        .canonicalize()
        .map_err(|err| format!("PathGuard 失败：无法解析目标路径: {}", err))?;

    if !target.starts_with(&wiki_root) {
        return Err("PathGuard 拒绝：仅允许读取 vault/wiki 目录".to_string());
    }
    if target.extension().and_then(|v| v.to_str()) != Some("md") {
        return Err("PathGuard 拒绝：仅允许读取 Markdown 页面".to_string());
    }
    Ok(())
}

/// PathGuard（最小版）：限制 agent 的写路径只能位于 vault/wiki 下且扩展名为 .md。
/// 与 read 校验不同，write 目标文件可能尚不存在，因此按“父目录 canonical + 文件扩展名”校验。
pub fn validate_agent_write_path(vault_path: &Path, target_path: &Path) -> Result<(), String> {
    let wiki_root = vault_path.join("wiki");
    let wiki_root = wiki_root
        .canonicalize()
        .map_err(|err| format!("PathGuard 失败：无法解析 wiki 根目录: {}", err))?;

    let absolute_target = if target_path.is_absolute() {
        target_path.to_path_buf()
    } else {
        vault_path.join(target_path)
    };
    let parent = absolute_target
        .parent()
        .ok_or_else(|| "PathGuard 失败：目标路径缺少父目录".to_string())?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|err| format!("PathGuard 失败：无法解析目标父目录: {}", err))?;

    if !canonical_parent.starts_with(&wiki_root) {
        return Err("PathGuard 拒绝：仅允许写入 vault/wiki 目录".to_string());
    }
    if absolute_target.extension().and_then(|v| v.to_str()) != Some("md") {
        return Err("PathGuard 拒绝：仅允许写入 Markdown 页面".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        classify_shell_policy, classify_shell_policy_with_config, validate_agent_read_path,
        validate_agent_write_path,
    };
    use crate::models::{ShellPolicyConfig, ShellPolicyDecision};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

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
        let dir =
            std::env::temp_dir().join(format!("{}-{}-{}", prefix, std::process::id(), unique));
        fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    #[test]
    fn destructive_is_denied() {
        let (action, decision, reason) = classify_shell_policy("rm -rf /tmp/x", "manual");
        assert_eq!(action, "destructive");
        assert_eq!(decision, "deny");
        assert!(reason.is_some());
    }

    #[test]
    fn agent_unknown_requires_approval() {
        let (action, decision, reason) =
            classify_shell_policy("powershell -Command FooBar", "agent");
        assert_eq!(action, "unknown");
        assert_eq!(decision, "require_approval");
        assert!(reason.is_some());
    }

    #[test]
    fn manual_read_auto_allow() {
        let (action, decision, reason) = classify_shell_policy("Get-Date", "manual");
        assert_eq!(action, "read");
        assert_eq!(decision, "auto_allow");
        assert!(reason.is_none());
    }

    #[test]
    fn manual_unknown_can_be_forced_to_approval() {
        let cfg = ShellPolicyConfig {
            manual_unknown_decision: ShellPolicyDecision::RequireApproval,
            ..ShellPolicyConfig::default()
        };
        let (action, decision, reason) =
            classify_shell_policy_with_config("foo-bar-custom", "manual", &cfg);
        assert_eq!(action, "unknown");
        assert_eq!(decision, "require_approval");
        assert!(reason.is_some());
    }

    #[test]
    fn agent_write_can_be_forced_to_deny() {
        let cfg = ShellPolicyConfig {
            agent_write_decision: ShellPolicyDecision::Deny,
            ..ShellPolicyConfig::default()
        };
        let (action, decision, reason) =
            classify_shell_policy_with_config("mkdir test-dir", "agent", &cfg);
        assert_eq!(action, "write");
        assert_eq!(decision, "deny");
        assert!(reason.is_some());
    }

    #[test]
    fn manual_write_can_be_forced_to_approval() {
        let cfg = ShellPolicyConfig {
            manual_write_decision: ShellPolicyDecision::RequireApproval,
            ..ShellPolicyConfig::default()
        };
        let (action, decision, reason) =
            classify_shell_policy_with_config("mkdir test-dir", "manual", &cfg);
        assert_eq!(action, "write");
        assert_eq!(decision, "require_approval");
        assert!(reason.is_some());
    }

    #[test]
    fn agent_read_can_be_forced_to_approval() {
        let cfg = ShellPolicyConfig {
            agent_read_decision: ShellPolicyDecision::RequireApproval,
            ..ShellPolicyConfig::default()
        };
        let (action, decision, reason) =
            classify_shell_policy_with_config("Get-Date", "agent", &cfg);
        assert_eq!(action, "read");
        assert_eq!(decision, "require_approval");
        assert!(reason.is_some());
    }

    #[test]
    fn pathguard_allows_markdown_in_wiki() {
        let vault = make_temp_dir("llm-wiki-policy-allow");
        let _guard = TempDirGuard(vault.clone());
        let wiki = vault.join("wiki");
        fs::create_dir_all(&wiki).expect("创建 wiki 目录失败");
        let page = wiki.join("a.md");
        fs::write(&page, "# a").expect("写入文件失败");

        let result = validate_agent_read_path(&vault, &page);
        assert!(result.is_ok(), "wiki 内 md 应允许");
    }

    #[test]
    fn pathguard_rejects_outside_wiki() {
        let vault = make_temp_dir("llm-wiki-policy-deny");
        let _guard = TempDirGuard(vault.clone());
        let wiki = vault.join("wiki");
        fs::create_dir_all(&wiki).expect("创建 wiki 目录失败");
        let outsider = vault.join("index.md");
        fs::write(&outsider, "# root").expect("写入文件失败");

        let result = validate_agent_read_path(&vault, &outsider);
        assert!(result.is_err(), "wiki 外文件应拒绝");
    }

    #[test]
    fn pathguard_write_allows_markdown_under_wiki() {
        let vault = make_temp_dir("llm-wiki-policy-write-allow");
        let _guard = TempDirGuard(vault.clone());
        let wiki = vault.join("wiki");
        fs::create_dir_all(&wiki).expect("创建 wiki 目录失败");

        let target = wiki.join("new-page.md");
        let result = validate_agent_write_path(&vault, &target);
        assert!(result.is_ok(), "wiki 内 md 写路径应允许");
    }

    #[test]
    fn pathguard_write_rejects_non_markdown() {
        let vault = make_temp_dir("llm-wiki-policy-write-deny-ext");
        let _guard = TempDirGuard(vault.clone());
        let wiki = vault.join("wiki");
        fs::create_dir_all(&wiki).expect("创建 wiki 目录失败");

        let target = wiki.join("notes.txt");
        let result = validate_agent_write_path(&vault, &target);
        assert!(result.is_err(), "非 md 写路径应拒绝");
    }

    // ── P1：network / script 分类测试 ─────────────────────────────────────

    #[test]
    fn network_command_uses_network_decision() {
        let cfg = ShellPolicyConfig {
            network_decision: ShellPolicyDecision::RequireApproval,
            ..ShellPolicyConfig::default()
        };
        for cmd in &["curl https://example.com", "wget http://x.com", "iwr http://x.com", "ping 8.8.8.8"] {
            let (action, decision, reason) = classify_shell_policy_with_config(cmd, "manual", &cfg);
            assert_eq!(action, "network", "cmd={cmd}");
            assert_eq!(decision, "require_approval", "cmd={cmd}");
            assert!(reason.is_some(), "cmd={cmd}");
        }
    }

    #[test]
    fn network_command_auto_allow_on_power_user_profile() {
        use crate::models::ShellPolicyProfile;
        let cfg = ShellPolicyConfig::from_profile(ShellPolicyProfile::PowerUser);
        let (action, decision, _) = classify_shell_policy_with_config("curl https://example.com", "manual", &cfg);
        assert_eq!(action, "network");
        assert_eq!(decision, "auto_allow");
    }

    #[test]
    fn network_command_deny_on_strict_profile() {
        use crate::models::ShellPolicyProfile;
        let cfg = ShellPolicyConfig::from_profile(ShellPolicyProfile::Strict);
        let (action, decision, reason) = classify_shell_policy_with_config("wget http://x.com", "agent", &cfg);
        assert_eq!(action, "network");
        assert_eq!(decision, "deny");
        assert!(reason.is_some());
    }

    #[test]
    fn script_command_ps1_uses_script_decision() {
        let cfg = ShellPolicyConfig {
            script_decision: ShellPolicyDecision::RequireApproval,
            ..ShellPolicyConfig::default()
        };
        let (action, decision, reason) = classify_shell_policy_with_config(".\\deploy.ps1", "manual", &cfg);
        assert_eq!(action, "script");
        assert_eq!(decision, "require_approval");
        assert!(reason.is_some());
    }

    #[test]
    fn script_command_bat_uses_script_decision() {
        let cfg = ShellPolicyConfig::default();
        let (action, decision, _) = classify_shell_policy_with_config("./build.bat", "agent", &cfg);
        assert_eq!(action, "script");
        assert_eq!(decision, "require_approval");
    }

    #[test]
    fn script_command_powershell_file_uses_script_decision() {
        let cfg = ShellPolicyConfig {
            script_decision: ShellPolicyDecision::Deny,
            ..ShellPolicyConfig::default()
        };
        let (action, decision, _) = classify_shell_policy_with_config("powershell -File myscript.ps1", "manual", &cfg);
        assert_eq!(action, "script");
        assert_eq!(decision, "deny");
    }

    #[test]
    fn destructive_always_denied_regardless_of_profile() {
        use crate::models::ShellPolicyProfile;
        let cfg = ShellPolicyConfig::from_profile(ShellPolicyProfile::PowerUser);
        let (action, decision, _) = classify_shell_policy_with_config("rm -rf /tmp/x", "manual", &cfg);
        assert_eq!(action, "destructive");
        assert_eq!(decision, "deny");
    }

    #[test]
    fn profile_strict_denies_agent_write() {
        use crate::models::ShellPolicyProfile;
        let cfg = ShellPolicyConfig::from_profile(ShellPolicyProfile::Strict);
        let (action, decision, _) = classify_shell_policy_with_config("mkdir new-dir", "agent", &cfg);
        assert_eq!(action, "write");
        assert_eq!(decision, "deny");
    }

    #[test]
    fn profile_power_user_allows_agent_write() {
        use crate::models::ShellPolicyProfile;
        let cfg = ShellPolicyConfig::from_profile(ShellPolicyProfile::PowerUser);
        let (action, decision, _) = classify_shell_policy_with_config("mkdir new-dir", "agent", &cfg);
        assert_eq!(action, "write");
        assert_eq!(decision, "auto_allow");
    }
}
