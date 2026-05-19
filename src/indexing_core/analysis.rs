use anyhow::Result;

use crate::database::SymbolDatabase;

pub fn run_sqlite_analysis(db: &SymbolDatabase) -> Result<()> {
    db.compute_reference_scores()?;
    let language_configs = crate::search::LanguageConfigs::load_embedded();
    crate::analysis::compute_test_quality_metrics(db, &language_configs)?;
    crate::analysis::compute_test_linkage(db)?;
    Ok(())
}
