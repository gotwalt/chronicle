# Chronicle Development History

## 2026-02-06: Initial build

### Phase 1: MVP write path (`7c0b854`)

Built the core annotation pipeline from scratch:

- **Schema** (`src/schema/`): `Annotation`, `RegionAnnotation`, `AstAnchor`, `LineRange`, `Constraint`, `SemanticDependency`, `CrossCuttingConcern`, `Provenance` types with serde serialization and structural validation.
- **Git operations** (`src/git/`): `GitOps` trait with `CliOps` implementation (shells out to git). Covers diff, note read/write, file-at-commit, commit info, config, ref resolution.
- **AST parsing** (`src/ast/`): Tree-sitter outline extraction for Rust source files. Anchor resolution with exact, qualified, and fuzzy (Levenshtein) matching.
- **Batch annotation agent** (`src/agent/`, `src/annotate/`): Tool-use conversation loop with the Anthropic API. Tools: get_diff, get_file_content, get_ast_outline, get_commit_info, emit_annotation, emit_cross_cutting. Pre-LLM filtering skips lockfile-only, merge/WIP/fixup, and trivial commits.
- **CLI** (`src/cli/`): `init`, `context`, `annotate` commands via clap 4.
- **Hooks** (`src/hooks/`): Post-commit hook installation and handler.
- **Error handling** (`src/error.rs`): snafu with `#[snafu(module(...))]` pattern.

### Phase 2: MCP annotate handler + live path (`8c2fd35`..`847541a`)

Added the zero-cost annotation path for agent-authored commits:

- **MCP handler** (`src/mcp/annotate_handler.rs`): `handle_annotate()` takes structured `AnnotateInput` from an authoring agent, resolves AST anchors, validates, and writes the annotation as a git note. No LLM call needed.
- **`--live` flag** on `git chronicle annotate`: Reads `AnnotateInput` JSON from stdin, calls the MCP handler, prints `AnnotateResult` JSON to stdout.
- **Integration test** (`tests/annotate_live.rs`): Calls `handle_annotate` against the real repo, verifies exact anchor resolution on 4 regions.
- **Claude Code skill** (`.claude/skills/annotate/SKILL.md`): Instructions for the agent to annotate after committing.
- **Post-tool-use hook** (`.claude/hooks/`): Reminds the agent to annotate after `git commit`.

### Phase 3: Read pipeline + test suite (team of 3 agents)

First parallel development round. Three agents working on shared branches (caused merge conflicts -- learned to use worktrees instead).

- **Read pipeline** (`src/read/`, `1170c01`): `ReadQuery` + `retrieve_regions()` with file path normalization, anchor filtering, and line range overlap. CLI: `git chronicle read <path> [--anchor] [--lines]`.
- **`log_for_file`** added to `GitOps` trait: `git log --follow --format=%H` for file history.
- **Integration test suite** (`tests/`, `5c626bd`): 33 tests across `git_ops_test.rs` (10), `ast_test.rs` (18), `write_path_test.rs` (5).

57 tests passing after merge.

### Phase 4: Features 08-11 (team of 4 agents in worktrees)

Second parallel development round using `git worktree add` for isolation.

#### Feature 08: Advanced queries (`dc47eec`, merged `93be678`)

- **Dependency inversion** (`src/read/deps.rs`): `find_dependents()` scans annotated commits' `semantic_dependencies` to find reverse dependencies. Unqualified anchor matching (e.g., "max_sessions" matches "TlsSessionCache::max_sessions").
- **Timeline reconstruction** (`src/read/history.rs`): `build_timeline()` shows chronological evolution of a file/anchor's annotations. Optionally follows `related_annotations` links.
- **Condensed summary** (`src/read/summary.rs`): `build_summary()` deduplicates by anchor, keeps most recent, strips verbose fields.
- **`list_annotated_commits`** added to `GitOps` trait: Lists all commits with chronicle notes.
- CLI: `git chronicle deps`, `git chronicle history`, `git chronicle summary`.
- 27 new unit tests.

#### Feature 09: History rewrites (`9b33dfc`+`bc4563d`, merged `f2debb8`)

- **Squash synthesis** (`src/annotate/squash.rs`): Merges region annotations from source commits into one annotation for the squash commit. Deduplicates regions by (file, anchor), merges constraints (never drops), expands line ranges.
- **Amend migration**: Clones old annotation, updates SHA/timestamp, detects message-only vs code amendments.
- **`prepare-commit-msg` hook** (`src/hooks/prepare_commit_msg.rs`): Detects squash operations (hook arg, SQUASH_MSG file, env var) and writes `PendingSquash` state for post-commit synthesis.
- **`post-rewrite` hook** (`src/hooks/post_rewrite.rs`): Handles amend/rebase annotation migration.
- `--squash-sources` and `--amend-source` flags on `git chronicle annotate`.
- 24 new unit tests.

#### Feature 10: Team operations (`00b6a80`+`dbd437e`, merged `6a2d223`)

- **Notes sync** (`src/sync/`): `enable`, `status`, `pull` subcommands. Adds push/fetch refspecs for `refs/notes/chronicle` to git config.
- **JSONL export** (`src/export.rs`): Iterates all chronicle notes, serializes as `ExportEntry` (commit_sha, timestamp, annotation).
- **JSONL import** (`src/import.rs`): Validates entries, checks commit existence, respects `--force` and `--dry-run`.
- **Doctor** (`src/doctor.rs`): 5 diagnostic checks (version, notes ref, hooks, credentials, config) with Pass/Warn/Fail status and fix hints.
- CLI: `git chronicle sync`, `git chronicle export`, `git chronicle import`, `git chronicle doctor`.
- 16 new integration tests.

#### Feature 11: Annotation corrections (`8a24f24`, merged as fast-forward)

- **Correction schema** (`src/schema/correction.rs`): `Correction` type with Flag/Remove/Amend variants. Confidence penalty model (15% per correction, 10% floor).
- **`corrections` field** added to `RegionAnnotation` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`.
- **`flag` command** (`src/cli/flag.rs`): Finds most recent annotation for a file/anchor, appends a Flag correction.
- **`correct` command** (`src/cli/correct.rs`): Applies precise Remove/Amend corrections to specific annotation fields.
- `resolve_author()`: Resolves correction author from `CHRONICLE_SESSION` env var or git config.
- 10 new unit tests.

### Phase 5: Rename to chronicle (`TBD`)

Renamed from `ultragit` to `chronicle` as a proper git extension:

- Binary renamed to `git-chronicle` (invoked as `git chronicle <command>`)
- Removed `commit` wrapper command (users run `git commit` directly; post-commit hook handles annotation)
- Schema version: `chronicle/v1`, notes ref: `refs/notes/chronicle`, config keys: `chronicle.*`, env vars: `CHRONICLE_*`
- Migrated all existing annotations from `refs/notes/ultragit` to `refs/notes/chronicle`

### Final state

- **51 source files**, ~10,500 lines of Rust
- **142 tests** across 7 test binaries, all passing
- **14 CLI commands**: init, context, annotate, read, deps, history, summary, flag, correct, sync (enable/status/pull), export, import, doctor
- **13 modules**: cli, git, ast, schema, mcp, read, annotate, hooks, provider, agent, sync, config, plus doctor/export/import
- Annotations stored in `refs/notes/chronicle`, synced via standard git notes infrastructure
