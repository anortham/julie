/// Facts schema version written to `meta.schema_version`. Any other value
/// deletes the index directory; the store never migrates.
pub const FACTS_SCHEMA_VERSION: i32 = 1;

/// Engine version compared verbatim against `meta.engine_version`.
///
/// Composed as `extractors=<EXTRACTION_CONTRACT_VERSION>+extractors-tag=<pinned tag>+facts=<FACTS_SCHEMA_VERSION>`.
/// The test in `tests/version.rs` enforces the composition, so a new extractor tag or a
/// schema bump that forgets this literal fails the build's tests.
pub const SEMANTIC_INDEX_ENGINE_VERSION: &str = "extractors=2026-06-30.ecmascript-swift-shape-v3.source-regions-v1.structural-facts-v1.complexity-metrics-v1.file-derived-component-symbols-v1.framework-route-facts-v1.react-nextjs-route-facts-v1.nuxt-route-facts-v1.web-route-facts-v3.http-boundary-facts-v1.containing-symbol-binding-v2.backend-http-boundary-v1.backend-http-boundary-v2.sql-tsql-facts-v1.test-role-strings-v2.csharp-visibility-v2.go-subtests-v1.rust-doc-test-facts-v1.fsharp-v1.marker-razorback-v1.receiver-type-facts-v1.receiver-type-facts-v2+extractors-tag=v2.42.0+facts=1";
