# Tree-Sitter Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Close the remaining dogfood gaps from the 2026-05-05 verification pass so Julie's tree-sitter extraction and graph navigation can honestly meet the best-in-class bar.

**Architecture:** Keep the original best-in-class plan as the master contract, and treat this as a focused delta plan. Relationship precision gets the main correctness lane. Navigation, health wording, and agent instruction fixes are separate product-quality lanes with narrow tests.

**Tech Stack:** Rust, tree-sitter, `julie-extractors`, SQLite relationship storage, Julie navigation tools, `cargo nextest`, Julie xtask test buckets.

---

## Current Evidence

Baseline verified at `c01b9516` on 2026-05-05:

- `cargo xtask test bucket extractors` passed in 36.1s.
- `cargo xtask test bucket parser-upgrade` passed in 1.3s.
- `cargo xtask test full` passed 30 buckets in 825.6s.
- Live Julie health returned to `FULLY READY` after test-induced catch-up settled.

These gates proved the new golden harness, capability matrix, parser-upgrade gate, real-world contract, parse-diagnostic safety, system tests, and dogfood search-quality gate. They did not prove the remaining relationship precision rollout or the specific live navigation bug found during dogfooding.

Gap-closure evidence at `c01b9516 + working tree` on 2026-05-05:

- Relationship precision rollout completed across reviewed relationship-capable extractor groups.
- `cargo nextest run -p julie-extractors relationship_precision` passed.
- `cargo nextest run -p julie-extractors golden` passed.
- `cargo xtask test bucket extractors` passed in 2.2s.
- `cargo xtask test bucket parser-upgrade` passed in 1.3s.
- `cargo xtask test changed` fell back to the dev tier and passed 22 buckets in 504.4s after the resolver submodule split.
- `cargo xtask test system` passed 6 buckets in 115.2s after the resolver submodule split.
- `cargo xtask test dogfood` passed in 203.6s after the resolver submodule split.
- `cargo build --release` passed in 2m 45s.
- Standalone CLI dogfood now finds `extract_symbols_static -> extract_canonical` as a one-hop path after forced reindex.
- Standalone CLI `refs LanguageSpec` returns the exact struct definition and three references, with no `language_spec` definition noise.
- Standalone CLI health now labels the list as indexed workspace languages.

Post-restart live MCP dogfood on 2026-05-05:

- Initial live MCP `call_path(extract_symbols_static -> extract_canonical)` still returned no path after rebuilding and restarting Codex.
- Root cause was stale persisted daemon graph data, not stale process code. `fast_refs(extract_canonical)` saw the call through identifier fallback, but SQLite had no resolved `relationships` row for `extract_symbols_static -> extract_canonical`.
- `manage_workspace(operation=refresh, workspace_id=julie_528d4264, force=true)` rebuilt the persisted daemon index. Relationship count moved from 24,599 to 32,787, canonical/projected revision became 3871/3871, and live MCP `call_path` then returned the expected one-hop path.
- Follow-up closure: resolver/extractor/indexer semantic changes now have a persisted workspace DB engine stamp. Existing non-empty indexes with a missing or stale stamp force a full re-index even when file hashes are unchanged, then record the current stamp after a successful pipeline.
- Post-fix live MCP restart evidence: daemon startup migrated schema 24, reported `semantic_version_changed`, ran startup repair, recorded `semantic_index_engine`, and left health `FULLY READY` at canonical/projected revision 3898/3898. Live MCP `call_path(extract_symbols_static -> extract_canonical)` returned the expected one-hop path without a manual force refresh.
- Additional dogfood gap closed during live verification: `fast_refs(SemanticVersionChanged)` returned only the enum member definition while content search found real uses. Root cause was Rust identifier extraction missing non-call `scoped_identifier` enum variant usages. The Rust extractor now records scoped enum variant final segments as `type_usage`, and the semantic engine stamp was bumped again so persisted indexes rebuild these new identifier rows after the next daemon restart.

