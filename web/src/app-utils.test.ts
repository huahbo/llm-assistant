import { afterEach, describe, expect, it } from "vitest";
import { formatBackendMode, formatLogLevel } from "./app-formatters";
import {
  createIngestMarkdownArgs,
  createQueryAskArgs,
  createQueryAskWithOptionsArgs,
  createVaultInitArgs,
  isTauriRuntime,
} from "./tauri-client";
import { formatLintCheckedAt, normalizeLintSeverity } from "./lint-utils";

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
  });
});

describe("Lint 展示辅助函数", () => {
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
});
