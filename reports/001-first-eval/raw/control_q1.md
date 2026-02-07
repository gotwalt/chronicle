# Q1 - Contract Awareness: Annotation JSON Deserialization Rule

## The Mandatory Function

**Function that MUST be used:** `schema::parse_annotation(json: &str) -> Result<Annotation, ParseAnnotationError>`

Located in: `/Users/aaron/src/git-chronicle/src/schema/mod.rs` (lines 17-44)

## What Must NOT Be Done Instead

**FORBIDDEN:** Direct deserialization using `serde_json::from_str`

Code comment explicitly states (lines 20-22):
```
This is the single deserialization chokepoint. All code that reads
annotations from git notes should call this instead of using
`serde_json::from_str` directly.
```

## Why This Constraint Exists

The constraint exists for **schema version detection and automatic migration**. The `parse_annotation()` function implements a critical multi-step process:

1. **Version Detection (lines 25-28):** It first "peeks" at the JSON to extract the `schema` field, examining only the version identifier without full deserialization.

2. **Version-Specific Handling (lines 30-43):**
   - If schema is `"chronicle/v2"`: Deserializes directly as `v2::Annotation`
   - If schema is `"chronicle/v1"`: Deserializes as `v1::Annotation`, then calls `migrate::v1_to_v2()` to automatically convert to canonical v2 format
   - If schema is unknown: Returns `UnknownVersion` error

3. **Single Canonical Type (line 14):** The codebase maintains a single canonical annotation type through type aliasing:
   ```rust
   pub use v2::Annotation;
   ```

This ensures that regardless of the stored schema version, all internal code consistently operates on `schema::Annotation` (v2), with old v1 annotations transparently migrated on read. Bypassing this function would break backward compatibility and create inconsistencies where some code sees v1 types and other code sees v2 types.

## Source Code Reference

**File:** `/Users/aaron/src/git-chronicle/src/schema/mod.rs`

**Key Function (lines 17-44):**
```rust
pub fn parse_annotation(json: &str) -> Result<v2::Annotation, ParseAnnotationError> {
    // Peek at the schema field to determine version.
    let peek: SchemaVersion =
        serde_json::from_str(json).map_err(|e| ParseAnnotationError::InvalidJson {
            source: e,
        })?;

    match peek.schema.as_str() {
        "chronicle/v2" => {
            serde_json::from_str::<v2::Annotation>(json)
                .map_err(|e| ParseAnnotationError::InvalidJson { source: e })
        }
        "chronicle/v1" => {
            let v1_ann: v1::Annotation = serde_json::from_str(json)
                .map_err(|e| ParseAnnotationError::InvalidJson { source: e })?;
            Ok(migrate::v1_to_v2(v1_ann))
        }
        other => Err(ParseAnnotationError::UnknownVersion {
            version: other.to_string(),
        }),
    }
}
```
