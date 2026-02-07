# Feature 20: Chronicle v2 Schema and Pipeline Redesign

**Status**: In Progress

## Motivation

The current `chronicle/v1` annotations are not useful to AI coding assistants at read time. They produce per-function annotations that restate the diff instead of capturing decision context. The schema forces granularity (per-region with required file/anchor/lines/intent) that creates overhead and noise, while missing the highest-value metadata: *why this approach was chosen*, *what was tried and rejected*, and *what's provisional*.

## Key Design Decisions

1. **The commit is the primary unit of annotation.** Code-level metadata is an optional secondary layer, used only when there's something genuinely non-obvious about a specific code location.

2. **Drop MCP, use skill+CLI as the integration surface.** The MCP server (Feature 12) was never built beyond `src/mcp/annotate_handler.rs`. Skills provide the workflow, the CLI provides self-documenting data contracts via `git chronicle schema`.

3. **Add `git chronicle schema <name>` subcommand.** Makes the CLI self-documenting for AI agents.

## Schema: `chronicle/v2`

### Top-level Annotation

- `schema`: "chronicle/v2"
- `commit`: commit SHA
- `timestamp`: RFC3339
- `narrative`: Narrative (commit-level, always present)
- `decisions`: Vec<Decision> (zero or more)
- `markers`: Vec<CodeMarker> (optional, only where valuable)
- `effort`: Option<EffortLink> (link to broader effort)
- `provenance`: Provenance (how this annotation was created)

### Narrative

- `summary`: What this commit does and WHY this approach. Not a diff restatement.
- `motivation`: Optional. What triggered this change?
- `rejected_alternatives`: Vec<RejectedAlternative>. HIGHEST-VALUE new field.
- `follow_up`: Optional. Expected follow-up work. None = complete.
- `files_changed`: Vec<String>. Auto-populated from diff for indexing.

### Decision

- `what`: What was decided
- `why`: Why
- `stability`: permanent | provisional | experimental
- `revisit_when`: Optional trigger for revisiting
- `scope`: Files/modules this applies to

### CodeMarker (replaces RegionAnnotation)

- `file`: String
- `anchor`: Optional AstAnchor
- `lines`: Optional LineRange
- `kind`: Contract | Hazard | Dependency | Unstable (typed variants)

### EffortLink

- `id`: Stable identifier (ticket ID, slug)
- `description`: Human-readable
- `phase`: start | in_progress | complete

### Provenance

- `source`: live | batch | backfill | squash | amend | migrated_v1
- `derived_from`: Vec<String>
- `notes`: Optional<String>

## What Was Removed from v1

| v1 | v2 | Why |
|----|----|----|
| `RegionAnnotation` (whole struct) | `CodeMarker` | Regions forced per-function granularity. Markers are optional, typed, targeted. |
| `region.intent` | `narrative.summary` | Intent belongs at the commit level |
| `region.reasoning` | `narrative` + `decisions` | Reasoning is commit-level or a decision record |
| `region.tags` | Removed | Low signal, redundant with commit message |
| `region.risk_notes` | `MarkerKind::Hazard` | Becomes a typed marker |
| `region.constraints` | `MarkerKind::Contract` | Becomes a typed marker |
| `region.semantic_dependencies` | `MarkerKind::Dependency` | Becomes a typed marker |
| `region.related_annotations` | `EffortLink` | Effort linking replaces ad-hoc cross-references |
| `CrossCuttingConcern` | `Decision` or narrative | Cross-cutting concerns are decisions |
| `context_level` | `provenance.source` | Live/batch/backfill tells you the quality level |

## Versioning Architecture

Designed for v3, v22, v105...

1. The "canonical" type is always the latest version's schema
2. Each schema version lives in its own module (schema::v1, schema::v2, ...)
3. A single `parse_annotation(json)` function detects version and returns the canonical type
4. Migration functions chain: v1->v2, later v2->v3, so v1->v3 = v1->v2 then v2->v3
5. All internal code uses only the canonical type

### Module layout

```
src/schema/
  mod.rs          -- pub type Annotation = v2::Annotation; parse_annotation();
  v1.rs           -- v1 types (moved from annotation.rs)
  v2.rs           -- v2 types (new canonical)
  migrate.rs      -- v1_to_v2(), future v2_to_v3(), etc.
  correction.rs   -- corrections (version-independent)
  common.rs       -- shared types: AstAnchor, LineRange (used by all versions)
```

### Migration strategy: Lazy

- All writes produce the latest version (v2)
- All reads parse any version and migrate to canonical on the fly
- No bulk rewrite needed. Old v1 notes stay as v1 in git.

## Implementation Phases

### Phase 1: Versioning infrastructure (src/schema/)
- Move current types to v1.rs, extract shared types to common.rs
- Add v2.rs with new types
- Add migrate.rs with v1_to_v2()
- Update mod.rs: canonical type alias + parse_annotation()
- Replace all scattered from_str::<Annotation> calls

### Phase 2: Live write path
- Move handler from src/mcp/annotate_handler.rs to src/annotate/live.rs
- Add v2 input types with flexible deserialization
- Auto-populate files_changed from diff

### Phase 3: Self-documenting CLI
- Add `git chronicle schema` subcommand (schemars)

### Phase 4: Read pipeline
- New query types: contracts, decisions, effort, unstable
- Update CLI commands

### Phase 5: Batch path
- Narrative-first agent prompt
- New agent tools: emit_narrative, emit_decision, emit_marker

### Phase 6: Squash/Amend/Export
- Update squash synthesis, export/import, web viewer

### Phase 7: Skills and docs
- Update skills and CLAUDE.md

## Dependencies

- Phase 1 is prerequisite for all other phases
- Phases 2-7 can be done somewhat independently after Phase 1
