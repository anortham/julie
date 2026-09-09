use crate::database::SymbolDatabase;
use anyhow::Result;
use tracing::info;

impl SymbolDatabase {
    pub(crate) fn migration_032_add_embedding_generations(&self) -> Result<()> {
        info!("Running migration 032: Add embedding_generations table");
        self.create_embedding_generations_table()?;
        info!("Migration 032 complete: embedding_generations table added");
        Ok(())
    }
}
