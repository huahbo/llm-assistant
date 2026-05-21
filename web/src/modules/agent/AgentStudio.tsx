import { useEffect, useMemo, useRef, useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import AgentMemoryPanel from "./AgentMemoryPanel";
import AgentRunHistory from "./AgentRunHistory";
import AgentSkillsPanel from "./AgentSkillsPanel";
import AgentToolsPane from "./AgentToolsPane";
import { useMode } from "../../contexts/ModeContext";
import { useToast } from "../../contexts/ToastContext";
import {
  shellPolicyProfiles,
  useShellPolicy,
} from "../../contexts/ShellPolicyContext";
import {
  isTauriRuntime,
  startAgentRun,
  appendAgentRunEvent,
  listAgentRuns,
  archiveAgentRun,
  restoreAgentRun,
  listAgentRunEvents,
  completeAgentRun,
  generateAgentDraft,
  listAgentDrafts,
  checkAgentDraftConflict,
  approveAgentDraft,
  listAgentMemories,
  upsertAgentMemory,
  deleteAgentMemory,
  rewriteAgentDraft,
  listAgentSkills,
  upsertAgentSkill,
  deleteAgentSkill,
  runAgentTask,
  runShell,
  approveAndRunShell,
  createShellSession,
  closeShellSession,
  listenShellStreamChunk,
  approveAgentWrite,
  rejectAgentWrite,
  fetchWikiPageDetail,
  askConfirmDialog,
} from "../../tauri-client";
import { formatLintCheckedAt } from "../../lint-utils";
import type {
  AgentChatMessage,
  AgentDraftConflictInfo,
  AgentDraftItem,
  AgentMemoryItem,
  AgentRunEventItem,
  AgentRunEventLevel,
  AgentRunItem,
  AgentRunStatus,
  AgentSkillItem,
  ShellHistoryEntry,
  ShellSessionInfo,
  ShellStreamChunk,
} from "../../types";

// ── Constants ──────────────────────────────────────────────────────────────────
const AGENT_RUN_LIST_LIMIT = 50;
const AGENT_RUN_EVENT_LIST_LIMIT = 200;
const AGENT_DRAFT_LIST_LIMIT = 50;
const AGENT_SKILL_LIST_LIMIT = 50;
const AGENT_ACTIVE_SKILL_STORAGE_KEY = "llm_wiki_agent_active_skill";
const agentEventLevelOptions: AgentRunEventLevel[] = ["info", "warn", "error"];
const agentCompleteStatusOptions: AgentRunStatus[] = ["applied", "failed"];

// ── Types ──────────────────────────────────────────────────────────────────────
type AgentReviewTab = "draft" | "diff" | "citations";
type AgentFlowMode = "idle" | "playing" | "paused" | "done";
type AgentRightTab = "task" | "draft" | "tools";
type AgentExecTimelineItem = {
  key: string;
  kind: "tool" | "marker";
  level: AgentRunEventLevel | "awaiting_approval";
  title: string;
  summary: string;
  detail?: string;
  createdAt: string;
  durationMs?: number;
};

export type WikiLineDiffKind = "unchanged" | "added" | "removed";
export type WikiLineDiffRow = {
  kind: WikiLineDiffKind;
  line: string;
  oldLineNumber?: number;
  newLineNumber?: number;
};

// ── Pure helper functions ──────────────────────────────────────────────────────
const formatAgentRunStatusLabel = (status: string): string => {
  const normalized = status.trim().toLowerCase();
  if (normalized === "running") return "生成中";
  if (normalized === "applied" || normalized === "approved") return "已写入 Wiki";
  if (normalized === "failed") return "执行失败";
  if (normalized === "reviewing") return "待审阅";
  if (normalized === "queued") return "排队中";
  return status || "未知状态";
};

const getAgentRunStatusTone = (status: string): "running" | "reviewing" | "applied" | "failed" | "queued" | "unknown" => {
  const normalized = status.trim().toLowerCase();
  if (normalized === "running") return "running";
  if (normalized === "reviewing") return "reviewing";
  if (normalized === "applied" || normalized === "approved") return "applied";
  if (normalized === "failed") return "failed";
  if (normalized === "queued") return "queued";
  return "unknown";
};

const getAgentStatusTone = (message: string): "info" | "success" | "warn" | "error" => {
  const text = String(message ?? "");
  if (!text) return "info";
  if (text.includes("失败") || text.includes("错误")) return "error";
  if (text.includes("拒绝") || text.includes("取消")) return "warn";
  if (text.includes("✅") || text.includes("已写入") || text.includes("已审批")) return "success";
  return "info";
};

const readAgentActiveSkillKeyFromStorage = (): string => {
  if (typeof window === "undefined") return "";
  try {
    const raw = globalThis.localStorage?.getItem(AGENT_ACTIVE_SKILL_STORAGE_KEY) ?? "";
    return raw.trim();
  } catch {
    return "";
  }
};

const writeAgentActiveSkillKeyToStorage = (skillKey: string): void => {
  if (typeof window === "undefined") return;
  try {
    if (skillKey.trim()) {
      globalThis.localStorage?.setItem(AGENT_ACTIVE_SKILL_STORAGE_KEY, skillKey.trim());
    } else {
      globalThis.localStorage?.removeItem(AGENT_ACTIVE_SKILL_STORAGE_KEY);
    }
  } catch {
    // 本地存储异常时静默降级，不阻塞主流程。
  }
};

const extractSkillKeyFromEventMessage = (message: string): string => {
  const normalized = message.trim();
  if (!normalized) return "";
  const matched = normalized.match(/skill:\s*([^\s，,)\]）]+)/i);
  return matched?.[1]?.trim() ?? "";
};

const normalizeEventLevel = (level: string): AgentRunEventLevel | "awaiting_approval" => {
  const normalized = String(level).trim().toLowerCase();
  if (normalized === "warn" || normalized === "error" || normalized === "awaiting_approval") {
    return normalized;
  }
  return "info";
};

const isAwaitingApprovalMarker = (level: string, message: string): boolean => {
  const normalizedLevel = normalizeEventLevel(level);
  if (normalizedLevel === "awaiting_approval") return true;
  const text = String(message ?? "");
  return text.includes("等待审批") || text.includes("等待人工确认") || text.includes("require_approval");
};

const isApprovalResolvedMarker = (message: string): boolean => {
  const text = String(message ?? "");
  return (
    text.includes("审批通过")
    || text.includes("审批拒绝")
    || text.includes("已取消写入")
    || text.includes("已写入:")
    || text.includes("已编辑:")
  );
};

const parseEventTimestamp = (value: string): number | null => {
  const raw = String(value ?? "").trim();
  if (!raw) return null;
  if (/^\d+$/.test(raw)) {
    const parsed = Number(raw);
    return Number.isFinite(parsed) ? parsed : null;
  }
  const parsed = Date.parse(raw);
  return Number.isFinite(parsed) ? parsed : null;
};

const truncateText = (text: string, max = 160): string => {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (normalized.length <= max) return normalized;
  return `${normalized.slice(0, max)}...`;
};

const chunkAgentDraftBlock = (block: string, maxLen = 180): string[] => {
  const normalized = block.trim();
  if (!normalized) return [];
  if (normalized.length <= maxLen || /^#{1,6}\s+/.test(normalized) || /^[-*]\s+/.test(normalized)) {
    return [`${normalized}\n\n`];
  }
  const chunks: string[] = [];
  let cursor = 0;
  while (cursor < normalized.length) {
    let end = Math.min(normalized.length, cursor + maxLen);
    if (end < normalized.length) {
      const cutAt = normalized.lastIndexOf(" ", end);
      if (cutAt > cursor + Math.floor(maxLen * 0.45)) {
        end = cutAt;
      }
    }
    const part = normalized.slice(cursor, end).trim();
    if (part) {
      chunks.push(`${part}${end >= normalized.length ? "\n\n" : " "}`);
    }
    cursor = end;
  }
  return chunks;
};

