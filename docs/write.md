# Chronicle: Semantic Commit Annotation System

## Writing Agent — Implementation Document

---

## 1. Overview

Chronicle is a local post-commit hook, distributed as a single Rust binary, that captures the reasoning, intent, and semantic context behind code changes at the moment they are made. It uses an AI agent (supporting Anthropic, OpenAI, Gemini, and OpenRouter, with a preference for Anthropic and Claude) to analyze each commit and produce structured metadata annotations stored as git notes. These annotations are designed for machine retrieval by future agents working on related code.

This document covers the **Writing Agent** — the system responsible for capturing and storing annotations at commit time. A companion **Reading Agent** (documented separately) will be responsible for retrieving and surfacing this metadata when an agent needs context about existing code.

---

## 2. Problem Statement

When an AI agent modifies code, it holds rich context that does not survive the commit: the task it was working on, the alternatives it considered and rejected, the implicit constraints it discovered, the assumptions it made about surrounding code, and the semantic dependencies it is aware of between its changes and other parts of the system.

Commit messages capture a fraction of this — a one-line summary optimized for human scanning. The rest evaporates. A future agent working on related code must reconstruct this intent from scratch, often incorrectly, leading to regressions, violated invariants, and redundant exploration of previously-rejected approaches.

The gap is not in version control (git tracks *what* changed precisely) but in knowledge management (nobody tracks *why* with sufficient depth and structure for programmatic retrieval).

---

## 3. Goals

### 3.1 Primary Goals

**Capture intent at maximum context.** The post-commit hook fires while the authoring agent (or developer) still has full task context. The system must extract and preserve the reasoning behind changes at a granularity finer than the commit — ideally at the level of individual semantic units (functions, structs, impl blocks, configuration stanzas).

**Produce machine-readable annotations.** The output is not documentation for humans. It is structured metadata optimized for a future agent to query, filter, and incorporate into its own reasoning. The schema must be consistent enough for programmatic access while flexible enough to capture unbounded reasoning.

**Zero-friction adoption.** Installing Chronicle is a single binary download or `cargo install`. Activating it in a repository is a single command (`git chronicle init`). The hook must not meaningfully slow down the development workflow — Rust's sub-millisecond startup time ensures the hook adds negligible overhead before the async LLM call is dispatched. The system should degrade gracefully when the API is unavailable, credentials are missing, or the commit is trivial.

**Preserve context through history rewrites.** Git workflows routinely destroy history through squash merges and amends. Each of these operations orphans annotations attached to the original commits. The system must actively detect these events and migrate or synthesize annotations so that reasoning context survives the transition from feature branch to main and from draft commit to polished commit. This is a core objective, not an edge case — squash-merge is the dominant workflow for most teams, and it is the single largest source of context loss.

**Leverage existing git primitives.** Annotations are stored as git notes, keyed by commit SHA. This means the retrieval path is `git blame` → commit SHA → `git notes show`. No external database, no additional infrastructure, no services to run.

### 3.2 Successful Outcomes

1. Every non-trivial commit made by an agent or developer in a Chronicle-enabled repository has a corresponding structured annotation in `refs/notes/chronicle`.

2. Each annotation contains, at minimum: the affected file regions, the enclosing AST-level semantic unit for each region, a description of intent, and any decision rationale the authoring context provides.

3. When the authoring agent provides explicit reasoning context (via `git chronicle commit`, `git chronicle context set`, or environment variables), annotations are rich — capturing rejected alternatives, implicit dependencies, and constraints. When no explicit context is provided (human commits), annotations still provide useful inferred intent from diff analysis.

4. The hook completes within 10 seconds for typical commits (under 500 lines changed across fewer than 10 files). Larger commits may take longer but never block the terminal — the hook runs asynchronously by default.

5. Annotations are retrievable via `git notes show <commit-sha>` and parseable as JSON.

6. The tool is installable via `cargo install chronicle` or a single binary download, and activatable via a single CLI command (`git chronicle init`).

---

## 4. Technical Architecture

### 4.1 System Context

