# Dead Code Audit Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Use dead-code evidence to remove test fossils and stale helpers exposed by the architecture cleanup, without adding a new MCP tool before the core lifecycle and projection boundaries are stable.

**Architecture:** Treat `.claude/skills/dead-code-audit` as the audit workflow for this stage. The plan creates repeatable evidence and cleanup gates, then limits production changes to deletion, privatization, or call-site merge work found on the path of the architecture refactors. Productizing a first-class Julie dead-code tool is intentionally a later plan after handler, workspace, and projection boundaries settle.

**Tech Stack:** Julie SQLite symbol graph, existing dead-code audit skill/script, `julie-server signals`, Rust code cleanup, `cargo nextest`, `cargo xtask`.

---

## File Structure

**Use As Evidence**
- `.claude/skills/dead-code-audit/SKILL.md`
- `.claude/skills/dead-code-audit/scripts/dead_code_inventory.py`
- `src/analysis/early_warnings.rs`
- `src/cli_tools/mod.rs:342-372`

**Modify During Cleanup**
- Architecture-plan files that expose fossils, such as:
  - `src/search/projection.rs`
  - `src/tools/workspace/indexing/pipeline.rs`
  - `src/daemon/lifecycle.rs`
  - `src/daemon/mod.rs`
  - `src/adapter/mod.rs`
  - `src/daemon/workspace_pool.rs`
  - `src/tools/workspace/commands/registry/open.rs`
  - `src/tools/workspace/commands/registry/cleanup.rs`

**Do Not Create In This Plan**
- No new MCP tool in `src/tools/`.
- No new handler registration in `src/handler.rs`.
- No new dashboard route.

The product tool version gets its own plan after the architecture cleanup proves which dead-code signals were useful and which were noisy.

## Implementation Tasks

### Task 1: Audit Baseline And Scope Ledger

**Files:**
- Modify: `docs/plans/verification-ledger-template.md` only if the testing-contract plan has already created it.
- Create: `docs/plans/2026-05-03-dead-code-audit-baseline.md`

**What to build:** Capture a read-only baseline of dead-code candidates before deleting anything. The baseline should record inventory sections, early-warning signals, projection freshness, commit SHA, and which architecture stage owns each candidate.

**Approach:** Use the skill workflow:
- `manage_workspace(operation="health", detailed=true)` to confirm index freshness.
- `manage_workspace(operation="stats")` to get the workspace ID.
- `python3 .claude/skills/dead-code-audit/scripts/dead_code_inventory.py --workspace-id <workspace_id> --limit 80`.
- `./target/debug/julie-server signals --workspace . --fresh --limit 80` after `cargo build` if the debug binary is stale.

The baseline is evidence only. It should not recommend deletion without symbol-level verification.

**Acceptance criteria:**
- [ ] Baseline records workspace ID, commit SHA, index health, and commands used.
- [ ] Candidates are grouped by planned cleanup stage: projection, lifecycle, transport, workspace runtime, or unrelated.
- [ ] Each candidate has an initial label from the skill: `delete`, `make-private`, `merge-into-caller`, `test-fossil`, `graph-gap`, `keep`, or `needs-design-review`.
- [ ] No production code is changed in this task.

### Task 2: Per-Stage Candidate Verification

**Files:**
- Modify only the baseline document from Task 1.

**What to build:** For each candidate that belongs to the active architecture stage, verify whether it is a real cleanup target or a graph false positive.

**Approach:** For every candidate before recommending a code edit:
- Resolve it with `fast_search(query="<name>", search_target="definitions", file_pattern="<path>")`.
- Check references with `fast_refs(symbol="<name>", include_definition=true, limit=200)`.
- Inspect behavior with `deep_dive(symbol="<name>", context_file="<path>", depth="context")`.
- Search non-graph usage with `fast_search(query="<name>", search_target="content", limit=50)`.
- Check impact with `blast_radius(file_paths=["<path>"], include_tests=true, max_depth=2)`.

Do not rely on zero reference score as proof. Dynamic entry points, trait hooks, CLI dispatch, MCP registrations, parser factories, dashboard routes, config strings, and macro usage are keep or review signals.

