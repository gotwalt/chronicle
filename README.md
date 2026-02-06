# Chronicle

**Git commits record *what* changed. Chronicle records *why*.**

Chronicle is a CLI tool that captures the intent, reasoning, and constraints behind code changes as structured metadata stored alongside your commits. It works with any git repository, requires no external services, and integrates directly into your existing workflow.

```
$ git chronicle read src/auth/middleware.rs --anchor validate_token

  intent: Extract token validation into standalone function for testability
  reasoning: Previous inline validation made unit testing impossible without
             spinning up a full HTTP server. Standalone function can be tested
             with mock tokens.
  constraints:
    - Must validate expiry before signature check (short-circuit on expired tokens)
  depends on: src/auth/jwt.rs::decode (assumes valid UTF-8 payload)
```

## Why

Every line of code exists because someone made a decision. They chose this data structure, added that retry loop, bounded a cache at four entries for a reason. That reasoning lives in the developer's head -- or the AI agent's context window -- and is gone within hours.

Chronicle captures this knowledge at commit time and makes it queryable. When you (or an AI agent) modify code later, you can check what assumptions it was built on, what depends on its behavior, and what will break if you change it.

## Install

```bash
cargo install git-chronicle
```

This puts the `git-chronicle` binary on your PATH. Git discovers it automatically -- all commands are available as `git chronicle <command>`.

Requires Rust 1.70+ and git.

## Getting started

```bash
# One-time machine setup (configures your LLM provider, installs Claude Code skills)
git chronicle setup

# Initialize Chronicle in a repository
cd my-project
git chronicle init

# That's it. Make commits normally:
git commit -m "refactor auth middleware"

# Then annotate the commit:
git chronicle annotate --commit HEAD
```

### Annotation paths

Chronicle has two ways to annotate commits:

**Batch path** -- an LLM reads the diff and produces annotations automatically. Requires an API key (`ANTHROPIC_API_KEY`).

```bash
git chronicle annotate --commit HEAD
```

**Live path** -- you (or an AI agent) provide the annotation as JSON. Zero cost, instant.

```bash
cat > /tmp/annotation.json << 'EOF'
{
  "commit": "HEAD",
  "summary": "Extract token validation for testability",
  "regions": [
    {
      "file": "src/auth/middleware.rs",
      "anchor": {"unit_type": "function", "name": "validate_token"},
      "intent": "Standalone function enables unit testing without full HTTP server",
      "reasoning": "Inline validation required integration test setup for every case",
      "constraints": ["Must validate expiry before signature check"]
    }
  ]
}
EOF
git chronicle annotate --live < /tmp/annotation.json
```

The live path is what Claude Code and other AI agents use -- they already know the intent behind their changes, so no LLM call is needed.

## Reading annotations

```bash
# Read annotations for a file
git chronicle read src/auth/middleware.rs

# Read a specific function's annotations
git chronicle read src/auth/middleware.rs --anchor validate_token

# Find code that depends on a function's behavior
git chronicle deps src/auth/jwt.rs --anchor decode

# See how a file's annotations evolved over time
git chronicle history src/auth/middleware.rs

# Get a condensed overview of all annotations in a file
git chronicle summary src/auth/middleware.rs
```

## Correcting annotations

Annotations can be wrong. Chronicle provides a correction mechanism rather than silent overwrites, so the evolution of understanding is preserved.

```bash
# Flag an annotation as potentially inaccurate
git chronicle flag src/auth/middleware.rs --anchor validate_token --reason "constraint is outdated"

# Apply a correction
git chronicle correct <sha> --region validate_token --field constraints --value '["Expiry check removed in v2"]'
```

## Team workflows

Annotations are stored as git notes and can be synced across your team.

```bash
# Enable sync with your remote
git chronicle sync enable

# Pull annotations from teammates
git chronicle sync pull

# Check sync status
git chronicle sync status

# Export/import for backups or migration
git chronicle export > annotations.jsonl
git chronicle import annotations.jsonl

# Run diagnostics
git chronicle doctor
```

## Backfilling historical commits

Already have a repository with history? Chronicle can annotate past commits.

```bash
# Annotate the last 20 commits
git chronicle backfill --limit 20
```

## Claude Code integration

Chronicle is designed to work seamlessly with [Claude Code](https://docs.anthropic.com/en/docs/claude-code). After running `git chronicle setup`, Claude Code will:

1. **Read annotations before modifying code** -- checking intent, constraints, and dependencies to avoid breaking assumptions
2. **Annotate after committing** -- using the live path (zero LLM cost) since it already knows why it made each change

This creates a feedback loop: agents leave structured context for future agents, dramatically reducing regressions from lost reasoning.

## How it works

Annotations are stored as [git notes](https://git-scm.com/docs/git-notes) under `refs/notes/chronicle`. Each commit gets a structured JSON annotation containing:

- **Summary** -- what the commit accomplishes and why
- **Regions** -- per-function annotations with intent, reasoning, constraints, and dependencies
- **Cross-cutting concerns** -- patterns that span multiple regions
- **Provenance** -- how the annotation was produced (initial, amended, synthesized from squash)

Reading uses `git blame` to find which commits produced the lines you're looking at, then fetches their annotations. No external database, no index to build -- it's just git.

## All commands

| Command | Description |
|---------|-------------|
| `git chronicle setup` | One-time machine-wide setup (LLM provider, skills, hooks) |
| `git chronicle init` | Initialize Chronicle in the current repository |
| `git chronicle annotate` | Annotate a commit (`--commit <sha>` for batch, `--live` for stdin) |
| `git chronicle backfill` | Annotate historical commits |
| `git chronicle read` | Read annotations for a file (`--anchor`, `--lines`) |
| `git chronicle deps` | Find code that depends on a file/anchor |
| `git chronicle history` | Annotation timeline across commits |
| `git chronicle summary` | Condensed annotation overview for a file |
| `git chronicle show` | Interactive TUI explorer |
| `git chronicle flag` | Flag an annotation as potentially inaccurate |
| `git chronicle correct` | Apply a correction to an annotation field |
| `git chronicle context set` | Set pending context for the next commit |
| `git chronicle sync` | Manage notes sync (`enable`, `status`, `pull`) |
| `git chronicle export` | Export annotations as JSONL |
| `git chronicle import` | Import annotations from JSONL |
| `git chronicle doctor` | Run diagnostic checks |
| `git chronicle reconfigure` | Rerun LLM provider selection |

## License

MIT
