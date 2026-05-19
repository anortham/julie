# External Extractor SQLite CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Build `julie-server extract ...`, a process-facing CLI that scans and incrementally updates a caller-owned SQLite DB without requiring daemon, MCP, Tantivy, embeddings, or Rust FFI.

**Architecture:** Add an internal `external_extract` module for CLI orchestration and an `indexing_core` module for shared SQLite-only extraction/persistence helpers. Keep the existing workspace/MCP indexing interface stable while factoring reusable pieces out from under `tools::workspace::indexing`. Do not split crates before this interface is proven; design the module boundary so a crate split into `julie-store` / `julie-indexer` would be mechanical after the feature lands.

**Tech Stack:** Rust 2024, clap, tokio, rusqlite/WAL, fs2 file locks, ignore crate, blake3, existing `julie-extractors`, existing Julie test hierarchy with `cargo nextest` and `cargo xtask`.

**Architecture Quality:** Medium risk. A new public CLI and SQLite contract touch shared database and indexing invariants. The required structural change is a SQLite-only indexing seam inside the main `julie` crate; a new crate or binary split is rejected for this implementation because the current `julie` library still pulls daemon, dashboard, Tantivy, MCP, database, watcher, and tools together.

---

## Source Documents

- Design: `docs/plans/2026-05-18-external-extractor-cli-design.md`
- Verification tiers: `AGENTS.md`
- Model routing: `RAZORBACK.md`
- Testing standards: `docs/TESTING_GUIDE.md`

## Structural Decision

Do not create a new crate before implementation.

Current facts:

- `crates/julie-extractors` already isolates the tree-sitter parser inventory.
- `src/lib.rs` exports daemon, adapter, dashboard, search, database, watcher, tools, and CLI from one fat crate.
- Adding a `julie-extract` bin target inside the current package would still compile the same fat `julie` lib and would not materially improve build time.
- `run_indexing_pipeline` currently takes `ManageWorkspaceTool`, `JulieServerHandler`, and `IndexRoute`; external extract should not depend on those MCP/workspace routing types.

Implementation target:

- Create `src/external_extract/` for caller-facing extract operations.
- Create `src/indexing_core/` for shared extraction, discovery, persistence, and analysis helpers usable by both workspace indexing and external extract.
- Keep `julie-server extract ...` as the initial CLI surface. A separate binary name can be added only after the module seam is stable and there is a packaging reason.

## File Structure

Create:

- `src/external_extract/mod.rs` — public module entry and `run_external_extract`.
- `src/external_extract/cli.rs` — clap args, shared flags, command enum, validation.
- `src/external_extract/report.rs` — JSON/text report shapes and status enum.
- `src/external_extract/lock.rs` — per-DB operation lock and lock ordering.
- `src/external_extract/metadata.rs` — external metadata read/write helpers.
- `src/external_extract/info.rs` — read-only info path.
- `src/external_extract/paths.rs` — root/file normalization and containment.
- `src/external_extract/operations.rs` — scan/update/delete/analyze orchestration.
- `src/indexing_core/mod.rs` — shared indexing core exports.
- `src/indexing_core/discovery.rs` — indexable file discovery with extra ignore files.
- `src/indexing_core/extraction.rs` — batch extraction independent of `ManageWorkspaceTool`.
- `src/indexing_core/persistence.rs` — SQLite-only batch persistence entry points.
- `src/indexing_core/analysis.rs` — reference-score/test-analysis execution and analysis-state update.
- `src/tests/external_extract/mod.rs` — test module root.
- `src/tests/external_extract/cli.rs` — CLI parse/dispatch tests.
- `src/tests/external_extract/info.rs` — read-only info/schema tests.
- `src/tests/external_extract/paths.rs` — root containment/path normalization tests.
- `src/tests/external_extract/operations.rs` — scan/update/delete/analyze behavior tests.
- `src/tests/external_extract/locking.rs` — per-DB lock tests.

Modify:

- `src/lib.rs` — add `external_extract` and `indexing_core` modules.
- `src/cli.rs` — add `Command::Extract`.
- `src/main.rs` — route `Command::Extract` without daemon startup.
- `src/database/mod.rs` — expose new DB submodules.
- `src/database/migrations.rs` — add external metadata table migration.
- `src/database/schema.rs` — initialize external metadata table for fresh DBs if using schema initialization.
- `src/database/bulk_operations.rs` — split existing bulk code into focused modules while preserving current public methods.
- `src/database/workspace.rs` — reuse or move orphan/delete primitives into the new atomic persistence module.
- `src/tools/workspace/indexing/pipeline.rs` — delegate extraction/persistence to `indexing_core` while preserving `run_indexing_pipeline`.
- `src/tools/workspace/indexing/incremental.rs` — share hash/orphan filtering logic or call `indexing_core` equivalent.
- `src/watcher/handlers.rs` — remove duplicated single-file persistence after `indexing_core` has the shared path; do not change watcher behavior except fixing the empty `workspace_id` revision write if tests expose it.
- `src/utils/walk.rs` and/or `src/watcher/filtering.rs` — support extra ignore files through shared discovery helpers without regressing workspace indexing.
- `src/tests/mod.rs` — register `external_extract`.

File-size constraint:

- Do not add new implementation files over 500 lines.
- `src/database/bulk_operations.rs` is already 1578 lines. Any task touching atomic persistence must split it into smaller modules instead of adding more code to it.
- `src/tools/workspace/indexing/pipeline.rs` is already 832 lines. Moving `ExtractedBatch` and extraction logic out is part of the plan.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, and this plan.

**Worker red/green scope:** Use the narrowest exact test name:

```bash
cargo nextest run --lib <exact_test_name> 2>&1 | tail -10
```

Each worker must verify RED and GREEN for the exact test it adds or changes.

**Worker ceiling:** Workers may run exact tests only unless the lead explicitly asks for diagnostic output. Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, or broad `cargo nextest run --lib`.

**Worker gate invariant:** Each assigned exact test must prove one caller-facing behavior from `julie-server extract ...` or one shared persistence invariant needed by that behavior.

**Lead affected-change scope:** After a coherent batch:

```bash
cargo check
cargo xtask test changed
```

**Branch gate:** Before handoff:

```bash
cargo xtask test dev
```

**Specialist gates:** Add `cargo xtask test system` only if implementation changes daemon/adapter startup, workspace pool lifecycle, or live watcher runtime beyond shared helper calls. Add `cargo xtask test dogfood` only if search scoring/query behavior changes, not merely because `extract analyze` recomputes existing DB fields.

**Replay/metric evidence:** No replay or metric gate in this plan. CLI behavior, DB rows, revisions, metadata, and analysis state are hard gates.

**Escalation triggers:** Escalate to strategy/escalation tier if a task requires changing public database schema semantics, `IndexRoute`, daemon session routing, Tantivy projection behavior, watcher event ordering, or any test reveals data loss/corruption across process boundaries.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, timestamp, and evidence reuse in the ledger at the end of this plan. Reuse evidence only when the scope label and commit SHA match current HEAD exactly.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** planning, architecture, decomposition, lead review, finding triage.

- Codex mapping: `gpt-5.5` high for this plan because it changes public CLI/schema/concurrency contracts.
- Claude mapping: Opus or Sonnet high based on risk; use Opus for adversarial review.

**Implementation tier:** bounded worker tasks with clear file ownership and exact tests.

- Codex mapping: `gpt-5.5` medium. Use high for database atomicity, locking, and schema migration tasks.
- Claude mapping: Sonnet high or Opus for schema/concurrency lanes.

**Mechanical tier:** docs, formatting, manifest wiring with no gate ownership.

- Codex mapping: `gpt-5.4-mini` low/medium.
- Claude mapping: Haiku or low-cost equivalent.

**Gate-review tier:** failing test interpretation, schema/revision semantics, concurrency failures.

- Codex mapping: `gpt-5.5` high or `gpt-5.3-codex` high for terminal-heavy diagnosis.
- Claude mapping: Opus or Sonnet high.

**Escalation tier:** data-loss risk, public schema compatibility, lock ordering, weak tests, repeated failures.

- Codex mapping: `gpt-5.5` high/xhigh.
- Claude mapping: Opus.

