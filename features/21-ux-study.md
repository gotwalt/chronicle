# Feature 21: AI Agent Usability Study -- Findings and Proposals

**Status**: Proposed

## Motivation

Chronicle's primary user is an AI coding assistant (Claude Code). A structured
usability study evaluated Chronicle from this perspective across four
dimensions: the **write path** (annotation workflow), the **read path** (context
retrieval), the **data model** (what's captured and what's missing), and **team
workflows** (multi-agent, sync, and scale). Four independent panelists
performed source-level analysis and surfaced concrete issues.

This document synthesizes their findings into a prioritized, actionable set of
proposals. Each proposal is grounded in specific source code observations and
assessed for feasibility within the current architecture.

---

## Critical Bugs (Fix Immediately)

### BUG-1: Three read modules hardcoded to v1, silently drop v2 annotations

**Source**: Panelist B
**Confirmed**: Yes -- source-level inspection validates the claim.

The following modules import `v1::Annotation` and deserialize with
`serde_json::from_str` instead of `schema::parse_annotation()`:

| Module | Line | Impact |
|--------|------|--------|
| `src/read/retrieve.rs` | L3-4, L28 | `read` command misses all native v2 annotations |
| `src/read/deps.rs` | L3-4, L66 | `deps` command misses all native v2 annotations |
| `src/read/history.rs` | L3-4, L88 | `history` command misses all native v2 annotations |

The correct modules (`contracts.rs`, `decisions.rs`, `summary.rs`) all use
`schema::parse_annotation()` and work with both v1 and v2. The three broken
modules silently skip v2 notes because `serde_json::from_str::<v1::Annotation>`
fails on v2 JSON and the error is caught by `Err(_) => continue`.

**Fix**: Replace `serde_json::from_str` with `schema::parse_annotation()` in
all three modules and update the type aliases. The `retrieve.rs` module also
needs structural changes since it returns `v1::RegionAnnotation` -- it needs to
either return v2 marker-based results or synthesize v1-compatible results from
v2 annotations. Since `read/mod.rs` (`execute()` at L38) and `MatchedRegion`
also use `v1::RegionAnnotation`, the migration touches the entire `read`
module's public API.

**Complexity**: Medium. The fix is straightforward for `deps.rs` and
`history.rs` (adapt to v2 fields). For `retrieve.rs`, the public types
`ReadResult` and `MatchedRegion` expose `v1::RegionAnnotation` directly, so
the fix requires either (a) a new unified result type, or (b) synthesizing
v1-compatible output from v2 annotations. Option (b) is simpler for now.

---

### BUG-2: Sync force-fetch silently overwrites local notes

**Source**: Panelist D
**Confirmed**: Yes -- `src/sync/push_fetch.rs` L118 and L162.

Two issues:

1. **`enable_sync`** (L118) configures the fetch refspec with a leading `+`,
   meaning `git fetch` will force-overwrite local `refs/notes/chronicle` with
   the remote version. Any local notes not yet pushed are silently lost.

2. **`pull_notes`** (L162) hardcodes `+{NOTES_REF}:{NOTES_REF}` in the fetch
   command, unconditionally force-overwriting.

The `NotesMergeStrategy` enum exists in `src/sync/mod.rs` (L8-29) but is never
used by any function. The merge logic is specified but not implemented.

**Fix**: Phase 1: Remove the `+` prefix from the fetch refspec so git refuses
a non-fast-forward update, surfacing conflicts instead of silently losing data.
Phase 2: Implement `git notes merge` with the configured strategy (ours,
theirs, or union).

**Complexity**: Low (phase 1), Medium (phase 2).

---

### BUG-3: Remote note counting returns local count as "approximation"

**Source**: Panelist D
**Confirmed**: Yes -- `src/sync/push_fetch.rs` L179-185.

`count_remote_notes()` ignores the `_remote` parameter and returns
`count_local_notes()`. This means `SyncStatus.remote_count` always equals
`local_count`, and `unpushed_count` is always 0.

**Fix**: Either (a) use `git ls-remote` to check if the remote notes ref
exists and count objects, or (b) after fetch, compare pre/post counts, or (c)
honestly report `remote_count: None` when the information is unavailable
instead of returning a misleading number.

**Complexity**: Low.

---

### BUG-4: `note_write()` silently overwrites without check

