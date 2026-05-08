import { createContext, useCallback, useContext, useState, type ReactNode } from "react";
import { getShellPolicyConfig, isTauriRuntime, setShellPolicyConfig } from "../tauri-client";
import type { ShellPolicyConfig, ShellPolicyDecision } from "../types";

export type ShellPolicyProfileKey = "strict" | "balanced" | "power_user";

export const shellPolicyDecisionOptions: Array<{ value: ShellPolicyDecision; label: string }> = [
  { value: "auto_allow", label: "自动放行" },
  { value: "require_approval", label: "需要审批" },
  { value: "deny", label: "直接拒绝" },
];

export const defaultShellPolicyConfig: ShellPolicyConfig = {
  manual_unknown_decision: "auto_allow",
  manual_write_decision: "auto_allow",
  agent_read_decision: "auto_allow",
  agent_write_decision: "require_approval",
  agent_unknown_decision: "require_approval",
  network_decision: "require_approval",
  script_decision: "require_approval",
};

export const shellPolicyProfiles: Array<{
  key: ShellPolicyProfileKey;
  label: string;
  config: ShellPolicyConfig;
}> = [
  {
    key: "strict",
    label: "严格",
    config: {
      manual_unknown_decision: "deny",
      manual_write_decision: "require_approval",
      agent_read_decision: "require_approval",
      agent_write_decision: "deny",
      agent_unknown_decision: "deny",
      network_decision: "deny",
      script_decision: "deny",
    },
  },
  {
    key: "balanced",
    label: "平衡",
    config: defaultShellPolicyConfig,
  },
  {
    key: "power_user",
    label: "高能力",
    config: {
      manual_unknown_decision: "auto_allow",
      manual_write_decision: "auto_allow",
      agent_read_decision: "auto_allow",
      agent_write_decision: "auto_allow",
      agent_unknown_decision: "auto_allow",
      network_decision: "auto_allow",
      script_decision: "require_approval",
    },
  },
];

type ShellPolicyValue = {
  config: ShellPolicyConfig | null;
  saving: boolean;
  dirty: boolean;
  message: string;
  clearMessage: () => void;
  reload: (options?: { silent?: boolean }) => Promise<void>;
  save: () => Promise<void>;
  applyProfile: (profileKey: ShellPolicyProfileKey) => void;
  applyAndSaveProfile: (profileKey: ShellPolicyProfileKey) => Promise<void>;
  setField: (field: keyof ShellPolicyConfig, decision: ShellPolicyDecision) => void;
};

export function getActiveProfileKey(config: ShellPolicyConfig | null): ShellPolicyProfileKey | null {
  if (!config) return null;
  const keys = Object.keys(config) as (keyof ShellPolicyConfig)[];
  for (const profile of shellPolicyProfiles) {
    if (keys.every((k) => config[k] === profile.config[k])) return profile.key;
  }
  return null;
}

const ShellPolicyContext = createContext<ShellPolicyValue | null>(null);

export function ShellPolicyProvider({ children }: { children: ReactNode }) {
  const [config, setConfig] = useState<ShellPolicyConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [message, setMessage] = useState("");

  const clearMessage = useCallback(() => setMessage(""), []);

  const setField = useCallback((field: keyof ShellPolicyConfig, decision: ShellPolicyDecision) => {
    setConfig((prev) => {
      const base = prev ?? defaultShellPolicyConfig;
      return { ...base, [field]: decision };
    });
    setDirty(true);
  }, []);

  const save = useCallback(async () => {
    if (!config || saving || !isTauriRuntime()) return;
    setSaving(true);
    try {
      const saved = await setShellPolicyConfig(config);
      if (!saved) {
        setMessage("Shell 策略保存失败，请检查后端日志。");
        return;
      }
      setConfig(saved);
      setDirty(false);
      setMessage("Shell 策略已保存，后续命令将按新策略执行。");
    } finally {
      setSaving(false);
    }
  }, [config, saving]);

  const applyProfile = useCallback((profileKey: ShellPolicyProfileKey) => {
    const profile = shellPolicyProfiles.find((item) => item.key === profileKey);
    if (!profile) return;
    setConfig({ ...profile.config });
    setDirty(true);
  }, []);

  const applyAndSaveProfile = useCallback(async (profileKey: ShellPolicyProfileKey) => {
    if (saving || !isTauriRuntime()) return;
    const profile = shellPolicyProfiles.find((item) => item.key === profileKey);
    if (!profile) return;
    setSaving(true);
    try {
      const saved = await setShellPolicyConfig({ ...profile.config });
      if (!saved) {
        setMessage("Shell 策略保存失败，请检查后端日志。");
        return;
      }
      setConfig(saved);
      setDirty(false);
      setMessage(`已切换为"${profile.label}"策略。`);
    } finally {
      setSaving(false);
    }
  }, [saving]);

  const reload = useCallback(async (options?: { silent?: boolean }) => {
    const nextConfig = await getShellPolicyConfig();
    if (!nextConfig) {
      setMessage("读取 Shell 策略失败，请检查后端日志。");
      return;
    }
    setConfig(nextConfig);
    setDirty(false);
    if (!options?.silent) {
      setMessage("已刷新 Shell 策略。");
    }
  }, []);

  return (
    <ShellPolicyContext.Provider
      value={{
        config,
        saving,
        dirty,
        message,
        clearMessage,
        reload,
        save,
        applyProfile,
        applyAndSaveProfile,
        setField,
      }}
    >
      {children}
    </ShellPolicyContext.Provider>
  );
}

export function useShellPolicy() {
  const value = useContext(ShellPolicyContext);
  if (!value) throw new Error("useShellPolicy 必须在 <ShellPolicyProvider> 内使用");
  return value;
}
