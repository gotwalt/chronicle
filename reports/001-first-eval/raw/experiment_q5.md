# Q5: Architecture Comprehension - Schema Versioning

## Question
Explain the versioning architecture: How are schema versions organized in the module structure? How is the 'canonical type' pattern implemented? How would a future v3 schema be added — what specific files change and what does NOT change?

## Answer

### 1. Schema Version Organization in Module Structure

The Chronicle schema versioning is organized hierarchically within `/Users/aaron/src/git-chronicle/src/schema/`:

```
src/schema/
├── mod.rs              # Entry point: defines canonical type pattern, parse_annotation() chokepoint
├── common.rs           # Shared types used across ALL versions (AstAnchor, LineRange)
├── correction.rs       # Version-independent correction types
├── v1.rs               # Legacy schema (chronicle/v1)
├── v2.rs               # Current canonical schema (chronicle/v2)
├── migrate.rs          # Migration logic: v1 → v2 conversion
```

**Key organizational principle:** Each schema version gets its own module (`v1.rs`, `v2.rs`), while shared/version-agnostic types live in `common.rs` and `correction.rs`.

### 2. The Canonical Type Pattern

The canonical type pattern is implemented via **re-exports and type aliasing in `mod.rs`**:

```rust
// From mod.rs lines 13-15:
// The canonical annotation type is always the latest version.
pub use v2::Annotation;
pub use v2::*;
```

This achieves three things:

1. **Single exported type**: Any code importing `schema::Annotation` always gets the latest version (currently v2).
2. **Version detection and migration**: The `parse_annotation()` function (lines 23-44) is the **single deserialization chokepoint**:
   - It peeks at the `schema` field to detect version
   - For v1 → automatically migrates via `migrate::v1_to_v2()`
   - For v2 → deserializes directly
   - For unknown versions → returns error
3. **Transparent migration**: All code reads/writes v2, migration happens transparently at deserialization boundary.

From `mod.rs` lines 23-44:
```rust
pub fn parse_annotation(json: &str) -> Result<v2::Annotation, ParseAnnotationError> {
    let peek: SchemaVersion = serde_json::from_str(json)...?;

    match peek.schema.as_str() {
        "chronicle/v2" => { /* direct deserialize */ }
        "chronicle/v1" => {
            let v1_ann: v1::Annotation = serde_json::from_str(json)...?;
            Ok(migrate::v1_to_v2(v1_ann))
        }
        other => Err(ParseAnnotationError::UnknownVersion { ... })
    }
}
```

### 3. How to Add a Future v3 Schema

Adding a v3 schema requires changes to **only 3 files**, with no other codebase changes needed:

#### **FILES THAT CHANGE:**

**1. Create `src/schema/v3.rs`**
   - Define the new `v3::Annotation` struct with new fields/structure
   - Include `schema: "chronicle/v3"` string literal
   - Implement `Serialize`, `Deserialize`, `JsonSchema`
   - Add validation in `impl Annotation { pub fn validate() }`

**2. Update `src/schema/mod.rs`**
   - Add module declaration: `pub mod v3;`
   - Update canonical re-exports (lines 13-15):
     ```rust
     // Update from v2 to v3
     pub use v3::Annotation;
     pub use v3::*;
     ```
   - Extend `parse_annotation()` match statement (lines 30-43):
     ```rust
     match peek.schema.as_str() {
         "chronicle/v3" => { /* deserialize v3 directly */ }
         "chronicle/v2" => { /* v2 → v3 migration */ }
         "chronicle/v1" => {
             let v1_ann: v1::Annotation = ...?;
             Ok(migrate::v1_to_v2(v1_ann)?)
             // OR: migrate directly v1 → v3 if more efficient
         }
         ...
     }
     ```

**3. Update `src/schema/migrate.rs`**
   - Add function `pub fn v2_to_v3(ann: v2::Annotation) -> v3::Annotation`
   - Implement field transformations (similar to `v1_to_v2`)
   - Add comprehensive migration tests
   - Optionally optimize v1 → v3 direct path

#### **FILES THAT DO NOT CHANGE:**

- **All code outside `src/schema/`**: CLI commands, agent loops, storage layers, export/import all continue working because:
  - They import `schema::Annotation` (not `schema::v2::Annotation`)
  - They call `schema::parse_annotation()` (not deserialize directly)
  - The chokepoint handles all version complexity

- **Test fixtures**: Can write v1, v2, and v3 fixtures interchangeably; all migrate to canonical type automatically

- **Git notes storage**: No migration needed; v1/v2 notes stay as-is in git; migration happens on read

### 4. Canonical Type Pattern Benefits

| Aspect | Benefit |
|--------|---------|
| **Single entry point** | All deserialization goes through `parse_annotation()`, impossible to miss migration |
| **Backward compatibility** | Old v1 notes transparently upgrade; readers don't care what version is stored |
| **Zero cost abstraction** | Migration happens once at read time; internal code uses v2 efficiently |
| **Version detection** | Peek-and-dispatch pattern scales to arbitrary schema versions |
| **Type safety** | `schema::Annotation` is always the canonical type; no version confusion |

### 5. Migration Example: v1 → v2

From `migrate.rs`, the v1 → v2 migration shows the pattern:
- **Input**: v1-specific types (`RegionAnnotation`, `Constraint`, `CrossCuttingConcern`)
- **Output**: v2 canonical structure
- **Transformations**:
  - `constraints` → `CodeMarker::Contract`
  - `risk_notes` → `CodeMarker::Hazard`
  - `semantic_dependencies` → `CodeMarker::Dependency`
  - `cross_cutting` → `Decision`
  - `summary` → `Narrative.summary`

Example from `migrate.rs` lines 20-27:
```rust
v2::CodeMarker {
    file: region.file.clone(),
    anchor: Some(region.ast_anchor.clone()),
    lines: Some(region.lines),
    kind: v2::MarkerKind::Contract {
        description: constraint.text.clone(),
        source: match constraint.source { ... }
    },
}
```

## Summary

**Schema versioning is centralized via the canonical type pattern:**

1. **Organization**: Each schema version (v1, v2) gets its own module; shared types in `common.rs`
2. **Canonical pattern**: `parse_annotation()` is the single chokepoint; internal code always uses `schema::Annotation` (→ v2)
3. **Adding v3**: Only modify `src/schema/v3.rs`, `src/schema/mod.rs`, and `src/schema/migrate.rs`; all other code remains unchanged
4. **No versioning in codebase**: External callers don't check schema versions; they call `parse_annotation()` and work with the canonical type
5. **Backward compatible**: Old v1 notes migrate transparently on read; no rewriting needed

This design ensures:
- Old annotations (v1) remain in git notes unchanged
- New annotations use v2 (or v3 later)
- Readers handle any version seamlessly
- Schema evolution is additive (old code can't break on new v3 fields if they're optional)
