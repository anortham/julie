use crate::database::FileInfo;
use crate::extractors::{Relationship, Symbol};

use super::type_arguments::TypeArgumentRow;

#[derive(Clone, Copy, Default)]
pub(crate) struct AtomicPersistenceMetadata<'a> {
    pub(crate) parse_diagnostics_by_file:
        &'a [(String, Vec<crate::extractors::base::ParseDiagnostic>)],
    pub(crate) repair_entries: &'a [(String, String)],
    pub(crate) mark_external_analysis_stale: bool,
}

/// References to the canonical per-file extraction data persisted by a single
/// atomic write.
///
/// The write set intentionally has borrowed slices so callers can assemble data
/// from `ExtractedBatch` without cloning. Empty slices are valid and used by
/// delete / no-op paths; callers should pass all non-symbol child rows
/// (`relationships`, `identifiers`, `types`, `type_arguments`, `literals`) that
/// should be inserted after stale file rows are removed.
#[derive(Clone, Copy, Default)]
pub(crate) struct CanonicalWriteSet<'a> {
    pub(crate) files: &'a [FileInfo],
    pub(crate) symbols: &'a [Symbol],
    pub(crate) relationships: &'a [Relationship],
    pub(crate) identifiers: &'a [crate::extractors::Identifier],
    pub(crate) types: &'a [crate::extractors::base::TypeInfo],
    /// Flattened type-argument usage rows. These are derived from extractor
    /// `TypeArgumentUsage` trees in the indexing persistence layer so the
    /// database bulk path can stay schema-shaped.
    pub(crate) type_arguments: &'a [TypeArgumentRow],
    /// String-literal call arguments captured by extractors and classified by
    /// the indexing pipeline before persistence.
    pub(crate) literals: &'a [crate::extractors::Literal],
}
