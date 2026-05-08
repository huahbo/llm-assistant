import { useState, useEffect, useCallback } from "react";
import QueuePanel from "./QueuePanel";
import type { ModuleId, VaultStats } from "../../types";
import {
  isTauriRuntime,
  listIngestQueue,
  cancelIngestItem,
  retryIngestItem,
  deleteIngestItem,
  getVaultStats,
} from "../../tauri-client";
import { useMode } from "../../contexts/ModeContext";

type OperationsTab = "queue" | "stats";

type OperationsModuleProps = {
  /** When App.tsx navigates here after enqueueing, it can request a specific tab. */
  requestedTab?: OperationsTab;
  navigateTo: (id: ModuleId) => void;
};

const formatSourceType = (sourceType: string): string => {
  const map: Record<string, string> = {
    file: "本地文件",
    url: "网页链接",
    pdf: "PDF 文档",
    clipboard: "剪贴板",
  };
  return map[sourceType] ?? sourceType;
};

export default function OperationsModule({
  requestedTab,
  navigateTo,
}: OperationsModuleProps) {
  const { activeModule } = useMode();
  const isActive = activeModule === "operations";

  const [operationsTab, setOperationsTab] = useState<OperationsTab>(requestedTab ?? "queue");
  const [ingestQueue, setIngestQueue] = useState<
    import("../../types").IngestQueueItem[]
  >([]);
  const [vaultStats, setVaultStats] = useState<VaultStats | null>(null);
  const [vaultStatsLoading, setVaultStatsLoading] = useState(false);

  // When App.tsx changes requestedTab (e.g. after enqueueing), honour it.
  useEffect(() => {
    if (requestedTab !== undefined) {
      setOperationsTab(requestedTab);
    }
  }, [requestedTab]);

  const refreshQueue = useCallback(async () => {
    if (!isTauriRuntime()) return;
    try {
      const items = await listIngestQueue();
      setIngestQueue(items);
    } catch {
      // silently ignore
    }
  }, []);

  const cancelQueueItem = useCallback(async (id: number) => {
    if (!isTauriRuntime()) return;
    try {
      await cancelIngestItem(id);
      const items = await listIngestQueue();
      setIngestQueue(items);
    } catch {
      // silently ignore
    }
  }, []);

  const retryQueueItem = useCallback(async (id: number) => {
    if (!isTauriRuntime()) return;
    try {
      await retryIngestItem(id);
      const items = await listIngestQueue();
      setIngestQueue(items);
    } catch {
      // silently ignore
    }
  }, []);

  const deleteQueueItem = useCallback(async (id: number) => {
    if (!isTauriRuntime()) return;
    try {
      await deleteIngestItem(id);
      const items = await listIngestQueue();
      setIngestQueue(items);
    } catch {
      // silently ignore
    }
  }, []);

  const loadVaultStats = useCallback(async () => {
    if (!isTauriRuntime()) return;
    setVaultStatsLoading(true);
    try {
      const stats = await getVaultStats();
      setVaultStats(stats);
    } catch {
      setVaultStats(null);
    } finally {
      setVaultStatsLoading(false);
    }
  }, []);

  // When the module becomes active, load data for the current tab.
  useEffect(() => {
    if (!isActive) return;
    if (operationsTab === "queue") {
      void refreshQueue();
    } else {
      void loadVaultStats();
    }
  }, [isActive]); // intentionally only trigger on activation change

  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">运行</h1>
        <p className="module-header__sub">任务队列与运行统计</p>
      </div>
      <section className="panel operations-module">
        <div className="operations-tabs">
          <button
            type="button"
            className={`operations-tabs__item${operationsTab === "queue" ? " operations-tabs__item--active" : ""}`}
            onClick={() => {
              setOperationsTab("queue");
              void refreshQueue();
            }}
          >
            任务队列
          </button>
          <button
            type="button"
            className={`operations-tabs__item${operationsTab === "stats" ? " operations-tabs__item--active" : ""}`}
            onClick={() => {
              setOperationsTab("stats");
              void loadVaultStats();
            }}
          >
            运行统计
          </button>
        </div>
        <p className="operations-tabs__hint">
          {operationsTab === "queue"
            ? "查看摄入任务进度，支持取消与失败重试。"
            : "查看知识库规模、近期增长与引用分布。"}
        </p>
        <div className="operations-content">
          {operationsTab === "queue" ? (
            <QueuePanel
              queue={ingestQueue}
              onRefresh={() => {
                void refreshQueue();
              }}
              onCancel={(id) => {
                void cancelQueueItem(id);
              }}
              onRetry={(id) => {
                void retryQueueItem(id);
              }}
              onDelete={(id) => {
                void deleteQueueItem(id);
              }}
            />
          ) : (
            <div className="stats-module">
              <div className="stats-module__header">
                <h2 className="stats-module__title">知识库统计</h2>
                <button
                  className="stats-module__refresh"
                  onClick={() => void loadVaultStats()}
                  disabled={vaultStatsLoading}
                >
                  {vaultStatsLoading ? "加载中…" : "刷新"}
                </button>
              </div>
              {vaultStats ? (
                <>
                  <div className="stats-cards">
                    <div className="stats-card">
                      <span className="stats-card__value">{vaultStats.total_pages}</span>
                      <span className="stats-card__label">Wiki 页面</span>
                    </div>
                    <div className="stats-card">
                      <span className="stats-card__value">{vaultStats.pages_last_7_days}</span>
                      <span className="stats-card__label">近 7 天新增</span>
                    </div>
                    <div className="stats-card">
                      <span className="stats-card__value">{vaultStats.pages_last_30_days}</span>
                      <span className="stats-card__label">近 30 天新增</span>
                    </div>
                    <div className="stats-card stats-card--warn">
                      <span className="stats-card__value">{vaultStats.orphan_pages}</span>
                      <span className="stats-card__label">孤立页面</span>
                    </div>
                    <div className="stats-card">
                      <span className="stats-card__value">{vaultStats.total_citations}</span>
                      <span className="stats-card__label">总引用数</span>
                    </div>
                  </div>
                  {vaultStats.top_cited_pages.length > 0 && (
                    <div className="stats-section">
                      <h3 className="stats-section__title">被引用最多的页面</h3>
                      <ol className="stats-top-list">
                        {vaultStats.top_cited_pages.map((item) => (
                          <li key={item.path} className="stats-top-list__item">
                            <span className="stats-top-list__rank-badge">{item.citation_count}</span>
                            <button
                              className="stats-top-list__title"
                              onClick={() => {
                                navigateTo("wiki");
                              }}
                            >
                              {item.title}
                            </button>
                          </li>
                        ))}
                      </ol>
                    </div>
                  )}
                  {vaultStats.ingest_source_counts.length > 0 && (
                    <div className="stats-section">
                      <h3 className="stats-section__title">摄入来源分布</h3>
                      <ul className="stats-source-list">
                        {vaultStats.ingest_source_counts.map((item) => (
                          <li key={item.source_type} className="stats-source-list__item">
                            <span className="stats-source-list__label">
                              {formatSourceType(item.source_type)}
                            </span>
                            <div className="stats-source-bar">
                              <div
                                className="stats-source-bar__fill"
                                style={{
                                  width: `${Math.min(100, (item.count / Math.max(1, vaultStats.total_pages)) * 100)}%`,
                                }}
                              />
                            </div>
                            <span className="stats-source-list__count">{item.count}</span>
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                </>
              ) : (
                <p className="stats-module__empty">
                  {vaultStatsLoading ? "正在加载统计数据，请稍候…" : "暂无统计数据。请先完成一次摄入或初始化 Vault。"}
                </p>
              )}
            </div>
          )}
        </div>
      </section>
    </>
  );
}
