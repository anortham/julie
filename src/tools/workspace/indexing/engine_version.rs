/// Stored index semantics version for symbols, identifiers, types, and relationships.
///
/// Bump this when extractor, resolver, or indexing behavior changes in a way
/// that can alter persisted derived data without changing source file hashes.
///
/// `pub` visibility (was `pub(crate)`) is required so the Phase 5.3
/// composition test in `src/tests/core/engine_version.rs` can import this
/// constant to verify it embeds `julie_extractors::EXTRACTION_CONTRACT_VERSION`.
pub const SEMANTIC_INDEX_ENGINE_COMPONENT: &str = "semantic_index_engine";

/// Composed engine version compared verbatim against the stored value to decide
/// whether persisted derived data must be rebuilt. Any change to this literal
/// forces a one-time reindex of every file.
///
/// It embeds `julie_extractors::EXTRACTION_CONTRACT_VERSION`, but that substring
/// alone does not detect drift: the contract version can stay byte-identical
/// across extractor releases that still change extraction output. The
/// `+extractors-tag=` marker therefore pins the consumed release and **must be
/// updated on every extractor re-pin**, even when the contract version is
/// unchanged. The regression test in `src/tests/core/engine_version.rs` reads
/// the tag straight out of `Cargo.toml` and enforces both links.
pub const SEMANTIC_INDEX_ENGINE_VERSION: &str = "extractors=2026-06-30.ecmascript-swift-shape-v3.source-regions-v1.structural-facts-v1.complexity-metrics-v1.file-derived-component-symbols-v1.framework-route-facts-v1.react-nextjs-route-facts-v1.nuxt-route-facts-v1.web-route-facts-v3.http-boundary-facts-v1.containing-symbol-binding-v2.backend-http-boundary-v1.backend-http-boundary-v2.sql-tsql-facts-v1+consumer-enrichments-v1+schema=2026-05-05.reference-identifier-v3+web-edges-v1+extractors-tag=v2.34.3";
