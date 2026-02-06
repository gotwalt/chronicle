# Chronicle Context Skill

## When to Use

Before modifying existing code, read its Chronicle annotations to understand
the original intent, constraints, and dependencies. This prevents accidentally
breaking behavioral contracts or violating design constraints.

**Always read annotations when:**
- You are about to modify a function, struct, or module you didn't write in
  this session
- You are debugging unexpected behavior in existing code
- You need to understand why code was written a particular way
- You are refactoring and need to preserve behavioral contracts

**You can skip when:**
- You are creating entirely new files with no existing annotations
- You are making trivial changes (typos, formatting)
- You already read the annotations earlier in this session and nothing has
  changed

## How to Read Annotations

### 1. Read a specific region

Call the `chronicle_read` MCP tool:

```json
{
  "path": "src/provider/anthropic.rs",
  "anchor": "AnthropicProvider::complete"
}
```

This returns:
- **intent**: What the code is supposed to accomplish
- **reasoning**: Why this approach was chosen
- **constraints**: Rules that must not be violated
- **semantic_dependencies**: Other code that depends on this region's behavior
- **risk_notes**: Known fragility or edge cases

### 2. Check dependencies before changing behavior

Call the `chronicle_deps` MCP tool:

```json
{
  "path": "src/provider/anthropic.rs",
  "anchor": "AnthropicProvider::complete"
}
```

This returns a list of code regions that make behavioral assumptions about the
target. If you change the target's behavior, you may need to update the
dependents too.

### 3. Get a file overview

Call the `chronicle_summary` MCP tool:

```json
{
  "path": "src/provider/anthropic.rs"
}
```

Returns a condensed view of intent and constraints for all annotated regions in
the file. Good for orientation.

### 4. Check change history

Call the `chronicle_history` MCP tool:

```json
{
  "path": "src/provider/anthropic.rs",
  "anchor": "AnthropicProvider::complete",
  "limit": 5
}
```

Shows what changed and why over time. Useful for understanding evolution and
debugging regressions.

## CLI Fallback

If MCP tools are not available:

```bash
git chronicle read src/provider/anthropic.rs --anchor "AnthropicProvider::complete"
git chronicle deps src/provider/anthropic.rs "AnthropicProvider::complete"
git chronicle summary src/provider/anthropic.rs
git chronicle history src/provider/anthropic.rs --anchor "AnthropicProvider::complete"
```

## Working with Constraints

Constraints in annotations are **binding design rules**. Examples:

- "Must not block the async runtime" -- don't add blocking I/O
- "Assumes caller holds the write lock" -- don't call without locking
- "Return value must be sorted" -- preserve sort invariant
- "Must be idempotent" -- no side effects on repeated calls

If you need to violate a constraint:
1. Note it explicitly in your commit annotation's reasoning field
2. Remove or update the old constraint via `git chronicle correct`
3. Add the new constraint to your annotation

Never silently violate a constraint -- future agents and developers rely on them.

## Integration with Annotating

If you read annotations before modifying code, reference what changed in your
post-commit annotation:

```json
{
  "reasoning": "Previous annotation noted 'assumes sorted input'. Changed to accept unsorted input and sort internally, because callers can no longer guarantee ordering after the new batch API was added."
}
```

This creates a clear trail of how design decisions evolved.
