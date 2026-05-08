import type { WikiPageDetail, WikiPageItem, LintIssue, LintPatchPreviewItem } from "./types";
import { resolveDisplayPath } from "./tauri-client";

// 简单的字符串哈希（用于编辑基线校验和）
export const simpleHash = (str: string): string => {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash |= 0;
  }
  return Math.abs(hash).toString(16);
};

// ---- Wiki types ----

export type WikiLineDiffKind = "unchanged" | "added" | "removed";

export type WikiLineDiffRow = {
  kind: WikiLineDiffKind;
  line: string;
  oldLineNumber?: number;
  newLineNumber?: number;
};

export type WikiHighlightSegment = {
  text: string;
  matched: boolean;
};

export type WikiSortMode = "updated_desc" | "updated_asc" | "title_asc";

export type WikiTreeNode = {
  key: string;
  kind: "folder" | "file";
  name: string;
  fullPath: string;
  pagePath: string | null;
  children: WikiTreeNode[];
};

export type WikiAutocompleteMatch = {
  triggerStart: number;
  query: string;
};

// ---- Wiki constants ----

export const WIKI_SORT_MODE_STORAGE_KEY = "llm_wiki_wiki_sort_mode_v1";

// ---- Wiki utility functions ----

export const normalizeWikiPathForCompare = (path: string | null | undefined) =>
  (path ?? "")
    .trim()
    // Windows 规范路径前缀：\\?\C:\... 或 \\?\UNC\server\share\...
    .replace(/^\\\\\?\\UNC\\/i, "\\\\")
    .replace(/^\\\\\?\\/i, "")
    .replaceAll("\\", "/")
    .toLowerCase();

export const isSameWikiPagePath = (left: string | null | undefined, right: string | null | undefined) => {
  const normalizedLeft = normalizeWikiPathForCompare(left);
  const normalizedRight = normalizeWikiPathForCompare(right);
  return Boolean(normalizedLeft) && normalizedLeft === normalizedRight;
};

export const buildWikiLineDiff = (currentContent: string, historyContent: string): WikiLineDiffRow[] => {
  const currentLines = currentContent.split(/\r?\n/);
  const historyLines = historyContent.split(/\r?\n/);
  const dp = Array.from({ length: historyLines.length + 1 }, () =>
    Array<number>(currentLines.length + 1).fill(0),
  );

  for (let i = historyLines.length - 1; i >= 0; i -= 1) {
    for (let j = currentLines.length - 1; j >= 0; j -= 1) {
      dp[i][j] = historyLines[i] === currentLines[j]
        ? dp[i + 1][j + 1] + 1
        : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const rows: WikiLineDiffRow[] = [];
  let oldIndex = 0;
  let newIndex = 0;

  while (oldIndex < historyLines.length && newIndex < currentLines.length) {
    if (historyLines[oldIndex] === currentLines[newIndex]) {
      rows.push({
        kind: "unchanged",
        line: historyLines[oldIndex],
        oldLineNumber: oldIndex + 1,
        newLineNumber: newIndex + 1,
      });
      oldIndex += 1;
      newIndex += 1;
    } else if (dp[oldIndex + 1][newIndex] >= dp[oldIndex][newIndex + 1]) {
      rows.push({
        kind: "removed",
        line: historyLines[oldIndex],
        oldLineNumber: oldIndex + 1,
      });
      oldIndex += 1;
    } else {
      rows.push({
        kind: "added",
        line: currentLines[newIndex],
        newLineNumber: newIndex + 1,
      });
      newIndex += 1;
    }
  }

  while (oldIndex < historyLines.length) {
    rows.push({
      kind: "removed",
      line: historyLines[oldIndex],
      oldLineNumber: oldIndex + 1,
    });
    oldIndex += 1;
  }

  while (newIndex < currentLines.length) {
    rows.push({
      kind: "added",
      line: currentLines[newIndex],
      newLineNumber: newIndex + 1,
    });
    newIndex += 1;
  }

  return rows;
};

/** 格式化字符计数显示文本（用于测试） */
export const formatEditorCharCount = (count: number): string =>
  `${count.toLocaleString()} 字符`;

export const tokenizeWikiKeyword = (keyword: string) => {
  const tokens = keyword
    .split(/[\s,，。;；、|/]+/)
    .map((item) => item.trim().toLowerCase())
    .filter(Boolean);
  const unique = Array.from(new Set(tokens));

  return unique.filter((token) => {
    if (/^[a-z0-9_-]+$/i.test(token)) {
      return token.length >= 2;
    }
    return true;
  });
};

// 摘要折叠阈值：超过此行数时才显示展开按钮
const wikiSummaryPreviewLines = 3;

// 按行数截断摘要，比按字符截断更符合阅读习惯
export const buildWikiSummaryDisplay = (summary: string, expanded: boolean, maxLines = wikiSummaryPreviewLines) => {
  const normalized = summary.trim();
  const lines = normalized.split('\n');
  if (expanded || lines.length <= maxLines) {
    return {
      text: normalized,
      isTruncated: false,
    };
  }

  return {
    text: `${lines.slice(0, maxLines).join('\n')}...`,
    isTruncated: true,
  };
};

const escapeRegex = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

export const buildWikiHighlightSegments = (text: string, keywords: string[]): WikiHighlightSegment[] => {
  if (!text) {
    return [];
  }
  if (!keywords.length) {
    return [{ text, matched: false }];
  }

  const normalizedKeywords = Array.from(
    new Set(
      keywords
        .map((item) => item.trim().toLowerCase())
        .filter(Boolean),
    ),
  ).sort((left, right) => right.length - left.length);

  if (!normalizedKeywords.length) {
    return [{ text, matched: false }];
  }

  const regex = new RegExp(`(${normalizedKeywords.map(escapeRegex).join("|")})`, "ig");
  const parts = text.split(regex).filter((part) => part.length > 0);

  return parts.map((part) => ({
    text: part,
    matched: normalizedKeywords.includes(part.toLowerCase()),
  }));
};

export const isWikiSortMode = (value: string): value is WikiSortMode =>
  value === "updated_desc" || value === "updated_asc" || value === "title_asc";

export const readWikiSortModeFromStorage = (): WikiSortMode => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return "updated_desc";
    }
    const raw = storage.getItem(WIKI_SORT_MODE_STORAGE_KEY);
    if (!raw) {
      return "updated_desc";
    }
    return isWikiSortMode(raw) ? raw : "updated_desc";
  } catch {
    return "updated_desc";
  }
};

