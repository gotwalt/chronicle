# Feature 23: Agent Knowledge Capture

**Status**: Proposed

## Motivation

Knowledge capture is entirely manual today. AI agents (like Claude Code) can
annotate commits with wisdom entries via the live path, but have no way to
simultaneously record repo-wide knowledge entries. The only way to add
knowledge is via the CLI command `git chronicle knowledge add`, which requires
the agent to make a separate tool call after each annotation.

This creates friction. An agent that discovers "all errors use snafu" or "git
module never imports provider" during annotation has no way to record that
insight in the same operation. The knowledge is either lost or requires a
manual follow-up step that's easy to skip.

**The core insight**: the annotating agent is the best observer of repo-wide
patterns. It should be able to capture conventions, anti-patterns, and module
boundaries alongside annotations, not as a separate step.

---

## Key Design Decisions

### 1. Additive, not replacing

A new optional `knowledge` field in `LiveInput`. No schema changes needed —
it writes to the existing `chronicle/knowledge-v1` store on
`refs/notes/chronicle-knowledge`. The knowledge store schema, types, and
read/write infrastructure are already implemented (Feature 21 P15).

### 2. Programmatic dedup at write time

Before writing a new entry, check the existing store for near-duplicates:
same type + similar scope/module + similar rule/pattern text. Skip silently
if a duplicate is found. This avoids bloating the knowledge store when agents
repeatedly discover the same conventions.

Similarity is intentionally simple — normalized substring or equality
matching, not embedding similarity. If the same rule string (case-insensitive,
whitespace-normalized) already exists in the same scope, it's a duplicate.

### 3. Auto-set provenance fields

All emitted entries auto-populate `decided_in` (conventions, boundaries) or
`learned_from` (anti-patterns) with the current commit SHA, providing
traceability without requiring the agent to specify it.

### 4. Backward-compatible

The `knowledge` field defaults to `[]`. Existing `LiveInput` JSON without
the field continues to work unchanged.

---

## Live Path Changes (`src/annotate/live.rs`)

Add an optional `knowledge` field to `LiveInput`:

```rust
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LiveInput {
    pub commit: String,
    pub summary: String,
    #[serde(default)]
    pub wisdom: Vec<WisdomEntryInput>,
    #[serde(default)]
    pub knowledge: Vec<KnowledgeEntryInput>,
    #[serde(skip)]
    pub staged_notes: Option<String>,
}
```

Where `KnowledgeEntryInput` has three variants:

```rust
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum KnowledgeEntryInput {
    #[serde(rename = "convention")]
    Convention {
        scope: String,
        rule: String,
        #[serde(default = "default_provisional")]
        stability: String,
    },
    #[serde(rename = "boundary")]
    Boundary {
        module: String,
        owns: String,
        boundary: String,
    },
    #[serde(rename = "anti_pattern")]
    AntiPattern {
        pattern: String,
        instead: String,
    },
}
```

The three entry types match the existing `chronicle/knowledge-v1` schema:

| Type | Required fields | Auto-set |
|------|----------------|----------|
| `convention` | `scope`, `rule` | `decided_in` <- commit SHA, `stability` defaults to `provisional` |
| `boundary` | `module`, `owns`, `boundary` | `decided_in` <- commit SHA |
| `anti_pattern` | `pattern`, `instead` | `learned_from` <- commit SHA |

On annotation, `handle_annotate_v3` writes any knowledge entries to the store
(after dedup check), and reports results in `LiveResult`:

```rust
pub struct LiveResult {
    pub success: bool,
    pub commit: String,
    pub wisdom_written: usize,
    pub knowledge_written: usize,
    pub knowledge_duplicates: usize,
    pub warnings: Vec<String>,
}
```

### Example: live annotation with knowledge

```json
{
  "commit": "HEAD",
  "summary": "Refactor error handling to use snafu consistently",
  "wisdom": [
    {
      "category": "insight",
      "content": "snafu's #[snafu(module(...))] pattern scopes context selectors cleanly",
      "file": "src/error.rs"
    }
  ],
  "knowledge": [
    {
      "type": "convention",
      "scope": "src/",
      "rule": "Use snafu 0.8 with #[snafu(module(...))] for all error types"
    },
    {
      "type": "anti_pattern",
      "pattern": "Using anyhow or thiserror for error handling",
      "instead": "Use snafu with scoped context selectors"
    }
  ]
}
```

---

## Dedup Helper: `is_duplicate`

New function in `src/knowledge/mod.rs`:

