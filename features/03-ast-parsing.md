# Feature 03: Tree-sitter AST Parsing

## Overview

The AST parsing layer gives Ultragit structural understanding of source code. Instead of operating on raw lines and diffs, Ultragit can identify semantic units — functions, methods, structs, classes, impl blocks, constants, type definitions — by name, signature, and line range. This enables two critical capabilities:

1. **Outline extraction** — the writing agent uses outlines to anchor annotations to named code elements rather than brittle line numbers.
2. **Anchor resolution** — the read pipeline resolves a name like `MqttClient::connect` to a line range, then feeds that range to blame.

Both capabilities use tree-sitter for parsing. Tree-sitter is fast (single-digit milliseconds for typical files), incremental, error-tolerant (partial parses still produce usable trees), and supports many languages via loadable grammars.

---

## Dependencies

- **Feature 01 (CLI & Config):** Uses `UltragitConfig` for repository root. Language grammars are selected via Cargo features defined in the project's `Cargo.toml`.

---

## Public API

### Types

```rust
use std::path::{Path, PathBuf};

/// A semantic unit in a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry {
    /// The kind of semantic unit.
    pub kind: SemanticKind,

    /// The name of the unit. For methods, includes the impl target:
    /// "MqttClient::connect". For free functions, just the name: "main".
    pub name: String,

    /// The full signature (for functions/methods) or declaration line
    /// (for types). Includes visibility, generics, and parameters.
    /// Example: "pub fn connect(&mut self, config: &MqttConfig) -> Result<()>"
    pub signature: String,

    /// Line range (1-indexed, inclusive) covering the entire definition,
    /// including the body.
    pub line_start: u32,
    pub line_end: u32,

    /// For methods: the enclosing impl/class name.
    /// For nested items: the parent entry name.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Impl,
    Class,
    Interface,
    TypeAlias,
    Constant,
    Static,
    Module,
    /// Python: decorated function/class
    Decorator,
}

/// Result of anchor resolution.
#[derive(Debug, Clone)]
pub enum AnchorMatch {
    /// Exact match on the name.
    Exact(OutlineEntry),
    /// Qualified match (e.g., "connect" matched "MqttClient::connect").
    Qualified(OutlineEntry),
    /// Fuzzy match with edit distance.
    Fuzzy {
        entry: OutlineEntry,
        distance: u32,
        query: String,
    },
    /// Multiple matches found — ambiguous.
    Ambiguous(Vec<OutlineEntry>),
    /// No match found.
    NotFound { query: String, available: Vec<String> },
}

/// Supported languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Tsx,
    Jsx,
    Python,
    Unsupported,
}
```

### Core Functions

```rust
/// Detect language from file extension.
pub fn detect_language(path: &Path) -> Language;

/// Parse a file and extract its outline.
/// Returns an empty Vec for unsupported languages (with a tracing::warn).
pub fn extract_outline(source: &str, language: Language) -> Result<Vec<OutlineEntry>>;

/// Resolve an anchor name to a line range in the given file.
/// Parses the file, extracts the outline, and searches for the name.
pub fn resolve_anchor(
    source: &str,
    language: Language,
    anchor_name: &str,
) -> Result<AnchorMatch>;

/// Extract outline from a file path (convenience wrapper that reads
/// the file, detects language, and calls extract_outline).
pub fn outline_for_file(path: &Path) -> Result<Vec<OutlineEntry>>;
```

---

## Internal Design

### Language Detection

File extension to language mapping:

| Extension | Language |
|-----------|----------|
| `.rs` | Rust |
| `.ts` | TypeScript |
| `.tsx` | Tsx |
| `.js` | JavaScript |
| `.jsx` | Jsx |
| `.py` | Python |
| everything else | Unsupported |

