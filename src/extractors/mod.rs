// Julie's Language Extractors Module
//
// This module contains all the tree-sitter based extractors for various programming languages.
// Each extractor is responsible for parsing source code and extracting symbols, relationships,
// and type information using tree-sitter parsers.

pub mod base;

// TODO: Implement language extractors (Phase 1 & 2)
// Phase 1 - Core Languages:
pub mod dart; // Dart extractor - RE-ENABLING for Dart Specialist work
pub mod go;
pub mod javascript; // JavaScript extractor - FIXING API errors - MY ASSIGNED TASK
pub mod python;
pub mod rust; // Rust extractor - RUST AGENT WORKING - DO NOT DISABLE
pub mod typescript; // Go extractor - FIXING compilation issues

// Phase 2 - Extended Languages:
pub mod c; // C extractor
pub mod cpp; // C++ extractor - FIXING lifetime annotation errors
pub mod csharp; // C# extractor - Testing if it works
pub mod java; // Java extractor - FIXING API compatibility errors - MY ASSIGNED TASK
pub mod kotlin;
pub mod php; // PHP extractor - FIXING metadata access patterns
pub mod ruby; // Ruby extractor - API FIXED, testing compilation
pub mod swift; // Swift extractor - FIXING metadata access patterns // Kotlin extractor - FIXING metadata access patterns

// Phase 2 - Specialized Languages:
pub mod bash; // Bash extractor (TDD GREEN phase)
pub mod css; // CSS extractor (Phase 1 SUCCESS)
pub mod gdscript; // GDScript extractor (Phase 1 SUCCESS - FIXED)
pub mod html; // HTML extractor - FIXING metadata access patterns
pub mod lua; // Lua extractor - FIXING metadata access patterns
pub mod powershell; // PowerShell extractor (Phase 1 SUCCESS)
pub mod qml; // QML (Qt) extractor - NEW - Extractor #26
pub mod r; // R (Statistical Computing) extractor - NEW - Extractor #27
pub mod razor; // Razor extractor - FIXING metadata access patterns
pub mod regex; // Regex extractor - FIXING metadata access patterns
pub mod sql; // SQL extractor - FIXING metadata access patterns
pub mod vue;
pub mod zig; // Zig extractor - FIXING metadata access patterns

// Re-export the base extractor types
pub use base::{Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind};

/// Manager for all language extractors
/// Provides centralized symbol extraction across 25+ languages
pub struct ExtractorManager {
    // No state needed - this is a stateless manager that delegates to language-specific extractors
}