const buildAgentDraftFlowModel = (content: string): { prefix: string; chunks: string[]; outline: string[] } => {
  const normalized = content.replace(/\r\n/g, "\n").trim();
  if (!normalized) return { prefix: "", chunks: [], outline: [] };
  const blocks = normalized
    .split(/\n{2,}/)
    .map((item) => item.trim())
    .filter(Boolean);
  const outline = blocks
    .filter((item) => /^#{1,6}\s+/.test(item))
    .map((item) => item.replace(/^#{1,6}\s+/, "").trim())
    .filter(Boolean);
  const prefix = outline.length > 0
    ? [
      "### 结构提纲",
      ...outline.map((item, index) => `${index + 1}. ${item}`),
      "",
      "### 正文生成",
      "",
    ].join("\n")
    : "";
  const chunks = blocks.flatMap((block) => chunkAgentDraftBlock(block));
  return { prefix, chunks, outline };
};

export const buildWikiLineDiff = (currentContent: string, historyContent: string): WikiLineDiffRow[] => {
  const currentLines = currentContent.split(/\r?\n/);
  const historyLines = historyContent.split(/\r?\n/);
  const rows: WikiLineDiffRow[] = [];
  let oldIndex = 0;
  let newIndex = 0;
  while (oldIndex < currentLines.length || newIndex < historyLines.length) {
    const currentLine = currentLines[oldIndex];
    const historyLine = historyLines[newIndex];
    if (currentLine === historyLine) {
      rows.push({ kind: "unchanged", line: currentLine ?? "", oldLineNumber: newIndex + 1, newLineNumber: oldIndex + 1 });
      oldIndex += 1;
      newIndex += 1;
    } else if (oldIndex < currentLines.length) {
      rows.push({ kind: "added", line: currentLine ?? "", newLineNumber: oldIndex + 1 });
      oldIndex += 1;
    } else {
      rows.push({ kind: "removed", line: historyLine ?? "", oldLineNumber: newIndex + 1 });
      newIndex += 1;
    }
  }
  return rows;
};

const copyTextToClipboard = async (text: string): Promise<boolean> => {
  try {
    if (globalThis.navigator?.clipboard?.writeText) {
      await globalThis.navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // 忽略后继续走降级路径
  }
  try {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    textarea.style.top = "-9999px";
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    const copied = document.execCommand("copy");
    document.body.removeChild(textarea);
    return copied;
  } catch {
    return false;
  }
};

// ── Props ──────────────────────────────────────────────────────────────────────
type AgentStudioProps = {
  onOpenWikiPage: (path: string) => void;
  onDebugToggle?: (open: boolean) => void;
};

// ── Component ──────────────────────────────────────────────────────────────────
export default function AgentStudio({ onOpenWikiPage: _onOpenWikiPage, onDebugToggle }: AgentStudioProps) {
  // Context hooks
  const { activeModule } = useMode();
  const { agentStatusMessage, setAgentStatusMessage } = useToast();
  const {
    config: agentShellPolicyConfig,
    saving: agentShellPolicySaving,
    message: shellPolicyStatusMessage,
    clearMessage: clearShellPolicyStatusMessage,
    reload: handleReloadShellPolicy,
    applyAndSaveProfile: handleApplyAndSaveShellPolicyProfile,
  } = useShellPolicy();

  // ── State ────────────────────────────────────────────────────────────────────
  const [agentRuns, setAgentRuns] = useState<AgentRunItem[]>([]);
  const [agentRunsLoading, setAgentRunsLoading] = useState(false);
  const [agentEvents, setAgentEvents] = useState<AgentRunEventItem[]>([]);
  const [agentEventsLoading, setAgentEventsLoading] = useState(false);
  const [agentDrafts, setAgentDrafts] = useState<AgentDraftItem[]>([]);
  const [agentDraftsLoading, setAgentDraftsLoading] = useState(false);
  const [agentSelectedRunId, setAgentSelectedRunId] = useState<number | null>(null);
  const [agentSelectedDraftId, setAgentSelectedDraftId] = useState<number | null>(null);
  const [agentTopicInput, setAgentTopicInput] = useState("");
  const [agentEventLevel, setAgentEventLevel] = useState<AgentRunEventLevel>("info");
  const [agentEventMessage, setAgentEventMessage] = useState("");
  const [agentCompleteStatus, setAgentCompleteStatus] = useState<AgentRunStatus>("applied");
  const [agentActionRunning, setAgentActionRunning] = useState(false);
  const [agentReviewTab, setAgentReviewTab] = useState<AgentReviewTab>("draft");
  const [agentRightTab, setAgentRightTab] = useState<AgentRightTab>("task");
  const [agentDraftConflictPreview, setAgentDraftConflictPreview] = useState<AgentDraftConflictInfo | null>(null);
  const [agentDraftDiffBaseContent, setAgentDraftDiffBaseContent] = useState("");
  const [agentDraftConflictLoading, setAgentDraftConflictLoading] = useState(false);
  const [agentFlowMode, setAgentFlowMode] = useState<AgentFlowMode>("idle");
  const [agentFlowDraftId, setAgentFlowDraftId] = useState<number | null>(null);
  const [agentFlowCursor, setAgentFlowCursor] = useState(0);
  const [agentFlowChunks, setAgentFlowChunks] = useState<string[]>([]);
  const [agentFlowOutline, setAgentFlowOutline] = useState<string[]>([]);
  const [agentFlowRenderedContent, setAgentFlowRenderedContent] = useState("");
  const [agentApproveConfirm, setAgentApproveConfirm] = useState<AgentDraftConflictInfo | null>(null);
  // 记忆面板
  const [agentMemories, setAgentMemories] = useState<AgentMemoryItem[]>([]);
  const [agentMemoriesLoading, setAgentMemoriesLoading] = useState(false);
  const [agentMemoryKeyInput, setAgentMemoryKeyInput] = useState("");
  const [agentMemoryValueInput, setAgentMemoryValueInput] = useState("");
  const [agentMemoryComposerOpen, setAgentMemoryComposerOpen] = useState(false);
  // 技能面板
  const [agentSkills, setAgentSkills] = useState<AgentSkillItem[]>([]);
  const [agentSkillsLoading, setAgentSkillsLoading] = useState(false);
  const [agentActiveSkillKey, setAgentActiveSkillKey] = useState<string>(
    () => readAgentActiveSkillKeyFromStorage(),
  );
  // 生成选项
  const [agentResearchMode, setAgentResearchMode] = useState(false);
  const [agentAskFirst, setAgentAskFirst] = useState(false);
  const [agentRewriteComment, setAgentRewriteComment] = useState("");
  // 调试面板
  const [agentDebugPanelOpen, setAgentDebugPanelOpen] = useState(false);
  // 任务模式
  const [agentTaskInstruction, setAgentTaskInstruction] = useState("");
  const [agentTaskMaxIterations, setAgentTaskMaxIterations] = useState(4);
  const [agentTaskRunning, setAgentTaskRunning] = useState(false);
  const [agentTaskResult, setAgentTaskResult] = useState("");
  // Shell
  const [agentShellCmd, setAgentShellCmd] = useState<string>("");
  const [agentShellHistory, setAgentShellHistory] = useState<ShellHistoryEntry[]>([]);
  const [agentShellRunning, setAgentShellRunning] = useState<boolean>(false);
  const [agentShellSession, setAgentShellSession] = useState<ShellSessionInfo | null>(null);
  const [agentShellTheme, setAgentShellTheme] = useState<"deep" | "light">("deep");
  const [agentShellHistoryCursor, setAgentShellHistoryCursor] = useState<number>(-1);
  const [agentShellDraftInput, setAgentShellDraftInput] = useState<string>("");
  // UI
  const [agentToolsSeenCount, setAgentToolsSeenCount] = useState<number>(0);
  const [agentRunStripOpen, setAgentRunStripOpen] = useState<boolean>(false);
  const [agentRunManageMode, setAgentRunManageMode] = useState<boolean>(false);
  const [agentRunMutatingId, setAgentRunMutatingId] = useState<number | null>(null);
  const [agentContextOpen, setAgentContextOpen] = useState<boolean>(false);
  const [agentMemoryPanelOpen, setAgentMemoryPanelOpen] = useState<boolean>(true);
  // 面板分割比例
  const [agentLeftRatio, setAgentLeftRatio] = useState(0.54);

  // ── Refs ─────────────────────────────────────────────────────────────────────
  const agentShellIdRef = useRef(0);
  const agentShellSessionIdRef = useRef("");
  const agentShellHistoryRef = useRef<HTMLDivElement>(null);
  const agentShellAutoFollowRef = useRef(true);
  const agentShellInputRef = useRef<HTMLInputElement>(null);
  const agentDragRef = useRef({ active: false, startX: 0, startRatio: 0.54, containerW: 0 });
  const agentLayoutRef = useRef<HTMLDivElement>(null);

  // ── useMemo computations ─────────────────────────────────────────────────────
  const selectedAgentRun = useMemo(
    () => agentRuns.find((run) => run.id === agentSelectedRunId) ?? null,
    [agentRuns, agentSelectedRunId],
  );
  const selectedAgentDraft = useMemo(
    () => agentDrafts.find((draft) => draft.id === agentSelectedDraftId) ?? null,
    [agentDrafts, agentSelectedDraftId],
  );
  const agentChatMessages = useMemo<AgentChatMessage[]>(() => {
    const sortedRuns = [...agentRuns].sort((left, right) => {
      if (left.id !== right.id) return left.id - right.id;
      return String(left.created_at).localeCompare(String(right.created_at));
    });
    const messages: AgentChatMessage[] = [];
    for (const run of sortedRuns) {
      const topic = run.topic?.trim() || `Run #${run.id}`;
      messages.push({
        id: `run-${run.id}-user`,
        run_id: run.id,
        role: "user",
        content: topic,
        created_at: run.created_at,
      });
      let agentContent = `状态：${formatAgentRunStatusLabel(String(run.status))}`;
      if (run.id === agentSelectedRunId) {
        const latestEvent = agentEvents[0];
        if (latestEvent?.message?.trim()) {
          agentContent = latestEvent.message.trim();
        }
        if (agentDrafts.length > 0) {
          const newest = agentDrafts[0];
          agentContent += ` · 草稿 ${agentDrafts.length} 份（最新 #${newest.id}）`;
        }
      }
      messages.push({
        id: `run-${run.id}-agent`,
        run_id: run.id,
        role: "agent",
        content: agentContent,
        created_at: run.updated_at || run.created_at,
        status: String(run.status),
        draft_id: run.id === agentSelectedRunId ? (agentDrafts[0]?.id ?? null) : null,
      });
    }
    return messages;
  }, [agentRuns, agentSelectedRunId, agentEvents, agentDrafts]);

  const agentDraftDiffRows = useMemo(() => {
    if (!selectedAgentDraft || !agentDraftDiffBaseContent.trim()) {
      return [] as WikiLineDiffRow[];
    }
    return buildWikiLineDiff(
      selectedAgentDraft.content ?? "",
      agentDraftDiffBaseContent,
    );
  }, [selectedAgentDraft, agentDraftDiffBaseContent]);

  const agentDraftCitations = useMemo(() => {
    if (!selectedAgentDraft?.content) return [] as string[];
    const matches = selectedAgentDraft.content.match(/\[\[([^\]]+)\]\]/g) ?? [];
    const unique = new Set<string>();
    for (const raw of matches) {
      const normalized = raw.replace(/^\[\[/, "").replace(/\]\]$/, "").trim();
      if (normalized) unique.add(normalized);
    }
    return Array.from(unique);
  }, [selectedAgentDraft]);

  const agentDraftDisplayContent = useMemo(() => {
    if (!selectedAgentDraft) return "";
    if (agentReviewTab !== "draft") return selectedAgentDraft.content ?? "";
    if (agentFlowDraftId === selectedAgentDraft.id && agentFlowRenderedContent.trim()) {
      return agentFlowRenderedContent;
    }
    return selectedAgentDraft.content ?? "";
  }, [selectedAgentDraft, agentReviewTab, agentFlowDraftId, agentFlowRenderedContent]);

  const agentRunCards = useMemo(() => {
    return [...agentRuns].sort((left, right) => {
      const leftTs = Number(left.updated_at || left.created_at || 0);
      const rightTs = Number(right.updated_at || right.created_at || 0);
      if (leftTs !== rightTs) return rightTs - leftTs;
      return right.id - left.id;
    });
  }, [agentRuns]);

  const agentArchivedRunCount = useMemo(
    () => agentRuns.filter((run) => Boolean(run.archived_at)).length,
    [agentRuns],
  );

  const agentVisibleRunCards = useMemo(
    () => agentRunCards.filter((run) => agentRunManageMode || !run.archived_at),
    [agentRunCards, agentRunManageMode],
  );

  const agentFlowProgress = useMemo(() => {
    if (agentFlowChunks.length === 0) {
      return agentFlowMode === "idle" ? 0 : 100;
    }
    return Math.min(100, Math.round((agentFlowCursor / agentFlowChunks.length) * 100));
  }, [agentFlowChunks.length, agentFlowCursor, agentFlowMode]);

  const agentDraftAppliedSkillKey = useMemo(() => {
    if (agentSelectedRunId == null || agentEvents.length === 0) return "";
    for (let index = agentEvents.length - 1; index >= 0; index -= 1) {
      const event = agentEvents[index];
      if (!event || event.run_id !== agentSelectedRunId) continue;
      const skillKey = extractSkillKeyFromEventMessage(event.message ?? "");
      if (skillKey) return skillKey;
    }
    return "";
  }, [agentEvents, agentSelectedRunId]);

  const agentExecTimeline = useMemo<AgentExecTimelineItem[]>(() => {
    if (agentEvents.length === 0) return [];
    const orderedEvents = [...agentEvents].reverse();
    const pendingStarts = new Map<
      string,
      { created_at: string; toolName: string; title: string; detail: string }
    >();
    const timeline: AgentExecTimelineItem[] = [];

    for (const event of orderedEvents) {
      const message = String(event.message ?? "").trim();
      if (!message) continue;
      const startMatched = message.match(/tool_start #(\d+)\s+([a-z_]+):\s*(.*)$/i);
      if (startMatched) {
        const key = startMatched[1];
        const toolName = startMatched[2];
        const detail = startMatched[3] ?? "";
        pendingStarts.set(key, {
          created_at: event.created_at,
          toolName,
          title: `#${key} ${toolName}`,
          detail: detail.trim(),
        });
        continue;
      }
      const endMatched = message.match(/tool_end #(\d+)\s+(.+)$/i);
      if (endMatched) {
        const key = endMatched[1];
        const startInfo = pendingStarts.get(key);
        if (startInfo) pendingStarts.delete(key);
        const startMs = startInfo ? parseEventTimestamp(startInfo.created_at) : null;
        const endMs = parseEventTimestamp(event.created_at);
        const durationMs = startMs != null && endMs != null ? Math.max(0, endMs - startMs) : undefined;
        const endDetail = endMatched[2].trim();
        timeline.push({
          key: `${event.id}-tool-${key}`,
          kind: "tool",
          level: normalizeEventLevel(event.level),
          title: startInfo?.title ?? `#${key} 工具调用`,
          summary: startInfo?.detail ? truncateText(startInfo.detail, 110) : truncateText(endDetail, 110),
          detail: endDetail,
          createdAt: event.created_at,
          durationMs,
        });
        continue;
      }
      const normalizedLevel = normalizeEventLevel(event.level);
      if (normalizedLevel === "awaiting_approval" || message.includes("等待人工确认")) {
        timeline.push({
          key: `${event.id}-approval`,
          kind: "marker",
          level: "awaiting_approval",
          title: "等待审批",
          summary: truncateText(message, 120),
          createdAt: event.created_at,
        });
      }
    }

    for (const [key, startInfo] of pendingStarts) {
      timeline.push({
        key: `pending-${key}-${startInfo.created_at}`,
        kind: "tool",
        level: "info",
        title: startInfo.title,
        summary: truncateText(startInfo.detail || "工具已启动，等待返回", 120),
        detail: "工具尚未返回 tool_end 事件。",
        createdAt: startInfo.created_at,
      });
    }

    return timeline;
  }, [agentEvents]);

  const agentHasPendingApproval = useMemo(() => {
    if (!selectedAgentRun || agentSelectedRunId == null) return false;
    const normalizedStatus = String(selectedAgentRun.status ?? "").trim().toLowerCase();
    if (normalizedStatus === "awaiting_approval") return true;
    let latestAwaitingIndex = -1;
    let latestResolvedIndex = -1;
    for (let i = 0; i < agentEvents.length; i += 1) {
      const event = agentEvents[i];
      if (isAwaitingApprovalMarker(event.level, event.message)) latestAwaitingIndex = i;
      if (isApprovalResolvedMarker(event.message)) latestResolvedIndex = i;
    }
    return latestAwaitingIndex >= 0 && latestAwaitingIndex > latestResolvedIndex;
  }, [selectedAgentRun, agentSelectedRunId, agentEvents]);

  const agentToolsNeedsAttention = useMemo(
    () =>
      (agentRightTab !== "tools")
      && (agentShellRunning || agentShellHistory.length > agentToolsSeenCount),
    [agentRightTab, agentShellRunning, agentShellHistory.length, agentToolsSeenCount],
  );

  const agentShellQuickCommands = [
    { label: "pwd", command: "pwd", run: true },
    { label: "ls", command: "ls", run: true },
    { label: "ls -a", command: "ls -a", run: true },
    { label: "cd ..", command: "cd ..", run: true },
    { label: "git status", command: "git status", run: true },
  ] as const;

  // ── Load data helpers ────────────────────────────────────────────────────────
  const loadAgentRunsData = async (
    preferredRunId?: number | null,
    includeArchivedOverride?: boolean,
  ) => {
    if (!isTauriRuntime()) return;
    setAgentRunsLoading(true);
    try {
      const runs = await listAgentRuns(
        AGENT_RUN_LIST_LIMIT,
        includeArchivedOverride ?? agentRunManageMode,
      );
      setAgentRuns(runs);
      if (runs.length === 0) {
        setAgentSelectedRunId(null);
        setAgentEvents([]);
        setAgentDrafts([]);
        setAgentSelectedDraftId(null);
        return;
      }
      const pinnedRunId = preferredRunId ?? agentSelectedRunId;
      const nextRunId =
        pinnedRunId != null && runs.some((run) => run.id === pinnedRunId)
          ? pinnedRunId
          : runs[0].id;
      setAgentSelectedRunId(nextRunId);
    } finally {
      setAgentRunsLoading(false);
    }
  };

  const loadAgentRunEventsData = async (runId: number) => {
    if (!isTauriRuntime()) return;
    setAgentEventsLoading(true);
    try {
      const events = await listAgentRunEvents(runId, AGENT_RUN_EVENT_LIST_LIMIT);
      setAgentEvents(events);
    } finally {
      setAgentEventsLoading(false);
    }
  };

  const loadAgentDraftsData = async (runId: number, preferredDraftId?: number | null) => {
    if (!isTauriRuntime()) return;
    setAgentDraftsLoading(true);
    try {
      const drafts = await listAgentDrafts(runId, AGENT_DRAFT_LIST_LIMIT);
      setAgentDrafts(drafts);
      if (drafts.length === 0) {
        setAgentSelectedDraftId(null);
        return;
      }
      const pinnedDraftId = preferredDraftId ?? agentSelectedDraftId;
      const nextDraftId =
        pinnedDraftId != null && drafts.some((draft) => draft.id === pinnedDraftId)
          ? pinnedDraftId
          : drafts[0].id;
      setAgentSelectedDraftId(nextDraftId);
    } finally {
      setAgentDraftsLoading(false);
    }
  };

  const loadAgentMemoriesData = async (runId: number | null) => {
    if (!isTauriRuntime()) return;
    setAgentMemoriesLoading(true);
    try {
      const mems = await listAgentMemories(runId);
      setAgentMemories(mems);
    } finally {
      setAgentMemoriesLoading(false);
    }
  };

  const loadAgentSkillsData = async () => {
    if (!isTauriRuntime()) return;
    setAgentSkillsLoading(true);
    try {
      const skills = await listAgentSkills(AGENT_SKILL_LIST_LIMIT);
      setAgentSkills(skills);
      if (skills.length === 0) {
        setAgentActiveSkillKey("");
        return;
      }
      setAgentActiveSkillKey((prev) => {
        if (prev === "") return "";
        if (skills.some((item) => item.skill_key === prev)) return prev;
        return "";
      });
    } finally {
      setAgentSkillsLoading(false);
    }
  };

  const emitAgentFlowEvent = async (message: string) => {
    if (agentSelectedRunId == null || !isTauriRuntime()) return;
    try {
      const ok = await appendAgentRunEvent(agentSelectedRunId, "info", message);
      if (!ok) return;
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } catch {
      // 事件写入失败不阻塞主流程。
    }
  };

  // ── useEffects ───────────────────────────────────────────────────────────────

  useEffect(() => {
    onDebugToggle?.(agentDebugPanelOpen);
  }, [agentDebugPanelOpen, onDebugToggle]);

  // 拖拽分割条（agent layout）
  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (agentDragRef.current.active) {
        const delta = e.clientX - agentDragRef.current.startX;
        const newLeft = agentDragRef.current.startRatio * agentDragRef.current.containerW + delta;
        setAgentLeftRatio(Math.max(0.25, Math.min(0.75, newLeft / agentDragRef.current.containerW)));
      }
    };
    const onUp = () => {
      agentDragRef.current.active = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.body.classList.remove("split-dragging");
    };
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
    return () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
    };
  }, []);

  // Tools tab: 标记已读
  useEffect(() => {
    if (agentRightTab === "tools") {
      setAgentToolsSeenCount(agentShellHistory.length);
    }
  }, [agentRightTab, agentShellHistory.length]);

  // Shell: 自动跟随滚动
  useEffect(() => {
    if (agentRightTab !== "tools" || !agentShellAutoFollowRef.current) return;
    const container = agentShellHistoryRef.current;
    if (!container) return;
    container.scrollTop = container.scrollHeight;
  }, [agentRightTab, agentShellHistory]);

  // Shell session: 激活时创建
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    if (activeModule === "agent" && !agentShellSession) {
      void createShellSession("manual").then((session) => {
        if (disposed || !session) return;
        setAgentShellSession(session);
        agentShellSessionIdRef.current = session.session_id;
      });
    }
    return () => {
      disposed = true;
    };
  }, [activeModule, agentShellSession]);

  // Shell policy config: reload on demand
  useEffect(() => {
    if (!isTauriRuntime()) return;
    if ((activeModule !== "agent" && activeModule !== "settings") || agentShellPolicyConfig) return;
    void handleReloadShellPolicy({ silent: true });
  }, [activeModule, agentShellPolicyConfig, handleReloadShellPolicy]);

  // Shell policy status message bridge
  useEffect(() => {
    if (!shellPolicyStatusMessage) return;
    setAgentStatusMessage(shellPolicyStatusMessage);
    clearShellPolicyStatusMessage();
  }, [shellPolicyStatusMessage, clearShellPolicyStatusMessage, setAgentStatusMessage]);

  // Shell stream listener
  useEffect(() => {
    if (!isTauriRuntime()) return;
    let unlisten: (() => void) | null = null;
    let disposed = false;
    void listenShellStreamChunk((payload: ShellStreamChunk) => {
      if (disposed) return;
      setAgentShellHistory((history) =>
        history.map((entry) => {
          if (!entry.stream_id || entry.stream_id !== payload.stream_id) return entry;
          const chunk = payload.chunk ?? "";
          if (payload.stream === "stderr") {
            return {
              ...entry,
              live_stderr: `${entry.live_stderr ?? ""}${chunk}${chunk ? "\n" : ""}`,
              running: payload.done ? false : entry.running,
            };
          }
          if (payload.stream === "stdout") {
            return {
              ...entry,
              live_stdout: `${entry.live_stdout ?? ""}${chunk}${chunk ? "\n" : ""}`,
              running: payload.done ? false : entry.running,
            };
          }
          return { ...entry, running: payload.done ? false : entry.running };
        }),
      );
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // Shell session cleanup on unmount
  useEffect(() => {
    return () => {
      const sid = agentShellSessionIdRef.current.trim();
      if (!sid || !isTauriRuntime()) return;
      void closeShellSession(sid);
    };
  }, []);

  // Shell session ID ref sync
  useEffect(() => {
    agentShellSessionIdRef.current = agentShellSession?.session_id ?? "";
  }, [agentShellSession?.session_id]);

  // Load runs/skills when agent tab activated
  useEffect(() => {
    if (activeModule !== "agent") return;
    void loadAgentRunsData();
    void loadAgentSkillsData();
    // H0 阶段仅在切到 Agent Studio 时刷新，不做后台轮询。
  }, [activeModule]);

  // Reload runs when manage mode changes
  useEffect(() => {
    if (activeModule !== "agent") return;
    void loadAgentRunsData(agentSelectedRunId, agentRunManageMode);
  }, [activeModule, agentRunManageMode]);

  // Persist active skill key
  useEffect(() => {
    writeAgentActiveSkillKeyToStorage(agentActiveSkillKey);
  }, [agentActiveSkillKey]);

  // Load events/drafts/memories when selected run changes
  useEffect(() => {
    if (activeModule !== "agent") return;
    if (agentSelectedRunId == null) {
      setAgentEvents([]);
      setAgentDrafts([]);
      setAgentSelectedDraftId(null);
      return;
    }
    void loadAgentRunEventsData(agentSelectedRunId);
    void loadAgentDraftsData(agentSelectedRunId);
    void loadAgentMemoriesData(agentSelectedRunId);
  }, [activeModule, agentSelectedRunId]);

  // Draft conflict loading
  useEffect(() => {
    if (activeModule !== "agent" || selectedAgentDraft == null || !isTauriRuntime()) {
      setAgentDraftConflictPreview(null);
      setAgentDraftDiffBaseContent("");
      setAgentDraftConflictLoading(false);
      return;
    }
    let canceled = false;
    setAgentDraftConflictLoading(true);
    void checkAgentDraftConflict(selectedAgentDraft.id)
      .then(async (info) => {
        if (!canceled) setAgentDraftConflictPreview(info);
        const fallbackPreview = info?.existing_preview?.trim() ?? "";
        if (!info?.conflict || !info?.existing_path?.trim()) {
          if (!canceled) setAgentDraftDiffBaseContent(fallbackPreview);
          return;
        }
        try {
          const detail = await fetchWikiPageDetail(info.existing_path.trim());
          if (!canceled) setAgentDraftDiffBaseContent(detail?.content ?? fallbackPreview);
        } catch {
          if (!canceled) setAgentDraftDiffBaseContent(fallbackPreview);
        }
      })
      .catch(() => {
        if (!canceled) {
          setAgentDraftConflictPreview(null);
          setAgentDraftDiffBaseContent("");
        }
      })
      .finally(() => {
        if (!canceled) setAgentDraftConflictLoading(false);
      });
    return () => {
      canceled = true;
    };
  }, [activeModule, selectedAgentDraft]);

  // Review tab reset on draft change
  useEffect(() => {
    if (activeModule === "agent") setAgentReviewTab("draft");
  }, [activeModule, selectedAgentDraft?.id]);

  // Flow mode management: reset/init when draft changes
  useEffect(() => {
    if (activeModule !== "agent" || selectedAgentDraft == null) {
      setAgentFlowMode("idle");
      setAgentFlowDraftId(null);
      setAgentFlowCursor(0);
      setAgentFlowChunks([]);
      setAgentFlowOutline([]);
      setAgentFlowRenderedContent("");
      return;
    }
    const model = buildAgentDraftFlowModel(selectedAgentDraft.content ?? "");
    setAgentFlowDraftId(selectedAgentDraft.id);
    setAgentFlowChunks(model.chunks);
    setAgentFlowOutline(model.outline);
    setAgentFlowCursor(0);
    setAgentFlowRenderedContent(model.prefix);
    setAgentFlowMode(model.chunks.length > 0 ? "playing" : "done");
  }, [activeModule, selectedAgentDraft?.id, selectedAgentDraft?.content]);

  // Flow playback animation
  useEffect(() => {
    if (
      activeModule !== "agent"
      || agentReviewTab !== "draft"
      || agentFlowMode !== "playing"
      || selectedAgentDraft == null
      || agentFlowDraftId !== selectedAgentDraft.id
    ) {
      return;
    }
    if (agentFlowCursor >= agentFlowChunks.length) {
      setAgentFlowMode("done");
      void emitAgentFlowEvent("草稿流式渲染已完成");
      return;
    }
    const nextChunk = agentFlowChunks[agentFlowCursor] ?? "";
    const delayMs = nextChunk.length > 220 ? 75 : nextChunk.length > 120 ? 56 : 40;
    const timer = globalThis.setTimeout(() => {
      setAgentFlowRenderedContent((prev) => `${prev}${nextChunk}`);
      setAgentFlowCursor((prev) => prev + 1);
    }, delayMs);
    return () => globalThis.clearTimeout(timer);
  }, [
    activeModule,
    agentReviewTab,
    agentFlowMode,
    selectedAgentDraft,
    agentFlowDraftId,
    agentFlowCursor,
    agentFlowChunks,
  ]);

  // Pause flow when leaving draft tab
  useEffect(() => {
    if (agentReviewTab !== "draft" && agentFlowMode === "playing") {
      setAgentFlowMode("paused");
    }
  }, [agentReviewTab, agentFlowMode]);

  // ── Handlers ─────────────────────────────────────────────────────────────────
  const handleSelectAgentRunFromChat = (runId: number) => {
    setAgentSelectedRunId(runId);
  };

  const handleArchiveAgentRun = async (runId: number) => {
    if (!isTauriRuntime() || agentRunMutatingId != null) return;
    const ok = await askConfirmDialog(
      `确认归档 run #${runId} 吗？归档后默认列表将隐藏该 run，可在管理模式恢复。`,
      { title: "归档历史 Run", kind: "warning", okLabel: "归档", cancelLabel: "取消" },
    );
    if (!ok) return;
    setAgentRunMutatingId(runId);
    try {
      const archived = await archiveAgentRun(runId);
      if (!archived) {
        setAgentStatusMessage(`归档 run #${runId} 失败，请检查后端日志。`);
        return;
      }
      setAgentStatusMessage(`已归档 run #${runId}。`);
      await loadAgentRunsData(agentSelectedRunId, agentRunManageMode);
      if (agentSelectedRunId != null) await loadAgentRunEventsData(agentSelectedRunId);
    } finally {
      setAgentRunMutatingId(null);
    }
  };

  const handleRestoreAgentRun = async (runId: number) => {
    if (!isTauriRuntime() || agentRunMutatingId != null) return;
    setAgentRunMutatingId(runId);
    try {
      const restored = await restoreAgentRun(runId);
      if (!restored) {
        setAgentStatusMessage(`恢复 run #${runId} 失败，请检查后端日志。`);
        return;
      }
      setAgentStatusMessage(`已恢复 run #${runId}。`);
      await loadAgentRunsData(runId, agentRunManageMode);
      await loadAgentRunEventsData(runId);
    } finally {
      setAgentRunMutatingId(null);
    }
  };

  const handleReplayAgentFlow = () => {
    if (!selectedAgentDraft) return;
    const model = buildAgentDraftFlowModel(selectedAgentDraft.content ?? "");
    setAgentFlowDraftId(selectedAgentDraft.id);
    setAgentFlowChunks(model.chunks);
    setAgentFlowOutline(model.outline);
    setAgentFlowCursor(0);
    setAgentFlowRenderedContent(model.prefix);
    setAgentFlowMode(model.chunks.length > 0 ? "playing" : "done");
  };

  const handlePauseAgentFlow = () => {
    if (agentFlowMode === "playing") {
      setAgentFlowMode("paused");
      setAgentStatusMessage("已暂停草稿流式渲染。");
      void emitAgentFlowEvent("草稿流式渲染已暂停");
    }
  };

  const handleResumeAgentFlow = () => {
    if (selectedAgentDraft == null) return;
    if (agentFlowCursor >= agentFlowChunks.length) {
      setAgentFlowMode("done");
      return;
    }
    setAgentFlowMode("playing");
    setAgentStatusMessage("已继续草稿流式渲染。");
    void emitAgentFlowEvent("草稿流式渲染已继续");
  };

  const handleCompleteAgentFlow = () => {
    if (selectedAgentDraft == null) return;
    const model = buildAgentDraftFlowModel(selectedAgentDraft.content ?? "");
    setAgentFlowDraftId(selectedAgentDraft.id);
    setAgentFlowChunks(model.chunks);
    setAgentFlowOutline(model.outline);
    setAgentFlowCursor(model.chunks.length);
    setAgentFlowRenderedContent(`${model.prefix}${model.chunks.join("")}`);
    setAgentFlowMode("done");
    void emitAgentFlowEvent("草稿流式渲染已直接完成");
  };

  const handleAgentChatSend = async () => {
    const topic = agentTopicInput.trim();
    if (!topic) {
      setAgentStatusMessage("请输入主题后再发送。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const runId = await startAgentRun(topic);
      if (!runId) {
        setAgentStatusMessage("创建 run 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentTopicInput("");
      setAgentStatusMessage(`run #${runId} 已创建，正在生成 draft...`);
      await loadAgentRunsData(runId);
      await loadAgentRunEventsData(runId);
      const ok = await generateAgentDraft(runId, topic, agentActiveSkillKey || null, agentResearchMode, agentAskFirst);
      if (!ok) {
        setAgentStatusMessage(`run #${runId} draft 生成失败，请检查后端日志。`);
        return;
      }
      await loadAgentDraftsData(runId);
      await loadAgentRunEventsData(runId);
      await loadAgentRunsData(runId);
      setAgentStatusMessage(
        `run #${runId} draft 已就绪${agentActiveSkillKey ? `（skill: ${agentActiveSkillKey}）` : ""}，可在右侧预览并审批。`,
      );
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleCreateAgentRun = async () => {
    const topic = agentTopicInput.trim();
    if (!topic) {
      setAgentStatusMessage("请先输入主题。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const runId = await startAgentRun(topic);
      if (!runId) {
        setAgentStatusMessage("创建 run 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentTopicInput("");
      setAgentStatusMessage(`已创建 run #${runId}`);
      await loadAgentRunsData(runId);
      await loadAgentRunEventsData(runId);
      await loadAgentDraftsData(runId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleDiscardAgentDraftSelection = () => {
    setAgentSelectedDraftId(null);
    setAgentStatusMessage("已取消当前草稿选择，可在左侧重新选择 run。");
  };

  const handleAppendAgentEvent = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    const message = agentEventMessage.trim();
    if (!message) {
      setAgentStatusMessage("请输入事件内容。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const ok = await appendAgentRunEvent(agentSelectedRunId, agentEventLevel, message);
      if (!ok) {
        setAgentStatusMessage("追加事件失败，请检查后端命令是否可用。");
        return;
      }
      setAgentEventMessage("");
      setAgentStatusMessage(`已追加事件到 run #${agentSelectedRunId}`);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleCompleteAgentRun = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const ok = await completeAgentRun(agentSelectedRunId, agentCompleteStatus);
      if (!ok) {
        setAgentStatusMessage("结束 run 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentStatusMessage(`run #${agentSelectedRunId} 已更新为 ${agentCompleteStatus}`);
      await loadAgentRunsData(agentSelectedRunId);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentDraftsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleGenerateAgentDraft = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    const topic = agentTopicInput.trim() || selectedAgentRun?.topic?.trim() || "";
    if (!topic) {
      setAgentStatusMessage("请先输入主题，或先创建带主题的 run。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const ok = await generateAgentDraft(agentSelectedRunId, topic, agentActiveSkillKey || null, agentResearchMode, agentAskFirst);
      if (!ok) {
        setAgentStatusMessage("生成 draft 失败，请检查后端命令是否可用。");
        return;
      }
      setAgentStatusMessage(
        `已触发 run #${agentSelectedRunId} 的 draft 生成${agentActiveSkillKey ? `（skill: ${agentActiveSkillKey}）` : ""}`,
      );
      await loadAgentDraftsData(agentSelectedRunId);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const doApproveAgentDraft = async () => {
    if (agentSelectedRunId == null || agentSelectedDraftId == null) return;
    setAgentApproveConfirm(null);
    setAgentActionRunning(true);
    try {
      const ok = await approveAgentDraft(agentSelectedDraftId);
      if (!ok) {
        setAgentStatusMessage("审批失败，草稿内容未写盘，可重试或检查 Vault 路径。");
        return;
      }
      setAgentStatusMessage(`draft #${agentSelectedDraftId} 已审批写盘 ✓`);
      await loadAgentDraftsData(agentSelectedRunId, agentSelectedDraftId);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleApproveAgentDraft = async () => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return;
    }
    if (agentSelectedDraftId == null) {
      setAgentStatusMessage("请先选择一个 draft。");
      return;
    }
    const conflictInfo = await checkAgentDraftConflict(agentSelectedDraftId);
    if (conflictInfo) {
      setAgentApproveConfirm(conflictInfo);
    } else {
      await doApproveAgentDraft();
    }
  };

  const handleRewriteAgentDraft = async () => {
    if (agentSelectedDraftId == null) return;
    const comment = agentRewriteComment.trim();
    if (!comment) return;
    setAgentActionRunning(true);
    try {
      const newDraft = await rewriteAgentDraft(agentSelectedDraftId, comment);
      if (!newDraft) {
        setAgentStatusMessage("重写草稿失败，请检查后端日志。");
        return;
      }
      setAgentRewriteComment("");
      setAgentSelectedDraftId(newDraft.id);
      setAgentStatusMessage(`已基于批注生成新草稿 #${newDraft.id}`);
      if (agentSelectedRunId != null) {
        await loadAgentDraftsData(agentSelectedRunId, newDraft.id);
        await loadAgentRunEventsData(agentSelectedRunId);
      }
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleApplyShellCommand = (command: string, runImmediately = false) => {
    setAgentShellCmd(command);
    setAgentShellHistoryCursor(-1);
    setAgentShellDraftInput("");
    if (runImmediately) void runShellCommand(command);
  };

  const handleShellHistoryNav = (direction: "prev" | "next") => {
    const commands = agentShellHistory.map((entry) => entry.command);
    if (commands.length === 0) return;
    if (direction === "prev") {
      if (agentShellHistoryCursor === -1) {
        setAgentShellDraftInput(agentShellCmd);
        setAgentShellHistoryCursor(commands.length - 1);
        setAgentShellCmd(commands[commands.length - 1] ?? "");
        return;
      }
      const nextCursor = Math.max(0, agentShellHistoryCursor - 1);
      setAgentShellHistoryCursor(nextCursor);
      setAgentShellCmd(commands[nextCursor] ?? "");
      return;
    }
    if (agentShellHistoryCursor === -1) return;
    const nextCursor = agentShellHistoryCursor + 1;
    if (nextCursor >= commands.length) {
      setAgentShellHistoryCursor(-1);
      setAgentShellCmd(agentShellDraftInput);
      return;
    }
    setAgentShellHistoryCursor(nextCursor);
    setAgentShellCmd(commands[nextCursor] ?? "");
  };

  const handleCopyShellOutput = async (entry: ShellHistoryEntry) => {
    const payload = [
      `> ${entry.command}`,
      entry.result.stdout || "",
      entry.result.stderr || "",
    ]
      .filter((line) => line.trim().length > 0)
      .join("\n");
    const copied = await copyTextToClipboard(payload || entry.command);
    setAgentStatusMessage(copied ? "已复制命令输出。" : "复制失败：当前环境不支持写入剪贴板。");
  };

  const handleClearShellHistory = () => {
    setAgentShellHistory([]);
    setAgentToolsSeenCount(0);
    setAgentShellHistoryCursor(-1);
    setAgentShellDraftInput("");
  };

  const focusAgentShellInput = () => {
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        agentShellInputRef.current?.focus();
      });
    });
  };

  const runShellCommand = async (rawCommand?: string) => {
    const cmd = (rawCommand ?? agentShellCmd).trim();
    if (!cmd || agentShellRunning) return;
    const sessionId = agentShellSession?.session_id ?? null;
    const streamId = `sh-${Date.now()}-${Math.floor(Math.random() * 1_000_000).toString(16)}`;
    setAgentShellRunning(true);
    const id = ++agentShellIdRef.current;
    setAgentShellHistory((h) => [
      ...h,
      {
        id,
        command: cmd,
        result: {
          command: cmd,
          stdout: "",
          stderr: "",
          exit_code: 0,
          working_dir: agentShellSession?.working_dir ?? "",
          blocked: false,
          blocked_reason: null,
          policy_action: "unknown",
          policy_decision: "streaming",
          executor: "manual",
        },
        ts: Date.now(),
        running: true,
        live_stdout: "",
        live_stderr: "",
        stream_id: streamId,
        session_id: sessionId,
      },
    ]);
    try {
      const result = await runShell(cmd, undefined, "manual", sessionId, streamId);
      if (result) {
        setAgentShellHistory((h) =>
          h.map((entry) =>
            entry.id === id
              ? {
                  ...entry,
                  result: {
                    ...result,
                    stdout: entry.live_stdout?.trim().length ? entry.live_stdout : result.stdout,
                    stderr: entry.live_stderr?.trim().length ? entry.live_stderr : result.stderr,
                  },
                  running: false,
                }
              : entry,
          ),
        );
        if (result.working_dir && sessionId) {
          setAgentShellSession((prev) =>
            prev && prev.session_id === sessionId
              ? { ...prev, working_dir: result.working_dir }
              : prev,
          );
        }
      }
    } catch (e) {
      setAgentShellHistory((h) =>
        h.map((entry) =>
          entry.id === id
            ? {
                ...entry,
                result: {
                  command: cmd,
                  stdout: entry.live_stdout || "",
                  stderr: entry.live_stderr || String(e),
                  exit_code: -1,
                  working_dir: agentShellSession?.working_dir ?? "",
                  blocked: false,
                  blocked_reason: null,
                  policy_action: "unknown",
                  policy_decision: "error",
                  executor: "manual",
                },
                running: false,
              }
            : entry,
        ),
      );
    } finally {
      setAgentShellRunning(false);
      setAgentShellCmd("");
      setAgentShellHistoryCursor(-1);
      setAgentShellDraftInput("");
      focusAgentShellInput();
    }
  };

  const handleRunShell = async () => {
    await runShellCommand(agentShellCmd);
  };

  const handleApproveShellCommand = async (command: string) => {
    if (agentShellRunning || !agentShellSession) return;
    const streamId = `approve-${Date.now()}`;
    const sessionId = agentShellSession.session_id;
    const entry: ShellHistoryEntry = {
      id: Date.now(),
      command,
      result: { command, stdout: "", stderr: "", exit_code: 0, working_dir: "", blocked: false, blocked_reason: null, policy_action: "approved", policy_decision: "approved_by_user", executor: "manual" },
      ts: Date.now(),
      running: true,
      live_stdout: "",
      live_stderr: "",
      stream_id: streamId,
      session_id: sessionId,
    };
    setAgentShellHistory((prev) => [...prev, entry]);
    setAgentShellRunning(true);
    try {
      const result = await approveAndRunShell(command, undefined, sessionId, streamId);
      if (!result) return;
      setAgentShellHistory((prev) =>
        prev.map((e) => (e.stream_id === streamId ? { ...e, result, running: false } : e))
      );
    } finally {
      setAgentShellRunning(false);
    }
  };

  const executeAgentTask = async (instructionInput: string, statusLabel: string): Promise<boolean> => {
    if (agentSelectedRunId == null) {
      setAgentStatusMessage("请先选择一个 run。");
      return false;
    }
    const instruction = instructionInput.trim();
    if (!instruction) {
      setAgentStatusMessage("请输入任务指令。");
      return false;
    }
    const budget = Math.min(8, Math.max(1, agentTaskMaxIterations));
    const memoryLines = agentMemories
      .map((mem) => `- ${mem.memory_key || "记忆"}: ${mem.memory_value}`)
      .join("\n");
    const activeSkill = agentSkills.find((item) => item.skill_key === agentActiveSkillKey);
    const skillPromptRendered = activeSkill?.prompt_template
      ? activeSkill.prompt_template
          .replaceAll("{{topic}}", instruction)
          .replaceAll("{{memories}}", memoryLines || "（无）")
          .trim()
      : "";
    const contextSections: string[] = [];
    if (skillPromptRendered) {
      contextSections.push(`[技能模板：${activeSkill?.skill_key || "未命名"}]\n${skillPromptRendered}`);
    } else if (agentActiveSkillKey) {
      contextSections.push(`[技能模板：${agentActiveSkillKey}]`);
    }
    if (memoryLines) {
      contextSections.push(`[记忆上下文]\n${memoryLines}`);
    }
    const memoryContext = contextSections.join("\n\n");
    setAgentTaskRunning(true);
    try {
      const result = await runAgentTask(
        agentSelectedRunId,
        instruction,
        budget,
        memoryContext || undefined,
      );
      if (!result) {
        setAgentStatusMessage(`${statusLabel}执行失败，请检查后端日志。`);
        return false;
      }
      setAgentTaskResult(result);
      setAgentStatusMessage(`${statusLabel}已完成（run #${agentSelectedRunId}）。`);
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
      return true;
    } finally {
      setAgentTaskRunning(false);
    }
  };

  const handleRunAgentTask = async () => {
    await executeAgentTask(agentTaskInstruction, "任务模式");
  };

  const handleContinueAgentTask = async () => {
    const baseInstruction = agentTaskInstruction.trim() || selectedAgentRun?.topic?.trim() || "继续当前任务";
    const recentTrace = agentEvents
      .slice(0, 16)
      .reverse()
      .filter((event) => {
        const msg = String(event.message ?? "");
        return /tool_start|tool_end|awaiting_approval|任务模式/.test(msg);
      })
      .map((event) => `- [${formatLintCheckedAt(event.created_at)}] ${truncateText(String(event.message ?? ""), 180)}`)
      .join("\n");
    const previousResult = agentTaskResult.trim().slice(0, 900);
    const continuationInstruction = [
      "继续上次未完成的任务，优先复用已获得信息，避免重复调用工具。",
      `原始指令：${baseInstruction}`,
      previousResult ? `上次阶段性结果：\n${previousResult}` : "",
      recentTrace ? `最近执行轨迹：\n${recentTrace}` : "",
      "若仍需工具调用，请先说明理由，再执行最小必要步骤。",
    ]
      .filter(Boolean)
      .join("\n\n");
    await executeAgentTask(continuationInstruction, "续跑任务");
  };

  const handleApproveAgentWrite = async () => {
    if (!agentSelectedRunId) return;
    setAgentActionRunning(true);
    try {
      const msg = await approveAgentWrite(agentSelectedRunId);
      if (msg) {
        setAgentStatusMessage(`✅ 已写入 Wiki：${msg}`);
      } else {
        setAgentStatusMessage("✅ 已写入 Wiki。");
      }
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } catch (e) {
      setAgentStatusMessage(`审批失败: ${String(e)}`);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleRejectAgentWrite = async () => {
    if (!agentSelectedRunId) return;
    setAgentActionRunning(true);
    try {
      const msg = await rejectAgentWrite(agentSelectedRunId);
      if (msg) {
        setAgentStatusMessage(`🚫 已拒绝写入：${msg}`);
      } else {
        setAgentStatusMessage("🚫 已拒绝写入。");
      }
      await loadAgentRunEventsData(agentSelectedRunId);
      await loadAgentRunsData(agentSelectedRunId);
    } catch (e) {
      setAgentStatusMessage(`拒绝失败: ${String(e)}`);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleUpsertAgentMemory = async () => {
    const value = agentMemoryValueInput.trim();
    if (!value) {
      setAgentStatusMessage("记忆内容不能为空。");
      return;
    }
    const key = agentMemoryKeyInput.trim() || value.slice(0, 20);
    setAgentActionRunning(true);
    try {
      const item = await upsertAgentMemory(agentSelectedRunId, key, value);
      if (!item) {
        setAgentStatusMessage("保存记忆失败，请检查后端。");
        return;
      }
      setAgentMemoryKeyInput("");
      setAgentMemoryValueInput("");
      setAgentMemoryComposerOpen(false);
      setAgentStatusMessage(`记忆「${key}」已保存。`);
      await loadAgentMemoriesData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleDeleteAgentMemory = async (id: number) => {
    setAgentActionRunning(true);
    try {
      const ok = await deleteAgentMemory(id);
      if (!ok) {
        setAgentStatusMessage("删除记忆失败。");
        return;
      }
      setAgentStatusMessage("记忆已删除。");
      await loadAgentMemoriesData(agentSelectedRunId);
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleUpsertAgentSkill = async (skillKey: string, promptTemplate: string) => {
    if (!skillKey.trim() || !promptTemplate.trim()) {
      setAgentStatusMessage("技能键与模板内容不能为空。");
      return;
    }
    setAgentActionRunning(true);
    try {
      const item = await upsertAgentSkill(skillKey.trim(), promptTemplate.trim());
      if (!item) {
        setAgentStatusMessage("保存技能模板失败，请检查后端。");
        return;
      }
      setAgentActiveSkillKey(item.skill_key);
      setAgentStatusMessage(`技能模板「${item.skill_key}」已保存（v${item.version}）。`);
      await loadAgentSkillsData();
    } finally {
      setAgentActionRunning(false);
    }
  };

  const handleDeleteAgentSkill = async (id: number, skillKey: string) => {
    // 立即清除 active 状态，避免 localStorage 在异步期间残留脏数据
    if (agentActiveSkillKey === skillKey) {
      setAgentActiveSkillKey("");
    }
    setAgentActionRunning(true);
    try {
      const ok = await deleteAgentSkill(id);
      if (!ok) {
        setAgentStatusMessage("删除技能模板失败。");
        return;
      }
      setAgentStatusMessage(`技能模板「${skillKey}」已删除。`);
      await loadAgentSkillsData();
    } finally {
      setAgentActionRunning(false);
    }
  };

  // ── JSX ──────────────────────────────────────────────────────────────────────
  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">Agent Studio</h1>
        <p className="module-header__sub">左侧对话驱动，右侧草稿预览与审批写盘</p>
      </div>
      <section className={`panel agent-studio agent-studio--b2${agentDebugPanelOpen ? " agent-studio--debug-open" : ""}`}>
        {agentStatusMessage ? (
          <p className={`agent-studio__status agent-studio__status--${getAgentStatusTone(agentStatusMessage)}`}>
            {agentStatusMessage}
          </p>
        ) : null}
        <div
          ref={agentLayoutRef}
          className="agent-studio__layout"
          style={{ gridTemplateColumns: `minmax(0,${agentLeftRatio}fr) 6px minmax(0,${1 - agentLeftRatio}fr)`, gap: 0 }}
        >
          <section className="agent-studio__left">
            <AgentRunHistory
              runStripOpen={agentRunStripOpen}
              setRunStripOpen={setAgentRunStripOpen}
              runManageMode={agentRunManageMode}
              setRunManageMode={setAgentRunManageMode}
              runCards={agentRunCards}
              visibleRunCards={agentVisibleRunCards}
              archivedRunCount={agentArchivedRunCount}
              runsLoading={agentRunsLoading}
              selectedRunId={agentSelectedRunId}
              runMutatingId={agentRunMutatingId}
              onSelectRun={handleSelectAgentRunFromChat}
              onArchiveRun={(runId) => { void handleArchiveAgentRun(runId); }}
              onRestoreRun={(runId) => { void handleRestoreAgentRun(runId); }}
              formatTime={formatLintCheckedAt}
              formatStatusLabel={formatAgentRunStatusLabel}
              getStatusTone={getAgentRunStatusTone}
            />
            <div className="agent-studio__context">
              <button
                type="button"
                className="agent-studio__context-toggle"
                onClick={() => setAgentContextOpen((prev) => !prev)}
              >
                <span>{agentContextOpen ? "▼" : "▶"} 上下文配置（记忆 / 技能）</span>
                <span className="agent-studio__context-meta">
                  记忆 {agentMemories.length} · 技能 {agentSkills.length}
                </span>
              </button>
              {agentContextOpen ? (
                <div className="agent-studio__context-body">
                  <AgentMemoryPanel
                    panelOpen={agentMemoryPanelOpen}
                    onTogglePanel={() => setAgentMemoryPanelOpen((prev) => !prev)}
                    memories={agentMemories}
                    memoriesLoading={agentMemoriesLoading}
                    actionRunning={agentActionRunning}
                    isTauri={isTauriRuntime()}
                    onDeleteMemory={handleDeleteAgentMemory}
                    composerOpen={agentMemoryComposerOpen}
                    onToggleComposer={() => setAgentMemoryComposerOpen((prev) => !prev)}
                    memoryKeyInput={agentMemoryKeyInput}
                    setMemoryKeyInput={setAgentMemoryKeyInput}
                    memoryValueInput={agentMemoryValueInput}
                    setMemoryValueInput={setAgentMemoryValueInput}
                    onUpsertMemory={handleUpsertAgentMemory}
                  />
                  <AgentSkillsPanel
                    skills={agentSkills}
                    skillsLoading={agentSkillsLoading}
                    activeSkillKey={agentActiveSkillKey}
                    setActiveSkillKey={setAgentActiveSkillKey}
                    onUpsertSkill={handleUpsertAgentSkill}
                    onDeleteSkill={handleDeleteAgentSkill}
                  />
                </div>
              ) : null}
            </div>
            <div className="agent-studio__chat-thread">
              {agentRunsLoading ? (
                <p className="agent-studio__empty">加载历史 run...</p>
              ) : agentChatMessages.length === 0 ? (
                <p className="agent-studio__empty">暂无历史，输入主题后开始第一轮创作。</p>
              ) : (
                agentChatMessages.map((message) => {
                  const selected = message.run_id === agentSelectedRunId;
                  return (
                    <button
                      key={message.id}
                      type="button"
                      className={`agent-studio__msg-row agent-studio__msg-row--${message.role}${selected ? " agent-studio__msg-row--selected" : ""}`}
                      onClick={() => handleSelectAgentRunFromChat(message.run_id)}
                    >
                      <span className="agent-studio__msg-meta">
                        <strong className="agent-studio__msg-role">{message.role === "user" ? "You" : "Agent"}</strong>
                        <time className="agent-studio__msg-time" dateTime={message.created_at}>
                          {formatLintCheckedAt(message.created_at)}
                        </time>
                      </span>
                      <p className="agent-studio__msg-body">{message.content}</p>
                      {message.role === "agent" && message.status ? (
                        <span className="agent-studio__msg-status">
                          {formatAgentRunStatusLabel(message.status)}
                        </span>
                      ) : null}
                    </button>
                  );
                })
              )}
            </div>
            <div className="agent-studio__composer">
              <input
                id="agent-topic-input"
                className="dev-panel__input"
                type="text"
                value={agentTopicInput}
                placeholder={'输入主题或指令，例如：写一篇“管网压力模型”Wiki'}
                onChange={(event) => setAgentTopicInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void handleAgentChatSend();
                  }
                }}
              />
              <button
                type="button"
                className="dev-panel__button dev-panel__button--accent"
                disabled={agentActionRunning || !agentTopicInput.trim() || !isTauriRuntime()}
                onClick={() => { void handleAgentChatSend(); }}
              >
                {agentActionRunning ? "生成中..." : "发送"}
              </button>
            </div>
            <div className="agent-studio__composer-tools">
              <label
                className="agent-studio__research-toggle"
                title="开启后读取 wiki 页面正文（前 400 字）作为上下文，生成质量更高但速度略慢"
              >
                <input
                  type="checkbox"
                  checked={agentResearchMode}
                  onChange={(e) => setAgentResearchMode(e.target.checked)}
                />
                检索增强
              </label>
              <label
                className="agent-studio__research-toggle"
                title="开启后先对主题做一次 Ask 问答，将知识库现有答案注入草稿上下文（需要 LLM 可用，速度较慢）"
              >
                <input
                  type="checkbox"
                  checked={agentAskFirst}
                  onChange={(e) => setAgentAskFirst(e.target.checked)}
                />
                Ask 联动
              </label>
              <button
                type="button"
                className="dev-panel__button"
                disabled={agentRunsLoading || !isTauriRuntime()}
                onClick={() => {
                  void loadAgentRunsData(agentSelectedRunId);
                  if (agentSelectedRunId != null) {
                    void loadAgentDraftsData(agentSelectedRunId, agentSelectedDraftId);
                    void loadAgentRunEventsData(agentSelectedRunId);
                  }
                }}
              >
                刷新
              </button>
              <button
                type="button"
                className="dev-panel__button"
                disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                onClick={() => { void handleGenerateAgentDraft(); }}
              >
                基于当前 Run 重写
              </button>
              <button
                type="button"
                className="dev-panel__button agent-studio__debug-btn"
                onClick={() => setAgentDebugPanelOpen((prev) => !prev)}
              >
                {agentDebugPanelOpen ? "收起调试" : "调试"}
              </button>
            </div>
          </section>
          {/* 左右面板拖拽分割条 */}
          <div
            className="split-handle"
            onMouseDown={(e) => {
              e.preventDefault();
              const container = agentLayoutRef.current;
              if (!container) return;
              agentDragRef.current = {
                active: true,
                startX: e.clientX,
                startRatio: agentLeftRatio,
                containerW: container.getBoundingClientRect().width,
              };
              document.body.style.cursor = "col-resize";
              document.body.style.userSelect = "none";
              document.body.classList.add("split-dragging");
            }}
          />
          <section className="agent-studio__right">
            <div className="agent-studio__draft-head">
              <h3 className="agent-studio__title">
                {selectedAgentDraft
                  ? `${selectedAgentDraft.title || "未命名草稿"}（#${selectedAgentDraft.id}）`
                  : "草稿预览"}
              </h3>
              {selectedAgentDraft ? (
                <span className="agent-studio__item-meta">
                  {selectedAgentDraft.status}
                  {" · "}
                  {formatLintCheckedAt(selectedAgentDraft.updated_at || selectedAgentDraft.created_at)}
                  {agentDraftAppliedSkillKey ? ` · skill:${agentDraftAppliedSkillKey}` : ""}
                </span>
              ) : null}
            </div>
            <div className="agent-studio__main-tabs">
              <button
                type="button"
                className={`agent-studio__main-tab${agentRightTab === "task" ? " agent-studio__main-tab--active" : ""}`}
                onClick={() => setAgentRightTab("task")}
              >
                任务
                {agentHasPendingApproval ? <span className="agent-studio__main-tab-dot">审批中</span> : null}
              </button>
              <button
                type="button"
                className={`agent-studio__main-tab${agentRightTab === "draft" ? " agent-studio__main-tab--active" : ""}`}
                onClick={() => setAgentRightTab("draft")}
              >
                草稿
              </button>
              <button
                type="button"
                className={`agent-studio__main-tab${agentRightTab === "tools" ? " agent-studio__main-tab--active" : ""}${agentToolsNeedsAttention ? " agent-studio__main-tab--attention" : ""}`}
                onClick={() => setAgentRightTab("tools")}
              >
                工具
                <span className="agent-studio__main-tab-meta">
                  {agentShellRunning ? "运行中" : `历史 ${agentShellHistory.length} 条`}
                </span>
              </button>
            </div>
            <div className="agent-studio__right-main">
              {agentRightTab === "task" ? (
                <section className="agent-studio__task-mode">
                  <h3 className="agent-studio__title">任务模式（Beta）</h3>
                  <div className="agent-studio__task-presets">
                    <button
                      type="button"
                      className="dev-panel__button"
                      disabled={agentTaskRunning}
                      onClick={() => {
                        setAgentTaskInstruction(
                          `请新建页面 wiki/agent-e2e-write.md，内容包含标题“Agent E2E Write”，并在执行前说明为什么要调用 write_wiki。`,
                        );
                      }}
                    >
                      填充：写入审批验证
                    </button>
                    <button
                      type="button"
                      className="dev-panel__button"
                      disabled={agentTaskRunning}
                      onClick={() => {
                        setAgentTaskInstruction(
                          `请编辑页面 wiki/agent-e2e-write.md，把“Agent E2E Write”替换为“Agent E2E Edited”，并优先使用 edit_wiki。`,
                        );
                      }}
                    >
                      填充：编辑审批验证
                    </button>
                  </div>
                  <textarea
                    className="dev-panel__input agent-studio__task-mode-input"
                    rows={3}
                    placeholder="输入任务指令，例如：梳理当前 run 的草稿风险并给出下一步执行计划。"
                    value={agentTaskInstruction}
                    onChange={(event) => setAgentTaskInstruction(event.target.value)}
                    disabled={agentTaskRunning}
                  />
                  <div className="agent-studio__task-mode-actions">
                    <label className="agent-studio__task-mode-budget">
                      预算轮次
                      <input
                        type="number"
                        min={1}
                        max={8}
                        className="dev-panel__input"
                        value={agentTaskMaxIterations}
                        onChange={(event) => {
                          const next = Number(event.target.value);
                          if (Number.isFinite(next)) {
                            setAgentTaskMaxIterations(Math.min(8, Math.max(1, Math.floor(next))));
                          }
                        }}
                        disabled={agentTaskRunning}
                      />
                    </label>
                    <button
                      type="button"
                      className="dev-panel__button"
                      disabled={
                        agentTaskRunning
                        || !agentTaskInstruction.trim()
                        || agentSelectedRunId == null
                        || !isTauriRuntime()
                      }
                      onClick={() => { void handleRunAgentTask(); }}
                    >
                      {agentTaskRunning ? "执行中..." : "运行任务"}
                    </button>
                    <button
                      type="button"
                      className="dev-panel__button"
                      disabled={
                        agentTaskRunning
                        || agentSelectedRunId == null
                        || !isTauriRuntime()
                        || (agentEvents.length === 0 && !agentTaskResult.trim())
                      }
                      onClick={() => { void handleContinueAgentTask(); }}
                    >
                      继续任务
                    </button>
                  </div>
                  {agentTaskResult ? (
                    <pre className="agent-studio__task-mode-result">{agentTaskResult}</pre>
                  ) : (
                    <p className="agent-studio__empty">任务结果将在此显示。</p>
                  )}
                  {agentSelectedRunId != null && agentExecTimeline.length > 0 && (
                    <div className="agent-studio__exec-log">
                      <div className="agent-studio__exec-log-head">
                        <span>工具时间线</span>
                        <span className="agent-studio__exec-log-count">{agentExecTimeline.length} 条</span>
                      </div>
                      <ul className="agent-studio__exec-log-list">
                        {agentExecTimeline.map((item) => (
                          <li key={item.key} className={`agent-studio__exec-log-row agent-studio__exec-log-row--${item.level}`}>
                            <div className="agent-studio__exec-log-row-main">
                              <span className="agent-studio__exec-log-title">{item.title}</span>
                              <span className="agent-studio__exec-log-side">
                                {item.durationMs != null ? `${item.durationMs}ms` : "—"}
                                {" · "}
                                {formatLintCheckedAt(item.createdAt)}
                              </span>
                            </div>
                            <span className="agent-studio__exec-log-msg">{item.summary}</span>
                            {item.detail ? (
                              <details className="agent-studio__exec-log-detail">
                                <summary>展开详情</summary>
                                <pre>{item.detail}</pre>
                              </details>
                            ) : null}
                          </li>
                        ))}
                      </ul>
                    </div>
                  )}
                  {agentHasPendingApproval ? (
                    <div className="agent-studio__approval-bar">
                      <span className="agent-studio__approval-label">
                        ⏸ Agent 请求修改 Wiki（写入 / 编辑），等待审批
                      </span>
                      <div className="agent-studio__approval-actions">
                        <button
                          className="agent-studio__approval-approve"
                          disabled={agentActionRunning}
                          onClick={() => void handleApproveAgentWrite()}
                        >
                          ✅ 批准
                        </button>
                        <button
                          className="agent-studio__approval-reject"
                          disabled={agentActionRunning}
                          onClick={() => void handleRejectAgentWrite()}
                        >
                          🚫 拒绝
                        </button>
                      </div>
                    </div>
                  ) : null}
                </section>
              ) : null}
              {agentRightTab === "draft" ? (
                <>
                  {selectedAgentDraft != null &&
                  !["approved", "applied"].includes(String(selectedAgentDraft.status).toLowerCase()) ? (
                    <div className="agent-studio__rewrite-bar">
                      <input
                        type="text"
                        className="dev-panel__input"
                        placeholder="批注（如：语气更专业、增加原理推导）"
                        value={agentRewriteComment}
                        onChange={(e) => setAgentRewriteComment(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") { e.preventDefault(); void handleRewriteAgentDraft(); }
                        }}
                      />
                      <button
                        type="button"
                        className="dev-panel__button"
                        disabled={agentActionRunning || !agentRewriteComment.trim() || !isTauriRuntime()}
                        onClick={() => void handleRewriteAgentDraft()}
                      >
                        基于批注重写
                      </button>
                    </div>
                  ) : null}
                  <div className="agent-studio__review-tabs">
                    <button
                      type="button"
                      className={`agent-studio__review-tab${agentReviewTab === "draft" ? " agent-studio__review-tab--active" : ""}`}
                      onClick={() => setAgentReviewTab("draft")}
                    >
                      Draft
                    </button>
                    <button
                      type="button"
                      className={`agent-studio__review-tab${agentReviewTab === "diff" ? " agent-studio__review-tab--active" : ""}`}
                      onClick={() => setAgentReviewTab("diff")}
                      disabled={selectedAgentDraft == null}
                    >
                      Diff
                    </button>
                    <button
                      type="button"
                      className={`agent-studio__review-tab${agentReviewTab === "citations" ? " agent-studio__review-tab--active" : ""}`}
                      onClick={() => setAgentReviewTab("citations")}
                      disabled={selectedAgentDraft == null}
                    >
                      Citations
                    </button>
                  </div>
                  {agentReviewTab === "draft" && selectedAgentDraft ? (
                    <div className="agent-studio__flow-controls">
                      <div className="agent-studio__flow-meta">
                        <span className="agent-studio__flow-badge">
                          {agentFlowMode === "playing"
                            ? "流式生成中"
                            : agentFlowMode === "paused"
                              ? "已暂停"
                              : agentFlowMode === "done"
                                ? "已完成"
                                : "待开始"}
                        </span>
                        <span className="agent-studio__flow-progress-text">{agentFlowProgress}%</span>
                      </div>
                      <div className="agent-studio__flow-progress">
                        <div
                          className="agent-studio__flow-progress-fill"
                          style={{ width: `${agentFlowProgress}%` }}
                        />
                      </div>
                      <div className="agent-studio__flow-buttons">
                        <button
                          type="button"
                          className="dev-panel__button"
                          disabled={agentFlowMode !== "playing"}
                          onClick={handlePauseAgentFlow}
                        >
                          暂停
                        </button>
                        <button
                          type="button"
                          className="dev-panel__button"
                          disabled={agentFlowMode !== "paused"}
                          onClick={handleResumeAgentFlow}
                        >
                          继续
                        </button>
                        <button
                          type="button"
                          className="dev-panel__button"
                          disabled={agentFlowMode === "done"}
                          onClick={handleCompleteAgentFlow}
                        >
                          完成
                        </button>
                        <button
                          type="button"
                          className="dev-panel__button"
                          onClick={handleReplayAgentFlow}
                        >
                          重播
                        </button>
                      </div>
                      {agentFlowOutline.length > 0 ? (
                        <p className="agent-studio__flow-outline">
                          提纲：{agentFlowOutline.join(" / ")}
                        </p>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="agent-studio__draft-pane">
                    {selectedAgentDraft == null ? (
                      agentActionRunning ? (
                        <div className="agent-studio__draft-skeleton" aria-live="polite">
                          <span />
                          <span />
                          <span />
                          <span />
                        </div>
                      ) : (
                        <p className="agent-studio__empty">请选择左侧 run，右侧将显示对应草稿内容。</p>
                      )
                    ) : agentReviewTab === "diff" ? (
                      agentDraftConflictLoading ? (
                        <p className="agent-studio__empty">正在加载差异基线...</p>
                      ) : agentDraftConflictPreview?.conflict && agentDraftDiffRows.length > 0 ? (
                        <div className="wiki-history-diff__rows">
                          {agentDraftDiffRows.map((row, index) => (
                            <div
                              key={`agent-diff-${index}-${row.kind}-${row.oldLineNumber ?? "na"}-${row.newLineNumber ?? "na"}`}
                              className={`wiki-history-diff__row wiki-history-diff__row--${row.kind}`}
                            >
                              <span className="wiki-history-diff__sign">
                                {row.kind === "added" ? "+" : row.kind === "removed" ? "-" : " "}
                              </span>
                              <span className="wiki-history-diff__line-no">
                                {row.kind === "added" ? row.newLineNumber : row.oldLineNumber}
                              </span>
                              <code>{row.line || " "}</code>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <p className="agent-studio__empty">
                          当前没有可用差异基线（仅在同名页面存在时展示 Diff）。
                        </p>
                      )
                    ) : agentReviewTab === "citations" ? (
                      agentDraftCitations.length > 0 ? (
                        <ul className="agent-studio__citation-list">
                          {agentDraftCitations.map((citation) => (
                            <li key={citation}>
                              <code>[[{citation}]]</code>
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <p className="agent-studio__empty">当前草稿未检测到 `[[wiki-link]]` 引用。</p>
                      )
                    ) : agentDraftDisplayContent.trim() ? (
                      <div
                        className="agent-studio__draft-markdown wiki-markdown"
                        // biome-ignore lint/security/noDangerouslySetInnerHtml: sanitized by DOMPurify
                        dangerouslySetInnerHTML={{
                          __html: DOMPurify.sanitize(
                            marked.parse(agentDraftDisplayContent.trim(), {
                              gfm: true,
                              breaks: false,
                            }) as string,
                          ),
                        }}
                      />
                    ) : (
                      <p className="agent-studio__empty">（该 draft 暂无内容）</p>
                    )}
                  </div>
                  <div className="agent-studio__draft-actions">
                    <button
                      type="button"
                      className="dev-panel__button dev-panel__button--primary"
                      disabled={
                        agentActionRunning
                        || selectedAgentDraft == null
                        || ["approved", "applied"].includes(
                          String(selectedAgentDraft?.status ?? "").toLowerCase(),
                        )
                        || !isTauriRuntime()
                      }
                      onClick={() => { void handleApproveAgentDraft(); }}
                    >
                      写入 Wiki
                    </button>
                    <button
                      type="button"
                      className="dev-panel__button"
                      disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                      onClick={() => { void handleGenerateAgentDraft(); }}
                    >
                      重写
                    </button>
                    <button
                      type="button"
                      className="dev-panel__button"
                      disabled={agentActionRunning || selectedAgentDraft == null}
                      onClick={handleDiscardAgentDraftSelection}
                    >
                      丢弃
                    </button>
                  </div>
                </>
              ) : null}
              {agentRightTab === "tools" ? (
                <AgentToolsPane
                  agentShellTheme={agentShellTheme}
                  setAgentShellTheme={setAgentShellTheme}
                  agentShellSession={agentShellSession}
                  agentShellRunning={agentShellRunning}
                  agentShellHistory={agentShellHistory}
                  handleClearShellHistory={handleClearShellHistory}
                  shellPolicyProfiles={shellPolicyProfiles}
                  agentShellPolicySaving={agentShellPolicySaving}
                  agentShellPolicyConfig={agentShellPolicyConfig}
                  handleApplyAndSaveShellPolicyProfile={handleApplyAndSaveShellPolicyProfile}
                  agentShellQuickCommands={agentShellQuickCommands}
                  handleApplyShellCommand={handleApplyShellCommand}
                  agentShellHistoryRef={agentShellHistoryRef}
                  agentShellAutoFollowRef={agentShellAutoFollowRef}
                  handleCopyShellOutput={handleCopyShellOutput}
                  agentShellInputRef={agentShellInputRef}
                  agentShellCmd={agentShellCmd}
                  setAgentShellCmd={setAgentShellCmd}
                  setAgentShellHistoryCursor={setAgentShellHistoryCursor}
                  handleShellHistoryNav={handleShellHistoryNav}
                  handleRunShell={handleRunShell}
                  handleApproveShellCommand={handleApproveShellCommand}
                />
              ) : null}
            </div>
          </section>
        </div>
        {agentDebugPanelOpen ? (
          <div className="agent-studio__debug">
            <div className="agent-studio__create">
              <label className="dev-panel__label" htmlFor="agent-event-level">事件级别</label>
              <select
                id="agent-event-level"
                className="dev-panel__input"
                value={agentEventLevel}
                onChange={(event) =>
                  setAgentEventLevel((event.target.value as AgentRunEventLevel) || "info")
                }
              >
                {agentEventLevelOptions.map((level) => (
                  <option key={level} value={level}>{level}</option>
                ))}
              </select>
              <input
                className="dev-panel__input"
                type="text"
                value={agentEventMessage}
                placeholder="事件内容（例如：完成初始检索）"
                onChange={(event) => setAgentEventMessage(event.target.value)}
              />
              <button
                type="button"
                className="dev-panel__button"
                disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                onClick={() => { void handleAppendAgentEvent(); }}
              >
                追加事件
              </button>
            </div>
            <div className="agent-studio__actions">
              <button
                type="button"
                className="dev-panel__button"
                disabled={agentActionRunning || !agentTopicInput.trim() || !isTauriRuntime()}
                onClick={() => { void handleCreateAgentRun(); }}
              >
                仅创建 Run
              </button>
              <select
                className="dev-panel__input"
                value={agentCompleteStatus}
                onChange={(event) =>
                  setAgentCompleteStatus((event.target.value as AgentRunStatus) || "applied")
                }
              >
                {agentCompleteStatusOptions.map((status) => (
                  <option key={status} value={status}>{status}</option>
                ))}
              </select>
              <button
                type="button"
                className="dev-panel__button"
                disabled={agentActionRunning || agentSelectedRunId == null || !isTauriRuntime()}
                onClick={() => { void handleCompleteAgentRun(); }}
              >
                结束 Run
              </button>
            </div>
            <section className="agent-studio__debug-events">
              <h3 className="agent-studio__title">
                Events{agentSelectedRunId != null ? `（Run #${agentSelectedRunId}）` : ""}
              </h3>
              {agentSelectedRunId == null ? (
                <p className="agent-studio__empty">请选择一个 run 查看事件。</p>
              ) : agentEventsLoading ? (
                <p className="agent-studio__empty">加载中...</p>
              ) : agentEvents.length === 0 ? (
                <p className="agent-studio__empty">暂无事件。</p>
              ) : (
                <ul className="agent-studio__list">
                  {agentEvents.map((event) => (
                    <li key={event.id} className="agent-studio__event-row">
                      <span className={`agent-studio__event-level agent-studio__event-level--${String(event.level).toLowerCase()}`}>
                        {event.level}
                      </span>
                      <span className="agent-studio__event-message">{event.message}</span>
                      <time className="agent-studio__event-time" dateTime={event.created_at}>
                        {formatLintCheckedAt(event.created_at)}
                      </time>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        ) : null}
      </section>
      {/* 审批前确认弹窗（H1） */}
      {agentApproveConfirm ? (
        <div
          className="agent-draft-confirm-overlay"
          role="dialog"
          aria-modal="true"
          aria-label="确认写盘"
          onClick={() => setAgentApproveConfirm(null)}
        >
          <div
            className="agent-draft-confirm-dialog"
            onClick={(e) => e.stopPropagation()}
          >
            <h3 className="agent-draft-confirm-dialog__title">确认写盘</h3>
            <p className="agent-draft-confirm-dialog__body">
              即将将 Draft「<strong>{agentApproveConfirm.title}</strong>
              」写入 Wiki。
              {agentApproveConfirm.conflict && (
                <span className="agent-draft-confirm-dialog__warn">
                  {" "}
                  ⚠ 同名页面已存在，写盘将覆盖现有内容。
                </span>
              )}
              {!agentApproveConfirm.conflict && " 此操作不可撤销，确认继续？"}
            </p>
            {agentApproveConfirm.conflict && agentApproveConfirm.existing_preview && (
              <details className="agent-draft-confirm-dialog__existing">
                <summary>查看现有页面预览（前 300 字）</summary>
                <pre>{agentApproveConfirm.existing_preview}</pre>
              </details>
            )}
            <div className="agent-draft-confirm-dialog__actions">
              <button
                type="button"
                className="dev-panel__button dev-panel__button--primary"
                disabled={agentActionRunning}
                onClick={() => void doApproveAgentDraft()}
              >
                确认写盘
              </button>
              <button
                type="button"
                className="dev-panel__button"
                onClick={() => setAgentApproveConfirm(null)}
              >
                取消
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
