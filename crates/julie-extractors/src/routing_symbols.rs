//! Projections from canonical extraction results.

use crate::ExtractionResults;
use crate::base::Symbol;

pub(crate) fn project_symbols(results: ExtractionResults) -> Vec<Symbol> {
    results.symbols
}
