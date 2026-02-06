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

// --- Language detection tests ---

#[test]
fn language_from_path_rs() {
    assert_eq!(Language::from_path("src/main.rs"), Language::Rust);
}

#[test]
fn language_from_path_py() {
    assert_eq!(Language::from_path("lib/app.py"), Language::Python);
}

#[test]
fn language_from_path_pyi() {
    assert_eq!(Language::from_path("lib/app.pyi"), Language::Python);
}

#[test]
fn language_from_path_ts() {
    assert_eq!(Language::from_path("src/index.ts"), Language::TypeScript);
}

#[test]
fn language_from_path_mts() {
    assert_eq!(Language::from_path("src/mod.mts"), Language::TypeScript);
}

#[test]
fn language_from_path_cts() {
    assert_eq!(Language::from_path("src/mod.cts"), Language::TypeScript);
}

#[test]
fn language_from_path_tsx() {
    assert_eq!(Language::from_path("src/App.tsx"), Language::Tsx);
}

#[test]
fn language_from_path_js() {
    assert_eq!(Language::from_path("src/index.js"), Language::JavaScript);
}

#[test]
fn language_from_path_mjs() {
    assert_eq!(Language::from_path("src/index.mjs"), Language::JavaScript);
}

#[test]
fn language_from_path_cjs() {
    assert_eq!(Language::from_path("src/index.cjs"), Language::JavaScript);
}

#[test]
fn language_from_path_jsx() {
    assert_eq!(Language::from_path("src/App.jsx"), Language::Jsx);
}

#[test]
fn language_from_path_go() {
    assert_eq!(Language::from_path("main.go"), Language::Go);
}

#[test]
fn language_from_path_java() {
    assert_eq!(Language::from_path("src/Main.java"), Language::Java);
}

#[test]
fn language_from_path_c() {
    assert_eq!(Language::from_path("src/main.c"), Language::C);
}

#[test]
fn language_from_path_h() {
    assert_eq!(Language::from_path("include/header.h"), Language::C);
}

#[test]
fn language_from_path_cpp() {
    assert_eq!(Language::from_path("src/main.cpp"), Language::Cpp);
}

#[test]
fn language_from_path_cc() {
    assert_eq!(Language::from_path("src/main.cc"), Language::Cpp);
}

#[test]
fn language_from_path_hpp() {
    assert_eq!(Language::from_path("include/header.hpp"), Language::Cpp);
}

#[test]
fn language_from_path_rb() {
    assert_eq!(Language::from_path("lib/app.rb"), Language::Ruby);
}

#[test]
fn language_from_path_rake() {
    assert_eq!(Language::from_path("tasks/build.rake"), Language::Ruby);
}

#[test]
fn language_from_path_gemspec() {
    assert_eq!(Language::from_path("my_gem.gemspec"), Language::Ruby);
}

#[test]
fn language_from_path_unknown() {
    assert_eq!(Language::from_path("Makefile"), Language::Unsupported);
}

// --- Rust outline tests ---

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

// --- Anchor resolution tests ---

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

// --- SemanticKind new variants ---

#[test]
fn semantic_kind_class() {
    assert_eq!(SemanticKind::Class.as_str(), "class");
    assert_eq!(SemanticKind::from_str_loose("class"), Some(SemanticKind::Class));
}

#[test]
fn semantic_kind_interface() {
    assert_eq!(SemanticKind::Interface.as_str(), "interface");
    assert_eq!(SemanticKind::from_str_loose("interface"), Some(SemanticKind::Interface));
}

#[test]
fn semantic_kind_namespace() {
    assert_eq!(SemanticKind::Namespace.as_str(), "namespace");
    assert_eq!(SemanticKind::from_str_loose("namespace"), Some(SemanticKind::Namespace));
    assert_eq!(SemanticKind::from_str_loose("package"), Some(SemanticKind::Namespace));
}

#[test]
fn semantic_kind_constructor() {
    assert_eq!(SemanticKind::Constructor.as_str(), "constructor");
    assert_eq!(SemanticKind::from_str_loose("constructor"), Some(SemanticKind::Constructor));
    assert_eq!(SemanticKind::from_str_loose("ctor"), Some(SemanticKind::Constructor));
}

