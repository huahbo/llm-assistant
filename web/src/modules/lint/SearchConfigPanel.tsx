import { useEffect, useState } from "react";
import { getSearchConfig, isTauriRuntime, setSearchConfig } from "../../tauri-client";
import type { SearchConfig } from "../../types";

export default function SearchConfigPanel() {
  const [config, setConfig] = useState<SearchConfig>({
    search_provider: "none",
    tavily_api_key: "",
    searxng_url: "http://localhost:8080",
    brave_api_key: "",
    breadth: 3,
    depth: 1,
  });
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    getSearchConfig()
      .then((cfg) => setConfig(cfg))
      .catch(() => {});
  }, []);

  const handleSave = async () => {
    setSaving(true);
    try {
      await setSearchConfig(config);
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
            <label className="dev-panel__label" htmlFor="search-provider">搜索提供商</label>
            <select
              id="search-provider"
              className="dev-panel__input"
              value={config.search_provider}
              onChange={(e) =>
                setConfig((prev) => ({
                  ...prev,
                  search_provider: e.target.value as SearchConfig["search_provider"],
                }))
              }
            >
              <option value="none">无（禁用联网搜索）</option>
              <option value="tavily">Tavily</option>
              <option value="brave">Brave Search</option>
              <option value="searxng">SearXNG（自托管）</option>
            </select>
          </div>

          {config.search_provider === "tavily" && (
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="tavily-api-key">Tavily API Key</label>
              <input
                id="tavily-api-key"
                className="dev-panel__input"
                type="password"
                value={config.tavily_api_key}
                onChange={(e) =>
                  setConfig((prev) => ({ ...prev, tavily_api_key: e.target.value }))
                }
                placeholder="tvly-..."
                spellCheck={false}
                autoComplete="off"
              />
            </div>
          )}

          {config.search_provider === "searxng" && (
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="searxng-url">SearXNG 地址</label>
              <input
                id="searxng-url"
                className="dev-panel__input"
                type="text"
                value={config.searxng_url}
                onChange={(e) =>
                  setConfig((prev) => ({ ...prev, searxng_url: e.target.value }))
                }
                placeholder="http://localhost:8080"
                spellCheck={false}
              />
            </div>
          )}

          {config.search_provider === "brave" && (
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="brave-api-key">Brave Search API Key</label>
              <input
                id="brave-api-key"
                className="dev-panel__input"
                type="password"
                value={config.brave_api_key}
                onChange={(e) =>
                  setConfig((prev) => ({ ...prev, brave_api_key: e.target.value }))
                }
                placeholder="BSA..."
                spellCheck={false}
                autoComplete="off"
              />
            </div>
          )}

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
            ? "搜索配置用于 Deep Research 与 AI 对话的联网搜索。支持 Tavily / Brave Search（需申请 API Key）和 SearXNG（自托管）。Agent 对话会自动级联尝试所有已配置的搜索服务。"
            : "浏览器预览模式下无法保存配置。"}
        </p>
      </div>
    </section>
  );
}