**Source**: Panelist D
**Confirmed**: Yes -- `src/git/cli_ops.rs` L117 uses `git notes add -f`.

The `-f` flag force-overwrites any existing note. In a multi-agent scenario,
two agents annotating the same commit will race, and the last writer wins.

**Fix**: Phase 1: Add a `note_exists()` check before writing and warn/error if
the note already exists. Phase 2: Implement optimistic concurrency with
read-check-write semantics (read existing note hash, write only if unchanged).

**Complexity**: Low (phase 1), Medium (phase 2).

---

## Tier 1: Near-Term Improvements (High Impact, Low-Medium Complexity)

### P1: Add `--summary` flag for one-command annotations

**Source**: Panelist A (P1)

**Problem**: The current live annotation path requires 3 tool invocations:
write a temp file, pipe it to the CLI, clean up. For the 80% case (summary-only
annotation), this is excessive friction.

**Proposal**: Add `--summary "text"` flag to `git chronicle annotate`.

```
git chronicle annotate HEAD --summary "Switch to exponential backoff for MQTT reconnect"
```

This bypasses stdin entirely and creates a minimal v2 annotation with just the
narrative summary.

**Implementation**: In `src/cli/annotate.rs`, add a `summary: Option<String>`
parameter. When present, construct a `LiveInput` directly and call
`handle_annotate_v2()` without reading stdin. The `LiveInput` struct already
supports this -- all fields except `commit` and `summary` are optional.

**CLI Impact**: New `--summary` flag on `annotate` subcommand.
**Schema Impact**: None.
**Complexity**: Low.

---

### P2: Add `--json` flag for inline rich annotations

**Source**: Panelist A (P2)

**Problem**: Even for rich annotations, the temp-file-to-stdin dance is
unnecessary when the agent has the JSON string ready.

**Proposal**: Add `--json '{...}'` flag.

```
git chronicle annotate --json '{"commit":"HEAD","summary":"...","decisions":[...]}'
```

**Implementation**: In `src/cli/annotate.rs`, add a `json: Option<String>`
parameter. Parse it identically to the current stdin path.

**CLI Impact**: New `--json` flag on `annotate` subcommand.
**Schema Impact**: None.
**Complexity**: Low.

---

### P3: Add `--auto` flag for zero-input annotations

**Source**: Panelist A (P4)

**Problem**: When the agent has already written a good commit message, the
annotation summary often duplicates it. A zero-input path would eliminate this
friction entirely.

**Proposal**: `git chronicle annotate HEAD --auto` derives the summary from
the commit message and auto-populates `files_changed` from the diff.

**Implementation**: In `src/cli/annotate.rs`, read `commit_info()` and
`diff()`, construct a `LiveInput` with `summary = commit_message`,
auto-populated `files_changed`, and no optional fields.

**CLI Impact**: New `--auto` flag on `annotate` subcommand.
**Schema Impact**: None.
**Complexity**: Low.

---

### P4: Composite `context` command

**Source**: Panelist B (P2)

**Problem**: Before modifying code, an agent ideally checks contracts,
decisions, history, and follow-ups. Currently this requires 4 separate CLI
calls (4 subprocess spawns, 4 JSON parses). No agent will do this reliably.

**Proposal**: `git chronicle context <file> [--anchor <name>]` returns a
single JSON object combining:
- Active contracts (from `contracts.rs`)
- Applicable decisions (from `decisions.rs`)
- Recent history (from `history.rs`, limited to last 3)
- Open follow-ups (from narrative.follow_up across recent annotations)

**Implementation**: New `src/cli/context.rs` command that calls the existing
query functions internally and assembles a composite result. This is
straightforward because `contracts.rs`, `decisions.rs`, and `summary.rs`
already use `parse_annotation()` correctly.

Note: `history.rs` must be fixed (BUG-1) before it can be included.

**CLI Impact**: New `context` subcommand.
**Schema Impact**: New output schema `chronicle-context/v1`.
**Complexity**: Medium (depends on BUG-1 fix).

---

### P5: Compact output format for AI consumers

**Source**: Panelist B (P4)

**Problem**: Current output includes schema metadata, query echoes, and stats
that consume tokens without providing value to AI consumers. Example: the
`deps` output includes `schema`, `query`, and `stats` fields that are
never actionable.

