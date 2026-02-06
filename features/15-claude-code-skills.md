# Feature 15: Claude Code Skills & Agent Workflow

## Overview

A complete set of Claude Code skills, hooks, and project configuration that makes Chronicle a first-class part of the agent's workflow. The agent should **read annotations before modifying code** and **write annotations after committing code**, with both paths feeling natural rather than bolted on.

This feature builds on the MCP handler (Feature 13) and MCP tool definitions (Feature 12) to create three skills:

1. **annotate** (exists, refined here) — Write annotations after committing
2. **context** (new) — Read chronicle annotations before modifying code
3. **backfill** (new) — Annotate historical commits that lack annotations

Plus: project-level CLAUDE.md instructions, MCP server configuration, and hooks that guide the agent at key decision points.

---

## Dependencies

| Feature | Reason |
|---------|--------|
| 12 MCP Server | `chronicle_read`, `chronicle_deps`, `chronicle_summary` tools for the context skill |
| 13 Claude Code Integration | `chronicle_annotate` MCP tool, annotate handler, existing annotate skill |
| 07 Read Pipeline | CLI fallback for reading annotations |
| 08 Advanced Queries | `deps`, `history`, `summary` for context gathering |
| 05 Writing Agent | Batch path fallback for backfill |

---

## Components

### 1. Context Skill (`.claude/skills/context/SKILL.md`)

Teaches the agent to **proactively query Chronicle annotations before modifying code**. This is the highest-value integration: the agent gains access to intent, constraints, semantic dependencies, and risk notes that would otherwise be invisible.

**When to trigger:**
- Before modifying a function, struct, or module the agent didn't write
- Before refactoring code that has known constraints or dependencies
- When debugging unexpected behavior
- When the agent opens a file it hasn't seen before in the session

**What the agent should do:**
1. Call `chronicle_read` with the file path and anchor (function/struct name) to get intent, reasoning, and constraints for the specific region
2. Call `chronicle_deps` to discover what other code depends on behavioral assumptions about this region — critical before changing signatures or semantics
3. Optionally call `chronicle_summary` for a broad overview when entering an unfamiliar module

**Key instruction: constraints are binding.** If a constraint says "must not block the async runtime" or "assumes caller holds the lock," the agent must respect it or explicitly note why it's being violated (and update the annotation accordingly).

### 2. Annotate Skill (`.claude/skills/annotate/SKILL.md`) — Refined

The existing skill is functional. Refinements:

- Add explicit guidance on **when to include semantic_dependencies**: any time the region calls, wraps, or makes behavioral assumptions about code in other files
- Add guidance on **cross_cutting concerns**: when a commit applies the same pattern across multiple regions (e.g., error handling, logging, validation)
- Add a **fallback to CLI pipe** section: when the MCP tool is unavailable, pipe `AnnotateInput` JSON to `git chronicle annotate --live` via Bash
- Reference the context skill: "If you read annotations before modifying, reference what changed from the prior annotation in your reasoning field"

### 3. Backfill Skill (`.claude/skills/backfill/SKILL.md`)

Annotates historical commits that don't have Chronicle annotations. Useful for:

- Onboarding a new repository to Chronicle
- Filling gaps where the hook wasn't installed
- Re-annotating after importing a project

**Workflow:**

1. Identify unannotated commits: `git log --format='%H' | while read sha; do git notes --ref=refs/notes/chronicle show "$sha" 2>/dev/null || echo "$sha"; done` (or a future `git chronicle backfill --list` command)
2. For each unannotated commit:
   a. Read the diff: `git show --stat <sha>` and `git diff <sha>~1..<sha>`
   b. Read the file content at that commit for each changed file
   c. Extract the AST outline to identify affected functions/structs
   d. Reconstruct intent and reasoning from the diff and commit message
   e. Call `chronicle_annotate` MCP tool (or pipe to `git chronicle annotate --live`)
3. Work in chronological order (oldest first) so later annotations can reference earlier ones
4. Quality bar: backfilled annotations should be honest about uncertainty — use reasoning like "Reconstructed from diff: appears to..." rather than stating intent with false confidence

