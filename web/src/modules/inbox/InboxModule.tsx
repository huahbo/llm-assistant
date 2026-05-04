import { formatLogLevel } from "../../app-formatters";
import { formatLintCheckedAt } from "../../lint-utils";
import type { OcrProvider } from "../../tauri-client";
import type {
  AppOverview,
  LogEntry,
  ModeId,
  WikiTemplate,
} from "../../types";

type DevAction = "init_vault" | "ingest_markdown" | "ingest_pdf" | "ingest_file" | "ingest_url";

type TemplateInitPreview = {
  dirs: string[];
  files: string[];
};

type ModeOption = {
  id: ModeId;
  label: string;
  isActive: boolean;
};

type InboxModuleProps = {
  overview: AppOverview | null;
  pagesCount: number;
  ingesting: boolean;
  supportedModesText: string;
  currentModeLabel: string;
  currentModeBadge: string;
  currentModeDescription: string;
  modeOptions: ModeOption[];
  switchingMode: ModeId | null;
  llmAvailabilityText: string;
  llmModelText: string;
  llmAddressText: string;
  llmHintText: string;
  isTauri: boolean;
  onModeSelect: (modeId: ModeId) => void | Promise<void>;
  vaultPath: string;
  setVaultPath: (path: string) => void;
  pickVaultFolder: () => void | Promise<void>;
  selectedTemplateId: string;
  setSelectedTemplateId: (id: string) => void;
  templates: WikiTemplate[];
  selectedTemplate: WikiTemplate;
  templateInitPreview: TemplateInitPreview;
  devAction: DevAction | null;
  onDemoIngest: () => void | Promise<void>;
  onInitVault: () => void | Promise<void>;
  recentVaultPaths: string[];
  clearRecentVaultPaths: () => void;
  selectRecentVaultPath: (path: string) => void;
  ingestUrlInput: string;
  setIngestUrlInput: (value: string) => void;
  onUrlIngest: () => void | Promise<void>;
  queueEnqueueing: boolean;
  enqueueUrl: (url: string) => void | Promise<void>;
  ingestFilePickedPaths: string[];
  ingestFilePath: string;
  setIngestFilePath: (value: string) => void;
  clearIngestFilePickedPaths: () => void;
  pickIngestFiles: () => void | Promise<void>;
  defaultIngestFilePath: string;
  ingestFileOcrProvider: OcrProvider;
  ocrProviderLabels: Record<OcrProvider, string>;
  setIngestFileOcrProvider: (provider: OcrProvider) => void | Promise<void>;
  onFileIngest: () => void | Promise<void>;
  enqueueFiles: (paths: string[]) => void | Promise<void>;
  clipServerOnline: boolean | null;
  clipServerPort: number;
  logs: LogEntry[];
};