**Proposal**: Add `--format compact` (or `--compact`) flag to read commands
that strips metadata and returns only the payload (contracts, decisions,
timeline entries, etc.).

**Implementation**: Each output struct already has the payload fields separate
from metadata. Add a format flag to the CLI layer that controls serialization.

**CLI Impact**: New `--format` flag on read subcommands.
**Schema Impact**: None.
**Complexity**: Low.

---

### P6: `git chronicle status` orientation command

**Source**: Panelists B (P6) and D (P4)

**Problem**: No quick way to understand annotation landscape for a repo.
Agents starting a session have no orientation.

**Proposal**: `git chronicle status` shows:
- Total annotations (local count)
- Coverage: annotated commits / total commits (last N)
- Sync status (if configured)
- Recent unannotated commits
- Files with most annotations

**Implementation**: Combines `list_annotated_commits()` with a commit log scan.
The `get_sync_status()` function already exists (bugs notwithstanding).

**CLI Impact**: New `status` subcommand.
**Schema Impact**: New output schema `chronicle-status/v1`.
**Complexity**: Low.

---

### P7: Improve pre-edit hook

**Source**: Panelists A and B

**Problem**: The pre-edit hook (`read-context-hint.sh`) suggests `read`
(which is the v1-stuck retrieve path) instead of `contracts`. It's a passive
TIP that's easily ignored. It fires on every edit, including non-source files
matched by the case statement.

**Proposal**:
1. Change suggested command from `read` to `contracts` (highest-value for
   pre-edit context).
2. Add file-level deduplication so the hint fires once per file per session,
   not on every edit.
3. After BUG-1 is fixed and P4 is implemented, change to suggest `context`.

**Implementation**: Update `.claude/hooks/pre-tool-use/read-context-hint.sh`
and `embedded/hooks/chronicle-read-context-hint.sh`. For deduplication, use a
temp file to track which files have been hinted this session.

**Complexity**: Low.

---

### P8: Post-annotation quality feedback

**Source**: Panelist A (P10)

**Problem**: After writing an annotation, the agent gets `{"success": true}`
but no feedback on what was captured vs. what could be improved.

The `LiveResult` struct (in `src/annotate/live.rs` L214-221) already returns
`warnings` and `anchor_resolutions`, and the `check_quality()` function
(L244-252) exists but only checks summary length.

**Proposal**: Expand `check_quality()` to detect:
- Missing motivation (for non-trivial diffs touching >3 files)
- Missing decisions (when diff adds new public API)
- Duplicate summary (when summary matches commit message verbatim)
- No markers on complex changes

**Implementation**: Enrich `check_quality()` with access to the diff and
commit message. The `handle_annotate_v2` function already has the diff
(via `files_changed`).

**CLI Impact**: None (existing output format).
**Schema Impact**: None.
**Complexity**: Low.

---

## Tier 2: Medium-Term Improvements (Medium Impact, Medium Complexity)

### P9: Extended MarkerKind variants

**Source**: Panelist C (B)

**Problem**: Current `MarkerKind` has 4 variants: `Contract`, `Hazard`,
`Dependency`, `Unstable`. Missing high-value kinds: `Security`, `Performance`,
`Deprecated`, `TechDebt`, `TestCoverage`.

**Proposal**: Add new variants to `MarkerKind` in `src/schema/v2.rs`.

**Schema Impact**: This is backward-compatible -- new variants are additive
to the tagged enum. Old readers that encounter unknown variants will fail
to parse, so this requires a minor version signal (e.g., `chronicle/v2.1`)
or consumers must use `#[serde(other)]` handling. **Recommendation**: add the
variants without a schema version bump, since the `v2` schema uses
`#[serde(tag = "type")]` and unknown types already produce parse errors that
are caught by `Err(_) => continue` in all read paths. New variants will be
invisible to old readers but won't corrupt data.

**Complexity**: Low (schema change), Medium (updating all read paths to
handle new variants usefully).

---

### P10: Author identity in provenance

**Source**: Panelist D (P1)

**Problem**: `v2::Provenance` tracks `source` (live/batch/backfill/etc.) but
not *who* created the annotation. In multi-agent and human+AI workflows,
provenance without identity is incomplete.

**Proposal**: Add optional `author` field to `v2::Provenance`:

