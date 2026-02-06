# Feature 11: Annotation Corrections

## Overview

Annotations are not immutable truths. Code evolves, external dependencies change, assumptions become invalid, and the annotation agent sometimes gets things wrong. The correction system provides a mechanism for agents and developers to flag inaccurate annotations and record specific amendments without destroying the original content.

Corrections are additive-only. The original annotation is never overwritten. Instead, corrections accumulate as separate entries linked to the original annotation's commit SHA. When the read path retrieves an annotation, it also retrieves any corrections and surfaces them inline. Flagged annotations have their confidence scores reduced so that downstream agents treat them with appropriate skepticism.

This creates a self-correcting knowledge base: annotations improve over time as agents discover and report inaccuracies, while the full history of what was believed and when it was corrected is preserved.

---

## Dependencies

| Feature | Reason |
|---------|--------|
| 02 Git Operations Layer | Corrections are stored as git notes; requires notes read/write |
| 07 Read Pipeline | Corrections must be surfaced during `git chronicle read`; the read path must be extended to fetch and merge corrections |

---

## Public API

### CLI Commands

#### `git chronicle flag`

Flags the most recent annotation for a code region as potentially inaccurate.

```
git chronicle flag <PATH> [<ANCHOR>] --reason "<TEXT>"
```

**Arguments:**
- `<PATH>` — file path relative to repository root.
- `<ANCHOR>` — optional function/type name to scope the flag to a specific region. If omitted, flags the annotation for the entire file.
- `--reason <TEXT>` — required. Why the annotation is being flagged.

**Behavior:**
1. Resolve the anchor to identify the target region (using tree-sitter, same as `git chronicle read`).
2. Run `git blame` on the resolved line range to find the most recent commit SHA that touched the region.
3. Fetch the existing annotation for that commit.
4. Verify that an annotation exists and contains a region matching the anchor.
5. Write a correction entry linked to that commit SHA.

**Output:**
```
Flagged annotation on commit abc1234 for MqttClient::connect
  Reason: Constraint about drain-before-reconnect is no longer required since broker v2.3
  Correction stored in refs/notes/chronicle
```

#### `git chronicle correct`

Targets a specific annotation by commit SHA and applies a precise correction to a specific field.

```
git chronicle correct <SHA> --region <ANCHOR> --field <FIELD> --remove <VALUE>
```

**Arguments:**
- `<SHA>` — commit SHA of the annotation to correct.
- `--region <ANCHOR>` — the AST anchor name of the region within the annotation.
- `--field <FIELD>` — the annotation field to correct. One of: `intent`, `reasoning`, `constraints`, `risk_notes`, `semantic_dependencies`, `tags`.
- `--remove <VALUE>` — the specific value to remove or mark as incorrect. For array fields (`constraints`, `semantic_dependencies`, `tags`), this removes the matching entry. For string fields (`intent`, `reasoning`, `risk_notes`), this records a correction noting the value is inaccurate.

**Additional flags:**
- `--amend <TEXT>` — provide replacement text for the corrected field. Can be used alongside or instead of `--remove`.

**Output:**
```
Corrected annotation on commit abc1234, region MqttClient::connect
  Field: constraints
  Removed: "Must drain queue before reconnecting"
  Correction stored in refs/notes/chronicle
```

---

## Internal Design

### Correction Schema

```rust
/// A single correction entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    /// Which annotation field this correction targets
    pub field: String,

    /// The type of correction
    pub correction_type: CorrectionType,

    /// Human/agent-readable explanation of the correction
    pub correction_text: String,

    /// The specific value being removed or amended (for array fields)
    pub target_value: Option<String>,

    /// Replacement value (for amend corrections)
    pub replacement: Option<String>,

    /// When the correction was made
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Who made the correction (agent session ID, git author, etc.)
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CorrectionType {
    /// General flag that the annotation may be inaccurate
    Flag,
    /// Specific removal of a value from an array field
    Remove,
    /// Amendment of a field with new content
    Amend,
}
```

