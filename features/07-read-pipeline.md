# Feature 07: Read Pipeline

## Core Retrieval Pipeline for Semantic Code Context

---

## 1. Overview

The read pipeline is Ultragit's primary retrieval interface. It resolves a query scope (file path, AST anchor, line range) into a set of relevant annotations by running git blame, fetching notes, filtering regions, following references, scoring confidence, trimming to token budget, and assembling formatted output (markdown by default, JSON or pretty-print via `--format`).

All operations are local git operations. No LLM calls on the read path. Target latency is under 500ms for single-file scoped queries.

This feature implements `ultragit read` — the command agents invoke before modifying code. It is the counterpart to the writing agent (Feature 05) and the foundation for advanced queries (Feature 08), annotation corrections (Feature 11), and the MCP server (Feature 12).

---

## 2. Dependencies

| Feature | What it provides |
|---------|-----------------|
| 01 CLI & Config | clap framework, subcommand registration, config access |
| 02 Git Operations | blame, notes fetch, diff, ref management via gix |
| 03 AST Parsing | tree-sitter anchor resolution (name to line range) |

Feature 07 does **not** depend on the writing agent (05), hooks (06), or LLM providers (04). It only reads annotations that already exist in `refs/notes/ultragit`.

---

## 3. Public API

### 3.1 CLI Interface

```
ultragit read [OPTIONS] <PATH> [<ANCHOR>]
```

**Positional arguments:**

| Argument | Required | Description |
|----------|----------|-------------|
| `PATH` | Yes | Relative file path from repo root. Multiple paths supported for multi-file mode. |
| `ANCHOR` | No | Named AST unit (e.g., `connect`, `MqttClient::connect`). Resolved via tree-sitter. |

**Scope selectors:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--lines` | `START:END` | — | Restrict to line range. Mutually exclusive with ANCHOR. |
| `--anchor` | `String` | — | Alias for positional ANCHOR. For backward compat and scripts. |

**Filtering options:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--since` | `String` | — | Date (ISO 8601) or commit SHA. Only annotations after this point. |
| `--tags` | `String` | — | Comma-separated tag filter. |
| `--context-level` | `enhanced\|inferred\|all` | `all` | Filter by annotation context level. |
| `--min-confidence` | `f64` | `0.0` | Drop regions below this confidence threshold. |

**Depth options:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--depth` | `u32` | `1` | Hops of `related_annotations` to follow. 0 = direct only. |
| `--max-regions` | `u32` | `20` | Cap on returned region annotations. |

**Output options:**

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--format` | `json\|markdown\|pretty` | `markdown` | Output format. `markdown` is token-efficient for LLM consumption. `json` for programmatic parsing. `pretty` for human debugging. |
| `--verbose` | `bool` | `false` | Include all fields, even null/empty. |
| `--max-tokens` | `u32` | — | Target token budget. Triggers trimming if exceeded. |

### 3.2 Core Library Types

