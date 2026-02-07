use crate::ast::outline::{
    extract_signature_with_delimiter, node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::error::AstError;

/// Extract an outline from Swift source code.
pub fn extract_swift_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
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
    walk_swift_node(tree.root_node(), bytes, None, &mut entries);
    Ok(entries)
}

fn walk_swift_node(
    node: tree_sitter::Node,
    source: &[u8],
    type_context: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if should_skip_node(&child) {
            continue;
        }
        match child.kind() {
            "class_declaration" => {
                extract_swift_type_declaration(child, source, type_context, entries);
            }
            "protocol_declaration" => {
                extract_swift_protocol(child, source, type_context, entries);
            }
            "function_declaration" | "protocol_function_declaration" => {
                if let Some(entry) = extract_swift_function(child, source, type_context) {
                    entries.push(entry);
                }
            }
            "init_declaration" => {
                if let Some(entry) = extract_swift_init(child, source, type_context) {
                    entries.push(entry);
                }
            }
            "typealias_declaration" => {
                if let Some(entry) = extract_swift_typealias(child, source, type_context) {
                    entries.push(entry);
                }
            }
            _ => {}
        }
    }
}

/// Handle `class_declaration` which covers class, struct, enum, extension, actor.
fn extract_swift_type_declaration(
    node: tree_sitter::Node,
    source: &[u8],
    type_context: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    // `declaration_kind` field tells us what this is.
    let decl_kind = node
        .child_by_field_name("declaration_kind")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("class");

    let name_node = node.child_by_field_name("name");
    let raw_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<unknown>");

    let qualified_name = match type_context {
        Some(ctx) => format!("{}::{}", ctx, raw_name),
        None => raw_name.to_string(),
    };

    let kind = match decl_kind {
        "class" | "actor" => SemanticKind::Class,
        "struct" => SemanticKind::Struct,
        "enum" => SemanticKind::Enum,
        "extension" => SemanticKind::Extension,
        _ => SemanticKind::Class,
    };

    let signature = extract_signature_with_delimiter(node, source, '{');

    entries.push(OutlineEntry {
        kind,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: type_context.map(String::from),
    });

    // Descend into the body for methods and nested types.
    if let Some(body) = node.child_by_field_name("body") {
        walk_swift_node(body, source, Some(&qualified_name), entries);
    }
}

fn extract_swift_protocol(
    node: tree_sitter::Node,
    source: &[u8],
    type_context: Option<&str>,
    entries: &mut Vec<OutlineEntry>,
) {
    let name_node = node.child_by_field_name("name");
    let raw_name = name_node
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<unknown>");

    let qualified_name = match type_context {
        Some(ctx) => format!("{}::{}", ctx, raw_name),
        None => raw_name.to_string(),
    };

    let signature = extract_signature_with_delimiter(node, source, '{');

    entries.push(OutlineEntry {
        kind: SemanticKind::Interface,
        name: qualified_name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: type_context.map(String::from),
    });

    // Descend into protocol body for method signatures.
    if let Some(body) = node.child_by_field_name("body") {
        walk_swift_node(body, source, Some(&qualified_name), entries);
    }
}

