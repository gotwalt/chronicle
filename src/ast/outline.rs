use crate::error::AstError;
use crate::schema::LineRange;

/// What kind of semantic unit this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Impl,
    Const,
    Static,
    TypeAlias,
    Module,
}

impl SemanticKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SemanticKind::Function => "function",
            SemanticKind::Method => "method",
            SemanticKind::Struct => "struct",
            SemanticKind::Enum => "enum",
            SemanticKind::Trait => "trait",
            SemanticKind::Impl => "impl",
            SemanticKind::Const => "const",
            SemanticKind::Static => "static",
            SemanticKind::TypeAlias => "type_alias",
            SemanticKind::Module => "module",
        }
    }

    /// Parse a unit_type string into a SemanticKind (for anchor matching).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s {
            "function" | "fn" => Some(SemanticKind::Function),
            "method" => Some(SemanticKind::Method),
            "struct" => Some(SemanticKind::Struct),
            "enum" => Some(SemanticKind::Enum),
            "trait" => Some(SemanticKind::Trait),
            "impl" => Some(SemanticKind::Impl),
            "const" => Some(SemanticKind::Const),
            "static" => Some(SemanticKind::Static),
            "type_alias" | "type" => Some(SemanticKind::TypeAlias),
            "module" | "mod" => Some(SemanticKind::Module),
            _ => None,
        }
    }
}

/// A semantic unit extracted from source code via tree-sitter.
#[derive(Debug, Clone)]
pub struct OutlineEntry {
    pub kind: SemanticKind,
    /// Qualified name, e.g. "MyStruct::my_method"
    pub name: String,
    /// The function/method signature (if applicable).
    pub signature: Option<String>,
    pub lines: LineRange,
    /// Parent entry name for nested items (e.g. impl block for methods).
    pub parent: Option<String>,
}

/// Extract an outline of semantic units from Rust source code using tree-sitter.
pub fn extract_rust_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|e| AstError::TreeSitter {
            message: e.to_string(),
            location: snafu::Location::new(file!(), line!(), 0),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or(AstError::ParseFailed {
            path: "<input>".to_string(),
            message: "tree-sitter returned None".to_string(),
            location: snafu::Location::new(file!(), line!(), 0),
        })?;

    let mut entries = Vec::new();
    let root = tree.root_node();
    let bytes = source.as_bytes();

    walk_rust_node(root, bytes, None, &mut entries);

    Ok(entries)
}

fn walk_rust_node(
    node: tree_sitter::Node,
    source: &[u8],
    impl_type_name: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_item" => {
                if let Some(entry) = extract_function(child, source, impl_type_name) {
                    entries.push(entry);
                }
            }
            "struct_item" => {
                if let Some(entry) = extract_named_item(child, source, SemanticKind::Struct) {
                    entries.push(entry);
                }
            }
            "enum_item" => {
                if let Some(entry) = extract_named_item(child, source, SemanticKind::Enum) {
                    entries.push(entry);
                }
            }
            "trait_item" => {
                if let Some(entry) = extract_named_item(child, source, SemanticKind::Trait) {
                    entries.push(entry);
                }
            }
            "impl_item" => {
                extract_impl(child, source, entries);
            }
            _ => {}
        }
    }
}

fn extract_function(
    node: tree_sitter::Node,
    source: &[u8],
    impl_type_name: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let fn_name = name_node.utf8_text(source).ok()?;

    let (kind, qualified_name, parent) = if let Some(type_name) = impl_type_name {
        (
            SemanticKind::Method,
            format!("{}::{}", type_name, fn_name),
            Some(type_name.to_string()),
        )
    } else {
        (SemanticKind::Function, fn_name.to_string(), None)
    };

    let signature = extract_signature(node, source);

    Some(OutlineEntry {
        kind,
        name: qualified_name,
        signature: Some(signature),
        lines: LineRange {
            start: node.start_position().row as u32 + 1,
            end: node.end_position().row as u32 + 1,
        },
        parent,
    })
}

fn extract_named_item(
    node: tree_sitter::Node,
    source: &[u8],
    kind: SemanticKind,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;

    let signature = extract_signature(node, source);

    Some(OutlineEntry {
        kind,
        name: name.to_string(),
        signature: Some(signature),
        lines: LineRange {
            start: node.start_position().row as u32 + 1,
            end: node.end_position().row as u32 + 1,
        },
        parent: None,
    })
}

fn extract_impl(
    node: tree_sitter::Node,
    source: &[u8],
    entries: &mut Vec<OutlineEntry>,
) {
    // Find the type name for the impl block.
    // impl blocks have a "type" field for the type being implemented.
    let type_node = node.child_by_field_name("type");
    let type_name = type_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<unknown>");

    let signature = extract_signature(node, source);

    entries.push(OutlineEntry {
        kind: SemanticKind::Impl,
        name: type_name.to_string(),
        signature: Some(signature),
        lines: LineRange {
            start: node.start_position().row as u32 + 1,
            end: node.end_position().row as u32 + 1,
        },
        parent: None,
    });

    // Descend into the impl body to find methods
    if let Some(body) = node.child_by_field_name("body") {
        walk_rust_node(body, source, Some(type_name), entries);
    }
}

/// Extract the signature: text from the start of the node up to (but not including)
/// the opening `{`.
fn extract_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let full_text = node.utf8_text(source).unwrap_or("");
    if let Some(brace_pos) = full_text.find('{') {
        full_text[..brace_pos].trim().to_string()
    } else {
        // For items without a body (e.g. unit structs), use the full text
        full_text.trim().to_string()
    }
}