## Findings To Close

1. **Closed: relationship precision is now reviewed and covered across the relationship-capable inventory.** Unsafe receiver-qualified or duplicate-name local resolution now stays unresolved or pending unless the extractor has enough scope evidence.
2. **Closed: `call_path` now follows the production re-export chain.** `crate::extractors::extract_canonical` resolves through `pub use julie_extractors::*` and the workspace crate root re-export to [crates/julie-extractors/src/pipeline.rs](../../crates/julie-extractors/src/pipeline.rs:8).
3. **Closed: `LanguageSpec` conflation was real, sparse refs were expected.** The bug was exact definition lookup falling back to naming variants and returning `language_spec`; exact definitions now suppress variant-definition noise.
4. **Closed: health language wording now says indexed workspace languages.** The output no longer implies that text fallback and missing `jsx` are support-matrix facts.
5. **Closed: agent startup instructions include the new gates.** `JULIE_AGENT_INSTRUCTIONS.md` now names both extractor and parser-upgrade buckets.
6. **Closed: Rust enum variant references are now extracted for `fast_refs`.** Non-call scoped identifiers such as `Self::SemanticVersionChanged` and `IndexingRepairReason::SemanticVersionChanged` now produce `type_usage` identifiers, so identifier-backed reference queries can find enum variant usages.

## Follow-Up Findings

1. **Closed: semantic index invalidation across binary upgrades.** A rebuilt/restarted daemon can no longer silently keep old derived graph data when source mtimes and hashes are unchanged. The workspace DB stores `semantic_index_engine`; missing or stale values trigger an effective full re-index and startup repair reports `semantic_version_changed`.

## File Structure

### Relationship Precision

- Modify: `crates/julie-extractors/src/base/relationship_resolution.rs`
- Modify: `crates/julie-extractors/src/*/relationships.rs`
- Modify: `crates/julie-extractors/src/tests/relationship_precision.rs`
- Modify: `fixtures/extraction/**/expected.json` only when a reviewed semantic change requires it.

### Navigation Dogfood

- Modify: `src/tools/workspace/indexing/resolver.rs`
- Create: `src/tools/workspace/indexing/resolver/rust_reexports.rs`
- Create: `src/tools/workspace/indexing/resolver/scoring.rs`
- Modify: `src/tests/tools/call_path_tests.rs`
- Modify: `src/tests/tools/call_path_disambiguation_tests.rs` if the regression fits that module better.

### Language And Health Output

- Modify: `src/tools/navigation/fast_refs.rs`
- Modify: `src/tools/navigation/target_workspace.rs`
- Modify: `src/tests/tools/target_workspace_fast_refs_tests.rs`
- Modify: `src/health/report.rs`
- Modify: `src/health/types.rs` or `src/health/data_plane.rs` only if the data contract needs to distinguish indexed languages from supported languages.
- Modify: related health tests under `src/tests/`.

### Docs And Instructions

- Modify: `JULIE_AGENT_INSTRUCTIONS.md`
- Modify: this plan and the master plan only to record verified status.
- Save: `.memories/` checkpoint files created during execution.

### Semantic Index Invalidation

- Create: `src/database/index_engine.rs`
- Create: `src/tools/workspace/indexing/engine_version.rs`
- Modify: `src/database/migrations.rs`
- Modify: `src/database/schema.rs`
- Modify: `src/database/mod.rs`
- Modify: `src/tools/workspace/indexing/index.rs`
- Modify: `src/tools/workspace/indexing/state.rs`
- Modify: `src/startup.rs`
- Modify: `src/tests/core/database.rs`
- Modify: `src/tests/integration/stale_index_detection.rs`
- Modify: `src/tests/tools/workspace/mod_tests.rs`

### Rust Enum Variant References

- Modify: `crates/julie-extractors/src/rust/identifiers.rs`
- Modify: `crates/julie-extractors/src/tests/rust/identifiers.rs`
- Modify: `src/tools/workspace/indexing/engine_version.rs`

