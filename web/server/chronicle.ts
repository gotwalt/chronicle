import { noteRead, listAnnotatedCommits } from "./git.js";
import type { FileAnnotation } from "../src/types.js";

// Matches the actual Chronicle JSON schema (chronicle/v1)
interface Region {
  file: string;
  ast_anchor?: { unit_type: string; name: string; signature?: string };
  lines?: { start: number; end: number } | null;
  intent?: string;
  reasoning?: string;
  constraints?: { text: string; source: string }[] | null;
  semantic_dependencies?: { file: string; anchor: string; nature: string }[] | null;
  tags?: string[] | null;
  risk_notes?: string | null;
  corrections?: {
    corrected_by: string;
    timestamp: string;
    field: string;
    old_value: string;
    new_value: string;
    reason: string;
  }[] | null;
}

interface Annotation {
  schema: string;
  commit: string;
  timestamp: string;
  summary: string;
  regions?: Region[];
  cross_cutting?: unknown[];
}

export function getAnnotationsForFile(filePath: string): FileAnnotation[] {
  // Check all annotated commits, not just git-log-for-file commits,
  // because an annotation can reference a file in its regions even if
  // the commit didn't directly modify that file.
  const commits = listAnnotatedCommits();
  const seen = new Map<string, FileAnnotation>();

  for (const commit of commits) {
    const raw = noteRead(commit);
    if (!raw) continue;

    let annotation: Annotation;
    try {
      annotation = JSON.parse(raw);
    } catch {
      continue;
    }

    for (const region of annotation.regions ?? []) {
      if (region.file !== filePath) continue;
      if (!region.ast_anchor) continue;

      const key = region.ast_anchor.name;
      const existing = seen.get(key);
      if (existing && existing.timestamp >= annotation.timestamp) continue;

      seen.set(key, {
        commit: annotation.commit,
        timestamp: annotation.timestamp,
        summary: annotation.summary,
        anchor: region.ast_anchor,
        lines: region.lines ?? null,
        intent: region.intent ?? "",
        reasoning: region.reasoning ?? "",
        constraints: region.constraints ?? [],
        semantic_dependencies: region.semantic_dependencies ?? [],
        tags: region.tags ?? [],
        risk_notes: region.risk_notes ?? null,
        corrections: region.corrections ?? [],
      });
    }
  }

  return [...seen.values()].sort((a, b) => {
    if (!a.lines && !b.lines) return 0;
    if (!a.lines) return 1;
    if (!b.lines) return -1;
    return a.lines.start - b.lines.start;
  });
}

export function getAnnotationCounts(): Map<string, number> {
  const counts = new Map<string, number>();
  const commits = listAnnotatedCommits();

  for (const commit of commits) {
    const raw = noteRead(commit);
    if (!raw) continue;

    let annotation: Annotation;
    try {
      annotation = JSON.parse(raw);
    } catch {
      continue;
    }

    for (const region of annotation.regions ?? []) {
      if (region.file) {
        counts.set(region.file, (counts.get(region.file) ?? 0) + 1);
      }
    }
  }

  return counts;
}
