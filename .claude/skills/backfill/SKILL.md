# Chronicle Backfill Skill

## When to Use

Use this skill to annotate historical commits that don't have Chronicle
annotations. This is useful when:

- A repository is being onboarded to Chronicle for the first time
- The post-commit hook wasn't installed for some commits
- You want to fill annotation gaps for better `chronicle contracts` and
  `chronicle history` coverage

## How to Backfill

### 1. Find unannotated commits

List recent commits and check which have annotations:

```bash
# List last 20 commits with annotation status
for sha in $(git log --format='%H' -20); do
  if git notes --ref=refs/notes/chronicle show "$sha" 2>/dev/null | head -1 | grep -q chronicle; then
    echo "  annotated: $(git log --format='%h %s' -1 $sha)"
  else
    echo "UNANNOTATED: $(git log --format='%h %s' -1 $sha)"
  fi
done
```

### 2. For each unannotated commit

Read the diff and commit message to reconstruct intent:

```bash
# See what changed
git show --stat <sha>
git log --format='%B' -1 <sha>
git diff <sha>~1..<sha>
```

Then read the file content at that commit for affected files:

```bash
git show <sha>:src/path/to/file.rs
```

### 3. Write the annotation (v2 format)

Write narrative-first annotations. Most backfilled commits need only a summary:

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{
  "commit": "<full-sha>",
  "summary": "Reconstructed: <what the commit does based on diff and message>"
}
EOF
git chronicle annotate --live < /tmp/chronicle-annotate.json
```

For commits with non-obvious behavior, add markers:

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{
  "commit": "<full-sha>",
  "summary": "Reconstructed: adds retry logic with exponential backoff for API calls",
  "motivation": "Inferred from commit message: API was returning intermittent 503 errors",
  "markers": [
    {
      "file": "src/api/client.rs",
      "anchor": {"unit_type": "function", "name": "retry_request"},
      "kind": {"type": "contract", "description": "Inferred: max retry count must not exceed 5 to avoid rate limiting", "source": "inferred"}
    }
  ]
}
EOF
git chronicle annotate --live < /tmp/chronicle-annotate.json
```

Or use the automated batch backfill:

```bash
git chronicle backfill --limit 20
```

## Quality Guidelines

**Be honest about uncertainty.** Backfilled annotations are reconstructed from
diffs and commit messages, not from firsthand knowledge. Use language that
signals this:

- Summary: "Reconstructed from diff: appears to..." or "Based on commit message: ..."
- Motivation: "Likely triggered by..." or "Inferred from the commit message: ..."
- Contracts: use `"source": "inferred"` when deducing constraints from code patterns

**Focus on narrative, not per-function details.** The v2 schema is designed
for commit-level stories. Don't try to annotate every function — focus on the
overall intent and any genuinely non-obvious behavior.

**Work chronologically** (oldest first) so later annotations can reference
decisions established by earlier ones.

**Skip these commits:**
- Merge commits with no manual conflict resolution
- Commits that only change lockfiles (Cargo.lock, package-lock.json, etc.)
- Commits that only change generated or vendored files
- Commits with trivial formatting-only changes

**Batch size:** Process 5-10 commits per session to avoid context overflow.
After each batch, verify with `git chronicle contracts` or `git chronicle
decisions` that the annotations look reasonable.

## Verifying Backfill Quality

After backfilling, spot-check:

```bash
# Check contracts for a key file
git chronicle contracts src/path/to/file.rs

# Check decisions
git chronicle decisions --path src/path/to/file.rs

# Interactive viewer
git chronicle show src/path/to/file.rs --commit <sha>
```

Look for:
- Accurate narratives that match the actual changes
- No fabricated constraints or dependencies
- Useful rejected alternatives when the commit message suggests alternatives were considered
