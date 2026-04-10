import { afterEach, describe, expect, it } from "vitest";
import { formatQueryAnswerStrategyLabel, formatQuerySearchStrategyLabel } from "./App";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import {
  createIngestMarkdownArgs,
  createQueryAskArgs,
  createQueryAskWithOptionsArgs,
  createSaveQueryAnswerArgs,
  createSearchWikiPagesArgs,
  createWikiPageCitationsArgs,
  createWikiPageDetailArgs,
  createSetQueryTopKArgs,
  createVaultInitArgs,
  formatLlmStatusSummary,
  isTauriRuntime,
  normalizeLlmStatus,
  resolveDisplayPath,
} from "./tauri-client";
import {
  filterLintIssuesBySeverity,
  formatLintCheckedAt,
  normalizeLintSeverity,
  resolveLintSeverityStats,
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

describe("Lint 展示辅助函数", () => {
  const sampleLintIssues = [
    { severity: "error", code: "E1" },
    { severity: "warning", code: "W1" },
    { severity: "info", code: "I1" },
    { severity: "critical", code: "U1" },
  ];

  it("将 lint 时间戳格式化为 UTC 字符串", () => {
    expect(formatLintCheckedAt(String(Date.UTC(2026, 3, 8, 9, 10, 11)))).toBe(
      "2026-04-08 09:10:11 UTC",
    );
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
    expect(filterLintIssuesBySeverity(sampleLintIssues, "error")).toEqual([
      { severity: "error", code: "E1" },
    ]);
  });

  it("severity 过滤器支持 warning", () => {
    expect(filterLintIssuesBySeverity(sampleLintIssues, "warning")).toEqual([
      { severity: "warning", code: "W1" },
    ]);
  });

  it("severity 过滤器支持 info 且未知 severity 归入 info", () => {
    expect(filterLintIssuesBySeverity(sampleLintIssues, "info")).toEqual([
      { severity: "info", code: "I1" },
      { severity: "critical", code: "U1" },
    ]);
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
