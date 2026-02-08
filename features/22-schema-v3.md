# Feature 22: Chronicle v3 Schema — Wisdom Over Metadata

**Status**: Proposed

## Motivation

Chronicle v2 tried to be a structured database of code metadata: contracts,
typed markers (9 variants), decisions with stability tags, dependency links.
In practice this approach is brittle and low-value:

1. **Contracts are redundant.** Type signatures, doc comments, assertions, and
   tests already convey preconditions. LSPs discover them on the fly. A contract
   like "must not exceed MAX_BACKOFF_SECS" can be read from the code itself.

2. **Stale metadata is worse than no metadata.** Code evolves but annotations
   don't. An outdated contract actively misleads an agent. Feature 21's
   staleness detection (P11) was a band-aid — the real fix is to stop storing
   information that goes stale.

3. **Most markers duplicate better homes.** `Performance`, `TestCoverage`,
   `TechDebt`, `Deprecated` belong in code comments or issue trackers — systems
   that live next to the code they describe. Chronicle can't compete with inline
   `// TODO` or a linter warning.

4. **Stability tags are overhead without payoff.** The `stability` /
   `revisit_when` metadata on decisions has never been queried or acted on
   in practice. It's project-management metadata masquerading as code context.

The highest-value v2 content comes from three fields: `rejected_alternatives`
(things tried and failed), `sentiments` (agent intuition), and good
`narrative.summary` lines (the "why"). Everything else is either reconstructible
from tools or goes stale.

**The core insight**: Chronicle's job is to capture *accumulated agent wisdom* —
the institutional intuition, dead ends, and "aha moments" that no tool can
reconstruct from code alone. v3 restructures the entire schema around this
principle.

This supersedes Feature 21 proposals P9 (extended MarkerKind), P11 (staleness
detection), and P13 (materialized per-file view) by eliminating the structures
they were extending or patching.

---

## Key Design Decisions

### 1. Wisdom over metadata

v2 asks "what metadata can we extract?" — contracts, dependencies, markers.
v3 asks "what did the agent learn that no tool can reconstruct?" The answer
is always prose: dead ends, gotchas, insights, unfinished threads.

### 2. Line-grounded writes, file-level reads

Each wisdom entry is anchored to specific line numbers (the code it was learned
about). But reads aggregate at the file level — `git chronicle read src/foo.rs`
returns all wisdom for a file across commits. This matches how agents work:
they write about specific code, but need to understand whole files.

### 3. Four categories replace nine marker kinds + decisions

v2's `MarkerKind` (9 variants) and `Decision` struct are replaced by four
wisdom categories that map to how agents actually think:

| Category | What it captures | Replaces in v2 |
|----------|-----------------|----------------|
| `dead_end` | Things tried and failed | `rejected_alternatives` |
| `gotcha` | Non-obvious traps invisible in the code | `Hazard` markers, `Contract` markers, worry/unease sentiments |
| `insight` | Mental models, key relationships, architecture | *New* — v2 couldn't capture this |
| `unfinished_thread` | Incomplete work, suspected better approaches | `follow_up`, `TechDebt` markers, uncertainty sentiments |

### 4. Simplified narrative

The `narrative` struct drops `motivation`, `follow_up`, and `files_changed`.
Motivation and follow-up are better expressed as wisdom entries (with line
grounding). `files_changed` was always auto-populated filler.

### 5. Provenance preserved, effort removed

`Provenance` carries forward (source, author, derived_from). `EffortLink` is
removed — ticket references belong in commit messages.

---

## Schema: `chronicle/v3`

### Top-level Annotation

```
{
  "schema": "chronicle/v3",
  "commit": "<sha>",
  "timestamp": "<RFC3339>",
  "summary": "Why this approach, not what changed",
  "wisdom": [ <WisdomEntry>, ... ],
  "provenance": <Provenance>
}
```

### WisdomEntry

```
{
  "category": "dead_end" | "gotcha" | "insight" | "unfinished_thread",
  "content": "Prose text — what was learned",
  "file": "src/foo.rs",             // optional: omit for repo-wide wisdom
  "lines": { "start": 42, "end": 67 }  // optional: omit for file-wide wisdom
}
```

**Fields:**

- `category` (required): One of four values. Determines how the entry is
  surfaced in reads.
- `content` (required): Free-form prose. Should express the *wisdom*, not
  restate what the code does. Good: "Tried using `tokio::spawn` here but it
  caused lifetime issues with the borrow checker because `&self` isn't
  `'static`." Bad: "This function calls tokio::spawn."
