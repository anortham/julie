//! Symbol extraction from parsed ASTs
//! Handles language-specific symbol extraction using all 27 tree-sitter extractors
//!
//! 🔥 REFACTORED 2025-11-02: This file now uses the centralized factory function
//! `crate::extractors::extract_symbols_and_relationships()` to eliminate duplicate
//! match statements that caused the R/QML/PHP bug.

use crate::extractors::{Relationship, Symbol};
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use tracing::debug;
use tree_sitter::Tree;

impl ManageWorkspaceTool {
    /// Static version for use in spawn_blocking (where self is not available)
    ///
    /// This method extracts symbols from an already-parsed tree without requiring &self,
    /// making it suitable for use inside spawn_blocking closures. It delegates to the
    /// centralized factory function which is the single source of truth for all language extractors.
    pub(crate) fn extract_symbols_static(
        tree: &Tree,
        file_path: &str,
        content: &str,
        language: &str,
        workspace_root_path: &std::path::Path,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!(
            "Extracting symbols (static): language={}, file={}",
            language, file_path
        );
        debug!("    Tree root node: {:?}", tree.root_node().kind());
        debug!("    Content length: {} chars", content.len());

        // Use centralized factory function (single source of truth)
        let (symbols, relationships) = crate::extractors::extract_symbols_and_relationships(
            tree,
            file_path,
            content,
            language,
            workspace_root_path,
        )?;

        debug!(
            "🎯 extract_symbols_static returning: {} symbols, {} relationships for {} file: {}",
            symbols.len(), relationships.len(), language, file_path
        );

        Ok((symbols, relationships))
    }

    /// Determine if we should extract symbols from a file based on language
    ///
    /// CSS and HTML are indexed for text search only - no symbol extraction
    pub(crate) fn should_extract_symbols(&self, language: &str) -> bool {
        !matches!(language, "css" | "html")
    }
}
