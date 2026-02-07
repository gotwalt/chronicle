# Feature 16: Multi-Language AST Parsing

## Overview

Feature 03 established tree-sitter AST parsing for Rust, with TypeScript/JavaScript and Python specified but deferred. This feature expands outline extraction and anchor resolution to thirteen languages: Rust, TypeScript, TSX, JavaScript, JSX, Python, Go, Java, C, C++, Ruby, Objective-C, and Swift.

Each language gets its own `outline_*.rs` extractor module. All languages use `::` as the internal qualified-name separator (e.g., `MyClass::my_method` even in Python/Go/Java), keeping the existing anchor resolution code in `anchor.rs` unchanged.

New `SemanticKind` variants are added for constructs not present in Rust: `Class`, `Interface`, `Namespace`, and `Constructor`.

---

## Dependencies

- **Feature 03 (AST Parsing):** Provides the outline/anchor infrastructure, `SemanticKind` enum, `OutlineEntry` struct, and Rust extractor. This feature extends all of these.
- **Feature 01 (CLI & Config):** Language grammars are selected via Cargo features.

---

## Languages & Grammars

| Language | Crate | Extensions | Feature flag |
|----------|-------|-----------|-------------|
| Rust | `tree-sitter-rust` 0.23 (exists) | `.rs` | `lang-rust` |
| TypeScript | `tree-sitter-typescript` (`LANGUAGE_TYPESCRIPT`) | `.ts`, `.mts`, `.cts` | `lang-typescript` |
| TSX | `tree-sitter-typescript` (`LANGUAGE_TSX`) | `.tsx` | `lang-typescript` |
| JavaScript | `tree-sitter-typescript` (`LANGUAGE_TYPESCRIPT`) | `.js`, `.mjs`, `.cjs` | `lang-typescript` |
| JSX | `tree-sitter-typescript` (`LANGUAGE_TSX`) | `.jsx` | `lang-typescript` |
| Python | `tree-sitter-python` | `.py`, `.pyi` | `lang-python` |
| Go | `tree-sitter-go` | `.go` | `lang-go` |
| Java | `tree-sitter-java` | `.java` | `lang-java` |
| C | `tree-sitter-c` | `.c`, `.h` | `lang-c` |
| C++ | `tree-sitter-cpp` | `.cc`, `.cpp`, `.cxx`, `.hpp`, `.hxx`, `.hh` | `lang-cpp` |
| Ruby | `tree-sitter-ruby` | `.rb`, `.rake`, `.gemspec` | `lang-ruby` |
| Objective-C | `tree-sitter-objc` 3 | `.m`, `.mm` | `lang-objc` |
| Swift | `tree-sitter-swift` 0.7 | `.swift` | `lang-swift` |

JavaScript and JSX reuse the TypeScript grammar crate — the TypeScript parser is a strict superset of JavaScript. JavaScript files use `LANGUAGE_TYPESCRIPT`, JSX files use `LANGUAGE_TSX`. Both share the `outline_typescript.rs` extractor and the `lang-typescript` feature flag.

**Key decision:** `::` is the universal qualified-name separator for all languages. This avoids any changes to `anchor.rs` and its exact/qualified/fuzzy matching logic.

---

## New SemanticKind Variants

| Variant | `as_str()` | `from_str_loose()` aliases |
|---------|-----------|--------------------------|
| `Class` | `"class"` | `"class"` |
| `Interface` | `"interface"` | `"interface"`, `"trait"`, `"protocol"` |
| `Extension` | `"extension"` | `"extension"`, `"impl"`, `"category"` |
| `Namespace` | `"namespace"` | `"namespace"`, `"package"` |
| `Constructor` | `"constructor"` | `"constructor"`, `"ctor"` |

The previous `Trait` variant has been merged into `Interface` (traits, protocols, and interfaces are all contracts). The previous `Impl` variant has been renamed to `Extension` (Rust impl blocks, Swift extensions, and ObjC categories all add implementation to existing types). Backward compatibility is maintained via `from_str_loose()` aliases.

---

## File Organization

