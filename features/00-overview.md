# Chronicle Implementation Plan

## Feature Breakdown & Dependency Graph

---

## Architecture

Chronicle is a single Rust binary distributed via `cargo install`, Homebrew, or prebuilt release binaries. It installs as git hooks and a CLI, communicates with LLM providers over HTTP, parses code with tree-sitter, and stores annotations as git notes.

```
chronicle (Rust binary)
├── CLI layer (clap)              Feature 01
├── Git operations (gix + fallback) Feature 02
├── AST parsing (tree-sitter)       Feature 03
├── LLM provider abstraction        Feature 04
├── Writing agent                    Feature 05
├── Hooks & context capture          Feature 06
├── Read pipeline                    Feature 07
├── Advanced queries                 Feature 08
├── History rewrite handling         Feature 09
├── Team operations                  Feature 10
├── Annotation corrections           Feature 11
├── MCP server                       Feature 12
├── Claude Code integration          Feature 13
├── Interactive show (TUI)           Feature 14
├── Claude Code skills & workflow    Feature 15
├── Multi-language AST parsing       Feature 16
└── Rapid onboarding                 Feature 17
```

---

## Features

| # | Feature | File | Est. Complexity | Description |
|---|---------|------|-----------------|-------------|
| 01 | CLI Framework & Config | `01-cli-and-config.md` | Medium | clap-based CLI skeleton, all subcommands, git config management, `.chronicle-config.toml` |
| 02 | Git Operations Layer | `02-git-operations.md` | High | gix integration for diff, blame, notes, config; git CLI fallback; notes read/write |
| 03 | Tree-sitter AST Parsing | `03-ast-parsing.md` | Medium | Grammar loading, outline extraction, anchor resolution, language fallback |
| 04 | LLM Provider Abstraction | `04-llm-providers.md` | High | Provider trait, credential discovery, Anthropic/OpenAI/Gemini/OpenRouter, tool-use normalization |
| 05 | Writing Agent | `05-writing-agent.md` | High | System prompt, tool definitions, agent loop, annotation schema production |
| 06 | Hooks & Context Capture | `06-hooks-and-context.md` | Medium | Hook installation/chaining, `git chronicle commit`, `git chronicle context set`, pre-LLM filtering, async model |
| 07 | Read Pipeline | `07-read-pipeline.md` | High | Blame retrieval, note fetching, region filtering, confidence scoring, token trimming, output schema |
| 08 | Advanced Queries | `08-advanced-queries.md` | Medium | `deps`, `history`, `summary`, multi-file queries, reverse index (v1.1) |
| 09 | History Rewrite Handling | `09-history-rewrites.md` | High | Squash detection/synthesis, amend migration, merge annotation, CI squash, provenance |
| 10 | Team Operations | `10-team-operations.md` | Medium | Notes sync, auto-sync, backfill, export/import, `git chronicle doctor`, skill install |
| 11 | Annotation Corrections | `11-corrections.md` | Low-Medium | `git chronicle flag`, `git chronicle correct`, correction storage, read-path surfacing |
| 12 | MCP Server | `12-mcp-server.md` | Medium | MCP protocol, tool definitions, server lifecycle, registration |
| 13 | Claude Code Integration | `13-claude-code-integration.md` | Low-Medium | MCP annotate tool, Claude Code skill, post-commit hook |
| 14 | Interactive Show (TUI) | `14-interactive-show.md` | High | `git chronicle show` TUI explorer, annotation panel, deps/history drill-down, plain-text fallback |
| 15 | Claude Code Skills | `15-claude-code-skills.md` | Low | Context/annotate/backfill skills, pre-edit hook, CLAUDE.md integration, MCP config |
| 16 | Multi-Language AST | `16-multi-language-ast.md` | Medium | TypeScript, JavaScript, Python, Go, Java, C, C++, Ruby outline extraction and anchor resolution |
| 17 | Rapid Onboarding | `17-onboarding.md` | Medium | `setup` command, user config, `backfill` CLI, enhanced `init`, Claude Code provider, embedded content distribution |

---

## Dependency Graph

