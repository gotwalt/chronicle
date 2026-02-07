use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// AST anchor identifying a code element.
/// Shared across all schema versions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AstAnchor {
    pub unit_type: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// A range of line numbers in a file.
/// Shared across all schema versions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct LineRange {
    pub start: u32,
    pub end: u32,
}
