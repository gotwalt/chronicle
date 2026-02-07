pub mod anchor;
pub mod outline;

#[cfg(feature = "lang-c")]
mod outline_c;
#[cfg(feature = "lang-cpp")]
mod outline_cpp;
#[cfg(feature = "lang-go")]
mod outline_go;
#[cfg(feature = "lang-java")]
mod outline_java;
#[cfg(feature = "lang-objc")]
mod outline_objc;
#[cfg(feature = "lang-python")]
mod outline_python;
#[cfg(feature = "lang-ruby")]
mod outline_ruby;
#[cfg(feature = "lang-swift")]
mod outline_swift;
#[cfg(feature = "lang-typescript")]
mod outline_typescript;

pub use anchor::AnchorMatch;
pub use outline::{OutlineEntry, SemanticKind};

use crate::error::AstError;

/// Supported languages for AST parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    ObjC,
    Swift,
    Unsupported,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Language::Rust,
            "ts" | "mts" | "cts" => Language::TypeScript,
            "tsx" => Language::Tsx,
            "js" | "mjs" | "cjs" => Language::JavaScript,
            "jsx" => Language::Jsx,
            "py" | "pyi" => Language::Python,
            "go" => Language::Go,
            "java" => Language::Java,
            "c" | "h" => Language::C,
            "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" => Language::Cpp,
            "rb" | "rake" | "gemspec" => Language::Ruby,
            "m" | "mm" => Language::ObjC,
            "swift" => Language::Swift,
            _ => Language::Unsupported,
        }
    }

    /// Detect language from file path.
    pub fn from_path(path: &str) -> Self {
        path.rsplit('.')
            .next()
            .map(Self::from_extension)
            .unwrap_or(Language::Unsupported)
    }
}

/// Extract an outline of semantic units from source code.
pub fn extract_outline(source: &str, language: Language) -> Result<Vec<OutlineEntry>, AstError> {
    match language {
        #[cfg(feature = "lang-rust")]
        Language::Rust => outline::extract_rust_outline(source),

        #[cfg(feature = "lang-typescript")]
        Language::TypeScript | Language::JavaScript => {
            outline_typescript::extract_typescript_outline(source, false)
        }
        #[cfg(feature = "lang-typescript")]
        Language::Tsx | Language::Jsx => {
            outline_typescript::extract_typescript_outline(source, true)
        }

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

        #[cfg(feature = "lang-objc")]
        Language::ObjC => outline_objc::extract_objc_outline(source),

        #[cfg(feature = "lang-swift")]
        Language::Swift => outline_swift::extract_swift_outline(source),

        Language::Unsupported => Err(AstError::UnsupportedLanguage {
            extension: "unknown".to_string(),
            location: snafu::Location::new(file!(), line!(), 0),
        }),

        // Feature-disabled arms: language recognized but grammar not compiled in
        #[allow(unreachable_patterns)]
        _ => Err(AstError::UnsupportedLanguage {
            extension: format!("{:?} (feature not enabled)", language),
            location: snafu::Location::new(file!(), line!(), 0),
        }),
    }
}

/// Resolve an anchor name against an outline, returning match quality.
pub fn resolve_anchor(
    outline: &[OutlineEntry],
    unit_type: &str,
    name: &str,
) -> Option<AnchorMatch> {
    anchor::resolve(outline, unit_type, name)
}
