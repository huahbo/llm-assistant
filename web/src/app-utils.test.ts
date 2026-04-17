import { afterEach, describe, expect, it } from "vitest";
import {
  buildFrontmatterCopyText,
  buildLlmProviderConfig,
  buildCloudProviderPresetConfig,
  buildWikiFrontmatterDisplay,
  cloudProviderPresets,
  formatQueryAnswerStrategyLabel,
  formatQuerySearchStrategyLabel,
  buildWikiSummaryDisplay,
  buildWikiHighlightSegments,
  tokenizeWikiKeyword,
  sortWikiPages,
  buildWikiTreeNodes,
  WIKI_SORT_MODE_STORAGE_KEY,
  isWikiSortMode,
  parseLegacyImportedAtFromContent,
  buildGraphVisibleData,
  buildGraphLocalData,
  clampGraphLocalDepth,
  GRAPH_LOCAL_DEPTH_STORAGE_KEY,
  GRAPH_LOCAL_DIRECTION_STORAGE_KEY,
  GRAPH_VIEW_MODE_STORAGE_KEY,
  isSameWikiPagePath,
  isGraphTraversalDirection,
  isGraphViewMode,
  normalizeWikiPathForCompare,
  shouldUseBackendSubgraph,
  readGraphLocalDirectionFromStorage,
  readGraphLocalDepthFromStorage,
  readGraphViewModeFromStorage,
  resolveGraphNodePagePath,
  parseLegacyWikiMetadataFromContent,
  formatPdfIngestErrorMessage,
  readWikiSortModeFromStorage,
  resolveWikiImportedAtDebugValue,
  resolveNextActiveProvider,
  hasUnsavedWikiEditChanges,
  shouldAutoDismissStatusMessage,
  writeGraphLocalDepthToStorage,
  writeGraphLocalDirectionToStorage,
  writeGraphViewModeToStorage,
  writeWikiSortModeToStorage,
  GRAPH_LOCAL_BACKEND_LINK_THRESHOLD,
  GRAPH_LOCAL_BACKEND_NODE_THRESHOLD,
  groupLintIssuesByPath,
  groupPatchPreviewItemsByPath,
  OCR_PROVIDER_STORAGE_KEY,
  isOcrProvider,
  readOcrProviderFromStorage,
  writeOcrProviderToStorage,
  formatEditorCharCount,
  QUERY_HISTORY_STORAGE_KEY,
  QUERY_HISTORY_MAX,
  parseQueryProgressPayload,
  normalizeQueryHistory,
  mergeQueryHistory,
  readQueryHistoryFromStorage,
  writeQueryHistoryToStorage,
  normalizeQueryHistoryItems,
  mergeQueryHistoryItems,
  filterQueryHistoryItems,
  formatAskHistoryCreatedAt,
  readQueryHistoryItemsFromStorage,
  writeQueryHistoryItemsToStorage,
} from "./App";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import {
  createIngestFileArgs,
  createIngestMarkdownArgs,
  createIngestPdfArgs,
  createIngestUrlArgs,
  createQueryAskArgs,
  createQueryAskWithOptionsArgs,
  createPreviewLintPatchesArgs,
  createFetchRecentLintPatchEventsArgs,
  createApplyLintPatchArgs,
  createApplyLintPatchesBatchArgs,
  createSaveQueryAnswerArgs,
  createSaveWikiPageArgs,
  createSearchWikiPagesArgs,
  createWikiPageCitationsArgs,
  createWikiPageDetailArgs,
  createSetOcrConfigArgs,
  createSetQueryTopKArgs,
  createVaultInitArgs,
  formatLlmStatusSummary,
  isTauriRuntime,
  ingestFile,
  ingestPdf,
  applyLintPatch,
  applyLintPatchesBatch,
  fetchRecentLintPatchEvents,
  normalizeLintPatchPreviewResponse,
  normalizeLlmStatus,
  normalizeLlmProviderConfig,
  saveLlmConfig,
  saveWikiPage,
  resolveDisplayPath,
} from "./tauri-client";
import {
  filterLintIssuesByCode,
  filterLintIssuesByPath,
  filterLintIssuesBySeverity,
  filterLintIssuesBySuggestion,
  formatLintCheckedAt,
  normalizeLintSeverity,
  readLintFilterState,
  resolveLintSeverityStats,
  writeLintFilterState,
} from "./lint-utils";

describe("格式化函数", () => {
  it("格式化运行模式", () => {
    expect(formatBackendMode("Hybrid")).toBe("Hybrid");
    expect(formatBackendMode("StrictLocal")).toBe("Strict Local");
  });

  it("格式化日志级别", () => {
    expect(formatLogLevel("Info")).toBe("Info");
    expect(formatLogLevel("Warn")).toBe("Warn");
    expect(formatLogLevel("Error")).toBe("Error");
  });
});

describe("Wiki frontmatter 展示构建", () => {
  it("生成 frontmatter 复制文本", () => {
    expect(buildFrontmatterCopyText("source", "wiki/source.md")).toBe("source: wiki/source.md");
  });

  it("在无详情时返回空结构", () => {
    expect(buildWikiFrontmatterDisplay(null)).toEqual({
      frontmatter: null,
      rows: [],
      entities: [],
      totalCount: 0,
      hasMeta: false,
    });
  });

  it("从旧格式元数据段落提取 source/raw", () => {
    expect(
      parseLegacyWikiMetadataFromContent(`
- Source: \`E:\\llm-wiki\\test-llm.md\`
- Raw: \`E:\\llm-wiki\\vault\\raw\\test.md\`
- Imported at: 1775811471352
`),
    ).toEqual({
      source: "E:\\llm-wiki\\test-llm.md",
      raw: "E:\\llm-wiki\\vault\\raw\\test.md",
    });
  });

  it("从旧格式元数据段落提取 imported_at", () => {
    expect(
      parseLegacyImportedAtFromContent(`
- Source: \`E:\\llm-wiki\\test-llm.md\`
- Imported at: 1775811471352
`),
    ).toBe("1775811471352");
  });

  it("过滤空字段并统计实体", () => {
    const result = buildWikiFrontmatterDisplay({
      title: "Page A",
      path: "wiki/a.md",
      display_path: "wiki/a.md",
      content: "# A",
      updated_at: "1",
      frontmatter: {
        title: "  ",
        source: "source/a.md",
        raw: "raw/a.md",
        imported_at: "2026-04-15T10:00:00+08:00",
        entities: ["Rust", " ", "SQLite"],
      },
    });

    expect(result.rows).toEqual([
      { key: "source", label: "source", value: "source/a.md", displayValue: "source/a.md" },
      { key: "raw", label: "raw", value: "raw/a.md", displayValue: "raw/a.md" },
    ]);
    expect(result.entities).toEqual(["Rust", "SQLite"]);
    expect(result.totalCount).toBe(3);
    expect(result.hasMeta).toBe(true);
  });

  it("调试信息优先取 frontmatter 的 imported_at", () => {
    expect(
      resolveWikiImportedAtDebugValue({
        title: "Page A",
        path: "wiki/a.md",
        display_path: "wiki/a.md",
        content: "- Imported at: 111",
        updated_at: "1",
        frontmatter: {
          imported_at: "222",
        },
      }),
    ).toBe("222");
  });

  it("无 frontmatter 时调试信息回退旧格式 imported_at", () => {
    expect(
      resolveWikiImportedAtDebugValue({
        title: "Legacy Page",
        path: "wiki/legacy.md",
        display_path: "wiki/legacy.md",
        content: "- Imported at: 1775811471352",
        updated_at: "1",
        frontmatter: null,
      }),
    ).toBe("1775811471352");
  });

  it("在缺少 YAML frontmatter 时回退解析旧格式元数据并可展示", () => {
    const result = buildWikiFrontmatterDisplay({
      title: "Legacy Page",
      path: "wiki/legacy.md",
      display_path: "wiki/legacy.md",
      content: `
# Legacy Page

- Source: \`E:\\llm-wiki\\legacy.md\`
- Raw: \`E:\\llm-wiki\\vault\\raw\\legacy.md\`
- Imported at: 1775811471352
`,
      updated_at: "1",
      frontmatter: null,
    });

    expect(result.rows).toEqual([
      {
        key: "source",
        label: "source",
        value: "E:\\llm-wiki\\legacy.md",
        displayValue: "E:\\llm-wiki\\legacy.md",
      },
      {
        key: "raw",
        label: "raw",
        value: "E:\\llm-wiki\\vault\\raw\\legacy.md",
        displayValue: "E:\\llm-wiki\\vault\\raw\\legacy.md",
      },
    ]);
    expect(result.hasMeta).toBe(true);
  });
});

