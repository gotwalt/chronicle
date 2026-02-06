# Feature 09: History Rewrite Handling

## Preserving Annotations Through Squash Merges, Amends, and Merge Commits

---

## 1. Overview

Git history rewrites — squash merges, amends, and merge commits — are the single largest source of annotation loss. Every rewrite produces new commit SHAs, orphaning git notes attached to the old ones. Without active intervention, weeks of captured reasoning vanish when a feature branch is squash-merged to main.

Feature 09 implements four mechanisms:

1. **Squash merge detection and synthesis** — `prepare-commit-msg` hook detects squash operations, records source commits in a tmpfile, and `post-commit` synthesizes a consolidated annotation from the source annotations.
2. **Amend migration** — `post-rewrite` hook receives old-to-new SHA mappings, fetches old annotations, passes them to the agent with the new diff, and writes updated annotations to new SHAs.
3. **Merge commit annotation** — detects merge commits (>1 parent), diffs against both parents, and annotates only the conflict resolutions.
4. **Server-side squash merges** — `ultragit annotate --squash-sources` synthesizes annotations for CI-triggered squash merges that bypass local hooks.

All mechanisms track provenance: every derived annotation records its `operation` (initial/squash/amend), `derived_from` SHAs, and `synthesis_notes`.

---

## 2. Dependencies

| Feature | What it provides |
|---------|-----------------|
| 05 Writing Agent | Agent loop for annotation synthesis and migration |
| 06 Hooks & Context | Hook installation framework, post-commit/prepare-commit-msg/post-rewrite hook infrastructure |
| 02 Git Operations | Notes read/write, diff, ref management, commit metadata |
| 04 LLM Providers | LLM access for synthesis (squash) and re-annotation (amend) |

Feature 09 depends heavily on the writing agent because squash synthesis and amend migration both require LLM calls — the agent must reason about how to consolidate or update annotations.

---

## 3. Public API

### 3.1 CLI Additions

**Squash-source annotation (for CI):**

```
ultragit annotate --commit <SHA> --squash-sources <SHA>[,<SHA>...]
```

Synthesizes an annotation for `--commit` by collecting annotations from the listed source commits and passing them to the agent alongside the commit's diff. Used in CI workflows for server-side squash merges.

**Manual amend re-annotation:**

```
ultragit annotate --commit <SHA> --amend-source <OLD_SHA>
```

Migrates the annotation from `OLD_SHA` to the new commit. Used when the `post-rewrite` hook didn't fire or failed.

### 3.2 Hook Entry Points

```rust
/// Called by prepare-commit-msg hook.
/// Detects squash operations and writes pending-squash.json.
pub fn handle_prepare_commit_msg(
    commit_msg_file: &Path,
    commit_source: Option<&str>,   // "squash", "merge", "message", etc.
    commit_sha: Option<&str>,      // SHA for "commit" source
) -> Result<()>;

/// Called by post-commit hook (extends Feature 06 behavior).
/// Checks for pending-squash.json and routes to synthesis if found.
pub fn handle_post_commit_squash(
    repo: &git::Repository,
    commit_sha: &str,
) -> Result<Option<SquashContext>>;

/// Called by post-rewrite hook.
/// Receives old→new SHA mappings on stdin and migrates annotations.
pub fn handle_post_rewrite(
    repo: &git::Repository,
    rewrite_type: &str, // "amend" or "rebase"
    mappings: &[(String, String)], // (old_sha, new_sha) pairs
) -> Result<()>;
```

### 3.3 Internal Types

