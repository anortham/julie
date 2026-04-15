//! Projections from canonical extraction results.

use crate::ExtractionResults;
use crate::base::Relationship;

pub(crate) fn project_relationships(results: ExtractionResults) -> Vec<Relationship> {
    results.relationships
}
