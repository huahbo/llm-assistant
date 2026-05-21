import { useEffect, useState } from "react";
import { getSearchConfig, isTauriRuntime, setSearchConfig } from "../../tauri-client";
import type { SearchConfig } from "../../types";

export default function SearchConfigPanel() {
  const [config, setConfig] = useState<SearchConfig>({
    search_provider: "none",
    search_providers: [],
    tavily_api_key: "",
    searxng_url: "http://localhost:8080",
    brave_api_key: "",
    breadth: 3,
    depth: 1,
  });
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    getSearchConfig().then((cfg) => {
      const providers = cfg.search_providers?.length
        ? cfg.search_providers
        : cfg.search_provider !== "none" ? [cfg.search_provider] : [];
      setConfig({ ...cfg, search_providers: providers });
    }).catch(() => {});
  }, []);

  const toggleProvider = (name: string) => {
    setConfig(prev => {
      const providers = prev.search_providers ?? [];
      const next = providers.includes(name)
        ? providers.filter(p => p !== name)
        : [...providers, name];
      return { ...prev, search_providers: next };
    });
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      const providers = config.search_providers ?? [];
      const toSave: SearchConfig = {
        ...config,
        search_provider: (providers[0] ?? "none") as SearchConfig["search_provider"],
        search_providers: providers,
      };
      await setSearchConfig(toSave);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch {
      // 静默
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="panel">
      <div className="section-head">
        <h2>搜索配置（Deep Research）</h2>
        <span className="section-head__hint">
          {isTauriRuntime() ? "本地配置文件" : "浏览器预览"}
        </span>
      </div>
      <div className="settings-panel">
        <div className="settings-panel__fields">
          <div className="dev-panel__field">
            <label className="dev-panel__label">搜索提供商（可多选）</label>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginTop: 6 }}>
              {/* Tavily checkbox */}
              <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={(config.search_providers ?? []).includes("tavily")}
                  onChange={() => toggleProvider("tavily")}
                />
                <span>Tavily（云端搜索，需 API Key）</span>
              </label>
              {(config.search_providers ?? []).includes("tavily") && (
                <div style={{ marginLeft: 24 }}>
                  <label className="dev-panel__label" htmlFor="tavily-api-key">Tavily API Key</label>
                  <input
                    id="tavily-api-key"
                    className="dev-panel__input"
                    type="password"
                    value={config.tavily_api_key}
                    onChange={(e) => setConfig(prev => ({ ...prev, tavily_api_key: e.target.value }))}
                    placeholder="tvly-..."
                    spellCheck={false}
                    autoComplete="off"
                  />
                </div>
              )}
              {/* SearXNG checkbox */}
              <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={(config.search_providers ?? []).includes("searxng")}
                  onChange={() => toggleProvider("searxng")}
                />
                <span>SearXNG（自托管，开源免费）</span>
              </label>
              {(config.search_providers ?? []).includes("searxng") && (
                <div style={{ marginLeft: 24 }}>
                  <label className="dev-panel__label" htmlFor="searxng-url">SearXNG 地址</label>
                  <input
                    id="searxng-url"
                    className="dev-panel__input"
                    type="text"
                    value={config.searxng_url}
                    onChange={(e) => setConfig(prev => ({ ...prev, searxng_url: e.target.value }))}
                    placeholder="http://localhost:8080"
                    spellCheck={false}
                  />
                </div>
              )}
            </div>
          </div>

          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="default-depth">默认研究深度</label>
            <select
              id="default-depth"
              className="dev-panel__input"
              value={config.depth}
              onChange={(e) =>
                setConfig((prev) => ({ ...prev, depth: Number(e.target.value) }))
              }
            >
              <option value={1}>1 - 标准 (快速)</option>
              <option value={2}>2 - 进阶 (更全面)</option>
              <option value={3}>3 - 深度 (多轮迭代)</option>
              <option value={4}>4 - 极深</option>
              <option value={5}>5 - 极限研究</option>
            </select>
          </div>

          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="default-breadth">默认搜索广度</label>
            <select
              id="default-breadth"
              className="dev-panel__input"
              value={config.breadth}
              onChange={(e) =>
                setConfig((prev) => ({ ...prev, breadth: Number(e.target.value) }))
              }
            >
              <option value={2}>2</option>
              <option value={3}>3</option>
              <option value={4}>4</option>
              <option value={5}>5</option>
            </select>
          </div>
        </div>

        <div className="settings-panel__save">
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={() => void handleSave()}
            disabled={saving || !isTauriRuntime()}
          >
            {saved ? "已保存" : saving ? "保存中..." : "保存搜索配置"}
          </button>
        </div>
        <p className="settings-panel__hint">
          {isTauriRuntime()
            ? "搜索配置用于 Deep Research 与 AI 对话的联网搜索。可同时勾选多个提供商，搜索时并行查询并合并去重结果。支持 Tavily（需 API Key）和 SearXNG（自托管）。"
            : "浏览器预览模式下无法保存配置。"}
        </p>
      </div>
    </section>
  );
}