**Constraints:**
- Backfill annotations get `context_level: Inferred` (not `Enhanced`) because the agent is reconstructing, not reporting firsthand knowledge
- The agent should batch commits in groups of 5-10 to avoid context overflow
- Skip merge commits unless they have manual conflict resolutions
- Skip commits that only touch lockfiles, generated files, or vendored code

### 4. Project Configuration

#### `.mcp.json` — MCP Server Registration

```json
{
  "mcpServers": {
    "chronicle": {
      "command": "git-chronicle",
      "args": ["mcp", "start"],
      "cwd": "."
    }
  }
}
```

When the MCP server (Feature 12) is implemented, this file registers it so Claude Code automatically starts and connects to the server. Until then, the skills use CLI fallbacks.

#### `CLAUDE.md` — Project Instructions

Add a section to CLAUDE.md teaching the agent about Chronicle:

```markdown
## Working with Chronicle annotations

This project uses Chronicle (`git-chronicle`) to store structured metadata
alongside commits as git notes. Before modifying existing code, query the
annotations to understand intent, constraints, and dependencies.

### Reading annotations (before modifying code)

Use the `chronicle_read` MCP tool to check annotations on code you're about
to modify:

- `chronicle_read(path: "src/foo.rs", anchor: "bar_function")` — get intent,
  reasoning, constraints for a specific function
- `chronicle_deps(path: "src/foo.rs", anchor: "bar_function")` — find code
  that depends on this function's behavior
- `chronicle_summary(path: "src/foo.rs")` — overview of all annotated regions

If MCP tools are unavailable, use the CLI:
  `git chronicle read src/foo.rs --anchor bar_function`

**Respect constraints.** Annotations may include constraints like "must not
allocate" or "assumes sorted input." Violating these without updating the
annotation is a bug.

### Writing annotations (after committing)

After creating a git commit, annotate it. See `.claude/skills/annotate/SKILL.md`.
```

#### `.claude/hooks/` — Agent Hooks

**`post-tool-use/annotate-reminder.sh`** (exists) — Reminds agent to annotate after `git commit`.

**`pre-tool-use/read-context-hint.sh`** (new) — When the agent is about to edit a file, suggest reading Chronicle annotations first. This fires before the Edit/Write tool is used.

```bash
#!/usr/bin/env bash
# PreToolUse hook: suggest reading chronicle annotations before editing

input=$(cat)
tool_name=$(echo "$input" | jq -r '.tool_name // empty' 2>/dev/null)

# Only for Edit and Write tools
case "$tool_name" in
    Edit|Write) ;;
    *) exit 0 ;;
esac

file_path=$(echo "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

# Only for source code files
case "$file_path" in
    *.rs|*.ts|*.tsx|*.js|*.jsx|*.py|*.go|*.java|*.cpp|*.c|*.h)
        echo "TIP: Consider reading Chronicle annotations for $(basename "$file_path") before modifying it. Use chronicle_read or: git chronicle read \"$file_path\""
        ;;
esac
```

This is a non-blocking hint — the agent is not required to read annotations, but is reminded that they exist.

---

## Skill File Details

### `.claude/skills/context/SKILL.md`

```markdown
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

- "Must not block the async runtime" — don't add blocking I/O
- "Assumes caller holds the write lock" — don't call without locking
- "Return value must be sorted" — preserve sort invariant
- "Must be idempotent" — no side effects on repeated calls

If you need to violate a constraint:
1. Note it explicitly in your commit annotation's reasoning field
2. Remove or update the old constraint via `git chronicle correct`
3. Add the new constraint to your annotation

Never silently violate a constraint — future agents and developers rely on them.

## Integration with Annotating

If you read annotations before modifying code, reference what changed in your
post-commit annotation:

```json
{
  "reasoning": "Previous annotation noted 'assumes sorted input'. Changed to
    accept unsorted input and sort internally, because callers can no longer
    guarantee ordering after the new batch API was added."
}
```

This creates a clear trail of how design decisions evolved.
```

### `.claude/skills/backfill/SKILL.md`

