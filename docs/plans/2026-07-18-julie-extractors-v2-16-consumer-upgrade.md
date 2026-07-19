# Julie Extractors v2.16 Complete Consumer Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Upgrade Julie from `julie-extractors` v2.14.0 to v2.16.0 and consume source regions, structural facts, and complexity metrics end to end through canonical persistence and agent-facing tools.

**Architecture:** Add one typed SQLite storage lane and one shared normalization seam from `ExtractionResults` to `CanonicalWriteSet`, used by batch indexing, watcher updates, and external extraction. Expose structural facts through a new `patterns` tool, source regions through a `fast_search` wire filter, and complexity through `deep_dive`, while leaving ordinary search behavior unchanged.

**Tech Stack:** Rust 2024, `julie-extractors` v2.16.0, tree-sitter 0.26.11, SQLite/rusqlite, Tantivy, rmcp, schemars, serde/serde_json, cargo-nextest, Julie xtask tiers.

**Architecture Quality:** Approved in `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-design.md`. Architecture risk is high because canonical persistence, all indexing writers, a new public tool, search parameters, and deep-dive formatting change. The required shape is typed domain tables plus one shared `NormalizedExtractionData` seam; tools must use `julie-core` query APIs rather than raw SQL.

## Global Constraints

- Target extractor release: `v2.16.0`.
- Current baseline: `v2.14.0` at Julie `origin/main` commit `d3dcda40c16a8f93b2dc4a9de9bd20ac62e72295`.
- Preserve all upstream `SourceRegion`, `StructuralFact`, and `ComplexityMetric` fields.
- Full indexing, watcher indexing, and external extraction must persist identical canonical data.
- `patterns` operations are exactly `list`, `summary`, and `search`.
- `patterns` summary groups are exactly `language_pattern_capture`, `file`, and `directory`.
- Source-region names are exactly `comment`, `doc_comment`, `string_literal`, and `embedded`; `docstring` aliases `doc_comment`.
- Region-scoped semantic and hybrid search are invalid because region facts describe source text, not symbol embeddings.
- Limits clamp to 1–500.
- Stored paths remain relative Unix-style paths.
- All 34 supported languages remain first-class; no language-name allowlist may be added to persistence or tool filtering.
- No new implementation file may exceed 500 lines; no test file may exceed 1000 lines.
- Tests contain no narration comments.
- `CLAUDE.md` and `AGENTS.md` remain byte-for-byte identical if either changes.
- Do not push, release, or publish without separate user approval.

---

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, `xtask/test_tiers.toml`, and `docs/plans/verification-ledger-template.md`.

**Worker red/green scope:** Run `cargo check` after implementation edits, then the exact named test assigned in each task with `cargo nextest run --lib <exact_test_name>`; the xtask dependency-contract integration test uses `cargo nextest run -p xtask extractor_dependency_release_is_v2_16_0`.

**Worker ceiling:** Two executions of each assigned exact test per TDD cycle: one RED and one GREEN. Workers do not run `cargo xtask test changed`, any xtask tier, an unfiltered nextest command, or concurrent test commands.

**Worker gate invariant:** The named test proves the task's new public or persistence contract, including cleanup and target-workspace isolation where applicable; `cargo check` proves the workspace compiles after the slice.

**Lead affected-change scope:** Run `cargo xtask test changed` after Tasks 3 and 6, once per coherent batch.

**Branch gate:** Run, sequentially, `cargo fmt --check`, `cargo clippy`, `cargo xtask test bucket extractor-dep-integration`, `cargo xtask test system`, `cargo xtask test dogfood`, and `cargo xtask test dev`.

**Replay/metric evidence:** Hard gates are exact row equality/cleanup assertions, exact tool output assertions, workspace-isolation assertions, and all branch commands exiting zero. Pattern counts, query timing, and output byte counts are report-only.

**Escalation triggers:** Run `cargo xtask test full` only if `changed` selects shared infrastructure beyond the documented buckets, a branch gate exposes cross-subsystem regressions, or the implementation changes Tantivy schema/scoring rather than only the region-filtered line path.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Copy `docs/plans/verification-ledger-template.md` to `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-verification.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. For replay or metric evidence, also record hard-gate metrics and report-only metrics. If the same HEAD already has a passing ledger entry for the required scope, reuse that evidence instead of rerunning the same expensive gate.

Baseline evidence already collected on clean `d3dcda40c16a8f93b2dc4a9de9bd20ac62e72295`: `cargo xtask test nano` passed `core-database` and `core-fast` in 106.7 seconds.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Pin and lock v2.16 | None - serial | Root/workspace Cargo manifests, `Cargo.lock`, engine-version stamp, dependency contract test | Yes | Establishes the exact upstream types and lockfile used by every later task. |
| Task 2: Add canonical enrichment storage | None - serial | `julie-core` schema, migrations, bulk write/cleanup/query modules, core persistence tests | Yes | Defines the database and `CanonicalWriteSet` contracts consumed by all writers and tools. |
| Task 3: Unify extraction writers | None - serial | `julie-pipeline` normalization/batch extraction, runtime watcher adapter, external-extract and indexing tests/fixture | Yes | Requires Task 2's write shape and must land before any consumer tool can query real indexed data. |
| Task 4: Add the `patterns` tool | Batch A | Structural-fact tool module, handler/router, CLI command, tool metadata/docs-contract tests | No | None - safe parallel batch. |
| Task 5: Add region-scoped `fast_search` | Batch A | Fast-search wire params, source-region line-search path, region tests and telemetry | No | None - safe parallel batch. |
| Task 6: Add complexity to `deep_dive` | Batch A | Deep-dive data/formatting and complexity tests | No | None - safe parallel batch. |
| Task 7: Synchronize docs and run branch gates | None - serial | Current product/dependency/extraction docs, TODO closure, verification ledger | Yes | Requires all code slices and their final public contracts. |

Tasks 1–3 and 7 use `serial-worker-commit`. Tasks 4–6 use `parallel-lead-commit`; workers hand verified diffs to the lead, and the lead reviews, stages, and commits the combined non-overlapping batch.

### Task 1: Pin and lock `julie-extractors` v2.16.0

**Files:**
- Create: `xtask/tests/extractor_dependency_contract_tests.rs`
- Modify: `Cargo.toml:36-41`
- Modify: `crates/julie-core/Cargo.toml:14-17`
- Modify: `crates/julie-index/Cargo.toml:10-13`
- Modify: `crates/julie-pipeline/Cargo.toml:14-18`
- Modify: `crates/julie-runtime/Cargo.toml:14-19`
- Modify: `crates/julie-tools/Cargo.toml:14-18`
- Modify: `Cargo.lock`
- Modify: `src/tools/workspace/indexing/engine_version.rs:8-17`
- Test: `src/tests/core/engine_version.rs:8-15`

**Interfaces:**
- Consumes: upstream tag `v2.16.0` and `julie_extractors::EXTRACTION_CONTRACT_VERSION`.
- Produces: every workspace crate resolves one `julie-extractors` v2.16.0 source; `SEMANTIC_INDEX_ENGINE_VERSION` includes the exact upstream contract plus `consumer-enrichments-v1`.

**Contract inputs:** [v2.15.0 release](https://github.com/anortham/julie-extractors/releases/tag/v2.15.0), [v2.16.0 release](https://github.com/anortham/julie-extractors/releases/tag/v2.16.0), and the upstream constant in `crates/julie-extractors/src/lib.rs`.

**File ownership:** Root/workspace Cargo manifests, `Cargo.lock`, engine-version stamp, dependency contract test

**Serialization required:** Yes

**Dependency reason:** Establishes the exact upstream types and lockfile used by every later task.

**Step 1: Write the failing dependency contract test**

```rust
use std::path::{Path, PathBuf};

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "crates/julie-core/Cargo.toml",
    "crates/julie-index/Cargo.toml",
    "crates/julie-pipeline/Cargo.toml",
    "crates/julie-runtime/Cargo.toml",
    "crates/julie-tools/Cargo.toml",
];

