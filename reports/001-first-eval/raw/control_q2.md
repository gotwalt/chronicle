# V1 to V2 Annotation Schema Migration Strategy

## The Strategy: Lazy Migration on Read

The codebase uses a **lazy migration strategy** for handling old v1 notes. Rather than performing a bulk rewrite of existing git notes when the v2 schema was introduced, all v1 annotations remain as-is in git, and the system transparently migrates them to v2 format only when they are read.

## Key Approach

**Single Deserialization Chokepoint**: `parse_annotation()`

The function `parse_annotation(json: &str) -> Result<v2::Annotation, ParseAnnotationError>` in `/Users/aaron/src/git-chronicle/src/schema/mod.rs` is the sole entry point for deserializing annotations from JSON. This function:

1. **Peeks at the schema version** by first parsing the JSON into a minimal `SchemaVersion` struct to extract the `schema` field
2. **Routes to the appropriate deserializer** based on the detected version:
   - For `"chronicle/v2"`: deserializes directly using `serde_json::from_str::<v2::Annotation>`
   - For `"chronicle/v1"`: deserializes to `v1::Annotation`, then calls `migrate::v1_to_v2()` to convert it
   - For unknown versions: returns `ParseAnnotationError::UnknownVersion`
3. **Always returns the canonical type** (`v2::Annotation`)

As documented in CLAUDE.md: "Single deserialization chokepoint: `schema::parse_annotation(json) -> Result<Annotation>` detects version and migrates. Never deserialize annotations directly with `serde_json::from_str`."

## Role of `parse_annotation()` in the Architecture

`parse_annotation()` serves multiple critical functions:

### 1. Version Detection and Routing
It examines the `schema` field in the JSON to determine which deserialization path to take. This design allows the system to support multiple schema versions simultaneously without requiring all code to handle version branching.

### 2. Transparent Migration
For v1 notes, it automatically calls the migration function:
```rust
match peek.schema.as_str() {
    "chronicle/v1" => {
        let v1_ann: v1::Annotation = serde_json::from_str(json)?;
        Ok(migrate::v1_to_v2(v1_ann))
    }
    ...
}
```

### 3. Canonical Type Contract
By always returning `v2::Annotation`, it ensures all internal code operates on the latest schema. The type alias `pub use v2::Annotation;` in `schema/mod.rs` makes v2 the default throughout the codebase.

## Why This Strategy Was Chosen

The feature document (features/20-schema-v2.md) explicitly states the migration philosophy:

> **Migration strategy: Lazy**
> - All writes produce the latest version (v2)
> - All reads parse any version and migrate to canonical on the fly
> - No bulk rewrite needed. Old v1 notes stay as v1 in git.

This approach offers several advantages:

1. **Non-disruptive**: Existing git notes don't need to be rewritten, avoiding large history-altering operations
2. **Backwards compatible**: Old v1 annotations continue to work seamlessly
3. **Forward-compatible**: The architecture supports future versions (v3, v4, etc.) through the same `parse_annotation()` mechanism
4. **Chainable migrations**: The system is designed so migrations can chain (v1→v2→v3), allowing multiple schema iterations
5. **Minimal runtime cost**: Migration happens only when an annotation is actually read, not upfront

## What `parse_annotation()` Handles

The migration via `parse_annotation()` transforms v1 structure into v2 structure through `migrate::v1_to_v2()`:

- **v1 Constraints** → **v2 Contract markers**: Converts constraint text and source
- **v1 Risk notes** → **v2 Hazard markers**: Wraps risk descriptions as hazard markers
- **v1 Semantic dependencies** → **v2 Dependency markers**: Preserves dependency relationships in typed markers
- **v1 Cross-cutting concerns** → **v2 Decisions**: Converts cross-cutting concerns to decision records with scope
- **v1 Summary** → **v2 Narrative.summary**: Moves the main summary to the narrative
- **v1 Task** → **v2 EffortLink**: Converts task identifiers to effort links
- **v1 Regions.file** → **v2 Narrative.files_changed**: Extracts and deduplicates touched files
- **Provenance tracking**: Sets `provenance.source` to `MigratedV1` to mark that this annotation came from a v1 note

The provenance tracking is important: it allows downstream systems to know that an annotation was migrated from v1, distinguishing it from native v2 annotations created via the live or batch paths.

## Where `parse_annotation()` Is Used

The function is called at every point where annotations are read from git notes:

1. **Read queries** (`src/read/contracts.rs`, `src/read/decisions.rs`, `src/read/summary.rs`): Query functions fetch notes and call `parse_annotation()` to obtain v2 annotations for filtering and aggregating results
2. **Import validation** (`src/import.rs`): When importing annotations from JSONL, each annotation is validated by calling `parse_annotation()` to ensure it's either valid v1 or v2
3. **Tests**: Both unit tests and integration tests use `parse_annotation()` to verify the migration path works correctly

## Design for Future Versions

The architecture anticipates multiple schema versions through:

- Module organization: Each version has its own module (v1.rs, v2.rs)
- Single migration entry point: `parse_annotation()` remains the chokepoint
- Chainable migrations: `migrate.rs` can implement `v2_to_v3()` if needed, and calling `parse_annotation()` would handle routing
- Comprehensive test coverage: Tests in `schema/mod.rs` cover both v1→v2 migration and native v2 parsing

This design means that when v3 is introduced, the same `parse_annotation()` mechanism will handle detection, routing, and transparent migration without requiring changes throughout the codebase.
