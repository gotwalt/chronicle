# Chronicle: Reading Agent

## `git chronicle read` — CLI for Semantic Code Context Retrieval

---

## 1. Overview

`git chronicle read` is the retrieval interface for Chronicle annotations. It is a CLI designed primarily for AI agents — invoked before modifying code to gather the reasoning, intent, constraints, and semantic dependencies captured at commit time by the Writing Agent.

The core workflow is simple: an agent identifies code it needs to understand, runs `git chronicle read` with a file path and optional scope, and receives structured JSON containing the accumulated reasoning behind that code. The CLI handles the mechanics of blame traversal, note retrieval, annotation filtering, and confidence scoring internally.

This document covers the CLI design and an accompanying LLM skill definition that teaches agents when and how to invoke it.

---

## 2. Problem Statement

An agent assigned a task — "add connection pooling to the MQTT client" — needs to understand the existing code before modifying it. Reading the code tells it *what* exists. `git log` and `git blame` tell it *when* and *who*. But neither tells it *why* the code is structured the way it is, what alternatives were rejected, what invariants are being protected, or what other code depends on assumptions embedded here.

Chronicle annotations capture all of this. But raw `git notes` are keyed by commit SHA and contain annotations for entire commits spanning multiple files and regions. An agent doesn't want to manually run blame, collect SHAs, fetch notes, parse JSON, and filter to the relevant regions. It wants a single command that returns "here's everything you should know about this code before you touch it."

---

## 3. Design Principles

**Agent-first interface.** The default output is markdown — a structured but token-efficient format designed for LLM consumption. Agents parse it naturally. `--format json` is available when programmatic parsing is needed, and `--format pretty` exists for human debugging.

**Blame is the index.** Every retrieval path ultimately runs through `git blame` to map current code to the commits that produced it, then fetches annotations from those commits. There is no separate index to build or maintain.

**Scope control.** An agent should be able to ask for context at different granularities: a single function, a line range, an entire file, or a set of files. Broader scope returns more annotations but costs more tokens in the agent's context window. The CLI lets the agent choose.

**Depth control.** Not all annotations are equally relevant. The CLI supports filtering by recency, context level, tags, and relationship depth (how many hops of `related_annotations` to follow). An agent working on a quick fix needs less context than one doing a major refactor.

**Fast.** The CLI must complete in under a second for typical single-file queries. Agents invoke it synchronously as part of their planning phase — latency here delays the entire task. No LLM calls on the read path. All operations are local git operations.

---

## 4. CLI Interface

### 4.1 Primary Command

```
git chronicle read [OPTIONS] <PATH> [<ANCHOR>] [--lines <START:END>]
```

Returns Chronicle annotations relevant to the specified code.

**Arguments:**

`<PATH>` — relative file path from the repository root. Required.

`<ANCHOR>` — optional positional argument specifying a named AST unit (function, struct, impl, etc.). This is the most common use case — querying a specific function or type — so it should be easy to type without a flag. Chronicle parses the file with tree-sitter to resolve the anchor to a line range, then proceeds as with `--lines`. Supports qualified names like `MqttClient::connect`. The `--anchor <NAME>` flag is retained as an alias for backward compatibility.

**Scope selectors (optional):**

`--lines <START:END>` — restrict to a specific line range. Uses `git blame` on those lines to find the relevant commits.

`--anchor <NAME>` — alias for the positional `<ANCHOR>` argument. Retained for backward compatibility and for use in scripts.

If neither anchor nor `--lines` is specified, returns annotations for the entire file.

**Filtering options:**

`--since <DATE|SHA>` — only include annotations from commits after this point. Useful for "what changed recently" queries.

`--tags <TAG,...>` — only include annotations with matching tags.

`--context-level <enhanced|inferred|all>` — filter by annotation context level. Default: `all`.

`--min-confidence <0.0-1.0>` — filter out annotations below a confidence threshold (see Section 5.3). Default: `0.0` (no filtering).

**Depth options:**

`--depth <N>` — how many hops of `related_annotations` to follow. Default: `1`. Set to `0` for only direct annotations, higher for deeper reasoning chains.

`--max-regions <N>` — cap the number of region annotations returned. Most relevant first. Default: `20`.

**Output options:**

`--format <json|markdown|pretty>` — output format. Default: `markdown`. `markdown` produces a structured but token-efficient format designed for LLM consumption. `json` produces machine-parseable JSON for programmatic access. `pretty` produces human-readable formatted output for debugging.

