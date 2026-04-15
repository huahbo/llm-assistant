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
  WIKI_SORT_MODE_STORAGE_KEY,
  isWikiSortMode,
  parseLegacyImportedAtFromContent,
  isSameWikiPagePath,
  normalizeWikiPathForCompare,
  parseLegacyWikiMetadataFromContent,
  readWikiSortModeFromStorage,
  resolveWikiImportedAtDebugValue,
  resolveNextActiveProvider,
  shouldAutoDismissStatusMessage,
  writeWikiSortModeToStorage,
} from "./App";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import {
  createIngestMarkdownArgs,
  createQueryAskArgs,
  createQueryAskWithOptionsArgs,
  createPreviewLintPatchesArgs,
  createFetchRecentLintPatchEventsArgs,
  createApplyLintPatchArgs,
  createApplyLintPatchesBatchArgs,
  createSaveQueryAnswerArgs,
  createSearchWikiPagesArgs,
  createWikiPageCitationsArgs,
  createWikiPageDetailArgs,
  createSetQueryTopKArgs,
  createVaultInitArgs,
  formatLlmStatusSummary,
  isTauriRuntime,
  applyLintPatch,
  applyLintPatchesBatch,
  fetchRecentLintPatchEvents,
  normalizeLintPatchPreviewResponse,
  normalizeLlmStatus,
  normalizeLlmProviderConfig,
  saveLlmConfig,
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

  it("摘要折叠时按长度截断", () => {
    const result = buildWikiSummaryDisplay("a".repeat(30), false, 10);
    expect(result).toEqual({
      text: "aaaaaaaaaa...",
      isTruncated: true,
    });
  });

  it("摘要展开或较短时不截断", () => {
    expect(buildWikiSummaryDisplay("short text", false, 20)).toEqual({
      text: "short text",
      isTruncated: false,
    });
    expect(buildWikiSummaryDisplay("long text", true, 4)).toEqual({
      text: "long text",
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
