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

Annotations are context for future agents — write what the diff cannot tell you.
Do NOT restate the commit message. Every annotation is a single Bash command:

```bash
# Default (any non-trivial commit — include rejected_alternatives, decisions, markers as relevant):
git chronicle annotate --live << 'EOF'
{"commit":"HEAD","summary":"WHY this approach","rejected_alternatives":[...],"decisions":[{"what":"...","why":"...","stability":"provisional"}]}
EOF

# Summary-only (trivial changes — typos, renames, dep bumps):
git chronicle annotate --summary "WHY, not what."
```

See the annotate skill for the full JSON field reference, good/bad summary
examples, and guidance on when to use each field.

### Knowledge store (repo-level rules)

Chronicle also maintains a knowledge store for conventions, module
boundaries, and anti-patterns that apply across the repo:

```bash
# Read repo knowledge before working in unfamiliar areas
git chronicle knowledge list

# Record a convention after annotating
git chronicle knowledge add --type convention --scope "src/" --rule "..." --stability permanent
```

See the annotate skill for when to capture knowledge.
<!-- chronicle-setup-end -->
