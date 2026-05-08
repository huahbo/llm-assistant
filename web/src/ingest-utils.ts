import type { WikiTemplate } from "./types";
import type { OcrProvider } from "./tauri-client";

// ---- Ingest types ----

export type DroppedIngestPathsResult = {
  accepted: string[];
  rejected: string[];
  duplicateCount: number;
};

export type TemplateInitPreview = {
  dirs: string[];
  files: string[];
};

// ---- Ingest constants ----

export const defaultIngestSourcePath = "E:\\llm-wiki\\test-llm.md";
export const defaultIngestPdfPath = "E:\\llm-wiki\\test.pdf";
export const defaultIngestFilePath = "E:\\llm-wiki\\test.docx";
export const defaultIngestFileOcrProvider: OcrProvider = "tesseract";

export const ingestSupportedFileExtensions = new Set([
  "md",
  "markdown",
  "pdf",
  "docx",
  "pptx",
  "txt",
  "png",
  "jpg",
  "jpeg",
  "webp",
  "bmp",
  "gif",
  "tif",
  "tiff",
]);

// ---- Ingest functions ----

export const normalizeTemplateDirPath = (dir: string): string => {
  const normalized = dir.trim().replace(/\\/g, "/").replace(/\/+/g, "/").replace(/^\/+/, "");
  if (!normalized) {
    return "";
  }
  if (
    normalized === "wiki"
    || normalized.startsWith("wiki/")
    || normalized === "raw"
    || normalized.startsWith("raw/")
    || normalized === ".app"
    || normalized.startsWith(".app/")
  ) {
    return normalized;
  }
  return `wiki/${normalized}`;
};

/**
 * 构建模板初始化预览：展示会创建的核心目录与文件（相对 vault 根路径）。
 */
export const buildTemplateInitPreview = (template: WikiTemplate): TemplateInitPreview => {
  const dirSet = new Set<string>(["raw", "wiki", ".app"]);
  const fileSet = new Set<string>(["index.md", "log.md", ".app/config.json", ".app/meta.db"]);

  if (template.id !== "general") {
    fileSet.add("wiki/schema.md");
    fileSet.add("wiki/purpose.md");
  }

  for (const dir of template.extraDirs) {
    const normalized = normalizeTemplateDirPath(dir);
    if (normalized) {
      dirSet.add(normalized);
    }
  }

  return {
    dirs: Array.from(dirSet).sort((a, b) => a.localeCompare(b, "zh-CN")),
    files: Array.from(fileSet).sort((a, b) => a.localeCompare(b, "zh-CN")),
  };
};

/**
 * 解析窗口拖拽文件路径：保留受支持扩展名并去重，返回被忽略条目用于提示。
 */
export const parseDroppedIngestPaths = (paths: string[]): DroppedIngestPathsResult => {
  const seen = new Set<string>();
  const accepted: string[] = [];
  const rejected: string[] = [];
  let duplicateCount = 0;

  for (const rawPath of paths) {
    const path = rawPath.trim();
    if (!path) {
      continue;
    }

    // Windows 路径大小写不敏感，统一小写比较去重。
    const normalizedKey = path.replaceAll("\\", "/").toLowerCase();
    if (seen.has(normalizedKey)) {
      duplicateCount += 1;
      continue;
    }
    seen.add(normalizedKey);

    const fileName = path.split(/[/\\]/).pop() ?? "";
    const extension = fileName.includes(".") ? (fileName.split(".").pop() ?? "").toLowerCase() : "";
    if (!extension || !ingestSupportedFileExtensions.has(extension)) {
      rejected.push(path);
      continue;
    }
    accepted.push(path);
  }

  return { accepted, rejected, duplicateCount };
};

export const formatPdfIngestErrorMessage = (error: unknown) => {
  const rawMessage = error instanceof Error ? error.message : String(error ?? "");
  const compactRaw = rawMessage.replace(/\s+/g, " ").trim();
  const normalized = compactRaw.toLowerCase();
  const hasAutoOcrFallbackHint =
    normalized.includes("已自动 ocr 回退")
    || normalized.includes("自动 ocr 回退")
    || normalized.includes("自动ocr回退")
    || normalized.includes("auto ocr fallback");

  let messagePrefix = "PDF 摄入失败：";
  let friendlyReason = "读取 PDF 失败，请确认文件可访问且内容有效。";
  if (hasAutoOcrFallbackHint) {
    messagePrefix = "PDF 摄入提示：";
    friendlyReason = "检测到解析兼容性问题，已自动 OCR 回退并继续处理。";
  } else if (normalized.includes("tounicode") || normalized.includes("cmap")) {
    friendlyReason = "PDF 字体映射解析失败，建议先用 PDF 工具另存为新文件后重试。";
  } else if (
    normalized.includes("pdftoppm")
    || normalized.includes("poppler")
    || normalized.includes("missing poppler")
    || normalized.includes("未安装 poppler")
  ) {
    friendlyReason = "未检测到 pdftoppm（Poppler），请安装 Poppler 并将其 bin 目录加入 PATH 后重试。";
  } else if (
    normalized.includes("解析器暂不兼容")
    || normalized.includes("结构不兼容")
    || normalized.includes("parser")
  ) {
    friendlyReason = "PDF 文件可打开，但当前解析器暂不兼容该结构，建议先在阅读器中另存为新 PDF 后重试。";
  } else if (
    normalized.includes("未提取到任何文本")
    || normalized.includes("未提取到可用文本")
    || normalized.includes("empty text")
    || normalized.includes("no text")
    || normalized.includes("扫描件")
  ) {
    friendlyReason = "PDF 中没有可提取文本，可能是扫描件或图片型文档，建议先做 OCR。";
  } else if (
    normalized.includes("is not a pdf")
    || normalized.includes("不是 pdf")
    || normalized.includes("扩展名错误")
  ) {
    friendlyReason = "文件类型不是有效的 PDF，请检查路径或文件格式。";
  }

  if (!compactRaw) {
    return `${messagePrefix}${friendlyReason}`;
  }

  // 原始原因仅保留短片段，避免整段底层错误直接透出。
  const rawSnippetMaxLength = 60;
  const rawSnippet = compactRaw.length > rawSnippetMaxLength
    ? `${compactRaw.slice(0, rawSnippetMaxLength)}...`
    : compactRaw;
  return `${messagePrefix}${friendlyReason}（原因：${rawSnippet}）`;
};