```rust
/// Query parameters parsed from CLI args.
pub struct ReadQuery {
    pub files: Vec<PathBuf>,
    pub anchor: Option<String>,
    pub lines: Option<LineRange>,
    pub since: Option<SinceFilter>,
    pub tags: Option<Vec<String>>,
    pub context_level: ContextLevelFilter,
    pub min_confidence: f64,
    pub depth: u32,
    pub max_regions: u32,
    pub max_tokens: Option<u32>,
    pub verbose: bool,
}

#[derive(Clone, Copy)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

pub enum SinceFilter {
    Date(chrono::DateTime<chrono::Utc>),
    Sha(String),
}

pub enum ContextLevelFilter {
    All,
    Enhanced,
    Inferred,
}

/// A blame result: which commit introduced which lines.
pub struct BlameEntry {
    pub commit_sha: String,
    pub original_start: u32,
    pub original_end: u32,
    pub final_start: u32,
    pub final_end: u32,
}

/// A scored, filtered region ready for output.
pub struct ScoredRegion {
    pub region: schema::Region,
    pub commit_sha: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub confidence: f64,
    pub confidence_factors: ConfidenceFactors,
    pub related: Vec<ResolvedRelated>,
}

pub struct ConfidenceFactors {
    pub recency: f64,
    pub context_level: f64,
    pub anchor_stability: f64,
    pub provenance: f64,
}

pub struct ResolvedRelated {
    pub commit: String,
    pub anchor: String,
    pub relationship: String,
    pub confidence: f64,
    pub intent: Option<String>,
}

/// The complete read output, serialized to JSON.
pub struct ReadOutput {
    pub schema: String,             // "ultragit-read/v1"
    pub query: QueryEcho,
    pub regions: Vec<ScoredRegion>,
    pub dependencies_on_this: Vec<DependencyEntry>,
    pub cross_cutting: Vec<CrossCuttingEntry>,
    pub stats: ReadStats,
    pub trimmed: Option<TrimmedInfo>,
}

pub struct ReadStats {
    pub commits_examined: u32,
    pub annotations_found: u32,
    pub regions_returned: u32,
    pub related_hops: u32,
}

pub struct TrimmedInfo {
    pub original_regions: u32,
    pub returned_regions: u32,
    pub dropped_commits: Vec<String>,
    pub strategy: String,
    pub estimated_tokens: u32,
}
```

### 3.3 Pipeline Trait

```rust
/// The read pipeline, composable for testing.
pub trait ReadPipeline {
    fn execute(&self, query: &ReadQuery) -> Result<ReadOutput>;
}

/// Default implementation using real git operations.
pub struct GitReadPipeline {
    repo: git::Repository,
    ast: ast::AstParser,
}
```

---

## 4. Internal Design

### 4.1 Pipeline Stages

The read pipeline is a linear sequence of seven stages. Each stage transforms the output of the previous one.

```
ReadQuery
  │
  ▼
┌─────────────────────────────────────┐
│ Stage 1: Resolve Scope              │
│   PATH → validate file exists at HEAD │
│   ANCHOR → tree-sitter → LineRange  │
│   --lines → validate bounds         │
│   Output: Vec<ResolvedScope>        │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│ Stage 2: Blame                      │
│   git blame on resolved lines       │
│   Output: Vec<BlameEntry>           │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│ Stage 3: Fetch Notes                │
│   Dedupe commit SHAs from blame     │
│   git notes show for each SHA       │
│   Parse JSON → Vec<Annotation>      │
│   Apply --since, --context-level    │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│ Stage 4: Filter Regions             │
│   For each annotation:              │
│     Match regions by file path      │
│     Match by ast_anchor.name        │
│     Fall back to line range overlap │
│   Output: Vec<MatchedRegion>        │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│ Stage 5: Follow References          │
│   For each related_annotations ref: │
│     Fetch referenced commit note    │
│     Filter to referenced anchor     │
│     Recurse up to --depth hops      │
│   Output: enriched MatchedRegions   │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│ Stage 6: Score and Rank             │
│   Compute confidence per region     │
│   Sort by confidence descending     │
│   Apply --min-confidence filter     │
│   Apply --max-regions cap           │
│   Apply --tags filter               │
│   Output: Vec<ScoredRegion>         │
└─────────────────┬───────────────────┘
                  │
                  ▼
┌─────────────────────────────────────┐
│ Stage 7: Assemble Output            │
│   Merge regions                     │
│   Collect dependencies_on_this      │
│   Collect cross_cutting             │
│   Apply --max-tokens trimming       │
│   Format output (markdown/json/pretty)│
└─────────────────────────────────────┘
```

### 4.2 Stage 1: Scope Resolution

**File path validation.** Verify the file exists at HEAD using `gix`. For multi-file queries, validate each path. Return an error with the specific path if not found.