JSON representation:

```json
{
  "field": "constraints",
  "correction_type": "remove",
  "correction_text": "Constraint no longer required since broker v2.3",
  "target_value": "Must drain queue before reconnecting",
  "replacement": null,
  "timestamp": "2025-12-20T14:30:00Z",
  "author": "agent-session-abc123"
}
```

### Storage Design

Corrections are stored within the same notes system as annotations, under `refs/notes/chronicle`. They are embedded in the annotation's JSON document as an additional `corrections` field on the relevant region.

**Storage approach: augment the annotation JSON.**

When a correction is written:

1. Read the existing annotation note for the target commit SHA.
2. Parse the JSON.
3. Find the matching region by `ast_anchor.name` (or file path if no anchor).
4. Append the correction to the region's `corrections` array (create the array if it doesn't exist).
5. Write the updated annotation back as the note.

This keeps corrections co-located with the annotation they target, which means the read path doesn't need to do a separate lookup — corrections come for free when the annotation is fetched.

```rust
/// Augmented region annotation with corrections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionAnnotation {
    pub file: String,
    pub ast_anchor: AstAnchor,
    pub lines: LineRange,
    pub intent: String,
    pub reasoning: Option<String>,
    pub constraints: Vec<Constraint>,
    pub semantic_dependencies: Vec<SemanticDependency>,
    pub related_annotations: Vec<RelatedAnnotation>,
    pub tags: Vec<String>,
    pub risk_notes: Option<String>,

    /// Corrections accumulated for this region (additive-only)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrections: Vec<Correction>,
}
```

#### Why Not a Separate Notes Ref?

An alternative design would store corrections under a separate ref like `refs/notes/chronicle-corrections`. This was rejected because:

- The read path would need to make two note lookups per commit (annotation + corrections), doubling I/O.
- Corrections would be easy to lose during notes sync if only the main ref is synced.
- The co-location model keeps all knowledge about a commit in one place.

The tradeoff is that writing a correction requires a read-modify-write on the annotation note, which creates a brief race condition if two corrections are written simultaneously. In practice, this is unlikely — corrections are rare relative to annotations, and two agents correcting the same annotation at the same time is extraordinarily unlikely. If it happens, the second write wins and the first correction is lost. This is acceptable for v1.

### Read-Path Integration

When `git chronicle read` fetches an annotation and the region has corrections, the output includes them inline:

```json
{
  "file": "src/mqtt/client.rs",
  "ast_anchor": {
    "type": "method",
    "name": "MqttClient::connect"
  },
  "intent": "Establishes mTLS connection to the cloud MQTT broker...",
  "constraints": [
    {
      "text": "Must drain queue before reconnecting",
      "source": "author"
    }
  ],
  "corrections": [
    {
      "field": "constraints",
      "correction_type": "remove",
      "correction_text": "Constraint no longer required since broker v2.3",
      "target_value": "Must drain queue before reconnecting",
      "timestamp": "2025-12-20T14:30:00Z",
      "author": "agent-session-abc123"
    }
  ],
  "confidence": 0.65
}
```

#### Confidence Impact

Corrections reduce the confidence score of the affected region. The scoring module (Feature 07) applies a penalty:

```rust
/// Confidence penalty per correction on a region
const CORRECTION_PENALTY: f64 = 0.15;

/// Minimum confidence floor (corrections can't reduce below this)
const CORRECTION_FLOOR: f64 = 0.1;

pub fn apply_correction_penalty(base_confidence: f64, correction_count: usize) -> f64 {
    let penalty = correction_count as f64 * CORRECTION_PENALTY;
    (base_confidence - penalty).max(CORRECTION_FLOOR)
}
```

A single flag reduces confidence by 0.15. Two flags reduce it by 0.30. The floor of 0.1 ensures the annotation is still visible (agents should see the corrected annotation with its corrections rather than having it disappear entirely).

The `flag` correction type applies the penalty to the entire region. The `remove` correction type applies the penalty only when computing confidence for queries that specifically reference the removed field. The `amend` correction type does not reduce confidence — it improves the annotation.

