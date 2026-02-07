# Q3: Hazard Identification - Manual CodeMarker Construction vs Serde

## Question
In `src/agent/tools.rs`, the function `dispatch_emit_marker` manually constructs `CodeMarker` structs from JSON input rather than using serde deserialization. What is the specific technical reason it cannot use serde, and what maintenance hazard does this create?

## Answer

### The Core Issue: Serde Tagged Enum Representation Mismatch

The `MarkerKind` enum in `src/schema/v2.rs` (line 148) uses Rust's externally-tagged serde format:

```rust
#[serde(rename_all = "snake_case", tag = "type")]
pub enum MarkerKind {
    Contract { description: String, source: ContractSource },
    Hazard { description: String },
    Dependency { target_file: String, target_anchor: String, assumption: String },
    Unstable { description: String, revisit_when: String },
}
```

The `tag = "type"` attribute produces JSON like:
```json
{"type": "contract", "description": "...", "source": "author"}
```

However, the agent's `emit_marker` tool schema (lines 139-181 in tools.rs) defines the input as a **flat object with `kind` as a string discriminator** (not `type`), with kind-specific fields at the top level:

```json
{
  "file": "src/foo.rs",
  "kind": "contract",
  "description": "...",
  "source": "author"
}
```

**This is fundamentally incompatible with serde's `tag = "type"` deserialization.** Serde expects the enum variant tag and all its fields in a single nested structure keyed by the tag field name. Since the agent emits `kind` (not `type`) as a flat discriminator with fields at the top level, direct `serde_json::from_value()` would fail to deserialize the input.

### Why This Design Was Chosen

The agent's flat structure is intentional and appropriate: it simplifies the LLM's mental model. An LLM can naturally emit a top-level `kind` field with contextual fields alongside it, rather than needing to construct a nested `{"type": "...", ...}` object. This keeps the tool contract intuitive and reduces cognitive load on the agent.

### The Maintenance Hazard: Silent Field Loss and Dual Code Paths

The manual construction in `dispatch_emit_marker` (lines 329-447) creates a **critical maintenance hazard**: any future changes to `MarkerKind` enum variants or their fields must be manually reflected in `dispatch_emit_marker`'s match statement, or bugs will silently propagate.

#### Specific hazard patterns:

1. **No exhaustiveness checking on variants**: Adding a new `MarkerKind` variant (e.g., `Warning { description: String }`) would not cause a compile error in `dispatch_emit_marker`. The default case on line 428-433 would silently catch it:
   ```rust
   _ => {
       return Err(AgentError::InvalidAnnotation {
           message: format!("Unknown marker kind: {kind_str}"),
   ```
   The agent could emit valid markers that Chronicle rejects silently.

2. **Field drift via unwrap_or defaults**: The manual extraction uses defensive patterns throughout (lines 339-427):
   ```rust
   .unwrap_or("")           // Returns empty string, not an error
   .unwrap_or("hazard")     // Silent fallback
   Some(AstAnchor {...})    // Silently returns None if any sub-field missing
   ```
   If a field becomes required or its semantics change, `dispatch_emit_marker` won't be forced to update. It will continue backfilling with defaults, producing incomplete markers without error.

3. **Example scenario - ContractSource expansion**: If `ContractSource` enum gains a new variant:
   ```rust
   pub enum ContractSource {
       Author,
       Inferred,
       Derived,  // New variant
   }
   ```
   The match arm at lines 373-376 has a catch-all:
   ```rust
   let source = match input.get("source").and_then(|v| v.as_str()) {
       Some("author") => ContractSource::Author,
       _ => ContractSource::Inferred,  // Silent default
   };
   ```
   The agent can emit `source: "derived"`, but it silently becomes `Inferred`. This creates a data loss bug that's invisible to the manual deserializer.

4. **Dual deserialization code paths**: This violates the single-deserialization-chokepoint principle stated in CLAUDE.md. The live path (`src/annotate/live.rs`) has separate deserialization logic for `MarkerKind`, requiring synchronization across two implementations.

### Files Involved
- `/Users/aaron/src/git-chronicle/src/agent/tools.rs` - Lines 329-447 (dispatch_emit_marker function with manual construction)
- `/Users/aaron/src/git-chronicle/src/schema/v2.rs` - Lines 147-175 (MarkerKind enum with `tag = "type"` serde attribute)