```
src/ast/
  mod.rs                 -- expand Language enum + extract_outline dispatcher
  outline.rs             -- add SemanticKind variants, make helpers pub(crate)
  anchor.rs              -- unchanged
  outline_typescript.rs  -- new: TypeScript/TSX/JavaScript/JSX extractor
  outline_python.rs      -- new: Python extractor
  outline_go.rs          -- new: Go extractor
  outline_java.rs        -- new: Java extractor
  outline_c.rs           -- new: C extractor
  outline_cpp.rs         -- new: C++ extractor
  outline_ruby.rs        -- new: Ruby extractor
```

---

## Changes by File

### 1. `Cargo.toml`

Add optional grammar dependencies. Make `tree-sitter-rust` optional (it already should be behind `lang-rust`):

```toml
tree-sitter-rust = { version = "0.23", optional = true }
tree-sitter-typescript = { version = "0.23", optional = true }
tree-sitter-python = { version = "0.23", optional = true }
tree-sitter-go = { version = "0.23", optional = true }
tree-sitter-java = { version = "0.23", optional = true }
tree-sitter-c = { version = "0.23", optional = true }
tree-sitter-cpp = { version = "0.23", optional = true }
tree-sitter-ruby = { version = "0.23", optional = true }

[features]
default = ["tui", "lang-rust", "lang-typescript", "lang-python", "lang-go", "lang-java", "lang-c", "lang-cpp", "lang-ruby"]
tui = ["ratatui", "crossterm"]
lang-rust = ["dep:tree-sitter-rust"]
lang-typescript = ["dep:tree-sitter-typescript"]
lang-python = ["dep:tree-sitter-python"]
lang-go = ["dep:tree-sitter-go"]
lang-java = ["dep:tree-sitter-java"]
lang-c = ["dep:tree-sitter-c"]
lang-cpp = ["dep:tree-sitter-cpp"]
lang-ruby = ["dep:tree-sitter-ruby"]
```

All language features are enabled by default. Users who want a smaller binary can build with `--no-default-features --features tui,lang-rust`.

### 2. `src/ast/outline.rs`

- Add `Class`, `Interface`, `Namespace`, `Constructor` to `SemanticKind` with serde rename and `as_str()` / `from_str_loose()` arms
- Extract shared helpers as `pub(crate)`:
  - `pub(crate) fn node_line_range(node: tree_sitter::Node) -> (u32, u32)` — returns 1-indexed inclusive line range
  - `pub(crate) fn extract_signature(node: tree_sitter::Node, source: &[u8], delimiter: char) -> String` — existing logic, make visible to sibling modules
  - `pub(crate) fn should_skip_node(node: &tree_sitter::Node) -> bool` — ERROR/missing node check
- Gate the Rust-specific extraction logic with `#[cfg(feature = "lang-rust")]`

### 3. `src/ast/mod.rs`

Expand the `Language` enum and dispatcher:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Jsx,
    Python,
    Go,
    Java,
    C,
    Cpp,
    Ruby,
    Unsupported,
}

impl Language {
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Self::Rust,
            "ts" | "mts" | "cts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "js" | "mjs" | "cjs" => Self::JavaScript,
            "jsx" => Self::Jsx,
            "py" | "pyi" => Self::Python,
            "go" => Self::Go,
            "java" => Self::Java,
            "c" | "h" => Self::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" => Self::Cpp,
            "rb" | "rake" | "gemspec" => Self::Ruby,
            _ => Self::Unsupported,
        }
    }
}
```

Add `#[cfg]`-gated module declarations:

```rust
#[cfg(feature = "lang-typescript")]
mod outline_typescript;
#[cfg(feature = "lang-python")]
mod outline_python;
#[cfg(feature = "lang-go")]
mod outline_go;
#[cfg(feature = "lang-java")]
mod outline_java;
#[cfg(feature = "lang-c")]
mod outline_c;
#[cfg(feature = "lang-cpp")]
mod outline_cpp;
#[cfg(feature = "lang-ruby")]
mod outline_ruby;
```

The `extract_outline` dispatcher routes to the appropriate module:

```rust
pub fn extract_outline(source: &str, language: Language) -> Result<Vec<OutlineEntry>, AstError> {
    match language {
        #[cfg(feature = "lang-rust")]
        Language::Rust => outline::extract_rust_outline(source),
        #[cfg(feature = "lang-typescript")]
        Language::TypeScript | Language::JavaScript => outline_typescript::extract_typescript_outline(source, false),
        #[cfg(feature = "lang-typescript")]
        Language::Tsx | Language::Jsx => outline_typescript::extract_typescript_outline(source, true),
        #[cfg(feature = "lang-python")]
        Language::Python => outline_python::extract_python_outline(source),
        #[cfg(feature = "lang-go")]
        Language::Go => outline_go::extract_go_outline(source),
        #[cfg(feature = "lang-java")]
        Language::Java => outline_java::extract_java_outline(source),
        #[cfg(feature = "lang-c")]
        Language::C => outline_c::extract_c_outline(source),
        #[cfg(feature = "lang-cpp")]
        Language::Cpp => outline_cpp::extract_cpp_outline(source),
        #[cfg(feature = "lang-ruby")]
        Language::Ruby => outline_ruby::extract_ruby_outline(source),
        Language::Unsupported => Ok(vec![]),
        // Feature-disabled arms:
        _ => UnsupportedLanguageSnafu { /* ... */ }.fail(),
    }
}
```

### 4. `src/show/data.rs` (line ~103)

Change the current `extract_rust_outline(&source)` call to use the language-aware dispatcher:

```rust
let lang = crate::ast::Language::from_path(file_path);
let outline = crate::ast::extract_outline(&source, lang).unwrap_or_default();
```

### 5. Seven New `outline_*.rs` Modules

Each module exports a single public function and is gated behind its feature flag. All use the shared helpers from `outline.rs`. The TypeScript module handles JavaScript/JSX as well (same grammar).

---

## Per-Language Node-Kind Mappings

### TypeScript / JavaScript (`outline_typescript.rs`)