// --- TypeScript outline tests ---

const SAMPLE_TS: &str = r#"
function greet(name: string): string {
    return `Hello, ${name}`;
}

class Greeter {
    constructor(public greeting: string) {}

    greet(name: string): string {
        return this.greeting + name;
    }

    static create(): Greeter {
        return new Greeter("Hi ");
    }
}

interface Greetable {
    greet(name: string): string;
}

enum Color {
    Red,
    Green,
    Blue,
}

type StringAlias = string;

const handler = (req: Request) => {
    return new Response("ok");
};

export function exported(): void {}
"#;

#[test]
fn typescript_outline_function() {
    let outline = ast::extract_outline(SAMPLE_TS, Language::TypeScript).unwrap();
    let fns: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Function).collect();
    assert!(fns.iter().any(|e| e.name == "greet"), "expected function greet, got: {:?}", fns);
    assert!(fns.iter().any(|e| e.name == "handler"), "expected arrow function handler, got: {:?}", fns);
    assert!(fns.iter().any(|e| e.name == "exported"), "expected exported function, got: {:?}", fns);
}

#[test]
fn typescript_outline_class_and_methods() {
    let outline = ast::extract_outline(SAMPLE_TS, Language::TypeScript).unwrap();
    let classes: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Class).collect();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, "Greeter");

    let constructors: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Constructor).collect();
    assert_eq!(constructors.len(), 1);
    assert_eq!(constructors[0].name, "Greeter::constructor");

    let methods: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Method).collect();
    let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
    assert!(method_names.contains(&"Greeter::greet"));
    assert!(method_names.contains(&"Greeter::create"));
}

#[test]
fn typescript_outline_interface() {
    let outline = ast::extract_outline(SAMPLE_TS, Language::TypeScript).unwrap();
    let ifaces: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Interface).collect();
    assert_eq!(ifaces.len(), 1);
    assert_eq!(ifaces[0].name, "Greetable");
}

#[test]
fn typescript_outline_enum() {
    let outline = ast::extract_outline(SAMPLE_TS, Language::TypeScript).unwrap();
    let enums: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Enum).collect();
    assert_eq!(enums.len(), 1);
    assert_eq!(enums[0].name, "Color");
}

#[test]
fn typescript_outline_type_alias() {
    let outline = ast::extract_outline(SAMPLE_TS, Language::TypeScript).unwrap();
    let aliases: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::TypeAlias).collect();
    assert_eq!(aliases.len(), 1);
    assert_eq!(aliases[0].name, "StringAlias");
}

// --- JavaScript outline tests ---

const SAMPLE_JS: &str = r#"
function hello() {
    console.log("hello");
}

class MyClass {
    constructor() {
        this.x = 0;
    }

    doStuff() {
        return this.x;
    }
}

const arrow = () => {
    return 42;
};
"#;

#[test]
fn javascript_outline_function_and_class() {
    let outline = ast::extract_outline(SAMPLE_JS, Language::JavaScript).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Function && e.name == "hello"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Class && e.name == "MyClass"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Constructor && e.name == "MyClass::constructor"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Method && e.name == "MyClass::doStuff"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Function && e.name == "arrow"));
}

// --- Python outline tests ---

const SAMPLE_PY: &str = r#"
def top_level():
    pass

class Animal:
    def __init__(self, name):
        self.name = name

    def speak(self):
        pass

    @staticmethod
    def create(name):
        return Animal(name)
"#;

#[test]
fn python_outline_function() {
    let outline = ast::extract_outline(SAMPLE_PY, Language::Python).unwrap();
    let fns: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Function).collect();
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "top_level");
}

#[test]
fn python_outline_class_and_methods() {
    let outline = ast::extract_outline(SAMPLE_PY, Language::Python).unwrap();
    let classes: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Class).collect();
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, "Animal");

    let ctors: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Constructor).collect();
    assert_eq!(ctors.len(), 1);
    assert_eq!(ctors[0].name, "Animal::__init__");

    let methods: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Method).collect();
    let method_names: Vec<&str> = methods.iter().map(|m| m.name.as_str()).collect();
    assert!(method_names.contains(&"Animal::speak"));
    assert!(method_names.contains(&"Animal::create"));
}

