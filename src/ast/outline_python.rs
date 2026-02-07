use crate::ast::outline::{node_line_range, should_skip_node, OutlineEntry, SemanticKind};
use crate::error::AstError;

/// Extract an outline from Python source code.
pub fn extract_python_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
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
    walk_python_node(tree.root_node(), bytes, None, &mut entries);
    Ok(entries)
}

fn walk_python_node(
    node: tree_sitter::Node,
    source: &[u8],
    class_name: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if should_skip_node(&child) {
            continue;
        }
        match child.kind() {
            "function_definition" => {
                if let Some(entry) = extract_python_function(child, source, class_name) {
                    entries.push(entry);
                }
            }
            "class_definition" => {
                extract_python_class(child, source, entries);
            }
            "decorated_definition" => {
                // Transparent: use outer line range but extract inner definition
                extract_decorated(child, source, class_name, entries);
            }
            _ => {}
        }
    }
}

fn extract_python_function(
    node: tree_sitter::Node,
    source: &[u8],
    class_name: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let fn_name = name_node.utf8_text(source).ok()?;

    let (kind, qualified_name, parent) = match class_name {
        Some(cn) => {
            let k = if fn_name == "__init__" {
                SemanticKind::Constructor
            } else {
                SemanticKind::Method
            };
            (k, format!("{}::{}", cn, fn_name), Some(cn.to_string()))
        }
        None => (SemanticKind::Function, fn_name.to_string(), None),
    };

    let signature = extract_python_signature(node, source);
    Some(OutlineEntry {
        kind,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent,
    })
}

fn extract_python_class(node: tree_sitter::Node, source: &[u8], entries: &mut Vec<OutlineEntry>) {
    let name_node = node.child_by_field_name("name");
    let class_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let signature = extract_python_signature(node, source);
    entries.push(OutlineEntry {
        kind: SemanticKind::Class,
        name: class_name.to_string(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    });

    // Descend into class body for methods
    if let Some(body) = node.child_by_field_name("body") {
        walk_python_node(body, source, Some(class_name), entries);
    }
}

fn extract_decorated(
    node: tree_sitter::Node,
    source: &[u8],
    class_name: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    // Find the inner definition node
    let outer_lines = node_line_range(node);
    if let Some(definition) = node.child_by_field_name("definition") {
        match definition.kind() {
            "function_definition" => {
                if let Some(mut entry) = extract_python_function(definition, source, class_name) {
                    // Use the outer (decorated) line range
                    entry.lines = outer_lines;
                    entries.push(entry);
                }
            }
            "class_definition" => {
                // Extract class with outer line range
                let name_node = definition.child_by_field_name("name");
                let cls_name = name_node
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("<anonymous>");
                let signature = extract_python_signature(definition, source);
                entries.push(OutlineEntry {
                    kind: SemanticKind::Class,
                    name: cls_name.to_string(),
                    signature: Some(signature),
                    lines: outer_lines,
                    parent: None,
                });
                if let Some(body) = definition.child_by_field_name("body") {
                    walk_python_node(body, source, Some(cls_name), entries);
                }
            }
            _ => {}
        }
    }
}

/// Python signature: text from node start to the colon before the body.
fn extract_python_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let full_text = node.utf8_text(source).unwrap_or("");
    // Find the body node to determine where the signature ends
    if let Some(body) = node.child_by_field_name("body") {
        let body_offset = body.start_byte() - node.start_byte();
        if body_offset > 0 && body_offset <= full_text.len() {
            let before_body = &full_text[..body_offset];
            // Trim trailing colon and whitespace
            return before_body.trim().trim_end_matches(':').trim().to_string();
        }
    }
    // Fallback: up to first colon
    if let Some(pos) = full_text.find(':') {
        full_text[..pos].trim().to_string()
    } else {
        full_text.trim().to_string()
    }
}
