# Ultragit: Installation & Setup

## From Zero to Annotated Repository

---

## 1. Overview

Getting Ultragit running involves five steps:

1. **Install the binary** — get `ultragit` on your PATH.
2. **Initialize a repository** — install hooks, configure the notes ref.
3. **Configure credentials** — ensure an LLM provider is available.
4. **Backfill historical commits** — annotate the existing codebase.
5. **Install agent skills** — teach your agents when and how to use Ultragit.

Steps 1–3 take under two minutes. Step 4 runs in the background and takes minutes to hours depending on repository size. Step 5 depends on your agent framework.

An optional step 6 covers team configuration — syncing notes across clones so annotations are shared.

---

## 2. Install the Binary

Ultragit is a single statically-linked Rust binary with no runtime dependencies.

### From crates.io

```bash
cargo install ultragit
```

Requires a Rust toolchain. Installs to `~/.cargo/bin/` which should already be on your PATH.

### From GitHub Releases

```bash
# macOS (Apple Silicon)
curl -L https://github.com/anthropics/ultragit/releases/latest/download/ultragit-aarch64-apple-darwin.tar.gz | tar xz
sudo mv ultragit /usr/local/bin/

# macOS (Intel)
curl -L https://github.com/anthropics/ultragit/releases/latest/download/ultragit-x86_64-apple-darwin.tar.gz | tar xz
sudo mv ultragit /usr/local/bin/

# Linux (x86_64)
curl -L https://github.com/anthropics/ultragit/releases/latest/download/ultragit-x86_64-unknown-linux-musl.tar.gz | tar xz
sudo mv ultragit /usr/local/bin/
```

### From Homebrew

```bash
brew install anthropics/tap/ultragit
```

### Verify

```bash
ultragit --version
# ultragit 0.1.0
```

---

## 3. Initialize a Repository

From the root of any git repository:

```bash
ultragit init
```

This does several things:

**Installs git hooks.** Ultragit installs three hooks into `.git/hooks/`:

- `post-commit` — triggers annotation after each commit.
- `prepare-commit-msg` — detects squash merges and captures source commit SHAs for annotation synthesis.
- `post-rewrite` — migrates annotations when commits are amended.

If any of these hooks already exist, Ultragit does not overwrite them. Instead, it appends a Ultragit invocation to the existing script, preserving the existing hook behavior. Ultragit logs a message showing what was added:

```
✓ Appended to existing post-commit hook
✓ Installed prepare-commit-msg hook
✓ Installed post-rewrite hook
```

**Creates the notes ref.** Initializes `refs/notes/ultragit` if it doesn't exist.

**Creates the Ultragit directory.** Creates `.git/ultragit/` for temporary state (pending squash metadata, failed annotation logs).

**Writes default configuration.** Adds a `[ultragit]` section to `.git/config`:

```ini
[ultragit]
    enabled = true
    async = true
    noteref = refs/notes/ultragit
```

### Init Options

```bash
# Use a specific LLM provider and model
ultragit init --provider anthropic --model claude-sonnet-4-5-20250929

# Synchronous mode (blocks on commit until annotation completes)
ultragit init --sync

# Only annotate files matching these patterns
ultragit init --include "src/**" --include "lib/**"

# Exclude patterns
ultragit init --exclude "*.generated.*" --exclude "vendor/**"

# Skip hook installation (just configure, useful if managing hooks externally)
ultragit init --no-hooks

# Dry run — show what would be installed without doing it
ultragit init --dry-run
```

### Confirming Installation

`ultragit init` performs a credential check inline and reports results immediately:

```
ultragit init
  ✓ Hooks installed: post-commit, prepare-commit-msg, post-rewrite
  ✓ Notes ref created: refs/notes/ultragit
  ✓ Credentials: ANTHROPIC_API_KEY found
  ✓ Dry-run annotation test... OK

  Ultragit is ready. Your next commit will be annotated.
```

If credentials are missing, init still completes but warns loudly:

```
ultragit init
  ✓ Hooks installed: post-commit, prepare-commit-msg, post-rewrite
  ✓ Notes ref created: refs/notes/ultragit
  ✗ No LLM credentials found.
    Ultragit hooks are installed but annotations will fail until credentials are configured.
    Run: ultragit auth check
```

After the first successful annotation, Ultragit prints a one-time confirmation:

```
[ultragit] ✓ First annotation written. Run 'ultragit show HEAD' to see it.
```

For a detailed view of current status, use `ultragit status`:

```bash
ultragit status
```

