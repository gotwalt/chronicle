# Chronicle - Claude Code Instructions

## Project

Rust CLI tool (`git-chronicle`) for AI-powered commit annotations stored as git notes. Installed as a git extension -- users type `git chronicle <command>`. 51 source files, ~10k lines of Rust, 142 tests.

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
- Annotation schema is `chronicle/v1` in `src/schema/annotation.rs`. The `Annotation` struct has a `validate()` method -- always call it before writing.
- Two annotation paths: **batch** (LLM agent loop in `src/annotate/`) and **live** (MCP handler in `src/mcp/annotate_handler.rs`, zero LLM cost).

## Key conventions

- `RegionAnnotation` has a `corrections: Vec<Correction>` field. When constructing in tests, always include `corrections: vec![]`.
- Git notes use `-F tempfile` pattern in `note_write` to avoid shell escaping. Do not pass note content as command-line args.
- AST anchor resolution: `src/ast/anchor.rs` resolves anchors with exact/qualified/fuzzy matching via Levenshtein distance.
- Tree-sitter supports Rust, TypeScript, Python, Go, Java, C, C++, and Ruby via optional feature flags.
- The `annotate_live` integration test requires a real `.git` directory (not a worktree gitlink). It will fail in git worktrees.

## Module map

| Module | Purpose |
|--------|---------|
| `cli/` | Clap CLI commands |
| `git/` | `GitOps` trait + `CliOps` |
| `ast/` | Tree-sitter outline extraction + anchor resolution |
| `schema/` | `Annotation`, `RegionAnnotation`, `Correction` types |
| `mcp/` | MCP annotate handler (live path) |
| `read/` | Read pipeline: `retrieve`, `deps`, `history`, `summary` |
| `annotate/` | Batch annotation agent + squash synthesis |
| `hooks/` | Git hook handlers |
| `provider/` | LLM provider trait + Anthropic |
| `agent/` | Agent conversation loop |
| `sync/` | Notes sync with remotes |
| `config/` | Config management |
| `doctor.rs` | Diagnostic checks |
| `export.rs` | JSONL export |
| `import.rs` | JSONL import |

## Working with Chronicle annotations

This project uses Chronicle (`git-chronicle`) to store structured metadata
alongside commits as git notes. Before modifying existing code, query the
annotations to understand intent, constraints, and dependencies.

### Reading annotations (before modifying code)

Use the `chronicle_read` MCP tool to check annotations on code you're about
to modify:

- `chronicle_read(path: "src/foo.rs", anchor: "bar_function")` -- get intent,
  reasoning, constraints for a specific function
- `chronicle_deps(path: "src/foo.rs", anchor: "bar_function")` -- find code
  that depends on this function's behavior
- `chronicle_summary(path: "src/foo.rs")` -- overview of all annotated regions

If MCP tools are unavailable, use the CLI:

```bash
git chronicle read src/foo.rs --anchor bar_function
```

**Respect constraints.** Annotations may include constraints like "must not
allocate" or "assumes sorted input." Violating these without updating the
annotation is a bug.

See `.claude/skills/context/SKILL.md` for the full context-reading workflow.

### Writing annotations (after committing)

After committing, annotate using the live path. Use a temp file with a
quoted heredoc to avoid shell escaping issues:

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{ "commit": "HEAD", "summary": "...", "regions": [...], "cross_cutting": [] }
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