### Author Resolution

The `author` field on a correction is populated from:

1. The git user name/email from `git config user.name` / `git config user.email`.
2. If an `CHRONICLE_SESSION` environment variable is set (for agent identification), use that.
3. Fall back to `"unknown"`.

This is best-effort identification, not authentication. Corrections are not access-controlled — any user with write access to the repository can write corrections.

---

## Error Handling

| Failure Mode | Handling |
|---|---|
| Target commit SHA has no annotation | Return error: "No annotation found for commit <SHA>. Cannot apply correction." |
| Target anchor not found in annotation | Return error: "No region matching '<ANCHOR>' found in annotation for commit <SHA>." List available regions. |
| Target field doesn't exist or is empty | Return error: "Field '<FIELD>' is empty in region '<ANCHOR>'. Nothing to correct." |
| `--remove` value doesn't match any entry | Return error: "Value not found in '<FIELD>'. Available values: ..." List existing values for the field. |
| `git blame` fails to resolve anchor | Return error with suggestion to use `--lines` or specify the commit SHA directly with `git chronicle correct`. |
| Note write fails (permissions, corrupt repo) | Return error from git operations layer. Do not leave partial state. |
| Concurrent correction write (race condition) | Last writer wins. Lost corrections are logged to `.git/chronicle/failed.log` with details for manual re-application. |

---

## Configuration

No additional configuration is required for corrections. The correction system uses the same notes ref (`refs/notes/chronicle`) and follows the same sync configuration as annotations.

One optional config key:

| Key | Default | Description |
|-----|---------|-------------|
| `chronicle.corrections.confidencePenalty` | `0.15` | Confidence penalty per correction |
| `chronicle.corrections.confidenceFloor` | `0.1` | Minimum confidence after corrections |

---

## Implementation Steps

### Step 1: Correction Schema
**Scope:** `src/schema/correction.rs`

- Define `Correction` struct with serde serialization.
- Define `CorrectionType` enum.
- Add `corrections: Vec<Correction>` field to `RegionAnnotation`.
- Ensure backward compatibility: existing annotations without the `corrections` field deserialize correctly (serde `default`).
- Tests: serialize/deserialize corrections, deserialize annotations without corrections field.

### Step 2: Flag Command
**Scope:** `src/cli/flag.rs`

- Parse `<PATH>`, `<ANCHOR>`, `--reason` arguments.
- Resolve anchor to line range via tree-sitter (reuse `anchor_resolve` from Feature 03).
- Run `git blame` on resolved lines to find the most recent commit SHA.
- Fetch the annotation for that commit.
- Find the matching region.
- Create a `Correction` with `correction_type: Flag`.
- Write the updated annotation back.
- Tests: flag an existing annotation, flag with anchor, flag without anchor (file-level).

### Step 3: Correct Command
**Scope:** `src/cli/correct.rs`

- Parse `<SHA>`, `--region`, `--field`, `--remove`, `--amend` arguments.
- Fetch the annotation for the specified SHA.
- Find the matching region by anchor name.
- Validate the field and value exist.
- Create a `Correction` with `correction_type: Remove` or `Amend`.
- For `--remove` on array fields, also add the correction entry (do not actually remove the original value — the original stays, the correction records what was retracted).
- Write the updated annotation back.
- Tests: correct a constraint, correct a semantic dependency, amend a reasoning field.

### Step 4: Read-Path Integration
**Scope:** `src/read/retrieve.rs`, `src/read/scoring.rs`

- No changes needed for correction retrieval — corrections are already in the annotation JSON.
- Modify confidence scoring to apply `apply_correction_penalty()` when corrections are present.
- Ensure the output schema includes corrections in the serialized JSON.
- Tests: read an annotation with corrections, verify confidence reduction, verify corrections appear in output.

### Step 5: Skill Definition Update
**Scope:** `src/skill.rs`

