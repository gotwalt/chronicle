# Feature 08: Advanced Queries

## Dependency Inversion, Timeline Reconstruction, and Condensed Views

---

## 1. Overview

Feature 08 builds three specialized query commands on top of the read pipeline (Feature 07):

- **`ultragit deps`** — Dependency inversion: "what other code depends on assumptions about this code?" Answers the highest-value question for preventing regressions.
- **`ultragit history`** — Timeline reconstruction: the reasoning chain across commits that touched a code region. `git log` for intent.
- **`ultragit summary`** — Condensed view: most recent annotation per AST unit, trimmed to intent + constraints + risk_notes. Fast orientation on an unfamiliar module.

All three commands share infrastructure with the read pipeline: blame integration, note fetching, and confidence scoring. They differ in traversal strategy and output shape.

Feature 08 also specifies the **reverse index** (v1.1) that transforms `deps` from a linear scan to an O(1) lookup.

---

## 2. Dependencies

| Feature | What it provides |
|---------|-----------------|
| 07 Read Pipeline | Blame, note fetching, region filtering, confidence scoring, output assembly |
| 02 Git Operations | Notes read/write, blame, ref management |
| 03 AST Parsing | Anchor resolution for scope specification |

Feature 08 extends Feature 07. It reuses `blame_scope()`, `fetch_notes()`, `compute_confidence()`, and the output serialization infrastructure.

---

## 3. Public API

### 3.1 `ultragit deps`

```
ultragit deps [OPTIONS] <PATH> [<ANCHOR>]
```

Returns annotations from **other code** whose `semantic_dependencies` reference the specified file+anchor. This is the inverse of a normal read — instead of "what does this code depend on?", it answers "what depends on this code?"

**Arguments:** Same as `ultragit read` for PATH and ANCHOR.

**Options:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--format` | `json\|pretty` | `json` | Output format |
| `--max-results` | `u32` | `50` | Cap on returned dependency entries |
| `--scan-limit` | `u32` | `500` | Max commits to scan in v1 linear mode |

**Output schema:**

```json
{
  "$schema": "ultragit-deps/v1",
  "query": {
    "file": "src/tls/session.rs",
    "anchor": "TlsSessionCache::max_sessions"
  },
  "dependents": [
    {
      "file": "src/mqtt/reconnect.rs",
      "anchor": "ReconnectHandler::attempt",
      "nature": "assumes max_sessions is 4; will leak file descriptors if increased",
      "commit": "<sha>",
      "timestamp": "<iso8601>",
      "confidence": 0.85,
      "context_level": "enhanced"
    }
  ],
  "stats": {
    "commits_scanned": 500,
    "dependencies_found": 3,
    "scan_method": "linear"
  }
}
```

### 3.2 `ultragit history`

```
ultragit history [OPTIONS] <PATH> [<ANCHOR>]
```

Returns the chronological chain of annotations across commits that touched the specified code region. Follows `related_annotations` links to include connected reasoning from commits that didn't directly modify the code.

**Arguments:** Same as `ultragit read` for PATH and ANCHOR.

**Options:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--limit` | `u32` | `10` | Max annotation entries to return |
| `--format` | `json\|pretty` | `json` | Output format |
| `--follow-related` | `bool` | `true` | Follow related_annotations links |

**Output schema:**

```json
{
  "$schema": "ultragit-history/v1",
  "query": {
    "file": "src/mqtt/client.rs",
    "anchor": "MqttClient::connect"
  },
  "timeline": [
    {
      "commit": "<sha>",
      "timestamp": "<iso8601>",
      "commit_message": "initial MQTT client implementation",
      "context_level": "enhanced",
      "provenance": "initial",
      "intent": "Establishes mTLS connection to the cloud MQTT broker...",
      "reasoning": "Chose mutual TLS over token auth because...",
      "constraints": ["Requires TLS session cache to hold <= 4 sessions"],
      "risk_notes": "Broker silently drops idle connections after 30min",
      "related_context": [
        {
          "commit": "<sha>",
          "anchor": "TlsSessionCache::new",
          "relationship": "depends on session cache size limit",
          "intent": "Session cache bounded to 4 entries..."
        }
      ]
    },
    {
      "commit": "<sha>",
      "timestamp": "<iso8601>",
      "commit_message": "add heartbeat keepalive for idle detection",
      "context_level": "enhanced",
      "provenance": "initial",
      "intent": "Application-level heartbeats detect broker disconnects...",
      "reasoning": "Considered MQTT keep-alive but BSP TCP stack bug...",
      "constraints": ["Heartbeat interval must be < 30min"],
      "risk_notes": null,
      "related_context": []
    }
  ],
  "stats": {
    "commits_in_blame": 5,
    "annotations_found": 3,
    "related_followed": 2
  }
}
```