fn extract_swift_function(
    node: tree_sitter::Node,
    source: &[u8],
    type_context: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let fn_name = name_node.utf8_text(source).ok()?;

    let (kind, qualified_name, parent) = match type_context {
        Some(ctx) => (
            SemanticKind::Method,
            format!("{}::{}", ctx, fn_name),
            Some(ctx.to_string()),
        ),
        None => (SemanticKind::Function, fn_name.to_string(), None),
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

fn extract_swift_init(
    node: tree_sitter::Node,
    source: &[u8],
    type_context: Option<&str>,
) -> Option<OutlineEntry> {
    let ctx = type_context?;
    let qualified_name = format!("{}::init", ctx);
    let signature = extract_signature_with_delimiter(node, source, '{');

    Some(OutlineEntry {
        kind: SemanticKind::Constructor,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent: Some(ctx.to_string()),
    })
}

fn extract_swift_typealias(
    node: tree_sitter::Node,
    source: &[u8],
    type_context: Option<&str>,
) -> Option<OutlineEntry> {
    let name_node = node.child_by_field_name("name")?;
    let raw_name = name_node.utf8_text(source).ok()?;

    let qualified_name = match type_context {
        Some(ctx) => format!("{}::{}", ctx, raw_name),
        None => raw_name.to_string(),
    };

    let full_text = node.utf8_text(source).unwrap_or("").trim().to_string();

    Some(OutlineEntry {
        kind: SemanticKind::TypeAlias,
        name: qualified_name,
        signature: Some(full_text),
        lines: node_line_range(node),
        parent: type_context.map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SWIFT: &str = r#"
import Foundation

protocol Drawable {
    func draw()
    func bounds() -> CGRect
}

class Shape: NSObject, Drawable {
    var name: String

    init(name: String) {
        self.name = name
    }

    func draw() {
        print("Drawing \(name)")
    }

    static func defaultShape() -> Shape {
        return Shape(name: "default")
    }
}

struct Point {
    var x: Double
    var y: Double

    func distance(to other: Point) -> Double {
        let dx = x - other.x
        let dy = y - other.y
        return (dx * dx + dy * dy).squareRoot()
    }
}

enum Direction {
    case north, south, east, west

    func opposite() -> Direction {
        switch self {
        case .north: return .south
        case .south: return .north
        case .east: return .west
        case .west: return .east
        }
    }
}

extension Shape {
    func description() -> String {
        return "Shape: \(name)"
    }
}

typealias Coordinate = (Double, Double)

func freeFunction(x: Int) -> Int {
    return x * 2
}
"#;

    #[test]
    fn swift_protocol() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let protos: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Interface)
            .collect();
        assert_eq!(
            protos.len(),
            1,
            "got: {:?}",
            protos.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(protos[0].name, "Drawable");
    }

    #[test]
    fn swift_class() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let classes: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Class)
            .collect();
        assert_eq!(
            classes.len(),
            1,
            "got: {:?}",
            classes.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(classes[0].name, "Shape");
    }

    #[test]
    fn swift_struct() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let structs: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "Point");
    }

    #[test]
    fn swift_enum() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let enums: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].name, "Direction");
    }

    #[test]
    fn swift_extension() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let exts: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Extension)
            .collect();
        assert_eq!(
            exts.len(),
            1,
            "got: {:?}",
            exts.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(exts[0].name, "Shape");
    }

    #[test]
    fn swift_init() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let ctors: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Constructor)
            .collect();
        assert_eq!(
            ctors.len(),
            1,
            "got: {:?}",
            ctors.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(ctors[0].name, "Shape::init");
    }

    #[test]
    fn swift_methods() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let methods: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Method)
            .collect();
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"Drawable::draw"), "got: {:?}", names);
        assert!(names.contains(&"Drawable::bounds"), "got: {:?}", names);
        assert!(names.contains(&"Shape::draw"), "got: {:?}", names);
        assert!(names.contains(&"Shape::defaultShape"), "got: {:?}", names);
        assert!(names.contains(&"Point::distance"), "got: {:?}", names);
        assert!(names.contains(&"Direction::opposite"), "got: {:?}", names);
        assert!(names.contains(&"Shape::description"), "got: {:?}", names);
    }

    #[test]
    fn swift_free_function() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let fns: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Function)
            .collect();
        assert_eq!(
            fns.len(),
            1,
            "got: {:?}",
            fns.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert_eq!(fns[0].name, "freeFunction");
    }

    #[test]
    fn swift_typealias() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let aliases: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::TypeAlias)
            .collect();
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].name, "Coordinate");
    }

    #[test]
    fn swift_line_ranges_valid() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        for entry in &outline {
            assert!(
                entry.lines.start > 0,
                "line start should be 1-based for {}",
                entry.name
            );
            assert!(
                entry.lines.end >= entry.lines.start,
                "end ({}) should be >= start ({}) for {}",
                entry.lines.end,
                entry.lines.start,
                entry.name
            );
        }
    }

    #[test]
    fn swift_methods_have_parent() {
        let outline = extract_swift_outline(SAMPLE_SWIFT).unwrap();
        let methods: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Method || e.kind == SemanticKind::Constructor)
            .collect();
        for m in methods {
            assert!(m.parent.is_some(), "method {} should have parent", m.name);
        }
    }
}
