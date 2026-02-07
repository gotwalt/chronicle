<!-- chronicle-setup-begin -->
## Working with Chronicle annotations

This project uses Chronicle (`git-chronicle`) to store structured metadata
alongside commits as git notes. Before modifying existing code, query the
annotations to understand intent, constraints, and dependencies.

### Reading annotations (before modifying code)

```bash
# Check contracts — "What must I not break?"
git chronicle contracts src/foo.rs --anchor bar_function

# Check decisions — "What was decided and why?"
git chronicle decisions --path src/foo.rs

# Read raw annotations for a file/anchor
git chronicle read src/foo.rs --anchor bar_function

# Quick orientation for a file
git chronicle summary src/foo.rs

# What depends on this code?
git chronicle deps src/foo.rs bar_function
```

**Respect contracts.** Annotations may include contracts like "must not
block the async runtime" or "assumes sorted input." Violating these without
updating the annotation is a bug. See the context skill for details.

### Writing annotations (after committing)

After committing, annotate using the live path (v2 format). Use a temp file
with a quoted heredoc to avoid shell escaping issues:

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{
  "commit": "HEAD",
  "summary": "What this commit does and WHY this approach."
}
EOF
git chronicle annotate --live < /tmp/chronicle-annotate.json
```

See the annotate skill for the full annotation workflow.

### Backfilling annotations

To annotate historical commits that lack annotations, see
the backfill skill.
<!-- chronicle-setup-end -->