### 3.3 `ultragit summary`

```
ultragit summary [OPTIONS] <PATH> [<ANCHOR>]
```

Returns a condensed view: the most recent annotation per AST unit in the file (or for a single anchor), with only `intent`, `constraints`, and `risk_notes` fields. Designed for broad orientation.

**Arguments:** Same as `ultragit read` for PATH and ANCHOR. PATH can also be a directory for module-level summaries.

**Options:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--format` | `json\|pretty` | `json` | Output format |

**Output schema:**

```json
{
  "$schema": "ultragit-summary/v1",
  "query": {
    "file": "src/mqtt/client.rs"
  },
  "units": [
    {
      "anchor": {
        "type": "struct",
        "name": "MqttClient",
        "signature": "pub struct MqttClient"
      },
      "lines": { "start": 12, "end": 25 },
      "intent": "Core MQTT client managing broker connections and message dispatch",
      "constraints": ["Single-threaded: must not be shared across threads"],
      "risk_notes": null,
      "last_modified": "<iso8601>",
      "confidence": 0.91
    },
    {
      "anchor": {
        "type": "method",
        "name": "MqttClient::connect",
        "signature": "pub fn connect(&mut self, config: &MqttConfig) -> Result<()>"
      },
      "lines": { "start": 42, "end": 67 },
      "intent": "Establishes mTLS connection to the cloud MQTT broker",
      "constraints": [
        "Requires TLS session cache to hold <= 4 sessions",
        "Message queue must be drained before reconnecting"
      ],
      "risk_notes": "Broker silently drops idle connections after 30min",
      "last_modified": "<iso8601>",
      "confidence": 0.88
    }
  ],
  "stats": {
    "ast_units_in_file": 12,
    "annotated_units": 8,
    "unannotated_units": 4
  }
}
```

### 3.4 Core Library Types

```rust
/// Dependency inversion query result.
pub struct DepsOutput {
    pub schema: String,             // "ultragit-deps/v1"
    pub query: QueryEcho,
    pub dependents: Vec<DependentEntry>,
    pub stats: DepsStats,
}

pub struct DependentEntry {
    pub file: String,
    pub anchor: String,
    pub nature: String,
    pub commit: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub confidence: f64,
    pub context_level: String,
}

pub struct DepsStats {
    pub commits_scanned: u32,
    pub dependencies_found: u32,
    pub scan_method: String, // "linear" or "reverse_index"
}

/// Timeline entry for history queries.
pub struct TimelineEntry {
    pub commit: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub commit_message: String,
    pub context_level: String,
    pub provenance: String,
    pub intent: String,
    pub reasoning: Option<String>,
    pub constraints: Vec<String>,
    pub risk_notes: Option<String>,
    pub related_context: Vec<RelatedContext>,
}

pub struct RelatedContext {
    pub commit: String,
    pub anchor: String,
    pub relationship: String,
    pub intent: Option<String>,
}

/// Summary unit for condensed views.
pub struct SummaryUnit {
    pub anchor: ast::OutlineEntry,
    pub lines: LineRange,
    pub intent: String,
    pub constraints: Vec<String>,
    pub risk_notes: Option<String>,
    pub last_modified: chrono::DateTime<chrono::Utc>,
    pub confidence: f64,
}
```

---

## 4. Internal Design

### 4.1 `deps` — Dependency Inversion Query

The `deps` query inverts the direction of `semantic_dependencies`. Instead of asking "what does this function depend on?" (which is in the annotation for this function's commit), it asks "what depends on this function?" (which is scattered across annotations for other functions' commits).

**v1: Linear scan.**

```
1. Resolve the query scope (file + anchor) to a canonical identifier:
   "src/tls/session.rs:TlsSessionCache::max_sessions"