```rust
pub fn detect_language(path: &Path) -> Language {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => Language::Rust,
        Some("ts") => Language::TypeScript,
        Some("tsx") => Language::Tsx,
        Some("js") => Language::JavaScript,
        Some("jsx") => Language::Jsx,
        Some("py") => Language::Python,
        _ => Language::Unsupported,
    }
}
```

This is deliberately simple. No content-based detection, no shebang parsing. Extensions are reliable enough for this use case and avoid the cost of reading file content just to determine the language.

### Grammar Loading

Each language grammar is compiled into the binary via its tree-sitter crate. Grammar initialization is done once per language per process and cached.

```rust
use std::sync::OnceLock;
use tree_sitter::Language as TsLanguage;

fn rust_language() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_rust::LANGUAGE.into())
}

fn typescript_language() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
}

fn tsx_language() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_typescript::LANGUAGE_TSX.into())
}

fn python_language() -> &'static TsLanguage {
    static LANG: OnceLock<TsLanguage> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_python::LANGUAGE.into())
}

fn get_ts_language(lang: Language) -> Option<&'static TsLanguage> {
    match lang {
        Language::Rust => Some(rust_language()),
        Language::TypeScript => Some(typescript_language()),
        Language::Tsx | Language::Jsx => Some(tsx_language()),
        Language::JavaScript => Some(typescript_language()), // TS parser handles JS
        Language::Python => Some(python_language()),
        Language::Unsupported => None,
    }
}
```

### Cargo Features for Optional Grammars

Each language grammar is behind a Cargo feature so users can build a smaller binary:

```toml
[features]
default = ["lang-rust", "lang-typescript", "lang-python"]
lang-rust = ["dep:tree-sitter-rust"]
lang-typescript = ["dep:tree-sitter-typescript"]
lang-python = ["dep:tree-sitter-python"]

[dependencies]
tree-sitter = "0.24"
tree-sitter-rust = { version = "0.23", optional = true }
tree-sitter-typescript = { version = "0.23", optional = true }
tree-sitter-python = { version = "0.23", optional = true }
```

The grammar loading functions check feature availability at compile time:

```rust
fn get_ts_language(lang: Language) -> Option<&'static TsLanguage> {
    match lang {
        #[cfg(feature = "lang-rust")]
        Language::Rust => Some(rust_language()),
        #[cfg(feature = "lang-typescript")]
        Language::TypeScript | Language::JavaScript => Some(typescript_language()),
        #[cfg(feature = "lang-typescript")]
        Language::Tsx | Language::Jsx => Some(tsx_language()),
        #[cfg(feature = "lang-python")]
        Language::Python => Some(python_language()),
        _ => None,
    }
}
```

### Runtime Grammar Loading (Future)

For extensibility beyond the compiled-in grammars, reserve a path for loading grammars at runtime from `.so`/`.dylib` files:

```rust
/// Future: load a grammar from a shared library.
/// Not implemented in v1 — returns Err.
pub fn load_grammar_runtime(path: &Path) -> Result<TsLanguage> {
    RuntimeLoadingNotSupportedSnafu.fail()
}
```

The infrastructure for this (a grammar registry, a search path like `~/.ultragit/grammars/`) is deferred. The API surface is reserved so that adding it later doesn't break the trait.

### Outline Extraction

The core of this feature. Given parsed source, walk the tree-sitter CST and extract semantic units.

#### General Algorithm

```
1. Parse source with tree-sitter → Tree
2. Walk the tree with a cursor (depth-first)
3. At each node, check if it matches a semantic unit pattern for the language
4. If yes, extract: kind, name, signature, line range, parent context
5. Collect into Vec<OutlineEntry>
```

The walk uses tree-sitter's `TreeCursor` for efficient traversal without allocating child vectors.

#### Rust Extraction Rules

| tree-sitter node type | SemanticKind | Name extraction |
|----------------------|--------------|-----------------|
| `function_item` | Function | `name` child node text |
| `struct_item` | Struct | `name` child (type_identifier) text |
| `enum_item` | Enum | `name` child text |
| `trait_item` | Trait | `name` child text |
| `impl_item` | Impl | `type` child text (the target type) |
| `type_item` | TypeAlias | `name` child text |
| `const_item` | Constant | `name` child text |
| `static_item` | Static | `name` child text |
| `mod_item` | Module | `name` child text |

