export type ModeId = "hybrid" | "strict-local";

export interface ModeOption {
  id: ModeId;
  name: string;
  description: string;
  badge: string;
}

export type ModuleId =
  | "inbox"
  | "wiki"
  | "ask"
  | "lint"
  | "graph"
  | "settings"
  | "queue"
  | "research"
  | "stats"
  | "agent";

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

export interface WikiTemplate {
  id: string;
  name: string;
  description: string;
  icon: string;
  schema: string;
  purpose: string;
  extraDirs: string[];
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

export interface IngestPreview {
  preview_id: string;
  source_path: string;
  summary: string;
  entities: string[];
  updated_pages: string[];
}

export interface PageQuickLint {
  wiki_path: string;
  /** [[wiki-links]] 中没有对应页面的断链 */
  broken_links: string[];
  /** frontmatter 实体中没有对应 wiki 页面的缺页实体 */
  missing_entity_pages: string[];
  issues_count: number;
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

export interface QuerySearchRouteDebug {
  route: string;
  candidate_count: number;
  top_candidates: string[];
  contributed_paths: string[];
}

export interface QuerySearchDebug {
  strategy: string;
  rrf_k?: number | null;
  fused_top_paths: string[];
  routes: QuerySearchRouteDebug[];
}

export interface QueryAnswerResult {
  question: string;
  answer: string;
  search_strategy?: string | null;
  answer_strategy?: QueryAnswerStrategy | LegacyQueryAnswerStrategy | null;
  search_debug?: QuerySearchDebug | null;
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

export interface WikiPageHistorySummary {
  id: number;
  path: string;
  title: string;
  content_hash: string;
  checksum: string;
  created_at: string;
}

export interface WikiPageHistoryEntry extends WikiPageHistorySummary {
  content: string;
}

export type AgentRunStatus = "queued" | "running" | "reviewing" | "applied" | "failed";
export type AgentRunEventLevel = "info" | "warn" | "error";

export interface AgentRunItem {
  id: number;
  topic: string;
  status: AgentRunStatus | string;
  created_at: string;
  updated_at: string;
  completed_at?: string | null;
}

export interface AgentRunEventItem {
  id: number;
  run_id: number;
  level: AgentRunEventLevel | string;
  message: string;
  created_at: string;
}

export type AgentDraftStatus = "draft" | "approved" | "applied" | "failed" | string;

export interface AgentDraftItem {
  id: number;
  run_id: number;
  title: string;
  content: string;
  status: AgentDraftStatus;
  created_at: string;
  updated_at: string;
}

export type AgentChatRole = "user" | "agent";

export interface AgentChatMessage {
  id: string;
  run_id: number;
  role: AgentChatRole;
  content: string;
  created_at: string;
  status?: string;
  draft_id?: number | null;
}

/** Agent 记忆项（H2：记忆增强） */
export interface AgentMemoryItem {
  id: number;
  run_id: number | null;
  memory_key: string;
  memory_value: string;
  created_at: string;
  updated_at: string;
}

/** Agent 技能模板项（H3） */
export interface AgentSkillItem {
  id: number;
  skill_key: string;
  prompt_template: string;
  version: number;
  created_at: string;
  updated_at: string;
}

/** 审批前冲突检测结果（H1 确认弹窗用） */
export interface AgentDraftConflictInfo {
  draft_id: number;
  title: string;
  conflict: boolean;
  existing_path: string | null;
  existing_preview: string | null;
}

export interface AskHistoryItem {
  id: number;
  question: string;
  created_at: string;
}

export interface AskSessionItem {
  session_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  turn_count: number;
  last_turn_role?: string | null;
  last_turn_content?: string | null;
}

/** Ask 会话单轮记录 */
export interface AskTurn {
  role: "user" | "assistant";
  content: string;
}

export interface AskSessionTurnItem {
  id: number;
  session_id: string;
  role: "user" | "assistant";
  content: string;
  created_at: string;
  citations?: QueryCitation[];
  meta?: {
    mode: BackendAppMode;
    search_strategy?: string | null;
    answer_strategy?: string | null;
    top_k?: number | null;
    matched_pages?: number | null;
    search_debug?: QuerySearchDebug | null;
  } | null;
}

export interface AskSessionSearchHitItem {
  session_id: string;
  session_title: string;
  turn_id: number;
  role: "user" | "assistant" | string;
  snippet: string;
  created_at: string;
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
  /** 当 code 为 BROKEN_WIKILINK 时，指向不存在页面的标题 */
  target_page?: string | null;
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
  /** 本地 Ollama 模型名（LLM 用，空字符串时由后端使用默认值） */
  ollama_model: string;
  /** 本地 Ollama Base URL（LLM 用，空字符串时默认 http://localhost:11434） */
  ollama_base_url: string;
  /** Embedding 专用 Ollama 模型（默认 nomic-embed-text:latest） */
  embed_ollama_model: string;
  /** Embedding 专用 Ollama Base URL（空字符串时与 ollama_base_url 相同） */
  embed_ollama_base_url: string;
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

/** 子图遍历方向：双向 / 仅出边 / 仅入边 */
export type GraphTraversalDirection = "both" | "out" | "in";

/** 知识子图元信息（后端用于返回统计与裁剪状态） */
export interface KnowledgeSubgraphMeta {
  center_page_path: string;
  hop: number;
  direction: GraphTraversalDirection;
  truncated: boolean;
  node_count: number;
  link_count: number;
}

/** 知识子图返回结构（节点/边 + 元信息） */
export interface KnowledgeSubgraphData {
  nodes: KnowledgeGraphNode[];
  links: KnowledgeGraphLink[];
  meta: KnowledgeSubgraphMeta;
}

/** 获取知识子图请求参数（前端契约） */
export interface KnowledgeSubgraphRequestParams {
  centerPagePath: string;
  hop?: number;
  direction?: GraphTraversalDirection;
  limitNodes?: number;
  limitLinks?: number;
}

export interface OutboxEventItem {
  id: number;
  event_type: string;
  payload_json: string;
  created_at: string;
  processed_at?: string | null;
  consumer_tag?: string | null;
}

export type IngestQueueStatus = "queued" | "running" | "done" | "failed" | "cancelled";

export type ResearchTaskStatus =
  | "queued"
  | "decomposing"
  | "searching"
  | "synthesizing"
  | "saving"
  | "done"
  | "failed"
  | "cancelled";

export interface ResearchTaskItem {
  id: number;
  topic: string;
  status: ResearchTaskStatus;
  sub_queries: string[];
  web_results_count: number;
  depth: number;
  breadth: number;
  saved_path: string | null;
  error: string | null;
  created_at: string;
  updated_at: string;
}

export interface SearchConfig {
  search_provider: "tavily" | "searxng" | "none";
  tavily_api_key: string;
  searxng_url: string;
  breadth: number;
  depth: number;
}

export interface IngestQueueItem {
  id: number;
  source_type: string;   // "file" | "url" | "markdown"
  source_path: string;
  status: IngestQueueStatus;
  error?: string;
  created_at: string;
  updated_at: string;
}

export interface CitedPageStat {
  path: string;
  title: string;
  citation_count: number;
}

export interface IngestSourceCount {
  source_type: string;
  count: number;
}

export interface VaultStats {
  total_pages: number;
  pages_last_7_days: number;
  pages_last_30_days: number;
  orphan_pages: number;
  total_citations: number;
  top_cited_pages: CitedPageStat[];
  ingest_source_counts: IngestSourceCount[];
}

export interface NewPageResult {
  wiki_path: string;
  title: string;
  content_preview: string;
}

export interface ShellResult {
  command: string;
  stdout: string;
  stderr: string;
  exit_code: number;
  blocked: boolean;
  blocked_reason: string | null;
  policy_action: string;
  policy_decision: string;
  executor: string;
}

export interface ShellHistoryEntry {
  id: number;
  command: string;
  result: ShellResult;
  ts: number;
}
