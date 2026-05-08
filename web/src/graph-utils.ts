import type { KnowledgeGraphNode, KnowledgeGraphData } from "./types";

// ---- Graph types ----

export type GraphNormalizedEdge = {
  sourceId: string;
  targetId: string;
};

export type GraphInsightKind = "isolated-node" | "sparse-group" | "bridge-node" | "surprising-link";

export type GraphInsightItem = {
  kind: GraphInsightKind;
  title: string;
  description: string;
  suggestion: string;
  nodeIds: string[];
  group?: string;
  score: number;
  evidence: string[];
};

export type GraphInsightConfig = {
  isolatedMaxDegree: number;
  sparseGroupMinSize: number;
  sparseDensityThreshold: number;
  bridgeMinGroups: number;
  bridgeCandidateLimit: number;
  surprisingMaxJaccard: number;
  surprisingMinConfidence: number;
  surprisingCandidateLimit: number;
};

/** 聚合后的超节点 */
export type AggregatedNode = {
  id: string;
  label: string;
  group: string;
  isAggregate: boolean;
  count: number;
};

/** 聚合后的边（weight 为折叠的边数） */
export type AggregatedEdge = {
  source: string;
  target: string;
  weight: number;
};

/** 聚合后的图谱数据结构 */
export type AggregatedGraphData = {
  nodes: AggregatedNode[];
  links: AggregatedEdge[];
};

export type GraphViewMode = "global" | "local";
export type GraphTraversalDirection = "both" | "out" | "in";

// ---- Graph constants ----

/** 大图聚合模式触发阈值（节点数超过此值时可启用） */
export const GRAPH_AGGREGATE_THRESHOLD = 200;

export const GRAPH_VIEW_MODE_STORAGE_KEY = "llm_wiki_graph_view_mode_v1";
export const GRAPH_LOCAL_DEPTH_STORAGE_KEY = "llm_wiki_graph_local_depth_v1";
export const GRAPH_LOCAL_DIRECTION_STORAGE_KEY = "llm_wiki_graph_local_direction_v1";
export const GRAPH_INSIGHT_SPARSE_DENSITY_STORAGE_KEY = "llm_wiki_graph_insight_sparse_density_v1";
export const GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_STORAGE_KEY = "llm_wiki_graph_insight_bridge_min_groups_v1";
export const GRAPH_INSIGHT_SURPRISING_JACCARD_STORAGE_KEY = "llm_wiki_graph_insight_surprising_jaccard_v1";
export const GRAPH_INSIGHT_SURPRISING_CONFIDENCE_STORAGE_KEY =
  "llm_wiki_graph_insight_surprising_confidence_v1";
export const GRAPH_LOCAL_DEPTH_MIN = 1;
export const GRAPH_LOCAL_DEPTH_MAX = 3;
export const GRAPH_LOCAL_BACKEND_NODE_THRESHOLD = 1200;
export const GRAPH_LOCAL_BACKEND_LINK_THRESHOLD = 4000;
export const GRAPH_LOCAL_BACKEND_MAX_NODES = 1500;
export const GRAPH_LOCAL_BACKEND_MAX_LINKS = 8000;
export const GRAPH_INSIGHT_SPARSE_DENSITY_MIN = 0.05;
export const GRAPH_INSIGHT_SPARSE_DENSITY_MAX = 0.6;
export const GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN = 2;
export const GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX = 6;
export const GRAPH_INSIGHT_SURPRISING_JACCARD_MIN = 0;
export const GRAPH_INSIGHT_SURPRISING_JACCARD_MAX = 0.6;
export const GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN = 0.3;
export const GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX = 0.95;

export const DEFAULT_GRAPH_INSIGHT_CONFIG: GraphInsightConfig = {
  isolatedMaxDegree: 1,
  sparseGroupMinSize: 3,
  sparseDensityThreshold: 0.15,
  bridgeMinGroups: 3,
  bridgeCandidateLimit: 3,
  surprisingMaxJaccard: 0.1,
  surprisingMinConfidence: 0.55,
  surprisingCandidateLimit: 3,
};

export const graphInsightKindLabels: Record<GraphInsightKind, string> = {
  "isolated-node": "孤立页",
  "sparse-group": "稀疏分组",
  "bridge-node": "桥接节点",
  "surprising-link": "异常连接",
};

// ---- Graph functions ----