describe("Wiki 摘要展示与高亮", () => {
  it("关键词分词去重并过滤短英文噪声", () => {
    expect(tokenizeWikiKeyword("Rust rust, a, qa, 本地, 本地")).toEqual(["rust", "qa", "本地"]);
  });

  it("摘要折叠时按行数截断", () => {
    const multiLine = "第一行\n第二行\n第三行\n第四行\n第五行";
    const result = buildWikiSummaryDisplay(multiLine, false, 3);
    expect(result).toEqual({
      text: "第一行\n第二行\n第三行...",
      isTruncated: true,
    });
  });

  it("摘要展开或行数未超限时不截断", () => {
    expect(buildWikiSummaryDisplay("单行摘要", false, 3)).toEqual({
      text: "单行摘要",
      isTruncated: false,
    });
    expect(buildWikiSummaryDisplay("第一行\n第二行\n第三行", true, 3)).toEqual({
      text: "第一行\n第二行\n第三行",
      isTruncated: false,
    });
  });

  it("按关键词生成高亮片段", () => {
    expect(buildWikiHighlightSegments("Rust + SQLite + 本地", ["rust", "本地"])).toEqual([
      { text: "Rust", matched: true },
      { text: " + SQLite + ", matched: false },
      { text: "本地", matched: true },
    ]);
  });
});

describe("Wiki 列表排序", () => {
  const source = [
    {
      title: "zeta",
      path: "wiki/z.md",
      summary: "z",
      updated_at: "1000",
    },
    {
      title: "alpha",
      path: "wiki/a.md",
      summary: "a",
      updated_at: "2000",
    },
    {
      title: "beta",
      path: "wiki/b.md",
      summary: "b",
      updated_at: "1500",
    },
  ];

  it("按更新时间新到旧排序", () => {
    expect(sortWikiPages(source, "updated_desc").map((item) => item.title)).toEqual([
      "alpha",
      "beta",
      "zeta",
    ]);
  });

  it("按更新时间旧到新排序", () => {
    expect(sortWikiPages(source, "updated_asc").map((item) => item.title)).toEqual([
      "zeta",
      "beta",
      "alpha",
    ]);
  });

  it("按标题排序", () => {
    expect(sortWikiPages(source, "title_asc").map((item) => item.title)).toEqual([
      "alpha",
      "beta",
      "zeta",
    ]);
  });
});

describe("Wiki 文件树构建", () => {
  it("按目录层级构建树并保持文件在目录后排序", () => {
    const tree = buildWikiTreeNodes([
      {
        title: "Rust",
        path: "C:/vault/wiki/rust/intro.md",
        display_path: "wiki/rust/intro.md",
        summary: "",
        updated_at: "1",
      },
      {
        title: "SQLite",
        path: "C:/vault/wiki/rust/sqlite.md",
        display_path: "wiki/rust/sqlite.md",
        summary: "",
        updated_at: "1",
      },
      {
        title: "Index",
        path: "C:/vault/wiki/index.md",
        display_path: "wiki/index.md",
        summary: "",
        updated_at: "1",
      },
    ]);

    expect(tree.map((node) => node.name)).toEqual(["wiki"]);
    const wikiNode = tree[0];
    expect(wikiNode.kind).toBe("folder");
    expect(wikiNode.children.map((node) => node.name)).toEqual(["rust", "index.md"]);
    expect(wikiNode.children[0].children.map((node) => node.name)).toEqual([
      "intro.md",
      "sqlite.md",
    ]);
  });

  it("无 display_path 时兼容反斜杠路径", () => {
    const tree = buildWikiTreeNodes([
      {
        title: "A",
        path: "E:\\vault\\wiki\\topic\\a.md",
        summary: "",
        updated_at: "1",
      },
      {
        title: "B",
        path: "E:\\vault\\wiki\\topic\\sub\\b.md",
        summary: "",
        updated_at: "1",
      },
    ]);

    expect(tree[0].name.toLowerCase()).toContain("e:");
    expect(tree[0].children[0].name).toBe("vault");
    expect(tree[0].children[0].children[0].name).toBe("wiki");
  });
});

describe("Wiki 排序偏好持久化", () => {
  const installLocalStorageMock = (initial: Record<string, string> = {}) => {
    const store = new Map(Object.entries(initial));
    const localStorageMock = {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => {
        store.set(key, String(value));
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => {
        store.clear();
      },
    };

    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: localStorageMock,
    });

    return { store };
  };

  afterEach(() => {
    Reflect.deleteProperty(globalThis, "localStorage");
  });

  it("识别合法排序值", () => {
    expect(isWikiSortMode("updated_desc")).toBe(true);
    expect(isWikiSortMode("updated_asc")).toBe(true);
    expect(isWikiSortMode("title_asc")).toBe(true);
    expect(isWikiSortMode("invalid")).toBe(false);
  });

  it("默认读取为 updated_desc", () => {
    Reflect.deleteProperty(globalThis, "localStorage");
    expect(readWikiSortModeFromStorage()).toBe("updated_desc");
  });

  it("读取存储中的合法值", () => {
    installLocalStorageMock({
      [WIKI_SORT_MODE_STORAGE_KEY]: "title_asc",
    });
    expect(readWikiSortModeFromStorage()).toBe("title_asc");
  });

  it("存储非法值时回退默认", () => {
    installLocalStorageMock({
      [WIKI_SORT_MODE_STORAGE_KEY]: "unknown_sort_mode",
    });
    expect(readWikiSortModeFromStorage()).toBe("updated_desc");
  });

  it("可写入排序偏好", () => {
    const { store } = installLocalStorageMock();
    writeWikiSortModeToStorage("updated_asc");
    expect(store.get(WIKI_SORT_MODE_STORAGE_KEY)).toBe("updated_asc");
  });
});

describe("图谱视图偏好持久化", () => {
  const installLocalStorageMock = (initial: Record<string, string> = {}) => {
    const store = new Map(Object.entries(initial));
    const localStorageMock = {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => {
        store.set(key, String(value));
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => {
        store.clear();
      },
    };

    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: localStorageMock,
    });

    return { store };
  };

  afterEach(() => {
    Reflect.deleteProperty(globalThis, "localStorage");
  });

  it("识别合法图谱视图模式", () => {
    expect(isGraphViewMode("global")).toBe(true);
    expect(isGraphViewMode("local")).toBe(true);
    expect(isGraphViewMode("invalid")).toBe(false);
  });

  it("图谱视图模式默认 global", () => {
    Reflect.deleteProperty(globalThis, "localStorage");
    expect(readGraphViewModeFromStorage()).toBe("global");
  });

  it("图谱视图模式可读写", () => {
    const { store } = installLocalStorageMock();
    writeGraphViewModeToStorage("local");
    expect(store.get(GRAPH_VIEW_MODE_STORAGE_KEY)).toBe("local");
    expect(readGraphViewModeFromStorage()).toBe("local");
  });

  it("图谱 local 深度读写时自动裁剪", () => {
    const { store } = installLocalStorageMock({
      [GRAPH_LOCAL_DEPTH_STORAGE_KEY]: "100",
    });
    expect(readGraphLocalDepthFromStorage()).toBe(3);
    expect(clampGraphLocalDepth(0)).toBe(1);
    writeGraphLocalDepthToStorage(9);
    expect(store.get(GRAPH_LOCAL_DEPTH_STORAGE_KEY)).toBe("3");
  });

  it("图谱 local 方向可读写", () => {
    const { store } = installLocalStorageMock();
    expect(isGraphTraversalDirection("both")).toBe(true);
    expect(isGraphTraversalDirection("out")).toBe(true);
    expect(isGraphTraversalDirection("in")).toBe(true);
    expect(isGraphTraversalDirection("none")).toBe(false);
    writeGraphLocalDirectionToStorage("in");
    expect(store.get(GRAPH_LOCAL_DIRECTION_STORAGE_KEY)).toBe("in");
    expect(readGraphLocalDirectionFromStorage()).toBe("in");
  });
});

