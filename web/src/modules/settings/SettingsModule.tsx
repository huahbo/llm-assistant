import SearchConfigPanel from "../lint/SearchConfigPanel";
import {
  shellPolicyDecisionOptions,
  shellPolicyProfiles,
  useShellPolicy,
} from "../../contexts/ShellPolicyContext";
import { isTauriRuntime } from "../../tauri-client";
import type { LlmProviderConfig, ShellPolicyDecision } from "../../types";

type DropMode = "direct" | "queue";

type SettingsModuleProps = {
  llmConfig: LlmProviderConfig | null;
  defaultCloudModel: string;
  defaultCloudProviderName: string;
  defaultCloudBaseUrl: string;
  selectedPreset: string;
  llmPresets: [string, string, string][];
  onPresetChange: (presetName: string) => void;
  llmConfigActiveProvider: "cloud" | "ollama";
  setLlmConfigActiveProvider: (provider: "cloud" | "ollama") => void;
  llmConfigCloudProviderName: string;
  setLlmConfigCloudProviderName: (value: string) => void;
  llmConfigCloudApiKey: string;
  setLlmConfigCloudApiKey: (value: string) => void;
  llmConfigCloudBaseUrl: string;
  setLlmConfigCloudBaseUrl: (value: string) => void;
  llmConfigCloudModel: string;
  setLlmConfigCloudModel: (value: string) => void;
  llmConfigEmbedModel: string;
  setLlmConfigEmbedModel: (value: string) => void;
  llmConfigEmbedBaseUrl: string;
  setLlmConfigEmbedBaseUrl: (value: string) => void;
  llmConfigSaving: boolean;
  onSaveLlmConfig: () => void | Promise<void>;
  dropMode: DropMode;
  onDropModeChange: (mode: DropMode) => void;
};

