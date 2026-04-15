//! Projections from canonical extraction results.

use crate::ExtractionResults;
use crate::base::Identifier;

pub(crate) fn project_identifiers(results: ExtractionResults) -> Vec<Identifier> {
    results.identifiers
}