export const writeWikiSortModeToStorage = (mode: WikiSortMode) => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(WIKI_SORT_MODE_STORAGE_KEY, mode);
  } catch {
    // 本地存储不可用时静默降级，避免影响主流程。
  }
};

const parseWikiUpdatedAt = (value: string) => {
  const normalized = value.trim();
  if (!normalized) {
    return 0;
  }
  if (/^\d+$/.test(normalized)) {
    return Number(normalized) || 0;
  }
  const parsed = Date.parse(normalized);
  if (Number.isNaN(parsed)) {
    return 0;
  }
  return parsed;
};

export const sortWikiPages = (pages: WikiPageItem[], mode: WikiSortMode) => {
  const next = pages.slice();
  next.sort((left, right) => {
    if (mode === "title_asc") {
      return left.title.localeCompare(right.title, "zh-CN", { sensitivity: "base" });
    }

    const leftUpdatedAt = parseWikiUpdatedAt(left.updated_at);
    const rightUpdatedAt = parseWikiUpdatedAt(right.updated_at);
    if (leftUpdatedAt === rightUpdatedAt) {
      return left.title.localeCompare(right.title, "zh-CN", { sensitivity: "base" });
    }
    if (mode === "updated_asc") {
      return leftUpdatedAt - rightUpdatedAt;
    }
    return rightUpdatedAt - leftUpdatedAt;
  });
  return next;
};

type MutableWikiTreeNode = WikiTreeNode;

const normalizeWikiTreeDisplayPath = (path: string) =>
  path
    .trim()
    .replaceAll("\\", "/")
    .split("/")
    .map((segment) => segment.trim())
    .filter(Boolean)
    .join("/");

const resolveWikiTreeDisplayPath = (page: WikiPageItem) => {
  const preferred = (page.display_path ?? page.displayPath ?? "").trim();
  if (preferred) {
    return preferred;
  }
  const resolved = resolveDisplayPath(page).trim();
  if (resolved) {
    return resolved;
  }
  return page.path.trim();
};

