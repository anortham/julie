//! ExtractorManager - Public API for symbol/identifier/relationship extraction
//!
//! Handles file parsing and delegates to language-specific extractors through
//! the routing layer. This module provides the main public interface for clients
//! to extract symbols, identifiers, and relationships from source files.

use crate::extractors::base::{Identifier, Relationship, Symbol};
use std::path::Path;
use tree_sitter::Parser;

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
        workspace_root: &Path,
    ) -> Result<Vec<Symbol>, anyhow::Error> {
        // Determine language from file extension
        let language = self.get_language_from_extension(file_path)?;

        // Create parser for the language
        let mut parser = Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(&language)?;

        parser.set_language(&tree_sitter_language).map_err(|e| {
            anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract symbols using the routing layer
        let symbols = super::routing_symbols::extract_symbols_for_language(
            file_path,
            content,
            &language,
            &tree,
            workspace_root,
        )?;

        tracing::debug!(
            "Extracted {} symbols from {} file: {}",
            symbols.len(),
            language,
            file_path
        );
        Ok(symbols)
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
        // Determine language from file extension
        let language = self.get_language_from_extension(file_path)?;

        // Create parser for the language
        let mut parser = Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(&language)?;

        parser.set_language(&tree_sitter_language).map_err(|e| {
            anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract identifiers using the routing layer
        let identifiers = super::routing_identifiers::extract_identifiers_for_language(
            file_path,
            content,
            &language,
            &tree,
            symbols,
        )?;

        tracing::debug!(
            "Extracted {} identifiers from {} file: {}",
            identifiers.len(),
            language,
            file_path
        );
        Ok(identifiers)
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
        // Determine language from file extension
        let language = self.get_language_from_extension(file_path)?;

        // Create parser for the language
        let mut parser = Parser::new();
        let tree_sitter_language = self.get_tree_sitter_language(&language)?;

        parser.set_language(&tree_sitter_language).map_err(|e| {
            anyhow::anyhow!("Failed to set parser language for {}: {}", language, e)
        })?;

        // Parse the file
        let tree = parser
            .parse(content, None)
            .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))?;

        // Extract relationships using the routing layer
        let relationships = super::routing_relationships::extract_relationships_for_language(
            file_path,
            content,
            &language,
            &tree,
            symbols,
        )?;

        tracing::debug!(
            "Extracted {} relationships from {} file: {}",
            relationships.len(),
            language,
            file_path
        );
        Ok(relationships)
    }

    /// Get tree-sitter language for given language name (delegates to shared module)
    fn get_tree_sitter_language(
        &self,
        language: &str,
    ) -> Result<tree_sitter::Language, anyhow::Error> {
        crate::language::get_tree_sitter_language(language)
    }

    /// Determine language from file extension
    fn get_language_from_extension(&self, file_path: &str) -> Result<String, anyhow::Error> {
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
                return Ok(String::new());
            }
        };

        // 🔍 DEBUG: Log language mapping for R files
        if file_path.ends_with(".R") || file_path.ends_with(".r") {
            tracing::warn!("  - Mapped to language: '{}'", language);
        }

        Ok(language.to_string())
    }
}