```markdown
# Chronicle Backfill Skill

## When to Use

Use this skill to annotate historical commits that don't have Chronicle
annotations. This is useful when:

- A repository is being onboarded to Chronicle for the first time
- The post-commit hook wasn't installed for some commits
- You want to fill annotation gaps for better `chronicle deps` and
  `chronicle history` coverage

## How to Backfill

### 1. Find unannotated commits

List recent commits and check which have annotations:

```bash
# List last 20 commits with annotation status
for sha in $(git log --format='%H' -20); do
  if git notes --ref=refs/notes/chronicle show "$sha" 2>/dev/null | head -1 | grep -q chronicle; then
    echo "  annotated: $(git log --format='%h %s' -1 $sha)"
  else
    echo "UNANNOTATED: $(git log --format='%h %s' -1 $sha)"
  fi
done
```

### 2. For each unannotated commit

Read the diff and commit message to reconstruct intent:

```bash
# See what changed
git show --stat <sha>
git log --format='%B' -1 <sha>
git diff <sha>~1..<sha>
```

Then read the file content at that commit for affected files:

```bash
git show <sha>:src/path/to/file.rs
```

### 3. Write the annotation

Call the `chronicle_annotate` MCP tool (or pipe to CLI):

```json
{
  "commit": "<full-sha>",
  "summary": "Reconstructed: <what the commit does based on diff and message>",
  "regions": [
    {
      "file": "src/path/to/file.rs",
      "anchor": { "unit_type": "function", "name": "changed_function" },
      "lines": { "start": 10, "end": 25 },
      "intent": "Reconstructed from diff: appears to add error handling for ...",
      "reasoning": "Reconstructed: likely chosen because ...",
      "constraints": [
        { "text": "Inferred: must handle null input based on added guard clause" }
      ],
      "semantic_dependencies": [],
      "tags": ["backfill"]
    }
  ]
}
```

CLI fallback:

```bash
echo '<AnnotateInput JSON>' | git chronicle annotate --live
```

### Quality Guidelines

**Be honest about uncertainty.** Backfilled annotations are reconstructed from
diffs and commit messages, not from firsthand knowledge. Use language that
signals this:

- Intent: "Reconstructed from diff: appears to..." or "Based on commit message: ..."
- Reasoning: "Likely chosen because..." or "Inferred from the diff pattern: ..."
- Constraints: prefix with "Inferred:" when you're deducing constraints from
  code patterns rather than knowing them directly

**Always include the `backfill` tag** so readers know these were reconstructed.

**Work chronologically** (oldest first) so later annotations can reference
dependencies established by earlier ones.

**Skip these commits:**
- Merge commits with no manual conflict resolution
- Commits that only change lockfiles (Cargo.lock, package-lock.json, etc.)
- Commits that only change generated or vendored files
- Commits with trivial formatting-only changes

**Batch size:** Process 5-10 commits per session to avoid context overflow.
After each batch, verify with `git chronicle read` that the annotations look
reasonable.

### Verifying Backfill Quality

After backfilling, spot-check with the TUI:

```bash
git chronicle show src/path/to/file.rs --commit <sha>
```

Or read directly:

```bash
git chronicle read src/path/to/file.rs
```

Look for:
- Accurate line ranges (AST resolution should correct approximate lines)
- Reasonable intent descriptions that match the actual code
- No fabricated constraints or dependencies
```

---

## Workflow Integration

The three skills form a complete read-modify-write loop:

```
Agent receives task
       │
       ▼
[Context Skill] ← "Read annotations before modifying"
  ├── chronicle_read: intent, constraints, reasoning
  ├── chronicle_deps: who depends on this code?
  └── chronicle_summary: file-level overview
       │
       ▼
Agent understands constraints and dependencies
       │
       ▼
Agent modifies code (respecting constraints)
       │
       ▼
Agent commits
       │
       ▼
[PostToolUse Hook] ← "Reminder: annotate this commit"
       │
       ▼
[Annotate Skill] ← "Write annotation with intent, reasoning, constraints"
  ├── Reference prior annotations in reasoning field
  ├── Note any constraint changes
  └── Declare semantic dependencies
       │
       ▼
Annotation stored as git note
```

The backfill skill operates outside this loop, filling gaps in historical coverage.

---

## File Manifest

