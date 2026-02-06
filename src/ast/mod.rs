pub mod outline;
pub mod anchor;

pub use outline::{OutlineEntry, SemanticKind};
pub use anchor::AnchorMatch;

use crate::error::AstError;

/// Supported languages for AST parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Unsupported,
}

impl Language {
    /// Detect language from file extension.
    pub fn from_extension(ext: &str) -> Self {
        match ext {
            "rs" => Language::Rust,
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
        Language::Rust => outline::extract_rust_outline(source),
        Language::Unsupported => {
            Err(AstError::UnsupportedLanguage {
                extension: "unknown".to_string(),
                location: snafu::Location::new(file!(), line!(), 0),
            })
        }
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
