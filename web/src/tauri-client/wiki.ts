import type {
  KnowledgeGraphData,
  KnowledgeSubgraphData,
  KnowledgeSubgraphRequestParams,
  LintPatchApplyResult,
  LintPatchBatchItemInput,
  LintPatchBatchItemResult,
  LintPatchBatchResult,
  LintPatchEvent,
  LintPatchPreviewItem,
  LintReport,
  NewPageResult,
  PageQuickLint,
  DeleteWikiPageResult,
  RenameWikiPageResult,
  SaveWikiPageResult,
  SaveQueryAnswerInput,
  SaveQueryAnswerResult,
  WikiPageCitation,
  WikiPageDetail,
  WikiPageHistoryEntry,
  WikiPageHistorySummary,
  WikiPageItem,
} from "../types";
import { isTauriRuntime, withTimeout } from "./base";

// ── Display path utility ──────────────────────────────────────────────────────

type DisplayPathSource = {
  display_path?: string | null;
  displayPath?: string | null;
  cited_page_display_path?: string | null;
  path?: string | null;
  page_path?: string | null;
  cited_page_path?: string | null;
  wiki_path?: string | null;
  source_path?: string | null;
  raw_path?: string | null;
};

export const resolveDisplayPath = (source: DisplayPathSource) => {
  // 优先展示后端返回的友好路径，缺失时回退到原始路径。
  const candidates = [
    source.display_path,
    source.displayPath,
    source.cited_page_display_path,
    source.path,
    source.page_path,
    source.cited_page_path,
    source.wiki_path,
    source.source_path,
    source.raw_path,
  ];

  for (const candidate of candidates) {
    const value = candidate?.trim();
    if (value) {
      return value;
    }
  }

  return "";
};

// ── Raw response types (private) ─────────────────────────────────────────────

type RawLintPatchPreviewResponse =
  | LintPatchPreviewItem[]
  | {
      generated_at?: string | null;
      total?: number | null;
      suggestions?: LintPatchPreviewItem[] | null;
    };

type RawLintPatchApplyResponse =
  | LintPatchApplyResult
  | string
  | null
  | undefined;

type RawLintPatchBatchItemResponse = {
  issue_code?: string | null;
  path?: string | null;
  applied?: boolean | null;
  skipped?: boolean | null;
  status?: string | null;
  message?: string | null;
  touched_paths?: string[] | null;
  error?: string | null;
};

type RawLintPatchBatchEnvelope = {
  summary?: string | null;
  message?: string | null;
  total?: number | null;
  success?: number | null;
  failed?: number | null;
  skipped?: number | null;
  success_count?: number | null;
  failure_count?: number | null;
  skipped_count?: number | null;
  total_count?: number | null;
  applied_count?: number | null;
  results?: Array<RawLintPatchApplyResponse | RawLintPatchBatchItemResponse> | null;
  items?: Array<RawLintPatchApplyResponse | RawLintPatchBatchItemResponse> | null;
};

type RawLintPatchBatchResponse =
  | RawLintPatchBatchEnvelope
  | Array<RawLintPatchApplyResponse | RawLintPatchBatchItemResponse>
  | string
  | null
  | undefined;

type RawLintPatchEvent = {
  issue_code?: string | null;
  path?: string | null;
  applied?: boolean | null;
  message?: string | null;
  created_at?: string | null;
};

// ── Args builders ─────────────────────────────────────────────────────────────

export const createSaveWikiPageArgs = (path: string, content: string) => ({
  path,
  content,
});

export const createSearchWikiPagesArgs = (keyword: string) => ({
  keyword,
});

export const createWikiPageDetailArgs = (pagePath: string) => ({
  pagePath,
  page_path: pagePath,
});

export const createWikiPageCitationsArgs = (pagePath: string) => ({
  pagePath,
  page_path: pagePath,
});

export const createWikiPageHistoryArgs = (pagePath: string, limit?: number) => ({
  path: pagePath,
  pagePath,
  page_path: pagePath,
  limit,
});

export const createWikiPageHistoryEntryArgs = (id: number) => ({ id });

export const createGetKnowledgeSubgraphArgs = (
  params: KnowledgeSubgraphRequestParams,
) => ({
  centerPagePath: params.centerPagePath,
  center_page_path: params.centerPagePath,
  hop: params.hop,
  direction: params.direction,
  limitNodes: params.limitNodes,
  limit_nodes: params.limitNodes,
  limitLinks: params.limitLinks,
  limit_links: params.limitLinks,
});