// --- Go outline tests ---

const SAMPLE_GO: &str = r#"
package main

func main() {
    fmt.Println("hello")
}

type Server struct {
    Port int
}

func (s *Server) Start() error {
    return nil
}

type Handler interface {
    Handle()
}

const MaxRetries = 3
"#;

#[test]
fn go_outline_function() {
    let outline = ast::extract_outline(SAMPLE_GO, Language::Go).unwrap();
    let fns: Vec<&OutlineEntry> = outline.iter().filter(|e| e.kind == SemanticKind::Function).collect();
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "main");
}

#[test]
fn go_outline_struct_and_method() {
    let outline = ast::extract_outline(SAMPLE_GO, Language::Go).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Struct && e.name == "Server"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Method && e.name == "Server::Start"));
}

#[test]
fn go_outline_interface() {
    let outline = ast::extract_outline(SAMPLE_GO, Language::Go).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Interface && e.name == "Handler"));
}

#[test]
fn go_outline_const() {
    let outline = ast::extract_outline(SAMPLE_GO, Language::Go).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Const && e.name == "MaxRetries"));
}

// --- Java outline tests ---

const SAMPLE_JAVA: &str = r#"
public class Calculator {
    public Calculator() {}

    public int add(int a, int b) {
        return a + b;
    }

    public interface Operation {
        int apply(int a, int b);
    }

    public enum Mode {
        BASIC,
        SCIENTIFIC
    }
}
"#;

#[test]
fn java_outline_class_and_methods() {
    let outline = ast::extract_outline(SAMPLE_JAVA, Language::Java).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Class && e.name == "Calculator"),
        "expected Calculator class, got: {:?}", outline.iter().map(|e| (&e.kind, &e.name)).collect::<Vec<_>>());
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Constructor && e.name == "Calculator::Calculator"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Method && e.name == "Calculator::add"));
}

#[test]
fn java_outline_nested_interface() {
    let outline = ast::extract_outline(SAMPLE_JAVA, Language::Java).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Interface && e.name == "Calculator::Operation"));
}

#[test]
fn java_outline_nested_enum() {
    let outline = ast::extract_outline(SAMPLE_JAVA, Language::Java).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Enum && e.name == "Calculator::Mode"));
}

// --- C outline tests ---

const SAMPLE_C: &str = r#"
struct Point {
    int x;
    int y;
};

enum Color {
    RED,
    GREEN,
    BLUE
};

int add(int a, int b) {
    return a + b;
}

typedef unsigned int uint;
"#;

#[test]
fn c_outline_function() {
    let outline = ast::extract_outline(SAMPLE_C, Language::C).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Function && e.name == "add"),
        "got: {:?}", outline.iter().map(|e| (&e.kind, &e.name)).collect::<Vec<_>>());
}

#[test]
fn c_outline_struct() {
    let outline = ast::extract_outline(SAMPLE_C, Language::C).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Struct && e.name == "Point"));
}

#[test]
fn c_outline_enum() {
    let outline = ast::extract_outline(SAMPLE_C, Language::C).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Enum && e.name == "Color"));
}

#[test]
fn c_outline_typedef() {
    let outline = ast::extract_outline(SAMPLE_C, Language::C).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::TypeAlias && e.name == "uint"));
}

// --- C++ outline tests ---

const SAMPLE_CPP: &str = r#"
namespace math {

class Calculator {
public:
    Calculator() {}

    int add(int a, int b) {
        return a + b;
    }
};

}

using Integer = int;
"#;

#[test]
fn cpp_outline_namespace() {
    let outline = ast::extract_outline(SAMPLE_CPP, Language::Cpp).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Namespace && e.name == "math"),
        "got: {:?}", outline.iter().map(|e| (&e.kind, &e.name)).collect::<Vec<_>>());
}

