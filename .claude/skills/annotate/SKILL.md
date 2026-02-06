# Ultragit Annotate Skill

## When to Use

After you create a git commit, annotate it using the `ultragit_annotate` MCP tool. This captures your intent, reasoning, and constraints as structured metadata stored alongside the commit.

**Always annotate when:**
- You created a commit that adds, modifies, or removes code
- The commit has meaningful changes (not just formatting or whitespace)

**Skip annotation when:**
- The commit only changes lockfiles, generated files, or vendored dependencies
- The commit is a merge commit with no manual conflict resolution
- The `ultragit_annotate` tool is not available

## How to Annotate

Call the `ultragit_annotate` MCP tool with:

```json
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
      "lines": { "start": 10, "end": 25 },
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
```

## Quality Bar

For each region, provide:

- **intent** (required): At least one sentence explaining what the change accomplishes. Not just "update function" — explain the purpose.
- **reasoning** (recommended): Why you chose this approach. Especially important for non-obvious decisions, performance trade-offs, or when you considered alternatives.
- **constraints** (recommended): Design invariants, assumptions, or rules that future developers should know. Things that would break if violated.
- **semantic_dependencies** (when applicable): Code that depends on assumptions about this region, or that this region depends on. Critical for behavioral contracts.
- **tags** (optional): Categorization for filtering and search.
- **risk_notes** (when applicable): Known risks, edge cases, or fragility.

## Anchor Types

Use these `unit_type` values:
- `function` — free functions
- `method` — methods on a struct/impl
- `struct` — struct definitions
- `enum` — enum definitions
- `trait` — trait definitions
- `impl` — impl blocks
- `module` — module declarations

The handler will resolve your anchor against the AST and correct the line numbers. Provide approximate lines — the AST will fix them.

## Fallback

If the `ultragit_annotate` MCP tool is not available, fall back to the CLI:

```bash
ultragit annotate --commit HEAD
```

This uses the batch path (LLM-based) which is slower and costs API tokens, but works without the MCP server.