export const createPreviewLintPatchesArgs = () => ({});

export const createFetchRecentLintPatchEventsArgs = () => ({});

export const createApplyLintPatchArgs = (item: LintPatchPreviewItem) => ({
  issueCode: item.issue_code,
  issue_code: item.issue_code,
  path: item.path ?? null,
});

export const createApplyLintPatchesBatchArgs = (items: LintPatchBatchItemInput[]) => ({
  inputs: items.map((item) => ({
    issueCode: item.issue_code,
    issue_code: item.issue_code,
    path: item.path ?? null,
  })),
  // 兼容旧后端/测试桩，保留 items 字段。
  items: items.map((item) => ({
    issueCode: item.issue_code,
    issue_code: item.issue_code,
    path: item.path ?? null,
  })),
});

// ── Normalize helpers ─────────────────────────────────────────────────────────

export const normalizeLintPatchPreviewResponse = (
  payload: RawLintPatchPreviewResponse | null | undefined,
): LintPatchPreviewItem[] => {
  if (!payload) {
    return [];
  }

  if (Array.isArray(payload)) {
    return payload;
  }

  if (Array.isArray(payload.suggestions)) {
    return payload.suggestions;
  }

  return [];
};

export const normalizeLintPatchApplyResponse = (
  payload: RawLintPatchApplyResponse,
): LintPatchApplyResult | null => {
  if (payload == null) {
    return null;
  }

  if (typeof payload === "string") {
    return { message: payload };
  }

  return {
    issue_code: payload.issue_code ?? undefined,
    path: payload.path ?? undefined,
    applied: payload.applied ?? undefined,
    message: payload.message ?? undefined,
    touched_paths: Array.isArray(payload.touched_paths) ? payload.touched_paths : undefined,
  };
};

const normalizeLintPatchBatchItemResponse = (
  payload: RawLintPatchApplyResponse | RawLintPatchBatchItemResponse,
): LintPatchBatchItemResult => {
  if (typeof payload === "string") {
    return {
      message: payload,
      skipped: true,
      applied: false,
    };
  }

  if (payload == null) {
    return {
      applied: false,
      skipped: true,
    };
  }

  if (typeof payload === "object" && "issue_code" in payload) {
    const batchItem = payload as RawLintPatchBatchItemResponse;
    const status = batchItem.status?.trim().toLowerCase();
    const inferredSkipped =
      typeof batchItem.skipped === "boolean"
        ? batchItem.skipped
        : status === "skipped";
    const inferredApplied =
      typeof batchItem.applied === "boolean"
        ? batchItem.applied
        : status === "success";

    return {
      issue_code: batchItem.issue_code ?? undefined,
      path: batchItem.path ?? undefined,
      applied: inferredApplied,
      skipped: inferredSkipped,
      status: batchItem.status ?? undefined,
      message: batchItem.message ?? undefined,
      touched_paths: Array.isArray(batchItem.touched_paths) ? batchItem.touched_paths : undefined,
      error: batchItem.error ?? undefined,
    };
  }

  return {
    ...(normalizeLintPatchApplyResponse(payload as RawLintPatchApplyResponse) ?? {}),
    skipped: false,
  };
};

export const normalizeLintPatchBatchResponse = (
  payload: RawLintPatchBatchResponse,
): LintPatchBatchResult | null => {
  if (payload == null) {
    return null;
  }

  if (typeof payload === "string") {
    return {
      summary: payload,
      success_count: 0,
      failure_count: 0,
      skipped_count: 0,
      total_count: 0,
      items: [],
    };
  }

  const envelope = !Array.isArray(payload) && typeof payload === "object" ? payload : null;
  const rawItems = Array.isArray(payload)
    ? payload
    : Array.isArray(envelope?.results)
      ? envelope.results
      : Array.isArray(envelope?.items)
        ? envelope.items
        : [];
  const items = rawItems.map((item) => normalizeLintPatchBatchItemResponse(item));

  const countFromEnvelope = (
    value: number | null | undefined,
    fallback: number,
  ) => {
    if (typeof value !== "number" || Number.isNaN(value)) {
      return fallback;
    }
    return value;
  };

  const success_count = envelope
    ? countFromEnvelope(
        envelope.success_count ?? envelope.applied_count ?? envelope.success,
        items.filter((item) => item.applied === true).length,
      )
    : items.filter((item) => item.applied === true).length;
  const failure_count = envelope
    ? countFromEnvelope(
        envelope.failure_count ?? envelope.failed,
        items.filter((item) => item.applied === false && item.skipped !== true).length,
      )
    : items.filter((item) => item.applied === false && item.skipped !== true).length;
  const skipped_count = envelope
    ? countFromEnvelope(
        envelope.skipped_count ?? envelope.skipped,
        items.filter((item) => item.skipped === true).length,
      )
    : items.filter((item) => item.skipped === true).length;
  const total_count = envelope
    ? countFromEnvelope(
        envelope.total_count ?? envelope.total,
        items.length,
      )
    : items.length;

  return {
    summary: envelope ? envelope.summary ?? envelope.message ?? undefined : undefined,
    success_count,
    failure_count,
    skipped_count,
    total_count,
    items,
  };
};