**Anchor resolution.** When an anchor is provided (positional or `--anchor`):

1. Parse the file with tree-sitter via the `ast::AstParser` from Feature 03.
2. Extract the outline: list of semantic units with name, type, signature, and line range.
3. Match the anchor string against unit names:
   - **Exact match:** `MqttClient::connect` matches `impl MqttClient { fn connect() }`.
   - **Unqualified match:** `connect` matches `fn connect()`.
   - **Fuzzy match:** If no exact or unqualified match, compute Levenshtein distance. Accept if distance <= 3, emit a warning.
4. If the anchor resolves to multiple matches (overloaded methods, same name in different impl blocks), include all and set a `ambiguous_anchor: true` flag in the output.
5. The resolved anchor produces a `LineRange`.

**Line range validation.** When `--lines START:END` is provided, verify `START <= END` and `END <= file line count`. Return a clear error if out of bounds.

**Fallback for no scope.** If neither anchor nor `--lines` is provided, the scope is the entire file. Blame is run on all lines.

```rust
pub struct ResolvedScope {
    pub file: PathBuf,
    pub line_range: Option<LineRange>,
    pub anchor_name: Option<String>,
    pub anchor_type: Option<String>,
    pub anchor_signature: Option<String>,
    pub ambiguous: bool,
}

fn resolve_scope(
    query: &ReadQuery,
    repo: &git::Repository,
    ast: &ast::AstParser,
) -> Result<Vec<ResolvedScope>>;
```

### 4.3 Stage 2: Blame

For each `ResolvedScope`, run `git blame` on the resolved line range.

- Use `gix` blame if available with line-range support.
- Fall back to `git blame -L START,END -- FILE` via CLI if `gix` doesn't support scoped blame.
- Parse output into `Vec<BlameEntry>`.
- Deduplicate by commit SHA across all scopes (a single commit may blame to multiple line ranges).

```rust
fn blame_scope(
    repo: &git::Repository,
    scope: &ResolvedScope,
) -> Result<Vec<BlameEntry>>;
```

**Performance note:** `git blame` on a 1000-line file takes ~50ms. For `--lines` queries on smaller ranges, it's faster. Scoped blame (`-L`) is critical for performance on large files.

### 4.4 Stage 3: Fetch Notes

1. Collect unique commit SHAs from all blame entries.
2. For each SHA, fetch the note from `refs/notes/ultragit` via `git notes --ref=ultragit show <sha>`.
3. Parse the note body as JSON into `schema::Annotation`.
4. Apply `--since` filter: if the annotation's timestamp is before the since cutoff, discard it. If `--since` is a SHA, resolve it to a timestamp first.
5. Apply `--context-level` filter: if the filter is `Enhanced`, discard `inferred` annotations, and vice versa.
6. Track SHAs that had no note (commit exists but was never annotated). Report in `stats.commits_examined` vs `stats.annotations_found`.

```rust
fn fetch_notes(
    repo: &git::Repository,
    shas: &[String],
    filters: &NoteFilters,
) -> Result<Vec<(String, schema::Annotation)>>;

pub struct NoteFilters {
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub context_level: ContextLevelFilter,
}
```

**Error handling:** If a note exists but contains invalid JSON, log a warning with the SHA and skip it. Do not fail the entire query for one malformed note.

### 4.5 Stage 4: Filter Regions

For each fetched annotation, extract the regions that are relevant to the query scope.

**Matching order (most specific first):**

1. **File path match.** Region's `file` field must match the queried file path. If it doesn't match, skip the region entirely.

2. **AST anchor name match (preferred).** If the query specified an anchor, compare the region's `ast_anchor.name` against the query anchor. Match using the same logic as scope resolution (exact > unqualified > fuzzy). This is the preferred matching mechanism because anchor names are more stable than line numbers across edits.