export const resolveGraphNodePagePath = (node: Partial<KnowledgeGraphNode> | null | undefined) => {
  if (!node || typeof node.id !== "string") {
    return "";
  }
  return node.id.trim();
};

export const isGraphViewMode = (value: string): value is GraphViewMode =>
  value === "global" || value === "local";

export const isGraphTraversalDirection = (value: string): value is GraphTraversalDirection =>
  value === "both" || value === "out" || value === "in";

export const shouldUseBackendSubgraph = (input: {
  viewMode: GraphViewMode;
  selectedNodeId: string;
  totalNodes: number;
  totalLinks: number;
}) => {
  if (input.viewMode !== "local") {
    return false;
  }
  if (!input.selectedNodeId.trim()) {
    return false;
  }
  return (
    input.totalNodes > GRAPH_LOCAL_BACKEND_NODE_THRESHOLD ||
    input.totalLinks > GRAPH_LOCAL_BACKEND_LINK_THRESHOLD
  );
};

export const readGraphViewModeFromStorage = (): GraphViewMode => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return "global";
    }
    const raw = storage.getItem(GRAPH_VIEW_MODE_STORAGE_KEY);
    if (!raw) {
      return "global";
    }
    return isGraphViewMode(raw) ? raw : "global";
  } catch {
    return "global";
  }
};

export const writeGraphViewModeToStorage = (mode: GraphViewMode) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(GRAPH_VIEW_MODE_STORAGE_KEY, mode);
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphLocalDirectionFromStorage = (): GraphTraversalDirection => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return "both";
    }
    const raw = storage.getItem(GRAPH_LOCAL_DIRECTION_STORAGE_KEY);
    if (!raw) {
      return "both";
    }
    return isGraphTraversalDirection(raw) ? raw : "both";
  } catch {
    return "both";
  }
};

export const writeGraphLocalDirectionToStorage = (direction: GraphTraversalDirection) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(GRAPH_LOCAL_DIRECTION_STORAGE_KEY, direction);
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const clampGraphLocalDepth = (depth: number) =>
  Math.max(GRAPH_LOCAL_DEPTH_MIN, Math.min(GRAPH_LOCAL_DEPTH_MAX, depth));

export const clampGraphInsightSparseDensity = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_SPARSE_DENSITY_MIN,
    Math.min(
      GRAPH_INSIGHT_SPARSE_DENSITY_MAX,
      Number.isFinite(value) ? Number(value.toFixed(2)) : DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold,
    ),
  );

export const clampGraphInsightBridgeMinGroups = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MIN,
    Math.min(
      GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_MAX,
      Number.isFinite(value) ? Math.round(value) : DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups,
    ),
  );

export const clampGraphInsightSurprisingJaccard = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_SURPRISING_JACCARD_MIN,
    Math.min(
      GRAPH_INSIGHT_SURPRISING_JACCARD_MAX,
      Number.isFinite(value) ? Number(value.toFixed(2)) : DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard,
    ),
  );

export const clampGraphInsightSurprisingConfidence = (value: number) =>
  Math.max(
    GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MIN,
    Math.min(
      GRAPH_INSIGHT_SURPRISING_CONFIDENCE_MAX,
      Number.isFinite(value) ? Number(value.toFixed(2)) : DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence,
    ),
  );

export const readGraphLocalDepthFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return 1;
    }
    const raw = storage.getItem(GRAPH_LOCAL_DEPTH_STORAGE_KEY);
    if (!raw) {
      return 1;
    }
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) {
      return 1;
    }
    return clampGraphLocalDepth(Math.round(parsed));
  } catch {
    return 1;
  }
};

export const writeGraphLocalDepthToStorage = (depth: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(GRAPH_LOCAL_DEPTH_STORAGE_KEY, String(clampGraphLocalDepth(depth)));
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphInsightSparseDensityFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_SPARSE_DENSITY_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold;
    }
    const parsed = Number(raw);
    return clampGraphInsightSparseDensity(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.sparseDensityThreshold;
  }
};

export const writeGraphInsightSparseDensityToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_SPARSE_DENSITY_STORAGE_KEY,
      String(clampGraphInsightSparseDensity(value)),
    );
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphInsightBridgeMinGroupsFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups;
    }
    const parsed = Number(raw);
    return clampGraphInsightBridgeMinGroups(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.bridgeMinGroups;
  }
};

export const writeGraphInsightBridgeMinGroupsToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_BRIDGE_MIN_GROUPS_STORAGE_KEY,
      String(clampGraphInsightBridgeMinGroups(value)),
    );
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphInsightSurprisingJaccardFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_SURPRISING_JACCARD_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard;
    }
    const parsed = Number(raw);
    return clampGraphInsightSurprisingJaccard(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMaxJaccard;
  }
};

export const writeGraphInsightSurprisingJaccardToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_SURPRISING_JACCARD_STORAGE_KEY,
      String(clampGraphInsightSurprisingJaccard(value)),
    );
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readGraphInsightSurprisingConfidenceFromStorage = (): number => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence;
    }
    const raw = storage.getItem(GRAPH_INSIGHT_SURPRISING_CONFIDENCE_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence;
    }
    const parsed = Number(raw);
    return clampGraphInsightSurprisingConfidence(parsed);
  } catch {
    return DEFAULT_GRAPH_INSIGHT_CONFIG.surprisingMinConfidence;
  }
};

export const writeGraphInsightSurprisingConfidenceToStorage = (value: number) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(
      GRAPH_INSIGHT_SURPRISING_CONFIDENCE_STORAGE_KEY,
      String(clampGraphInsightSurprisingConfidence(value)),
    );
  } catch {
    // 本地存储不可用时静默降级
  }
};

/**
 * 将原始 KnowledgeGraphData 按 group 聚合：
 * - group 内节点数 >= groupMinSize 的节点合并为超节点
 * - 其余节点（ungrouped 或小组）保持原样
 * - 同组内部的边（自环）去除
 * - 跨超节点的多条边合并（weight 累加）
 */
export function buildAggregatedGraphData(
  nodes: KnowledgeGraphNode[],
  links: GraphNormalizedEdge[],
  groupMinSize: number = 2,
): AggregatedGraphData {
  if (nodes.length === 0) {
    return { nodes: [], links: [] };
  }

  const groupCount = new Map<string, number>();
  for (const node of nodes) {
    if (node.group) {
      groupCount.set(node.group, (groupCount.get(node.group) ?? 0) + 1);
    }
  }

  const shouldAggregate = (group: string): boolean =>
    Boolean(group) && (groupCount.get(group) ?? 0) >= groupMinSize;

  const nodeToAgg = new Map<string, string>();
  for (const node of nodes) {
    if (shouldAggregate(node.group)) {
      nodeToAgg.set(node.id, node.group);
    } else {
      nodeToAgg.set(node.id, node.id);
    }
  }

  const aggNodeMap = new Map<string, AggregatedNode>();
  for (const node of nodes) {
    const aggId = nodeToAgg.get(node.id)!;
    if (!aggNodeMap.has(aggId)) {
      const isAgg = shouldAggregate(node.group);
      aggNodeMap.set(aggId, {
        id: aggId,
        label: isAgg ? `${node.group} (${groupCount.get(node.group)})` : node.label,
        group: node.group,
        isAggregate: isAgg,
        count: isAgg ? (groupCount.get(node.group) ?? 1) : 1,
      });
    }
  }

  const edgeWeightMap = new Map<string, number>();
  for (const link of links) {
    const srcAgg = nodeToAgg.get(link.sourceId);
    const tgtAgg = nodeToAgg.get(link.targetId);
    if (!srcAgg || !tgtAgg) continue;
    if (srcAgg === tgtAgg) continue;
    const key = `${srcAgg}|||${tgtAgg}`;
    edgeWeightMap.set(key, (edgeWeightMap.get(key) ?? 0) + 1);
  }

  const aggLinks: AggregatedEdge[] = [];
  for (const [key, weight] of edgeWeightMap) {
    const [source, target] = key.split("|||");
    aggLinks.push({ source, target, weight });
  }

  return {
    nodes: Array.from(aggNodeMap.values()),
    links: aggLinks,
  };
}

// isSameWikiPagePath is needed internally - import from wiki-utils would create a circular dep,
// so we keep a local copy for graph-utils internal use only.
const normalizeForCompare = (path: string | null | undefined) =>
  (path ?? "")
    .trim()
    .replace(/^\\\\\?\\UNC\\/i, "\\\\")
    .replace(/^\\\\\?\\/i, "")
    .replaceAll("\\", "/")
    .toLowerCase();

