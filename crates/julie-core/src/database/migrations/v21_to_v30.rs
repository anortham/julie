use crate::database::SymbolDatabase;
use anyhow::Result;
use tracing::{debug, info};

impl SymbolDatabase {
    pub(super) fn migration_021_add_early_warning_reports(&self) -> Result<()> {
        info!("Running migration 021: Add early_warning_reports table");

        self.create_early_warning_reports_table()?;

        info!("Migration 021 complete: early_warning_reports table added");
        Ok(())
    }

    pub(super) fn migration_022_add_sql_performance_indexes(&self) -> Result<()> {
        info!("Running migration 022: Add SQL performance indexes");

        if self.table_exists("symbols")? {
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_symbols_reference_score_desc
                 ON symbols(reference_score DESC)
                 WHERE reference_score > 0",
                [],
            )?;
        }

        if self.table_exists("identifiers")? {
            self.conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_identifiers_file_line_kind
                 ON identifiers(file_path, start_line, kind);
                 CREATE INDEX IF NOT EXISTS idx_identifiers_file_name
                 ON identifiers(file_path, name);
                 CREATE INDEX IF NOT EXISTS idx_identifiers_kind_containing
                 ON identifiers(kind, containing_symbol_id);
                 CREATE INDEX IF NOT EXISTS idx_identifiers_name_kind_containing
                 ON identifiers(name, kind, containing_symbol_id);",
            )?;
        }

        if self.table_exists("relationships")? {
            self.conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_rel_file ON relationships(file_path)",
                [],
            )?;
        }

        info!("Migration 022 complete: SQL performance indexes added");
        Ok(())
    }

    pub(super) fn migration_023_add_tool_call_input_bytes(&self) -> Result<()> {
        info!("Running migration 023: Add input_bytes to tool_calls");
        if !self.table_exists("tool_calls")? {
            debug!("tool_calls table does not exist, skipping migration 023");
            return Ok(());
        }
        if self.has_column("tool_calls", "input_bytes")? {
            debug!("tool_calls.input_bytes already exists, skipping migration 023");
            return Ok(());
        }
        self.conn
            .execute("ALTER TABLE tool_calls ADD COLUMN input_bytes INTEGER", [])?;
        info!("Migration 023 complete: input_bytes column added to tool_calls");
        Ok(())
    }

    pub(super) fn migration_024_add_index_engine_state(&self) -> Result<()> {
        info!("Running migration 024: Add index_engine_state table");
        self.create_index_engine_state_table()?;
        info!("Migration 024 complete: index_engine_state table added");
        Ok(())
    }

    pub(super) fn migration_025_add_symbol_body_fields(&self) -> Result<()> {
        info!("Running migration 025: Add symbol body span and hash columns");
        if !self.table_exists("symbols")? {
            debug!("symbols table does not exist, skipping migration 025");
            return Ok(());
        }

        for (column, column_type) in [
            ("body_start_line", "INTEGER"),
            ("body_start_col", "INTEGER"),
            ("body_end_line", "INTEGER"),
            ("body_end_col", "INTEGER"),
            ("body_start_byte", "INTEGER"),
            ("body_end_byte", "INTEGER"),
            ("body_hash", "TEXT"),
        ] {
            if !self.has_column("symbols", column)? {
                self.conn.execute(
                    &format!("ALTER TABLE symbols ADD COLUMN {column} {column_type}"),
                    [],
                )?;
            }
        }

        info!("Migration 025 complete: symbol body fields added");
        Ok(())
    }

    pub(super) fn migration_026_add_external_extract_metadata(&self) -> Result<()> {
        info!("Running migration 026: Add external_extract_metadata table");
        self.create_external_extract_metadata_table()?;
        info!("Migration 026 complete: external_extract_metadata table added");
        Ok(())
    }

    pub(super) fn migration_027_add_type_arguments(&self) -> Result<()> {
        info!("Running migration 027: Add type_arguments table");
        self.create_type_arguments_table()?;
        info!("Migration 027 complete: type_arguments table added");
        Ok(())
    }

    pub(super) fn migration_028_add_literals(&self) -> Result<()> {
        info!("Running migration 028: Add literals table");
        self.create_literals_table()?;
        info!("Migration 028 complete: literals table added");
        Ok(())
    }

    pub(super) fn migration_029_add_extractor_enrichments(&self) -> Result<()> {
        info!("Running migration 029: Add extractor enrichment tables");
        self.create_source_regions_table()?;
        self.create_structural_facts_table()?;
        self.create_complexity_metrics_table()?;
        info!("Migration 029 complete: extractor enrichment tables added");
        Ok(())
    }

    pub(super) fn migration_030_add_web_edges(&self) -> Result<()> {
        info!("Running migration 030: Add web_edges table");
        self.create_web_edges_table()?;
        info!("Migration 030 complete: web_edges table added");
        Ok(())
    }
}
