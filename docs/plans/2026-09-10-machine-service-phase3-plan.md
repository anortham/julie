# Machine Service Phase 3 Implementation Plan: Facts, Graph, Snapshots, Read Tools

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** Replace the path-keyed `symbols.db` (25 tables, 10,220 lines in `crates/julie-core/src/database/`) with the blob-keyed `facts.sqlite`, an in-memory graph, and immutable snapshots, and move every read tool onto them, deleting the old storage, its writers, its projection bookkeeping, and its readers in the same phase.

**Architecture:** A new `julie-facts` crate owns the `facts.sqlite` schema and the one writer per checkout: a pure function from `(path, bytes)` to immutable rows keyed by blob hash, plus a `paths` table that says which blob each path holds. `julie-index` gains the Tantivy projection built from facts, the in-memory graph (identifier resolution as a pure function, adjacency arrays, derived web edges, reference scores), the brute-force vector scan, and the `Snapshot` type (an `Arc` to the graph, a Tantivy searcher, a vector set, and a short-query facts reader). Tools see only `ToolContext::snapshot(target)`. The writer publishes a new snapshot after each commit by swapping the `Arc`. Old and new stores coexist only between Task 3 and Task 13 of this plan; Task 13 deletes `SymbolDatabase`, the old pipeline persistence, the watcher's old handlers, projection states, canonical revisions, repairs, and embedding generations. The branch does not merge before Task 13.

**Tech Stack:** Rust, `rusqlite` (existing), `tantivy` (existing), `blake3` (existing), `julie-extractors` v2.42.0 (existing pin), `tokio`, `notify` (existing watcher), `cargo nextest`, `cargo xtask test`, `tokei`.

