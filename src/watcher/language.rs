//! Language detection from file extensions
//!
//! This module provides utilities for detecting programming languages
//! based on file extensions.

use anyhow::Result;
use std::path::Path;

/// Detect programming language from file extension
pub fn detect_language(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("No file extension"))?;

    let language = match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "java" => "java",
        "cs" => "csharp",
        "cpp" | "cxx" | "cc" => "cpp",
        "c" | "h" => "c",
        "go" => "go",
        "php" => "php",
        "rb" => "ruby",
        "swift" => "swift",
        "kt" => "kotlin",
        "lua" => "lua",
        "gd" => "gdscript",
        "sql" => "sql",
        "html" | "htm" => "html",
        "css" => "css",
        "vue" => "vue",
        "razor" => "razor",
        "ps1" => "powershell",
        "sh" | "bash" => "bash",
        "qml" => "qml",
        "r" | "R" => "r",
        "zig" => "zig",
        "dart" => "dart",
        _ => return Err(anyhow::anyhow!("Unsupported file extension: {}", ext)),
    };

    Ok(language.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_language_detection_by_extension() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path().to_path_buf();

        let test_files = vec![
            ("test.rs", "rust"),
            ("app.ts", "typescript"),
            ("script.js", "javascript"),
            ("main.py", "python"),
            ("App.java", "java"),
            ("Program.cs", "csharp"),
        ];

        for (filename, expected_lang) in test_files {
            let file_path = workspace_root.join(filename);
            fs::write(&file_path, "// test content").unwrap();

            let result = detect_language(&file_path);
            if let Ok(lang) = result {
                assert_eq!(lang, expected_lang);
            }
        }
    }
}