**Acceptance criteria:**
- [ ] Every cleanup recommendation has symbol-level evidence, not only inventory output.
- [ ] Every `graph-gap` candidate includes the missing product usage evidence.
- [ ] Every `test-fossil` candidate names the test that preserves it and whether that test should be removed or rewritten.
- [ ] Shared lifecycle, indexing, database, search ranking, parser extraction, and MCP contract candidates are labeled `needs-design-review` unless the active architecture plan already owns the cleanup.

### Task 3: Projection And Indexing Fossil Cleanup

**Files:**
- Modify only files touched by `docs/plans/2026-05-03-projection-invariants.md`.
- Test files named by the projection plan.

**What to build:** Remove, privatize, or merge projection/indexing symbols that are real fossils after the projection invariant plan has reduced duplicate projection paths.

**Approach:** This task runs inside the projection invariant implementation, not before it. Candidate examples to verify include duplicated projection helpers, one-off compatibility shortcuts, and tests that only preserve document-count blessing or duplicated projection behavior. Do not delete compatibility behavior until the projection plan replaces it with explicit stale-projection handling and tests.

**Acceptance criteria:**
- [ ] No deleted symbol has product references outside the projection plan's new path.
- [ ] Tests that existed only to preserve old projection shortcuts are removed or rewritten to assert the new invariant.
- [ ] Projection plan worker-scope verification passes.
- [ ] Lead runs projection affected-change and dogfood gates named in the projection plan.

### Task 4: Daemon, Adapter, And Workspace Fossil Cleanup

**Files:**
- Modify only files touched by the daemon lifecycle, HTTP transport, or workspace service split plans.
- Test files named by those plans.

**What to build:** Remove lifecycle, adapter, workspace, and watcher helpers that become stale after responsibility moves into the lifecycle controller, HTTP transport boundary, or workspace service split.

**Approach:** Cleanup is subordinate to the active architecture plan. Do not delete old adapter or IPC helpers until the replacement transport path has parity tests. Do not remove workspace session bookkeeping until the workspace service split has an authoritative runtime owner and migration tests.

**Acceptance criteria:**
- [ ] Stale lifecycle helpers are removed only after controller tests cover their old transition cases.
- [ ] Stale adapter/IPC helpers are removed only after stdio shim and HTTP parity tests pass.
- [ ] Stale workspace pool helpers are removed only after registry/runtime/watcher tests pass.
- [ ] Lead runs `cargo xtask test reliability` when cleanup touches daemon, adapter, watcher, restart, or workspace lifecycle behavior.

**Task 4 execution notes so far:**
- Adapter/IPC candidates from the baseline were re-checked after HTTP stdio parity. `forward_streams`, `ReadyOutcome`, `ForwardOutcome`, `BranchOutcome`, `forward_bytes`, `connect_and_handshake`, `read_daemon_ready`, and `build_ipc_header` are keep or graph-gap cases while legacy IPC remains a migration fallback.
- `flag_restart_pending_for_restart` was a real lifecycle test fossil after `DaemonLifecycleController::mark_restart_pending` became the runtime owner. The remaining shutdown-phase invariant moved to a controller test before the helper and old state tests were deleted.
- `store_phase` was merged into the private lifecycle publish helper, and state-file write helpers were narrowed to `pub(crate)` for daemon state tests.
- `WorkspacePool::new` no longer accepts migration-only watcher or embedding arguments. Session lifecycle ownership lives in `WorkspaceSessionAttachment`; the pool now only owns shared workspace instances and daemon registry persistence.
- `WorkspacePool::active_count` was a test fossil. Tests now assert observable cache behavior through `get` and `get_or_init` instead of exposing the pool map length.
- Julie graph evidence had two useful misses during this pass. `fast_refs(WorkspacePool::active_count)` initially conflated same-named methods, and `blast_radius(src/daemon/workspace_pool.rs)` returned extractor-heavy unrelated callers. Raw `rg` and focused tests were the deciding evidence.
- `WatcherPool::increment_ref` and `WatcherPool::decrement_ref` were removed from the public API. `attach` now owns increments, `detach` owns saturated decrement and grace-period startup, and tests use the product lifecycle instead of raw ref-count hooks.

### Task 5: Product Tool Readiness Decision

**Files:**
- Create: `docs/plans/2026-05-03-dead-code-tool-readiness.md`