```rust
/// Written to .git/ultragit/pending-squash.json by prepare-commit-msg.
/// Consumed and deleted by post-commit.
#[derive(Serialize, Deserialize)]
pub struct PendingSquash {
    pub source_commits: Vec<String>,
    pub source_ref: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Context passed to the writing agent for squash synthesis.
pub struct SquashSynthesisContext {
    /// The squash commit's SHA.
    pub squash_commit: String,
    /// The squash commit's combined diff.
    pub diff: String,
    /// Annotations from source commits (those that had annotations).
    pub source_annotations: Vec<schema::Annotation>,
    /// Commit messages from source commits.
    pub source_messages: Vec<(String, String)>, // (sha, message)
    /// The squash commit's own commit message.
    pub squash_message: String,
}

/// Context passed to the writing agent for amend migration.
pub struct AmendMigrationContext {
    /// The new (post-amend) commit SHA.
    pub new_commit: String,
    /// The new commit's diff (against its parent).
    pub new_diff: String,
    /// The old (pre-amend) annotation.
    pub old_annotation: schema::Annotation,
    /// The new commit message.
    pub new_message: String,
}

/// Provenance metadata attached to derived annotations.
#[derive(Serialize, Deserialize, Clone)]
pub struct Provenance {
    /// "initial", "squash", or "amend"
    pub operation: String,
    /// Original commit SHAs this annotation derives from.
    pub derived_from: Vec<String>,
    /// Whether original annotations were fully preserved.
    pub original_annotations_preserved: bool,
    /// Agent's notes on how source annotations were combined.
    pub synthesis_notes: Option<String>,
}
```

---

## 4. Internal Design

### 4.1 Squash Merge Detection

Squash detection happens in the `prepare-commit-msg` hook, which fires before the commit message editor opens. Git passes arguments indicating the commit source.

**Detection signals (any one sufficient):**

1. **Hook argument.** The second argument to `prepare-commit-msg` is `squash` when doing `git merge --squash`.

2. **`.git/SQUASH_MSG` file.** Present during `git merge --squash`. Contains concatenated commit messages from the squashed branch. Parse this to extract source commit information.

3. **`ULTRAGIT_SQUASH_SOURCES` env var.** Explicitly set by an agent or CI script. Contains a comma-separated list of SHAs or a range like `main..feature-branch`.

```rust
pub fn detect_squash(
    commit_source: Option<&str>,
    repo_path: &Path,
) -> Result<Option<Vec<String>>> {
    // Check 1: hook argument
    if commit_source == Some("squash") {
        return resolve_squash_sources_from_squash_msg(repo_path);
    }

    // Check 2: SQUASH_MSG file existence
    let squash_msg = repo_path.join(".git/SQUASH_MSG");
    if squash_msg.exists() {
        return resolve_squash_sources_from_squash_msg(repo_path);
    }

    // Check 3: environment variable
    if let Ok(sources) = std::env::var("ULTRAGIT_SQUASH_SOURCES") {
        return resolve_squash_sources_from_env(&sources, repo_path);
    }

    Ok(None)
}
```

**Resolving source commits from `SQUASH_MSG`:**

During `git merge --squash`, the SQUASH_MSG contains lines like:
```
Squash commit -- not updating HEAD
commit abc1234
Author: ...
Date: ...
    First commit message
commit def5678
...
```

Parse this to extract the commit SHAs. Alternatively, if the merge base is known, use `git log --format=%H merge_base..MERGE_HEAD` to enumerate source commits.

**Resolving source commits from env var:**

The env var can be:
- A comma-separated list: `abc123,def456,ghi789`
- A range: `main..feature-branch`

For ranges, resolve via `git rev-list <range>`.

### 4.2 Pending-Squash Tmpfile Lifecycle

The handshake between `prepare-commit-msg` and `post-commit` uses a tmpfile at `.git/ultragit/pending-squash.json`.

```
Timeline:

1. Developer runs `git merge --squash feature && git commit`
   (or agent runs `ultragit commit --squash-sources ...`)

2. prepare-commit-msg fires:
   - Detects squash via hook arg or SQUASH_MSG.
   - Resolves source commit SHAs.
   - Writes .git/ultragit/pending-squash.json:
     {
       "source_commits": ["abc123", "def456", "ghi789"],
       "source_ref": "feature-branch",
       "timestamp": "2025-12-15T10:30:00Z"
     }

3. Commit proceeds normally. Commit message editor opens (if interactive).

4. post-commit fires:
   - Checks for .git/ultragit/pending-squash.json.
   - If found: reads it, enters squash synthesis path.
   - If not found: normal annotation path.

5. After synthesis (or on error):
   - Deletes .git/ultragit/pending-squash.json.
```

