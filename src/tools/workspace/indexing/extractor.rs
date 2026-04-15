//! Symbol extraction for workspace indexing.
//! Uses the canonical extractor pipeline so workspace indexing matches the supported public API.

use crate::extractors::ExtractionResults;
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use tracing::debug;
use tree_sitter::Tree;

impl ManageWorkspaceTool {
    /// Static version for use in spawn_blocking (where self is not available)
    ///
    /// This method extracts symbols without requiring `&self`, making it suitable for use inside
    /// `spawn_blocking` closures. It delegates to the canonical extractor pipeline so indexing,
    /// JSONL handling, and path normalization all follow the same production path.
    pub(crate) fn extract_symbols_static(
        _tree: &Tree,
        file_path: &str,
        content: &str,
        _language: &str,
        workspace_root_path: &std::path::Path,
    ) -> Result<ExtractionResults> {
        debug!(
            "Extracting symbols (static): language={}, file={}",
            _language, file_path
        );
        debug!("    Tree root node: {:?}", _tree.root_node().kind());
        debug!("    Content length: {} chars", content.len());

        let results =
            crate::extractors::extract_canonical(file_path, content, workspace_root_path)?;

        debug!(
            "🎯 extract_symbols_static returning: {} symbols, {} relationships, {} identifiers, {} types for {} file: {}",
            results.symbols.len(),
            results.relationships.len(),
            results.identifiers.len(),
            results.types.len(),
            _language,
            file_path
        );

        Ok(results)
    }

    /// Determine if we should extract symbols from a file based on language
    ///
    /// CSS and HTML are indexed for text search only - no symbol extraction
    pub(crate) fn should_extract_symbols(&self, language: &str) -> bool {
        !matches!(language, "css" | "html")
    }
}
