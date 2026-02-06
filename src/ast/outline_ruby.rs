use crate::ast::outline::{
    node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::error::AstError;

/// Extract an outline from Ruby source code.
pub fn extract_ruby_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_ruby::LANGUAGE.into())
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
    walk_ruby_node(tree.root_node(), bytes, None, &mut entries);
    Ok(entries)
}

fn walk_ruby_node(
    node: tree_sitter::Node,
    source: &[u8],
    enclosing: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if should_skip_node(&child) {
            continue;
        }
        match child.kind() {
            "method" => {
                if let Some(entry) = extract_ruby_method(child, source, enclosing) {
                    entries.push(entry);
                }
            }
            "singleton_method" => {
                if let Some(entry) = extract_ruby_singleton_method(child, source, enclosing) {
                    entries.push(entry);
                }
            }
            "class" => {
                extract_ruby_class(child, source, enclosing, entries);
            }
            "module" => {
                extract_ruby_module(child, source, enclosing, entries);
            }
            _ => {}
        }
    }
}

fn extract_ruby_method(
    node: tree_sitter::Node,
    source: &[u8],
    enclosing: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let method_name = name_node.utf8_text(source).ok()?;

    let (kind, qualified_name, parent) = match enclosing {
        Some(enc) => {
            let k = if method_name == "initialize" {
                SemanticKind::Constructor
            } else {
                SemanticKind::Method
            };
            (k, format!("{}::{}", enc, method_name), Some(enc.to_string()))
        }
        None => (SemanticKind::Function, method_name.to_string(), None),
    };

    let signature = extract_ruby_method_signature(node, source);
    Some(OutlineEntry {
        kind,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent,
    })
}

fn extract_ruby_singleton_method(
    node: tree_sitter::Node,
    source: &[u8],
    enclosing: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let method_name = name_node.utf8_text(source).ok()?;

    let (qualified_name, parent) = match enclosing {
        Some(enc) => (format!("{}::{}", enc, method_name), Some(enc.to_string())),
        None => (method_name.to_string(), None),
    };

    let signature = extract_ruby_method_signature(node, source);
    Some(OutlineEntry {
        kind: SemanticKind::Method,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent,
    })
}

fn extract_ruby_class(
    node: tree_sitter::Node,
    source: &[u8],
    enclosing: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let name_node = node.child_by_field_name("name");
    let raw_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let qualified_name = match enclosing {
        Some(enc) => format!("{}::{}", enc, raw_name),
        None => raw_name.to_string(),
    };

    let signature = extract_ruby_class_signature(node, source);
    entries.push(OutlineEntry {
        kind: SemanticKind::Class,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: enclosing.map(String::from),
    });

    // Descend into class body
    if let Some(body) = node.child_by_field_name("body") {
        walk_ruby_node(body, source, Some(&qualified_name), entries);
    }
}

fn extract_ruby_module(
    node: tree_sitter::Node,
    source: &[u8],
    enclosing: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let name_node = node.child_by_field_name("name");
    let raw_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let qualified_name = match enclosing {
        Some(enc) => format!("{}::{}", enc, raw_name),
        None => raw_name.to_string(),
    };

    let signature = format!("module {}", raw_name);
    entries.push(OutlineEntry {
        kind: SemanticKind::Module,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: enclosing.map(String::from),
    });

    // Descend into module body
    if let Some(body) = node.child_by_field_name("body") {
        walk_ruby_node(body, source, Some(&qualified_name), entries);
    }
}

/// Extract Ruby method signature: "def method_name(params)"
fn extract_ruby_method_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let full_text = node.utf8_text(source).unwrap_or("");
    // Find the end of the first line (method signature is typically one line)
    let first_line = full_text.lines().next().unwrap_or(full_text);
    first_line.trim().to_string()
}

/// Extract Ruby class signature: "class ClassName < SuperClass"
fn extract_ruby_class_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let full_text = node.utf8_text(source).unwrap_or("");
    let first_line = full_text.lines().next().unwrap_or(full_text);
    first_line.trim().to_string()
}