```
┌─────────────────────────────────────────────────────────────┐
│                    Developer / Agent                        │
│                                                             │
│  1. Makes code changes                                      │
│  2. Runs `git chronicle commit` (or `git commit` with env vars)  │
│                                                             │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│            .git/hooks/prepare-commit-msg                     │
│                                                             │
│  If squash/merge detected:                                  │
│    Write source commit SHAs to                              │
│    .git/chronicle/pending-squash.json                       │
│                                                             │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│                  .git/hooks/post-commit                      │
│                                                             │
│  Thin shell wrapper that invokes:                           │
│    git chronicle annotate --commit HEAD                         │
│                                                             │
│  If chronicle is not on PATH or fails, exits silently.      │
└──────────────────────┬──────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────┐
│                Chronicle Binary (Rust)                       │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  Phase 1: Gather                                            │
│    - Read commit diff (git diff HEAD~1..HEAD)               │
│    - Read affected file contents at HEAD                    │
│    - Parse AST of affected files (tree-sitter)              │
│    - Read agent context from pending-context.json or env    │
│    - Read commit message                                    │
│                                                             │
│  Phase 2: Annotate                                          │
│    - Discover LLM credentials (Anthropic → OpenAI →         │
│      Gemini → OpenRouter)                                   │
│    - Invoke agent via tool-use loop                         │
│    - Agent has tools to inspect diff, files, and AST        │
│    - Agent produces structured annotation per semantic unit │
│                                                             │
│  Phase 3: Store                                             │
│    - Serialize annotation as JSON                           │
│    - Write to git notes: refs/notes/chronicle               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 Components

#### 4.2.1 CLI Entrypoint (`chronicle`)

Chronicle is distributed as a single statically-linked Rust binary. Installation is one of:

- `cargo install chronicle`
- Download a prebuilt binary from GitHub Releases (Linux, macOS, Windows)
- `brew install chronicle` (if we publish a tap)

The binary provides the following subcommands:

`git chronicle init` — Installs the git hooks into the current repository's `.git/hooks/` directory. This installs multiple hooks:

- `post-commit` — primary hook; triggers annotation of new commits. Also checks for `.git/chronicle/pending-squash.json` to detect squash commits and enter the synthesis path.
- `prepare-commit-msg` — detects squash operations and writes source commit SHAs to `.git/chronicle/pending-squash.json` for the `post-commit` hook to consume.
- `post-rewrite` — fires after `amend` operations; receives old→new SHA mappings on stdin and migrates annotations from the pre-amend commit to the new one.

Each hook is a thin shell wrapper that calls the appropriate `chronicle` subcommand. The init command also configures the notes ref namespace and optionally sets up `.git/config` entries for notes push/fetch if the user wants to share annotations.

`git chronicle commit [git-commit-args] [--task <task>] [--reasoning <text>] [--dependencies <text>] [--tags <tags>]` — Wraps `git commit` with context capture. Writes the provided context to `.git/chronicle/pending-context.json`, invokes `git commit` with the message and any pass-through flags, and the post-commit hook reads and consumes the context file. This is the recommended way for agents to commit with context.

`git chronicle context set [--task <task>] [--reasoning <text>] [--dependencies <text>] [--tags <tags>]` — Writes context to `.git/chronicle/pending-context.json` without committing. The next `git commit` will pick up and consume the context via the post-commit hook. Useful for workflows where context gathering and committing are separate steps.

`git chronicle annotate --commit <sha>` — The core command. Analyzes the given commit and produces a structured annotation stored as a git note. This is what the hook calls, but it can also be invoked manually to annotate historical commits or re-annotate after a failed run.

`git chronicle annotate-range --since <sha|date>` — Batch mode. Walks a range of commits and annotates each one. Useful for bootstrapping annotations on an existing repository.

`git chronicle show <sha>` — Convenience wrapper around `git notes --ref=chronicle show <sha>` with pretty-printing. (Minimal; the Reading Agent will provide the real query interface.)

`git chronicle config` — Manages Chronicle-specific configuration (model selection, async behavior, file path filters, etc.) stored in `.git/config` under a `[chronicle]` section.

#### 4.2.2 Context Gathering

Before invoking the agent, the CLI assembles a context bundle:

**Diff extraction.** Uses `gix` (gitoxide) to compute the diff between HEAD and its parent. For merge commits, diffs against the first parent. The diff is parsed into per-file hunks with line numbers.

**File content.** Reads the full content of each affected file at HEAD. The agent needs more than just the diff — it needs surrounding context to understand the role of a changed function within its module.

**AST parsing.** Each affected file is parsed using tree-sitter with the appropriate language grammar. The parse extracts an outline of semantic units: functions, methods, structs, classes, impl blocks, constants, type definitions, and their line ranges. This gives the agent a structural map of the file and lets it anchor annotations to named code elements rather than brittle line numbers.

**Commit metadata.** The commit message, author, timestamp, and parent SHA(s).

**Agent-provided context (optional).** The authoring agent can provide structured reasoning context through `git chronicle commit`, which wraps `git commit` with context capture:

```bash
# Primary: git chronicle commit wraps git commit with context capture
git chronicle commit -m "add connection pooling to MQTT client" \
  --task "PROJ-442: implement connection pooling" \
  --reasoning "Chose bounded pool with LRU eviction because the device has limited RAM and connections are long-lived" \
  --dependencies "Assumes max_sessions in TlsSessionCache is 4" \
  --tags "mqtt,performance"
