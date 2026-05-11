import { useState } from "react";
import type { MutableRefObject } from "react";
import { type ShellPolicyProfileKey, getActiveProfileKey } from "../../contexts/ShellPolicyContext";
import type { ShellPolicyConfig, ShellHistoryEntry, ShellSessionInfo, ShellAuditEvent } from "../../types";
import { listShellAuditEvents } from "../../tauri-client";

type ShellPolicyProfileItem = {
  key: ShellPolicyProfileKey;
  label: string;
};

type AgentShellQuickCommandItem = {
  label: string;
  command: string;
  run: boolean;
};

type AgentToolsPaneProps = {
  agentShellTheme: "deep" | "light";
  setAgentShellTheme: (updater: (prev: "deep" | "light") => "deep" | "light") => void;
  agentShellSession: ShellSessionInfo | null;
  agentShellRunning: boolean;
  agentShellHistory: ShellHistoryEntry[];
  handleClearShellHistory: () => void;
  shellPolicyProfiles: ShellPolicyProfileItem[];
  agentShellPolicySaving: boolean;
  agentShellPolicyConfig: ShellPolicyConfig | null;
  handleApplyAndSaveShellPolicyProfile: (profileKey: ShellPolicyProfileKey) => Promise<void>;
  agentShellQuickCommands: readonly AgentShellQuickCommandItem[];
  handleApplyShellCommand: (command: string, runImmediately?: boolean) => void;
  agentShellHistoryRef: MutableRefObject<HTMLDivElement | null>;
  agentShellAutoFollowRef: MutableRefObject<boolean>;
  handleCopyShellOutput: (entry: ShellHistoryEntry) => Promise<void>;
  agentShellInputRef: MutableRefObject<HTMLInputElement | null>;
  agentShellCmd: string;
  setAgentShellCmd: (value: string) => void;
  setAgentShellHistoryCursor: (value: number) => void;
  handleShellHistoryNav: (direction: "prev" | "next") => void;
  handleRunShell: () => Promise<void>;
  handleApproveShellCommand: (command: string) => Promise<void>;
};