describe("Wiki 路径比较", () => {
  it("统一路径分隔符与大小写用于比较", () => {
    expect(normalizeWikiPathForCompare("E:\\LLM-Wiki\\vault\\wiki\\A.md")).toBe(
      "e:/llm-wiki/vault/wiki/a.md",
    );
  });

  it("识别同一路径（Windows 反斜杠与大小写差异）", () => {
    expect(
      isSameWikiPagePath(
        "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
        "e:/LLM-WIKI/vault/wiki/ingest-1.md",
      ),
    ).toBe(true);
  });

  it("识别同一路径（Windows 规范路径前缀差异）", () => {
    expect(
      isSameWikiPagePath(
        "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
        "\\\\?\\E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
      ),
    ).toBe(true);
  });

  it("空路径不视为同一路径", () => {
    expect(isSameWikiPagePath("", "")).toBe(false);
  });

  it("图谱节点路径解析仅接受非空字符串", () => {
    expect(resolveGraphNodePagePath({ id: "  E:\\llm-wiki\\vault\\wiki\\a.md  " })).toBe(
      "E:\\llm-wiki\\vault\\wiki\\a.md",
    );
    expect(resolveGraphNodePagePath({ id: "" })).toBe("");
    expect(resolveGraphNodePagePath({ id: 1 as unknown as string })).toBe("");
    expect(resolveGraphNodePagePath(null)).toBe("");
  });
});

describe("图谱过滤与邻居模式", () => {
  const graphNodes = [
    { id: "A", label: "A", group: "alpha" },
    { id: "B", label: "B", group: "alpha" },
    { id: "C", label: "C", group: "" },
  ];
  const graphEdges = [{ sourceId: "A", targetId: "B" }];
  const totalDegree = new Map<string, number>([
    ["A", 1],
    ["B", 1],
    ["C", 0],
  ]);

  it("可按是否展示孤儿页过滤", () => {
    const withoutOrphans = buildGraphVisibleData({
      nodes: graphNodes,
      edges: graphEdges,
      totalDegree,
      groupFilter: "__all__",
      showOrphans: false,
      neighborOnly: false,
      selectedNodeId: "",
    });
    expect(withoutOrphans.nodes.map((node) => node.id)).toEqual(["A", "B"]);
  });

  it("邻居模式只保留选中节点与一跳邻居", () => {
    const neighborOnly = buildGraphVisibleData({
      nodes: graphNodes,
      edges: graphEdges,
      totalDegree,
      groupFilter: "__all__",
      showOrphans: true,
      neighborOnly: true,
      selectedNodeId: "B",
    });
    expect(neighborOnly.nodes.map((node) => node.id).sort()).toEqual(["A", "B"]);
    expect(neighborOnly.links).toHaveLength(1);
  });

  it("可按分组筛选节点", () => {
    const grouped = buildGraphVisibleData({
      nodes: graphNodes,
      edges: graphEdges,
      totalDegree,
      groupFilter: "alpha",
      showOrphans: true,
      neighborOnly: false,
      selectedNodeId: "",
    });
    expect(grouped.nodes.map((node) => node.id).sort()).toEqual(["A", "B"]);
  });

  it("Local 子图按 hop 深度裁剪", () => {
    const localDepth1 = buildGraphLocalData({
      nodes: [
        { id: "A", label: "A", group: "" },
        { id: "B", label: "B", group: "" },
        { id: "C", label: "C", group: "" },
      ],
      edges: [
        { sourceId: "A", targetId: "B" },
        { sourceId: "B", targetId: "C" },
      ],
      selectedNodeId: "A",
      maxDepth: 1,
      direction: "both",
    });
    expect(localDepth1.nodes.map((node) => node.id).sort()).toEqual(["A", "B"]);

    const localDepth2 = buildGraphLocalData({
      nodes: [
        { id: "A", label: "A", group: "" },
        { id: "B", label: "B", group: "" },
        { id: "C", label: "C", group: "" },
      ],
      edges: [
        { sourceId: "A", targetId: "B" },
        { sourceId: "B", targetId: "C" },
      ],
      selectedNodeId: "A",
      maxDepth: 2,
      direction: "both",
    });
    expect(localDepth2.nodes.map((node) => node.id).sort()).toEqual(["A", "B", "C"]);
  });

  it("Local 子图支持方向遍历", () => {
    const directional = buildGraphLocalData({
      nodes: [
        { id: "A", label: "A", group: "" },
        { id: "B", label: "B", group: "" },
        { id: "C", label: "C", group: "" },
      ],
      edges: [
        { sourceId: "A", targetId: "B" },
        { sourceId: "B", targetId: "C" },
      ],
      selectedNodeId: "B",
      maxDepth: 1,
      direction: "out",
    });
    expect(directional.nodes.map((node) => node.id).sort()).toEqual(["B", "C"]);

    const reverseDirectional = buildGraphLocalData({
      nodes: [
        { id: "A", label: "A", group: "" },
        { id: "B", label: "B", group: "" },
        { id: "C", label: "C", group: "" },
      ],
      edges: [
        { sourceId: "A", targetId: "B" },
        { sourceId: "B", targetId: "C" },
      ],
      selectedNodeId: "B",
      maxDepth: 1,
      direction: "in",
    });
    expect(reverseDirectional.nodes.map((node) => node.id).sort()).toEqual(["A", "B"]);
  });

  it("大图 Local 模式触发后端子图策略", () => {
    expect(
      shouldUseBackendSubgraph({
        viewMode: "global",
        selectedNodeId: "A",
        totalNodes: GRAPH_LOCAL_BACKEND_NODE_THRESHOLD + 10,
        totalLinks: 10,
      }),
    ).toBe(false);
    expect(
      shouldUseBackendSubgraph({
        viewMode: "local",
        selectedNodeId: "",
        totalNodes: GRAPH_LOCAL_BACKEND_NODE_THRESHOLD + 10,
        totalLinks: GRAPH_LOCAL_BACKEND_LINK_THRESHOLD + 10,
      }),
    ).toBe(false);
    expect(
      shouldUseBackendSubgraph({
        viewMode: "local",
        selectedNodeId: "A",
        totalNodes: GRAPH_LOCAL_BACKEND_NODE_THRESHOLD + 1,
        totalLinks: 10,
      }),
    ).toBe(true);
    expect(
      shouldUseBackendSubgraph({
        viewMode: "local",
        selectedNodeId: "A",
        totalNodes: 10,
        totalLinks: GRAPH_LOCAL_BACKEND_LINK_THRESHOLD + 1,
      }),
    ).toBe(true);
  });
});

describe("状态提示自动收起策略", () => {
  it("成功类消息可自动收起", () => {
    expect(shouldAutoDismissStatusMessage("已打开页面：ingest-1")).toBe(true);
  });

  it("失败/错误类消息不自动收起", () => {
    expect(shouldAutoDismissStatusMessage("读取页面失败：xxx")).toBe(false);
    expect(shouldAutoDismissStatusMessage("Error: xxx")).toBe(false);
  });

  it("进行中消息不自动收起", () => {
    expect(shouldAutoDismissStatusMessage("查询中...")).toBe(false);
    expect(shouldAutoDismissStatusMessage("正在处理中")).toBe(false);
  });
});

