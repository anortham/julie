use julie_core::database::FileInfo;
use julie_core::database::bulk::atomic::CanonicalWriteSet;
use julie_pipeline::indexing_core::normalized::NormalizedExtractionData;

pub(crate) struct WatcherExtractionWrite {
    pub normalized: NormalizedExtractionData,
    pub file_info: FileInfo,
}

impl WatcherExtractionWrite {
    pub(crate) fn canonical_write_set(&self) -> CanonicalWriteSet<'_> {
        CanonicalWriteSet {
            files: std::slice::from_ref(&self.file_info),
            symbols: &self.normalized.symbols,
            relationships: &self.normalized.relationships,
            identifiers: &self.normalized.identifiers,
            types: &self.normalized.types,
            type_arguments: &self.normalized.type_argument_rows,
            literals: &self.normalized.literals,
            source_regions: &self.normalized.source_regions,
            structural_facts: &self.normalized.structural_facts,
            complexity_metrics: &self.normalized.complexity_metrics,
        }
    }
}