**What to build:** Decide, from cleanup evidence, whether Julie should productize a dead-code audit tool and what its initial contract should be.

**Approach:** Summarize the cleanup campaign:
- number of candidates reviewed
- number deleted, privatized, merged, kept, and labeled graph-gap
- false-positive causes
- which evidence sources were useful
- which evidence sources were noisy

If productization earns its keep, write a separate product plan that creates the tool after this architecture series. The likely product shape is an analysis module plus CLI command first, MCP tool second only if the CLI output proves stable.

**Acceptance criteria:**
- [ ] Readiness document includes counts by decision label.
- [ ] Readiness document names graph gaps that should become Julie bugs or regression tests.
- [ ] Productization decision is explicit: CLI-only first, MCP later, or no product tool.
- [ ] No handler or MCP surface is added by this cleanup plan.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, `.claude/skills/dead-code-audit/SKILL.md`, and the active architecture plan.

**Worker red/green scope:** Cleanup workers run the exact tests assigned by the active architecture plan for the file they touch. Pure audit-document tasks do not run code tests unless they modify executable scripts.

**Worker ceiling:** Workers may run exact tests only. Audit workers may run read-only Julie tools and the inventory script. Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test dogfood`, or `cargo xtask test reliability`.

**Worker gate invariant:** The gate proves either that a candidate is correctly classified, or that behavior still passes after deleting, privatizing, or merging the symbol.

**Lead affected-change scope:** The lead runs the affected-change gate from the active architecture plan after cleanup lands. For projection cleanup, include the projection/search gate. For lifecycle, transport, or workspace cleanup, include `cargo xtask test reliability` when those plans require it.

**Branch gate:** The active architecture plan owns branch gates. This cleanup plan never weakens them.

**Replay/metric evidence:** Candidate counts and label counts are report-only. Exact tests and architecture gates are hard gates.

**Escalation triggers:** Escalate any candidate in daemon lifecycle, watcher ownership, indexing persistence, search scoring, MCP handler registration, parser factory registration, or public CLI surface when evidence is not decisive.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless the active architecture plan explicitly says to update that gate.

**Verification ledger:** Record candidate, decision label, evidence commands, exact test command, scope label, commit SHA, result, and timestamp.

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Do not copy the global model table into this plan. If a local sentence conflicts with `RAZORBACK.md`, `RAZORBACK.md` wins.

**Plan-specific overrides:** Candidate classification, graph-gap interpretation, dynamic dispatch evidence, registration-string evidence, public CLI/MCP surface, and cleanup safety are strategy or gate-review work. Use Codex `gpt-5.5 high` for ambiguous deletion decisions, `gpt-5.3-codex high` for evidence review and bounded cleanup implementation, and `gpt-5.3-codex xhigh` when graph evidence or terminal-heavy validation is messy.

**Worker eligibility:** Implementation-tier workers are eligible only when a candidate has completed Task 2 verification and belongs to the active architecture plan's write scope.

**Escalation triggers:** Escalate when evidence depends on dynamic dispatch, registration strings, trait impls, macros, public CLI/MCP contracts, or graph behavior that may itself be wrong.

**Mechanical exclusion:** Mechanical workers cannot decide deletion, classify graph gaps, or own failing tests.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Task 1 creates the audit baseline.
- Task 2 verifies candidates for the currently active architecture stage.
- Task 3 performs projection/indexing cleanup only inside the projection invariant plan.
- Task 4 performs lifecycle/transport/workspace cleanup only inside those architecture plans.
- Task 5 writes the product-tool readiness decision after cleanup evidence exists.

Tasks 1 and 2 are read-only except for plan evidence documents. Tasks 3 and 4 run only when their corresponding architecture plan is active. Task 5 runs after at least one architecture cleanup stage has produced real candidate outcomes.

## Risks

- Dead-code reports are noisy by nature. Treating inventory output as deletion proof would cause real damage.
- Test fossils can hide weak tests. Removing the fossil often requires deleting or rewriting the test, not preserving the old behavior under a new name.
- Graph gaps are Julie bugs, not cleanup wins. They should feed search/navigation quality work.
- Productizing this too early would expand the MCP handler and tool surface while the architecture cleanup is trying to shrink and clarify it.