describe("Tauri 运行时与参数映射", () => {
  afterEach(() => {
    Reflect.deleteProperty(globalThis, "window");
  });

  it("在没有 window 时识别为非 Tauri 环境", () => {
    Reflect.deleteProperty(globalThis, "window");

    expect(isTauriRuntime()).toBe(false);
  });

  it("在存在 Tauri 内部标记时识别为 Tauri 环境", () => {
    Object.defineProperty(globalThis, "window", {
      configurable: true,
      value: { __TAURI_INTERNALS__: {} },
    });

    expect(isTauriRuntime()).toBe(true);
  });

  it("同时生成 camelCase 与 snake_case 参数", () => {
    expect(createVaultInitArgs("vault")).toEqual({
      vaultPath: "vault",
      vault_path: "vault",
    });

    expect(createIngestMarkdownArgs("README.md")).toEqual({
      sourcePath: "README.md",
      source_path: "README.md",
    });

    expect(createQueryAskArgs("什么是 Query v1")).toEqual({
      question: "什么是 Query v1",
    });

    expect(createQueryAskWithOptionsArgs("什么是 Query v1", { top_k: 5 })).toEqual({
      question: "什么是 Query v1",
      options: {
        topK: 5,
        top_k: 5,
      },
    });

    expect(createSetQueryTopKArgs(6)).toEqual({
      topK: 6,
      top_k: 6,
    });

    expect(
      createSaveQueryAnswerArgs({
        question: "什么是 Query v1",
        answer: "这是一个本地检索问答流程。",
        citations: [
          {
            page_path: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
            score: 3,
            excerpt: "本项目用于实现一个 Windows 优先的个人 Wiki 桌面应用。",
          },
        ],
      }),
    ).toEqual({
      input: {
        question: "什么是 Query v1",
        answer: "这是一个本地检索问答流程。",
        citations: [
          {
            page_path: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
            score: 3,
            excerpt: "本项目用于实现一个 Windows 优先的个人 Wiki 桌面应用。",
          },
        ],
        title: undefined,
      },
    });

    expect(createSearchWikiPagesArgs("rust")).toEqual({
      keyword: "rust",
    });

    expect(createWikiPageDetailArgs("E:\\llm-wiki\\vault\\wiki\\ingest-1.md")).toEqual({
      pagePath: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
      page_path: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
    });

    expect(createWikiPageCitationsArgs("E:\\llm-wiki\\vault\\wiki\\ingest-1.md")).toEqual({
      pagePath: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
      page_path: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
    });

    expect(
      createSaveWikiPageArgs("E:\\llm-wiki\\vault\\wiki\\ingest-1.md", "# Updated"),
    ).toEqual({
      path: "E:\\llm-wiki\\vault\\wiki\\ingest-1.md",
      content: "# Updated",
    });

    expect(createPreviewLintPatchesArgs()).toEqual({});
    expect(createFetchRecentLintPatchEventsArgs()).toEqual({});

    for (const [presetId, preset] of Object.entries(cloudProviderPresets) as Array<
      [keyof typeof cloudProviderPresets, (typeof cloudProviderPresets)[keyof typeof cloudProviderPresets]]
    >) {
      expect(buildCloudProviderPresetConfig(presetId, "cloud", "sk-test")).toEqual({
        cloud_api_key: "sk-test",
        cloud_base_url: preset.baseUrl,
        cloud_model: preset.model,
        cloud_provider_name: preset.providerName,
        active_provider: "cloud",
      });
    }

    expect(
      buildLlmProviderConfig({
        activeProvider: "cloud",
        cloudApiKey: "  sk-test  ",
        cloudBaseUrl: "  https://api.deepseek.com/v1  ",
        cloudModel: "  deepseek-chat  ",
        cloudProviderName: "  DeepSeek  ",
      }),
    ).toEqual({
      cloud_api_key: "sk-test",
      cloud_base_url: "https://api.deepseek.com/v1",
      cloud_model: "deepseek-chat",
      cloud_provider_name: "DeepSeek",
      active_provider: "cloud",
    });

    expect(
      buildLlmProviderConfig({
        activeProvider: "cloud",
        cloudApiKey: "   ",
        cloudBaseUrl: "  ",
        cloudModel: "  ",
        cloudProviderName: "  ",
      }),
    ).toEqual({
      cloud_api_key: "",
      cloud_base_url: "",
      cloud_model: "",
      cloud_provider_name: "",
      active_provider: "cloud",
    });

    expect(
      buildLlmProviderConfig({
        activeProvider: "ollama",
        cloudApiKey: " sk-test ",
        cloudBaseUrl: " https://api.deepseek.com/v1 ",
        cloudModel: " deepseek-chat ",
        cloudProviderName: " DeepSeek ",
      }),
    ).toEqual({
      cloud_api_key: "sk-test",
      cloud_base_url: "https://api.deepseek.com/v1",
      cloud_model: "deepseek-chat",
      cloud_provider_name: "DeepSeek",
      active_provider: "ollama",
    });

    expect(
      createApplyLintPatchArgs({
        issue_code: "BROKEN_CITATION",
        path: "E:\\llm-wiki\\vault\\wiki\\missing.md",
        title: "修复引用目标页面",
        proposed_action: "补回被引用页面或修正引用路径",
        patch_preview: "```text\n引用目标缺失：E:\\llm-wiki\\vault\\wiki\\missing.md\n```",
      }),
    ).toEqual({
      issueCode: "BROKEN_CITATION",
      issue_code: "BROKEN_CITATION",
      path: "E:\\llm-wiki\\vault\\wiki\\missing.md",
    });

    expect(
      createApplyLintPatchesBatchArgs([
        {
          issue_code: "BROKEN_CITATION",
          path: "E:\\llm-wiki\\vault\\wiki\\missing.md",
        },
        {
          issue_code: "STRICT_LOCAL_GATE",
          path: null,
        },
      ]),
    ).toEqual({
      inputs: [
        {
          issueCode: "BROKEN_CITATION",
          issue_code: "BROKEN_CITATION",
          path: "E:\\llm-wiki\\vault\\wiki\\missing.md",
        },
        {
          issueCode: "STRICT_LOCAL_GATE",
          issue_code: "STRICT_LOCAL_GATE",
          path: null,
        },
      ],
      items: [
        {
          issueCode: "BROKEN_CITATION",
          issue_code: "BROKEN_CITATION",
          path: "E:\\llm-wiki\\vault\\wiki\\missing.md",
        },
        {
          issueCode: "STRICT_LOCAL_GATE",
          issue_code: "STRICT_LOCAL_GATE",
          path: null,
        },
      ],
    });
  });

  it("createSetOcrConfigArgs 生成正确参数", () => {
    expect(createSetOcrConfigArgs("paddle")).toEqual({ provider: "paddle" });
    expect(createSetOcrConfigArgs(null)).toEqual({ provider: null });
  });

  it("createIngestUrlArgs 生成正确参数", () => {
    expect(createIngestUrlArgs("https://example.com")).toEqual({ url: "https://example.com" });
  });

  it("createIngestPdfArgs 生成正确参数", () => {
    expect(createIngestPdfArgs("E:\\llm-wiki\\docs\\sample.pdf")).toEqual({
      sourcePath: "E:\\llm-wiki\\docs\\sample.pdf",
      source_path: "E:\\llm-wiki\\docs\\sample.pdf",
    });
  });

  it("createIngestFileArgs 生成正确参数（含 OCR Provider 映射）", () => {
    expect(createIngestFileArgs("E:\\llm-wiki\\docs\\sample.docx", "paddle")).toEqual({
      sourcePath: "E:\\llm-wiki\\docs\\sample.docx",
      source_path: "E:\\llm-wiki\\docs\\sample.docx",
      ocrProvider: "paddle",
      ocr_provider: "paddle",
    });
  });

  it("浏览器预览模式下 ingestPdf 直接回退为 null", async () => {
    Reflect.deleteProperty(globalThis, "window");

    await expect(ingestPdf("E:\\llm-wiki\\docs\\sample.pdf")).resolves.toBeNull();
  });

  it("浏览器预览模式下 ingestFile 直接回退为 null", async () => {
    Reflect.deleteProperty(globalThis, "window");

    await expect(ingestFile("E:\\llm-wiki\\docs\\sample.docx", "tesseract")).resolves.toBeNull();
  });

  it("浏览器预览模式下 saveWikiPage 直接回退为 null", async () => {
    Reflect.deleteProperty(globalThis, "window");

    await expect(
      saveWikiPage("E:\\llm-wiki\\vault\\wiki\\ingest-1.md", "# Updated"),
    ).resolves.toBeNull();
  });

  it("按 query/list/citation 对象的优先级顺序解析友好路径", () => {
    expect(
      resolveDisplayPath({
        page_path: "E:\\llm-wiki\\vault\\wiki\\query.md",
        display_path: "知识库 / Query 页面",
        displayPath: "知识库 / Query 页面（camel）",
        path: "E:\\llm-wiki\\vault\\wiki\\query-raw.md",
      }),
    ).toBe("知识库 / Query 页面");

    expect(
      resolveDisplayPath({
        path: "E:\\llm-wiki\\vault\\wiki\\list.md",
        displayPath: "知识库 / 列表页面",
      }),
    ).toBe("知识库 / 列表页面");

    expect(
      resolveDisplayPath({
        cited_page_path: "E:\\llm-wiki\\vault\\wiki\\citation.md",
        display_path: "知识库 / 引用页面（display_path）",
        cited_page_display_path: "知识库 / 引用页面",
      }),
    ).toBe("知识库 / 引用页面（display_path）");

    expect(
      resolveDisplayPath({
        cited_page_path: "E:\\llm-wiki\\vault\\wiki\\citation.md",
        cited_page_display_path: "知识库 / 引用页面",
      }),
    ).toBe("知识库 / 引用页面");

    expect(
      resolveDisplayPath({
        path: "E:\\llm-wiki\\vault\\wiki\\raw.md",
        displayPath: "  ",
      }),
    ).toBe("E:\\llm-wiki\\vault\\wiki\\raw.md");
  });
});

