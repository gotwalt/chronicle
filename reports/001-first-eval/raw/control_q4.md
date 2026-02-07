# Q4: Rejected Alternatives in v2 Schema Design

## Answer

Based on a thorough examination of `src/schema/v2.rs`, the git commit history, and the feature documentation in `features/20-schema-v2.md`, **the source code does not explicitly document three specific alternative approaches that were considered and rejected during the v2 annotation schema design**.

## What the Code Does Show

The source code and commit history reveal:

1. **What was replaced**: The `RejectedAlternative` struct was added to the schema (lines 82-86 in v2.rs) with fields `approach: String` and `reason: String`, designed to allow annotations to document alternatives considered at the commit level.

2. **Design rationale**: The feature document (features/20-schema-v2.md) describes the v2 redesign moving from per-function regions to commit-level narrative, but does not enumerate three rejected alternative design approaches.

3. **What was removed from v1**: The document clearly lists v1 structures that were eliminated (RegionAnnotation, intent, reasoning, tags, risk_notes, constraints, semantic_dependencies, related_annotations, CrossCuttingConcern, context_level) in favor of the new v2 types (CodeMarker, Narrative, Decision, EffortLink, Provenance), but this describes evolutionary changes, not rejected alternatives to the v2 design itself.

## Conclusion

The three rejected alternative approaches to the v2 schema design are **not documented in the source code, git commit messages, or feature specifications available in this repository**. The `RejectedAlternative` type exists as a data structure for future annotations to record rejected alternatives at the commit level, but the design process that led to v2 itself does not appear to have been formally documented with three enumerated alternatives.
