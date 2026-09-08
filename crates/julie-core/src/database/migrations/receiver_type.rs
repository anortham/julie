use crate::database::SymbolDatabase;
use anyhow::Result;
use tracing::info;

impl SymbolDatabase {
    pub(crate) fn migration_031_add_identifier_receiver_type(&self) -> Result<()> {
        info!("Running migration 031: Add receiver_type column to identifiers");
        if self.table_exists("identifiers")? && !self.has_column("identifiers", "receiver_type")? {
            self.conn
                .execute("ALTER TABLE identifiers ADD COLUMN receiver_type TEXT", [])?;
        }
        info!("Migration 031 complete: receiver_type column added to identifiers");
        Ok(())
    }
}