```

`git chronicle commit` writes context to `.git/chronicle/pending-context.json`, calls `git commit` with the message and any pass-through flags, and the post-commit hook reads and consumes the context file. This is atomic (no ambient state leakage), avoids shell quoting issues, and can't be "forgotten" since the context is part of the commit command itself.

For workflows where context is set separately from the commit:

```bash
git chronicle context set --task "PROJ-442" --reasoning "Chose bounded pool..."
git commit -m "add connection pooling"
# Hook reads and consumes .git/chronicle/pending-context.json automatically
```

**Fallback: Environment Variables.** For CI pipelines and environments where `git chronicle commit` is not available, context can be provided via environment variables:

| Variable | Purpose |
|---|---|
| `CHRONICLE_TASK` | Task identifier or description (e.g., "PROJ-442: implement mTLS for MQTT broker connections") |
| `CHRONICLE_REASONING` | Free-text reasoning from the agent — rejected alternatives, constraints discovered, tradeoffs made |
| `CHRONICLE_DEPENDENCIES` | Explicit list of semantic dependencies the agent is aware of (e.g., "assumes session cache max size of 4 in src/tls/session.rs") |
| `CHRONICLE_TAGS` | Comma-separated tags for categorization |
| `CHRONICLE_SQUASH_SOURCES` | Commit range or list of SHAs being squashed (e.g., "main..feature-branch" or "abc123,def456,ghi789"). Enables annotation synthesis from source commits. |

Agent-provided context — whether via `git chronicle commit`, `git chronicle context set`, or environment variables — is the highest-value input to the system. When present, the annotation is marked `context_level: "enhanced"` and the agent can produce significantly richer output than diff analysis alone.

When no context is provided (typically a human-initiated commit), the hook still fires and the agent annotates the commit from diff analysis, file context, and AST structure alone. The annotation is marked `context_level: "inferred"`. These annotations are less rich but still valuable — the agent can infer a great deal about intent from the structure of a diff, the names chosen, the patterns used, and the commit message. The Reading Agent can use the context level to weight annotations appropriately when multiple sources of reasoning exist for a code region.

#### 4.2.3 LLM Provider Layer

Chronicle uses a multi-provider LLM client that discovers credentials automatically and normalizes the request/response format across providers. All providers expose HTTP JSON APIs with similar structures — the abstraction is thin.

**Credential discovery chain (first match wins):**

| Priority | Credential Source | Provider | Notes |
|---|---|---|---|
| 1 | `ANTHROPIC_API_KEY` env var | Anthropic API | Preferred provider |
| 2 | `~/.config/claude/credentials` | Claude subscription | Picks up credentials from Claude Code / Claude CLI |
| 3 | `OPENAI_API_KEY` env var | OpenAI API | |
| 4 | `GOOGLE_API_KEY` or `GEMINI_API_KEY` env var | Google Gemini | |
| 5 | `OPENROUTER_API_KEY` env var | OpenRouter | Fallback; routes to any model |
| 6 | `CHRONICLE_API_KEY` + `CHRONICLE_PROVIDER` env vars | Explicit override | User specifies both key and provider |

The config can also pin a provider and model explicitly:

```ini
[chronicle]
    provider = anthropic
    model = claude-sonnet-4-5-20250929
