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

Remaining candidates:

- **Candidate 8:** remaining slices are full-output line enrichment trace/degradation metadata, dashboard preview enrichment, and explicit line-mode failure surfacing. Do not mix these with ranking changes.
- **Candidate 10:** remaining slice is resolver same-language scoring/content-aware language metadata for relationship candidates, if re-review confirms enough value and test leverage.
- **Candidate 11:** wide module splits after contracts and harnesses are stronger.
- **Candidate 12:** remaining slices are direct MCP session helpers and DB fixture builders. Do not consolidate daemon roots, adapter duplex tests, or schema/migration raw SQL without per-cluster review.

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