export default function InboxModule({
  overview,
  pagesCount,
  ingesting,
  supportedModesText,
  currentModeLabel,
  currentModeBadge,
  currentModeDescription,
  modeOptions,
  switchingMode,
  llmAvailabilityText,
  llmModelText,
  llmAddressText,
  llmHintText,
  isTauri,
  onModeSelect,
  vaultPath,
  setVaultPath,
  pickVaultFolder,
  selectedTemplateId,
  setSelectedTemplateId,
  templates,
  selectedTemplate,
  templateInitPreview,
  devAction,
  onDemoIngest,
  onInitVault,
  recentVaultPaths,
  clearRecentVaultPaths,
  selectRecentVaultPath,
  ingestUrlInput,
  setIngestUrlInput,
  onUrlIngest,
  queueEnqueueing,
  enqueueUrl,
  ingestFilePickedPaths,
  ingestFilePath,
  setIngestFilePath,
  clearIngestFilePickedPaths,
  pickIngestFiles,
  defaultIngestFilePath,
  ingestFileOcrProvider,
  ocrProviderLabels,
  setIngestFileOcrProvider,
  onFileIngest,
  enqueueFiles,
  clipServerOnline,
  clipServerPort,
  logs,
}: InboxModuleProps) {
  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">概览</h1>
        <p className="module-header__sub">应用状态、Vault 操作与最近日志</p>
      </div>

      {overview ? (
        <div className="stats-row">
          <div className="stat-card">
            <div className="stat-card__value">{pagesCount}</div>
            <div className="stat-card__label">Wiki 页面</div>
          </div>
          <div className="stat-card">
            <div className="stat-card__value">{overview.recent_log_count}</div>
            <div className="stat-card__label">最近日志</div>
          </div>
          <div className="stat-card">
            <div className="stat-card__value">{overview.pending_tasks}</div>
            <div className="stat-card__label">待处理任务</div>
          </div>
          {ingesting ? (
            <div className="stat-card stat-card--ingesting">
              <div className="stat-card__value stat-card__loader">⏳</div>
              <div className="stat-card__label">正在摄入...</div>
            </div>
          ) : null}
        </div>
      ) : null}

      <section className="panel">
        <div className="section-head">
          <h2>运行模式</h2>
          <span className="section-head__hint">{supportedModesText}</span>
        </div>
        <div className="runtime-banner">
          <div>
            <span className="runtime-banner__mode">
              {currentModeLabel}
              <span className="runtime-banner__badge">{currentModeBadge}</span>
            </span>
            <p className="runtime-banner__description">{currentModeDescription}</p>
          </div>
          <div className="dev-panel__actions">
            {modeOptions.map((mode) => (
              <button
                key={mode.id}
                type="button"
                className={`mode-option${mode.isActive ? " mode-option--active" : ""}`}
                onClick={() => void onModeSelect(mode.id)}
                disabled={!isTauri || !overview || switchingMode !== null}
              >
                <span className="mode-option__name">
                  {mode.label}
                  {switchingMode === mode.id ? (
                    <span className="mode-option__badge">切换中...</span>
                  ) : mode.isActive ? (
                    <span className="mode-option__badge">当前</span>
                  ) : null}
                </span>
              </button>
            ))}
          </div>
        </div>
        <div className="llm-status-grid">
          <div className="llm-status-card">
            <div className="llm-status-card__label">LLM 状态</div>
            <div className="llm-status-card__value">{llmAvailabilityText}</div>
          </div>
          <div className="llm-status-card">
            <div className="llm-status-card__label">模型</div>
            <div className="llm-status-card__value">{llmModelText}</div>
          </div>
          <div className="llm-status-card">
            <div className="llm-status-card__label">地址</div>
            <div className="llm-status-card__value">{llmAddressText}</div>
          </div>
          <div className="llm-status-card">
            <div className="llm-status-card__label">提示</div>
            <div className="llm-status-card__value">{llmHintText}</div>
          </div>
        </div>
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>Vault 操作</h2>
          <span className="section-head__hint">
            {isTauri ? "Tauri 可用" : "浏览器预览"}
          </span>
        </div>
        <div className="dev-panel">
          <div className="dev-panel__vault-row">
            <div className="dev-panel__field dev-panel__vault-path">
              <label className="dev-panel__label" htmlFor="vault-path">
                Vault 路径
              </label>
              <div className="path-input-row">
                <input
                  id="vault-path"
                  className="dev-panel__input"
                  type="text"
                  value={vaultPath}
                  onChange={(event) => setVaultPath(event.target.value)}
                  placeholder="vault"
                  spellCheck={false}
                />
                <button
                  type="button"
                  className="dev-panel__button path-pick-btn"
                  onClick={() => void pickVaultFolder()}
                  disabled={!isTauri}
                  title="选择文件夹"
                >
                  📁
                </button>
              </div>
            </div>
            <div className="dev-panel__field" style={{ flex: 1 }}>
              <label className="dev-panel__label">选择项目模板</label>
              <select
                className="dev-panel__input"
                value={selectedTemplateId}
                onChange={(event) => setSelectedTemplateId(event.target.value)}
                disabled={devAction !== null}
              >
                {templates.map((template) => (
                  <option key={template.id} value={template.id}>
                    {template.icon} {template.name} — {template.description}
                  </option>
                ))}
              </select>
            </div>
            <button
              type="button"
              className="dev-panel__button dev-panel__vault-action"
              onClick={() => void onDemoIngest()}
              disabled={!isTauri || devAction !== null}
              title="用内置示例文件测试摄入流程"
            >
              {devAction === "ingest_markdown" ? "摄入中..." : "示例摄入"}
            </button>
            <button
              type="button"
              className="dev-panel__button dev-panel__vault-action"
              onClick={() => void onInitVault()}
              disabled={!isTauri || devAction !== null}
            >
              {devAction === "init_vault" ? "初始化中..." : "初始化 Vault"}
            </button>
          </div>

          {recentVaultPaths.length > 0 ? (
            <div className="recent-vaults">
              <div className="recent-vaults__head">
                <span className="dev-panel__hint">最近项目</span>
                <button
                  type="button"
                  className="dev-panel__button recent-vaults__clear"
                  onClick={clearRecentVaultPaths}
                >
                  清空
                </button>
              </div>
              <div className="recent-vaults__list">
                {recentVaultPaths.map((path) => (
                  <button
                    key={path}
                    type="button"
                    className="recent-vaults__item"
                    title={path}
                    onClick={() => selectRecentVaultPath(path)}
                  >
                    {path}
                  </button>
                ))}
              </div>
            </div>
          ) : null}

          <div className="template-init-preview">
            <div className="template-init-preview__head">
              <div className="template-init-preview__title">
                <span>{selectedTemplate.icon}</span>
                <strong>{selectedTemplate.name}</strong>
              </div>
              <span className="template-init-preview__desc">{selectedTemplate.description}</span>
            </div>
            <div className="template-init-preview__meta">
              <span>schema：{selectedTemplate.schema.split(/\r?\n/).length} 行</span>
              <span>purpose：{selectedTemplate.purpose.split(/\r?\n/).length} 行</span>
            </div>
            <div className="template-init-preview__grid">
              <div className="template-init-preview__block">
                <h4>将创建目录（{templateInitPreview.dirs.length}）</h4>
                <ul>
                  {templateInitPreview.dirs.map((dirPath) => (
                    <li key={dirPath}>
                      <code>{dirPath}</code>
                    </li>
                  ))}
                </ul>
              </div>
              <div className="template-init-preview__block">
                <h4>将创建文件（{templateInitPreview.files.length}）</h4>
                <ul>
                  {templateInitPreview.files.map((filePath) => (
                    <li key={filePath}>
                      <code>{filePath}</code>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </div>

          <div className="ingest-grid">
            <div className="ingest-card">
              <span className="ingest-card__title">URL 摄入</span>
              <div className="dev-panel__field">
                <label className="dev-panel__label" htmlFor="ingest-url-input">
                  网页地址
                </label>
                <input
                  id="ingest-url-input"
                  className="dev-panel__input"
                  type="url"
                  value={ingestUrlInput}
                  onChange={(event) => setIngestUrlInput(event.target.value)}
                  placeholder="https://example.com/article"
                  spellCheck={false}
                />
              </div>
              <div className="ingest-card__footer">
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--accent"
                  onClick={() => void onUrlIngest()}
                  disabled={!isTauri || devAction !== null}
                >
                  {devAction === "ingest_url" ? "摄入中..." : "URL 摄入"}
                </button>
                <button
                  type="button"
                  className="dev-panel__button"
                  disabled={!isTauri || queueEnqueueing || !ingestUrlInput.trim()}
                  onClick={() => {
                    if (!ingestUrlInput.trim()) return;
                    void enqueueUrl(ingestUrlInput.trim());
                  }}
                >
                  {queueEnqueueing ? "入队中..." : "加入队列"}
                </button>
              </div>
            </div>

            <div className="ingest-card">
              <span className="ingest-card__title">文件摄入</span>
              <div className="dev-panel__field">
                <label className="dev-panel__label" htmlFor="ingest-file-path">
                  文件路径
                </label>
                <div className="path-input-row">
                  <input
                    id="ingest-file-path"
                    className="dev-panel__input"
                    type="text"
                    value={ingestFilePickedPaths.length > 0 ? "" : ingestFilePath}
                    onChange={(event) => {
                      setIngestFilePath(event.target.value);
                      clearIngestFilePickedPaths();
                    }}
                    placeholder={
                      ingestFilePickedPaths.length > 0
                        ? `已选 ${ingestFilePickedPaths.length} 个文件`
                        : defaultIngestFilePath
                    }
                    disabled={ingestFilePickedPaths.length > 0}
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    className="dev-panel__button path-pick-btn"
                    onClick={() => void pickIngestFiles()}
                    disabled={!isTauri}
                    title="选择文件（支持多选）"
                  >
                    📄
                  </button>
                </div>
                {ingestFilePickedPaths.length > 0 ? (
                  <div className="picked-files">
                    <div className="picked-files__head">
                      <span>{ingestFilePickedPaths.length} 个文件</span>
                      <button
                        type="button"
                        className="dev-panel__button picked-files__clear"
                        onClick={clearIngestFilePickedPaths}
                      >
                        清除
                      </button>
                    </div>
                    <ul className="picked-files__list">
                      {ingestFilePickedPaths.map((path) => (
                        <li key={path} className="picked-files__item" title={path}>
                          {path.split(/[/\\]/).pop()}
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : (
                  <p className="dev-panel__hint">
                    md · txt · pdf · docx · pptx · png · jpg · bmp · webp · tif
                  </p>
                )}
              </div>
              <div className="dev-panel__field">
                <label className="dev-panel__label" htmlFor="ingest-file-ocr-provider">
                  OCR
                </label>
                <select
                  id="ingest-file-ocr-provider"
                  className="dev-panel__input"
                  value={ingestFileOcrProvider}
                  onChange={(event) => {
                    const provider: OcrProvider =
                      event.target.value === "paddle" ? "paddle" : "tesseract";
                    void setIngestFileOcrProvider(provider);
                  }}
                >
                  <option value="tesseract">{ocrProviderLabels.tesseract}</option>
                  <option value="paddle">{ocrProviderLabels.paddle}</option>
                </select>
              </div>
              <div className="ingest-card__footer">
                <button
                  type="button"
                  className="dev-panel__button dev-panel__button--accent"
                  onClick={() => void onFileIngest()}
                  disabled={!isTauri || devAction !== null}
                >
                  {devAction === "ingest_file" ? "摄入中..." : "文件摄入"}
                </button>
                <button
                  type="button"
                  className="dev-panel__button"
                  disabled={
                    !isTauri ||
                    queueEnqueueing ||
                    (ingestFilePickedPaths.length === 0 && !ingestFilePath.trim())
                  }
                  onClick={() => {
                    const paths =
                      ingestFilePickedPaths.length > 0
                        ? ingestFilePickedPaths
                        : [ingestFilePath.trim()];
                    const validPaths = paths.filter(Boolean);
                    if (validPaths.length === 0) return;
                    void enqueueFiles(validPaths);
                  }}
                >
                  {queueEnqueueing ? "入队中..." : "加入队列"}
                </button>
              </div>
            </div>
          </div>

          <p className="dev-panel__hint">
            {isTauri
              ? "文件摄入自动按扩展名路由，图片/PDF 默认 tesseract OCR，失败自动回退。成功后刷新概览与日志。"
              : "浏览器预览模式下按钮保持禁用，仅用于界面预览。"}
          </p>
        </div>
      </section>

      <section className="panel" style={{ marginTop: "16px" }}>
        <div className="section-head">
          <h2>网页剪藏扩展</h2>
          <span
            style={{
              display: "inline-block",
              padding: "2px 8px",
              borderRadius: "12px",
              background:
                clipServerOnline === false
                  ? "var(--color-warning-bg, #fffbe6)"
                  : "var(--color-success-bg, #ecfdf5)",
              color:
                clipServerOnline === false
                  ? "var(--color-warning-text, #7c5a00)"
                  : "var(--color-success, #065f46)",
              fontSize: "12px",
              fontWeight: 600,
            }}
          >
            {clipServerOnline === false
              ? "⚠ 服务未启动"
              : `● 服务运行中 :${clipServerPort}`}
          </span>
        </div>
        <p
          style={{
            marginBottom: "12px",
            color: "var(--color-text-2, #555)",
            fontSize: "13px",
          }}
        >
          浏览器扩展可将网页一键剪藏到知识库，并自动触发摄入。剪藏内容保存至{" "}
          <code>raw/clips/</code>。
        </p>
        <div
          style={{
            background: "var(--color-bg-2, #f5f5f5)",
            borderRadius: "8px",
            padding: "12px 16px",
          }}
        >
          <p style={{ fontWeight: 600, marginBottom: "8px", fontSize: "13px" }}>
            安装步骤（Chrome / Edge）：
          </p>
          <ol style={{ paddingLeft: "18px", fontSize: "13px", lineHeight: "2" }}>
            <li>打开浏览器，访问 <code>chrome://extensions</code></li>
            <li>启用右上角「<strong>开发者模式</strong>」</li>
            <li>点击「<strong>加载已解压的扩展程序</strong>」</li>
            <li>选择项目根目录下的 <code>extension/</code> 文件夹</li>
            <li>扩展安装后点击工具栏中的 📚 图标即可剪藏当前页面</li>
          </ol>
        </div>
        <p style={{ marginTop: "10px", fontSize: "12px", color: "var(--color-text-3, #888)" }}>
          ℹ️ 确保应用保持运行，扩展通过 HTTP 与本应用通信（端口 {clipServerPort}）
        </p>
      </section>

      <section className="panel">
        <div className="section-head">
          <h2>最近日志</h2>
          <span className="section-head__hint">
            {logs.length ? `最近 ${logs.length} 条` : "暂无日志"}
          </span>
        </div>
        {logs.length ? (
          <div className="log-list">
            {logs.map((log) => (
              <article
                key={log.id}
                className={`log-item log-item--${log.level.toLowerCase()}`}
              >
                <div className="log-item__head">
                  <span className="log-item__level">{formatLogLevel(log.level)}</span>
                  <time dateTime={log.created_at}>{formatLintCheckedAt(log.created_at)}</time>
                </div>
                <p>{log.message}</p>
              </article>
            ))}
          </div>
        ) : (
          <p className="empty-state">
            {isTauri ? "后端尚未返回最近日志。" : "浏览器预览模式下不加载 Tauri 日志。"}
          </p>
        )}
      </section>
    </>
  );
}