```rust
pub fn extract_typescript_outline(source: &str, is_tsx: bool) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `function_declaration` | Function | `name` child text | |
| `class_declaration` | Class | `name` child text | Descend for methods |
| `method_definition` | Method / Constructor | `ClassName::name` | `constructor` name -> Constructor kind |
| `interface_declaration` | Interface | `name` child text | |
| `enum_declaration` | Enum | `name` child text | |
| `type_alias_declaration` | TypeAlias | `name` child text | |
| `export_statement` | (transparent) | Unwrap to inner | |
| `lexical_declaration` | Function | Variable name | Only when value is `arrow_function` |

**Grammar:** `tree_sitter_typescript::LANGUAGE_TYPESCRIPT` or `LANGUAGE_TSX` based on the `is_tsx` parameter. JavaScript uses `LANGUAGE_TYPESCRIPT`, JSX uses `LANGUAGE_TSX` — the TypeScript grammar is a strict superset of JavaScript.

**Signature delimiter:** `{`

### Python (`outline_python.rs`)

```rust
pub fn extract_python_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `function_definition` | Function / Method / Constructor | `name` child text | Method if inside class; Constructor if name is `__init__` |
| `class_definition` | Class | `name` child text | Descend into `block` for methods |
| `decorated_definition` | (transparent) | Unwrap to inner def/class | Use outer node's line range (includes decorator) |

**Qualified names:** `ClassName::method_name`, `ClassName::__init__`

**Signature delimiter:** `:` (Python uses `:` before the body, not `{`). Custom extraction: read from node start to body byte offset.

**Grammar:** `tree_sitter_python::LANGUAGE`

### Go (`outline_go.rs`)

```rust
pub fn extract_go_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `function_declaration` | Function | `name` child text | |
| `method_declaration` | Method | `ReceiverType::name` | Extract receiver type from `parameters` child |
| `type_spec` with `struct_type` | Struct | `name` child text | Only when inner type is `struct_type` |
| `type_spec` with `interface_type` | Interface | `name` child text | Only when inner type is `interface_type` |
| `type_spec` (other) | TypeAlias | `name` child text | All other type specs |
| `const_spec` | Constant | `name` child text | Inside `const_declaration` |

**Qualified names:** `ReceiverType::method_name` (e.g., `Server::Start`). The receiver type is extracted from the method's parameter list (the `(s *Server)` part).

**Signature delimiter:** `{`

**Grammar:** `tree_sitter_go::LANGUAGE`

### Java (`outline_java.rs`)

```rust
pub fn extract_java_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `class_declaration` | Class | `name` child text | Descend for methods |
| `interface_declaration` | Interface | `name` child text | Descend for methods |
| `enum_declaration` | Enum | `name` child text | |
| `record_declaration` | Struct | `name` child text | Java records map to Struct |
| `method_declaration` | Method | `ClassName::name` | |
| `constructor_declaration` | Constructor | `ClassName::ClassName` | |

**Nested classes:** `OuterClass::InnerClass::method_name`

**Signature delimiter:** `{`

**Grammar:** `tree_sitter_java::LANGUAGE`

### C (`outline_c.rs`)

```rust
pub fn extract_c_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `function_definition` | Function | Recursive declarator chain for name | C function names are nested in declarators |
| `struct_specifier` (with body) | Struct | Tag name | Only when `field_declaration_list` child present |
| `enum_specifier` (with body) | Enum | Tag name | Only when `enumerator_list` child present |
| `union_specifier` (with body) | Struct | Tag name | Unions map to Struct kind |
| `type_definition` | TypeAlias | Declarator name | `typedef` |

**Name extraction for functions:** C function names require walking the declarator chain. A `function_definition` has a `declarator` child which may be a `function_declarator`, whose first child is either an `identifier` or another `pointer_declarator` wrapping an `identifier`.

**Signature delimiter:** `{`

**Grammar:** `tree_sitter_c::LANGUAGE`

### C++ (`outline_cpp.rs`)

```rust
pub fn extract_cpp_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

C++ includes all C node kinds plus:

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `class_specifier` (with body) | Class | Tag name | Descend for methods |
| `namespace_definition` | Namespace | `name` child text | Descend with qualified prefix |
| `template_declaration` | (transparent) | Unwrap to inner | |
| `function_definition` (in class body) | Method / Constructor | `ClassName::name` | Constructor if name == class name |
| `alias_declaration` | TypeAlias | `name` child text | `using Foo = Bar;` |

**Nested namespaces:** `Outer::Inner::function_name`

**Constructor detection:** A function definition inside a class body whose name matches the class name is a Constructor.

**Signature delimiter:** `{`

**Grammar:** `tree_sitter_cpp::LANGUAGE`

### Ruby (`outline_ruby.rs`)

```rust
pub fn extract_ruby_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `method` | Function / Method | `name` child text | Method if inside class/module body |
| `singleton_method` | Method | `ClassName::name` | Class-level methods (`def self.foo`) |
| `class` | Class | `name` child text | Descend for methods |
| `module` | Module | `name` child text | Descend with qualified prefix |
| `alias` | (skip) | — | Method aliases are not structural |

**Qualified names:** `ClassName::method_name`, `Module::ClassName::method_name`. Nested modules and classes produce nested `::` prefixes.

**`initialize` method:** Maps to `Constructor` kind (Ruby's constructor convention). Name is still `ClassName::initialize`.

**Singleton methods:** `def self.method_name` inside a class produces a Method with `ClassName::method_name`. The `singleton_method` node has an `object` child (`self`) and a `name` child.

**Signature delimiter:** Ruby has no `{` or `:` body delimiter. Extract from `def` keyword to end of parameter list (closing `)`), or to end of method name if no parameters. For classes/modules, extract the `class`/`module` keyword plus the name and any superclass (`< Base`).

**Grammar:** `tree_sitter_ruby::LANGUAGE`

### Objective-C (`outline_objc.rs`)

```rust
pub fn extract_objc_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `class_interface` | Class | First identifier child | Descend for method declarations |
| `class_implementation` | Class | First identifier child | Descend for method definitions |
| `protocol_declaration` | Interface | First identifier child | Descend for method declarations; skip forward declarations |
| `class_interface` (with `category` field) | Extension | `ClassName(CategoryName)` | Category interface |
| `class_implementation` (with `category` field) | Extension | `ClassName(CategoryName)` | Category implementation |
| `method_declaration` / `method_definition` | Method | `ClassName::±selector` | `+` for class methods, `-` for instance methods |
| `function_definition` | Function | Declarator chain name | C-style free functions |

**Qualified names:** `ClassName::±selectorName:` (e.g., `Shape::-initWithName:`, `Shape::+defaultShape`). The `+`/`-` prefix distinguishes class methods from instance methods. Multi-part selectors include colons (e.g., `initWithName:age:`).

**Signature delimiter:** `{` for method definitions and functions, `;` for method declarations.

**Grammar:** `tree_sitter_objc::LANGUAGE`

### Swift (`outline_swift.rs`)

```rust
pub fn extract_swift_outline(source: &str) -> Result<Vec<OutlineEntry>, AstError>
```

| tree-sitter node kind | SemanticKind | Name extraction | Notes |
|----------------------|--------------|-----------------|-------|
| `class_declaration` (kind=class/actor) | Class | `name` field | Descend into `body` for methods |
| `class_declaration` (kind=struct) | Struct | `name` field | Descend into `body` for methods |
| `class_declaration` (kind=enum) | Enum | `name` field | Descend into `body` for methods |
| `class_declaration` (kind=extension) | Extension | `name` field (extended type) | Descend into `body` for methods |
| `protocol_declaration` | Interface | `name` field | Descend into `body` for protocol methods |
| `function_declaration` / `protocol_function_declaration` | Function / Method | `name` field | Method if inside type body |
| `init_declaration` | Constructor | `TypeName::init` | Only inside type bodies |
| `typealias_declaration` | TypeAlias | `name` field | |

**Note:** The Swift grammar uses a single `class_declaration` node for class, struct, enum, actor, and extension, differentiated by the `declaration_kind` field. Protocol methods use `protocol_function_declaration` rather than `function_declaration`.

**Qualified names:** `TypeName::methodName` (e.g., `Shape::draw`, `Vehicle::init`).

**Signature delimiter:** `{`

**Grammar:** `tree_sitter_swift::LANGUAGE`

---

## Anchor Resolution

No changes to `src/ast/anchor.rs`. The existing exact/qualified/fuzzy/ambiguous/not-found logic works for all languages because all extractors produce `::` as the qualified-name separator.

---

## Error Handling

Uses the existing `AstError` enum from Feature 03. The `GrammarNotAvailable` variant covers the case where a language feature is not compiled in.

When a feature flag is disabled and the user requests parsing for that language:
- `extract_outline()` returns `Err(AstError::GrammarNotAvailable { language })`
- The caller (writing agent, read pipeline, show command) handles this by falling back to line-range-only annotation

---

## Configuration

No runtime configuration. Language support is determined by compiled Cargo features, same as Feature 03.

---

## Implementation Steps

### Step 1: Expand SemanticKind

Add `Class`, `Interface`, `Namespace`, `Constructor` variants to `SemanticKind` in `outline.rs`. Update `as_str()`, `from_str_loose()`, and serde attributes.

**Deliverable:** Existing tests pass. New variants serialize/deserialize correctly.

### Step 2: Extract Shared Helpers

Make `node_line_range`, `extract_signature`, and `should_skip_node` `pub(crate)` in `outline.rs`. Gate the Rust-specific extraction with `#[cfg(feature = "lang-rust")]`.

**Deliverable:** Rust extraction still works identically. Helpers are accessible from sibling modules.

### Step 3: Expand Language Enum and Dispatcher

Add `JavaScript`, `Jsx`, `Go`, `Java`, `C`, `Cpp`, `Ruby` to the `Language` enum. Update `from_extension()` with all new mappings. Add stub dispatcher arms that return `UnsupportedLanguage` errors (implementations come in later steps).

**Deliverable:** `Language::from_extension("go")` returns `Language::Go`, `Language::from_extension("js")` returns `Language::JavaScript`, `Language::from_extension("rb")` returns `Language::Ruby`. `extract_outline()` for new languages returns an appropriate error until implemented.

### Step 4: Cargo.toml Feature Flags

Add optional dependencies for all new grammar crates. Update the `[features]` section.

**Deliverable:** `cargo build --no-default-features --features lang-rust` compiles without the new grammars.

### Step 5: TypeScript/TSX/JavaScript/JSX Extractor

Implement `outline_typescript.rs`. Cover: function declarations, class declarations (with method descent), interface declarations, enum declarations, type alias declarations, export statements (transparent), const arrow functions. The same module handles JavaScript and JSX via grammar selection (`LANGUAGE_TYPESCRIPT` vs `LANGUAGE_TSX`).

**Deliverable:** TypeScript, TSX, JavaScript, and JSX files produce correct outlines. Anchor resolution works via the existing `anchor.rs`.

### Step 6: Python Extractor

Implement `outline_python.rs`. Cover: function definitions, class definitions (with method descent), decorated definitions (transparent with outer line range), `__init__` as Constructor.

**Deliverable:** Python files produce correct outlines with `ClassName::method` qualified names.

### Step 7: Go Extractor

Implement `outline_go.rs`. Cover: function declarations, method declarations (with receiver type extraction), type specs (struct/interface/alias), const specs.

**Deliverable:** Go files produce correct outlines with `ReceiverType::method` qualified names.

### Step 8: Java Extractor

Implement `outline_java.rs`. Cover: class/interface/enum/record declarations, method declarations, constructor declarations, nested class descent.

**Deliverable:** Java files produce correct outlines with `ClassName::method` qualified names.

### Step 9: C Extractor

Implement `outline_c.rs`. Cover: function definitions (with declarator chain walking), struct/enum/union specifiers (with body), type definitions.

**Deliverable:** C files produce correct outlines. Function names are correctly extracted from nested declarators.

### Step 10: C++ Extractor

Implement `outline_cpp.rs`. Build on C extraction, adding: class specifiers (with method descent), namespace definitions (with prefix propagation), template declarations (transparent), alias declarations, constructor detection.

**Deliverable:** C++ files produce correct outlines with `Namespace::Class::method` qualified names.

### Step 11: Ruby Extractor

Implement `outline_ruby.rs`. Cover: method definitions (instance and singleton), class definitions (with method descent), module definitions (with prefix propagation), `initialize` as Constructor.

**Deliverable:** Ruby files produce correct outlines with `Module::Class::method` qualified names.

### Step 12: Update Show Command

Update `src/show/data.rs` to use `Language::from_path` + `extract_outline` instead of hardcoded `extract_rust_outline`.

**Deliverable:** `git chronicle show` works for all supported languages.

### Step 13: Integration Tests

Add sample code and tests for each language. Verify outline extraction, anchor resolution, and feature-disabled error path.

**Deliverable:** All new tests pass. Existing tests unaffected.

---

## Test Plan

### Unit Tests

**Language detection (expand existing tests):**
- `.ts`, `.mts`, `.cts` -> TypeScript
- `.tsx` -> Tsx
- `.js`, `.mjs`, `.cjs` -> JavaScript
- `.jsx` -> Jsx
- `.py`, `.pyi` -> Python
- `.go` -> Go
- `.java` -> Java
- `.c`, `.h` -> C
- `.cc`, `.cpp`, `.cxx`, `.hpp`, `.hxx`, `.hh` -> Cpp
- `.rb`, `.rake`, `.gemspec` -> Ruby
- `.rs` -> unchanged

**SemanticKind serialization:**
- `Class` -> `"class"`, `Interface` -> `"interface"`, `Namespace` -> `"namespace"`, `Constructor` -> `"constructor"`
- Round-trip through serde
- `from_str_loose("ctor")` -> `Constructor`, `from_str_loose("package")` -> `Namespace`

**TypeScript outline extraction:**
- Function declaration, exported function
- Class with constructor, methods, static methods
- Interface declaration
- Enum declaration
- Type alias
- `const fn = () => {}` arrow function pattern
- `export default class` (transparent export)
- Async functions and generators
- TSX file with JSX elements (elements are ignored, only declarations extracted)

**JavaScript outline extraction (same extractor as TypeScript):**
- Function declarations and expressions
- Class with constructor and methods
- `const fn = () => {}` arrow function pattern
- `module.exports` / `export default` (transparent)
- CommonJS-style `.mjs` and `.cjs` files parse correctly
- JSX file with JSX elements (elements are ignored, only declarations extracted)
- No TypeScript-specific nodes (interfaces, type aliases, enums) — these are absent from JS source and the extractor handles their absence gracefully

**Python outline extraction:**
- Top-level functions
- Class with `__init__` (Constructor), regular methods, `@staticmethod`, `@classmethod`
- Decorated functions (outer line range includes decorator)
- Functions with type annotations
- Nested classes with qualified names

**Go outline extraction:**
- Function declarations
- Method declarations with pointer and value receivers
- Struct type specs, interface type specs, type aliases
- Constants inside `const` blocks
- Multiple return values in signature

**Java outline extraction:**
- Class with methods and constructor
- Interface with method signatures
- Enum declaration
- Record declaration (-> Struct kind)
- Nested classes with qualified names
- Static methods, abstract methods

**C outline extraction:**
- Function definitions (simple and pointer-returning)
- Struct, enum, union with bodies (forward declarations excluded)
- Typedefs
- Function pointer typedefs (name extraction from declarator chain)
- Header file with only declarations (no function bodies -> no Function entries)

**C++ outline extraction:**
- All C tests, plus:
- Class with methods, constructor, destructor
- Namespace with nested functions and classes
- Template functions and classes (transparent template_declaration)
- `using` alias declarations
- Nested `Namespace::Class::method` qualified names

**Ruby outline extraction:**
- Top-level methods (`def foo`)
- Class with instance methods, `initialize` (Constructor)
- Singleton methods (`def self.bar`)
- Module with nested classes and methods
- Nested `Module::Class::method` qualified names
- Method with parameters and default values in signature
- Class with superclass (`class Foo < Bar`) in signature
- Open classes (multiple `class Foo` blocks) — each block's methods are extracted independently

**Anchor resolution (all languages):**
- Exact match works with `::` separator
- Qualified match: `"method"` matches `"ClassName::method"`
- Fuzzy match: typo in method name, distance <= 3
- Not found for unrelated names

**Feature-disabled path:**
- Build with a language feature disabled
- `extract_outline()` for that language returns `AstError::GrammarNotAvailable`

### Integration Tests

- Parse a multi-file project fixture with mixed languages, verify all outlines
- Round-trip: extract outline, resolve each name as anchor, verify line ranges match
- `git chronicle show` on a TypeScript/JavaScript/Python/Go/Ruby file produces correct output

### Property Tests

- For any supported language source, `extract_outline()` never panics
- All line ranges satisfy `line_start <= line_end`
- Outline entries are sorted by `line_start`
- Every entry name can be resolved via `resolve_anchor()` to `Exact` or `Qualified`

---

## Acceptance Criteria

1. `Language::from_extension()` correctly identifies all languages (Rust, TypeScript, TSX, JavaScript, JSX, Python, Go, Java, C, C++, Ruby) and their file extensions.
2. `SemanticKind` includes `Class`, `Interface`, `Namespace`, and `Constructor` with correct serialization.
3. TypeScript/TSX outline extraction handles function declarations, classes with methods/constructors, interfaces, enums, type aliases, exported items, and const arrow functions.
4. JavaScript/JSX files are parsed by the same TypeScript extractor and produce correct outlines (no TypeScript-specific nodes expected in JS source).
5. Python outline extraction handles function definitions, classes with methods, `__init__` as Constructor, and decorated definitions with correct outer line ranges.
6. Go outline extraction handles function/method declarations (with receiver types), struct/interface type specs, and constants.
7. Java outline extraction handles classes, interfaces, enums, records, methods, constructors, and nested classes.
8. C outline extraction handles function definitions (with declarator chain walking), struct/enum/union specifiers, and typedefs.
9. C++ outline extraction handles everything C does, plus classes, namespaces, templates (transparent), alias declarations, and constructor detection.
10. Ruby outline extraction handles instance methods, singleton methods, classes (with `initialize` as Constructor), modules (with prefix propagation), and nested qualified names.
11. All languages use `::` as the qualified-name separator, and anchor resolution works unchanged.
12. Language grammars are behind individual Cargo feature flags. Building with a language disabled produces `GrammarNotAvailable` errors for that language, not compilation failures.
13. `git chronicle show` dispatches to the correct language extractor based on file extension.
14. All existing tests continue to pass without modification (beyond adding `corrections: vec![]` if new `RegionAnnotation` instances are created).
15. Outline entries across all languages are sorted by `line_start` and use 1-indexed inclusive line ranges.
