import { Suspense, type Dispatch, type MutableRefObject, type RefObject, type SetStateAction } from "react";
import type { KnowledgeGraphData, KnowledgeGraphNode, ModuleId } from "../../types";
import type { GraphInsightKind, GraphTraversalDirection, GraphViewMode } from "../../App";

type GraphLikeNode = KnowledgeGraphNode & {
  isAggregate?: boolean;
  count?: number;
  x?: number;
  y?: number;
};

type GraphLikeLink = {
  source: string | { id: string };
  target: string | { id: string };
  weight?: number;
};

type GraphLikeData = {
  nodes: GraphLikeNode[];
  links: GraphLikeLink[];
};

type GraphInsightItem = {
  kind: GraphInsightKind;
  title: string;
  description: string;
  evidence: string[];
  suggestion: string;
};

type GraphMetrics = {
  inDegree: Map<string, number>;
  outDegree: Map<string, number>;
  totalDegree: Map<string, number>;
  orphanCount: number;
};

type GraphModuleProps = {
  ForceGraph2D: React.ComponentType<Record<string, unknown>>;
  GraphErrorBoundary: React.ComponentType<{ children: React.ReactNode }>;
  graphInsightKindLabels: Record<GraphInsightKind, string>;
  graphViewMode: GraphViewMode;
  graphSearchInputRef: RefObject<HTMLInputElement>;
  graphSearchQuery: string;
  setGraphSearchQuery: Dispatch<SetStateAction<string>>;
  graphGroupFilter: string;
  setGraphGroupFilter: Dispatch<SetStateAction<string>>;
  graphGroupOptions: string[];
  graphLocalDepth: number;
  graphSelectedNode: KnowledgeGraphNode | null;
  graphLocalDirection: GraphTraversalDirection;
  graphShowOrphans: boolean;
  setGraphShowOrphans: Dispatch<SetStateAction<boolean>>;
  graphNeighborOnly: boolean;
  setGraphNeighborOnly: Dispatch<SetStateAction<boolean>>;
  graphVisibleData: KnowledgeGraphData | null;
  graphLayoutFrozen: boolean;
  graphRenderData: GraphLikeData | null;
  graphAggregateMode: boolean;
  GRAPH_AGGREGATE_THRESHOLD: number;
  graphLocalSubgraphTruncated: boolean;
  graphNodes: KnowledgeGraphNode[];
  graphNormalizedLinks: GraphLikeLink[];
  graphVisibleOrphanCount: number;
  graphMetrics: GraphMetrics;
  graphShouldUseBackendSubgraph: boolean;
  graphLocalSubgraphLoading: boolean;
  graphLocalSubgraphError: string;
  graphContainerRef: RefObject<HTMLDivElement>;
  graphLoading: boolean;
  graphError: string;
  graphData: KnowledgeGraphData | null;
  graphRef: MutableRefObject<any>;
  graphDimensions: { width: number; height: number };
  groupColor: (group: string) => string;
  graphSelectedNodeId: string;
  isSameWikiPagePath: (left: string, right: string) => boolean;
  graphSearchHits: Set<string>;
  resolveGraphNodePagePath: (node: Partial<KnowledgeGraphNode> | null | undefined) => string;
  setActiveModule: (moduleId: ModuleId) => void;
  handleOpenWikiPage: (path: string) => void | Promise<void>;
  handleGraphNodeClick: (node: object) => void;
  graphSearchableNodes: Array<{ id: string; label: string; group?: string }>;
  graphSelectedAggregateId: string;
  setGraphAggregateMode: Dispatch<SetStateAction<boolean>>;
  handleGraphViewModeChange: (mode: GraphViewMode) => void;
  GRAPH_LOCAL_DEPTH_MIN: number;
  GRAPH_LOCAL_DEPTH_MAX: number;
  handleGraphLocalDepthChange: (depth: number) => void;
  handleGraphLocalDirectionChange: (direction: string) => void;
  handleGraphZoomToFit: () => void;
  handleToggleGraphLayoutFreeze: () => void;
  handleExportGraphJson: () => void;
  handleResetGraphFilters: () => void;
  handleGraphInsightSparseDensityChange: (value: number) => void;
  graphInsightSparseDensity: number;
  GRAPH_INSIGHT_SPARSE_DENSITY_MIN: number;
  GRAPH_INSIGHT_SPARSE_DENSITY_MAX: number;
  handleGraphInsightBridgeMinGroupsChange: (value: number) => void;
  graphInsightBridgeMinGroups: number;
  GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN: number;
  GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX: number;
  handleGraphInsightSurprisingJaccardChange: (value: number) => void;
  graphInsightSurprisingJaccard: number;
  GRAPH_INSIGHT_SURPRISING_JACCARD_MIN: number;
  GRAPH_INSIGHT_SURPRISING_JACCARD_MAX: number;
  handleGraphInsightSurprisingConfidenceChange: (value: number) => void;
  graphInsightSurprisingConfidence: number;
  GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN: number;
  GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX: number;
  graphInsights: GraphInsightItem[];
  handleApplyGraphInsight: (insight: GraphInsightItem) => void;
  graphSelectedAggregateNode: { id: string; label: string } | null;
  graphSelectedAggregateMembers: Array<{ id: string; label: string; group?: string }>;
  handleExpandSelectedAggregateNode: () => void;
  handleExitAggregateMode: () => void;
  handleOpenAggregateMemberPage: (path: string) => void | Promise<void>;
  handleOpenSelectedGraphNode: () => void | Promise<void>;
  graphSelectedNeighbors: Array<{ id: string; label: string; group?: string }>;
};