```
Phase 1 (Foundation)
  ┌──────────────┐     ┌──────────────┐
  │  01 CLI &    │     │  02 Git Ops  │
  │  Config      │     │  Layer       │
  └──────┬───────┘     └──────┬───────┘
         │                    │
         └────────┬───────────┘
                  │
Phase 2 (Parsing & LLM) — parallel
  ┌──────────────┐     ┌──────────────┐
  │  03 AST      │     │  04 LLM      │
  │  Parsing     │     │  Providers   │
  └──────┬───────┘     └──────┬───────┘
         │                    │
         └────────┬───────────┘
                  │
Phase 3 (Core Paths) — parallel
  ┌──────────────┐     ┌──────────────┐
  │  05 Writing  │     │  07 Read     │
  │  Agent       │     │  Pipeline    │
  └──────┬───────┘     └──────┬───────┘
         │                    │
Phase 4 (Integration)
  ┌──────────────┐            │
  │  06 Hooks &  │            │
  │  Context     │            │
  └──────┬───────┘            │
         │                    │
Phase 5 (Advanced) — parallel, all features below can proceed independently
  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
  │ 08 Adv.  │ │ 09 Hist. │ │ 10 Team  │ │ 11 Corr. │ │ 12 MCP   │ │ 13 CC    │ │ 14 Show  │
  │ Queries  │ │ Rewrites │ │ Ops      │ │          │ │ Server   │ │ Integr.  │ │ (TUI)    │
  └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────────┘ └──────┬───┘ └──────────┘
                                                                         │
Phase 6 (Agent Workflow & Language Expansion)                            │
  ┌──────────────┐                                                       │
  │ 15 CC Skills │◄──────────────────────────────────────────────────────┘
  │ & Workflow   │
  └──────┬───────┘
         │
  ┌──────────────┐
  │ 17 Rapid     │  (depends on 01, 06, 15)
  │ Onboarding   │
  └──────────────┘
  ┌──────────────┐
  │ 16 Multi-    │  (depends on 03, can proceed independently of Phase 5)
  │ Lang AST     │
  └──────────────┘
```

### Dependency Details

| Feature | Depends On | Blocks |
|---------|-----------|--------|
| 01 CLI & Config | — | Everything |
| 02 Git Operations | 01 | 05, 06, 07, 08, 09, 10, 11 |
| 03 AST Parsing | 01 | 05, 07, 08 |
| 04 LLM Providers | 01 | 05 |
| 05 Writing Agent | 02, 03, 04 | 06, 09 |
| 06 Hooks & Context | 02, 05 | 09 |
| 07 Read Pipeline | 02, 03 | 08, 11, 12 |
| 08 Advanced Queries | 07 | 12 |
| 09 History Rewrites | 05, 06 | — |
| 10 Team Operations | 02, 06 | — |
| 11 Corrections | 02, 07 | — |
| 12 MCP Server | 07, 08 | — |
| 13 Claude Code Integration | 02, 03, 05, 12 | — |
| 14 Interactive Show (TUI) | 02, 03, 07, 08 | — |
| 15 Claude Code Skills | 12, 13 | 17 |
| 16 Multi-Language AST | 03 | 14 (show uses outline dispatch) |
| 17 Rapid Onboarding | 01, 06, 15 | — |

---

## Recommended Team Allocation

With a team of 3-4 engineers:

**Engineer A (Write Path):** 01 → 04 → 05 → 06 → 09
**Engineer B (Read Path):** 02 → 07 → 08 → 12
**Engineer C (Infrastructure):** 03 → 10 → 11
**Engineer D (or split across A-C):** Integration testing, CI, packaging

Engineers A and B start in parallel on 01 and 02 (the two foundation features). Once those land, the team fans out. Features 03 and 04 can proceed in parallel. The write agent (05) and read pipeline (07) can proceed in parallel once their dependencies are met.

Phase 5 features (08-12) are largely independent and can be distributed across the team as bandwidth allows.

---

## Crate Structure