fn repo_file(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(path)
}

#[test]
fn extractor_dependency_release_is_v2_16_0() {
    for manifest in MANIFESTS {
        let contents = std::fs::read_to_string(repo_file(manifest)).unwrap();
        let parsed: toml::Value = toml::from_str(&contents).unwrap();
        let dependency = parsed
            .get("dependencies")
            .and_then(|value| value.get("julie-extractors"))
            .unwrap_or_else(|| panic!("{manifest} has no julie-extractors dependency"));

        assert_eq!(
            dependency.get("tag").and_then(toml::Value::as_str),
            Some("v2.16.0"),
            "{manifest} must pin v2.16.0"
        );
        assert_eq!(
            dependency.get("git").and_then(toml::Value::as_str),
            Some("https://github.com/anortham/julie-extractors"),
            "{manifest} must use the canonical upstream"
        );
    }
}
```

**Step 2: Run the test to verify it fails**

Run: `cargo nextest run -p xtask extractor_dependency_release_is_v2_16_0`

Expected: FAIL because every manifest still reports `v2.14.0`.

**Step 3: Update every pin, lockfile, and engine stamp**

Use this exact dependency entry in all six manifests:

```toml
julie-extractors = { git = "https://github.com/anortham/julie-extractors", tag = "v2.16.0" }
```

Set the engine version to this exact v2.16 contract literal:

```rust
pub const SEMANTIC_INDEX_ENGINE_VERSION: &str =
    "extractors=2026-06-30.ecmascript-swift-shape-v3.source-regions-v1.structural-facts-v1.complexity-metrics-v1.file-derived-component-symbols-v1.framework-route-facts-v1.react-nextjs-route-facts-v1.nuxt-route-facts-v1.web-route-facts-v3.http-boundary-facts-v1.containing-symbol-binding-v2.backend-http-boundary-v1.backend-http-boundary-v2.sql-tsql-facts-v1+consumer-enrichments-v1+schema=2026-05-05.reference-identifier-v3";
```

Extend the existing engine-version test:

```rust
#[test]
fn test_semantic_index_engine_version_includes_extraction_contract() {
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains(julie_extractors::EXTRACTION_CONTRACT_VERSION)
    );
    assert!(
        SEMANTIC_INDEX_ENGINE_VERSION.contains("consumer-enrichments-v1")
    );
}
```

Run `cargo update -p julie-extractors` to refresh `Cargo.lock`; do not hand-edit
the lockfile.

**Step 4: Run checks and exact tests**

Run: `cargo check`

Expected: PASS.

Run: `cargo nextest run -p xtask extractor_dependency_release_is_v2_16_0`

Expected: PASS.

Run: `cargo nextest run --lib test_semantic_index_engine_version_includes_extraction_contract`

Expected: PASS.

**Step 5: Apply commit mode**

- `serial-worker-commit`: commit only the owned files with
  `chore(extractors): pin v2.16.0` and record the SHA.

**Acceptance criteria:**
- [ ] All six manifests and `Cargo.lock` resolve v2.16.0.
- [ ] The engine version forces a one-time reindex for Julie's newly consumed domains.
- [ ] Both exact contract tests and `cargo check` pass.
- [ ] The owned change is committed and its SHA is recorded.

### Task 2: Add canonical storage for all three enrichment domains

**Files:**
- Create: `crates/julie-core/src/database/bulk/source_regions.rs`
- Create: `crates/julie-core/src/database/bulk/structural_facts.rs`
- Create: `crates/julie-core/src/database/bulk/complexity_metrics.rs`
- Create: `crates/julie-core/src/database/source_regions.rs`
- Create: `crates/julie-core/src/database/structural_facts.rs`
- Create: `crates/julie-core/src/database/complexity_metrics.rs`
- Modify: `crates/julie-core/src/database/mod.rs:19-41`
- Modify: `crates/julie-core/src/database/schema.rs:9-34`
- Modify: `crates/julie-core/src/database/schema_enrichments.rs:80-123`
- Modify: `crates/julie-core/src/database/migrations.rs:16,97-129,912-917`
- Modify: `crates/julie-core/src/database/bulk/mod.rs:8-55`
- Modify: `crates/julie-core/src/database/bulk/write_set.rs:23-36`
- Modify: `crates/julie-core/src/database/bulk/atomic.rs:30-45,299-338`
- Modify: `crates/julie-core/src/database/bulk/cleanup.rs:14-83`
- Test: `src/tests/core/incremental_update_atomic/enrichments.rs`
- Test: `crates/julie-core/src/tests/database/migrations.rs`

**Interfaces:**
- Consumes: upstream `SourceRegion`, `SourceRegionKind`, `StructuralFact`, and `ComplexityMetric`.
- Produces: schema version 29; three typed table modules; `CanonicalWriteSet` fields `source_regions`, `structural_facts`, and `complexity_metrics`; read APIs used by Tasks 4–6.

**Contract inputs:** Task 1's v2.16 types; upstream field shapes cited in the design; existing `insert_literals_tx`, `create_literals_table`, and `delete_file_rows_tx` are the persistence patterns.

**File ownership:** `julie-core` schema, migrations, bulk write/cleanup/query modules, core persistence tests

**Serialization required:** Yes

**Dependency reason:** Defines the database and `CanonicalWriteSet` contracts consumed by all writers and tools.

**Step 1: Write the failing atomic round-trip and cleanup test**

Add a focused test named
`test_extractor_enrichment_domains_roundtrip_replace_and_delete` using this
complete fixture shape:

```rust
let source_region = julie_extractors::base::SourceRegion {
    id: "region-1".into(),
    file_path: "src/lib.rs".into(),
    language: "rust".into(),
    kind: julie_extractors::base::SourceRegionKind::DocComment,
    containing_symbol_id: Some("symbol-1".into()),
    start_line: 1,
    start_column: 0,
    end_line: 1,
    end_column: 12,
    start_byte: 0,
    end_byte: 12,
    metadata: Some(std::collections::HashMap::from([(
        "style".into(),
        serde_json::json!("outer"),
    )])),
};

let structural_fact = julie_extractors::base::StructuralFact {
    id: "fact-1".into(),
    file_path: "src/lib.rs".into(),
    language: "rust".into(),
    pattern_id: "http.client_request.v1".into(),
    capture_name: "request".into(),
    node_kind: "call_expression".into(),
    containing_symbol_id: Some("symbol-1".into()),
    start_line: 3,
    start_column: 4,
    end_line: 3,
    end_column: 42,
    start_byte: 30,
    end_byte: 68,
    confidence: 0.95,
    metadata: Some(std::collections::HashMap::from([
        ("client".into(), serde_json::json!("reqwest")),
        ("method".into(), serde_json::json!("GET")),
    ])),
};