**Architecture Quality:** Design `docs/plans/2026-09-09-machine-service-design.md`, sections 4, 5.3, 5.4, 6, 7, 8, 9, 10, 12, 13, 14 item 3, 15. Approved shape: `julie-facts` (schema, writer, reader, seed copy) and `julie-index` (projection, graph, vector scan, snapshot) are the only crates that know SQLite or Tantivy exist. `ToolContext` exposes `snapshot(target) -> Arc<Snapshot>`; no tool receives a database handle. `RuntimeFactory` still binds one handler per `(root, index_root)`; the handler's `JulieWorkspace` owns one `CheckoutStore` (facts + tantivy + current snapshot) instead of `db` + `search_index`. Risk: high. This deletes the storage layer of two products and rewrites eight tools' data access; the control is the per-tool porting order (each task deletes the reads it replaces and keeps that tool's existing tests green through an in-memory fixture), the section 12 budgets measured in Task 14, and the phase gate: if any budget fails, stop and redesign before phase 4.

## Sequencing decisions (need user approval with the plan)

1. **Vectors move from phase 4 into this phase.** Deleting `SymbolDatabase` deletes `symbol_vectors`, `embedding_config`, and `embedding_generations`. To avoid a semantics outage (the reason phase 2 kept the broker client), Task 10 adds the design's `vectors(blob_hash, symbol_ordinal, encoder_id, vector)` and `encoder` rows to `facts.sqlite` and the brute-force cosine scan to `julie-index`, fed by the existing native provider through the facts writer. Phase 4 keeps the `serve`-mode sidecar child, the deletion of the broker client, and the model scorecard finding.
2. **Storage location stays `$JULIE_HOME/indexes/<id>/`** (`facts.sqlite` and `tantivy/` replace `db/symbols.db` and `tantivy/`). Design 6.1 puts the two roots under `<checkout>/.julie/`; moving them there while the standalone CLI still opens stores in its own process would put two writers on one file. The relocation goes with the CLI-over-`/api` move in phase 6. The standalone CLI keeps `<root>/.julie/indexes/<id>/` with the same two roots.
3. **Tool names and parameters stay Julie's** (`fast_search`, `get_symbols`, `deep_dive`, `fast_refs`, `call_path`, `blast_radius`, `get_context`, `patterns`); phase 6 renames them behind the Miller contract. One parameter is deleted: `blast_radius` seeding from a canonical revision range (`crates/julie-tools/src/impact/seed.rs:57`, reads the `revision_file_changes` journal). Phase 6's `impact` seeds from the git diff.
4. **`julie-server extract`** (`src/external_extract/`, 1,622 lines) writes the `facts.sqlite` schema through the same writer. The `symbols.db`-shaped export is gone; design 6.1 names the facts row schema as the producer contract.
5. **Web edges are derived, not stored.** `rebuild_web_edges_for_workspace` (`crates/julie-pipeline/src/indexing_core/web_edges.rs`, 460 lines) becomes a pure function in the graph load; the `web_edges` table is not carried over.
6. **Crate renames wait for phase 6.** `julie-facts` is new because its contents are new. `julie-tools` -> `julie-engine` and `julie-runtime` -> `julie-service` are mechanical renames with no deletion value; they happen with the contract change.
7. **Reference scores live in the graph.** `compute_reference_scores` (`crates/julie-core/src/database/relationships.rs:236`) wrote `symbols.reference_score`; the graph computes the same score in memory at load and per changed file.

## Global Constraints

- Design section 4 is normative. A worker who needs any of these words in new code stops and reports: lock, lease, fence, generation, epoch, cursor, claim, pin, coordinator, broker, journal, repair, continuation, handoff. `scripts/complexity-words.sh main` is run by the lead after each batch.
- Durable roots per checkout after this phase: `$JULIE_HOME/indexes/<id>/facts.sqlite` (plus `-wal` and `-shm`) and `$JULIE_HOME/indexes/<id>/tantivy/`. Per machine: `registry.db` and the runtime `service.json`. `src/tests/service/durable_roots.rs` is updated in Task 3 to the new names and must fail on `db/symbols.db`.
- `facts.sqlite` rows are immutable once written. The only `UPDATE` is on `paths`; the only `DELETE` is `workspace rebuild`, which deletes `indexes/<id>/`. No vacuum, no garbage collector, no retention.
- `facts.sqlite` is never migrated. A `meta` row records `schema_version` and `engine_version` (`SEMANTIC_INDEX_ENGINE_VERSION`); a mismatch on open deletes `indexes/<id>/` and reindexes. Tantivy records its schema `compatibility_signature` and the same engine version in `tantivy/julie.meta.json`; a mismatch or a missing directory rebuilds it from facts. No `projection_states`, `canonical_revisions`, `revision_file_changes`, `indexing_repairs`, or `index_engine_state` table exists after Task 13.
- No read transaction is held across a tool call. A `Snapshot` holds `Arc<Graph>`, a `tantivy::Searcher`, `Arc<VectorSet>`, and a `FactsReader` that opens short read-only queries. `SymbolDatabase::into_read_snapshot` and `with_read_transaction` are deleted with the crate.
- The in-memory graph is bounded: `JULIE_GRAPH_MAX_SYMBOLS` (default 2,000,000) is the full-load bound; above it the checkout loads per file on demand. The status page reports graph symbol count, load time, and resident bytes per checkout so the bound is tuned from evidence (design 6.3).
- `mutation_gate` stays the only serialization primitive. The facts writer is called under `MutationGuard<'_>` exactly where `incremental_update_atomic_with_metadata` is called today.
- No new crate dependency in any `Cargo.toml` except the new workspace member `crates/julie-facts`. `sqlite-vec` (`symbol_vectors` vec0) is removed in Task 10.
- Tests use an in-memory facts store (`FactsStore::in_memory()`), a RAM Tantivy index (`Index::create_in_ram`), and a fixture tree under `fixtures/`. No test opens a real repository; no focused test takes more than two seconds. `crates/julie-test-support` gains `SnapshotFixture::from_tree(path)`; `FakeToolContext` serves it.
- Every task ends with `cargo build` green, its worker scope green, and the tests that encoded the deleted behavior deleted (not `#[ignore]`d). Implementation files at most 500 lines; test files at most 1,000 lines.
- Exact strings that stay: the truncation line `Output truncated at {kept} results; narrow the query or pass a smaller limit.` (`crates/julie-tools/src/shared.rs:11`); `ManageWorkspaceOperation::OPERATIONS` = `index, list, open, remove, refresh, health, rebuild, status, recover_edit, recover-edit, dashboard`; exit codes in `src/request_engine/types.rs:299-319`; the error classifier `classify_tool_failure` (ADR-0002); the strict path resolver (ADR-0001); source-aware language detection (ADR-0005).
- `julie-extractors` stays at v2.42.0. `SEMANTIC_INDEX_ENGINE_VERSION` (`src/tools/workspace/indexing/engine_version.rs:24`) is rewritten once in Task 1 to `extractors=<EXTRACTION_CONTRACT_VERSION>+extractors-tag=v2.42.0+facts=1` and moves to `crates/julie-facts/src/version.rs`; the `epoch=9` drift against `EXTRACTION_IDENTITY_EPOCH = 10` ends there.

## Verification Strategy

**Project source of truth:** `AGENTS.md` ("Quick Reference", "Canonical Test Tiers"); `xtask/test_tiers.toml`; `xtask/tests/support/manifest_contract_expected.rs`.

**Worker red/green scope:** the exact test module each task names, `cargo nextest run -p <crate> --lib <module>` or `cargo nextest run --lib <module>`.

**Worker ceiling:** `cargo build` plus the modules the task names. Workers never run `cargo xtask test` tiers.

**Worker gate invariant:** each task's acceptance list states the behavior its tests prove and the files its deletions remove.

**Lead affected-change scope:** `cargo xtask test dev` once per completed batch (every task touches unmapped or handler paths, so `changed` falls back anyway), plus `cargo xtask test bucket <tool bucket>` for the tool a batch ported.

**Branch gate:** `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test bucket service-process`, `cargo xtask test dogfood`, then `cargo xtask test full` once before handoff, then Task 14's measurements.

**Security scope:** `cargo audit` (security-deps); `rg -n '(AKIA|BEGIN (RSA|OPENSSH) PRIVATE|api[_-]?key\s*=\s*"[A-Za-z0-9])'` over `git diff main...HEAD` (security-secrets).

**Replay/metric evidence:** hard gates at phase exit (design 12): durable roots test; `fast` tier under 10 s warm (unit tests only); `full` tier under 120 s warm on Linux excluding `search-quality`, `tools-dogfood-repo-index`, and `extractor-dep-integration`; `service-process` under 20 s; complexity words zero in added code without a written exception; `tokei` net negative against `63cefbcf`; no test opens a real repository. Report-only: clean build time; resident bytes, graph load time, and facts size per checkout on the Julie and Miller repos; hybrid search latency with the brute-force scan.

**Escalation triggers:** any need for a table that is not in design 6.1 stops the task. Any `UPDATE` or `DELETE` on a fact row stops the task. Any need to keep a `SymbolDatabase` method "for now" past Task 13 stops the task. Any focused test that needs a real repository or more than two seconds stops the task.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-10-machine-service-phase3-ledger.md`, created from `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse only when scope label and HEAD SHA match exactly.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: `julie-facts` schema and writer | None - serial | Create `crates/julie-facts/` (`Cargo.toml`, `src/lib.rs`, `src/schema.rs`, `src/store.rs`, `src/writer.rs`, `src/rows.rs`, `src/reader.rs`, `src/version.rs`, `src/tests/`); modify `Cargo.toml` (workspace members), `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs` | Yes | Everything else consumes its types. |
| Task 2: Graph and resolution | None - serial | Create `crates/julie-index/src/graph/` (`mod.rs`, `load.rs`, `resolve.rs`, `edges.rs`, `web_edges.rs`, `scores.rs`), `crates/julie-index/src/tests/graph/`; modify `crates/julie-index/src/lib.rs`, `crates/julie-index/Cargo.toml` (dep on julie-facts) | Yes | Needs Task 1 row types. |
| Task 3: Snapshot, checkout store, writer wiring, projection from facts | None - serial | Create `crates/julie-index/src/snapshot.rs`, `crates/julie-index/src/checkout_store.rs`, `crates/julie-index/src/search/projection/from_facts.rs`, `crates/julie-index/src/vectors/` (empty `VectorSet` type only), `crates/julie-test-support/src/snapshot_fixture.rs`; modify `crates/julie-context/src/tool_context.rs`, `crates/julie-test-support/src/fake_tool_context.rs`, `crates/julie-runtime/src/workspace/mod.rs`, `crates/julie-runtime/src/watcher/handlers/created_modified.rs`, `crates/julie-runtime/src/watcher/handlers/delete_rename.rs`, `crates/julie-runtime/src/watcher/runtime/processing.rs`, `src/tools/workspace/indexing/pipeline.rs`, `src/tools/workspace/indexing/pipeline_persistence.rs`, `src/handler/tool_context_impl.rs`, `src/handler.rs`, `src/tests/service/durable_roots.rs` | Yes | Needs Tasks 1 and 2; every tool port needs `ToolContext::snapshot`. |
| Task 4: `fast_search` on the snapshot | Batch A | Modify `crates/julie-tools/src/search/**` (all files), `crates/julie-tools/src/tests/search_*`, `src/tests/tools/search/**`, `src/tests/tools/search_context_lines.rs`, `src/tests/tools/text_search_tantivy.rs` | No | None - safe parallel batch. |
| Task 5: `get_symbols` and `patterns` on the snapshot | Batch A | Modify `crates/julie-tools/src/symbols/**`, `crates/julie-tools/src/patterns/**`, `src/tests/tools/get_symbols*.rs`, `src/tests/tools/patterns.rs`; `xtask/test_tiers.toml` (`tools-get-symbols` gains `tests::tools::patterns`) | No | None - safe parallel batch. |
| Task 6: `fast_refs` and `call_path` on the graph | Batch A | Modify `crates/julie-tools/src/navigation/**`, `src/tests/tools/call_path_tests.rs`, `src/tests/tools/call_path_disambiguation_tests.rs`, `src/tests/tools/fast_refs_primary_rebind_tests.rs`, `src/tests/tools/target_workspace_fast_refs_tests/**`, `src/tests/tools/web_navigation.rs`; `xtask/test_tiers.toml` (`tools-call-path` gains `tests::tools::web_navigation`) | No | None - safe parallel batch. |
| Task 7: `deep_dive` on the graph | Batch B | Modify `crates/julie-tools/src/deep_dive/**`, `crates/julie-tools/src/tests/deep_dive_*`, `src/tests/tools/deep_dive_complexity.rs`, `src/tests/tools/deep_dive_primary_rebind_tests.rs` | No | None - safe parallel batch. |
| Task 8: `blast_radius` on the graph | Batch B | Modify `crates/julie-tools/src/impact/**`, `crates/julie-tools/src/tests/blast_radius_*`, `src/tests/tools/blast_radius*` (files and directories) | No | None - safe parallel batch. |
| Task 9: `get_context` on the snapshot | Batch B | Modify `crates/julie-tools/src/get_context/**`, `crates/julie-tools/src/tests/get_context_*`, `src/tests/tools/get_context_*.rs`; `crates/julie-index/src/search/hybrid.rs` | No | None - safe parallel batch. |
| Task 10: Vectors in facts, brute-force scan | None - serial | Modify `crates/julie-facts/src/schema.rs`, `crates/julie-facts/src/writer.rs`, `crates/julie-facts/src/reader.rs`, `crates/julie-index/src/vectors/**`, `crates/julie-index/src/snapshot.rs`, `crates/julie-index/src/search/similarity.rs`, `crates/julie-index/src/search/hybrid.rs`, `crates/julie-pipeline/src/embeddings/pipeline/**`, `src/tools/workspace/indexing/embeddings.rs`, `src/tools/workspace/indexing/pipeline_runner.rs`, `crates/julie-tools/src/search/execution/semantic.rs`, `crates/julie-tools/src/deep_dive/data/similarity.rs`, `crates/julie-tools/src/navigation/fast_refs_semantic.rs`, `src/health/**`; delete `crates/julie-core/src/database/{vectors,embedding_generation,embedding_generation_eligibility,embedding_generation_types,memory_vectors}.rs` and their tests | Yes | Touches three tools' semantic branches after Batches A and B land. |
| Task 11: Workspace commands, startup, seed on facts | None - serial | Modify `src/tools/workspace/commands/**`, `src/tools/workspace/indexing/**` (all files), `src/startup.rs`, `src/startup_repair_plan.rs`, `src/request_engine/runtime_factory.rs`, `src/tests/tools/workspace/**`, `src/tests/tools/workspace/seed.rs`, `src/tests/core/workspace_init/**` | Yes | Needs Task 10 (embedding scheduling moves with the writer). |
| Task 12: Edit tools, health, dashboard, metrics, `extract` | None - serial | Modify `crates/julie-tools/src/editing/**`, `crates/julie-tools/src/refactoring/**`, `src/handler/tools/{edit_file,rewrite_symbol,rename_symbol}.rs`, `src/workspace_runtime/**`, `src/health/**`, `src/dashboard/**`, `src/tools/metrics/**`, `src/external_extract/**`, `src/registry/**`, their tests | Yes | Needs Task 11 (store API final). |
| Task 13: Delete the old storage | None - serial | Delete `crates/julie-core/src/database/` (all), `crates/julie-core/src/connection_pool.rs`, `crates/julie-core/src/indexing_state.rs` repair parts, `crates/julie-core/src/tests/database*`, `crates/julie-pipeline/src/indexing_core/{persistence.rs,web_edges.rs,web_edges/}`, `crates/julie-index/src/search/projection.rs` DB path and `projection/apply.rs` DB loaders, `crates/julie-runtime/src/watcher/runtime/repairs.rs`, `src/tests/core/incremental_update_atomic*`, `src/tests/core/revision_changes.rs`, `src/tests/integration/projection_repair.rs`; modify every file that still imports them | Yes | Runs last so each earlier task keeps its own tests green. |
| Task 14: Gates, measurements, docs, ledger, verdict (lead) | None - serial | Create `docs/findings/2026-09-DD-machine-service-phase3-gate.md`, `docs/plans/2026-09-10-machine-service-phase3-ledger.md`; modify `src/service/status.rs`, `src/tools/workspace/commands/registry/status.rs`, `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs`, `xtask/src/manifest.rs`, `CLAUDE.md`, `AGENTS.md`, `docs/WORKSPACE_ARCHITECTURE.md`, `docs/SEARCH_FLOW.md`, `docs/ARCHITECTURE.md`, `docs/TESTING_GUIDE.md`, `docs/OPERATIONS.md`, `README.md`, `docs/plans/2026-09-09-machine-service-design.md` (section 14 only) | Yes | Lead task; needs the final tree. |

Commit mode: `serial-worker-commit` for Tasks 1, 2, 3, 10, 11, 12, 13. `parallel-lead-commit` for Batch A (Tasks 4, 5, 6) and Batch B (Tasks 7, 8, 9). Task 14 is lead work. Batch B dispatches after Batch A's lead commit because both batches share `crates/julie-test-support` fixtures produced in Task 3 and the lead runs `dev` between them.

---

### Task 1: `julie-facts` schema and writer

**Files:**
- Create: `crates/julie-facts/Cargo.toml` (deps: `rusqlite` with the same features as `crates/julie-core/Cargo.toml`, `blake3`, `serde`, `serde_json`, `anyhow`, `julie-extractors` v2.42.0), `crates/julie-facts/src/lib.rs`, `src/schema.rs` (DDL and `meta`), `src/store.rs` (`FactsStore::open(path)`, `FactsStore::in_memory()`, version check), `src/rows.rs` (row structs mirroring `julie_extractors` types plus `blob_hash` and `ordinal`), `src/writer.rs` (`FactsWriter`), `src/reader.rs` (`FactsReader`), `src/version.rs` (`SEMANTIC_INDEX_ENGINE_VERSION`, `FACTS_SCHEMA_VERSION: i32 = 1`), `src/tests/{schema,writer,reader,version}.rs`
- Modify: `Cargo.toml` workspace `members`; `xtask/test_tiers.toml` (new bucket `core-facts`: `cargo nextest run -p julie-facts`, `expected_seconds = 5`, `timeout_seconds = 60`, `scope_label = "core"`, added to `nano`, `fast`, `smoke`, `dev`, `full`); `xtask/tests/support/manifest_contract_expected.rs`; `src/tools/workspace/indexing/engine_version.rs` becomes `pub use julie_facts::version::SEMANTIC_INDEX_ENGINE_VERSION;`

**Interfaces:**
- Consumes: `julie_extractors::{ExtractionResults, Symbol, Identifier, Relationship, TypeInfo, SourceRegion, StructuralFact, ComplexityMetric, Literal, TypeArgumentUsage, ParseDiagnostic}` (`~/.cargo/git/checkouts/julie-extractors-6a4e4815186029a5/3868ed8/crates/julie-extractors/src/base/types.rs`), `EXTRACTION_CONTRACT_VERSION` (`lib.rs:139`).
- Produces: `FactsStore { path: Option<PathBuf>, conn }` with `open(path) -> Result<Opened>` where `Opened::{Ready(FactsStore), VersionMismatch { found_schema, found_engine }}` (the caller deletes the index directory on mismatch; the store never migrates); `FactsWriter::apply(&mut self, changes: &[PathChange]) -> Result<Applied>` where `PathChange::{Upsert { path: String, bytes: Vec<u8>, language: String }, Remove { path: String }}` and `Applied { new_blobs: usize, reused_blobs: usize, removed_paths: usize, paths_now: Vec<(String, String)> }`; extraction runs inside `apply` through an injected `Extractor` trait object (`fn extract(&self, path, content, language) -> ExtractionResults`) so tests pass a fake; `FactsReader` with `symbols_for_paths(&[&str]) -> Vec<SymbolRow>`, `identifiers_for_paths`, `relationships_for_paths`, `types_for_paths`, `source_regions_for_path`, `structural_facts(query)`, `complexity_for_symbol`, `diagnostics_for_path`, `paths() -> Vec<PathRow>`, `blob_count()`, `file_size_bytes()`; `SymbolRow.id` is `"<blob_hash>:<ordinal>"` and `SymbolRow.path` is joined from `paths` at read time.
- Tables (all from design 6.1): `meta(key TEXT PRIMARY KEY, value TEXT)`; `blobs(hash TEXT PRIMARY KEY, language, extractor_version, byte_len INTEGER)`; `paths(path TEXT PRIMARY KEY, blob_hash TEXT NOT NULL, language)`; `symbols(blob_hash, ordinal, name, kind, start_line, start_col, end_line, end_col, start_byte, end_byte, body_start_line, ..., signature, doc_comment, visibility, parent_ordinal, annotations JSON, metadata JSON, semantic_group, confidence, content_type, PRIMARY KEY(blob_hash, ordinal))`; `identifiers(blob_hash, ordinal, name, kind, span cols, containing_ordinal, receiver_type, code_context, confidence, PRIMARY KEY(blob_hash, ordinal))`; `relationships(blob_hash, ordinal, from_ordinal, to_name, to_blob_hash NULL, to_ordinal NULL, kind, line_number, span JSON, reference_site_is_exact, confidence, metadata JSON)`; `types(blob_hash, symbol_ordinal, resolved_type, generic_params JSON, constraints JSON, is_inferred)`; `source_regions`, `structural_facts`, `complexity_metrics`, `literals`, `type_arguments`, `diagnostics` each keyed `(blob_hash, ordinal)`; `vectors` and `encoder` come in Task 10; `test_verdicts` is created empty. Indexes: `symbols(name)`, `identifiers(name)`, `identifiers(blob_hash, kind)`, `paths(blob_hash)`.

**Contract inputs:** design 6.1 and 6.4; `blake3::hash(bytes).to_hex()` as the blob hash (the same algorithm as `crates/julie-core/src/database/files.rs:557`); normalization from `crates/julie-pipeline/src/indexing_core/normalized.rs:24-63` (literal carrier classification, test-role classification, `flatten_type_argument_usages`) is applied before rows are written, so those three functions move into `julie-facts::rows` in this task.

**File ownership:** Create `crates/julie-facts/` (all), modify `Cargo.toml`, `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs`, `src/tools/workspace/indexing/engine_version.rs`.

**Serialization required:** Yes

**Dependency reason:** Everything else consumes its types.

**What to build:** The one durable database and its one writer as a library with no knowledge of workspaces, handlers, or Tantivy. `apply` hashes each upsert, skips extraction when the blob exists (`reused_blobs`), extracts and inserts rows for new blobs in one transaction, and updates `paths`. Rows for blobs no path references stay. Cross-file relationship targets (`to_name`) are stored unresolved; resolution is the graph's job (Task 2). Symbol ids stop being extractor strings: the writer assigns `ordinal` in extraction order per blob and maps `parent_id`, `containing_symbol_id`, and `from_symbol_id` to ordinals within the same blob; a reference to an id outside the blob becomes a name-only target.

**Approach:** Start from `julie_extractors::ExtractionResults` and `crates/julie-pipeline/src/indexing_core/batch.rs:46-59` (the ten collections) to define the row set. Copy `SymbolDatabase::new` pragmas (`crates/julie-core/src/database/mod.rs:120-183`: WAL, `busy_timeout(5000)`, `synchronous=NORMAL`, `foreign_keys=ON`) without sqlite-vec. Tests: `writer.rs` proves that applying the same bytes under two paths extracts once (`reused_blobs == 1`), that a second `apply` with changed bytes leaves the old blob's rows in place and repoints `paths`, that `Remove` deletes only the `paths` row, and that ordinals are stable across a reopen; `version.rs` proves `open` on a store with a different `meta.engine_version` returns `VersionMismatch` without touching the file. Follow razorback:test-driven-development.

**Acceptance criteria:**
- [x] `cargo nextest run -p julie-facts` passes; every table in design 6.1 except `vectors` and `encoder` exists after `FactsStore::in_memory()`; no `UPDATE` or `DELETE` statement in `writer.rs` targets a table other than `paths`.
- [x] `FactsStore::open` on a mismatched `meta` row returns `VersionMismatch` and does not alter the file (blake3 of the file before and after is equal in the test).
- [x] `cargo xtask test list` shows `core-facts` in `nano`, `fast`, `smoke`, `dev`, `full`; `cargo nextest run -p xtask` passes.
- [x] `cargo build` green; worker scope green; committed per commit mode.

---

### Task 2: Graph and resolution

**Files:**
- Create: `crates/julie-index/src/graph/mod.rs` (`Graph`, `SymbolId(u32)` dense ids, `GraphStats { symbols, edges, load_millis, resident_bytes }`), `graph/load.rs` (`Graph::load(reader: &FactsReader, paths: Option<&[String]>) -> Graph`, `Graph::apply_paths(&self, reader, changed: &[String], removed: &[String]) -> Graph` returning a new graph that shares unchanged per-file arrays), `graph/resolve.rs` (pure `resolve(symbols, identifiers, relationships) -> Vec<Edge>`), `graph/edges.rs` (adjacency arrays: `callers(id)`, `callees(id)`, `references_to(id)`, `references_from(id)`, `children(id)`, `parent(id)`, `implementations(id)`), `graph/web_edges.rs` (derived HTTP route edges), `graph/scores.rs` (`reference_score(id)` with the propagation rules of `crates/julie-core/src/database/relationships.rs:236-480`), `crates/julie-index/src/tests/graph/{resolve,edges,web_edges,scores,load,incremental}.rs`
- Modify: `crates/julie-index/src/lib.rs` (`pub mod graph`), `crates/julie-index/Cargo.toml` (add `julie-facts`)

**Interfaces:**
- Consumes: Task 1 `FactsReader` and row types.
- Produces: `Graph` (immutable once built, `Send + Sync`), `Edge { from: SymbolId, to: SymbolId, kind: EdgeKind }` with `EdgeKind::{Calls, References, Imports, Implements, Extends, Contains, WebRoute}`; `Graph::symbol(id) -> &SymbolRow`; `Graph::find_by_name(name) -> &[SymbolId]` (exact), `Graph::find_by_name_suffix(qualified) -> Vec<SymbolId>` (the `Symbol::suffix` / `Symbol.suffix` rule from `crates/julie-core/src/database/impact_graph.rs:128`), `Graph::symbols_in_path(path) -> &[SymbolId]`, `Graph::paths() -> &[String]`, `Graph::stats() -> GraphStats`.

**Contract inputs:** resolution rules to keep, each named in a test: `IMPACT_IDENTIFIER_KINDS = ["type_usage", "import", "call"]` and the kind mapping call->Calls, import->Imports, type_usage->References (`impact_graph.rs:21,92`); ambiguity drops the edge (`resolve_frontier_target`, `impact_graph.rs:105`); definition priority and qualified-name parsing from `crates/julie-tools/src/navigation/resolution.rs:23-92`; web edge derivation from `crates/julie-pipeline/src/indexing_core/web_edges.rs` (route facts to handler symbols); reference score propagation from `relationships.rs:236-480`.

**File ownership:** Create `crates/julie-index/src/graph/` and `crates/julie-index/src/tests/graph/`; modify `crates/julie-index/src/lib.rs`, `crates/julie-index/Cargo.toml`.

**Serialization required:** Yes

**Dependency reason:** Needs Task 1 row types.

**What to build:** The graph as pure resolution over facts: identifiers and unresolved relationship targets are matched to symbol definitions by name (exact, then qualified suffix, with the existing priority), ambiguous matches drop, and the result is adjacency arrays indexed by a dense id. `load` reads every path's rows through `FactsReader`, resolves once, computes reference scores, and records `load_millis` and an estimated `resident_bytes` (sum of array capacities). `apply_paths` re-reads only the changed paths and re-resolves edges that touch them; identifiers in unchanged files that name a symbol in a changed file are re-resolved by name lookup, so a rename in one file updates callers in others.

**Approach:** Read `impact_graph.rs`, `navigation/resolution.rs`, `navigation/call_path.rs:120-224`, `deep_dive/data/graph.rs`, and `web_edges.rs` first; each contributes rules, not code. Build the graph from vectors, not hash maps of structs: `names: Vec<String>`, `by_name: HashMap<String, Vec<SymbolId>>`, CSR-style `edge_offsets: Vec<u32>` + `edges: Vec<(SymbolId, EdgeKind)>` per direction. Tests feed hand-written rows (no extractor): a three-file fixture with a definition, a call, a qualified call, an ambiguous name, a route handler; assert the edge list. Incremental test: change one file's rows, assert only edges touching it changed and the rest are pointer-equal. No `petgraph`; no new dependency.

**Acceptance criteria:**
- [x] `cargo nextest run -p julie-index --lib tests::graph` passes with tests named for each kept rule (exact match, suffix match, ambiguity drop, priority, web route, score propagation, incremental re-resolution).
- [x] `Graph::load` on `fixtures/seed/a` extracted through the real extractor (one integration test in `load.rs`, under two seconds) yields at least one `Calls` edge and stats with `load_millis > 0`.
- [x] No file under `crates/julie-index/src/graph/` exceeds 500 lines; no section 4 word appears in the new code.
- [x] `cargo build` green; worker scope green; committed per commit mode.

---

### Task 3: Snapshot, checkout store, writer wiring, projection from facts

**Files:**
- Create: `crates/julie-index/src/snapshot.rs` (`Snapshot { graph: Arc<Graph>, searcher: tantivy::Searcher, vectors: Arc<VectorSet>, facts: FactsReader, published_at }`), `crates/julie-index/src/checkout_store.rs` (`CheckoutStore::open(index_dir, root) -> Result<CheckoutStore>`, `current() -> Arc<Snapshot>`, `apply(&self, changes, guard: &MutationGuard<'_>) -> Result<Applied>` which writes facts, updates the graph, projects the changed paths into Tantivy, commits, and swaps the `Arc`; `rebuild_tantivy_if_needed()`), `crates/julie-index/src/search/projection/from_facts.rs` (documents from `SymbolRow` + blob text; replaces `apply.rs:294-413` builders' DB inputs), `crates/julie-index/src/vectors/mod.rs` (`VectorSet::empty()` only; Task 10 fills it), `crates/julie-test-support/src/snapshot_fixture.rs` (`SnapshotFixture::from_tree(path) -> SnapshotFixture { store: CheckoutStore }` on `FactsStore::in_memory()` and `Index::create_in_ram`)
- Modify: `crates/julie-context/src/tool_context.rs` (add `fn snapshot(&self, target: &WorkspaceTarget) -> Result<Arc<Snapshot>>`; keep the DB accessors until Task 13), `crates/julie-test-support/src/fake_tool_context.rs`, `crates/julie-runtime/src/workspace/mod.rs:33-68` (`JulieWorkspace` gains `store: Option<Arc<CheckoutStore>>`; `db` and `search_index` stay until Task 13), `crates/julie-runtime/src/watcher/handlers/created_modified.rs:191-266` and `delete_rename.rs:42-65` (call `store.apply` beside the old write), `crates/julie-runtime/src/watcher/runtime/processing.rs`, `src/tools/workspace/indexing/pipeline.rs:114` and `pipeline_persistence.rs:18-137` (call `store.apply` with the batch's file bytes beside the old persist), `src/handler/tool_context_impl.rs` (implement `snapshot`), `src/handler.rs` (open the store where `initialize_database` runs), `src/tests/service/durable_roots.rs` (expect `facts.sqlite` beside `db/symbols.db` during the transition; Task 13 removes the old names)

**Interfaces:**
- Consumes: Task 1 `FactsStore`, `FactsWriter`, `FactsReader`; Task 2 `Graph`; existing `SearchIndex` (`crates/julie-index/src/search/index.rs`) and `create_schema` (`schema.rs:57`); `mutation_gate::MutationGuard`.
- Produces: `ToolContext::snapshot(&WorkspaceTarget) -> Result<Arc<Snapshot>>`; `Snapshot::graph()`, `Snapshot::searcher()`, `Snapshot::facts()`, `Snapshot::vectors()`; `CheckoutStore::status() -> StoreStatus { blob_count, facts_bytes, tantivy: TantivyState::{Present, Building, Stale, Absent}, tantivy_age_secs, graph: GraphStats, last_write_at }`; `SnapshotFixture::from_tree`.

**Contract inputs:** Tantivy field set stays `crates/julie-index/src/search/schema.rs:24-51` (21 fields, two doc types) so query code in Task 4 is unchanged; `tantivy/julie.meta.json` holds `{ "schema_signature": compatibility_signature(), "engine_version": SEMANTIC_INDEX_ENGINE_VERSION }`; a mismatch or missing directory rebuilds from facts under the guard. Document ids become `"<blob_hash>:<ordinal>"`; a path change re-projects only that path's documents (delete by `file_path` term, add from the new blob).

**File ownership:** as listed in the Parallel Execution Contract row for Task 3.

**Serialization required:** Yes

**Dependency reason:** Needs Tasks 1 and 2; every tool port needs `ToolContext::snapshot`.

**What to build:** The one seam between storage and tools. During this task the old `SymbolDatabase` write path keeps running so un-ported tools stay green; the new store is written in the same guarded sections. The transitional double write is removed in Task 13. `SnapshotFixture` is what every tool test uses from Task 4 on: extract a fixture tree into an in-memory facts store, build the graph, project into a RAM index, publish one snapshot.

**Approach:** `CheckoutStore::apply` order: `FactsWriter::apply` (commit) -> `Graph::apply_paths` -> `from_facts::project(paths)` -> `IndexWriter::commit` -> `searcher` reload -> `Arc` swap. Readers never wait; the previous `Arc<Snapshot>` stays valid for in-flight tool calls. Test `checkout_store.rs`: apply three files, assert the snapshot's graph has their symbols and the searcher finds a name; apply a change, assert the old `Arc` still answers the old bytes and the new one answers the new; delete `tantivy/`, reopen, assert `rebuild_tantivy_if_needed` restores the document count from facts. `durable_roots.rs` lists `indexes/<id>/` after an index and accepts only `facts.sqlite`, `facts.sqlite-wal`, `facts.sqlite-shm`, `tantivy/`, and, until Task 13, `db/`. Follow razorback:test-driven-development.

**Acceptance criteria:**
- [x] `cargo nextest run -p julie-index --lib tests::checkout_store` and `cargo nextest run -p julie-test-support` pass; `SnapshotFixture::from_tree("fixtures/seed/a")` builds in under two seconds.
- [x] `cargo nextest run --lib tests::service::durable_roots` passes with `facts.sqlite` and `tantivy/julie.meta.json` present after a full index.
- [x] `cargo nextest run -p julie-runtime --lib tests::watcher_handlers` passes with the watcher writing both stores.
- [x] `cargo build` green; worker scope green; committed per commit mode.

---

### Task 4: `fast_search` on the snapshot

**Files:**
- Modify: `crates/julie-tools/src/search/tool_execution.rs:126-227` (index handles), `text_search.rs:37-323` (`get_symbols_by_ids` -> `graph.symbol`), `line_mode.rs:279-283` (`get_file_content` -> blob bytes through `facts.blob_text(hash)`; `get_source_regions_for_file` -> `facts.source_regions_for_path`), `execution/semantic.rs` (generation checks deleted; semantic branch calls `snapshot.vectors()` which is empty until Task 10 and reports `SEMANTICS_NOT_READY`), `search/mod.rs`; tests `crates/julie-tools/src/tests/search_*`, `src/tests/tools/search/**`, `src/tests/tools/search_context_lines.rs`, `src/tests/tools/text_search_tantivy.rs` rebuilt on `SnapshotFixture`

**Interfaces:**
- Consumes: Task 3 `ToolContext::snapshot`, `Snapshot::searcher()`, `Snapshot::graph()`, `Snapshot::facts()`; `SearchExecutionResult`, `SearchTrace` (`search/trace.rs`) unchanged.
- Produces: `fast_search` with identical output text and trace fields for the lexical paths; `FastSearchParams` unchanged.

**Contract inputs:** the definition-first path and `is_definition_name_match` (`formatting.rs:143`), line enrichment statuses, zero-hit hints, the truncation line; `SearchIndex` query methods (`search_symbols`, `search_unified_kind_filtered`, `search_unified_with_meta`) now take the snapshot's `Searcher` instead of opening their own.

**File ownership:** `crates/julie-tools/src/search/**`, `crates/julie-tools/src/tests/search_*`, `src/tests/tools/search/**`, `src/tests/tools/search_context_lines.rs`, `src/tests/tools/text_search_tantivy.rs`.

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** The same search over the snapshot. Every `get_search_index_for_workspace`, `primary_pooled_database*`, and `get_pooled_database_for_workspace` call in the search directory is replaced by one `handler.snapshot(&target)?` at the top of `execute_with_trace`, threaded down. Symbol hydration by id reads the graph; file text reads the blob.

**Approach:** Start with `julie-server symbols crates/julie-tools/src/search/tool_execution.rs --workspace .` and `refs get_symbols_by_ids`. Port the tests first: replace `index_workspace(...)` helpers in `src/tests/tools/search/**` with `SnapshotFixture::from_tree(tempdir)`; keep every assertion string. Then port the code until the tests are green. Delete `execution/semantic.rs` generation checks (`get_latest_ready_generation`, `embedding_generation_ready`, `get_latest_canonical_revision_number`) rather than porting them.

**Acceptance criteria:**
- [x] `rg -n 'SymbolDatabase|pooled_database|get_search_index_for_workspace|with_read_transaction' crates/julie-tools/src/search` returns nothing.
- [x] `cargo nextest run -p julie-tools --lib tests::search` and `cargo nextest run --lib tests::tools::search` pass on `SnapshotFixture`.
- [x] `cargo build` green; worker scope green; handed to the lead per commit mode.

---

### Task 5: `get_symbols` and `patterns` on the snapshot

**Files:**
- Modify: `crates/julie-tools/src/symbols/primary.rs:32-66`, `symbols/target_workspace.rs:35-108` (`get_symbols_for_file*` -> `graph.symbols_in_path` with rows from `graph.symbol`), `crates/julie-tools/src/patterns/mod.rs:104,179` (`search_structural_facts` -> `facts.structural_facts(query)` with `StructuralFactQuery` moved to `julie-facts::reader`), tests `src/tests/tools/get_symbols*.rs`, `src/tests/tools/patterns.rs`; `xtask/test_tiers.toml` `tools-get-symbols` gains `cargo nextest run --lib tests::tools::patterns`; `xtask/tests/support/manifest_contract_expected.rs`

**Interfaces:**
- Consumes: Task 3 `Snapshot`; `StructuralFactQuery` (`crates/julie-core/src/database/structural_facts.rs`) moves to `julie-facts::reader::StructuralFactQuery` in this task (the julie-core copy stays until Task 13).
- Produces: `get_symbols` and `patterns` outputs unchanged; ADR-0001 path resolution kept in `symbols/target_workspace.rs`.

**Contract inputs:** `GetSymbolsTool` and `PatternsTool` parameter structs unchanged; lightweight mode (`get_symbols_for_file_lightweight`) becomes a projection over the same rows.

**File ownership:** `crates/julie-tools/src/symbols/**`, `crates/julie-tools/src/patterns/**`, `src/tests/tools/get_symbols*.rs`, `src/tests/tools/patterns.rs`, `xtask/test_tiers.toml` (that bucket only), `xtask/tests/support/manifest_contract_expected.rs` (that bucket only).

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** The two smallest tools, ported first in the batch so the fixture pattern is proven. `patterns` finally gets a bucket.

**Approach:** Tests first on `SnapshotFixture`; the `patterns` tests in `src/tests/tools/patterns.rs` currently run in no bucket, so run them by name during the port.

**Acceptance criteria:**
- [x] `rg -n 'SymbolDatabase|pooled_database|into_read_snapshot' crates/julie-tools/src/symbols crates/julie-tools/src/patterns` returns nothing.
- [x] `cargo nextest run --lib tests::tools::get_symbols` and `cargo nextest run --lib tests::tools::patterns` pass; `cargo nextest run -p xtask` passes with the bucket change.
- [x] `cargo build` green; worker scope green; handed to the lead per commit mode.

---

### Task 6: `fast_refs` and `call_path` on the graph

**Files:**
- Modify: `crates/julie-tools/src/navigation/fast_refs.rs:191-367`, `navigation/target_workspace.rs:55-176`, `navigation/fast_refs_semantic.rs` (generation checks deleted; zero-reference fallback asks `snapshot.vectors()`), `navigation/call_path.rs:120-395` (`bfs_shortest_path` walks `graph.callees`/`graph.callers` instead of per-hop SQL), `navigation/call_path_web.rs` (`web_edges_from_symbols` -> `graph` `WebRoute` edges), `navigation/resolution.rs` (priority helpers now operate on `SymbolId`), tests listed in the contract row; `xtask/test_tiers.toml` `tools-call-path` gains `cargo nextest run --lib tests::tools::web_navigation`; `xtask/tests/support/manifest_contract_expected.rs`

**Interfaces:**
- Consumes: Task 2 `Graph::{find_by_name, find_by_name_suffix, callers, callees, references_to, symbol}`; Task 3 `Snapshot`.
- Produces: `fast_refs` and `call_path` outputs unchanged, including `reference_kind` filtering (kinds map to `EdgeKind`) and the `--web` route path.

**Contract inputs:** the dogfood finding "`fast-refs` lists a `pub use` re-export line twice" is fixed here by construction: the graph holds one edge per `(from, to, kind)`; add a test for a re-export line.

**File ownership:** `crates/julie-tools/src/navigation/**`, the six test paths in the contract row, `xtask/test_tiers.toml` (that bucket only), `xtask/tests/support/manifest_contract_expected.rs` (that bucket only).

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Reference and path queries as graph walks. Definition lookup by name uses the graph's name index and the existing priority order; identifier-based references are `references_to(id)` filtered by kind; BFS is over adjacency arrays with the same depth limit.

**Approach:** Port `target_workspace.rs` first (it is the smaller mirror of `fast_refs.rs`), then delete the duplication if both paths become one function over a snapshot. `web_navigation.rs` tests currently run in no bucket; run them by name during the port.

**Acceptance criteria:**
- [x] `rg -n 'SymbolDatabase|pooled_database|get_relationships_to_symbols|get_identifiers_by_names' crates/julie-tools/src/navigation` returns nothing.
- [x] `cargo nextest run --lib tests::tools::call_path_tests tests::tools::call_path_disambiguation_tests tests::tools::fast_refs_primary_rebind_tests tests::tools::target_workspace_fast_refs_tests tests::tools::web_navigation` passes; a new test proves a `pub use` re-export line is listed once.
- [x] `cargo build` green; worker scope green; handed to the lead per commit mode.

---

### Task 7: `deep_dive` on the graph

**Files:**
- Modify: `crates/julie-tools/src/deep_dive/mod.rs:84-344`, `deep_dive/data/lookup.rs`, `data/mod.rs:54-157`, `data/graph.rs`, `data/similarity.rs` (asks `snapshot.vectors()`; empty until Task 10), tests `crates/julie-tools/src/tests/deep_dive_*`, `src/tests/tools/deep_dive_complexity.rs`, `src/tests/tools/deep_dive_primary_rebind_tests.rs`

**Interfaces:**
- Consumes: Task 2 `Graph::{callers, callees, children, parent, implementations, references_to, reference_score}`; Task 3 `Snapshot::facts().complexity_for_symbol`.
- Produces: `deep_dive` output unchanged for the non-semantic sections; the "similar symbols" section reads `snapshot.vectors()`.

**Contract inputs:** `DeepDiveTool` params unchanged; centrality disambiguation uses `graph.reference_score`.

**File ownership:** `crates/julie-tools/src/deep_dive/**`, `crates/julie-tools/src/tests/deep_dive_*`, `src/tests/tools/deep_dive_complexity.rs`, `src/tests/tools/deep_dive_primary_rebind_tests.rs`.

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** The symbol report from graph neighbors plus one facts query for complexity. The `#[cfg(test)]` generation helpers at `mod.rs:404-428` are deleted.

**Approach:** Tests first on `SnapshotFixture`; the incoming/outgoing/children/implementations sections map one-to-one onto graph accessors.

**Acceptance criteria:**
- [x] `rg -n 'SymbolDatabase|pooled_database|into_read_snapshot|get_latest_ready_generation' crates/julie-tools/src/deep_dive` returns nothing.
- [x] `cargo nextest run -p julie-tools --lib tests::deep_dive` and `cargo nextest run --lib tests::tools::deep_dive` pass.
- [x] `cargo build` green; worker scope green; handed to the lead per commit mode.

---

### Task 8: `blast_radius` on the graph

**Files:**
- Modify: `crates/julie-tools/src/impact/mod.rs:118-149`, `impact/walk.rs:60-327`, `impact/likely_tests.rs:116-179`, `impact/seed.rs` (the revision-range seed at `:57` is deleted; seeds are symbol names or file paths), tests `crates/julie-tools/src/tests/blast_radius_*`, `src/tests/tools/blast_radius*`

**Interfaces:**
- Consumes: Task 2 `Graph::{references_to, callers, symbols_in_path, reference_score}` and `WebRoute` edges.
- Produces: `blast_radius` output unchanged for symbol and file seeds; the `since_revision` style parameter is removed from `BlastRadiusTool` and from the catalog schema (`src/request_engine/catalog.rs:122` picks it up from the struct).

**Contract inputs:** determinism tests in `src/tests/tools/blast_radius_determinism_tests/` keep their ordering assertions; likely-test discovery keeps the path heuristics (`test`, `tests`, `.Tests`, `_test`, `spec`, `__tests__`).

**File ownership:** `crates/julie-tools/src/impact/**`, `crates/julie-tools/src/tests/blast_radius_*`, `src/tests/tools/blast_radius*` (files and directories).

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** The impact walk as a bounded BFS over `references_to` and web edges with the same depth and fan-out limits.

**Approach:** `walk.rs` currently does per-hop SQL (`get_relationships_to_symbols`, `web_edges_to_symbols`); replace each with an array read. Delete the `revision_file_changes` seed and its tests rather than porting them (sequencing decision 3).

**Acceptance criteria:**
- [x] `rg -n 'SymbolDatabase|pooled_database|get_revision_file_changes_between|web_edges_to_symbols' crates/julie-tools/src/impact` returns nothing.
- [x] `cargo nextest run --lib tests::tools::blast_radius` and `cargo nextest run -p julie-tools --lib tests::blast_radius` pass.
- [x] `cargo build` green; worker scope green; handed to the lead per commit mode.

---

### Task 9: `get_context` on the snapshot

**Files:**
- Modify: `crates/julie-tools/src/get_context/pipeline.rs:23-318`, `get_context/graph.rs:89-142`, `get_context/entries.rs`, `get_context/task_signals.rs:289-358`, `get_context/scoring.rs` (centrality from `graph.reference_score`), `crates/julie-index/src/search/hybrid.rs:115-329` (`hybrid_search_with_tagged_embedding` takes `&Snapshot`), tests `crates/julie-tools/src/tests/get_context_*`, `src/tests/tools/get_context_*.rs`

**Interfaces:**
- Consumes: Task 3 `Snapshot`; Task 2 graph accessors; `hybrid.rs` vector side reads `snapshot.vectors()` (empty until Task 10).
- Produces: `get_context` output unchanged for the lexical path; `hybrid_search_with_tagged_embedding(snapshot, query, ...)` signature is the one Task 10 fills.

**Contract inputs:** token allocation (`allocation.rs`), pivot selection (`scoring.rs:48`), second hop (`second_hop.rs`), formatting unchanged.

**File ownership:** `crates/julie-tools/src/get_context/**`, `crates/julie-tools/src/tests/get_context_*`, `src/tests/tools/get_context_*.rs`, `crates/julie-index/src/search/hybrid.rs`.

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Pivot search over the searcher, one-hop and two-hop expansion over the graph, allocation and formatting untouched.

**Approach:** `expand_graph_from_ids` (`graph.rs:89`) becomes `graph.references_to` + `graph.references_from` + identifier edges already merged in the graph; `get_all_indexed_files` becomes `graph.paths()`.

**Acceptance criteria:**
- [x] `rg -n 'SymbolDatabase|pooled_database|into_read_snapshot' crates/julie-tools/src/get_context crates/julie-index/src/search/hybrid.rs` returns nothing.
- [x] `cargo nextest run -p julie-tools --lib tests::get_context` and `cargo nextest run --lib tests::tools::get_context` pass.
- [x] `cargo build` green; worker scope green; handed to the lead per commit mode.

---

### Task 10: Vectors in facts, brute-force scan

**Files:**
- Modify: `crates/julie-facts/src/schema.rs` (add `vectors(blob_hash, symbol_ordinal, encoder_id, vector BLOB, PRIMARY KEY(blob_hash, symbol_ordinal, encoder_id))`, `encoder(id TEXT PRIMARY KEY, model_checksum, dimensions, pooling, normalization, instruction_policy)`), `writer.rs` (`store_vectors(encoder_id, rows)`; `set_encoder(row)` deletes `vectors` rows when the identity differs, the one exception to "no DELETE", written down here), `reader.rs` (`vectors_for_encoder`, `encoder()`), `crates/julie-index/src/vectors/mod.rs` (`VectorSet { encoder, ids: Vec<SymbolId>, matrix: Vec<f32>, dims }`, `VectorSet::scan(query, limit) -> Vec<(SymbolId, f32)>` brute-force cosine), `crates/julie-index/src/snapshot.rs` (loads the set at publish), `crates/julie-index/src/search/similarity.rs`, `crates/julie-index/src/search/hybrid.rs` (KNN -> scan), `crates/julie-pipeline/src/embeddings/pipeline/**` (write through `FactsWriter::store_vectors`; generations gone), `src/tools/workspace/indexing/embeddings.rs`, `src/tools/workspace/indexing/pipeline_runner.rs`, `crates/julie-tools/src/search/execution/semantic.rs`, `crates/julie-tools/src/deep_dive/data/similarity.rs`, `crates/julie-tools/src/navigation/fast_refs_semantic.rs`, `src/health/**` (semantic readiness = encoder row present and vector count for the snapshot's symbols above zero)
- Delete: `crates/julie-core/src/database/{vectors,embedding_generation,embedding_generation_eligibility,embedding_generation_types,memory_vectors}.rs`, `crates/julie-core/src/tests/{vector_storage,memory_vectors,embeddings_identity}.rs`, `crates/julie-core/src/tests/database/embedding*`, `sqlite-vec` from `crates/julie-core/Cargo.toml` and `Cargo.lock`

**Interfaces:**
- Consumes: Task 3 `Snapshot`; the existing `EmbeddingProvider` and native provider (`crates/julie-pipeline/src/embeddings/native/`, which phase 4 deletes) and `EncoderIdentity` (`crates/julie-core/src/embeddings_identity.rs`, kept).
- Produces: `Snapshot::vectors() -> Arc<VectorSet>`; `VectorSet::scan`; semantic readiness through `system_readiness` without generations; `CheckoutStatus.vector_count` from the facts reader.

**Contract inputs:** design 8: one encoder row; a different identity deletes `vectors` and rebuilds; brute-force scan is the query; the status page shows vector count and query latency. Thresholds stay: symbol-to-symbol 0.5, query-to-symbol 0.2 (`CLAUDE.md` item 8).

**File ownership:** as listed in the Parallel Execution Contract row for Task 10.

**Serialization required:** Yes

**Dependency reason:** Touches three tools' semantic branches after Batches A and B land.

**What to build:** Vectors as fact rows keyed by blob and ordinal, held in memory beside the graph, queried by a cosine scan. The embedding job embeds symbols of blobs with no vector row for the current encoder, so an unchanged file across worktrees is embedded once per machine only if the sibling seed (Task 11) copies its rows.

**Approach:** Keep the pipeline's batching and enrichment (`embedding_metadata*`); replace `begin_/publish_/store_embeddings_for_generation` with `store_vectors`. Scan test: 10,000 random 384-d vectors, top-5 by cosine matches a naive reference, under 50 ms. Readiness test through `FakeToolContext`: no encoder row -> `SEMANTICS_NOT_READY`; encoder and vectors -> ready.

**Acceptance criteria:**
- [x] `rg -n 'embedding_generation|symbol_vectors|sqlite_vec|sqlite-vec|knn_search' crates src --glob '!**/tests/**'` returns nothing.
- [x] `cargo nextest run -p julie-index --lib tests::vectors`, `cargo nextest run -p julie-pipeline --lib tests::embedding`, and `cargo nextest run --lib tests::core::embedding_provider` pass.
- [x] `cargo build` green; worker scope green; committed per commit mode.

---

### Task 11: Workspace commands, startup, seed on facts

**Files:**
- Modify: `src/tools/workspace/commands/**` (`index`, `refresh`, `rebuild`, `status`, `health`, `open`, `list`, `remove` over `CheckoutStore`), `src/tools/workspace/indexing/**` (`index.rs`, `pipeline.rs`, `pipeline_persistence.rs`, `incremental.rs`, `source_check.rs`, `seed.rs`, `finalize.rs`, `route.rs`; delete `resolver/` if only `finalize` used it), `src/startup.rs`, `src/startup_repair_plan.rs` (the plan compares `paths` to the scanned tree and blob hashes to disk; engine mismatch means the store failed to open and the directory was deleted), `src/request_engine/runtime_factory.rs:131-144`, tests `src/tests/tools/workspace/**`, `src/tests/core/workspace_init/**`

**Interfaces:**
- Consumes: Task 3 `CheckoutStore::{apply, status, rebuild_tantivy_if_needed}`; Task 1 `FactsWriter`; Task 10 `store_vectors`; `git_common_dir` (`crates/julie-core/src/workspace/git_identity.rs`).
- Produces: `manage_workspace status` fields from `StoreStatus` (`blob_count`, `facts_bytes`, `tantivy`, `tantivy_age_seconds`, `graph_symbols`, `graph_load_millis`, `graph_resident_bytes`, `vector_count`, `last_write_at`; `file_count` = paths, `symbol_count` = graph symbols); `rebuild` deletes `indexes/<id>/` and reindexes; `open` seeds from a sibling by copying `blobs` and fact rows for every hash present in the new tree (`ATTACH` the sibling read-only, `INSERT ... SELECT` by hash), extracts only missing blobs, builds `paths`, rebuilds `tantivy/`; the seed report line stays `Seeded from <sibling>: <copied> blobs copied, <extracted> extracted, <removed> removed in <ms> ms` (word `files` becomes `blobs`; update `src/tests/tools/workspace/seed.rs`).

**Contract inputs:** `ManageWorkspaceOperation::OPERATIONS` unchanged; `IndexingRuntimeState` stages stay for the dashboard; the startup catch-up runs under the guard as today (`src/startup.rs:59-70`).

**File ownership:** as listed in the Parallel Execution Contract row for Task 11.

**Serialization required:** Yes

**Dependency reason:** Needs Task 10 (embedding scheduling moves with the writer).

**What to build:** The full and incremental index as one function: scan -> `PathChange` list (upserts for new or changed hashes, removes for missing paths) -> `CheckoutStore::apply` -> embedding job for blobs without vectors. `source_check.rs`'s torn-read defense becomes a hash comparison inside `FactsWriter::apply` (the bytes it hashed are the bytes it extracted, so the check is free).

**Approach:** Delete `pipeline_persistence.rs`'s old branch and the double write from Task 3 in this task for the full-index path (the watcher's double write goes in Task 13). `runtime_factory.rs` opens the store through `CheckoutStore::open`; `VersionMismatch` deletes `index_root` and reopens.

**Acceptance criteria:**
- [x] `cargo nextest run --lib tests::tools::workspace` and `cargo nextest run --lib tests::core::workspace_init` pass; the seed test copies 6 blobs and extracts 2 on `fixtures/seed/{a,b}`.
- [x] `rg -n 'incremental_update_atomic|bulk_store_fresh_atomic|record_indexing_repair|upsert_projection_state' src/tools/workspace src/startup*.rs src/request_engine` returns nothing.
- [x] `cargo build` green; worker scope green; committed per commit mode.

---

### Task 12: Edit tools, health, dashboard, metrics, `extract`

**Files:**
- Modify: `crates/julie-tools/src/editing/**` and `crates/julie-tools/src/refactoring/**` (symbol lookup by name and file hash through the snapshot: `graph.find_by_name`, `facts.paths()` blob hash instead of `get_file_hash`), `src/handler/tools/{edit_file,rewrite_symbol,rename_symbol}.rs`, `src/workspace_runtime/source_edit.rs`, `source_edit_ops.rs` (after an applied edit, call `store.apply` for the written paths under the guard, replacing the old refresh hint), `src/workspace_runtime/{publication.rs,recovery.rs,edit_journal.rs,dirty_queue.rs}` (publication and recovery are deleted: the snapshot swap is the publication; the edit journal is deleted if `recover_edit` can be served from `paths` blob hashes, else reduced to a per-path last-write record in memory), `src/health/**` (projection and generation checks deleted; health reports `StoreStatus`), `src/dashboard/routes/intelligence.rs` and `src/dashboard/**` (analytics from the graph and `FactsReader`, or deleted where they only mirrored `analytics.rs`), `src/tools/metrics/**` and `src/registry/**` (`tool_calls` stays on `registry.db`; the `SymbolDatabase` copy is deleted), `src/external_extract/**` (`extract` writes a `facts.sqlite` through `FactsWriter`; `info` reads `meta` and counts), their tests

**Interfaces:**
- Consumes: Tasks 3, 10, 11 store API.
- Produces: `edit_file`, `rewrite_symbol`, `rename_symbol` unchanged in output; `manage_workspace health` from `StoreStatus`; `julie-server extract --out <file>` producing a facts store; dashboard pages that survive read the snapshot.

**Contract inputs:** ADR-0003 prepared-once edits; ADR-0004's process-global edit map stays as is (it carries a section 4 word already; no new use); `recover_edit`/`recover-edit` operations stay in `OPERATIONS`.

**File ownership:** as listed in the Parallel Execution Contract row for Task 12.

**Serialization required:** Yes

**Dependency reason:** Needs Task 11 (store API final).

**What to build:** The last readers of `SymbolDatabase` outside the deleted modules, each moved or deleted. Health and dashboard shrink to what the status document already carries (design 10).

**Approach:** `rg -l 'SymbolDatabase|pooled_database|get_database_for_workspace' src crates --glob '!**/tests/**'` is the worklist; it must be empty at the end of this task except `crates/julie-core/src/database/` itself and `crates/julie-runtime/src/workspace/mod.rs` (Task 13). Delete dashboard routes whose only data source was `analytics.rs` unless a test or doc names them as a product feature.

**Acceptance criteria:**
- [ ] `rg -l 'SymbolDatabase|pooled_database|get_database_for_workspace|primary_pooled_database' src crates --glob '!**/tests/**' --glob '!crates/julie-core/src/database/**'` lists only `crates/julie-runtime/src/workspace/mod.rs` and `crates/julie-context/src/tool_context.rs`. (Task 12 owned paths are empty. Remaining hits are handler/indexing/watcher/pipeline/analysis — Task 13.)
- [x] `cargo nextest run --lib tests::tools::editing tests::tools::refactoring tests::external_extract tests::dashboard tests::health` passes; `cargo nextest run -p julie-tools --lib tests::editing` passes.
- [x] `cargo build` green; worker scope green; committed per commit mode.

---

### Task 13: Delete the old storage

**Files:**
- Delete: `crates/julie-core/src/database/` (all 10,220 lines), `crates/julie-core/src/connection_pool.rs`, `crates/julie-core/src/tests/database/`, `crates/julie-core/src/tests/{database.rs,database_lightweight_query.rs,database_row_mapping.rs,bulk_store_types_tests.rs,bulk_store_types_tdd.rs,extractor_projection.rs,receiver_type_storage.rs}`, `crates/julie-pipeline/src/indexing_core/{persistence.rs,web_edges.rs,web_edges/}`, `crates/julie-index/src/search/projection.rs` DB entry (`ensure_current_inner`, `ensure_current_from_database`, `repair_recreated_open_if_needed`) and `projection/apply.rs` DB loaders (`load_symbol_contexts_from_database`, `load_enriched_relationship_text`, `apply_documents_with_db`), `crates/julie-runtime/src/watcher/runtime/repairs.rs`, `crates/julie-runtime/src/watcher/extraction_write.rs`, `crates/julie-core/src/indexing_state.rs` repair reasons that name deleted tables, `src/tests/core/incremental_update_atomic*`, `src/tests/core/revision_changes.rs`, `src/tests/integration/projection_repair.rs`, `src/tests/core/annotation_storage.rs`, `src/tests/core/early_warning_report_cache.rs` (if `early_warning_reports` has no reader left)
- Modify: `crates/julie-context/src/tool_context.rs` (delete the four DB accessors and `get_search_index_for_workspace`), `crates/julie-runtime/src/workspace/mod.rs` (`db` and `search_index` fields deleted; `store` only), `crates/julie-runtime/src/watcher/handlers/*` (old write path deleted; `store.apply` only), `crates/julie-core/Cargo.toml` (`rusqlite` stays only if `registry` still needs it through julie-core; else moves), `src/tests/service/durable_roots.rs` (`db/` no longer accepted), `xtask/test_tiers.toml` and `xtask/tests/support/manifest_contract_expected.rs` (buckets whose commands no longer match anything are deleted: `projection`, `core-database` becomes `cargo nextest run -p julie-core` only if julie-core still has tests, else deleted)

**Interfaces:**
- Consumes: the finished tree of Tasks 1-12.
- Produces: one storage path. `JulieWorkspace { root, julie_dir, store: Arc<CheckoutStore>, watcher, embedding_provider, embedding_runtime_status, config, index_root_override, indexing_runtime }`.

**Contract inputs:** the deletion list for the phase gate (design 5.4 and 6.1): `symbols.db`, migrations, `projection_states`, `canonical_revisions`, `revision_file_changes`, `indexing_repairs`, `index_engine_state`, `embedding_generations`, `symbol_vectors`, `memory_vectors`, `tool_calls` (symbols.db copy), `early_warning_reports`, `external_extract_metadata`, `WorkspaceConnectionPool`, publication and recovery.

**File ownership:** as listed in the Parallel Execution Contract row for Task 13.

**Serialization required:** Yes

**Dependency reason:** Runs last so each earlier task keeps its own tests green.

**What to build:** Nothing. Delete until `cargo build` is green with no `database` module in julie-core, then delete the tests that named the deleted behavior.

**Approach:** Work from the compiler: delete `crates/julie-core/src/database/mod.rs` first and follow the errors. Any reader that still needs a deleted method is a Task 12 miss; fix it through the snapshot, never by keeping the method. Run `sh scripts/complexity-words.sh main` at the end; the only accepted hits are names that survive in `mutation_gate.rs` and ADR-0004's edit map.

**Acceptance criteria:**
- [x] `crates/julie-core/src/database/` does not exist; `rg -n 'SymbolDatabase|canonical_revision|projection_state|indexing_repair|index_engine_state' src crates xtask --glob '!docs/**'` returns nothing.
- [x] `src/tests/service/durable_roots.rs` passes with only `facts.sqlite*` and `tantivy/` under `indexes/<id>/`.
- [x] `cargo nextest run -p julie-core -p julie-facts -p julie-index -p julie-pipeline -p julie-runtime -p julie-tools` passes; `cargo nextest run -p xtask` passes with the bucket changes.
- [x] `cargo build` green; worker scope green; committed per commit mode.

---

### Task 14: Gates, measurements, docs, ledger, verdict (lead)

**Files:**
- Create: `docs/findings/2026-09-DD-machine-service-phase3-gate.md`, `docs/plans/2026-09-10-machine-service-phase3-ledger.md`
- Modify: `src/service/status.rs` (per-checkout `graph_symbols`, `graph_load_millis`, `graph_resident_bytes`, `blob_count`, `facts_bytes`, `tantivy`, `vector_count`, `vector_scan_millis` from `StoreStatus`; design 10), `src/tools/workspace/commands/registry/status.rs` (same fields), `xtask/test_tiers.toml` and `xtask/src/manifest.rs:8` (`fast` redefined as unit-only buckets with `FAST_TIER_MAX_EXPECTED_SECONDS = 10`; `full` composition trimmed to the design's scope), `xtask/tests/support/manifest_contract_expected.rs`, `CLAUDE.md`, `AGENTS.md`, `docs/WORKSPACE_ARCHITECTURE.md`, `docs/SEARCH_FLOW.md`, `docs/ARCHITECTURE.md`, `docs/TESTING_GUIDE.md`, `docs/OPERATIONS.md`, `README.md`, `docs/plans/2026-09-09-machine-service-design.md` (section 14: record sequencing decisions 1, 2, 4, 6 under phases 3, 4, and 6)

**Interfaces:**
- Consumes: the finished tree at the final code commit.
- Produces: the gate verdict with every section 12 budget measured: durable roots (test), `fast` under 10 s warm, `full` under 120 s warm on Linux with the excluded buckets named, `service-process` under 20 s, complexity words, `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` before (`145,367` at `63cefbcf`) and after, clean build time (`cargo clean && time cargo build`), resident bytes and graph load time for the Julie repo and the Miller repo (`/home/murphy/source/miller`) from `/status` after `manage_workspace open`, facts and tantivy size per checkout, vector scan latency, and the deletion-list ledger.

**Contract inputs:** design 14 item 3 gate: "every budget in section 12 holds on the Julie and Miller repos, and the phase is net negative in lines. If not, stop and redesign before phase 4."

**File ownership:** as listed in the Parallel Execution Contract row for Task 14.

**Serialization required:** Yes

**Dependency reason:** Lead task; needs the final tree.

**What to build:** The evidence, and docs that describe facts, graph, and snapshots instead of `symbols.db`, projections, and repairs. If a budget fails, the finding says which and by how much, and the phase does not pass; the gate is not adjusted.

**Approach:** Run `cargo xtask test dev`, `system`, `bucket service-process`, `dogfood`, `full`, recording each in the ledger with the HEAD SHA. Time `fast` and `full` three times and report the median. Measure Miller by opening `/home/murphy/source/miller` in a temporary `JULIE_HOME` and reading `/status`. Then the docs pass and the finding.

**Acceptance criteria:**
- [x] The finding states the verdict and every section 12 measurement with its command.
- [x] `CLAUDE.md`, `AGENTS.md`, `docs/WORKSPACE_ARCHITECTURE.md`, `docs/SEARCH_FLOW.md` no longer describe `symbols.db`, `projection_states`, `canonical_revisions`, repairs, or embedding generations as product behavior (`rg -n` in the finding).
- [x] The ledger has rows for `dev`, `system`, `service-process`, `dogfood`, and `full` at the final HEAD, all `pass`.
- [x] `cargo xtask test dev` green at the final commit.

## Execution handoff

- `local_commit_authority: authorized — the user said "let's move to phase 3" after merging phase 2 locally; phase 1 and 2 execution committed per task under the same authority.`
- `push_authority: missing — no user instruction to push.`
- `pr_authority: missing — no user instruction to open a PR.`
- Reviewer choice: `none` unless the approval names one.
- Worktree: `/home/murphy/source/julie/.worktrees/machine-service`, branch `machine-service` (equal to `main` at `63cefbcf`).
- Dogfooding: every worker prompt carries `JULIE_HOME=/home/murphy/.julie-dogfood ./target/debug/julie-server --json --semantics off search|refs|symbols ... --workspace .` for navigation; a bug found while dogfooding is investigated, not worked around.