`--verbose` — include all fields in JSON output, even when empty/null. Default output is compact (omits empty/null fields) to reduce token count for agent consumption.

`--max-tokens <N>` — target maximum token count for the output. Chronicle estimates token count (roughly 4 characters per token) and trims the output to fit while maintaining valid, parseable output regardless of format. Trimming happens at the semantic level (dropping regions, then fields) before formatting. See Section 5.4 for the trimming strategy.

### 4.2 Multi-File Queries

```
git chronicle read [OPTIONS] <PATH...>
```

When multiple paths are provided, `git chronicle read` returns a combined annotation set with cross-file `semantic_dependencies` and `cross_cutting` concerns surfaced automatically. Useful when an agent is planning a change that spans multiple files and needs to understand the coupling between them.

```
git chronicle read src/mqtt/client.rs src/tls/session.rs src/mqtt/reconnect.rs
```

### 4.3 Dependency Query

```
git chronicle deps <PATH> [<ANCHOR>]
```

Returns only the `semantic_dependencies` and `cross_cutting` entries that reference the specified code, aggregated across all annotations in the repository. This answers: "what other code depends on assumptions about this function?"

This is the highest-value query for preventing regressions. An agent about to modify `TlsSessionCache::max_sessions()` runs `git chronicle deps src/tls/session.rs max_sessions` and immediately learns that the MQTT reconnection logic assumes a max of 4 sessions.

### 4.4 History Query

```
git chronicle history <PATH> [<ANCHOR>] [--limit <N>]
```

Returns the annotation timeline for a code region — the chain of annotations across commits that have touched it, ordered chronologically. This is `git log` but for reasoning: not "what changed" but "what was the thinking at each step."

Follows `related_annotations` links to include connected reasoning even from commits that didn't directly touch the specified code.

### 4.5 Summary Query

```
git chronicle summary <PATH> [<ANCHOR>]
```

Returns a condensed view: the most recent annotation for each AST-level unit in the file (or for the specified anchor), with only the `intent`, `constraints`, and `risk_notes` fields. Designed for broad orientation — an agent scanning a module to understand its shape before diving into a specific function.

---

## 5. Retrieval Architecture

### 5.1 Core Retrieval Pipeline

All queries follow the same fundamental pipeline:

```
┌──────────────────────────────────────────────────────┐
│                   git chronicle read                      │
├──────────────────────────────────────────────────────┤
│                                                       │
│  1. Resolve scope                                     │
│     PATH → file exists at HEAD?                       │
│     ANCHOR → tree-sitter parse → line range            │
│     --lines → validate range                          │
│                                                       │
│  2. Blame                                             │
│     git blame on resolved line range                  │
│     → set of (commit SHA, original line range) pairs  │
│                                                       │
│  3. Fetch notes                                       │
│     For each unique commit SHA:                       │
│       git notes --ref=chronicle show <sha>            │
│     → set of raw annotation JSON documents            │
│                                                       │
│  4. Filter regions                                    │
│     For each annotation document:                     │
│       Match regions by file path                      │
│       Match by AST anchor name (preferred)            │
│       Fall back to line range overlap                 │
│     → set of relevant region annotations              │
│                                                       │
│  5. Follow references (if --depth > 0)                │
│     For each related_annotations entry:               │
│       Fetch the referenced commit's annotation        │
│       Filter to the referenced anchor                 │
│       Recurse up to --depth hops                      │
│     → extended set with linked reasoning              │
│                                                       │
│  6. Score and rank                                    │
│     Apply confidence scoring (Section 5.3)            │
│     Sort by relevance                                 │
│     Apply --max-regions cap                           │
│                                                       │
│  7. Assemble output                                   │
│     Merge region annotations                          │
│     Deduplicate semantic_dependencies                 │
│     Surface cross_cutting concerns                    │
│     Emit JSON                                         │
│                                                       │
└──────────────────────────────────────────────────────┘
```

### 5.2 Anchor Resolution

When an anchor is specified (either as a positional argument or via `--anchor`), Chronicle parses the file with tree-sitter and searches for a matching semantic unit. The matching is flexible:

- Exact match: `MqttClient::connect` matches `impl MqttClient { fn connect() }`
- Unqualified match: `connect` matches `fn connect()`
- Fuzzy match: if no exact match, find the closest name (Levenshtein distance) and warn

