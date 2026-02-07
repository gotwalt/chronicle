use crate::ast::outline::{
    extract_signature_with_delimiter, node_line_range, should_skip_node, OutlineEntry, SemanticKind,
};
use crate::error::AstError;

/// Extract an outline from Objective-C source code.
pub fn extract_objc_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_objc::LANGUAGE.into())
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
    walk_objc_node(tree.root_node(), bytes, None, &mut entries);
    Ok(entries)
}

fn walk_objc_node(
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
            "class_interface" | "class_implementation" => {
                extract_objc_class(child, source, entries);
            }
            "protocol_declaration" => {
                extract_objc_protocol(child, source, entries);
            }
            "function_definition" => {
                if let Some(entry) = extract_objc_function(child, source) {
                    entries.push(entry);
                }
            }
            "method_declaration" | "method_definition" => {
                if let Some(entry) = extract_objc_method(child, source, class_name) {
                    entries.push(entry);
                }
            }
            _ => {}
        }
    }
}

fn extract_objc_class(node: tree_sitter::Node, source: &[u8], entries: &mut Vec<OutlineEntry>) {
    // First child identifier is the class name.
    let class_name = first_identifier_text(node, source).unwrap_or("<unknown>");

    // Check if this is a category (has a `category` field).
    let category = node.child_by_field_name("category");
    let (kind, name) = if let Some(cat_node) = category {
        let cat_name = cat_node.utf8_text(source).unwrap_or("");
        (
            SemanticKind::Extension,
            format!("{}({})", class_name, cat_name),
        )
    } else {
        (SemanticKind::Class, class_name.to_string())
    };

    let signature = extract_signature_with_delimiter(node, source, '{');

    entries.push(OutlineEntry {
        kind,
        name: name.clone(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    });

    // Descend into children for methods.
    walk_objc_node(node, source, Some(class_name), entries);
}

fn extract_objc_protocol(node: tree_sitter::Node, source: &[u8], entries: &mut Vec<OutlineEntry>) {
    let name = first_identifier_text(node, source).unwrap_or("<unknown>");
    let signature = extract_signature_with_delimiter(node, source, '{');

    // Skip forward declarations like `@protocol Foo;`
    // These don't have a body (no method_declaration children and very short).
    let has_end = node
        .utf8_text(source)
        .map(|t| t.contains("@end"))
        .unwrap_or(false);
    if !has_end {
        return;
    }

    entries.push(OutlineEntry {
        kind: SemanticKind::Interface,
        name: name.to_string(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    });

    // Descend for method declarations.
    walk_objc_node(node, source, Some(name), entries);
}

fn extract_objc_method(
    node: tree_sitter::Node,
    source: &[u8],
    class_name: Option<&str>,
) -> Option<OutlineEntry> {
    let class = class_name?;

    // Determine class vs instance method: first non-whitespace char is '+' or '-'.
    let text = node.utf8_text(source).ok()?;
    let prefix = if text.trim_start().starts_with('+') {
        "+"
    } else {
        "-"
    };

    // Extract method name: collect all identifier and keyword_declarator texts.
    let method_name = extract_objc_selector(node, source)?;
    let qualified_name = format!("{}::{}{}", class, prefix, method_name);
    let signature = extract_objc_method_signature(node, source);

    Some(OutlineEntry {
        kind: SemanticKind::Method,
        name: qualified_name,
        signature: Some(signature),
        lines: node_line_range(node),
        parent: Some(class.to_string()),
    })
}

fn extract_objc_function(node: tree_sitter::Node, source: &[u8]) -> Option<OutlineEntry> {
    // C-style function: use the declarator field to find the name.
    let declarator = node.child_by_field_name("declarator")?;
    let name = extract_c_declarator_name(declarator, source)?;
    let signature = extract_signature_with_delimiter(node, source, '{');

    Some(OutlineEntry {
        kind: SemanticKind::Function,
        name: name.to_string(),
        signature: Some(signature),
        lines: node_line_range(node),
        parent: None,
    })
}

/// Extract the ObjC selector name from a method_declaration or method_definition.
///
/// In the tree-sitter-objc grammar:
/// - Keyword selectors: `identifier` followed by `method_parameter` sibling →
///   collect as `keyword:` parts. E.g., `initWithName:(NSString*)n age:(int)a`
///   produces identifiers `initWithName` and `age`, each followed by a
///   `method_parameter`, yielding selector `initWithName:age:`.
/// - Simple selectors: a lone `identifier` with no following `method_parameter`.
fn extract_objc_selector(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut keyword_parts = Vec::new();
    let mut simple_name = None;

    let child_count = node.child_count();
    for i in 0..child_count {
        let child = node.child(i)?;
        if child.kind() == "identifier" {
            let text = child.utf8_text(source).ok()?;
            // Check if the next sibling is a method_parameter (keyword selector).
            let next = if i + 1 < child_count {
                node.child(i + 1)
            } else {
                None
            };
            if next.map(|n| n.kind()) == Some("method_parameter") {
                keyword_parts.push(format!("{}:", text));
            } else if simple_name.is_none() {
                simple_name = Some(text.to_string());
            }
        }
    }

    if !keyword_parts.is_empty() {
        Some(keyword_parts.join(""))
    } else {
        simple_name
    }
}

/// Extract the signature of an ObjC method (up to `{` or `;`).
fn extract_objc_method_signature(node: tree_sitter::Node, source: &[u8]) -> String {
    let text = node.utf8_text(source).unwrap_or("");
    // Find the first `{` or `;` as the delimiter.
    let end = text
        .find('{')
        .or_else(|| text.find(';'))
        .unwrap_or(text.len());
    text[..end].trim().to_string()
}

/// Extract function name from a C-style declarator chain.
fn extract_c_declarator_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "identifier" => node.utf8_text(source).ok(),
        "function_declarator" => {
            // First child is the declarator containing the name.
            let inner = node.child(0)?;
            extract_c_declarator_name(inner, source)
        }
        "pointer_declarator" => {
            // Walk children to find the actual declarator.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "*" {
                    return extract_c_declarator_name(child, source);
                }
            }
            None
        }
        "parenthesized_declarator" => {
            let inner = node.child(1)?; // Skip the `(`
            extract_c_declarator_name(inner, source)
        }
        _ => None,
    }
}

