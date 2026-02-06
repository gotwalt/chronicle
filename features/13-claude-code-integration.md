# Feature 13: Claude Code Integration

## Overview

Integrates Chronicle with Claude Code so that the agent automatically annotates commits it creates. This is the "live path" — the agent calls `chronicle_annotate` as an MCP tool immediately after committing, providing intent, reasoning, constraints, and dependencies from its own working context. Zero LLM cost because the authoring agent already knows everything needed for the annotation.

**Two-path architecture:**
- **Live path**: Agent calls `chronicle_annotate` MCP tool after committing. The agent provides structured metadata from its own context. Zero LLM cost.
- **Batch path**: `git chronicle annotate --commit <sha>` uses the existing API-based agent loop (Feature 05). For CI pipelines, backfill of historical commits, and commits made by humans without agent assistance.

---

## Dependencies

| Feature | Reason |
|---------|--------|
| 12 MCP Server | `chronicle_annotate` is exposed as an MCP tool |
| 05 Writing Agent | Batch path fallback uses the existing agent loop |
| 03 AST Parsing | Anchor resolution for line correction and signature extraction |
| 02 Git Operations | Note writing, ref resolution, file-at-commit |

---

## Components

### 1. MCP Annotate Handler (`src/mcp/annotate_handler.rs`)

The core handler that receives structured annotation data from the calling agent and writes it as a git note. See Feature 12 for the `chronicle_annotate` tool definition.

**Key design decisions:**
- **`context_level: Enhanced`** — always, because the authoring agent has direct knowledge
- **`ConstraintSource::Author`** — always, because constraints come from the agent that wrote the code
- **AST anchor resolution** — the handler loads the file at the commit, extracts the outline, and resolves the agent's anchor name to get corrected line ranges and filled signatures
- **Quality warnings** — non-blocking feedback about short intents, missing reasoning, or absent constraints; returned to the agent but don't prevent the write
- **Validation** — structural errors (empty summary, invalid line ranges) reject the annotation

### 2. Claude Code Skill (`.claude/skills/annotate/SKILL.md`)

A Claude Code skill definition that teaches the agent when and how to annotate commits. The skill:

- Triggers after the agent creates a git commit
- Instructs the agent on what fields to provide (summary, regions, intent, reasoning, constraints, dependencies)
- Sets quality expectations (minimum detail for intent/reasoning, constraint coverage)
- Provides the `chronicle_annotate` tool call template
- Includes fallback instructions if the MCP tool is unavailable

### 3. PostToolUse Hook (`.claude/hooks/post-tool-use/annotate-reminder.sh`)

A Claude Code hook that fires after the Bash tool is used. If the command was a `git commit`, it reminds the agent to annotate the commit using the `chronicle_annotate` MCP tool.

---

## Workflow

```
Agent writes code
       │
       ▼
Agent commits via Bash tool
       │
       ▼
PostToolUse hook fires ──── "Remember to annotate this commit"
       │
       ▼
Agent calls chronicle_annotate MCP tool
  ├── commit: "HEAD"
  ├── summary: "..."
  ├── regions: [{ file, anchor, intent, reasoning, constraints, ... }]
  └── cross_cutting: [...]
       │
       ▼
MCP handler:
  1. resolve_ref("HEAD") → full SHA
  2. For each region: AST outline → resolve_anchor → correct lines
  3. Build Annotation (Enhanced, Author constraints)
  4. validate() → reject or proceed
  5. check_quality() → warnings
  6. note_write() → git note
       │
       ▼
Result returned to agent:
  ├── success: true
  ├── regions_written: N
  ├── warnings: [...]
  └── anchor_resolutions: [...]
```

---

## Error Handling

| Failure Mode | Handling |
|---|---|
| MCP tool unavailable | Agent falls back to `git chronicle annotate --commit HEAD` CLI |
| Anchor doesn't resolve | Handler uses input lines as-is, returns `Unresolved` status |
| File not available at commit | Handler uses input as-is, anchor unresolved |
| Unsupported language | Handler skips AST resolution, uses input as-is |
| Empty summary or intent | `validate()` rejects — agent must fix and retry |
| Quality issues (short intent, no reasoning) | Non-blocking warnings returned to agent |

---

## Test Plan

### Unit Tests (in `src/mcp/annotate_handler.rs`)

- **Handler writes note:** Mock GitOps → call `handle_annotate` → verify note written with correct schema, commit, context_level, provenance
- **Anchor resolution exact:** Provide a Rust file with known functions → verify exact resolution
- **Anchor resolution corrects lines:** Verify AST-corrected lines differ from input approximation
- **Constraints have Author source:** All constraints from the handler have `ConstraintSource::Author`
- **Quality warnings:** Short summary, short intent, missing reasoning, missing constraints → verify warnings generated
- **Validation rejection:** Empty summary → verify error returned, no note written
- **Unsupported language:** Python file → verify handler succeeds with unresolved anchor
- **Missing file:** File not at commit → verify handler succeeds with unresolved anchor

### Integration Tests

- **Full round-trip:** Create a real git commit → call `handle_annotate` → read the note back → verify annotation matches input
- **MCP tool invocation:** Start MCP server → send `tools/call` for `chronicle_annotate` → verify note written

---

## Acceptance Criteria

1. An MCP-connected agent can call `chronicle_annotate` to write an annotation after committing.
2. The handler resolves AST anchors, corrects line ranges, and fills signatures for supported languages.
3. Annotations written via the handler have `context_level: Enhanced` and `ConstraintSource::Author`.
4. The handler validates annotations and rejects structural errors.
5. Quality warnings are returned but don't block the write.
6. The Claude Code skill teaches the agent when and how to annotate.
7. The PostToolUse hook reminds the agent to annotate after `git commit`.
8. The batch path (`git chronicle annotate --commit <sha>`) continues to work for CI and backfill.