If the anchor resolves to multiple matches (e.g., overloaded methods in different impl blocks), all are included with a note about ambiguity.

### 5.3 Confidence Scoring

Not all annotations are equally trustworthy or relevant. Each returned region annotation is scored on a 0.0–1.0 confidence scale based on:

**Recency (40% weight).** How recently was this annotation produced relative to the current HEAD? An annotation from the most recent commit touching this code scores 1.0. Annotations from older commits score lower on a decay curve. Code that hasn't been touched in a year may have annotations that reference now-changed assumptions.

**Context level (30% weight).** `enhanced` annotations (with explicit agent reasoning) score higher than `inferred` annotations (diff analysis only). The reasoning in an enhanced annotation is direct testimony from the authoring agent. Inferred reasoning is reconstructed and may be wrong.

**Anchor stability (20% weight).** Does the AST anchor in the annotation still match the current code? If the annotation references `fn connect(config: &Config)` but the current signature is `fn connect(config: &Config, timeout: Duration)`, the annotation is still relevant but the code has evolved — score is reduced.

**Provenance (10% weight).** `initial` annotations score slightly higher than `squash`-synthesized or `amend`-migrated ones, since synthesis can lose nuance.

The confidence score is included in the output for each region. Agents can use `--min-confidence` to filter, or use the score to weight annotations in their own reasoning.

### 5.4 Token Budget Trimming

When `--max-tokens` is specified, Chronicle must reduce the output to fit while guaranteeing the result is valid, parseable JSON. This is a hard constraint — a truncated or malformed response is worse than a smaller correct one.

**Token estimation.** Chronicle uses a simple heuristic: 1 token ≈ 4 characters of JSON output. This slightly overestimates for structured JSON (which has a high ratio of syntax characters to content), making it a conservative bound. The estimate is applied after serialization, not before — Chronicle builds the full output, measures it, and trims if needed.

**Trimming strategy: newest commits first.**

The trimming order is deliberate. Older annotations are closer to the foundational reasoning about why code exists in its current form. Newer annotations are more likely to be incremental changes, follow-ups, or refinements. When budget is constrained, preserving the origin story matters more than the latest tweak.

The trimming pipeline:

```
1. Build full output (all regions, dependencies, cross-cutting)
2. Estimate token count
3. If under budget → emit as-is
4. If over budget → enter trimming loop:

   a. Sort regions by commit timestamp (newest first)
   b. Drop the newest region
   c. Recalculate dependencies_on_this and cross_cutting
      (remove entries that only referenced dropped regions)
   d. Re-estimate token count
   e. If still over → repeat from (b)
   f. If under → emit

5. If all regions dropped and still over budget:
   a. Trim related annotations (depth 0 equivalent)
   b. Trim reasoning/constraints fields to first sentence
   c. As last resort, return a minimal skeleton with only
      intent fields and a truncation warning
```

**The output always includes a `trimmed` field when trimming occurs:**

```json
{
  "trimmed": {
    "original_regions": 12,
    "returned_regions": 5,
    "dropped_commits": ["<sha>", "<sha>", ...],
    "strategy": "newest_commits_first",
    "estimated_tokens": 2048
  }
}
```

This tells the agent that context was lost and which commits were dropped. If the agent decides it needs the full picture, it can re-query without `--max-tokens` or query the dropped commits individually.

**Within a single region, fields are trimmed in this order if further reduction is needed:**

1. `related` (linked annotations from other commits — available via separate query)
2. `reasoning` (truncated to first sentence, then removed)
3. `risk_notes` (truncated to first sentence, then removed)
4. `tags` (removed)
5. `constraints` (never removed — these are the highest-value field for preventing breakage)
6. `intent` (never removed — this is the minimum viable annotation)

The invariant is: **if a region is present in the output, it always has `intent` and `constraints`.** An agent can always rely on these two fields existing.

### 5.5 Output Schema

