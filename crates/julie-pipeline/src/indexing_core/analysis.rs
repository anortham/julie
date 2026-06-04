use anyhow::Result;

use julie_core::database::SymbolDatabase;

pub fn run_sqlite_analysis(db: &SymbolDatabase) -> Result<()> {
    db.compute_reference_scores()?;
    let language_configs = julie_index::search::LanguageConfigs::load_embedded();
    julie_index::analysis::compute_test_quality_metrics(db, &language_configs)?;
    julie_index::analysis::compute_test_linkage(db)?;
    Ok(())
}
