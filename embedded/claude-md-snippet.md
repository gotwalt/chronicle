<!-- chronicle-setup-begin -->
## Working with Chronicle annotations

This project uses Chronicle (`git-chronicle`) to store structured metadata
alongside commits as git notes. Before modifying existing code, query the
annotations to understand intent, constraints, and dependencies.

### Reading annotations (before modifying code)

Use the CLI to check annotations on code you're about to modify:

- `git chronicle read src/foo.rs --anchor bar_function` — get intent,
  reasoning, constraints for a specific function
- `git chronicle deps src/foo.rs bar_function` — find code that depends
  on this function's behavior
- `git chronicle summary src/foo.rs` — overview of all annotated regions

**Respect constraints.** Annotations may include constraints like "must not
allocate" or "assumes sorted input." Violating these without updating the
annotation is a bug.

### Writing annotations (after committing)

After committing, annotate using the live path:

```bash
echo '<AnnotateInput JSON>' | git chronicle annotate --live
```

See the annotate skill (`~/.claude/skills/chronicle/annotate/SKILL.md`) for
the full annotation workflow.

### Backfilling annotations

To annotate historical commits that lack annotations:

```bash
git chronicle backfill --limit 20
```

See the backfill skill (`~/.claude/skills/chronicle/backfill/SKILL.md`) for
the full backfill workflow.
<!-- chronicle-setup-end -->
