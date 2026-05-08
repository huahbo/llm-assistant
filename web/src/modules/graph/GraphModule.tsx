import { Component, lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import type { KnowledgeGraphData, KnowledgeGraphLink, KnowledgeGraphNode, ModuleId } from "../../types";
import { useMode } from "../../contexts/ModeContext";
import { useToast } from "../../contexts/ToastContext";
import {
  getKnowledgeGraph,
  getKnowledgeSubgraph,
  getPageEmbeddingPairs,
} from "../../tauri-client";
import {
  AggregatedEdge,
  AggregatedNode,
  buildAggregatedGraphData,
  buildGraphInsights,
  buildGraphLocalData,
  buildGraphVisibleData,
  clampGraphInsightBridgeMinGroups,
  clampGraphInsightSparseDensity,
  clampGraphInsightSurprisingConfidence,
  clampGraphInsightSurprisingJaccard,
  clampGraphLocalDepth,
  DEFAULT_GRAPH_INSIGHT_CONFIG,
  GRAPH_AGGREGATE_THRESHOLD,
  GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX,
  GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN,
  GRAPH_INSIGHT_SPARSE_DENSITY_MAX,
  GRAPH_INSIGHT_SPARSE_DENSITY_MIN,
  GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX,
  GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN,
  GRAPH_INSIGHT_SURPRISING_JACCARD_MAX,
  GRAPH_INSIGHT_SURPRISING_JACCARD_MIN,
  GRAPH_LOCAL_BACKEND_MAX_LINKS,
  GRAPH_LOCAL_BACKEND_MAX_NODES,
  GRAPH_LOCAL_DEPTH_MAX,
  GRAPH_LOCAL_DEPTH_MIN,
  type GraphInsightConfig,
  type GraphInsightItem,
  type GraphInsightKind,
  type GraphNormalizedEdge,
  type GraphTraversalDirection,
  type GraphViewMode,
  isGraphTraversalDirection,
  readGraphInsightBridgeMinGroupsFromStorage,
  readGraphInsightSparseDensityFromStorage,
  readGraphInsightSurprisingConfidenceFromStorage,
  readGraphInsightSurprisingJaccardFromStorage,
  readGraphLocalDepthFromStorage,
  readGraphLocalDirectionFromStorage,
  readGraphViewModeFromStorage,
  resolveGraphNodePagePath,
  shouldUseBackendSubgraph,
  writeGraphInsightBridgeMinGroupsToStorage,
  writeGraphInsightSparseDensityToStorage,
  writeGraphInsightSurprisingConfidenceToStorage,
  writeGraphInsightSurprisingJaccardToStorage,
  writeGraphLocalDepthToStorage,
  writeGraphLocalDirectionToStorage,
  writeGraphViewModeToStorage,
} from "../../graph-utils";
import { isSameWikiPagePath } from "../../wiki-utils";

const graphInsightKindLabels: Record<GraphInsightKind, string> = {
  "isolated-node": "孤立页",
  "sparse-group": "稀疏分组",
  "bridge-node": "桥接节点",
  "surprising-link": "异常连接",
};

const ForceGraph2D = lazy(() => import("react-force-graph-2d"));

// ─── Types ────────────────────────────────────────────────────────────────────

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

type GraphMetrics = {
  inDegree: Map<string, number>;
  outDegree: Map<string, number>;
  totalDegree: Map<string, number>;
  orphanCount: number;
  adjacency: Map<string, Set<string>>;
};

// ─── Helpers ──────────────────────────────────────────────────────────────────

function groupColor(group: string): string {
  const palette = [
    "#4a9eff", "#ff7043", "#66bb6a", "#ab47bc",
    "#ffa726", "#26c6da", "#ec407a", "#8d6e63",
  ];
  let hash = 0;
  for (let i = 0; i < group.length; i++) {
    hash = group.charCodeAt(i) + ((hash << 5) - hash);
  }
  return palette[Math.abs(hash) % palette.length];
}

const GRAPH_STRUCTURAL_PAGE_NAMES = new Set(["index", "log", "overview"]);

const resolveGraphNodeLeafName = (id: string) =>
  id
    .trim()
    .replaceAll("\\", "/")
    .split("/")
    .pop()
    ?.replace(/\.md$/i, "")
    .toLowerCase() ?? "";

function resolveGraphLinkNodeId(
  endpoint: string | KnowledgeGraphNode | null | undefined,
): string {
  if (!endpoint) return "";
  if (typeof endpoint === "string") return endpoint;
  return resolveGraphNodePagePath(endpoint) ?? "";
}

// ─── GraphErrorBoundary ───────────────────────────────────────────────────────

type GraphErrorBoundaryProps = { children: React.ReactNode };
type GraphErrorBoundaryState = { hasError: boolean; message: string };

class GraphErrorBoundary extends Component<GraphErrorBoundaryProps, GraphErrorBoundaryState> {
  override state: GraphErrorBoundaryState = { hasError: false, message: "" };

  static getDerivedStateFromError(error: unknown): GraphErrorBoundaryState {
    const message = error instanceof Error ? error.message : String(error);
    return { hasError: true, message: message.trim() };
  }

  override componentDidCatch(error: unknown) {
    console.error("图谱渲染异常:", error);
  }

  override render() {
    if (this.state.hasError) {
      return (
        <div className="graph-module__empty">
          <p>{this.state.message ? `图谱渲染失败：${this.state.message}` : "图谱渲染失败。"}</p>
          <p>请先检查图谱依赖是否安装完整，或切换模块后重试。</p>
        </div>
      );
    }
    return this.props.children;
  }
}

// ─── Props ────────────────────────────────────────────────────────────────────

type GraphModuleProps = {
  handleOpenWikiPage: (path: string) => void | Promise<void>;
};

// ─── GraphModule ──────────────────────────────────────────────────────────────

export default function GraphModule({ handleOpenWikiPage }: GraphModuleProps) {
  const { activeModule, navigateTo: setActiveModule } = useMode();
  const { setStatusMessage } = useToast();

  // ── State ──────────────────────────────────────────────────────────────────

  const [graphEmbeddingSim, setGraphEmbeddingSim] = useState<Record<string, number> | undefined>(undefined);
  const [graphData, setGraphData] = useState<KnowledgeGraphData | null>(null);
  const [graphLoading, setGraphLoading] = useState(false);
  const [graphError, setGraphError] = useState("");
  const [graphLocalSubgraphData, setGraphLocalSubgraphData] = useState<KnowledgeGraphData | null>(null);
  const [graphLocalSubgraphLoading, setGraphLocalSubgraphLoading] = useState(false);
  const [graphLocalSubgraphError, setGraphLocalSubgraphError] = useState("");
  const [graphLocalSubgraphTruncated, setGraphLocalSubgraphTruncated] = useState(false);
  const [graphSelectedNodeId, setGraphSelectedNodeId] = useState("");
  const [graphSelectedAggregateId, setGraphSelectedAggregateId] = useState("");
  const [graphViewMode, setGraphViewMode] = useState<GraphViewMode>(() => readGraphViewModeFromStorage());
  const [graphLocalDepth, setGraphLocalDepth] = useState(() => readGraphLocalDepthFromStorage());
  const [graphLocalDirection, setGraphLocalDirection] = useState<GraphTraversalDirection>(
    () => readGraphLocalDirectionFromStorage(),
  );
  const [graphInsightSparseDensity, setGraphInsightSparseDensity] = useState(() =>
    readGraphInsightSparseDensityFromStorage(),
  );
  const [graphInsightBridgeMinGroups, setGraphInsightBridgeMinGroups] = useState(() =>
    readGraphInsightBridgeMinGroupsFromStorage(),
  );
  const [graphInsightSurprisingJaccard, setGraphInsightSurprisingJaccard] = useState(() =>
    readGraphInsightSurprisingJaccardFromStorage(),
  );
  const [graphInsightSurprisingConfidence, setGraphInsightSurprisingConfidence] = useState(() =>
    readGraphInsightSurprisingConfidenceFromStorage(),
  );
  const [graphGroupFilter, setGraphGroupFilter] = useState("__all__");
  const [graphShowOrphans, setGraphShowOrphans] = useState(true);
  const [graphNeighborOnly, setGraphNeighborOnly] = useState(false);
  const [graphLayoutFrozen, setGraphLayoutFrozen] = useState(false);
  const [graphSearchQuery, setGraphSearchQuery] = useState("");
  const [graphDimensions, setGraphDimensions] = useState({ width: 800, height: 600 });
  const [graphAggregateMode, setGraphAggregateMode] = useState(false);

  const graphContainerRef = useRef<HTMLDivElement>(null);
  const graphRef = useRef<any>(null);
  const graphSearchInputRef = useRef<HTMLInputElement>(null);

  // ── Data loading ───────────────────────────────────────────────────────────

  async function refreshGraphData() {
    if (activeModule !== "graph") return;
    setGraphLoading(true);
    setGraphError("");
    try {
      const data = await getKnowledgeGraph();
      setGraphData(data);
    } catch (err) {
      console.error("图谱加载失败:", err);
      const message = err instanceof Error ? err.message : String(err);
      setGraphData(null);
      setGraphError(`图谱加载失败：${message}`);
    } finally {
      setGraphLoading(false);
    }
  }

  // Reload graph when switching to this module
  useEffect(() => {
    void refreshGraphData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeModule]);

  // ── Derived state ──────────────────────────────────────────────────────────

  const graphNodes = graphData?.nodes ?? [];

  const graphNodeById = useMemo(() => {
    const map = new Map<string, KnowledgeGraphNode>();
    for (const node of graphNodes) {
      map.set(node.id, node);
    }
    return map;
  }, [graphNodes]);

  const graphNormalizedLinks = useMemo(() => {
    if (!graphData) return [] as Array<{ sourceId: string; targetId: string }>;
    const normalized: Array<{ sourceId: string; targetId: string }> = [];
    for (const link of graphData.links ?? []) {
      const edge = link as KnowledgeGraphLink;
      const sourceId = resolveGraphLinkNodeId(
        edge.source as string | KnowledgeGraphNode | null | undefined,
      );
      const targetId = resolveGraphLinkNodeId(
        edge.target as string | KnowledgeGraphNode | null | undefined,
      );
      if (!sourceId || !targetId) continue;
      normalized.push({ sourceId, targetId });
    }
    return normalized;
  }, [graphData]);

  const graphMetrics = useMemo<GraphMetrics>(() => {
    const inDegree = new Map<string, number>();
    const outDegree = new Map<string, number>();
    const totalDegree = new Map<string, number>();
    const adjacency = new Map<string, Set<string>>();

    for (const node of graphNodes) {
      inDegree.set(node.id, 0);
      outDegree.set(node.id, 0);
      totalDegree.set(node.id, 0);
      adjacency.set(node.id, new Set());
    }

    for (const edge of graphNormalizedLinks) {
      outDegree.set(edge.sourceId, (outDegree.get(edge.sourceId) ?? 0) + 1);
      inDegree.set(edge.targetId, (inDegree.get(edge.targetId) ?? 0) + 1);
      totalDegree.set(edge.sourceId, (totalDegree.get(edge.sourceId) ?? 0) + 1);
      totalDegree.set(edge.targetId, (totalDegree.get(edge.targetId) ?? 0) + 1);

      if (!adjacency.has(edge.sourceId)) adjacency.set(edge.sourceId, new Set());
      if (!adjacency.has(edge.targetId)) adjacency.set(edge.targetId, new Set());
      adjacency.get(edge.sourceId)?.add(edge.targetId);
      adjacency.get(edge.targetId)?.add(edge.sourceId);
    }

    const orphanCount = graphNodes.filter((node) => (totalDegree.get(node.id) ?? 0) === 0).length;
    return { inDegree, outDegree, totalDegree, adjacency, orphanCount };
  }, [graphNodes, graphNormalizedLinks]);

  const graphGroupOptions = useMemo(() => {
    const groups = new Set<string>();
    for (const node of graphNodes) {
      const group = node.group?.trim() ?? "";
      if (group) groups.add(group);
    }
    return Array.from(groups).sort((l, r) => l.localeCompare(r, "zh-CN"));
  }, [graphNodes]);

  const graphShouldUseBackendSubgraph = useMemo(
    () =>
      shouldUseBackendSubgraph({
        viewMode: graphViewMode,
        selectedNodeId: graphSelectedNodeId,
        totalNodes: graphNodes.length,
        totalLinks: graphNormalizedLinks.length,
      }),
    [graphNormalizedLinks, graphNodes.length, graphSelectedNodeId, graphViewMode],
  );

  const graphVisibleData = useMemo<KnowledgeGraphData | null>(() => {
    if (!graphData) return null;
    const baseVisible = buildGraphVisibleData({
      nodes: graphNodes,
      edges: graphNormalizedLinks,
      totalDegree: graphMetrics.totalDegree,
      groupFilter: graphGroupFilter,
      showOrphans: graphShowOrphans,
      neighborOnly: graphNeighborOnly,
      selectedNodeId: graphSelectedNodeId,
    });
    if (graphViewMode === "local" && graphShouldUseBackendSubgraph && graphLocalSubgraphData) {
      return buildGraphVisibleData({
        nodes: graphLocalSubgraphData.nodes,
        edges: graphLocalSubgraphData.links
          .map((link) => ({
            sourceId: resolveGraphLinkNodeId(link.source as string | KnowledgeGraphNode | null | undefined),
            targetId: resolveGraphLinkNodeId(link.target as string | KnowledgeGraphNode | null | undefined),
          }))
          .filter((edge) => edge.sourceId && edge.targetId),
        totalDegree: graphMetrics.totalDegree,
        groupFilter: graphGroupFilter,
        showOrphans: graphShowOrphans,
        neighborOnly: graphNeighborOnly,
        selectedNodeId: graphSelectedNodeId,
      });
    }
    if (graphViewMode === "local") {
      return buildGraphLocalData({
        nodes: baseVisible.nodes,
        edges: baseVisible.links
          .map((link) => ({
            sourceId: resolveGraphLinkNodeId(link.source as string | KnowledgeGraphNode | null | undefined),
            targetId: resolveGraphLinkNodeId(link.target as string | KnowledgeGraphNode | null | undefined),
          }))
          .filter((edge) => edge.sourceId && edge.targetId),
        selectedNodeId: graphSelectedNodeId,
        maxDepth: graphLocalDepth,
        direction: graphLocalDirection,
      });
    }
    return baseVisible;
  }, [
    graphData,
    graphGroupFilter,
    graphLocalDepth,
    graphLocalDirection,
    graphLocalSubgraphData,
    graphMetrics.totalDegree,
    graphNeighborOnly,
    graphNodes,
    graphNormalizedLinks,
    graphSelectedNodeId,
    graphShouldUseBackendSubgraph,
    graphViewMode,
    graphShowOrphans,
  ]);

  const graphSelectedNode = useMemo(() => {
    if (!graphSelectedNodeId) return null;
    return graphNodeById.get(graphSelectedNodeId) ?? null;
  }, [graphNodeById, graphSelectedNodeId]);

  const graphSelectedNeighbors = useMemo(() => {
    if (!graphSelectedNode) return [] as KnowledgeGraphNode[];
    const adjacency = graphMetrics.adjacency.get(graphSelectedNode.id);
    if (!adjacency || adjacency.size === 0) return [] as KnowledgeGraphNode[];
    return Array.from(adjacency)
      .map((neighborId) => graphNodeById.get(neighborId))
      .filter((node): node is KnowledgeGraphNode => Boolean(node))
      .sort((l, r) => l.label.localeCompare(r.label, "zh-CN"));
  }, [graphMetrics.adjacency, graphNodeById, graphSelectedNode]);

  const graphVisibleOrphanCount = useMemo(() => {
    if (!graphVisibleData) return 0;
    return graphVisibleData.nodes.filter((node) => (graphMetrics.totalDegree.get(node.id) ?? 0) === 0).length;
  }, [graphMetrics.totalDegree, graphVisibleData]);

  const aggregatedGraphData = useMemo<{
    nodes: AggregatedNode[];
    links: AggregatedEdge[];
  } | null>(() => {
    if (!graphAggregateMode || !graphVisibleData) return null;
    if (graphVisibleData.nodes.length <= GRAPH_AGGREGATE_THRESHOLD) return null;
    const normalizedForAgg: GraphNormalizedEdge[] = graphVisibleData.links.map((link) => {
      const srcId = typeof link.source === "string" ? link.source : (link.source as KnowledgeGraphNode).id;
      const tgtId = typeof link.target === "string" ? link.target : (link.target as KnowledgeGraphNode).id;
      return { sourceId: srcId, targetId: tgtId };
    });
    return buildAggregatedGraphData(graphVisibleData.nodes, normalizedForAgg);
  }, [graphAggregateMode, graphVisibleData]);

  const graphRenderData = useMemo<{
    nodes: Array<KnowledgeGraphNode | AggregatedNode>;
    links: Array<KnowledgeGraphLink | AggregatedEdge>;
  } | null>(() => {
    if (!graphVisibleData) return null;
    if (aggregatedGraphData) {
      return { nodes: aggregatedGraphData.nodes, links: aggregatedGraphData.links };
    }
    return graphVisibleData;
  }, [aggregatedGraphData, graphVisibleData]);

  const graphSearchableNodes = useMemo<Array<KnowledgeGraphNode | AggregatedNode>>(() => {
    if (!graphRenderData) return [];
    return graphRenderData.nodes;
  }, [graphRenderData]);

  const graphSelectedAggregateNode = useMemo(() => {
    if (!graphSelectedAggregateId || !aggregatedGraphData) return null;
    return (
      aggregatedGraphData.nodes.find(
        (node) => node.isAggregate && node.id === graphSelectedAggregateId,
      ) ?? null
    );
  }, [aggregatedGraphData, graphSelectedAggregateId]);

  const graphSelectedAggregateMembers = useMemo(() => {
    if (!graphSelectedAggregateNode || !graphVisibleData) return [] as KnowledgeGraphNode[];
    return graphVisibleData.nodes
      .filter((node) => (node.group ?? "") === graphSelectedAggregateNode.id)
      .sort((l, r) => l.label.localeCompare(r.label, "zh-CN"));
  }, [graphSelectedAggregateNode, graphVisibleData]);

  const graphSearchHits = useMemo(() => {
    const query = graphSearchQuery.trim().toLowerCase();
    if (!query) return new Set<string>();
    const hits = new Set<string>();
    for (const node of graphSearchableNodes) {
      if (
        (node.label || "").toLowerCase().includes(query) ||
        (node.id || "").toLowerCase().includes(query)
      ) {
        hits.add(node.id);
      }
    }
    return hits;
  }, [graphSearchQuery, graphSearchableNodes]);

  const graphVisibleNormalizedEdges = useMemo<GraphNormalizedEdge[]>(() => {
    if (!graphVisibleData) return [];
    return graphVisibleData.links
      .map((link) => ({
        sourceId: resolveGraphLinkNodeId(link.source as string | KnowledgeGraphNode | null | undefined),
        targetId: resolveGraphLinkNodeId(link.target as string | KnowledgeGraphNode | null | undefined),
      }))
      .filter((edge) => edge.sourceId && edge.targetId);
  }, [graphVisibleData]);

  const graphInsightConfig = useMemo<GraphInsightConfig>(
    () => ({
      ...DEFAULT_GRAPH_INSIGHT_CONFIG,
      sparseDensityThreshold: clampGraphInsightSparseDensity(graphInsightSparseDensity),
      bridgeMinGroups: clampGraphInsightBridgeMinGroups(graphInsightBridgeMinGroups),
      surprisingMaxJaccard: clampGraphInsightSurprisingJaccard(graphInsightSurprisingJaccard),
      surprisingMinConfidence: clampGraphInsightSurprisingConfidence(graphInsightSurprisingConfidence),
    }),
    [
      graphInsightBridgeMinGroups,
      graphInsightSparseDensity,
      graphInsightSurprisingConfidence,
      graphInsightSurprisingJaccard,
    ],
  );

  const graphInsights = useMemo<GraphInsightItem[]>(() => {
    if (!graphVisibleData) return [];
    return buildGraphInsights(graphVisibleData.nodes, graphVisibleNormalizedEdges, 8, graphInsightConfig, graphEmbeddingSim);
  }, [graphEmbeddingSim, graphInsightConfig, graphVisibleData, graphVisibleNormalizedEdges]);

  // ── Effects ────────────────────────────────────────────────────────────────

  // Backend subgraph for large local graphs
  useEffect(() => {
    if (
      activeModule !== "graph" ||
      !graphShouldUseBackendSubgraph ||
      !graphSelectedNodeId ||
      graphViewMode !== "local"
    ) {
      setGraphLocalSubgraphData(null);
      setGraphLocalSubgraphLoading(false);
      setGraphLocalSubgraphError("");
      setGraphLocalSubgraphTruncated(false);
      return;
    }
    let cancelled = false;
    void (async () => {
      setGraphLocalSubgraphLoading(true);
      setGraphLocalSubgraphError("");
      try {
        const subgraph = await getKnowledgeSubgraph({
          centerPagePath: graphSelectedNodeId,
          hop: graphLocalDepth,
          direction: graphLocalDirection,
          limitNodes: GRAPH_LOCAL_BACKEND_MAX_NODES,
          limitLinks: GRAPH_LOCAL_BACKEND_MAX_LINKS,
        });
        if (cancelled) return;
        if (!subgraph) {
          setGraphLocalSubgraphData(null);
          setGraphLocalSubgraphTruncated(false);
          setGraphLocalSubgraphError("后端未返回子图数据，已回退前端计算。");
          return;
        }
        setGraphLocalSubgraphData({ nodes: subgraph.nodes, links: subgraph.links });
        setGraphLocalSubgraphTruncated(Boolean(subgraph.meta?.truncated));
      } catch (err) {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        setGraphLocalSubgraphData(null);
        setGraphLocalSubgraphTruncated(false);
        setGraphLocalSubgraphError(`后端子图计算失败：${message}。已回退前端计算。`);
      } finally {
        if (!cancelled) setGraphLocalSubgraphLoading(false);
      }
    })();
    return () => { cancelled = true; };
  }, [
    activeModule,
    graphLocalDepth,
    graphLocalDirection,
    graphSelectedNodeId,
    graphShouldUseBackendSubgraph,
    graphViewMode,
  ]);

  // Container resize observer
  useEffect(() => {
    const updateSize = () => {
      if (graphContainerRef.current) {
        setGraphDimensions({
          width: graphContainerRef.current.clientWidth || 800,
          height: graphContainerRef.current.clientHeight || 600,
        });
      }
    };
    updateSize();
    window.addEventListener("resize", updateSize);
    return () => window.removeEventListener("resize", updateSize);
  }, [activeModule]);

  // Deselect node if it disappears from visible data
  useEffect(() => {
    if (!graphSelectedNodeId || !graphVisibleData) return;
    const exists = graphVisibleData.nodes.some((node) =>
      isSameWikiPagePath(node.id, graphSelectedNodeId),
    );
    if (!exists) {
      setGraphSelectedNodeId("");
      setGraphNeighborOnly(false);
    }
  }, [graphSelectedNodeId, graphVisibleData]);

  // Reset group filter if group disappears
  useEffect(() => {
    if (graphGroupFilter === "__all__" || graphGroupFilter === "__ungrouped__") return;
    if (!graphGroupOptions.includes(graphGroupFilter)) {
      setGraphGroupFilter("__all__");
    }
  }, [graphGroupFilter, graphGroupOptions]);

  // Persist view settings to localStorage
  useEffect(() => { writeGraphViewModeToStorage(graphViewMode); }, [graphViewMode]);
  useEffect(() => { writeGraphLocalDepthToStorage(graphLocalDepth); }, [graphLocalDepth]);
  useEffect(() => { writeGraphLocalDirectionToStorage(graphLocalDirection); }, [graphLocalDirection]);
  useEffect(() => { writeGraphInsightSparseDensityToStorage(graphInsightSparseDensity); }, [graphInsightSparseDensity]);
  useEffect(() => { writeGraphInsightBridgeMinGroupsToStorage(graphInsightBridgeMinGroups); }, [graphInsightBridgeMinGroups]);
  useEffect(() => { writeGraphInsightSurprisingJaccardToStorage(graphInsightSurprisingJaccard); }, [graphInsightSurprisingJaccard]);
  useEffect(() => { writeGraphInsightSurprisingConfidenceToStorage(graphInsightSurprisingConfidence); }, [graphInsightSurprisingConfidence]);

  // Ctrl+F to focus search box when graph tab is active
  useEffect(() => {
    if (activeModule !== "graph") return;
    const handleKeyDown = (e: globalThis.KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        graphSearchInputRef.current?.focus();
        graphSearchInputRef.current?.select();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [activeModule]);

  // Auto-disable aggregate mode when node count drops below threshold
  useEffect(() => {
    if ((graphVisibleData?.nodes.length ?? 0) <= GRAPH_AGGREGATE_THRESHOLD && graphAggregateMode) {
      setGraphAggregateMode(false);
    }
  }, [graphAggregateMode, graphVisibleData?.nodes.length]);

  // Clear aggregate selection when aggregate mode is off or the node disappears
  useEffect(() => {
    if (!graphAggregateMode) {
      if (graphSelectedAggregateId) setGraphSelectedAggregateId("");
      return;
    }
    if (!graphSelectedAggregateId || !aggregatedGraphData) return;
    const exists = aggregatedGraphData.nodes.some(
      (node) => node.isAggregate && node.id === graphSelectedAggregateId,
    );
    if (!exists) setGraphSelectedAggregateId("");
  }, [aggregatedGraphData, graphAggregateMode, graphSelectedAggregateId]);

  // Layout freeze / unfreeze effect
  useEffect(() => {
    if (!graphRef.current) return;
    if (graphLayoutFrozen) {
      graphRef.current.pauseAnimation?.();
      return;
    }
    graphRef.current.resumeAnimation?.();
    graphRef.current.d3ReheatSimulation?.();
  }, [graphLayoutFrozen, graphRenderData?.links.length, graphRenderData?.nodes.length]);

  // Async embedding similarity (silent degradation)
  useEffect(() => {
    if (!graphVisibleData) return;
    const paths = graphVisibleData.nodes.map((n) => n.id);
    if (paths.length === 0) return;
    let cancelled = false;
    getPageEmbeddingPairs(paths)
      .then((result) => {
        if (!cancelled) {
          setGraphEmbeddingSim(Object.keys(result).length > 0 ? result : undefined);
        }
      })
      .catch(() => {
        // Silent degradation: keep lexical distance when embeddings unavailable
      });
    return () => { cancelled = true; };
  }, [graphVisibleData]);

  // ── Handlers ───────────────────────────────────────────────────────────────

  const handleGraphNodeClick = (node: object) => {
    const graphNode = node as any;
    if (graphNode?.isAggregate) {
      setGraphSelectedNodeId("");
      setGraphSelectedAggregateId(graphNode.id);
      setGraphNeighborOnly(false);
      setStatusMessage(`已选中聚合节点「${graphNode.label || graphNode.id}」，可在右侧展开成员页。`);
      return;
    }
    const pagePath = resolveGraphNodePagePath(graphNode);
    if (!pagePath) {
      setStatusMessage("图谱节点数据异常，无法选中。");
      return;
    }
    setGraphSelectedNodeId(pagePath);
    setGraphSelectedAggregateId("");
    if (graphRef.current && typeof graphNode.x === "number" && typeof graphNode.y === "number") {
      graphRef.current.centerAt(graphNode.x, graphNode.y, 400);
      graphRef.current.zoom(2.0, 400);
    }
  };

  const handleExportGraphJson = () => {
    if (!graphRenderData) return;
    const normalizedPayload = {
      exported_at: new Date().toISOString(),
      view_mode: graphViewMode,
      aggregate_mode: Boolean(aggregatedGraphData),
      nodes: graphRenderData.nodes.map((node) => {
        const n = node as KnowledgeGraphNode & AggregatedNode;
        return {
          id: n.id,
          label: n.label,
          group: n.group ?? "",
          is_aggregate: Boolean(n.isAggregate),
          count: n.isAggregate ? (n.count ?? 1) : 1,
        };
      }),
      links: graphRenderData.links
        .map((link) => {
          const edge = link as KnowledgeGraphLink & AggregatedEdge;
          const sourceId = resolveGraphLinkNodeId(edge.source as string | KnowledgeGraphNode | null | undefined);
          const targetId = resolveGraphLinkNodeId(edge.target as string | KnowledgeGraphNode | null | undefined);
          if (!sourceId || !targetId) return null;
          return { source: sourceId, target: targetId, weight: edge.weight ?? 1 };
        })
        .filter((edge): edge is { source: string; target: string; weight: number } => Boolean(edge)),
    };
    const json = JSON.stringify(normalizedPayload, null, 2);
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `llm-wiki-graph-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleOpenSelectedGraphNode = async () => {
    if (!graphSelectedNode) return;
    setActiveModule("wiki" as ModuleId);
    await handleOpenWikiPage(graphSelectedNode.id);
  };

  const handleOpenAggregateMemberPage = async (memberPath: string) => {
    if (!memberPath.trim()) return;
    setGraphAggregateMode(false);
    setGraphSelectedAggregateId("");
    setGraphSelectedNodeId(memberPath);
    setActiveModule("wiki" as ModuleId);
    await handleOpenWikiPage(memberPath);
  };

  const handleExpandSelectedAggregateNode = () => {
    if (!graphSelectedAggregateNode) return;
    const targetGroup = graphSelectedAggregateNode.id;
    const nextSelected = graphSelectedAggregateMembers[0]?.id ?? "";
    setGraphAggregateMode(false);
    setGraphGroupFilter(targetGroup);
    setGraphNeighborOnly(false);
    setGraphSelectedAggregateId("");
    if (nextSelected) {
      setGraphSelectedNodeId(nextSelected);
      setStatusMessage(`已切回明细模式，分组「${targetGroup}」包含 ${graphSelectedAggregateMembers.length} 个页面。`);
    } else {
      setStatusMessage(`已切回明细模式，但分组「${targetGroup}」暂无可展示页面。`);
    }
  };

  const handleExitAggregateMode = () => {
    setGraphAggregateMode(false);
    setGraphSelectedAggregateId("");
    setStatusMessage("已切回明细模式。");
  };

  const handleApplyGraphInsight = (insight: GraphInsightItem) => {
    if (insight.group) setGraphGroupFilter(insight.group);
    setGraphAggregateMode(false);
    setGraphSelectedAggregateId("");
    setGraphSearchQuery("");
    if (insight.nodeIds.length === 0) {
      setStatusMessage(`已定位洞察：${insight.title}`);
      return;
    }
    const targetNodeId = insight.nodeIds[0];
    setGraphSelectedNodeId(targetNodeId);
    setGraphNeighborOnly(insight.kind === "bridge-node" || insight.kind === "surprising-link");
    if (insight.kind === "surprising-link" && insight.nodeIds.length > 1) {
      const pairedNodeId = insight.nodeIds[1];
      const pairedLabel = graphNodeById.get(pairedNodeId)?.label || resolveGraphNodeLeafName(pairedNodeId);
      setStatusMessage(`已定位洞察：${insight.title}（配对节点：${pairedLabel}）`);
      return;
    }
    setStatusMessage(`已定位洞察：${insight.title}`);
  };

  const handleGraphZoomToFit = () => {
    graphRef.current?.zoomToFit?.(350, 40);
  };

  const handleGraphViewModeChange = (mode: GraphViewMode) => {
    setGraphViewMode(mode);
    if (mode === "global") return;
    if (!graphSelectedNodeId) {
      setStatusMessage("已切换为 Local 图模式，请先点击一个节点作为中心。");
    }
  };

  const handleGraphLocalDepthChange = (value: number) => {
    setGraphLocalDepth(clampGraphLocalDepth(value));
  };

  const handleGraphLocalDirectionChange = (value: string) => {
    if (!isGraphTraversalDirection(value)) return;
    setGraphLocalDirection(value);
  };

  const handleGraphInsightSparseDensityChange = (value: number) => {
    setGraphInsightSparseDensity(clampGraphInsightSparseDensity(value));
  };

  const handleGraphInsightBridgeMinGroupsChange = (value: number) => {
    setGraphInsightBridgeMinGroups(clampGraphInsightBridgeMinGroups(value));
  };

  const handleGraphInsightSurprisingJaccardChange = (value: number) => {
    setGraphInsightSurprisingJaccard(clampGraphInsightSurprisingJaccard(value));
  };

  const handleGraphInsightSurprisingConfidenceChange = (value: number) => {
    setGraphInsightSurprisingConfidence(clampGraphInsightSurprisingConfidence(value));
  };

  const handleToggleGraphLayoutFreeze = () => {
    setGraphLayoutFrozen((prev) => !prev);
  };

  const handleResetGraphFilters = () => {
    setGraphViewMode("global");
    setGraphLocalDepth(1);
    setGraphLocalDirection("both");
    setGraphLocalSubgraphData(null);
    setGraphLocalSubgraphLoading(false);
    setGraphLocalSubgraphError("");
    setGraphLocalSubgraphTruncated(false);
    setGraphGroupFilter("__all__");
    setGraphShowOrphans(true);
    setGraphNeighborOnly(false);
    setGraphInsightSparseDensity(DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold);
    setGraphInsightBridgeMinGroups(DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups);
    setGraphInsightSurprisingJaccard(DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard);
    setGraphInsightSurprisingConfidence(DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence);
    setGraphLayoutFrozen(false);
    setGraphAggregateMode(false);
    setGraphSelectedAggregateId("");
  };

  // ── Render ─────────────────────────────────────────────────────────────────

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
                        if (pagePath) { setActiveModule("wiki" as ModuleId); void handleOpenWikiPage(pagePath); }
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