**Worker eligibility:** Implementation-tier workers may own tasks with narrow file ownership, exact test gates, and no unresolved schema/concurrency decision. Database atomicity and lock-order tasks require high reasoning or lead ownership.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, revision evidence, lock behavior, schema migration behavior, or acceptance gates.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit` and state that in the worker report.

## Implementation Tasks

### Task 1: CLI Surface And Report Contract

**Files:**

- Create: `src/external_extract/mod.rs`
- Create: `src/external_extract/cli.rs`
- Create: `src/external_extract/report.rs`
- Create: `src/tests/external_extract/mod.rs`
- Create: `src/tests/external_extract/cli.rs`
- Modify: `src/lib.rs`
- Modify: `src/tests/mod.rs`

**What to build:** Add external extract argument and report types without wiring the command into `julie-server` yet. Implement standalone clap parsing for `scan`, `update`, `delete`, `analyze`, and `info`, including shared flags `--db`, `--root`, `--strict-schema`, `--ignore-file`, `--workspace-id`, and `--analyze`.

**Approach:**

- Keep extract args outside `cli_tools`; this is not an MCP tool wrapper.
- Make `extract info` not require `--root`.
- Define `ExternalExtractReport`, `ExternalExtractStatus`, and typed error/report serialization contracts.
- Do not add `Command::Extract` to `src/cli.rs` until operation bodies exist in Tasks 7-9 and Task 10 wires the command.
- Do not use `todo!`, `unimplemented!`, `panic!`, `NotImplemented`, or equivalent stubs.

**Acceptance criteria:**

- [ ] `Cli::command().debug_assert()` still passes without the new top-level command wired.
- [ ] `ExternalExtractArgs` parses `scan/update/delete/analyze/info` with expected required flags.
- [ ] `ExternalExtractArgs` parses `info --db path` without root.
- [ ] Report/status JSON serializes deterministically.
- [ ] Worker exact tests pass.

**Worker tests:**

```bash
cargo nextest run --lib external_extract_args_parse_scan_update_delete_analyze_info 2>&1 | tail -10
cargo nextest run --lib external_extract_args_info_does_not_require_root 2>&1 | tail -10
```

### Task 2: External Metadata, Read-Only Info, And Schema Policy

**Files:**

- Create: `src/external_extract/metadata.rs`
- Create: `src/external_extract/info.rs`
- Create: `src/tests/external_extract/info.rs`
- Modify: `src/database/migrations.rs`
- Modify: `src/database/schema.rs`
- Modify: `src/database/mod.rs`

**What to build:** Add the `external_extract_metadata` table and helpers for versioned DB-owned metadata. Implement `extract info` as a read-only path that does not call `SymbolDatabase::new` and does not migrate or initialize the DB.

**Approach:**

- Add a migration for `external_extract_metadata`.
- Required metadata keys: `julie_version`, `sqlite_schema_version`, `extract_contract_version`, `workspace_id`, `root_path`, `created_at`, `updated_at`, `analysis_state`, `analyzed_revision`.
- First write generates UUIDv4 workspace id unless `--workspace-id` is supplied.
- Later commands reject a mismatched `--workspace-id`.
- `extract info` opens SQLite read-only through rusqlite flags, queries `schema_version` and metadata directly, and reports stale/missing metadata without writing.
- `--strict-schema` fails if DB schema is older than the current binary instead of migrating.

**Acceptance criteria:**

- [ ] Write path creates metadata on first scan/update setup.
- [ ] `extract info` reports metadata and counts without mutating an older DB.
- [ ] Newer-than-binary schema fails clearly.
- [ ] `--strict-schema` rejects older DBs.
- [ ] Existing Julie DB migration tests continue to pass.

**Worker tests:**

```bash
cargo nextest run --lib extract_info_is_read_only_and_does_not_migrate 2>&1 | tail -10
cargo nextest run --lib extract_metadata_generates_stable_workspace_id 2>&1 | tail -10
cargo nextest run --lib extract_strict_schema_rejects_older_db 2>&1 | tail -10
```

### Task 3: Per-DB Operation Locking

**Files:**

- Create: `src/external_extract/lock.rs`
- Create: `src/tests/external_extract/locking.rs`
- Create: `src/external_extract/operations.rs`

**What to build:** Implement exclusive per-DB operation locking for `scan`, `update`, `delete`, and `analyze`, with fixed ordering before DB open/migration. Keep `info` read-only and avoid exclusive lock unless cross-platform constraints force it.

**Approach:**

- Lock path: `<db_path>.julie-extract.lock`.
- Use `fs2` file locks consistently with existing DB init locking.
- Lock ordering: extract lock, DB open/init lock, operation, close DB handles, release extract lock.
- Timeout default: 30 seconds.
- Timeout returns non-zero through CLI with machine-readable error.
- Leave lock file on disk after release; document in error/help text if needed.

**Acceptance criteria:**

- [ ] Concurrent write commands serialize.
- [ ] Lock timeout reports the lock path and timeout.
- [ ] `extract info` does not block behind a held exclusive write lock when using read-only DB open.
- [ ] No deadlock with existing DB init lock.

**Worker tests:**

```bash
cargo nextest run --lib extract_write_operations_serialize_per_db_lock 2>&1 | tail -10
cargo nextest run --lib extract_info_does_not_take_exclusive_write_lock 2>&1 | tail -10
```

### Task 4: Path Semantics And Ignore Policy

**Files:**

- Create: `src/external_extract/paths.rs`
- Create: `src/indexing_core/mod.rs`
- Create: `src/indexing_core/discovery.rs`
- Create: `src/tests/external_extract/paths.rs`
- Modify: `src/lib.rs`
- Modify: `src/utils/walk.rs`
- Modify: `src/watcher/filtering.rs` only if needed to share matcher construction
- Modify: `src/tools/workspace/discovery.rs` to delegate shared discovery while preserving auto-`.julieignore` behavior for workspace indexing

**What to build:** Implement exact root/file normalization and caller-supplied ignore-file layering for external extract. Reuse existing hard blacklists and gitignore/.julieignore behavior.

**Approach:**

- Normalize root once: expand `~`, make absolute, canonicalize.
- Resolve root-relative file args against canonical root.
- Canonicalize existing file paths before containment checks.
- For delete paths that do not exist, reject textual traversal outside root and derive relative Unix-style key.
- Reject symlinks resolving outside root.
- Support repeatable `--ignore-file`; ignore files narrow the indexable set and never override hard blacklist.
- External mode does not auto-create `.julieignore`.
- Workspace indexing keeps existing vendor-scan and auto-generation behavior.

**Acceptance criteria:**

- [ ] Absolute and root-relative `--file` map to the same DB key.
- [ ] Outside-root and symlink-outside-root paths are rejected.
- [ ] Deleted missing paths normalize safely.
- [ ] `.gitignore`, `.julieignore`, and `--ignore-file` are respected.
- [ ] Hard blacklist still wins.
- [ ] Existing workspace discovery tests remain green.

**Worker tests:**

```bash
cargo nextest run --lib extract_paths_reject_files_outside_root 2>&1 | tail -10
cargo nextest run --lib extract_paths_reject_symlink_outside_root 2>&1 | tail -10
cargo nextest run --lib extract_ignore_file_excludes_matching_files 2>&1 | tail -10
```

### Task 5: Shared Extraction Core

**Files:**

- Create: `src/indexing_core/extraction.rs`
- Create: `src/indexing_core/batch.rs` if batch types need their own file
- Modify: `src/tools/workspace/indexing/pipeline.rs`
- Modify: `src/tools/workspace/indexing/processor.rs`
- Modify: `src/tests/integration/indexing_pipeline.rs`
- Create or modify: `src/tests/external_extract/operations.rs`

**What to build:** Factor batch extraction out of `ManageWorkspaceTool` and workspace pipeline so both workspace indexing and external extract can assemble identical `ExtractedBatch` data.

**Approach:**

- Move `ExtractedBatch`, extraction outcome handling, language grouping, parse diagnostics, repair entries, and relative path storage into `indexing_core`.
- Keep `run_indexing_pipeline` signature stable and adapt it to call `indexing_core`.
- Preserve parser-backed vs text-only behavior.
- Preserve test-role classification before persistence.
- Keep Tantivy projection out of `indexing_core`.
- Avoid adding more code to `pipeline.rs`; net line count should go down.

**Acceptance criteria:**

- [ ] Workspace indexing tests using `run_indexing_pipeline` still pass.
- [ ] External extract can request an `ExtractedBatch` without `JulieServerHandler`, `IndexRoute`, or `ManageWorkspaceTool`.
- [ ] Parser failure repair entries are preserved.
- [ ] Existing relationship extraction behavior is unchanged.

**Worker tests:**

```bash
cargo nextest run --lib test_indexing_pipeline_reports_stage_history_for_parser_backed_files 2>&1 | tail -10
cargo nextest run --lib extract_scan_extracts_parser_backed_symbols_without_workspace_handler 2>&1 | tail -10
```

### Task 6: Atomic SQLite Persistence Refactor

**Files:**

- Create: `src/database/bulk/mod.rs`
- Create: `src/database/bulk/identifiers.rs`
- Create: `src/database/bulk/types.rs`
- Create: `src/database/bulk/relationships.rs`
- Create: `src/database/bulk/atomic.rs`
- Create: `src/indexing_core/persistence.rs`
- Create or modify: `src/tests/core/incremental_update_atomic.rs`
- Create: `src/tests/external_extract/operations.rs`
- Modify: `src/database/bulk_operations.rs`
- Modify: `src/database/mod.rs`
- Modify: `src/database/workspace.rs`
- Modify: `src/tools/workspace/indexing/pipeline.rs`

**What to build:** Split the oversized bulk operations file and add the atomic persistence primitives external extract needs.

**Approach:**

- Preserve existing public methods as wrappers if needed:
  - `bulk_store_identifiers`
  - `bulk_store_types`
  - `bulk_store_relationships`
  - `incremental_update_atomic`
  - `bulk_store_fresh_atomic`
- Add a force-rebuild primitive that deletes old rows and inserts replacement rows in one transaction after extraction has succeeded.
- Add an incremental scan primitive that commits changed/new files plus orphan deletions in one transaction and records one canonical revision.
- Add or expose a single-file delete primitive that removes file-owned rows, relationships pointing to deleted symbols, and identifier targets in other files that pointed at deleted symbols.
- Ensure every revision write receives the stable external workspace id and never `""`.
- Store parse diagnostics and clear/record repairs consistently.

**Acceptance criteria:**

- [ ] `bulk_operations.rs` shrinks materially and no new bulk module exceeds 500 lines.
- [ ] Existing incremental update tests pass.
- [ ] `scan --force` persistence cannot empty the DB between delete and insert.
- [ ] Mixed changed/orphan scan records one revision.
- [ ] Delete clears dangling cross-file identifier targets.
- [ ] Workspace indexing persistence behavior remains unchanged.

**Worker tests:**

```bash
cargo nextest run --lib test_incremental_update_records_revision_file_changes 2>&1 | tail -10
cargo nextest run --lib extract_force_rebuild_is_atomic_after_extraction_success 2>&1 | tail -10
cargo nextest run --lib extract_mixed_scan_records_single_revision 2>&1 | tail -10
cargo nextest run --lib extract_delete_clears_cross_file_identifier_targets 2>&1 | tail -10
```

### Task 7: External Scan Operation

**Files:**

- Modify: `src/external_extract/operations.rs`
- Modify: `src/indexing_core/discovery.rs`
- Modify: `src/indexing_core/extraction.rs`
- Modify: `src/indexing_core/persistence.rs`
- Modify: `src/external_extract/report.rs`
- Modify: `src/tests/external_extract/operations.rs`

**What to build:** Implement `extract scan` incremental and force modes end to end.

**Approach:**

- Open/migrate DB through write path unless `--strict-schema` rejects it.
- Load or initialize external metadata and stable workspace id.
- Discover indexable files using shared policy.
- Compare hashes before extraction; unchanged files are not passed to atomic persistence.
- Detect orphans from DB file hashes and current discovered set.
- Incremental mode commits changed/new/orphan state in one transaction and one revision.
- Force mode extracts full replacement batch first, then commits atomic replace.
- Mark analysis stale when canonical revision changes.
- Populate JSON counters from orchestrator-tracked counts and post-write DB totals.

**Acceptance criteria:**

- [ ] First scan creates caller-owned DB with metadata and symbols.
- [ ] Repeat scan with no changes processes 0 files and creates no revision.
- [ ] Changed file plus orphan deletion records one revision.
- [ ] Force scan preserves old data if extraction fails before commit.
- [ ] `--ignore-file` affects scan.

**Worker tests:**

```bash
cargo nextest run --lib extract_scan_writes_caller_owned_sqlite_db 2>&1 | tail -10
cargo nextest run --lib extract_scan_unchanged_produces_zero_revisions 2>&1 | tail -10
cargo nextest run --lib extract_scan_changed_and_orphaned_files_commit_one_revision 2>&1 | tail -10
```

### Task 8: External Update And Delete Operations

**Files:**

- Modify: `src/external_extract/operations.rs`
- Modify: `src/indexing_core/extraction.rs`
- Modify: `src/indexing_core/persistence.rs`
- Modify: `src/external_extract/report.rs`
- Modify: `src/tests/external_extract/operations.rs`
- Modify: `src/watcher/handlers.rs` only to delegate shared helpers or fix empty workspace id behavior under existing watcher tests

**What to build:** Implement single-file `update` and `delete` idempotently for caller-owned DBs.

**Approach:**

- `update` checks ignore policy and hard blacklist. If ignored/unsupported, delete stale rows for that file and report `ignored`.
- Existing missing file for `update` returns a clear error suggesting `delete`.
- Unchanged hash writes nothing and preserves revision.
- Changed supported file extracts once, atomically replaces that file's rows, resolves pending relationships, marks analysis stale, and reports `changed`.
- `delete` removes rows and dangling references; missing DB rows report `not_found` with exit 0.
- Renames remain delete-old plus update-new.

**Acceptance criteria:**

- [ ] Unchanged update is no-op.
- [ ] Changed update replaces only touched file rows.
- [ ] Ignored update removes stale rows.
- [ ] Delete is idempotent.
- [ ] Update/delete mark analysis stale only when revision changes.
- [ ] Existing watcher handler tests still pass.

**Worker tests:**

```bash
cargo nextest run --lib extract_update_unchanged_file_is_noop 2>&1 | tail -10
cargo nextest run --lib extract_update_changed_file_replaces_only_that_file 2>&1 | tail -10
cargo nextest run --lib extract_update_ignored_file_deletes_stale_rows 2>&1 | tail -10
cargo nextest run --lib extract_delete_missing_file_is_idempotent 2>&1 | tail -10
```

### Task 9: External Analyze Operation

**Files:**

- Create: `src/indexing_core/analysis.rs`
- Modify: `src/external_extract/operations.rs`
- Modify: `src/external_extract/metadata.rs`
- Modify: `src/external_extract/report.rs`
- Modify: `src/tools/workspace/indexing/pipeline.rs` only if workspace analysis can share the helper without behavior change
- Modify: `src/tests/external_extract/operations.rs`

**What to build:** Implement explicit `extract analyze` and `--analyze` after write commands.

**Approach:**

- Reuse existing DB-derived analysis functions:
  - `SymbolDatabase::compute_reference_scores`
  - `analysis::compute_test_quality_metrics`
  - `analysis::compute_test_linkage`
- Update `analysis_state=current` and `analyzed_revision=<latest canonical revision>` on success.
- Leave analysis stale if any analysis step fails; return non-zero with context.
- Do not involve Tantivy or embeddings.

**Acceptance criteria:**

- [ ] `update` marks analysis stale by default.
- [ ] `extract analyze` recomputes derived scores and marks current revision analyzed.
- [ ] `update --analyze` performs update then analysis under one operation lock.
- [ ] Failed analyze leaves stale state visible in `extract info`.

**Worker tests:**

```bash
cargo nextest run --lib extract_update_marks_analysis_stale 2>&1 | tail -10
cargo nextest run --lib extract_analyze_marks_current_revision_analyzed 2>&1 | tail -10
cargo nextest run --lib extract_update_analyze_runs_under_one_operation_lock 2>&1 | tail -10
```

### Task 10: CLI Output, Error Mapping, And Documentation

**Files:**

- Modify: `src/external_extract/report.rs`
- Modify: `src/main.rs`
- Modify: `src/cli.rs`
- Create: `docs/EXTERNAL_EXTRACT.md`
- Modify: `docs/plans/2026-05-18-external-extractor-cli-design.md` only if implementation decisions refine the approved contract
- Modify: `src/tests/external_extract/cli.rs`
- Modify: `src/tests/external_extract/operations.rs`

**What to build:** Make CLI output and docs usable from Go/C# host programs.

**Approach:**

- JSON output is a single object with `operation`, `db_path`, `root`, `workspace_id`, `schema_version`, `extract_contract_version`, `revision`, `analysis_state`, file counters, DB totals, and `status`.
- Exit code 0 for success/no-op states: unchanged, ignored, deleted, not_found, scanned, rebuilt, analyzed.
- Non-zero for invalid root, outside-root file, schema failure, migration failure, extraction failure that preserves old data, and lock timeout.
- Document examples for scan/update/delete/analyze/info and watcher integration.
- Document lock file lifecycle and ignore-file behavior.

**Acceptance criteria:**

- [ ] Host programs can parse all success/error JSON shapes.
- [ ] Text output stays concise for human use.
- [ ] Docs include Go/C#-friendly process examples without requiring Rust bindings.
- [ ] Existing CLI output tests are not regressed.

**Worker tests:**

```bash
cargo nextest run --lib extract_json_success_shape_is_stable 2>&1 | tail -10
cargo nextest run --lib extract_json_error_shape_is_stable 2>&1 | tail -10
```

### Task 11: Integration Review And Regression Gates

**Files:**

- Modify only files needed to fix issues found by affected-change verification.
- Update verification ledger in this plan after each lead-owned gate.

**What to build:** Integrate all tasks, fix conflicts, and prove existing Julie workspace/MCP behavior remains intact.

**Approach:**

- Review changed public symbols with Julie `deep_dive` and `fast_refs`.
- Run `cargo check`.
- Run `cargo xtask test changed`.
- If changed maps into broad shared infrastructure, accept the runner's fallback to `dev`.
- Run `cargo xtask test dev` once before handoff.
- Add `cargo xtask test system` only if daemon/adapter/workspace lifecycle files changed beyond CLI dispatch.

**Acceptance criteria:**

- [ ] Exact tests from Tasks 1-10 pass.
- [ ] `cargo check` passes.
- [ ] `cargo xtask test changed` passes or its failure is diagnosed and fixed.
- [ ] `cargo xtask test dev` passes before handoff.
- [ ] Verification ledger has fresh current-HEAD rows.

## Parallelization Plan

Initial sequence is constrained by interfaces:

1. Task 1 establishes CLI/report types.
2. Tasks 2, 3, and 4 can run in parallel after Task 1 because metadata, locking, and path/ignore policy own different files.
3. Task 5 must complete before Tasks 6-9 because it creates shared extraction/persistence seams.
4. Tasks 7, 8, and 9 can run in parallel after Task 5 if file ownership is split by operation modules or carefully coordinated under `src/external_extract/operations/`.
5. Task 10 can run after first working scan/update output exists.
6. Task 11 is lead-owned.

Suggested worker ownership after Task 1:

- Worker A: metadata/info/schema files.
- Worker B: lock files and lock tests.
- Worker C: path/ignore/discovery files.
- Worker D: extraction-core refactor.
- Worker E: database atomic persistence refactor.
- Worker F: external scan operation.
- Worker G: external update/delete operations.
- Worker H: analyze operation and docs output polish.

Database atomicity (Worker E) is high-risk and should use escalation/strategy-tier review before merge.

## Plan Mismatch Rules

Workers must stop and report a mismatch if they find:

- `SymbolDatabase::new` cannot be made safe for external write migration without broad database redesign.
- `extract info` cannot open read-only without running migrations.
- Mixed scan atomicity requires changing existing workspace revision semantics.
- Sharing extraction code would require changing `run_indexing_pipeline` caller-facing signature.
- Locking cannot be made cross-platform with current `fs2` behavior.

The lead resolves the mismatch before implementation continues.

## Verification Ledger

Record one row per verification run. Leave empty until commands actually run.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
