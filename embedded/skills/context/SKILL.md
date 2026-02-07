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

### 1. Read annotations for a file/region

```bash
git chronicle read src/provider/anthropic.rs --anchor "AnthropicProvider::complete"
```

Returns the raw annotation data for the given file and optional anchor.

### 2. Check contracts and dependencies ("What must I not break?")

```bash
git chronicle contracts src/provider/anthropic.rs --anchor "AnthropicProvider::complete"
```

Returns:
- **contracts**: Behavioral invariants, preconditions, assumptions
- **dependencies**: Code that assumes things about this location's behavior

This is the most important query before modifying code.

### 3. Check decisions ("What was decided and why?")

```bash
git chronicle decisions --path src/provider/anthropic.rs
```

Returns:
- **decisions**: Architectural/design choices with stability levels
- **rejected_alternatives**: Approaches that were tried and why they failed

Reading rejected alternatives prevents repeating dead ends.

### 4. Get a file overview

```bash
git chronicle summary src/provider/anthropic.rs
```

Returns a condensed view of contracts, hazards, and dependencies for all
annotated regions in the file. Good for orientation.

### 5. Check dependency graph ("What depends on this?")

```bash
git chronicle deps src/provider/anthropic.rs "AnthropicProvider::complete"
```

Returns code regions that make behavioral assumptions about the target.
If you change the target's behavior, you may need to update dependents too.

### 6. Check change history

```bash
git chronicle history src/provider/anthropic.rs --anchor "AnthropicProvider::complete"
```

Shows what changed and why over time. Useful for understanding evolution and
debugging regressions.

## Working with Contracts

Contracts in annotations are **binding design rules**. Examples:

- "Must not block the async runtime" -- don't add blocking I/O
- "Assumes caller holds the write lock" -- don't call without locking
- "Return value must be sorted" -- preserve sort invariant
- "Must be idempotent" -- no side effects on repeated calls

If you need to violate a contract:
1. Note it explicitly in your commit annotation's reasoning
2. Remove or update the old contract via `git chronicle correct`
3. Add the new contract to your annotation

Never silently violate a contract -- future agents and developers rely on them.

## Working with Decisions

Decisions have stability levels:
- **permanent**: Expected to last. Violating requires strong justification.
- **provisional**: Intentionally temporary. Check `revisit_when` for when to reconsider.
- **experimental**: May be reverted. Extra caution when depending on this.

## Integration with Annotating

If you read annotations before modifying code, reference what changed in your
post-commit annotation:

```json
{
  "summary": "Changed sort behavior in parse() to accept unsorted input",
  "rejected_alternatives": [
    {"approach": "Keep sorted-input requirement", "reason": "Callers can no longer guarantee ordering after the new batch API"}
  ]
}
```

This creates a clear trail of how design decisions evolved.