3. **Line range overlap (fallback).** If no anchor name match is found, check whether the region's `lines` overlap with the query's resolved line range. Two ranges overlap if `region.start <= query.end && region.end >= query.start`. This handles cases where the annotation was written before the current code's anchor names existed, or where the code has been restructured.

4. **Whole-file mode.** If the query has no anchor and no `--lines`, all regions matching the file path are included.

```rust
pub struct MatchedRegion {
    pub region: schema::Region,
    pub commit_sha: String,
    pub annotation_timestamp: chrono::DateTime<chrono::Utc>,
    pub annotation_context_level: String,
    pub annotation_provenance: schema::Provenance,
    pub match_type: MatchType,
}

pub enum MatchType {
    ExactAnchor,
    UnqualifiedAnchor,
    FuzzyAnchor { distance: u32 },
    LineOverlap { overlap_ratio: f64 },
    WholeFile,
}
```

### 4.6 Stage 5: Follow References

For each matched region, inspect its `related_annotations` entries. For each entry (up to `--depth` hops):

1. Fetch the referenced commit's annotation from `refs/notes/ultragit`.
2. Filter to the referenced anchor within that annotation.
3. Extract `intent`, `confidence`, and the relationship description.
4. If `--depth > 1`, recurse: inspect the referenced region's own `related_annotations`.
5. Track visited commit+anchor pairs to avoid cycles.

```rust
fn follow_references(
    repo: &git::Repository,
    regions: &[MatchedRegion],
    depth: u32,
) -> Result<Vec<ResolvedRelated>>;
```

**Cycle detection:** Maintain a `HashSet<(String, String)>` of `(commit_sha, anchor_name)` pairs already visited. Skip any reference that would revisit a pair.

**Performance:** Each hop requires a note fetch. With `--depth 2` and 5 regions each having 2 related annotations, that's up to 20 additional note fetches. Keep this bounded by `--depth` default of 1.

### 4.7 Stage 6: Confidence Scoring

Each matched region is scored on a 0.0-1.0 scale. The score is a weighted sum of four factors:

**Recency (40% weight).**

```rust
fn score_recency(annotation_timestamp: DateTime<Utc>, head_timestamp: DateTime<Utc>) -> f64 {
    let age_days = (head_timestamp - annotation_timestamp).num_days() as f64;
    // Exponential decay with half-life of 180 days
    let half_life = 180.0;
    (0.5_f64).powf(age_days / half_life)
}
```

An annotation from today scores 1.0. An annotation from 6 months ago scores 0.5. An annotation from a year ago scores 0.25.

**Context level (30% weight).**

| Level | Score |
|-------|-------|
| `enhanced` | 1.0 |
| `inferred` | 0.5 |

**Anchor stability (20% weight).**

Compare the annotation's `ast_anchor` against the current code's AST outline:

| Match | Score |
|-------|-------|
| Exact name and signature match | 1.0 |
| Name matches, signature differs | 0.7 |
| Name not found (anchor was renamed/removed) | 0.3 |
| Line range overlap only (no anchor match) | 0.4 |

```rust
fn score_anchor_stability(
    region: &schema::Region,
    current_outline: &[ast::OutlineEntry],
) -> f64;
```

**Provenance (10% weight).**

| Provenance | Score |
|------------|-------|
| `initial` | 1.0 |
| `amend` | 0.8 |
| `squash` | 0.7 |

**Combined score:**

```rust
fn compute_confidence(factors: &ConfidenceFactors) -> f64 {
    factors.recency * 0.4
        + factors.context_level * 0.3
        + factors.anchor_stability * 0.2
        + factors.provenance * 0.1
}
```

After scoring:
- Sort regions by confidence descending.
- Apply `--min-confidence` filter.
- Apply `--tags` filter (if the region's tags don't intersect the filter tags, drop it).
- Apply `--max-regions` cap (keep the top N).

### 4.8 Stage 7: Assemble Output