export default function GraphModule({
  ForceGraph2D,
  GraphErrorBoundary,
  graphInsightKindLabels,
  graphViewMode,
  graphSearchInputRef,
  graphSearchQuery,
  setGraphSearchQuery,
  graphGroupFilter,
  setGraphGroupFilter,
  graphGroupOptions,
  graphLocalDepth,
  graphSelectedNode,
  graphLocalDirection,
  graphShowOrphans,
  setGraphShowOrphans,
  graphNeighborOnly,
  setGraphNeighborOnly,
  graphVisibleData,
  graphLayoutFrozen,
  graphRenderData,
  graphAggregateMode,
  GRAPH_AGGREGATE_THRESHOLD,
  graphLocalSubgraphTruncated,
  graphNodes,
  graphNormalizedLinks,
  graphVisibleOrphanCount,
  graphMetrics,
  graphShouldUseBackendSubgraph,
  graphLocalSubgraphLoading,
  graphLocalSubgraphError,
  graphContainerRef,
  graphLoading,
  graphError,
  graphData,
  graphRef,
  graphDimensions,
  groupColor,
  graphSelectedNodeId,
  isSameWikiPagePath,
  graphSearchHits,
  resolveGraphNodePagePath,
  setActiveModule,
  handleOpenWikiPage,
  handleGraphNodeClick,
  graphSearchableNodes,
  graphSelectedAggregateId,
  setGraphAggregateMode,
  handleGraphViewModeChange,
  GRAPH_LOCAL_DEPTH_MIN,
  GRAPH_LOCAL_DEPTH_MAX,
  handleGraphLocalDepthChange,
  handleGraphLocalDirectionChange,
  handleGraphZoomToFit,
  handleToggleGraphLayoutFreeze,
  handleExportGraphJson,
  handleResetGraphFilters,
  handleGraphInsightSparseDensityChange,
  graphInsightSparseDensity,
  GRAPH_INSIGHT_SPARSE_DENSITY_MIN,
  GRAPH_INSIGHT_SPARSE_DENSITY_MAX,
  handleGraphInsightBridgeMinGroupsChange,
  graphInsightBridgeMinGroups,
  GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN,
  GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX,
  handleGraphInsightSurprisingJaccardChange,
  graphInsightSurprisingJaccard,
  GRAPH_INSIGHT_SURPRISING_JACCARD_MIN,
  GRAPH_INSIGHT_SURPRISING_JACCARD_MAX,
  handleGraphInsightSurprisingConfidenceChange,
  graphInsightSurprisingConfidence,
  GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN,
  GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX,
  graphInsights,
  handleApplyGraphInsight,
  graphSelectedAggregateNode,
  graphSelectedAggregateMembers,
  handleExpandSelectedAggregateNode,
  handleExitAggregateMode,
  handleOpenAggregateMemberPage,
  handleOpenSelectedGraphNode,
  graphSelectedNeighbors,
}: GraphModuleProps) {
  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">知识图谱</h1>
        <p className="module-header__sub">Wiki 页面与引用关系可视化</p>
      </div>
      <div className="graph-layout">
        <section className="graph-workspace">
          <div className="graph-toolbar">
            <div className="graph-toolbar__controls">
              <div className="graph-mode-switch" role="group" aria-label="图谱模式切换">
                <button
                  type="button"
                  className={`dev-panel__button ${graphViewMode === "global" ? "dev-panel__button--accent" : ""}`}
                  onClick={() => handleGraphViewModeChange("global")}
                >
                  Global
                </button>
                <button
                  type="button"
                  className={`dev-panel__button ${graphViewMode === "local" ? "dev-panel__button--accent" : ""}`}
                  onClick={() => handleGraphViewModeChange("local")}
                >
                  Local
                </button>
              </div>

              <div className="graph-control graph-control--search">
                <input
                  ref={graphSearchInputRef}
                  type="text"
                  className="dev-panel__input"
                  placeholder="搜索节点..."
                  value={graphSearchQuery}
                  onChange={(e) => setGraphSearchQuery(e.target.value)}
                />
                {graphSearchQuery && (
                  <button
                    type="button"
                    className="dev-panel__button dev-panel__button--small"
                    onClick={() => setGraphSearchQuery("")}
                    title="清除搜索"
                  >
                    ✕
                  </button>
                )}
              </div>

              <label className="graph-control" htmlFor="graph-group-filter"><span className="graph-control__label">分组</span>
                <select
                  id="graph-group-filter"
                  className="dev-panel__input"
                  value={graphGroupFilter}
                  onChange={(event) => setGraphGroupFilter(event.target.value)}
                >
                  <option value="__all__">全部</option>
                  <option value="__ungrouped__">未分组</option>
                  {graphGroupOptions.map((group) => (
                    <option key={group} value={group}>
                      {group}
                    </option>
                  ))}
                </select>
              </label>
              <label className="graph-control graph-control--depth" htmlFor="graph-local-depth">
                <span className="graph-control__label">Hop 深度</span>
                <input
                  id="graph-local-depth"
                  type="range"
                  min={GRAPH_LOCAL_DEPTH_MIN}
                  max={GRAPH_LOCAL_DEPTH_MAX}
                  step={1}
                  value={graphLocalDepth}
                  disabled={graphViewMode !== "local" || !graphSelectedNode}
                  onChange={(event) => handleGraphLocalDepthChange(Number(event.target.value))}
                />
                <span className="graph-control__value">{graphLocalDepth}</span>
              </label>
              <label className="graph-control" htmlFor="graph-local-direction">
                <span className="graph-control__label">方向</span>
                <select
                  id="graph-local-direction"
                  className="dev-panel__input"
                  value={graphLocalDirection}
                  disabled={graphViewMode !== "local" || !graphSelectedNode}
                  onChange={(event) => handleGraphLocalDirectionChange(event.target.value)}
                >
                  <option value="both">双向</option>
                  <option value="out">向外</option>
                  <option value="in">向内</option>
                </select>
              </label>
              <button
                type="button"
                className={`dev-panel__button ${graphShowOrphans ? "dev-panel__button--accent" : ""}`}
                onClick={() => setGraphShowOrphans((prev) => !prev)}
              >
                {graphShowOrphans ? "显示孤儿页" : "隐藏孤儿页"}
              </button>
              <button
                type="button"
                className={`dev-panel__button ${graphNeighborOnly ? "dev-panel__button--accent" : ""}`}
                onClick={() => setGraphNeighborOnly((prev) => !prev)}
                disabled={!graphSelectedNode}
                title={graphSelectedNode ? "仅显示当前节点与一跳邻居" : "先选中节点后可启用"}
              >
                {graphNeighborOnly ? "仅看邻居：开" : "仅看邻居：关"}
              </button>
              <button
                type="button"
                className="dev-panel__button"
                onClick={handleGraphZoomToFit}
                disabled={!graphVisibleData || graphVisibleData.nodes.length === 0}
              >
                适配视图
              </button>
              <button
                type="button"
                className={`dev-panel__button ${graphLayoutFrozen ? "dev-panel__button--accent" : ""}`}
                onClick={handleToggleGraphLayoutFreeze}
                disabled={!graphRenderData || graphRenderData.nodes.length === 0}
              >
                {graphLayoutFrozen ? "恢复布局" : "冻结布局"}
              </button>
              <button
                type="button"
                className={`dev-panel__button ${graphAggregateMode ? "dev-panel__button--accent" : ""}`}
                disabled={(graphVisibleData?.nodes.length ?? 0) <= GRAPH_AGGREGATE_THRESHOLD}
                onClick={() => setGraphAggregateMode((prev) => !prev)}
                title={
                  (graphVisibleData?.nodes.length ?? 0) <= GRAPH_AGGREGATE_THRESHOLD
                    ? `节点数未超过 ${GRAPH_AGGREGATE_THRESHOLD}，无需聚合`
                    : "按分组聚合显示大图，降低渲染压力"
                }
              >
                {graphAggregateMode ? "聚合模式：开" : "聚合模式：关"}
              </button>
              <button
                type="button"
                className="dev-panel__button"
                onClick={handleExportGraphJson}
                disabled={!graphRenderData || graphRenderData.nodes.length === 0}
                title="导出当前视图图谱 JSON"
              >
                导出 JSON
              </button>
              <button
                type="button"
                className="dev-panel__button"
                onClick={handleResetGraphFilters}
              >
                重置筛选
              </button>
            </div>
            <div className="graph-toolbar__stats">
              <span className="pill">{`模式 ${graphViewMode === "local" ? "Local" : "Global"}`}</span>
              {graphViewMode === "local" ? (
                <span className="pill">{`方向 ${graphLocalDirection}`}</span>
              ) : null}
              {graphViewMode === "local" ? (
                <span className="pill">{`局部计算 ${graphShouldUseBackendSubgraph ? "后端" : "前端"}`}</span>
              ) : null}
              {graphViewMode === "local" && graphLocalSubgraphTruncated ? (
                <span className="pill pill--warn">子图已裁剪</span>
              ) : null}
              {(graphVisibleData?.nodes.length ?? 0) > GRAPH_AGGREGATE_THRESHOLD ? (
                <span className="pill">{`聚合 ${graphAggregateMode ? "ON" : "OFF"}`}</span>
              ) : null}
              <span className="pill">{`节点 ${graphVisibleData?.nodes.length ?? 0}/${graphNodes.length}`}</span>
              <span className="pill">{`边 ${graphVisibleData?.links.length ?? 0}/${graphNormalizedLinks.length}`}</span>
              <span className="pill">{`孤儿页 ${graphVisibleOrphanCount}/${graphMetrics.orphanCount}`}</span>
            </div>
          </div>

          {graphViewMode === "local" && graphShouldUseBackendSubgraph ? (
            <p className="runtime-hint">当前图规模较大，Local 模式已自动启用后端子图计算。</p>
          ) : null}
          {graphViewMode === "local" && graphLocalSubgraphLoading ? (
            <p className="runtime-hint">正在请求后端子图...</p>
          ) : null}
          {graphViewMode === "local" && graphLocalSubgraphError ? (
            <p className="runtime-status">{graphLocalSubgraphError}</p>
          ) : null}

          <div className="graph-module" ref={graphContainerRef}>
            {graphLoading && (
              <div className="graph-module__loading">加载图谱中...</div>
            )}
            {!graphLoading && graphError && (
              <div className="graph-module__empty">
                <p>{graphError}</p>
                <p>请检查后端日志或稍后重试。</p>
              </div>
            )}
            {!graphLoading && graphData && graphData.nodes.length === 0 && (
              <div className="graph-module__empty">
                <p>暂无 Wiki 页面数据。</p>
                <p>请先在 Inbox 中摄入文档，生成 Wiki 页面后图谱将自动显示。</p>
              </div>
            )}
            {!graphLoading && !graphError && !graphData && (
              <div className="graph-module__empty">
                <p>当前未获取到图谱数据。</p>
                <p>请稍后重试，或先确认后端服务运行正常。</p>
              </div>
            )}
            {!graphLoading && graphData && graphData.nodes.length > 0 && graphVisibleData && graphVisibleData.nodes.length === 0 && (
              <div className="graph-module__empty">
                {graphViewMode === "local" && !graphSelectedNode ? (
                  <>
                    <p>Local 模式需要先选择中心节点。</p>
                    <p>请先在图中点击一个节点，或切回 Global 模式。</p>
                  </>
                ) : (
                  <>
                    <p>当前筛选条件下无节点。</p>
                    <p>请放宽分组、孤儿页、邻居或 Hop 深度筛选条件。</p>
                  </>
                )}
              </div>
            )}
            {!graphLoading && graphRenderData && graphRenderData.nodes.length > 0 && (
              <GraphErrorBoundary>
                <Suspense fallback={<div className="graph-module__loading">图谱渲染中...</div>}>
                  <ForceGraph2D
                    ref={graphRef}
                    graphData={graphRenderData}
                    width={graphDimensions.width}
                    height={graphDimensions.height}
                    nodeLabel="label"
                    nodeRelSize={6}
                    nodeVal={(node: object) => {
                      const n = node as GraphLikeNode;
                      if (n.isAggregate) {
                        return 5 + Math.min(16, n.count ?? 1);
                      }
                      const degree = graphMetrics.totalDegree.get(n.id) ?? 0;
                      return 2 + Math.min(8, degree);
                    }}
                    nodeColor={(node: object) => {
                      const n = node as GraphLikeNode;
                      return n.group ? groupColor(n.group) : "#4a9eff";
                    }}
                    linkColor={() => "rgba(120,120,180,0.4)"}
                    linkWidth={(link: object) => {
                      const edge = link as GraphLikeLink;
                      return edge.weight ? Math.min(1 + edge.weight * 0.5, 4) : 1;
                    }}
                    onNodeClick={(node: object) => {
                      const n = node as GraphLikeNode;
                      const now = Date.now();
                      const last = graphRef.current?.__lastNodeClickTime ?? 0;
                      const lastId = graphRef.current?.__lastNodeClickId ?? "";
                      graphRef.current.__lastNodeClickTime = now;
                      graphRef.current.__lastNodeClickId = n.id;
                      if (now - last < 400 && lastId === n.id && !n.isAggregate) {
                        const pagePath = resolveGraphNodePagePath(n);
                        if (pagePath) { setActiveModule("wiki"); void handleOpenWikiPage(pagePath); }
                        return;
                      }
                      handleGraphNodeClick(node);
                    }}
                    nodeCanvasObject={(node: object, ctx: CanvasRenderingContext2D, globalScale: number) => {
                      const n = node as GraphLikeNode;
                      const label = n.label || n.id;
                      const isAggregateNode = Boolean(n.isAggregate);
                      const selected = Boolean(
                        !isAggregateNode && graphSelectedNodeId && isSameWikiPagePath(graphSelectedNodeId, n.id),
                      );
                      const isSearchHit = graphSearchHits.has(n.id);
                      const radius = isAggregateNode
                        ? Math.min(14, 7 + Math.floor((n.count ?? 1) / 3))
                        : selected
                          ? 8
                          : 5;

                      if (isSearchHit) {
                        ctx.beginPath();
                        ctx.arc(n.x ?? 0, n.y ?? 0, radius + 8, 0, 2 * Math.PI, false);
                        ctx.fillStyle = "rgba(255, 235, 59, 0.4)";
                        ctx.fill();
                      }

                      const fontSize = Math.max(10 / globalScale, 3);
                      ctx.font = `${fontSize}px Sans-Serif`;
                      ctx.fillStyle = n.group ? groupColor(n.group) : "#4a9eff";
                      ctx.beginPath();
                      ctx.arc(n.x ?? 0, n.y ?? 0, radius, 0, 2 * Math.PI, false);
                      ctx.fill();
                      if (selected) {
                        ctx.strokeStyle = "rgba(255, 255, 255, 0.9)";
                        ctx.lineWidth = Math.max(2 / globalScale, 1);
                        ctx.beginPath();
                        ctx.arc(n.x ?? 0, n.y ?? 0, radius + 2, 0, 2 * Math.PI, false);
                        ctx.stroke();
                      }
                      if (globalScale > 1.4 || selected || isAggregateNode) {
                        ctx.fillStyle = "rgba(255,255,255,0.9)";
                        ctx.fillText(label, (n.x ?? 0) + 10, (n.y ?? 0) + 4);
                      }
                    }}
                    cooldownTicks={100}
                    d3AlphaDecay={0.02}
                    d3VelocityDecay={0.3}
                  />
                </Suspense>
              </GraphErrorBoundary>
            )}
          </div>
        </section>

        <aside className="graph-sidepanel">
          {graphSearchQuery.trim() !== "" && (
            <div className="graph-search-results">
              <div className="graph-sidepanel__head">
                <h3>搜索结果</h3>
                <span>{graphSearchHits.size} 条命中</span>
              </div>
              {graphSearchHits.size === 0 ? (
                <p className="graph-sidepanel__hint">未找到匹配的节点。</p>
              ) : (
                <ul className="graph-neighbors__list" style={{ maxHeight: "240px", overflowY: "auto" }}>
                  {Array.from(graphSearchHits).slice(0, 50).map((id) => {
                    const node = graphSearchableNodes.find((n) => n.id === id);
                    if (!node) return null;
                    return (
                      <li key={node.id}>
                        <button
                          type="button"
                          className={`graph-neighbors__item ${
                            graphSelectedNodeId === node.id || graphSelectedAggregateId === node.id
                              ? "graph-neighbors__item--active"
                              : ""
                          }`}
                          onClick={() => handleGraphNodeClick(node)}
                          title={node.id}
                        >
                          <span>{node.label || node.id}</span>
                          <code>{node.group || "未分组"}</code>
                        </button>
                      </li>
                    );
                  })}
                </ul>
              )}
              <hr className="graph-sidepanel__divider" />
            </div>
          )}

          <div className="graph-insight-config">
            <div className="graph-sidepanel__head">
              <h3>洞察阈值</h3>
              <span>本地持久化</span>
            </div>
            <label className="graph-insight-config__item" htmlFor="graph-insight-sparse-density">
              <span>稀疏密度 ≤ {graphInsightSparseDensity.toFixed(2)}</span>
              <input
                id="graph-insight-sparse-density"
                type="range"
                min={GRAPH_INSIGHT_SPARSE_DENSITY_MIN}
                max={GRAPH_INSIGHT_SPARSE_DENSITY_MAX}
                step={0.05}
                value={graphInsightSparseDensity}
                onChange={(event) => handleGraphInsightSparseDensityChange(Number(event.target.value))}
              />
            </label>
            <label className="graph-insight-config__item" htmlFor="graph-insight-bridge-min-groups">
              <span>桥接分组数 ≥ {graphInsightBridgeMinGroups}</span>
              <input
                id="graph-insight-bridge-min-groups"
                type="range"
                min={GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN}
                max={GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX}
                step={1}
                value={graphInsightBridgeMinGroups}
                onChange={(event) => handleGraphInsightBridgeMinGroupsChange(Number(event.target.value))}
              />
            </label>
            <label className="graph-insight-config__item" htmlFor="graph-insight-surprising-jaccard">
              <span>异常连接相似度 ≤ {graphInsightSurprisingJaccard.toFixed(2)}</span>
              <input
                id="graph-insight-surprising-jaccard"
                type="range"
                min={GRAPH_INSIGHT_SURPRISING_JACCARD_MIN}
                max={GRAPH_INSIGHT_SURPRISING_JACCARD_MAX}
                step={0.05}
                value={graphInsightSurprisingJaccard}
                onChange={(event) => handleGraphInsightSurprisingJaccardChange(Number(event.target.value))}
              />
            </label>
            <label
              className="graph-insight-config__item"
              htmlFor="graph-insight-surprising-confidence"
            >
              <span>异常连接置信度 ≥ {graphInsightSurprisingConfidence.toFixed(2)}</span>
              <input
                id="graph-insight-surprising-confidence"
                type="range"
                min={GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN}
                max={GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX}
                step={0.05}
                value={graphInsightSurprisingConfidence}
                onChange={(event) =>
                  handleGraphInsightSurprisingConfidenceChange(Number(event.target.value))
                }
              />
            </label>
            <hr className="graph-sidepanel__divider" />
          </div>

          <div className="graph-insights">
            <div className="graph-sidepanel__head">
              <h3>图谱洞察</h3>
              <span>{graphInsights.length} 条</span>
            </div>
            {graphInsights.length === 0 ? (
              <p className="graph-sidepanel__hint">当前视图未发现明显结构风险。</p>
            ) : (
              <ul className="graph-insights__list">
                {graphInsights.map((insight) => (
                  <li key={`${insight.kind}-${insight.title}`}>
                    <button
                      type="button"
                      className="graph-insights__item"
                      onClick={() => handleApplyGraphInsight(insight)}
                    >
                      <div className="graph-insights__head">
                        <strong>{insight.title}</strong>
                        <span className="pill">{graphInsightKindLabels[insight.kind]}</span>
                      </div>
                      <p>{insight.description}</p>
                      {insight.evidence.length > 0 ? (
                        <ul className="graph-insights__evidence">
                          {insight.evidence.map((item) => (
                            <li key={item}>{item}</li>
                          ))}
                        </ul>
                      ) : null}
                      <p className="graph-insights__suggestion">{insight.suggestion}</p>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <hr className="graph-sidepanel__divider" />
          </div>

          <div className="graph-sidepanel__head">
            <h3>节点详情</h3>
            <span>
              {graphSelectedNode || graphSelectedAggregateNode ? "已选中" : "未选中"} ·{" "}
              {graphViewMode === "local" ? "Local" : "Global"}
            </span>
          </div>
          {graphSelectedAggregateNode ? (
            <>
              <div className="graph-node-card">
                <p className="graph-node-card__title">
                  {graphSelectedAggregateNode.label || graphSelectedAggregateNode.id}
                </p>
                <code className="graph-node-card__path">{graphSelectedAggregateNode.id}</code>
                <div className="graph-node-card__stats">
                  <span className="pill">{`聚合节点`}</span>
                  <span className="pill">{`成员 ${graphSelectedAggregateMembers.length}`}</span>
                </div>
                <div className="graph-node-card__actions">
                  <button
                    type="button"
                    className="dev-panel__button dev-panel__button--accent"
                    onClick={handleExpandSelectedAggregateNode}
                  >
                    展开查看成员页
                  </button>
                  <button
                    type="button"
                    className="dev-panel__button"
                    onClick={handleExitAggregateMode}
                  >
                    切回明细模式
                  </button>
                </div>
              </div>
              <div className="graph-neighbors">
                <div className="graph-neighbors__head">
                  <h4>成员页面</h4>
                  <span>{graphSelectedAggregateMembers.length}</span>
                </div>
                {graphSelectedAggregateMembers.length === 0 ? (
                  <p className="graph-sidepanel__hint">该聚合节点暂无可展开成员。</p>
                ) : (
                  <ul className="graph-neighbors__list">
                    {graphSelectedAggregateMembers.slice(0, 20).map((member) => (
                      <li key={member.id}>
                        <button
                          type="button"
                          className="graph-neighbors__item"
                          onClick={() => {
                            void handleOpenAggregateMemberPage(member.id);
                          }}
                          title={member.id}
                        >
                          <span>{member.label}</span>
                          <code>{member.group || "未分组"}</code>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </>
          ) : !graphSelectedNode ? (
            <p className="graph-sidepanel__hint">点击左侧节点可查看标题、度数和关联页面。</p>
          ) : (
            <>
              <div className="graph-node-card">
                <p className="graph-node-card__title">{graphSelectedNode.label || "未命名页面"}</p>
                <code className="graph-node-card__path">{graphSelectedNode.id}</code>
                <div className="graph-node-card__stats">
                  <span className="pill">{`入度 ${graphMetrics.inDegree.get(graphSelectedNode.id) ?? 0}`}</span>
                  <span className="pill">{`出度 ${graphMetrics.outDegree.get(graphSelectedNode.id) ?? 0}`}</span>
                  <span className="pill">{`总连接 ${graphMetrics.totalDegree.get(graphSelectedNode.id) ?? 0}`}</span>
                </div>
                <div className="graph-node-card__actions">
                  <button
                    type="button"
                    className="dev-panel__button dev-panel__button--accent"
                    onClick={() => void handleOpenSelectedGraphNode()}
                  >
                    打开页面
                  </button>
                  <button
                    type="button"
                    className={`dev-panel__button ${graphNeighborOnly ? "dev-panel__button--accent" : ""}`}
                    onClick={() => setGraphNeighborOnly((prev) => !prev)}
                  >
                    {graphNeighborOnly ? "退出邻居模式" : "仅看该节点邻居"}
                  </button>
                </div>
              </div>
              <div className="graph-neighbors">
                <div className="graph-neighbors__head">
                  <h4>关联页面</h4>
                  <span>{graphSelectedNeighbors.length}</span>
                </div>
                {graphSelectedNeighbors.length === 0 ? (
                  <p className="graph-sidepanel__hint">该页面当前没有关联边。</p>
                ) : (
                  <ul className="graph-neighbors__list">
                    {graphSelectedNeighbors.slice(0, 12).map((neighbor) => (
                      <li key={neighbor.id}>
                        <button
                          type="button"
                          className="graph-neighbors__item"
                          onClick={() => handleGraphNodeClick(neighbor)}
                          title={neighbor.id}
                        >
                          <span>{neighbor.label}</span>
                          <code>{neighbor.group || "未分组"}</code>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </>
          )}
        </aside>
      </div>
    </>
  );
}
