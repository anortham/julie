use crate::database::FileInfo;
use julie_extractors::{Relationship, Symbol};

use super::type_arguments::TypeArgumentRow;

#[derive(Clone, Copy, Default)]
pub struct AtomicPersistenceMetadata<'a> {
    pub parse_diagnostics_by_file: &'a [(String, Vec<julie_extractors::base::ParseDiagnostic>)],
    pub repair_entries: &'a [(String, String)],
    pub mark_external_analysis_stale: bool,
}

/// References to the canonical per-file extraction data persisted by a single
/// atomic write.
///
/// The write set intentionally has borrowed slices so callers can assemble data
/// from `ExtractedBatch` without cloning. Empty slices are valid and used by
/// delete / no-op paths; callers should pass all non-symbol child rows
/// (`relationships`, `identifiers`, `types`, `type_arguments`, `literals`,
/// `source_regions`, `structural_facts`, `complexity_metrics`) that should be
/// inserted after stale file rows are removed.
#[derive(Clone, Copy, Default)]
pub struct CanonicalWriteSet<'a> {
    pub files: &'a [FileInfo],
    pub symbols: &'a [Symbol],
    pub relationships: &'a [Relationship],
    pub identifiers: &'a [julie_extractors::Identifier],
    pub types: &'a [julie_extractors::base::TypeInfo],
    /// Flattened type-argument usage rows. These are derived from extractor
    /// `TypeArgumentUsage` trees in the indexing persistence layer so the
    /// database bulk path can stay schema-shaped.
    pub type_arguments: &'a [TypeArgumentRow],
    /// String-literal call arguments captured by extractors and classified by
    /// the indexing pipeline before persistence.
    pub literals: &'a [julie_extractors::Literal],
    pub source_regions: &'a [julie_extractors::base::SourceRegion],
    pub structural_facts: &'a [julie_extractors::base::StructuralFact],
    pub complexity_metrics: &'a [julie_extractors::base::ComplexityMetric],
}