**Dependencies on this.** Scan annotations for `semantic_dependencies` entries that reference the queried file+anchor. This is the "what will break if I change this" signal.

For v1, this is a linear scan of recently annotated commits:

1. Walk `refs/notes/ultragit` and read annotations from the most recent 500 annotated commits (configurable).
2. For each annotation, inspect every region's `semantic_dependencies`.
3. If a dependency's `file` and `anchor` match the queried file+anchor, include it in `dependencies_on_this`.

This is the most expensive part of the query. See Feature 08 for the v1.1 reverse index that makes this O(1).

**Cross-cutting concerns.** From the annotations already fetched (from blame, not the dependency scan), collect any `cross_cutting` entries that mention the queried file+anchor in their `regions` list.

**Token budget trimming.** If `--max-tokens` is specified:

1. Serialize the full `ReadOutput` to JSON.
2. Estimate token count: `json_bytes.len() / 4` (conservative: 1 token ~ 4 chars).
3. If under budget, emit as-is.
4. If over budget, enter the trimming loop.

**Trimming strategy: newest-first region drop.**

```rust
fn trim_to_budget(output: &mut ReadOutput, max_tokens: u32) -> TrimmedInfo {
    let mut dropped_commits = Vec::new();

    // Sort regions by timestamp, newest first
    output.regions.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    while estimate_tokens(output) > max_tokens && !output.regions.is_empty() {
        let dropped = output.regions.remove(0); // remove newest
        dropped_commits.push(dropped.commit_sha.clone());
        // Recompute dependencies_on_this and cross_cutting
        // (remove entries only referenced by dropped regions)
    }

    // If still over budget after dropping all but one region,
    // apply field-level trimming to remaining regions
    if estimate_tokens(output) > max_tokens {
        for region in &mut output.regions {
            trim_region_fields(region);
        }
    }

    TrimmedInfo {
        original_regions: original_count,
        returned_regions: output.regions.len() as u32,
        dropped_commits,
        strategy: "newest_commits_first".to_string(),
        estimated_tokens: estimate_tokens(output),
    }
}
```

**Field-level trimming order within a region:**

1. `related` — linked annotations from other commits (available via separate query). Remove entirely.
2. `reasoning` — truncate to first sentence, then remove entirely.
3. `risk_notes` — truncate to first sentence, then remove entirely.
4. `tags` — remove entirely.
5. `constraints` — **never removed**. Highest-value field for preventing breakage.
6. `intent` — **never removed**. Minimum viable annotation.

**Invariant:** If a region is present in the output, it always has `intent` and `constraints`.

**Compact vs verbose output.** Default output omits null/empty fields to reduce token count. `--verbose` includes all fields. This is handled at serialization time with `#[serde(skip_serializing_if = "Option::is_none")]` on optional fields, toggled by the verbose flag.

### 4.9 Markdown Formatter

The markdown formatter is the final formatting pass applied to the `ReadOutput` struct when `--format markdown` is selected (the default). It takes the same internal `ReadOutput` that JSON serialization uses and renders it as structured markdown.

```rust
/// Render a ReadOutput as structured markdown.
/// This is the default output format, optimized for LLM consumption.
pub fn format_markdown(output: &ReadOutput) -> String {
    let mut out = String::new();

    // Query header
    write_query_header(&mut out, &output.query);

    // Each region becomes a ## section
    for region in &output.regions {
        write_region_markdown(&mut out, region);
    }

    // Dependencies on this
    if !output.dependencies_on_this.is_empty() {
        write_dependencies_markdown(&mut out, &output.dependencies_on_this);
    }

    // Cross-cutting concerns
    if !output.cross_cutting.is_empty() {
        write_cross_cutting_markdown(&mut out, &output.cross_cutting);
    }

    // Stats footer
    write_stats_markdown(&mut out, &output.stats);

    // Trimming notice
    if let Some(trimmed) = &output.trimmed {
        write_trimmed_markdown(&mut out, trimmed);
    }

    out
}
```

