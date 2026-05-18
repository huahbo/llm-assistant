import { useEffect, useRef, useState } from "react";
import { getEmbedStatus, isTauriRuntime, rebuildEmbeddings } from "../../tauri-client";
import type { EmbedStatus, LlmProviderConfig } from "../../types";

type EmbeddingPanelProps = {
  llmConfig: LlmProviderConfig | null;
  embedBackend: string;
  embedOnnxModel: string;
  embedOllamaModel: string;
  embedOllamaBaseUrl: string;
  onEmbedBackendChange: (value: string) => void;
  onEmbedOnnxModelChange: (value: string) => void;
  onEmbedOllamaModelChange: (value: string) => void;
  onEmbedOllamaBaseUrlChange: (value: string) => void;
  onSaveConfig: () => Promise<void>;
  saving: boolean;
};

export default function EmbeddingPanel({
  llmConfig,
  embedBackend,
  embedOnnxModel,
  embedOllamaModel,
  embedOllamaBaseUrl,
  onEmbedBackendChange,
  onEmbedOnnxModelChange,
  onEmbedOllamaModelChange,
  onEmbedOllamaBaseUrlChange,
  onSaveConfig,
  saving,
}: EmbeddingPanelProps) {
  const [status, setStatus] = useState<EmbedStatus | null>(null);
  const [statusLoading, setStatusLoading] = useState(false);
  const [rebuilding, setRebuilding] = useState(false);
  const [rebuildResult, setRebuildResult] = useState<string>("");
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  const loadStatus = async () => {
    setStatusLoading(true);
    try {
      const s = await getEmbedStatus();
      if (mountedRef.current) setStatus(s);
    } finally {
      if (mountedRef.current) setStatusLoading(false);
    }
  };

  useEffect(() => {
    void loadStatus();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleRebuild = async () => {
    setRebuilding(true);
    setRebuildResult("");
    try {
      const count = await rebuildEmbeddings();
      setRebuildResult(`已重建 ${count} 个页面的向量索引。`);
      void loadStatus();
    } catch (e) {
      setRebuildResult(`重建失败：${e instanceof Error ? e.message : String(e)}`);
    } finally {
      if (mountedRef.current) setRebuilding(false);
    }
  };

  const backendLabel = (id: string) => {
    if (!id || id === "noop") return "未初始化";
    if (id.startsWith("onnx:")) return `本地 ONNX（${id.slice(5)}）`;
    if (id.startsWith("ollama:")) return `Ollama（${id.slice(7)}）`;
    return id;
  };

  const isTauri = isTauriRuntime();
  const currentBackend = llmConfig?.embed_backend ?? embedBackend;

  return (
    <section className="panel">
      <div className="section-head">
        <h2>向量嵌入（Embedding）</h2>
        <span className="section-head__hint">本地 ONNX / Ollama / 关闭</span>
      </div>
      <div className="settings-panel">
        <div className="settings-panel__fields">
          <div className="dev-panel__field">
            <label className="dev-panel__label" htmlFor="embed-backend">Embedding 后端</label>
            <select
              id="embed-backend"
              className="dev-panel__input"
              value={embedBackend}
              onChange={(e) => onEmbedBackendChange(e.target.value)}
              disabled={!isTauri}
            >
              <option value="onnx">本地 ONNX（推荐，无需外部服务）</option>
              <option value="ollama">Ollama（需要本地 Ollama 运行）</option>
              <option value="disabled">关闭（语义搜索不可用）</option>
            </select>
          </div>
          {embedBackend === "onnx" && (
            <div className="dev-panel__field">
              <label className="dev-panel__label" htmlFor="embed-onnx-model">ONNX 模型</label>
              <select
                id="embed-onnx-model"
                className="dev-panel__input"
                value={embedOnnxModel}
                onChange={(e) => onEmbedOnnxModelChange(e.target.value)}
                disabled={!isTauri}
              >
                <option value="multilingual-e5-small">multilingual-e5-small（384 维，中英文）</option>
                <option value="bge-small-zh-v1.5">bge-small-zh-v1.5（512 维，中文优先）</option>
              </select>
            </div>
          )}
          {embedBackend === "ollama" && (
            <>
              <div className="dev-panel__field">
                <label className="dev-panel__label" htmlFor="embed-ollama-model">Embedding 模型（Ollama）</label>
                <input
                  id="embed-ollama-model"
                  className="dev-panel__input"
                  type="text"
                  value={embedOllamaModel}
                  onChange={(e) => onEmbedOllamaModelChange(e.target.value)}
                  placeholder="nomic-embed-text:latest"
                  disabled={!isTauri}
                  spellCheck={false}
                />
              </div>
              <div className="dev-panel__field">
                <label className="dev-panel__label" htmlFor="embed-ollama-base-url">Ollama Base URL（可选）</label>
                <input
                  id="embed-ollama-base-url"
                  className="dev-panel__input"
                  type="text"
                  value={embedOllamaBaseUrl}
                  onChange={(e) => onEmbedOllamaBaseUrlChange(e.target.value)}
                  placeholder="http://localhost:11434（默认）"
                  disabled={!isTauri}
                  spellCheck={false}
                />
              </div>
            </>
          )}
        </div>

        <div className="settings-panel__section-title" style={{ marginTop: 16 }}>当前状态</div>
        {statusLoading ? (
          <p className="dev-panel__hint">加载中…</p>
        ) : status ? (
          <div className="embed-status-card">
            <div className="embed-status-card__row">
              <span className="embed-status-card__label">后端</span>
              <span className="embed-status-card__value">{backendLabel(status.backend_id)}</span>
            </div>
            <div className="embed-status-card__row">
              <span className="embed-status-card__label">向量维度</span>
              <span className="embed-status-card__value">{status.dimension > 0 ? `${status.dimension} 维` : "—"}</span>
            </div>
            <div className="embed-status-card__row">
              <span className="embed-status-card__label">已索引页数</span>
              <span className="embed-status-card__value">{status.indexed_count}</span>
            </div>
            <div className="embed-status-card__row">
              <span className="embed-status-card__label">健康状态</span>
              <span
                className="embed-status-card__value"
                style={{ color: status.healthy ? "var(--accent)" : "var(--error)" }}
              >
                {status.healthy ? "正常" : (currentBackend === "disabled" ? "已关闭" : "异常")}
              </span>
            </div>
            {status.model_dir && (
              <div className="embed-status-card__row" style={{ gridColumn: "1 / -1" }}>
                <span className="embed-status-card__label">模型目录</span>
                <span className="embed-status-card__value" style={{ fontSize: "0.82em", wordBreak: "break-all", color: "var(--text-muted)" }}>
                  {status.model_dir}
                </span>
              </div>
            )}
            {status.last_error && (
              <div className="embed-status-card__row" style={{ gridColumn: "1 / -1" }}>
                <span className="embed-status-card__label">错误详情</span>
                <span className="embed-status-card__value" style={{ fontSize: "0.82em", wordBreak: "break-all", color: "var(--error)" }}>
                  {status.last_error}
                </span>
              </div>
            )}
          </div>
        ) : (
          <p className="dev-panel__hint">
            {isTauri ? "状态不可用（Vault 未初始化？）" : "浏览器预览模式下不可用。"}
          </p>
        )}

        <div className="settings-panel__save" style={{ marginTop: 16, gap: 8, display: "flex", alignItems: "center", flexWrap: "wrap" }}>
          <button
            type="button"
            className="dev-panel__button dev-panel__button--accent"
            onClick={async () => {
              await onSaveConfig();
              // 后台 spawn_blocking 重新初始化 ONNX 可能耗时 5-10 秒，轮询状态直到 backend_id 变化或出错
              for (let i = 0; i < 12; i++) {
                await new Promise((r) => setTimeout(r, 1200));
                if (!mountedRef.current) return;
                try {
                  const s = await getEmbedStatus();
                  if (mountedRef.current) setStatus(s);
                  if (s.backend_id !== "noop" || s.last_error) break;
                } catch {
                  break;
                }
              }
            }}
            disabled={saving || !isTauri}
            title="保存嵌入后端配置（同时保存 LLM 配置）"
          >
            {saving ? "保存中…" : "保存嵌入配置"}
          </button>
          <button
            type="button"
            className="dev-panel__button"
            onClick={() => void loadStatus()}
            disabled={statusLoading || !isTauri}
          >
            刷新状态
          </button>
          <button
            type="button"
            className="dev-panel__button"
            onClick={() => void handleRebuild()}
            disabled={rebuilding || !isTauri || embedBackend === "disabled"}
            title="对所有 Wiki 页面重新生成向量并写入数据库"
          >
            {rebuilding ? "重建中…" : "全量重建向量索引"}
          </button>
          {rebuildResult && (
            <span className="dev-panel__hint" style={{ marginLeft: 8 }}>{rebuildResult}</span>
          )}
        </div>

        <p className="settings-panel__hint" style={{ marginTop: 10 }}>
          {embedBackend === "onnx"
            ? status?.model_dir
              ? `ONNX 模型未找到。请将模型文件（onnx/model.onnx、tokenizer.json 等）放至：${status.model_dir}。开发者可运行 scripts/download-embed-models.ps1 自动下载（~450 MB）。`
              : "ONNX 后端在本机 CPU 上推理，无需外部服务。"
            : embedBackend === "ollama"
              ? "Ollama 后端使用本地 Ollama 服务生成向量，请确保 Ollama 已启动且已拉取 Embedding 模型。"
              : "Embedding 已关闭，语义搜索功能不可用。"}
        </p>
      </div>
    </section>
  );
}
