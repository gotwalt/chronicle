use crate::ast::outline::{
    extract_signature_with_delimiter, node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::error::AstError;

/// Extract an outline from Go source code.
pub fn extract_go_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
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
    walk_go_node(tree.root_node(), bytes, &mut entries);
    Ok(entries)
}

fn walk_go_node(
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
            "function_declaration" => {
                if let Some(entry) = extract_go_function(child, source) {
                    entries.push(entry);
                }
            }
            "method_declaration" => {
                if let Some(entry) = extract_go_method(child, source) {
                    entries.push(entry);
                }
            }
            "type_declaration" => {
                extract_go_type_declaration(child, source, entries);
            }
            "const_declaration" => {
                extract_go_const_declaration(child, source, entries);
            }
            _ => {}
        }
    }
}

fn extract_go_function(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind: SemanticKind::Function,
        name: name.to_string(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    })
}

fn extract_go_method(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let method_name = name_node.utf8_text(source).ok()?;

    // Extract receiver type from the receiver parameter
    let receiver_type = node
        .child_by_field_name("receiver")
        .and_then(|r| extract_receiver_type(r, source))
        .unwrap_or_else(|| "<unknown>".to_string());

    let qualified_name = format!("{}::{}", receiver_type, method_name);
    let signature = extract_signature_with_delimiter(node, source, '{');

    Some(OutlineEntry {
        kind: SemanticKind::Method,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent: Some(receiver_type),
    })
}

fn extract_receiver_type(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    // Receiver is a parameter_list containing a parameter_declaration.
    // The type may be a pointer_type (*Server) or a plain type_identifier (Server).
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "parameter_declaration" {
            if let Some(type_node) = child.child_by_field_name("type") {
                let type_text = type_node.utf8_text(source).ok()?;
                // Strip the pointer prefix
                return Some(type_text.trim_start_matches('*').to_string());
            }
        }
    }
    None
}

fn extract_go_type_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_spec" {
            if let Some(entry) = extract_go_type_spec(child, source) {
                entries.push(entry);
            }
        }
    }
}

fn extract_go_type_spec(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;

    let type_node = node.child_by_field_name("type")?;
    let kind = match type_node.kind() {
        "struct_type" => SemanticKind::Struct,
        "interface_type" => SemanticKind::Interface,
        _ => SemanticKind::TypeAlias,
    };

    let signature = node.utf8_text(source).unwrap_or("").trim().to_string();
    // For struct/interface, truncate at the opening brace
    let sig = if let Some(pos) = signature.find('{') {
        signature[..pos].trim().to_string()
    } else {
        signature
    };

    Some(OutlineEntry {
        kind,
        name: name.to_string(),
        signature: Some(sig),
        lines: node_line_range(node),
        parent: None,
    })
}

fn extract_go_const_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const_spec" {
            if let Some(entry) = extract_go_const_spec(child, source) {
                entries.push(entry);
            }
        }
    }
}

fn extract_go_const_spec(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let signature = node.utf8_text(source).unwrap_or("").trim().to_string();
    Some(OutlineEntry {
        kind: SemanticKind::Const,
        name: name.to_string(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    })
}
