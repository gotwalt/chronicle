# Feature 05: Writing Agent

## Overview

The writing agent is the core intelligence of Ultragit's write path. It is a tool-using LLM agent that receives a git commit and produces structured annotations — one per semantic code unit affected by the commit — stored as git notes under `refs/notes/ultragit`.

The agent is not a procedural analyzer. It is given tools to inspect the diff, read files, examine AST structure, and retrieve existing annotations, then makes judgment calls about what to annotate, at what depth, and how to connect new annotations to existing ones. The output is a well-structured JSON document conforming to the `ultragit/v1` schema.

This feature encompasses the system prompt, tool definitions, agent loop orchestration, annotation schema production, token budget management, and large-diff handling. It depends on the LLM provider layer (Feature 04) for model communication, the AST parser (Feature 03) for code structure, and the git operations layer (Feature 02) for diff extraction, file reading, and notes storage.

---

## Dependencies

| Feature | What it provides |
|---------|-----------------|
| 01 CLI & Config | Configuration types, CLI argument parsing |
| 02 Git Operations | Diff extraction, file content at commit, notes read/write, commit metadata |
| 03 AST Parsing | `get_ast_outline()` — structural outline of affected files |
| 04 LLM Providers | `LlmProvider` trait, `CompletionRequest`/`CompletionResponse`, `ContentBlock`, tool-use normalization |

---

## Public API

### Agent Entrypoint

```rust
/// Run the annotation agent for a single commit.
/// Returns the produced annotation, which the caller stores as a git note.
pub async fn annotate_commit(
    commit_sha: &str,
    context: &AnnotationContext,
    provider: &dyn LlmProvider,
    config: &UltragitConfig,
) -> Result<Annotation, AgentError>;
```

### Context Bundle

```rust
/// Everything the agent needs to annotate a commit.
/// Assembled by the gather phase before the agent loop starts.
pub struct AnnotationContext {
    /// The commit being annotated.
    pub commit: CommitInfo,
    /// Per-file diffs with hunks and line numbers.
    pub diffs: Vec<FileDiff>,
    /// Full content of affected files at HEAD.
    pub file_contents: HashMap<String, String>,
    /// AST outlines for affected files (if parseable).
    pub ast_outlines: HashMap<String, Vec<AstUnit>>,
    /// Author-provided context from pending-context.json or ULTRAGIT_* env vars.
    pub author_context: Option<AuthorContext>,
    /// Existing annotations on recent commits touching these files.
    /// Used by the agent to populate `related_annotations`.
    pub recent_annotations: Vec<ExistingAnnotation>,
}

pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub parent_shas: Vec<String>,
}

pub struct AuthorContext {
    pub task: Option<String>,
    pub reasoning: Option<String>,
    pub dependencies: Option<String>,
    pub tags: Option<Vec<String>>,
    pub squash_sources: Option<Vec<String>>,
}

pub struct ExistingAnnotation {
    pub commit_sha: String,
    pub annotation: Annotation,
}
```

### Annotation Schema Types

```rust
/// The top-level annotation document stored as a git note.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    #[serde(rename = "$schema")]
    pub schema: String, // "ultragit/v1"
    pub commit: String,
    pub timestamp: String,
    pub task: Option<String>,
    pub summary: String,
    pub context_level: ContextLevel,
    pub regions: Vec<RegionAnnotation>,
    pub cross_cutting: Vec<CrossCuttingConcern>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextLevel {
    Enhanced,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionAnnotation {
    pub file: String,
    pub ast_anchor: AstAnchor,
    pub lines: LineRange,
    pub intent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub semantic_dependencies: Vec<SemanticDependency>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_annotations: Vec<RelatedAnnotation>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstAnchor {
    #[serde(rename = "type")]
    pub unit_type: String, // "function", "method", "struct", "impl", etc.
    pub name: String,       // e.g., "MqttClient::connect"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    pub text: String,
    pub source: ConstraintSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintSource {
    Author,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticDependency {
    pub file: String,
    pub anchor: String,
    pub nature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedAnnotation {
    pub commit: String,
    pub anchor: String,
    pub relationship: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossCuttingConcern {
    pub description: String,
    pub regions: Vec<String>, // "file:anchor" format
    pub nature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub operation: ProvenanceOperation,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    pub original_annotations_preserved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthesis_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceOperation {
    Initial,
    Squash,
    Amend,
}
```

