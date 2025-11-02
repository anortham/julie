//! Utility functions for refactoring operations

use super::SmartRefactorTool;

impl SmartRefactorTool {
    /// Detect the base indentation level of code lines
    #[allow(dead_code)]
    pub fn detect_base_indentation(&self, lines: &[&str]) -> usize {
        lines
            .iter()
            .filter(|line| !line.trim().is_empty()) // Skip empty lines
            .map(|line| line.len() - line.trim_start().len()) // Count leading whitespace
            .min()
            .unwrap_or(0)
    }

    /// Remove base indentation from code lines
    #[allow(dead_code)]
    pub fn dedent_code(&self, lines: &[&str], base_indent: usize) -> String {
        lines
            .iter()
            .map(|line| {
                if line.trim().is_empty() {
                    "" // Keep empty lines empty
                } else if line.len() > base_indent {
                    &line[base_indent..] // Remove base indentation
                } else {
                    line.trim_start() // Line has less indentation than base
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Detect programming language from file extension using shared language module
    pub fn detect_language(&self, file_path: &str) -> String {
        match std::path::Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
        {
            Some("rs") => "rust".to_string(),
            Some("ts") | Some("tsx") => "typescript".to_string(),
            Some("js") | Some("jsx") => "javascript".to_string(),
            Some("py") => "python".to_string(),
            Some("java") => "java".to_string(),
            Some("cs") => "csharp".to_string(),
            Some("php") => "php".to_string(),
            Some("rb") => "ruby".to_string(),
            Some("swift") => "swift".to_string(),
            Some("kt") => "kotlin".to_string(),
            Some("go") => "go".to_string(),
            Some("c") => "c".to_string(),
            Some("cpp") | Some("cc") | Some("cxx") | Some("h") => "cpp".to_string(),
            Some("lua") => "lua".to_string(),
            Some("sql") => "sql".to_string(),
            Some("html") | Some("htm") => "html".to_string(),
            Some("css") => "css".to_string(),
            Some("vue") => "vue".to_string(),
            Some("razor") | Some("cshtml") => "razor".to_string(),
            Some("sh") | Some("bash") => "bash".to_string(),
            Some("ps1") => "powershell".to_string(),
            Some("zig") => "zig".to_string(),
            Some("dart") => "dart".to_string(),
            Some("qml") => "qml".to_string(),
            _ => "unknown".to_string(),
        }
    }

    pub fn optimize_response(&self, message: &str) -> String {
        // Messages are now minimal 2-line summaries - no optimization needed
        message.to_string()
    }
}
