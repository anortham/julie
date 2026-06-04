use julie_extractors::{Identifier, Literal, PendingRelationship, Relationship, Symbol};
use julie_extractors::base::{ParseDiagnostic, StructuredPendingRelationship};

#[derive(Debug)]
pub struct ExtractedBatch {
    pub all_symbols: Vec<Symbol>,
    pub all_relationships: Vec<Relationship>,
    pub all_pending_relationships: Vec<PendingRelationship>,
    pub all_structured_pending_relationships: Vec<StructuredPendingRelationship>,
    pub all_identifiers: Vec<Identifier>,
    pub all_types: Vec<julie_extractors::base::TypeInfo>,
    /// Flattened ordered/nested generic type-argument rows (Miller bridge
    /// Phase 2), accumulated per file from each result's `TypeArgumentUsage`
    /// trees. Borrowed by `canonical_write_set()` for persistence.
    pub(crate) all_type_argument_rows: Vec<julie_core::database::bulk::type_arguments::TypeArgumentRow>,
    /// String-literal call-args captured at carrier sites (Miller bridge Phase
    /// 3). Already carrier-classified-and-gated by the time the batch leaves
    /// `extract_files_for_indexing_with_records` (non-carrier literals dropped).
    /// Borrowed by `canonical_write_set()` for persistence.
    pub all_literals: Vec<Literal>,
    pub all_file_infos: Vec<julie_core::database::FileInfo>,
    pub parse_diagnostics_by_file: Vec<(String, Vec<ParseDiagnostic>)>,
    pub files_to_clean: Vec<String>,
    pub repair_entries: Vec<(String, String)>,
    pub files_processed: usize,
}

impl ExtractedBatch {
    /// Borrow this batch's canonical collections as a single
    /// [`CanonicalWriteSet`](julie_core::database::bulk::atomic::CanonicalWriteSet).
    ///
    /// This is the single batch-to-write-set mapping point for every
    /// production indexing path (live pipeline + external-extract CLI). When a
    /// new canonical collection is added to both `ExtractedBatch` and
    /// `CanonicalWriteSet`, this constructor fails to compile until the new
    /// field is wired — which is the whole point of the parameter object (plan
    /// cross-cutting Rule 3): no production path can silently drop the new data.
    pub fn canonical_write_set(
        &self,
    ) -> julie_core::database::bulk::atomic::CanonicalWriteSet<'_> {
        julie_core::database::bulk::atomic::CanonicalWriteSet {
            files: &self.all_file_infos,
            symbols: &self.all_symbols,
            relationships: &self.all_relationships,
            identifiers: &self.all_identifiers,
            types: &self.all_types,
            type_arguments: &self.all_type_argument_rows,
            literals: &self.all_literals,
        }
    }

    pub fn new() -> Self {
        Self {
            all_symbols: Vec::new(),
            all_relationships: Vec::new(),
            all_pending_relationships: Vec::new(),
            all_structured_pending_relationships: Vec::new(),
            all_identifiers: Vec::new(),
            all_types: Vec::new(),
            all_type_argument_rows: Vec::new(),
            all_literals: Vec::new(),
            all_file_infos: Vec::new(),
            parse_diagnostics_by_file: Vec::new(),
            files_to_clean: Vec::new(),
            repair_entries: Vec::new(),
            files_processed: 0,
        }
    }
}

impl Default for ExtractedBatch {
    fn default() -> Self {
        Self::new()
    }
}