```
Ultragit v0.1.0
  Repository:    /home/user/project
  Hooks:         ✓ post-commit  ✓ prepare-commit-msg  ✓ post-rewrite
  Notes ref:     refs/notes/ultragit (0 annotated commits)
  Provider:      anthropic (claude-sonnet-4-5-20250929)
  Credentials:   ✓ ANTHROPIC_API_KEY found
  Config:
    async:       true
    include:     src/**, lib/**
    exclude:     *.generated.*, vendor/**
    maxDiffLines: 2000
```

---

## 4. Configure Credentials

Ultragit needs access to an LLM provider. It discovers credentials automatically using the following chain (first match wins):

| Priority | Source | How to configure |
|---|---|---|
| 1 | `ANTHROPIC_API_KEY` env var | `export ANTHROPIC_API_KEY=sk-ant-...` |
| 2 | Claude CLI credentials | Install Claude Code; credentials at `~/.config/claude/` are picked up automatically |
| 3 | `OPENAI_API_KEY` env var | `export OPENAI_API_KEY=sk-...` |
| 4 | `GOOGLE_API_KEY` or `GEMINI_API_KEY` env var | `export GOOGLE_API_KEY=...` |
| 5 | `OPENROUTER_API_KEY` env var | `export OPENROUTER_API_KEY=...` |
| 6 | `ULTRAGIT_API_KEY` + `ULTRAGIT_PROVIDER` env vars | Explicit override for any provider |

For most users, one of these is already set. If you use Claude Code, Ultragit picks up your existing credentials with zero configuration.

### Verifying Credentials

```bash
ultragit auth check
```

```
✓ Anthropic API key found (ANTHROPIC_API_KEY)
  Model: claude-sonnet-4-5-20250929
  Testing connection... ✓ OK
```

If no credentials are found:

```
✗ No LLM credentials found.

  Set one of the following:
    export ANTHROPIC_API_KEY=sk-ant-...    (preferred)
    export OPENAI_API_KEY=sk-...
    export OPENROUTER_API_KEY=...

  Or install Claude Code to use subscription credentials:
    https://docs.anthropic.com/claude-code
```

### Pinning a Provider

If you have multiple API keys set and want to force a specific provider:

```bash
ultragit config set provider anthropic
ultragit config set model claude-sonnet-4-5-20250929
```

Or in `.git/config`:

```ini
[ultragit]
    provider = anthropic
    model = claude-sonnet-4-5-20250929
```

---

## 5. Backfill Historical Commits

A freshly initialized repository has zero annotations. New commits going forward will be annotated automatically, but the existing codebase — potentially years of history — is unannotated. Backfilling retroactively annotates historical commits so that `ultragit read` has context for existing code from day one.

### Basic Backfill

```bash
ultragit backfill
```

With no arguments, this annotates every commit on the current branch that doesn't already have a Ultragit note. It processes commits oldest-first so that annotation references between commits can be built chronologically.

This is the most expensive operation Ultragit performs. Every commit requires an LLM API call. For a repository with 1,000 commits, at ~5 seconds per annotation, expect roughly 90 minutes. Ultragit logs progress:

```
Backfilling 1,247 commits on main...
  [=====>                    ] 127/1,247 (10%)  ~18 min remaining
  Current: abc1234 "initial MQTT client implementation"
```

### Scoped Backfill

For large repositories, full backfill may be impractical or unnecessary. Scope it down:

```bash
# Only backfill the last 100 commits
ultragit backfill --limit 100

# Only backfill commits since a date
ultragit backfill --since 2025-01-01

# Only backfill commits since a specific commit
ultragit backfill --since abc1234

# Only backfill commits that touch specific paths
ultragit backfill --path src/mqtt/ --path src/tls/

# Combine: recent commits touching critical paths
ultragit backfill --since 2025-01-01 --path src/mqtt/
```

The `--path` filter is particularly useful. If your agent's next task involves the MQTT client, backfill just the MQTT-related history. You get relevant annotations in minutes instead of hours.

### Backfill Performance Options

```bash
# Control concurrency (default: 4 concurrent API calls)
ultragit backfill --concurrency 8

# Use a cheaper/faster model for bulk backfill
ultragit backfill --model claude-haiku-4-5-20251001

# Dry run — show which commits would be annotated
ultragit backfill --dry-run

# Resume a previously interrupted backfill
ultragit backfill --resume
```

The `--model` flag is worth noting. Backfill annotations are inherently `inferred` (no agent-provided context exists for historical commits). Using a faster, cheaper model for backfill is a reasonable tradeoff — the annotations are less rich than real-time `enhanced` annotations regardless of model quality. Save the expensive model for live commits where enhanced context is available.

### Backfill Output

After completion:

```
Backfill complete.
  Commits annotated:  1,247
  Commits skipped:    23 (empty commits, merge-only)
  Annotations stored: refs/notes/ultragit
  API calls:          1,247
  Estimated cost:     $3.42

  Run `ultragit status` to verify.
  Run `ultragit read <file>` to retrieve annotations.
```

---

## 6. Configure Notes Sync

By default, git notes are local. Other clones of the repository won't see your annotations. For solo use this is fine. For teams, you want annotations to travel with the repository.

### Enable Push

```bash
ultragit sync enable
```

This adds the notes ref to your push and fetch configuration:

```ini
[remote "origin"]
    push = refs/notes/ultragit
    fetch = +refs/notes/ultragit:refs/notes/ultragit
```

Equivalent to running manually:

```bash
git config --add remote.origin.push refs/notes/ultragit
git config --add remote.origin.fetch "+refs/notes/ultragit:refs/notes/ultragit"
```

### Push Existing Annotations

```bash
git push origin refs/notes/ultragit
```

Or simply `git push` — with the push refspec configured, notes are included in normal pushes.

### Verify Sync

```bash
ultragit sync status
```

```
Notes sync: enabled
  Push refspec:  refs/notes/ultragit → origin
  Fetch refspec: +refs/notes/ultragit:refs/notes/ultragit
  Local notes:   1,247 annotated commits
  Remote notes:  1,130 annotated commits (117 not yet pushed)
```

### Team Setup

For a team adopting Ultragit, the recommended sequence is:

1. One person runs `ultragit init` and `ultragit backfill` on the shared repository.
2. They push annotations: `git push origin refs/notes/ultragit`.
3. They commit a `.ultragit-config` file (optional, see below) that records the recommended Ultragit configuration for the repository.
4. Each team member installs Ultragit and runs `ultragit init`.
5. On the next `git fetch`, everyone receives the annotations.

When `ultragit init` detects that the remote already has `refs/notes/ultragit`, it automatically configures sync and informs the user:

```
ultragit init
  ✓ Hooks installed: post-commit, prepare-commit-msg, post-rewrite
  ✓ Notes ref created: refs/notes/ultragit
  ✓ Detected existing Ultragit annotations on origin (1,247 commits)
  ✓ Sync configured automatically
  ✓ Credentials: ANTHROPIC_API_KEY found
  ✓ Dry-run annotation test... OK

  Ultragit is ready. Your next commit will be annotated.
```

This removes the manual `ultragit sync enable` step for the common team-joining scenario. If no remote annotations are detected, sync is not configured automatically and must be enabled explicitly.

### Shared Configuration File

For team consistency, Ultragit supports a `.ultragit-config.toml` file in the repository root:

```toml
# .ultragit-config.toml
# Checked into the repository. Ultragit reads this as a base configuration
# that can be overridden by .git/config settings.

[ultragit]
enabled = true
async = true

[ultragit.model]
# Recommended model for this repository
provider = "anthropic"
model = "claude-sonnet-4-5-20250929"
# Cheaper model acceptable for backfill
backfill_model = "claude-haiku-4-5-20251001"

[ultragit.scope]
include = ["src/**", "lib/**", "config/**"]
exclude = ["*.generated.*", "vendor/**", "node_modules/**"]
max_diff_lines = 2000

[ultragit.sync]
# Prompt users to enable sync on init
auto_sync = true
```

When `ultragit init` detects this file, it uses it as the default configuration and prompts:

```
Found .ultragit-config.toml with repository defaults.
  Provider:  anthropic (claude-sonnet-4-5-20250929)
  Include:   src/**, lib/**, config/**
  Sync:      enabled

  Apply these settings? [Y/n]
```

### CI Annotation for Server-Side Squash Merges

GitHub and GitLab "Squash and merge" buttons perform merges server-side, which bypasses local hooks entirely. Without explicit handling, annotations from feature branch commits are lost when squash-merged.

Add a CI workflow to annotate squash merges after they land:

```yaml
# .github/workflows/ultragit-annotate.yml
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
      - run: |
          cargo install ultragit
          ultragit annotate --commit HEAD \
            --squash-sources $(git log --format=%H origin/main..HEAD~1)
```

The `--squash-sources` flag tells Ultragit to synthesize the annotation from the original feature branch commits, preserving the context that would otherwise be lost. This is the same mechanism the local `prepare-commit-msg` hook uses for local squash merges, applied to the CI case.

### Backup and Portability

Annotations are durable in git notes, but for repository migrations, hosting platform changes, or backup purposes, Ultragit provides export/import in a portable JSON format:

```bash
# Export all annotations to a portable JSON file
ultragit export > annotations.json

# Import annotations after a repo migration
ultragit import annotations.json

# Export only annotations for specific paths
ultragit export --path src/mqtt/ > mqtt-annotations.json
```

Exported files are self-contained: each annotation includes the commit SHA, timestamp, and full annotation payload. Import matches annotations to commits by SHA, so it works across clones and forks as long as the commit history is shared.

---

## 7. Install Agent Skills

Ultragit's value depends on agents knowing it exists and using it. The skill definition (documented in the Reading Agent specification) must be installed into whatever agent framework you use.

### Claude Code

Claude Code supports custom skills via the `CLAUDE.md` file in your repository root or in `~/.claude/CLAUDE.md` globally. Add the Ultragit skill:

```bash
ultragit skill install --target claude-code
```

This appends the Ultragit skill definition to your `CLAUDE.md`. The skill teaches Claude Code to:

- Run `ultragit read` before modifying existing code.
- Run `ultragit deps` before changing function behavior or signatures.
- Run `ultragit summary` when orienting on an unfamiliar module.
- Use `ultragit commit` or `ultragit context set` to provide context before committing.

If `CLAUDE.md` doesn't exist, Ultragit creates it with the skill definition as its initial content.

You can also install the skill globally so it applies to all repositories:

```bash
ultragit skill install --target claude-code --global
```

This writes to `~/.claude/CLAUDE.md`.

### Generic Skill File

For other agent frameworks, Ultragit can emit the raw skill definition:

```bash
# Write skill definition to a file
ultragit skill export > ultragit-skill.md

# Write to a specific path
ultragit skill export --output .agent/skills/ultragit.md
```

The exported file is a self-contained Markdown document that any LLM can follow. It includes:

- When to use each Ultragit command.
- Command syntax and common invocations.
- How to read the output (which fields matter, what confidence scores mean).
- How to provide context when committing (`ultragit commit` and `ultragit context set`).
- Common patterns and antipatterns.

### MCP Tool Definition (Future)

```bash
ultragit skill install --target mcp
```

Generates an MCP tool definition that exposes `ultragit_read`, `ultragit_deps`, `ultragit_history`, and `ultragit_summary` as tools an MCP-connected agent can call directly. This is the cleanest integration path — the agent calls Ultragit as a tool rather than shelling out — but requires MCP support in the agent framework.

### Verifying Skill Installation

```bash
ultragit skill check
```

```
Skill installations found:
  ✓ Claude Code (CLAUDE.md in repository root)
    Last updated: 2025-12-15
    Skill version: ultragit-skill/v1

  ✗ Claude Code global (~/.claude/CLAUDE.md)
    Not installed. Run: ultragit skill install --target claude-code --global

  ✗ MCP
    Not installed. Run: ultragit skill install --target mcp
```

---

## 8. Post-Setup Verification

After completing setup, verify everything works end-to-end:

### Make a Test Commit

```bash
echo "// test" >> src/main.rs
git add src/main.rs
git commit -m "test: verify ultragit annotation"
```

Wait a few seconds (if async mode), then:

```bash
ultragit show HEAD
```

You should see a structured JSON annotation:

```json
{
  "$schema": "ultragit/v1",
  "commit": "a1b2c3d",
  "timestamp": "2025-12-15T10:30:00Z",
  "context_level": "inferred",
  "summary": "Added a test comment to main.rs...",
  "regions": [...]
}
```

### Test Reading

```bash
ultragit read src/main.rs --compact
```

Should return annotations for any annotated commits that touch `main.rs`, including the test commit and any backfilled history.

### Test with Agent Context

The recommended way to provide agent context is the `ultragit commit` wrapper:

```bash
echo "// test with context" >> src/main.rs
git add src/main.rs
ultragit commit -m "test: verify enhanced annotation" \
  --task "Testing Ultragit installation" \
  --reasoning "Verifying that enhanced context is captured correctly"
ultragit show HEAD
```

`ultragit commit` writes context to `.git/ultragit/pending-context.json`, then calls `git commit` with any pass-through flags. The post-commit hook reads and consumes the context file. No ambient state leakage — context is scoped to exactly one commit.

For workflows where you want to set context separately from committing, use `ultragit context set`:

```bash
ultragit context set --task "PROJ-442" --reasoning "Chose bounded pool over unbounded to prevent memory exhaustion under load"
git commit -m "add connection pooling"
# Hook reads and consumes .git/ultragit/pending-context.json
```

The annotation should show `"context_level": "enhanced"` and include the task and reasoning you provided.

#### Environment Variables (Fallback)

