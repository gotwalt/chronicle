use crate::ast::outline::{
    extract_signature_with_delimiter, node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::error::AstError;

/// Extract an outline from TypeScript/TSX/JavaScript/JSX source code.
///
/// When `is_tsx` is true, uses the TSX grammar (also handles JSX).
/// When false, uses the TypeScript grammar (also handles plain JS).
pub fn extract_typescript_outline(
    source: &str,
    is_tsx: bool,
) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    let ts_lang: tree_sitter::Language = if is_tsx {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    };
    parser.set_language(&ts_lang).map_err(|e| AstError::TreeSitter {
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
    walk_ts_node(tree.root_node(), bytes, None, &mut entries);
    Ok(entries)
}

fn walk_ts_node(
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
            "function_declaration" | "generator_function_declaration" => {
                if let Some(entry) = extract_ts_function(child, source) {
                    entries.push(entry);
                }
            }
            "class_declaration" => {
                extract_ts_class(child, source, entries);
            }
            "interface_declaration" => {
                if let Some(entry) = extract_ts_named(child, source, SemanticKind::Interface) {
                    entries.push(entry);
                }
            }
            "enum_declaration" => {
                if let Some(entry) = extract_ts_named(child, source, SemanticKind::Enum) {
                    entries.push(entry);
                }
            }
            "type_alias_declaration" => {
                if let Some(entry) = extract_ts_named(child, source, SemanticKind::TypeAlias) {
                    entries.push(entry);
                }
            }
            "export_statement" => {
                // Transparent: descend into exported declaration
                walk_ts_node(child, source, class_name, entries);
            }
            "lexical_declaration" => {
                // Check for `const foo = () => {}` pattern
                extract_ts_arrow_functions(child, source, entries);
            }
            "method_definition" => {
                if let Some(entry) = extract_ts_method(child, source, class_name) {
                    entries.push(entry);
                }
            }
            "public_field_definition" => {
                // Arrow function assigned to class field
                if has_arrow_function_value(&child) {
                    if let Some(entry) = extract_ts_method(child, source, class_name) {
                        entries.push(entry);
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_ts_function(
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

fn extract_ts_named(
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

fn extract_ts_class(
    node: tree_sitter::Node,
    source: &[u8],
    entries: &mut Vec<OutlineEntry>,
) {
    let name_node = node.child_by_field_name("name");
    let class_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<anonymous>");

    let signature = extract_signature_with_delimiter(node, source, '{');
    entries.push(OutlineEntry {
        kind: SemanticKind::Class,
        name: class_name.to_string(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    });

    // Descend into class body for methods
    if let Some(body) = node.child_by_field_name("body") {
        walk_ts_node(body, source, Some(class_name), entries);
    }
}

fn extract_ts_method(
    node: tree_sitter::Node,
    source: &[u8],
    class_name: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let method_name = name_node.utf8_text(source).ok()?;

    let (kind, qualified_name, parent) = if method_name == "constructor" {
        let qn = match class_name {
            Some(cn) => format!("{}::constructor", cn),
            None => "constructor".to_string(),
        };
        (SemanticKind::Constructor, qn, class_name.map(String::from))
    } else {
        match class_name {
            Some(cn) => (
                SemanticKind::Method,
                format!("{}::{}", cn, method_name),
                Some(cn.to_string()),
            ),
            None => (SemanticKind::Method, method_name.to_string(), None),
        }
    };

    let signature = extract_signature_with_delimiter(node, source, '{');
    Some(OutlineEntry {
        kind,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent,
    })
}

fn extract_ts_arrow_functions(
    node: tree_sitter::Node,
    source: &[u8],
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let Some(value) = child.child_by_field_name("value") {
                if value.kind() == "arrow_function" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        if let Ok(name) = name_node.utf8_text(source) {
                            let signature = extract_signature_with_delimiter(node, source, '{');
                            entries.push(OutlineEntry {
                                kind: SemanticKind::Function,
                                name: name.to_string(),
                                signature: Some(signature),
                                lines: node_line_range(node),
                                parent: None,
                            });
                        }
                    }
                }
            }
        }
    }
}

fn has_arrow_function_value(node: &tree_sitter::Node) -> bool {
    node.child_by_field_name("value")
        .is_some_and(|v| v.kind() == "arrow_function")
}