### Error Types

```rust
use snafu::{Snafu, ResultExt, Location};

#[derive(Debug, Snafu)]
pub enum AgentError {
    #[snafu(display("Provider error, at {location}"))]
    Provider {
        #[snafu(source)]
        source: ProviderError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Agent produced no annotations after {turns} turns, at {location}"))]
    NoAnnotations {
        turns: u32,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Agent exceeded maximum turns ({max_turns}), at {location}"))]
    MaxTurnsExceeded {
        max_turns: u32,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Agent exceeded token budget ({budget} tokens), at {location}"))]
    TokenBudgetExceeded {
        budget: u32,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Failed to parse annotation from agent output: {message}, at {location}"))]
    InvalidAnnotation {
        message: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Git operation failed: {message}, at {location}"))]
    Git {
        message: String,
        #[snafu(implicit)]
        location: Location,
    },
}
```

---

## Internal Design

### System Prompt

The system prompt is constructed from a template with context-level-specific additions:

```
You are a code annotation agent for Ultragit. Your job is to analyze a git commit
and produce structured metadata that captures the reasoning, intent, constraints,
and semantic dependencies behind the changes.

Your audience is a future AI agent that will modify this code. Write for machines:
be precise, structured, and comprehensive. Do not write prose or documentation.

For each meaningful semantic unit affected by the commit (function, method, struct,
type, impl block, configuration stanza), produce one annotation via the
emit_annotation tool. Annotations must include:

- intent: What the change accomplishes in the context of the broader task.
  Not "added retry logic" but "the cloud MQTT broker silently drops idle connections
  after 30 minutes; this implements application-level heartbeats."
- constraints: Invariants this code protects or assumes. State explicitly.
  Mark each constraint with source "author" if it comes from the provided
  task context, "inferred" if you deduced it from code analysis.
- semantic_dependencies: Non-obvious couplings to other code. Things that
  imports and call graphs don't reveal.
- reasoning: What alternatives existed and why this path was chosen.
  Only include when the choice is non-obvious.
- risk_notes: Anything a future modifier should be cautious about.

Skip trivial changes: formatting, import reordering, whitespace, version bumps.
Only annotate changes where a future agent would benefit from knowing the reasoning.

Use the provided tools to inspect the diff, read file contents, examine AST
structure, and check existing annotations. Do not guess about code you haven't read.
```

**When `context_level` is `enhanced`** (author context is present), append:

```
The commit author has provided explicit context:
  Task: {task}
  Reasoning: {reasoning}
  Dependencies: {dependencies}
  Tags: {tags}

This is high-value information. Incorporate it into annotations and mark
constraints derived from it as source: "author". This context has been verified
by the person who made the change — weight it heavily.
```

**When `context_level` is `inferred`** (no author context), append:

```
No author-provided context is available. All annotations will be inferred from
diff analysis, code structure, commit message, and surrounding context. Mark all
constraints as source: "inferred". Be more conservative in your claims — state
when you are uncertain.
```

### Tool Definitions

The agent has access to six read tools and one write tool:

#### `get_diff()`

Returns the full unified diff for the commit, parsed into per-file hunks.

```json
{
  "name": "get_diff",
  "description": "Returns the unified diff for this commit, organized by file with line numbers.",
  "input_schema": {
    "type": "object",
    "properties": {},
    "required": []
  }
}
```

**Implementation:** Returns the pre-computed diff from `AnnotationContext.diffs`, formatted as unified diff text with file headers and hunk markers. If the diff exceeds the token budget allocated for tool responses, return a summary with per-file statistics and the agent can request individual files.