2. Walk refs/notes/ultragit, reading the most recent `scan_limit` annotated commits.

3. For each annotation, for each region, for each semantic_dependency:
   - Normalize the dependency's file + anchor to the same format.
   - If it matches the query target, record a DependentEntry.

4. Deduplicate: if the same file+anchor appears as a dependent from
   multiple commits, keep the most recent.

5. Sort by confidence descending.

6. Apply --max-results cap.
```

**Walking the notes ref.** The notes under `refs/notes/ultragit` are stored as a tree of git objects. We enumerate them by:
1. Resolving `refs/notes/ultragit` to a tree.
2. Walking the tree entries (each entry's name encodes a commit SHA).
3. Reading the blob for each entry.

Alternatively, use `git log --format=%H refs/notes/ultragit` to list all annotated commits, then read notes for the most recent N.

**Performance target:** <2s for a repository with 500 annotated commits. Each note read is a git object lookup (~1-2ms). 500 reads = ~1s. JSON parsing and string matching add overhead but should stay under 2s total.

**Matching logic.** A dependency reference like `{"file": "src/tls/session.rs", "anchor": "max_sessions"}` should match the query `src/tls/session.rs:TlsSessionCache::max_sessions` via the same unqualified matching used in the read pipeline — `max_sessions` matches `TlsSessionCache::max_sessions`.

### 4.2 `deps` — Reverse Index (v1.1)

The linear scan doesn't scale past ~1000 annotated commits. The reverse index makes `deps` O(1) + O(k) where k is the number of dependents.

**Storage.** A separate notes ref: `refs/notes/ultragit-deps`. This ref contains a single JSON document (or a set of documents keyed by file path) that maps dependency targets to the commits that depend on them:

```json
{
  "$schema": "ultragit-deps-index/v1",
  "entries": {
    "src/tls/session.rs:TlsSessionCache::max_sessions": [
      {
        "commit": "<sha>",
        "file": "src/mqtt/reconnect.rs",
        "anchor": "ReconnectHandler::attempt",
        "nature": "assumes max_sessions is 4"
      }
    ],
    "src/mqtt/client.rs:MqttClient::connect": [
      {
        "commit": "<sha>",
        "file": "src/mqtt/reconnect.rs",
        "anchor": "ReconnectHandler::attempt",
        "nature": "assumes connect() is idempotent"
      }
    ]
  }
}
```

**Update at write time.** The writing agent (Feature 05), after producing an annotation, extracts all `semantic_dependencies` from the annotation's regions and updates the reverse index:

```rust
pub fn update_reverse_index(
    repo: &git::Repository,
    annotation: &schema::Annotation,
) -> Result<()> {
    let mut index = read_reverse_index(repo)?;

    for region in &annotation.regions {
        for dep in &region.semantic_dependencies {
            let key = format!("{}:{}", dep.file, dep.anchor);
            let entry = ReverseIndexEntry {
                commit: annotation.commit.clone(),
                file: region.file.clone(),
                anchor: region.ast_anchor.name.clone(),
                nature: dep.nature.clone(),
            };
            index.entries
                .entry(key)
                .or_default()
                .push(entry);
        }
    }

    write_reverse_index(repo, &index)?;
    Ok(())
}
```

**Query with reverse index:**

```
1. Read the reverse index from refs/notes/ultragit-deps.
2. Look up the canonical key for the queried file+anchor.
3. Return the entries. Each entry has the commit SHA, so we can
   fetch the full annotation if the caller wants more detail.
