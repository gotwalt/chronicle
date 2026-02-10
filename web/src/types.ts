// --- Tree ---

export interface TreeFile {
  path: string;
  annotation_count: number;
}

export interface TreeResponse {
  files: TreeFile[];
}

// --- Status ---

export interface StatusOutput {
  total_annotations: number;
  recent_commits: number;
  recent_annotated: number;
  coverage_pct: number;
  unannotated_commits: string[];
}

// --- File View (combined endpoint) ---

export interface FileViewResponse {
  path: string;
  content: string;
  language: string;
  lookup: LookupOutput;
  summary: SummaryOutput;
}

// --- Lookup ---

export interface LookupOutput {
  schema: string;
  file: string;
  contracts: ContractEntry[];
  dependencies: DependencyEntry[];
  decisions: DecisionEntry[];
  recent_history: TimelineEntry[];
  open_follow_ups: FollowUpEntry[];
  staleness?: StalenessInfo[];
  knowledge?: FilteredKnowledge;
}

export interface ContractEntry {
  file: string;
  anchor?: string;
  description: string;
  source: string;
  commit: string;
  timestamp: string;
}

export interface DependencyEntry {
  file: string;
  anchor?: string;
  target_file: string;
  target_anchor: string;
  assumption: string;
  commit: string;
  timestamp: string;
}

export interface DecisionEntry {
  what: string;
  why: string;
  stability: string;
  revisit_when?: string;
  scope: string[];
  commit: string;
  timestamp: string;
}

export interface TimelineEntry {
  commit: string;
  timestamp: string;
  commit_message: string;
  context_level: string;
  provenance: string;
  intent: string;
  original_schema: string;
  reasoning?: string;
  constraints: string[];
  risk_notes?: string;
}

export interface FollowUpEntry {
  commit: string;
  follow_up: string;
}

export interface StalenessInfo {
  annotation_commit: string;
  latest_file_commit: string;
  commits_since: number;
  stale: boolean;
}

// --- Knowledge ---

export interface FilteredKnowledge {
  conventions: Convention[];
  boundaries: ModuleBoundary[];
  anti_patterns: AntiPattern[];
}

export interface KnowledgeStore {
  schema: string;
  conventions: Convention[];
  boundaries: ModuleBoundary[];
  anti_patterns: AntiPattern[];
}

export interface Convention {
  id: string;
  scope: string;
  rule: string;
  decided_in?: string;
  stability: string;
}

export interface ModuleBoundary {
  id: string;
  module: string;
  owns: string;
  boundary: string;
  decided_in?: string;
}

export interface AntiPattern {
  id: string;
  pattern: string;
  instead: string;
  learned_from?: string;
}

// --- Summary ---

export interface SummaryOutput {
  schema: string;
  query: { file: string; anchor?: string };
  units: SummaryUnit[];
  stats: { regions_found: number; commits_examined: number };
}

export interface SummaryUnit {
  anchor: {
    type: string;
    name: string;
    signature?: string;
  };
  lines: { start: number; end: number };
  intent: string;
  constraints: string[];
  risk_notes?: string;
  last_modified: string;
}

// --- Sentiments ---

export interface SentimentEntry {
  feeling: string;
  detail: string;
  commit: string;
  timestamp: string;
  summary: string;
}

export interface SentimentsOutput {
  schema: string;
  sentiments: SentimentEntry[];
}

// --- Recent Annotations ---

export interface RecentAnnotation {
  commit: string;
  message: string;
  timestamp: string;
  summary: string;
  files: string[];
}

// --- Decisions (repo-wide) ---

export interface DecisionsOutput {
  schema: string;
  decisions: DecisionEntry[];
  rejected_alternatives: RejectedAlternativeEntry[];
}

export interface RejectedAlternativeEntry {
  approach: string;
  reason: string;
  commit: string;
  timestamp: string;
}
