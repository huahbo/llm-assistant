import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { formatLogLevel } from "../../app-formatters";
import { formatLintCheckedAt } from "../../lint-utils";
import { formatBackendMode } from "../../app-formatters";
import {
  fetchDefaultPaths,
  fetchOcrConfig,
  saveOcrConfig,
  getClipServerStatus,
  get_outbox_events,
  enqueueIngest,
  previewIngestFile,
  applyIngestPreview,
  ingestFile,
  initVault,
  initVaultWithTemplate,
  isTauriRuntime,
  listenProgress,
  pickFiles,
  pickFolder,
  pickSaveFile,
  setBackendMode,
  type OcrProvider,
} from "../../tauri-client";
import { formatPdfIngestErrorMessage, buildTemplateInitPreview, parseDroppedIngestPaths } from "../../ingest-utils";
import { isOcrProvider, readOcrProviderFromStorage, writeOcrProviderToStorage } from "../../ask-utils";
import { useToast } from "../../contexts/ToastContext";
import { useMode } from "../../contexts/ModeContext";
import { useVault } from "../../contexts/VaultContext";
import { mergeRecentVaultPaths } from "../../vault-utils";
import { templates, getTemplate } from "../../templates";
import type {
  AppOverview,
  BackendAppMode,
  IngestPreview,
  LogEntry,
  ModeId,
  ModuleId,
  WikiTemplate,
} from "../../types";
import type { DropMode } from "../../ask-utils";
import type { TemplateInitPreview } from "../../ingest-utils";

// ---- local type aliases ----
type DevAction = "init_vault" | "ingest_markdown" | "ingest_pdf" | "ingest_file" | "ingest_url";

type ModeOption = {
  id: ModeId;
  label: string;
  isActive: boolean;
};

// ---- constants ----
const defaultVaultPath = "vault";
const CLIP_SERVER_PORT = 19827;

const ocrProviderLabels: Record<OcrProvider, string> = {
  tesseract: "tesseract（本地默认）",
  paddle: "paddle（高精度）",
};

const modeIdToBackendMode: Record<ModeId, BackendAppMode> = {
  hybrid: "Hybrid",
  "strict-local": "StrictLocal",
};

const backendModeToModeId: Record<BackendAppMode, ModeId> = {
  Hybrid: "hybrid",
  StrictLocal: "strict-local",
};

const modeIdLabels: Record<ModeId, string> = {
  hybrid: "Hybrid（自由模式）",
  "strict-local": "Strict Local（仅本地）",
};

const modeIdDescriptions: Record<ModeId, string> = {
  hybrid: "允许本地与云 Provider 按任务路由，适合常规工作流。",
  "strict-local": "只允许本地 Ollama，自动拦截云调用与外部模型请求。",
};

// ---- prop types ----
export type InboxModuleProps = {
  overview: AppOverview | null;
  pagesCount: number;
  logs: LogEntry[];
  dropMode: DropMode;
  onRefreshAppData: () => Promise<void>;
  navigateTo: (id: ModuleId) => void;
  llmAvailabilityText: string;
  llmModelText: string;
  llmAddressText: string;
  llmHintText: string;
};

// Imperative handle exposed to App.tsx for drag-drop integration
export type InboxModuleHandle = {
  handleDroppedFiles: (paths: string[]) => void;
};

