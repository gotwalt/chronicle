# Feature 24: Remove Batch Annotation and Backfill Paths

**Status**: Complete

## Motivation

Chronicle originally had two annotation paths:

1. **Batch path** — an LLM agent loop (`src/agent/`) that analyzed diffs and
   produced annotations via tool calls. Required an LLM provider (Anthropic API
   key), added ~3,400 lines of code, and introduced latency + cost per annotation.

2. **Live path** — the caller provides the annotation as JSON. Zero cost, instant.

In practice, the batch path was never the right fit. Claude Code (and other AI
coding agents) already have the reasoning context when they make a commit — they
know *why* they chose this approach, what they tried that failed, and what's
fragile. Having Chronicle make a *second* LLM call to reconstruct that reasoning
from the diff is wasteful and produces inferior results.

The live path is strictly better for the primary use case: an AI agent annotates
immediately after committing, while reasoning is still in context. The batch path
only made sense for backfilling historical commits, but even there the quality
was limited — an LLM analyzing a cold diff can't know what alternatives were
considered or what constraints shaped the decision.

**The core insight**: the agent that wrote the code is the best annotator. Making
a separate LLM call to guess at the reasoning is an anti-pattern. Chronicle's
value is in *capturing* wisdom, not *generating* it.

---

## What Was Removed

### Deleted modules (~3,400 lines)

| Module | Purpose | Lines |
|--------|---------|-------|
| `src/agent/mod.rs` | Agent conversation loop | ~400 |
| `src/agent/prompt.rs` | System prompt construction | ~300 |
| `src/agent/tools.rs` | Tool definitions and dispatch | ~500 |
| `src/provider/mod.rs` | LLM provider trait | ~100 |
| `src/provider/anthropic.rs` | Anthropic API client | ~400 |
| `src/provider/claude_code.rs` | Claude Code subprocess provider | ~300 |
| `src/annotate/filter.rs` | Diff filtering for LLM context | ~200 |
| `src/annotate/gather.rs` | Context gathering for batch | ~300 |
| `src/cli/context.rs` | `git chronicle context` command | ~150 |
| `src/cli/reconfigure.rs` | `git chronicle reconfigure` command | ~100 |
| `src/cli/backfill.rs` | `git chronicle backfill` command | ~200 |

### Simplified modules

| Module | Change |
|--------|--------|
| `src/error.rs` | Removed `ProviderError` (8 variants), `AgentError` (6 variants), `ChronicleError::Provider/Agent` |
| `src/hooks/mod.rs` | Removed `PendingContext`, `AuthorContext`, context capture machinery |
| `src/doctor.rs` | Removed `check_credentials()` (LLM provider config check) |
| `src/setup/mod.rs` | Removed `prompt_provider_selection()`, provider config writes |
| `src/cli/init.rs` | Removed `provider`/`model` parameters |
| `src/cli/annotate.rs` | Removed batch fallback; now errors if no mode flag specified |
| `Cargo.toml` | Removed `ureq` dependency (HTTP client for LLM API calls) |

### Removed CLI commands

| Command | What it did |
|---------|------------|
| `git chronicle context set` | Set context for next batch annotation |
| `git chronicle reconfigure` | Change LLM provider/model settings |
| `git chronicle backfill` | Batch-annotate historical commits via LLM |

### Changed behavior

| Before | After |
|--------|-------|
| `git chronicle annotate` (no flags) → batch LLM annotation | Error: must specify `--live`, `--summary`, `--auto`, `--json`, `--squash-sources`, or `--amend-source` |
| `git chronicle setup` prompts for LLM provider | Only installs skills, hooks, CLAUDE.md |
| `git chronicle init --provider anthropic` | `git chronicle init` (no provider params) |
| Post-commit hook: `git-chronicle annotate --commit HEAD --sync &` | `git-chronicle annotate --auto --commit HEAD &` |
| `git chronicle doctor` checks LLM credentials | Only checks skills directory |

---

## What Remains

The annotation system is now purely caller-driven:

- **`--live`** — Rich JSON annotation from stdin (primary path for AI agents)
- **`--summary`** — Quick string summary (typos, renames, dep bumps)
- **`--auto`** — Uses commit message as summary (post-commit hook default)
- **`--json`** — Inline JSON on command line
- **`--squash-sources`** — Synthesize annotation from squashed commits
- **`--amend-source`** — Carry annotation forward on amend

All reading commands (`read`, `contracts`, `decisions`, `deps`, `history`,
`summary`, `lookup`) are unchanged. The knowledge store is unchanged. Team
operations (sync, export, import) are unchanged.

---

## Lessons Learned

### Dead ends

1. **LLM-as-annotator was the wrong abstraction.** We built a full agent loop
   with tool dispatch, provider abstraction, prompt engineering, and diff
   filtering — all to have an LLM guess at reasoning that was already known
   by the calling agent. The complexity was high and the output quality was
   mediocre compared to live annotations.

2. **Provider abstraction was premature.** We built traits for Anthropic,
   OpenAI, and Claude Code subprocess providers, but only Anthropic was ever
   implemented. The abstraction layer added complexity without enabling anything.

3. **Backfill produced low-value annotations.** An LLM analyzing a cold diff
   months later can identify *what* changed but not *why* — which is the entire
   point of Chronicle. Backfilled annotations were mostly summaries of the diff,
   adding no information beyond what `git log` already provides.

### Insights

1. **The agent that writes the code should annotate it.** This is Chronicle's
   fundamental design principle post-removal. The live path captures reasoning
   at the moment of highest context, with zero additional LLM cost.

2. **Fewer lines = better maintainability.** Removing ~3,400 lines reduced the
   test surface, simplified the error hierarchy, eliminated an external dependency
   (ureq), and made the codebase navigable in a single session.

3. **The post-commit hook had a double bug.** It called the batch path (which
   was being removed) with a `--sync` flag that never existed on the annotate
   command. Replacing it with `--auto` was both a fix and a simplification.

### Gotchas

1. **Stale documentation is pervasive.** The CLAUDE.md, embedded snippets,
   skill files, and README all referenced v2 schema fields (`rejected_alternatives`,
   `decisions`, `markers`) and batch-era concepts (`provider`, `reconfigure`).
   Each documentation surface needed independent verification and update.

2. **`#[snafu(module(...))]` error variants cascade.** Removing `ProviderError`
   and `AgentError` required removing the `ChronicleError::Provider` and
   `ChronicleError::Agent` variants that wrapped them, which required updating
   all error handling call sites.

---

## Test Impact

- Before: 213 tests
- After: 201 tests (175 unit + 10 git_ops + 16 team_ops)
- 12 tests removed (agent loop, provider, batch annotation tests)
- All remaining tests pass; no test modifications needed beyond removing deleted modules

---

## Dependencies

- **Feature 22 (v3 schema)**: Must be complete first — the batch path emitted
  v3 annotations, so removing it required v3 to be the established format.
- **Feature 23 (agent knowledge capture)**: Was simplified by this removal —
  the original spec included `emit_knowledge` as an agent tool, which was
  removed in favor of a `knowledge` field in `LiveInput` only.

---

## Acceptance Criteria

1. All batch/backfill code removed (agent loop, providers, context capture)
2. `git chronicle annotate` without flags returns a clear error message
3. Setup no longer prompts for LLM provider
4. Post-commit hook uses `--auto` path
5. All 201 remaining tests pass
6. Documentation updated: README, CLAUDE.md, skills, embedded snippets
7. No runtime dependency on external LLM services
