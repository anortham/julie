# Codebase Cleanup Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Remove dead and test-only production code, reduce duplicate cleanup and path policy, and split the widest runtime/search/test surfaces into deeper modules with smaller caller obligations.

**Architecture:** Execute in staged cleanup tracks. Start with false-confidence deletions and no-op tests, then introduce typed request/path/error contracts before splitting high-blast-radius runtime, search, editing, and watcher modules.

**Tech Stack:** Rust, rmcp, Tantivy, SQLite/rusqlite, tree-sitter, cargo-nextest, Julie MCP tools.

**Architecture Quality:** High architecture impact. The primary issues are shallow or overly wide interfaces: `JulieServerHandler`, `ManageWorkspaceTool`, `SearchIndex`, `JulieWorkspace`, search orchestration, editing tools, and test harnesses expose too much implementation detail to callers and tests.

---

## Evidence Summary

Review inputs:
- Local evidence: `cargo check --all-targets` passed on `f92bead5`; `cargo clippy --all-targets` surfaced 454 `julie-extractors` warnings and 229 `julie` library warnings.
- Size evidence: 53 production/xtask files exceed 500 lines; 24 `src/tests` files exceed 1000 lines.
- Julie evidence: `manage_workspace stats` reported 6,916 files and 190,247 symbols.
- Subagent passes: dead/test-only review, architecture-boundary review, duplicate-policy review, and test-surface review.

High-confidence cleanup signals:
- `src/tracing/mod.rs` is production code used only by tests and contains mock/stub comments.
- `WorkspaceCommand` is defined but unused; live `manage_workspace` uses `operation: String` plus optional fields.
- `clear_cache_for_test` is a production `*_for_test` API with definition-only refs. Follow-up verification found `is_autocommit_for_test` and `loaded_workspace_file_watcher_running_for_test` still have real test callers, so they are not Phase 1 deletion candidates.
- Tests include no-op/stub modules that pass without asserting behavior.
- Search, workspace, editing, and daemon lifecycle code have repeated path/status/error/output policies.

## Architecture Quality

**Affected modules:** `src/handler.rs`, `src/tools/workspace/commands/**`, `src/search/index.rs`, `src/tools/search/**`, `src/workspace/mod.rs`, `src/tools/editing/**`, `src/watcher/**`, `src/daemon/database.rs`, `src/tests/**`.

**Caller-facing interface:** MCP tools, CLI subcommands, workspace registry rows, search results, editing tool results, test fixtures, and daemon/session lifecycle helpers.

**Depth/locality check:** Current high-risk modules are wide and shallow. Callers often know raw workspace status strings, path normalization rules, mutex-backed database/search internals, or tool-specific error classification. This spreads policy across tools and tests.

**Test surface:** Tests should move toward MCP tool calls, CLI calls, and typed helper APIs. Unit tests remain useful for pure policy modules such as path resolution, status parsing, search filtering, and edit preparation.

**Seams/adapters:** New seams are justified only where they reduce repeated caller obligations: typed manage-workspace requests, workspace path resolution, tool error mapping, edit preparation, search test-exclusion policy, and test harness helpers.

**Rejected shortcuts:** Do not split `SearchIndex`, `JulieServerHandler`, or watcher runtime first. They are high-blast-radius and need typed contracts/harness cleanup before large moves. Do not keep no-op tests as placeholders.

**Architecture risk:** high.

## Candidate Plan

### Candidate 1: Delete Or Quarantine `CrossLanguageTracer`

**Files:** `src/tracing/mod.rs`, `src/lib.rs`, `src/tests/core/tracing.rs`, `src/tests/integration/tracing.rs`.

**Current friction:** `CrossLanguageTracer` is production code with only test callers. `trace_data_flow` callers are in tracing tests only. The file contains comments such as “mock flow to make tests pass” and stub methods `find_direct_relationship` / `find_pattern_match`.

**Deletion test:** Removing `pub(crate) mod tracing` should not affect production if tests are migrated or removed.

**Proposed module/interface:** Delete `src/tracing/mod.rs` unless a real caller-facing tracing tool exists. If any behavior is still useful, re-express it through `TraceCallPathTool` / `call_path` with real symbol and relationship fixtures.

**Why this improves locality/leverage:** Removes a false production surface and forces tracing behavior through the tool agents actually use.

**Test surface:** `cargo nextest run --lib tracing`; replacement tests should call `call_path` or the current trace tool through handler/MCP.

**Risk:** medium.

**Recommendation:** first cleanup batch.

### Candidate 2: Remove No-Op And Copied-Pattern Tests

**Files:** `src/tests/tools/smart_read.rs`, `src/tests/regression_prevention_tests.rs`, `src/tests/main_error_handling.rs`, `src/tests/integration/search_regression_tests.rs`.

**Current friction:** `smart_read.rs` contains tests returning `Ok(())` with placeholder comments and no assertions. `regression_prevention_tests.rs` has an empty RED-phase test. `main_error_handling.rs` tests copied `EnvFilter` and `MockDatabase` patterns rather than current production behavior; `update_workspace_statistics` no longer exists in production. `search_regression_tests.rs` has commented ranking tests.

**Deletion test:** Deleting or rewriting these tests should not reduce real behavior coverage because they currently do not execute caller-facing behavior or have stale targets.

**Proposed module/interface:** Replace with behavior tests through `GetSymbolsTool`, `FastSearchTool`, `logging::install_file_tracing`, or remove when no current behavior remains.

**Why this improves locality/leverage:** Test output stops overstating coverage. Future coverage work targets real interfaces.

**Test surface:** Exact tests for the replacement behavior. If removing only, `cargo nextest run --lib regression_prevention_tests smart_read main_error_handling search_regression_tests` should show removed filters rather than passing stubs.

**Risk:** low.

**Recommendation:** first cleanup batch.

### Candidate 3: Prune Dead Test-Only Production APIs

**Files:** `src/workspace/mutation_gate.rs`, `src/database/mod.rs`, `src/handler.rs`.

**Current friction:** Verified definition-only ref:
- `clear_cache_for_test` at `src/workspace/mutation_gate.rs:162`.

Follow-up verification disproved the original broader claim:
- `is_autocommit_for_test` at `src/database/mod.rs:275` is called by `src/tests/daemon/symbol_db_pooled_test.rs`.
- `loaded_workspace_file_watcher_running_for_test` at `src/handler.rs:1291` is called by `src/tests/core/workspace_init.rs`.

**Deletion test:** Remove `clear_cache_for_test` and run `cargo check --all-targets`; no caller should break.

**Proposed module/interface:** Delete definition-only helpers. For used `*_for_test` helpers, consolidate behind test harness modules instead of adding more public production methods.

**Why this improves locality/leverage:** Reduces test-only holes in production interfaces.

**Test surface:** `cargo check --all-targets`, plus exact tests if any replacement helper is added.

**Risk:** low.

**Recommendation:** first cleanup batch.

### Candidate 4: Typed `manage_workspace` Request And Status Contract

**Files:** `src/tools/workspace/commands/mod.rs`, `src/cli_tools/commands.rs`, `src/daemon/database.rs`, `src/tools/workspace/commands/registry/**`.

**Current friction:** `WorkspaceCommand` exists but has only a definition ref. Live API uses `ManageWorkspaceTool { operation: String, ...optional fields... }`. Workspace statuses are raw strings such as `pending`, `indexing`, `ready`, and `error`, with string comparisons in cleanup and registration code.

**Deletion test:** If the typed enum were wired in, invalid argument combinations disappear from command handlers and CLI validation becomes a parser concern.

**Proposed module/interface:** Keep the public MCP JSON shape for compatibility, but parse it into an internal `ManageWorkspaceRequest` enum plus `WorkspaceStatus`. Put `targets_primary()`, required-argument validation, and daemon-only classification on the typed request.

**Why this improves locality/leverage:** Callers stop re-learning which optional fields are valid for which operation. Registry cleanup stops comparing raw status strings.

**Test surface:** JSON deserialization/validation tests and MCP `manage_workspace` calls for invalid op, missing args, register/index/open/refresh/remove/list/clean/health.

**Risk:** high.

**Recommendation:** first structural refactor before broad workspace cleanup.

### Candidate 5: Shared Workspace Path Resolver

**Files:** `src/tools/symbols/primary.rs`, `src/tools/symbols/target_workspace.rs`, `src/tools/refactoring/rename.rs`, `src/tools/workspace/commands/index.rs`.

**Current friction:** Absolute/relative/canonicalized/native/relative-Unix path handling is repeated with slightly different fallback behavior. Some paths fall back to raw input on conversion errors.

**Deletion test:** A shared resolver should remove duplicated path logic from get-symbols primary/target, rename scope, and index command path handling.

**Proposed module/interface:** `WorkspacePathResolver::resolve_file_input(input, root) -> ResolvedWorkspacePath { absolute_native, relative_unix, existed }`. Add explicit outside-workspace and missing-file behavior.

**Why this improves locality/leverage:** Tools stop owning path invariants individually.

**Test surface:** `get_symbols` primary and target absolute/relative paths; outside-workspace rejection; `rename_symbol(scope=...)` acceptance/rejection parity; index command root resolution.

**Risk:** high.

**Recommendation:** first structural refactor, after typed manage-workspace request.

### Candidate 6: Shared Workspace Target Error And Metrics Contract