describe("云端 Provider 配置辅助函数", () => {
  afterEach(() => {
    Reflect.deleteProperty(globalThis, "window");
  });

  it("兼容旧 OpenAI 字段并归一化为云端字段", () => {
    expect(
      normalizeLlmProviderConfig({
        openai_api_key: "sk-old",
        openai_base_url: "https://api.deepseek.com/v1",
        openai_model: "deepseek-chat",
        openai_provider_name: "DeepSeek",
        active_provider: "openai",
      }),
    ).toEqual({
      cloud_api_key: "sk-old",
      cloud_base_url: "https://api.deepseek.com/v1",
      cloud_model: "deepseek-chat",
      cloud_provider_name: "DeepSeek",
      active_provider: "cloud",
    });
  });

  it("浏览器预览模式下保存配置直接回退为 null", async () => {
    Reflect.deleteProperty(globalThis, "window");

    await expect(
      saveLlmConfig({
        cloud_api_key: "sk-test",
        cloud_base_url: "https://api.deepseek.com/v1",
        cloud_model: "deepseek-chat",
        cloud_provider_name: "DeepSeek",
        active_provider: "cloud",
      }),
    ).resolves.toBeNull();
  });

  it("保存参数保持 cloud_* 字段结构", () => {
    expect(
      buildLlmProviderConfig({
        activeProvider: "cloud",
        cloudApiKey: "sk-test",
        cloudBaseUrl: "https://api.deepseek.com/v1",
        cloudModel: "deepseek-chat",
        cloudProviderName: "DeepSeek",
      }),
    ).toEqual({
      cloud_api_key: "sk-test",
      cloud_base_url: "https://api.deepseek.com/v1",
      cloud_model: "deepseek-chat",
      cloud_provider_name: "DeepSeek",
      active_provider: "cloud",
    });
  });

  it("当选择 cloud 且 API Key 为空时自动回退为 ollama", () => {
    expect(resolveNextActiveProvider("cloud", "   ")).toEqual({
      activeProvider: "ollama",
      fallbackMessage: "检测到你选择了云端 Provider，但 API Key 为空，已自动回退为本地 Ollama。",
    });
  });

  it("当 cloud API Key 有值时保持 cloud 选择", () => {
    expect(resolveNextActiveProvider("cloud", "sk-test")).toEqual({
      activeProvider: "cloud",
      fallbackMessage: "",
    });
  });

  it("当用户选择 ollama 时始终保持 ollama", () => {
    expect(resolveNextActiveProvider("ollama", "")).toEqual({
      activeProvider: "ollama",
      fallbackMessage: "",
    });
  });
});

describe("Wiki 编辑未保存判断", () => {
  it("仅在编辑态且内容变化时提示未保存", () => {
    expect(hasUnsavedWikiEditChanges(false, "A", "B")).toBe(false);
    expect(hasUnsavedWikiEditChanges(true, "A", "A")).toBe(false);
    expect(hasUnsavedWikiEditChanges(true, "A", "B")).toBe(true);
    expect(hasUnsavedWikiEditChanges(true, "A", null)).toBe(true);
  });
});

describe("PDF 摄入错误文案映射", () => {
  it("ToUnicode/CMap 错误映射为易懂提示并保留短原因", () => {
    const result = formatPdfIngestErrorMessage(
      "ToUnicode CMap error: Could not parse ToUnicodeCMap: Error!",
    );
    expect(result).toContain("PDF 字体映射解析失败");
    expect(result).toContain("（原因：");
  });

  it("无文本或扫描件错误映射为 OCR 提示", () => {
    const result = formatPdfIngestErrorMessage("PDF 文件未提取到任何文本内容，可能是扫描件或图片型 PDF。");
    expect(result).toContain("可能是扫描件或图片型文档，建议先做 OCR");
  });

  it("未知错误仅附短片段而非整段直出", () => {
    const raw = `底层异常:${"x".repeat(120)}`;
    const result = formatPdfIngestErrorMessage(raw);
    expect(result).toContain("PDF 摄入失败：读取 PDF 失败");
    expect(result).toContain("...");
    expect(result).not.toContain(raw);
  });
});

