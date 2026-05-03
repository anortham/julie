# Workspace Service Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Split workspace service responsibilities so registry storage, runtime pooling, watcher ownership, and session attachment have clear boundaries.

**Architecture:** `WorkspacePool` should own live `JulieWorkspace` runtimes, not registry policy, watcher lifetime policy, and session attachment policy all at once. Registry commands use a registry store facade, watcher attachment stays in `WatcherPool`, and `JulieServerHandler` delegates primary workspace/session attachment through a small service instead of open-coded helper chains.

**Tech Stack:** Rust, tokio `RwLock`, daemon `DaemonDatabase`, `WatcherPool`, `JulieWorkspace`, handler session state, cargo nextest, xtask system and reliability tiers.

---

## File Structure

- Modify: `src/daemon/workspace_pool.rs`
  - Keep runtime cache responsibilities: `get`, `get_or_init`, `mark_indexed`, `sync_indexed_from_db`, `projection_inputs`, `indexing_snapshots`, and eviction.
  - Remove direct session-count and watcher-attachment policy from this type if new services own those decisions.
- Create: `src/daemon/workspace_registry_store.rs`
  - Wrap `DaemonDatabase` workspace registry reads/writes used by open, cleanup, register, remove, and list commands.
  - Keep storage errors and missing-daemon errors consistent.
- Create: `src/daemon/workspace_session_attachment.rs`
  - Own attach/detach semantics for sessions, active workspace IDs, and loaded workspace binding transitions.
  - Coordinate `WorkspacePool` runtime refs with `WatcherPool` refs without making either type know command policy.
- Modify: `src/daemon/watcher_pool.rs`
  - Keep watcher ownership here: `increment_ref`, `decrement_ref`, `attach`, `detach`, `reap_expired`, `pause_workspace`, and `resume_workspace`.
  - Do not move registry cleanup or primary workspace switching policy into watcher code.
- Modify: `src/tools/workspace/commands/registry/open.rs`
  - Route registry lookup, auto-prune, runtime init, and session attachment through the split services.
- Modify: `src/tools/workspace/commands/registry/cleanup.rs`
  - Use registry store and service-level activity checks instead of reaching into both `WorkspacePool` and `WatcherPool` ad hoc.
- Modify: `src/handler.rs`
  - Reduce primary workspace helpers around `attach_daemon_primary_binding_if_needed`, `teardown_loaded_workspace`, `acquire_pooled_workspace_for_rebind`, `primary_workspace_snapshot_from_pool`, `activate_workspace_with_root`, `switch_primary_workspace_with_root`, and `ensure_primary_pool_membership_for`.
- Test: existing workspace lifecycle tests under `src/tests/daemon/`, `src/tests/tools/workspace/`, `src/tests/integration/daemon_lifecycle.rs`, and primary rebind tests under `src/tests/tools/*primary_rebind*`.

## Implementation Tasks

### Task 1: Introduce A Registry Store Facade

**Files:**
- Create: `src/daemon/workspace_registry_store.rs`
- Modify: `src/daemon/mod.rs`
- Modify: `src/tools/workspace/commands/registry/open.rs:33`
- Modify: `src/tools/workspace/commands/registry/cleanup.rs:191`
- Test: `src/tests/daemon/workspace_cleanup.rs`

Write a failing test that proves a missing-path workspace is pruned through the same registry operation whether the caller is `handle_open_command` auto-prune or cleanup sweep. The point is to remove the current command-level duplication around `DaemonDatabase`, workspace IDs, and index directory lookup.

Acceptance criteria:
- Registry commands do not directly compose low-level `DaemonDatabase` calls when a store method exists.
- `delete_workspace_if_allowed` keeps its current safety checks, but gets registry and index-dir operations from the store.
- Missing daemon mode still returns the existing helpful errors.

### Task 2: Keep Runtime Pooling Focused

**Files:**
- Modify: `src/daemon/workspace_pool.rs:20`
- Modify: `src/tests/daemon/workspace_pool.rs`

Refactor `WorkspacePool` into a runtime pool with a narrow contract: initialize or return `JulieWorkspace`, expose snapshots, mark indexed state, and evict runtimes. Session counts and watcher refs should move out unless a temporary adapter is needed for compatibility.

Acceptance criteria:
- `WorkspacePool::get_or_init` still initializes `JulieWorkspace` with shared storage and shared embedding provider.
- Runtime cache behavior is unchanged for dashboard search and tool targeting.
- No command code uses `WorkspacePool` as a proxy for registry policy.

### Task 3: Add Session Attachment Service

**Files:**
- Create: `src/daemon/workspace_session_attachment.rs`
- Modify: `src/handler.rs:584`
- Modify: `src/handler.rs:776`
- Modify: `src/handler.rs:1240`
- Modify: `src/handler.rs:2033`
- Modify: `src/handler.rs:2059`
- Test: `src/tests/daemon/handler.rs`
- Test: `src/tests/integration/stale_index_detection.rs`

Create a small service that owns "this session is attached to this workspace" semantics. It should be responsible for incrementing/decrementing runtime refs, coordinating watcher refs, marking attached workspace IDs, and restoring primary bindings after swaps.

Acceptance criteria:
- `JulieServerHandler` keeps user-facing tool behavior, but primary workspace helper methods stop duplicating attach/rebind logic.
- `was_workspace_attached_in_session`, `session_attached_workspace_ids`, and `active_workspace_ids` still report the same state.
- Rebinding current primary workspace through `switch_primary_workspace_with_root` still uses pooled storage when available.