const isSameWikiPagePath = (left: string | null | undefined, right: string | null | undefined) => {
  const normalizedLeft = normalizeForCompare(left);
  const normalizedRight = normalizeForCompare(right);
  return Boolean(normalizedLeft) && normalizedLeft === normalizedRight;
};

export const buildGraphVisibleData = (input: {
  nodes: KnowledgeGraphNode[];
  edges: GraphNormalizedEdge[];
  totalDegree: Map<string, number>;
  groupFilter: string;
  showOrphans: boolean;
  neighborOnly: boolean;
  selectedNodeId: string;
}): KnowledgeGraphData => {
  let visibleNodes = input.nodes.filter((node) => {
    const group = node.group?.trim() ?? "";
    if (input.groupFilter === "__all__") {
      return true;
    }
    if (input.groupFilter === "__ungrouped__") {
      return !group;
    }
    return group === input.groupFilter;
  });

  if (!input.showOrphans) {
    visibleNodes = visibleNodes.filter((node) => (input.totalDegree.get(node.id) ?? 0) > 0);
  }

  let visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
  let visibleEdges = input.edges.filter(
    (edge) => visibleNodeIds.has(edge.sourceId) && visibleNodeIds.has(edge.targetId),
  );

  if (input.neighborOnly && input.selectedNodeId && visibleNodeIds.has(input.selectedNodeId)) {
    const neighborIds = new Set<string>([input.selectedNodeId]);
    for (const edge of visibleEdges) {
      if (isSameWikiPagePath(edge.sourceId, input.selectedNodeId)) {
        neighborIds.add(edge.targetId);
      }
      if (isSameWikiPagePath(edge.targetId, input.selectedNodeId)) {
        neighborIds.add(edge.sourceId);
      }
    }
    visibleNodes = visibleNodes.filter((node) => neighborIds.has(node.id));
    visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
    visibleEdges = visibleEdges.filter(
      (edge) => visibleNodeIds.has(edge.sourceId) && visibleNodeIds.has(edge.targetId),
    );
  }

  return {
    nodes: visibleNodes.map((node) => ({ ...node })),
    links: visibleEdges.map((edge) => ({ source: edge.sourceId, target: edge.targetId })),
  };
};

export const buildGraphLocalData = (input: {
  nodes: KnowledgeGraphNode[];
  edges: GraphNormalizedEdge[];
  selectedNodeId: string;
  maxDepth: number;
  direction: GraphTraversalDirection;
}): KnowledgeGraphData => {
  const centerId = input.selectedNodeId.trim();
  if (!centerId) {
    return { nodes: [], links: [] };
  }

  const undirectedAdjacency = new Map<string, Set<string>>();
  const outboundAdjacency = new Map<string, Set<string>>();
  const inboundAdjacency = new Map<string, Set<string>>();
  for (const node of input.nodes) {
    undirectedAdjacency.set(node.id, new Set());
    outboundAdjacency.set(node.id, new Set());
    inboundAdjacency.set(node.id, new Set());
  }
  for (const edge of input.edges) {
    if (!undirectedAdjacency.has(edge.sourceId)) {
      undirectedAdjacency.set(edge.sourceId, new Set());
      outboundAdjacency.set(edge.sourceId, new Set());
      inboundAdjacency.set(edge.sourceId, new Set());
    }
    if (!undirectedAdjacency.has(edge.targetId)) {
      undirectedAdjacency.set(edge.targetId, new Set());
      outboundAdjacency.set(edge.targetId, new Set());
      inboundAdjacency.set(edge.targetId, new Set());
    }
    undirectedAdjacency.get(edge.sourceId)?.add(edge.targetId);
    undirectedAdjacency.get(edge.targetId)?.add(edge.sourceId);
    outboundAdjacency.get(edge.sourceId)?.add(edge.targetId);
    inboundAdjacency.get(edge.targetId)?.add(edge.sourceId);
  }

  if (!undirectedAdjacency.has(centerId)) {
    return { nodes: [], links: [] };
  }

  const depthLimit = clampGraphLocalDepth(input.maxDepth);
  const visited = new Set<string>([centerId]);
  const queue: Array<{ id: string; depth: number }> = [{ id: centerId, depth: 0 }];

  while (queue.length > 0) {
    const current = queue.shift();
    if (!current) {
      continue;
    }
    if (current.depth >= depthLimit) {
      continue;
    }
    const neighbors =
      input.direction === "out"
        ? outboundAdjacency.get(current.id)
        : input.direction === "in"
          ? inboundAdjacency.get(current.id)
          : undirectedAdjacency.get(current.id);
    if (!neighbors) {
      continue;
    }
    for (const neighborId of neighbors) {
      if (visited.has(neighborId)) {
        continue;
      }
      visited.add(neighborId);
      queue.push({ id: neighborId, depth: current.depth + 1 });
    }
  }

  const visibleNodes = input.nodes
    .filter((node) => visited.has(node.id))
    .map((node) => ({ ...node }));
  const visibleNodeIds = new Set(visibleNodes.map((node) => node.id));
  const visibleEdges = input.edges
    .filter((edge) => visibleNodeIds.has(edge.sourceId) && visibleNodeIds.has(edge.targetId))
    .map((edge) => ({ source: edge.sourceId, target: edge.targetId }));

  return {
    nodes: visibleNodes,
    links: visibleEdges,
  };
};

