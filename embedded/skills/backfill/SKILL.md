# Chronicle Backfill Skill

## When to Use

Use this skill to annotate historical commits that don't have Chronicle
annotations. This is useful when:

- A repository is being onboarded to Chronicle for the first time
- The post-commit hook wasn't installed for some commits
- You want to fill annotation gaps for better `chronicle deps` and
  `chronicle history` coverage

## How to Backfill

### Quick: CLI command

```bash
# Annotate up to 20 recent unannotated commits
git chronicle backfill --limit 20

# Preview what would be annotated
git chronicle backfill --limit 20 --dry-run
```

### Manual: per-commit annotation

#### 1. Find unannotated commits

```bash
for sha in $(git log --format='%H' -20); do
  if git notes --ref=refs/notes/chronicle show "$sha" 2>/dev/null | head -1 | grep -q chronicle; then
    echo "  annotated: $(git log --format='%h %s' -1 $sha)"
  else
    echo "UNANNOTATED: $(git log --format='%h %s' -1 $sha)"
  fi
done
```

#### 2. For each unannotated commit

Read the diff and commit message to reconstruct intent:

```bash
git show --stat <sha>
git log --format='%B' -1 <sha>
git diff <sha>~1..<sha>
```

Then read the file content at that commit for affected files:

```bash
git show <sha>:src/path/to/file.rs
```

#### 3. Write the annotation

```bash
echo '{
  "commit": "<full-sha>",
  "summary": "Reconstructed: <what the commit does based on diff and message>",
  "regions": [
    {
      "file": "src/path/to/file.rs",
      "anchor": { "unit_type": "function", "name": "changed_function" },
      "lines": { "start": 10, "end": 25 },
      "intent": "Reconstructed from diff: appears to add error handling for ...",
      "reasoning": "Reconstructed: likely chosen because ...",
      "constraints": [
        { "text": "Inferred: must handle null input based on added guard clause" }
      ],
      "semantic_dependencies": [],
      "tags": ["backfill"]
    }
  ]
}' | git chronicle annotate --live
```

## Quality Guidelines

**Be honest about uncertainty.** Backfilled annotations are reconstructed from
diffs and commit messages, not from firsthand knowledge. Use language that
signals this:

- Intent: "Reconstructed from diff: appears to..." or "Based on commit message: ..."
- Reasoning: "Likely chosen because..." or "Inferred from the diff pattern: ..."
- Constraints: prefix with "Inferred:" when you're deducing constraints from
  code patterns rather than knowing them directly

**Always include the `backfill` tag** so readers know these were reconstructed.

**Work chronologically** (oldest first) so later annotations can reference
dependencies established by earlier ones.

**Skip these commits:**
- Merge commits with no manual conflict resolution
- Commits that only change lockfiles (Cargo.lock, package-lock.json, etc.)
- Commits that only change generated or vendored files
- Commits with trivial formatting-only changes

**Batch size:** Process 5-10 commits per session to avoid context overflow.
After each batch, verify with `git chronicle read` that the annotations look
reasonable.

## Verifying Backfill Quality

After backfilling, spot-check with the TUI:

```bash
git chronicle show src/path/to/file.rs --commit <sha>
```

Or read directly:

```bash
git chronicle read src/path/to/file.rs
```

Look for:
- Accurate line ranges (AST resolution should correct approximate lines)
- Reasonable intent descriptions that match the actual code
- No fabricated constraints or dependencies