### Task 4: Clarify Watcher Ownership

**Files:**
- Modify: `src/daemon/watcher_pool.rs:37`
- Modify: `src/daemon/workspace_session_attachment.rs`
- Modify: `src/startup.rs:328`
- Test: `src/tests/daemon/roots.rs`
- Test: `src/tests/integration/daemon_lifecycle.rs`

Make `WatcherPool` the only owner of watcher lifecycle, while session attachment decides when watcher refs change. `WorkspacePool` should not attach watchers as a side effect of runtime lookup unless the new attachment service explicitly asks it to.

Acceptance criteria:
- Ref counts remain balanced across attach, detach, primary swap, session close, and cleanup.
- `WatcherPool::reap_expired` and `remove_if_inactive` keep their grace-period behavior.
- Startup pause/resume paths still call `pause_workspace` and `resume_workspace` for the right workspace ID.

### Task 5: Update Command Wiring Without Changing Tool Contracts

**Files:**
- Modify: `src/tools/workspace/commands/registry/open.rs:33`
- Modify: `src/tools/workspace/commands/registry/cleanup.rs:373`
- Modify: `src/tools/workspace/commands/registry/register_remove.rs`
- Modify: `src/tools/workspace/commands/registry/list_clean.rs`
- Test: `src/tests/tools/workspace/mod_tests.rs`
- Test: `src/tests/tools/workspace/global_targeting.rs`

Keep MCP output and command semantics stable while routing through the new service boundaries. This is a refactor plan, not a behavior redesign.

Acceptance criteria:
- `open`, `register`, `remove`, `list`, `clean`, and cleanup sweep continue to return the same user-visible results for existing tests.
- Opening a known workspace row does not preactivate it in a new session unless the current command attaches it.
- Cleanup does not delete a workspace with live indexing, live watcher refs, or an attached session.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, and `docs/TESTING_GUIDE.md`.

**Worker red/green scope:** Each worker writes the failing test first and runs only the exact test it owns:
`cargo nextest run --lib <exact_test_name> 2>&1 | tail -10`

**Worker ceiling:** Workers may run at most their exact test filter, twice per fix cycle: once RED, once GREEN. Workers must not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test reliability`, or broad `cargo nextest run --lib`.

**Worker gate invariant:** The worker report must state the lifecycle invariant proved, for example "session close decrements watcher ref count and keeps runtime eviction blocked while indexing is active."

**Lead affected-change scope:** After each coherent batch, the lead runs:
`cargo xtask test changed`

**Branch gate:** Before handoff, the lead runs:
`cargo xtask test dev`

**Specialist gates:** Because this touches workspace lifecycle, daemon runtime state, watcher refs, and session attachment, the lead also runs:
`cargo xtask test system`
`cargo xtask test reliability`

**Dogfood trigger:** Add `cargo xtask test dogfood` only if the split changes search targeting, projection input collection, or dashboard/search code paths that depend on `WorkspacePool::projection_inputs`.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless this plan is explicitly updated to change that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse an existing passing ledger entry for the same HEAD and scope instead of rerunning an expensive gate.

## Model Routing

**Project source of truth:** `RAZORBACK.md`. Do not copy the global model table into this plan. If a local sentence conflicts with `RAZORBACK.md`, `RAZORBACK.md` wins.

**Plan-specific overrides:** Workspace session attachment, watcher refs, runtime ref counts, cleanup safety, primary workspace repair, and handler/session binding are shared-invariant work. Use Codex `gpt-5.3-codex high` for bounded workspace lifecycle implementation and `gpt-5.3-codex xhigh` for watcher/session ref-count bugs, restart interactions, or terminal-heavy debugging.

**Worker eligibility:** Use implementation-tier workers only for narrow store, command, or test tasks with non-overlapping files. Use coupled implementation or lead-owned work for handler/session attachment and watcher ref-count changes.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Worker A: registry store facade and cleanup/open tests. Write scope: `src/daemon/workspace_registry_store.rs`, registry command files, matching tests.
- Worker B: runtime-only `WorkspacePool` changes. Write scope: `src/daemon/workspace_pool.rs`, `src/tests/daemon/workspace_pool.rs`.
- Worker C: session attachment service. Write scope: `src/daemon/workspace_session_attachment.rs`, focused handler helpers, matching daemon/primary rebind tests. Coupled implementation tier.
- Worker D: watcher ownership and ref-count tests. Write scope: `src/daemon/watcher_pool.rs`, startup pause/resume touch points, daemon lifecycle tests. Coupled implementation tier.
- Worker E: command contract cleanup for `open`, `register`, `remove`, `list`, and `clean` after services exist. Write scope must not overlap Worker C or D.
- Lead: integration review, check that user-visible MCP command output did not drift, then run changed/dev/system/reliability gates.

## Risks

- Ref-count bugs are easy to hide because the happy path still opens a workspace. Tests must assert counts and cleanup blocking, not just command success.
- `JulieServerHandler` currently has many primary workspace helpers in one file. A partial split can make the code worse if it creates a service but leaves the old helper chain doing half the work.
- Cleanup safety depends on live indexing, watcher refs, and attached sessions. Any new store facade must not turn cleanup into a pure registry delete.
- Dashboard and tool targeting rely on `WorkspacePool` snapshots and projection inputs. Keep those contracts stable unless the plan is updated with dashboard/search verification.
- Startup root reconciliation and primary workspace repair are lifecycle code. If touched, the lead owns system and reliability gates, no exceptions.