**Stale tmpfile handling.** The tmpfile includes a `timestamp`. If the `post-commit` hook finds a pending-squash.json older than 60 seconds, it's stale — likely from a `prepare-commit-msg` that ran but whose commit was aborted (e.g., user exited the editor without saving). Stale tmpfiles are deleted with a warning logged.

```rust
const PENDING_SQUASH_EXPIRY_SECS: i64 = 60;

pub fn read_pending_squash(repo_path: &Path) -> Result<Option<PendingSquash>> {
    let path = repo_path.join(".git/ultragit/pending-squash.json");
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let pending: PendingSquash = serde_json::from_str(&content)?;

    let age = chrono::Utc::now() - pending.timestamp;
    if age.num_seconds() > PENDING_SQUASH_EXPIRY_SECS {
        tracing::warn!(
            "Stale pending-squash.json ({}s old), deleting",
            age.num_seconds()
        );
        std::fs::remove_file(&path)?;
        return Ok(None);
    }

    Ok(Some(pending))
}
```

**Cleanup hooks.** As additional safety, check for and delete stale pending-squash.json in:
- `post-merge` hook (fires after merge completes).
- `post-checkout` hook (fires after branch switch).

These are opportunistic cleanups — the primary deletion is in `post-commit`.

### 4.3 Squash Annotation Synthesis

When `post-commit` finds a valid pending-squash.json, it enters the synthesis path:

```
1. Read pending-squash.json → source commit SHAs.

2. For each source SHA, attempt to fetch the Ultragit annotation
   from refs/notes/ultragit.
   → Vec<(sha, Option<Annotation>)>

   Some source commits may not have annotations (unannotated commits,
   trivial changes that were skipped). That's fine.

3. Collect commit messages from source commits.
   → Vec<(sha, message)>

4. Compute the squash commit's diff: git diff HEAD~1..HEAD.

5. Assemble SquashSynthesisContext.

6. Pass to the writing agent with a synthesis-specific system prompt.

7. Agent produces a synthesized annotation that:
   a. Preserves ALL constraints from source annotations.
   b. Preserves ALL semantic_dependencies from source annotations.
   c. Preserves ALL cross_cutting concerns from source annotations.
   d. Consolidates reasoning per region (merging multiple commits'
      reasoning about the same function into one coherent narrative).
   e. Preserves related_annotations references, remapping to
      still-reachable commits where possible.
   f. Sets provenance:
      {
        "operation": "squash",
        "derived_from": ["abc123", "def456", "ghi789"],
        "original_annotations_preserved": true,
        "synthesis_notes": "Synthesized from 3 commits on feature-branch.
          Commits abc123 and def456 both modified connect() — reasoning
          consolidated into single region."
      }

8. Write the synthesized annotation to refs/notes/ultragit for the
   squash commit SHA.

9. Delete pending-squash.json.
```

**Agent prompt for synthesis:**

The writing agent receives a modified system prompt for synthesis operations:

> You are synthesizing a consolidated annotation from multiple source annotations. A squash merge has combined N commits into one. The source annotations capture per-commit reasoning that must be preserved in the consolidated annotation.
>
> Your priorities:
> 1. NEVER drop constraints. Every constraint from every source annotation must appear in the output.
> 2. NEVER drop semantic_dependencies. Every dependency must be preserved.
> 3. NEVER drop cross_cutting concerns.
> 4. Consolidate reasoning: if multiple source annotations describe reasoning about the same code region, merge them into one coherent narrative. Preserve the key decisions and rejected alternatives from each.
> 5. Set provenance.operation to "squash" and list all source commit SHAs in derived_from.
> 6. In synthesis_notes, briefly describe how the source annotations were combined.

**Agent tools for synthesis:**

In addition to the standard writing agent tools, the synthesis agent gets:

`get_source_annotations()` — Returns all source annotations as a structured list.

`get_source_messages()` — Returns commit messages from source commits.

The agent has the squash commit's diff available via `get_diff()` (standard tool).

### 4.4 Amend Migration via post-rewrite Hook

The `post-rewrite` hook fires after `git commit --amend` (and after interactive rebase, though we handle only amend in v1). Git passes the rewrite type as the first argument and old-new SHA mappings on stdin:

```
old_sha1 new_sha1
old_sha2 new_sha2
...
```

