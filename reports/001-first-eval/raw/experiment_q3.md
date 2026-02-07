# Q3 - Hazard Identification: Manual JSON Construction in `dispatch_emit_marker`

## Question
In `src/agent/tools.rs`, the function `dispatch_emit_marker` manually constructs `CodeMarker` structs from JSON input rather than using serde deserialization. What is the specific technical reason it cannot use serde, and what maintenance hazard does this create?

## Answer

### Technical Reason: Serde Tag Discriminator Format Mismatch

The function cannot use direct serde deserialization due to **serialization format incompatibility** in the `MarkerKind` enum.

The `MarkerKind` enum in `src/schema/v2.rs` (lines 147-168) uses serde's **internally tagged** discriminator:
```rust
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MarkerKind {
    Contract { description: String, source: ContractSource },
    Hazard { description: String },
    Dependency { target_file: String, target_anchor: String, assumption: String },
    Unstable { description: String, revisit_when: String },
}
```

The `tag = "type"` attribute means that serialized JSON looks like:
```json
{ "type": "contract", "description": "...", "source": "author" }
{ "type": "hazard", "description": "..." }
{ "type": "dependency", "target_file": "...", ... }
```

However, **the agent emits a flat JSON object with `kind` as a string discriminator** (not `type`), and kind-specific fields at the top level. The tool schema defined in `tool_definitions()` (lines 139-181) shows the agent sends:
```json
{
  "file": "src/foo.rs",
  "kind": "contract",        // <- string discriminator, not "type"
  "description": "...",      // <- kind-specific field at top level
  "source": "author"
}
```

This format **cannot be directly deserialized** by serde because:
1. The agent uses `"kind"` as the field name, not `"type"`
2. The agent places kind-specific fields like `description`, `source`, etc. at the top level alongside `kind`, rather than nested within the type discriminator

### Manual Construction Approach (Lines 329-447)

To bridge this format gap, `dispatch_emit_marker` manually parses the JSON:
1. Extracts `kind_str` from input (line 344-347)
2. Uses a `match` statement to branch by kind (lines 366-434)
3. For each branch, manually constructs the appropriate `MarkerKind` variant by extracting its specific fields

Example (lines 366-381):
```rust
let marker_kind = match kind_str {
    "contract" => {
        let description = input.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let source = match input.get("source").and_then(|v| v.as_str()) {
            Some("author") => ContractSource::Author,
            _ => ContractSource::Inferred,
        };
        MarkerKind::Contract { description, source }
    }
    // ... other variants
};
```

### Maintenance Hazard

This creates a **brittle, duplicate maintenance burden**:

1. **Schema Drift**: If `MarkerKind` enum variants change (fields added/removed/renamed), the manual construction logic in `dispatch_emit_marker` must be manually updated in parallel. Serde would catch schema mismatches at runtime; this code will silently accept incomplete or malformed input.

2. **Two Sources of Truth**: The contract of what fields each marker kind requires is defined in two places:
   - The `MarkerKind` enum definition (canonical)
   - The manual extraction and match logic in `dispatch_emit_marker` (must stay in sync)

3. **Silent Failures**: Using `.unwrap_or("")` (lines 371, 385-386, 393-404, 413-421) means missing required fields silently become empty strings. An agent that forgets to include a `description` field will not error; it will silently produce a marker with an empty description. Serde would error on deserialization failure.

4. **Inconsistency with Other Emit Functions**: `dispatch_emit_narrative` (line 312) and `dispatch_emit_decision` (line 321) both use `serde_json::from_value()` directly, making the codebase inconsistent. Future maintainers may not understand why `dispatch_emit_marker` is different.

5. **Type Conversion Fragility**: Manual `.as_str()` and `.as_u64()` chains (lines 350-364) can fail silently with `.and_then()`, returning `None` rather than clear error messages about type mismatches.

### Code Location
- **Implementation**: `/Users/aaron/src/git-chronicle/src/agent/tools.rs`, lines 329-447
- **Schema Definition**: `/Users/aaron/src/git-chronicle/src/schema/v2.rs`, lines 147-168
- **Tool Definition**: `/Users/aaron/src/git-chronicle/src/agent/tools.rs`, lines 139-181
