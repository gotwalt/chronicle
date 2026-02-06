use crate::ast::outline::{
    extract_signature_with_delimiter, node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::ast::outline_c;
use crate::error::AstError;

/// Extract an outline from C++ source code.
pub fn extract_cpp_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
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
    walk_cpp_node(tree.root_node(), bytes, None, &mut entries);
    Ok(entries)
}

fn walk_cpp_node(
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
            "function_definition" => {
                if let Some(entry) = extract_cpp_function(child, source, enclosing) {
                    entries.push(entry);
                }
            }
            "class_specifier" => {
                if has_body(&child) {
                    extract_cpp_class(child, source, enclosing, entries);
                }
            }
            "struct_specifier" => {
                if has_body(&child) {
                    if let Some(entry) = extract_cpp_tagged(child, source, SemanticKind::Struct, enclosing) {
                        entries.push(entry);
                    }
                }
            }
            "enum_specifier" => {
                if has_body(&child) {
                    if let Some(entry) = extract_cpp_tagged(child, source, SemanticKind::Enum, enclosing) {
                        entries.push(entry);
                    }
                }
            }
            "union_specifier" => {
                if has_body(&child) {
                    if let Some(entry) = extract_cpp_tagged(child, source, SemanticKind::Struct, enclosing) {
                        entries.push(entry);
                    }
                }
            }
            "namespace_definition" => {
                extract_cpp_namespace(child, source, enclosing, entries);
            }
            "template_declaration" => {
                // Transparent: descend into the templated declaration
                walk_cpp_node(child, source, enclosing, entries);
            }
            "type_definition" => {
                if let Some(entry) = extract_cpp_typedef(child, source) {
                    entries.push(entry);
                }
            }
            "alias_declaration" => {
                if let Some(entry) = extract_cpp_alias(child, source) {
                    entries.push(entry);
                }
            }
            "declaration" => {
                // Could contain a function declaration inside a class, skip for now
            }
            _ => {}
        }
    }
}

fn extract_cpp_function(
    node: tree_sitter::Node,
    source: &[u8],
    enclosing: Option<&str>,
) -> Option<OutlineEntry> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = outline_c::extract_declarator_name(declarator, source)?;
    let signature = extract_signature_with_delimiter(node, source, '{');

    let (kind, qualified_name, parent) = match enclosing {
        Some(enc) => {
            // Detect constructor: function name matches class name
            let k = if is_class_constructor(enc, &name) {
                SemanticKind::Constructor
            } else {
                SemanticKind::Method
            };
            (k, format!("{}::{}", enc, name), Some(enc.to_string()))
        }
        None => (SemanticKind::Function, name, None),
    };

    Some(OutlineEntry {
        kind,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent,
    })
}

fn extract_cpp_class(
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

    let signature = extract_signature_with_delimiter(node, source, '{');
    entries.push(OutlineEntry {
        kind: SemanticKind::Class,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: enclosing.map(String::from),
    });

    if let Some(body) = node.child_by_field_name("body") {
        walk_cpp_node(body, source, Some(&qualified_name), entries);
    }
}

fn extract_cpp_tagged(
    node: tree_sitter::Node,
    source: &[u8],
    kind: SemanticKind,
    enclosing: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let raw_name = name_node.utf8_text(source).ok()?;

    let qualified_name = match enclosing {
        Some(enc) => format!("{}::{}", enc, raw_name),
        None => raw_name.to_string(),
    };

    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent: enclosing.map(String::from),
    })
}

fn extract_cpp_namespace(
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

    let signature = extract_signature_with_delimiter(node, source, '{');
    entries.push(OutlineEntry {
        kind: SemanticKind::Namespace,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: enclosing.map(String::from),
    });

    if let Some(body) = node.child_by_field_name("body") {
        walk_cpp_node(body, source, Some(&qualified_name), entries);
    }
}

fn extract_cpp_typedef(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let declarator = node.child_by_field_name("declarator")?;
    let name = outline_c::extract_declarator_name(declarator, source)?;
    let full_text = node.utf8_text(source).unwrap_or("").trim().to_string();
    Some(OutlineEntry {
        kind: SemanticKind::TypeAlias,
        name,
        signature: Some(full_text),
        lines: node_line_range(node),
        parent: None,
    })
}

fn extract_cpp_alias(
    node: tree_sitter::Node,
    source: &[u8],
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source).ok()?;
    let full_text = node.utf8_text(source).unwrap_or("").trim().to_string();
    Some(OutlineEntry {
        kind: SemanticKind::TypeAlias,
        name: name.to_string(),
        signature: Some(full_text),
        lines: node_line_range(node),
        parent: None,
    })
}

/// Check if a function name matches the enclosing class/struct name (constructor).
fn is_class_constructor(enclosing: &str, fn_name: &str) -> bool {
    // The enclosing might be qualified (e.g., "Namespace::Class"), use the last segment
    let class_short_name = enclosing.rsplit("::").next().unwrap_or(enclosing);
    fn_name == class_short_name
}

fn has_body(node: &tree_sitter::Node) -> bool {
    node.child_by_field_name("body").is_some()
}