```
chronicle/
├── Cargo.toml
├── src/
│   ├── main.rs                  # CLI entrypoint (clap)
│   ├── cli/
│   │   ├── mod.rs               # Subcommand dispatch
│   │   ├── init.rs              # `git chronicle init`
│   │   ├── commit.rs            # `git chronicle commit`
│   │   ├── context.rs           # `git chronicle context set`
│   │   ├── annotate.rs          # `git chronicle annotate`
│   │   ├── read.rs              # `git chronicle read`
│   │   ├── deps.rs              # `git chronicle deps`
│   │   ├── history.rs           # `git chronicle history`
│   │   ├── summary.rs           # `git chronicle summary`
│   │   ├── backfill.rs          # `git chronicle backfill`
│   │   ├── inspect.rs           # `git chronicle inspect`
│   │   ├── flag.rs              # `git chronicle flag`
│   │   ├── correct.rs           # `git chronicle correct`
│   │   ├── doctor.rs            # `git chronicle doctor`
│   │   ├── sync.rs              # `git chronicle sync`
│   │   ├── export.rs            # `git chronicle export`
│   │   ├── import.rs            # `git chronicle import`
│   │   ├── skill.rs             # `git chronicle skill`
│   │   ├── auth.rs              # `git chronicle auth`
│   │   ├── config_cmd.rs        # `git chronicle config`
│   │   ├── mcp.rs               # `git chronicle mcp`
│   │   ├── setup.rs             # `git chronicle setup` (F17)
│   │   └── reconfigure.rs       # `git chronicle reconfigure` (F17)
│   ├── git/
│   │   ├── mod.rs               # Git operation abstractions
│   │   ├── diff.rs              # Diff extraction (gix + fallback)
│   │   ├── blame.rs             # Blame wrapper with line-range support
│   │   ├── notes.rs             # Notes read/write under refs/notes/chronicle
│   │   ├── config.rs            # Git config read/write
│   │   └── refs.rs              # Ref management
│   ├── ast/
│   │   ├── mod.rs               # AST parsing coordination, Language enum
│   │   ├── outline.rs           # Rust outline extraction, SemanticKind, shared helpers
│   │   ├── anchor.rs            # Anchor name → line range resolution
│   │   ├── outline_typescript.rs # TypeScript/TSX outline extraction (F16)
│   │   ├── outline_python.rs    # Python outline extraction (F16)
│   │   ├── outline_go.rs        # Go outline extraction (F16)
│   │   ├── outline_java.rs      # Java outline extraction (F16)
│   │   ├── outline_c.rs         # C outline extraction (F16)
│   │   ├── outline_cpp.rs       # C++ outline extraction (F16)
│   │   └── outline_ruby.rs     # Ruby outline extraction (F16)
│   ├── provider/
│   │   ├── mod.rs               # LlmProvider trait, credential discovery
│   │   ├── anthropic.rs         # Anthropic Messages API
│   │   ├── claude_code.rs       # Claude Code subprocess provider (F17)
│   │   ├── openai.rs            # OpenAI Chat Completions API
│   │   ├── gemini.rs            # Google Gemini API
│   │   └── openrouter.rs        # OpenRouter (OpenAI-compatible)
│   ├── agent/
│   │   ├── mod.rs               # Agent loop orchestration
│   │   ├── tools.rs             # Tool definitions and dispatch
│   │   ├── prompt.rs            # System prompt construction
│   │   └── structured.rs        # Structured-output fallback mode
│   ├── annotate/
│   │   ├── mod.rs               # Annotation orchestration
│   │   ├── gather.rs            # Context gathering (diff, files, AST, env)
│   │   ├── filter.rs            # Pre-LLM trivial commit filtering
│   │   └── squash.rs            # Squash detection and synthesis
│   ├── read/
│   │   ├── mod.rs               # Read pipeline orchestration
│   │   ├── retrieve.rs          # Blame → notes → filter pipeline
│   │   ├── scoring.rs           # Confidence scoring (4-factor model)
│   │   ├── trimming.rs          # Token budget trimming
│   │   ├── deps.rs              # Dependency inversion queries
│   │   ├── history.rs           # Timeline reconstruction
│   │   └── summary.rs           # Condensed view generation
│   ├── hooks/
│   │   ├── mod.rs               # Hook management
│   │   ├── install.rs           # Hook installation and chaining
│   │   ├── post_commit.rs       # Post-commit hook logic
│   │   ├── prepare_commit_msg.rs # Squash detection
│   │   └── post_rewrite.rs      # Amend migration
│   ├── schema/
│   │   ├── mod.rs               # Annotation schema types
│   │   ├── annotation.rs        # Core annotation struct
│   │   ├── region.rs            # Region annotation struct
│   │   ├── correction.rs        # Correction/flag structs
│   │   └── output.rs            # Read output schema
│   ├── setup/
│   │   ├── mod.rs               # Setup orchestration (F17)
│   │   └── embedded.rs          # include_str!() embedded content (F17)
│   ├── config/
│   │   ├── mod.rs               # Configuration management
│   │   ├── git_config.rs        # [chronicle] section in .git/config
│   │   ├── shared_config.rs     # .chronicle-config.toml parsing
│   │   └── user_config.rs       # ~/.git-chronicle.toml load/save (F17)
│   ├── sync/
│   │   ├── mod.rs               # Notes sync management
│   │   ├── push_fetch.rs        # Refspec configuration
│   │   └── merge.rs             # Notes merge strategy
│   ├── backfill.rs              # Historical commit annotation
│   ├── export.rs                # Export annotations to JSON
│   ├── import.rs                # Import annotations from JSON
│   ├── doctor.rs                # Diagnostic checks
│   ├── skill.rs                 # Skill definition management
│   └── mcp/
│       ├── mod.rs               # MCP server entrypoint
│       ├── server.rs            # MCP protocol handling
│       └── tools.rs             # MCP tool definitions
├── embedded/                       # Distribution content (F17)
│   ├── skills/
│   │   ├── context/SKILL.md
│   │   ├── annotate/SKILL.md
│   │   └── backfill/SKILL.md
│   ├── hooks/
│   │   ├── chronicle-annotate-reminder.sh
│   │   └── chronicle-read-context-hint.sh
│   └── claude-md-snippet.md
├── tests/
│   ├── integration/
│   │   ├── annotate_test.rs     # End-to-end annotation tests
│   │   ├── read_test.rs         # End-to-end read tests
│   │   ├── hooks_test.rs        # Hook installation and firing
│   │   ├── squash_test.rs       # Squash merge handling
│   │   └── provider_test.rs     # LLM provider integration
│   └── fixtures/
│       ├── repos/               # Test git repositories
│       └── annotations/         # Sample annotation JSON
└── features/                    # Feature specifications
    ├── 00-overview.md
    ├── ...
    ├── 16-multi-language-ast.md
    └── 17-onboarding.md
```