```rust
/// Check whether a candidate entry is a near-duplicate of an existing one.
///
/// Matching rules:
/// - Convention: same scope + normalized rule text matches
/// - Boundary: same module + normalized boundary text matches
/// - AntiPattern: normalized pattern text matches
///
/// "Normalized" = lowercased, whitespace-collapsed, trailing punctuation stripped.
pub fn is_duplicate(store: &KnowledgeStore, candidate: &DedupCandidate) -> bool
```

Where `DedupCandidate` is an enum:

```rust
pub enum DedupCandidate {
    Convention { scope: String, rule: String },
    Boundary { module: String, boundary: String },
    AntiPattern { pattern: String },
}
```

This keeps dedup logic centralized and testable.

---

## Schema Documentation Updates

### `git chronicle schema live-input`

Update the self-documenting schema to include the `knowledge` field and
`KnowledgeEntryInput` variants.

### Skills and hooks

| File | Change |
|------|--------|
| `.claude/skills/annotate/SKILL.md` | Document `knowledge` field in live input JSON reference |
| `embedded/skills/annotate/SKILL.md` | Mirror the above |

Add guidance: "If you discover a convention or anti-pattern that applies
beyond this single commit, include it in the `knowledge` array."

---

## Implementation Steps

### Step 1: Add dedup helper to `src/knowledge/mod.rs`

- Add `DedupCandidate` enum and `is_duplicate()` function
- Add `normalize_text()` helper (lowercase, collapse whitespace, strip
  trailing punctuation)
- Unit tests for dedup: exact match, near-match, non-match, different scope
  not duplicate

**Files**: `src/knowledge/mod.rs`

### Step 2: Add `knowledge` field to `LiveInput`

- Add `KnowledgeEntryInput` enum to `src/annotate/live.rs`
- Add `knowledge` field to `LiveInput` (with `#[serde(default)]`)
- In `handle_annotate_v3`, after writing the annotation:
  1. Read knowledge store
  2. For each entry, check dedup, write if new
  3. Add `knowledge_written` and `knowledge_duplicates` to `LiveResult`
- Update test constructions of `LiveInput` to include `knowledge: vec![]`

**Files**: `src/annotate/live.rs`

### Step 3: Update schema docs and skills

- Update `git chronicle schema live-input` output to include knowledge field
- Update `.claude/skills/annotate/SKILL.md` with knowledge field docs
- Mirror to `embedded/skills/annotate/SKILL.md`

**Files**: `src/cli/schema.rs`, `.claude/skills/annotate/SKILL.md`,
`embedded/skills/annotate/SKILL.md`

### Step 4: Tests

- **Unit tests** (`src/knowledge/mod.rs`):
  - `is_duplicate` with exact match convention
  - `is_duplicate` with near-match (different casing/whitespace)
  - `is_duplicate` returns false for different scope
  - `is_duplicate` for boundaries and anti-patterns

- **Unit tests** (`src/annotate/live.rs`):
  - LiveInput deserialization with knowledge entries
  - LiveInput deserialization without knowledge (backward compat)
  - Knowledge entries written during annotation
  - Duplicate knowledge entries skipped

---

## Key Files to Modify

| File | Change |
|------|--------|
| `src/knowledge/mod.rs` | `DedupCandidate`, `is_duplicate()`, `normalize_text()` |
| `src/annotate/live.rs` | `KnowledgeEntryInput`, `knowledge` field in `LiveInput`, write logic |
| `src/cli/schema.rs` | Update `live-input` schema output |
| `.claude/skills/annotate/SKILL.md` | Document knowledge field |
| `embedded/skills/annotate/SKILL.md` | Mirror skills update |

---

## Dependencies

- **Feature 21 P15** (knowledge store): Already implemented. This feature
  builds on the existing `chronicle/knowledge-v1` schema, `KnowledgeStore`
  types, and `knowledge::read_store/write_store` infrastructure.

- **No schema changes needed**. Uses existing `chronicle/knowledge-v1`.

---

## Risks

- **Dedup false positives**: Simple text normalization may miss semantic
  duplicates or falsely match genuinely different rules. Acceptable for v1 --
  the knowledge store is human-reviewable via `knowledge list` and entries
  can be removed with `knowledge remove`.

- **LiveInput test churn**: Adding `knowledge: vec![]` to ~10 test
  construction sites. Mechanical but necessary (documented in MEMORY.md).

---

## Acceptance Criteria

1. Live path accepts optional `knowledge` array in `LiveInput` JSON
2. Knowledge entries are written atomically to the existing knowledge store
3. Duplicate entries are detected and skipped (not written twice)
4. `decided_in` / `learned_from` are auto-populated with commit SHA
5. `LiveResult` reports `knowledge_written` and `knowledge_duplicates`
6. Existing `knowledge list` / `knowledge remove` commands work unchanged
   with entries written via the live path
7. All new code has unit tests
8. Skills docs updated to reference the new `knowledge` field