```rust
pub struct Provenance {
    pub source: ProvenanceSource,
    pub author: Option<String>,  // NEW: "claude-code", "human:alice", etc.
    pub derived_from: Vec<String>,
    pub notes: Option<String>,
}
```

**Schema Impact**: Backward-compatible addition (optional field with
`#[serde(skip_serializing_if = "Option::is_none")]`). Existing annotations
without the field will deserialize with `author: None`.

**Implementation**: Auto-populate from git config `user.name` or a new
`chronicle.author` config key.

**Complexity**: Low.

---

### P11: Staleness detection

**Source**: Panelists C (D) and D (P2)

**Problem**: Annotations are written once and never updated. Code evolves but
annotations don't. There's no way to detect stale annotations.

**Proposal**: Compute freshness at read time by comparing the annotation's
commit timestamp with the most recent commit touching the same file+anchor.
Add a `freshness` field to read output:

```json
{
  "freshness": {
    "annotation_commit": "abc123",
    "latest_commit_touching_file": "def456",
    "commits_since_annotation": 5,
    "stale": true
  }
}
```

**Implementation**: At read time, after retrieving an annotation, check
`log_for_file()` to see how many commits have touched the file since the
annotation's commit. This is a computed view, not stored data.

Add `git chronicle doctor --staleness` to report stale annotations across the
repo.

**CLI Impact**: New `--staleness` flag on `doctor`.
**Schema Impact**: None (computed at read time, not stored).
**Complexity**: Medium.

---

### P12: Session-level annotations

**Source**: Panelist D (P6)

**Problem**: Rich session context (user request, files explored, approaches
tried, conversation trajectory) is completely lost when a session ends. Each
annotation captures a single commit's context but not the session narrative
that led to a series of commits.

**Proposal**: A new `session` annotation type that links multiple commits
and captures the session narrative:

```json
{
  "schema": "chronicle/session-v1",
  "session_id": "uuid",
  "summary": "Implemented exponential backoff for MQTT reconnect",
  "commits": ["abc123", "def456", "789abc"],
  "context": {
    "user_request": "Make reconnect more reliable",
    "files_explored": ["src/mqtt/mod.rs", "src/mqtt/reconnect.rs"],
    "approaches_tried": ["Linear backoff (too aggressive)", "Jittered backoff (landed on)"]
  }
}
```

**Implementation**: This is a separate notes ref (`refs/notes/chronicle-sessions`)
to avoid conflicting with per-commit annotations. New CLI:
`git chronicle session start|add-commit|finish`.

**Complexity**: High. Requires new schema, new storage, new CLI commands, and
agent-side integration to track sessions.

---

### P13: Materialized per-file view

**Source**: Panelist C (A)

**Problem**: The commit is the right write unit but the wrong read unit.
Agents think in terms of files, not commits. Every read query currently does a
linear scan of commits.

**Proposal**: A per-file view that aggregates annotations across commits:

```
git chronicle file-view src/foo.rs
```

Returns: active contracts, current decisions, latest intent per anchor, open
follow-ups, dependency graph -- all for a single file, deduplicated and with
freshness scores.

**Implementation**: This is largely what `summary.rs` does but extended to
include decisions and follow-ups. The existing `build_summary()` function
already handles deduplication and newest-first ordering. Extend it to include
data from `contracts.rs` and `decisions.rs`.

This overlaps significantly with P4 (composite `context` command). Consider
making P4 the implementation and P13 the long-term evolution.

**Complexity**: Medium.

---

## Tier 3: Repo-Level and Session Knowledge

### P15: Repo-level knowledge store

**Source**: Panelist C (C)

**Problem**: Conventions, module boundaries, anti-patterns, and test topology
have no home in the current per-commit model. These are repo-wide concerns
that transcend individual commits. An AI agent starting a new session has no
way to learn "in this repo, we never use raw SQL" or "module X is being
deprecated in favor of Y" unless it happens to find a commit annotation that
mentions it. This is the gap between per-commit annotations and the
institutional knowledge that makes a codebase navigable.

**Proposal**: A dedicated `refs/notes/chronicle-knowledge` ref that stores
repo-level knowledge, keyed to a well-known blob (e.g., the root tree or a
synthetic empty blob) so it's not tied to any single commit:

