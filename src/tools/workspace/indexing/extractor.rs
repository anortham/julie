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
    /// Extract symbols from an already-parsed tree (PERFORMANCE OPTIMIZED)
    ///
    /// This bypasses the expensive tree-sitter parsing step when parser is reused.
    ///
    /// # Phase 2: Relative Unix-Style Path Storage
    /// Now requires workspace_root_path parameter to enable relative path storage in extractors
    ///
    /// # Refactoring Note
    /// This function now delegates to `crate::extractors::extract_symbols_and_relationships()`,
    /// which is the single source of truth for all 27 language extractors. This eliminates
    /// the duplicate match statement that caused bugs when languages were added to one location
    /// but not the other.
    pub(crate) fn extract_symbols_with_existing_tree(
        &self,
        tree: &Tree,
        file_path: &str,
        content: &str,
        language: &str,
        workspace_root_path: &std::path::Path,
    ) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
        debug!(
            "Extracting symbols: language={}, file={}",
            language, file_path
        );
        debug!("    Tree root node: {:?}", tree.root_node().kind());
        debug!("    Content length: {} chars", content.len());

        // 🔥 REFACTORED: Use centralized factory function (single source of truth)
        // This eliminates the duplicate match statement that caused the R/QML/PHP bug
        //
        // OLD: 300+ lines of duplicate match statement (lines 33-345)
        // NEW: Single function call to shared factory
        let (symbols, relationships) = crate::extractors::extract_symbols_and_relationships(
            tree,
            file_path,
            content,
            language,
            workspace_root_path,
        )?;

        debug!(
            "🎯 extract_symbols_with_existing_tree returning: {} symbols, {} relationships for {} file: {}",
            symbols.len(), relationships.len(), language, file_path
        );

        Ok((symbols, relationships))
    }

    /// Static version for use in spawn_blocking (where self is not available)
    ///
    /// This is identical to extract_symbols_with_existing_tree but doesn't require &self
    /// since it only delegates to the static factory function anyway.
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
