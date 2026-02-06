//! Utility functions for refactoring operations

use super::SmartRefactorTool;

impl SmartRefactorTool {
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
            Some("r") | Some("R") => "r".to_string(),
            _ => "unknown".to_string(),
        }
    }
}
