# Toolkit Metrics, Blast Radius, and Edit Telemetry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Make tool usage metrics honest, make `blast_radius` fast enough to use routinely, and add edit-tool telemetry that can prove whether Julie editing saves tokens in Claude Code and Codex-style harnesses.

**Architecture:** Record both successful and failed tool calls through one metrics path, preserving typed metadata and known input paths even when execution fails. Bound `blast_radius` traversal during the walk rather than only trimming final output, and make identifier lookups name-first so common method names stop detonating the graph. Add first-class request/input byte metrics plus edit-specific outcome metadata so adoption can be judged by evidence instead of vibes.

**Tech Stack:** Rust, rmcp tool handlers, SQLite schemas and migrations, Julie symbol database, Tantivy-backed indexed workspaces, cargo-nextest, xtask verification tiers.

---

## Scope

Included:
- Failure-aware metrics for public MCP tool handlers.
- Accurate `success=false` rows in per-workspace `tool_calls` and daemon `tool_calls`.
- Bounded `blast_radius` graph traversal and index-friendly identifier lookup.
- Edit-tool metrics for request bytes, changed bytes, diff bytes, file size, dry-run/apply outcome, and failure class.
- Tests and local metric evidence that prove the changes.

Excluded:
- `manage_workspace` redesign. Low usage is expected because the tool is necessary but naturally rare.
- Native harness telemetry for built-in Claude Code `Edit` or Codex `apply_patch`. Julie cannot observe those unless the host reports them.
- Deleting low-use tools. This plan is about correctness and evidence first.

## Current Evidence

- `record_tool_call` only runs after successful tool execution in `src/handler.rs:1666`, and all inserts pass `success=true` in `run_metrics_writer`.
- Failure rows are supported by `src/database/tool_calls.rs:30` and `src/daemon/database.rs:543`, but production handlers do not create them.
- `blast_radius` depth 2 is the performance cliff. Local telemetry showed depth 2 average in seconds, while depth 1 on the same file was roughly hundreds of milliseconds.
- `walk_impacts` in `src/tools/impact/walk.rs:19` expands through identifier-name fallback. Common names such as `new`, `is_empty`, `get`, and `find` create thousands of identifier-derived candidates.
- `get_identifiers_by_names_kinds_excluding_containers` in `src/database/identifiers.rs:269` builds a broad `OR` query. SQLite chose `idx_identifiers_kind_containing`, effectively scanning most `call` identifiers before applying name predicates.
- `edit_file`, `rewrite_symbol`, and `rename_symbol` can save Claude Code context because they avoid mandatory host `Read`, but current metrics do not record enough input or edit outcome data to prove it.

## File Structure

### Metrics

- Modify `src/handler.rs`
  - Keep public handler methods small.
  - Replace success-only instrumentation with success/failure recording.
  - Add or call helper functions from `src/handler/tool_metrics.rs`.
- Create `src/handler/tool_metrics.rs`
  - Own `MetricsTask`, the metrics writer, success/failure task construction, metadata enrichment, and shared insertion helpers.
  - Move existing metrics writer code out of `handler.rs`; keep only handler-facing wrapper methods in `handler.rs`.
- Modify `src/tools/metrics/session.rs`
  - Extend `ToolCallReport` with input/request byte support.
  - Keep in-memory counters simple; do not make session atomics the source of truth for failure rates.
- Modify `src/database/tool_calls.rs`
  - Insert/query `input_bytes`.
  - Preserve existing `source_bytes` and `output_bytes` semantics.
- Modify `src/database/schema.rs`, `src/database/migrations.rs`
  - Add `input_bytes INTEGER` to project-level `tool_calls` schema and migration path.
- Modify `src/daemon/database.rs`
  - Add `input_bytes INTEGER` to daemon `tool_calls`.
  - Pass `success` from `MetricsTask`.
  - Query summaries include `total_input_bytes`.
- Test in `src/tests/core/handler.rs` and `src/tests/daemon/database.rs`.

### Blast Radius

- Modify `src/tools/impact/walk.rs`
  - Add bounded walk options.
  - Cap frontier size per depth and identifier-derived fanout per name.
  - Prefer resolved `target_symbol_id` edges over unresolved name fallback.
- Modify `src/tools/impact/mod.rs`
  - Pass walk budget derived from `limit` and `max_depth`.
  - Keep visible output behavior compatible.