```

Each provider module implements a common trait:

```rust
trait LlmProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
    fn supports_tool_use(&self) -> bool;
    fn name(&self) -> &str;
}
```

The `CompletionRequest` includes messages, tool definitions, and system prompt. The provider modules handle serialization differences (Anthropic's `messages` API vs OpenAI's `chat/completions` vs Gemini's `generateContent`). Tool-use flows (for the annotation agent's tools) map naturally across all three — the wire format differs but the semantics are equivalent.

For providers that don't support tool use (or if we want to simplify), the agent can fall back to a structured-output-only mode where the prompt asks for JSON directly and the response is parsed.

**Implementation:** The HTTP layer is `reqwest` with `tokio` for async. Provider-specific serialization uses `serde` with per-provider request/response types. No third-party SDK dependencies — the APIs are simple enough that a thin client is more maintainable and avoids version churn.

#### 4.2.4 Annotation Agent

The core annotation logic is implemented as a tool-using agent built on the LLM provider layer. Rather than writing procedural analysis code, we give the model a set of tools and a clear directive, and let it make judgment calls about what's worth annotating and at what depth.

**System prompt (conceptual):**

> You are a code annotation agent. Your job is to analyze a git commit and produce structured metadata that will help a future AI agent understand the intent, reasoning, and semantic context behind the changes. You are writing for machines, not humans — be precise, structured, and comprehensive. Annotate at the granularity of semantic code units (functions, types, modules). Skip trivial changes (formatting, import reordering) unless they have non-obvious implications.

**Tools provided to the agent:**

`get_diff()` → Returns the full unified diff for the commit, parsed into per-file hunks with line numbers.

`get_file_content(path: str)` → Returns the full content of a file at HEAD, with line numbers.

`get_ast_outline(path: str)` → Returns the tree-sitter-derived structural outline of a file: a list of semantic units with their type (function, struct, impl, etc.), name, signature, and line range.

`get_surrounding_context(path: str, line: int, radius: int)` → Returns lines around a specific point in a file. Useful for understanding the neighborhood of a change.

`get_commit_info()` → Returns commit message, author, timestamp, parent SHAs, any agent-provided context (from `pending-context.json` or `CHRONICLE_*` environment variables), and the context level (`enhanced` if agent context is present, `inferred` otherwise).

`get_recent_annotations(file: str, n: int)` → Returns the Chronicle annotations from the most recent `n` commits that touched the given file. This allows the agent to discover existing annotations and populate `related_annotations` references — linking its new annotation to prior reasoning about the same code.

`emit_annotation(annotation: dict)` → Writes a structured annotation for one semantic unit. The agent calls this once per unit it wants to annotate. The schema is defined in Section 4.3.

The agent processes the commit by examining the diff, using the AST outline to identify which semantic units were affected, reading file context as needed, and emitting annotations for each meaningful change. It has latitude to decide that some changes don't warrant annotation (a typo fix in a comment, a trivial rename) and to group related changes across files when they form a logical unit.

For providers that support tool use (Anthropic, OpenAI, Gemini), the agent runs as a standard tool-use loop: the model is called with the tools defined, it returns tool-use requests, Chronicle executes them locally and returns results, and the loop continues until the model emits all annotations. For providers without tool use, the full context (diff, AST outlines, commit info) is assembled upfront and included in a single prompt that requests structured JSON output.

#### 4.2.5 Storage Layer

Annotations are stored as git notes under the ref `refs/notes/chronicle`.

Each commit gets a single JSON document as its note, containing an array of region annotations plus commit-level metadata. Notes are written via `gix` (gitoxide) operations against the notes ref, with a fallback to shelling out to `git notes` if needed:

```
git notes --ref=chronicle add -f -m '<json>' <commit-sha>
```

The `-f` (force) flag allows re-annotation of a commit if the tool is run again (idempotent).

Notes live in the local repository by default. For shared environments, users can configure push/fetch:

```
git config --add remote.origin.push refs/notes/chronicle
git config --add remote.origin.fetch +refs/notes/chronicle:refs/notes/chronicle
```

The `git chronicle init` command will offer to configure this.

### 4.3 Annotation Schema

```json
{
  "$schema": "chronicle/v1",
  "commit": "<sha>",
  "timestamp": "<iso8601>",
  "task": "<task identifier or null>",
  "summary": "<one-paragraph commit-level summary of the change's purpose>",

  "context_level": "<enhanced|inferred>",

  "regions": [
    {
      "file": "<relative file path>",
      "ast_anchor": {
        "type": "<function|method|struct|class|impl|module|const|type|config>",
        "name": "<qualified name, e.g., MqttClient::connect>",
        "signature": "<full signature if applicable>"
      },
      "lines": {
        "start": 42,
        "end": 67
      },

      "intent": "<what this change does and why, in the context of the broader task>",

      "reasoning": "<decisions made, alternatives considered, why this approach was chosen>",

      "constraints": [
        {
          "text": "<invariant this code protects or assumes>",
          "source": "<author|inferred>"
        }
      ],

      "semantic_dependencies": [
        {
          "file": "<path>",
          "anchor": "<name of the depended-upon unit>",
          "nature": "<what the dependency is — 'assumes max 4 sessions', 'must be called after init()', etc.>"
        }
      ],

      "related_annotations": [
        {
          "commit": "<sha of the related commit>",
          "anchor": "<file:name of the related region>",
          "relationship": "<e.g., 'follow-up to', 'reverses', 'extends', 'depends on reasoning in', 'must be updated in tandem with'>"
        }
      ],

      "tags": ["<category tags>"],

      "risk_notes": "<anything a future modifier should be cautious about>"
    }
  ],

  "cross_cutting": [
    {
      "description": "<description of a concern that spans multiple regions>",
      "regions": ["<file:anchor>", "<file:anchor>"],
      "nature": "<e.g., 'these must be updated together', 'shared invariant', etc.>"
    }
  ],

  "provenance": {
    "operation": "<initial|squash|amend>",
    "derived_from": ["<original commit SHAs, if any>"],
    "original_annotations_preserved": true,
    "synthesis_notes": "<how source annotations were combined, if applicable>"
  }
}
```

**Notes on the schema:**

The `context_level` field indicates how the annotation was generated. `enhanced` means the authoring agent provided explicit reasoning via `git chronicle commit`, `git chronicle context set`, or `CHRONICLE_*` environment variables — these annotations have the highest fidelity. `inferred` means the annotation was generated from diff analysis alone (typically a human commit without agent context). The Reading Agent should weight `enhanced` annotations more heavily when reasoning conflicts exist, while still treating `inferred` annotations as useful signal.

The `source` field on `constraints` distinguishes claims by origin. `"author"` means the claim came directly from the committing agent's `--reasoning` or `--dependencies` flags. `"inferred"` means it was deduced by the annotation agent from code analysis. These have different reliability — the Reading Agent should weight author-stated claims more heavily. For `inferred` context-level annotations (human commits without agent context), all claims are `source: "inferred"`.

The `ast_anchor` is the primary stable identifier for a code region. Line numbers shift constantly; function names are much more durable. The Reading Agent should match on `ast_anchor.name` first and use line numbers only as a hint.

`semantic_dependencies` captures the non-obvious couplings between code regions — the things that `import` statements and call graphs don't tell you.

`related_annotations` creates an explicit graph between annotations across commits. An agent annotating a follow-up change can reference the annotation on the original commit it's building on. An agent reverting a change can point back to the annotation it's undoing. This allows the Reading Agent to traverse a chain of reasoning across commits — not just the blame history of a line, but the semantic history of a decision. The `relationship` field is free-text to allow nuanced descriptions: "extends the retry logic introduced in", "reverses due to performance regression discovered in", "must be updated in tandem with".

`cross_cutting` captures multi-region concerns that are invisible at the individual function level. "If you change the serialization format in `encode()`, you must also update `decode()` and the migration in `v2_compat.rs`."

`risk_notes` is a free-text field for anything the annotating agent thinks a future modifier should know — fragile code, performance-sensitive paths, known technical debt, workarounds for external bugs.

`provenance` tracks how this annotation came into existence. An `initial` operation means it was captured fresh at commit time. `squash` and `amend` indicate the annotation was derived from one or more prior annotations on commits that have been rewritten. The `derived_from` array preserves the original commit SHAs for traceability even after those commits become unreachable. The Reading Agent can use this to assess annotation freshness and understand when a single annotation represents synthesized reasoning from multiple original commits.

The schema is versioned (`chronicle/v1`) to allow evolution without breaking existing annotations.

### 4.4 Execution Model

**Default: asynchronous.** The post-commit hook spawns `git chronicle annotate` as a background process and exits immediately. The developer or agent is not blocked. Annotation happens in the background and the note appears within a few seconds.

```bash
#!/bin/sh
# .git/hooks/post-commit
git chronicle annotate --commit HEAD --async 2>/dev/null &
```

**Optional: synchronous.** For agent workflows where the agent wants to confirm that the annotation was written before proceeding (e.g., it will immediately reference the note in a task log), the hook can run synchronously:

```bash
#!/bin/sh
git chronicle annotate --commit HEAD --sync
```

**Failure handling.** If the API is unreachable, credentials are missing, or annotation fails for any reason, the hook exits silently. A failed annotation must never interfere with the git workflow. Failed annotations are logged to `.git/chronicle/failed.log` with the commit SHA so they can be retried later via `git chronicle annotate-range`.

#### 4.4.1 Pre-LLM Filtering for Trivial Commits

Before making the LLM API call, Chronicle applies heuristics to skip trivial commits:

- Diff touches only lockfiles (`Cargo.lock`, `package-lock.json`, `yarn.lock`), generated files, or paths matching `exclude` patterns — skip entirely.
- Commit message matches patterns: `Merge branch`, `WIP`, `fixup!`, `squash!` — skip (or defer to squash synthesis).
- Total diff is a single-line change to a version string — produce a minimal annotation locally without an API call.
- Diff is under a configurable threshold (default: 3 lines of non-whitespace, non-comment changes) — produce a minimal annotation locally.

This avoids wasting API calls on changes that don't warrant annotation. Skipped commits are logged:

```
[chronicle] Skipped annotation for abc1234: trivial change (lockfile update only)
```

Configuration:

```ini
[chronicle]
    skipTrivial = true
    trivialThreshold = 3