```json
{
  "$schema": "chronicle-read/v1",
  "query": {
    "file": "src/mqtt/client.rs",
    "anchor": "MqttClient::connect",
    "lines": [42, 67],
    "depth": 1
  },

  "regions": [
    {
      "commit": "<sha>",
      "timestamp": "<iso8601>",
      "context_level": "enhanced",
      "confidence": 0.92,
      "confidence_factors": {
        "recency": "2 days old",
        "context_level": "enhanced",
        "anchor_match": "exact",
        "provenance": "initial"
      },

      "file": "src/mqtt/client.rs",
      "ast_anchor": {
        "type": "method",
        "name": "MqttClient::connect",
        "signature": "pub fn connect(&mut self, config: &MqttConfig) -> Result<()>"
      },
      "lines": { "start": 42, "end": 67 },

      "intent": "Establishes mTLS connection to the cloud MQTT broker...",
      "reasoning": "Chose mutual TLS over token auth because...",
      "constraints": ["Requires TLS session cache to hold ≤4 sessions..."],
      "risk_notes": "The broker silently drops idle connections after 30min...",
      "tags": ["mqtt", "security", "iot"],

      "related": [
        {
          "commit": "<sha>",
          "anchor": "TlsSessionCache::new",
          "relationship": "depends on session cache size limit",
          "confidence": 0.85,
          "intent": "Session cache bounded to 4 entries to limit memory..."
        }
      ]
    }
  ],

  "dependencies_on_this": [
    {
      "from_file": "src/mqtt/reconnect.rs",
      "from_anchor": "ReconnectHandler::attempt",
      "nature": "assumes connect() is idempotent and can be retried safely",
      "commit": "<sha>",
      "confidence": 0.78
    }
  ],

  "cross_cutting": [
    {
      "description": "TLS cert rotation requires updating both connect() and the cert watcher in deploy/rotate.rs",
      "regions": ["src/mqtt/client.rs:MqttClient::connect", "deploy/rotate.rs:CertWatcher::on_renewal"],
      "commit": "<sha>"
    }
  ],

  "stats": {
    "commits_examined": 12,
    "annotations_found": 8,
    "regions_returned": 3,
    "related_hops": 1
  },

  "trimmed": null
}
```

**Key design decisions in the output:**

`regions` contains the direct annotations for the queried code, enriched with confidence scores and with `related_annotations` resolved inline (the `related` array within each region). The agent doesn't need to make follow-up queries to understand referenced annotations.

`dependencies_on_this` is an inverted view — other code's annotations that declare a semantic dependency on the queried code. This is the "what will break if I change this" signal. It requires scanning annotations beyond just the blamed commits, so it's computed by searching all annotations that reference the queried file+anchor in their `semantic_dependencies`. This is the most expensive part of the query but also the most valuable.

`cross_cutting` surfaces multi-region concerns from any annotation that includes the queried code in a cross-cutting group.

`stats` gives the agent a sense of annotation coverage. If `annotations_found` is 0, the agent knows it's flying blind.

`trimmed` is `null` when no trimming occurred, or an object describing what was dropped when `--max-tokens` forced output reduction. The `dropped_commits` array lets the agent selectively re-query specific commits if it later decides it needs the context that was trimmed.

`confidence_factors` breaks down the confidence score into its components, allowing agents to make nuanced decisions. An agent might trust an old-but-enhanced annotation differently from a recent-but-inferred one. The factors map directly to the scoring weights described in Section 5.3: `recency`, `context_level`, `anchor_match` (how well the AST anchor still matches the current code), and `provenance` (whether the annotation is original, squash-synthesized, or amend-migrated).

### 5.6 Markdown Output Format

Markdown is the default output format (`--format markdown`). It renders the same `ReadOutput` data as the JSON format but uses structured markdown instead of JSON syntax, producing output that is approximately 40-60% fewer tokens for the same annotation content.

**Why markdown is the default.** For agents consuming annotations as context, markdown's reduction in token count translates directly to more annotations fitting within a token budget. The structured headers (`##`, `**bold**`, `-` lists) are easily parsed by LLMs while avoiding JSON's syntactic overhead — braces, quotes, repeated key names, and escape sequences carry no semantic information for an LLM but consume tokens.

**Example: the same annotation in markdown vs JSON.**

Markdown format (`--format markdown`, the default):

```markdown
## src/mqtt/client.rs — MqttClient::connect
**Commit:** a1b2c3d (2 days ago) | **Confidence:** 0.92 (enhanced, exact anchor)

**Intent:** Establishes mTLS connection to cloud MQTT broker with automatic session resumption...

**Reasoning:** Chose mutual TLS over token auth because...

**Constraints:**
- [author] Requires TLS session cache to hold ≤4 sessions...
- [inferred] Function assumes single-threaded execution context

**Risk:** The broker silently drops idle connections after 30min...

**Dependencies on this:**
- `src/mqtt/reconnect.rs — ReconnectHandler::attempt`: assumes connect() is idempotent

**Cross-cutting:**
- TLS cert rotation requires updating both connect() and deploy/rotate.rs:CertWatcher::on_renewal

---
```