## Task 1: Relationship Precision Inventory And Shared Contract

**Files:**
- Modify: `crates/julie-extractors/src/tests/relationship_precision.rs`
- Modify: `crates/julie-extractors/src/base/relationship_resolution.rs` if the shared API lacks a needed helper.

**What to build:** Add tests that make the relationship precision contract explicit for local calls: duplicate unqualified targets are ambiguous, self/this receiver calls resolve inside the caller's parent scope, receiver-qualified foreign calls remain unresolved or pending unless the extractor can prove the target, and overloads do not collapse to an arbitrary symbol.

**Approach:** Start with shared helper tests before touching per-language extractors. If a language cannot express one of the cases cleanly, record that in a fixture or test name rather than silently skipping it.

**Acceptance criteria:**
- [x] Shared tests fail if duplicate local callable names resolve to the first arbitrary symbol.
- [x] Shared tests fail if self/this receiver resolution ignores caller parent scope.
- [x] Shared tests fail if receiver-qualified calls confidently point at an unrelated local method.
- [x] Worker-scope verification passes: `cargo nextest run -p julie-extractors relationship_precision`.

## Task 2: Relationship Precision Rollout By Language Group

**Files:**
- Modify: `crates/julie-extractors/src/*/relationships.rs`
- Modify: `fixtures/extraction/**/expected.json` only for reviewed semantic changes.

**What to build:** Review every relationship-capable extractor and replace unsafe name-only call resolution with `ScopedSymbolIndex` or structured pending relationship output. Type, inheritance, and import edges may keep name matching only when the candidate set is semantically unique or the extractor has no stronger information, but the worker must state why each retained name lookup is safe.

**Language groups for parallel ownership:**
- `c`, `cpp`, `go`, `zig`, `bash`, `powershell`, `gdscript`
- `java`, `csharp`, `vbnet`, `php`, `dart`
- `ruby`, `swift`, `kotlin`, `scala`, `elixir`
- `lua`, `qml`, `r`, `html`, `razor`, `sql`

**Acceptance criteria:**
- [x] Every `relationships.rs` file has been reviewed and classified: fixed, already safe, or intentionally unresolved.
- [x] Local call extraction no longer uses first-match name lookup when duplicate callable targets can exist in the same file.
- [x] Ambiguous calls become structured pending relationships or missing local relationships, not wrong confident edges.
- [x] Each language group gets at least one focused regression test or golden fixture assertion when behavior changes.
- [x] Worker-scope verification passes for each changed exact test, usually `cargo nextest run -p julie-extractors relationship_precision` or the exact language test.

## Task 3: `call_path` Re-Export Regression

**Files:**
- Modify: `src/tests/tools/call_path_tests.rs`
- Modify: `src/tools/workspace/indexing/resolver.rs`
- Create: `src/tools/workspace/indexing/resolver/rust_reexports.rs`
- Create: `src/tools/workspace/indexing/resolver/scoring.rs`

**What to build:** Make `call_path` find the indexed call from `extract_symbols_static` to `extract_canonical`, or produce a diagnostic that correctly explains why the edge is not traversable. The desired outcome is a found one-hop path when `fast_refs` already sees the call edge.

**Approach:** Follow the data: inspect endpoint resolution, stored outgoing relationships for `extract_symbols_static`, and target ID resolution for `extract_canonical`. The likely bug is symbol resolution or relationship target identity around re-exported `crate::extractors::extract_canonical`.

**Acceptance criteria:**
- [x] Add a regression test that fails before the fix for the `extract_symbols_static -> extract_canonical` path shape.
- [x] The test asserts a found path and meaningful hop fields, not just absence of an error.
- [x] Worker-scope verification passes with exact call-path regression tests.

## Task 4: `LanguageSpec` Reference Quality Investigation