const sortWikiTreeNodes = (nodes: MutableWikiTreeNode[]) => {
  nodes.sort((left, right) => {
    if (left.kind !== right.kind) {
      return left.kind === "folder" ? -1 : 1;
    }
    return left.name.localeCompare(right.name, "zh-CN", { sensitivity: "base" });
  });
  for (const node of nodes) {
    if (node.children.length > 0) {
      sortWikiTreeNodes(node.children);
    }
  }
};

export const buildWikiTreeNodes = (pages: WikiPageItem[]): WikiTreeNode[] => {
  const roots: MutableWikiTreeNode[] = [];
  const folderMap = new Map<string, MutableWikiTreeNode>();
  const fileKeySet = new Set<string>();

  for (const page of pages) {
    const normalized = normalizeWikiTreeDisplayPath(resolveWikiTreeDisplayPath(page));
    if (!normalized) {
      continue;
    }

    const segments = normalized.split("/").filter(Boolean);
    if (segments.length === 0) {
      continue;
    }

    let parentChildren = roots;
    let currentPath = "";

    for (let index = 0; index < segments.length; index += 1) {
      const segment = segments[index];
      const isFile = index === segments.length - 1;
      currentPath = currentPath ? `${currentPath}/${segment}` : segment;

      if (isFile) {
        const fileKey = `file:${normalizeWikiPathForCompare(page.path)}`;
        if (fileKeySet.has(fileKey)) {
          continue;
        }
        fileKeySet.add(fileKey);
        parentChildren.push({
          key: fileKey,
          kind: "file",
          name: segment,
          fullPath: currentPath,
          pagePath: page.path,
          children: [],
        });
        continue;
      }

      const folderKey = `folder:${currentPath.toLowerCase()}`;
      let folderNode = folderMap.get(folderKey);
      if (!folderNode) {
        folderNode = {
          key: folderKey,
          kind: "folder",
          name: segment,
          fullPath: currentPath,
          pagePath: null,
          children: [],
        };
        folderMap.set(folderKey, folderNode);
        parentChildren.push(folderNode);
      }
      parentChildren = folderNode.children;
    }
  }

  sortWikiTreeNodes(roots);
  return roots;
};

export const collectWikiTreeFolderKeys = (nodes: WikiTreeNode[]) => {
  const keys = new Set<string>();
  const walk = (items: WikiTreeNode[]) => {
    for (const item of items) {
      if (item.kind === "folder") {
        keys.add(item.key);
        walk(item.children);
      }
    }
  };
  walk(nodes);
  return keys;
};

// Lint 问题按页面路径分组，用于分组折叠展示
export type LintIssueGroup = { path: string; issues: LintIssue[] };

export const groupLintIssuesByPath = (issues: LintIssue[]): LintIssueGroup[] => {
  const map = new Map<string, LintIssue[]>();
  for (const issue of issues) {
    const key = issue.path ?? "全局";
    const existing = map.get(key);
    if (existing) {
      existing.push(issue);
    } else {
      map.set(key, [issue]);
    }
  }
  return Array.from(map.entries()).map(([path, items]) => ({ path, issues: items }));
};

// 补丁建议按路径分组，用于折叠展示
export const groupPatchPreviewItemsByPath = (items: LintPatchPreviewItem[]): { path: string; items: LintPatchPreviewItem[] }[] => {
  const map = new Map<string, LintPatchPreviewItem[]>();
  for (const item of items) {
    const key = item.path ?? "全局";
    const existing = map.get(key);
    if (existing) {
      existing.push(item);
    } else {
      map.set(key, [item]);
    }
  }
  return Array.from(map.entries()).map(([path, items]) => ({ path, items }));
};

export const parseLegacyWikiMetadataFromContent = (content: string | null | undefined) => {
  const sourcePattern = /^-\s*source:\s*(.+)$/i;
  const rawPattern = /^-\s*raw:\s*(.+)$/i;
  const stripMarkdownCode = (value: string) => value.trim().replace(/^`/, "").replace(/`$/, "");
  const result: {
    source?: string;
    raw?: string;
  } = {};

  for (const line of (content ?? "").split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }

    const sourceMatched = trimmed.match(sourcePattern);
    if (sourceMatched) {
      result.source = stripMarkdownCode(sourceMatched[1] ?? "");
      continue;
    }

    const rawMatched = trimmed.match(rawPattern);
    if (rawMatched) {
      result.raw = stripMarkdownCode(rawMatched[1] ?? "");
    }
  }

  return result;
};

