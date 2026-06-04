//! Language detection utilities.
//!
//! Delegates to `julie_extractors::language::detect_language_from_extension()`.

use std::path::Path;

/// Detect programming language from file extension.
pub fn detect_language(path: &Path) -> Option<&'static str> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(julie_extractors::language::detect_language_from_extension)
}
