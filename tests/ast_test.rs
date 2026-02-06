use chronicle::ast::{self, AnchorMatch, Language, OutlineEntry, SemanticKind};

const SAMPLE_RUST: &str = r#"
fn standalone() {
    println!("standalone");
}

pub struct Config {
    pub name: String,
    pub value: u32,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Processor {
    fn process(&self);
}

impl Config {
    pub fn new(name: String, value: u32) -> Self {
        Self { name, value }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}
"#;

#[test]
fn language_from_path_rs() {
    assert_eq!(Language::from_path("src/main.rs"), Language::Rust);
}

#[test]
fn language_from_path_py() {
    assert_eq!(Language::from_path("lib/app.py"), Language::Unsupported);
}

#[test]
fn language_from_path_ts() {
    assert_eq!(Language::from_path("src/index.ts"), Language::Unsupported);
}

#[test]
fn language_from_path_unknown() {
    assert_eq!(Language::from_path("Makefile"), Language::Unsupported);
}

#[test]
fn outline_extracts_standalone_function() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let fns: Vec<&OutlineEntry> = outline
        .iter()
        .filter(|e| e.kind == SemanticKind::Function)
        .collect();
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "standalone");
    assert!(fns[0].parent.is_none());
}

#[test]
fn outline_extracts_struct() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let structs: Vec<&OutlineEntry> = outline
        .iter()
        .filter(|e| e.kind == SemanticKind::Struct)
        .collect();
    assert_eq!(structs.len(), 1);
    assert_eq!(structs[0].name, "Config");
}

#[test]
fn outline_extracts_enum() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let enums: Vec<&OutlineEntry> = outline
        .iter()
        .filter(|e| e.kind == SemanticKind::Enum)
        .collect();
    assert_eq!(enums.len(), 1);
    assert_eq!(enums[0].name, "Status");
}

#[test]
fn outline_extracts_trait() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let traits: Vec<&OutlineEntry> = outline
        .iter()
        .filter(|e| e.kind == SemanticKind::Trait)
        .collect();
    assert_eq!(traits.len(), 1);
    assert_eq!(traits[0].name, "Processor");
}

#[test]
fn outline_extracts_impl_and_methods() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();

    let impls: Vec<&OutlineEntry> = outline
        .iter()
        .filter(|e| e.kind == SemanticKind::Impl)
        .collect();
    assert_eq!(impls.len(), 1);
    assert_eq!(impls[0].name, "Config");

    let methods: Vec<&OutlineEntry> = outline
        .iter()
        .filter(|e| e.kind == SemanticKind::Method)
        .collect();
    assert_eq!(methods.len(), 2);

    let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
    assert!(method_names.contains(&"Config::new"));
    assert!(method_names.contains(&"Config::name"));

    for m in &methods {
        assert_eq!(m.parent.as_deref(), Some("Config"));
    }
}

#[test]
fn outline_entries_have_valid_line_ranges() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    for entry in &outline {
        assert!(entry.lines.start > 0, "line start should be 1-based");
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
fn outline_entries_have_signatures() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    for entry in &outline {
        assert!(entry.signature.is_some(), "expected signature for {}", entry.name);
        let sig = entry.signature.as_ref().unwrap();
        assert!(!sig.is_empty(), "signature should not be empty for {}", entry.name);
        assert!(!sig.contains('{'), "signature for {} should not contain body: {}", entry.name, sig);
    }
}

#[test]
fn outline_unsupported_language_errors() {
    let result = ast::extract_outline("whatever", Language::Unsupported);
    assert!(result.is_err());
}

#[test]
fn anchor_exact_match() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let m = ast::resolve_anchor(&outline, "function", "standalone").unwrap();
    assert!(matches!(m, AnchorMatch::Exact(_)));
    assert_eq!(m.entry().name, "standalone");
}

#[test]
fn anchor_exact_match_struct() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let m = ast::resolve_anchor(&outline, "struct", "Config").unwrap();
    assert!(matches!(m, AnchorMatch::Exact(_)));
    assert_eq!(m.entry().name, "Config");
}

#[test]
fn anchor_qualified_match() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let m = ast::resolve_anchor(&outline, "method", "new").unwrap();
    assert!(matches!(m, AnchorMatch::Qualified(_)));
    assert_eq!(m.entry().name, "Config::new");
}

#[test]
fn anchor_fuzzy_match() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let m = ast::resolve_anchor(&outline, "function", "standalon").unwrap();
    assert!(matches!(m, AnchorMatch::Fuzzy(_, _)));
    assert_eq!(m.entry().name, "standalone");
}

#[test]
fn anchor_no_match_returns_none() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let m = ast::resolve_anchor(&outline, "function", "completely_nonexistent_function_name");
    assert!(m.is_none());
}

#[test]
fn anchor_lines_are_correct() {
    let outline = ast::extract_outline(SAMPLE_RUST, Language::Rust).unwrap();
    let m = ast::resolve_anchor(&outline, "function", "standalone").unwrap();
    let lines = m.lines();
    assert!(lines.start >= 2);
    assert!(lines.end >= lines.start);
}