JSON format (`--format json`):

```json
{
  "regions": [{
    "commit": "a1b2c3d",
    "timestamp": "2025-06-10T14:30:00Z",
    "context_level": "enhanced",
    "confidence": 0.92,
    "confidence_factors": {
      "recency": "2 days old",
      "context_level": "enhanced",
      "anchor_match": "exact",
      "provenance": "initial"
    },
    "file": "src/mqtt/client.rs",
    "ast_anchor": {
      "type": "method",
      "name": "MqttClient::connect"
    },
    "intent": "Establishes mTLS connection to cloud MQTT broker with automatic session resumption...",
    "reasoning": "Chose mutual TLS over token auth because...",
    "constraints": [
      "Requires TLS session cache to hold ≤4 sessions...",
      "Function assumes single-threaded execution context"
    ],
    "risk_notes": "The broker silently drops idle connections after 30min..."
  }],
  "dependencies_on_this": [{
    "from_file": "src/mqtt/reconnect.rs",
    "from_anchor": "ReconnectHandler::attempt",
    "nature": "assumes connect() is idempotent"
  }],
  "cross_cutting": [{
    "description": "TLS cert rotation requires updating both connect() and deploy/rotate.rs:CertWatcher::on_renewal"
  }]
}
```

The markdown version conveys the same information in significantly fewer tokens. The JSON version's structural overhead — nested braces, repeated key names like `"from_file"`, `"from_anchor"`, `"nature"`, quote characters around every string — adds no semantic value for an LLM reader.

**Trimming behavior.** `--max-tokens` trimming works the same way regardless of format. The trimming pipeline operates at the semantic level — dropping regions, then trimming fields within regions — and produces a `ReadOutput` struct. Formatting (markdown, JSON, or pretty) is the last step, applied to the already-trimmed `ReadOutput`. The token estimation uses the target format's typical overhead for accurate budgeting.

**When to use JSON instead.** Use `--format json` when the output will be parsed programmatically — for example, by a script that extracts specific fields, or by an MCP tool wrapper that needs structured data. Use `--format markdown` (the default) when the output will be consumed as context by an LLM.

---

## 6. Performance

All operations are local. No network calls, no LLM invocations on the read path.

**Blame** is the bottleneck for large files. `git blame` on a 1000-line file takes ~50ms typically. For `--lines` scoped queries on smaller ranges, it's faster. `gix` (gitoxide) blame should be faster than shelling out.

**Notes fetch** is O(n) in the number of unique commits from blame. Each note fetch is a git object lookup — fast, single-digit milliseconds per note.

**Dependency scan** (`dependencies_on_this`) is the expensive operation. It requires reading annotations from commits beyond those in the blame set. For v1, the dependency scan walks recent annotation history (capped at the last 500 annotated commits). For v1.1, a reverse index will be built at write time: when the Writing Agent emits an annotation with `semantic_dependencies`, it also updates a reverse-index note mapping `file:anchor → [commit SHAs that depend on it]`. This makes `deps` queries O(1) lookup instead of O(n) scan.

**Target latency:** <500ms for a single-file scoped query. <2s for multi-file queries across 5-10 files. `deps` queries may be slower depending on repository size until the v1.1 reverse index is in place.

### 6.1 Annotation Correction and Flagging

Annotations are not immutable truths — agents that discover incorrect annotations should flag them so future agents aren't misled. This creates a self-correcting knowledge base rather than a write-once store.

**Flagging an annotation:**

```
git chronicle flag <PATH> [<ANCHOR>] --reason "Constraint about drain-before-reconnect is incorrect; verified safe to reconnect first"
```

This writes a correction note that the read path surfaces alongside the original annotation. The flagged annotation gets its confidence reduced. Future agents see both the original claim and the correction, allowing them to make informed decisions.

**Formal correction:**

```
git chronicle correct <SHA> --region "MqttClient::connect" --field constraints --remove "Must drain queue before reconnecting"
```

`git chronicle correct` targets a specific annotation by commit SHA and removes or amends individual fields. The correction is stored as a separate note linked to the original, preserving the history of what was believed and when it was corrected.

