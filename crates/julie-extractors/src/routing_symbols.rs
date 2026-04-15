//! Projections from canonical extraction results.

use crate::base::Symbol;
use crate::ExtractionResults;

pub(crate) fn project_symbols(results: ExtractionResults) -> Vec<Symbol> {
    results.symbols
}