The original env var approach still works and is useful for CI pipelines and scripts where the `ultragit` binary may not be available at commit time:

```bash
export ULTRAGIT_TASK="Testing Ultragit installation"
export ULTRAGIT_REASONING="Verifying that enhanced context is captured correctly"
git commit -m "test: verify enhanced annotation"
```

Prefer `ultragit commit` for interactive and agent workflows. Use env vars when integrating with CI systems or existing commit tooling that cannot be easily wrapped.

### Revert Test Commits

```bash
git reset --hard HEAD~2
```

---

## 9. Uninstall

### Remove from a Single Repository

```bash
ultragit uninstall
```

This removes:

- Ultragit hook invocations from `.git/hooks/` (preserving any non-Ultragit hook content).
- The `[ultragit]` section from `.git/config`.
- The `.git/ultragit/` directory.

It does **not** remove:

- Existing annotations in `refs/notes/ultragit`. These are harmless metadata and can be removed manually with `git notes --ref=ultragit prune` or `git update-ref -d refs/notes/ultragit`.
- The `.ultragit-config.toml` file (this is tracked in the repository and should be removed via a normal commit if desired).
- Skill definitions from `CLAUDE.md` or other agent configurations (these reference the CLI and will be harmlessly ignored if the binary is not on PATH).

### Remove the Binary

```bash
# If installed via cargo
cargo uninstall ultragit

# If installed via Homebrew
brew uninstall ultragit

# If installed manually
rm /usr/local/bin/ultragit
```

---

## 10. Quick Reference

```bash
# Full setup from scratch
cargo install ultragit
cd /path/to/repo
ultragit init
ultragit backfill --since 2025-01-01 --concurrency 8
ultragit sync enable
ultragit skill install --target claude-code
git push origin refs/notes/ultragit

# Verify everything
ultragit doctor

# Day-to-day usage (handled by hooks and agent skills)
# Hooks annotate automatically on commit.
# Agents use `ultragit read` / `ultragit deps` before modifying code.
# That's it.
```

### `ultragit doctor`

A single command that validates the entire setup:

```bash
ultragit doctor
```

```
ultragit doctor
  ✓ Binary version: 0.1.0 (up to date)
  ✓ Hooks: post-commit, prepare-commit-msg, post-rewrite
  ✓ Credentials: ANTHROPIC_API_KEY found, connection OK
  ✓ Sync: configured, 0 unpushed annotations
  ✓ Skill: installed in CLAUDE.md (current version)
  ✓ Last annotation: 2 hours ago (commit abc1234)
  ✗ Backfill: 342 commits unannotated in last 6 months
    Run: ultragit backfill --since 2025-06-01
```

This combines the checks from `ultragit status`, `ultragit auth check`, `ultragit sync status`, and `ultragit skill check` into one diagnostic command. Run it after initial setup to confirm everything is working, or any time annotations stop appearing.

---

## 11. Troubleshooting

### "No LLM credentials found"

Run `ultragit auth check` to see which credentials are detected. The most common fix is setting `ANTHROPIC_API_KEY` in your shell profile. If you use Claude Code, ensure it's logged in — Ultragit picks up its credentials automatically.

### Annotations not appearing after commit

Check if the hook is installed: `cat .git/hooks/post-commit`. It should contain a `ultragit annotate` invocation. If running in async mode (default), annotations may take a few seconds to appear. Check `.git/ultragit/failed.log` for errors.

### Backfill is slow

Use `--concurrency` to increase parallelism (default is 4). Use `--model` with a faster model (Haiku) for bulk backfill. Scope with `--path` and `--since` to annotate only what you need now.

### Notes not syncing to remote

Run `ultragit sync status` to verify refspecs are configured. Some Git hosting platforms have restrictions on pushing custom refs. GitHub supports `refs/notes/*` pushes but doesn't display them in the UI. GitLab and Bitbucket may behave differently — verify with `git push origin refs/notes/ultragit` and check for errors.

### Hook conflicts

If another tool manages your git hooks (Husky, Lefthook, pre-commit), `ultragit init` may not be able to install its hooks directly. Use `ultragit init --no-hooks` and manually add Ultragit invocations to your hook manager's configuration. For example, with Lefthook:

```yaml
# .lefthook.yml
post-commit:
  commands:
    ultragit:
      run: ultragit annotate --commit HEAD --async
```

### Existing annotations on amended commits

When you amend a commit, the `post-rewrite` hook migrates the annotation from the old SHA to the new one. If this fails (e.g., hook not installed properly), the old annotation is orphaned but harmless. Run `ultragit annotate --commit HEAD` manually to re-annotate the amended commit.