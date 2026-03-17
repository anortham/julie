use crate::tools::workspace::commands::ManageWorkspaceTool;
use std::path::Path;

impl ManageWorkspaceTool {
    /// Detect programming language from file path.
    ///
    /// Delegates to the canonical `detect_language_from_extension()` for extension-based
    /// detection, with a filename fallback for extensionless files (Dockerfile, Makefile, etc.).
    pub(crate) fn detect_language(&self, file_path: &Path) -> String {
        // Try extension-based detection first (canonical source)
        if let Some(ext) = file_path.extension().and_then(|ext| ext.to_str()) {
            if let Some(lang) = julie_extractors::language::detect_language_from_extension(ext) {
                return lang.to_string();
            }
        }

        // Fallback: check filename for extensionless files
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");

        match file_name.to_lowercase().as_str() {
            "dockerfile" | "containerfile" => "dockerfile".to_string(),
            "makefile" | "gnumakefile" => "makefile".to_string(),
            "cargo.toml" | "cargo.lock" => "toml".to_string(),
            "package.json" | "tsconfig.json" | "jsconfig.json" => "json".to_string(),
            name if name.starts_with("bash")
                || name.contains("bashrc")
                || name.contains("bash_") =>
            {
                "bash".to_string()
            }
            _ => "text".to_string(),
        }
    }
}