- `file` (optional): Relative path from repo root. When omitted, the wisdom
  applies to the commit as a whole.
- `lines` (optional): `LineRange` from v2 (`{ start, end }`). Only meaningful
  when `file` is present. Enables future git-blame integration.

### Provenance

```
{
  "source": "live" | "batch" | "backfill" | "squash" | "amend" | "migrated_v1" | "migrated_v2",
  "author": "claude-code",   // optional
  "derived_from": [],        // commit SHAs for squash/amend
  "notes": null              // optional free text
}
```

Unchanged from v2 except: new `migrated_v2` source value.

### Rust types (src/schema/v3.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub schema: String,               // "chronicle/v3"
    pub commit: String,
    pub timestamp: String,
    pub summary: String,
    pub wisdom: Vec<WisdomEntry>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WisdomEntry {
    pub category: WisdomCategory,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<LineRange>,      // from common.rs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WisdomCategory {
    DeadEnd,
    Gotcha,
    Insight,
    UnfinishedThread,
}
```

---

## What Was Removed from v2

| v2 | v3 | Why |
|----|----|----|
| `narrative` (struct) | `summary` (flat string) | Motivation and follow-up are better as wisdom entries with line grounding |
| `narrative.motivation` | Wisdom entry (`insight` or `gotcha`) | Line-grounded prose is more useful than a floating string |
| `narrative.rejected_alternatives` | Wisdom entries (`dead_end`) | Same content, now line-grounded and categorized |
| `narrative.follow_up` | Wisdom entry (`unfinished_thread`) | Line-grounded, with context about *where* the thread is |
| `narrative.files_changed` | Removed | Always auto-populated filler; git diff provides this |
| `narrative.sentiments` | Folded into wisdom categories | Worry/unease → `gotcha`; uncertainty → `unfinished_thread`; confidence is noise |
| `decisions` (Vec\<Decision\>) | Wisdom entries (`insight` or `dead_end`) | Decisions are insights; "what was decided" → `insight`, "what was rejected" → `dead_end` |
| `Decision.stability` | Removed | Never queried in practice |
| `Decision.revisit_when` | Removed | Project management metadata without demonstrated value |
| `Decision.scope` | `WisdomEntry.file` | Line-grounded file reference replaces a scope list |
| `markers` (Vec\<CodeMarker\>) | Wisdom entries | All 9 marker kinds collapse into 4 wisdom categories |
| `MarkerKind::Contract` | `gotcha` | Contracts invisible in code become gotchas |
| `MarkerKind::Hazard` | `gotcha` | Hazards are gotchas by definition |
| `MarkerKind::Dependency` | `insight` | Cross-code relationships are insights |
| `MarkerKind::Unstable` | `unfinished_thread` | Unstable code = unfinished work |
| `MarkerKind::Security` | `gotcha` | Security traps are gotchas |
| `MarkerKind::Performance` | `gotcha` or `insight` | Perf cliffs are gotchas; perf architecture is insight |
| `MarkerKind::Deprecated` | `unfinished_thread` | Deprecation = unfinished migration |
| `MarkerKind::TechDebt` | `unfinished_thread` | Tech debt = acknowledged incomplete work |
| `MarkerKind::TestCoverage` | Removed | Belongs in code comments or CI config |
| `effort` (EffortLink) | Removed | Ticket references belong in commit messages |
| `AstAnchor` | Removed | Line ranges are sufficient; AST anchors required parsing infrastructure that added complexity without proportional value |

### What Was Preserved

| v2 | v3 | Notes |
|----|----|----|
| `narrative.summary` | `summary` | Top-level "why" framing — the single most important field |
| `rejected_alternatives` | `dead_end` wisdom entries | Highest-value v2 field, now line-grounded |
| `sentiments` (concept) | Integrated into wisdom categories | Agent intuition is preserved, just categorized differently |
| `Provenance` | `Provenance` | Unchanged except new `migrated_v2` source |
| `LineRange` | `WisdomEntry.lines` | Still used for line grounding |

---

## Migration: v2 → v3

Same lazy strategy as v1 → v2: migrate on read, never bulk-rewrite.

### Rules

`parse_annotation()` detects `"schema": "chronicle/v3"` and returns it
directly. For `"chronicle/v2"`, apply `v2_to_v3()`:

1. `summary` ← `v2.narrative.summary`

2. Convert `v2.narrative.rejected_alternatives` → wisdom entries:
   ```
   { category: "dead_end", content: "{approach}: {reason}", file: None, lines: None }
   ```

3. Convert `v2.narrative.motivation` (if present) → wisdom entry:
   ```
   { category: "insight", content: v2.narrative.motivation, file: None, lines: None }
   ```

4. Convert `v2.narrative.follow_up` (if present) → wisdom entry:
   ```
   { category: "unfinished_thread", content: v2.narrative.follow_up, file: None, lines: None }
   ```

5. Convert `v2.narrative.sentiments` → wisdom entries:
   - feeling contains "worry" | "unease" | "concern" → `gotcha`
   - feeling contains "uncertain" | "doubt" → `unfinished_thread`
   - otherwise → `insight`
   - `content` ← `"{feeling}: {detail}"`

6. Convert `v2.decisions` → wisdom entries:
   - `category`: `insight`
   - `content` ← `"{what}: {why}"`
   - `file` ← first element of `decision.scope` (if any)
   - `lines` ← `None`

7. Convert `v2.markers` → wisdom entries:
   - `file` ← `marker.file`
   - `lines` ← `marker.lines`
   - Category mapping per the "What Was Removed" table above
   - `content` ← the marker's `description` field (or for `Dependency`:
     `"Depends on {target_file}:{target_anchor} — {assumption}"`)

8. `provenance` ← `v2.provenance` with source unchanged (not rewritten to
   `migrated_v2` — that's only for stored rewrites).

9. `v2.effort` is dropped (no v3 equivalent).

10. `v2.narrative.files_changed` is dropped.

### Chained migration

v1 notes hit `v1_to_v2()` first (already implemented), then `v2_to_v3()`.
Migration functions chain: `v1 → v2 → v3`.

---

## Read Path: Unified `read` Command

v2 has five read subcommands: `contracts`, `decisions`, `deps`, `summary`,
`read`. v3 replaces them all with a single `read` command that returns wisdom
entries filtered and aggregated by file.

### CLI

```
git chronicle read <file> [--anchor <name>] [--category <cat>]
git chronicle read --recent [--limit N]
```

### Output schema: `chronicle-read/v1`

```json
{
  "schema": "chronicle-read/v1",
  "file": "src/foo.rs",
  "wisdom": [
    {
      "category": "gotcha",
      "content": "The borrow checker won't allow ...",
      "lines": { "start": 42, "end": 67 },
      "commit": "abc123",
      "timestamp": "2025-01-15T10:30:00Z",
      "commits_since": 3
    }
  ]
}
```

Each entry includes `commit` and `timestamp` from the source annotation, plus
`commits_since` (how many commits have touched this file since the annotation —
replacing the staleness detection machinery from Feature 21 P11).

### Behavior

- File-scoped: returns all wisdom entries where `file` matches the query path
- `--anchor` filters to entries whose `lines` overlap the anchor's line range
  (requires resolving the anchor to a line range via the current file)
- `--category` filters to a single category
- `--recent` returns the N most recent wisdom entries across all files
  (orientation mode)
- Entries sorted newest-first within each category
- Deduplication: if the same file+lines has multiple entries in different
  commits, all are returned (agent can see how understanding evolved)

### Removed subcommands

| v2 subcommand | v3 replacement |
|---------------|---------------|
| `contracts` | `read <file> --category gotcha` |
| `decisions` | `read --recent --category insight` |
| `deps` | `read <file> --category insight` (dependency wisdom surfaces here) |
| `summary` | `read <file>` (all categories) |
| `read` (v2 retrieve) | `read <file>` (same name, new implementation) |

---

## Write Path: LiveInput v3

### CLI

```
# Full wisdom annotation (pipe JSON to stdin or use --json)
git chronicle annotate --live <<'EOF'
{
  "commit": "HEAD",
  "summary": "Switch to exponential backoff — linear was too aggressive",
  "wisdom": [
    {
      "category": "dead_end",
      "content": "Tried linear backoff first. Under load, all clients reconnect simultaneously creating a thundering herd.",
      "file": "src/mqtt/reconnect.rs",
      "lines": { "start": 42, "end": 67 }
    },
    {
      "category": "gotcha",
      "content": "The max_backoff config is in seconds but the sleep() call expects milliseconds. Off-by-1000x if you forget.",
      "file": "src/mqtt/reconnect.rs",
      "lines": { "start": 55, "end": 55 }
    }
  ]
}
EOF