```

This is O(1) for the lookup plus O(k) for serializing k entries. No scanning.

**Migration.** When the reverse index doesn't exist (fresh install or upgrade from v1), `deps` falls back to linear scan. A `ultragit index build` command (or automatic background task) can build the index from existing annotations.

```rust
pub fn build_reverse_index(repo: &git::Repository) -> Result<()> {
    let mut index = ReverseIndex::new();
    for (sha, annotation) in walk_all_annotations(repo)? {
        // Same logic as update_reverse_index but for all annotations
        for region in &annotation.regions {
            for dep in &region.semantic_dependencies {
                let key = format!("{}:{}", dep.file, dep.anchor);
                index.entries
                    .entry(key)
                    .or_default()
                    .push(ReverseIndexEntry { /* ... */ });
            }
        }
    }
    write_reverse_index(repo, &index)?;
    Ok(())
}
```

### 4.3 `history` — Timeline Reconstruction

The history query reconstructs the reasoning chain for a code region across commits.

**Algorithm:**

```
1. Resolve scope (file + anchor → line range).

2. Run git blame on the resolved lines.
   → Set of (commit_sha, line_range) pairs.

3. For each commit SHA, fetch the annotation.
   Filter to the matching region (same logic as read pipeline Stage 4).

4. Sort matches chronologically (oldest first).
   → This is the direct modification timeline.

5. If --follow-related is true (default):
   For each annotation in the timeline, inspect related_annotations.
   For each related reference:
     - Fetch the referenced commit's annotation.
     - Extract the referenced region.
     - Insert into the timeline at the correct chronological position,
       marked as "related" rather than "direct".

6. Apply --limit cap (keep the N most recent entries).

7. Assemble TimelineEntry for each match:
   - commit, timestamp, commit_message
   - intent, reasoning, constraints, risk_notes
   - related_context (resolved inline)
```

**Difference from `ultragit read`:** The read pipeline returns the current state — the best available annotations for the code as it exists now. The history query returns the temporal sequence — how the reasoning evolved over time. Read is for "what should I know before modifying?". History is for "how did this code get here?"

**Performance target:** <1s. The blame provides the commit set; note fetches are bounded by `--limit`. Related annotations add some overhead but are bounded by the timeline length.

### 4.4 `summary` — Condensed View

The summary query provides a structural overview of a file or anchor.

**Algorithm:**

```
1. Parse the file with tree-sitter to extract the AST outline:
   list of semantic units (functions, structs, methods, etc.)
   with name, type, signature, and line range.

2. For each AST unit:
   a. Run git blame on the unit's line range.
   b. Fetch annotations from blamed commits.
   c. Filter to matching regions.
   d. Select the most recent annotation.
   e. Extract only: intent, constraints, risk_notes.
   f. Compute confidence score.

3. Assemble SummaryUnit for each AST unit that has an annotation.
   Report unannotated units in stats.

4. If PATH is a directory, recurse into all files in the directory:
   - Use the same language detection as Feature 03.
   - Skip files without tree-sitter grammar support.
   - Aggregate all SummaryUnits across files.
