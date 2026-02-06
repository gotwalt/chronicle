# ultragit

AI-powered commit annotation tool that captures the **reasoning and intent** behind code changes, stored as structured metadata in git notes.

## Why

Git commits record *what* changed. Ultragit records *why* -- the intent, constraints, dependencies, and reasoning that informed each change. This metadata lives alongside commits as git notes (`refs/notes/ultragit`) and can be queried, synced, exported, and corrected over time.

## Install

```
cargo install --path .
```

Requires Rust 1.70+ and git.

## Quick start

```bash
# Initialize in a git repo
ultragit init

# Make a commit (wraps git commit with annotation context)
ultragit commit -m "refactor auth middleware" --task PROJ-42

# Annotate an existing commit via the LLM batch path
export ANTHROPIC_API_KEY=sk-...
ultragit annotate --commit HEAD

# Or annotate via the live path (zero LLM cost, stdin JSON)
echo '{"commit":"HEAD","summary":"...","regions":[...]}' | ultragit annotate --live
```

## Commands

### Write path

| Command | Description |
|---------|-------------|
| `ultragit init` | Initialize ultragit in the current repository |
| `ultragit commit` | Commit with annotation context (wraps `git commit`) |
| `ultragit annotate` | Annotate a commit (batch LLM or `--live` stdin) |
| `ultragit context set` | Set pending context for the next commit |

### Read path

| Command | Description |
|---------|-------------|
| `ultragit read <path>` | Read annotations for a file, optionally filtered by `--anchor` or `--lines` |
| `ultragit deps <path>` | Find code that depends on a given file/anchor (dependency inversion) |
| `ultragit history <path>` | Show annotation timeline across commits |
| `ultragit summary <path>` | Condensed annotation summary for a file |

### Corrections

| Command | Description |
|---------|-------------|
| `ultragit flag <path> --reason "..."` | Flag a region annotation as potentially inaccurate |
| `ultragit correct <sha> --region <anchor> --field <field>` | Apply a precise correction to an annotation field |

### Team operations

| Command | Description |
|---------|-------------|
| `ultragit sync enable` | Enable notes sync with a remote |
| `ultragit sync status` | Show sync status (local/remote note counts) |
| `ultragit sync pull` | Fetch and merge remote notes |
| `ultragit export` | Export all annotations as JSONL |
| `ultragit import <file>` | Import annotations from JSONL (`--force`, `--dry-run`) |
| `ultragit doctor` | Run diagnostic checks on the ultragit setup |

## Annotation schema

Annotations use the `ultragit/v1` schema and are stored as JSON in git notes:

```json
{
  "schema": "ultragit/v1",
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

- **Batch path**: `ultragit annotate --commit <sha>` runs an LLM agent loop that inspects the diff, reads source files, extracts AST outlines, and emits structured annotations. Requires `ANTHROPIC_API_KEY`.
- **Live path**: `ultragit annotate --live` reads `AnnotateInput` JSON from stdin. Zero LLM cost -- the calling agent (e.g. Claude Code) already knows intent and reasoning because it wrote the code.

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
cargo test           # all tests (143 across 7 test binaries)
cargo test --lib     # unit tests only (92)
cargo test --test annotate_live  # live annotation integration test
```

## Claude Code integration

Ultragit ships with a Claude Code skill (`.claude/skills/annotate/SKILL.md`) and a post-tool-use hook (`.claude/hooks/`) that reminds the agent to annotate after committing. When Claude Code is the authoring agent, it uses the live annotation path -- zero LLM cost because the agent already knows the intent behind its changes.

## License

MIT