# Summary-only (trivial changes)
git chronicle annotate --summary "Fix typo in error message"
```

### LiveInput struct

```rust
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct LiveInput {
    pub commit: String,
    pub summary: String,
    pub wisdom: Vec<WisdomEntryInput>,
    #[serde(skip)]
    pub staged_notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WisdomEntryInput {
    pub category: WisdomCategory,
    pub content: String,
    pub file: Option<String>,
    pub lines: Option<LineRange>,
}
```

Dramatically simpler than v2's LiveInput (which has 11 fields, 6 input types,
and a 9-variant MarkerKindInput enum).

### Staged notes integration

Staged notes (from `git chronicle note`) are incorporated as wisdom entries
at annotation time, same as v2. The staged note format may be updated to
include a category hint.

---

## Future: Git-Blame Integration

v3's line-grounded wisdom entries are designed with git-blame integration in
mind, though this is not part of the initial implementation.

### Concept

When an agent runs `git chronicle read src/foo.rs`, Chronicle could
transparently use `git blame` to:

1. **Discover relevant commits**: Instead of scanning all annotated commits,
   blame the file to find which commits touched which lines, then look up
   only those commits' annotations.

2. **Remap line numbers**: A wisdom entry written for lines 42-67 in commit
   `abc123` may correspond to lines 50-75 in the current working tree.
   Blame data enables this remapping.

3. **Compute line-level freshness**: If the lines a wisdom entry refers to
   have been modified since the annotation was written, the entry is
   potentially stale. This is more precise than the file-level
   `commits_since` counter.

### Why not now

- Blame is expensive for large files (shelling out to `git blame`)
- Line remapping requires careful handling of insertions and deletions
- The simpler file-scan approach works fine for the current annotation volume
- This becomes valuable when repos have hundreds of annotations

### Design influence

The choice to make `file` and `lines` explicit fields on `WisdomEntry` (rather
than inheriting position from a parent CodeMarker-like struct) is specifically
to enable this integration. Each wisdom entry carries its own location data,
making blame-based lookup straightforward.

---

## Implementation Phases

### Phase 1: Schema types and migration

- Add `src/schema/v3.rs` with `Annotation`, `WisdomEntry`, `WisdomCategory`
- Update `src/schema/mod.rs`: change canonical type alias to `v3::Annotation`
- Add `v2_to_v3()` in `src/schema/migrate.rs`
- Update `parse_annotation()` to detect v3 and chain v2 → v3
- Move current v2 types to historical (like v1 is today)
- Update all internal code that constructs or matches on `schema::Annotation`

**Dependencies**: None.

### Phase 2: Write path

- Rewrite `src/annotate/live.rs` with v3 `LiveInput`
- Update `check_quality()` for v3 fields (warn on empty wisdom, overly
  short content, summary that restates commit message)
- Update `src/annotate/mod.rs` (batch path) to emit v3 annotations
- Update squash synthesis to merge wisdom entries
- Update staged notes integration

**Dependencies**: Phase 1.

### Phase 3: Read path

- Implement unified `read` command in new `src/read/wisdom.rs`
- Wire up `--category`, `--anchor`, `--recent` filters
- Add `commits_since` freshness counter to output
- Deprecate (but don't yet remove) `contracts`, `decisions`, `deps`, `summary`
  subcommands — have them delegate to `read` with appropriate filters
- Update `src/cli/read.rs`

**Dependencies**: Phase 1.

### Phase 4: Agent tools and batch path

- Update agent tools in `src/agent/` to emit wisdom entries instead of
  markers and decisions
- Update batch prompt to focus on wisdom capture
- Update `src/export.rs` and `src/import.rs` for v3

**Dependencies**: Phase 1.

### Phase 5: Skills, hooks, and docs

- Update `embedded/` and `.claude/` skills for v3 write format
- Update hooks for v3 read output
- Update `CLAUDE.md` documentation
- Update web viewer for wisdom display
- Remove deprecated v2 read subcommands

**Dependencies**: Phases 2-4.

### Phase 6 (future): Git-blame integration

- Blame-based commit discovery for `read`
- Line remapping for wisdom entries
- Line-level freshness scoring

**Dependencies**: Phase 3 + real-world usage data.

---

## Dependencies and Risks

- **Phase 1 is the gate.** All other phases depend on it.
- **Phases 2 and 3 can proceed in parallel** after Phase 1.
- **Migration correctness is critical.** The v2 → v3 migration must be
  exhaustive — every v2 field must have a defined mapping (including fields
  that map to "dropped"). The rules above cover all v2 fields.
- **Backward compatibility.** Old Chronicle binaries reading a v3 note will
  fail to parse it. This is the same situation as v1 → v2 and is acceptable:
  users update their binary, old notes are migrated on read.
- **Knowledge store is orthogonal.** The `chronicle/knowledge-v1` schema
  (Feature 21 P15, already implemented) is unaffected by v3. It lives on a
  separate ref and has its own schema.
