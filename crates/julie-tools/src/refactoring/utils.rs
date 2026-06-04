//! Utility functions for refactoring operations

use super::SmartRefactorTool;

impl SmartRefactorTool {
    /// Detect programming language from file extension.
    ///
    /// Delegates to `julie_extractors::language::detect_language_from_extension()`.
    pub fn detect_language(&self, file_path: &str) -> String {
        std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(julie_extractors::language::detect_language_from_extension)
            .unwrap_or("unknown")
            .to_string()
    }
}
