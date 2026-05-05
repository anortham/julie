/// Stored index semantics version for symbols, identifiers, types, and relationships.
///
/// Bump this when extractor, resolver, or indexing behavior changes in a way
/// that can alter persisted derived data without changing source file hashes.
pub(crate) const SEMANTIC_INDEX_ENGINE_COMPONENT: &str = "semantic_index_engine";
pub(crate) const SEMANTIC_INDEX_ENGINE_VERSION: &str = "2026-05-05.reference-identifier-v3";
