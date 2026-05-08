import {
  getAppOverview,
  fetchLlmStatus,
  fetchRecentLogs,
  fetchRecentWikiPages,
} from "./tauri-client";
import type { AppOverview, LlmStatus, LogEntry, WikiPageItem } from "./types";

export type LoadResult = {
  overview: AppOverview | null;
  logs: LogEntry[];
  pages: WikiPageItem[];
  llmStatus: LlmStatus | null;
};

export const loadAppData = async (): Promise<LoadResult> => {
  const [overviewResult, logsResult, pagesResult, llmStatusResult] = await Promise.allSettled([
    getAppOverview(),
    fetchRecentLogs(),
    fetchRecentWikiPages(),
    fetchLlmStatus(),
  ]);

  return {
    overview: overviewResult.status === "fulfilled" ? overviewResult.value : null,
    logs: logsResult.status === "fulfilled" ? logsResult.value : [],
    pages: pagesResult.status === "fulfilled" ? pagesResult.value : [],
    llmStatus: llmStatusResult.status === "fulfilled" ? llmStatusResult.value : null,
  };
};
