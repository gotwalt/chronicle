# Three Rejected Approaches in v2 Annotation Schema Design

When designing the v2 annotation schema (committed in c160564437bf39f2fa7808cd719b66be78db37e3), three alternative approaches were considered and rejected:

## 1. Use MCP server as the integration surface

**Rejection reason:** MCP server was never fully built. Skills + CLI provide better workflow with self-documenting schema via `git chronicle schema`.

**Context:** The original plan for Chronicle included building a full Model Context Protocol (MCP) server as the integration point. However, this approach proved unnecessary and overly complex. The MCP server was never built beyond the initial `src/mcp/annotate_handler.rs` stub. Instead, the team found that a combination of skills (reusable workflow instructions) and a self-documenting CLI were more practical and achieved better integration with AI agent workflows. The `git chronicle schema` subcommand makes the CLI self-describing for agents to query input/output formats at runtime.

## 2. Bulk-migrate existing v1 notes to v2 on disk

**Rejection reason:** Risky rewrite of git notes history. Lazy migration on read is safer and requires no bulk operation.

**Context:** One approach would have been to immediately rewrite all existing v1 annotations to v2 format in git notes when the system was upgraded. This was rejected because it carries significant risk: any bug in the migration script would corrupt the git notes history, and there's no way to atomically update all notes across all branches and worktrees. Instead, the team adopted a lazy migration strategy where old v1 notes remain untouched on disk and are transparently converted to the canonical v2 type at read time via the `schema::parse_annotation()` function. This provides backward compatibility with zero risk to the git repository.

## 3. Enrich v1 with optional commit-level fields

**Rejection reason:** Per-region structure still dominates and creates noise. Would be a half-measure that keeps the wrong primary unit.

**Context:** Rather than a complete schema redesign, an incremental approach would have been to keep the v1 per-region structure but add optional fields for commit-level metadata (summary, decisions, rejected alternatives) to the top level. This was rejected because it would not address the fundamental design flaw in v1: the per-function region structure forced granularity that created overhead and noise while missing the highest-value metadata. Adding commit-level fields on top of per-region annotations would have created a half-measure with conflicting primary units. The v2 design inverted this: the commit becomes the primary unit of annotation, with code-level metadata (markers) only as an optional secondary layer used when there's something genuinely non-obvious about a specific code location.