**Algorithm:**

```
1. Parse stdin into Vec<(old_sha, new_sha)>.

2. Filter to rewrite_type == "amend".
   (Rebase handling is out of scope for v1.)

3. For each (old_sha, new_sha):
   a. Fetch the annotation from refs/notes/ultragit for old_sha.
   b. If no annotation exists: skip. The old commit was unannotated.
   c. Compute the new commit's diff: git diff new_sha~1..new_sha.
   d. Assemble AmendMigrationContext.
   e. Pass to the writing agent with an amend-specific system prompt.
   f. Agent produces an updated annotation that:
      - Preserves still-relevant reasoning from the old annotation.
      - Updates or adds reasoning for regions that changed in the amend.
      - Removes reasoning for regions that were reverted in the amend.
      - Sets provenance:
        {
          "operation": "amend",
          "derived_from": ["old_sha"],
          "original_annotations_preserved": true,
          "synthesis_notes": "Migrated from amend. Updated reasoning for
            connect() to reflect added timeout parameter."
        }
   g. Write the updated annotation to refs/notes/ultragit for new_sha.
   h. Optionally: remove the orphaned note from old_sha.
      (The old commit is unreachable; the note will be garbage-collected
      with the commit. Removing it is tidy but not required.)
```

**Agent prompt for amend migration:**

> You are migrating an annotation from a pre-amend commit to the post-amend commit. The original annotation captured reasoning about the code before the amend. The amend may have added, removed, or modified code.
>
> Your priorities:
> 1. Preserve all reasoning from the original annotation that is still relevant to the post-amend code.
> 2. Update reasoning for regions that changed in the amend.
> 3. Remove reasoning for regions that no longer exist after the amend.
> 4. If the amend only changed the commit message (no code changes), preserve the original annotation unchanged except for updating the commit SHA.
> 5. Set provenance.operation to "amend".

**Handling message-only amends:** If `git diff old_sha..new_sha` is empty (the amend only changed the commit message), the agent can be skipped entirely. Copy the old annotation verbatim, update the commit SHA and provenance.

```rust
pub fn handle_amend(
    repo: &git::Repository,
    old_sha: &str,
    new_sha: &str,
    agent: &WritingAgent,
) -> Result<()> {
    let old_annotation = match storage::fetch_note(repo, old_sha)? {
        Some(ann) => ann,
        None => return Ok(()), // No annotation to migrate
    };

    let diff = git::diff(repo, &format!("{}~1..{}", new_sha, new_sha))?;

    if diff.is_empty() {
        // Message-only amend: copy annotation with updated provenance
        let mut new_annotation = old_annotation.clone();
        new_annotation.commit = new_sha.to_string();
        new_annotation.provenance = Provenance {
            operation: "amend".to_string(),
            derived_from: vec![old_sha.to_string()],
            original_annotations_preserved: true,
            synthesis_notes: Some("Message-only amend; annotation unchanged.".to_string()),
        };
        storage::write_note(repo, new_sha, &new_annotation)?;
        return Ok(());
    }

    // Code changed: invoke agent for migration
    let context = AmendMigrationContext {
        new_commit: new_sha.to_string(),
        new_diff: diff,
        old_annotation,
        new_message: git::commit_message(repo, new_sha)?,
    };

    let new_annotation = agent.migrate_amend(context)?;
    storage::write_note(repo, new_sha, &new_annotation)?;

    Ok(())
}
```

### 4.5 Merge Commit Annotation

Merge commits are annotated **only for conflict resolutions**. Non-conflicting portions are already annotated on their source branches.

**Detection:** A commit with more than one parent is a merge commit.

```rust
pub fn is_merge_commit(repo: &git::Repository, sha: &str) -> Result<bool> {
    let parents = git::parent_shas(repo, sha)?;
    Ok(parents.len() > 1)
}
```

**Algorithm:**

