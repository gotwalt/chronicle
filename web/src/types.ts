export interface TreeFile {
  path: string;
  annotation_count: number;
}

export interface TreeResponse {
  files: TreeFile[];
}

export interface Anchor {
  unit_type: string;
  name: string;
  signature?: string;
}

export interface Constraint {
  text: string;
  source: string;
}

export interface SemanticDependency {
  file: string;
  anchor: string;
  nature: string;
}

export interface Correction {
  corrected_by: string;
  timestamp: string;
  field: string;
  old_value: string;
  new_value: string;
  reason: string;
}

export interface FileAnnotation {
  commit: string;
  timestamp: string;
  summary: string;
  anchor: Anchor;
  lines: { start: number; end: number } | null;
  intent: string;
  reasoning: string;
  constraints: Constraint[];
  semantic_dependencies: SemanticDependency[];
  tags: string[];
  risk_notes: string | null;
  corrections: Correction[];
}

export interface FileResponse {
  path: string;
  content: string;
  language: string;
  annotations: FileAnnotation[];
}
