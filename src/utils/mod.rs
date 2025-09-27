// Julie's Utilities Module
//
// Common utilities and helper functions used throughout the Julie codebase.

use anyhow::Result;
use std::path::Path;

/// File utilities
pub mod file_utils {
    use super::*;
    use std::fs;

    /// Check if a file has a supported language extension
    pub fn is_supported_file(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(
                ext,
                "rs" | "py"
                    | "js"
                    | "ts"
                    | "tsx"
                    | "jsx"
                    | "go"
                    | "java"
                    | "c"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "cs"
                    | "php"
                    | "rb"
                    | "swift"
                    | "kt"
                    | "lua"
                    | "gd"
                    | "vue"
                    | "html"
                    | "css"
                    | "sql"
                    | "sh"
                    | "bash"
            )
        } else {
            false
        }
    }

    /// Read file content safely
    pub fn read_file_content(path: &Path) -> Result<String> {
        Ok(fs::read_to_string(path)?)
    }
}

/// Token estimation utilities
pub mod token_estimation;

/// Context truncation utilities
pub mod context_truncation;

/// Progressive reduction utilities
pub mod progressive_reduction;

/// Path relevance scoring utilities
pub mod path_relevance;

/// Exact match boost utilities
pub mod exact_match_boost;

/// Language detection utilities
pub mod language {
    use std::path::Path;

    /// Detect programming language from file extension
    pub fn detect_language(path: &Path) -> Option<&'static str> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext {
                "rs" => Some("rust"),
                "py" => Some("python"),
                "js" => Some("javascript"),
                "ts" => Some("typescript"),
                "tsx" => Some("typescript"),
                "jsx" => Some("javascript"),
                "go" => Some("go"),
                "java" => Some("java"),
                "c" => Some("c"),
                "cpp" | "cc" | "cxx" => Some("cpp"),
                "h" => Some("c"),
                "hpp" | "hxx" => Some("cpp"),
                "cs" => Some("csharp"),
                "php" => Some("php"),
                "rb" => Some("ruby"),
                "swift" => Some("swift"),
                "kt" => Some("kotlin"),
                "lua" => Some("lua"),
                "gd" => Some("gdscript"),
                "vue" => Some("vue"),
                "html" => Some("html"),
                "css" => Some("css"),
                "sql" => Some("sql"),
                "sh" | "bash" => Some("bash"),
                _ => None,
            })
    }
}
