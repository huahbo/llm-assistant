export type ModeId = "hybrid" | "strict-local";

export interface ModeOption {
  id: ModeId;
  name: string;
  description: string;
  badge: string;
}

export type ModuleId = "inbox" | "wiki" | "ask" | "lint" | "settings";

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

export interface WikiPageItem {
  title: string;
  path: string;
  display_path?: string | null;
  displayPath?: string | null;
  summary: string;
  updated_at: string;
}

export interface WikiPageDetail {
  title: string;
  path: string;
  display_path?: string | null;
  displayPath?: string | null;
  content: string;
  updated_at: string;
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

export interface LogEntry {
  id: number;
  level: LogLevel;
  message: string;
  created_at: string;
}