Both mechanisms ensure that annotations improve over time. An agent that encounters a constraint it can prove is wrong should `flag` it immediately. A more thorough correction via `git chronicle correct` can be applied when the agent has identified the specific field and value to remove or amend.

---

## 7. LLM Skill Definition

The following skill definition is designed to be included in an agent's system prompt or skill library. It teaches the agent when and how to use `git chronicle read`.

---

```markdown
# Skill: Chronicle — Semantic Code Context

## What It Is

Chronicle annotations capture the reasoning, intent, constraints, and semantic
dependencies behind code changes at commit time. The `git chronicle read` CLI
retrieves these annotations for code you're about to work with.

## When To Use It

**Always use before modifying existing code.** Before changing any function,
struct, module, or configuration, run `git chronicle read` to understand why the
code exists in its current form. This is especially important for:

- Code with non-obvious structure or naming
- Code that interacts with external systems (APIs, hardware, protocols)
- Code that other modules depend on
- Code you didn't write (or don't remember writing)

**Always use before deleting or refactoring code.** Run `git chronicle deps` to
check if other code declares dependencies on what you're about to change.

**Use `git chronicle summary` when orienting on a new file or module.** Get a
high-level map of intent and constraints before diving into specifics.

**Use `git chronicle history` when debugging.** If current behavior is surprising,
the annotation timeline can reveal the reasoning chain that produced it.

## When Not To Use It

- New files you're creating from scratch (no annotations exist yet)
- Trivial changes (fixing a typo, updating a version number)
- When you just ran it and the code hasn't changed

## Commands

### Read annotations for a function
```
git chronicle read src/mqtt/client.rs MqttClient::connect
```
Returns intent, reasoning, constraints, dependencies, and risk notes for the
specified function. Follow `related` entries for connected reasoning.

### Read annotations for a line range
```
git chronicle read src/mqtt/client.rs --lines 42:67
```
Use when you don't know the function name, or the relevant code spans multiple
functions.

### Read annotations for an entire file
```
git chronicle read src/mqtt/client.rs
```
Returns annotations for all annotated regions in the file. Use `--max-regions`
to limit output if the file is large.

### Check what depends on this code
```
git chronicle deps src/tls/session.rs TlsSessionCache::max_sessions
```
**Critical before modifying any function's behavior or signature.** Returns
annotations from other code that explicitly declares assumptions about this
function. Failing to check this is the primary cause of regressions.

### Get the reasoning timeline
```
git chronicle history src/mqtt/client.rs MqttClient::connect --limit 5
```
Returns the chain of annotations across commits that have modified this
function. Useful for understanding how and why the code evolved.

### Broad orientation on a module
```
git chronicle summary src/mqtt/
```
Returns condensed intent + constraints for each annotated unit in the module.

### Multi-file context for cross-cutting changes
```
git chronicle read src/mqtt/client.rs src/tls/session.rs src/mqtt/reconnect.rs
```
Returns annotations for all specified files with cross-file dependencies and
cross-cutting concerns surfaced automatically.

### Controlling output size
```
git chronicle read src/mqtt/client.rs --max-tokens 2000
```
Limits output to approximately 2000 tokens. Use this when your context window
is constrained or you're querying broad scopes. When output is trimmed, older
annotations are kept and newer ones are dropped. This is intentional — older
annotations explain why the code was originally designed this way. Check the
`trimmed` field in the output — if regions were dropped, you can query them
individually if needed.

```
git chronicle read src/mqtt/client.rs connect --max-tokens 1000
```

## Reading the Output

The default output is markdown — a structured, token-efficient format designed
for LLM consumption. Use `--format json` when you need programmatic parsing.
Key fields to pay attention to:

- **`confidence`**: 0.0–1.0 score. Below 0.5, treat the annotation as a hint
  rather than a fact — the code may have evolved since the annotation was
  written.
- **`context_level`**: `enhanced` means the original author provided explicit
  reasoning. `inferred` means the annotation was generated from diff analysis
  alone. Prefer `enhanced` when reasoning conflicts.
- **`constraints`**: Invariants this code protects. Violating these is the most
  common source of subtle bugs. Read every constraint before modifying the code.
- **`dependencies_on_this`**: Other code that will break if you change this
  function's behavior. Check every entry.
- **`cross_cutting`**: Groups of code that must be updated together. If you're
  modifying one member of a cross-cutting group, you likely need to modify the
  others.
- **`risk_notes`**: Free-text warnings from the original author. Read these.
- **`related`**: Linked annotations on other commits. Follow these when you
  need deeper context on why a decision was made.
- **`trimmed`**: Present when output was truncated by `--max-tokens`. Shows how
  many regions were dropped and which commits were excluded. If you need context
  from dropped commits, query them individually without the token limit.
- **`confidence_factors`**: Breaks down the confidence score into `recency`,
  `context_level`, `anchor_match`, and `provenance`. Use these to make
  nuanced trust decisions — e.g., an old-but-enhanced annotation may be more
  trustworthy than a recent-but-inferred one.

**Note on trimming:** When output is trimmed, older annotations are kept and
newer ones are dropped. This is intentional — older annotations explain why the
code was originally designed this way. Newer annotations are more likely to be
incremental refinements that can be queried separately if needed.

## After Making Changes

Use `git chronicle commit` to commit with annotation context:

```bash
git chronicle commit -m "add connection pooling to MQTT client" \
  --reasoning "Chose bounded pool with LRU eviction because..." \
  --dependencies "Assumes max_sessions in TlsSessionCache is 4"
