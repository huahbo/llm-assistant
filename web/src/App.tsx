import { type MouseEvent as ReactMouseEvent, useEffect, useRef, useState, useCallback } from "react";
import InboxModule from "./modules/inbox/InboxModule";
import AskModule from "./modules/ask/AskModule";
import AgentStudio from "./modules/agent/AgentStudio";
import ChatModule from "./modules/chat/ChatModule";
import LintModule from "./modules/lint/LintModule";
import OperationsModule from "./modules/operations/OperationsModule";
import ResearchModule from "./modules/research/ResearchModule";
import SettingsModule from "./modules/settings/SettingsModule";
import WikiModule, { type WikiModuleHandle } from "./modules/wiki/WikiModule";
import GraphModule from "./modules/graph/GraphModule";
import DiscoveryModule from "./modules/discovery/DiscoveryModule";
import CommandPalette from "./modules/palette/CommandPalette";
import { useVault } from "./contexts/VaultContext";
import { useMode } from "./contexts/ModeContext";
import { useToast } from "./contexts/ToastContext";
import {
  fetchDefaultPaths,
  fetchWikiPageDetail,
  saveWikiPage,
  isTauriRuntime,
  formatLlmStatusSummary,
} from "./tauri-client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import appLogo from "./assets/LLM_Wiki.png";
import type {
  AppOverview,
  LlmStatus,
  LogEntry,
  ModuleId,
  WikiPageItem,
} from "./types";

import { readDropModeFromStorage, writeDropModeToStorage, type DropMode } from "./ask-utils";
import { loadAppData } from "./app-data";
import ErrorBoundary from "./components/ErrorBoundary";
import SkeletonPane from "./components/SkeletonPane";