/// Find the first `identifier` child node text.
fn first_identifier_text<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OBJC: &str = r#"
#import <Foundation/Foundation.h>

@protocol Drawable
- (void)draw;
- (CGRect)bounds;
@end

@interface Shape : NSObject <Drawable>
@property (nonatomic, strong) NSString *name;
- (instancetype)initWithName:(NSString *)name;
- (void)draw;
+ (Shape *)defaultShape;
@end

@implementation Shape
- (instancetype)initWithName:(NSString *)name {
    self = [super init];
    if (self) {
        _name = name;
    }
    return self;
}

- (void)draw {
    NSLog(@"Drawing %@", self.name);
}

+ (Shape *)defaultShape {
    return [[Shape alloc] initWithName:@"default"];
}
@end

@interface Shape (Color)
- (void)setColor:(NSColor *)color;
@end

@implementation Shape (Color)
- (void)setColor:(NSColor *)color {
    // set color
}
@end

void freeFunction(int x) {
    printf("%d\n", x);
}
"#;

    #[test]
    fn objc_protocol() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
        let protos: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Interface)
            .collect();
        assert_eq!(protos.len(), 1);
        assert_eq!(protos[0].name, "Drawable");
    }

    #[test]
    fn objc_class_interface_and_implementation() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
        let classes: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Class)
            .collect();
        // class_interface + class_implementation = 2
        assert_eq!(
            classes.len(),
            2,
            "got: {:?}",
            classes.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(classes.iter().all(|c| c.name == "Shape"));
    }

    #[test]
    fn objc_category() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
        let exts: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Extension)
            .collect();
        assert_eq!(
            exts.len(),
            2,
            "got: {:?}",
            exts.iter().map(|e| &e.name).collect::<Vec<_>>()
        );
        assert!(exts.iter().all(|e| e.name == "Shape(Color)"));
    }

    #[test]
    fn objc_methods() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
        let methods: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Method)
            .collect();
        let names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
        // Protocol methods
        assert!(names.contains(&"Drawable::-draw"), "got: {:?}", names);
        assert!(names.contains(&"Drawable::-bounds"), "got: {:?}", names);
        // Class interface methods
        assert!(names.contains(&"Shape::-initWithName:"), "got: {:?}", names);
        assert!(names.contains(&"Shape::-draw"), "got: {:?}", names);
        assert!(names.contains(&"Shape::+defaultShape"), "got: {:?}", names);
        // Category methods
        assert!(names.contains(&"Shape::-setColor:"), "got: {:?}", names);
    }

    #[test]
    fn objc_instance_vs_class_method() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
        let methods: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Method)
            .collect();
        // +defaultShape is a class method
        assert!(methods.iter().any(|m| m.name.contains("::+")));
        // -draw is an instance method
        assert!(methods.iter().any(|m| m.name.contains("::-")));
    }

    #[test]
    fn objc_free_function() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
        let fns: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "freeFunction");
    }

    #[test]
    fn objc_line_ranges_valid() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
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
    fn objc_methods_have_parent() {
        let outline = extract_objc_outline(SAMPLE_OBJC).unwrap();
        let methods: Vec<&OutlineEntry> = outline
            .iter()
            .filter(|e| e.kind == SemanticKind::Method)
            .collect();
        for m in methods {
            assert!(m.parent.is_some(), "method {} should have parent", m.name);
        }
    }
}
