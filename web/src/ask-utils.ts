import type { AskHistoryItem, AskSessionItem, AskSessionTurnItem, ProgressPayload } from "./types";
import type { OcrProvider } from "./tauri-client";

// ---- Ask/Query types ----

export type DropMode = "direct" | "queue";

export type QueryProgressUpdate =
  | { kind: "status"; text: string }
  | { kind: "chunk"; text: string };

// LintIssueGroup is defined in wiki-utils; re-used here if needed via import.
// DropMode is here since it's used by InboxModule and SettingsModule for ask/drop behaviour.

// ---- Ask/Query constants ----

export const OCR_PROVIDER_STORAGE_KEY = "llm_wiki_ocr_provider_v1";
export const ASK_SEARCH_DEBUG_VISIBLE_STORAGE_KEY = "llm_wiki_ask_search_debug_visible_v1";
export const DROP_MODE_STORAGE_KEY = "llm-wiki-drop-mode";
export const QUERY_HISTORY_STORAGE_KEY = "llm_wiki_query_history";
export const QUERY_HISTORY_MAX = 30;

// ---- Ask/Query functions ----

export const isDropMode = (value: string): value is DropMode =>
  value === "direct" || value === "queue";

export const readDropModeFromStorage = (): DropMode => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return "direct";
    const raw = storage.getItem(DROP_MODE_STORAGE_KEY);
    if (!raw) return "direct";
    return isDropMode(raw) ? raw : "direct";
  } catch {
    return "direct";
  }
};

export const writeDropModeToStorage = (mode: DropMode): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return;
    storage.setItem(DROP_MODE_STORAGE_KEY, mode);
  } catch {
    // 本地存储不可用时静默降级
  }
};

export const readAskSearchDebugVisibleFromStorage = (): boolean => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return false;
    }
    return storage.getItem(ASK_SEARCH_DEBUG_VISIBLE_STORAGE_KEY) === "1";
  } catch {
    return false;
  }
};

export const writeAskSearchDebugVisibleToStorage = (visible: boolean): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(ASK_SEARCH_DEBUG_VISIBLE_STORAGE_KEY, visible ? "1" : "0");
  } catch {
    // 本地存储不可用时静默降级，避免影响主流程。
  }
};

export const normalizeQueryHistory = (
  questions: string[],
  max = QUERY_HISTORY_MAX,
): string[] => {
  const normalized: string[] = [];
  const seen = new Set<string>();

  for (const rawQuestion of questions) {
    const question = rawQuestion.trim();
    if (!question || seen.has(question)) {
      continue;
    }
    normalized.push(question);
    seen.add(question);
    if (normalized.length >= max) {
      break;
    }
  }

  return normalized;
};

export const mergeQueryHistory = (
  previous: string[],
  nextQuestion: string,
  max = QUERY_HISTORY_MAX,
): string[] => normalizeQueryHistory([nextQuestion, ...previous], max);

export const normalizeQueryHistoryItems = (
  history: AskHistoryItem[],
  max = QUERY_HISTORY_MAX,
): AskHistoryItem[] => {
  const normalized: AskHistoryItem[] = [];
  const seen = new Set<string>();

  for (const item of history) {
    const question = item.question.trim();
    if (!question || seen.has(question)) {
      continue;
    }
    normalized.push({
      id: item.id,
      question,
      created_at: item.created_at,
    });
    seen.add(question);
    if (normalized.length >= max) {
      break;
    }
  }

  return normalized;
};

export const readQueryHistoryItemsFromStorage = (): AskHistoryItem[] => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return [];
    }
    const raw = storage.getItem(QUERY_HISTORY_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    const items = parsed
      .map((item, index) => {
        if (typeof item === "string") {
          return {
            id: index + 1,
            question: item,
            created_at: "",
          } as AskHistoryItem;
        }
        if (!item || typeof item !== "object") {
          return null;
        }
        const record = item as Partial<AskHistoryItem>;
        if (typeof record.question !== "string") {
          return null;
        }
        return {
          id: typeof record.id === "number" ? record.id : index + 1,
          question: record.question,
          created_at: typeof record.created_at === "string" ? record.created_at : "",
        } as AskHistoryItem;
      })
      .filter((item): item is AskHistoryItem => Boolean(item));

    return normalizeQueryHistoryItems(items);
  } catch {
    return [];
  }
};

export const readQueryHistoryFromStorage = (): string[] => {
  return readQueryHistoryItemsFromStorage().map((item) => item.question);
};

export const writeQueryHistoryItemsToStorage = (
  history: AskHistoryItem[],
  max = QUERY_HISTORY_MAX,
): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) {
      return;
    }
    storage.setItem(QUERY_HISTORY_STORAGE_KEY, JSON.stringify(normalizeQueryHistoryItems(history, max)));
  } catch {
    // 本地存储不可用时静默降级，避免影响查询主流程。
  }
};