```

**Performance target:** <500ms for a single file. The operation is blame + note fetch for each AST unit, but blame results overlap heavily (the same commit often touches multiple units), so the note cache from a single blame pass can be reused.

**Optimization:** Run blame once for the entire file, partition blame results by AST unit line ranges, then fetch notes once for all unique SHAs.

```rust
fn summary_pipeline(
    repo: &git::Repository,
    ast: &ast::AstParser,
    path: &Path,
) -> Result<Vec<SummaryUnit>> {
    let outline = ast.outline(path)?;
    let blame_entries = blame_scope(repo, &ResolvedScope::whole_file(path))?;

    // Partition blame entries by AST unit
    let unit_blames = partition_by_units(&outline, &blame_entries);

    // Fetch all unique notes once
    let all_shas: Vec<_> = blame_entries.iter()
        .map(|e| &e.commit_sha)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let notes = fetch_notes(repo, &all_shas, &NoteFilters::default())?;

    // For each unit, find matching regions from blamed commits
    let mut units = Vec::new();
    for unit in &outline {
        let unit_shas = &unit_blames[&unit.name];
        let matching = filter_regions_for_unit(&notes, unit, unit_shas);
        if let Some(most_recent) = matching.into_iter()
            .max_by_key(|m| m.annotation_timestamp) {
            units.push(SummaryUnit {
                anchor: unit.clone(),
                lines: unit.line_range,
                intent: most_recent.region.intent.clone(),
                constraints: extract_constraint_texts(&most_recent.region.constraints),
                risk_notes: most_recent.region.risk_notes.clone(),
                last_modified: most_recent.annotation_timestamp,
                confidence: compute_confidence(&score_factors(&most_recent)),
            });
        }
    }
    Ok(units)
}
```

### 4.5 Shared Infrastructure

All three commands share:

| Component | Module | Used by |
|-----------|--------|---------|
| Scope resolution | `read/mod.rs` | deps, history, summary |
| Blame | `read/retrieve.rs` | deps (indirectly), history, summary |
| Note fetching | `read/retrieve.rs` | deps, history, summary |
| Region filtering | `read/retrieve.rs` | deps (matching), history, summary |
| Confidence scoring | `read/scoring.rs` | deps, history (optional), summary |
| Anchor resolution | `ast/anchor.rs` | deps, history, summary |
| Output serialization | `schema/output.rs` | deps, history, summary |

The `deps` command doesn't use blame on the queried code directly — it scans other annotations' dependencies. But it reuses the anchor resolution and matching logic.

---

## 5. Error Handling

| Failure Mode | Behavior |
|--------------|----------|
| File not found | Same as Feature 07: error with suggestion |
| Anchor not found | Same as Feature 07: error with available anchors |
| No annotations found for `deps` | Return empty `dependents` with `stats.dependencies_found: 0` |
| No annotations found for `history` | Return empty `timeline` with stats |
| No annotations found for `summary` | Return `units` with all AST units listed as unannotated |
| Reverse index missing (v1.1) | Fall back to linear scan, log info message |
| Reverse index corrupted (v1.1) | Fall back to linear scan, log warning |
| Directory path for `summary` | Valid: recurse into files. Error if directory doesn't exist. |
| Very large repo for `deps` linear scan | Respect `--scan-limit`, report partial coverage in stats |

---

## 6. Configuration

```ini
[ultragit]
    # v1 linear scan limit for deps queries
    depsScanLimit = 500

    # Use reverse index when available (v1.1)
    depsUseReverseIndex = true

    # Default limit for history queries
    historyDefaultLimit = 10