export const parseLegacyImportedAtFromContent = (content: string | null | undefined) => {
  const importedAtPattern = /^-\s*imported\s+at:\s*(.+)$/i;
  const stripMarkdownCode = (value: string) => value.trim().replace(/^`/, "").replace(/`$/, "");

  for (const line of (content ?? "").split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) {
      continue;
    }
    const importedMatched = trimmed.match(importedAtPattern);
    if (importedMatched) {
      return stripMarkdownCode(importedMatched[1] ?? "");
    }
  }

  return "";
};

export const resolveWikiImportedAtDebugValue = (detail: WikiPageDetail | null | undefined) => {
  const frontmatterValue = detail?.frontmatter?.imported_at?.trim() ?? "";
  if (frontmatterValue) {
    return frontmatterValue;
  }
  return parseLegacyImportedAtFromContent(detail?.content);
};

export const buildWikiFrontmatterDisplay = (detail: WikiPageDetail | null | undefined) => {
  const frontmatter = detail?.frontmatter ?? null;
  const legacyMetadata = parseLegacyWikiMetadataFromContent(detail?.content);
  const sourceRaw = frontmatter?.source ?? legacyMetadata.source ?? "";
  const rawRaw = frontmatter?.raw ?? legacyMetadata.raw ?? "";
  const rows = [
    {
      key: "title",
      label: "title",
      value: frontmatter?.title ?? "",
      displayValue: (frontmatter?.title ?? "").trim(),
    },
    {
      key: "source",
      label: "source",
      value: sourceRaw,
      displayValue: sourceRaw.trim(),
    },
    {
      key: "raw",
      label: "raw",
      value: rawRaw,
      displayValue: rawRaw.trim(),
    },
  ].filter((item) => item.value.trim().length > 0);
  const entities = (frontmatter?.entities ?? [])
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
  const hasMeta = rows.length > 0 || entities.length > 0;

  return {
    frontmatter,
    rows,
    entities,
    totalCount: rows.length + (entities.length ? 1 : 0),
    hasMeta,
  };
};

export const buildFrontmatterCopyText = (field: string, value: string) => `${field}: ${value}`;

// 解析光标前是否处于 [[... 自动补全上下文
export const resolveWikiAutocompleteMatch = (
  textBeforeCursor: string,
): WikiAutocompleteMatch | null => {
  const match = textBeforeCursor.match(/\[\[([^\]\n]*)$/);
  if (!match) {
    return null;
  }
  const query = match[1] ?? "";
  const triggerStart = textBeforeCursor.length - query.length - 2;
  if (triggerStart < 0) {
    return null;
  }
  return { triggerStart, query };
};

// 将光标前的 [[query 替换为 [[path]]，并返回新光标位置
export const applyWikiAutocompleteSelection = (input: {
  content: string;
  cursor: number;
  path: string;
}) => {
  const textBeforeCursor = input.content.slice(0, input.cursor);
  const match = resolveWikiAutocompleteMatch(textBeforeCursor);
  if (!match) {
    return null;
  }

  const prefix = input.content.slice(0, match.triggerStart);
  const suffix = input.content.slice(input.cursor);
  const replacedBefore = `${prefix}[[${input.path}]]`;
  const nextContent = `${replacedBefore}${suffix}`;
  return {
    content: nextContent,
    cursor: replacedBefore.length,
  };
};

// 编辑态下内容与原文不一致时，视为存在未保存改动。
export const hasUnsavedWikiEditChanges = (
  wikiEditMode: boolean,
  wikiEditContent: string,
  detailContent: string | null | undefined,
) => {
  if (!wikiEditMode) {
    return false;
  }
  return wikiEditContent !== (detailContent ?? "");
};

export const shouldAutoDismissStatusMessage = (message: string) => {
  const normalized = message.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  const stickyKeywords = ["失败", "错误", "error", "failed", "warning", "告警"];
  if (stickyKeywords.some((keyword) => normalized.includes(keyword))) {
    return false;
  }

  const progressKeywords = ["中...", "加载中", "切换中", "running", "处理中"];
  if (progressKeywords.some((keyword) => normalized.includes(keyword))) {
    return false;
  }

  return true;
};
