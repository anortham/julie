use julie_core::Symbol;
use julie_extractors::{
    ComplexityMetric, Identifier, Literal, ParseDiagnostic, PendingRelationship, Relationship,
    SourceRegion, StructuralFact, StructuredPendingRelationship, TypeInfo,
};

#[derive(Debug)]
pub struct ExtractedBatch {
    pub all_symbols: Vec<Symbol>,
    pub all_relationships: Vec<Relationship>,
    pub all_pending_relationships: Vec<PendingRelationship>,
    pub all_structured_pending_relationships: Vec<StructuredPendingRelationship>,
    pub all_identifiers: Vec<Identifier>,
    pub all_types: Vec<TypeInfo>,
    /// Flattened ordered/nested generic type-argument rows (Miller bridge
    /// Phase 2), accumulated per file from each result's `TypeArgumentUsage`
    /// trees. Borrowed by `canonical_write_set()` for persistence.
    pub(crate) all_type_argument_rows:
        Vec<julie_core::database::bulk::type_arguments::TypeArgumentRow>,
    /// String-literal call-args captured at carrier sites (Miller bridge Phase
    /// 3). Already carrier-classified-and-gated by the time the batch leaves
    /// `extract_files_for_indexing_with_records` (non-carrier literals dropped).
    /// Borrowed by `canonical_write_set()` for persistence.
    pub all_literals: Vec<Literal>,
    pub all_source_regions: Vec<SourceRegion>,
    pub all_structural_facts: Vec<StructuralFact>,
    pub all_complexity_metrics: Vec<ComplexityMetric>,
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
    pub fn canonical_write_set(&self) -> julie_core::database::bulk::atomic::CanonicalWriteSet<'_> {
        julie_core::database::bulk::atomic::CanonicalWriteSet {
            files: &self.all_file_infos,
            symbols: &self.all_symbols,
            relationships: &self.all_relationships,
            identifiers: &self.all_identifiers,
            types: &self.all_types,
            type_arguments: &self.all_type_argument_rows,
            literals: &self.all_literals,
            source_regions: &self.all_source_regions,
            structural_facts: &self.all_structural_facts,
            complexity_metrics: &self.all_complexity_metrics,
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
            all_source_regions: Vec::new(),
            all_structural_facts: Vec::new(),
            all_complexity_metrics: Vec::new(),
            all_file_infos: Vec::new(),
            parse_diagnostics_by_file: Vec::new(),
            files_to_clean: Vec::new(),
            repair_entries: Vec::new(),
            files_processed: 0,
        }
    }

    /// Retain only items belonging to paths in `keep_paths`.
    pub fn retain_files(&mut self, keep_paths: &std::collections::HashSet<String>) {
        self.all_file_infos
            .retain(|fi| keep_paths.contains(&fi.path));
        self.all_symbols
            .retain(|s| keep_paths.contains(&s.file_path));
        let retained_symbol_ids: std::collections::HashSet<String> =
            self.all_symbols.iter().map(|s| s.id.clone()).collect();
        self.all_relationships
            .retain(|r| keep_paths.contains(&r.file_path));
        self.all_pending_relationships
            .retain(|pr| keep_paths.contains(&pr.file_path));
        self.all_structured_pending_relationships
            .retain(|spr| keep_paths.contains(&spr.pending.file_path));
        self.all_identifiers
            .retain(|id| keep_paths.contains(&id.file_path));
        self.all_types
            .retain(|t| retained_symbol_ids.contains(&t.symbol_id));
        self.all_type_argument_rows
            .retain(|ta| keep_paths.contains(&ta.file_path));
        self.all_literals
            .retain(|lit| keep_paths.contains(&lit.file_path));
        self.all_source_regions
            .retain(|sr| keep_paths.contains(&sr.file_path));
        self.all_structural_facts
            .retain(|sf| keep_paths.contains(&sf.file_path));
        self.all_complexity_metrics
            .retain(|cm| keep_paths.contains(&cm.file_path));
        self.parse_diagnostics_by_file
            .retain(|(path, _)| keep_paths.contains(path));
        self.repair_entries
            .retain(|(path, _)| keep_paths.contains(path));
        self.files_to_clean.retain(|path| keep_paths.contains(path));
        self.files_processed = self.all_file_infos.len();
    }
}

impl Default for ExtractedBatch {
    fn default() -> Self {
        Self::new()
    }
}