const InboxModule = forwardRef<InboxModuleHandle, InboxModuleProps>(function InboxModule(
  {
    overview,
    pagesCount,
    logs,
    dropMode,
    onRefreshAppData,
    navigateTo,
    llmAvailabilityText,
    llmModelText,
    llmAddressText,
    llmHintText,
  },
  ref,
) {
  const { setStatusMessage } = useToast();
  const { activeModule } = useMode();
  const { vaultPath, setVaultPath, recentVaultPaths, setRecentVaultPaths } = useVault();

  // ---- state ----
  const [switchingMode, setSwitchingMode] = useState<ModeId | null>(null);
  const [devAction, setDevAction] = useState<DevAction | null>(null);
  const [selectedTemplateId, setSelectedTemplateId] = useState<string>("general");
  const selectedTemplate = useMemo<WikiTemplate>(() => {
    try {
      return getTemplate(selectedTemplateId);
    } catch {
      return templates[0];
    }
  }, [selectedTemplateId]);
  const templateInitPreview = useMemo<TemplateInitPreview>(
    () => buildTemplateInitPreview(selectedTemplate),
    [selectedTemplate],
  );

  const [ingestSourcePath, setIngestSourcePath] = useState("");
  const [ingestFilePath, setIngestFilePath] = useState("");
  const [ingestFilePickedPaths, setIngestFilePickedPaths] = useState<string[]>([]);
  const [ingestFileOcrProvider, setIngestFileOcrProvider] = useState<OcrProvider>(
    () => readOcrProviderFromStorage(),
  );
  const [ingestUrlInput, setIngestUrlInput] = useState("");
  const [clipServerOnline, setClipServerOnline] = useState<boolean | null>(null);
  const [ingestDragActive, setIngestDragActive] = useState(false);
  const [outboxLastId, setOutboxLastId] = useState(0);
  const [outboxInitialized, setOutboxInitialized] = useState(false);
  const [ingesting, setIngesting] = useState(false);
  const [queueEnqueueing, setQueueEnqueueing] = useState(false);
  const [ingestPreviewDialog, setIngestPreviewDialog] = useState<IngestPreview | null>(null);
  const ingestPreviewResolverRef = useRef<((approved: boolean) => void) | null>(null);

  // ---- effects: data loading ----

  // Check clip server status on mount
  useEffect(() => {
    getClipServerStatus()
      .then((s) => setClipServerOnline(s === "running"))
      .catch(() => setClipServerOnline(false));
  }, []);

  // Load default paths
  useEffect(() => {
    fetchDefaultPaths()
      .then((paths) => {
        if (paths) {
          setIngestSourcePath(paths.ingest_source_path);
        }
      })
      .catch(() => {
        // fallback to defaults
      });
  }, []);

  // Load OCR config from backend
  useEffect(() => {
    fetchOcrConfig()
      .then((provider) => {
        if (provider && isOcrProvider(provider)) {
          setIngestFileOcrProvider(provider);
          writeOcrProviderToStorage(provider);
        }
      })
      .catch(() => {
        // fallback to localStorage value
      });
  }, []);

  // Fast-forward outboxLastId on mount to skip historical events
  useEffect(() => {
    const init = async () => {
      try {
        const events = await get_outbox_events({ last_id: 0 });
        if (events && events.length > 0) {
          const maxId = events.reduce((max, e) => Math.max(max, e.id), 0);
          setOutboxLastId(maxId);
        }
      } catch (err) {
        console.error("初始化 outbox 快进失败:", err);
      }
      setOutboxInitialized(true);
    };
    void init();
  }, []);

  // Outbox polling
  useEffect(() => {
    if (!outboxInitialized) return;
    let timerId: ReturnType<typeof globalThis.setInterval> | null = null;
    let polling = false;

    const poll = async () => {
      if (polling) return;
      polling = true;
      try {
        const events = await get_outbox_events({ last_id: outboxLastId });
        if (events && events.length > 0) {
          let shouldRefresh = false;
          let newIngesting = ingesting;
          let maxId = outboxLastId;

          for (const event of events) {
            maxId = Math.max(maxId, event.id);
            const type = event.event_type;
            if (
              type === "ingest_completed" ||
              type === "ingest_failed" ||
              type === "wiki_page_deleted" ||
              type === "wiki_page_renamed" ||
              type === "query_saved_to_wiki"
            ) {
              shouldRefresh = true;
            }
            if (type === "ingest_started") {
              newIngesting = true;
            } else if (type === "ingest_completed" || type === "ingest_failed") {
              newIngesting = false;
            }
          }

          if (shouldRefresh) {
            void onRefreshAppData();
          }
          if (newIngesting !== ingesting) {
            setIngesting(newIngesting);
          }
          if (maxId > outboxLastId) {
            setOutboxLastId(maxId);
          }
        }
      } catch (err) {
        console.error("Outbox 轮询失败:", err);
      } finally {
        polling = false;
      }
    };

    timerId = globalThis.setInterval(poll, 3000);
    return () => {
      if (timerId) globalThis.clearInterval(timerId);
    };
  }, [activeModule, outboxLastId, ingesting, outboxInitialized, onRefreshAppData]);

  // Drag-drop handler
  useEffect(() => {
    if (!isTauriRuntime()) return;

    let disposed = false;
    let unlisten: (() => void) | null = null;
    void import("@tauri-apps/api/webview")
      .then(async ({ getCurrentWebview }) => {
        if (disposed) return;
        unlisten = await getCurrentWebview().onDragDropEvent((event) => {
          if (event.payload.type === "enter" || event.payload.type === "over") {
            setIngestDragActive(true);
            return;
          }
          if (event.payload.type === "leave") {
            setIngestDragActive(false);
            return;
          }
          setIngestDragActive(false);
          if (event.payload.type !== "drop") return;
          if (devAction === "ingest_file" || ingesting) {
            setStatusMessage("当前正在摄入中，已忽略本次拖拽文件。");
            return;
          }
          const parsed = parseDroppedIngestPaths(event.payload.paths);
          if (parsed.accepted.length === 0) {
            setStatusMessage("未检测到可摄入文件（支持 md/pdf/docx/pptx/txt/图片）。");
            return;
          }
          const ignoredMsg = parsed.rejected.length > 0 ? `，忽略 ${parsed.rejected.length} 项` : "";
          const duplicateMsg = parsed.duplicateCount > 0 ? `，去重 ${parsed.duplicateCount} 项` : "";
          if (dropMode === "queue") {
            setStatusMessage(`已接收拖拽文件 ${parsed.accepted.length} 项${ignoredMsg}${duplicateMsg}，加入队列...`);
            Promise.all(parsed.accepted.map((p) => enqueueIngest("file", p)))
              .then(() => {
                navigateTo("operations");
              })
              .catch((err: unknown) => {
                console.error("拖拽入队失败:", err);
                setStatusMessage("拖拽入队失败，请检查后端日志。");
              });
          } else {
            navigateTo("inbox");
            setIngestFilePickedPaths(parsed.accepted);
            setIngestFilePath(parsed.accepted[0] ?? "");
            setStatusMessage(`已接收拖拽文件 ${parsed.accepted.length} 项${ignoredMsg}${duplicateMsg}，开始摄入...`);
            void runIngestFilePaths(parsed.accepted, "drag", true);
          }
        });
      })
      .catch((error) => {
        console.warn("注册拖拽摄入监听失败。", error);
      });

    return () => {
      disposed = true;
      if (unlisten) unlisten();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [devAction, dropMode, ingestFileOcrProvider, ingesting]);

  // Cleanup preview resolver on unmount
  useEffect(() => {
    return () => {
      if (ingestPreviewResolverRef.current) {
        ingestPreviewResolverRef.current(false);
        ingestPreviewResolverRef.current = null;
      }
    };
  }, []);

  // ---- handlers ----

  const requestIngestPreviewApproval = useCallback((preview: IngestPreview) => {
    return new Promise<boolean>((resolve) => {
      ingestPreviewResolverRef.current = resolve;
      setIngestPreviewDialog(preview);
    });
  }, []);

  const closeIngestPreviewDialog = useCallback((approved: boolean) => {
    const resolver = ingestPreviewResolverRef.current;
    ingestPreviewResolverRef.current = null;
    setIngestPreviewDialog(null);
    if (resolver) {
      resolver(approved);
    }
  }, []);

  const handleModeSelect = async (modeId: ModeId) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法切换运行模式。");
      return;
    }
    if (!overview) return;
    const nextMode = modeIdToBackendMode[modeId];
    if (overview.mode === nextMode) return;
    setSwitchingMode(modeId);
    setStatusMessage("");
    try {
      const result = await setBackendMode(nextMode);
      if (!result) {
        setStatusMessage("当前环境不支持运行模式切换。");
        return;
      }
      await onRefreshAppData();
      setStatusMessage(`已切换到 ${formatBackendMode(result.current_mode)}。`);
    } catch (error) {
      console.error(error);
      setStatusMessage("模式切换失败，请稍后重试。");
    } finally {
      setSwitchingMode(null);
    }
  };

  const handleInitVault = async () => {
    setStatusMessage("收到初始化请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法初始化 Vault。");
      return;
    }
    const nextVaultPath = vaultPath.trim() || defaultVaultPath;
    setDevAction("init_vault");
    setStatusMessage("");
    try {
      let result;
      if (selectedTemplate.id === "general") {
        result = await initVault(nextVaultPath);
      } else {
        result = await initVaultWithTemplate(
          nextVaultPath,
          selectedTemplate.schema,
          selectedTemplate.purpose,
          selectedTemplate.extraDirs,
        );
      }
      if (!result) {
        setStatusMessage("当前环境不支持 Vault 初始化。");
        return;
      }
      await onRefreshAppData();
      const mergedRecent = mergeRecentVaultPaths(result.vault_path || nextVaultPath, recentVaultPaths);
      setRecentVaultPaths(mergedRecent);
      const createdCount = result.created_paths?.length ?? 0;
      setStatusMessage(
        `${result.message || `Vault 已初始化：${result.vault_path}`}\n模板：${selectedTemplate.name} · 创建 ${createdCount} 项`,
      );
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`Vault 初始化失败：${message}`);
    } finally {
      setDevAction(null);
    }
  };

  const handleDemoIngest = async () => {
    setStatusMessage("收到摄入请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行示例摄入。");
      return;
    }
    const nextSourcePath = ingestSourcePath.trim();
    setDevAction("ingest_markdown");
    setStatusMessage("摄入中...");
    let unlisten: (() => void) | null = null;
    try {
      try {
        unlisten = await listenProgress("ingest_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 ingest 进度事件失败，继续执行摄入流程。", error);
      }
      setStatusMessage("分析中，等待审核确认...");
      const preview = await previewIngestFile("markdown", nextSourcePath);
      if (!preview) {
        setStatusMessage("当前环境不支持摄入预览。");
        return;
      }
      const approved = await requestIngestPreviewApproval(preview);
      if (!approved) {
        setStatusMessage("已取消摄入。");
        return;
      }
      setStatusMessage("写入中...");
      const result = await applyIngestPreview(preview.preview_id);
      if (!result) {
        setStatusMessage("当前环境不支持示例摄入。");
        return;
      }
      await onRefreshAppData();
      const entitiesMsg =
        result.entities && result.entities.length > 0
          ? `\n提取实体：${result.entities.join("、")}`
          : "";
      const updatedMsg =
        result.updated_pages && result.updated_pages.length > 0
          ? `\n更新相关页面：${result.updated_pages.length} 个`
          : "";
      setStatusMessage(
        `${result.message || `已处理 ${result.source_path}`}${entitiesMsg}${updatedMsg}`,
      );
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`示例摄入失败：${message}`);
    } finally {
      if (unlisten) unlisten();
      setIngesting(false);
      setDevAction(null);
    }
  };

  const handleUrlIngest = async () => {
    setStatusMessage("收到 URL 摄入请求，正在调用后端...");
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行 URL 摄入。");
      return;
    }
    const trimmedUrl = ingestUrlInput.trim();
    if (!trimmedUrl) {
      setStatusMessage("请输入有效的 URL。");
      return;
    }
    setDevAction("ingest_url");
    setStatusMessage("摄入中...");
    let unlisten: (() => void) | null = null;
    try {
      try {
        unlisten = await listenProgress("ingest_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 ingest 进度事件失败，继续执行 URL 摄入流程。", error);
      }
      setStatusMessage("分析中，等待审核确认...");
      const preview = await previewIngestFile("url", trimmedUrl);
      if (!preview) {
        setStatusMessage("当前环境不支持摄入预览。");
        return;
      }
      const approved = await requestIngestPreviewApproval(preview);
      if (!approved) {
        setStatusMessage("已取消摄入。");
        return;
      }
      setStatusMessage("写入中...");
      const result = await applyIngestPreview(preview.preview_id);
      if (!result) {
        setStatusMessage("当前环境不支持 URL 摄入。");
        return;
      }
      await onRefreshAppData();
      const entitiesMsg =
        result.entities && result.entities.length > 0
          ? `\n提取实体：${result.entities.join("、")}`
          : "";
      const updatedMsg =
        result.updated_pages && result.updated_pages.length > 0
          ? `\n更新相关页面：${result.updated_pages.length} 个`
          : "";
      setStatusMessage(
        `${result.message || `已处理 ${trimmedUrl}`}${entitiesMsg}${updatedMsg}`,
      );
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`URL 摄入失败：${message}`);
    } finally {
      if (unlisten) unlisten();
      setIngesting(false);
      setDevAction(null);
    }
  };

  const runIngestFilePaths = async (
    pathsToIngest: string[],
    sourceLabel: "manual" | "drag",
    previewBeforeApply = false,
  ) => {
    if (!isTauriRuntime()) {
      setStatusMessage("浏览器预览模式下无法执行通用文件摄入。");
      return;
    }
    if (pathsToIngest.length === 0) {
      setStatusMessage("请选择或输入要摄入的文件路径（支持 md/pdf/docx/pptx/txt/图片）。");
      return;
    }
    setDevAction("ingest_file");
    setStatusMessage(sourceLabel === "drag" ? "检测到拖拽文件，摄入中..." : "摄入中...");
    let unlisten: (() => void) | null = null;
    try {
      try {
        unlisten = await listenProgress("ingest_progress", (payload) => {
          setStatusMessage(payload.message);
        });
      } catch (error) {
        console.warn("订阅 ingest 进度事件失败，继续执行通用文件摄入流程。", error);
      }
      let successCount = 0;
      for (const filePath of pathsToIngest) {
        const fileName = filePath.split(/[/\\]/).pop() ?? filePath;
        let result: Awaited<ReturnType<typeof ingestFile>>;
        if (previewBeforeApply) {
          setStatusMessage(`分析中 (${successCount + 1}/${pathsToIngest.length})：${fileName}`);
          const preview = await previewIngestFile("file", filePath, ingestFileOcrProvider);
          if (!preview) {
            setStatusMessage("当前环境不支持摄入预览。");
            return;
          }
          const approved = await requestIngestPreviewApproval(preview);
          if (!approved) {
            setStatusMessage("已取消摄入。");
            return;
          }
          setStatusMessage(`写入中 (${successCount + 1}/${pathsToIngest.length})：${fileName}`);
          result = await applyIngestPreview(preview.preview_id);
        } else {
          setStatusMessage(`摄入中 (${successCount + 1}/${pathsToIngest.length})：${fileName}`);
          result = await ingestFile(filePath, ingestFileOcrProvider);
        }
        if (!result) {
          setStatusMessage("当前环境不支持通用文件摄入。");
          return;
        }
        await onRefreshAppData();
        const entitiesMsg =
          result.entities && result.entities.length > 0
            ? `\n提取实体：${result.entities.join("、")}`
            : "";
        const updatedMsg =
          result.updated_pages && result.updated_pages.length > 0
            ? `\n更新相关页面：${result.updated_pages.length} 个`
            : "";
        if (pathsToIngest.length === 1) {
          setStatusMessage(`${result.message || `已处理 ${filePath}`}${entitiesMsg}${updatedMsg}`);
        }
        successCount++;
      }
      if (pathsToIngest.length > 1) {
        setStatusMessage(`摄入完成：${successCount}/${pathsToIngest.length} 个文件。`);
      }
      setIngestFilePickedPaths([]);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : String(error);
      const singleFilePath = pathsToIngest.length === 1 ? pathsToIngest[0] : "";
      const isSinglePdf = singleFilePath.toLowerCase().endsWith(".pdf");
      if (isSinglePdf && message.toLowerCase().includes("pdf")) {
        setStatusMessage(formatPdfIngestErrorMessage(message));
        return;
      }
      const normalizedMessage = message.toLowerCase();
      const isTesseractLanguageMissing =
        normalizedMessage.includes("缺少可用语言包")
        || normalizedMessage.includes("chi_sim")
        || normalizedMessage.includes("traineddata")
        || normalizedMessage.includes("failed loading language");
      if (isTesseractLanguageMissing) {
        setStatusMessage(
          "Tesseract 已安装，但缺少语言包（chi_sim/eng）。请安装语言包，或在 OCR 下拉切换到 PaddleOCR。",
        );
        return;
      }
      const isPrimaryProviderMissing =
        ingestFileOcrProvider === "paddle"
          ? (
            message.includes("未检测到 paddleocr 命令")
            || (normalizedMessage.includes("is not recognized") && normalizedMessage.includes("paddleocr"))
          )
          : (
            message.includes("未检测到 tesseract 命令")
            || (normalizedMessage.includes("is not recognized") && normalizedMessage.includes("tesseract"))
          );
      if (isPrimaryProviderMissing) {
        const guide =
          ingestFileOcrProvider === "paddle"
            ? "PaddleOCR 未安装，请运行：pip install paddleocr paddlepaddle"
            : "Tesseract 未安装，请从 https://github.com/UB-Mannheim/tesseract/wiki 下载安装后加入 PATH";
        setStatusMessage(`OCR 工具未找到：${guide}`);
        return;
      }
      setStatusMessage(`通用文件摄入失败：${message}`);
    } finally {
      if (unlisten) unlisten();
      if (ingestPreviewResolverRef.current) {
        ingestPreviewResolverRef.current(false);
        ingestPreviewResolverRef.current = null;
      }
      setIngestPreviewDialog(null);
      setIngesting(false);
      setDevAction(null);
    }
  };

  const handleFileIngest = async () => {
    setStatusMessage("收到通用文件摄入请求，正在调用后端...");
    const pathsToIngest =
      ingestFilePickedPaths.length > 0
        ? ingestFilePickedPaths
        : [ingestFilePath.trim()].filter(Boolean);
    await runIngestFilePaths(pathsToIngest, "manual", true);
  };

  // Expose imperative handle for parent (App.tsx drag integration)
  useImperativeHandle(ref, () => ({
    handleDroppedFiles(paths: string[]) {
      setIngestFilePickedPaths(paths);
      setIngestFilePath(paths[0] ?? "");
      void runIngestFilePaths(paths, "drag", true);
    },
  }));

  // Computed values for mode display
  const supportedModesText = overview
    ? overview.supported_modes.map(formatBackendMode).join(" / ")
    : "浏览器预览";
  const currentModeLabel = overview ? formatBackendMode(overview.mode) : "Browser Preview";
  const currentModeBadge = overview ? backendModeToModeId[overview.mode] : "—";
  const currentModeDescription = overview
    ? modeIdDescriptions[backendModeToModeId[overview.mode]]
    : "浏览器预览模式下不可切换运行策略。";
  const modeOptions: ModeOption[] = (["hybrid", "strict-local"] as ModeId[]).map((modeId) => ({
    id: modeId,
    label: modeIdLabels[modeId],
    isActive: Boolean(overview && backendModeToModeId[overview.mode] === modeId),
  }));

  const isTauri = isTauriRuntime();

  return (
    <>
      {ingestDragActive ? (
        <div className="status-bar status-bar--dragging">
          <span>释放鼠标即可开始摄入（支持 md/pdf/docx/pptx/txt/图片）。</span>
        </div>
      ) : null}

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
                onClick={() => void handleModeSelect(mode.id)}
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
                  onClick={() =>
                    pickFolder().then((path) => {
                      if (path) setVaultPath(path);
                    })
                  }
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
              onClick={() => void handleDemoIngest()}
              disabled={!isTauri || devAction !== null}
              title="用内置示例文件测试摄入流程"
            >
              {devAction === "ingest_markdown" ? "摄入中..." : "示例摄入"}
            </button>
            <button
              type="button"
              className="dev-panel__button dev-panel__vault-action"
              onClick={() => void handleInitVault()}
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
                  onClick={() => setRecentVaultPaths([])}
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
                    onClick={() => setVaultPath(path)}
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
                  onClick={() => void handleUrlIngest()}
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
                    setQueueEnqueueing(true);
                    enqueueIngest("url", ingestUrlInput.trim())
                      .then(() => {
                        navigateTo("operations");
                      })
                      .catch((error: unknown) => {
                        console.error("加入队列失败:", error);
                      })
                      .finally(() => {
                        setQueueEnqueueing(false);
                      });
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
                      setIngestFilePickedPaths([]);
                    }}
                    placeholder={
                      ingestFilePickedPaths.length > 0
                        ? `已选 ${ingestFilePickedPaths.length} 个文件`
                        : "输入文件路径，或点击右侧按钮选择"
                    }
                    disabled={ingestFilePickedPaths.length > 0}
                    spellCheck={false}
                  />
                  <button
                    type="button"
                    className="dev-panel__button path-pick-btn"
                    onClick={() =>
                      pickFiles({
                        multiple: true,
                        filters: [
                          {
                            name: "支持的文件",
                            extensions: ["md", "txt", "pdf", "docx", "pptx", "png", "jpg", "jpeg", "bmp", "webp", "tif", "tiff"],
                          },
                        ],
                      }).then((paths) => {
                        if (paths && paths.length > 0) {
                          setIngestFilePickedPaths(paths);
                          setIngestFilePath("");
                        }
                      })
                    }
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
                        onClick={() => setIngestFilePickedPaths([])}
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
                    setIngestFileOcrProvider(provider);
                    writeOcrProviderToStorage(provider);
                    void saveOcrConfig(provider);
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
                  onClick={() => void handleFileIngest()}
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
                    setQueueEnqueueing(true);
                    Promise.all(validPaths.map((path) => enqueueIngest("file", path)))
                      .then(() => {
                        navigateTo("operations");
                      })
                      .catch((error: unknown) => {
                        console.error("加入队列失败:", error);
                      })
                      .finally(() => {
                        setQueueEnqueueing(false);
                      });
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
                  ? "var(--warning-bg)"
                  : "var(--success-bg)",
              color:
                clipServerOnline === false
                  ? "var(--warning)"
                  : "var(--success)",
              fontSize: "12px",
              fontWeight: 600,
            }}
          >
            {clipServerOnline === false
              ? "⚠ 服务未启动"
              : `● 服务运行中 :${CLIP_SERVER_PORT}`}
          </span>
        </div>
        <p
          style={{
            marginBottom: "12px",
            color: "var(--text-muted)",
            fontSize: "13px",
          }}
        >
          浏览器扩展可将网页一键剪藏到知识库，并自动触发摄入。剪藏内容保存至{" "}
          <code>raw/clips/</code>。
        </p>
        <div
          style={{
            background: "var(--bg-muted)",
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
        <p style={{ marginTop: "10px", fontSize: "12px", color: "var(--text-muted)" }}>
          ℹ️ 确保应用保持运行，扩展通过 HTTP 与本应用通信（端口 {CLIP_SERVER_PORT}）
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

      {ingestPreviewDialog ? (
        <div
          className="ingest-preview-modal"
          role="dialog"
          aria-modal="true"
          aria-label="摄入分析卡"
          onClick={() => closeIngestPreviewDialog(false)}
        >
          <div className="ingest-preview-modal__panel" onClick={(event) => event.stopPropagation()}>
            <div className="ingest-preview-modal__head">
              <div>
                <h3>摄入分析卡</h3>
                <p>确认后写入 Wiki 与索引</p>
              </div>
              <button
                type="button"
                className="dev-panel__button"
                onClick={() => closeIngestPreviewDialog(false)}
              >
                取消
              </button>
            </div>
            <div className="ingest-preview-modal__source">
              <span>来源文件</span>
              <code>{ingestPreviewDialog.source_path}</code>
            </div>
            <div className="ingest-preview-modal__section">
              <h4>摘要</h4>
              <pre>{ingestPreviewDialog.summary?.trim() || "（未生成摘要）"}</pre>
            </div>
            <div className="ingest-preview-modal__columns">
              <section className="ingest-preview-modal__section">
                <h4>提取实体（{ingestPreviewDialog.entities.length}）</h4>
                {ingestPreviewDialog.entities.length > 0 ? (
                  <ul>
                    {ingestPreviewDialog.entities.map((entity) => (
                      <li key={entity}>{entity}</li>
                    ))}
                  </ul>
                ) : (
                  <p className="ingest-preview-modal__empty">暂无实体</p>
                )}
              </section>
              <section className="ingest-preview-modal__section">
                <h4>拟更新页面（{ingestPreviewDialog.updated_pages.length}）</h4>
                {ingestPreviewDialog.updated_pages.length > 0 ? (
                  <ul>
                    {ingestPreviewDialog.updated_pages.map((pagePath) => (
                      <li key={pagePath}>
                        <code>{pagePath}</code>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="ingest-preview-modal__empty">无关联页面更新</p>
                )}
              </section>
            </div>
            <div className="ingest-preview-modal__actions">
              <button
                type="button"
                className="dev-panel__button"
                onClick={() => closeIngestPreviewDialog(false)}
              >
                取消
              </button>
              <button
                type="button"
                className="dev-panel__button dev-panel__button--accent"
                onClick={() => closeIngestPreviewDialog(true)}
              >
                确认写入
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
});

export default InboxModule;