export const normalizeLintPatchEvents = (
  payload: RawLintPatchEvent[] | null | undefined,
): LintPatchEvent[] => {
  if (!Array.isArray(payload)) {
    return [];
  }

  return payload
    .map((item) => ({
      issue_code: item.issue_code?.trim() || "未知",
      path: item.path ?? null,
      applied: Boolean(item.applied),
      message: item.message?.trim() || "",
      created_at: item.created_at?.trim() || "",
    }))
    .filter((item) => item.created_at);
};

// ── Wiki page CRUD ────────────────────────────────────────────────────────────

export async function fetchRecentWikiPages(): Promise<WikiPageItem[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await invoke<WikiPageItem[]>("get_recent_wiki_pages");
  } catch {
    return [];
  }
}

export async function searchWikiPages(keyword: string): Promise<WikiPageItem[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<WikiPageItem[]>("search_wiki_pages", {
        ...createSearchWikiPagesArgs(keyword),
      }),
    );
  } catch {
    return [];
  }
}

/** FTS5 + 向量双路召回 wiki 页面（Ollama 不可用时自动降级为纯 FTS5）。 */
export async function searchWikiPagesHybrid(keyword: string): Promise<WikiPageItem[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<WikiPageItem[]>("search_wiki_pages_hybrid", { keyword }),
      20000,
    );
  } catch {
    return searchWikiPages(keyword);
  }
}

/** 搜索所有 wiki 页面路径（用于链接自动补全） */
export async function searchWikiPaths(query: string): Promise<string[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<string[]>("search_wiki_paths", {
        query,
      }),
    );
  } catch {
    return [];
  }
}

export async function fetchWikiPageDetail(pagePath: string): Promise<WikiPageDetail | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<WikiPageDetail>("get_wiki_page_detail", {
      ...createWikiPageDetailArgs(pagePath),
    }),
  );
}

export async function fetchWikiPageCitations(pagePath: string): Promise<WikiPageCitation[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<WikiPageCitation[]>("get_wiki_page_citations", {
        ...createWikiPageCitationsArgs(pagePath),
      }),
    );
  } catch {
    return null;
  }
}

export async function listWikiPageHistory(
  pagePath: string,
  limit = 30,
): Promise<WikiPageHistorySummary[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<WikiPageHistorySummary[]>("list_wiki_page_history", {
        ...createWikiPageHistoryArgs(pagePath, limit),
      }),
    );
  } catch {
    return [];
  }
}

export async function getWikiPageHistoryEntry(
  id: number,
): Promise<WikiPageHistoryEntry | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<WikiPageHistoryEntry>("get_wiki_page_history_entry", {
        ...createWikiPageHistoryEntryArgs(id),
      }),
    );
  } catch {
    return null;
  }
}

// 从历史版本恢复 Wiki 页面内容
export async function restoreWikiPageFromHistory(
  id: number,
): Promise<SaveWikiPageResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await withTimeout(
      invoke<SaveWikiPageResult>("restore_wiki_page_from_history", {
        id,
      }),
    );
  } catch {
    return null;
  }
}

export async function saveWikiPage(
  path: string,
  content: string,
  expectedChecksum?: string,
): Promise<SaveWikiPageResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<SaveWikiPageResult>("save_wiki_page", {
      ...createSaveWikiPageArgs(path, content),
      expected_checksum: expectedChecksum || null,
    }),
  );
}

