use crate::extractors::{Identifier, PendingRelationship, Relationship, Symbol};
use julie_extractors::base::{ParseDiagnostic, StructuredPendingRelationship};

#[derive(Debug)]
pub struct ExtractedBatch {
    pub all_symbols: Vec<Symbol>,
    pub all_relationships: Vec<Relationship>,
    pub all_pending_relationships: Vec<PendingRelationship>,
    pub all_structured_pending_relationships: Vec<StructuredPendingRelationship>,
    pub all_identifiers: Vec<Identifier>,
    pub all_types: Vec<crate::extractors::base::TypeInfo>,
    pub all_file_infos: Vec<crate::database::FileInfo>,
    pub parse_diagnostics_by_file: Vec<(String, Vec<ParseDiagnostic>)>,
    pub files_to_clean: Vec<String>,
    pub repair_entries: Vec<(String, String)>,
    pub files_processed: usize,
}

impl ExtractedBatch {
    pub fn new() -> Self {
        Self {
            all_symbols: Vec::new(),
            all_relationships: Vec::new(),
            all_pending_relationships: Vec::new(),
            all_structured_pending_relationships: Vec::new(),
            all_identifiers: Vec::new(),
            all_types: Vec::new(),
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
