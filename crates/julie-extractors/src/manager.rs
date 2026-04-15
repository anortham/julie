//! ExtractorManager - Public API for symbol/identifier/relationship extraction.

use crate::base::{Identifier, Relationship, Symbol};
use crate::ExtractionResults;
use std::path::Path;

/// Manager for all language extractors.
/// Provides centralized symbol extraction across supported languages.
pub struct ExtractorManager {}

impl Default for ExtractorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorManager {
    pub fn new() -> Self {
        Self {}
    }

    /// Get supported languages from the canonical registry.
    pub fn supported_languages(&self) -> Vec<&'static str> {
        crate::registry::supported_languages()
    }

    pub fn extract_symbols(
        &self,
        file_path: &str,
        content: &str,
        workspace_root: &Path,
    ) -> Result<Vec<Symbol>, anyhow::Error> {
        let results = self.extract_all(file_path, content, workspace_root)?;

        tracing::debug!(
            "Extracted {} symbols from {} file: {}",
            results.symbols.len(),
            crate::pipeline::detect_language_for_path(file_path).unwrap_or("unknown"),
            file_path
        );
        Ok(super::routing_symbols::project_symbols(results))
    }

    /// Extract all data from a file using the canonical parse-and-dispatch pipeline.
    pub fn extract_all(
        &self,
        file_path: &str,
        content: &str,
        workspace_root: &Path,
    ) -> Result<ExtractionResults, anyhow::Error> {
        crate::pipeline::extract_canonical(file_path, content, workspace_root)
    }
    /// Thin convenience wrapper over [`Self::extract_all`].
    ///
    /// Breaking change: callers must pass `workspace_root` and may no longer
    /// provide their own `symbols` slice to bypass canonical parsing.
    pub fn extract_identifiers(
        &self,
        file_path: &str,
        content: &str,
        workspace_root: &Path,
    ) -> Result<Vec<Identifier>, anyhow::Error> {
        let results = self.extract_all(file_path, content, workspace_root)?;

        tracing::debug!(
            "Extracted {} identifiers from {} file: {}",
            results.identifiers.len(),
            crate::pipeline::detect_language_for_path(file_path).unwrap_or("unknown"),
            file_path
        );
        Ok(super::routing_identifiers::project_identifiers(results))
    }

    /// Thin convenience wrapper over [`Self::extract_all`].
    ///
    /// Breaking change: callers must pass `workspace_root` and may no longer
    /// provide their own `symbols` slice to bypass canonical parsing.
    pub fn extract_relationships(
        &self,
        file_path: &str,
        content: &str,
        workspace_root: &Path,
    ) -> Result<Vec<Relationship>, anyhow::Error> {
        let results = self.extract_all(file_path, content, workspace_root)?;

        tracing::debug!(
            "Extracted {} relationships from {} file: {}",
            results.relationships.len(),
            crate::pipeline::detect_language_for_path(file_path).unwrap_or("unknown"),
            file_path
        );
        Ok(super::routing_relationships::project_relationships(results))
    }
}
