# Annotation JSON Deserialization Rule

## Mandatory Rule

All annotation JSON deserialization in the git-chronicle codebase must use the single function:

```rust
schema::parse_annotation(json: &str) -> Result<v2::Annotation, ParseAnnotationError>
```

## What Must NOT Be Done

Never deserialize annotations directly using `serde_json::from_str`, regardless of whether deserializing to a specific type (e.g., `v2::Annotation`, `v1::Annotation`) or a generic value.

**Incorrect examples:**
```rust
// Wrong - direct serde_json deserialization
let annotation: Annotation = serde_json::from_str(&note)?;

// Wrong - even for Value types
let value: serde_json::Value = serde_json::from_str(&note_content)?;
```

## Why This Constraint Exists

The `parse_annotation()` function is the **single deserialization chokepoint** that:

1. **Detects schema version**: Peeks at the `schema` field in the JSON to determine whether the annotation is `chronicle/v1` or `chronicle/v2`

2. **Handles migration**: Automatically converts `chronicle/v1` annotations to the canonical `chronicle/v2` format via the `migrate::v1_to_v2()` function

3. **Enforces consistency**: Ensures all annotation reading goes through a single, validated path that understands the versioning contract

4. **Preserves backward compatibility**: Allows the codebase to support both v1 (legacy) and v2 (current) annotation formats transparently, with migration happening at the deserialization boundary

Without using this function, code risks:
- Deserializing v1 annotations that would fail when typed as v2
- Bypassing version detection logic
- Creating inconsistent annotation state across the codebase
- Breaking when schema migrations are needed

## Implementation Location

The function is defined in `/Users/aaron/src/git-chronicle/src/schema/mod.rs` (lines 17-44) and includes:
- A minimal `SchemaVersion` struct to peek at the version field without full deserialization
- Version-specific deserialization paths for v1 and v2
- Migration logic for v1→v2 conversion
- Error handling for unknown versions and invalid JSON

## Correct Usage Pattern

```rust
use crate::schema::parse_annotation;

let annotation_json = /* read from git note */;
let annotation = parse_annotation(&annotation_json)?;  // Returns v2::Annotation automatically
```