let complexity_metric = julie_extractors::base::ComplexityMetric {
    id: "complexity-1".into(),
    file_path: "src/lib.rs".into(),
    language: "rust".into(),
    scope: "function".into(),
    symbol_id: Some("symbol-1".into()),
    algorithm_id: "structural-v1".into(),
    covered_lines: 8,
    covered_bytes: 96,
    decision_count: 2,
    loop_count: 1,
    max_nesting_depth: 2,
    parameter_count: Some(1),
    start_line: 2,
    start_column: 0,
    end_line: 9,
    end_column: 1,
    start_byte: 13,
    end_byte: 109,
    metadata: None,
};

let write_set = CanonicalWriteSet {
    files: &[file_info.clone()],
    symbols: &[symbol.clone()],
    source_regions: &[source_region.clone()],
    structural_facts: &[structural_fact.clone()],
    complexity_metrics: &[complexity_metric.clone()],
    ..Default::default()
};
db.incremental_update_atomic_with_metadata(
    &["src/lib.rs".into()],
    &write_set,
    "workspace-a",
    AtomicPersistenceMetadata::default(),
)
.unwrap();

assert_eq!(
    db.get_source_regions_for_file("src/lib.rs", &[]).unwrap(),
    vec![source_region]
);
assert_eq!(
    db.search_structural_facts(&Default::default()).unwrap(),
    vec![structural_fact]
);
assert_eq!(
    db.get_complexity_metric_for_symbol("symbol-1").unwrap(),
    Some(complexity_metric)
);

db.incremental_update_atomic_with_metadata(
    &["src/lib.rs".into()],
    &CanonicalWriteSet::default(),
    "workspace-a",
    AtomicPersistenceMetadata::default(),
)
.unwrap();

