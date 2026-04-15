//! Projections from canonical extraction results.

use crate::base::Relationship;
use crate::ExtractionResults;

pub(crate) fn project_relationships(results: ExtractionResults) -> Vec<Relationship> {
    results.relationships
}