```

This passes your reasoning directly to the Writing Agent, which creates
richer annotations. If you relied on Chronicle annotations during your work,
reference them: the Writing Agent will link your new annotation to the ones
you built on via `related_annotations`.
```

---

## 8. Integration Patterns

### 8.1 Claude Code / Agentic Coding Tools

The primary integration is an agent that:

1. Receives a task.
2. Identifies the files and functions it needs to modify.
3. Runs `git chronicle read` or `git chronicle deps` for each.
4. Incorporates the annotations into its planning context.
5. Makes changes.
6. Commits via `git chronicle commit` with reasoning and dependency context.
7. The Writing Agent annotates the commit.

Steps 3–4 are where `git chronicle read` adds the most value. The annotations become part of the agent's working context alongside the code itself.

### 8.2 MCP Server

Chronicle will ship an MCP server that exposes `chronicle_read`, `chronicle_deps`, `chronicle_history`, and `chronicle_summary` as tools any MCP-connected agent can call directly. This is the preferred integration path — agents call Chronicle as a tool rather than shelling out to the CLI.

Installation:

```bash
chronicle mcp install
```

This registers the MCP server in the agent's MCP configuration. The tool interface mirrors the CLI: same arguments, same JSON output schema, same filtering and depth options.

### 8.3 Editor Integration (Future)

An LSP-adjacent service that surfaces Chronicle annotations as inline hints, hover information, or diagnostic warnings ("3 other modules depend on this function's behavior"). Out of scope for the initial implementation but the CLI provides the data layer.

---

## 9. Crate Integration

The read functionality lives in the same `chronicle` binary as the write side. The relevant additions to the crate structure:

```
chronicle/
├── src/
│   ├── read.rs                 # Core retrieval pipeline (blame → notes → filter → score)
│   ├── read_deps.rs            # Dependency inversion queries
│   ├── read_history.rs         # Timeline reconstruction
│   ├── scoring.rs              # Confidence scoring logic
│   ├── anchor_resolve.rs       # Tree-sitter anchor name → line range resolution
│   └── output.rs               # Output formatting (markdown, JSON, pretty-print)
```

Shares `ast_outline.rs`, `storage.rs`, `schema.rs`, and `config.rs` with the write side.

---

## 10. Open Questions

1. **Dependency scan performance.** The `dependencies_on_this` query needs to search annotations beyond the blame set. For large repositories with thousands of annotated commits, a linear scan is slow. A reverse index (maintained by the Writing Agent at annotation time) would make this fast but adds write-side complexity. Is the linear scan acceptable for v1?

2. **Partial file blame.** `git blame` on a 10,000-line generated file is wasteful if the agent only cares about lines 50-60. Scoping blame to a line range is supported by `git blame -L`, but `gix` may not support this natively. May need to shell out for scoped blame.

3. **Annotation staleness beyond confidence scoring.** If a function's signature has changed but the annotation still references the old signature, should `git chronicle read` attempt to surface this as an explicit warning? Or is the reduced confidence score sufficient?

4. **Trimming heuristic refinement.** The "newest commits first" trimming strategy is a good default, but some newer annotations may be more important than older ones (e.g., a recent annotation that says "WARNING: this function is being deprecated, use X instead"). Should the trimming order factor in annotation content signals beyond timestamp? Or is keeping the strategy simple and predictable more valuable?