assert!(db.get_source_regions_for_file("src/lib.rs", &[]).unwrap().is_empty());
assert!(db.search_structural_facts(&Default::default()).unwrap().is_empty());
assert_eq!(db.get_complexity_metric_for_symbol("symbol-1").unwrap(), None);
```

Add `test_migration_029_adds_extractor_enrichment_tables` that opens a v28
fixture, runs migrations, asserts version 29, and checks all three table names
through `sqlite_master`.

**Step 2: Run the tests to verify they fail**

Run: `cargo nextest run --lib test_extractor_enrichment_domains_roundtrip_replace_and_delete`

Expected: FAIL because `CanonicalWriteSet` and database APIs lack the domains.

Run: `cargo nextest run --lib test_migration_029_adds_extractor_enrichment_tables`

Expected: FAIL because schema version 29 does not exist.

**Step 3: Add schema version 29**

Set:

```rust
pub const LATEST_SCHEMA_VERSION: i32 = 29;
```

Add `29 => self.migration_029_add_extractor_enrichments()?` and implement:

```rust
fn migration_029_add_extractor_enrichments(&self) -> Result<()> {
    self.create_source_regions_table()?;
    self.create_structural_facts_table()?;
    self.create_complexity_metrics_table()?;
    Ok(())
}
```

The three create methods must execute this complete logical schema:

```sql
CREATE TABLE IF NOT EXISTS source_regions (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    language TEXT NOT NULL,
    kind TEXT NOT NULL,
    containing_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
    start_line INTEGER NOT NULL,
    start_col INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_col INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    metadata TEXT,
    last_indexed INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_source_regions_file_kind
    ON source_regions(file_path, kind);
CREATE INDEX IF NOT EXISTS idx_source_regions_containing
    ON source_regions(containing_symbol_id);

CREATE TABLE IF NOT EXISTS structural_facts (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    language TEXT NOT NULL,
    pattern_id TEXT NOT NULL,
    capture_name TEXT NOT NULL,
    node_kind TEXT NOT NULL,
    containing_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
    start_line INTEGER NOT NULL,
    start_col INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_col INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    confidence REAL NOT NULL,
    metadata TEXT,
    last_indexed INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_structural_facts_pattern
    ON structural_facts(pattern_id);
CREATE INDEX IF NOT EXISTS idx_structural_facts_file
    ON structural_facts(file_path);
CREATE INDEX IF NOT EXISTS idx_structural_facts_language_pattern
    ON structural_facts(language, pattern_id);
CREATE INDEX IF NOT EXISTS idx_structural_facts_containing
    ON structural_facts(containing_symbol_id);

CREATE TABLE IF NOT EXISTS complexity_metrics (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
    language TEXT NOT NULL,
    scope TEXT NOT NULL,
    symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
    algorithm_id TEXT NOT NULL,
    covered_lines INTEGER NOT NULL,
    covered_bytes INTEGER NOT NULL,
    decision_count INTEGER NOT NULL,
    loop_count INTEGER NOT NULL,
    max_nesting_depth INTEGER NOT NULL,
    parameter_count INTEGER,
    start_line INTEGER NOT NULL,
    start_col INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    end_col INTEGER NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    metadata TEXT,
    last_indexed INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_complexity_metrics_symbol
    ON complexity_metrics(symbol_id);
CREATE INDEX IF NOT EXISTS idx_complexity_metrics_file
    ON complexity_metrics(file_path);
CREATE INDEX IF NOT EXISTS idx_complexity_metrics_language_scope
    ON complexity_metrics(language, scope);
```

Fresh schema initialization must call the same three create methods.

**Step 4: Extend the atomic write and cleanup contracts**

Extend `CanonicalWriteSet`:

```rust
pub source_regions: &'a [julie_extractors::base::SourceRegion],
pub structural_facts: &'a [julie_extractors::base::StructuralFact],
pub complexity_metrics: &'a [julie_extractors::base::ComplexityMetric],
```

Extend `InsertCounts` with the same three count fields and include them in
`has_changes`. In `insert_batch_tx`, insert after symbols and normalize missing
`containing_symbol_id`/`symbol_id` values to `NULL` using the existing
`valid_symbol_ids` set:

```rust
counts.source_regions = insert_source_regions_tx(
    tx,
    write_set.source_regions,
    Some(&valid_symbol_ids),
)?;
counts.structural_facts = insert_structural_facts_tx(
    tx,
    write_set.structural_facts,
    Some(&valid_symbol_ids),
)?;
counts.complexity_metrics = insert_complexity_metrics_tx(
    tx,
    write_set.complexity_metrics,
    Some(&valid_symbol_ids),
)?;
```

Before identifiers/symbols/files are deleted, add:

```rust
tx.execute(
    "DELETE FROM source_regions
     WHERE file_path = ?1
        OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
    params![file_path],
)?;
tx.execute(
    "DELETE FROM structural_facts
     WHERE file_path = ?1
        OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
    params![file_path],
)?;
tx.execute(
    "DELETE FROM complexity_metrics
     WHERE file_path = ?1
        OR symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
    params![file_path],
)?;
```

`delete_all_indexed_rows_tx` must delete all three tables before `symbols` and
`files`.

**Step 5: Implement typed read APIs**

The produced public APIs are:

```rust
#[derive(Debug, Clone)]
pub struct StructuralFactQuery {
    pub pattern_ids: Vec<String>,
    pub path_pattern: Option<String>,
    pub language: Option<String>,
    pub metadata_equals: Vec<(String, String)>,
    pub limit: usize,
}

impl Default for StructuralFactQuery {
    fn default() -> Self {
        Self {
            pattern_ids: Vec::new(),
            path_pattern: None,
            language: None,
            metadata_equals: Vec::new(),
            limit: 50,
        }
    }
}

impl SymbolDatabase {
    pub fn get_source_regions_for_file(
        &self,
        file_path: &str,
        kinds: &[SourceRegionKind],
    ) -> Result<Vec<SourceRegion>>;

    pub fn observed_structural_patterns(
        &self,
        language: Option<&str>,
        path_pattern: Option<&str>,
    ) -> Result<Vec<(String, u64)>>;

    pub fn search_structural_facts(
        &self,
        query: &StructuralFactQuery,
    ) -> Result<Vec<StructuralFact>>;

    pub fn get_complexity_metric_for_symbol(
        &self,
        symbol_id: &str,
    ) -> Result<Option<ComplexityMetric>>;
}
```

Use parameterized SQL for values. Apply `path_pattern` through
`julie_core::glob::matches_glob_pattern` after a bounded database read. For
metadata equality, require `json_valid(metadata)` and bind each JSON path and
value; never interpolate user values into SQL. Convert stored kind strings with
an exhaustive match and return an error for corrupt values.

**Step 6: Run checks and exact tests**

Run: `cargo check`

Expected: PASS.

Run: `cargo nextest run --lib test_migration_029_adds_extractor_enrichment_tables`

Expected: PASS.

Run: `cargo nextest run --lib test_extractor_enrichment_domains_roundtrip_replace_and_delete`

Expected: PASS.

**Step 7: Apply commit mode**

- `serial-worker-commit`: commit only the owned files with
  `feat(database): persist extractor enrichments` and record the SHA.

**Acceptance criteria:**
- [ ] Fresh and migrated databases have all three typed tables and indexes.
- [ ] Atomic insert, replace, workspace rebuild, file delete, and full delete cannot leave stale enrichment rows.
- [ ] Missing symbol references normalize to `NULL`.
- [ ] Typed read APIs round-trip all upstream fields.
- [ ] Exact tests and `cargo check` pass.
- [ ] The owned change is committed and its SHA is recorded.

### Task 3: Unify full indexing, watcher updates, and external extraction

**Files:**
- Create: `crates/julie-pipeline/src/indexing_core/normalized.rs`
- Create: `crates/julie-runtime/src/watcher/extraction_write.rs`
- Create: `fixtures/extraction/consumer-upgrade/rust_http_client.rs`
- Modify: `crates/julie-pipeline/src/indexing_core/mod.rs`
- Modify: `crates/julie-pipeline/src/indexing_core/batch.rs:5-72`
- Modify: `crates/julie-pipeline/src/indexing_core/extraction.rs:45-274,308-444`
- Modify: `crates/julie-runtime/src/watcher/mod.rs`
- Modify: `crates/julie-runtime/src/watcher/handlers.rs:76-438`
- Test: `src/tests/tools/workspace/processor.rs`
- Test: `src/tests/external_extract/operations/enrichment_scan.rs`
- Test: `crates/julie-runtime/src/tests/watcher_handlers.rs`

**Interfaces:**
- Consumes: Task 2's `CanonicalWriteSet` fields and upstream `ExtractionResults`.
- Produces: `NormalizedExtractionData`, `normalize_extraction_results`, enriched `ExtractedBatch`, and one watcher write adapter.

**Contract inputs:** Existing literal carrier and test-role classifiers, `flatten_type_argument_usages`, relative-path storage, and Task 2 typed slices.

**File ownership:** `julie-pipeline` normalization/batch extraction, runtime watcher adapter, external-extract and indexing tests/fixture

**Serialization required:** Yes

**Dependency reason:** Requires Task 2's write shape and must land before any consumer tool can query real indexed data.

**Step 1: Add a failing end-to-end extraction test**

Create this fixture:

```rust
/// Sends a request when the feature is enabled.
pub async fn fetch_user(enabled: bool, retries: usize) -> Result<(), reqwest::Error> {
    if enabled {
        for _ in 0..retries {
            reqwest::Client::new()
                .get("/api/users/{id}")
                .send()
                .await?;
        }
    }
    Ok(())
}
```

Add `extract_scan_persists_v2_16_enrichment_domains`:

```rust
#[tokio::test]
async fn extract_scan_persists_v2_16_enrichment_domains() {
    let fixture = external_extract_fixture("fixtures/extraction/consumer-upgrade");
    let report = run_external_scan(&fixture.scan_args()).await.unwrap();
    assert_eq!(report.counts.files_updated, 1);

    let db = SymbolDatabase::new(fixture.db_path()).unwrap();
    let regions = db
        .get_source_regions_for_file("rust_http_client.rs", &[])
        .unwrap();
    assert!(regions.iter().any(|region| {
        region.kind == SourceRegionKind::DocComment
    }));

    let facts = db
        .search_structural_facts(&StructuralFactQuery {
            pattern_ids: vec!["http.client_request.v1".into()],
            limit: 50,
            ..Default::default()
        })
        .unwrap();
    assert!(facts.iter().any(|fact| {
        fact.metadata.as_ref().is_some_and(|metadata| {
            metadata.get("client") == Some(&serde_json::json!("reqwest"))
        })
    }));

    let symbol = db
        .find_symbols_by_name("fetch_user")
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let metric = db
        .get_complexity_metric_for_symbol(&symbol.id)
        .unwrap()
        .unwrap();
    assert!(metric.decision_count >= 1);
    assert!(metric.loop_count >= 1);
    assert_eq!(metric.parameter_count, Some(2));
}
```

Add `watcher_replaces_all_extractor_enrichment_domains` that indexes the same
fixture, rewrites it without the request/branch/loop/doc comment, invokes the
watcher handler, and asserts the old fact/regions/metric are absent or replaced.

**Step 2: Run the tests to verify they fail**

Run: `cargo nextest run --lib extract_scan_persists_v2_16_enrichment_domains`

Expected: FAIL with empty enrichment tables.

Run: `cargo nextest run --lib watcher_replaces_all_extractor_enrichment_domains`

Expected: FAIL with empty or stale enrichment tables.

**Step 3: Add the shared normalization seam**

Create this complete data contract:

```rust
use julie_extractors::base::{
    ComplexityMetric, SourceRegion, StructuralFact, TypeInfo,
};
use julie_extractors::{
    ExtractionResults, Identifier, Literal, PendingRelationship, Relationship,
    StructuredPendingRelationship, Symbol,
};

#[derive(Debug)]
pub struct NormalizedExtractionData {
    pub symbols: Vec<Symbol>,
    pub relationships: Vec<Relationship>,
    pub pending_relationships: Vec<PendingRelationship>,
    pub structured_pending_relationships: Vec<StructuredPendingRelationship>,
    pub identifiers: Vec<Identifier>,
    pub types: Vec<TypeInfo>,
    pub type_argument_rows:
        Vec<julie_core::database::bulk::type_arguments::TypeArgumentRow>,
    pub literals: Vec<Literal>,
    pub source_regions: Vec<SourceRegion>,
    pub structural_facts: Vec<StructuralFact>,
    pub complexity_metrics: Vec<ComplexityMetric>,
    pub parse_diagnostics: Vec<julie_extractors::base::ParseDiagnostic>,
}

pub fn normalize_extraction_results(
    mut results: ExtractionResults,
    configs: &julie_index::search::LanguageConfigs,
) -> NormalizedExtractionData {
    if !results.literals.is_empty() {
        let carriers = configs.build_literal_carrier_configs();
        julie_index::analysis::literals::classify_literals_by_carrier(
            &mut results.literals,
            &carriers,
        );
    }
    if !results.symbols.is_empty() {
        let roles = configs.build_test_role_configs();
        julie_index::analysis::test_roles::classify_symbols_by_role(
            &mut results.symbols,
            &roles,
        );
    }

    NormalizedExtractionData {
        symbols: results.symbols,
        relationships: results.relationships,
        pending_relationships: results.pending_relationships,
        structured_pending_relationships: results.structured_pending_relationships,
        identifiers: results.identifiers,
        types: results.types.into_values().collect(),
        type_argument_rows:
            julie_core::database::bulk::type_arguments::flatten_type_argument_usages(
                &results.type_argument_usages,
            ),
        literals: results.literals,
        source_regions: results.source_regions,
        structural_facts: results.structural_facts,
        complexity_metrics: results.complexity_metrics,
        parse_diagnostics: results.parse_diagnostics,
    }
}
```

Replace `ParserFileProcessResult`'s tuple with a named struct containing
`NormalizedExtractionData` and `FileInfo`. Load `LanguageConfigs` once per batch
and pass an `Arc` into parser work. Remove the post-batch duplicate
classification block.

**Step 4: Carry the domains through `ExtractedBatch`**

Add:

```rust
pub all_source_regions: Vec<SourceRegion>,
pub all_structural_facts: Vec<StructuralFact>,
pub all_complexity_metrics: Vec<ComplexityMetric>,
```

Initialize all three to empty, extend them from every parsed result, and return
them from `canonical_write_set`:

```rust
CanonicalWriteSet {
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
```

**Step 5: Make the watcher use the same normalized data**

Move watcher-only write-set assembly into
`watcher/extraction_write.rs`. Its public crate-local contract is:

```rust
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
```

`handle_file_created_or_modified_static` must call
`normalize_extraction_results` and delete its separate literal/test-role/type
argument transformation. Keep `handlers.rs` from growing; move code rather than
adding another block to the 438-line function.

The external extract scan/update paths need no separate storage branch: they
already consume `ExtractedBatch` and the shared atomic persistence functions.

**Step 6: Run checks and exact tests**

Run: `cargo check`

Expected: PASS.

Run: `cargo nextest run --lib extract_scan_persists_v2_16_enrichment_domains`

Expected: PASS.

Run: `cargo nextest run --lib watcher_replaces_all_extractor_enrichment_domains`

Expected: PASS.

Lead run: `cargo xtask test changed`

Expected: PASS; record selected buckets and duration.

**Step 7: Apply commit mode**

- `serial-worker-commit`: commit only the owned files with
  `feat(indexing): consume extractor enrichment domains` and record the SHA.

**Acceptance criteria:**
- [ ] The full indexer, watcher, and external extract CLI use one normalization function.
- [ ] All three domains persist from a real v2.16 Rust extraction.
- [ ] Watcher replacement removes stale enrichment rows atomically.
- [ ] Existing literal, type-argument, test-role, and parse-diagnostic behavior remains in the shared seam.
- [ ] Exact tests, `cargo check`, and lead `changed` gate pass.
- [ ] The owned change is committed and its SHA is recorded.

### Task 4: Add the generic `patterns` MCP and CLI tool

**Files:**
- Create: `crates/julie-tools/src/patterns/mod.rs`
- Create: `crates/julie-tools/src/patterns/formatting.rs`
- Create: `src/handler/tools/patterns.rs`
- Create: `src/tests/tools/patterns.rs`
- Modify: `crates/julie-tools/src/lib.rs:17-29`
- Modify: `src/tools/mod.rs:20-36`
- Modify: `src/handler/tools/mod.rs`
- Modify: `src/handler.rs:2541-2554`
- Modify: `src/handler/tool_targets.rs`
- Modify: `src/cli_tools/subcommands.rs`
- Modify: `src/cli_tools/commands.rs`
- Modify: `src/cli_tools/generic.rs:34-61`
- Modify: `src/cli_tools/mod.rs`
- Modify: `src/cli.rs:30-59`
- Modify: `src/main.rs`
- Modify: `src/tests/tools/mod.rs`
- Modify: `src/tests/cli_tools_tests.rs`
- Modify: `src/tests/core/handler_telemetry.rs`
- Modify: `xtask/tests/docs_contract_tests.rs:168-217`

**Interfaces:**
- Consumes: Task 2's `observed_structural_patterns` and `search_structural_facts`.
- Produces: public MCP tool and standalone CLI command `patterns` with the exact parameters below.

**Contract inputs:** Proven contract shape from Miller's current `patterns` tool; observed v2.16 registry has 194 pattern IDs; `where` is semicolon-separated `key=value` AND filters.

**File ownership:** Structural-fact tool module, handler/router, CLI command, tool metadata/docs-contract tests

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**Step 1: Write the failing tool contract test**

```rust
#[tokio::test]
async fn patterns_lists_searches_summarizes_and_filters_metadata() {
    let handler = indexed_handler_with_structural_facts(vec![
        structural_fact(
            "fact-1",
            "http.client_request.v1",
            "request",
            "src/client.rs",
            "rust",
            serde_json::json!({"client": "reqwest", "method": "GET"}),
        ),
        structural_fact(
            "fact-2",
            "symfony.route.v1",
            "route",
            "src/Controller.php",
            "php",
            serde_json::json!({"method": "POST"}),
        ),
    ])
    .await;

    let listed = PatternsTool {
        operation: PatternsOperation::List,
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&handler)
    .await
    .unwrap();
    assert_tool_text_contains(&listed, "\"http.client_request.v1\"");
    assert_tool_text_contains(&listed, "\"symfony.route.v1\"");

    let searched = PatternsTool {
        operation: PatternsOperation::Search,
        query: Some("client_request".into()),
        where_filter: Some("client=reqwest;method=GET".into()),
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&handler)
    .await
    .unwrap();
    assert_tool_text_contains(&searched, "\"fact-1\"");
    assert_tool_text_excludes(&searched, "\"fact-2\"");

    let summary = PatternsTool {
        operation: PatternsOperation::Summary,
        group_by: PatternsGroupBy::Directory,
        format: PatternsFormat::Json,
        ..Default::default()
    }
    .call_tool(&handler)
    .await
    .unwrap();
    assert_tool_text_contains(&summary, "\"src\"");
}
```

Add exact invalid-parameter assertions for search without
`pattern_id`/`query`, malformed `where`, and unknown enum values.

**Step 2: Run the test to verify it fails**

Run: `cargo nextest run --lib patterns_lists_searches_summarizes_and_filters_metadata`

Expected: FAIL because `PatternsTool` does not exist.

**Step 3: Implement the public parameter contract**

```rust
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternsOperation {
    #[default]
    List,
    Summary,
    Search,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternsGroupBy {
    #[default]
    LanguagePatternCapture,
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PatternsFormat {
    #[default]
    Compact,
    Json,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct PatternsTool {
    #[serde(default)]
    pub operation: PatternsOperation,
    #[serde(default)]
    pub pattern_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default, rename = "where")]
    pub where_filter: Option<String>,
    #[serde(default)]
    pub facet: Option<String>,
    #[serde(default)]
    pub group_by: PatternsGroupBy,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub format: PatternsFormat,
}
```

Validation rules:

```rust
impl PatternsTool {
    fn metadata_filters(&self) -> Result<Vec<(String, String)>> {
        self.where_filter
            .as_deref()
            .unwrap_or_default()
            .split(';')
            .filter(|part| !part.trim().is_empty())
            .map(|part| {
                let (key, value) = part
                    .split_once('=')
                    .ok_or_else(|| anyhow!("where filters must use key=value"))?;
                let key = key.trim();
                let value = value.trim();
                if key.is_empty() || value.is_empty() {
                    return Err(anyhow!("where filters must use non-empty key=value"));
                }
                Ok((key.to_string(), value.to_string()))
            })
            .collect()
    }

    fn effective_limit(&self) -> usize {
        self.limit.clamp(1, 500) as usize
    }
}
```

Search maps a free-text `query` to every observed `pattern_id` containing that
substring case-insensitively. Summary grouping and optional `facet` are computed
from the bounded typed rows returned by `julie-core`. Compact output includes
`file:line`, pattern ID, capture, and selected metadata; JSON serializes stable
records and summary groups.

**Step 4: Wire MCP, CLI, metadata, and docs contracts**

Register the handler with:

```rust
#[tool_router(router = tool_router_patterns, vis = "pub(crate)")]
impl JulieServerHandler {
    #[tool(
        name = "patterns",
        description = "Query generic code-shape facts extracted across all supported languages"
    )]
    async fn patterns(
        &self,
        Parameters(params): Parameters<PatternsTool>,
    ) -> Result<CallToolResult, McpError> {
        params
            .call_tool(self)
            .await
            .map_err(|error| classify_tool_failure("patterns", &error))
    }
}
```

Compose `Self::tool_router_patterns()` in `tool_router`, add `PatternsArgs` and
`impl CliToolCommand`, dispatch `"patterns"` in the generic CLI, and include
`patterns` in `public_tool_names()`. Named CLI flags mirror every field:
`--operation`, `--pattern-id`, `--query`, `--path`, `--language`, repeated
`--where`, `--facet`, `--group-by`, `--limit`, `--workspace`, and `--format`.

**Step 5: Run checks and exact tests**

Run: `cargo check`

Expected: PASS.

Run: `cargo nextest run --lib patterns_lists_searches_summarizes_and_filters_metadata`

Expected: PASS.

Run: `cargo nextest run --lib patterns_rejects_invalid_parameters`

Expected: PASS.

**Step 6: Apply commit mode**

- `parallel-lead-commit`: do not commit. Hand the owned diff and exact-test
  output to the lead for inline review and batch commit.

**Acceptance criteria:**
- [ ] MCP and standalone CLI expose the same `patterns` contract.
- [ ] List, summary, exact search, substring search, path/language filters, metadata AND filters, facet, limits, compact, and JSON work.
- [ ] Unknown or malformed inputs return invalid-parameter errors.
- [ ] Workspace routing never crosses the selected workspace.
- [ ] Exact tests and `cargo check` pass.
- [ ] The verified diff is handed to the lead without a worker commit.

### Task 5: Add source-region filtering to `fast_search`

**Files:**
- Create: `crates/julie-tools/src/search/regions.rs`
- Create: `src/tests/tools/search/source_regions.rs`
- Modify: `crates/julie-tools/src/search/mod.rs:58-95,240-651`
- Modify: `crates/julie-tools/src/search/line_mode.rs:26-80,232-293,369-673,694-759`
- Modify: `crates/julie-tools/src/search/trace.rs:252-265`
- Modify: `crates/julie-tools/src/search/hint_formatter.rs`
- Modify: `src/handler/tools/fast_search.rs`
- Modify: `src/handler/search_telemetry.rs`
- Modify: `src/cli_tools/subcommands.rs`
- Modify: `src/cli_tools/commands.rs:119-166`
- Modify: `src/cli_tools/generic.rs`
- Modify: `src/tests/tools/search/mod.rs:14-29`
- Modify: `src/tests/core/handler_telemetry.rs`

**Interfaces:**
- Consumes: Task 2's `get_source_regions_for_file`.
- Produces: wire-level `FastSearchParams { search, regions }`, CLI `--regions`, parsed `SourceRegionFilter`, and region-scoped line results.

**Contract inputs:** Existing `FastSearchTool`, `line_mode_matches`, `LineModeStageCounts`, `ZeroHitReason`, and `SearchHit::from_line_match`.

**File ownership:** Fast-search wire params, source-region line-search path, region tests and telemetry

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**Step 1: Write the failing region-search test**

```rust
#[tokio::test]
async fn fast_search_regions_returns_only_matching_source_region_lines() {
    let handler = indexed_handler_with_file(
        "src/lib.rs",
        "rust",
        "// region needle\nlet region_needle = 1;\n",
    )
    .await;
    seed_source_regions(
        &handler,
        vec![SourceRegion {
            id: "comment-1".into(),
            file_path: "src/lib.rs".into(),
            language: "rust".into(),
            kind: SourceRegionKind::Comment,
            containing_symbol_id: None,
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 16,
            start_byte: 0,
            end_byte: 16,
            metadata: None,
        }],
    )
    .await;

    let result = FastSearchParams {
        search: FastSearchTool {
            query: "region needle".into(),
            return_format: "full".into(),
            ..Default::default()
        },
        regions: Some("comment".into()),
    }
    .call_tool(&handler)
    .await
    .unwrap();

    let text = tool_text(&result);
    assert!(text.contains("src/lib.rs:1"));
    assert!(!text.contains("src/lib.rs:2"));
}
```

Add `fast_search_regions_rejects_unknown_region_and_symbol_backends` and
`fast_search_regions_respects_target_workspace`.

**Step 2: Run the test to verify it fails**

Run: `cargo nextest run --lib fast_search_regions_returns_only_matching_source_region_lines`

Expected: FAIL because the wire parameter and region path do not exist.

**Step 3: Add a wire wrapper without breaking internal struct literals**

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FastSearchParams {
    #[serde(flatten)]
    pub search: FastSearchTool,
    #[serde(default)]
    pub regions: Option<String>,
}

impl From<FastSearchTool> for FastSearchParams {
    fn from(search: FastSearchTool) -> Self {
        Self {
            search,
            regions: None,
        }
    }
}
```

Use `FastSearchParams` at the MCP and generic JSON boundaries. Keep
`FastSearchTool` as the existing execution config so internal tests and callers
do not need mechanical field edits.

Create the exact parser:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRegionFilter(pub Vec<SourceRegionKind>);

impl SourceRegionFilter {
    pub fn parse(value: &str) -> Result<Self> {
        let mut kinds = Vec::new();
        for raw in value.split(',') {
            let kind = match raw.trim().to_ascii_lowercase().as_str() {
                "comment" => SourceRegionKind::Comment,
                "doc_comment" | "docstring" => SourceRegionKind::DocComment,
                "string_literal" => SourceRegionKind::StringLiteral,
                "embedded" => SourceRegionKind::Embedded,
                "" => continue,
                unknown => return Err(anyhow!("unknown source region: {unknown}")),
            };
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        if kinds.is_empty() {
            return Err(anyhow!("regions must contain at least one source region"));
        }
        Ok(Self(kinds))
    }
}
```

**Step 4: Add a correct region-aware line path**

Add an optional `SourceRegionFilter` parameter to private line-fetch functions,
not to the existing public `line_mode_matches` signature. Provide a new
`line_mode_matches_in_regions` wrapper for `FastSearchParams`.

For each candidate file:

```rust
let allowed_regions = match region_filter {
    Some(filter) => db.get_source_regions_for_file(
        &file_result.file_path,
        &filter.0,
    )?,
    None => Vec::new(),
};

collect_line_matches(
    &mut matches,
    &content,
    &file_result.file_path,
    match_strategy,
    base_limit,
    region_filter.map(|_| allowed_regions.as_slice()),
);
```

The line predicate is:

```rust
fn line_is_in_allowed_region(line_number: usize, regions: &[SourceRegion]) -> bool {
    let line_number = line_number as u32;
    regions.iter().any(|region| {
        region.start_line <= line_number && line_number <= region.end_line
    })
}
```

Apply it before pushing a `LineMatch`, including the density-ranked file-level
branch. Add `region_dropped` to `LineModeStageCounts` and
`ZeroHitReason::RegionFiltered`; place the stage after line matching so telemetry
distinguishes “query absent” from “query present outside requested regions.”

`FastSearchParams::call_tool` validates that an explicit semantic or hybrid
backend is rejected, invokes the region line path, converts matches through
`SearchHit::from_line_match`, and uses existing full/locations formatters.
Ordinary requests call `FastSearchTool` unchanged.

**Step 5: Wire CLI and telemetry**

Add `--regions <comma-list>` to `SearchArgs`. Serialize it as the top-level
`regions` key. Add `regions` and `region_filtered` to search telemetry metadata;
do not change existing intent or zero-hit fields.

**Step 6: Run checks and exact tests**

Run: `cargo check`

Expected: PASS.

Run: `cargo nextest run --lib fast_search_regions_returns_only_matching_source_region_lines`

Expected: PASS.

Run: `cargo nextest run --lib fast_search_regions_rejects_unknown_region_and_symbol_backends`

Expected: PASS.

Run: `cargo nextest run --lib fast_search_regions_respects_target_workspace`

Expected: PASS.

**Step 7: Apply commit mode**

- `parallel-lead-commit`: do not commit. Hand the owned diff and exact-test
  output to the lead for inline review and batch commit.

**Acceptance criteria:**
- [ ] Region requests search only stored source spans and return exact matching lines.
- [ ] All four kinds and the `docstring` alias parse exactly.
- [ ] Ordinary fast search output, fallback, ranking, and struct literals remain unchanged.
- [ ] Semantic/hybrid plus regions is rejected clearly.
- [ ] Target-workspace isolation and telemetry are covered.
- [ ] Exact tests and `cargo check` pass.
- [ ] The verified diff is handed to the lead without a worker commit.

### Task 6: Show complexity metrics in `deep_dive`

**Files:**
- Create: `src/tests/tools/deep_dive_complexity.rs`
- Modify: `crates/julie-tools/src/deep_dive/data.rs:19-42,197-327`
- Modify: `crates/julie-tools/src/deep_dive/formatting.rs:15-105`
- Modify: `crates/julie-tools/src/tests/deep_dive_regression_tests.rs:111-125`
- Modify: `crates/julie-tools/src/tests/deep_dive_tests/formatting_tests.rs:50-64`
- Modify: `crates/julie-tools/src/tests/deep_dive_tests/formatting_tests/refs_quality_budget.rs`
- Modify: `src/tests/tools/mod.rs`

**Interfaces:**
- Consumes: Task 2's `get_complexity_metric_for_symbol`.
- Produces: `SymbolContext.complexity: Option<ComplexityMetric>` and one stable compact header line.

**Contract inputs:** Existing `build_symbol_context`, `format_symbol_context`, and deep-dive token-budget tests.

**File ownership:** Deep-dive data/formatting and complexity tests

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**Step 1: Write the failing deep-dive test**

```rust
#[test]
fn deep_dive_prints_stored_complexity_for_selected_symbol() {
    let fixture = deep_dive_fixture();
    let symbol = fixture.store_function("process", "src/lib.rs", 10, 17);
    fixture
        .store_complexity(ComplexityMetric {
            id: "metric-1".into(),
            file_path: "src/lib.rs".into(),
            language: "rust".into(),
            scope: "function".into(),
            symbol_id: Some(symbol.id.clone()),
            algorithm_id: "structural-v1".into(),
            covered_lines: 8,
            covered_bytes: 120,
            decision_count: 4,
            loop_count: 2,
            max_nesting_depth: 3,
            parameter_count: Some(2),
            start_line: 10,
            start_column: 0,
            end_line: 17,
            end_column: 1,
            start_byte: 100,
            end_byte: 220,
            metadata: None,
        })
        .unwrap();

    let output = deep_dive_query(
        fixture.db(),
        "process",
        Some("src/lib.rs"),
        "overview",
        20,
        20,
    )
    .unwrap();

    assert!(output.contains(
        "complexity: decisions=4 loops=2 nesting=3 params=2 lines=8"
    ));
}
```

Add `deep_dive_omits_complexity_line_when_metric_is_absent`.

**Step 2: Run the test to verify it fails**

Run: `cargo nextest run --lib deep_dive_prints_stored_complexity_for_selected_symbol`

Expected: FAIL because deep dive does not query or format complexity.

**Step 3: Enrich `SymbolContext`**

Add:

```rust
pub complexity: Option<julie_extractors::base::ComplexityMetric>,
```

In `build_symbol_context`, after primary symbol enrichment:

```rust
let complexity = db.get_complexity_metric_for_symbol(&symbol.id)?;
```

Include `complexity` in the returned struct. Update the two shared test
constructors with `complexity: None`; compiler errors identify any remaining
literal.

**Step 4: Format one stable header line**

After `format_header`, call:

```rust
fn format_complexity(out: &mut String, ctx: &SymbolContext) {
    let Some(metric) = &ctx.complexity else {
        return;
    };
    let params = metric
        .parameter_count
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    writeln!(
        out,
        "complexity: decisions={} loops={} nesting={} params={} lines={}",
        metric.decision_count,
        metric.loop_count,
        metric.max_nesting_depth,
        params,
        metric.covered_lines,
    )
    .unwrap();
}
```

Do not print algorithm IDs or metadata in the default output. Preserve the
existing overview/context/full token budgets; increase test ceilings only by
the exact maximum length of this one line if necessary.

**Step 5: Run checks and exact tests**

Run: `cargo check`

Expected: PASS.

Run: `cargo nextest run --lib deep_dive_prints_stored_complexity_for_selected_symbol`

Expected: PASS.

Run: `cargo nextest run --lib deep_dive_omits_complexity_line_when_metric_is_absent`

Expected: PASS.

Lead run after Tasks 4–6 are integrated: `cargo xtask test changed`

Expected: PASS; record selected buckets and duration.

**Step 6: Apply commit mode**

- `parallel-lead-commit`: do not commit. Hand the owned diff and exact-test
  output to the lead for inline review and batch commit
  `feat(tools): expose extractor enrichments`.

**Acceptance criteria:**
- [ ] Deep dive shows exact stored counts for a selected symbol.
- [ ] Symbols without metrics have no empty or placeholder line.
- [ ] All depths use the same compact line.
- [ ] Existing token budgets remain green.
- [ ] Exact tests, `cargo check`, and lead `changed` gate pass.
- [ ] The verified diff is handed to the lead without a worker commit.

### Task 7: Synchronize shipped docs and run final gates

**Files:**
- Create: `docs/plans/2026-07-18-julie-extractors-v2-16-consumer-upgrade-verification.md`
- Modify: `README.md:317-354,596-612`
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`
- Modify: `docs/DEPENDENCIES.md:8-16`
- Modify: `docs/EXTRACTION_CONTRACT.md`
- Modify: `docs/EXTERNAL_EXTRACT.md:172-230`
- Modify: `docs/SEARCH_FLOW.md`
- Modify: `docs/ARCHITECTURE.md`
- Modify: `docs/TREE_SITTER_UPGRADES.md:16-45`
- Modify: `docs/site/index.html`
- Modify: `TODO.md:8-14`
- Modify: `xtask/tests/docs_contract_tests.rs`

**Interfaces:**
- Consumes: final Tasks 1–6 public names, parameters, storage schema, and verification results.
- Produces: current shipped documentation and a commit-SHA-bound verification ledger.

**Contract inputs:** `docs/plans/verification-ledger-template.md`; current tool names are derived by `public_tool_names()` rather than manually counted.

**File ownership:** Current product/dependency/extraction docs, TODO closure, verification ledger

**Serialization required:** Yes

**Dependency reason:** Requires all code slices and their final public contracts.

**Step 1: Write the failing docs contract test**

```rust
#[test]
fn docs_contract_tests_extractor_enrichment_surfaces_are_documented() {
    let readme = read_repo_file("README.md");
    let instructions = read_repo_file("JULIE_AGENT_INSTRUCTIONS.md");
    let dependencies = read_repo_file("docs/DEPENDENCIES.md");
    let extraction = read_repo_file("docs/EXTRACTION_CONTRACT.md");

    for required in [
        "patterns",
        "regions",
        "source_regions",
        "structural_facts",
        "complexity_metrics",
    ] {
        assert!(readme.contains(required), "README missing {required}");
        assert!(
            instructions.contains(required),
            "agent instructions missing {required}"
        );
    }
    assert!(dependencies.contains("julie-extractors v2.16.0"));
    assert!(extraction.contains("Schema version 29"));
}
```

**Step 2: Run the test to verify it fails**

Run: `cargo nextest run -p xtask docs_contract_tests_extractor_enrichment_surfaces_are_documented`

Expected: FAIL on v2.14 and missing tool/data documentation.

**Step 3: Update current documentation**

Document:

- `patterns` list/summary/search syntax and compact examples.
- `fast_search regions="comment,doc_comment"` and the four accepted kinds.
- `deep_dive` complexity output semantics.
- schema version 29 and all three tables in external-extract docs.
- the one shared normalization/persistence path in architecture/search-flow docs.
- dependency v2.16.0 and both official upstream release links.
- the v2.14 → v2.16 upgrade entry in `TREE_SITTER_UPGRADES.md`.
- tool card/site copy for `patterns` without hard-coded stale tool counts.

Replace the stale TODO entries for a raw tree-sitter pattern query tool and
not-yet-calculated AST complexity with accurate status:

```markdown
- [x] **Extractor structural facts query** -- `patterns` queries the typed, upstream-maintained structural registry without accepting raw grammar-specific tree-sitter expressions.
- [x] **AST-based complexity metrics** -- Persist upstream `complexity_metrics` and show per-symbol counts in `deep_dive`; hotspot ranking remains a separate product feature.
```

Do not rewrite historical release notes that correctly describe v7.15.4's
v2.14.0 pin.

**Step 4: Create and populate the verification ledger**

Copy the template and record:

```markdown
| Invariant | Command | Scope label | Commit SHA | Result | Timestamp | Reused |
|---|---|---|---|---|---|---|
| Clean v2.14 baseline | `cargo xtask test nano` | baseline-nano | `d3dcda40c16a8f93b2dc4a9de9bd20ac62e72295` | PASS: 2 buckets, 106.7s | 2026-07-18 | No |
```

Append each exact task test, changed gate, specialist bucket, system, dogfood,
dev, fmt, clippy, and standalone dogfood command with the actual final SHA,
result, and timestamp. Never record a planned command as PASS.

**Step 5: Run docs, format, lint, and branch gates sequentially**

Run: `cargo nextest run -p xtask docs_contract_tests_extractor_enrichment_surfaces_are_documented`

Expected: PASS.

Run: `cargo fmt --check`

Expected: PASS.

Run: `cargo clippy`

Expected: PASS with no new warnings.

Run: `cargo xtask test bucket extractor-dep-integration`

Expected: PASS.

Run: `cargo xtask test system`

Expected: PASS.

Run: `cargo xtask test dogfood`

Expected: PASS.

Run: `cargo xtask test dev`

Expected: PASS.

Do not run these commands concurrently.

**Step 6: Dogfood the shipped CLI surfaces**

Build once:

```bash
cargo build
```

Run:

```bash
./target/debug/julie-server patterns --workspace . --standalone --json
./target/debug/julie-server patterns --operation search --query http --workspace . --standalone --json
./target/debug/julie-server search "TODO" --regions comment,doc_comment --workspace . --standalone --json
./target/debug/julie-server tool deep_dive --params '{"symbol":"process_file_with_parser_using","depth":"overview"}' --workspace . --standalone --json
```

Expected:

- list returns observed pattern IDs;
- search returns bounded structural facts or an honest empty result;
- region search returns only comment/doc-comment lines;
- deep dive includes a complexity line when the selected symbol has a stored metric.

Record exact output counts as report-only evidence and every nonzero exit as a
hard failure.

**Step 7: Perform final worktree-state and request review**

Run:

```bash
git rev-parse --show-toplevel
git branch --show-current
git rev-parse HEAD
git status --short --branch
git worktree list
```

Inspect every related worktree status before review. The feature worktree must
contain no untracked or uncommitted task files after the final commit.

**Step 8: Apply commit mode**

- `serial-worker-commit`: commit only the owned files with
  `docs(extractors): document v2.16 consumer surfaces` and record the SHA.
- Do not push or release.

**Acceptance criteria:**
- [ ] Current docs name v2.16.0 and all three consumed domains.
- [ ] Agent instructions, CLI examples, README, architecture, external extract, site, and TODO status match code.
- [ ] Verification ledger contains only observed evidence at exact SHAs.
- [ ] Docs contract, fmt, clippy, extractor integration, system, dogfood, and dev gates pass.
- [ ] Standalone `patterns`, region search, and deep-dive dogfood succeed.
- [ ] Final related worktree state is clean and intentional.
- [ ] The owned change is committed and its SHA is recorded; nothing is pushed or released.