```
1. Detect merge commit (>1 parent).

2. For each parent, compute the diff: git diff parent..HEAD.
   → diff_from_parent1, diff_from_parent2

3. Identify conflict resolution regions: lines that differ from
   BOTH parents. These are the regions where the merge author made
   a choice about how to reconcile competing changes.

   Technically: for each file, find hunks where the merge result
   differs from parent1 AND differs from parent2 at the same lines.

4. If no conflict resolutions exist (fast-forward or clean merge):
   skip annotation entirely. The merge added no new reasoning.

5. If conflict resolutions exist:
   Pass only those regions to the writing agent, with context about
   what each parent contributed. The annotation focuses on WHY the
   conflict was resolved this way.

6. Provenance:
   {
     "operation": "initial",
     "derived_from": [],
     "synthesis_notes": "Merge commit; annotated conflict resolutions only."
   }
```

**Identifying conflict regions:**

```rust
pub struct ConflictRegion {
    pub file: PathBuf,
    pub lines: LineRange,
    pub parent1_content: String,
    pub parent2_content: String,
    pub merge_content: String,
}

pub fn find_conflict_resolutions(
    repo: &git::Repository,
    merge_sha: &str,
    parent_shas: &[String],
) -> Result<Vec<ConflictRegion>> {
    let diff1 = git::diff_trees(repo, &parent_shas[0], merge_sha)?;
    let diff2 = git::diff_trees(repo, &parent_shas[1], merge_sha)?;

    // Find files modified relative to both parents
    let files1: HashSet<_> = diff1.files().collect();
    let files2: HashSet<_> = diff2.files().collect();
    let conflict_files = files1.intersection(&files2);

    let mut regions = Vec::new();
    for file in conflict_files {
        // Find line ranges that differ from both parents
        let hunks1 = diff1.hunks_for_file(file);
        let hunks2 = diff2.hunks_for_file(file);
        let overlaps = find_overlapping_hunks(&hunks1, &hunks2);
        for overlap in overlaps {
            regions.push(ConflictRegion {
                file: file.clone(),
                lines: overlap.lines,
                parent1_content: overlap.from_parent1,
                parent2_content: overlap.from_parent2,
                merge_content: overlap.merged,
            });
        }
    }

    Ok(regions)
}
```

### 4.6 Server-Side Squash Merges (CI)

GitHub's "Squash and merge" and GitLab's equivalent bypass local hooks entirely. The `--squash-sources` flag on `ultragit annotate` enables CI-based synthesis.

**Usage:**

```bash
ultragit annotate --commit HEAD --squash-sources abc123,def456,ghi789
```

Or with a branch reference:

```bash
ultragit annotate --commit HEAD --squash-sources $(git log --format=%H origin/main..HEAD~1)
```

**Implementation:** This follows the same synthesis path as local squash detection, except the source commits are provided explicitly rather than discovered via `prepare-commit-msg`.

```rust
pub fn annotate_with_squash_sources(
    repo: &git::Repository,
    commit_sha: &str,
    source_shas: &[String],
    agent: &WritingAgent,
) -> Result<()> {
    // Same as post-commit squash path:
    // 1. Fetch annotations from source commits
    // 2. Collect source commit messages
    // 3. Get squash commit diff
    // 4. Assemble SquashSynthesisContext
    // 5. Run synthesis agent
    // 6. Write annotation

    let source_annotations = source_shas.iter()
        .filter_map(|sha| {
            storage::fetch_note(repo, sha).ok().flatten()
                .map(|ann| (sha.clone(), ann))
        })
        .collect::<Vec<_>>();

    let source_messages = source_shas.iter()
        .filter_map(|sha| {
            git::commit_message(repo, sha).ok()
                .map(|msg| (sha.clone(), msg))
        })
        .collect::<Vec<_>>();

    let diff = git::diff(repo, &format!("{}~1..{}", commit_sha, commit_sha))?;
    let squash_message = git::commit_message(repo, commit_sha)?;

    let context = SquashSynthesisContext {
        squash_commit: commit_sha.to_string(),
        diff,
        source_annotations: source_annotations.into_iter().map(|(_, a)| a).collect(),
        source_messages,
        squash_message,
    };

    let annotation = agent.synthesize_squash(context)?;
    storage::write_note(repo, commit_sha, &annotation)?;

    Ok(())
}
```