- Create `src/tools/impact/likely_tests.rs`
  - Move `LikelyTests`, `collect_likely_tests`, and helper functions out of `mod.rs` so `mod.rs` stays below the 500-line target while changing it.
- Modify `src/database/identifiers.rs`
  - Split exact identifier name lookup from qualified-prefix lookup.
  - Make exact lookup use a name-first query shape.
- Modify `src/database/schema.rs`, `src/database/migrations.rs`, `src/database/bulk_operations.rs`
  - Add `idx_identifiers_name_kind_containing ON identifiers(name, kind, containing_symbol_id)`.
  - Ensure fresh indexes and migrated indexes both get the new index.
- Modify `src/tests/core/performance_indexes.rs`
  - Assert the new composite identifier index exists.
- Test in `src/tests/tools/blast_radius_tests.rs` and `src/tests/tools/blast_radius_determinism_tests.rs`.

### Edit Telemetry

- Modify `src/handler/tool_targets.rs`
  - Add edit-specific metadata fields for `edit_file`, `rewrite_symbol`, and `rename_symbol`.
- Modify `src/tools/editing/edit_file.rs`
  - Report meaningful failures as errors or explicit failure metadata, not successful `"Error: ..."` content.
  - Compute file size, request byte size, changed byte count, diff byte count, dry-run/apply status, occurrence, and match mode.
- Modify `src/tools/editing/rewrite_symbol.rs`
  - Report validation, symbol resolution, ambiguity, and operation errors as failures or explicit failure metadata.
  - Record operation, symbol span size, content bytes, diff bytes, dry-run/apply status, and ambiguity count.
- Modify `src/tools/refactoring/mod.rs` and `src/tools/refactoring/rename.rs`
  - Report validation and rename failures as failures or explicit failure metadata.
  - Record dry-run/apply status, scope, changed file count, changed line count, and reference count.