#### `get_file_content(path)`

Returns the full content of a file at HEAD, with line numbers.

```json
{
  "name": "get_file_content",
  "description": "Returns the full content of a file at the commit being annotated, with line numbers.",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "Relative file path from repository root."
      }
    },
    "required": ["path"]
  }
}
```

**Implementation:** Lookup in `AnnotationContext.file_contents`. If not found (file not affected by commit), read from git at the commit SHA. Return with line numbers prepended (`{line_number}| {content}`).

#### `get_ast_outline(path)`

Returns the tree-sitter-derived structural outline of a file.

```json
{
  "name": "get_ast_outline",
  "description": "Returns the structural outline of a file: functions, types, methods, impl blocks with names, signatures, and line ranges.",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "Relative file path from repository root."
      }
    },
    "required": ["path"]
  }
}
```

**Implementation:** Lookup in `AnnotationContext.ast_outlines`. If no outline is available (unsupported language), return a message stating that AST parsing is not available for this file type and suggest using line ranges instead.

#### `get_surrounding_context(path, line, radius)`

Returns lines around a specific point in a file.

```json
{
  "name": "get_surrounding_context",
  "description": "Returns lines around a specific point in a file. Useful for understanding the neighborhood of a change without reading the entire file.",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": {
        "type": "string",
        "description": "Relative file path from repository root."
      },
      "line": {
        "type": "integer",
        "description": "Center line number."
      },
      "radius": {
        "type": "integer",
        "description": "Number of lines above and below to include. Default: 20."
      }
    },
    "required": ["path", "line"]
  }
}
```

**Implementation:** Slice `file_contents[path]` from `max(1, line - radius)` to `min(total_lines, line + radius)`. Return with line numbers.

#### `get_commit_info()`

Returns commit metadata and author-provided context.

```json
{
  "name": "get_commit_info",
  "description": "Returns the commit message, author, timestamp, parent SHAs, and any author-provided context (task, reasoning, dependencies, tags).",
  "input_schema": {
    "type": "object",
    "properties": {},
    "required": []
  }
}
```

**Implementation:** Serialize `AnnotationContext.commit` and `AnnotationContext.author_context` as a formatted text block.

#### `get_recent_annotations(file, n)`

Returns existing annotations on recent commits touching a file.

```json
{
  "name": "get_recent_annotations",
  "description": "Returns Ultragit annotations from the most recent N commits that touched the given file. Use this to discover existing annotations and create related_annotations links.",
  "input_schema": {
    "type": "object",
    "properties": {
      "file": {
        "type": "string",
        "description": "Relative file path."
      },
      "n": {
        "type": "integer",
        "description": "Maximum number of recent annotations to return. Default: 5."
      }
    },
    "required": ["file"]
  }
}
```

**Implementation:** Filter `AnnotationContext.recent_annotations` by file path, take the most recent `n`. Return as formatted JSON. If no recent annotations exist, return an empty array with a note.

#### `emit_annotation(annotation)`

Emits a single region annotation. The agent calls this once per semantic unit.

```json
{
  "name": "emit_annotation",
  "description": "Emit a structured annotation for one semantic code unit. Call this for each function, type, or code region affected by the commit that warrants annotation. Skip trivial changes.",
  "input_schema": {
    "type": "object",
    "properties": {
      "file": { "type": "string" },
      "ast_anchor": {
        "type": "object",
        "properties": {
          "type": { "type": "string", "enum": ["function", "method", "struct", "class", "impl", "module", "const", "type", "config"] },
          "name": { "type": "string" },
          "signature": { "type": "string" }
        },
        "required": ["type", "name"]
      },
      "lines": {
        "type": "object",
        "properties": {
          "start": { "type": "integer" },
          "end": { "type": "integer" }
        },
        "required": ["start", "end"]
      },
      "intent": { "type": "string" },
      "reasoning": { "type": "string" },
      "constraints": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "text": { "type": "string" },
            "source": { "type": "string", "enum": ["author", "inferred"] }
          },
          "required": ["text", "source"]
        }
      },
      "semantic_dependencies": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "file": { "type": "string" },
            "anchor": { "type": "string" },
            "nature": { "type": "string" }
          },
          "required": ["file", "anchor", "nature"]
        }
      },
      "related_annotations": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "commit": { "type": "string" },
            "anchor": { "type": "string" },
            "relationship": { "type": "string" }
          },
          "required": ["commit", "anchor", "relationship"]
        }
      },
      "tags": { "type": "array", "items": { "type": "string" } },
      "risk_notes": { "type": "string" }
    },
    "required": ["file", "ast_anchor", "lines", "intent"]
  }
}
```