For methods inside `impl` blocks:

- Walk into `impl_item` → `declaration_list` children.
- `function_item` nodes inside an `impl_item` become `Method` with `parent` set to the impl target.
- Name is `ImplTarget::method_name` (e.g., `MqttClient::connect`).

Signature extraction: read from the node's start byte to the opening `{` of the body (or end of node for bodyless items like trait declarations). Trim whitespace.

#### TypeScript/JavaScript Extraction Rules

| tree-sitter node type | SemanticKind | Name extraction |
|----------------------|--------------|-----------------|
| `function_declaration` | Function | `name` child text |
| `class_declaration` | Class | `name` child text |
| `interface_declaration` | Interface | `name` child text |
| `type_alias_declaration` | TypeAlias | `name` child text |
| `method_definition` (inside class) | Method | `ClassName::method_name` |
| `lexical_declaration` with `const` + arrow function | Function | variable name |
| `export_statement` wrapping any of the above | (same, extract inner) | (same) |

Arrow functions assigned to `const` are common in TS/JS. Detect pattern:
```
const handler = async (req, res) => { ... }
```
This is a `lexical_declaration` → `variable_declarator` with an `arrow_function` value. Name is the variable name, kind is Function.

#### Python Extraction Rules

| tree-sitter node type | SemanticKind | Name extraction |
|----------------------|--------------|-----------------|
| `function_definition` | Function (or Method if inside class) | `name` child text |
| `class_definition` | Class | `name` child text |
| `decorated_definition` | (unwrap to inner) | (inner node's name) |

For methods inside classes:
- `function_definition` nodes inside `class_definition` → `block` become Method.
- Name is `ClassName::method_name`.
- `__init__`, `__str__`, etc. are included (they're methods).

Decorated functions: `decorated_definition` wraps a `function_definition` or `class_definition`. Unwrap to the inner node for name/kind extraction. The decorator itself is noted in the signature.

### Signature Extraction

For each outline entry, extract a human-readable signature. The strategy:

1. Find the node's text from start to the body delimiter.
   - Rust: from node start to the first `{`.
   - TypeScript/Python: from node start to the first `:` (Python) or `{` (TS).
2. Collapse whitespace and newlines into single spaces.
3. Trim to a reasonable length (512 chars max).

```rust
fn extract_signature(source: &str, node: &tree_sitter::Node, language: Language) -> String {
    let node_text = &source[node.start_byte()..node.end_byte()];

    let body_delimiters = match language {
        Language::Rust | Language::TypeScript | Language::JavaScript
        | Language::Tsx | Language::Jsx => ['{'],
        Language::Python => [':'],
        Language::Unsupported => return String::new(),
    };

    let sig_end = node_text
        .find(|c: char| body_delimiters.contains(&c))
        .unwrap_or(node_text.len());

    let sig = &node_text[..sig_end];
    let collapsed: String = sig.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.len() > 512 {
        format!("{}...", &collapsed[..509])
    } else {
        collapsed
    }
}
```

### Anchor Resolution

Given a name string and a file's outline, find the matching entry.

```rust
pub fn resolve_anchor(
    source: &str,
    language: Language,
    anchor_name: &str,
) -> Result<AnchorMatch> {
    let outline = extract_outline(source, language)?;

    if outline.is_empty() {
        return Ok(AnchorMatch::NotFound {
            query: anchor_name.to_string(),
            available: vec![],
        });
    }

    // 1. Exact match
    let exact: Vec<_> = outline.iter()
        .filter(|e| e.name == anchor_name)
        .cloned()
        .collect();

    if exact.len() == 1 {
        return Ok(AnchorMatch::Exact(exact.into_iter().next().unwrap()));
    }
    if exact.len() > 1 {
        return Ok(AnchorMatch::Ambiguous(exact));
    }

    // 2. Qualified match — anchor_name matches the suffix
    //    e.g., "connect" matches "MqttClient::connect"
    let qualified: Vec<_> = outline.iter()
        .filter(|e| {
            e.name.ends_with(&format!("::{anchor_name}"))
                || e.name.split("::").last() == Some(anchor_name)
        })
        .cloned()
        .collect();

    if qualified.len() == 1 {
        return Ok(AnchorMatch::Qualified(qualified.into_iter().next().unwrap()));
    }
    if qualified.len() > 1 {
        return Ok(AnchorMatch::Ambiguous(qualified));
    }

    // 3. Fuzzy match — Levenshtein distance
    let mut best: Option<(OutlineEntry, u32)> = None;
    for entry in &outline {
        let short_name = entry.name.split("::").last().unwrap_or(&entry.name);
        let dist = levenshtein(anchor_name, short_name);
        // Only consider matches with distance <= 3
        if dist <= 3 {
            if best.is_none() || dist < best.as_ref().unwrap().1 {
                best = Some((entry.clone(), dist));
            }
        }
    }

    if let Some((entry, distance)) = best {
        return Ok(AnchorMatch::Fuzzy {
            entry,
            distance,
            query: anchor_name.to_string(),
        });
    }

    // 4. No match
    Ok(AnchorMatch::NotFound {
        query: anchor_name.to_string(),
        available: outline.iter().map(|e| e.name.clone()).collect(),
    })
}
```

The Levenshtein implementation can be a simple dynamic programming version — anchor names are short (typically under 50 chars) so performance isn't a concern. No external crate needed.

```rust
fn levenshtein(a: &str, b: &str) -> u32 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 0..=m { dp[i][0] = i as u32; }
    for j in 0..=n { dp[0][j] = j as u32; }

    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }

    dp[m][n]
}
```

### Error Handling in Parsing

Tree-sitter is error-tolerant — it produces a tree even when the source has syntax errors. Error nodes appear in the tree as `ERROR` type nodes. The strategy:

1. **Parse always succeeds.** `tree_sitter::Parser::parse()` returns a tree even for invalid source. It returns `None` only when cancelled (which Ultragit never does) or on allocation failure.
2. **ERROR nodes in the tree.** Walk the tree normally. Skip `ERROR` nodes and their children — don't attempt to extract outline entries from malformed regions.
3. **Log warnings.** If `ERROR` nodes are present, emit a `tracing::warn` with the line number and a snippet of the error region. The outline may be incomplete but the valid parts are still usable.
4. **Partial outlines are fine.** An outline missing one function because it has a syntax error is better than no outline at all. Callers (the writing agent, the read pipeline) handle incomplete outlines gracefully.

```rust
fn should_skip_node(node: &tree_sitter::Node) -> bool {
    if node.is_error() || node.is_missing() {
        tracing::warn!(
            "Parse error at line {}: skipping malformed region",
            node.start_position().row + 1,
        );
        return true;
    }
    false
}
```

### Unsupported Language Fallback

For files with `Language::Unsupported`:

- `extract_outline()` returns `Ok(vec![])`.
- `resolve_anchor()` returns `AnchorMatch::NotFound` with an empty available list.
- The writing agent falls back to line-range-only annotation (no `ast_anchor` field, uses `file` + `lines` only).
- The read pipeline falls back to full-file blame when no anchor can be resolved.

This is a graceful degradation, not a failure. Annotations for unsupported languages are less durable (line numbers shift) but still valuable.

---

## Error Handling

### Error Types

```rust
use snafu::{Snafu, ResultExt, Location};

#[derive(Debug, Snafu)]
pub enum AstError {
    #[snafu(display("Tree-sitter parse returned no tree for {path:?}, at {location}"))]
    ParseFailed {
        path: PathBuf,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Unsupported language for {path:?}, at {location}"))]
    UnsupportedLanguage {
        path: PathBuf,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Grammar not available for {language:?} (feature not enabled), at {location}"))]
    GrammarNotAvailable {
        language: Language,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Runtime grammar loading is not yet supported, at {location}"))]
    RuntimeLoadingNotSupported {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("IO error reading {path:?}, at {location}"))]
    Io {
        path: PathBuf,
        #[snafu(source)]
        source: std::io::Error,
        #[snafu(implicit)]
        location: Location,
    },
}
```

### Failure Modes

- **File can't be read.** `Io` error. Caller decides whether to skip or abort.
- **Unsupported language.** Not an error — returns empty outline. Logged at debug level.
- **Grammar feature not compiled in.** `GrammarNotAvailable` error. Should only happen if someone builds with `--no-default-features`. Error message tells them which feature to enable.
- **Parse produces only ERROR nodes.** Returns empty outline. Logged at warn level.
- **Very large files.** Tree-sitter handles large files well (it's designed for editors), but outline extraction time scales linearly with file size. Files over 100K lines may take noticeable time. No explicit limit — tree-sitter's performance is sufficient.

---

## Configuration

No runtime configuration. Language support is determined by compiled Cargo features. The mapping from extensions to languages is hardcoded and not user-configurable (adding configuration here would be over-engineering for the initial version).

Future: if users need custom extension-to-language mappings, add an `[ultragit.languages]` config section. Not in v1.

---

## Implementation Steps

### Step 1: Language Detection and Grammar Loading

Implement `detect_language()` and `get_ts_language()`. Set up the Cargo features. Verify that each grammar compiles and initializes.

**Deliverable:** `detect_language("src/main.rs")` returns `Language::Rust`. `get_ts_language(Language::Rust)` returns a valid tree-sitter language.

### Step 2: Basic Parsing Infrastructure

Create a `parse()` function that takes source and language, returns a `tree_sitter::Tree`. Handle the error/missing node logging. Write a helper that walks a tree with a cursor and collects nodes of specified types.

**Deliverable:** Can parse a Rust file and print the top-level node types.

### Step 3: Rust Outline Extraction

Implement `extract_outline()` for Rust. Cover: functions, structs, enums, traits, impl blocks, methods within impl blocks, type aliases, constants, statics, modules. Qualified method names (`Type::method`).

**Deliverable:** Given a multi-function Rust source file, returns correct `OutlineEntry` for each semantic unit.

### Step 4: TypeScript/JavaScript Outline Extraction

Implement `extract_outline()` for TypeScript and JavaScript. Cover: function declarations, class declarations, interface declarations, type alias declarations, methods within classes, const arrow functions, exported items.

**Deliverable:** Given a TypeScript file with classes, interfaces, and arrow functions, returns correct outline.

### Step 5: Python Outline Extraction

Implement `extract_outline()` for Python. Cover: function definitions, class definitions, methods within classes, decorated definitions.

**Deliverable:** Given a Python file with classes and decorated functions, returns correct outline.

### Step 6: Signature Extraction

Implement `extract_signature()` for all three languages. Handle multi-line signatures (collapse to single line). Handle generics, where clauses (Rust), and default parameters.

**Deliverable:** Signatures match expected format for all test cases.

### Step 7: Anchor Resolution

Implement `resolve_anchor()` with exact, qualified, fuzzy, ambiguous, and not-found cases. Implement the Levenshtein function.

**Deliverable:** Anchor resolution passes all test cases including fuzzy matching and ambiguity detection.

### Step 8: `outline_for_file()` Convenience Wrapper

Implement the file-reading wrapper. Handle IO errors, unsupported languages.

**Deliverable:** `outline_for_file("src/main.rs")` reads the file and returns its outline.

---

## Test Plan

### Unit Tests

**Language detection:**
- `.rs` -> Rust, `.ts` -> TypeScript, `.tsx` -> Tsx, `.js` -> JavaScript, `.jsx` -> Jsx, `.py` -> Python.
- `.go`, `.java`, `.c`, `.md` -> Unsupported.
- No extension -> Unsupported.
- Case sensitivity: `.RS` -> Unsupported (extensions are case-sensitive on most systems).

**Rust outline extraction:**
- File with a single function.
- File with struct, enum, trait, impl block with methods.
- Nested: function inside impl block gets qualified name.
- `pub`, `pub(crate)` — visibility is included in signature.
- Generic functions and types.
- Where clauses.
- Constants and statics.
- Module declarations.
- `impl Trait for Type` — name includes both.
- Multiple impl blocks for the same type.

**TypeScript outline extraction:**
- Function declarations and default exports.
- Class with constructor, methods, static methods.
- Interface declarations.
- Type alias.
- `const fn = () => {}` arrow function pattern.
- `export function`, `export default class`.
- Async functions.

**Python outline extraction:**
- Top-level functions.
- Class with `__init__`, regular methods, `@staticmethod`, `@classmethod`.
- Decorated functions (`@app.route`).
- Nested classes (should still be extracted with qualified name).
- Functions with type annotations.

**Anchor resolution:**
- Exact match: `"connect"` matches `"connect"`.
- Qualified match: `"connect"` matches `"MqttClient::connect"`.
- Qualified exact: `"MqttClient::connect"` matches `"MqttClient::connect"`.
- Fuzzy match: `"conect"` (typo) matches `"connect"` with distance 1.
- Ambiguous: `"new"` matches both `"SessionCache::new"` and `"MqttClient::new"`.
- Not found: `"nonexistent"` returns NotFound with available names listed.
- Fuzzy threshold: `"xyzzy"` (distance > 3 from everything) returns NotFound.

**Error tolerance:**
- Source with a syntax error mid-file: outline includes entries before and after the error, skipping the malformed region.
- Completely invalid source: returns empty outline without panicking.
- Empty source: returns empty outline.

### Integration Tests

- Parse a real-world Rust file (e.g., a snapshot of a file from a well-known crate) and verify the outline matches expected entries.
- Parse a real-world TypeScript file and verify.
- Parse a real-world Python file and verify.
- Round-trip: extract outline, resolve each entry's name as an anchor, verify the resolved line range matches the original.

### Property Tests

- For any valid Rust/TS/Python source, `extract_outline()` never panics.
- For any outline entry, `resolve_anchor(source, language, entry.name)` returns `Exact` or `Qualified` (never `NotFound` for names that are in the outline).
- All line ranges are valid: `line_start <= line_end`, both within the file's line count.
- Outline entries are sorted by `line_start` (this is a convention callers rely on).

---

## Acceptance Criteria

1. `detect_language()` correctly identifies Rust, TypeScript, JavaScript, TSX, JSX, and Python from file extensions.
2. `extract_outline()` returns correct entries for Rust files containing functions, structs, enums, traits, impl blocks with methods, type aliases, constants.
3. `extract_outline()` returns correct entries for TypeScript files containing functions, classes, interfaces, type aliases, methods, and const arrow functions.
4. `extract_outline()` returns correct entries for Python files containing functions, classes, methods, and decorated definitions.
5. Method names are qualified with their enclosing type: `MqttClient::connect`, `SessionCache::new`.
6. Signatures are human-readable, single-line, and correctly extracted for all languages.
7. `resolve_anchor()` handles exact, qualified, fuzzy (Levenshtein <= 3), ambiguous, and not-found cases.
8. Unsupported languages return empty outlines without errors.
9. Parse errors in source code produce partial outlines (valid regions still extracted) with warnings logged.
10. Language grammars are behind Cargo features. Building with `--no-default-features` excludes all grammars. Each grammar can be individually enabled.
11. Outline entries are sorted by `line_start`.
12. All line ranges are 1-indexed and inclusive.
