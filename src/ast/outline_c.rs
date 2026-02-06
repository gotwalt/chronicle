use crate::ast::outline::{
    extract_signature_with_delimiter, node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::error::AstError;

/// Extract an outline from C source code.
pub fn extract_c_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|e| AstError::TreeSitter {
            message: e.to_string(),
            location: snafu::Location::new(file!(), line!(), 0),
        })?;

    let tree = parser.parse(source, None).ok_or(AstError::ParseFailed {
        path: "<input>".to_string(),
        message: "tree-sitter returned None".to_string(),
        location: snafu::Location::new(file!(), line!(), 0),
    })?;

    let mut entries = Vec::new();
    let bytes = source.as_bytes();
    walk_c_node(tree.root_node(), bytes, &mut entries);
    Ok(entries)
}

pub(crate) fn walk_c_node(
    node: tree_sitter::Node,
    source: &[u8],
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if should_skip_node(&child) {
            continue;
        }
        match child.kind() {
            "function_definition" => {
                if let Some(entry) = extract_c_function(child, source) {
                    entries.push(entry);
                }
            }
            "struct_specifier" => {
                if has_body(&child) {
                    if let Some(entry) = extract_c_tagged(child, source, SemanticKind::Struct) {
                        entries.push(entry);
                    }
                }
            }
            "enum_specifier" => {
                if has_body(&child) {
                    if let Some(entry) = extract_c_tagged(child, source, SemanticKind::Enum) {
                        entries.push(entry);
                    }
                }
            }
            "union_specifier" => {
                if has_body(&child) {
                    if let Some(entry) = extract_c_tagged(child, source, SemanticKind::Struct) {
                        entries.push(entry);
                    }
                }
            }
            "type_definition" => {
                if let Some(entry) = extract_c_typedef(child, source) {
                    entries.push(entry);
                }
            }
            _ => {}
        }
    }
}

fn extract_c_function(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_declarator_name(declarator, source)?;
    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind: SemanticKind::Function,
        name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    })
}

/// Walk the declarator chain to find the function/variable name.
/// C declarators can be nested: function_declarator -> pointer_declarator -> identifier
pub(crate) fn extract_declarator_name(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" | "field_identifier" => {
            node.utf8_text(source).ok().map(String::from)
        }
        "function_declarator" | "pointer_declarator" | "parenthesized_declarator"
        | "array_declarator" => {
            // Try the "declarator" field first, then fall back to first named child
            if let Some(inner) = node.child_by_field_name("declarator") {
                extract_declarator_name(inner, source)
            } else {
                // For function_declarator, the name might be the first child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Some(name) = extract_declarator_name(child, source) {
                        return Some(name);
                    }
                }
                None
            }
        }
        _ => None,
    }
}

fn extract_c_tagged(
    node: tree_sitter::Node,
    source: &[u8],
    kind: SemanticKind,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind,
        name: name.to_string(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    })
}

fn extract_c_typedef(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_declarator_name(declarator, source)?;
    let full_text = node.utf8_text(source).unwrap_or("").trim().to_string();
    Some(OutlineEntry {
        kind: SemanticKind::TypeAlias,
        name,
        signature: Some(full_text),
        lines: node_line_range(node),
        parent: None,
    })
}

fn has_body(node: &tree_sitter::Node) -> bool {
    node.child_by_field_name("body").is_some()
}