**Implementation:** Validate the annotation against the schema, store it in a collector. Return a confirmation: `"Annotation emitted for {file}:{name}"`. The collector accumulates all emitted annotations until the agent loop completes. Cross-cutting concerns are emitted separately via an optional `emit_cross_cutting` tool, or the agent can include them in a final text message that is parsed post-loop.

#### `emit_cross_cutting(concern)` (optional)

```json
{
  "name": "emit_cross_cutting",
  "description": "Emit a cross-cutting concern that spans multiple code regions. These are groups of code that must be updated together.",
  "input_schema": {
    "type": "object",
    "properties": {
      "description": { "type": "string" },
      "regions": { "type": "array", "items": { "type": "string" } },
      "nature": { "type": "string" }
    },
    "required": ["description", "regions", "nature"]
  }
}
```

### Agent Loop

The agent loop is a standard tool-use conversation loop:

```
1. Build initial messages:
   - System prompt (with context-level additions)
   - User message: "Analyze the following commit and emit annotations for
     each meaningful semantic unit affected."

2. Call provider.complete(request)

3. Process response:
   a. For each ContentBlock::Text — log (agent thinking/reasoning)
   b. For each ContentBlock::ToolUse — dispatch to tool handler:
      - get_diff → return diff text
      - get_file_content → return file content
      - get_ast_outline → return outline
      - get_surrounding_context → return context lines
      - get_commit_info → return commit metadata
      - get_recent_annotations → return recent annotations
      - emit_annotation → validate and collect
      - emit_cross_cutting → validate and collect
   c. If stop_reason == ToolUse:
      - Append assistant message (model's response) to conversation
      - Append tool result messages
      - Go to step 2
   d. If stop_reason == EndTurn or MaxTokens:
      - Agent is done. Proceed to assembly.

4. Assemble the Annotation document:
   - Populate $schema, commit, timestamp, context_level
   - Set task from author_context if present
   - Generate summary from intent fields (or from a final text block)
   - Collect all emitted region annotations into regions[]
   - Collect all emitted cross-cutting concerns into cross_cutting[]
   - Set provenance to { operation: "initial", derived_from: [], ... }
   - Validate the assembled annotation
   - Return it
```

**Maximum turns:** Cap the loop at 20 turns (each turn = one `complete` call). If the agent hasn't emitted all annotations by then, assemble what was collected and log a warning. This prevents runaway conversations from burning tokens.

**Turn counting:** Each `complete` call counts as one turn, regardless of how many tool calls are in the response.

### Enhanced vs. Inferred Context Handling

**Enhanced** (`author_context` is present):
- Author context is included in the system prompt.
- The agent can use `get_commit_info()` to see it at any time.
- Constraints from author context should be marked `source: "author"`.
- The agent is instructed to weight author-provided reasoning heavily.
- Annotations are richer — the agent has access to rejected alternatives, explicit dependency declarations, task context.

**Inferred** (no author context):
- System prompt instructs more conservative annotation.
- All constraints are `source: "inferred"`.
- The agent infers intent from: diff structure, naming, commit message, code patterns, AST structure, surrounding context.
- Annotations are still valuable but less deep. The agent should state uncertainty when appropriate.

### Constraint Source Field

