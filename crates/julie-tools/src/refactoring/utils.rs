//! Utility functions for refactoring operations

use super::SmartRefactorTool;

impl SmartRefactorTool {
    /// Detect programming language from file extension.
    ///
    /// Delegates to `julie_extractors::language::detect_language_from_extension()`.
    pub fn detect_language(&self, file_path: &str) -> String {
        julie_extractors::detect_language_for_path(std::path::Path::new(file_path), "")
            .unwrap_or("unknown")
            .to_string()
    }
}