export default function App() {
  const [theme, setTheme] = useState<"light" | "dark">(() =>
    (localStorage.getItem("theme") as "light" | "dark") ??
    (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
  );
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("theme", theme);
  }, [theme]);
  const toggleTheme = () => setTheme((t) => (t === "dark" ? "light" : "dark"));

  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [pages, setPages] = useState<WikiPageItem[]>([]);
  const [llmStatus, setLlmStatus] = useState<LlmStatus | null>(null);
  const [llmStatusLoaded, setLlmStatusLoaded] = useState(false);
  const [appReady, setAppReady] = useState(false);
  const { statusMessage, setStatusMessage } = useToast();
  const { vaultPath, setVaultPath } = useVault();
  const [dropMode, setDropMode] = useState<DropMode>(() => readDropModeFromStorage());
  // Wiki 模块通过 ref 暴露 openPage 方法，供跨模块调用
  const wikiModuleRef = useRef<WikiModuleHandle | null>(null);

  const [paletteOpen, setPaletteOpen] = useState(false);

  // 全局 Ctrl+K 打开命令面板
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  // 当前激活的导航模块（来自 ModeContext）
  const { activeModule, navigateTo: setActiveModule } = useMode();
  // requestedOperationsTab: App 导航到 operations 时请求的 tab（由 OperationsModule 消费后保持自管）
  const [requestedOperationsTab, setRequestedOperationsTab] = useState<"queue" | "stats" | undefined>(undefined);
  const [agentDebugOpen, setAgentDebugOpen] = useState(false);
  // ── 面板拖拽分割 ──────────────────────────────────────────────
  const [sidebarWidth, setSidebarWidth] = useState(220);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const sidebarDragRef = useRef({ active: false, startX: 0, startW: 220 });

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (sidebarDragRef.current.active) {
        const delta = e.clientX - sidebarDragRef.current.startX;
        setSidebarWidth(Math.max(160, Math.min(400, sidebarDragRef.current.startW + delta)));
      }
    };
    const onUp = () => {
      sidebarDragRef.current.active = false;
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      document.body.classList.remove('split-dragging');
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
    return () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    };
  }, []);

  // 窗口激活/失活时切换 CSS class，使顶部 border-top 颜色跟随 DWM 边框变化
  useEffect(() => {
    if (!isTauriRuntime()) return;
    const win = getCurrentWindow();
    // 初始化：先查询一次当前焦点状态
    void win.isFocused().then((focused) => {
      document.documentElement.classList.toggle('window-focused', focused);
    });
    const unlisten = win.onFocusChanged(({ payload: focused }) => {
      document.documentElement.classList.toggle('window-focused', focused);
    });
    return () => { void unlisten.then((fn) => fn()); };
  }, []);
  // ─────────────────────────────────────────────────────────────

  useEffect(() => {
    let cancelled = false;

    const load = async () => {
      const [data, defaultPaths] = await Promise.all([
        loadAppData(),
        fetchDefaultPaths(),
      ]);

      if (!cancelled) {
        setOverview(data.overview);
        setLogs(data.logs);
        setPages(data.pages);
        if (defaultPaths) {
          setVaultPath(defaultPaths.vault_path);
        }
        setLlmStatus(data.llmStatus);
        setLlmStatusLoaded(true);
        setAppReady(true);
      }
    };

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const refreshAppData = async (_options?: { includeGraph?: boolean }) => {
    const data = await loadAppData();
    setOverview(data.overview);
    setLogs(data.logs);
    setPages(data.pages);
    setLlmStatus(data.llmStatus);
    setLlmStatusLoaded(true);
  };

  /** 跨模块打开 Wiki 页面（委托给 WikiModule 的 ref handle） */
  const handleOpenWikiPage = async (pagePath: string) => {
    await wikiModuleRef.current?.openPage(pagePath);
  };

  const llmStatusSummary = llmStatus ? formatLlmStatusSummary(llmStatus) : null;
  const llmAvailabilityText = !isTauriRuntime()
    ? "浏览器预览"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.availabilityText
      : "加载中...";
  const llmModelText = !isTauriRuntime()
    ? "未连接 Tauri"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.modelText
      : "加载中...";
  const llmAddressText = !isTauriRuntime()
    ? "未连接 Tauri"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.addressText
      : "加载中...";
  const llmHintText = !isTauriRuntime()
    ? "浏览器预览模式下无法读取本地 LLM 状态。"
    : llmStatusLoaded && llmStatusSummary
      ? llmStatusSummary.hintText
      : "正在读取 LLM 状态...";

  const handleCreateLintTargetPage = async (targetTitle: string) => {
    try {
      setStatusMessage(`正在创建页面：${targetTitle}...`);
      await saveWikiPage(targetTitle, `# ${targetTitle}\n`);
      setStatusMessage(`页面 ${targetTitle} 创建成功！`);
      await refreshAppData();
    } catch (error) {
      console.error("创建页面失败:", error);
      setStatusMessage("页面创建失败，请重试。");
    }
  };

  const handleOpenLintPatchPage = async (path: string) => {
    setActiveModule("wiki");
    await handleOpenWikiPage(path);
  };

  const handleNavModuleSelect = (moduleId: ModuleId) => {
    setActiveModule(moduleId);
  };

  const handleOpenResearchWikiPage = (path: string) => {
    fetchWikiPageDetail(path)
      .then((detail) => {
        if (detail) {
          setActiveModule("wiki");
        }
      })
      .catch(() => {});
  };

  // 侧边栏按"核心 / 运行 / 系统"分组，运行与系统下沉到底。
  const navGroups: Array<{
    id: string;
    title: string;
    items: Array<{ id: ModuleId; icon: string; label: string }>;
    isolated?: boolean;
  }> = [
    {
      id: "core",
      title: "核心",
      items: [
        { id: "chat", icon: "💬", label: "对话" },
        { id: "agent", icon: "🤖", label: "Agent" },
        { id: "ask", icon: "🔍", label: "Ask" },
        { id: "wiki", icon: "📄", label: "Wiki" },
        { id: "lint", icon: "🧹", label: "Lint" },
        { id: "graph", icon: "🌐", label: "图谱" },
        { id: "research", icon: "🔬", label: "研究" },
        { id: "inbox", icon: "📊", label: "概览" },
      ],
    },
    {
      id: "operations",
      title: "运行",
      items: [
        { id: "operations", icon: "📦", label: "运行" },
      ],
    },
    {
      id: "system",
      title: "系统",
      items: [
        { id: "discovery", icon: "🧩", label: "MCP 市场" },
        { id: "settings", icon: "🛠️", label: "设置" },
      ],
    },
  ];

  const handleWindowControl = useCallback(async (action: "minimize" | "toggleMaximize" | "close") => {
    if (!isTauriRuntime()) {
      return;
    }
    try {
      const currentWindow = getCurrentWindow();
      if (action === "minimize") {
        await currentWindow.minimize();
        return;
      }
      if (action === "toggleMaximize") {
        await currentWindow.toggleMaximize();
        return;
      }
      await currentWindow.close();
    } catch (error) {
      console.warn("窗口控制操作失败。", error);
      const message = error instanceof Error ? error.message : String(error);
      setStatusMessage(`窗口操作失败：${message}`);
    }
  }, []);

  const handleTitlebarDoubleClick = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest(".window-titlebar__actions")) {
      return;
    }
    void handleWindowControl("toggleMaximize");
  }, [handleWindowControl]);

  const tauriRuntime = isTauriRuntime();

  return (
    <div className={`app-root${tauriRuntime ? " app-root--tauri" : ""}`}>
      {tauriRuntime ? (
        <header
          className="window-titlebar"
          onDoubleClick={handleTitlebarDoubleClick}
        >
          <div
            className="window-titlebar__drag-region"
            data-tauri-drag-region
          >
            <div className="window-titlebar__brand">
              <div className="window-titlebar__logo" aria-hidden="true">
                <img className="window-titlebar__logo-image" src={appLogo} alt="" />
              </div>
              <span className="window-titlebar__title">
                LLM Wiki
              </span>
            </div>
          </div>
          <div className="window-titlebar__drag-spacer" data-tauri-drag-region />
          <div className="window-titlebar__actions">
            <button
              type="button"
              className="window-titlebar__action-btn window-titlebar__action-btn--theme"
              aria-label={theme === "dark" ? "切换浅色主题" : "切换暗黑主题"}
              title={theme === "dark" ? "切换浅色主题" : "切换暗黑主题"}
              onClick={toggleTheme}
            >
              {theme === "dark" ? (
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
                </svg>
              ) : (
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
                </svg>
              )}
            </button>
            <button
              type="button"
              className="window-titlebar__action-btn window-titlebar__action-btn--minimize"
              aria-label="最小化窗口"
              onClick={() => {
                void handleWindowControl("minimize");
              }}
            >
              <span className="window-titlebar__action-glyph" aria-hidden="true">—</span>
            </button>
            <button
              type="button"
              className="window-titlebar__action-btn window-titlebar__action-btn--maximize"
              aria-label="最大化或还原窗口"
              onClick={() => {
                void handleWindowControl("toggleMaximize");
              }}
            >
              <span className="window-titlebar__action-glyph window-titlebar__action-glyph--maximize" aria-hidden="true" />
            </button>
            <button
              type="button"
              className="window-titlebar__action-btn window-titlebar__action-btn--close"
              aria-label="关闭窗口"
              onClick={() => {
                void handleWindowControl("close");
              }}
            >
              <span className="window-titlebar__action-glyph" aria-hidden="true">✕</span>
            </button>
          </div>
        </header>
      ) : null}
      <div className="app-shell">
      {/* 侧边栏导航 */}
      <nav
        className={`sidebar${sidebarCollapsed ? " sidebar--collapsed" : ""}`}
        style={{ width: sidebarCollapsed ? 52 : sidebarWidth }}
      >
        <div className="sidebar__brand">
          <div className="sidebar__brand-logo" aria-hidden="true">
            <img className="sidebar__brand-logo-image" src={appLogo} alt="" />
          </div>
          {!sidebarCollapsed && <span className="sidebar__brand-name">LLM Wiki</span>}
        </div>
        <div className="sidebar__nav">
          {navGroups.map((group) => (
            <section
              key={group.id}
              className={`sidebar__nav-group${group.isolated ? " sidebar__nav-group--isolated" : ""}`}
            >
              {!sidebarCollapsed && <header className="sidebar__nav-group-title">{group.title}</header>}
              <ul className="sidebar__nav-group-list">
                {group.items.map((item) => (
                  <li key={item.id}>
                    <button
                      type="button"
                      className={`sidebar__nav-item${activeModule === item.id ? " sidebar__nav-item--active" : ""}`}
                      title={sidebarCollapsed ? item.label : undefined}
                      onClick={() => {
                        handleNavModuleSelect(item.id);
                      }}
                    >
                      <span className="sidebar__nav-icon">{item.icon}</span>
                      {!sidebarCollapsed && <span className="sidebar__nav-label">{item.label}</span>}
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </div>
        <div className="sidebar__footer">
          <button
            type="button"
            className="sidebar__collapse-btn"
            title={sidebarCollapsed ? "展开侧边栏" : "收起侧边栏"}
            onClick={() => setSidebarCollapsed((v) => !v)}
          >
            {sidebarCollapsed ? "▶" : "◀"}
          </button>
          {!sidebarCollapsed && (
            <div className="sidebar__llm-status">
              <span
                className={`sidebar__llm-dot${llmStatus?.available ? " sidebar__llm-dot--ok" : " sidebar__llm-dot--off"}`}
              />
              <span className="sidebar__llm-label">{llmModelText}</span>
            </div>
          )}
        </div>
      </nav>
      {/* 侧边栏 / 主内容 分割拖拽条 */}
      {!sidebarCollapsed && (
        <div
          className="split-handle"
          onMouseDown={(e) => {
            e.preventDefault();
            sidebarDragRef.current = { active: true, startX: e.clientX, startW: sidebarWidth };
            document.body.style.cursor = 'col-resize';
            document.body.style.userSelect = 'none';
            document.body.classList.add('split-dragging');
          }}
        />
      )}

      {/* 主内容区 */}
      <div className="main-content">
        {statusMessage ? (
          <div className="status-bar">
            <span>{statusMessage}</span>
            <button
              type="button"
              className="status-bar__close"
              onClick={() => setStatusMessage("")}
            >
              ✕
            </button>
          </div>
        ) : null}

        <div className={`module-viewport${activeModule === "ask" ? " module-viewport--ask" : ""}${activeModule === "agent" ? ` module-viewport--agent${agentDebugOpen ? " module-viewport--agent-debug" : ""}` : ""}${activeModule === "chat" ? " module-viewport--chat" : ""}`}>
          {!appReady && <SkeletonPane rows={8} />}
          {/* ---- Chat 模块 ---- */}
          {appReady && activeModule === "chat" && <ErrorBoundary label="Chat"><ChatModule /></ErrorBoundary>}
          {/* ---- 概览模块 ---- */}
          {appReady && activeModule === "inbox" && (
            <ErrorBoundary label="Inbox">
              <InboxModule
                overview={overview}
                pagesCount={pages.length}
                logs={logs}
                dropMode={dropMode}
                onRefreshAppData={refreshAppData}
                navigateTo={setActiveModule}
                llmAvailabilityText={llmAvailabilityText}
                llmModelText={llmModelText}
                llmAddressText={llmAddressText}
                llmHintText={llmHintText}
              />
            </ErrorBoundary>
          )}
          {/* ---- Wiki 模块 ---- */}
          {appReady && activeModule === "wiki" && (
            <ErrorBoundary label="Wiki">
              <WikiModule
                ref={wikiModuleRef}
                pages={pages}
                onPagesChange={setPages}
              />
            </ErrorBoundary>
          )}

          {/* ---- Ask 模块 ---- */}
          {appReady && activeModule === "ask" && (
            <ErrorBoundary label="Ask"><AskModule onOpenWikiPage={handleOpenWikiPage} /></ErrorBoundary>
          )}

          {/* ---- Lint 模块 ---- */}
          {appReady && activeModule === "lint" && (
            <ErrorBoundary label="Lint">
              <LintModule
                onRefreshAppData={refreshAppData}
                onCreateBrokenWikiLinkPage={handleCreateLintTargetPage}
                onOpenPatchPage={handleOpenLintPatchPage}
              />
            </ErrorBoundary>
          )}

          {/* ---- 图谱模块 ---- */}
          {appReady && activeModule === "graph" && (
            <ErrorBoundary label="Graph">
              <GraphModule handleOpenWikiPage={handleOpenWikiPage} />
            </ErrorBoundary>
          )}

          {/* ---- Discovery（MCP 市场）模块 ---- */}
          {appReady && activeModule === "discovery" && (
            <ErrorBoundary label="Discovery">
              <DiscoveryModule />
            </ErrorBoundary>
          )}

          {/* ---- Settings 模块 ---- */}
          {appReady && activeModule === "settings" && (
            <ErrorBoundary label="Settings">
              <SettingsModule
                onRefreshAppData={refreshAppData}
                dropMode={dropMode}
                onDropModeChange={(mode) => {
                  setDropMode(mode);
                  writeDropModeToStorage(mode);
                }}
              />
            </ErrorBoundary>
          )}
          {/* ---- 运行模块（队列 + 统计合并） ---- */}
          {appReady && activeModule === "operations" && (
            <ErrorBoundary label="Operations">
              <OperationsModule
                requestedTab={requestedOperationsTab}
                navigateTo={setActiveModule}
              />
            </ErrorBoundary>
          )}
          {/* ---- Deep Research 模块 ---- */}
          {appReady && activeModule === "research" && (
            <ErrorBoundary label="Research">
              <ResearchModule onOpenWikiPage={handleOpenResearchWikiPage} />
            </ErrorBoundary>
          )}
          {/* ---- Agent Studio 模块 ---- */}
          {appReady && activeModule === "agent" && (
            <ErrorBoundary label="Agent Studio">
              <AgentStudio onOpenWikiPage={handleOpenWikiPage} onDebugToggle={setAgentDebugOpen} />
            </ErrorBoundary>
          )}
        </div>
      </div>
    </div>
    <CommandPalette
      open={paletteOpen}
      onClose={() => setPaletteOpen(false)}
      onOpenWikiPage={handleOpenWikiPage}
    />
    </div>
  );
}