export default function SettingsModule({
  llmConfig,
  defaultCloudModel,
  defaultCloudProviderName,
  defaultCloudBaseUrl,
  selectedPreset,
  llmPresets,
  onPresetChange,
  llmConfigActiveProvider,
  setLlmConfigActiveProvider,
  llmConfigCloudProviderName,
  setLlmConfigCloudProviderName,
  llmConfigCloudApiKey,
  setLlmConfigCloudApiKey,
  llmConfigCloudBaseUrl,
  setLlmConfigCloudBaseUrl,
  llmConfigCloudModel,
  setLlmConfigCloudModel,
  llmConfigEmbedModel,
  setLlmConfigEmbedModel,
  llmConfigEmbedBaseUrl,
  setLlmConfigEmbedBaseUrl,
  llmConfigSaving,
  onSaveLlmConfig,
  dropMode,
  onDropModeChange,
}: SettingsModuleProps) {
  const {
    config: agentShellPolicyConfig,
    saving: agentShellPolicySaving,
    dirty: agentShellPolicyDirty,
    reload: handleReloadShellPolicy,
    save: handleSaveShellPolicy,
    applyProfile: applyShellPolicyProfile,
    setField: handleChangeShellPolicyDecision,
  } = useShellPolicy();

  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">设置</h1>
        <p className="module-header__sub">Provider 配置与运行策略</p>
      </div>
      <section className="panel">
        <div className="section-head">
          <h2>LLM Provider 配置</h2>
          <span className="section-head__hint">
            {isTauriRuntime() ? "本地配置文件" : "浏览器预览"}
          </span>
        </div>
        <div className="settings-panel">
          <p className="dev-panel__hint settings-panel__status">
            当前活跃 Provider：
            <strong>
              {llmConfig
                ? llmConfig.active_provider === "cloud"
                  ? `${llmConfig.cloud_provider_name || "云端 Provider"}（${llmConfig.cloud_model || defaultCloudModel}）`
                  : "本地 Ollama"
                : "加载中..."}
            </strong>
          </p>
          <div className="settings-panel__presets">
            <label className="dev-panel__label" htmlFor="preset-select">Provider 预设</label>
            <select
              id="preset-select"
              className="dev-panel__input"
              value={selectedPreset}
              onChange={(event) => onPresetChange(event.target.value)}
            >
              <option value="Custom">Custom (自定义)</option>
              {llmPresets.map(([name]) => (
                <option key={name} value={name}>{name}</option>
              ))}
            </select>
          </div>
          <div className="settings-panel__fields">
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="active-provider">活跃 Provider</label>
              <select
                id="active-provider"
                className="dev-panel__input"
                value={llmConfigActiveProvider}
                onChange={(event) =>
                  setLlmConfigActiveProvider(event.target.value === "cloud" ? "cloud" : "ollama")
                }
              >
                <option value="ollama">ollama（本地）</option>
                <option value="cloud">cloud（云端）</option>
              </select>
            </div>
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="cloud-provider-name">云端 Provider 名称</label>
              <input
                id="cloud-provider-name"
                className="dev-panel__input"
                type="text"
                value={llmConfigCloudProviderName}
                onChange={(event) => setLlmConfigCloudProviderName(event.target.value)}
                placeholder={`${defaultCloudProviderName}（可改为 OpenAI / DeepSeek / GLM / MiniMax）`}
                spellCheck={false}
              />
            </div>
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="cloud-api-key">云端 API Key（OpenAI-compatible）</label>
              <input
                id="cloud-api-key"
                className="dev-panel__input"
                type="password"
                value={llmConfigCloudApiKey}
                onChange={(event) => setLlmConfigCloudApiKey(event.target.value)}
                placeholder="sk-...（选择 cloud 时必填）"
                spellCheck={false}
                autoComplete="off"
              />
            </div>
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="cloud-base-url">云端 Base URL</label>
              <input
                id="cloud-base-url"
                className="dev-panel__input"
                type="text"
                value={llmConfigCloudBaseUrl}
                onChange={(event) => setLlmConfigCloudBaseUrl(event.target.value)}
                placeholder={defaultCloudBaseUrl}
                spellCheck={false}
              />
            </div>
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="cloud-model">云端模型名</label>
              <input
                id="cloud-model"
                className="dev-panel__input"
                type="text"
                value={llmConfigCloudModel}
                onChange={(event) => setLlmConfigCloudModel(event.target.value)}
                placeholder={defaultCloudModel}
                spellCheck={false}
              />
            </div>
          </div>
          <div className="settings-panel__section-title">本地 Ollama（Embedding 专用）</div>
          <div className="settings-panel__fields">
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="embed-ollama-model">Embedding 模型（本地 Ollama）</label>
              <input
                id="embed-ollama-model"
                className="dev-panel__input"
                type="text"
                value={llmConfigEmbedModel}
                onChange={(event) => setLlmConfigEmbedModel(event.target.value)}
                placeholder="nomic-embed-text:latest"
                spellCheck={false}
              />
            </div>
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="embed-ollama-base-url">Embedding Ollama Base URL（可选）</label>
              <input
                id="embed-ollama-base-url"
                className="dev-panel__input"
                type="text"
                value={llmConfigEmbedBaseUrl}
                onChange={(event) => setLlmConfigEmbedBaseUrl(event.target.value)}
                placeholder="http://localhost:11434（默认）"
                spellCheck={false}
              />
            </div>
          </div>
          <div className="settings-panel__save">
            <button
              type="button"
              className="dev-panel__button dev-panel__button--accent"
              onClick={() => void onSaveLlmConfig()}
              disabled={!isTauriRuntime() || llmConfigSaving}
            >
              {llmConfigSaving ? "保存中..." : "保存 LLM 配置"}
            </button>
          </div>
          <p className="settings-panel__hint">
            {isTauriRuntime()
              ? "云端配置仅保存在本地配置文件中，不会提交到仓库。可用 DeepSeek、GLM、MiniMax 三家预设，也可自由编辑为任意 OpenAI-compatible Provider。StrictLocal 模式下云 Provider 将被忽略。"
              : "浏览器预览模式下无法保存配置。"}
          </p>
        </div>
      </section>
      <section className="panel">
        <div className="section-head">
          <h2>拖拽行为</h2>
          <span className="section-head__hint">全局</span>
        </div>
        <div className="settings-panel">
          <div className="settings-panel__fields settings-panel__fields--single">
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="drop-mode">拖拽行为</label>
              <select
                id="drop-mode"
                className="dev-panel__input"
                value={dropMode}
                onChange={(event) => {
                  const mode: DropMode = event.target.value === "queue" ? "queue" : "direct";
                  onDropModeChange(mode);
                }}
              >
                <option value="direct">立即摄入</option>
                <option value="queue">加入队列</option>
              </select>
            </div>
          </div>
        </div>
      </section>
      <section className="panel">
        <div className="section-head">
          <h2>Shell 策略（全局）</h2>
          <span className="section-head__hint">独立模块</span>
        </div>
        <div className="settings-panel">
          <div className="settings-panel__shell-policy agent-studio__shell-policy">
            <div className="agent-studio__shell-policy-head">
              <strong>命令决策策略</strong>
              <div className="settings-panel__shell-policy-actions">
                <button
                  type="button"
                  className="dev-panel__button"
                  onClick={() => {
                    void handleReloadShellPolicy();
                  }}
                  disabled={agentShellPolicySaving || !isTauriRuntime()}
                >
                  刷新
                </button>
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--accent"
                  disabled={!agentShellPolicyDirty || agentShellPolicySaving || !agentShellPolicyConfig || !isTauriRuntime()}
                  onClick={() => {
                    void handleSaveShellPolicy();
                  }}
                >
                  {agentShellPolicySaving ? "保存中..." : "保存策略"}
                </button>
              </div>
            </div>
            <div className="agent-studio__shell-policy-presets">
              <span>档位：</span>
              {shellPolicyProfiles.map((profile) => (
                <button
                  key={`settings-${profile.key}`}
                  type="button"
                  className="agent-studio__shell-policy-preset"
                  disabled={agentShellPolicySaving || !agentShellPolicyConfig || !isTauriRuntime()}
                  onClick={() => applyShellPolicyProfile(profile.key)}
                >
                  {profile.label}
                </button>
              ))}
            </div>
            {agentShellPolicyConfig ? (
              <div className="agent-studio__shell-policy-grid">
                <label>
                  <span>manual: unknown</span>
                  <select
                    className="dev-panel__input"
                    value={agentShellPolicyConfig.manual_unknown_decision}
                    onChange={(event) => handleChangeShellPolicyDecision("manual_unknown_decision", event.target.value as ShellPolicyDecision)}
                    disabled={!isTauriRuntime()}
                  >
                    {shellPolicyDecisionOptions.map((option) => (
                      <option key={`settings-manual-${option.value}`} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>manual: write</span>
                  <select
                    className="dev-panel__input"
                    value={agentShellPolicyConfig.manual_write_decision}
                    onChange={(event) => handleChangeShellPolicyDecision("manual_write_decision", event.target.value as ShellPolicyDecision)}
                    disabled={!isTauriRuntime()}
                  >
                    {shellPolicyDecisionOptions.map((option) => (
                      <option key={`settings-manual-write-${option.value}`} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>agent: read</span>
                  <select
                    className="dev-panel__input"
                    value={agentShellPolicyConfig.agent_read_decision}
                    onChange={(event) => handleChangeShellPolicyDecision("agent_read_decision", event.target.value as ShellPolicyDecision)}
                    disabled={!isTauriRuntime()}
                  >
                    {shellPolicyDecisionOptions.map((option) => (
                      <option key={`settings-agent-read-${option.value}`} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>agent: write</span>
                  <select
                    className="dev-panel__input"
                    value={agentShellPolicyConfig.agent_write_decision}
                    onChange={(event) => handleChangeShellPolicyDecision("agent_write_decision", event.target.value as ShellPolicyDecision)}
                    disabled={!isTauriRuntime()}
                  >
                    {shellPolicyDecisionOptions.map((option) => (
                      <option key={`settings-write-${option.value}`} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>agent: unknown</span>
                  <select
                    className="dev-panel__input"
                    value={agentShellPolicyConfig.agent_unknown_decision}
                    onChange={(event) => handleChangeShellPolicyDecision("agent_unknown_decision", event.target.value as ShellPolicyDecision)}
                    disabled={!isTauriRuntime()}
                  >
                    {shellPolicyDecisionOptions.map((option) => (
                      <option key={`settings-unknown-${option.value}`} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>network（curl/wget 等）</span>
                  <select
                    className="dev-panel__input"
                    value={agentShellPolicyConfig.network_decision}
                    onChange={(event) => handleChangeShellPolicyDecision("network_decision", event.target.value as ShellPolicyDecision)}
                    disabled={!isTauriRuntime()}
                  >
                    {shellPolicyDecisionOptions.map((option) => (
                      <option key={`settings-network-${option.value}`} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>script（.ps1/.bat/.sh）</span>
                  <select
                    className="dev-panel__input"
                    value={agentShellPolicyConfig.script_decision}
                    onChange={(event) => handleChangeShellPolicyDecision("script_decision", event.target.value as ShellPolicyDecision)}
                    disabled={!isTauriRuntime()}
                  >
                    {shellPolicyDecisionOptions.map((option) => (
                      <option key={`settings-script-${option.value}`} value={option.value}>{option.label}</option>
                    ))}
                  </select>
                </label>
              </div>
            ) : (
              <p className="agent-studio__shell-policy-empty">策略加载中...</p>
            )}
            <p className="agent-studio__shell-policy-tip">
              这是全局策略配置，Agent 工具页中的同名面板会读取同一份配置。
            </p>
          </div>
        </div>
      </section>
      <SearchConfigPanel />
    </>
  );
}
