# Chronicle

**Commit messages say what changed. Chronicle captures what was learned.**

Chronicle is a CLI tool that stores the reasoning, constraints, and hard-won
lessons behind code changes as structured metadata alongside your commits.
It works with any git repository, requires no external services, and turns
every commit into a piece of institutional memory that survives context
switches, team changes, and the end of an AI agent's session.

```
$ git chronicle contracts src/auth/middleware.rs --anchor validate_token

  contract: Must validate expiry before signature check — callers sleep on
            the return value without validation (short-circuit on expired tokens)
  source:   author (commit a1b2c3d)

$ git chronicle decisions --path src/auth/middleware.rs

  decision: Extract token validation into standalone function
  why:      Inline validation made unit testing impossible without a full HTTP server
  stability: permanent
```

## Why

A diff shows *what* changed. A commit message summarizes the change. Neither
captures the reasoning that shaped it — why this approach and not three others
that were tried first, what invisible constraint the code depends on, which
part of the design is load-bearing and which is provisional.

That reasoning lives in someone's head, or an AI agent's context window, and
is gone within hours. The next person to touch the code has to reverse-engineer
it from the implementation, often rediscovering the same dead ends.

Chronicle captures this knowledge at commit time and makes it queryable.
When you or an agent modify code later, you can check what assumptions it was
built on, what depends on its behavior, and what will break if you change it.

## Install

```bash
cargo install git-chronicle
```

This puts the `git-chronicle` binary on your PATH. Git discovers it
automatically — all commands are available as `git chronicle <command>`.

Requires Rust 1.70+ and git.

## Getting started

```bash
# One-time machine setup (configures your LLM provider, installs Claude Code skills)
git chronicle setup

# Initialize Chronicle in a repository
cd my-project
git chronicle init

# Make commits normally, then annotate:
git commit -m "refactor auth middleware"
git chronicle annotate --live << 'EOF'
{
  "commit": "HEAD",
  "summary": "Extract validation into standalone function — inline validation made unit testing impossible without spinning up a full HTTP server.",
  "decisions": [
    {"what": "Standalone function over test harness", "why": "Simpler, no mock HTTP server needed", "stability": "permanent"}
  ],
  "markers": [
    {
      "file": "src/auth/middleware.rs",
      "anchor": {"unit_type": "function", "name": "validate_token"},
      "kind": {"type": "contract", "description": "Must validate expiry before signature check; callers sleep on this without validation"}
    }
  ]
}
EOF
```

### Annotation paths

**Live path** — you or an AI agent provide the annotation as JSON. Zero cost,
instant. This is the primary path — agents already know the reasoning behind
their changes, so no LLM call is needed.

```bash
# Rich annotation with decisions, markers, and sentiments:
git chronicle annotate --live << 'EOF'
{"commit":"HEAD","summary":"Why this approach, not what changed","decisions":[...],"markers":[...]}
EOF

# Quick summary for trivial changes (typos, renames, dep bumps):
git chronicle annotate --summary "Pin serde to 1.0.193 — 1.0.194 has a regression with flattened enums."
```

**Batch path** — an LLM reads the diff and produces annotations automatically.
Useful for backfilling history. Requires an API key (`ANTHROPIC_API_KEY`).

```bash
git chronicle annotate --commit HEAD
```

### What goes in an annotation

The annotation schema (`chronicle/v2`) is narrative-first — the commit-level
*why* comes before any per-file detail:

| Field | Purpose |
|-------|---------|
| **summary** | Why this approach — must add information beyond the diff |
| **motivation** | What triggered the change |
| **rejected_alternatives** | Approaches tried and abandoned (highest-value field) |
| **decisions** | Design choices with stability level: `permanent`, `provisional`, `experimental` |
| **markers** | Per-file contracts, hazards, dependencies, and other code-level metadata |
| **sentiments** | Agent intuitions — worry, confidence, unease — that don't fit formal categories |
| **effort** | Link to a broader ticket or feature |

Agents can query the exact schema at runtime: `git chronicle schema annotate-input`.

## Querying annotations

```bash
# What constraints must I respect before editing this code?
git chronicle contracts src/auth/middleware.rs --anchor validate_token

# What was decided here and why?
git chronicle decisions --path src/auth/middleware.rs

# Quick orientation for a file
git chronicle summary src/auth/middleware.rs

# Read raw annotations
git chronicle read src/auth/middleware.rs --anchor validate_token

# What depends on this code's behavior?
git chronicle deps src/auth/jwt.rs --anchor decode

# How has understanding of this code evolved?
git chronicle history src/auth/middleware.rs

# One-stop context lookup (contracts + decisions + history)
git chronicle lookup src/auth/middleware.rs
```

## Knowledge store

Beyond per-commit annotations, Chronicle maintains a repo-wide knowledge store
for lessons that apply across many commits — coding conventions, module
boundaries, and anti-patterns discovered over time.