- Test in existing editing/refactoring test files under `src/tests/tools/`.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, and `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** Workers run the narrowest exact test they added or changed:
- Metrics: `cargo nextest run --lib test_tool_failure_metrics_records_failed_handler_call 2>&1 | tail -10`
- Metrics: `cargo nextest run --lib test_tool_success_rate_counts_recorded_handler_failures 2>&1 | tail -10`
- Blast radius: `cargo nextest run --lib test_walk_impacts_caps_identifier_fanout_for_common_names 2>&1 | tail -10`
- Blast radius: `cargo nextest run --lib test_identifier_name_kind_container_index_exists 2>&1 | tail -10`
- Edit telemetry: `cargo nextest run --lib test_edit_file_metrics_include_input_and_edit_outcome 2>&1 | tail -10`
- Edit telemetry: `cargo nextest run --lib test_rewrite_symbol_metrics_include_symbol_span_and_failure_kind 2>&1 | tail -10`

**Worker ceiling:** Workers may run only exact tests for their lane. They must not run `cargo xtask test changed`, `cargo xtask test dev`, or broad nextest filters.

**Worker gate invariant:** Each worker-owned test must prove the behavior named in the test:
- Metrics failure tests prove failed handler/tool calls create `tool_calls.success = 0` rows.
- Blast-radius tests prove common identifier names cannot create unbounded depth frontier growth and the required index exists.
- Edit telemetry tests prove edit tools record request/input and outcome fields on both preview/apply and failure paths.

**Lead affected-change scope:** After each coherent batch, run `cargo xtask test changed`.

**Branch gate:** Before handoff, run `cargo xtask test dev`.

**Replay/metric evidence:** Hard gates:
- A failing MCP tool call is visible in daemon `tool_calls` with `success = 0`.
- `get_tool_success_rate` reports total calls including failures and succeeded calls excluding failures.
- `blast_radius` depth 2 on `src/tools/impact/mod.rs` with `include_tests=false`, `limit=12`, and `format=compact` completes in under 2 seconds on the local dogfood workspace after the debug binary is rebuilt.
- `blast_radius` depth 2 on the watcher file set from the investigation completes in under 4 seconds with `include_tests=false`, `limit=12`, and `format=compact`.

Report-only metrics:
- `blast_radius` depth 2 with `include_tests=true`.
- Output row counts, spillover counts, and likely-test overflow counts.
- Edit-tool estimated token savings against Claude full-file-read and Codex patch-hunk baselines.

Metric commands for the lead:
```bash
cargo build
/usr/bin/time -p ./target/debug/julie-server tool blast_radius --workspace . --params '{"file_paths":["src/tools/impact/mod.rs"],"include_tests":false,"limit":12,"max_depth":2,"format":"compact"}' 2>&1 | tail -30
/usr/bin/time -p ./target/debug/julie-server tool blast_radius --workspace . --params '{"file_paths":["src/watcher/runtime.rs","src/watcher/events.rs","src/watcher/queue.rs","src/watcher/mod.rs","src/watcher/handlers.rs"],"include_tests":false,"limit":12,"max_depth":2,"format":"compact"}' 2>&1 | tail -30
sqlite3 -header -column ~/.julie/daemon.db "select tool_name,count(*) total,sum(success) succeeded from tool_calls where timestamp >= strftime('%s','now','-1 hour') group by tool_name order by total desc;"
```

**Escalation triggers:** Run `cargo xtask test system` if daemon DB migration, workspace routing, or handler lifecycle tests fail. Run `cargo xtask test dogfood` if identifier query changes affect search quality, ranking, or tokenization behavior outside impact/context graph expansion.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless their task explicitly updates that failing gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. For metric evidence, record hard-gate metrics and report-only metrics separately. Reuse prior ledger evidence only when the current HEAD SHA and scope label match exactly.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** Lead planning, final integration review, and any change to public metrics semantics.
- Harness mapping: follow `RAZORBACK.md`; in Codex use `gpt-5.5` medium/high when explicit routing is available.

**Implementation tier:** Narrow tests and implementation for edit telemetry after the metric contract is fixed.
- Harness mapping: follow `RAZORBACK.md`; in Codex use `gpt-5.4-mini` xhigh when explicit routing is available.

**Coupled implementation tier:** Handler instrumentation, daemon/project DB schema changes, and blast-radius traversal/query semantics.
- Harness mapping: follow `RAZORBACK.md`; in Codex use `gpt-5.3-codex` high, xhigh if terminal-heavy profiling or repeated failure appears.

**Gate-interpretation reviewer:** Read test, metric output, and diff to decide whether the gate or implementation is wrong.
- Harness mapping: follow `RAZORBACK.md`; in Codex use `gpt-5.3-codex` high.

**Escalation tier:** Repeated failure, schema migration breakage, hidden workspace lifecycle invariant, or query-semantics uncertainty.
- Harness mapping: follow `RAZORBACK.md`; in Codex use `gpt-5.3-codex` high/xhigh or `gpt-5.5` high for architecture changes.

**Worker eligibility:** Use workers only for lanes with clear file ownership and exact tests. Do not assign one worker both metrics schema and blast-radius traversal.

**Escalation triggers:** Escalate if a worker has to interpret dashboard semantics, change the public MCP error contract broadly, alter search ranking, or weaken a test to make a metric pass.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates. Formatting and docs updates may be mechanical only after evidence is already interpreted.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task 1: Failure-Aware Tool Metrics

**Files:**
- Create: `src/handler/tool_metrics.rs`
- Modify: `src/handler.rs:40-318`
- Modify: `src/handler.rs:2421-2938`
- Modify: `src/tools/metrics/session.rs:87-203`
- Modify: `src/database/tool_calls.rs:10-155`
- Modify: `src/database/schema.rs`
- Modify: `src/database/migrations.rs`
- Modify: `src/daemon/database.rs:543-665`
- Test: `src/tests/core/handler.rs`
- Test: `src/tests/daemon/database.rs`

**What to build:** Replace success-only tool call metrics with outcome-aware metrics. A tool call must produce one metrics row whether it succeeds, returns an execution error, or hits a tool-declared validation/match failure.

**Step 1: Write failing tests**

Add tests with these names and assertions:
- `test_tool_failure_metrics_records_failed_handler_call`
  - Arrange a handler with an indexed temp workspace.
  - Call `get_symbols` with an invalid `mode` value or another typed handler error that reaches the handler.
  - Assert the MCP call returns an error.
  - Poll the project `tool_calls` table.
  - Expected after fix: one row for `get_symbols`, `success = 0`, `output_bytes = 0`, metadata contains an `error.message`.
  - Current behavior: no row.
- `test_tool_success_rate_counts_recorded_handler_failures`
  - Arrange a handler with `daemon_db`.
  - Trigger one successful tool call and one failed tool call.
  - Assert `get_tool_success_rate(workspace_id, days)` returns `(2, 1)`.
  - Current behavior: `(1, 1)` or `(0, 0)` depending on attribution.
- `test_edit_file_validation_errors_are_recorded_as_failures`
  - Call `edit_file` with empty `old_text`.
  - Assert the tool outcome is treated as failure in metrics, not a successful `"Error: ..."` response.

Run each test once to verify RED:
```bash
cargo nextest run --lib test_tool_failure_metrics_records_failed_handler_call 2>&1 | tail -10
cargo nextest run --lib test_tool_success_rate_counts_recorded_handler_failures 2>&1 | tail -10
cargo nextest run --lib test_edit_file_validation_errors_are_recorded_as_failures 2>&1 | tail -10
```

**Step 2: Add outcome fields and schema support**

Implement these concrete data changes:
- Add `input_bytes: Option<u64>` to `ToolCallReport`.
- Add `success: bool` and `input_bytes: Option<u64>` to `MetricsTask`.
- Add `input_bytes INTEGER` to project and daemon `tool_calls` tables.
- Update insert functions to accept `input_bytes` before `source_bytes` or after `output_bytes`; use one consistent order in both DB types.
- Update summary structs to keep existing fields stable and add `total_input_bytes`.

Migration requirements:
- New databases create the column.
- Existing databases get `ALTER TABLE tool_calls ADD COLUMN input_bytes INTEGER`.
- Migration is idempotent.

**Step 3: Centralize metrics writes**

Move metrics task construction and `run_metrics_writer` to `src/handler/tool_metrics.rs`. Keep these exported items `pub(crate)`:
- `MetricsTask`
- `run_metrics_writer`
- `build_failure_metadata(base: serde_json::Value, error: &str) -> serde_json::Value`
- `merge_metadata(base: serde_json::Value, additions: serde_json::Value) -> serde_json::Value`

Keep `record_tool_call` as a success wrapper so existing tests have a stable entry point. Add:
- `record_tool_call_outcome(..., success: bool)`
- `record_tool_failure(tool_name, duration, metadata, input_bytes, source_file_paths, workspace_snapshot, error_message)`

Failure rows must:
- increment in-memory call counters
- write to the project DB when available
- write to daemon DB when available
- publish dashboard live-feed event
- set `success=false`
- set `output_bytes=0`
- include `error.message` in metadata

**Step 4: Instrument every public handler method**

For all public tool handlers in `src/handler.rs`, replace `?`-before-recording with explicit `match`.

Required handlers:
- `fast_search`
- `fast_refs`
- `call_path`
- `get_symbols`
- `deep_dive`
- `get_context`
- `blast_radius`
- `spillover_get`
- `rename_symbol`
- `manage_workspace`
- `edit_file`
- `rewrite_symbol`

Success path:
- preserve current metadata and source path extraction.
- compute `input_bytes` from serialized tool params for every public handler method.
- call `record_tool_call`.

Failure path:
- preserve current MCP error mapping.
- record failure before returning the mapped `McpError`.
- use typed input metadata from `tool_targets`.
- include known input source paths, such as `get_symbols.file_path` and `edit_file.file_path`.

Do not parse formatted output to detect errors. Tool-declared validation failures must return `Err` or produce explicit failure metadata from the tool implementation.

**Step 5: Convert edit/refactor validation failures**

Change validation and match failures that currently return successful `"Error: ..."` text into real errors or explicit failure reports:
- `src/tools/editing/edit_file.rs:364-389`
- `src/tools/editing/rewrite_symbol.rs:485-665`
- `src/tools/refactoring/mod.rs:176-188`

Use `anyhow!` errors inside tool implementations, allowing handler instrumentation to record them consistently.

**Step 6: Verify GREEN and commit**

Run:
```bash
cargo nextest run --lib test_tool_failure_metrics_records_failed_handler_call 2>&1 | tail -10
cargo nextest run --lib test_tool_success_rate_counts_recorded_handler_failures 2>&1 | tail -10
cargo nextest run --lib test_edit_file_validation_errors_are_recorded_as_failures 2>&1 | tail -10
```

Expected: PASS.

Commit:
```bash
git add src/handler.rs src/handler/tool_metrics.rs src/tools/metrics/session.rs src/database/tool_calls.rs src/database/schema.rs src/database/migrations.rs src/daemon/database.rs src/tests/core/handler.rs src/tests/daemon/database.rs src/tools/editing/edit_file.rs src/tools/editing/rewrite_symbol.rs src/tools/refactoring/mod.rs
git commit -m "fix(metrics): record failed tool calls"
```

## Task 2: Bounded and Faster Blast Radius

**Files:**
- Modify: `src/tools/impact/walk.rs:19-184`
- Modify: `src/tools/impact/mod.rs:1-493`
- Create: `src/tools/impact/likely_tests.rs`
- Modify: `src/database/identifiers.rs:54-350`
- Modify: `src/database/schema.rs`
- Modify: `src/database/migrations.rs`
- Modify: `src/database/bulk_operations.rs`
- Modify: `src/tests/core/performance_indexes.rs`
- Test: `src/tests/tools/blast_radius_tests.rs`
- Test: `src/tests/tools/blast_radius_determinism_tests.rs`

**What to build:** Make `blast_radius` traversal bounded before ranking, and fix the identifier query shape so common names do not dominate runtime or output.

**Step 1: Write failing tests**

Add tests with these names and assertions:
- `test_walk_impacts_caps_identifier_fanout_for_common_names`
  - Build a fixture where a seed named `new` has more identifier containers than the configured per-name cap.
  - Include resolved `target_symbol_id` rows and unresolved name-only rows.
  - Assert resolved rows are retained first.
  - Assert total identifier-derived candidates for `new` is capped.
  - Assert relationships table candidates are not replaced by identifier fallback candidates.
- `test_blast_radius_limit_bounds_depth_frontier`
  - Build a two-depth fixture where depth 1 has many candidates.
  - Run `blast_radius` with small `limit`.
  - Assert depth 2 traversal only uses the bounded frontier and output remains deterministic across repeated calls.
- `test_identifier_name_kind_container_index_exists`
  - Assert `idx_identifiers_name_kind_containing` exists on fresh schema.

Run RED:
```bash
cargo nextest run --lib test_walk_impacts_caps_identifier_fanout_for_common_names 2>&1 | tail -10
cargo nextest run --lib test_blast_radius_limit_bounds_depth_frontier 2>&1 | tail -10
cargo nextest run --lib test_identifier_name_kind_container_index_exists 2>&1 | tail -10
```

**Step 2: Add walk budget types**

In `src/tools/impact/walk.rs`, add:
- `WalkBudget`
- `WalkStats`, including `depth`, `relationship_edges_seen`, `identifier_edges_seen`, `identifier_edges_retained`, and `frontier_retained`.
- `walk_impacts_with_budget(db, seed_symbols, max_depth, budget)`.

Keep `walk_impacts(db, seed_symbols, max_depth)` as a compatibility wrapper for existing tests, using the default budget.

Default budget rules:
- `max_frontier_per_depth`: derived from tool `limit`, minimum 100, maximum 500.
- `max_identifier_edges_per_name`: default 50.
- `max_unresolved_identifier_edges_per_name`: default 10.
- relationships table edges are evaluated before identifier fallback.
- resolved identifier edges outrank unresolved identifier edges.
- common unresolved names over the cap are truncated deterministically, not allowed to fan out unbounded.

Candidate ordering must remain deterministic:
- relationship priority
- resolved target before unresolved
- reference score descending
- file path
- start line
- symbol name
- symbol id as final tie-breaker

**Step 3: Make `limit` meaningful during traversal**

In `src/tools/impact/mod.rs`, derive the walk budget from the request:
- `page_limit = tool.limit.max(1) as usize`
- `max_frontier_per_depth = (page_limit * 10).clamp(100, 500)`
- use the default per-name identifier caps from `WalkBudget`.

Call `walk_impacts_with_budget` instead of `walk_impacts`.

**Step 4: Move likely-test code out of `mod.rs`**

Create `src/tools/impact/likely_tests.rs` and move:
- `LikelyTests`
- `collect_likely_tests`
- `push_unique`
- `finalize_likely_tests`
- `sort_identifier_refs`
- `is_test_symbol`

Expose only what `mod.rs` needs. This keeps `mod.rs` below the 500-line target while touching it.

Do not change likely-test behavior in this step except import paths.

**Step 5: Fix identifier lookup query shape**

Add composite index:
```sql
CREATE INDEX IF NOT EXISTS idx_identifiers_name_kind_containing
ON identifiers(name, kind, containing_symbol_id)
```

Update:
- fresh schema
- migrations
- bulk index creation after fresh indexing
- tests in `performance_indexes.rs`

Refactor `get_identifiers_by_names_kinds_excluding_containers`:
- Run exact-name lookup separately with `name IN (...) AND kind IN (...) AND containing_symbol_id IS NOT NULL`.
- Run qualified-prefix lookup separately.
- Avoid one giant `OR` clause mixing exact and prefix matches with kind filters.
- Preserve existing escaping semantics from `escape_sql_like`.
- Deduplicate results by `(name, file_path, start_line, containing_symbol_id, target_symbol_id, kind)`.

If prefix lookup still needs `LIKE`, keep it isolated so exact lookup gets the composite index. Do not force a full table scan for exact names.

**Step 6: Verify GREEN and commit**

Run:
```bash
cargo nextest run --lib test_walk_impacts_caps_identifier_fanout_for_common_names 2>&1 | tail -10
cargo nextest run --lib test_blast_radius_limit_bounds_depth_frontier 2>&1 | tail -10
cargo nextest run --lib test_identifier_name_kind_container_index_exists 2>&1 | tail -10
```

Run metric probes after `cargo build`:
```bash
cargo build
/usr/bin/time -p ./target/debug/julie-server tool blast_radius --workspace . --params '{"file_paths":["src/tools/impact/mod.rs"],"include_tests":false,"limit":12,"max_depth":2,"format":"compact"}' 2>&1 | tail -30
/usr/bin/time -p ./target/debug/julie-server tool blast_radius --workspace . --params '{"file_paths":["src/watcher/runtime.rs","src/watcher/events.rs","src/watcher/queue.rs","src/watcher/mod.rs","src/watcher/handlers.rs"],"include_tests":false,"limit":12,"max_depth":2,"format":"compact"}' 2>&1 | tail -30
```

Expected hard gates:
- first command under 2 seconds
- watcher command under 4 seconds

Commit:
```bash
git add src/tools/impact/walk.rs src/tools/impact/mod.rs src/tools/impact/likely_tests.rs src/database/identifiers.rs src/database/schema.rs src/database/migrations.rs src/database/bulk_operations.rs src/tests/core/performance_indexes.rs src/tests/tools/blast_radius_tests.rs src/tests/tools/blast_radius_determinism_tests.rs
git commit -m "fix(impact): bound blast radius graph expansion"
```

## Task 3: Edit Tool Efficiency and Friction Telemetry

**Files:**
- Modify: `src/handler/tool_targets.rs`
- Modify: `src/tools/metrics/session.rs`
- Modify: `src/tools/editing/edit_file.rs:93-432`
- Modify: `src/tools/editing/rewrite_symbol.rs:55-740`
- Modify: `src/tools/refactoring/mod.rs:101-260`
- Modify: `src/tools/refactoring/rename.rs`
- Test: existing edit/refactor test files under `src/tests/tools/`
- Test: `src/tests/core/handler.rs`

**What to build:** Add enough metrics to answer whether Julie edit tools are saving tokens and where agents abandon them.

**Step 1: Write failing tests**

Add tests with these names and assertions:
- `test_edit_file_metrics_include_input_and_edit_outcome`
  - Dry-run an edit.
  - Assert tool metrics metadata includes `dry_run=true`, `file_size_bytes`, `old_text_bytes`, `new_text_bytes`, `input_bytes`, `diff_bytes`, `changed_bytes`, `occurrence`, `match_mode`, and `applied=false`.
- `test_edit_file_apply_metrics_record_conversion_outcome`
  - Apply the same edit.
  - Assert metadata includes `dry_run=false`, `applied=true`, changed byte count, and diff byte count.
- `test_rewrite_symbol_metrics_include_symbol_span_and_failure_kind`
  - Trigger one ambiguous or missing symbol failure.
  - Assert a failure row exists with `success=false`, `operation`, `symbol`, and `failure_kind`.
- `test_rename_symbol_metrics_include_reference_and_change_counts`
  - Dry-run a rename.
  - Assert metadata includes `dry_run`, `scope`, `reference_count`, `changed_file_count`, and `changed_line_count`.

Run RED:
```bash
cargo nextest run --lib test_edit_file_metrics_include_input_and_edit_outcome 2>&1 | tail -10
cargo nextest run --lib test_edit_file_apply_metrics_record_conversion_outcome 2>&1 | tail -10
cargo nextest run --lib test_rewrite_symbol_metrics_include_symbol_span_and_failure_kind 2>&1 | tail -10
cargo nextest run --lib test_rename_symbol_metrics_include_reference_and_change_counts 2>&1 | tail -10
```

**Step 2: Define edit metadata contract**

Use these metadata keys:
- `kind`: `"edit_file"`, `"rewrite_symbol"`, or `"rename_symbol"`
- `dry_run`: boolean
- `applied`: boolean
- `input_bytes`: integer
- `file_size_bytes`: integer when known
- `diff_bytes`: integer when a diff exists
- `changed_bytes`: integer when content changed
- `failure_kind`: stable string for failures

Tool-specific keys:
- `edit_file`: `old_text_bytes`, `new_text_bytes`, `occurrence`, `match_mode`
- `rewrite_symbol`: `operation`, `symbol`, `symbol_span_bytes`, `content_bytes`, `match_count`
- `rename_symbol`: `old_name`, `new_name`, `scope`, `reference_count`, `changed_file_count`, `changed_line_count`

Use stable snake_case keys. Do not store full edit content in metadata.

**Step 3: Emit metadata from tool implementations**

Add small outcome structs in the editing/refactoring modules. The handler must not parse human-readable output to infer outcome.

Implementation requirements:
- `edit_file` records match mode from `apply_edit`.
- `rewrite_symbol` records symbol span bytes before and after rewrite.
- `rename_symbol` records reference count and computed line changes from existing rename planning.
- Validation and match failures must expose a stable `failure_kind`.

**Step 4: Integrate with metrics handler**

Success reports should include:
- `input_bytes`
- output bytes
- source file paths
- edit metadata

Failure reports should include:
- `input_bytes`
- known file paths
- `success=false`
- `failure_kind`
- no full replacement text

**Step 5: Verify GREEN and commit**

Run:
```bash
cargo nextest run --lib test_edit_file_metrics_include_input_and_edit_outcome 2>&1 | tail -10
cargo nextest run --lib test_edit_file_apply_metrics_record_conversion_outcome 2>&1 | tail -10
cargo nextest run --lib test_rewrite_symbol_metrics_include_symbol_span_and_failure_kind 2>&1 | tail -10
cargo nextest run --lib test_rename_symbol_metrics_include_reference_and_change_counts 2>&1 | tail -10
```

Commit:
```bash
git add src/handler/tool_targets.rs src/tools/metrics/session.rs src/tools/editing/edit_file.rs src/tools/editing/rewrite_symbol.rs src/tools/refactoring/mod.rs src/tools/refactoring/rename.rs src/tests/tools src/tests/core/handler.rs
git commit -m "feat(metrics): track edit tool efficiency"
```

## Task 4: Dashboard and Query Surface Update

**Files:**
- Modify: `src/dashboard/routes/metrics.rs`
- Modify: `dashboard/templates/metrics.html`
- Modify: `dashboard/templates/partials/metrics_table.html`
- Modify: `dashboard/templates/partials/metrics_summary.html`
- Modify: `src/tools/metrics/query.rs`
- Test: `src/tests/dashboard/integration.rs`
- Test: `src/tests/tools/metrics/query_tests.rs`

**What to build:** Surface the corrected success rate and input/output/source-byte fields without turning the dashboard into a spreadsheet cosplay incident.

**Step 1: Write failing tests**

Add focused tests that verify:
- `test_metrics_page_counts_failed_tool_calls_in_success_rate`: success rate includes failed rows.
- `test_metrics_table_renders_input_bytes_for_tools`: tool summaries include input bytes when present.
- `test_query_metrics_formats_null_input_bytes`: query formatting can show input/source/output bytes without panicking when old rows have `NULL input_bytes`.

Run:
```bash
cargo nextest run --lib test_metrics_page_counts_failed_tool_calls_in_success_rate 2>&1 | tail -10
cargo nextest run --lib test_metrics_table_renders_input_bytes_for_tools 2>&1 | tail -10
cargo nextest run --lib test_query_metrics_formats_null_input_bytes 2>&1 | tail -10
```

**Step 2: Update summaries and display**

Dashboard requirements:
- Keep current call count, average duration, p95, source bytes, output bytes, saved bytes.
- Add input bytes where it helps interpret edit tools.
- Do not treat `input_bytes` as LLM prompt truth for non-edit tools; label it as Julie request bytes or edit request bytes.
- Display failure counts or success rate in a way that makes recorded failures visible.

Metrics query requirements:
- Include `input_bytes` in aggregate result structs.
- Preserve compatibility with old rows where `input_bytes IS NULL`.

**Step 3: Verify and commit**

Run:
```bash
cargo nextest run --lib test_metrics_page_counts_failed_tool_calls_in_success_rate 2>&1 | tail -10
cargo nextest run --lib test_metrics_table_renders_input_bytes_for_tools 2>&1 | tail -10
cargo nextest run --lib test_query_metrics_formats_null_input_bytes 2>&1 | tail -10
```

Commit:
```bash
git add src/dashboard/routes/metrics.rs dashboard/templates/metrics.html dashboard/templates/partials/metrics_table.html dashboard/templates/partials/metrics_summary.html src/tools/metrics/query.rs src/tests/dashboard/integration.rs src/tests/tools/metrics/query_tests.rs
git commit -m "feat(metrics): surface failure and edit efficiency data"
```

## Lead Integration

After all task commits land:

1. Review diffs for:
   - no string-parsing of `"Error:"` to infer failure
   - no unbounded identifier frontier path in `blast_radius`
   - no full edit content stored in metadata
   - schema changes applied to fresh, migrated, daemon, and project DBs
   - `src/tools/impact/mod.rs` remains under 500 lines after moving likely-test code

2. Run affected-change gate:
```bash
cargo xtask test changed
```

3. Run metric probes:
```bash
cargo build
/usr/bin/time -p ./target/debug/julie-server tool blast_radius --workspace . --params '{"file_paths":["src/tools/impact/mod.rs"],"include_tests":false,"limit":12,"max_depth":2,"format":"compact"}' 2>&1 | tail -30
/usr/bin/time -p ./target/debug/julie-server tool blast_radius --workspace . --params '{"file_paths":["src/watcher/runtime.rs","src/watcher/events.rs","src/watcher/queue.rs","src/watcher/mod.rs","src/watcher/handlers.rs"],"include_tests":false,"limit":12,"max_depth":2,"format":"compact"}' 2>&1 | tail -30
sqlite3 -header -column ~/.julie/daemon.db "select tool_name,count(*) total,sum(success) succeeded from tool_calls where timestamp >= strftime('%s','now','-1 hour') group by tool_name order by total desc;"
```

4. Run branch gate:
```bash
cargo xtask test dev
```

5. If daemon DB migration or handler lifecycle behavior changed substantially, also run:
```bash
cargo xtask test system
```

6. Save a Goldfish checkpoint before final commit or PR:
```text
Use mcp__goldfish__checkpoint with tags ["metrics", "blast-radius", "edit-tools"].
```

## Acceptance Criteria

- [ ] Failed handler/tool calls are recorded as `success=false` in project and daemon `tool_calls`.
- [ ] Success-rate queries include failure rows in the denominator.
- [ ] Validation and match failures in edit/refactor tools are not counted as successes.
- [ ] `blast_radius` depth 2 traversal is bounded during the walk.
- [ ] `blast_radius` hard-gate metric probes meet the local time thresholds.
- [ ] Identifier lookup has a name-first composite index and exact-name query path.
- [ ] Edit tools record request/input bytes and outcome metadata without storing full edit content.
- [ ] Dashboard/query surfaces expose corrected failure and edit-efficiency data.
- [ ] `cargo xtask test changed` passes after coherent batches.
- [ ] `cargo xtask test dev` passes before handoff.

## Risks

- Changing tool validation failures from successful text responses to errors could affect clients that expected `"Error: ..."` content. This is acceptable for metrics honesty, but review the user-facing message quality.
- Capping `blast_radius` can hide low-ranked impacts in giant fanout cases. That is better than flooding agents with thousands of noisy callers, but the output must disclose when traversal was capped.
- Adding schema columns requires both daemon and project DB paths to migrate cleanly. A missing migration would make the dashboard lie in a new and slightly more sophisticated way.
- Edit input-byte metrics are approximations. They measure Julie request payloads, not total harness prompt tokens.
