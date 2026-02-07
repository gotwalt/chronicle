# Chronicle Annotate Skill

## When to Use

After you create a git commit, annotate it using the live annotation path. This captures the story behind the change: why this approach was chosen, what was considered and rejected, and what is non-obvious about the code.

**Always annotate when:**
- You created a commit that adds, modifies, or removes code
- The commit has meaningful changes (not just formatting or whitespace)

**Skip annotation when:**
- The commit only changes lockfiles, generated files, or vendored dependencies
- The commit is a merge commit with no manual conflict resolution

## How to Annotate (v2 format)

Write annotation JSON to a temp file and pipe it to the CLI. Use `<< 'EOF'` (quoted heredoc) to prevent shell expansion:

### Minimal annotation (most commits need only this):

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{
  "commit": "HEAD",
  "summary": "Switch from fixed-interval to exponential backoff for MQTT reconnect. The broker rate-limits reconnect attempts, so rapid retries cause longer lockout periods."
}
EOF
git chronicle annotate --live < /tmp/chronicle-annotate.json
```

### Rich annotation (when warranted):

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{
  "commit": "HEAD",
  "summary": "Redesign annotation schema from per-function regions to commit-level narrative with optional code markers.",
  "motivation": "Current annotations restate diffs instead of capturing decision context.",
  "rejected_alternatives": [
    {"approach": "Enrich v1 with optional commit-level fields", "reason": "Per-region structure still dominates and creates noise"}
  ],
  "decisions": [
    {"what": "Lazy v1->v2 migration (translate on read)", "why": "Avoids risky bulk rewrite of git notes", "stability": "permanent"}
  ],
  "markers": [
    {
      "file": "src/schema/v2.rs",
      "anchor": {"unit_type": "function", "name": "validate"},
      "kind": {"type": "contract", "description": "Must be called before writing to git notes", "source": "author"}
    }
  ],
  "effort": {"id": "schema-v2", "description": "Chronicle schema v2 redesign", "phase": "start"}
}
EOF
git chronicle annotate --live < /tmp/chronicle-annotate.json
```

## What to Include

### Narrative (required)
- **summary** (required): What this commit does and WHY this approach. Not a diff restatement.
- **motivation** (when useful): What triggered this change?
- **rejected_alternatives** (highest value): What was tried and why it didn't work. This prevents repeating dead ends.
- **follow_up** (when applicable): Expected follow-up work. Omit if this is complete.

### Decisions (optional)
For architectural or design choices made in this commit:
- **what**: What was decided
- **why**: Why this decision was made
- **stability**: `permanent`, `provisional`, or `experimental`
- **revisit_when**: When should this be reconsidered?
- **scope**: Files/modules this applies to

### Code Markers (optional, only where non-obvious)
Do NOT annotate every function. Only emit markers where there is something genuinely non-obvious:

- **contract**: Behavioral invariant or precondition (e.g., "Must not block the async runtime")
- **hazard**: Something that could cause bugs if misunderstood (e.g., "Not thread-safe without external locking")
- **dependency**: Code that assumes something about code elsewhere (e.g., "Assumes Config::load returns defaults on missing file")
- **unstable**: Provisional code that should be revisited (e.g., "Hardcoded timeout, replace when config system is ready")

### Effort Link (optional)
Link commits to a broader effort:
- **id**: Stable identifier (ticket ID, slug)
- **phase**: `start`, `in_progress`, or `complete`

## Self-Documenting Schema

To see the exact JSON Schema for the input format:

```bash
git chronicle schema annotate-input
```

To see the stored annotation format:

```bash
git chronicle schema annotation
```

## Legacy v1 Format

The v1 format (with `regions` and `cross_cutting` arrays) is still accepted for backward compatibility. If your input JSON has a `regions` key, it will be routed to the v1 handler.

## Batch Fallback

If you don't have structured input, the batch path uses an LLM to produce annotations:

```bash
git chronicle annotate --commit HEAD
```

This costs API tokens but works without any structured input.
