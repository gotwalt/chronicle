# Feature 23: Agent Knowledge Capture

**Status**: Proposed

## Motivation

Knowledge capture is entirely manual today. The agent loop (batch annotation)
can emit narrative, decisions, and markers, but has no tool for knowledge
entries. The post-commit hook reminds the human to run `knowledge add`
manually, but batch annotations never produce knowledge.

The agent already sees patterns (e.g., "every error uses snafu", "git module
never imports provider") but has no tool to record them. Cross-repo
conventions discovered during batch annotation are lost unless a human
reviews and manually runs `knowledge add`.

**The core insight**: the annotation agent is the best observer of repo-wide
patterns — it reads every commit's diff and context. It should be able to
capture conventions, anti-patterns, and module boundaries it discovers, not
just per-commit wisdom.

---

## Key Design Decisions

### 1. Additive, not replacing

`emit_knowledge` is a new optional tool alongside existing agent tools. No
schema changes needed — it writes to the existing `chronicle/knowledge-v1`
store on `refs/notes/chronicle-knowledge`. The knowledge store schema, types,
and read/write infrastructure are already implemented (Feature 21 P15).

### 2. Programmatic dedup at write time

Before writing a new entry, check the existing store for near-duplicates:
same type + similar scope/module + similar rule/pattern text. Skip silently
if a duplicate is found, returning "already exists" to the agent. This avoids
needing to inject the full knowledge store into the prompt (which would waste
tokens and scale poorly).

Similarity is intentionally simple — normalized substring or equality
matching, not embedding similarity. If the same rule string (case-insensitive,
whitespace-normalized) already exists in the same scope, it's a duplicate.

### 3. Guard rails via prompt instructions

The system prompt instructs:
- Only emit knowledge when the pattern applies beyond this one commit
- Prefer `provisional` stability for inferred conventions
- Never emit knowledge for one-off implementation choices
- Most commits produce zero knowledge entries

### 4. Both agent loop and live path

- **Agent loop**: `emit_knowledge` tool in `src/agent/tools.rs`
- **Live path**: optional `knowledge` field in `LiveInput` for `--live`
  annotations (so Claude Code can emit knowledge alongside annotations)

### 5. Atomic writes per tool call

Each `emit_knowledge` call writes immediately to the knowledge store via
`knowledge::write_store` (read-modify-write). This means knowledge
accumulates across agent turns rather than being batched to the end.
Immediate writes also mean knowledge is persisted even if the agent loop
fails partway through.

### 6. Auto-set provenance fields

All emitted entries auto-populate `decided_in` (conventions, boundaries) or
`learned_from` (anti-patterns) with the current commit SHA, providing
traceability without requiring the agent to specify it.

---

## New Agent Tool: `emit_knowledge`

### Tool Definition

```json
{
  "name": "emit_knowledge",
  "description": "Record a repo-wide convention, module boundary, or anti-pattern discovered in this commit. Only use when the pattern clearly applies beyond this single commit. Most commits produce zero knowledge entries.",
  "input_schema": {
    "type": "object",
    "properties": {
      "entry_type": {
        "type": "string",
        "enum": ["convention", "boundary", "anti_pattern"],
        "description": "Type of knowledge entry"
      },
      "scope": {
        "type": "string",
        "description": "For convention: directory/file scope (e.g. 'src/', 'src/schema/', '*')"
      },
      "rule": {
        "type": "string",
        "description": "For convention: the rule or convention text"
      },
      "stability": {
        "type": "string",
        "enum": ["permanent", "provisional", "experimental"],
        "description": "For convention: how stable is this convention? Default: provisional"
      },
      "module": {
        "type": "string",
        "description": "For boundary: the module path (e.g. 'src/git/')"
      },
      "owns": {
        "type": "string",
        "description": "For boundary: what this module is responsible for"
      },
      "boundary": {
        "type": "string",
        "description": "For boundary: the boundary rule (what it must not do)"
      },
      "pattern": {
        "type": "string",
        "description": "For anti_pattern: the pattern to avoid"
      },
      "instead": {
        "type": "string",
        "description": "For anti_pattern: what to do instead"
      }
    },
    "required": ["entry_type"]
  }
}
```

The three entry types match the existing `chronicle/knowledge-v1` schema:

| Type | Required fields | Auto-set |
|------|----------------|----------|
| `convention` | `scope`, `rule` | `decided_in` ← commit SHA, `stability` defaults to `provisional` |
| `boundary` | `module`, `owns`, `boundary` | `decided_in` ← commit SHA |
| `anti_pattern` | `pattern`, `instead` | `learned_from` ← commit SHA |

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

This keeps dedup logic centralized and testable, separate from the agent tool
dispatch.

---

## System Prompt Changes (`src/agent/prompt.rs`)

Add a new section after the markers instruction:

```
## Knowledge capture (optional)

If you observe a convention, module boundary, or anti-pattern that applies
repo-wide or module-wide — not just to this one commit — use `emit_knowledge`
to record it.

Guidelines:
- Only emit when the pattern clearly applies beyond this single commit
- Prefer `provisional` stability for inferred conventions
- Never emit for one-off implementation choices
- Typical commits produce zero knowledge entries
- Good examples: "all errors use snafu", "git module never imports provider",
  "don't use serde_json::from_str for annotations — use parse_annotation()"
- Bad examples: "this function takes a &str" (too specific),
  "code should be clean" (too vague)
```

---

## Agent Loop Changes (`src/agent/mod.rs`)

1. Add a new field to `CollectedOutput`:
   ```rust
   pub knowledge_entries: Vec<KnowledgeEmission>,
   ```
   Where `KnowledgeEmission` is a lightweight tracking struct:
   ```rust
   pub struct KnowledgeEmission {
       pub entry_type: String,  // "convention", "boundary", "anti_pattern"
       pub was_duplicate: bool,
   }
   ```

2. Pass `commit_sha` into `dispatch_tool` (already available via
   `context.commit_sha`).

3. `dispatch_tool` already receives `git_ops: &dyn GitOps` — no threading
   changes needed for the git reference.

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

Where `KnowledgeEntryInput` mirrors the agent tool's three variants:

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

### Step 2: Add `emit_knowledge` tool definition and dispatch

- Add `emit_knowledge` to `tool_definitions()` in `src/agent/tools.rs`
- Add `KnowledgeEmission` struct to `CollectedOutput`
- Implement `dispatch_emit_knowledge()`:
  1. Parse `entry_type` and variant-specific fields from input
  2. Read current store via `knowledge::read_store(git_ops)`
  3. Build `DedupCandidate`, check `is_duplicate()`
  4. If not duplicate: construct entry, add to store, write via
     `knowledge::write_store()`, record emission
  5. If duplicate: return "already exists" message, record as duplicate
- Add `generate_id()` helper (reuse logic from `cli/knowledge.rs` or extract
  shared helper)

**Files**: `src/agent/tools.rs`, `src/agent/mod.rs`

### Step 3: Update system prompt

- Add knowledge capture section to `build_system_prompt()` in
  `src/agent/prompt.rs`
- Include guidelines on when to emit and when not to

**Files**: `src/agent/prompt.rs`

### Step 4: Add `knowledge` field to `LiveInput`

- Add `KnowledgeEntryInput` enum to `src/annotate/live.rs`
- Add `knowledge` field to `LiveInput` (with `#[serde(default)]`)
- In `handle_annotate_v3`, after writing the annotation:
  1. Read knowledge store
  2. For each entry, check dedup, write if new
  3. Add `knowledge_written` and `knowledge_duplicates` to `LiveResult`
- Update test constructions of `LiveInput` to include `knowledge: vec![]`

**Files**: `src/annotate/live.rs`

### Step 5: Update schema docs and skills

- Update `git chronicle schema live-input` output to include knowledge field
- Update `.claude/skills/annotate/SKILL.md` with knowledge field docs
- Mirror to `embedded/skills/annotate/SKILL.md`

**Files**: `src/cli/schema.rs`, `.claude/skills/annotate/SKILL.md`,
`embedded/skills/annotate/SKILL.md`

### Step 6: Tests

- **Unit tests** (`src/knowledge/mod.rs`):
  - `is_duplicate` with exact match convention
  - `is_duplicate` with near-match (different casing/whitespace)
  - `is_duplicate` returns false for different scope
  - `is_duplicate` for boundaries and anti-patterns

- **Unit tests** (`src/agent/tools.rs`):
  - `dispatch_emit_knowledge` with convention input
  - `dispatch_emit_knowledge` with boundary input
  - `dispatch_emit_knowledge` with anti-pattern input
  - `dispatch_emit_knowledge` with duplicate detection
  - `dispatch_emit_knowledge` with missing required fields → error

- **Unit tests** (`src/annotate/live.rs`):
  - LiveInput deserialization with knowledge entries
  - LiveInput deserialization without knowledge (backward compat)
  - Knowledge entries written during annotation
  - Duplicate knowledge entries skipped

- **Integration test**: full agent loop emitting knowledge entry

---

## Key Files to Modify

| File | Change |
|------|--------|
| `src/knowledge/mod.rs` | `DedupCandidate`, `is_duplicate()`, `normalize_text()` |
| `src/agent/tools.rs` | `emit_knowledge` tool definition + `dispatch_emit_knowledge()` |
| `src/agent/prompt.rs` | Knowledge emission instructions in system prompt |
| `src/agent/mod.rs` | `KnowledgeEmission` in `CollectedOutput` |
| `src/annotate/live.rs` | `KnowledgeEntryInput`, `knowledge` field in `LiveInput`, write logic |
| `src/cli/schema.rs` | Update `live-input` schema output |
| `.claude/skills/annotate/SKILL.md` | Document knowledge field |
| `embedded/skills/annotate/SKILL.md` | Mirror skills update |

---

## Dependencies

- **Feature 21 P15** (knowledge store): Already implemented. This feature
  builds on the existing `chronicle/knowledge-v1` schema, `KnowledgeStore`
  types, and `knowledge::read_store/write_store` infrastructure.

- **No dependency on Feature 22** (schema v3). Knowledge capture is
  orthogonal to the annotation schema — the knowledge store lives on a
  separate ref (`refs/notes/chronicle-knowledge`). This feature works with
  both v2 and v3 annotations.

- **No schema changes needed**. Uses existing `chronicle/knowledge-v1`.

---

## Risks

- **Agent over-emitting knowledge**: Mitigated by prompt guard rails and
  dedup. Even if the agent emits duplicates, `is_duplicate` filters them.
  The prompt explicitly says "most commits produce zero knowledge entries."

- **Dedup false positives**: Simple text normalization may miss semantic
  duplicates or falsely match genuinely different rules. Acceptable for v1 —
  the knowledge store is human-reviewable via `knowledge list` and entries
  can be removed with `knowledge remove`.

- **LiveInput test churn**: Adding `knowledge: vec![]` to ~10 test
  construction sites. Mechanical but necessary (documented in MEMORY.md).

---

## Acceptance Criteria

1. Agent loop can emit knowledge entries via `emit_knowledge` tool
2. Knowledge entries are written atomically to the existing knowledge store
3. Duplicate entries are detected and skipped (not written twice)
4. `decided_in` / `learned_from` are auto-populated with commit SHA
5. Live path accepts optional `knowledge` array in `LiveInput` JSON
6. `LiveResult` reports `knowledge_written` and `knowledge_duplicates`
7. System prompt includes knowledge capture guidelines
8. Existing `knowledge list` / `knowledge remove` commands work unchanged
   with agent-emitted entries
9. All new code has unit tests
10. Skills docs updated to reference the new `knowledge` field