describe("Lint 展示辅助函数", () => {
  const sampleLintIssues = [
    { severity: "error", code: "E1", path: "wiki/error.md", suggestion: "修复错误" },
    { severity: "warning", code: "W1", path: "wiki/warn.md", suggestion: "优化警告" },
    { severity: "info", code: "I1", path: "wiki/info.md", suggestion: "参考信息" },
    { severity: "critical", code: "U1", path: "wiki/unknown.md", suggestion: "未知建议" },
    { severity: "info", code: null, path: null, suggestion: null },
  ];

  it("将 lint 时间戳格式化为北京时间字符串（精度到分钟）", () => {
    expect(formatLintCheckedAt(String(Date.UTC(2026, 3, 8, 9, 10, 11)))).toBe(
      "2026-04-08 17:10 北京时间",
    );
  });

  it("支持 ISO 时间字符串并格式化到分钟", () => {
    expect(formatLintCheckedAt("2026-04-08T09:10:11Z")).toBe("2026-04-08 17:10 北京时间");
  });

  it("对非法时间戳回退原始字符串", () => {
    expect(formatLintCheckedAt("not-a-timestamp")).toBe("not-a-timestamp");
  });

  it("归一化 lint 严重级别", () => {
    expect(normalizeLintSeverity(" Warning ")).toBe("warning");
    expect(normalizeLintSeverity("")).toBe("info");
  });

  it("有 severity_stats 时优先使用存储值", () => {
    expect(
      resolveLintSeverityStats({
        severity_stats: {
          error: 9,
          warning: 8,
          info: 7,
        },
        issues: [
          { severity: "error" },
          { severity: "warning" },
          { severity: "info" },
        ],
      }),
    ).toEqual({
      error: 9,
      warning: 8,
      info: 7,
    });
  });

  it("无 severity_stats 时从 issues 计算统计", () => {
    expect(
      resolveLintSeverityStats({
        issues: [
          { severity: "error" },
          { severity: "warning" },
          { severity: "info" },
          { severity: " warning " },
        ],
      }),
    ).toEqual({
      error: 1,
      warning: 2,
      info: 1,
    });
  });

  it("未知 severity 归入 info", () => {
    expect(
      resolveLintSeverityStats({
        issues: [
          { severity: "critical" },
          { severity: "  " },
          {},
        ],
      }),
    ).toEqual({
      error: 0,
      warning: 0,
      info: 3,
    });
  });

  it("severity 过滤器支持 all 并返回全部问题", () => {
    expect(filterLintIssuesBySeverity(sampleLintIssues, "all")).toEqual(sampleLintIssues);
  });

  it("severity 过滤器支持 error", () => {
    expect(
      filterLintIssuesBySeverity(
        [
          { severity: "error" },
          { severity: "warning" },
          { severity: "critical" },
        ],
        "error",
      ),
    ).toEqual([{ severity: "error" }]);
  });

  it("severity 过滤器支持 warning", () => {
    expect(
      filterLintIssuesBySeverity(
        [
          { severity: "error" },
          { severity: "warning" },
          { severity: "critical" },
        ],
        "warning",
      ),
    ).toEqual([{ severity: "warning" }]);
  });

  it("severity 过滤器支持 info 且未知 severity 归入 info", () => {
    expect(
      filterLintIssuesBySeverity(
        [
          { severity: "info" },
          { severity: "critical" },
          { severity: "" },
        ],
        "info",
      ),
    ).toEqual([
      { severity: "info" },
      { severity: "critical" },
      { severity: "" },
    ]);
  });

  it("code 过滤器在空关键词时返回全部问题", () => {
    expect(filterLintIssuesByCode(sampleLintIssues, "   ")).toEqual(sampleLintIssues);
  });

  it("code 过滤器支持大小写不敏感匹配", () => {
    expect(
      filterLintIssuesByCode(
        [
          { code: "BROKEN_CITATION", severity: "error" },
          { code: "lint-code-keyword", severity: "warning" },
          { code: null, severity: "info" },
        ],
        "broken",
      ),
    ).toEqual([{ code: "BROKEN_CITATION", severity: "error" }]);

    expect(
      filterLintIssuesByCode(
        [
          { code: "BROKEN_CITATION", severity: "error" },
          { code: "lint-code-keyword", severity: "warning" },
          { code: null, severity: "info" },
        ],
        "CODE",
      ),
    ).toEqual([{ code: "lint-code-keyword", severity: "warning" }]);
  });

  it("code 过滤器在无匹配时返回空数组", () => {
    expect(
      filterLintIssuesByCode(
        [
          { code: "BROKEN_CITATION", severity: "error" },
          { code: null, severity: "info" },
        ],
        "missing",
      ),
    ).toEqual([]);
  });

  it("path 过滤器在空关键词时返回全部问题", () => {
    expect(filterLintIssuesByPath(sampleLintIssues, "   ")).toEqual(sampleLintIssues);
  });

  it("path 过滤器支持大小写不敏感匹配", () => {
    expect(
      filterLintIssuesByPath(
        [
          { path: "wiki/error.md", severity: "error" },
          { path: "Wiki/Warning.MD", severity: "warning" },
          { path: null, severity: "info" },
        ],
        "WIKI",
      ),
    ).toEqual([
      { path: "wiki/error.md", severity: "error" },
      { path: "Wiki/Warning.MD", severity: "warning" },
    ]);
  });

  it("path 过滤器在无匹配时返回空数组", () => {
    expect(
      filterLintIssuesByPath(
        [
          { path: "wiki/error.md", severity: "error" },
          { path: null, severity: "info" },
        ],
        "missing",
      ),
    ).toEqual([]);
  });

  it("suggestion 过滤器在空关键词时返回全部问题", () => {
    expect(filterLintIssuesBySuggestion(sampleLintIssues, "   ")).toEqual(sampleLintIssues);
  });

  it("suggestion 过滤器支持大小写不敏感匹配", () => {
    expect(
      filterLintIssuesBySuggestion(
        [
          { suggestion: "Fix the citation", severity: "error" },
          { suggestion: "Improve warning text", severity: "warning" },
          { suggestion: null, severity: "info" },
        ],
        "CITATION",
      ),
    ).toEqual([{ suggestion: "Fix the citation", severity: "error" }]);
  });

  it("suggestion 过滤器在无匹配时返回空数组", () => {
    expect(
      filterLintIssuesBySuggestion(
        [
          { suggestion: "Fix the citation", severity: "error" },
          { suggestion: null, severity: "info" },
        ],
        "missing",
      ),
    ).toEqual([]);
  });

  it("浏览器环境下 previewLintPatches 回退为空", async () => {
    expect(await import("./tauri-client").then((mod) => mod.previewLintPatches())).toBeNull();
  });

  it("浏览器环境下 fetchRecentLintPatchEvents 回退为空数组", async () => {
    await expect(fetchRecentLintPatchEvents()).resolves.toEqual([]);
  });

  it("浏览器环境下 applyLintPatch 回退为空", async () => {
    expect(
      await import("./tauri-client").then((mod) =>
        mod.applyLintPatch({
          issue_code: "VAULT_NOT_INITIALIZED",
          title: "初始化 Vault",
          proposed_action: "先执行 init_vault 创建本地 Vault",
          patch_preview: "```text\n执行 init_vault 后将生成必要文件。\n```",
        }),
      ),
    ).toBeNull();
  });

  it("浏览器环境下 applyLintPatchesBatch 回退为空", async () => {
    expect(
      await import("./tauri-client").then((mod) =>
        mod.applyLintPatchesBatch([
          {
            issue_code: "BROKEN_CITATION",
            path: "E:\\llm-wiki\\vault\\wiki\\missing.md",
          },
        ]),
      ),
    ).toBeNull();
  });

  it("补丁预览响应兼容 suggestions 包装对象", () => {
    expect(
      normalizeLintPatchPreviewResponse({
        generated_at: "1",
        total: 1,
        suggestions: [
          {
            issue_code: "MISSING_INDEX_ENTRY",
            path: "E:\\llm-wiki\\vault\\wiki\\a.md",
            title: "补齐 index 引用",
            proposed_action: "把缺失页面加入 index.md",
            patch_preview: "- [[wiki/a.md|a]]",
          },
        ],
      }),
    ).toEqual([
      {
        issue_code: "MISSING_INDEX_ENTRY",
        path: "E:\\llm-wiki\\vault\\wiki\\a.md",
        title: "补齐 index 引用",
        proposed_action: "把缺失页面加入 index.md",
        patch_preview: "- [[wiki/a.md|a]]",
      },
    ]);
  });

  it("补丁预览响应兼容数组形式", () => {
    expect(
      normalizeLintPatchPreviewResponse([
        {
          issue_code: "STRICT_LOCAL_GATE",
          path: null,
          title: "严格本地模式提示",
          proposed_action: "无需修改",
          patch_preview: "noop",
        },
      ]),
    ).toEqual([
      {
        issue_code: "STRICT_LOCAL_GATE",
        path: null,
        title: "严格本地模式提示",
        proposed_action: "无需修改",
        patch_preview: "noop",
      },
    ]);
  });
});