export default function AgentToolsPane({
  agentShellTheme,
  setAgentShellTheme,
  agentShellSession,
  agentShellRunning,
  agentShellHistory,
  handleClearShellHistory,
  shellPolicyProfiles,
  agentShellPolicySaving,
  agentShellPolicyConfig,
  handleApplyAndSaveShellPolicyProfile,
  agentShellQuickCommands,
  handleApplyShellCommand,
  agentShellHistoryRef,
  agentShellAutoFollowRef,
  handleCopyShellOutput,
  agentShellInputRef,
  agentShellCmd,
  setAgentShellCmd,
  setAgentShellHistoryCursor,
  handleShellHistoryNav,
  handleRunShell,
  handleApproveShellCommand,
}: AgentToolsPaneProps) {
  const activeProfileKey = getActiveProfileKey(agentShellPolicyConfig);
  const [showAudit, setShowAudit] = useState(false);
  const [auditLog, setAuditLog] = useState<ShellAuditEvent[]>([]);
  const [auditLoading, setAuditLoading] = useState(false);

  const handleToggleAudit = async () => {
    if (!showAudit) {
      setAuditLoading(true);
      try {
        const events = await listShellAuditEvents(20);
        setAuditLog(events);
      } finally {
        setAuditLoading(false);
      }
    }
    setShowAudit((v) => !v);
  };
  return (
    <section className={`agent-studio__tools-workspace agent-studio__tools-workspace--${agentShellTheme}`}>
      <div className="agent-studio__tools-head">
        <div className="agent-studio__tools-session">
          <span>会话：{agentShellSession?.session_id ? agentShellSession.session_id.slice(-8) : "未就绪"}</span>
          <span>目录：{agentShellSession?.working_dir || "—"}</span>
        </div>
        <div className="agent-studio__tools-actions">
          <button
            type="button"
            className="dev-panel__button"
            onClick={handleClearShellHistory}
            disabled={agentShellRunning || agentShellHistory.length === 0}
          >
            清空历史
          </button>
          <button
            type="button"
            className="dev-panel__button"
            onClick={() => setAgentShellTheme((prev) => (prev === "deep" ? "light" : "deep"))}
          >
            {agentShellTheme === "deep" ? "浅色" : "深色"}
          </button>
        </div>
      </div>
      <p className="agent-studio__shell-hint">
        任务模式中 Agent 会自动调用工具；此处为会话式终端，支持连续命令与目录上下文。
      </p>
      <div className="agent-studio__shell-policy agent-studio__shell-policy--compact">
        <div className="agent-studio__shell-policy-head">
          <strong>Shell 策略</strong>
          <div className="agent-studio__shell-policy-presets">
            <span>档位：</span>
            {shellPolicyProfiles.map((profile) => (
              <button
                key={profile.key}
                type="button"
                className={`agent-studio__shell-policy-preset${activeProfileKey === profile.key ? " agent-studio__shell-policy-preset--active" : ""}`}
                disabled={agentShellPolicySaving || !agentShellPolicyConfig}
                onClick={() => {
                  void handleApplyAndSaveShellPolicyProfile(profile.key);
                }}
              >
                {profile.label}
              </button>
            ))}
          </div>
        </div>
        <p className="agent-studio__shell-policy-tip">
          详细策略请到设置页修改；此处仅快速切换档位。
        </p>
      </div>
      <div className="agent-studio__shell-quick">
        {agentShellQuickCommands.map((item) => (
          <button
            key={item.label}
            type="button"
            className="agent-studio__shell-quick-btn"
            disabled={agentShellRunning || !agentShellSession}
            onClick={() => handleApplyShellCommand(item.command, item.run)}
          >
            {item.label}
          </button>
        ))}
      </div>
      <div
        className="agent-studio__shell-history"
        ref={agentShellHistoryRef}
        onScroll={(event) => {
          const el = event.currentTarget;
          const gap = el.scrollHeight - el.scrollTop - el.clientHeight;
          agentShellAutoFollowRef.current = gap < 56;
        }}
      >
        {agentShellHistory.length === 0 ? (
          <p className="agent-studio__shell-empty">暂无执行历史，输入命令后可在此查看结果。</p>
        ) : (
          agentShellHistory.map((e) => (
            <div
              key={e.id}
              className={`agent-studio__shell-entry ${e.result.blocked ? "blocked" : e.result.exit_code === 0 ? "ok" : "err"}`}
            >
              <div className="agent-studio__shell-entry-head">
                <div className="agent-studio__shell-prompt">❯ {e.command}</div>
                <div className="agent-studio__shell-entry-actions">
                  <span className={`agent-studio__shell-status-badge ${e.running ? "running" : e.result.exit_code === 0 ? "ok" : "err"}`}>
                    {e.running ? "运行中" : `exit ${e.result.exit_code}`}
                  </span>
                  <button
                    type="button"
                    className="agent-studio__shell-copy-btn"
                    onClick={() => {
                      void handleCopyShellOutput(e);
                    }}
                  >
                    复制
                  </button>
                </div>
              </div>
              {e.result.blocked ? (
                <div className="agent-studio__shell-blocked">
                  <span>⛔ {e.result.blocked_reason}</span>
                  {e.result.policy_decision === "require_approval" && (
                    <button
                      type="button"
                      className="agent-studio__shell-approve-btn"
                      disabled={agentShellRunning}
                      onClick={() => { void handleApproveShellCommand(e.command); }}
                    >
                      审批执行
                    </button>
                  )}
                </div>
              ) : (
                <pre className="agent-studio__shell-output">
                  {(e.running
                    ? `${e.live_stdout || ""}${e.live_stderr || ""}`.trim() || "执行中..."
                    : (e.result.stdout || e.result.stderr || `(exit ${e.result.exit_code})`))
                  }
                </pre>
              )}
              <div className="agent-studio__shell-meta">
                {e.result.executor} · {e.result.policy_action} · {e.result.policy_decision}
                {e.result.working_dir ? ` · cwd=${e.result.working_dir}` : ""}
                {e.running ? " · streaming..." : ""}
              </div>
            </div>
          ))
        )}
      </div>
      <div className="agent-studio__shell-input-wrap">
        <div className="agent-studio__shell-input-row">
          <input
            ref={agentShellInputRef}
            type="text"
            className="agent-studio__shell-input"
            placeholder="PowerShell 命令（Enter 执行，↑↓ 浏览历史）"
            value={agentShellCmd}
            onChange={(ev) => {
              setAgentShellCmd(ev.target.value);
              setAgentShellHistoryCursor(-1);
            }}
            onKeyDown={(ev) => {
              if (ev.key === "ArrowUp") {
                ev.preventDefault();
                handleShellHistoryNav("prev");
                return;
              }
              if (ev.key === "ArrowDown") {
                ev.preventDefault();
                handleShellHistoryNav("next");
                return;
              }
              if (ev.key === "Enter" && !agentShellRunning) {
                ev.preventDefault();
                void handleRunShell();
              }
            }}
            disabled={agentShellRunning || !agentShellSession}
          />
          <button
            className="agent-studio__shell-run-btn"
            disabled={!agentShellCmd.trim() || agentShellRunning || !agentShellSession}
            onMouseDown={(ev) => {
              ev.preventDefault();
            }}
            onClick={() => void handleRunShell()}
          >
            {agentShellRunning ? "执行中" : "运行"}
          </button>
        </div>
        <p className="agent-studio__shell-input-tip">支持会话命令（例如 `cd ..`）和流式输出；工具调用与任务模式隔离。</p>
      </div>
      <div className="agent-studio__audit">
        <button
          type="button"
          className="agent-studio__audit-toggle"
          onClick={() => { void handleToggleAudit(); }}
        >
          {showAudit ? "▲ 收起审计日志" : "▼ 查看审计日志"}
          {auditLoading && " …"}
        </button>
        {showAudit && (
          <div className="agent-studio__audit-panel">
            {auditLog.length === 0 ? (
              <p className="agent-studio__audit-empty">暂无审计记录</p>
            ) : (
              <table className="agent-studio__audit-table">
                <thead>
                  <tr>
                    <th>命令</th>
                    <th>来源</th>
                    <th>决策</th>
                    <th>退出码</th>
                    <th>耗时</th>
                  </tr>
                </thead>
                <tbody>
                  {auditLog.map((e) => (
                    <tr key={e.id} className={e.blocked ? "blocked" : ""}>
                      <td title={e.command}>{e.command.length > 40 ? `${e.command.slice(0, 40)}…` : e.command}</td>
                      <td>{e.executor}</td>
                      <td>{e.policy_decision}</td>
                      <td>{e.exit_code ?? "—"}</td>
                      <td>{e.latency_ms != null ? `${e.latency_ms}ms` : "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