```json
{
  "schema": "chronicle/knowledge-v1",
  "conventions": [
    {
      "scope": "src/provider/",
      "rule": "All LLM calls go through the LlmProvider trait. No direct HTTP to model APIs.",
      "decided_in": "abc123",
      "stability": "permanent"
    }
  ],
  "module_boundaries": [
    {
      "module": "src/git/",
      "owns": "All git operations",
      "boundary": "Nothing outside this module should shell out to git directly",
      "decided_in": "def456"
    }
  ],
  "anti_patterns": [
    {
      "pattern": "Deserializing annotations with serde_json::from_str directly",
      "instead": "Always use schema::parse_annotation() for version-aware deserialization",
      "learned_from": "BUG-1"
    }
  ],
  "test_topology": {
    "integration_tests_require": "Real .git directory (not worktree gitlink)",
    "unit_tests": "cargo test --lib (fast, no git fixtures)"
  }
}
```

**Key design questions**:

1. **Storage**: A single blob attached via `git notes add` to a well-known
   ref (e.g., `refs/notes/chronicle-knowledge` on the repo's initial commit).
   This keeps it in the git notes ecosystem and syncable via the existing
   push/fetch machinery.

2. **Read integration**: `git chronicle context <file>` (P4) should
   automatically include applicable knowledge entries filtered by scope.
   `git chronicle status` (P6) should report knowledge entry count.

3. **Write surface**: `git chronicle knowledge add --convention "..."
   --scope src/foo/` and `git chronicle knowledge list`. Agents can write
   knowledge entries as they discover patterns; humans can curate.

4. **Relationship to CLAUDE.md**: This is complementary, not competing.
   CLAUDE.md is agent-specific instructions; the knowledge store is
   codebase-level institutional memory that's queryable, scopeable, and
   version-tracked in git.

**Implementation**: New `src/knowledge/` module with `KnowledgeStore` type.
Schema in `src/schema/knowledge.rs`. CLI commands in `src/cli/knowledge.rs`.
The store is a single JSON document read/written atomically.

**Complexity**: Medium-High. New schema, new storage, new CLI commands, but
the storage mechanism (git notes on a fixed ref) is well-understood and the
read integration piggybacks on P4 and P6.

---

### P17: Incremental context accumulation

**Source**: Panelist A (P3)

**Problem**: Context is freshest during work, not after commit. The current
model only captures context at commit time, losing the exploration and
decision-making that happened during development.

**Proposal**: `git chronicle note "Tried X, didn't work because Y"` appends
timestamped notes to a staging area. At commit time, staged notes are
incorporated into the annotation automatically.

**Implementation**: Store staged notes in `.git/chronicle/staged-notes.json`.
The `handle_annotate_v2()` function reads and incorporates them, then clears
the staging area.

**Complexity**: Medium.

---

## v3 Schema Considerations

The proposals above were designed to be backward-compatible with `chronicle/v2`
where possible. Here's the assessment:

### No schema version bump needed

These changes are additive (optional fields, new variants) and existing parsers
will either handle them or gracefully ignore them:

- **P9** (extended MarkerKind): New tagged enum variants. Old readers skip
  unknown variants via `Err(_) => continue`.
- **P10** (author in provenance): Optional field, skipped if missing.
- **P11** (freshness): Computed at read time, not stored.

### Separate schemas (not v3)

These features introduce new data that lives outside the per-commit annotation
model. They get their own schema identifiers and storage refs rather than
inflating the per-commit annotation:

- **P15** (repo-level knowledge): `chronicle/knowledge-v1` on
  `refs/notes/chronicle-knowledge`. Repo-wide conventions, boundaries, and
  anti-patterns. Not tied to any single commit.
- **P12** (session annotations): `chronicle/session-v1` on
  `refs/notes/chronicle-sessions`. Multi-commit session narratives.

### Recommendation

Stay on `chronicle/v2` for per-commit annotations. The most impactful
proposals (BUG fixes, P1-P8) require no schema changes at all. Introduce
`chronicle/knowledge-v1` as the next schema work (P15), and
`chronicle/session-v1` (P12) when session tracking is ready. These are
parallel schemas, not successors to v2.

---

## Implementation Phases

### Phase 0: Critical Bug Fixes (immediate)

1. **BUG-1**: Fix v1/v2 schema split in `retrieve.rs`, `deps.rs`, `history.rs`
2. **BUG-2**: Remove `+` prefix from sync fetch refspec
3. **BUG-3**: Fix remote note counting (return `None` instead of fake count)
4. **BUG-4**: Add `note_exists()` check before force-write

**Dependencies**: None.
**Estimated scope**: ~4 source files, ~200 lines changed.

### Phase 1: Write Path Friction (near-term)

5. **P1**: `--summary` flag
6. **P2**: `--json` flag
7. **P3**: `--auto` flag
8. **P7**: Improve pre-edit hook
9. **P8**: Post-annotation quality feedback

**Dependencies**: None (can proceed in parallel with Phase 0).
**Estimated scope**: ~3 source files, ~150 lines added.

### Phase 2: Read Path Improvements (near-term, after Phase 0)

10. **P4**: Composite `context` command (depends on BUG-1 fix)
11. **P5**: Compact output format
12. **P6**: `status` command

**Dependencies**: BUG-1 must be fixed before P4.
**Estimated scope**: ~4 new files, ~400 lines.

### Phase 3: Data Model Enrichment (medium-term)

13. **P9**: Extended MarkerKind
14. **P10**: Author identity in provenance
15. **P11**: Staleness detection

**Dependencies**: None, but benefits from Phase 2.
**Estimated scope**: ~6 files modified, ~300 lines.

### Phase 4: Repo-Level Knowledge (medium-term, after Phase 2)

16. **P15**: Repo-level knowledge store (`chronicle/knowledge-v1`)
17. **P17**: Incremental context accumulation

**Dependencies**: Benefits from P4 (`context` command) for read integration.
P15 is the highest-value new capability — it fills the gap between per-commit
annotations and the institutional knowledge that makes a codebase navigable.
**Estimated scope**: ~4 new files, ~500 lines.

### Phase 5: Session and File Views (longer-term)

18. **P12**: Session-level annotations
19. **P13**: Materialized per-file view

**Dependencies**: P12 requires agent-side integration to track session
boundaries. P13 overlaps with P4 and is its natural evolution.

### Future: Scale Optimization (when needed)

- **P14** (SQLite annotation index): Valuable when repos reach ~1000+
  annotations and linear scans become noticeable. Not needed until Chronicle
  has real adoption pushing against the current architecture. The right time
  to build this is when users report performance issues, not before.

### Removed: MCP Server (P16)

The MCP server proposal has been dropped. The skill+CLI integration surface
is the chosen path forward (see Feature 20, Key Decision #2). Skills provide
the workflow, the CLI provides self-documenting data contracts via
`git chronicle schema`. The incomplete MCP skeleton in `src/mcp/` should be
cleaned up rather than completed.

---

## Appendix: Validated Claims Summary

| Claim | Source | Verified | Notes |
|-------|--------|----------|-------|
| 3 read modules use `serde_json::from_str` on v1 | Panelist B | Yes | `retrieve.rs:28`, `deps.rs:66`, `history.rs:88` |
| `contracts.rs` uses `parse_annotation()` | Panelist B | Yes | `contracts.rs:76` |
| `decisions.rs` uses `parse_annotation()` | Panelist B | Yes | `decisions.rs:71` |
| `summary.rs` uses `parse_annotation()` | Panelist B | Yes | `summary.rs:91` |
| Sync fetch uses `+` force prefix | Panelist D | Yes | `push_fetch.rs:118,162` |
| `NotesMergeStrategy` exists but unused | Panelist D | Yes | `sync/mod.rs:8-29` |
| `count_remote_notes` returns local count | Panelist D | Yes | `push_fetch.rs:179-185` |
| `note_write` uses `-f` flag unconditionally | Panelist D | Yes | `cli_ops.rs:117` |
| Live path is stdin-only | Panelist A | Yes | `cli/annotate.rs:162` reads stdin |
| No `--summary` or `--json` CLI flags | Panelist A | Yes | Only `--live`, `--squash-sources`, `--amend-source` |
| Hook suggests `read` not `contracts` | Panelist B | Yes | `read-context-hint.sh:23` |
| Hook fires on every edit (no dedup) | Panelist B | Yes | No state tracking in hook |
| `LiveResult` has warnings field | Panelist A | Yes | `live.rs:219` |
| `check_quality` only checks summary length | Panelist A | Yes | `live.rs:244-252` |
