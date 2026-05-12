import { useState } from "react";
import { upsertMcpServer, reloadMcpServerTools } from "../../tauri-client";

interface CuratedServer {
  id: string;
  name: string;
  desc: string;
  pkg: string;
  params: Array<{ key: string; label: string; placeholder: string }>;
  envs: Array<{ key: string; label: string }>;
}

const CURATED: CuratedServer[] = [
  { id: "filesystem",   name: "Filesystem",         desc: "本地文件读写",          pkg: "@modelcontextprotocol/server-filesystem",          params: [{ key: "path", label: "允许访问的目录", placeholder: "C:\\Users\\用户名\\Documents" }], envs: [] },
  { id: "fetch",        name: "Fetch",               desc: "网页内容抓取",          pkg: "@modelcontextprotocol/server-fetch",               params: [], envs: [] },
  { id: "memory",       name: "Memory",              desc: "跨对话持久化记忆",      pkg: "@modelcontextprotocol/server-memory",              params: [], envs: [] },
  { id: "sequential",   name: "Sequential Thinking", desc: "分步推理辅助工具",      pkg: "@modelcontextprotocol/server-sequential-thinking", params: [], envs: [] },
  { id: "everything",   name: "Everything",          desc: "功能演示全套工具",      pkg: "@modelcontextprotocol/server-everything",          params: [], envs: [] },
  { id: "puppeteer",    name: "Puppeteer",            desc: "浏览器自动化控制",      pkg: "@modelcontextprotocol/server-puppeteer",           params: [], envs: [] },
  { id: "sqlite",       name: "SQLite",               desc: "SQLite 数据库查询",     pkg: "@modelcontextprotocol/server-sqlite",              params: [{ key: "db_path", label: "数据库文件路径", placeholder: "C:\\data\\my.db" }], envs: [] },
  { id: "postgres",     name: "PostgreSQL",           desc: "PostgreSQL 数据库",     pkg: "@modelcontextprotocol/server-postgres",            params: [{ key: "conn", label: "连接字符串",     placeholder: "postgresql://user:pass@localhost/db" }], envs: [] },
  { id: "github",       name: "GitHub",               desc: "GitHub 仓库与 PR 操作", pkg: "@modelcontextprotocol/server-github",              params: [], envs: [{ key: "GITHUB_TOKEN", label: "GitHub Token" }] },
  { id: "slack",        name: "Slack",                desc: "Slack 消息与频道",      pkg: "@modelcontextprotocol/server-slack",               params: [], envs: [{ key: "SLACK_BOT_TOKEN", label: "Bot Token" }, { key: "SLACK_TEAM_ID", label: "Team ID" }] },
  { id: "brave-search", name: "Brave Search",         desc: "Brave 联网搜索",        pkg: "@modelcontextprotocol/server-brave-search",        params: [], envs: [{ key: "BRAVE_API_KEY", label: "Brave API Key" }] },
  { id: "google-maps",  name: "Google Maps",          desc: "地图与地理位置",        pkg: "@modelcontextprotocol/server-google-maps",         params: [], envs: [{ key: "GOOGLE_MAPS_API_KEY", label: "Maps API Key" }] },
];

interface Props {
  onClose: () => void;
  onAdded: (name: string) => void;
}

export default function McpGallery({ onClose, onAdded }: Props) {
  const [selected, setSelected] = useState<CuratedServer | null>(null);
  const [paramValues, setParamValues] = useState<Record<string, string>>({});
  const [envValues, setEnvValues] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleSelect = (s: CuratedServer) => {
    setSelected(s);
    setParamValues({});
    setEnvValues({});
    setError("");
  };

  const handleAdd = async () => {
    if (!selected) return;
    setLoading(true);
    setError("");
    try {
      // Build args: ["cmd", "/c", "npx", "-y", pkg, ...param_values]
      const extraArgs = selected.params.map((p) => paramValues[p.key]?.trim() ?? "");
      const args = ["/c", "npx", "-y", selected.pkg, ...extraArgs].filter(Boolean);
      const env: Record<string, string> = {};
      for (const e of selected.envs) {
        if (envValues[e.key]?.trim()) env[e.key] = envValues[e.key].trim();
      }
      await upsertMcpServer(selected.id, "cmd", args, env);
      await reloadMcpServerTools(selected.id);
      onAdded(selected.name);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="mcp-gallery">
      <div className="mcp-gallery__header">
        <span className="mcp-gallery__title">添加 MCP 服务器</span>
        <button className="mcp-gallery__close" onClick={onClose}>✕</button>
      </div>

      {!selected ? (
        <div className="mcp-gallery__list">
          {CURATED.map((s) => (
            <button key={s.id} className="mcp-gallery__item" onClick={() => handleSelect(s)}>
              <span className="mcp-gallery__item-name">{s.name}</span>
              <span className="mcp-gallery__item-desc">{s.desc}</span>
            </button>
          ))}
        </div>
      ) : (
        <div className="mcp-gallery__form">
          <button className="mcp-gallery__back" onClick={() => setSelected(null)}>← 返回列表</button>
          <div className="mcp-gallery__form-title">{selected.name}</div>
          <div className="mcp-gallery__form-pkg">{selected.pkg}</div>

          {selected.params.map((p) => (
            <label key={p.key} className="mcp-gallery__label">
              {p.label}
              <input
                className="mcp-gallery__input"
                placeholder={p.placeholder}
                value={paramValues[p.key] ?? ""}
                onChange={(e) => setParamValues((v) => ({ ...v, [p.key]: e.target.value }))}
              />
            </label>
          ))}

          {selected.envs.length > 0 && (
            <div className="mcp-gallery__env-section">
              <div className="mcp-gallery__env-title">环境变量</div>
              {selected.envs.map((e) => (
                <label key={e.key} className="mcp-gallery__label">
                  {e.label}
                  <input
                    className="mcp-gallery__input"
                    placeholder={e.key}
                    value={envValues[e.key] ?? ""}
                    onChange={(ev) => setEnvValues((v) => ({ ...v, [e.key]: ev.target.value }))}
                  />
                </label>
              ))}
            </div>
          )}

          {error && <div className="mcp-gallery__error">{error}</div>}

          <button className="mcp-gallery__add-btn" onClick={handleAdd} disabled={loading}>
            {loading ? "添加中…" : "添加并加载工具"}
          </button>
        </div>
      )}
    </div>
  );
}