The `source` field on constraints distinguishes provenance:

- `"author"` — Derived from the committing agent's explicit `--reasoning`, `--dependencies`, or `--task` input. High confidence.
- `"inferred"` — Deduced by the annotation agent from code analysis. Lower confidence; may be wrong.

The reading agent (Feature 07) weights `"author"` constraints more heavily in confidence scoring.

### Token Budget Management

The agent operates within the LLM's context window. For large commits, the full context (diff + file contents + AST outlines + recent annotations) may exceed the window.

**Budget allocation:**
- System prompt: ~500 tokens (fixed)
- Diff: variable. Cap at 50% of remaining budget.
- File contents: loaded on-demand via tools (not pre-loaded into context).
- AST outlines: small (~100-500 tokens per file).
- Recent annotations: cap at 2,000 tokens.
- Agent reasoning + tool calls: remaining budget.

**When the diff exceeds the budget:**

1. **Chunking by file:** If the commit touches many files, process them in groups. Emit annotations for each group separately.

2. **Diff summarization:** For very large diffs (>2,000 lines), the initial `get_diff()` call returns a summary instead of the full diff:
   ```
   This commit modifies 47 files with 3,200 lines changed.
   Top files by change size:
     src/mqtt/client.rs: +120 -45
     src/tls/session.rs: +80 -20
     ...

   Use get_file_content(path) to examine specific files,
   or get_diff_for_file(path) to see the diff for a single file.
   ```

3. **Per-file diff tool:** Add an implicit `get_diff_for_file(path)` tool that returns the diff for a single file. This keeps the initial context small and lets the agent drill into specific files.

**Max tokens per response:** Set `CompletionRequest.max_tokens` to leave room for tool results in subsequent turns. A reasonable default is 4,096 output tokens per turn.

### Large Diff Chunking Strategy

For commits touching more than 10 files or exceeding 2,000 changed lines:

1. Sort files by change magnitude (added + deleted lines, descending).
2. Group files into chunks of ~5 files each, staying under 1,500 lines of diff per chunk.
3. Run the agent once per chunk, with the system prompt noting which chunk this is and how many remain.
4. Merge annotations from all chunks into a single `Annotation` document.
5. Run a final pass: check for cross-cutting concerns across chunks. If files in different chunks have related changes, the final pass may need one more agent call with just the summaries.

For most commits (under 10 files, under 500 lines), no chunking is needed and the agent processes everything in a single conversation.

### Annotation Validation

Before returning, validate the assembled `Annotation`:

- Every region has a non-empty `intent`.
- Every region has a valid `ast_anchor` with non-empty `name`.
- Every region has valid `lines` (start <= end, both > 0).
- Every constraint has a non-empty `text` and valid `source`.
- Every semantic dependency has non-empty `file`, `anchor`, `nature`.
- File paths in regions are relative and correspond to files in the commit.
- `$schema` is set to `"ultragit/v1"`.
- `commit` matches the input commit SHA.
- `timestamp` is valid ISO 8601.

Validation failures log warnings but don't fail the annotation — partial annotations are better than no annotations. Strip invalid regions and proceed.

---

## Error Handling

| Failure Mode | Handling |
|-------------|----------|
| Provider returns error | Propagate `ProviderError` wrapped in `AgentError::Provider`. Caller decides whether to retry or log. |
| Agent emits no annotations after max turns | Return `AgentError::NoAnnotations`. Caller logs to `failed.log`. May indicate a trivial commit that slipped through pre-filtering. |
| Agent exceeds max turns without finishing | Collect whatever annotations were emitted. Log warning. Return partial result. |
| Agent emits invalid annotation (missing required fields) | Log warning, skip the invalid region, continue. |
| Agent emits annotation for wrong file/commit | Log warning, skip. |
| Token budget exceeded mid-conversation | When provider returns `StopReason::MaxTokens`, collect emitted annotations and return partial result with a note. |
| Diff too large to fit in context | Apply chunking strategy. If individual files are too large, truncate to the changed hunks plus surrounding context. |
| AST parsing fails for a file | Agent falls back to line-range-only annotations for that file. Not a fatal error. |