The implementation is straightforward — a `format_markdown(output: &ReadOutput) -> String` function alongside the existing JSON serialization (`serde_json::to_string`). The trimming pipeline produces a `ReadOutput` regardless of format; formatting is the last step.

**Markdown rendering rules:**

- Each region becomes a `## file — anchor` header with commit metadata on the next line.
- Scalar fields (`intent`, `reasoning`, `risk_notes`) render as `**Label:** value` lines.
- List fields (`constraints`, `tags`) render as markdown bullet lists.
- `dependencies_on_this` and `cross_cutting` render as sections with bullet lists.
- `trimmed` info renders as a notice block at the end.
- Regions are separated by `---` horizontal rules.
- Empty/null fields are omitted (same as compact JSON mode) unless `--verbose` is set.

**Token estimation for markdown.** The token estimator (`estimate_tokens`) accepts the format as a parameter. Markdown output is typically 40-60% smaller than JSON for the same content, so the token budget goes further. The estimator applies the same `bytes / 4` heuristic but to the markdown-rendered output rather than JSON.

### 4.10 Multi-File Mode

When multiple `PATH` arguments are provided:

1. Resolve scope for each path independently.
2. Run blame for each.
3. Merge all blame results, deduplicating SHAs.
4. Fetch notes once for the merged SHA set.
5. Filter regions per file.
6. In the output, group regions by file.
7. Surface cross-file `semantic_dependencies` and `cross_cutting` that span the queried files.

The output schema is the same; `regions` contains entries from all files, each with its `file` field set.

---

## 5. Error Handling

| Failure Mode | Behavior |
|--------------|----------|
| File not found at HEAD | Return error: `"File not found: {path}. Does it exist at HEAD?"` |
| Anchor not found | If fuzzy match available: warn and proceed. Otherwise: error with available anchors listed. |
| `--lines` out of bounds | Error: `"Line range {start}:{end} exceeds file length ({n} lines)"` |
| Malformed annotation JSON | Warn: `"Skipping malformed annotation on commit {sha}"`. Continue with remaining annotations. |
| No annotations found | Return a valid `ReadOutput` with empty `regions`, `stats.annotations_found: 0`. Not an error. |
| git blame fails | Error with the underlying git error. Could indicate a corrupt repo or untracked file. |
| Notes ref doesn't exist | Return empty results with a hint: `"No Ultragit annotations found. Run 'ultragit init' to set up."` |
| Tree-sitter grammar not available | Fall back to line-range-only mode. Warn: `"No tree-sitter grammar for {language}. Anchor resolution unavailable."` |

All errors are returned as structured JSON when `--format json` (errors always use JSON regardless of the selected output format for consistent machine parsing):

```json
{
  "$schema": "ultragit-read/v1",
  "error": {
    "code": "file_not_found",
    "message": "File not found: src/missing.rs. Does it exist at HEAD?"
  }
}
```

---

## 6. Configuration

Read pipeline configuration in `.git/config` under `[ultragit]`:

```ini
[ultragit]
    # Maximum commits to scan for dependencies_on_this (v1 linear scan)
    depsScanLimit = 500

    # Default max-regions if not specified on CLI
    defaultMaxRegions = 20

    # Confidence scoring half-life in days
    recencyHalfLife = 180
```

These can be overridden by `.ultragit-config.toml` for shared team defaults:

```toml
[ultragit.read]
deps_scan_limit = 500
default_max_regions = 20
recency_half_life = 180
```

---

## 7. Implementation Steps

### Step 1: Read Output Schema Types
**Scope:** Define all Rust types for `ReadOutput`, `ScoredRegion`, `ConfidenceFactors`, `TrimmedInfo`, `ReadStats`, etc. in `src/schema/output.rs`. Add serde derive macros. Write serialization tests verifying the JSON output matches the documented schema.