**Files:**
- Modify: `src/tests/tools/target_workspace_fast_refs_tests.rs`
- Modify: `src/tools/navigation/fast_refs.rs`
- Modify: `src/tools/navigation/target_workspace.rs`

**What to build:** Decide whether sparse `LanguageSpec` references are expected or a type-reference indexing bug. If it is a bug, add a failing test and fix it. If it is expected, record the explanation in this plan's completion notes.

**Completion notes:** The sparse reference count is expected for the current codebase. The real bug was exact lookup including naming-variant definitions, which let `language_spec` pollute `LanguageSpec` results. The fix keeps naming variants only as a fallback when exact definitions are absent.

**Acceptance criteria:**
- [x] Evidence explains why `LanguageSpec` refs are sparse, or a regression test proves the missing type-reference behavior.
- [x] Any production change follows red-green TDD.
- [x] Worker-scope verification passes for the exact affected navigation/reference test.

## Task 5: Health Language Wording

**Files:**
- Modify: `src/health/report.rs`
- Modify: `src/health/types.rs` or `src/health/data_plane.rs` only if the data contract needs a split.
- Modify: health report tests under `src/tests/`.

**What to build:** Make health output say what it actually reports. If it is indexed file languages, label it that way and allow `text`. Do not imply it is the tree-sitter support matrix. If supported tree-sitter inventory is needed, expose it separately.

**Acceptance criteria:**
- [x] Health output no longer makes `text` vs `jsx` look like a support-matrix contradiction.
- [x] Tests assert the wording or data contract.
- [x] Worker-scope verification passes for the exact health/report test.

