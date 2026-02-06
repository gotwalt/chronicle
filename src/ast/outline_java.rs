use crate::ast::outline::{
    extract_signature_with_delimiter, node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::error::AstError;

/// Extract an outline from Java source code.
pub fn extract_java_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
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
    walk_java_node(tree.root_node(), bytes, None, &mut entries);
    Ok(entries)
}

fn walk_java_node(
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
            "class_declaration" => {
                extract_java_class(child, source, class_name, entries);
            }
            "interface_declaration" => {
                extract_java_interface(child, source, class_name, entries);
            }
            "enum_declaration" => {
                if let Some(entry) = extract_java_named(child, source, SemanticKind::Enum, class_name) {
                    entries.push(entry);
                }
            }
            "record_declaration" => {
                if let Some(entry) = extract_java_named(child, source, SemanticKind::Struct, class_name) {
                    entries.push(entry);
                }
            }
            "method_declaration" => {
                if let Some(entry) = extract_java_method(child, source, class_name) {
                    entries.push(entry);
                }
            }
            "constructor_declaration" => {
                if let Some(entry) = extract_java_constructor(child, source, class_name) {
                    entries.push(entry);
                }
            }
            // Skip into program/class_body transparently
            "program" | "class_body" | "interface_body" | "enum_body" => {
                walk_java_node(child, source, class_name, entries);
            }
            _ => {}
        }
    }
}

fn extract_java_class(
    node: tree_sitter::Node,
    source: &[u8],
    parent_class: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let name_node = node.child_by_field_name("name");
    let raw_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let qualified_name = match parent_class {
        Some(p) => format!("{}::{}", p, raw_name),
        None => raw_name.to_string(),
    };

    let signature = extract_signature_with_delimiter(node, source, '{');
    entries.push(OutlineEntry {
        kind: SemanticKind::Class,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: parent_class.map(String::from),
    });

    // Descend into class body
    if let Some(body) = node.child_by_field_name("body") {
        walk_java_node(body, source, Some(&qualified_name), entries);
    }
}

fn extract_java_interface(
    node: tree_sitter::Node,
    source: &[u8],
    parent_class: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let name_node = node.child_by_field_name("name");
    let raw_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let qualified_name = match parent_class {
        Some(p) => format!("{}::{}", p, raw_name),
        None => raw_name.to_string(),
    };

    let signature = extract_signature_with_delimiter(node, source, '{');
    entries.push(OutlineEntry {
        kind: SemanticKind::Interface,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: parent_class.map(String::from),
    });

    if let Some(body) = node.child_by_field_name("body") {
        walk_java_node(body, source, Some(&qualified_name), entries);
    }
}

fn extract_java_named(
    node: tree_sitter::Node,
    source: &[u8],
    kind: SemanticKind,
    parent_class: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let raw_name = name_node.utf8_text(source).ok()?;

    let qualified_name = match parent_class {
        Some(p) => format!("{}::{}", p, raw_name),
        None => raw_name.to_string(),
    };

    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent: parent_class.map(String::from),
    })
}

fn extract_java_method(
    node: tree_sitter::Node,
    source: &[u8],
    class_name: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let method_name = name_node.utf8_text(source).ok()?;

    let (qualified_name, parent) = match class_name {
        Some(cn) => (format!("{}::{}", cn, method_name), Some(cn.to_string())),
        None => (method_name.to_string(), None),
    };

    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind: SemanticKind::Method,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent,
    })
}

fn extract_java_constructor(
    node: tree_sitter::Node,
    source: &[u8],
    class_name: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let ctor_name = name_node.utf8_text(source).ok()?;

    let (qualified_name, parent) = match class_name {
        Some(cn) => (format!("{}::{}", cn, ctor_name), Some(cn.to_string())),
        None => (ctor_name.to_string(), None),
    };

    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind: SemanticKind::Constructor,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent,
    })
}