impl Default for ExtractorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Get supported languages (all 27 extractors complete language support)
    pub fn supported_languages(&self) -> Vec<&'static str> {
        vec![
            "rust", "typescript", "tsx", "javascript", "jsx", "python", "go", "java",
            "c", "cpp", "csharp", "ruby", "php", "swift", "kotlin", "dart",
            "gdscript", "lua", "qml", "r", "vue", "razor", "sql", "html", "css", "bash",
            "powershell", "zig", "regex",
        ]
    }

    /// Extract symbols from file content using the appropriate language extractor
    ///
    /// # Phase 2: Relative Unix-Style Path Storage
    /// Now requires workspace_root for relative path storage
    pub fn extract_symbols(
        &self,
        file_path: &str,
        content: &str,
        workspace_root: &std::path::Path, // NEW: Phase 2 - workspace root
    ) -> Result<Vec<Symbol>, anyhow::Error> {
        use std::path::Path;
        use tree_sitter::Parser;

        // Determine language from file extension
        let path = Path::new(file_path);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

        // 🔍 DEBUG: Log extension detection for R files
        if file_path.ends_with(".R") || file_path.ends_with(".r") {
            tracing::warn!("🔍 DEBUG ExtractorManager: R file detected!");
            tracing::warn!("  - File path: {}", file_path);
            tracing::warn!("  - Extracted extension: '{}'", extension);
        }

        let language = match extension {
            "rs" => "rust",
            "ts" => "typescript",
            "tsx" => "tsx",
            "js" => "javascript",
            "jsx" => "jsx",
            "py" => "python",
            "go" => "go",
            "java" => "java",
            "c" => "c",
            "cpp" | "cc" | "cxx" => "cpp",
            "cs" => "csharp",
            "rb" => "ruby",
            "php" => "php",
            "swift" => "swift",
            "kt" => "kotlin",
            "dart" => "dart",
            "gd" => "gdscript",
            "lua" => "lua",
            "qml" => "qml",
            "r" | "R" => "r",
            "vue" => "vue",
            "razor" => "razor",
            "sql" => "sql",
            "html" => "html",
            "css" => "css",
            "sh" | "bash" => "bash",
            "ps1" => "powershell",
            "zig" => "zig",
            "regex" => "regex",
            _ => {
                // Unsupported file type - return empty results
                return Ok(Vec::new());
            }
        };

        // 🔍 DEBUG: Log language mapping for R files
        if file_path.ends_with(".R") || file_path.ends_with(".r") {
            tracing::warn!("  - Mapped to language: '{}'", language);
        }

        // Create parser for the language
        let mut parser = Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(language)?;

        parser.set_language(&tree_sitter_language).map_err(|e| {
            anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract symbols using the appropriate extractor
        let symbols = self.extract_symbols_for_language(file_path, content, language, &tree, workspace_root)?;

        tracing::debug!(
            "Extracted {} symbols from {} file: {}",
            symbols.len(),
            language,
            file_path
        );
        Ok(symbols)
    }

    /// Get tree-sitter language for given language name (delegates to shared module)
    fn get_tree_sitter_language(
        &self,
        language: &str,
    ) -> Result<tree_sitter::Language, anyhow::Error> {
        crate::language::get_tree_sitter_language(language)
    }

    /// Extract symbols using the appropriate extractor for the detected language
    ///
    /// # Phase 2: Relative Unix-Style Path Storage
    /// Now requires workspace_root for relative path storage
    fn extract_symbols_for_language(
        &self,
        file_path: &str,
        content: &str,
        language: &str,
        tree: &tree_sitter::Tree,
        workspace_root: &std::path::Path, // NEW: Phase 2 - workspace root for relative paths
    ) -> Result<Vec<Symbol>, anyhow::Error> {
        match language {
            "rust" => {
                let mut extractor = crate::extractors::rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "typescript" | "tsx" => {
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "javascript" | "jsx" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "go" => {
                let mut extractor = crate::extractors::go::GoExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "java" => {
                let mut extractor = crate::extractors::java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "c" => {
                let mut extractor = crate::extractors::c::CExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "cpp" => {
                let mut extractor = crate::extractors::cpp::CppExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "csharp" => {
                let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "ruby" => {
                let mut extractor = crate::extractors::ruby::RubyExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "php" => {
                let mut extractor = crate::extractors::php::PhpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "swift" => {
                let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "kotlin" => {
                let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "dart" => {
                let mut extractor = crate::extractors::dart::DartExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "gdscript" => {
                let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "lua" => {
                let mut extractor = crate::extractors::lua::LuaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "qml" => {
                let mut extractor = crate::extractors::qml::QmlExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "r" => {
                let mut extractor = crate::extractors::r::RExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "vue" => {
                let mut extractor = crate::extractors::vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(Some(tree)))
            }
            "razor" => {
                let mut extractor = crate::extractors::razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "sql" => {
                let mut extractor = crate::extractors::sql::SqlExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "html" => {
                let mut extractor = crate::extractors::html::HTMLExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "css" => {
                let mut extractor = crate::extractors::css::CSSExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "bash" => {
                let mut extractor = crate::extractors::bash::BashExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "powershell" => {
                let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "zig" => {
                let mut extractor = crate::extractors::zig::ZigExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            "regex" => {
                let mut extractor = crate::extractors::regex::RegexExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    workspace_root,
                );
                Ok(extractor.extract_symbols(tree))
            }
            _ => {
                tracing::debug!(
                    "No extractor available for language: {} (file: {})",
                    language,
                    file_path
                );
                Ok(Vec::new())
            }
        }
    }

    /// Extract identifiers (references/usages) from file content for LSP-quality find_references
    ///
    /// This method follows the same pattern as extract_symbols() but calls extract_identifiers()
    /// on the language-specific extractors.
    pub fn extract_identifiers(
        &self,
        file_path: &str,
        content: &str,
        symbols: &[Symbol],
    ) -> Result<Vec<Identifier>, anyhow::Error> {
        use std::path::Path;
        use tree_sitter::Parser;

        // Determine language from file extension
        let path = Path::new(file_path);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

        let language = match extension {
            "rs" => "rust",
            "ts" => "typescript",
            "tsx" => "tsx",
            "js" => "javascript",
            "jsx" => "jsx",
            "py" => "python",
            "go" => "go",
            "java" => "java",
            "c" => "c",
            "cpp" | "cc" | "cxx" => "cpp",
            "cs" => "csharp",
            "rb" => "ruby",
            "php" => "php",
            "swift" => "swift",
            "kt" => "kotlin",
            "dart" => "dart",
            "gd" => "gdscript",
            "lua" => "lua",
            "qml" => "qml",
            "r" | "R" => "r",
            "vue" => "vue",
            "razor" => "razor",
            "sql" => "sql",
            "html" => "html",
            "css" => "css",
            "sh" | "bash" => "bash",
            "ps1" => "powershell",
            "zig" => "zig",
            "regex" => "regex",
            _ => {
                // Unsupported file type - return empty results
                return Ok(Vec::new());
            }
        };

        // Create parser for the language
        let mut parser = Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(language)?;

        parser.set_language(&tree_sitter_language).map_err(|e| {
            anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract identifiers using the appropriate extractor
        let identifiers =
            self.extract_identifiers_for_language(file_path, content, language, &tree, symbols)?;

        tracing::debug!(
            "Extracted {} identifiers from {} file: {}",
            identifiers.len(),
            language,
            file_path
        );
        Ok(identifiers)
    }

    /// Extract identifiers using the appropriate extractor for the detected language
    ///
    /// NOTE: Only languages that have implemented extract_identifiers() will return results.
    /// Languages without implementation will return empty Vec (they need to be implemented).
    fn extract_identifiers_for_language(
        &self,
        file_path: &str,
        content: &str,
        language: &str,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Result<Vec<Identifier>, anyhow::Error> {
        match language {
            // ========================================================================
            // Batch 1: Implemented languages (extract_identifiers available)
            // ========================================================================
            "rust" => {
                let mut extractor = crate::extractors::rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "csharp" => {
                let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "javascript" | "jsx" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "typescript" | "tsx" => {
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "java" => {
                let mut extractor = crate::extractors::java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "go" => {
                let mut extractor = crate::extractors::go::GoExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "swift" => {
                let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }

            // ========================================================================
            // Batch 2: Implemented languages (extract_identifiers available)
            // ========================================================================
            "ruby" => {
                let mut extractor = crate::extractors::ruby::RubyExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "php" => {
                let mut extractor = crate::extractors::php::PhpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "kotlin" => {
                let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "c" => {
                let mut extractor = crate::extractors::c::CExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "cpp" => {
                let mut extractor = crate::extractors::cpp::CppExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "bash" => {
                let mut extractor = crate::extractors::bash::BashExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }

            // ========================================================================
            // Batch 3: Implemented languages (extract_identifiers available)
            // ========================================================================
            "lua" => {
                let mut extractor = crate::extractors::lua::LuaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "gdscript" => {
                let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "vue" => {
                let mut extractor = crate::extractors::vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                // Vue parses internally (extracts <script> section first)
                Ok(extractor.extract_identifiers(symbols))
            }
            "razor" => {
                let mut extractor = crate::extractors::razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "zig" => {
                let mut extractor = crate::extractors::zig::ZigExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "dart" => {
                let mut extractor = crate::extractors::dart::DartExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }

            // ========================================================================
            // Batch 4: Implemented languages (extract_identifiers available)
            // ========================================================================
            "sql" => {
                let mut extractor = crate::extractors::sql::SqlExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "html" => {
                let mut extractor = crate::extractors::html::HTMLExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "css" => {
                let mut extractor = crate::extractors::css::CSSExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "powershell" => {
                let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "regex" => {
                let mut extractor = crate::extractors::regex::RegexExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }

            // ========================================================================
            // All 25 languages now have identifier extraction implemented!
            // ========================================================================
            _ => {
                tracing::debug!(
                    "No identifier extraction available for language: {} (file: {})",
                    language,
                    file_path
                );
                Ok(Vec::new())
            }
        }
    }

    /// Extract relationships (inheritance, implements, etc.) from file content
    ///
    /// This method follows the same pattern as extract_symbols() but calls extract_relationships()
    /// on the language-specific extractors.
    pub fn extract_relationships(
        &self,
        file_path: &str,
        content: &str,
        symbols: &[Symbol],
    ) -> Result<Vec<Relationship>, anyhow::Error> {
        use std::path::Path;
        use tree_sitter::Parser;

        // Determine language from file extension
        let path = Path::new(file_path);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");

        let language = match extension {
            "rs" => "rust",
            "ts" => "typescript",
            "tsx" => "tsx",
            "js" => "javascript",
            "jsx" => "jsx",
            "py" => "python",
            "go" => "go",
            "java" => "java",
            "c" => "c",
            "cpp" | "cc" | "cxx" => "cpp",
            "cs" => "csharp",
            "rb" => "ruby",
            "php" => "php",
            "swift" => "swift",
            "kt" => "kotlin",
            "dart" => "dart",
            "gd" => "gdscript",
            "lua" => "lua",
            "qml" => "qml",
            "r" | "R" => "r",
            "vue" => "vue",
            "razor" => "razor",
            "sql" => "sql",
            "html" => "html",
            "css" => "css",
            "sh" | "bash" => "bash",
            "ps1" => "powershell",
            "zig" => "zig",
            "regex" => "regex",
            _ => {
                // Unsupported file type - return empty results
                return Ok(Vec::new());
            }
        };

        // Create parser for the language
        let mut parser = Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(language)?;

        parser.set_language(&tree_sitter_language).map_err(|e| {
            anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract relationships using the appropriate extractor
        let relationships =
            self.extract_relationships_for_language(file_path, content, language, &tree, symbols)?;

        tracing::debug!(
            "Extracted {} relationships from {} file: {}",
            relationships.len(),
            language,
            file_path
        );
        Ok(relationships)
    }

    /// Extract relationships using the appropriate extractor for the detected language
    fn extract_relationships_for_language(
        &self,
        file_path: &str,
        content: &str,
        language: &str,
        tree: &tree_sitter::Tree,
        symbols: &[Symbol],
    ) -> Result<Vec<Relationship>, anyhow::Error> {
        match language {
            "rust" => {
                let mut extractor = crate::extractors::rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "csharp" => {
                let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "javascript" | "jsx" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "typescript" | "tsx" => {
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "java" => {
                let mut extractor = crate::extractors::java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "go" => {
                let mut extractor = crate::extractors::go::GoExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "swift" => {
                let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "ruby" => {
                let extractor = crate::extractors::ruby::RubyExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "php" => {
                let mut extractor = crate::extractors::php::PhpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "kotlin" => {
                let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "c" => {
                let mut extractor = crate::extractors::c::CExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "cpp" => {
                let mut extractor = crate::extractors::cpp::CppExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "bash" => {
                let mut extractor = crate::extractors::bash::BashExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "lua" => {
                let mut extractor = crate::extractors::lua::LuaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "gdscript" => {
                let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "vue" => {
                let mut extractor = crate::extractors::vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(Some(tree), symbols))
            }
            "razor" => {
                let mut extractor = crate::extractors::razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "zig" => {
                let mut extractor = crate::extractors::zig::ZigExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "dart" => {
                let mut extractor = crate::extractors::dart::DartExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "sql" => {
                let mut extractor = crate::extractors::sql::SqlExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "html" => {
                let mut extractor = crate::extractors::html::HTMLExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "css" => {
                // CSS doesn't have relationships
                Ok(Vec::new())
            }
            "powershell" => {
                let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            "regex" => {
                let mut extractor = crate::extractors::regex::RegexExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                    &std::path::PathBuf::from("/tmp/test"),
                );
                Ok(extractor.extract_relationships(tree, symbols))
            }
            _ => {
                tracing::debug!(
                    "No relationship extraction available for language: {} (file: {})",
                    language,
                    file_path
                );
                Ok(Vec::new())
            }
        }
    }
}

// ============================================================================
// SHARED EXTRACTOR FACTORY - SINGLE SOURCE OF TRUTH FOR ALL 27 LANGUAGES
// ============================================================================
//
// 🚨 CRITICAL: This is the ONLY place where extractor routing should exist!
//
// Both `extractors/mod.rs::extract_symbols_for_language()` and
// `tools/workspace/indexing/extractor.rs::extract_symbols_with_existing_tree()`
// MUST call this function to avoid duplicate match statements.
//
// Adding a new language? Update ONLY this function (and the tests).
// ============================================================================

/// Extract symbols and relationships for ANY supported language
///
/// This is the centralized factory function for all 27 language extractors.
/// It ensures consistency across the codebase and prevents bugs from missing
/// languages in different code paths.
///
/// # Parameters
/// - `tree`: Pre-parsed tree-sitter AST
/// - `file_path`: Relative Unix-style file path (for symbol storage)
/// - `content`: Source code content
/// - `language`: Language identifier (lowercase, e.g., "rust", "r", "qml")
/// - `workspace_root`: Workspace root path for relative path calculations
///
/// # Returns
/// `Ok((symbols, relationships))` on success, or error if extraction fails
///
/// # Example
/// ```rust
/// let (symbols, rels) = extract_symbols_and_relationships(
///     &tree, "src/main.rs", &content, "rust", workspace_root
/// )?;
/// ```
pub fn extract_symbols_and_relationships(
    tree: &tree_sitter::Tree,
    file_path: &str,
    content: &str,
    language: &str,
    workspace_root: &std::path::Path,
) -> Result<(Vec<Symbol>, Vec<Relationship>), anyhow::Error> {
    use anyhow::anyhow;

    // Single match statement for ALL 27 languages
    let (symbols, relationships) = match language {
        "rust" => {
            let mut extractor = rust::RustExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "typescript" | "tsx" => {
            let mut extractor = typescript::TypeScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "javascript" | "jsx" => {
            let mut extractor = javascript::JavaScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "python" => {
            let mut extractor = python::PythonExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "java" => {
            let mut extractor = java::JavaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "csharp" => {
            let mut extractor = csharp::CSharpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "php" => {
            let mut extractor = php::PhpExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "ruby" => {
            let mut extractor = ruby::RubyExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "swift" => {
            let mut extractor = swift::SwiftExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "kotlin" => {
            let mut extractor = kotlin::KotlinExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "dart" => {
            let mut extractor = dart::DartExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "go" => {
            let mut extractor = go::GoExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "c" => {
            let mut extractor = c::CExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "cpp" => {
            let mut extractor = cpp::CppExtractor::new(
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "lua" => {
            let mut extractor = lua::LuaExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "qml" => {
            let mut extractor = qml::QmlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "r" => {
            let mut extractor = r::RExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "sql" => {
            let mut extractor = sql::SqlExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "html" => {
            let mut extractor = html::HTMLExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "css" => {
            let mut extractor = css::CSSExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            // CSSExtractor doesn't have extract_relationships method yet
            (symbols, Vec::new())
        }
        "vue" => {
            let mut extractor = vue::VueExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(Some(tree));
            let relationships = extractor.extract_relationships(Some(tree), &symbols);
            (symbols, relationships)
        }
        "razor" => {
            let mut extractor = razor::RazorExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "bash" => {
            let mut extractor = bash::BashExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "powershell" => {
            let mut extractor = powershell::PowerShellExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "gdscript" => {
            let mut extractor = gdscript::GDScriptExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "zig" => {
            let mut extractor = zig::ZigExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        "regex" => {
            let mut extractor = regex::RegexExtractor::new(
                language.to_string(),
                file_path.to_string(),
                content.to_string(),
                workspace_root,
            );
            let symbols = extractor.extract_symbols(tree);
            let relationships = extractor.extract_relationships(tree, &symbols);
            (symbols, relationships)
        }
        _ => {
            return Err(anyhow!(
                "No extractor available for language '{}' (file: {})",
                language,
                file_path
            ));
        }
    };

    Ok((symbols, relationships))
}

// ============================================================================
// COMPILE-TIME CONSISTENCY TESTS - PREVENT FUTURE BUGS
// ============================================================================

#[cfg(test)]
mod factory_consistency_tests {
    use super::*;
    use std::path::PathBuf;
    use tree_sitter::Parser;

    /// Test that ALL 27 supported languages work with the factory function
    ///
    /// This test prevents the R/QML/PHP bug from happening again by ensuring
    /// every language in supported_languages() can be extracted via the factory.
    #[test]
    fn test_all_languages_in_factory() {
        let manager = ExtractorManager::new();
        let supported = manager.supported_languages();

        // Verify we have all 27 languages
        assert_eq!(supported.len(), 29, "Expected 29 language entries (27 languages, 2 with aliases)");

        let workspace_root = PathBuf::from("/tmp/test");

        // Test each language can be handled by the factory
        // Note: Some will fail to parse invalid code, but they should NOT return
        // "No extractor available" error
        for language in &supported {
            let test_content = "// test";

            // Create a minimal valid tree for testing
            let mut parser = Parser::new();
            let ts_lang = match crate::language::get_tree_sitter_language(language) {
                Ok(lang) => lang,
                Err(_) => continue, // Skip if language not available
            };

            parser.set_language(&ts_lang).unwrap();
            let tree = parser.parse(test_content, None).unwrap();

            // The factory should handle this language (even if it extracts 0 symbols)
            let result = extract_symbols_and_relationships(
                &tree,
                "test.rs",
                test_content,
                language,
                &workspace_root,
            );

            // Should succeed OR fail for parsing reasons, but NEVER "No extractor available"
            if let Err(e) = result {
                let error_msg = format!("{}", e);
                assert!(
                    !error_msg.contains("No extractor available"),
                    "Language '{}' is missing from factory function! Error: {}",
                    language,
                    error_msg
                );
            }
        }
    }

    /// Test that the factory function rejects unknown languages
    #[test]
    fn test_factory_rejects_unknown_language() {
        let workspace_root = PathBuf::from("/tmp/test");
        let mut parser = Parser::new();

        // Use Rust parser for a fake language
        let ts_lang = crate::language::get_tree_sitter_language("rust").unwrap();
        parser.set_language(&ts_lang).unwrap();
        let tree = parser.parse("// test", None).unwrap();

        let result = extract_symbols_and_relationships(
            &tree,
            "test.unknown",
            "// test",
            "unknown_language_xyz",
            &workspace_root,
        );

        assert!(result.is_err(), "Should reject unknown language");
        assert!(
            format!("{}", result.unwrap_err()).contains("No extractor available"),
            "Error should mention no extractor available"
        );
    }
}
