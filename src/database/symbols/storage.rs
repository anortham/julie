// Symbol storage and deletion operations

use super::super::helpers::SYMBOL_UPSERT_SQL;
use super::super::*;
use super::annotations::{delete_annotations_for_file, replace_annotations_batch};
use anyhow::Result;
use rusqlite::params;
use tracing::debug;

impl SymbolDatabase {
    /// Store symbols and annotation rows atomically.
    pub fn store_symbols(&mut self, symbols: &[Symbol]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        debug!("Storing {} symbols", symbols.len());
        let tx = self.conn.transaction()?;

        for symbol in symbols {
            let metadata_json = symbol
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            // Serialize visibility enum to string
            let visibility_str = symbol.visibility.as_ref().map(|v| match v {
                crate::extractors::base::Visibility::Public => "public",
                crate::extractors::base::Visibility::Private => "private",
                crate::extractors::base::Visibility::Protected => "protected",
            });

            // Debug log for markdown symbols
            if symbol.language == "markdown" {
                debug!(
                    "💾 Storing markdown symbol: name={}, file={}, content_type={:?}",
                    symbol.name, symbol.file_path, symbol.content_type
                );
            }

            tx.execute(
                SYMBOL_UPSERT_SQL,
                params![
                    symbol.id,
                    symbol.name,
                    symbol.kind.to_string(),
                    symbol.language,
                    symbol.file_path,
                    symbol.signature,
                    symbol.start_line,
                    symbol.start_column,
                    symbol.end_line,
                    symbol.end_column,
                    symbol.start_byte,
                    symbol.end_byte,
                    symbol.doc_comment,
                    visibility_str,
                    symbol.code_context,
                    symbol.parent_id,
                    metadata_json,
                    symbol.semantic_group,
                    symbol.confidence,
                    symbol.content_type
                ],
            )?;
        }

        replace_annotations_batch(&tx, symbols)?;
        tx.commit()?;
        debug!("Successfully stored {} symbols", symbols.len());
        Ok(())
    }

    /// Store symbols with automatic transaction management
    /// Use this for simple one-off storage where no transaction is active
    pub fn store_symbols_transactional(&mut self, symbols: &[Symbol]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        debug!("Storing {} symbols (with transaction)", symbols.len());

        let tx = self.conn.transaction()?;

        for symbol in symbols {
            let metadata_json = symbol
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            // Serialize visibility enum to string
            let visibility_str = symbol.visibility.as_ref().map(|v| match v {
                crate::extractors::base::Visibility::Public => "public",
                crate::extractors::base::Visibility::Private => "private",
                crate::extractors::base::Visibility::Protected => "protected",
            });

            tx.execute(
                SYMBOL_UPSERT_SQL,
                params![
                    symbol.id,
                    symbol.name,
                    symbol.kind.to_string(),
                    symbol.language,
                    symbol.file_path,
                    symbol.signature,
                    symbol.start_line,
                    symbol.start_column,
                    symbol.end_line,
                    symbol.end_column,
                    symbol.start_byte,
                    symbol.end_byte,
                    symbol.doc_comment,
                    visibility_str,
                    symbol.code_context,
                    symbol.parent_id,
                    metadata_json,
                    symbol.semantic_group,
                    symbol.confidence,
                    symbol.content_type
                ],
            )?;
        }

        replace_annotations_batch(&tx, symbols)?;
        tx.commit()?;
        debug!(
            "Successfully stored {} symbols (transaction committed)",
            symbols.len()
        );
        Ok(())
    }

    pub fn delete_symbols_for_file(&self, file_path: &str) -> Result<()> {
        delete_annotations_for_file(&self.conn, file_path)?;
        self.conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;
        Ok(())
    }

    pub fn delete_symbols_for_file_in_workspace(&self, file_path: &str) -> Result<()> {
        delete_annotations_for_file(&self.conn, file_path)?;
        let count = self.conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;
        debug!("Deleted {} symbols for file '{}'", count, file_path);
        Ok(())
    }
}