---

## Configuration

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `maxDiffLines` | integer | 2000 | Skip annotation for commits with more total changed lines |
| `maxAgentTurns` | integer | 20 | Maximum tool-use conversation turns |
| `maxTokensPerResponse` | integer | 4096 | Max tokens per LLM response |
| `recentAnnotationsLimit` | integer | 5 | How many recent annotations per file to load for context |
| `chunkSize` | integer | 5 | Max files per chunk when processing large commits |

---

## Implementation Steps

### Step 1: Annotation schema types
- Define all schema types: `Annotation`, `RegionAnnotation`, `AstAnchor`, `Constraint`, `SemanticDependency`, `RelatedAnnotation`, `CrossCuttingConcern`, `Provenance`.
- Implement `Serialize`/`Deserialize`.
- Add validation function.
- Unit tests: round-trip serialization, validation catches missing fields.
- **PR scope:** `src/schema/annotation.rs`, `src/schema/region.rs`.

### Step 2: Context gathering
- Implement `AnnotationContext` assembly: given a commit SHA, gather diffs, file contents, AST outlines, author context (from `pending-context.json` or env vars), recent annotations.
- Wire together Feature 02 (git diff, file read, notes read) and Feature 03 (AST outline).
- Unit tests with a test repository: verify context is correctly assembled.
- **PR scope:** `src/annotate/gather.rs`.

### Step 3: Tool definitions and dispatch
- Define all tool JSON schemas.
- Implement tool dispatch: given a tool name and input JSON, execute the tool against `AnnotationContext` and return the result string.
- Implement annotation collector for `emit_annotation` results.
- Unit tests: each tool returns expected output for known input.
- **PR scope:** `src/agent/tools.rs`.

### Step 4: System prompt construction
- Implement prompt builder: base template + context-level additions + author context injection.
- Unit tests: verify prompt contains expected sections for enhanced vs. inferred.
- **PR scope:** `src/agent/prompt.rs`.

### Step 5: Agent loop
- Implement the core loop: build initial messages, call `provider.complete()`, process response, dispatch tool calls, iterate until done or max turns.
- Handle `StopReason::ToolUse`, `StopReason::EndTurn`, `StopReason::MaxTokens`.
- Track turn count, enforce limit.
- Unit tests with a mock provider: verify loop handles multi-turn conversations correctly.
- **PR scope:** `src/agent/mod.rs`.

### Step 6: Annotation assembly and validation
- After agent loop completes, assemble the `Annotation` document from collected regions and cross-cutting concerns.
- Populate provenance, schema version, commit, timestamp.
- Generate summary field.
- Run validation.
- Unit tests: verify assembly from collected annotations, verify validation catches errors.
- **PR scope:** Part of `src/agent/mod.rs` or `src/annotate/mod.rs`.

### Step 7: Large diff handling
- Implement diff size detection and chunking strategy.
- Implement `get_diff_for_file` tool for per-file diff retrieval.
- Implement diff summary for oversized commits.
- Implement chunk merging.
- Unit tests: verify chunking logic for various commit sizes.
- **PR scope:** `src/annotate/mod.rs`, tool additions to `src/agent/tools.rs`.

### Step 8: Structured-output fallback mode
- Implement the non-tool-use path: assemble full context upfront, construct a single prompt requesting JSON output, parse the response.
- Wire this as a fallback when `provider.supports_tool_use()` returns false.
- Unit tests with mock provider.
- **PR scope:** `src/agent/structured.rs`.

### Step 9: `ultragit annotate` CLI command
- Wire up the CLI subcommand: parse `--commit`, `--async`/`--sync`, `--squash-sources`.
- Call `annotate_commit()`, write result to git notes.
- Handle async spawning (for the post-commit hook path).
- **PR scope:** `src/cli/annotate.rs`.

