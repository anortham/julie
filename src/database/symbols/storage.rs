// Symbol storage and deletion operations

use super::super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::debug;

impl SymbolDatabase {
    /// Store symbols within an existing transaction
    /// Use this when the caller is already managing transactions (file watcher, bulk operations)
    pub fn store_symbols(&mut self, symbols: &[Symbol]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        debug!("Storing {} symbols", symbols.len());

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

            self.conn.execute(
                "INSERT OR REPLACE INTO symbols
                 (id, name, kind, language, file_path, signature, start_line, start_col,
                  end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                  parent_id, metadata, semantic_group, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                    symbol.confidence
                ],
            )?;
        }

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
                "INSERT OR REPLACE INTO symbols
                 (id, name, kind, language, file_path, signature, start_line, start_col,
                  end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                  parent_id, metadata, semantic_group, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                    symbol.confidence
                ],
            )?;
        }

        tx.commit()?;
        debug!("Successfully stored {} symbols (transaction committed)", symbols.len());
        Ok(())
    }

    pub fn delete_symbols_for_file(&self, file_path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;
        Ok(())
    }

    pub fn delete_symbols_for_file_in_workspace(&self, file_path: &str) -> Result<()> {
        let count = self.conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;

        debug!("Deleted {} symbols for file '{}'", count, file_path);
        Ok(())
    }
}
