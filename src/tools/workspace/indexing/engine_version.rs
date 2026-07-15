/// Stored index semantics version for symbols, identifiers, types, and relationships.
///
/// Bump this when extractor, resolver, or indexing behavior changes in a way
/// that can alter persisted derived data without changing source file hashes.
///
/// `pub` visibility (was `pub(crate)`) is required so the Phase 5.3
/// composition test in `src/tests/core/engine_version.rs` can import this
/// constant to verify it embeds `julie_extractors::EXTRACTION_CONTRACT_VERSION`.
pub const SEMANTIC_INDEX_ENGINE_COMPONENT: &str = "semantic_index_engine";

/// Composed engine version embedding the extractor crate's
/// `EXTRACTION_CONTRACT_VERSION` so any shape drift in extractor outputs
/// triggers a stored-index mismatch. Keep this literal in lockstep with
/// `julie_extractors::EXTRACTION_CONTRACT_VERSION`; the regression test in
/// `src/tests/core/engine_version.rs` enforces the link.
pub const SEMANTIC_INDEX_ENGINE_VERSION: &str = "extractors=2026-06-30.ecmascript-swift-shape-v3.source-regions-v1.structural-facts-v1.complexity-metrics-v1.file-derived-component-symbols-v1.framework-route-facts-v1.react-nextjs-route-facts-v1.nuxt-route-facts-v1.web-route-facts-v3.http-boundary-facts-v1.containing-symbol-binding-v2.backend-http-boundary-v1.backend-http-boundary-v2.sql-tsql-facts-v1+schema=2026-05-05.reference-identifier-v3";