**Files:** `src/tools/navigation/resolution.rs`, `src/handler/tools/*.rs`, `src/tools/navigation/call_path.rs`, `src/handler.rs`.

**Current friction:** Target workspace failures map differently by tool: wrappers use `internal_error`, substring-based `invalid_params`, success text diagnostics, or metric fallback. This makes behavior hard to reason about and hard to test consistently.

**Deletion test:** Tool wrappers should not need to inspect workspace error strings.

**Proposed module/interface:** Typed `WorkspaceParam` and `ToolErrorKind`, plus one `map_tool_error` adapter for handler wrappers. Metrics binding should receive a typed target resolution result.

**Why this improves locality/leverage:** Error classification and telemetry attribution become one policy.

**Test surface:** Invalid workspace id through `fast_refs`, `deep_dive`, `get_context`, `get_symbols`, `blast_radius`, and `call_path`; target-workspace metrics binding assertions.

**Risk:** high.

**Recommendation:** structural refactor after path resolver.

### Candidate 7: Search Test-Exclusion Policy

**Files:** `src/search/index.rs`, `src/tools/search/execution.rs`, `src/tools/search/line_mode.rs`.

**Current friction:** Lexical search excludes tests by path and role; semantic KNN uses `SearchFilter::matches_symbol_result`, which excludes by path only. Line-mode has separate candidate filtering.

**Deletion test:** One `SearchFilter::matches_test_policy(path, role)` should be used by lexical, semantic, hybrid, and line-mode paths.

**Proposed module/interface:** Centralize test exclusion policy in `SearchFilter` or a small `search::filtering` module.

**Why this improves locality/leverage:** Search backends stop diverging on inline test symbols in production paths.

**Test surface:** `fast_search(backend="semantic", exclude_tests=true)`, hybrid, and lexical fixtures with inline test-role symbols in production paths.

**Risk:** medium-high.

**Recommendation:** search cleanup batch; run dogfood if ranking/scoring shifts.

### Candidate 8: Integrate Line Matches Into Search Execution

**Files:** `src/tools/search/mod.rs`, `src/tools/search/line_mode.rs`, `src/tools/search/execution.rs`.

**Current friction:** Full output enriches unified hits through line-mode and swallows errors. Locations mode runs unified search, then conditionally re-runs line-mode to preserve old content-search behavior. Language/file/test filters exist in both paths.

**Deletion test:** `FastSearchTool::call_tool` should execute one search plan whose result includes optional line matches and explicit degradation metadata.

**Proposed module/interface:** Make line matches part of `SearchExecutionResult`, with one candidate filter and a trace field for line enrichment failure.

**Why this improves locality/leverage:** Removes additive old/new search paths and makes zero-hit/degraded behavior visible.

**Test surface:** `fast_search(return_format="locations")` content queries, path queries, file-pattern queries, language-filtered line results, and zero-hit trace assertions.

**Risk:** high.

**Recommendation:** after Candidate 7.

### Candidate 9: Prepare Editing Work Once

**Files:** `src/tools/editing/edit_file.rs`, `src/tools/editing/rewrite_symbol.rs`, `src/handler/tools/edit_file.rs`, `src/handler/tools/rewrite_symbol.rs`.

**Current friction:** `success_metrics_metadata` resolves paths, reads files, applies edits, and computes diffs; `call_tool` then repeats the same work. `rewrite_symbol` calls `prepare_rewrite` once for metrics and again for the actual call.

**Deletion test:** A request should have exactly one prepared edit/rewrite object per handler invocation.

**Proposed module/interface:** `PreparedEdit` and `PreparedRewrite` produced once by tool wrapper, then consumed by metrics and render/apply logic.

**Why this improves locality/leverage:** Removes duplicate file reads/parses and closes race windows between metrics preparation and commit.

**Test surface:** Dry-run/apply outputs unchanged; failure metrics fields preserved; race test where file changes between prepare and commit.

**Risk:** medium-high.

**Recommendation:** independent cleanup batch.

### Candidate 10: Shared Language Detection Policy

**Files:** `crates/julie-extractors/src/pipeline.rs`, `crates/julie-extractors/src/language_spec/specs.rs`, `src/tools/editing/rewrite_symbol.rs`, `src/tools/workspace/indexing/file_policy.rs`.

**Current friction:** Extractor source-aware detection can classify `.h` as C++; language specs map `.h` to C by default. `rewrite_symbol` uses extension-only detection. Indexing policy adds filename/fallback rules for Dockerfile, Makefile, TOML, JSON, and shell names.

**Deletion test:** Indexing, live rewrite parsing, watcher indexing, and external extraction should ask one policy for language/parser support.

**Proposed module/interface:** `LanguageDetectionPolicy::{Indexing, ParserRequired, SourceAware}` backed by `LanguageSpec`.

**Why this improves locality/leverage:** Language behavior stops drifting across tools.

**Test surface:** Index and rewrite C++ `.h`; Dockerfile/Makefile/Cargo.toml classification through indexing/watcher/external extract; unsupported-parser error messages.

**Risk:** high.

**Recommendation:** separate extractor/language batch.

### Candidate 11: Split Wide Runtime Modules

**Files:** `src/handler.rs`, `src/workspace/mod.rs`, `src/daemon/workspace_pool.rs`, `src/watcher/runtime.rs`, `src/watcher/mod.rs`, `src/daemon/database.rs`, `xtask/src/changed.rs`.

**Current friction:**
- `JulieServerHandler`: 2,565 LOC, 186 symbols, 121 direct refs to the handler symbol.
- `JulieWorkspace`: exposes DB/search/watcher/provider/config runtime fields.
- `SearchIndex`: 2,239 LOC, mixes Tantivy adapter, document shaping, query execution, compatibility marker, locking, and ranking helpers.
- `DaemonDatabase`: 1,336 LOC, mixes migrations, workspace registry, cleanup events, tool calls, search compare, codehealth, and id migration.
- Watcher runtime constructor has many shared-state parameters and mixes scheduling, dedupe, repair, retry, overflow, and gate handling.

**Deletion test:** Each split should move policy out of callers, not just move code between files.

**Proposed module/interface:**
- `ToolRuntime` facets: workspace resolver, store/search access, metrics, embeddings, spillover.
- `WorkspaceLayout`, `WorkspaceRuntime`, `WorkspaceHealthReporter`.
- `daemon/database/{migrations,workspaces,tool_calls,search_compare,codehealth}` behind `DaemonDatabase`.
- `WatcherContext`, `QueueScheduler`, `RepairCoordinator`, `ProjectionRetryQueue`.
- `changed::{collect,select,render}` plus declarative route rules.

**Why this improves locality/leverage:** These splits are valuable after typed contracts exist because callers can depend on narrower capabilities instead of raw runtime objects.

**Test surface:** Existing subsystem buckets: daemon DB, workspace init, watcher integration, xtask changed tests, and affected MCP tool tests.

**Risk:** high.

**Recommendation:** later structural batches, not first.

### Candidate 12: Test Harness And Fixture Cleanup

**Files:** `src/tests/**`, `src/tests/fixtures/**`, `fixtures/databases/julie-snapshot`.

**Current friction:** 24 test files exceed 1000 lines. MCP session helpers, temp workspace setup, raw `Symbol` literals, SQL row inserts, and text extraction repeat across daemon/search/workspace tests. `fixtures/databases/julie-snapshot/symbols.db` is about 96MB and includes generated artifacts.

**Deletion test:** Adding harness helpers should shrink repeated setup without making behavior harder to see.

**Proposed module/interface:**
- `src/tests/harness/mcp_session.rs`: session creation, initialized notification, roots request/response, tool call, text extraction.
- `src/tests/harness/db_fixture.rs`: `FixtureDb`, `FileBuilder`, `SymbolBuilder`, `RelationshipBuilder`, `IdentifierBuilder`.
- Split large tests by behavior only after helpers land.
- Convert ignored tests into deterministic exact tests or explicit manual diagnostics outside normal discovery.

**Why this improves locality/leverage:** Tests become shorter and assert caller-facing behavior instead of private state.

**Test surface:** Harness self-tests plus migrated exact tests; lead runs `cargo xtask test changed` after each coherent migration batch.

**Risk:** medium.

**Recommendation:** parallel with first cleanup batches, but do not migrate huge files before the helpers exist.

## Execution Order

1. **False-confidence cleanup:** Candidates 1, 2, and 3. Remove prototype/stub surfaces and no-op tests first so later coverage signals mean something.
2. **Workspace contracts:** Candidates 4, 5, and 6. Typed requests, status, paths, and errors reduce duplicated policy before wider workspace refactors.
3. **Focused behavior cleanup:** Candidates 7, 8, 9, and 10. Search policy, line-mode integration, editing prepare-once, and language detection are independent enough for separate workers.
4. **Harness cleanup:** Candidate 12. Add harness helpers early, then migrate large tests in behavior slices.
5. **Wide module splits:** Candidate 11. Split handler/workspace/search/watcher/daemon DB only after contracts and harnesses reduce the blast radius.

## Execution Progress

Completed on branch `cleanup/codebase-cleanup-2026-05-24`:

- **Candidate 1:** deleted prototype `CrossLanguageTracer` production module and its tests.
- **Candidate 2:** deleted stale/decorative test modules and removed their xtask routes.
- **Candidate 3:** removed only verified-unused `clear_cache_for_test`; retained the two helpers that still have real test callers.
- **Candidate 4:** replaced dead `WorkspaceCommand` with typed internal `ManageWorkspaceOperation` / `ManageWorkspaceRequest`, keeping public MCP JSON flat.
- **Candidate 5:** completed the safe first slice: shared tool file-input resolver for `get_symbols` primary/target and rename scope normalization. Workspace command root resolution remains intentionally separate.
- **Candidate 6:** completed the safe first slice: typed resolver-created workspace failures and `deep_dive` type-based invalid-params classification. Broad wrapper normalization and metrics attribution remain separate.
- **Candidate 7:** unified symbol-result test exclusion for semantic/hybrid KNN by honoring path OR metadata role `test`; line-mode behavior remains intentionally unchanged.
- **Candidate 8:** completed the first trace slice: locations-only content queries now replace unified symbol/file execution hits with line hits, refresh trace top hits/result counts, and preserve current locations text/fallback behavior. Lead review fixed exposed line-hit language metadata for non-Rust files.
- **Candidate 9:** prepared edit/rewrite work once per handler invocation via `PreparedEdit` / `PreparedRewrite`, with metrics and apply/render consuming the same prepared object. Lead review removed obsolete dead wrapper methods.
- **Candidate 10:** completed the first language slice: public source-aware extractor language detection, app-level content-aware indexing wrapper, and content-aware detection in rewrite, indexing-core, and watcher paths. Resolver same-language scoring remains path-only and is still a separate slice.
- **Candidate 12:** completed the first safe helper slice: shared MCP `CallToolResult` text extraction helper, migrated `get_symbols.rs`, `get_symbols_smart_read.rs`, and `search_context_lines.rs`.

Post-review fixes completed:

- **Candidate 8:** line-mode locations now use indexed language metadata instead of re-checking file extensions, so source-aware C++ `.h` headers survive `language="cpp"` filtering and still return content line hits.
- **Candidate 9:** public MCP `edit_file` validation now rejects empty `old_text` before file I/O on both direct and handler paths. Failed apply metrics now force `applied=false` for `edit_file` and `rewrite_symbol`.
- **Candidate 9:** `EditingTransaction` now serializes same-process writes by normalized target path. `commit_if_unchanged` re-checks after any previous Julie writer commits, and refactoring callers with original content now use checked commits instead of blind last-writer-wins writes.
- **Candidate 10:** `.h` source-aware detection no longer treats C identifiers named like C++ keywords as C++ syntax. The decision compares C and C++ parse diagnostics and keeps `.h` as C unless C++ parses more cleanly.
- **Candidate 10:** `rename_symbol` now uses source-aware language detection for live AST rewrites, fixing C++ `.h` parse refusal.
- **Candidate 10:** external extraction now has end-to-end C++ `.h` coverage when discovery groups a header as C but content proves C++.
- **Search API cleanup:** legacy `SearchIndex::search_content`, `SearchIndex::search_files`, `SearchDocument::symbol_from_parts`, `SearchDocument::file_from_parts`, and their content/file wrapper result types are now test-only where verified. The one non-test `xtask` caller was moved to `SearchDocument::for_file`.
- **Claude CLI verification follow-up:** a fresh Claude review approved the plan/fix slice and found only low-severity residuals. The real residual was closed by making `MultiFileTransaction` acquire the same per-path edit locks as `EditingTransaction`. The parser-init fallback is now logged before falling back to the conservative `::` token heuristic. The prepare-failure `applied=false` concern was already covered by base metadata and was pinned with handler assertions.
- **Candidate 8:** full-output and locations line enrichment now record `applied`, `no_matches`, or `failed` trace status, line match counts, no-match diagnostics, and failure text in handler telemetry. The previous silent fallback on line-mode errors is now visible in `SearchTrace`.
- **Candidate 8 review follow-up:** fresh Claude review approved the observability slice with low residuals. Follow-up fixed the actionable items: documented raw line-match count semantics, made snippet scope-rescue non-propagation explicit, and made failed enrichment clear stale companion fields with a focused trace test.
- **Candidate 10:** resolver same-language scoring now uses stored file-language metadata for caller files, falling back to extension detection only when metadata is absent. This closes the remaining source-aware `.h` gap where C++ headers were still treated as C during relationship resolution. Fresh Claude review approved the slice; follow-up documented the file-row-before-resolution dependency and pinned missing-row extension fallback with a test.
- **Candidate 12:** direct MCP session tests now share JSON-line helpers for sending responses, reading server messages, and answering `ListRootsRequest`. The slice removes duplicated roots-response blocks from `get_symbols`, `fast_search`, and workspace health/list tests, and folds touched local text-extraction duplicates into `tests::helpers::mcp::call_tool_result_text`.
- **Candidate 12 review follow-up:** fresh Claude review found no wire-format, deadlock, lifetime, or JSON-RPC regressions. The actionable cleanup residual was inconsistent text-extraction consolidation in touched files; `line_mode.rs` and `global_targeting.rs` now alias the shared helper instead of keeping local duplicate implementations.
- **Candidate 12:** added shared test DB row builders for `FileInfo`, `Symbol`, `Relationship`, and `Identifier` under `src/tests/helpers/db/`, split below the 500-line new-file limit, and migrated the first row-literal clusters in `external_extract::operations` and `blast_radius_determinism_tests`.
- **Candidate 12 review follow-up:** fresh Claude review first caught that untracked helper files were missing from the review diff. After intent-to-add exposed the helper source, the review approved the slice and verified migrated defaults preserve prior row semantics. A speculative-setter cleanup trimmed unused builder methods before the final dev fallback.
- **Candidate 11:** completed the first safe runtime split by moving daemon DB schema migrations from `src/daemon/database.rs` to `src/daemon/database/migrations.rs`. `DaemonDatabase::open` remains the only caller, and public daemon DB APIs stayed unchanged.
- **Candidate 12:** migrated the dashboard intelligence `FileInfo` fixture helper to the shared DB row builder, adding an explicit `symbol_count` setter to preserve the prior row shape. Raw SQL analytics edge cases stayed local and visible.
- **Candidate 11/12 review follow-up:** fresh Claude review approved the combined daemon migration split and dashboard helper slice with no findings after verifying migration parity and explicit dashboard fixture overrides.
- **Candidate 11:** split daemon DB search-compare persistence into `src/daemon/database/search_compare.rs`, moving the four search-compare methods and row/input types behind stable `database` module re-exports.
- **Candidate 11 review follow-up:** fresh Claude review approved the search-compare split with no findings, verifying SQL parity, transactional behavior, type re-exports, and private module access.
- **Candidate 12:** extended `SymbolBuilder` with parent ID, code context, and annotations setters, then migrated `early_warning_report_tests` file/symbol fixtures to shared DB builders while preserving metadata, parent, annotation, span, byte, and content fields.
- **Candidate 12 review follow-up:** fresh Claude review approved the early-warning fixture migration with no findings, verifying builder defaults and fixture field parity.
- **Candidate 11:** split daemon DB tool-call persistence into `src/daemon/database/tool_calls.rs`, moving insert/history/success-rate/prune/search-analysis methods and `SearchToolCallRow` behind a stable `database` module re-export.
- **Candidate 11 review follow-up:** fresh Claude review approved the tool-call split with no findings, verifying SQL parity, timestamp/cutoff behavior, re-export stability, and private module access.
- **Candidate 11:** split daemon DB codehealth snapshot persistence into `src/daemon/database/codehealth.rs`, moving snapshot insert/read/history/materialization methods and `CodehealthSnapshot` row types behind stable `database` module re-exports. `migrate_workspace_ids` remains in the root daemon DB module because it still coordinates multiple tables.
- **Candidate 11 review follow-up:** fresh Claude review approved the codehealth split with no findings, verifying `now_unix`/private field visibility, stable re-export paths, SQL parity, lock-ordering docs, and the decision to leave workspace-ID migration in the root module.
- **Candidate 12:** migrated `test_quality_tests` pipeline setup clusters to shared DB row builders and storage APIs for metadata update, identifier evidence, empty evidence config fallback, no-match fallback, and no-substring matching tests. The slice preserves behavior through `compute_test_quality_metrics`; builder-backed symbols now use canonical `Symbol` storage defaults instead of raw SQL omitted nullable span columns.
- **Candidate 12 review follow-up:** fresh Claude review approved the test-quality fixture migration with no findings, verifying the span/default deltas are inert for `compute_test_quality_metrics`, identifier containment survives `bulk_store_identifiers`, and scope stayed inside the selected setup clusters.
- **Candidate 12:** migrated the first `test_linkage_tests` setup cluster to shared DB row builders and storage APIs for relationship-based linkage and identifier-only linkage. Later linkage edge-case raw SQL remains local. Lead review removed decorative `reference_score` updates after verifying `compute_test_linkage` does not read that column.
- **Candidate 12 review follow-up:** fresh Claude review approved the first linkage fixture migration with no findings, verifying `reference_score` removal is safe, builder defaults match the raw rows for linkage behavior, and later raw SQL edge cases remain untouched.
- **Candidate 11:** split daemon DB workspace registry storage into `src/daemon/database/workspaces.rs`, moving workspace CRUD/stat/session/cleanup-event methods and workspace row types behind stable `database` module re-exports. `normalize_workspace_paths` and `migrate_workspace_ids` remain in the root daemon DB module because they are startup/migration repair paths.
- **Candidate 11 review follow-up:** fresh Claude review approved the workspace registry split with no findings, verifying re-export stability, private parent access from the submodule, method/SQL parity, and the decision to keep startup/migration repair in the root module.
- **Candidate 12:** migrated the remaining ordinary `test_quality_tests` pipeline setup clusters for no-body, metadata preservation, and fixture-not-applicable cases to shared DB row builders and storage APIs. Lead cleanup removed redundant builder default setters from adjacent migrated tests; persisted metadata assertions remain raw SQL by design.
- **Candidate 12 review follow-up:** fresh Claude review approved the remaining test-quality fixture migration with no findings. The first schema run exhausted turns and the second semantic review used malformed keys, so the final strict no-tool rerun produced schema-valid approval.
- **Candidate 12:** migrated the next `test_linkage_tests` edge-case setup cluster for uncovered production symbols and test-to-test relationship exclusion to shared DB row builders and storage APIs. Symbol confidence is set explicitly where raw SQL previously relied on the schema default; relationship default setters were trimmed after lead review.
- **Candidate 12 review follow-up:** fresh Claude review approved the linkage edge-case fixture migration with no actionable findings. A semantic review first returned informational notes in the findings array, so the final strict rerun recorded schema-valid approval.
- **Candidate 12:** migrated the `test_linkage_tests` linked-tests cap setup loop to shared DB row builders and storage APIs. New symbols preserve the prior schema-default confidence, and relationships rely on builder defaults for calls/confidence while keeping file path and line numbers explicit.
- **Candidate 12 review follow-up:** fresh Claude review approved the linkage cap fixture migration with no findings. The first review returned only a partial JSON object, so the final strict rerun recorded schema-valid approval.
- **Candidate 12:** migrated the `test_linkage_tests` class-name similarity fallback setup cluster to shared DB row builders and storage APIs. The slice preserves method/csharp/visibility/test metadata/identifier targeting shape; decorative `reference_score` values were not carried over because `compute_test_linkage` does not read them for this path.
- **Candidate 12 review follow-up:** fresh Claude review approved the class-name similarity fixture migration with no findings. The first review returned only a partial JSON object, so the final strict rerun recorded schema-valid approval.
- **Candidate 12:** migrated the `test_linkage_tests` language-scoped ambiguity guard setup cluster to shared DB row builders and storage APIs. The slice preserves rust/python language partitioning, test metadata, visibility, identifier shape, and symbol confidence while leaving output assertion SQL explicit.
- **Candidate 12 review follow-up:** fresh Claude review approved the ambiguity-guard fixture migration with no findings. The first review returned only a partial JSON object, so the final strict rerun recorded schema-valid approval.
- **Candidate 12:** migrated the `test_linkage_tests` class-inherits-method-linkage setup cluster to shared DB row builders and storage APIs. The slice preserves class/method parentage, csharp visibility, test metadata, and relationship shape while leaving output assertion SQL explicit.
- **Candidate 12 review follow-up:** fresh Claude review approved the parent-inheritance fixture migration with no findings. The first review exhausted the turn cap, so the final strict rerun recorded schema-valid approval.
- **Candidate 12:** migrated the `test_linkage_tests` deduplication-across-strategies identifier fixture from raw SQL to the shared identifier builder and storage API. The slice preserves the duplicate `test_1` to `prod_1` identifier edge and leaves output assertions explicit.
- **Candidate 12 review follow-up:** fresh Claude review approved the deduplication fixture migration with no findings, verifying builder row semantics, `bulk_store_identifiers` usage, plan wording, and verification evidence.
- **Candidate 12:** split the late linkage edge cases into `src/tests/analysis/linkage_edge_cases_tests.rs` before larger fixture migrations. `test_linkage_tests.rs` is now 584 lines and the new edge-case file is 407 lines, keeping both below the 1000-line test-file cap.
- **Candidate 12 review follow-up:** fresh Claude review approved the linkage edge-case file split with no findings, verifying test discovery, moved-body parity, git visibility, line counts, and remaining raw SQL plan accuracy.
- **Candidate 12:** migrated the `linkage_edge_cases_tests` tied-score fallback setup cluster to shared symbol/identifier builders and storage APIs. The slice preserves C# method symbols, test metadata, identifier fallback targeting, and deterministic output assertions while leaving the metadata reset SQL explicit.
- **Candidate 12 review follow-up:** fresh Claude review approved the tied-score fallback fixture migration with no findings, verifying row semantics, identifier NULL-target fallback, plan accuracy, and verification evidence.
- **Candidate 12:** migrated the `linkage_edge_cases_tests` stale-cleanup setup cluster to shared symbol/relationship builders and storage APIs. The slice preserves parent/child/test relationships and leaves the stale-linkage `DELETE FROM relationships` behavior SQL explicit.
- **Candidate 12 review follow-up:** fresh Claude review approved the stale-cleanup fixture migration with no findings, verifying parent/child/test row semantics, relationship shape, and ledger accuracy.
- **Candidate 12:** migrated the `linkage_edge_cases_tests` scorable-filter setup cluster to shared symbol/relationship builders and storage APIs. The slice preserves test-case, fixture-setup, and legacy-test metadata roles while leaving caller-facing assertions unchanged.
- **Candidate 12 review follow-up:** fresh Claude review approved the scorable-filter fixture migration with no findings, verifying metadata roles, relationship file/line shape, ledger accuracy, and remaining raw SQL list.
- **Candidate 12:** migrated the `linkage_edge_cases_tests` direct best-confidence setup cluster to shared symbol/relationship builders and storage APIs. The slice preserves test-quality confidence metadata, production/test visibility, and relationship file/line shape while leaving output assertion SQL explicit.
- **Candidate 12 review follow-up:** fresh Claude review approved the direct best-confidence fixture migration with no findings, verifying row shape, metadata preservation, intentional `reference_score` removal, plan wording, and verification evidence.

