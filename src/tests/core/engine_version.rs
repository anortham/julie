//! Phase 5.3 — assert SEMANTIC_INDEX_ENGINE_VERSION embeds
//! `julie_extractors::EXTRACTION_CONTRACT_VERSION` so any extractor
//! shape drift triggers a stored-index mismatch.

use crate::tools::workspace::indexing::engine_version::SEMANTIC_INDEX_ENGINE_VERSION;

#[test]
fn test_semantic_index_engine_version_includes_extraction_contract() {
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains(julie_extractors::EXTRACTION_CONTRACT_VERSION),
        "SEMANTIC_INDEX_ENGINE_VERSION ({}) must include EXTRACTION_CONTRACT_VERSION ({}) for drift detection",
        SEMANTIC_INDEX_ENGINE_VERSION,
        julie_extractors::EXTRACTION_CONTRACT_VERSION
    );
}