- Update the embedded skill definition to teach agents about `git chronicle flag` and `git chronicle correct`.
- Add guidance: "If you discover an annotation's constraint or reasoning is incorrect, use `git chronicle flag` immediately to prevent future agents from being misled."
- Bump the skill version marker.
- Tests: verify updated skill content includes correction commands.

---

## Test Plan

### Unit Tests

- **Correction serialization:** Round-trip serialize/deserialize of `Correction` struct. Verify all fields.
- **CorrectionType variants:** Verify each variant serializes to the expected string.
- **Backward compatibility:** Deserialize an annotation JSON from before corrections were added (no `corrections` field). Verify it parses without error and `corrections` is empty.
- **Confidence penalty calculation:** Test `apply_correction_penalty` with 0, 1, 2, 5 corrections. Verify floor is respected.
- **Region matching:** Test finding a region by anchor name in an annotation with multiple regions. Test fuzzy matching.
- **Flag target resolution:** Test blame + anchor resolution pipeline to find the correct commit SHA.

### Integration Tests

- **Flag round-trip:**
  1. Create a repo with a commit.
  2. Write an annotation for the commit.
  3. `git chronicle flag <path> <anchor> --reason "..."`.
  4. `git chronicle read <path> <anchor>`.
  5. Verify the correction appears in the output.
  6. Verify confidence is reduced.

- **Correct round-trip:**
  1. Create a repo with a commit.
  2. Write an annotation with a specific constraint.
  3. `git chronicle correct <SHA> --region <anchor> --field constraints --remove "the constraint"`.
  4. `git chronicle read <path> <anchor>`.
  5. Verify the correction appears, the original constraint is still present, and confidence is reduced.

- **Multiple corrections accumulate:**
  1. Write an annotation.
  2. Flag it once.
  3. Flag it again with a different reason.
  4. Read and verify both corrections appear.
  5. Verify confidence is reduced by 2x penalty.

- **Amend correction:**
  1. Write an annotation with a reasoning field.
  2. `git chronicle correct <SHA> --region <anchor> --field reasoning --amend "Updated reasoning"`.
  3. Read and verify the amend correction appears alongside original reasoning.

- **Corrections survive sync:**
  1. Clone A writes an annotation. Clone B flags it.
  2. Sync between clones.
  3. Both clones see the annotation with the correction.

### Edge Cases

- Flag a commit that has no annotation (error).
- Flag an anchor that doesn't exist in the annotation (error with helpful message).
- Correct with `--remove` value that doesn't match (error with available values).
- Flag the same region twice with the same reason (both corrections are stored — deduplication is not enforced).
- Correct a field that is empty/null (error).
- Flag an annotation in a file that no longer exists at HEAD (should still work — the annotation is on the commit, not the current file).
- Concurrent flags on the same annotation from two agents (last writer wins, logged).

---

## Acceptance Criteria

1. `git chronicle flag <PATH> <ANCHOR> --reason "..."` writes a correction entry to the annotation for the most recent commit touching that code region.
2. `git chronicle correct <SHA> --region <ANCHOR> --field <FIELD> --remove <VALUE>` writes a precise correction entry targeting a specific field and value.
3. `git chronicle correct <SHA> --region <ANCHOR> --field <FIELD> --amend <TEXT>` writes an amendment correction.
4. Corrections are stored within the annotation JSON on the target commit's note, not in a separate notes ref.
5. Original annotation content is never deleted or overwritten by corrections — corrections are additive only.
6. `git chronicle read` output includes the `corrections` array for any region that has corrections.
7. Confidence scores are reduced by 0.15 per flag/remove correction on the region, with a floor of 0.1.
8. Amend corrections do not reduce confidence.
9. Corrections sync correctly with `git chronicle sync pull` — they are part of the annotation JSON and travel with it.
10. Annotations without corrections deserialize correctly (backward compatible).
11. Error messages for invalid targets (missing annotation, missing region, missing value) are specific and actionable.
12. Multiple corrections on the same region accumulate without interfering with each other.
