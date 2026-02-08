# Chronicle Context Skill

## Why This Matters

You start every session with no memory of previous work. The code shows
*what* exists but not *why*, what was tried before, or what invisible
constraints you must respect. Chronicle annotations are the institutional
memory of this project — reasoning, dead ends, contracts, and decisions
left by every agent who worked here before you.

Reading them before modifying code gives you:
- **Contracts** you must not violate (invariants, preconditions, threading
  assumptions invisible in the type system)
- **Rejected alternatives** already explored — skip the dead ends
- **Design decisions** with rationale, so you know what's load-bearing
- **Dependencies** between distant parts of the codebase

## When to Read

**Always** before modifying code you didn't write in this session —
especially when debugging, choosing between approaches, or refactoring.
**Skip** for new files, trivial changes, or files you already checked.

## Commands

```bash
# Most important — contracts and dependencies ("what must I not break?")
git chronicle contracts src/foo.rs --anchor "bar_function"

# Decisions and rejected alternatives ("what was decided and why?")
git chronicle decisions --path src/foo.rs

# Quick orientation for a file
git chronicle summary src/foo.rs

# Raw annotations for a file/region
git chronicle read src/foo.rs --anchor "bar_function"

# What depends on this code?
git chronicle deps src/foo.rs "bar_function"

# How has this code evolved?
git chronicle history src/foo.rs --anchor "bar_function"

# Check repo-level knowledge
git chronicle knowledge list
```

## Knowledge Store

Beyond per-commit annotations, the knowledge store holds repo-level rules:

- **Conventions** — scoped coding rules (e.g., "All errors must include location")
- **Module boundaries** — what each module owns and its public interface constraints
- **Anti-patterns** — things to avoid, with what to do instead

```bash
# Check repo-level knowledge
git chronicle knowledge list
```

Knowledge entries are binding like contracts. If a convention applies to your
scope, follow it. If you need to change one, update the store.

## Respecting Contracts

Contracts are binding. Examples: "Must not block the async runtime", "Return
value must be sorted", "Assumes caller holds the write lock."

If you need to violate a contract:
1. Document why in your commit annotation
2. Update or remove the old contract via `git chronicle correct`
3. Add the new contract to your annotation

Never silently violate a contract.

## The Read-Write Cycle

Read before you modify, annotate after you commit. If you change or
supersede a contract or decision, say so in your annotation — this creates
a chain of reasoning future agents can follow:

```json
{
  "summary": "Accept unsorted input in parse() — the new batch API makes pre-sorting impractical, so the old contract is no longer viable.",
  "rejected_alternatives": [
    {"approach": "Sort in each caller", "reason": "6 call sites, O(n log n) each vs. one O(n) scan"}
  ],
  "decisions": [
    {"what": "Remove sorted-input precondition", "why": "Internal scan is cheaper than distributed sorting", "stability": "permanent"}
  ]
}
```