| File | Status | Purpose |
|------|--------|---------|
| `.claude/skills/annotate/SKILL.md` | Exists (refine) | Write annotations after committing |
| `.claude/skills/context/SKILL.md` | New | Read annotations before modifying code |
| `.claude/skills/backfill/SKILL.md` | New | Annotate historical commits |
| `.claude/hooks/post-tool-use/annotate-reminder.sh` | Exists | Remind agent to annotate after `git commit` |
| `.claude/hooks/pre-tool-use/read-context-hint.sh` | New | Suggest reading annotations before editing |
| `.mcp.json` | New | MCP server registration (when F12 is implemented) |
| `CLAUDE.md` | Update | Add "Working with Chronicle annotations" section |

---

## Implementation Steps

### Step 1: Context Skill
**Scope:** `.claude/skills/context/SKILL.md`

Write the skill file as specified above. The skill teaches the agent to use `chronicle_read`, `chronicle_deps`, `chronicle_summary`, and `chronicle_history` MCP tools (with CLI fallbacks) before modifying code.

### Step 2: Backfill Skill
**Scope:** `.claude/skills/backfill/SKILL.md`

Write the skill file as specified above. Include the commit discovery workflow, annotation format with uncertainty markers, and quality guidelines.

### Step 3: Refine Annotate Skill
**Scope:** `.claude/skills/annotate/SKILL.md`

Update the existing skill with:
- Guidance on semantic_dependencies (when to include them)
- Cross-cutting concern examples
- CLI pipe fallback (AnnotateInput JSON to `git chronicle annotate --live`)
- Reference to context skill for change-trail reasoning

### Step 4: Read-Context Hook
**Scope:** `.claude/hooks/pre-tool-use/read-context-hint.sh`

Write the pre-tool-use hook that suggests reading annotations before editing source files.

### Step 5: CLAUDE.md Update
**Scope:** `CLAUDE.md`

Add the "Working with Chronicle annotations" section that teaches the agent about the read/write annotation loop and references the skills.

### Step 6: MCP Server Config (Placeholder)
**Scope:** `.mcp.json`

Create the MCP server registration file. This is a placeholder until Feature 12 (full MCP server) is implemented — the skills use CLI fallbacks in the meantime.

---

## Test Plan

Skills and hooks are text/config files, not compiled code. Testing is behavioral:

### Manual Verification

1. **Context skill triggers:** Start a new Claude Code session in the repo. Ask the agent to modify a function that has Chronicle annotations. Verify the agent reads annotations before making changes.
2. **Constraint respect:** Ask the agent to modify code that has a constraint. Verify it either respects the constraint or explicitly notes the violation.
3. **Annotate skill after commit:** Ask the agent to implement a small feature and commit. Verify it annotates with intent, reasoning, and constraints.
4. **Backfill workflow:** Ask the agent to backfill annotations for 3-5 historical commits. Verify:
   - Annotations include the `backfill` tag
   - Intent uses "Reconstructed from diff" language
   - Line ranges are corrected by AST resolution
   - Merge/lockfile commits are skipped
5. **Hook fires:** Make an edit via Claude Code. Verify the pre-tool-use hook prints the context hint. Make a commit. Verify the post-tool-use hook prints the annotate reminder.
6. **CLI fallback:** Disable the MCP tools (no `.mcp.json`). Verify the agent falls back to `git chronicle read` and `git chronicle annotate --live` via Bash.

### Automated Checks

- Hook scripts are valid bash: `bash -n <script>`
- Skill markdown renders correctly: no broken links, valid code blocks
- CLAUDE.md section is present and references the correct tool names

---

## Acceptance Criteria

1. Three skill files exist: `annotate`, `context`, `backfill`.
2. The context skill instructs the agent to query Chronicle MCP tools (or CLI fallbacks) before modifying annotated code.
3. The context skill explains how to interpret and respect constraints from annotations.
4. The backfill skill provides a complete workflow for annotating historical commits, including uncertainty language and the `backfill` tag.
5. The annotate skill includes guidance on semantic dependencies, cross-cutting concerns, and references the context skill.
6. A pre-tool-use hook suggests reading annotations before editing source files.
7. CLAUDE.md includes a "Working with Chronicle annotations" section.
8. All skills include CLI fallback instructions for when MCP tools are unavailable.
9. The three skills form a coherent read-modify-write loop documented in the workflow section.
