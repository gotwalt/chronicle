# Q5: Schema Versioning Architecture

## Answer

### How are schema versions organized in the module structure?

The schema versioning architecture uses a **modular, version-aware organization** in `/src/schema/`:

**Directory structure:**
```
src/schema/
├── mod.rs          # Entry point & canonical type alias
├── v1.rs           # Version 1 types (old format)
├── v2.rs           # Version 2 types (current canonical)
├── common.rs       # Shared types across all versions
├── correction.rs   # Version-independent correction types
└── migrate.rs      # Migration functions (v1 -> v2)
```

**Module organization principle:**
- Each major schema version gets its own file (`v1.rs`, `v2.rs`, etc.)
- Shared types used by multiple versions live in `common.rs` (e.g., `AstAnchor`, `LineRange`)
- Version-independent types live in `correction.rs`
- Migrations are centralized in `migrate.rs`
- The entry point `mod.rs` orchestrates version detection and re-exports

### How is the 'canonical type' pattern implemented?

The canonical type pattern is implemented through **strategic re-exports in `mod.rs`:**

```rust
// mod.rs lines 1-15:
pub mod v1;
pub mod v2;

// Shared types
pub use common::{AstAnchor, LineRange};
pub use correction::*;

// THE CANONICAL TYPE IS ALWAYS THE LATEST VERSION
pub use v2::Annotation;
pub use v2::*;  // Re-export all v2 types
```

**Key aspects:**

1. **Single deserialization chokepoint** via `parse_annotation()`:
   ```rust
   pub fn parse_annotation(json: &str) -> Result<v2::Annotation, ParseAnnotationError>
   ```
   - Peeks at the `schema` field to detect version
   - For v2: deserializes directly to `v2::Annotation`
   - For v1: deserializes to `v1::Annotation`, then migrates via `migrate::v1_to_v2()`
   - Unknown versions are rejected with `UnknownVersion` error

2. **Type alias pattern**: All internal code uses `schema::Annotation`, which is aliased to `v2::Annotation` via the re-export. This ensures all code operates on the canonical v2 type without explicit versioning.

3. **Automatic migration on read**: When old v1 notes are deserialized, they are transparently converted to v2 in `parse_annotation()`, so all downstream code never sees v1 types.

### How would a future v3 schema be added — what specific files change and what does NOT change?

To add v3, these **4 files WOULD change**:

1. **Create `/src/schema/v3.rs`** (NEW FILE)
   - Define all v3 types (the new schema structure)
   - Implement `validate()` methods
   - Use `#[derive(Serialize, Deserialize, JsonSchema)]`
   - Example structure:
     ```rust
     pub struct Annotation {
         pub schema: String,  // "chronicle/v3"
         // ... v3-specific fields
     }
     ```

2. **Update `/src/schema/mod.rs`**
   - Add module declaration: `pub mod v3;`
   - Move canonical re-export from v2 to v3:
     ```rust
     // CHANGE FROM:
     pub use v2::Annotation;
     pub use v2::*;

     // TO:
     pub use v3::Annotation;
     pub use v3::*;
     ```
   - Update `parse_annotation()` to handle v3:
     ```rust
     match peek.schema.as_str() {
         "chronicle/v3" => { /* deserialize directly */ }
         "chronicle/v2" => { /* deserialize, then migrate v2_to_v3 */ }
         "chronicle/v1" => { /* deserialize, then migrate v1_to_v2_to_v3 */ }
         // ...
     }
     ```

3. **Update `/src/schema/migrate.rs`**
   - Add `v2_to_v3()` function following the same pattern as `v1_to_v2()`:
     ```rust
     pub fn v2_to_v3(ann: v2::Annotation) -> v3::Annotation {
         // Transform v2 fields to v3 structure
         // Update ProvenanceSource to include MigratedV2
     }
     ```
   - Optionally chain migrations if v1 -> v2 -> v3 is needed

4. **Update `/src/schema/migrate.rs` tests**
   - Add tests like `test_v2_to_v3_*` mirroring the v1->v2 test patterns
   - Ensure roundtrip preservation of data

### Files that do NOT change:

- **`/src/schema/common.rs`** - Remains unchanged (shared types used by all versions)
- **`/src/schema/correction.rs`** - Remains unchanged (version-independent)
- **`/src/schema/v1.rs`** - Remains unchanged (frozen schema)
- **`/src/schema/v2.rs`** - Remains unchanged (now a legacy version)
- **All non-schema code** - All code using `schema::Annotation` automatically uses v3 via the canonical re-export. No changes needed in:
  - `src/cli/`
  - `src/annotate/`
  - `src/read/`
  - `src/export.rs`
  - `src/import.rs`
  - `src/agent/`
  - Test fixtures and integration tests

### Migration Strategy

The architecture follows a **chained migration pattern**:

- **v1 JSON** → (deserialize as v1::Annotation) → (migrate::v1_to_v2) → v2::Annotation → (migrate::v2_to_v3) → **v3::Annotation**
- Each migration function is isolated and testable
- The `parse_annotation()` function handles all version detection and orchestration
- Data loss/transformation is explicit and tested (e.g., v1's regions → v2's markers with specific type conversions)

### Design Rationale

This architecture achieves:

1. **Backward compatibility** - Old v1 annotations are automatically migrated on read
2. **Forward transparency** - All code uses `schema::Annotation` (canonical type); adding v3 doesn't require code changes outside schema module
3. **Single responsibility** - Each version owns its types, each migration function owns its transformation
4. **Testability** - Migrations are pure functions tested independently
5. **Extensibility** - Adding v3 (or v4, v5, ...) follows the same mechanical pattern: new file, update mod.rs and migrate.rs, add tests