#[test]
fn cpp_outline_class_and_methods() {
    let outline = ast::extract_outline(SAMPLE_CPP, Language::Cpp).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Class && e.name == "math::Calculator"),
        "got: {:?}", outline.iter().map(|e| (&e.kind, &e.name)).collect::<Vec<_>>());
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Constructor && e.name == "math::Calculator::Calculator"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Method && e.name == "math::Calculator::add"));
}

#[test]
fn cpp_outline_alias() {
    let outline = ast::extract_outline(SAMPLE_CPP, Language::Cpp).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::TypeAlias && e.name == "Integer"),
        "got: {:?}", outline.iter().map(|e| (&e.kind, &e.name)).collect::<Vec<_>>());
}

// --- Ruby outline tests ---

const SAMPLE_RUBY: &str = r#"
def top_level_method
  puts "hello"
end

module MyModule
  class MyClass < Base
    def initialize(name)
      @name = name
    end

    def greet
      "hello #{@name}"
    end

    def self.create(name)
      new(name)
    end
  end
end
"#;

#[test]
fn ruby_outline_top_level_function() {
    let outline = ast::extract_outline(SAMPLE_RUBY, Language::Ruby).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Function && e.name == "top_level_method"),
        "got: {:?}", outline.iter().map(|e| (&e.kind, &e.name)).collect::<Vec<_>>());
}

#[test]
fn ruby_outline_module() {
    let outline = ast::extract_outline(SAMPLE_RUBY, Language::Ruby).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Module && e.name == "MyModule"));
}

#[test]
fn ruby_outline_class_and_methods() {
    let outline = ast::extract_outline(SAMPLE_RUBY, Language::Ruby).unwrap();
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Class && e.name == "MyModule::MyClass"),
        "got: {:?}", outline.iter().map(|e| (&e.kind, &e.name)).collect::<Vec<_>>());
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Constructor && e.name == "MyModule::MyClass::initialize"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Method && e.name == "MyModule::MyClass::greet"));
    assert!(outline.iter().any(|e| e.kind == SemanticKind::Method && e.name == "MyModule::MyClass::create"));
}

// --- Cross-language anchor resolution ---

#[test]
fn anchor_resolution_works_for_python() {
    let outline = ast::extract_outline(SAMPLE_PY, Language::Python).unwrap();
    let m = ast::resolve_anchor(&outline, "method", "speak").unwrap();
    assert!(matches!(m, AnchorMatch::Qualified(_)));
    assert_eq!(m.entry().name, "Animal::speak");
}

#[test]
fn anchor_resolution_works_for_go() {
    let outline = ast::extract_outline(SAMPLE_GO, Language::Go).unwrap();
    let m = ast::resolve_anchor(&outline, "method", "Start").unwrap();
    assert!(matches!(m, AnchorMatch::Qualified(_)));
    assert_eq!(m.entry().name, "Server::Start");
}

// --- Line range validity across all languages ---

fn assert_valid_line_ranges(outline: &[OutlineEntry], lang: &str) {
    for entry in outline {
        assert!(entry.lines.start > 0, "{}: line start should be 1-based for {}", lang, entry.name);
        assert!(
            entry.lines.end >= entry.lines.start,
            "{}: end ({}) should be >= start ({}) for {}",
            lang, entry.lines.end, entry.lines.start, entry.name
        );
    }
}

#[test]
fn all_languages_have_valid_line_ranges() {
    let cases: &[(&str, Language)] = &[
        (SAMPLE_RUST, Language::Rust),
        (SAMPLE_TS, Language::TypeScript),
        (SAMPLE_JS, Language::JavaScript),
        (SAMPLE_PY, Language::Python),
        (SAMPLE_GO, Language::Go),
        (SAMPLE_JAVA, Language::Java),
        (SAMPLE_C, Language::C),
        (SAMPLE_CPP, Language::Cpp),
        (SAMPLE_RUBY, Language::Ruby),
    ];
    for (source, lang) in cases {
        let outline = ast::extract_outline(source, *lang).unwrap();
        assert_valid_line_ranges(&outline, &format!("{:?}", lang));
    }
}
