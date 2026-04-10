import type { LintSeverityStats } from "./types";

export const normalizeLintSeverity = (severity: string) => {
  const normalized = severity.trim().toLowerCase();
  return normalized || "info";
};

export type LintSeverityFilter = "all" | "error" | "warning" | "info";

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