**GitHub Actions workflow:**

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
          ref: ${{ github.event.pull_request.base.ref }}
      - name: Fetch notes
        run: git fetch origin refs/notes/ultragit:refs/notes/ultragit || true
      - name: Install Ultragit
        run: cargo install ultragit
      - name: Fetch feature branch
        run: git fetch origin ${{ github.event.pull_request.head.sha }}
      - name: Annotate squash merge
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          # Get source commits from the feature branch
          MERGE_BASE=$(git merge-base HEAD ${{ github.event.pull_request.head.sha }})
          SOURCE_SHAS=$(git log --format=%H $MERGE_BASE..${{ github.event.pull_request.head.sha }})
          ultragit annotate --commit HEAD \
            --squash-sources $(echo $SOURCE_SHAS | tr '\n' ',')
      - name: Push annotations
        run: git push origin refs/notes/ultragit
```

### 4.7 Provenance Tracking

Every annotation includes a `provenance` field. The writing agent (Feature 05) sets `"operation": "initial"` for normal commits. Feature 09 sets the operation field based on the rewrite type.

| Operation | Trigger | derived_from | synthesis_notes |
|-----------|---------|-------------|-----------------|
| `initial` | Normal post-commit | `[]` | `null` |
| `squash` | Squash merge (local or CI) | Source commit SHAs | How annotations were consolidated |
| `amend` | `git commit --amend` | Pre-amend SHA | What changed in the migration |

The `derived_from` field preserves the original SHAs even after those commits become unreachable (garbage-collected). This provides traceability — a reader of the annotation can see that this squash annotation was synthesized from 5 feature branch commits, even though those commits no longer exist.

The `original_annotations_preserved` field indicates whether the synthesis agent was able to fully incorporate all source annotations. If some source commits had no annotations, this is `false` with a note in `synthesis_notes`.

---

## 5. Error Handling

| Failure Mode | Behavior |
|--------------|----------|
| `prepare-commit-msg` fails to detect squash | Commit proceeds normally. Post-commit runs normal annotation (not synthesis). Reasoning from source commits is lost, but the squash commit gets its own annotation. |
| `pending-squash.json` write fails (permissions) | Log error, commit proceeds. Fall back to normal annotation. |
| `pending-squash.json` is stale (>60s old) | Delete with warning. Run normal annotation. |
| Source commit has no annotation | Skip it in synthesis. Note in `synthesis_notes`: "N of M source commits had annotations." Set `original_annotations_preserved: false`. |
| All source commits lack annotations | Run normal annotation instead of synthesis. No source context to synthesize from. |
| LLM API fails during synthesis | Log to `.git/ultragit/failed.log` with the squash commit SHA and source SHAs. The commit proceeds. Can be retried: `ultragit annotate --commit <sha> --squash-sources <shas>`. |
| `post-rewrite` receives rebase mappings | Log info: "Rebase annotation migration not yet supported." Skip. |
| `post-rewrite` fails to read old annotation | Skip migration for that SHA. Log warning. |
| Merge commit with no conflict regions | Skip annotation entirely. Log: "Clean merge, no annotation needed." |
| `--squash-sources` with invalid SHAs | Error with specific invalid SHA. |
| `pending-squash.json` with invalid JSON | Delete with warning. Fall back to normal annotation. |

**Principle:** Feature 09 must never block the git workflow. All hooks exit silently on failure. All failures are logged for retry.

---

## 6. Configuration

```ini
[ultragit]
    # Enable squash synthesis (default: true)
    squashSynthesis = true

    # Enable amend migration (default: true)
    amendMigration = true

    # Enable merge commit annotation (default: true)
    mergeAnnotation = true

    # Tmpfile expiry in seconds (default: 60)
    pendingSquashExpiry = 60

    # Remove orphaned notes after amend (default: false)
    cleanOrphanedNotes = false
