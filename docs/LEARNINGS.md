# Chronicle Learnings Journal

A running record of what we learned building Chronicle — what the docs got right, what was wrong, and what surprised us. Each entry captures a point-in-time reflection so future decisions have context.

---

## Entry 1: Docs vs Reality Audit

**Date:** 2026-02-07
**Context:** Chronicle shipped Features 1–21 across multiple sessions. After a comprehensive codebase cleanup refactor (dead code removal, deduplication, type safety improvements), we reread the original five design docs (`vision.md`, `write.md`, `read.md`, `setup.md`, `competition.md`) — all written before any code existed — and compared them to what was actually built. The codebase at this point has 25 CLI commands, 202 unit tests, a v2 annotation schema, and a live annotation path that dominates real usage.

### The core thesis held up

The fundamental bet — structured reasoning captured at commit time, stored as git notes, queryable by agents — is fully realized and works. The five design docs described a system where AI agents could check constraints before modifying code, trace reasoning chains across commits, and avoid re-exploring rejected alternatives. That system exists and ships.

What changed is the *shape*, not the *substance*.

### What the docs got right

- **Git notes as storage substrate.** Zero-infrastructure, travels with the repo, syncs with push/fetch. No regrets.
- **Post-commit hook as capture point.** Annotations written at the moment reasoning is freshest. The hook chain (post-commit, prepare-commit-msg, post-rewrite) works exactly as designed.
- **Squash merge synthesis.** The prepare-commit-msg → tmpfile → post-commit handshake described in `write.md` shipped intact and handles the hardest annotation lifecycle case.
- **Blame-as-index for retrieval.** No separate index needed. `git log --follow` maps files to commits, notes provide the metadata. All local, no LLM on the read path.
- **Separation of write and read paths.** Batch (LLM-driven) and live (zero-cost, agent-provided) annotation paths. The read pipeline is purely local computation.
- **The competitive positioning.** `competition.md` argued Chronicle fills a specific gap (behavioral reasoning, rejected alternatives, cross-cutting constraints) that no existing tool covers. This remains accurate. The "honest segmentation" (high value for constraint-heavy systems, low for simple CRUD) has proven out.

### Where we diverged

#### Schema: v1 per-region → v2 narrative-first

The docs (especially `write.md` and `read.md`) describe a v1 schema organized around *code regions* — each region gets its own intent, reasoning, constraints, semantic_dependencies, related_annotations, tags, and risk_notes. Cross-cutting concerns are a separate top-level array.

What shipped is v2, which organizes around *commit narrative*: the commit has a summary, motivation, rejected alternatives, and follow-up. Markers point to specific code with typed kinds (Contract, Hazard, Dependency, etc.). Decisions are first-class objects with stability levels and scope.

**Why:** Developers think about what a commit does, not what each function does independently. Asking an LLM to populate per-region intent/reasoning/constraints for every touched function produced verbose, repetitive output. Narrative-first matches how people actually reason about changes.

#### Tree-sitter: implemented then removed

The docs devote significant space to tree-sitter AST parsing — anchor resolution (exact/unqualified/fuzzy matching), `get_ast_outline()` as an agent tool, language-specific support for Rust/TypeScript/Python. This was implemented, tested, and then removed entirely (commit `63229a9`).

**Why:** Massive dependency weight and language-specific maintenance for near-zero value. Agents already know what functions they're modifying. Simple string-based anchors like `fn validate` are sufficient. The fuzzy matching / Levenshtein distance logic described in `read.md` was never needed because anchor names provided by agents are already exact.

#### Confidence scoring → staleness detection

`read.md` describes a weighted confidence model: recency (40%), context_level (30%), anchor_stability (20%), provenance (10%), producing a 0.0–1.0 float per annotation. Token budget trimming would use these scores to decide what to drop.

What shipped: a boolean staleness check. Has the file been modified more than N commits since the annotation? Yes/no.

**Why:** The only question anyone actually asks is "is this annotation still relevant?" A float nobody calibrates is less useful than a binary signal. Staleness detection integrated into `doctor` and `lookup` gives actionable output.

#### Async annotation → sync live path

The docs say "async by default" — the post-commit hook fires annotation into the background so it doesn't block the developer's workflow.

What happened: the live annotation path (where the agent provides context directly via JSON on stdin) takes <1 second with zero LLM calls. There's nothing to background. The batch path (LLM-driven) does run in the foreground but is only used for backfill, not real-time annotation.

**Why:** The live path changed the economics. When there's no API call, async is unnecessary complexity.

#### Multi-provider → Anthropic only

`write.md` and `setup.md` describe four LLM providers (Anthropic, OpenAI, Gemini, OpenRouter) with a credential discovery chain and per-provider abstraction.

