# V1 to V2 Annotation Format Migration Strategy

## Strategy: Lazy On-Read Migration

The git-chronicle codebase uses a **lazy migration strategy** for handling the coexistence of `chronicle/v1` and `chronicle/v2` annotation formats. This means:

- **No bulk rewrite**: Old v1 notes remain stored as v1 in git notes
- **On-read translation**: When a v1 annotation is read, it is automatically migrated to the canonical v2 format in memory
- **Forward-writing only**: All new annotations are written in v2 format

This design choice allows the codebase to upgrade to v2 without disrupting existing repositories or requiring a migration step.

## The Approach

### Single Deserialization Chokepoint: `parse_annotation()`

The core mechanism is the `parse_annotation()` function in `src/schema/mod.rs` (lines 23-44), which serves as the **single deserialization chokepoint** for all annotation reads across the codebase:

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

This function:

1. **Peeks at the schema field** to determine the version without full deserialization
2. **Routes to the appropriate deserializer** (v1 or v2)
3. **Migrates v1 to v2** using `migrate::v1_to_v2()` before returning
4. **Always returns the canonical v2::Annotation type** regardless of input format

### The Migration Function: `v1_to_v2()`

The `src/schema/migrate.rs` module implements the detailed transformation from v1 to v2 schema. Key transformations include:

- **Constraints → Contracts**: v1 region constraints become `MarkerKind::Contract` markers in v2
- **Risk Notes → Hazards**: v1 risk notes become `MarkerKind::Hazard` markers
- **Semantic Dependencies → Dependencies**: v1 semantic dependencies become `MarkerKind::Dependency` markers
- **Cross-Cutting Concerns → Decisions**: v1 cross-cutting concerns map to v2 decisions
- **Task → Effort Link**: v1 task field becomes an optional `EffortLink` in v2
- **Provenance Marking**: The migrated annotation is marked with `ProvenanceSource::MigratedV1`

The migration preserves all semantic content while reorganizing it from a **per-region** (v1) to a **commit-level narrative with optional code markers** (v2) structure.

## Why This Strategy Was Chosen

The feature design document (features/20-schema-v2.md) explicitly documents this rationale:

> "Migration strategy: Lazy
> - All writes produce the latest version (v2)
> - All reads parse any version and migrate to canonical on the fly
> - No bulk rewrite needed. Old v1 notes stay as v1 in git."

### Benefits of Lazy Migration

1. **Backwards Compatibility**: Existing repositories with v1 notes continue to work without modification
2. **No Repository Disruption**: No need to run a bulk migration step or rewrite git history
3. **Seamless Coexistence**: v1 and v2 annotations can coexist during the transition period
4. **Gradual Adoption**: New commits annotated after the upgrade produce v2, creating a natural transition
5. **Infrastructure Readiness**: The pattern is designed to scale to v3, v4, etc. by chaining migrations (v1→v2→v3)

## Role of `parse_annotation()` in the Architecture

`parse_annotation()` is the **single enforced chokepoint** for all annotation deserialization. This is explicitly stated in the CLAUDE.md documentation:

> "Single deserialization chokepoint: `schema::parse_annotation(json) -> Result<Annotation>` detects version and migrates. Never deserialize annotations directly with `serde_json::from_str`."

### Usage Across the Codebase

The function is called in all annotation-reading paths:

- **Import validation** (`src/import.rs`): Validates annotations before import, handling both v1 and v2
- **Contracts query** (`src/read/contracts.rs`): Reads and migrates annotations to extract contract markers
- **Summary query** (`src/read/summary.rs`): Migrates annotations to build condensed summaries of code intent
- **Decisions query** (`src/read/decisions.rs`): Migrates to extract decision records
- **Export/Import** (`src/export.rs`): Preserves original format during export, validates during import

### Canonical Type Alias

The `schema::Annotation` type alias (in `src/schema/mod.rs`) always refers to `v2::Annotation`:

```rust
pub use v2::Annotation;
pub use v2::*;
```

All internal code uses `schema::Annotation` or explicitly `v2::Annotation`, ensuring the codebase operates on the canonical format regardless of storage format.

## Verification Through Tests

The migration is thoroughly tested in `src/schema/migrate.rs` with test cases demonstrating:

- Constraint to contract conversion
- Risk notes to hazard conversion
- Semantic dependencies to dependency markers
- Cross-cutting concerns to decisions
- Task to effort link conversion
- Provenance source marking
- Preservation of data through migration

Additionally, `src/schema/mod.rs` includes integration tests like `test_v1_roundtrip_preserves_data()` that verify v1 annotations can be parsed, migrated, and all semantic content is preserved.

## Conclusion

The codebase implements a **lazy, transparent migration strategy** where `parse_annotation()` acts as the single point of version detection and migration. Old v1 notes remain unmutated in git, while all code operates on the canonical v2 format. New writes always produce v2. This design balances backwards compatibility with forward momentum, enabling repositories to upgrade gradually without operational disruption.