```

```toml
# .ultragit-config.toml
[ultragit.rewrites]
squash_synthesis = true
amend_migration = true
merge_annotation = true
pending_squash_expiry = 60
clean_orphaned_notes = false
```

---

## 7. Implementation Steps

### Step 1: PendingSquash Types and Tmpfile I/O
**Scope:** Define `PendingSquash` struct in `src/annotate/squash.rs`. Implement `write_pending_squash()`, `read_pending_squash()`, and `delete_pending_squash()`. Stale file detection with configurable expiry. Tests: write/read roundtrip, stale detection, missing file, invalid JSON.

### Step 2: Squash Detection in prepare-commit-msg
**Scope:** Implement `handle_prepare_commit_msg()` in `src/hooks/prepare_commit_msg.rs`. Detect squash via hook argument, `.git/SQUASH_MSG`, and `ULTRAGIT_SQUASH_SOURCES` env var. Resolve source commit SHAs. Write pending-squash.json. Tests: detection via each signal, source SHA resolution from SQUASH_MSG, resolution from env var (comma-separated and range).

### Step 3: Post-Commit Squash Routing
**Scope:** Modify the post-commit handler (Feature 06) to check for pending-squash.json. If found, route to squash synthesis instead of normal annotation. If stale or invalid, fall back to normal annotation. Tests: routing with valid pending file, routing with stale file, routing with no pending file.

### Step 4: Squash Synthesis Agent Flow
**Scope:** Implement `synthesize_squash()` in the writing agent. Synthesis-specific system prompt. Source annotation collection. Agent tools: `get_source_annotations()`, `get_source_messages()`. Provenance setting. Tests: synthesis with 3 source annotations, synthesis with partial annotations (some sources missing), constraint preservation verification.

### Step 5: Amend Detection in post-rewrite
**Scope:** Implement `handle_post_rewrite()` in `src/hooks/post_rewrite.rs`. Parse stdin for old→new SHA mappings. Filter to `amend` type. Tests: stdin parsing with single mapping, multiple mappings, rebase type (skipped).

### Step 6: Amend Migration Agent Flow
**Scope:** Implement `migrate_amend()` in the writing agent. Fetch old annotation, compute new diff, invoke agent with amend-specific prompt, write new annotation. Handle message-only amends (copy without agent call). Tests: code-change amend, message-only amend, old annotation missing (skip).

### Step 7: Merge Commit Detection and Conflict Resolution
**Scope:** Implement `find_conflict_resolutions()` in `src/annotate/squash.rs`. Detect merge commits. Diff against both parents. Identify overlapping hunks. Pass conflict regions to writing agent. Tests: clean merge (no annotation), merge with conflicts, multiple conflict files.

### Step 8: Server-Side Squash (--squash-sources)
**Scope:** Add `--squash-sources` flag to `ultragit annotate` in `src/cli/annotate.rs`. Implement `annotate_with_squash_sources()`. Same synthesis path as local squash. Tests: end-to-end with explicit source SHAs, missing source annotations.

### Step 9: Provenance in Output
**Scope:** Ensure all annotation paths set the `provenance` field correctly. Verify the read pipeline (Feature 07) surfaces provenance in confidence scoring and output. Tests: provenance from initial, squash, amend annotations all correctly set and visible in read output.

### Step 10: Cleanup Hooks
**Scope:** Add stale pending-squash.json cleanup to `post-merge` and `post-checkout` hooks. Opportunistic cleanup, not critical path. Tests: stale file cleaned up on branch switch.

---

## 8. Test Plan

### Unit Tests

**Squash detection:**
- Hook argument `"squash"` → detected.
- Hook argument `"message"` → not detected.
- `.git/SQUASH_MSG` exists → detected; source SHAs parsed from content.
- `ULTRAGIT_SQUASH_SOURCES` env var with comma-separated SHAs → detected.
- `ULTRAGIT_SQUASH_SOURCES` with range `main..feature` → resolved to individual SHAs.
- No signals → not detected.

**Pending-squash tmpfile:**
- Write and read roundtrip: data matches.
- Read missing file: returns `None`.
- Read stale file (>60s): returns `None`, file deleted.
- Read invalid JSON: returns `None`, file deleted, warning logged.
- Expiry configurable: 30s expiry triggers on 31s-old file.

**Squash synthesis:**
- 3 source annotations with distinct regions: all regions present in output.
- 2 source annotations modifying the same region: reasoning consolidated.
- Constraints from all sources preserved in output (never dropped).
- Semantic dependencies from all sources preserved.
- Cross-cutting concerns from all sources preserved.
- Source with no annotation: skipped; `original_annotations_preserved: false`.
- All sources without annotations: falls back to normal annotation.
- Provenance set correctly: operation="squash", derived_from=[source SHAs].

**Amend migration:**
- Code-change amend: agent invoked, annotation updated.
- Message-only amend: no agent call, annotation copied with updated provenance.
- Old annotation missing: silently skipped.
- Provenance set correctly: operation="amend", derived_from=[old_sha].

**post-rewrite parsing:**
- Single `old new` mapping parsed correctly.
- Multiple mappings parsed correctly.
- Rewrite type "amend": processed.
- Rewrite type "rebase": skipped with info log.

**Merge commit:**
- Merge with 2 parents, no conflicts: no annotation produced.
- Merge with 2 parents, conflict in 1 file: annotation covers conflict region only.
- Merge with 2 parents, conflicts in 3 files: all conflict regions annotated.
- Fast-forward merge (1 parent): not treated as merge.

**Server-side squash:**
- `--squash-sources abc,def,ghi`: correctly parsed to 3 SHAs.
- Source annotations fetched and passed to synthesis.
- Missing source annotations handled gracefully.

### Integration Tests

**Local squash merge flow:**
1. Create a repo. Make 3 commits on a feature branch with annotations.
2. Switch to main. Run `git merge --squash feature && git commit`.
3. Verify `prepare-commit-msg` created pending-squash.json.
4. Verify `post-commit` consumed it and wrote a synthesized annotation.
5. Verify the annotation has `provenance.operation: "squash"` and all source SHAs in `derived_from`.
6. Verify constraints from all 3 source annotations are present.

**Amend flow:**
1. Create a repo. Make a commit with annotation.
2. Amend the commit (`git commit --amend`).
3. Verify `post-rewrite` migrated the annotation to the new SHA.
4. Verify `provenance.operation: "amend"` and old SHA in `derived_from`.
5. Verify reading the new commit returns the migrated annotation.

**Merge commit flow:**
1. Create a repo. Make diverging changes on two branches.
2. Merge with conflicts. Resolve conflicts. Commit.
3. Verify annotation covers only the conflict resolution regions.

**CI squash flow:**
1. Create a repo with annotated commits on a feature branch.
2. Simulate CI: run `ultragit annotate --commit HEAD --squash-sources <shas>`.
3. Verify synthesized annotation matches local squash behavior.

**Stale tmpfile cleanup:**
1. Manually create a pending-squash.json with old timestamp.
2. Run a normal commit.
3. Verify the stale file is cleaned up and normal annotation proceeds.

### Edge Case Tests

- Squash with 50 source commits (large synthesis).
- Squash where source branch has been deleted (commits may be unreachable but SHAs are in pending-squash.json — can still fetch if not yet GC'd).
- Amend of an already-amended commit (chain: initial → amend → amend).
- Merge commit where one parent is a squash commit (nested provenance).
- Concurrent pending-squash.json from interrupted commit (stale handling).

---

## 9. Acceptance Criteria

1. `git merge --squash feature && git commit` on a branch with annotated commits produces a synthesized annotation on the squash commit with `provenance.operation: "squash"` and all source SHAs in `derived_from`.

2. All constraints, semantic_dependencies, and cross_cutting concerns from source annotations are preserved in the synthesized annotation.

3. `git commit --amend` migrates the annotation from the old SHA to the new SHA with `provenance.operation: "amend"`.

4. Message-only amends (no code change) copy the annotation without an LLM call.

5. Merge commits are annotated only for conflict resolution regions. Clean merges produce no annotation.

6. `ultragit annotate --commit HEAD --squash-sources <shas>` produces the same synthesized annotation as a local squash merge, enabling CI workflows for server-side squash merges.

7. Stale `pending-squash.json` files (>60s old) are deleted without blocking commits.

8. All hook handlers exit silently on failure. Failed annotations are logged to `.git/ultragit/failed.log` for retry.

9. The `post-rewrite` hook handles `amend` rewrites. `rebase` rewrites are logged and skipped (out of scope for v1).

10. Provenance fields are correctly set for all annotation types and are surfaced in the read pipeline's confidence scoring (provenance factor).

11. The CI workflow example in the spec is a working GitHub Actions configuration that handles server-side squash merges.
