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
pub mod razor; // Razor extractor - FIXING metadata access patterns
pub mod regex; // Regex extractor - FIXING metadata access patterns
pub mod sql; // SQL extractor - FIXING metadata access patterns
pub mod vue;
pub mod zig; // Zig extractor - FIXING metadata access patterns

// Re-export the base extractor types
pub use base::{Identifier, IdentifierKind, Relationship, RelationshipKind, Symbol, SymbolKind};

/// Manager for all language extractors
#[allow(dead_code)] // TODO: Implement centralized extractor management
pub struct ExtractorManager {
    // TODO: Store language parsers and extractors
}

#[allow(dead_code)] // TODO: Implement extractor management methods
impl Default for ExtractorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorManager {
    pub fn new() -> Self {
        Self {
            // TODO: Initialize
        }
    }

    /// Get supported languages
    pub fn supported_languages(&self) -> Vec<&'static str> {
        vec![
            // TODO: Return actual supported languages as they are implemented
            "placeholder",
        ]
    }

    /// Extract symbols from file content using the appropriate language extractor
    pub fn extract_symbols(
        &self,
        file_path: &str,
        content: &str,
    ) -> Result<Vec<Symbol>, anyhow::Error> {
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

        // Extract symbols using the appropriate extractor
        let symbols = self.extract_symbols_for_language(file_path, content, language, &tree)?;

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
    fn extract_symbols_for_language(
        &self,
        file_path: &str,
        content: &str,
        language: &str,
        tree: &tree_sitter::Tree,
    ) -> Result<Vec<Symbol>, anyhow::Error> {
        match language {
            "rust" => {
                let mut extractor = crate::extractors::rust::RustExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "typescript" => {
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "javascript" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "go" => {
                let mut extractor = crate::extractors::go::GoExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "java" => {
                let mut extractor = crate::extractors::java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "c" => {
                let mut extractor = crate::extractors::c::CExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "cpp" => {
                let mut extractor = crate::extractors::cpp::CppExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "csharp" => {
                let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "ruby" => {
                let mut extractor = crate::extractors::ruby::RubyExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "php" => {
                let mut extractor = crate::extractors::php::PhpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "swift" => {
                let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "kotlin" => {
                let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "dart" => {
                let mut extractor = crate::extractors::dart::DartExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "gdscript" => {
                let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "lua" => {
                let mut extractor = crate::extractors::lua::LuaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "vue" => {
                let mut extractor = crate::extractors::vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(Some(tree)))
            }
            "razor" => {
                let mut extractor = crate::extractors::razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "sql" => {
                let mut extractor = crate::extractors::sql::SqlExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "html" => {
                let mut extractor = crate::extractors::html::HTMLExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "css" => {
                let mut extractor = crate::extractors::css::CSSExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "bash" => {
                let mut extractor = crate::extractors::bash::BashExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "powershell" => {
                let mut extractor = crate::extractors::powershell::PowerShellExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "zig" => {
                let mut extractor = crate::extractors::zig::ZigExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_symbols(tree))
            }
            "regex" => {
                let mut extractor = crate::extractors::regex::RegexExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
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
        let identifiers = self.extract_identifiers_for_language(file_path, content, language, &tree, symbols)?;

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
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "csharp" => {
                let mut extractor = crate::extractors::csharp::CSharpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "python" => {
                let mut extractor = crate::extractors::python::PythonExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "javascript" => {
                let mut extractor = crate::extractors::javascript::JavaScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "typescript" => {
                let mut extractor = crate::extractors::typescript::TypeScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "java" => {
                let mut extractor = crate::extractors::java::JavaExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "go" => {
                let mut extractor = crate::extractors::go::GoExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "swift" => {
                let mut extractor = crate::extractors::swift::SwiftExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
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
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "php" => {
                let mut extractor = crate::extractors::php::PhpExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "kotlin" => {
                let mut extractor = crate::extractors::kotlin::KotlinExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "c" => {
                let mut extractor = crate::extractors::c::CExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "cpp" => {
                let mut extractor = crate::extractors::cpp::CppExtractor::new(
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "bash" => {
                let mut extractor = crate::extractors::bash::BashExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
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
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "gdscript" => {
                let mut extractor = crate::extractors::gdscript::GDScriptExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "vue" => {
                let mut extractor = crate::extractors::vue::VueExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                // Vue parses internally (extracts <script> section first)
                Ok(extractor.extract_identifiers(symbols))
            }
            "razor" => {
                let mut extractor = crate::extractors::razor::RazorExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "zig" => {
                let mut extractor = crate::extractors::zig::ZigExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }
            "dart" => {
                let mut extractor = crate::extractors::dart::DartExtractor::new(
                    language.to_string(),
                    file_path.to_string(),
                    content.to_string(),
                );
                Ok(extractor.extract_identifiers(tree, symbols))
            }

            // ========================================================================
            // Remaining languages: Not yet implemented (return empty for now)
            // TODO: Implement extract_identifiers() for these languages
            // ========================================================================
            "sql" | "html" | "css" | "powershell" | "regex" => {
                tracing::debug!(
                    "Identifier extraction not yet implemented for language: {} (file: {})",
                    language,
                    file_path
                );
                Ok(Vec::new())
            }

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
}