export async function renameWikiPage(
  oldPath: string,
  newName: string,
): Promise<RenameWikiPageResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<RenameWikiPageResult>("rename_wiki_page", { oldPath, newName }),
  );
}

export async function deleteWikiPage(path: string): Promise<DeleteWikiPageResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke<DeleteWikiPageResult>("delete_wiki_page", { path }));
}

/** 设置或取消 Wiki 页面的 stale 标记 */
export async function markPageStale(
  pagePath: string,
  stale: boolean,
): Promise<void> {
  if (!isTauriRuntime()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke<void>("mark_page_stale", { pagePath, stale });
}

// ── Knowledge graph ───────────────────────────────────────────────────────────

/** 获取知识图谱数据（节点 = wiki 页面，边 = 引用关系） */
export async function getKnowledgeGraph(): Promise<KnowledgeGraphData | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return await invoke<KnowledgeGraphData>("get_knowledge_graph");
}

/** 获取知识子图（以中心页面为起点，按 hop/方向裁剪） */
export async function getKnowledgeSubgraph(
  params: KnowledgeSubgraphRequestParams,
): Promise<KnowledgeSubgraphData | null> {
  if (!isTauriRuntime()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return await withTimeout(
    invoke<KnowledgeSubgraphData>("get_knowledge_subgraph", {
      ...createGetKnowledgeSubgraphArgs(params),
    }),
  );
}

/** AI 辅助新建 Wiki 页面。非 Tauri 环境抛出错误。超时 90 秒。 */
export async function createWikiPageWithAi(topic: string): Promise<NewPageResult> {
  if (!isTauriRuntime()) {
    throw new Error("仅在 Tauri 环境下可用");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<NewPageResult>("create_wiki_page_with_ai", { topic }),
    90_000,
  );
}

// ── Lint ──────────────────────────────────────────────────────────────────────

/** 对单个 Wiki 页面执行快速结构检查（无 LLM，仅文件系统） */
export async function quickLintPage(
  wikiPath: string,
): Promise<PageQuickLint | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<PageQuickLint>("quick_lint_page", { wikiPath });
}

export async function runLint(): Promise<LintReport | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(invoke<LintReport>("run_lint"));
}

export async function previewLintPatches(): Promise<LintPatchPreviewItem[] | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    const result = await withTimeout(
      invoke<RawLintPatchPreviewResponse>("preview_lint_patches", {
        ...createPreviewLintPatchesArgs(),
      }),
    );
    return normalizeLintPatchPreviewResponse(result);
  } catch {
    return null;
  }
}

export async function fetchRecentLintPatchEvents(): Promise<LintPatchEvent[]> {
  if (!isTauriRuntime()) {
    return [];
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    const result = await withTimeout(
      invoke<RawLintPatchEvent[]>("get_recent_lint_patch_events", {
        ...createFetchRecentLintPatchEventsArgs(),
      }),
    );
    return normalizeLintPatchEvents(result);
  } catch {
    return [];
  }
}

export async function applyLintPatch(
  item: LintPatchPreviewItem,
): Promise<LintPatchApplyResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  const result = await withTimeout(
    invoke<RawLintPatchApplyResponse>("apply_lint_patch", {
      ...createApplyLintPatchArgs(item),
    }),
  );

  return normalizeLintPatchApplyResponse(result);
}

export async function applyLintPatchesBatch(
  items: LintPatchBatchItemInput[],
): Promise<LintPatchBatchResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  if (!items.length) {
    return {
      summary: "暂无可执行项。",
      success_count: 0,
      failure_count: 0,
      skipped_count: 0,
      total_count: 0,
      items: [],
    };
  }

  const { invoke } = await import("@tauri-apps/api/core");
  const result = await withTimeout(
    invoke<RawLintPatchBatchResponse>("apply_lint_patches_batch", {
      ...createApplyLintPatchesBatchArgs(items),
    }),
  );

  return normalizeLintPatchBatchResponse(result);
}

// ── Query answer (wiki-adjacent) ─────────────────────────────────────────────

export async function saveQueryAnswer(
  input: SaveQueryAnswerInput,
): Promise<SaveQueryAnswerResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");

  try {
    return await withTimeout(
      invoke<SaveQueryAnswerResult>("save_query_answer", {
        input: {
          question: input.question,
          answer: input.answer,
          citations: input.citations,
          title: input.title,
        },
      }),
    );
  } catch {
    return null;
  }
}
