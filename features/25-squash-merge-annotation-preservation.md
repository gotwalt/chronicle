# Feature 25: Preserve Chronicle Annotations Across GitHub Squash Merges

**Status**: In Progress

## Motivation

When a PR is squash-merged via GitHub's UI, the server creates a new commit
with a new SHA. Local git hooks never fire. All Chronicle annotations attached
to the feature branch commits become orphaned — the wisdom, dead ends, and
gotchas captured during development are lost.

Chronicle already has squash synthesis infrastructure for **local** squash
merges (`prepare-commit-msg` hook + `--squash-sources` CLI flag), but that
code is stuck on the v1 schema (region-based) while all new annotations use
v3 (wisdom-based). Two problems need solving:

1. **Schema gap**: Squash synthesis produces v1 annotations, but the rest of
   the system now writes v3.
2. **Server-side gap**: GitHub squash merges bypass local hooks entirely, so
   there's no trigger to run synthesis.

---

## Design

### V3-native squash synthesis

V3 synthesis is much simpler than v1 — no region merging needed:

- **Summary**: Use the squash commit message (GitHub auto-generates from the
  PR title + individual commits).
- **Wisdom**: Collect all wisdom entries from all source annotations.
  Deduplicate by exact `(category, content)` match. Preserve `file` and
  `lines` as-is.
- **Provenance**: `source: Squash`, `derived_from: [all source SHAs]`, `notes`
  with synthesis metadata ("Synthesized from N commits, M had annotations").

Source annotations may be v1, v2, or v3. The collection function uses
`schema::parse_annotation()` (the single deserialization chokepoint) to
normalize everything to v3 before merging.

### GitHub Actions workflow

A GitHub Actions workflow triggers on `pull_request` `closed` events where
`merged == true`. It:

1. Detects squash merges (1 parent = squash, 2 = regular merge; regular
   merges are skipped since original commits are preserved).
2. Gets PR source commit SHAs via the GitHub API.
3. Builds chronicle from the same repo.
4. Fetches existing notes, runs `--squash-sources`, pushes notes back.

Only needs `GITHUB_TOKEN` (automatic) — no LLM API key required since squash
synthesis is deterministic.

---

## Key Design Decisions

1. **Wisdom entries merge as-is** — `file` and `lines` already scope them.
   Per-entry source commit attribution would bloat the schema;
   `provenance.derived_from` tracks the source commits.

2. **Summary = squash commit message** — GitHub generates this from the PR
   title + individual commits. Individual source summaries are captured in
   `provenance.notes` for traceability.

3. **Only squash merges** — Regular merge commits preserve original commits
   (annotations stay reachable). Rebase merges need SHA mapping, not
   synthesis — that's a separate concern.

4. **Dedup by exact `(category, content)` match** — Simple and correct.
   Near-duplicate detection would add complexity without clear benefit.

5. **Keep v1 squash functions** — `post_rewrite.rs` still uses them. They
   can be removed when v1 support is fully deprecated.

---

## Implementation

### Files changed

| File | Change |
|------|--------|
| `src/annotate/squash.rs` | Add `SquashSynthesisContextV3`, `synthesize_squash_annotation_v3()`, `collect_source_annotations_v3()` |
| `src/cli/annotate.rs` | Wire `--squash-sources` to v3 synthesis |
| `.github/workflows/squash-annotate.yml` | New workflow for squash-merged PRs |

### New types

```rust
pub struct SquashSynthesisContextV3 {
    pub squash_commit: String,
    pub squash_message: String,
    pub source_annotations: Vec<v3::Annotation>,
    pub source_messages: Vec<(String, String)>,
}
```

### New functions

- `synthesize_squash_annotation_v3(ctx) -> v3::Annotation` — Merges wisdom,
  deduplicates, sets provenance.
- `collect_source_annotations_v3(git_ops, shas) -> Vec<(String, Option<v3::Annotation>)>` —
  Uses `parse_annotation()` for cross-version support.

---

## Dependencies

- **Feature 22 (v3 schema)**: Complete — v3 is the canonical format.
- **Feature 24 (remove batch/backfill)**: Complete — live path is the only
  annotation path.

---

## Acceptance Criteria

1. `--squash-sources` writes v3 annotations with merged wisdom
2. Source annotations in v1/v2/v3 format are all handled correctly
3. Wisdom entries are deduplicated by exact `(category, content)` match
4. Provenance tracks all source SHAs and synthesis metadata
5. GitHub Actions workflow triggers on squash-merged PRs
6. All existing tests continue to pass
7. New unit tests cover synthesis, dedup, partial annotations, and provenance