## Task 6: Agent Instruction Gate Mentions

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md`

**What to build:** Add the extractor and parser-upgrade gates to the injected agent instructions so plugin users get the same guidance as `AGENTS.md` and `CLAUDE.md`.

**Acceptance criteria:**
- [x] `JULIE_AGENT_INSTRUCTIONS.md` names `cargo xtask test bucket extractors`.
- [x] `JULIE_AGENT_INSTRUCTIONS.md` names `cargo xtask test bucket parser-upgrade`.
- [x] The wording makes parser dependency changes use the parser-upgrade gate.

## Task 7: Semantic Index Invalidation

**Files:**
- Create: `src/database/index_engine.rs`
- Create: `src/tools/workspace/indexing/engine_version.rs`
- Modify: `src/database/migrations.rs`
- Modify: `src/database/schema.rs`
- Modify: `src/tools/workspace/indexing/index.rs`
- Modify: `src/startup.rs`
- Modify: `src/tools/workspace/indexing/state.rs`
- Modify: related database, startup, and workspace tests.

**What to build:** Persist a semantic engine version for derived index data and treat a missing or stale value as an effective full re-index. This closes the binary-upgrade gap where new extractor/resolver behavior could run against old relationships because file hashes were unchanged.

**Completion notes:** Added migration 024 for `index_engine_state`, a deliberate `SEMANTIC_INDEX_ENGINE_VERSION` code stamp, startup repair reason `semantic_version_changed`, and an effective full-index path that overrides catch-up mode when the semantic stamp is stale.

**Acceptance criteria:**
- [x] A stale semantic version forces re-index even when file hashes match.
- [x] The regression restores deleted relationship rows, not just metadata.
- [x] Startup repair reports semantic version drift explicitly.
- [x] Fresh current indexes do not trigger repair.
- [x] No-change refresh still skips unnecessary embedding work.
- [x] System and dogfood tiers pass after the fix.

## Task 8: Rust Enum Variant Reference Extraction

**Files:**
- Modify: `crates/julie-extractors/src/rust/identifiers.rs`
- Modify: `crates/julie-extractors/src/tests/rust/identifiers.rs`
- Modify: `src/tools/workspace/indexing/engine_version.rs`

**What to build:** Make Rust identifier extraction record enum variant usages in non-call scoped identifiers so `fast_refs` can find references to enum members. Preserve existing scoped call behavior, where `HashMap::new()` still emits `new` as a call rather than a type-usage-only path.

**Completion notes:** Added a red/green extractor regression for `Self::SemanticVersionChanged` and `IndexingRepairReason::SemanticVersionChanged`. The fix emits the final segment of non-call `scoped_identifier` nodes as `IdentifierKind::TypeUsage`, while skipping scoped identifiers that are inside a call-expression function. Bumped `SEMANTIC_INDEX_ENGINE_VERSION` to `2026-05-05.reference-identifier-v3`.

**Acceptance criteria:**
- [x] Rust extractor emits enum variant scoped usages as `type_usage` identifiers.
- [x] Existing scoped call extraction still passes.
- [x] Standalone release binary `refs SemanticVersionChanged` returns the enum member definition and real use sites after reindex.
- [x] Extractor bucket, changed tier, dogfood tier, and release build pass after the fix.

## Verification Strategy

**Project source of truth:** [AGENTS.md](../../AGENTS.md), [RAZORBACK.md](../../RAZORBACK.md), [docs/TESTING_GUIDE.md](../TESTING_GUIDE.md), and [xtask/test_tiers.toml](../../xtask/test_tiers.toml).

**Worker red/green scope:** Workers run exact tests they add or change. Extractor workers usually run `cargo nextest run -p julie-extractors <exact_test_name>`. Main crate workers use `cargo nextest run --lib <exact_test_name>`.

**Worker ceiling:** Workers may run only assigned exact tests. Workers do not own `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test dogfood`, or parser-upgrade buckets.

**Worker gate invariant:** Relationship tests prove ambiguous local calls do not become wrong confident graph edges. Call-path tests prove traversability matches stored call edges. Health tests prove labels match the data contract. Instruction updates are docs-only and do not own a runtime gate.

**Lead affected-change scope:** After coherent batches, run `cargo xtask test changed`. If relationship extractor files or fixtures changed, also run `cargo xtask test bucket extractors`.

**Branch gate:** Run `cargo xtask test dev` once after the complete batch. Add `cargo xtask test system` for navigation, health, or workspace behavior changes. Add `cargo xtask test dogfood` after graph output or search/navigation behavior changes.

**Parser-upgrade gate:** Run `cargo xtask test bucket parser-upgrade` if any parser dependency, grammar adaptation, or golden expected output changes.

**Replay/metric evidence:** Golden fixture comparisons and exact regression tests are hard gates. Live Julie dogfood checks are acceptance evidence only after the daemon reports `FULLY READY`.

**Escalation triggers:** Escalate to strategy or gate-review tier for ambiguous relationship semantics, parser grammar shape surprises, language-specific extractor uncertainty, call graph identity bugs spanning extraction and database storage, or tests that pass while live dogfood still fails.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse evidence only when HEAD and scope match.

## Verification Ledger

This is a historical execution ledger for the gap-closure batch. Do not reuse these rows as release evidence unless the scope label and commit SHA match current HEAD exactly. Current release-readiness evidence belongs in [TREE_SITTER_QUALITY_BAR.md](../TREE_SITTER_QUALITY_BAR.md).

| Invariant | Command | Scope label | Commit SHA | Result | Timestamp |
| --- | --- | --- | --- | --- | --- |
| C declarations do not steal C definitions | `cargo nextest run -p julie-extractors test_scoped_symbol_index_prefers_unique_definition_over_declaration` | exact extractor regression | `c01b9516 + working tree` | Passed | 2026-05-05 |
| Shared relationship precision contract | `cargo nextest run -p julie-extractors relationship_precision` | relationship precision | `c01b9516 + working tree` | Passed 12 tests | 2026-05-05 |
| Golden extraction fixtures match reviewed semantic changes | `cargo nextest run -p julie-extractors golden` | extractor golden | `c01b9516 + working tree` | Passed | 2026-05-05 |
| Extractor bucket gate | `cargo xtask test bucket extractors` | extractor bucket | `c01b9516 + working tree` | Passed 2 commands in 2.2s | 2026-05-05T10:46:26Z |
| Parser-upgrade bucket gate | `cargo xtask test bucket parser-upgrade` | parser-upgrade bucket | `c01b9516 + working tree` | Passed 2 commands in 1.3s | 2026-05-05T10:46:26Z |
| Exact LanguageSpec definitions suppress variant-definition noise | `cargo nextest run --lib test_target_workspace_exact_definition_suppresses_variant_definition_noise` | exact navigation regression | `c01b9516 + working tree` | Passed | 2026-05-05 |
| Health output labels indexed workspace languages | `cargo nextest run --lib test_manage_workspace_health_detailed_uses_rebound_session_primary` | exact health regression | `c01b9516 + working tree` | Passed | 2026-05-05 |
| Direct Rust re-exported call path resolves to the definition | `cargo nextest run --lib test_call_path_resolves_reexported_crate_call_to_definition_target` | exact call-path regression | `c01b9516 + working tree` | Passed | 2026-05-05 |
| Workspace-crate glob re-exported call path resolves to the definition | `cargo nextest run --lib test_call_path_resolves_workspace_crate_glob_reexport_to_definition_target` | exact call-path dogfood regression | `c01b9516 + working tree` | Red before fix, passed after fix | 2026-05-05 |
| Debug binary builds | `cargo build` | debug build | `c01b9516 + working tree` | Passed | 2026-05-05 |
| Live standalone call-path dogfood finds production edge | `./target/debug/julie-server call-path extract_symbols_static extract_canonical --workspace . --standalone --json --max-hops 2` | CLI dogfood | `c01b9516 + working tree` | Passed after forced standalone reindex | 2026-05-05 |
| Live standalone fast refs stay exact for LanguageSpec | `./target/debug/julie-server refs LanguageSpec --workspace . --standalone --json -n 20` | CLI dogfood | `c01b9516 + working tree` | Passed, 4 exact references | 2026-05-05 |
| Live standalone health wording is clear | `./target/debug/julie-server workspace health --workspace . --standalone --json` | CLI dogfood | `c01b9516 + working tree` | Passed, says `Indexed workspace languages` | 2026-05-05 |
| Resolver submodule split preserves workspace-crate glob re-export fix | `cargo nextest run --lib test_call_path_resolves_workspace_crate_glob_reexport_to_definition_target` | exact call-path dogfood regression | `c01b9516 + working tree` | Passed after split | 2026-05-05T11:07:17Z |
| Resolver submodule split preserves direct re-export fix | `cargo nextest run --lib test_call_path_resolves_reexported_crate_call_to_definition_target` | exact call-path regression | `c01b9516 + working tree` | Passed after split | 2026-05-05T11:07:17Z |
| Refactored debug binary builds | `cargo build` | debug build | `c01b9516 + working tree` | Passed after split | 2026-05-05T11:07:17Z |
| Release binary builds | `cargo build --release` | release build | `c01b9516 + working tree` | Passed in 2m 45s | 2026-05-05T11:11:38Z |
| Changed-file regression tier | `cargo xtask test changed` | changed, dev fallback | `c01b9516 + working tree` | Passed 22 buckets in 504.4s after split | 2026-05-05T11:07:17Z |
| System tier | `cargo xtask test system` | system | `c01b9516 + working tree` | Passed 6 buckets in 115.2s after split | 2026-05-05T11:07:17Z |
| Dogfood tier | `cargo xtask test dogfood` | dogfood | `c01b9516 + working tree` | Passed 2 buckets in 203.6s after split | 2026-05-05T11:07:17Z |
| Semantic engine stale stamp rebuilds relationships | `cargo nextest run --lib test_incremental_indexing_forces_reindex_when_index_engine_version_is_stale` | exact workspace regression | `c01b9516 + working tree` | Red before fix, passed after fix | 2026-05-05T13:32:46Z |
| Startup repair reports semantic version drift | `cargo nextest run --lib test_primary_workspace_repair_plan_reports_semantic_version_changed` | exact startup regression | `c01b9516 + working tree` | Passed | 2026-05-05T13:32:46Z |
| Index engine state migration and API round trip | `cargo nextest run --lib test_migration_024_index_engine_state_round_trip` | exact database migration regression | `c01b9516 + working tree` | Passed | 2026-05-05T13:32:46Z |
| Fresh index does not request semantic repair | `cargo nextest run --lib test_fresh_index_no_reindex_needed` | exact startup control | `c01b9516 + working tree` | Passed | 2026-05-05T13:32:46Z |
| Empty database incremental repair still works | `cargo nextest run --lib test_incremental_indexing_detects_empty_database` | exact workspace control | `c01b9516 + working tree` | Passed | 2026-05-05T13:32:46Z |
| No-change refresh still skips embedding pipeline | `cargo nextest run --lib test_refresh_no_changes_skips_embedding_pipeline` | exact workspace control | `c01b9516 + working tree` | Passed | 2026-05-05T13:32:46Z |
| Rebound-primary freshness fixture includes current semantic stamp | `cargo nextest run --lib test_check_if_indexing_needed_uses_rebound_current_primary_snapshot` | exact system regression | `c01b9516 + working tree` | Passed after system tier exposed missing fixture metadata | 2026-05-05T13:32:46Z |
| Changed-file regression tier after semantic invalidation | `cargo xtask test changed` | changed | `c01b9516 + working tree` | Passed 22 buckets in 523.8s | 2026-05-05T13:32:46Z |
| System tier after semantic invalidation | `cargo xtask test system` | system | `c01b9516 + working tree` | Passed 6 buckets in 116.6s | 2026-05-05T13:32:46Z |
| Dogfood tier after semantic invalidation | `cargo xtask test dogfood` | dogfood | `c01b9516 + working tree` | Passed 2 buckets in 206.3s | 2026-05-05T13:32:46Z |
| Release binary builds after semantic invalidation | `cargo build --release` | release build | `c01b9516 + working tree` | Passed in 2m 44s | 2026-05-05T13:38:03Z |
| Live MCP restart auto-repairs stale semantic stamp | `manage_workspace health`, `call_path extract_symbols_static extract_canonical`, daemon log and SQLite stamp inspection | live MCP dogfood | `c01b9516 + working tree` | Passed after restart: health fully ready, call path found, schema 24 and `semantic_index_engine=2026-05-05.relationship-resolution-v2` recorded | 2026-05-05T14:01:09Z |
| Non-force live refresh does not loop full re-index | `manage_workspace refresh workspace_id=julie_528d4264 force=false` plus canonical revision check | live MCP dogfood | `c01b9516 + working tree` | Passed: already up-to-date, canonical revision stayed 3898 | 2026-05-05T14:01:09Z |
| Rust enum variant scoped identifiers are extracted | `cargo nextest run -p julie-extractors test_enum_variant_scoped_identifiers_are_type_usages` | exact extractor regression | `c01b9516 + working tree` | Red before fix, passed after fix | 2026-05-05T14:01:09Z |
| Rust scoped identifier controls still pass | `cargo nextest run -p julie-extractors tests::rust::identifiers` | rust identifier regression suite | `c01b9516 + working tree` | Passed 4 tests | 2026-05-05T14:01:09Z |
| Extractor bucket after enum variant reference fix | `cargo xtask test bucket extractors` | extractor bucket | `c01b9516 + working tree` | Passed 1 bucket in 0.8s | 2026-05-05T14:01:09Z |
| Semantic invalidation still respects bumped engine stamp | `cargo nextest run --lib test_incremental_indexing_forces_reindex_when_index_engine_version_is_stale` | exact workspace regression | `c01b9516 + working tree` | Passed after bump to `2026-05-05.reference-identifier-v3` | 2026-05-05T14:01:09Z |
| Changed-file regression tier after enum variant reference fix | `cargo xtask test changed` | changed | `c01b9516 + working tree` | Passed 22 buckets in 526.7s | 2026-05-05T14:01:09Z |
| Dogfood tier after enum variant reference fix | `cargo xtask test dogfood` | dogfood | `c01b9516 + working tree` | Passed 2 buckets in 197.4s | 2026-05-05T14:01:09Z |
| Release binary builds after enum variant reference fix | `cargo build --release` | release build | `c01b9516 + working tree` | Passed in 2m 36s | 2026-05-05T14:01:09Z |
| Standalone release refs find enum variant use sites | `./target/release/julie-server refs SemanticVersionChanged --workspace . --standalone --json -n 20` | CLI dogfood | `c01b9516 + working tree` | Passed: definition plus 4 uses | 2026-05-05T14:01:09Z |
| Live MCP restart auto-repairs to v3 semantic stamp | `manage_workspace health`, `fast_refs SemanticVersionChanged`, daemon log and SQLite stamp inspection | live MCP dogfood | `c01b9516 + working tree` | Passed after restart: health fully ready at 3903/3903, stamp `2026-05-05.reference-identifier-v3`, definition plus 4 uses | 2026-05-05T14:06:37Z |
| Non-force live refresh stays current after v3 repair | `manage_workspace refresh workspace_id=julie_528d4264 force=false` plus canonical revision check | live MCP dogfood | `c01b9516 + working tree` | Passed: already up-to-date, canonical revision stayed 3903, 4 `SemanticVersionChanged` identifiers remained | 2026-05-05T14:06:37Z |

## Model Routing

**Project source of truth:** [RAZORBACK.md](../../RAZORBACK.md).

**Strategy tier:** Planning, decomposition, relationship semantics review, and final lead review.
- Harness mapping: Codex `gpt-5.5` with medium or high reasoning.

**Implementation tier:** Bounded language-group extractor changes with exact tests.
- Harness mapping: Codex `gpt-5.4-mini` with xhigh reasoning when the invariant is local and clear.

**Coupled implementation tier:** Navigation identity fixes, shared relationship helper changes, and anything that crosses extraction, database, and tool behavior.
- Harness mapping: Codex `gpt-5.3-codex` high by default, xhigh for repeated or terminal-heavy failures.

**Mechanical tier:** Instruction and plan-document updates after evidence is already decided.
- Harness mapping: Codex `gpt-5.4-mini` low or medium.

**Gate-interpretation reviewer:** Review of relationship semantics, expected-output fixture changes, or call-path data-flow diagnosis.
- Harness mapping: Codex `gpt-5.3-codex` high.

**Escalation tier:** Subtle correctness, weak tests, repeated worker failures, or evidence that the graph model itself is wrong.
- Harness mapping: Codex `gpt-5.5` high or xhigh.

**Worker eligibility:** Workers can own a lane only with disjoint file ownership, exact acceptance criteria, and exact test scope. Shared helper changes and graph identity bugs stay lead-owned or coupled implementation tier.

**Escalation triggers:** A worker must stop and report if a language grammar cannot expose enough caller or receiver context, a fix would alter relationship semantics outside its group, or an exact gate fails for reasons not explained by the planned change.

**Mechanical exclusion:** Mechanical workers cannot decide expected-output semantics or acceptance evidence.

**Unsupported harness behavior:** If a harness cannot choose models per agent, use `inherit`, note it in the report, and continue.

## Execution Notes

- Start with Task 3 and Task 5 because they are narrow dogfood bugs with clear repros.
- Run Task 1 before broad relationship edits so every language group implements the same contract.
- Relationship worker reports must list every retained name-only lookup and why it is safe.
- Do not call the work best-in-class until relationship precision inventory is complete, live `call_path` dogfood passes, health wording is clear, and final verification is green.
