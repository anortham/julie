//! Projections from canonical extraction results.

use crate::base::Identifier;
use crate::ExtractionResults;

pub(crate) fn project_identifiers(results: ExtractionResults) -> Vec<Identifier> {
    results.identifiers
}
