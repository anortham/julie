//! Phase 5.3 — assert SEMANTIC_INDEX_ENGINE_VERSION embeds
//! `julie_extractors::EXTRACTION_CONTRACT_VERSION` and names the pinned
//! extractor tag so any extractor shape drift triggers a stored-index mismatch.

use crate::tools::workspace::indexing::engine_version::SEMANTIC_INDEX_ENGINE_VERSION;

const ROOT_MANIFEST: &str = include_str!("../../../Cargo.toml");

fn pinned_extractors_tag() -> &'static str {
    ROOT_MANIFEST
        .lines()
        .find(|line| line.trim_start().starts_with("julie-extractors") && line.contains("tag ="))
        .and_then(|line| line.split("tag =").nth(1))
        .and_then(|rest| rest.split('"').nth(1))
        .expect("Cargo.toml must pin julie-extractors to a git tag")
}

#[test]
fn test_semantic_index_engine_version_includes_extraction_contract() {
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains(julie_extractors::EXTRACTION_CONTRACT_VERSION),
        "SEMANTIC_INDEX_ENGINE_VERSION ({}) must include EXTRACTION_CONTRACT_VERSION ({}) for drift detection",
        SEMANTIC_INDEX_ENGINE_VERSION,
        julie_extractors::EXTRACTION_CONTRACT_VERSION
    );
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains("consumer-enrichments-v1"),
        "SEMANTIC_INDEX_ENGINE_VERSION must mark Julie's consumed extractor enrichments"
    );
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains("+epoch=9"),
        "SEMANTIC_INDEX_ENGINE_VERSION must mark extraction identity epoch 9"
    );
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains("+consumer-projection-v1+receiver-schema-v31"),
        "SEMANTIC_INDEX_ENGINE_VERSION must mark consumer projection and receiver schema"
    );
}

#[test]
fn test_semantic_index_engine_version_names_pinned_extractors_tag() {
    let tag = pinned_extractors_tag();
    let marker = format!("+extractors-tag={tag}");
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains(&marker),
        "SEMANTIC_INDEX_ENGINE_VERSION ({SEMANTIC_INDEX_ENGINE_VERSION}) must contain {marker}. \
         EXTRACTION_CONTRACT_VERSION can stay byte-identical across extractor releases that still \
         change extraction output, so only the pinned tag forces the one-time reindex."
    );
}