### Step 2: Scope Resolution
**Scope:** Implement `resolve_scope()` in `src/read/mod.rs`. File existence check via gix. Line range validation. Anchor resolution using the AST parser from Feature 03. Fuzzy matching with Levenshtein distance. Tests: exact anchor match, unqualified match, fuzzy match, missing anchor with suggestions, line range validation.

### Step 3: Blame Integration
**Scope:** Implement `blame_scope()` in `src/read/retrieve.rs`. Use gix blame with line-range support, falling back to `git blame -L`. Parse blame output into `Vec<BlameEntry>`. SHA deduplication across multiple scopes. Tests: single-file blame, scoped blame, multi-scope dedup.

### Step 4: Note Fetching and Filtering
**Scope:** Implement `fetch_notes()` in `src/read/retrieve.rs`. Fetch from `refs/notes/ultragit` for each SHA. JSON parsing with graceful error handling. Apply `--since` and `--context-level` filters. Tests: valid note fetch, missing note, malformed JSON, filter application.

### Step 5: Region Filtering
**Scope:** Implement region matching in `src/read/retrieve.rs`. File path match, anchor name match (exact/unqualified/fuzzy), line range overlap fallback, whole-file mode. Tests: each match type, priority order, multi-region annotations with partial matches.

### Step 6: Reference Following
**Scope:** Implement `follow_references()` in `src/read/retrieve.rs`. Recursive note fetch up to `--depth` hops. Cycle detection via visited set. Tests: single hop, multi-hop, cycle in references, missing referenced commit.

### Step 7: Confidence Scoring
**Scope:** Implement the 4-factor scoring model in `src/read/scoring.rs`. Recency exponential decay, context level scoring, anchor stability check against current AST, provenance scoring. Combined weighted score. Tests: each factor in isolation, combined scoring, edge cases (very old annotations, renamed anchors).

### Step 8: Token Budget Trimming
**Scope:** Implement `trim_to_budget()` in `src/read/trimming.rs`. Token estimation. Newest-first region drop loop. Field-level trimming order. Invariant preservation (intent + constraints always present). `TrimmedInfo` population. Tests: under budget (no trimming), over budget (region drops), extreme budget (field trimming), invariant holds.

### Step 9: Dependency Scan (v1)
**Scope:** Implement the linear dependency scan in `src/read/deps.rs`. Walk recent annotated commits, scan `semantic_dependencies` for matches to the queried file+anchor. Cap at `depsScanLimit`. Tests: dependency found, no dependencies, scan limit respected.

### Step 10: Output Assembly and CLI Integration
**Scope:** Wire all stages together in `src/read/mod.rs`. Implement the `ReadPipeline` trait. Register the `read` subcommand in `src/cli/read.rs`. Implement all three output formatters: `format_markdown()` (default), JSON via serde serialization, and pretty-print. The `format_markdown()` function takes a `&ReadOutput` and renders structured markdown. Verbose mode. Multi-file aggregation. End-to-end integration test with a fixture repo containing annotations.

### Step 11: Cross-Cutting Concern Collection
**Scope:** Extract `cross_cutting` entries from fetched annotations that reference the queried scope. Deduplicate. Include in output. Tests: cross-cutting found, no cross-cutting, multi-file cross-cutting.

---

## 8. Test Plan

### Unit Tests

**Scope resolution:**
- Exact anchor match for a Rust function, method, struct, impl block.
- Qualified name match (`MqttClient::connect`).
- Unqualified name match (`connect` matching `fn connect()`).
- Fuzzy match with warning (typo: `conect` matching `connect`).
- Anchor not found: error lists available anchors.
- Ambiguous anchor: multiple matches returned.
- Line range: valid range, out-of-bounds, inverted range.
- File not found at HEAD.

**Blame:**
- Single-file blame returns correct SHA-to-line mappings.
- Scoped blame (`-L`) returns only lines in range.
- SHA deduplication across multiple blame ranges.
- File with single commit (all lines blame to same SHA).