const GRAPH_STRUCTURAL_PAGE_NAMES = new Set(["index", "log", "overview"]);

const resolveGraphNodeLeafName = (id: string) =>
  id
    .trim()
    .replaceAll("\\", "/")
    .split("/")
    .pop()
    ?.replace(/\.md$/i, "")
    .toLowerCase() ?? "";

const isGraphStructuralNode = (node: KnowledgeGraphNode) =>
  GRAPH_STRUCTURAL_PAGE_NAMES.has(resolveGraphNodeLeafName(node.id));

const buildUndirectedEdgeKey = (left: string, right: string) =>
  left < right ? `${left}:::${right}` : `${right}:::${left}`;

/**
 * 基于当前图谱结构计算可操作洞察：
 * - 孤立页（度数 <= 1）
 * - 稀疏分组（同组密度低于阈值）
 * - 桥接节点（邻居覆盖分组数高于阈值）
 * - 异常连接（跨组且共同邻居相似度低）
 */
export const buildGraphInsights = (
  nodes: KnowledgeGraphNode[],
  edges: GraphNormalizedEdge[],
  limit: number = 8,
  configOverrides?: Partial<GraphInsightConfig>,
  embeddingSim?: Record<string, number>,
): GraphInsightItem[] => {
  if (nodes.length === 0) {
    return [];
  }

  const mergedConfig: GraphInsightConfig = {
    ...DEFAULT_GRAPH_INSIGHT_CONFIG,
    ...(configOverrides ?? {}),
  };
  const config: GraphInsightConfig = {
    isolatedMaxDegree: Math.max(0, Math.round(mergedConfig.isolatedMaxDegree)),
    sparseGroupMinSize: Math.max(2, Math.round(mergedConfig.sparseGroupMinSize)),
    sparseDensityThreshold: clampGraphInsightSparseDensity(mergedConfig.sparseDensityThreshold),
    bridgeMinGroups: clampGraphInsightBridgeMinGroups(mergedConfig.bridgeMinGroups),
    bridgeCandidateLimit: Math.max(1, Math.round(mergedConfig.bridgeCandidateLimit)),
    surprisingMaxJaccard: clampGraphInsightSurprisingJaccard(mergedConfig.surprisingMaxJaccard),
    surprisingMinConfidence: clampGraphInsightSurprisingConfidence(mergedConfig.surprisingMinConfidence),
    surprisingCandidateLimit: Math.max(1, Math.round(mergedConfig.surprisingCandidateLimit)),
  };

  const nodeMap = new Map(nodes.map((node) => [node.id, node]));
  const degreeMap = new Map<string, number>(nodes.map((node) => [node.id, 0]));
  const adjacency = new Map<string, Set<string>>(nodes.map((node) => [node.id, new Set<string>()]));
  const undirectedEdges = new Set<string>();

  for (const edge of edges) {
    const sourceId = edge.sourceId.trim();
    const targetId = edge.targetId.trim();
    if (!nodeMap.has(sourceId) || !nodeMap.has(targetId) || sourceId === targetId) {
      continue;
    }
    degreeMap.set(sourceId, (degreeMap.get(sourceId) ?? 0) + 1);
    degreeMap.set(targetId, (degreeMap.get(targetId) ?? 0) + 1);
    adjacency.get(sourceId)?.add(targetId);
    adjacency.get(targetId)?.add(sourceId);
    undirectedEdges.add(buildUndirectedEdgeKey(sourceId, targetId));
  }

  const insights: GraphInsightItem[] = [];
  const visibleNonStructuralNodes = nodes.filter((node) => !isGraphStructuralNode(node));
  const visibleNonStructuralNodeIds = new Set(visibleNonStructuralNodes.map((node) => node.id));
  const groupLabel = (group: string) => (group === "__ungrouped__" ? "未分组" : group);
  const resolveNodeLabel = (nodeId: string) =>
    nodeMap.get(nodeId)?.label?.trim() || resolveGraphNodeLeafName(nodeId);
  const toTokenSet = (value: string) => {
    const normalized = value
      .toLowerCase()
      .replace(/\.md$/i, "")
      .replace(/[_\-\\/]+/g, " ");
    const tokens = normalized
      .split(/[^a-z0-9一-龥]+/i)
      .map((item) => item.trim())
      .filter((item) => item.length >= 2);
    return new Set(tokens);
  };
  const buildNeighborSetWithoutPair = (nodeId: string, excludeId: string) => {
    const neighbors = adjacency.get(nodeId);
    if (!neighbors) {
      return new Set<string>();
    }
    const next = new Set<string>();
    for (const neighborId of neighbors) {
      if (!isSameWikiPagePath(neighborId, excludeId)) {
        next.add(neighborId);
      }
    }
    return next;
  };
  const calcJaccard = (left: Set<string>, right: Set<string>) => {
    if (left.size === 0 && right.size === 0) {
      return 0;
    }
    let intersection = 0;
    for (const item of left) {
      if (right.has(item)) {
        intersection += 1;
      }
    }
    const union = left.size + right.size - intersection;
    return union > 0 ? intersection / union : 0;
  };

  const calcLexicalDistance = (sourceId: string, targetId: string) => {
    const sourceLabel = resolveNodeLabel(sourceId);
    const targetLabel = resolveNodeLabel(targetId);
    const sourceTokens = toTokenSet(`${sourceLabel} ${resolveGraphNodeLeafName(sourceId)}`);
    const targetTokens = toTokenSet(`${targetLabel} ${resolveGraphNodeLeafName(targetId)}`);
    if (sourceTokens.size === 0 && targetTokens.size === 0) {
      return 1;
    }
    const overlap = calcJaccard(sourceTokens, targetTokens);
    return 1 - overlap;
  };

  const isolatedNodes = visibleNonStructuralNodes.filter(
    (node) => (degreeMap.get(node.id) ?? 0) <= config.isolatedMaxDegree,
  );
  if (isolatedNodes.length > 0) {
    const previewLabels = isolatedNodes
      .slice(0, 5)
      .map((node) => node.label || resolveGraphNodeLeafName(node.id));
    insights.push({
      kind: "isolated-node",
      title: `${isolatedNodes.length} 个孤立页面`,
      description:
        previewLabels.join("、") +
        (isolatedNodes.length > 5 ? ` 等 ${isolatedNodes.length} 个页面连接较弱。` : " 连接较弱。"),
      suggestion: "为这些页面补充 [[wiki-link]] 或补充相关引用，提升知识可达性。",
      nodeIds: isolatedNodes.map((node) => node.id),
      score: isolatedNodes.length,
      evidence: [
        `命中条件：度数 ≤ ${config.isolatedMaxDegree}`,
        `当前命中：${isolatedNodes.length} 个页面`,
      ],
    });
  }

  const groupedNodeIds = new Map<string, string[]>();
  for (const node of visibleNonStructuralNodes) {
    const group = node.group.trim();
    if (!group) {
      continue;
    }
    const bucket = groupedNodeIds.get(group) ?? [];
    bucket.push(node.id);
    groupedNodeIds.set(group, bucket);
  }

  for (const [group, memberIds] of groupedNodeIds) {
    if (memberIds.length < config.sparseGroupMinSize) {
      continue;
    }
    let internalEdges = 0;
    for (let left = 0; left < memberIds.length; left += 1) {
      for (let right = left + 1; right < memberIds.length; right += 1) {
        if (undirectedEdges.has(buildUndirectedEdgeKey(memberIds[left], memberIds[right]))) {
          internalEdges += 1;
        }
      }
    }
    const possibleEdges = (memberIds.length * (memberIds.length - 1)) / 2;
    const density = possibleEdges > 0 ? internalEdges / possibleEdges : 0;
    if (density <= config.sparseDensityThreshold) {
      insights.push({
        kind: "sparse-group",
        title: `稀疏分组：${group}`,
        description: `${memberIds.length} 个页面，组内密度 ${density.toFixed(2)}，内部连接偏弱。`,
        suggestion: "优先补齐该分组内部页面互链，减少知识断层。",
        nodeIds: memberIds,
        group,
        score: 1 + (config.sparseDensityThreshold - density),
        evidence: [
          `命中条件：密度 ≤ ${config.sparseDensityThreshold.toFixed(2)}，分组大小 ≥ ${config.sparseGroupMinSize}`,
          `当前密度：${density.toFixed(2)}（${internalEdges}/${possibleEdges}）`,
        ],
      });
    }
  }

  const bridgeCandidates = visibleNonStructuralNodes
    .map((node) => {
      const neighborIds = adjacency.get(node.id);
      if (!neighborIds || neighborIds.size === 0) {
        return null;
      }
      const neighborGroups = new Set<string>();
      for (const neighborId of neighborIds) {
        const neighbor = nodeMap.get(neighborId);
        if (!neighbor) {
          continue;
        }
        const groupName = neighbor.group.trim() || "__ungrouped__";
        neighborGroups.add(groupName);
      }
      if (neighborGroups.size < config.bridgeMinGroups) {
        return null;
      }
      return {
        node,
        groupCount: neighborGroups.size,
        degree: degreeMap.get(node.id) ?? 0,
      };
    })
    .filter((item): item is { node: KnowledgeGraphNode; groupCount: number; degree: number } => item !== null)
    .sort((left, right) => {
      if (left.groupCount !== right.groupCount) {
        return right.groupCount - left.groupCount;
      }
      return right.degree - left.degree;
    })
    .slice(0, config.bridgeCandidateLimit);

  for (const candidate of bridgeCandidates) {
    insights.push({
      kind: "bridge-node",
      title: `关键桥接页：${candidate.node.label || resolveGraphNodeLeafName(candidate.node.id)}`,
      description: `连接 ${candidate.groupCount} 个分组，总连接数 ${candidate.degree}。`,
      suggestion: "该页面是关键通路，建议优先维护并补充上下游引用。",
      nodeIds: [candidate.node.id],
      score: candidate.groupCount,
      evidence: [
        `命中条件：邻居分组数 ≥ ${config.bridgeMinGroups}`,
        `当前分组覆盖：${candidate.groupCount}，节点度数：${candidate.degree}`,
      ],
    });
  }

  const surprisingCandidates: Array<{
    sourceId: string;
    targetId: string;
    sourceGroup: string;
    targetGroup: string;
    sourceDegree: number;
    targetDegree: number;
    jaccard: number;
    lexicalDistance: number;
    crossGroupRarity: number;
    confidence: number;
    score: number;
    embSim: number | null;
  }> = [];
  const visitedSurprisingKeys = new Set<string>();
  const groupPairEdgeCount = new Map<string, number>();
  for (const edge of edges) {
    const sourceId = edge.sourceId.trim();
    const targetId = edge.targetId.trim();
    if (!sourceId || !targetId || sourceId === targetId) {
      continue;
    }
    if (!visibleNonStructuralNodeIds.has(sourceId) || !visibleNonStructuralNodeIds.has(targetId)) {
      continue;
    }
    const sourceNode = nodeMap.get(sourceId);
    const targetNode = nodeMap.get(targetId);
    if (!sourceNode || !targetNode) {
      continue;
    }
    const sourceGroup = sourceNode.group.trim() || "__ungrouped__";
    const targetGroup = targetNode.group.trim() || "__ungrouped__";
    if (sourceGroup === targetGroup) {
      continue;
    }
    const pairKey = buildUndirectedEdgeKey(sourceGroup, targetGroup);
    groupPairEdgeCount.set(pairKey, (groupPairEdgeCount.get(pairKey) ?? 0) + 1);
  }
  for (const edge of edges) {
    const sourceId = edge.sourceId.trim();
    const targetId = edge.targetId.trim();
    if (!sourceId || !targetId || sourceId === targetId) {
      continue;
    }
    if (!visibleNonStructuralNodeIds.has(sourceId) || !visibleNonStructuralNodeIds.has(targetId)) {
      continue;
    }
    const dedupKey = buildUndirectedEdgeKey(sourceId, targetId);
    if (visitedSurprisingKeys.has(dedupKey)) {
      continue;
    }
    visitedSurprisingKeys.add(dedupKey);
    const sourceNode = nodeMap.get(sourceId);
    const targetNode = nodeMap.get(targetId);
    if (!sourceNode || !targetNode) {
      continue;
    }
    const sourceGroup = sourceNode.group.trim() || "__ungrouped__";
    const targetGroup = targetNode.group.trim() || "__ungrouped__";
    if (sourceGroup === targetGroup) {
      continue;
    }
    const sourceNeighbors = buildNeighborSetWithoutPair(sourceId, targetId);
    const targetNeighbors = buildNeighborSetWithoutPair(targetId, sourceId);
    const jaccard = calcJaccard(sourceNeighbors, targetNeighbors);
    if (jaccard > config.surprisingMaxJaccard) {
      continue;
    }
    const sourceDegree = degreeMap.get(sourceId) ?? 0;
    const targetDegree = degreeMap.get(targetId) ?? 0;
    const lexicalDistance = calcLexicalDistance(sourceId, targetId);
    const pairKey = buildUndirectedEdgeKey(sourceGroup, targetGroup);
    const groupPairCount = groupPairEdgeCount.get(pairKey) ?? 1;
    const crossGroupRarity = 1 / Math.sqrt(groupPairCount);
    const embSimKey =
      sourceId < targetId ? `${sourceId}||${targetId}` : `${targetId}||${sourceId}`;
    const embSim = embeddingSim ? (embeddingSim[embSimKey] ?? null) : null;
    const semanticFactor = embSim !== null ? 1 - embSim : lexicalDistance;
    const confidence = Number(
      (
        (1 - jaccard) * 0.55 +
        semanticFactor * 0.25 +
        crossGroupRarity * 0.2
      ).toFixed(2),
    );
    if (confidence < config.surprisingMinConfidence) {
      continue;
    }
    const score = confidence * 2 + Math.min((sourceDegree + targetDegree) / 8, 1);
    surprisingCandidates.push({
      sourceId,
      targetId,
      sourceGroup,
      targetGroup,
      sourceDegree,
      targetDegree,
      jaccard,
      lexicalDistance,
      crossGroupRarity,
      confidence,
      score,
      embSim,
    });
  }

  surprisingCandidates
    .sort((left, right) => right.score - left.score)
    .slice(0, config.surprisingCandidateLimit)
    .forEach((candidate) => {
      const semanticLabel =
        candidate.embSim !== null
          ? `语义相似度: ${candidate.embSim.toFixed(2)}`
          : `词汇距离=${candidate.lexicalDistance.toFixed(2)}`;
      insights.push({
        kind: "surprising-link",
        title: `异常连接：${resolveNodeLabel(candidate.sourceId)} ↔ ${resolveNodeLabel(candidate.targetId)}`,
        description: `${groupLabel(candidate.sourceGroup)} 与 ${groupLabel(candidate.targetGroup)} 存在低相似跨组连接。`,
        suggestion: "建议核查该连接的引用依据，确认是高价值桥接还是误链。",
        nodeIds: [candidate.sourceId, candidate.targetId],
        score: candidate.score,
        evidence: [
          `命中条件：Jaccard ≤ ${config.surprisingMaxJaccard.toFixed(2)} 且置信度 ≥ ${config.surprisingMinConfidence.toFixed(2)}`,
          `当前：Jaccard=${candidate.jaccard.toFixed(2)}，${semanticLabel}，跨组稀有度=${candidate.crossGroupRarity.toFixed(2)}`,
          `当前置信度：${candidate.confidence.toFixed(2)}，度数：${candidate.sourceDegree}/${candidate.targetDegree}`,
        ],
      });
    });

  return insights
    .sort((left, right) => {
      if (left.kind === right.kind) {
        return right.score - left.score;
      }
      const weight: Record<GraphInsightKind, number> = {
        "surprising-link": 4,
        "bridge-node": 3,
        "isolated-node": 2,
        "sparse-group": 1,
      };
      return weight[right.kind] - weight[left.kind];
    })
    .slice(0, limit);
};

