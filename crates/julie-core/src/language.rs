//! Language detection utilities.
//!
//! Delegates to `julie_extractors::language::detect_language_from_extension()`.

use std::path::Path;

/// Detect programming language from file extension.
pub fn detect_language(path: &Path) -> Option<&'static str> {
    julie_extractors::detect_language_for_path(path, "")
}
