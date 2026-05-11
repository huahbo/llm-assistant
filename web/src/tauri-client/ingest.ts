import type {
  IngestPreview,
  IngestQueueItem,
  IngestResult,
  ProgressPayload,
} from "../types";
import { isTauriRuntime, withTimeout } from "./base";

/** 摄入操作超时（5分钟）：PDF/文件需要 PDF提取 + LLM摘要 + 实体抽取，耗时长 */
const INGEST_TIMEOUT_MS = 300_000;

export type OcrProvider = "tesseract" | "paddle";

// ── Args builders ─────────────────────────────────────────────────────────────

export const createIngestMarkdownArgs = (sourcePath: string) => ({
  sourcePath,
  source_path: sourcePath,
});

/** 构造 ingest_pdf 命令参数（用于测试） */
export const createIngestPdfArgs = (sourcePath: string) => ({
  sourcePath,
  source_path: sourcePath,
});

/** 构造 ingest_file 命令参数（用于测试） */
export const createIngestFileArgs = (sourcePath: string, ocrProvider?: OcrProvider) => {
  const providerArgs = ocrProvider
    ? {
        ocrProvider,
        ocr_provider: ocrProvider,
      }
    : {};

  return {
    sourcePath,
    source_path: sourcePath,
    ...providerArgs,
  };
};

/** 构造 preview_ingest_file 命令参数（用于测试） */
export const createPreviewIngestFileArgs = (
  sourceType: string,
  sourcePath: string,
  ocrProvider?: OcrProvider,
) => {
  const providerArgs = ocrProvider
    ? {
        ocrProvider,
        ocr_provider: ocrProvider,
      }
    : {};

  return {
    sourceType,
    source_type: sourceType,
    sourcePath,
    source_path: sourcePath,
    ...providerArgs,
  };
};

/** 构造 apply_ingest_preview 命令参数（用于测试） */
export const createApplyIngestPreviewArgs = (previewId: string) => ({
  previewId,
  preview_id: previewId,
});

/** 构造 ingest_url 命令参数（用于测试） */
export const createIngestUrlArgs = (url: string) => ({ url });

// ── Ingest pipeline functions ─────────────────────────────────────────────────

export async function ingestMarkdown(sourcePath: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_markdown", {
      ...createIngestMarkdownArgs(sourcePath),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function ingestPdf(sourcePath: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_pdf", {
      ...createIngestPdfArgs(sourcePath),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function ingestFile(
  sourcePath: string,
  ocrProvider?: OcrProvider,
): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_file", {
      ...createIngestFileArgs(sourcePath, ocrProvider),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function previewIngestFile(
  sourceType: string,
  sourcePath: string,
  ocrProvider?: OcrProvider,
): Promise<IngestPreview | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestPreview>("preview_ingest_file", {
      ...createPreviewIngestFileArgs(sourceType, sourcePath, ocrProvider),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function applyIngestPreview(previewId: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("apply_ingest_preview", {
      ...createApplyIngestPreviewArgs(previewId),
    }),
    INGEST_TIMEOUT_MS,
  );
}

/** 通过 URL 摄入网页内容，与 ingestMarkdown 共用返回结构 */
export async function ingestUrl(url: string): Promise<IngestResult | null> {
  if (!isTauriRuntime()) {
    return null;
  }

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<IngestResult>("ingest_url", {
      ...createIngestUrlArgs(url),
    }),
    INGEST_TIMEOUT_MS,
  );
}

export async function enqueueIngest(sourceType: string, sourcePath: string): Promise<number> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<number>("enqueue_ingest", { sourceType, sourcePath });
}

export async function listIngestQueue(): Promise<IngestQueueItem[]> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<IngestQueueItem[]>("list_ingest_queue");
}

export async function cancelIngestItem(id: number): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("cancel_ingest_item", { id });
}

export async function retryIngestItem(id: number): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("retry_ingest_item", { id });
}

export async function deleteIngestItem(id: number): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("delete_ingest_item", { id });
}

/**
 * 获取页面对之间的 embedding 余弦相似度。
 * key 格式："pathA||pathB"（路径顺序无关，内部已规范化）。
 * 非 Tauri 环境或后端命令不可用时返回空对象（静默降级）。
 */
export async function getPageEmbeddingPairs(paths: string[]): Promise<Record<string, number>> {
  if (!isTauriRuntime()) return {};
  if (paths.length === 0) return {};
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    const result = await invoke<Record<string, number>>("get_page_embedding_similarities", { paths });
    return result ?? {};
  } catch {
    // 后端命令尚未实现时静默降级，不影响前端已有功能。
    return {};
  }
}

/** 订阅进度事件，返回取消订阅函数。非 Tauri 环境时为空操作。 */
export async function listenProgress(
  event: "ingest_progress" | "query_progress",
  handler: (payload: ProgressPayload) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ProgressPayload>(event, (e) => handler(e.payload));
  return unlisten;
}
