# Chronicle - Claude Code Instructions

## Project

Rust CLI tool (`git-chronicle`) for AI-powered commit annotations stored as git notes. Installed as a git extension -- users type `git chronicle <command>`.

## Build & test

```bash
cargo build          # compile (produces git-chronicle binary)
cargo test           # all tests
cargo test --lib     # unit tests only (fast, no git fixtures)
cargo check          # type check without codegen
cargo clippy         # lint
```

## Architecture

- `GitOps` trait in `src/git/mod.rs` abstracts git operations. MVP uses `CliOps` (shells out to git). Do not add `GixOps` yet.
- `LlmProvider` trait in `src/provider/mod.rs`. MVP uses `AnthropicProvider`. Do not add other providers yet.
- Error handling uses **snafu 0.8** with `#[snafu(module(...))]` to scope context selectors. Variant names must NOT end in "Error".
- Annotation schema is `chronicle/v2` (canonical type in `src/schema/v2.rs`). All internal code uses `schema::Annotation` (a type alias to `v2::Annotation`). Old `chronicle/v1` notes are migrated on read via `schema::parse_annotation()`.
- Single deserialization chokepoint: `schema::parse_annotation(json) -> Result<Annotation>` detects version and migrates. Never deserialize annotations directly with `serde_json::from_str`.
- Two annotation paths: **batch** (LLM agent loop in `src/annotate/`) and **live** (handler in `src/annotate/live.rs`, zero LLM cost).
- `git chronicle schema <name>` makes the CLI self-documenting — agents can query input/output formats at runtime.

## Key conventions

- v1 `RegionAnnotation` has a `corrections: Vec<Correction>` field. When constructing v1 types in tests, always include `corrections: vec![]`.
- Git notes use `-F tempfile` pattern in `note_write` to avoid shell escaping. Do not pass note content as command-line args.
- AST anchor resolution: `src/ast/anchor.rs` resolves anchors with exact/qualified/fuzzy matching via Levenshtein distance.
- Tree-sitter supports Rust, TypeScript, Python, Go, Java, C, C++, and Ruby via optional feature flags.
- The `annotate_live` integration test requires a real `.git` directory (not a worktree gitlink). It will fail in git worktrees.

## Module map

| Module | Purpose |
|--------|---------|
| `cli/` | Clap CLI commands (includes `schema.rs` for self-documenting CLI) |
| `git/` | `GitOps` trait + `CliOps` |
| `ast/` | Tree-sitter outline extraction + anchor resolution |
| `schema/` | v1 types (`v1.rs`), v2 canonical types (`v2.rs`), shared types (`common.rs`), migration (`migrate.rs`), `parse_annotation()` |
| `annotate/` | Batch annotation agent + live handler (`live.rs`) + squash synthesis |
| `read/` | Read pipeline: `retrieve`, `contracts`, `decisions`, `deps`, `history`, `summary` |
| `hooks/` | Git hook handlers |
| `provider/` | LLM provider trait + Anthropic |
| `agent/` | Agent conversation loop (narrative-first v2 tools) |
| `sync/` | Notes sync with remotes |
| `config/` | Config management |
| `doctor.rs` | Diagnostic checks |
| `export.rs` | JSONL export (handles v1 and v2) |
| `import.rs` | JSONL import (validates via `parse_annotation()`) |

## Working with Chronicle annotations

This project uses Chronicle (`git-chronicle`) to store structured metadata
alongside commits as git notes. Before modifying existing code, query the
annotations to understand intent, constraints, and dependencies.

### Reading annotations (before modifying code)

```bash
# Check contracts — "What must I not break?"
git chronicle contracts src/foo.rs --anchor bar_function

# Check decisions — "What was decided and why?"
git chronicle decisions --path src/foo.rs

# Read raw annotations for a file/anchor
git chronicle read src/foo.rs --anchor bar_function

# Quick orientation for a file
git chronicle summary src/foo.rs

# What depends on this code?
git chronicle deps src/foo.rs bar_function
```

**Respect contracts.** Annotations may include contracts like "must not
block the async runtime" or "assumes sorted input." Violating these without
updating the annotation is a bug. See `.claude/skills/context/SKILL.md`.

### Writing annotations (after committing)

After committing, annotate using the live path (v2 format). Use a temp file
with a quoted heredoc to avoid shell escaping issues:

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{
  "commit": "HEAD",
  "summary": "What this commit does and WHY this approach."
}
EOF
git chronicle annotate --live < /tmp/chronicle-annotate.json
```

See `.claude/skills/annotate/SKILL.md` for the full annotation workflow.

### Backfilling annotations

To annotate historical commits that lack annotations, see
`.claude/skills/backfill/SKILL.md`.


## Errors
Use `snafu` to manage errors. 
* Errors should be organized in a hierarchy where errors from other crates are the leaves of the tree and are linked to higher-level errors with 'source'
* Every error variant must include a `location` field to track where it occurred. The message should end with `, at {location}` to make the error traces easy to read.
* Most of the message formatting should be done in the `snafu(display(` macro. 

```rust
use snafu::{Snafu, ResultExt, Location};

#[derive(Debug, Snafu)]
pub enum MessageError {
    #[snafu(display("Failed to deserialize message, at {location}"))]
    Deserialization {
        #[snafu(source)]
        source: serde_json::Error,
        #[snafu(implicit)]
        location: Location,
    },
    #[snafu(display("Failed to validate message, at {location}"))]
    Validate {
        #[snafu(implicit)]
        location: Location,
    }
}

pub fn read_message(json: &str) -> Result<Message, MessageError> {
    let message = serde_json::from_str(json).context(DeserializationSnafu)?;
    let validated_message = message.validate().context(ValidateSnafu)?;
    Ok(validated_message)
}
```

## Parallel agent workflow

Use git worktrees to allow simultaneous agent development in non-conflicting areas of code.

## Versioning

Do not increment the version number. It is done as part of a workflow step at github automatically upon release.