describe("Lint 筛选状态持久化", () => {
  const installLocalStorageMock = (initial: Record<string, string> = {}) => {
    const store = new Map(Object.entries(initial));
    const localStorageMock = {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => {
        store.set(key, String(value));
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => {
        store.clear();
      },
    };

    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: localStorageMock,
    });

    return { store, localStorageMock };
  };

  afterEach(() => {
    Reflect.deleteProperty(globalThis, "localStorage");
  });

  it("在没有 localStorage 时回退默认值", () => {
    Reflect.deleteProperty(globalThis, "localStorage");

    expect(readLintFilterState()).toEqual({
      severity: "all",
      codeKeyword: "",
      pathKeyword: "",
      suggestionKeyword: "",
    });
  });

  it("非法 JSON 回退默认值", () => {
    installLocalStorageMock({
      llm_wiki_lint_filters_v1: "{not-json",
    });

    expect(readLintFilterState()).toEqual({
      severity: "all",
      codeKeyword: "",
      pathKeyword: "",
      suggestionKeyword: "",
    });
  });

  it("合法值可以正确读取", () => {
    installLocalStorageMock({
      llm_wiki_lint_filters_v1: JSON.stringify({
        severity: "warning",
        codeKeyword: "BROKEN",
        pathKeyword: "wiki/",
        suggestionKeyword: "fix",
      }),
    });

    expect(readLintFilterState()).toEqual({
      severity: "warning",
      codeKeyword: "BROKEN",
      pathKeyword: "wiki/",
      suggestionKeyword: "fix",
    });
  });

  it("写入后可以再次读回", () => {
    const { store } = installLocalStorageMock();

    writeLintFilterState({
      severity: "info",
      codeKeyword: "  code  ",
      pathKeyword: "  path  ",
      suggestionKeyword: "  suggest  ",
    });

    expect(store.get("llm_wiki_lint_filters_v1")).toBe(
      JSON.stringify({
        severity: "info",
        codeKeyword: "  code  ",
        pathKeyword: "  path  ",
        suggestionKeyword: "  suggest  ",
      }),
    );
    expect(readLintFilterState()).toEqual({
      severity: "info",
      codeKeyword: "  code  ",
      pathKeyword: "  path  ",
      suggestionKeyword: "  suggest  ",
    });
  });
});

describe("LLM 状态辅助函数", () => {
  it("兼容蛇形字段并提取 LLM 状态", () => {
    expect(
      normalizeLlmStatus({
        available: true,
        model_name: "llama3:8b",
        endpoint: "http://127.0.0.1:11434",
        detail: "本地 Ollama 已就绪。",
      }),
    ).toEqual({
      available: true,
      model: "llama3:8b",
      address: "http://127.0.0.1:11434",
      message: "本地 Ollama 已就绪。",
    });
  });

  it("兼容 healthy 和 base_url 字段", () => {
    expect(
      normalizeLlmStatus({
        healthy: true,
        model: "llama3.1:8b",
        base_url: "http://127.0.0.1:11434",
      }),
    ).toEqual({
      available: true,
      model: "llama3.1:8b",
      address: "http://127.0.0.1:11434",
      message: "",
    });
  });

  it("格式化可用状态的展示文案", () => {
    expect(
      formatLlmStatusSummary({
        available: true,
        model: "mistral:7b",
        address: "http://localhost:11434",
        message: "LLM 服务已就绪。",
      }),
    ).toEqual({
      availabilityText: "LLM 可用",
      modelText: "mistral:7b",
      addressText: "http://localhost:11434",
      hintText: "LLM 服务已就绪。",
    });
  });

  it("对空状态回退浏览器预览文案", () => {
    expect(formatLlmStatusSummary(null)).toEqual({
      availabilityText: "LLM 状态未读取",
      modelText: "未知模型",
      addressText: "未知地址",
      hintText: "浏览器预览模式下无法读取 LLM 状态。",
    });
  });
});

describe("Query 结果策略展示", () => {
  it("将已知策略映射为中文标签", () => {
    expect(formatQueryAnswerStrategyLabel("llm")).toBe("LLM 合成");
    expect(formatQueryAnswerStrategyLabel("rule")).toBe("规则回退");
    expect(formatQueryAnswerStrategyLabel("llm_synthesis")).toBe("LLM 合成");
    expect(formatQueryAnswerStrategyLabel("rule_fallback")).toBe("规则回退");
  });

  it("对缺失或未知策略回退为未知", () => {
    expect(formatQueryAnswerStrategyLabel(undefined)).toBe("未知");
    expect(formatQueryAnswerStrategyLabel(null)).toBe("未知");
    expect(formatQueryAnswerStrategyLabel("custom_strategy")).toBe("未知");
  });

  it("将检索策略映射为中文标签", () => {
    expect(formatQuerySearchStrategyLabel("fts")).toBe("FTS 检索");
    expect(formatQuerySearchStrategyLabel("scan")).toBe("回退扫描");
    expect(formatQuerySearchStrategyLabel("empty")).toBe("空结果");
  });

  it("对缺失或未知检索策略回退为未知", () => {
    expect(formatQuerySearchStrategyLabel(undefined)).toBe("未知");
    expect(formatQuerySearchStrategyLabel(null)).toBe("未知");
    expect(formatQuerySearchStrategyLabel("custom_strategy")).toBe("未知");
  });
});

describe("Lint 问题分组", () => {
  it("空列表返回空分组", () => {
    expect(groupLintIssuesByPath([])).toEqual([]);
  });

  it("全局问题（无 path）归入全局分组", () => {
    const issues = [
      { code: "W001", severity: "warning", message: "m1", path: null, suggestion: "s1" },
      { code: "W002", severity: "warning", message: "m2", path: undefined, suggestion: "s2" },
    ];
    const groups = groupLintIssuesByPath(issues);
    expect(groups).toHaveLength(1);
    expect(groups[0].path).toBe("全局");
    expect(groups[0].issues).toHaveLength(2);
  });

  it("按路径分组并保留顺序", () => {
    const issues = [
      { code: "E001", severity: "error", message: "m1", path: "vault/a.md", suggestion: "s1" },
      { code: "E002", severity: "error", message: "m2", path: "vault/b.md", suggestion: "s2" },
      { code: "W001", severity: "warning", message: "m3", path: "vault/a.md", suggestion: "s3" },
    ];
    const groups = groupLintIssuesByPath(issues);
    expect(groups).toHaveLength(2);
    expect(groups[0].path).toBe("vault/a.md");
    expect(groups[0].issues).toHaveLength(2);
    expect(groups[1].path).toBe("vault/b.md");
    expect(groups[1].issues).toHaveLength(1);
  });

  it("混合有路径与无路径问题", () => {
    const issues = [
      { code: "E001", severity: "error", message: "m1", path: "vault/a.md", suggestion: "s1" },
      { code: "I001", severity: "info", message: "m2", path: null, suggestion: "s2" },
    ];
    const groups = groupLintIssuesByPath(issues);
    expect(groups).toHaveLength(2);
    const paths = groups.map((g) => g.path);
    expect(paths).toContain("vault/a.md");
    expect(paths).toContain("全局");
  });
});

describe("补丁建议分组", () => {
  it("空列表返回空分组", () => {
    expect(groupPatchPreviewItemsByPath([])).toEqual([]);
  });

  it("无路径的补丁建议归入全局分组", () => {
    const items = [
      { issue_code: "W001", title: "t1", proposed_action: "a1", patch_preview: "p1", path: null },
      { issue_code: "W002", title: "t2", proposed_action: "a2", patch_preview: "p2", path: undefined },
    ];
    const groups = groupPatchPreviewItemsByPath(items);
    expect(groups).toHaveLength(1);
    expect(groups[0].path).toBe("全局");
    expect(groups[0].items).toHaveLength(2);
  });

  it("多路径建议按路径分组并保留顺序", () => {
    const items = [
      { issue_code: "E001", title: "t1", proposed_action: "a1", patch_preview: "p1", path: "vault/a.md" },
      { issue_code: "E002", title: "t2", proposed_action: "a2", patch_preview: "p2", path: "vault/b.md" },
      { issue_code: "W001", title: "t3", proposed_action: "a3", patch_preview: "p3", path: "vault/a.md" },
    ];
    const groups = groupPatchPreviewItemsByPath(items);
    expect(groups).toHaveLength(2);
    expect(groups[0].path).toBe("vault/a.md");
    expect(groups[0].items).toHaveLength(2);
    expect(groups[1].path).toBe("vault/b.md");
    expect(groups[1].items).toHaveLength(1);
  });

  it("混合有路径与无路径的建议", () => {
    const items = [
      { issue_code: "E001", title: "t1", proposed_action: "a1", patch_preview: "p1", path: "vault/a.md" },
      { issue_code: "I001", title: "t2", proposed_action: "a2", patch_preview: "p2", path: null },
    ];
    const groups = groupPatchPreviewItemsByPath(items);
    expect(groups).toHaveLength(2);
    const paths = groups.map((g) => g.path);
    expect(paths).toContain("vault/a.md");
    expect(paths).toContain("全局");
  });
});