```

```toml
# .ultragit-config.toml
[ultragit.queries]
deps_scan_limit = 500
deps_use_reverse_index = true
history_default_limit = 10
```

---

## 7. Implementation Steps

### Step 1: `deps` v1 — Linear Scan
**Scope:** Implement `ultragit deps` in `src/read/deps.rs` and `src/cli/deps.rs`. Walk annotated commits via the notes ref, scan `semantic_dependencies` for matches, deduplicate, score, output. Reuse scope resolution from Feature 07. Tests: basic dependency found, no dependencies, scan limit, unqualified anchor matching.

### Step 2: `history` — Timeline Reconstruction
**Scope:** Implement `ultragit history` in `src/read/history.rs` and `src/cli/history.rs`. Blame-based timeline assembly. Related annotation following. Chronological sort. `--limit` cap. Tests: single-commit history, multi-commit timeline, related annotations included, `--limit` respected.

### Step 3: `summary` — Condensed View
**Scope:** Implement `ultragit summary` in `src/read/summary.rs` and `src/cli/summary.rs`. AST outline extraction. Per-unit blame and note lookup. Most-recent-annotation selection. Field trimming to intent/constraints/risk_notes. Tests: single file, directory recursion, unannotated units reported.

### Step 4: Summary Optimization — Shared Blame Pass
**Scope:** Optimize the summary pipeline to run blame once for the entire file and partition results by AST unit. Benchmark before/after on a repo with 50+ functions in a single file.

### Step 5: `deps` v1.1 — Reverse Index Write Path
**Scope:** Implement `update_reverse_index()` called by the writing agent after annotation. Store under `refs/notes/ultragit-deps`. JSON schema for the index. Tests: index updated after annotation, multiple deps tracked, duplicate handling.

### Step 6: `deps` v1.1 — Reverse Index Read Path
**Scope:** Modify `deps` to check for the reverse index first. If present, use O(1) lookup. If missing, fall back to linear scan. Report `scan_method` in stats. Tests: lookup with index, fallback without index, index build from existing annotations.

### Step 7: `ultragit index build` Command
**Scope:** Implement `ultragit index build` in `src/cli/` that walks all existing annotations and builds the reverse index from scratch. Progress reporting. Tests: build on empty repo, build with existing annotations, idempotent rebuild.

---

## 8. Test Plan

### Unit Tests

**deps v1:**
- Annotation with `semantic_dependency` referencing queried file+anchor: found.
- No dependencies: empty result.
- Multiple dependents from different commits: all returned.
- Same dependent from multiple commits: deduplicated to most recent.
- Unqualified anchor match: `max_sessions` matches `TlsSessionCache::max_sessions`.
- `--scan-limit` respected: scan stops after N commits.
- `--max-results` cap applied.

**deps v1.1 (reverse index):**
- Index build from 10 annotations with various dependencies.
- Lookup returns correct entries.
- Index update after new annotation includes new dependencies.
- Missing index: falls back to linear scan.
- Corrupted index JSON: falls back to linear scan with warning.
- `scan_method` reports "reverse_index" vs "linear" correctly.

**history:**
- Single commit touching the anchor: timeline with one entry.
- Multiple commits: chronological order (oldest first).
- `--follow-related`: related annotations included at correct position.
- `--follow-related=false`: no related annotations.
- `--limit 3`: only 3 most recent entries returned.
- Commit with no annotation: skipped in timeline.
- Annotation with `provenance: squash`: provenance field reflected.

**summary:**
- File with 5 AST units, 3 annotated: 3 SummaryUnits returned, stats show 2 unannotated.
- Most recent annotation selected when multiple exist for same unit.
- Only intent, constraints, risk_notes included (no reasoning, no tags, no dependencies).
- Directory mode: recurses into files.
- File without tree-sitter support: skipped with warning.

### Integration Tests

**deps end-to-end:**
1. Create a repo with two files: `session.rs` and `reconnect.rs`.
2. Write an annotation for `reconnect.rs` that declares a `semantic_dependency` on `session.rs:max_sessions`.
3. Run `ultragit deps src/session.rs max_sessions`.
4. Verify the dependency from `reconnect.rs` appears in output.

**history end-to-end:**
1. Create a repo. Make 3 commits modifying the same function.
2. Write annotations for each commit.
3. Run `ultragit history src/file.rs function_name`.
4. Verify timeline contains 3 entries in chronological order.

**summary end-to-end:**
1. Create a repo with a file containing multiple functions.
2. Write annotations for some commits.
3. Run `ultragit summary src/file.rs`.
4. Verify output lists each AST unit with its most recent annotation.

**reverse index end-to-end:**
1. Create a repo with annotations containing `semantic_dependencies`.
2. Run `ultragit index build`.
3. Run `ultragit deps` and verify `scan_method: "reverse_index"`.
4. Add a new annotation with a new dependency.
5. Run `ultragit deps` again and verify the new dependency appears.

---

## 9. Acceptance Criteria

1. `ultragit deps src/tls/session.rs max_sessions` returns all annotations whose `semantic_dependencies` reference that file+anchor, within 2s for a repo with 500 annotated commits (v1 linear scan).

2. `ultragit deps` with the reverse index (v1.1) completes in <100ms regardless of repository size.

3. `ultragit history src/mqtt/client.rs connect --limit 5` returns up to 5 chronologically-ordered annotations, including related annotations, within 1s.

4. `ultragit summary src/mqtt/client.rs` returns the most recent annotation per AST unit with only intent, constraints, and risk_notes, within 500ms.

5. `ultragit summary src/mqtt/` (directory) recurses into all files and aggregates results.

6. All three commands share blame, note-fetching, and scoring infrastructure with `ultragit read` — no duplicated pipeline logic.

7. The reverse index is updated at write time when a new annotation contains `semantic_dependencies`.

8. `ultragit index build` constructs the reverse index from all existing annotations.

9. When the reverse index is missing, `deps` falls back to linear scan without error.

10. Output JSON matches the documented schemas (`ultragit-deps/v1`, `ultragit-history/v1`, `ultragit-summary/v1`).

11. All commands degrade gracefully with zero annotations: empty results with informative stats, not errors.