export const writeQueryHistoryToStorage = (
  history: string[],
  max = QUERY_HISTORY_MAX,
): void => {
  const items = history.map((question, index) => ({
    id: index + 1,
    question,
    created_at: "",
  }));
  writeQueryHistoryItemsToStorage(items, max);
};

export const mergeQueryHistoryItems = (
  previous: AskHistoryItem[],
  nextQuestion: string,
  createdAt: string,
  max = QUERY_HISTORY_MAX,
): AskHistoryItem[] =>
  normalizeQueryHistoryItems(
    [
      {
        id: Date.now(),
        question: nextQuestion,
        created_at: createdAt,
      },
      ...previous,
    ],
    max,
  );

export const filterQueryHistoryItems = (
  history: AskHistoryItem[],
  keyword: string,
): AskHistoryItem[] => {
  const normalizedKeyword = keyword.trim().toLocaleLowerCase("zh-CN");
  if (!normalizedKeyword) {
    return history;
  }
  return history.filter((item) =>
    item.question.toLocaleLowerCase("zh-CN").includes(normalizedKeyword),
  );
};

export const formatAskHistoryCreatedAt = (createdAt: string): string => {
  const trimmed = createdAt.trim();
  if (!trimmed) {
    return "";
  }

  let timestampMs = 0;
  if (/^\d+$/.test(trimmed)) {
    const value = Number(trimmed);
    if (Number.isFinite(value) && value > 0) {
      timestampMs = value < 1_000_000_000_000 ? value * 1000 : value;
    }
  } else {
    const parsed = Date.parse(trimmed);
    if (!Number.isNaN(parsed)) {
      timestampMs = parsed;
    }
  }

  if (!timestampMs) {
    return "";
  }

  const date = new Date(timestampMs);
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  const hours = `${date.getHours()}`.padStart(2, "0");
  const minutes = `${date.getMinutes()}`.padStart(2, "0");
  return `${month}-${day} ${hours}:${minutes}`;
};

export const filterAskSessions = (
  sessions: AskSessionItem[],
  keyword: string,
): AskSessionItem[] => {
  const normalizedKeyword = keyword.trim().toLocaleLowerCase("zh-CN");
  if (!normalizedKeyword) {
    return sessions;
  }
  return sessions.filter((item) => {
    const title = item.title?.toLocaleLowerCase("zh-CN") ?? "";
    const preview = item.last_turn_content?.toLocaleLowerCase("zh-CN") ?? "";
    return title.includes(normalizedKeyword) || preview.includes(normalizedKeyword);
  });
};

export const formatAskSessionSearchSnippet = (raw: string): string => {
  const normalized = raw.replace(/\s+/g, " ").trim();
  if (!normalized) {
    return "";
  }
  return normalized.length > 88 ? `${normalized.slice(0, 88)}…` : normalized;
};

export const buildAskSessionExportMarkdown = (
  session: Pick<AskSessionItem, "title" | "session_id" | "created_at" | "updated_at">,
  turns: Array<Pick<AskSessionTurnItem, "role" | "content" | "created_at">>,
): string => {
  const lines = [
    `# ${session.title || "未命名会话"}`,
    "",
    `- Session ID: \`${session.session_id}\``,
    `- Created At: ${session.created_at || ""}`,
    `- Updated At: ${session.updated_at || ""}`,
    "",
    "## 对话记录",
    "",
  ];

  if (turns.length === 0) {
    lines.push("_暂无对话内容_");
    return lines.join("\n");
  }

  for (const turn of turns) {
    lines.push(`### ${turn.role === "user" ? "用户" : "助手"} · ${turn.created_at || ""}`);
    lines.push("");
    lines.push(turn.content || "");
    lines.push("");
  }
  return lines.join("\n");
};

export const parseQueryProgressPayload = (payload: ProgressPayload): QueryProgressUpdate => {
  const step = payload.step.trim().toLowerCase();
  const text = payload.message ?? "";
  if (step === "answer_chunk") {
    return { kind: "chunk", text };
  }
  return { kind: "status", text };
};

export const formatSourceType = (sourceType: string): string => {
  const map: Record<string, string> = {
    file: "本地文件",
    url: "网页链接",
    pdf: "PDF 文档",
    clipboard: "剪贴板",
  };
  return map[sourceType] ?? sourceType;
};

export const isOcrProvider = (value: string): value is OcrProvider =>
  value === "tesseract" || value === "paddle";

export const readOcrProviderFromStorage = (): OcrProvider => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return "tesseract";
    const raw = storage.getItem(OCR_PROVIDER_STORAGE_KEY);
    if (!raw) return "tesseract";
    return isOcrProvider(raw) ? raw : "tesseract";
  } catch {
    return "tesseract";
  }
};

export const writeOcrProviderToStorage = (provider: OcrProvider): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return;
    storage.setItem(OCR_PROVIDER_STORAGE_KEY, provider);
  } catch {
    // 本地存储不可用时静默降级
  }
};
