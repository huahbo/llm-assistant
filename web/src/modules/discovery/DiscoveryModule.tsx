import { useState, useEffect, useCallback } from "react";
import {
  searchMcpRegistry,
  getMcpRegistryServer,
  listMcpServers,
  upsertMcpServer,
  deleteMcpServer,
  reloadMcpServerTools,
} from "../../tauri-client/mcp";
import type { SmitheryServer, McpServerConfig } from "../../types";
import "./discovery.css";

type EnvEntry = { key: string; value: string };

export default function DiscoveryModule() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SmitheryServer[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);

  const [installed, setInstalled] = useState<McpServerConfig[]>([]);
  const [reloadStatus, setReloadStatus] = useState<Record<string, string>>({});
  const [installing, setInstalling] = useState<Set<string>>(new Set());

  // Env 配置对话框
  const [envDialog, setEnvDialog] = useState<{
    server: SmitheryServer;
    command: string;
    args: string[];
    requiredKeys: string[];
    entries: EnvEntry[];
  } | null>(null);

  const loadInstalled = useCallback(async () => {
    try {
      const list = await listMcpServers();
      setInstalled(list);
    } catch {
      // 静默失败
    }
  }, []);

  useEffect(() => {
    void loadInstalled();
  }, [loadInstalled]);

  const handleSearch = async () => {
    const q = query.trim();
    if (!q) return;
    setSearching(true);
    setSearchError(null);
    try {
      const list = await searchMcpRegistry(q, 20);
      setResults(list);
      if (list.length === 0) setSearchError("未找到相关 MCP Server，换个关键词试试。");
    } catch (e) {
      setSearchError(`搜索失败：${String(e)}`);
      setResults([]);
    } finally {
      setSearching(false);
    }
  };

  const isInstalled = (qualifiedName: string) =>
    installed.some((s) => s.name === qualifiedName || s.args.includes(qualifiedName));

  const handleInstallClick = async (server: SmitheryServer) => {
    setInstalling((prev) => new Set(prev).add(server.qualified_name));
    try {
      const detail = await getMcpRegistryServer(server.qualified_name);
      if (detail.required_env_keys.length > 0) {
        setEnvDialog({
          server,
          command: detail.command,
          args: detail.args,
          requiredKeys: detail.required_env_keys,
          entries: detail.required_env_keys.map((k) => ({ key: k, value: "" })),
        });
      } else {
        await doInstall(server.display_name || server.qualified_name, detail.command, detail.args, {});
      }
    } catch (e) {
      alert(`获取安装配置失败：${String(e)}`);
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(server.qualified_name);
        return next;
      });
    }
  };

  const doInstall = async (
    name: string,
    command: string,
    args: string[],
    env: Record<string, string>,
  ) => {
    await upsertMcpServer(name, command, args, env);
    await loadInstalled();
    try {
      const tools = await reloadMcpServerTools(name);
      setReloadStatus((prev) => ({ ...prev, [name]: `✅ 已连接，${tools.length} 个工具` }));
    } catch {
      setReloadStatus((prev) => ({ ...prev, [name]: "⚠️ 已保存，连接失败（请手动重载）" }));
    }
  };

  const handleEnvConfirm = async () => {
    if (!envDialog) return;
    const env: Record<string, string> = {};
    for (const { key, value } of envDialog.entries) {
      if (key.trim()) env[key.trim()] = value;
    }
    const name = envDialog.server.display_name || envDialog.server.qualified_name;
    setInstalling((prev) => new Set(prev).add(envDialog.server.qualified_name));
    setEnvDialog(null);
    try {
      await doInstall(name, envDialog.command, envDialog.args, env);
    } catch (e) {
      alert(`安装失败：${String(e)}`);
    } finally {
      setInstalling((prev) => {
        const next = new Set(prev);
        next.delete(envDialog?.server.qualified_name ?? "");
        return next;
      });
    }
  };

  const handleDelete = async (name: string) => {
    if (!confirm(`确认删除 MCP Server「${name}」？`)) return;
    try {
      await deleteMcpServer(name);
      await loadInstalled();
      setReloadStatus((prev) => { const n = { ...prev }; delete n[name]; return n; });
    } catch (e) {
      alert(`删除失败：${String(e)}`);
    }
  };

  const handleReload = async (name: string) => {
    setReloadStatus((prev) => ({ ...prev, [name]: "连接中…" }));
    try {
      const tools = await reloadMcpServerTools(name);
      setReloadStatus((prev) => ({ ...prev, [name]: `✅ ${tools.length} 个工具` }));
    } catch (e) {
      setReloadStatus((prev) => ({ ...prev, [name]: `❌ ${String(e)}` }));
    }
  };

  return (
    <div className="discovery">
      <div className="module-header">
        <h1 className="module-header__title">MCP 市场</h1>
        <p className="module-header__sub">搜索并一键安装 Smithery Registry 中的 MCP Server，扩展 Agent 工具箱</p>
      </div>

      <div className="discovery__body">
        {/* 搜索区 */}
        <section className="discovery__section">
          <div className="discovery__search-bar">
            <input
              className="discovery__search-input"
              type="text"
              placeholder="搜索 MCP Server（如 filesystem、github、database…）"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") void handleSearch(); }}
            />
            <button
              className="discovery__btn discovery__btn--primary"
              onClick={() => void handleSearch()}
              disabled={searching || !query.trim()}
            >
              {searching ? "搜索中…" : "搜索"}
            </button>
          </div>

          {searchError && <p className="discovery__hint discovery__hint--error">{searchError}</p>}

          {results.length > 0 && (
            <div className="discovery__results">
              {results.map((server) => {
                const done = isInstalled(server.qualified_name);
                const busy = installing.has(server.qualified_name);
                return (
                  <div key={server.qualified_name} className="discovery__result-item">
                    <div className="discovery__result-meta">
                      <span className="discovery__result-name">
                        {server.display_name || server.qualified_name}
                        {server.verified && <span className="discovery__badge discovery__badge--verified">已验证</span>}
                      </span>
                      <span className="discovery__result-qname">{server.qualified_name}</span>
                      <p className="discovery__result-desc">{server.description}</p>
                      {server.use_count > 0 && (
                        <span className="discovery__result-count">{server.use_count.toLocaleString()} installs</span>
                      )}
                    </div>
                    <button
                      className={`discovery__btn ${done ? "discovery__btn--installed" : "discovery__btn--primary"}`}
                      disabled={busy || done}
                      onClick={() => void handleInstallClick(server)}
                    >
                      {busy ? "安装中…" : done ? "已安装" : "安装"}
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </section>

        {/* 已安装区 */}
        <section className="discovery__section">
          <h2 className="discovery__section-title">已安装的 MCP Server</h2>
          {installed.length === 0 ? (
            <p className="discovery__hint">暂无已安装的 MCP Server。搜索并安装后会显示在这里。</p>
          ) : (
            <div className="discovery__installed-list">
              {installed.map((srv) => (
                <div key={srv.name} className="discovery__installed-item">
                  <div className="discovery__installed-meta">
                    <span className="discovery__installed-name">{srv.name}</span>
                    <span className="discovery__installed-cmd">
                      {srv.command} {srv.args.join(" ")}
                    </span>
                    {reloadStatus[srv.name] && (
                      <span className="discovery__installed-status">{reloadStatus[srv.name]}</span>
                    )}
                  </div>
                  <div className="discovery__installed-actions">
                    <button
                      className="discovery__btn discovery__btn--sm"
                      onClick={() => void handleReload(srv.name)}
                    >
                      重载工具
                    </button>
                    <button
                      className="discovery__btn discovery__btn--sm discovery__btn--danger"
                      onClick={() => void handleDelete(srv.name)}
                    >
                      删除
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>

      {/* Env 配置对话框 */}
      {envDialog && (
        <div className="discovery__overlay" onClick={() => setEnvDialog(null)}>
          <div className="discovery__dialog" onClick={(e) => e.stopPropagation()}>
            <h3 className="discovery__dialog-title">配置环境变量</h3>
            <p className="discovery__dialog-hint">
              「{envDialog.server.display_name}」需要以下环境变量（通常是 API Key）：
            </p>
            <div className="discovery__env-list">
              {envDialog.entries.map((entry, idx) => (
                <div key={entry.key} className="discovery__env-row">
                  <span className="discovery__env-key">{entry.key}</span>
                  <input
                    className="discovery__env-input"
                    type="text"
                    placeholder={`填写 ${entry.key} 的值`}
                    value={entry.value}
                    onChange={(e) => {
                      const updated = [...envDialog.entries];
                      updated[idx] = { ...entry, value: e.target.value };
                      setEnvDialog({ ...envDialog, entries: updated });
                    }}
                  />
                </div>
              ))}
            </div>
            <p className="discovery__dialog-hint discovery__dialog-hint--muted">
              留空的字段不会写入配置，可安装后在「Settings → MCP 服务器」中补填。
            </p>
            <div className="discovery__dialog-actions">
              <button className="discovery__btn" onClick={() => setEnvDialog(null)}>取消</button>
              <button className="discovery__btn discovery__btn--primary" onClick={() => void handleEnvConfirm()}>
                确认安装
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