describe("OCR Provider 持久化", () => {
  const installLocalStorageMock = (initial: Record<string, string> = {}) => {
    const store = new Map(Object.entries(initial));
    const localStorageMock = {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => {
        store.set(key, String(value));
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => {
        store.clear();
      },
    };

    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: localStorageMock,
    });

    return { store };
  };

  afterEach(() => {
    Reflect.deleteProperty(globalThis, "localStorage");
  });

  it("默认读取为 tesseract", () => {
    Reflect.deleteProperty(globalThis, "localStorage");
    expect(readOcrProviderFromStorage()).toBe("tesseract");
  });

  it("可读取已存储的合法值", () => {
    installLocalStorageMock({ [OCR_PROVIDER_STORAGE_KEY]: "paddle" });
    expect(readOcrProviderFromStorage()).toBe("paddle");
  });

  it("非法值回退到 tesseract", () => {
    installLocalStorageMock({ [OCR_PROVIDER_STORAGE_KEY]: "invalid" });
    expect(readOcrProviderFromStorage()).toBe("tesseract");
  });

  it("可写入并读回", () => {
    installLocalStorageMock();
    writeOcrProviderToStorage("paddle");
    expect(readOcrProviderFromStorage()).toBe("paddle");
  });
});

describe("Wiki 编辑器辅助函数", () => {
  it("字符计数格式化零值", () => {
    expect(formatEditorCharCount(0)).toBe("0 字符");
  });

  it("字符计数格式化非零值", () => {
    expect(formatEditorCharCount(42)).toBe("42 字符");
  });
});

describe("Query 进度事件解析", () => {
  it("answer_chunk 解析为分片更新", () => {
    const update = parseQueryProgressPayload({
      step: "answer_chunk",
      message: "这是分片",
    });
    expect(update).toEqual({ kind: "chunk", text: "这是分片" });
  });

  it("非分片步骤解析为状态更新", () => {
    const update = parseQueryProgressPayload({
      step: "generating",
      message: "正在合成回答（LLM）...",
    });
    expect(update).toEqual({ kind: "status", text: "正在合成回答（LLM）..." });
  });
});

describe("查询历史 dedup 与存储 key", () => {
  const installLocalStorageMock = (initial: Record<string, string> = {}) => {
    const store = new Map(Object.entries(initial));
    const localStorageMock = {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => {
        store.set(key, String(value));
      },
      removeItem: (key: string) => {
        store.delete(key);
      },
      clear: () => {
        store.clear();
      },
    };

    Object.defineProperty(globalThis, "localStorage", {
      configurable: true,
      value: localStorageMock,
    });

    return { store };
  };

  afterEach(() => {
    Reflect.deleteProperty(globalThis, "localStorage");
  });

  it("QUERY_HISTORY_STORAGE_KEY 与 QUERY_HISTORY_MAX 已导出并值合理", () => {
    expect(typeof QUERY_HISTORY_STORAGE_KEY).toBe("string");
    expect(QUERY_HISTORY_STORAGE_KEY.length).toBeGreaterThan(0);
    expect(QUERY_HISTORY_MAX).toBe(30);
  });

  it("normalizeQueryHistory 会去重、去空白并按上限截断", () => {
    const source = ["  问题A  ", "问题B", "", "问题A", "问题C", "问题D"];
    expect(normalizeQueryHistory(source, 3)).toEqual(["问题A", "问题B", "问题C"]);
  });

  it("mergeQueryHistory 新问题插入头部并去重", () => {
    const prev = ["问题A", "问题B", "问题C"];
    const next = mergeQueryHistory(prev, "问题B");
    expect(next).toEqual(["问题B", "问题A", "问题C"]);
  });

  it("mergeQueryHistory 超过 QUERY_HISTORY_MAX 时截断", () => {
    const prev = Array.from({ length: QUERY_HISTORY_MAX }, (_, i) => `问题${i}`);
    const next = mergeQueryHistory(prev, "新问题");
    expect(next).toHaveLength(QUERY_HISTORY_MAX);
    expect(next[0]).toBe("新问题");
  });

  it("read/writeQueryHistoryToStorage 可读写并保持去重结果", () => {
    installLocalStorageMock();
    writeQueryHistoryToStorage(["问题A", "问题B", "问题A", "   "], QUERY_HISTORY_MAX);
    expect(readQueryHistoryFromStorage()).toEqual(["问题A", "问题B"]);
  });

  it("readQueryHistoryFromStorage 在非法 JSON 时安全回退为空", () => {
    installLocalStorageMock({ [QUERY_HISTORY_STORAGE_KEY]: "{invalid-json" });
    expect(readQueryHistoryFromStorage()).toEqual([]);
  });

  it("normalizeQueryHistoryItems 会按 question 去重并保留 created_at", () => {
    const source = [
      { id: 1, question: "  问题A ", created_at: "100" },
      { id: 2, question: "问题B", created_at: "200" },
      { id: 3, question: "问题A", created_at: "300" },
    ];
    expect(normalizeQueryHistoryItems(source, 10)).toEqual([
      { id: 1, question: "问题A", created_at: "100" },
      { id: 2, question: "问题B", created_at: "200" },
    ]);
  });

  it("mergeQueryHistoryItems 会将新问题插入头部并去重", () => {
    const prev = [
      { id: 1, question: "问题A", created_at: "100" },
      { id: 2, question: "问题B", created_at: "200" },
    ];
    const next = mergeQueryHistoryItems(prev, "问题B", "300", QUERY_HISTORY_MAX);
    expect(next[0].question).toBe("问题B");
    expect(next[0].created_at).toBe("300");
    expect(next.map((item) => item.question)).toEqual(["问题B", "问题A"]);
  });

  it("read/writeQueryHistoryItemsToStorage 可读写对象历史", () => {
    installLocalStorageMock();
    writeQueryHistoryItemsToStorage(
      [
        { id: 1, question: "问题A", created_at: "100" },
        { id: 2, question: "问题B", created_at: "200" },
      ],
      QUERY_HISTORY_MAX,
    );
    expect(readQueryHistoryItemsFromStorage()).toEqual([
      { id: 1, question: "问题A", created_at: "100" },
      { id: 2, question: "问题B", created_at: "200" },
    ]);
  });

  it("readQueryHistoryItemsFromStorage 兼容旧版字符串数组", () => {
    installLocalStorageMock({
      [QUERY_HISTORY_STORAGE_KEY]: JSON.stringify(["问题A", "问题B"]),
    });
    expect(readQueryHistoryItemsFromStorage()).toEqual([
      { id: 1, question: "问题A", created_at: "" },
      { id: 2, question: "问题B", created_at: "" },
    ]);
  });

  it("filterQueryHistoryItems 支持关键词筛选", () => {
    const source = [
      { id: 1, question: "Rust FTS5 是什么", created_at: "100" },
      { id: 2, question: "SQLite 索引怎么建", created_at: "200" },
    ];
    expect(filterQueryHistoryItems(source, "rust")).toEqual([
      { id: 1, question: "Rust FTS5 是什么", created_at: "100" },
    ]);
    expect(filterQueryHistoryItems(source, "  ")).toEqual(source);
  });

  it("formatAskHistoryCreatedAt 在有效时间戳时返回紧凑格式", () => {
    expect(formatAskHistoryCreatedAt("1713300000")).toMatch(/^\d{2}-\d{2} \d{2}:\d{2}$/);
    expect(formatAskHistoryCreatedAt("")).toBe("");
    expect(formatAskHistoryCreatedAt("invalid")).toBe("");
  });
});