```

### 4.5 Language Support

Tree-sitter grammars are required for AST parsing. The initial implementation should support:

- Rust (`.rs`)
- TypeScript / JavaScript (`.ts`, `.tsx`, `.js`, `.jsx`)
- Python (`.py`)

For unsupported languages, the agent falls back to diff-only analysis without AST anchors. Annotations will use file + line range instead of named semantic units. This is less durable but still valuable.

Additional grammars can be added incrementally. The tree-sitter grammar loading should be pluggable — check for installed grammars at runtime.

### 4.6 Configuration

Stored in `.git/config` under `[chronicle]`:

```ini
[chronicle]
    enabled = true
    async = true
    provider = anthropic
    model = claude-sonnet-4-5-20250929
    noteref = refs/notes/chronicle
    include = src/**,lib/**
    exclude = tests/**,*.generated.*
    maxDiffLines = 2000
    contextEnvPrefix = CHRONICLE_
```

`provider` / `model` — which LLM provider and model to use. If omitted, Chronicle uses the credential discovery chain (Section 4.2.3) and selects a default model per provider. Anthropic defaults to Claude Sonnet, OpenAI to GPT-4o, Gemini to Gemini Pro, OpenRouter to Claude Sonnet via routing.

`include` / `exclude` — glob patterns for files to annotate. Avoids wasting API calls on generated code, test fixtures, lock files, etc.

`maxDiffLines` — skip annotation for very large commits (likely bulk refactors or generated code). Log a warning instead.

`model` — which Claude model to use. Sonnet is a good default for speed/cost balance. Opus for maximum annotation quality when cost is not a concern.

### 4.7 Crate Structure

```
chronicle/
├── Cargo.toml
├── src/
│   ├── main.rs                 # CLI entrypoint (clap-based)
│   ├── annotate.rs             # Core annotation orchestration
│   ├── agent.rs                # Agent loop: prompt, tool dispatch, iteration
│   ├── provider/
│   │   ├── mod.rs              # LlmProvider trait, credential discovery
│   │   ├── anthropic.rs        # Anthropic messages API
│   │   ├── openai.rs           # OpenAI chat completions API
│   │   ├── gemini.rs           # Google Gemini API
│   │   └── openrouter.rs       # OpenRouter API (OpenAI-compatible)
│   ├── gather.rs               # Diff extraction, file reading, env var collection
│   ├── ast_outline.rs          # Tree-sitter parsing and outline extraction
│   ├── storage.rs              # Git notes read/write operations
│   ├── rewrite.rs              # History rewrite handling (amend, squash detection + synthesis)
│   ├── config.rs               # Configuration management (git config integration)
│   ├── hooks.rs                # Hook installation/management
│   └── schema.rs               # Annotation schema types (serde Serialize/Deserialize)
└── tests/
```

**Key dependencies:**

- `clap` — CLI argument parsing with derive macros
- `reqwest` + `tokio` — async HTTP client and runtime
- `serde` + `serde_json` — serialization for API payloads and annotation schema
- `tree-sitter` + language grammar crates (`tree-sitter-rust`, `tree-sitter-typescript`, `tree-sitter-python`) — AST parsing
- `gix` (gitoxide) — pure Rust git operations (diff, blame, notes, config)
- `blake3` — fast content hashing (optional, for content-addressed lookup path)

No Python runtime. No Node.js. No external SDK dependencies. The binary is self-contained.

---

## 5. Scope Boundaries

**In scope for the Writing Agent:**

- Post-commit hook installation and lifecycle
- Context gathering (diff, files, AST, `git chronicle commit` context, env vars)
- Agent-driven annotation generation for both agent and human commits
- Context level tracking (`enhanced` vs `inferred`)
- Cross-annotation references (`related_annotations`)
- Git notes storage under `refs/notes/chronicle`
- Batch annotation of historical commits
- Configuration management
- Annotation migration through amends (`post-rewrite` hook)
- Annotation synthesis through squash merges (`prepare-commit-msg` tmpfile handshake + `post-commit` synthesis)
- Merge commit annotation scoped to conflict resolutions
- Provenance tracking across history rewrites
- Pre-LLM filtering of trivial commits
- CI-based annotation for server-side squash merges
- Annotation correction/flagging mechanism

**Out of scope (deferred to Reading Agent):**

- Querying annotations by file, function, or line range
- Blame-based annotation retrieval for a code region
- Annotation aggregation across commit lineage
- Staleness detection and confidence scoring
- Integration with agent task workflows

**Out of scope (deferred to future work):**

- Annotation migration through rebases (rebased commits get new SHAs and lose their annotations; the cost of tracking and migrating is not justified yet)
- Multi-repo annotation graphs
- Annotation-aware merge conflict resolution
- Web UI for browsing annotations
- GitHub Actions workflow for CI-based annotation beyond squash merges (Section 6.2.1 covers the squash merge case)

---

## 6. Context Preservation Through Git History Rewrites

A core objective of Chronicle is to **preserve reasoning context even as git history is rewritten** through squashes, merges, and amends. Git's content-addressed storage means that any history rewrite produces new commit SHAs, orphaning notes attached to the old ones. Chronicle must actively migrate and synthesize annotations across these transitions.

### 6.1 Merge Commits

Merge commits are annotated **only for their conflict resolutions**. The non-conflicting portions of a merge are already annotated on their respective source branches — the merge itself adds no new reasoning. Conflict resolutions, however, represent genuine decision-making: the author chose how to reconcile competing changes, and that reasoning is valuable.

The hook detects merge commits (more than one parent) and:

1. Computes the diff between the merge result and each parent.
2. Identifies regions where the merge result differs from *both* parents — these are the conflict resolutions.
3. Sends only those regions to the annotation agent, along with context about what each parent contributed.
4. The annotation focuses on *why* the conflict was resolved this way — which side was preferred, what was manually synthesized, and what semantic concern drove the resolution.

Fast-forward merges produce no annotation (there is no merge commit).

### 6.2 Squash Commits

Squash commits are the highest-risk event for context loss. A sequence of carefully annotated commits — each with its own reasoning trail — collapses into a single commit with a combined diff. Without intervention, all of the per-step reasoning is destroyed.

**Detection mechanism: `prepare-commit-msg` hook → tmpfile → `post-commit` hook.**

Git fires the `prepare-commit-msg` hook before the commit message editor opens, and passes a second argument indicating the commit type. During a squash merge, this argument is `squash` or `merge`. During `git merge --squash`, git also leaves `.git/SQUASH_MSG` with the concatenated messages. Chronicle exploits this:

1. The `prepare-commit-msg` hook detects a squash operation (via the hook argument, or by checking for `.git/SQUASH_MSG`, or via the `CHRONICLE_SQUASH_SOURCES` env var set explicitly by an agent).

2. The hook resolves the source commits — the commits being squashed — and writes them to a known tmpfile: `.git/chronicle/pending-squash.json`:

    ```json
    {
      "source_commits": ["abc123", "def456", "ghi789"],
      "source_ref": "feature-branch",
      "timestamp": "<iso8601>"
    }
    ```

3. The `post-commit` hook checks for the presence of `.git/chronicle/pending-squash.json`. If found, it knows this commit is a squash and enters the synthesis path.

4. For each source commit that has an existing Chronicle annotation, the hook collects those annotations.

5. The annotation agent receives:
   - The squash commit's combined diff.
   - The full set of annotations from the source commits.
   - The individual commit messages from the source commits.

6. The agent **synthesizes** a new annotation for the squash commit that:
   - Preserves all semantic dependencies, constraints, and cross-cutting concerns from the source annotations.
   - Consolidates reasoning that was spread across multiple commits into coherent per-region entries.
   - Preserves `related_annotations` references from the source annotations, remapping them to still-reachable commits where possible.
   - Flags where the squash combined changes whose original annotations described distinct intents (so the Reading Agent knows this region has layered reasoning).
   - Includes a `provenance` field linking back to the original commit SHAs (even though they may become unreachable, the record is preserved for traceability).

7. The hook deletes `.git/chronicle/pending-squash.json` after processing.

This handshake between `prepare-commit-msg` and `post-commit` via a tmpfile is deterministic — no heuristic guessing about whether a commit is a squash. The tmpfile has a short lifespan (created during commit preparation, consumed and deleted during post-commit) and lives in `.git/chronicle/` which is never tracked.

The `CHRONICLE_SQUASH_SOURCES` env var remains as an additional input path for agent workflows that may perform squashes programmatically without going through the normal git merge path.

#### 6.2.1 Server-Side Squash Merges (GitHub, GitLab)

GitHub's "Squash and merge" button and GitLab's "Merge when pipeline succeeds" with squash option are server-side operations. No local hooks fire. Annotations from feature branch commits are lost.

Solution: a CI job that runs after PR merge:

```yaml
# .github/workflows/chronicle-annotate.yml
name: Annotate squash merges
on:
  pull_request:
    types: [closed]
jobs:
  annotate:
    if: github.event.pull_request.merged == true
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
          ref: ${{ github.event.pull_request.base.ref }}
      - name: Install Chronicle
        run: cargo install chronicle
      - name: Annotate squash merge
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          git chronicle annotate --commit HEAD \
            --squash-sources "${{ github.event.pull_request.head.sha }}"
      - name: Push annotations
        run: git push origin refs/notes/chronicle
```

`--squash-sources` tells Chronicle to look for existing annotations on the feature branch commits and synthesize them into the squash commit's annotation. The agent receives the squash commit's combined diff alongside the collected source annotations and produces a synthesized annotation following the same process described in Section 6.2.

### 6.3 Amended Commits

When a commit is amended (`git commit --amend`), git creates a new commit SHA. The old commit becomes unreachable (eventually garbage collected) and its Chronicle annotation is orphaned.

The hook handles this by:

1. Detecting that the commit is an amend. Detection: the `GIT_REFLOG_ACTION` environment variable contains "amend", or the hook can check the reflog for the immediately prior HEAD to find the pre-amend SHA.

2. Retrieving the existing Chronicle annotation from the pre-amend commit.

3. Passing both the old annotation and the new diff to the annotation agent, which produces an updated annotation that preserves still-relevant reasoning from the original and adds or modifies entries for whatever changed in the amend.

4. Writing the updated annotation as a note on the new commit SHA.

5. Optionally removing the orphaned note from the old SHA (or leaving it for garbage collection with the unreachable commit).

This ensures that iterative commit refinement (`commit`, `amend`, `amend`, `amend`) doesn't create a trail of orphaned annotations while losing context with each iteration.

### 6.4 Provenance Tracking

To support traceability through history rewrites, every annotation includes an optional `provenance` field:

```json
{
  "provenance": {
    "derived_from": ["<original commit SHAs>"],
    "operation": "<squash|amend|initial>",
    "original_annotations_preserved": true,
    "synthesis_notes": "<how the agent combined source annotations, if applicable>"
  }
}
```

This allows the Reading Agent to understand the lineage of an annotation — whether it was captured fresh at commit time (`initial`), migrated from an amend, or synthesized from a squash of five feature branch commits.

### 6.5 Annotation Corrections

Annotations are a knowledge base, not a write-once store. Code evolves, external dependencies change, and assumptions become invalid. When an agent or developer discovers an annotation is incorrect, they can flag it:

```bash
# Flag an annotation as inaccurate
git chronicle flag src/mqtt/client.rs MqttClient::connect \
  --reason "Constraint about drain-before-reconnect is no longer required since broker v2.3"

# The flag is stored as a correction note in refs/notes/chronicle
# Future `git chronicle read` surfaces the flag alongside the original annotation
# The flagged region's confidence score is reduced
```

The read path includes corrections inline alongside the original annotation:

```json
{
  "constraints": [
    {
      "text": "Must drain queue before reconnecting",
      "source": "author"
    }
  ],
  "corrections": [
    {
      "field": "constraints",
      "correction": "Constraint no longer required since broker v2.3",
      "timestamp": "2025-12-20T...",
      "author": "agent-session-abc"
    }
  ]
}
```

Corrections do not delete or overwrite the original annotation. They are additive — the Reading Agent sees both the original claim and the correction and can weigh them accordingly. This preserves the full reasoning history: even a retracted constraint may be relevant context ("this was once believed necessary because...").

---

## 7. Open Questions

1. **Claude subscription credential format.** The credential discovery chain prefers Claude subscription credentials from `~/.config/claude/` (or wherever Claude Code stores them). The exact file format and token type needs to be verified — is it an OAuth token, an API key, or a session token? This affects how Chronicle authenticates against the Anthropic API with those credentials.

2. **Annotation size limits.** Git notes can be arbitrarily large, but very large annotations (from complex commits touching many files) could slow down retrieval. Should there be a soft limit on annotation size, with the agent encouraged to be more selective on large commits?

3. **`prepare-commit-msg` hook contention.** Some tools and workflows already install a `prepare-commit-msg` hook (e.g., for commit template insertion, ticket number prefixing). Chronicle's hook must coexist with these. The `git chronicle init` command should detect existing hooks and chain them rather than overwriting. Alternatively, `core.hooksPath` with a hook runner that executes multiple scripts in sequence.

4. **Related annotation discovery at write time.** For the agent to populate `related_annotations`, it needs to know about existing annotations on recent commits. How much history should be loaded into the agent's context? The most recent N annotated commits? Only commits that touch the same files? This is a cost/quality tradeoff.