```bash
# List all knowledge entries
git chronicle knowledge list

# Record a convention
git chronicle knowledge add --type convention \
  --scope "src/git/" --rule "All git operations go through the GitOps trait" \
  --stability permanent

# Record an anti-pattern
git chronicle knowledge add --type anti-pattern \
  --pattern "Passing note content as CLI args" \
  --instead "Use -F tempfile to avoid shell escaping issues"

# Record a module boundary
git chronicle knowledge add --type boundary \
  --module "src/schema/" --owns "annotation types and migration" \
  --boundary "Never deserialize annotations directly; use parse_annotation()"

# Remove an entry
git chronicle knowledge remove conv-3
```

## Correcting annotations

Annotations can be wrong. Chronicle provides a correction mechanism rather
than silent overwrites, so the evolution of understanding is preserved.

```bash
# Flag an annotation as potentially inaccurate
git chronicle flag src/auth/middleware.rs --anchor validate_token \
  --reason "constraint is outdated after v2 refactor"

# Apply a correction
git chronicle correct <sha> --region validate_token \
  --field constraints --value '["Expiry check removed in v2"]'
```

## Team workflows

Annotations are stored as [git notes](https://git-scm.com/docs/git-notes)
under `refs/notes/chronicle` and sync like any other git data.

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
```

## Browsing annotations

```bash
# Interactive TUI explorer
git chronicle show

# Web viewer
git chronicle web
```

## Claude Code integration

Chronicle is designed to work with [Claude Code](https://docs.anthropic.com/en/docs/claude-code)
and other AI coding agents. Running `git chronicle setup` installs skills
and hooks so agents automatically:

1. **Read contracts and decisions before modifying code** — checking what
   assumptions the code was built on and what will break if they're violated
2. **Annotate after committing** — using the live path (zero LLM cost) to
   capture reasoning, rejected alternatives, and sentiments while they're
   still in context
3. **Accumulate knowledge** — recording repo-wide conventions and anti-patterns
   in the knowledge store as they're discovered

Each agent session builds on the last. Contracts written by one agent prevent
the next from making the same mistake. Rejected alternatives save future agents
from rediscovering dead ends. Over time, the repository accumulates a layer of
institutional memory that makes every agent (and every human) more effective.

## How it works

Annotations are stored as git notes under `refs/notes/chronicle`, separate
from your commit history. Each commit gets a structured JSON annotation
(`chronicle/v2` schema) containing:

- **Narrative** — summary, motivation, rejected alternatives, sentiments
- **Decisions** — design choices with stability levels (permanent, provisional, experimental)
- **Markers** — per-file metadata: contracts, hazards, dependencies, security boundaries, performance notes, deprecations, tech debt, test coverage gaps
- **Provenance** — how the annotation was produced (live, batch, backfill, squash, amend) and by whom

The knowledge store lives on a separate ref (`refs/notes/chronicle-knowledge`)
and holds repo-wide conventions, module boundaries, and anti-patterns.

Querying uses `git blame` to find which commits produced the lines you're
looking at, then fetches their annotations. No external database, no index
to build — it's just git.

## All commands

| Command | Description |
|---------|-------------|
| **Setup** | |
| `git chronicle setup` | One-time machine-wide setup (LLM provider, skills, hooks) |
| `git chronicle init` | Initialize Chronicle in the current repository |
| `git chronicle reconfigure` | Rerun LLM provider selection |
| **Writing** | |
| `git chronicle annotate` | Annotate a commit (`--live`, `--summary`, `--commit <sha>`) |
| `git chronicle backfill` | Annotate historical commits that lack annotations |
| `git chronicle note` | Stage a note to include in the next annotation |
| `git chronicle flag` | Flag an annotation as potentially inaccurate |
| `git chronicle correct` | Apply a correction to a specific annotation field |
| **Reading** | |
| `git chronicle read` | Read annotations for a file (`--anchor`, `--lines`) |
| `git chronicle contracts` | Query contracts and dependencies ("what must I not break?") |
| `git chronicle decisions` | Query design decisions and rejected alternatives |
| `git chronicle deps` | Find code that depends on a file/anchor |
| `git chronicle history` | Annotation timeline across commits |
| `git chronicle summary` | Condensed annotation overview for a file |
| `git chronicle lookup` | One-stop context lookup (contracts + decisions + history) |
| **Knowledge** | |
| `git chronicle knowledge list` | List repo-wide conventions, boundaries, and anti-patterns |
| `git chronicle knowledge add` | Add a knowledge entry |
| `git chronicle knowledge remove` | Remove a knowledge entry by ID |
| **Browsing** | |
| `git chronicle show` | Interactive TUI explorer |
| `git chronicle web` | Launch web viewer |
| **Team** | |
| `git chronicle sync` | Manage notes sync (`enable`, `status`, `pull`) |
| `git chronicle export` | Export annotations as JSONL |
| `git chronicle import` | Import annotations from JSONL |
| `git chronicle status` | Show annotation coverage for the repository |
| `git chronicle doctor` | Run diagnostic checks |
| **Developer** | |
| `git chronicle schema` | Print JSON Schema for annotation types |
| `git chronicle context` | Manage pending context (`set`, `show`, `clear`) |

## License

MIT