---

## Key Dependencies (Cargo.toml)

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
gix = { version = "0.68", features = ["blocking-network-client"] }
tree-sitter = "0.24"
tree-sitter-rust = "0.23"
tree-sitter-typescript = { version = "0.23", optional = true }
tree-sitter-python = { version = "0.23", optional = true }
tree-sitter-go = { version = "0.23", optional = true }
tree-sitter-java = { version = "0.23", optional = true }
tree-sitter-c = { version = "0.23", optional = true }
tree-sitter-cpp = { version = "0.23", optional = true }
tree-sitter-ruby = { version = "0.23", optional = true }
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
snafu = "0.8"
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

## Testing Strategy

Each feature file includes its own test plan. The global testing approach:

**Unit tests:** Per-module, co-located in source files. Mock git operations and LLM responses.

**Integration tests:** In `tests/integration/`. Create temporary git repositories, make real commits, run hooks, verify annotations. Use recorded HTTP responses (via `wiremock` or similar) for LLM provider tests.

**End-to-end tests:** A small shell script test suite that installs chronicle in a fresh repo, makes commits, reads annotations, verifies the full loop. Run in CI.

**Property tests:** For the confidence scoring algorithm and token trimming logic, property-based tests (via `proptest`) ensure invariants hold across diverse inputs.

---

## Quality Gates

Before merging each feature:

1. All existing tests pass.
2. New tests cover the feature's acceptance criteria.
3. `cargo clippy` clean.
4. `cargo fmt` clean.
5. No new `unsafe` without justification.
6. Documentation for public APIs.
7. Feature demo showing the capability working end-to-end (recorded in PR description).

---

## Implementation Notes

- **Start with gix, fall back to git CLI.** Every git operation should have a fallback path that shells out to `git`. This unblocks development when gix doesn't support an operation and provides a safety net in production.

- **Record LLM responses for tests.** Integration tests should never make real API calls. Record sample request/response pairs and replay them. This makes tests fast, deterministic, and free.

- **Schema versioning from day one.** The annotation schema includes `"$schema": "chronicle/v1"`. Parse this on read and handle unknown versions gracefully (warn, don't crash).

- **Feature flags via Cargo features.** Language grammars should be optional Cargo features so users can build a smaller binary if they only need specific languages.

- **Tracing, not println.** Use the `tracing` crate for all diagnostic output. This enables structured logging, log levels, and integration with observability tools.
