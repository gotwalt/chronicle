# chronicle

AI-powered commit annotation tool that captures the **reasoning and intent** behind code changes, stored as structured metadata in git notes.

## Why

Git commits record *what* changed. Chronicle records *why* -- the intent, constraints, dependencies, and reasoning that informed each change. This metadata lives alongside commits as git notes (`refs/notes/chronicle`) and can be queried, synced, exported, and corrected over time.

## Install

```
cargo install --path .
```

This installs the `git-chronicle` binary. Once on your PATH, git discovers it automatically -- use `git chronicle <command>`.

Requires Rust 1.70+ and git.

## Quick start

```bash
# Initialize in a git repo
git chronicle init

# Make a commit (use regular git commit; post-commit hook handles annotation)
git commit -m "refactor auth middleware"

# Annotate an existing commit via the LLM batch path
export ANTHROPIC_API_KEY=sk-...
git chronicle annotate --commit HEAD

# Or annotate via the live path (zero LLM cost, stdin JSON)
echo '{"commit":"HEAD","summary":"...","regions":[...]}' | git chronicle annotate --live
```

## Commands

### Write path

| Command | Description |
|---------|-------------|
| `git chronicle init` | Initialize chronicle in the current repository |
| `git chronicle annotate` | Annotate a commit (batch LLM or `--live` stdin) |
| `git chronicle context set` | Set pending context for the next commit |

### Read path

| Command | Description |
|---------|-------------|
| `git chronicle read <path>` | Read annotations for a file, optionally filtered by `--anchor` or `--lines` |
| `git chronicle deps <path>` | Find code that depends on a given file/anchor (dependency inversion) |
| `git chronicle history <path>` | Show annotation timeline across commits |
| `git chronicle summary <path>` | Condensed annotation summary for a file |

### Corrections

| Command | Description |
|---------|-------------|
| `git chronicle flag <path> --reason "..."` | Flag a region annotation as potentially inaccurate |
| `git chronicle correct <sha> --region <anchor> --field <field>` | Apply a precise correction to an annotation field |

### Team operations

| Command | Description |
|---------|-------------|
| `git chronicle sync enable` | Enable notes sync with a remote |
| `git chronicle sync status` | Show sync status (local/remote note counts) |
| `git chronicle sync pull` | Fetch and merge remote notes |
| `git chronicle export` | Export all annotations as JSONL |
| `git chronicle import <file>` | Import annotations from JSONL (`--force`, `--dry-run`) |
| `git chronicle doctor` | Run diagnostic checks on the chronicle setup |

## Annotation schema

Annotations use the `chronicle/v1` schema and are stored as JSON in git notes:

```json
{
  "schema": "chronicle/v1",
  "commit": "abc123...",
  "timestamp": "2026-02-06T12:00:00Z",
  "summary": "Refactor auth middleware to support JWT",
  "context_level": "enhanced",
  "regions": [
    {
      "file": "src/auth/middleware.rs",
      "ast_anchor": { "unit_type": "fn", "name": "validate_token", "signature": "..." },
      "lines": { "start": 42, "end": 78 },
      "intent": "Extract token validation into standalone function for testability",
      "reasoning": "Previous inline validation made unit testing impossible...",
      "constraints": [{ "text": "Must validate expiry before signature", "source": "author" }],
      "semantic_dependencies": [{ "file": "src/auth/jwt.rs", "anchor": "decode", "nature": "calls" }],
      "tags": ["refactor", "auth"]
    }
  ],
  "provenance": { "operation": "initial", "derived_from": [], "original_annotations_preserved": false }
}
```

## Architecture

Two annotation paths:

- **Batch path**: `git chronicle annotate --commit <sha>` runs an LLM agent loop that inspects the diff, reads source files, extracts AST outlines, and emits structured annotations. Requires `ANTHROPIC_API_KEY`.
- **Live path**: `git chronicle annotate --live` reads `AnnotateInput` JSON from stdin. Zero LLM cost -- the calling agent (e.g. Claude Code) already knows intent and reasoning because it wrote the code.

Key modules:

```
src/
  cli/          CLI commands (clap)
  git/          GitOps trait + CliOps implementation (shells out to git)
  ast/          Tree-sitter parsing, outline extraction, anchor resolution
  schema/       Annotation and Correction types (serde)
  mcp/          MCP annotate handler (live path core)
  read/         Read pipeline: retrieve, deps, history, summary
  annotate/     Batch annotation agent + squash synthesis
  hooks/        Git hook handlers (post-commit, prepare-commit-msg, post-rewrite)
  provider/     LLM provider trait + Anthropic implementation
  agent/        Agent loop for batch annotation
  sync/         Notes sync with remotes
  config/       Configuration management
  doctor.rs     Diagnostic checks
  export.rs     JSONL export
  import.rs     JSONL import
```

## Testing

```bash
cargo test           # all tests (142 across 7 test binaries)
cargo test --lib     # unit tests only (92)
cargo test --test annotate_live  # live annotation integration test
```

## Claude Code integration

Chronicle ships with a Claude Code skill (`.claude/skills/annotate/SKILL.md`) and a post-tool-use hook (`.claude/hooks/`) that reminds the agent to annotate after committing. When Claude Code is the authoring agent, it uses the live annotation path -- zero LLM cost because the agent already knows the intent behind its changes.

## License

MIT