What shipped: Anthropic and Claude Code providers only.

**Why:** Chronicle is a tool for Claude Code users. Provider proliferation is maintenance without demonstrated demand. The `LlmProvider` trait exists and could support others, but we haven't needed to.

#### `git chronicle commit` → `context set` + `note`

The docs describe a `git chronicle commit` wrapper with `--task`, `--reasoning`, `--dependencies`, `--tags` flags that wraps `git commit` to capture context at commit time.

What shipped: `git chronicle context set` and `git chronicle note` as separate commands. Users commit normally and layer context alongside their existing workflow.

**Why:** Wrapping `git commit` fights muscle memory. Developers have their own commit workflows, aliases, and tooling. Providing context as a side-channel (staged notes, context set) integrates without friction.

#### Token budget trimming → not implemented

`read.md` describes `--max-tokens` with a field trimming hierarchy: drop related → reasoning → risk_notes → tags, always preserving intent and constraints. Regions dropped newest-first.

**Why not built:** Annotations are small JSON (typically 500–2000 bytes). Token limits haven't been a real problem. The `--compact` flag on query commands handles the "less output" case simply.

#### Cross-cutting concerns → dropped in v2

V1 schema had a `cross_cutting[]` array linking regions across files. V2 dropped it entirely.

**Why:** Hard to populate meaningfully, either by LLM or by humans. Decisions with scope arrays and effort linking accomplish the same thing more naturally — you declare that a decision applies to specific files/modules rather than trying to enumerate cross-cutting groups.

### What's still unfulfilled

**Infrastructure:**
- `.chronicle-config.toml` shared team config (using git config only)
- CI/GitHub Actions workflow for server-side squash annotation
- MCP server exposing query tools to external agents
- Editor/LSP integration (inline hints, hover)
- `chronicle skill install` as separate lifecycle (skills install via `setup`)

**Query capabilities:**
- Multi-file queries (across files in one call)
- `--since`/`--tags` filters on read
- Backfill `--since`/`--path`/`--resume`/`--concurrency`
- `annotate-range` for batch annotation by date range

**Providers:**
- OpenAI, Gemini, OpenRouter
- `chronicle auth check` standalone command

### Surprising additions not in any doc

Several shipped features emerged from real usage, not from the design docs:

- **Knowledge store** — repo-level conventions, module boundaries, anti-patterns stored on a separate notes ref. Addresses "what are the rules here?" which is distinct from per-commit reasoning.
- **Staged notes** — `git chronicle note "..."` accumulates context incrementally during a work session, consumed by the next annotation. Came from observing that agents know useful things mid-session that are lost by commit time.
- **`show` TUI** — interactive terminal explorer for annotated source code. Not in any doc.
- **`status` command** — repo-wide annotation coverage metrics.
- **`schema` command** — self-documenting CLI where agents can query input/output formats at runtime. Emerged from the realization that agents need to know *how* to call Chronicle, not just *that* they should.
- **`lookup` command** — composite query (contracts + decisions + history + staleness + knowledge) for one-stop orientation. The docs describe individual queries; practice showed agents want everything at once.
- **9 marker kinds** — Contract, Hazard, Dependency, Unstable, Security, Performance, Deprecated, TechDebt, TestCoverage. The docs describe a simpler constraint/dependency model. Real usage demanded more granularity.
- **EffortLink** — connecting annotations to broader efforts/phases. Not in the original design.
- **Staleness detection** — binary "is this stale?" with doctor integration. Replaced the theoretical confidence scoring.

### What we learned was wrong

1. **Tree-sitter was the biggest wrong bet.** Enormous complexity for marginal value when agents already know their anchors.
2. **Per-region schema was wrong.** Commits are about intent, not about what each function does independently.
3. **Confidence scoring was premature optimization.** Binary staleness is more actionable than uncalibrated floats.
4. **Cross-cutting concerns don't work as declared structures.** Decisions with scope arrays are more natural.
5. **Context levels (enhanced vs inferred) don't matter much.** The live path dominates; the distinction adds schema noise.
6. **Wrapping `git commit` was the wrong integration point.** Layer alongside workflow, don't replace it.
7. **Multi-provider was speculative.** Build for your actual users, not hypothetical ones.
8. **Token budget trimming was premature.** Annotations are small. Solve real problems.

### Summary

The docs described a **theoretically complete system** optimized for elegance. What got built is a **practically useful system** optimized for the live annotation path. The biggest wins came from simplification (removing tree-sitter, narrative-first schema) and from features the docs never imagined (knowledge store, staged notes, self-documenting schema). The biggest unfulfilled gaps are around team/CI workflows and multi-tool integration (MCP, LSP).