Remaining candidates:

- **Candidate 8:** dashboard preview rendering was re-verified as already implemented and tested. The remaining structural slice is deeper integration of line matches into the unified search execution plan so MCP/dashboard callers do not need post-execution line-mode reruns. Do not mix this with ranking changes.
- **Candidate 10:** completed. Continue to reject path-only `.h` assumptions in new language-detection or relationship-resolution code.
- **Candidate 11:** daemon DB root is now below the 500-line implementation target. Broad handler/workspace/search/watcher/workspace-pool splits still wait on stronger contracts and harnesses. Avoid moving `migrate_workspace_ids` until table-specific modules exist and its cross-table transaction can stay obvious.
- **Candidate 12:** remaining slices are analysis/local SQL fixture helpers and targeted migrations in `linkage_edge_cases_tests`. `test_quality_tests` pipeline setup is now builder-backed except for persisted metadata assertion queries that should stay explicit. The late linkage edge cases are split below the test-file cap, but still have raw SQL setup for parent aggregation and default-confidence behavior; migrate only one reviewed cluster at a time. Do not consolidate daemon roots, adapter duplex tests, schema/migration raw SQL, or analytics SQL edge cases without per-cluster review.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, and `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** Exact test by name: `cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`.

**Worker ceiling:** Workers may run exact tests only. The lead owns `cargo xtask test changed`, specialist tiers, and `cargo xtask test dev`.

**Worker gate invariant:** Each worker must state the behavior their exact test proves. Deletion-only workers must state the deleted symbol/file and prove no remaining caller through `cargo check --all-targets` or exact compile/test scope assigned by the lead.

**Lead affected-change scope:** `cargo xtask test changed` after each coherent batch.

**Branch gate:** `cargo xtask test dev` before handoff. Add specialist gates below when triggered.

**Replay/metric evidence:** Search ranking, zero-hit, and dogfood metrics are hard gates only when the candidate changes search/scoring/tokenization behavior. Report-only metrics must be labeled as report-only.

**Escalation triggers:**
- Search/scoring/tokenization changes: add `cargo xtask test dogfood`.
- Startup/workspace/system behavior: add `cargo xtask test system`.
- Daemon/watcher/lifecycle/restart behavior: add `cargo xtask test reliability`.
- Extractor/language-detection changes: add `cargo xtask test bucket extractors` and `cargo xtask certify tree-sitter --check`; add parser-upgrade bucket if parser dependencies move.
- Xtask routing changes: run the exact xtask tests first, then `cargo xtask test changed`.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless the task explicitly changes that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, timestamp, and evidence reuse.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** Cleanup sequencing, architecture decisions, candidate triage, contract shape.
- Harness mapping: use `RAZORBACK.md`; Codex route is `gpt-5.5` medium/high when selectable.

**Implementation tier:** Bounded local edits with clear tests, such as deleting unused helpers or adding a pure resolver.
- Harness mapping: use `RAZORBACK.md`; Codex route is `gpt-5.5` low/medium when selectable.

**Mechanical tier:** Formatting, docs-only updates, fixture metadata with no gate ownership.
- Harness mapping: use `RAZORBACK.md`; Codex route is `gpt-5.4-mini` low/medium when selectable.

**Gate-interpretation reviewer:** Search behavior, daemon lifecycle, workspace cleanup safety, public MCP/CLI compatibility, and failed exact tests.
- Harness mapping: use `RAZORBACK.md`; Codex route is `gpt-5.5` high when selectable.

**Escalation tier:** Search ranking, watcher/daemon concurrency, language detection across extractors, shared lifecycle, and public protocol compatibility.
- Harness mapping: use `RAZORBACK.md`; Codex route is `gpt-5.5` high/xhigh when selectable.

**Worker eligibility:** Use workers when file ownership is narrow, tests are exact, and no shared invariant needs interpretation.

**Escalation triggers:** Public MCP/CLI contract changes, lifecycle/concurrency, search ranking, language detection, failed-worker diagnosis, or weak/missing tests.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates.

**Unsupported harness behavior:** If the harness cannot choose per-agent model/reasoning, use `inherit` and state that in the worker report.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Current branch type-checks before planning cleanup | `cargo check --all-targets` | planning-baseline | f92bead5 | pass | 2026-05-24T15:44:19Z | no |
| Clippy signal collected for cleanup inventory; warnings are report evidence, not an acceptance gate | `cargo clippy --all-targets --message-format short` | planning-inventory | f92bead5 | report-only: 454 julie-extractors lib warnings; 229 julie lib warnings; 386 julie lib-test warnings including duplicates | 2026-05-24T15:44:19Z | no |
| Candidate 9 formatting preserved | `cargo fmt --check` | affected-change | 97ab2ea98608 | pass | 2026-05-24T18:18:00Z | no |
| Candidate 9 type-checks after prepare-once editing changes | `cargo check --all-targets` | affected-change | 97ab2ea98608 | pass | 2026-05-24T18:18:00Z | no |
| Candidate 9 edit-file prepared flow preserves editing behavior and stale-target protection | `cargo xtask test bucket tools-editing` | affected-change | 97ab2ea98608 | pass: tools-editing bucket | 2026-05-24T18:16:00Z | no |
| Candidate 9 handler metadata still records editing/telemetry contracts | `cargo xtask test bucket core-handler-telemetry` | affected-change | 97ab2ea98608 | pass: core-handler-telemetry bucket | 2026-05-24T18:17:00Z | no |
| Candidate 12 helper extraction compiles and preserves formatting | `cargo check`; `cargo fmt --check` | affected-change | 2e81be351214 | pass | 2026-05-24T18:25:00Z | no |
| Candidate 12 migrated get_symbols helpers preserve behavior | `cargo xtask test bucket tools-get-symbols` | affected-change | 2e81be351214 | pass: tools-get-symbols bucket | 2026-05-24T18:23:00Z | no |
| Candidate 12 migrated search-context helper preserves behavior | `cargo xtask test bucket tools-search-context` | affected-change | 2e81be351214 | pass: tools-search-context bucket | 2026-05-24T18:24:00Z | no |
| Candidate 8 locations line-hit trace preserves line-mode locations and search formatting | `cargo xtask test bucket tools-search-line`; `cargo xtask test bucket tools-search-format-quality` | affected-change | 7287ce35ef31 | pass: tools-search-line and tools-search-format-quality buckets | 2026-05-24T18:43:00Z | no |
| Candidate 8 file/path and semantic locations stay unified-only | `cargo xtask test bucket tools-search-file-mode`; `cargo xtask test bucket tools-search-hybrid` | affected-change | 7287ce35ef31 | pass: tools-search-file-mode and tools-search-hybrid buckets | 2026-05-24T18:44:00Z | no |
| Candidate 10 source-aware extractor language detection preserves extractor contract | `cargo xtask test bucket extractors` | expensive-specialist | 071bc8ccfebb | pass: extractors bucket | 2026-05-24T18:52:00Z | no |
| Candidate 10 content-aware indexing/watcher changes preserve system behavior | `cargo xtask test system` | expensive-specialist | 071bc8ccfebb | pass: system tier | 2026-05-24T18:55:00Z | no |
| Post-review edit validation/metrics regressions are fixed through the public MCP handler path | `cargo nextest run --lib tests::core::handler::test_edit_file_empty_old_text_validation_precedes_file_io`; `cargo nextest run --lib tests::core::handler::test_edit_file_failed_apply_metrics_record_applied_false`; `cargo nextest run --lib tests::core::handler::test_rewrite_symbol_failed_apply_metrics_record_applied_false` | worker-red-green | working-tree on 98e075a0 | pass | 2026-05-24T19:25:27Z | no |
| Same-process checked edit commits serialize per file and preserve stale-target rejection | `cargo nextest run --lib test_single_file_commit_if_unchanged_serializes_julie_writers`; `cargo nextest run --lib test_single_file_transaction_rejects_changed_target_before_commit`; `cargo nextest run --lib test_edit_file_apply_rejects_changed_target_before_commit`; `cargo nextest run --lib test_prepared_rewrite_rejects_changed_target_before_commit` | worker-red-green | working-tree on 98e075a0 | pass | 2026-05-24T19:25:27Z | no |
| Source-aware `.h` language fixes preserve C headers, C++ header rename, external extraction, and indexed language line hits | `cargo nextest run -p julie-extractors --lib test_detect_language_for_source_preserves_c_headers_with_cpp_keyword_identifiers`; `cargo nextest run --lib tests::tools::refactoring::rename_symbol::test_rename_symbol_cpp_h_header_uses_source_aware_parser`; `cargo nextest run --lib tests::tools::search::fast_search_regression_tests::content_locations_cpp_h_language_filter_keeps_line_hits`; `cargo nextest run --lib extract_scan_routes_cpp_h_header_through_source_aware_detection`; `cargo nextest run --lib test_extract_files_for_indexing_records_cpp_language_for_cpp_h_header_grouped_as_c` | worker-red-green | working-tree on 98e075a0 | pass | 2026-05-24T19:25:27Z | no |
| Test-only search API cleanup preserves xtask build and direct wrapper/constructor coverage | `cargo check -p xtask`; `cargo nextest run --lib test_search_files_prefers_exact_basename_over_fragment_matches`; `cargo nextest run --lib three_tokens_no_single_file_contains_all_falls_back_to_or`; `cargo nextest run --lib test_add_file_content_and_search`; `cargo nextest run --lib tantivy_indexed_qualified_name_found_by_partial_token`; `cargo nextest run --lib test_incremental_indexing_preserves_tantivy_file_content` | affected-change | working-tree on 98e075a0 | pass | 2026-05-24T19:25:27Z | no |
| Post-review editing/refactoring handler/search/language affected buckets pass | `cargo xtask test bucket tools-editing`; `cargo xtask test bucket tools-refactoring`; `cargo xtask test bucket tools-search-line`; `cargo xtask test bucket tools-search-file-mode`; `cargo xtask test bucket tools-search-format-quality`; `cargo xtask test bucket tools-search-tantivy`; `cargo xtask test bucket tools-search-unified`; `cargo xtask test bucket tools-workspace`; `cargo xtask test bucket extractors`; `cargo nextest run --lib tests::external_extract -- --skip search_quality`; `cargo xtask test bucket core-fast`; `cargo xtask test bucket core-handler-telemetry` | affected-change | working-tree on 98e075a0 | pass; external_extract included one nextest LEAK report but zero failures | 2026-05-24T19:25:27Z | no |
| Full affected diff passes dev fallback and dogfood search-quality after post-review fixes | `cargo fmt --check`; `cargo xtask test changed` | affected-change + dogfood | working-tree on 98e075a0 | pass: changed fell back to 36 buckets including dev and search-quality; 675.8s total | 2026-05-24T19:37:38Z | no |
| Claude CLI independently verifies plan and post-review fix slice; low real residuals are addressed | `claude -p --no-session-persistence --dangerously-skip-permissions --tools "Read,Bash" --model opus --effort high --max-turns 40 --max-budget-usd 10.00 ...` | gate-review | 5a4e3b1c | approve with low residuals: one fixed, one made observable, one verified already covered | 2026-05-24T19:48:34Z | no |
| Claude follow-up fixes preserve multi-file edit serialization, handler metadata, and extractor behavior | `cargo check --all-targets`; `cargo nextest run --lib test_multi_file_transaction_serializes_with_single_file_checked_writer`; `cargo nextest run --lib tests::core::handler::test_edit_file_empty_old_text_validation_precedes_file_io`; `cargo nextest run --lib tests::core::handler::test_rewrite_symbol_metrics_include_symbol_span_and_failure_kind`; `cargo nextest run -p julie-extractors --lib test_detect_language_for_source_preserves_c_headers_with_cpp_keyword_identifiers`; `cargo xtask test bucket tools-editing`; `cargo xtask test bucket core-fast`; `cargo xtask test bucket extractors` | affected-change | working-tree on 5a4e3b1c | pass; an initial typo filter `test_rewrite_symbol_metrics_include_input_and_rewrite_outcome` ran zero tests and was replaced by the correct exact test | 2026-05-24T19:48:34Z | no |
| Claude follow-up affected diff passes selected buckets | `cargo fmt --check`; `cargo xtask test changed` | affected-change | working-tree on 5a4e3b1c | pass: changed selected extractors, tools-editing, core-fast; 63.4s total | 2026-05-24T19:50:18Z | no |
| Candidate 8 line enrichment trace status is recorded for success, no-match, locations, and telemetry paths | `cargo nextest run --lib content_full_output_trace_records_line_enrichment_success`; `cargo nextest run --lib content_full_output_trace_records_line_enrichment_no_matches`; `cargo nextest run --lib content_locations_trace_uses_line_hits_without_matching_line_text`; `cargo nextest run --lib test_fast_search_metadata_serializes_line_enrichment_fields` | worker-red-green | working-tree on a96bbac19f62 | pass | 2026-05-24T20:10:44Z | no |
| Candidate 8 line enrichment observability preserves typecheck, formatting, search formatting, line-mode, and telemetry buckets | `cargo check --all-targets`; `cargo fmt --check`; `cargo xtask test bucket tools-search-format-quality`; `cargo xtask test bucket tools-search-line`; `cargo xtask test bucket core-handler-telemetry` | affected-change | working-tree on a96bbac19f62 | pass | 2026-05-24T20:10:44Z | no |
| Candidate 8 full changed diff preserves all affected search and telemetry buckets | `cargo xtask test changed` | affected-change | working-tree on a96bbac19f62 | pass: changed selected 12 buckets; 177.5s total | 2026-05-24T20:14:31Z | no |
| Claude CLI reviews Candidate 8 line-enrichment observability slice | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on a96bbac19f62 | approve with low residuals; F1-F3 addressed, F4-F5 classified as low/non-blocking after local verification | 2026-05-24T20:24:28Z | no |
| Candidate 8 review follow-up preserves trace enum/status behavior and affected search buckets | `cargo nextest run --lib record_line_enrichment_failed_clears_stale_companion_fields`; `cargo nextest run --lib line_enrichment_status_serializes_snake_case`; `cargo check --all-targets`; `cargo fmt --check`; `cargo xtask test bucket tools-search-promotion`; `cargo xtask test changed` | affected-change | working-tree on a96bbac19f62 | pass: changed selected 12 buckets; 178.5s total | 2026-05-24T20:24:28Z | no |
| Candidate 10 resolver uses stored caller language metadata and DB lookup returns requested file languages | `cargo nextest run --lib test_get_file_languages_by_paths_returns_requested_languages`; `cargo nextest run --lib test_resolve_batch_prefers_content_aware_cpp_h_caller_language` | worker-red-green | working-tree on 55e81df81561 | pass | 2026-05-24T20:44:40Z | no |
| Candidate 10 resolver language scoring preserves database/workspace/init buckets | `cargo check --all-targets`; `cargo fmt --check`; `cargo xtask test bucket core-database`; `cargo xtask test bucket tools-workspace`; `cargo xtask test bucket tools-workspace-targeting`; `cargo xtask test bucket workspace-init` | affected-change | working-tree on 55e81df81561 | pass | 2026-05-24T20:44:40Z | no |
| Candidate 10 full changed diff and dogfood search quality pass after resolver scoring change | `cargo xtask test changed`; `cargo xtask test dogfood` | affected-change + dogfood | working-tree on 55e81df81561 | pass: changed fell back to dev, 35 buckets, 516.8s; dogfood 2 buckets, 226.7s | 2026-05-24T20:44:40Z | no |
| Claude CLI reviews Candidate 10 resolver language-scoring slice | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on 55e81df81561 | approve; residuals addressed with dependency comment and missing-row fallback test | 2026-05-24T21:01:59Z | no |
| Candidate 10 review follow-up preserves fallback behavior and final affected/dogfood gates | `cargo nextest run --lib test_resolve_batch_falls_back_to_extension_language_when_caller_file_missing`; `cargo check --all-targets`; `cargo fmt --check`; `cargo xtask test changed`; `cargo xtask test dogfood` | affected-change + dogfood | working-tree on 55e81df81561 | pass: changed fell back to dev, 35 buckets, 530.4s; dogfood 2 buckets, 227.7s | 2026-05-24T21:01:59Z | no |
| Candidate 12 direct MCP roots helper preserves migrated get_symbols/search/workspace session behavior | `cargo nextest run --lib test_get_symbols_primary_wrapper_resolves_roots_before_reading`; `cargo nextest run --lib test_manage_workspace_list_triggers_roots_resolution_when_primary_missing`; `cargo nextest run --lib test_fast_search_primary_wrapper_resolves_roots_before_searching`; `cargo nextest run --lib test_manage_workspace_health_triggers_roots_resolution_when_primary_missing` | worker-red-green | working-tree on cbf1cbd26ac1 | pass | 2026-05-24T21:38:21Z | no |
| Candidate 12 helper cleanup preserves typecheck, formatting, and affected direct-session buckets | `cargo check --all-targets`; `cargo fmt --check`; `cargo xtask test bucket tools-get-symbols`; `cargo xtask test bucket tools-workspace-targeting`; `cargo xtask test bucket tools-search-line`; `cargo xtask test bucket tools-workspace` | affected-change | working-tree on cbf1cbd26ac1 | pass; post-review reran tools-workspace-targeting and tools-search-line after text-extraction consolidation | 2026-05-24T21:38:21Z | no |
| Claude CLI reviews Candidate 12 direct MCP helper slice | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high --max-turns 1 ...` | gate-review | working-tree on cbf1cbd26ac1 | reviewed clean for wire format, deadlocks, lifetimes, and JSON-RPC behavior; actionable text-extraction consolidation residual addressed | 2026-05-24T21:38:21Z | no |
| Candidate 12 final changed diff passes dev fallback after review follow-up | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test changed` | affected-change | working-tree on cbf1cbd26ac1 | pass: changed fell back to dev, 34 buckets, 512.3s total | 2026-05-24T21:38:21Z | no |
| Candidate 12 DB row builders preserve default row semantics and migrated fixture behavior | `cargo nextest run --lib test_file_info_builder_sets_stable_defaults`; `cargo nextest run --lib test_symbol_builder_overrides_metadata_and_span`; `cargo nextest run --lib test_relationship_and_identifier_builders_cover_common_reference_rows`; `cargo nextest run --lib extract_force_rebuild_is_atomic_after_extraction_success`; `cargo nextest run --lib extract_delete_clears_cross_file_identifier_targets`; `cargo nextest run --lib extract_bulk_insert_nulls_dangling_parent_id`; `cargo nextest run --lib test_blast_radius_surfaces_identifier_only_callers`; `cargo nextest run --lib test_walk_impacts_traverses_extends_relationships`; `cargo nextest run --lib test_walk_impacts_identifier_edges_choose_strongest_kind_without_replacing_relationships`; `cargo nextest run --lib test_walk_impacts_caps_identifier_fanout_for_common_names`; `cargo nextest run --lib test_blast_radius_is_deterministic_across_repeated_calls` | worker-red-green | working-tree on f0bfb4372967 | pass | 2026-05-24T22:13:11Z | no |
| Candidate 12 DB row builder migration preserves external extraction and blast-radius affected areas | `cargo fmt --check`; `cargo check --all-targets`; `cargo nextest run --lib tests::external_extract -- --skip search_quality`; `cargo xtask test bucket tools-blast-spillover` | affected-change | working-tree on f0bfb4372967 | pass: external_extract 43 passed with 3 leaky reports and zero failures; tools-blast-spillover passed | 2026-05-24T22:13:11Z | no |
| Claude CLI reviews Candidate 12 DB row builder slice with helper source included | `git add -N src/tests/helpers/db/mod.rs src/tests/helpers/db/rows.rs src/tests/helpers/db/tests.rs`; `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high --max-turns 1 ...` | gate-review | working-tree on f0bfb4372967 | approve after verifying builder defaults and migrated row parity; earlier review without untracked helper files was treated as incomplete | 2026-05-24T22:13:11Z | no |
| Candidate 12 DB row builder final diff passes dev fallback after review trim | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test changed` | affected-change | working-tree on f0bfb4372967 | pass: changed fell back to dev, 34 buckets, 504.1s total | 2026-05-24T22:13:11Z | no |
| Candidate 11 daemon migration split preserves daemon DB migration behavior | `cargo nextest run --lib test_daemon_db_create_and_migrate`; `cargo nextest run --lib test_migration_003_drops_legacy_pairings_and_preserves_workspaces`; `cargo nextest run --lib test_daemon_db_idempotent_open`; `cargo nextest run --lib tests::daemon::database -- --skip search_quality` | affected-change | working-tree on 1abd537fe101 | pass | 2026-05-24T22:41:31Z | no |
| Candidate 12 dashboard FileInfo builder migration preserves fixture defaults and dashboard intelligence behavior | `cargo nextest run --lib test_file_info_builder_sets_stable_defaults`; `cargo nextest run --lib test_file_info_builder_overrides_symbol_count`; `cargo nextest run --lib test_get_file_hotspots_returns_ordered_by_composite_score`; `cargo nextest run --lib test_get_aggregate_stats_counts_files_and_symbols`; `cargo nextest run --lib tests::dashboard::intelligence -- --skip search_quality` | worker-red-green | working-tree on 1abd537fe101 | pass; worker first verified RED for missing `symbol_count` setter | 2026-05-24T22:41:31Z | no |
| Candidate 11/12 combined split/helper diff preserves formatting, typecheck, daemon/dashboard buckets, and affected dev fallback | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket daemon`; `cargo xtask test bucket dashboard`; `cargo xtask test changed` | affected-change | working-tree on 1abd537fe101 | pass: changed fell back to dev, 34 buckets, 523.5s total | 2026-05-24T22:41:31Z | no |
| Claude CLI reviews Candidate 11 daemon migration split and Candidate 12 dashboard helper slice | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on 1abd537fe101 | approve with no findings; first response approved but missed schema keys, strict rerun returned schema-valid approval | 2026-05-24T22:41:31Z | no |
| Candidate 11 search-compare persistence split preserves daemon DB and dashboard callers | `cargo nextest run --lib test_insert_and_list_search_compare_runs_and_cases`; `cargo nextest run --lib test_all_dashboard_pages_return_200`; `cargo nextest run --lib tests::daemon::database -- --skip search_quality` | affected-change | working-tree on 63bc30b00759 | pass | 2026-05-24T22:53:50Z | no |
| Candidate 11 search-compare split preserves formatting, typecheck, daemon/dashboard buckets, and changed selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket daemon`; `cargo xtask test bucket dashboard`; `cargo xtask test changed` | affected-change | working-tree on 63bc30b00759 | pass: changed selected daemon only, 61.4s total | 2026-05-24T22:53:50Z | no |
| Claude CLI reviews Candidate 11 search-compare split | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on 63bc30b00759 | approve with no findings; optional changed gate was already run and passed | 2026-05-24T22:53:50Z | no |
| Candidate 12 early-warning fixture migration preserves builder setters and analysis behavior | `cargo nextest run --lib test_symbol_builder_overrides_parent_context_and_annotations`; `cargo nextest run --lib test_symbol_builder_overrides_metadata_and_span`; `cargo nextest run --lib tests::helpers::db::tests`; `cargo nextest run --lib tests::analysis::early_warning_report_tests -- --skip search_quality` | worker-red-green | working-tree on ee1dfadd8146 | pass; first run verified RED for missing `parent_id` setter | 2026-05-24T23:07:27Z | no |
| Candidate 12 early-warning fixture migration preserves formatting, typecheck, analysis bucket, and affected dev fallback | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on ee1dfadd8146 | pass: changed fell back to dev, 34 buckets, 516.0s total | 2026-05-24T23:07:27Z | no |
| Claude CLI reviews Candidate 12 early-warning fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on ee1dfadd8146 | approve with no findings; suggested full helper DB module check was run and passed | 2026-05-24T23:07:27Z | no |
| Candidate 11 tool-call persistence split preserves daemon DB, metrics, dashboard, and search-analysis callers | `cargo nextest run --lib test_insert_and_query_tool_calls`; `cargo nextest run --lib test_insert_tool_call_persists_metadata_json`; `cargo nextest run --lib test_prune_old_tool_calls`; `cargo nextest run --lib tests::tools::metrics::tool_calls_db_tests -- --skip search_quality`; `cargo nextest run --lib test_metrics_page_renders_aggregated_tool_history`; `cargo nextest run --lib test_metrics_page_counts_failed_tool_calls_in_success_rate`; `cargo nextest run --lib test_query_metrics_formats_null_input_bytes`; `cargo nextest run --lib tests::dashboard::search_analysis -- --skip search_quality` | affected-change | working-tree on 43893740b3e2 | pass | 2026-05-24T23:15:30Z | no |
| Candidate 11 tool-call split preserves formatting, typecheck, selected buckets, and changed selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket tools-metrics`; `cargo xtask test bucket daemon`; `cargo xtask test bucket dashboard`; `cargo xtask test changed` | affected-change | working-tree on 43893740b3e2 | pass: changed selected daemon only, 61.4s total | 2026-05-24T23:15:30Z | no |
| Claude CLI reviews Candidate 11 tool-call split | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on 43893740b3e2 | approve with no findings; requested check and changed gate were already passed | 2026-05-24T23:15:30Z | no |
| Candidate 11 codehealth persistence split preserves snapshot insert/read/history/materialization and workspace-ID migration behavior | `cargo nextest run --lib test_snapshot_and_retrieve_codehealth`; `cargo nextest run --lib test_snapshot_history`; `cargo nextest run --lib test_get_latest_snapshot_returns_none_when_empty`; `cargo nextest run --lib test_snapshot_codehealth_from_symbols_db`; `cargo nextest run --lib test_migrate_workspace_ids_updates_all_tables`; `cargo nextest run --lib test_migrate_workspace_ids_merges_when_target_workspace_exists`; `cargo nextest run --lib tests::daemon::database -- --skip search_quality` | affected-change | working-tree on 9d7d01ec275e | pass | 2026-05-24T23:25:53Z | no |
| Candidate 11 codehealth split preserves formatting, typecheck, daemon bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket daemon`; `cargo xtask test changed` | affected-change | working-tree on 9d7d01ec275e | pass: changed selected daemon only, 61.4s total | 2026-05-24T23:25:53Z | no |
| Claude CLI reviews Candidate 11 codehealth split | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on 9d7d01ec275e | approve with no findings; verified SQL parity, re-export stability, private module access, and migration scope | 2026-05-24T23:27:53Z | no |
| Candidate 12 test-quality fixture migration preserves pipeline behavior for metadata update, identifier evidence, fallback, and exact identifier matching | `cargo nextest run --lib test_pipeline_integration_updates_metadata`; `cargo nextest run --lib test_pipeline_integration_with_identifier_evidence`; `cargo nextest run --lib test_empty_evidence_config_falls_back_to_regex`; `cargo nextest run --lib test_identifier_evidence_without_matches_falls_back_to_regex_body`; `cargo nextest run --lib test_identifier_evidence_no_substring_matching` | worker-red-green | working-tree on 54ca28912a3e | pass | 2026-05-24T23:35:46Z | no |
| Candidate 12 test-quality fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 54ca28912a3e | pass: changed selected analysis only, 1.0s total | 2026-05-24T23:35:46Z | no |
| Claude CLI reviews Candidate 12 test-quality fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on 54ca28912a3e | approve with no findings; verified span/default delta is inert, identifier containment survives storage API, and scope stayed narrow | 2026-05-24T23:37:51Z | no |
| Candidate 12 test-linkage fixture migration preserves relationship-based and identifier-only linkage behavior | `cargo nextest run --lib test_compute_linkage_relationship_linkage`; `cargo nextest run --lib test_identifier_only_linkage` | worker-red-green | working-tree on bd640c7a135c | pass | 2026-05-24T23:46:23Z | no |
| Candidate 12 test-linkage fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on bd640c7a135c | pass: changed selected analysis only, 1.0s total | 2026-05-24T23:46:23Z | no |
| Claude CLI reviews Candidate 12 first linkage fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on bd640c7a135c | approve with no findings; verified `reference_score` removal, builder row defaults, and narrow scope | 2026-05-24T23:48:18Z | no |
| Candidate 11 workspace registry split preserves workspace CRUD, stats, sessions, cleanup events, normalize sentinels, and workspace-ID migration behavior | `cargo nextest run --lib test_upsert_and_get_workspace`; `cargo nextest run --lib test_increment_decrement_session_count`; `cargo nextest run --lib test_reset_all_session_counts`; `cargo nextest run --lib test_update_workspace_stats`; `cargo nextest run --lib test_update_vector_count`; `cargo nextest run --lib test_insert_and_list_cleanup_events`; `cargo nextest run --lib test_cleanup_event_log_is_capped_at_fifty_rows`; `cargo nextest run --lib test_delete_workspace_with_root_path`; `cargo nextest run --lib test_upsert_workspace_path_conflict_updates_status`; `cargo nextest run --lib test_upsert_workspace_allows_upgrade_to_ready`; `cargo nextest run --lib test_normalize_workspace_paths_fixes_slashes_and_status`; `cargo nextest run --lib test_normalize_workspace_paths_skips_pending_without_symbols`; `cargo nextest run --lib test_normalize_restores_ready_on_all_platforms`; `cargo nextest run --lib test_migrate_workspace_ids_updates_all_tables`; `cargo nextest run --lib test_migrate_workspace_ids_merges_when_target_workspace_exists`; `cargo nextest run --lib tests::daemon::database -- --skip search_quality` | affected-change | working-tree on 3480b5b3be9d | pass | 2026-05-24T23:59:31Z | no |
| Candidate 11 workspace registry split preserves formatting, typecheck, daemon bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket daemon`; `cargo xtask test changed` | affected-change | working-tree on 3480b5b3be9d | pass: changed selected daemon only, 61.4s total | 2026-05-24T23:59:31Z | no |
| Claude CLI reviews Candidate 11 workspace registry split | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "Read,Bash" --model opus --effort high ...` | gate-review | working-tree on 3480b5b3be9d | approve with no findings; verified re-export stability, private submodule access, SQL parity, and root-owned startup/migration repair scope | 2026-05-25T00:01:28Z | no |
| Candidate 12 remaining test-quality pipeline fixture migration preserves metadata update, identifier evidence, no-body, metadata preservation, and fixture-not-applicable behavior | `cargo nextest run --lib test_pipeline_integration_updates_metadata`; `cargo nextest run --lib test_pipeline_integration_with_identifier_evidence`; `cargo nextest run --lib test_pipeline_integration_no_body`; `cargo nextest run --lib test_pipeline_integration_preserves_existing_metadata`; `cargo nextest run --lib test_pipeline_integration_fixture_not_applicable` | worker-red-green | working-tree on f147d0aac95d | pass | 2026-05-25T00:14:49Z | no |
| Candidate 12 remaining test-quality pipeline fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on f147d0aac95d | pass: changed selected analysis only, 1.0s total; final rerun ignored docs/memories and selected analysis | 2026-05-25T00:19:37Z | no |
| Claude CLI reviews Candidate 12 remaining test-quality fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high --max-turns 1 ...` | gate-review | working-tree on f147d0aac95d | approve with no findings; earlier read-tool run hit max turns and semantic rerun approved with malformed key names, so strict no-tool rerun supplied schema-valid approval | 2026-05-25T00:19:05Z | no |
| Candidate 12 linkage edge-case fixture migration preserves uncovered-symbol and test-to-test exclusion behavior | `cargo nextest run --lib test_uncovered_symbol_has_no_test_linkage_key`; `cargo nextest run --lib test_test_to_test_relationships_excluded` | worker-red-green | working-tree on 84c67252711a | pass | 2026-05-25T00:29:53Z | no |
| Candidate 12 linkage edge-case fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 84c67252711a | pass: changed selected analysis only, 1.0s total; final rerun ignored plan doc and selected analysis | 2026-05-25T00:34:00Z | no |
| Claude CLI reviews Candidate 12 linkage edge-case fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on 84c67252711a | approve with no findings; prior semantic review approved but used informational findings, so strict rerun supplied schema-valid approval | 2026-05-25T00:33:35Z | no |
| Candidate 12 linkage cap fixture migration preserves linked-test count and display cap behavior | `cargo nextest run --lib test_linked_tests_capped_at_five` | worker-red-green | working-tree on 9e364e51495d | pass | 2026-05-25T00:39:24Z | no |
| Candidate 12 linkage cap fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 9e364e51495d | pass: changed selected analysis only, 1.0s total; final rerun ignored plan doc and selected analysis | 2026-05-25T00:41:22Z | no |
| Claude CLI reviews Candidate 12 linkage cap fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on 9e364e51495d | approve with no findings; first review emitted only `findings`, so strict rerun supplied schema-valid approval | 2026-05-25T00:40:59Z | no |
| Candidate 12 linkage class-similarity fixture migration preserves name-match fallback behavior | `cargo nextest run --lib test_name_match_prefers_class_name_similarity` | worker-red-green | working-tree on df483d47f1d8 | pass | 2026-05-25T00:49:41Z | no |
| Candidate 12 linkage class-similarity fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on df483d47f1d8 | pass: changed selected analysis only, 1.0s total; final rerun ignored plan doc and selected analysis | 2026-05-25T00:51:55Z | no |
| Claude CLI reviews Candidate 12 linkage class-similarity fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on df483d47f1d8 | approve with no findings; first review emitted only `findings`, so strict rerun supplied schema-valid approval | 2026-05-25T00:51:30Z | no |
| Candidate 12 linkage ambiguity-guard fixture migration preserves same-language fallback and cross-language exclusion behavior | `cargo nextest run --lib test_name_match_ambiguity_guard_is_language_scoped` | worker-red-green | working-tree on de3bc57b06d3 | pass | 2026-05-25T00:58:50Z | no |
| Candidate 12 linkage ambiguity-guard fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on de3bc57b06d3 | pass: changed selected analysis only, 1.0s total; final rerun ignored plan doc and selected analysis | 2026-05-25T01:01:57Z | no |
| Claude CLI reviews Candidate 12 linkage ambiguity-guard fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on de3bc57b06d3 | approve with no findings; first review emitted only `findings`, so strict rerun supplied schema-valid approval | 2026-05-25T01:01:29Z | no |
| Candidate 12 linkage parent-inheritance fixture migration preserves method linkage and parent aggregation behavior | `cargo nextest run --lib test_class_inherits_method_linkage` | worker-red-green | working-tree on 75ef98e96e91 | pass | 2026-05-25T01:08:26Z | no |
| Candidate 12 linkage parent-inheritance fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt`; `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 75ef98e96e91 | pass: fmt applied indentation-only fix, changed selected analysis only, 1.1s total; final rerun ignored plan doc and selected analysis | 2026-05-25T01:11:06Z | no |
| Claude CLI reviews Candidate 12 linkage parent-inheritance fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on 75ef98e96e91 | approve with no findings; first review exhausted turn cap, so strict rerun supplied schema-valid approval | 2026-05-25T01:10:40Z | no |
| Candidate 12 linkage deduplication fixture migration preserves duplicate identifier deduplication behavior | `cargo nextest run --lib test_deduplication_across_strategies` | worker-red-green | working-tree on 2e0cf493980b | pass | 2026-05-25T01:19:57Z | no |
| Candidate 12 linkage deduplication fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 2e0cf493980b | pass: changed selected analysis only, 1.0s total; final rerun ignored plan doc and selected analysis | 2026-05-25T01:23:29Z | no |
| Claude CLI reviews Candidate 12 linkage deduplication fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on 2e0cf493980b | approve with no findings; first review tried tool use and hit max turns, strict no-tool rerun approved builder semantics, plan wording, and verification evidence | 2026-05-25T01:22:36Z | no |
| Candidate 12 linkage edge-case file split preserves moved test behavior | `cargo nextest run --lib test_name_match_fallback_is_deterministic_on_tied_scores`; `cargo nextest run --lib test_compute_linkage_clears_stale_symbol_and_parent_linkage`; `cargo nextest run --lib test_scorable_filter_excludes_fixture_includes_test_case_and_legacy`; `cargo nextest run --lib test_best_confidence_present_in_linkage_output`; `cargo nextest run --lib test_parent_aggregation_includes_best_confidence`; `cargo nextest run --lib test_best_confidence_defaults_when_metadata_absent` | worker-red-green | working-tree on f607ca943832 | pass | 2026-05-25T01:33:33Z | no |
| Candidate 12 linkage edge-case file split preserves formatting, typecheck, analysis bucket, changed-file selection, and file-size cap | `cargo fmt`; `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed`; `wc -l src/tests/analysis/test_linkage_tests.rs src/tests/analysis/linkage_edge_cases_tests.rs` | affected-change | working-tree on f607ca943832 | pass: changed selected analysis; final rerun ignored plan doc; line counts 584 and 407 | 2026-05-25T01:36:40Z | no |
| Claude CLI reviews Candidate 12 linkage edge-case file split | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on f607ca943832 | approve with no findings; verified moved-body parity, module discovery, git-visible filename, line counts, and remaining raw SQL plan accuracy | 2026-05-25T01:35:20Z | no |
| Candidate 12 linkage tied-score fallback fixture migration preserves deterministic name-match behavior | `cargo nextest run --lib test_name_match_fallback_is_deterministic_on_tied_scores` | worker-red-green | working-tree on 3ab99e838cab | pass; rerun after `cargo fmt` | 2026-05-25T01:45:06Z | no |
| Candidate 12 linkage tied-score fallback fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt`; `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 3ab99e838cab | pass: fmt applied builder-block formatting, changed selected analysis only; final rerun ignored plan doc | 2026-05-25T01:48:08Z | no |
| Claude CLI reviews Candidate 12 linkage tied-score fallback fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on 3ab99e838cab | approve with no findings; verified row semantics, identifier NULL-target fallback, plan accuracy, and verification evidence | 2026-05-25T01:46:47Z | no |
| Candidate 12 linkage stale-cleanup fixture migration preserves stale relationship removal and parent linkage clearing behavior | `cargo nextest run --lib test_compute_linkage_clears_stale_symbol_and_parent_linkage` | worker-red-green | working-tree on 1cb8e4ad0537 | pass; rerun after `cargo fmt` | 2026-05-25T01:53:57Z | no |
| Candidate 12 linkage stale-cleanup fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt`; `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 1cb8e4ad0537 | pass: fmt applied relationship-builder formatting, changed selected analysis only; final rerun ignored plan doc | 2026-05-25T01:57:02Z | no |
| Claude CLI reviews Candidate 12 linkage stale-cleanup fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on 1cb8e4ad0537 | approve with no findings; verified parent/child/test row semantics, relationship shape, and ledger accuracy | 2026-05-25T01:55:50Z | no |
| Candidate 12 linkage scorable-filter fixture migration preserves fixture exclusion and legacy inclusion behavior | `cargo nextest run --lib test_scorable_filter_excludes_fixture_includes_test_case_and_legacy` | worker-red-green | working-tree on d9b3d92c28c0 | pass | 2026-05-25T02:04:05Z | no |
| Candidate 12 linkage scorable-filter fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on d9b3d92c28c0 | pass: changed selected analysis only; final rerun ignored plan doc | 2026-05-25T02:07:11Z | no |
| Claude CLI reviews Candidate 12 linkage scorable-filter fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on d9b3d92c28c0 | approve with no findings; verified metadata roles, relationship file/line shape, ledger accuracy, and remaining raw SQL list | 2026-05-25T02:05:59Z | no |
| Candidate 12 linkage direct best-confidence fixture migration preserves test-quality confidence output | `cargo nextest run --lib test_best_confidence_present_in_linkage_output` | worker-red-green | working-tree on 9dfc8f767ca0 | pass; lead rerun after `cargo fmt` | 2026-05-25T02:14:54Z | no |
| Candidate 12 linkage direct best-confidence fixture migration preserves formatting, typecheck, analysis bucket, and changed-file selection | `cargo fmt`; `cargo fmt --check`; `cargo check --all-targets`; `cargo xtask test bucket analysis`; `cargo xtask test changed` | affected-change | working-tree on 9dfc8f767ca0 | pass: fmt applied relationship-builder indentation; changed selected analysis only; final rerun ignored plan doc | 2026-05-25T02:17:56Z | no |
| Claude CLI reviews Candidate 12 linkage direct best-confidence fixture migration | `claude -p --no-session-persistence --dangerously-skip-permissions --output-format json --json-schema ... --tools "" --model opus --effort high ...` | gate-review | working-tree on 9dfc8f767ca0 | approve with no findings; verified row shape, metadata preservation, intentional `reference_score` removal, plan wording, and verification evidence | 2026-05-25T02:16:55Z | no |
