use crate::tools::workspace::commands::ManageWorkspaceTool;
use std::path::Path;

impl ManageWorkspaceTool {
    /// Detect programming language from file extension
    pub(crate) fn detect_language(&self, file_path: &Path) -> String {
        let extension = file_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        // Match by extension first
        match extension.to_lowercase().as_str() {
            // Rust
            "rs" => "rust".to_string(),

            // TypeScript/JavaScript
            "ts" | "mts" | "cts" => "typescript".to_string(),
            "tsx" => "typescript".to_string(),
            "js" | "mjs" | "cjs" => "javascript".to_string(),
            "jsx" => "javascript".to_string(),

            // Python
            "py" | "pyi" | "pyw" => "python".to_string(),

            // Java
            "java" => "java".to_string(),

            // C#
            "cs" => "csharp".to_string(),

            // PHP
            "php" | "phtml" | "php3" | "php4" | "php5" => "php".to_string(),

            // Ruby
            "rb" | "rbw" => "ruby".to_string(),

            // Swift
            "swift" => "swift".to_string(),

            // Kotlin
            "kt" | "kts" => "kotlin".to_string(),

            // Go
            "go" => "go".to_string(),

            // C
            "c" => "c".to_string(),

            // C++
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" => "cpp".to_string(),
            "h" => {
                // Could be C or C++ header, default to C
                if file_path.to_string_lossy().contains("cpp")
                    || file_path.to_string_lossy().contains("c++")
                {
                    "cpp".to_string()
                } else {
                    "c".to_string()
                }
            }

            // Lua
            "lua" => "lua".to_string(),

            // SQL
            "sql" | "mysql" | "pgsql" | "sqlite" => "sql".to_string(),

            // HTML
            "html" | "htm" => "html".to_string(),

            // CSS
            "css" => "css".to_string(),

            // Vue
            "vue" => "vue".to_string(),

            // Razor
            "cshtml" | "razor" => "razor".to_string(),

            // Shell scripts
            "sh" | "bash" | "zsh" | "fish" => "bash".to_string(),

            // PowerShell
            "ps1" | "psm1" | "psd1" => "powershell".to_string(),

            // GDScript
            "gd" => "gdscript".to_string(),

            // Zig
            "zig" => "zig".to_string(),

            // Dart
            "dart" => "dart".to_string(),

            // Regex patterns (special handling)
            "regex" | "regexp" => "regex".to_string(),

            // Default case - check filename
            _ => {
                // Handle files without extensions or special cases
                match file_name.to_lowercase().as_str() {
                    // Build files
                    "dockerfile" | "containerfile" => "dockerfile".to_string(),
                    "makefile" | "gnumakefile" => "makefile".to_string(),
                    "cargo.toml" | "cargo.lock" => "toml".to_string(),
                    "package.json" | "tsconfig.json" | "jsconfig.json" => "json".to_string(),

                    // Shell scripts
                    name if name.starts_with("bash")
                        || name.contains("bashrc")
                        || name.contains("bash_") =>
                    {
                        "bash".to_string()
                    }

                    // Default to unknown
                    _ => "text".to_string(),
                }
            }
        }
    }
}
