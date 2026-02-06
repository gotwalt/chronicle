# Chronicle Annotate Skill

## When to Use

After you create a git commit, annotate it to capture your intent, reasoning,
and constraints as structured metadata stored alongside the commit.

**Always annotate when:**
- You created a commit that adds, modifies, or removes code
- The commit has meaningful changes (not just formatting or whitespace)

**Skip annotation when:**
- The commit only changes lockfiles, generated files, or vendored dependencies
- The commit is a merge commit with no manual conflict resolution

## How to Annotate

Write AnnotateInput JSON to a temp file, then pipe it to the CLI. This avoids
shell escaping issues with special characters in your annotation text:

```bash
cat > /tmp/chronicle-annotate.json << 'EOF'
{
  "commit": "HEAD",
  "summary": "One paragraph: what the commit does and why",
  "task": "TASK-123 (if applicable)",
  "regions": [
    {
      "file": "src/path/to/file.rs",
      "anchor": {
        "unit_type": "function",
        "name": "function_name"
      },
      "intent": "What this specific change accomplishes",
      "reasoning": "Why this approach was chosen over alternatives",
      "constraints": [
        { "text": "Must not block the async runtime" },
        { "text": "Assumes caller holds the lock" }
      ],
      "semantic_dependencies": [
        {
          "file": "src/other.rs",
          "anchor": "OtherStruct::method",
          "nature": "Calls this method and assumes it returns sorted results"
        }
      ],
      "tags": ["error-handling", "performance"],
      "risk_notes": "Potential deadlock if called from within a transaction"
    }
  ],
  "cross_cutting": [
    {
      "description": "Error handling pattern: all public functions return Result with context",
      "regions": [
        { "file": "src/foo.rs", "anchor": "foo_function" },
        { "file": "src/bar.rs", "anchor": "bar_function" }
      ],
      "tags": ["error-handling"]
    }
  ]
}
EOF
git chronicle annotate --live < /tmp/chronicle-annotate.json
```

**Important:** Use `<< 'EOF'` (quoted) to prevent shell expansion of special
characters.

**Flexible input:**
- `anchor` is optional -- omit it for file-level annotations (config, YAML, etc.)
- `path` works as an alias for `file`
- `constraints` accepts plain strings (`["Must not allocate"]`) or objects (`[{"text": "Must not allocate"}]`)
- `lines` is optional -- AST anchor resolution will determine the correct line range from the anchor name

## Quality Bar

For each region, provide:

- **intent** (required): At least one sentence explaining what the change
  accomplishes. Not just "update function" -- explain the purpose.
- **reasoning** (recommended): Why you chose this approach. Especially
  important for non-obvious decisions, performance trade-offs, or when you
  considered alternatives.
- **constraints** (recommended): Design invariants, assumptions, or rules that
  future developers should know. Things that would break if violated.
- **semantic_dependencies** (when applicable): Code that depends on assumptions
  about this region, or that this region depends on. Critical for behavioral
  contracts.
- **tags** (optional): Categorization for filtering and search.
- **risk_notes** (when applicable): Known risks, edge cases, or fragility.

## When to Include Semantic Dependencies

Include `semantic_dependencies` whenever:
- Your region calls functions in other files and assumes something about their
  behavior (e.g., "assumes `parse()` returns valid UTF-8")
- Your region implements an interface or trait that other code relies on
- Your region modifies shared state that other code reads
- Your region changes the signature or return type of a public function

Each dependency should state the **nature** of the assumption: what would break
if the dependency changed its behavior.

## Cross-Cutting Concerns

Use `cross_cutting` when a single commit applies the same pattern across
multiple regions. Common examples:
- Error handling: switching all functions to return `Result` with context
- Logging: adding tracing spans to all public methods
- Validation: adding input checks to all API handlers
- Refactoring: renaming a type across multiple files

This avoids duplicating the same reasoning across every region.

## Anchor Types

Use these `unit_type` values:
- `function` -- free functions
- `method` -- methods on a struct/impl
- `struct` -- struct definitions
- `enum` -- enum definitions
- `trait` -- trait definitions
- `impl` -- impl blocks
- `module` -- module declarations

The handler will resolve your anchor against the AST and correct the line
numbers. Provide approximate lines -- the AST will fix them.

## Fallback

If the `--live` path fails, fall back to the batch path:

```bash
git chronicle annotate --commit HEAD
```

This uses the LLM-based agent loop which is slower and costs API tokens, but
works without any structured input.
