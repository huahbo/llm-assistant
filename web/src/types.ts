export type ModeId = "hybrid" | "strict-local";

export interface ModeOption {
  id: ModeId;
  name: string;
  description: string;
  badge: string;
}

export type ModuleId = "inbox" | "wiki" | "ask" | "lint" | "graph" | "settings";

export interface ModuleItem {
  id: ModuleId;
  name: string;
  description: string;
}

export type BackendAppMode = "Hybrid" | "StrictLocal";

export type LogLevel = "Info" | "Warn" | "Error";

export interface LlmStatus {
  available: boolean;
  model: string;
  address: string;
  message: string;
}

export interface AppOverview {
  app_name: string;
  mode: BackendAppMode;
  vault_path: string;
  recent_log_count: number;
  pending_tasks: number;
  supported_modes: BackendAppMode[];
}

export interface ModeChangeResult {
  previous_mode: BackendAppMode;
  current_mode: BackendAppMode;
  strict_local_enabled: boolean;
}

export interface VaultInitResult {
  vault_path: string;
  created_paths: string[];
  message: string;
}

export interface IngestResult {
  source_path: string;
  raw_path: string;
  wiki_path: string;
  message: string;
  /** LLM 提取的关键实体列表（P1 复利机制） */
  entities?: string[];
  /** 被注入反向链接的相关 Wiki 页面路径（P1 复利机制） */
  updated_pages?: string[];
}

export interface DefaultPaths {
  vault_path: string;
  ingest_source_path: string;
}

export interface QueryCitation {
  page_path: string;
  display_path?: string | null;
  displayPath?: string | null;
  score: number;
  excerpt: string;
}

export type QueryAnswerStrategy = "llm" | "rule";
export type LegacyQueryAnswerStrategy = "llm_synthesis" | "rule_fallback";

export interface QueryAnswerResult {
  question: string;
  answer: string;
  search_strategy?: string | null;
  answer_strategy?: QueryAnswerStrategy | LegacyQueryAnswerStrategy | null;
  citations: QueryCitation[];
  matched_pages: string[];
  mode: BackendAppMode;
  checked_at: string;
}

export interface QueryAskOptions {
  top_k?: number;
}

export interface QuerySettings {
  top_k: number;
  min_top_k: number;
  max_top_k: number;
}

export interface SaveQueryAnswerInput {
  question: string;
  answer: string;
  citations: QueryCitation[];
  title?: string;
}

export interface SaveQueryAnswerResult {
  wiki_path: string;
  page_title: string;
  message: string;
}

export interface SaveWikiPageResult {
  path: string;
  message: string;
}

export interface DeleteWikiPageResult {
  path: string;
  message: string;
}

export interface RenameWikiPageResult {
  new_path: string;
  message: string;
}

export interface AskHistoryItem {
  id: number;
  question: string;
  created_at: string;
}

/** Ask 会话单轮记录 */
export interface AskTurn {
  role: "user" | "assistant";
  content: string;
}

export interface WikiPageItem {
  title: string;
  path: string;
  display_path?: string | null;
  displayPath?: string | null;
  summary: string;
  updated_at: string;
  score?: number;
  tags?: string[];
}

export interface WikiPageDetail {
  title: string;
  path: string;
  display_path?: string | null;
  displayPath?: string | null;
  frontmatter?: WikiPageFrontmatter | null;
  content: string;
  updated_at: string;
}

export interface WikiPageFrontmatter {
  title?: string | null;
  source?: string | null;
  raw?: string | null;
  imported_at?: string | null;
  entities?: string[];
  stale?: boolean | null;
}

export interface WikiPageCitation {
  cited_page_path: string;
  display_path?: string | null;
  displayPath?: string | null;
  cited_page_display_path?: string | null;
  score: number;
  excerpt: string;
  target_exists: boolean;
}

export interface LintIssue {
  code: string;
  severity: string;
  message: string;
  path?: string | null;
  suggestion: string;
}

export interface LintSeverityStats {
  error: number;
  warning: number;
  info: number;
}

export interface LintReport {
  mode: BackendAppMode;
  checked_at: string;
  summary: string;
  severity_stats?: LintSeverityStats | null;
  issues: LintIssue[];
}

export interface LintPatchPreviewItem {
  issue_code: string;
  title: string;
  proposed_action: string;
  patch_preview: string;
  path?: string | null;
}

export interface LintPatchApplyResult {
  issue_code?: string;
  path?: string | null;
  applied?: boolean;
  message?: string | null;
  touched_paths?: string[];
}

export interface LintPatchBatchItemInput {
  issue_code: string;
  path?: string | null;
}

export interface LintPatchBatchItemResult {
  issue_code?: string;
  path?: string | null;
  applied?: boolean;
  skipped?: boolean;
  status?: string;
  message?: string | null;
  touched_paths?: string[];
  error?: string | null;
}

export interface LintPatchBatchResult {
  summary?: string | null;
  success_count: number;
  failure_count: number;
  skipped_count: number;
  total_count: number;
  items: LintPatchBatchItemResult[];
}

export interface LintPatchEvent {
  issue_code: string;
  path?: string | null;
  applied: boolean;
  message: string;
  created_at: string;
}

export interface LogEntry {
  id: number;
  level: LogLevel;
  message: string;
  created_at: string;
}

/** 长时间操作的进度事件载荷（ingest_progress / query_progress） */
export interface ProgressPayload {
  step: string;
  message: string;
}

/** LLM Provider 配置（Settings 页面读写） */
export interface LlmProviderConfig {
  /** 云端 API Key，空字符串表示未配置 */
  cloud_api_key: string;
  /** 云端 Base URL，空字符串时由后端使用默认值 */
  cloud_base_url: string;
  /** 云端模型名，空字符串时由后端使用默认值 */
  cloud_model: string;
  /** 云端 Provider 显示名，例如 OpenAI / DeepSeek */
  cloud_provider_name: string;
  /** 当前活跃的 provider 类型，"ollama" 或 "cloud" */
  active_provider: string;
}

export interface KnowledgeGraphNode {
  id: string;       // 页面绝对路径
  label: string;    // 页面标题
  group: string;    // 分组标签（第一个 entity 或空字符串）
  // react-force-graph-2d 会在运行时追加 x/y/vx/vy 等字段
  x?: number;
  y?: number;
}

export interface KnowledgeGraphLink {
  source: string | KnowledgeGraphNode;
  target: string | KnowledgeGraphNode;
}

export interface KnowledgeGraphData {
  nodes: KnowledgeGraphNode[];
  links: KnowledgeGraphLink[];
}
