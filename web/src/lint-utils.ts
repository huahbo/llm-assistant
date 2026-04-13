import type { LintSeverityStats } from "./types";

export const normalizeLintSeverity = (severity: string) => {
  const normalized = severity.trim().toLowerCase();
  return normalized || "info";
};

export type LintSeverityFilter = "all" | "error" | "warning" | "info";

export interface LintFilterState {
  severity: LintSeverityFilter;
  codeKeyword: string;
  pathKeyword: string;
  suggestionKeyword: string;
}

export const LINT_FILTER_STATE_STORAGE_KEY = "llm_wiki_lint_filters_v1";

const createDefaultLintFilterState = (): LintFilterState => ({
  severity: "all",
  codeKeyword: "",
  pathKeyword: "",
  suggestionKeyword: "",
});

const isLintSeverityFilter = (value: string): value is LintSeverityFilter =>
  value === "all" || value === "error" || value === "warning" || value === "info";

const normalizeLintFilterState = (
  state:
    | Partial<LintFilterState>
    | null
    | undefined,
): LintFilterState => ({
  severity: isLintSeverityFilter(state?.severity ?? "")
    ? (state?.severity ?? "all")
    : "all",
  codeKeyword: typeof state?.codeKeyword === "string" ? state.codeKeyword : "",
  pathKeyword: typeof state?.pathKeyword === "string" ? state.pathKeyword : "",
  suggestionKeyword: typeof state?.suggestionKeyword === "string" ? state.suggestionKeyword : "",
});

export const readLintFilterState = (): LintFilterState => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return createDefaultLintFilterState();
    }

    const rawValue = storage.getItem(LINT_FILTER_STATE_STORAGE_KEY);
    if (!rawValue) {
      return createDefaultLintFilterState();
    }

    const parsed = JSON.parse(rawValue) as Partial<LintFilterState> | null;
    return normalizeLintFilterState(parsed);
  } catch {
    return createDefaultLintFilterState();
  }
};

export const writeLintFilterState = (state: Partial<LintFilterState>) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }

    storage.setItem(
      LINT_FILTER_STATE_STORAGE_KEY,
      JSON.stringify(normalizeLintFilterState(state)),
    );
  } catch {
    // 本地存储不可用时静默降级，避免影响页面交互。
  }
};

export const filterLintIssuesBySeverity = <T extends { severity: string }>(
  issues: T[],
  filter: LintSeverityFilter,
) => {
  if (filter === "all") {
    return issues.slice();
  }

  return issues.filter((issue) => {
    const normalizedSeverity = normalizeLintSeverity(issue.severity);
    if (normalizedSeverity === "error") {
      return filter === "error";
    }
    if (normalizedSeverity === "warning") {
      return filter === "warning";
    }
    return filter === "info";
  });
};

export const filterLintIssuesByCode = <T extends { code?: string | null }>(
  issues: T[],
  keyword: string,
) => {
  const normalizedKeyword = keyword.trim().toLowerCase();
  if (!normalizedKeyword) {
    return issues.slice();
  }

  return issues.filter((issue) =>
    (issue.code ?? "").toLowerCase().includes(normalizedKeyword),
  );
};

export const filterLintIssuesByPath = <T extends { path?: string | null }>(
  issues: T[],
  keyword: string,
) => {
  const normalizedKeyword = keyword.trim().toLowerCase();
  if (!normalizedKeyword) {
    return issues.slice();
  }

  return issues.filter((issue) =>
    (issue.path ?? "").toLowerCase().includes(normalizedKeyword),
  );
};

export const filterLintIssuesBySuggestion = <T extends { suggestion?: string | null }>(
  issues: T[],
  keyword: string,
) => {
  const normalizedKeyword = keyword.trim().toLowerCase();
  if (!normalizedKeyword) {
    return issues.slice();
  }

  return issues.filter((issue) =>
    (issue.suggestion ?? "").toLowerCase().includes(normalizedKeyword),
  );
};

const createEmptySeverityStats = (): LintSeverityStats => ({
  error: 0,
  warning: 0,
  info: 0,
});

export const resolveLintSeverityStats = (
  report:
    | {
        severity_stats?: LintSeverityStats | null;
        issues?: Array<{ severity?: string | null }>;
      }
    | null
    | undefined,
): LintSeverityStats => {
  const storedStats = report?.severity_stats;
  if (
    storedStats &&
    Number.isFinite(storedStats.error) &&
    Number.isFinite(storedStats.warning) &&
    Number.isFinite(storedStats.info)
  ) {
    return {
      error: storedStats.error,
      warning: storedStats.warning,
      info: storedStats.info,
    };
  }

  const stats = createEmptySeverityStats();
  for (const issue of report?.issues ?? []) {
    const severity = normalizeLintSeverity(issue.severity ?? "info");
    if (severity === "error") {
      stats.error += 1;
    } else if (severity === "warning") {
      stats.warning += 1;
    } else {
      stats.info += 1;
    }
  }
  return stats;
};

export const formatLintCheckedAt = (checkedAt: string) => {
  const timestamp = Number(checkedAt);

  if (!Number.isFinite(timestamp)) {
    return checkedAt;
  }

  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) {
    return checkedAt;
  }

  return `${date.toISOString().slice(0, 19).replace("T", " ")} UTC`;
};