**Note fetching:**
- Valid note fetch and JSON parse.
- Missing note (commit exists, no annotation).
- Malformed JSON: warning logged, note skipped.
- `--since` filter by date.
- `--since` filter by SHA.
- `--context-level` filter: enhanced only, inferred only, all.

**Region filtering:**
- File path match, mismatch.
- AST anchor exact match.
- AST anchor unqualified match.
- AST anchor fuzzy match.
- Line range overlap: full overlap, partial overlap, no overlap.
- Whole-file mode: all regions for the file returned.
- Annotation with multiple regions, only some matching.

**Confidence scoring:**
- Recency: today = 1.0, 180 days = 0.5, 360 days = 0.25.
- Context level: enhanced = 1.0, inferred = 0.5.
- Anchor stability: exact match, signature change, anchor removed.
- Provenance: initial, amend, squash.
- Combined score weighted correctly.
- `--min-confidence` filter.
- `--max-regions` cap.
- `--tags` filter.

**Token trimming:**
- Output under budget: no trimming.
- Output over budget: newest regions dropped first.
- Extreme budget: field-level trimming applied.
- `intent` and `constraints` never removed.
- `TrimmedInfo` correctly populated.
- Empty `trimmed` field when no trimming.

**Reference following:**
- Single hop: related annotation fetched and resolved.
- Multi-hop: depth 2 follows references of references.
- Depth 0: no references followed.
- Cycle detection: A references B, B references A.
- Missing referenced commit: skip gracefully.

### Integration Tests

**End-to-end read pipeline:**
1. Create a temporary git repository.
2. Make commits with known content.
3. Write annotations as git notes (simulating the writing agent).
4. Run `ultragit read` and verify the output matches expectations.

**Multi-file query:**
1. Create a repo with annotations across multiple files.
2. Query multiple files at once.
3. Verify cross-file dependencies and cross-cutting concerns appear.

**Trimming integration:**
1. Create a repo with many annotations.
2. Query with `--max-tokens 500`.
3. Verify output is valid JSON, under budget, and contains `trimmed` metadata.

### Property Tests

**Confidence scoring invariants:**
- Score is always in [0.0, 1.0].
- Newer annotations score >= older annotations (all else equal).
- Enhanced scores >= inferred (all else equal).
- Exact anchor match scores >= fuzzy match (all else equal).

**Trimming invariants:**
- Output token estimate <= max_tokens (when max_tokens specified).
- If any region is present, it has `intent` and `constraints`.
- Output is always valid JSON.

---

## 9. Acceptance Criteria

1. `ultragit read src/file.rs FunctionName` returns annotations for the named function with confidence scores, in under 500ms for a typical repository.

2. `ultragit read src/file.rs --lines 10:20` returns annotations for the specified line range via blame lookup.

3. `ultragit read src/file.rs` (no anchor/lines) returns all annotated regions in the file.

4. `ultragit read src/a.rs src/b.rs` returns combined annotations with cross-file concerns surfaced.

5. Confidence scoring produces scores that correctly rank enhanced-recent annotations above inferred-old annotations.

6. `--max-tokens` trimming produces valid output within the token budget regardless of format, dropping newest regions first and preserving `intent` and `constraints` on every retained region.

7. `--depth 0` returns only direct annotations. `--depth 2` follows references through two hops.

8. `--min-confidence`, `--tags`, `--context-level`, `--since`, and `--max-regions` all filter correctly.

9. Default output is structured markdown. `--format json` produces JSON matching the documented `ultragit-read/v1` schema. `--format pretty` produces human-readable output.

10. The dependency scan surfaces `dependencies_on_this` entries from other annotations that reference the queried code.

11. Malformed annotations, missing notes, and unsupported languages degrade gracefully without crashing the pipeline.

12. The read pipeline makes zero network calls and zero LLM calls.