### Step 10: Integration tests
- End-to-end test: create a test repository, make a commit, run `annotate_commit` with a mock provider, verify annotation is stored as a git note and is valid JSON.
- Test enhanced vs. inferred context levels.
- Test large diff chunking.
- **PR scope:** `tests/integration/annotate_test.rs`.

---

## Test Plan

### Unit Tests

**Schema types:**
- Round-trip serialization/deserialization for all annotation types.
- Validation: missing `intent` is caught, missing `ast_anchor.name` is caught, invalid `lines` is caught.
- Optional fields are correctly omitted from serialized JSON when empty.

**Tool dispatch:**
- `get_diff()` returns formatted diff.
- `get_file_content("src/main.rs")` returns file content with line numbers.
- `get_ast_outline("src/main.rs")` returns outline for supported languages, returns message for unsupported.
- `get_surrounding_context("src/main.rs", 50, 10)` returns lines 40-60.
- `get_commit_info()` includes author context when present, omits when absent.
- `get_recent_annotations("src/main.rs", 3)` returns at most 3 recent annotations.
- `emit_annotation(valid_annotation)` returns confirmation and stores annotation.
- `emit_annotation(invalid_annotation)` returns error and does not store.

**System prompt:**
- Enhanced context level includes author task and reasoning.
- Inferred context level includes conservative annotation instructions.
- Prompt is under 500 tokens for the base template.

**Agent loop:**
- Mock provider returns tool-use response, loop dispatches and continues.
- Mock provider returns end-turn, loop terminates.
- Mock provider returns max-tokens, loop terminates with partial result.
- Loop terminates after max turns with whatever was collected.
- Zero tool calls result in `AgentError::NoAnnotations`.

**Large diff handling:**
- Commit with 3 files, 100 lines: no chunking.
- Commit with 20 files, 3000 lines: chunked into groups.
- `get_diff()` returns summary for oversized commits.
- Chunk merging produces valid annotation.

### Integration Tests

- Full annotation pipeline with mock LLM: test repo → gather context → run agent → store note → read note back.
- Enhanced annotation: set pending-context.json, verify `context_level: "enhanced"` and author constraints.
- Inferred annotation: no context, verify `context_level: "inferred"` and inferred constraints.
- Multi-file commit: verify cross-file annotations and cross-cutting concerns.

### Edge Cases

- Empty commit (merge commit with no changes): agent should produce no annotations.
- Binary files in diff: skip them, annotate only text files.
- Commit deleting a file: agent notes the deletion but has limited context.
- Commit adding a new file: agent annotates new functions without recent annotation history.
- File with no AST support: annotations use line ranges instead of AST anchors.
- Agent calls a tool that doesn't exist: return error to agent, let it recover.
- Agent emits duplicate annotations for the same region: deduplicate in assembly.

---

## Acceptance Criteria

1. `annotate_commit()` produces a valid `ultragit/v1` JSON annotation for a typical commit (5-10 files, 100-300 changed lines) in under 30 seconds.
2. Enhanced annotations include author-provided task, reasoning, and constraints with `source: "author"`.
3. Inferred annotations produce useful intent and constraints with `source: "inferred"` from diff analysis alone.
4. The agent correctly uses tools to inspect diffs, read files, examine AST structure, and discover existing annotations.
5. `related_annotations` links are populated when recent annotations exist for affected files.
6. Cross-cutting concerns are emitted when the agent detects multi-region coordination requirements.
7. Large commits (>2,000 lines or >10 files) are handled via chunking without exceeding the context window.
8. The agent loop terminates within `maxAgentTurns` turns.
9. Invalid annotations are logged and skipped; partial results are returned rather than failing entirely.
10. The annotation is stored as a git note under `refs/notes/ultragit` and is retrievable via `git notes --ref=ultragit show <sha>`.
11. `provenance.operation` is set correctly: `"initial"` for fresh annotations, `"squash"` and `"amend"` for derived ones (though squash/amend assembly is Feature 09's responsibility, the schema supports it here).
