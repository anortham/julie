use std::path::Path;

use serde::{Deserialize, Serialize};
use tantivy::Index;

use super::SearchIndex;
use crate::search::error::{Result, SearchError};
use crate::search::schema::{SchemaCompatibilitySignature, compatibility_signature};
use crate::search::tokenizer::{CodeTokenizer, TokenizerCompatibilitySignature};

const SEARCH_COMPAT_MARKER_VERSION: u32 = 4;
pub const SEARCH_COMPAT_MARKER_FILE: &str = "julie-search-compat.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct SearchCompatMarker {
    marker_version: u32,
    schema_signature: SchemaCompatibilitySignature,
    tokenizer_signature: TokenizerCompatibilitySignature,
}

impl SearchIndex {
    pub(super) fn expected_compat_marker(
        schema: &tantivy::schema::Schema,
        tokenizer: &CodeTokenizer,
    ) -> SearchCompatMarker {
        SearchCompatMarker {
            marker_version: SEARCH_COMPAT_MARKER_VERSION,
            schema_signature: compatibility_signature(schema),
            tokenizer_signature: tokenizer.compatibility_signature(),
        }
    }

    fn read_compat_marker(path: &Path) -> std::result::Result<Option<SearchCompatMarker>, String> {
        let marker_path = path.join(SEARCH_COMPAT_MARKER_FILE);
        if !marker_path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(&marker_path)
            .map_err(|err| format!("failed to read {}: {err}", marker_path.display()))?;
        let marker = serde_json::from_str::<SearchCompatMarker>(&raw)
            .map_err(|err| format!("failed to parse {}: {err}", marker_path.display()))?;

        Ok(Some(marker))
    }

    pub(super) fn write_compat_marker(path: &Path, marker: &SearchCompatMarker) -> Result<()> {
        let marker_path = path.join(SEARCH_COMPAT_MARKER_FILE);
        let payload = serde_json::to_string_pretty(marker).map_err(|err| {
            SearchError::IndexError(format!(
                "failed to serialize compatibility marker for {}: {err}",
                marker_path.display()
            ))
        })?;
        std::fs::write(marker_path, payload)?;
        Ok(())
    }

    pub(super) fn index_is_compatible(
        path: &Path,
        expected_schema: &tantivy::schema::Schema,
        actual_schema: &tantivy::schema::Schema,
        expected_marker: &SearchCompatMarker,
    ) -> bool {
        if !Self::schema_is_compatible(expected_schema, actual_schema) {
            return false;
        }

        match Self::read_compat_marker(path) {
            Ok(Some(marker)) => {
                if marker == *expected_marker {
                    true
                } else {
                    tracing::warn!(
                        "Compatibility marker mismatch at {} (expected Julie marker v{}, found v{}), recreating",
                        path.display(),
                        SEARCH_COMPAT_MARKER_VERSION,
                        marker.marker_version
                    );
                    false
                }
            }
            Ok(None) => {
                tracing::warn!(
                    "Compatibility marker missing at {} ({}), recreating",
                    path.display(),
                    SEARCH_COMPAT_MARKER_FILE
                );
                false
            }
            Err(err) => {
                tracing::warn!(
                    "Compatibility marker unreadable at {} ({}), recreating",
                    path.display(),
                    err
                );
                false
            }
        }
    }

    /// Rebuild the Tantivy index at `path`.
    ///
    /// # Atomic rename
    ///
    /// The rebuilt index is first created in `<parent>/<dirname>.tmp-rebuild`.
    /// Only after a successful `write_compat_marker` is the old directory removed
    /// and the temp directory renamed into place.  If the process crashes mid-way,
    /// the next caller cleans up the orphaned `.tmp-rebuild` before proceeding.
    pub(super) fn recreate_index(
        path: &Path,
        schema: &tantivy::schema::Schema,
        marker: &SearchCompatMarker,
    ) -> Result<Index> {
        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "tantivy".to_string());
        let parent = path.parent().unwrap_or(path);
        let tmp_path = parent.join(format!("{dir_name}.tmp-rebuild"));

        // Clean up any orphaned temp directory from a previous crashed rebuild.
        if tmp_path.exists() {
            std::fs::remove_dir_all(&tmp_path).map_err(|err| {
                SearchError::IndexError(format!(
                    "failed to remove stale tmp rebuild dir {}: {err}",
                    tmp_path.display()
                ))
            })?;
        }

        // Build the new index into the temp directory.
        std::fs::create_dir_all(&tmp_path)?;
        let _tmp_index = Index::create_in_dir(&tmp_path, schema.clone())?;
        Self::write_compat_marker(&tmp_path, marker)?;
        drop(_tmp_index);

        // Atomically replace: remove old, rename temp into final location.
        if path.exists() {
            std::fs::remove_dir_all(path)?;
        }
        std::fs::rename(&tmp_path, path).map_err(|err| {
            SearchError::IndexError(format!(
                "failed to rename {} → {}: {err}",
                tmp_path.display(),
                path.display()
            ))
        })?;

        // Re-open from the final location (the index object pointed at tmp_path).
        Index::open_in_dir(path).map_err(|err| {
            SearchError::IndexError(format!(
                "failed to open rebuilt index at {}: {err}",
                path.display()
            ))
        })
    }

    /// Check whether on-disk schema metadata matches Julie's expected schema shape.
    fn schema_is_compatible(
        expected: &tantivy::schema::Schema,
        actual: &tantivy::schema::Schema,
    ) -> bool {
        compatibility_signature(expected) == compatibility_signature(actual)
    }
}
