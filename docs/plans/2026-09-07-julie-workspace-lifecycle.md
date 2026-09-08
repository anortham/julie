# Julie workspace lifecycle and concurrent worktrees implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Let concurrent agents use full Julie functionality across worktrees without duplicate index writers, permanently stranded followers, lost file changes, or request-owned workspace state.

**Architecture:** Keep a process-local manager of immutable workspace runtimes and one OS-lock-elected index owner per physical workspace. Requests borrow runtime handles; their cancellation cannot destroy ownership or interrupt a committing mutation. Source edits use a separate cross-process edit lock and file-content preconditions; SQLite remains canonical and Tantivy is a recoverable projection. No global daemon is required.

**Tech Stack:** Rust, Tokio, existing OS advisory lock implementation, SQLite WAL, Tantivy, immutable request-engine bindings, existing watcher and projection recovery.

**Architecture Quality:** High risk. Correctness depends on distinct ownership, request, source-edit, and index-commit lifetimes. Preserve the existing canonical/projection revision machinery and mutation proof tokens, add a real ownership proof, and verify transitions with separate OS processes. Do not substitute process IDs or heartbeat timestamps for an acquired OS lock.

## Global Constraints

- Work in `/home/murphy/source/julie`. Prerequisite: accepted `2026-09-07-julie-request-engine-mcp-cli.md` and its extractor migration prerequisite. That earlier plan remains independently shippable.
- This plan changes lifecycle and concurrency, not retrieval ranking or the embedding backend. Preserve the request-engine semantic seam for `/home/murphy/source/julie-semantic-sidecar` integration in its separate plan.
- Stateless MCP means a self-contained request. It does not mean a new process, new index, or new model for each call. Do not reintroduce a global daemon, HTTP MCP bridge, per-request server process, or implicit primary workspace.
- No continuous testing. Do not modify upstream extractor/sidecar repositories from this plan.
- Index paths, index-owner lock, index-mutation state, publication lock and continuation storage derive from the same canonical workspace binding and selected index root. Source-edit locks/journals derive only from the canonical source root, independent of index mode or `JULIE_HOME`.
- Different worktree roots get distinct workspace IDs, SQLite files, Tantivy directories, source-edit locks, revision state, and handles. Shared `.git` metadata is never workspace identity.
- All eight existing index writers retain mutation-gate serialization. In addition, every writer must carry an OS-owner proof. Followers are index readers even when they are allowed to edit source files.
- No read timeout or transport disconnect cancels an in-progress canonical transaction, loses queued watcher work, or releases the owner lock while its writer is still active.
- New implementation files <=500 lines; tests under package `src/tests/`, <=1,000 lines per file; fixture data under `fixtures/`. Split touched legacy modules by responsibility.
- Root Codex performs final implementation review. Workers do not commit; the lead reconciles worktree state and the verification ledger before staging.

## Source grounding

Inspected Julie `0432158c173c30ae2f76d92773380c744ddc6542`, `main`, on 2026-09-07 before prerequisite execution. Existing memory/findings/plans were dirty and preserved. Re-resolve exact signatures after the request-engine plan lands.

| Existing file/symbol | Verified behavior |
|---|---|
| `src/server_in_process.rs:168`, `run_in_process_server` | Acquires one workspace leader lock at process startup and constructs `LeadershipState::leader` or `follower`. |
| `src/leadership.rs:23`, `LeadershipState` | Stores optional lock and in-process flag; current constructors describe a fixed role. |
| `crates/julie-core/src/workspace/leader_lock.rs:57`, `DaemonLockGuard::try_acquire` | Existing OS lock plus in-process duplicate-acquisition protection. Reuse it; naming cleanup must not change exclusion semantics. |
| `crates/julie-core/src/workspace/mutation_gate.rs:83`, `Registry::acquire` | Provides process-local async serialization and `MutationGuard`; it is not a cross-process ownership lock. |
| `crates/julie-runtime/src/workspace/mod.rs:33`, `JulieWorkspace` | Owns live SQLite/search/watcher-related runtime data. Move lifetime management around it rather than copying it into every request. |
| `src/handler.rs:1264`, `acquire_mutation_gate`; `:1189`, `is_in_process_follower` | Existing gate and role checks are integration points for explicit owner proofs. |
| `src/handler/tools/edit_file.rs:26`, `edit_file` | Rejects a follower before checking dry_run. `prepare_edit` and `call_prepared` already separate preparation/application. |
| `crates/julie-tools/src/editing/edit_file.rs:175`, `EditFileTool` | Fields are `file_path`, `old_text`, `new_text`, `workspace`, `dry_run`, `occurrence`; prepared edits already test changed-target rejection. |
| `src/startup.rs:307`, `reconcile_projection_lag_if_needed` | Detects canonical > projected revision and rebuilds derived search from canonical SQLite. Preserve and promote to every ownership-acquisition path. |
| `crates/julie-index/src/search/projection.rs:43`, `ensure_current_from_database` | Existing idempotent projection recovery, used by startup and watcher recovery. |
| `src/tests/integration/t9_handoff_recovery.rs:86` | Existing crash-gap fixture stores canonical rows without Tantivy projection and proves reconciliation. Extend it to actual ownership turnover. |
| `src/tests/core/handler/t9_bounded_read.rs:234` | Existing deferred-repair single-flight regression. Preserve the invariant through the manager. |
| `src/tests/integration/concurrent_mcp.rs:353` | Existing concurrent MCP fixture and edit exercises. Add cross-process failover proof; task concurrency alone is insufficient. |
| `crates/julie-tools/src/spillover/store.rs` | Re-export boundary for existing spillover behavior; trace its backing store before replacing handle lifetime. |

`impact(LeadershipState)` reaches constructors, server startup, and bounded-read tests. `trace(ensure_current_from_database)` reaches startup, watcher repair, and dashboard fixtures. Existing recovery is not missing; the missing contract is reliable ownership turnover and independent request lifetime.

## Fixed lifecycle design

New interfaces below are proposed, not existing names.

```rust
pub enum RuntimePhase {
    Opening,
    Follower,
    Recovering { epoch: u64 },
    Owner { epoch: u64 },
    Draining,
    Failed { code: String },
}
pub struct WorkspaceRuntime {
    pub binding: WorkspaceBinding,
    pub phase: tokio::sync::watch::Receiver<RuntimePhase>,
    pub workspace: std::sync::Arc<JulieWorkspace>,
}
pub struct RuntimeLease {
    pub runtime: std::sync::Arc<WorkspaceRuntime>,
}
pub struct WriterPermit<'a> {
    owner: &'a OwnerEpoch,
    mutation: julie_core::workspace::mutation_gate::MutationGuard<'a>,
}
```

Put `OwnerEpoch` and `WriterPermit` in new `crates/julie-core/src/workspace/ownership.rs`, alongside the lock/gate primitives. Root orchestration owns phase transitions but lower crates consume only julie-core proofs. Never make julie-runtime or julie-index depend on root-package orchestration. Expose no unrestricted proof constructor; acquisition consumes a real `DaemonLockGuard` and a canonical binding identity.

The manager holds a strong runtime reference while its owner worker, watcher, recovery or pending commit exists. A request holds `RuntimeLease`; dropping it only decrements request use. Start with at most eight idle runtimes per process, idle expiry 60 seconds, no expiry while requests/queued work/commits exist. Runtime shutdown stops admission, drains queued canonical work, stops watcher tasks, closes writer/search handles, then releases the OS lock. Process exit performs bounded drain; an OS-killed process is recovered through canonical revisions next time.

Owner election uses the existing lock file. A follower checks acquisition on demand before a writer-required operation and through one background probe per attached workspace, every 500ms with up to 100ms jitter. Busy lock leaves it a follower. Permission/I/O failure becomes a structured unavailable state, not a follower impersonation. Successful acquisition first constructs a new owner epoch, opens writer handles, repairs canonical/projection lag, and installs one watcher, then publishes Owner. Requests can read the last published projection during recovery with explicit lag evidence; `freshness=required` waits to deadline or returns `INDEX_NOT_READY`.

Do not serialize source edits under the index owner. All source-edit apply paths take `<canonical-root>/.julie/locks/source-edit.lock`, revalidate prepared file hashes under that lock, then apply. Journals live in `<canonical-root>/.julie/edit-journals/`. Shared-index CLI, standalone CLI, different `JULIE_HOME` values and MCP therefore contend on the same source lock. A follower may preview without this exclusive lock and may apply through the same edit path as the owner. It never opens a writable canonical/index connection. `source-edit.lock` serializes Julie writers, not arbitrary editors; reread immediately before replacement and never overwrite a known changed file. Cross-tool rename/rewrite use the same coordinator. Failure to create/open the source lock or journal returns `SOURCE_EDIT_UNAVAILABLE` before source writes; previews remain available on a read-only root.

For multi-file edits, hold the workspace edit lock once, prepare all replacements, validate all before writing, and write a journal containing per-file old/new hashes, durable before/after payloads and staged filenames. Persist the journal and payloads before replacing the first source file. After each atomic file replacement, update its disposition. A crash may leave partial application: on next use report `EDIT_RECOVERY_REQUIRED` with the `edit_id`, exact changed/pending paths, and both explicit recovery commands. `julie-server workspace recover-edit --edit-id ID --action resume|rollback --workspace /absolute/root` maps to `manage_workspace` operation `recover_edit`, with `edit_id` and `recovery_action` fields. Task 3 implements this command under the source-root lock and hash guards; recovery never runs automatically. Never overwrite intervening external edits. A source-edit command returns `applied_paths`, `pending_paths`, and `index_refresh_pending`; it does not claim index freshness just because source writes succeeded.

SQLite canonical revision and Tantivy projected revision stay separate. A read response reports `canonical_revision`, `projected_revision`, `source_check_state` and `owner_phase`. Equal revisions alone do not prove the disk was checked. An owner transaction writes canonical rows and revision atomically, then updates Tantivy, commits/reloads it, then stamps the projection revision. Source file hashes are verified again before canonical commit; changed extraction input is requeued, never stamped current. Hold a brief shared publication lock while acquiring a coherent SQLite read snapshot and Tantivy searcher; owner publication takes its exclusive form. The reader verifies equal generation/revision stamps after acquisition; on mismatch it retries within deadline or returns `PROJECTION_LAG`, not a mixed-generation answer.

Continuation handles are explicit immutable result snapshots. Their meaning survives a new CLI process or MCP connection using the same home/index root. They are bound to workspace, canonical/projection generation, tool, normalized arguments, ordering and source hashes; they are never paths, cursor offsets into a mutable current result, or client-selected SQL. Exact storage rules are in task 5.

## Resource admission contract

Lifecycle owns **index/extraction jobs only**. A pool of OS lock slots at `$JULIE_HOME/scheduler/index-{n}.lock` limits active jobs across processes sharing that canonical `JULIE_HOME`. Intentional separate homes have independent quotas; this is not a machine-wide guarantee across isolated homes or test fixtures. Default is `min(2, available_parallelism)` with minimum one; `JULIE_MAX_INDEX_JOBS` may explicitly select 1–8. Every process sharing a home reads one atomically created pool configuration; conflicting active settings return `RESOURCE_CONFIG_CONFLICT` rather than opening extra slot names. Reconfiguration requires no active slot holders and an exclusive configuration lock.

Each runtime coalesces dirty file paths into at most 4,096 pending entries. Overflow sets `rescan_required` and clears per-path excess; no event silently disappears. One queued job per workspace and FIFO among that process's workspaces avoid duplicate queued work. Target a 10-second scheduling quantum, checked between completed file commits. Existing `extract_canonical` is synchronous and lacks cooperative cancellation: a single file may exceed the quantum. Retain the host permit until it actually completes, enforce existing source-size limits before reading, and report longest-file duration plus admission wait. Request timeouts stop waiting without pretending to preempt this owner work. Cross-process lock acquisition uses cancellable 50–100ms backoff and deadline; the OS does not promise strict fairness, so report starvation observations rather than claiming global FIFO. A resumed chunk releases the host slot before requeueing.

Acquire host index admission before the per-workspace mutation gate. Never hold a SQLite mutex while waiting for a host slot. Drop all index permits before semantic provider initialization or inference. The native semantic implementation owns its own inference fairness, device memory and batching. This plan consumes only the prerequisite `SemanticRuntime::ensure_ready` deadline/cancellation interface and reports its status. No duplicate embedding scheduler is introduced here.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/TESTING_GUIDE.md`, package manifests and `docs/plans/verification-ledger-template.md`.

**Worker red/green scope:** Root integration tests: `cargo nextest run -p julie --lib <exact_name>`; runtime tests: `cargo nextest run -p julie-runtime --lib <exact_name>`; core lock tests: `cargo nextest run -p julie-core --lib <exact_name>`; projection tests: `cargo nextest run -p julie-index --lib <exact_name>`. Run `cargo check -p <changed-package>` before GREEN.

**Worker ceiling:** One exact test per RED/GREEN cycle, two runs, one test process at a time. No xtask tiers from workers. Process tests consume a prebuilt absolute `JULIE_TEST_BIN`; never compile inside a test.

**Worker gate invariant:** Separate-process tests prove exclusion/failover; task-level tests prove request cancellation/queue coalescing. Neither substitutes for the other.

**Lead affected-change scope:** `cargo xtask test changed`; on OverBudget record rationale and use `cargo xtask test changed --scale` or the documented fast alternative.

**Branch gate:** `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test reliability`. Reuse overlapping passing evidence only where scope and exact HEAD match; do not treat inventory as a gate.

**Security scope:** none declared. Source path containment, forged handle rejection and edit conflict preservation are correctness gates here.

**Replay/metric evidence:** Hard gates: exactly one owner/writer/watcher per workspace, no cross-worktree contamination, recoverable commit boundaries, source edits preserved, truthful revisions, bounded configured jobs and valid handles. Report latency, RSS, CPU, disk growth, admission waits and recovery duration; performance budget selection belongs to the comparison plan.

**Escalation triggers:** Platform lock differences require Linux and Windows process checks and macOS when available. No Linux-only proof may be labeled cross-platform. Missing a platform runner is explicit outstanding verification, not a reason to weaken lock/file replacement behavior.

**Assigned verification failure:** Diagnose the owned slice; escalate an incompatible proof-token or runtime-interface shape to the lead. Never bypass a gate to finish the plan.

**Verification ledger:** Record invariant, command, scope, exact HEAD, result and UTC timestamp. For fault injection record phase and observed canonical/projected revisions. Keep the ledger empty until execution.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| 1: Runtime/owner lifecycle | None - serial | `src/workspace_runtime/{mod,manager,owner,shutdown}.rs`, `src/leadership.rs`, `src/server_in_process.rs`, `src/request_engine/runtime_factory.rs`, `src/lib.rs`, `src/tests/runtime_lifecycle.rs` | Yes | Defines leases, epochs and owner-transition API consumed by all later tasks. |
| 2: Bounded index jobs | Batch A | `src/workspace_runtime/{scheduler,dirty_queue}.rs`, `crates/julie-core/src/workspace/{host_slots,mod}.rs`, `src/tests/runtime_scheduler.rs`, `crates/julie-core/src/tests/host_slots.rs` | No | None - safe parallel batch. |
| 3: Source edits on followers | Batch A | `src/workspace_runtime/{source_edit,edit_journal}.rs`, `src/handler/tools/{edit_file,rename_symbol,rewrite_symbol,manage_workspace}.rs`, `src/tools/workspace/commands/{mod,recover_edit}.rs`, `src/cli_tools/{subcommands,commands}.rs`, `crates/julie-tools/src/editing/{edit_file,rewrite_symbol}.rs`, `crates/julie-tools/src/refactoring/{mod,rename}.rs`, `src/tests/{follower_source_edits,edit_recovery_contract}.rs` | No | None - safe parallel batch. |
| 4: Writer proof and revision publication | None - serial | `src/workspace_runtime/{owner,publication,recovery}.rs`, `src/startup.rs`, `src/handler.rs`, `crates/julie-core/src/workspace/{ownership,mutation_gate}.rs`, `crates/julie-index/src/search/projection.rs`, exact writer paths in the table below, `src/tests/runtime_recovery.rs` | Yes | Requires task 1 and scheduler integration. |
| 5: Durable continuation snapshots | None - serial | `src/workspace_runtime/{continuation,continuation_store}.rs`, `src/handler/tools/spillover_get.rs`, `crates/julie-tools/src/spillover/{mod,store}.rs`, `src/tests/runtime_continuation.rs` | Yes | Depends on task 4 generation stamps and source-edit invalidation. |
| 6: Multi-process/worktree acceptance | None - serial | `src/tests/{workspace_process_lifecycle,request_process_helpers}.rs`, `src/tests/mod.rs`, `fixtures/workspace-lifecycle/`, `docs/{WORKSPACE_ARCHITECTURE,DEVELOPMENT,TESTING_GUIDE}.md` | Yes | Proves the final integrated binary and owns shared test-module wiring. |

The lead owns module declarations in `src/workspace_runtime/mod.rs` and package test roots during Batch A; workers submit declaration changes for lead application. Tests are still serialized even when editing lanes are parallel. Task 4 owns the following verified writer entry paths; re-resolve renamed prerequisite paths before dispatch, without dropping any writer.

| Writer | Exact current entry path and symbol |
|---|---|
| Watcher event processing | `crates/julie-runtime/src/watcher/runtime/processing.rs:52`, `process_queue_batch` and shutdown drain in the same file |
| Watcher repair scan | `crates/julie-runtime/src/watcher/runtime/repairs.rs:194`, `run_repair_scan_if_needed` |
| Watcher repair replay | `crates/julie-runtime/src/watcher/runtime/repairs.rs:4`, `retry_persisted_repairs` |
| Watcher Tantivy retry | `crates/julie-runtime/src/watcher/runtime/projection.rs:4`, `retry_dirty_tantivy`; commit helpers in the same file |
| Catch-up repair/index | `src/startup.rs:77`, `run_primary_workspace_repair`, with `src/tools/workspace/commands/index.rs:39`, `handle_index_command_with_guard` |
| Force reindex | `src/tools/workspace/commands/index.rs:57`, `handle_index_command_internal` |
| Refresh stats | `src/tools/workspace/commands/registry/refresh_stats.rs:26`, `refresh_workspace_internal` |
| Register | `src/tools/workspace/commands/registry/register_remove.rs:15`, `handle_register_command` |

Task 4 also owns proof threading through `crates/julie-runtime/src/watcher/{runtime,handlers,extraction_write}.rs`, `crates/julie-runtime/src/watcher/observability.rs`, and its workspace mutation-gate re-export. The lead runs impact on the changed proof signatures and assigns any newly exposed compile-time caller explicitly before another worker edits it.

Commit mode: `parallel-lead-commit` for all tasks. Workers report checkout, branch, HEAD, dirty state, owned paths, tests and remaining concerns; the lead does inline review and final Codex implementation review. No implementation pauses between accepted slices.

### Task 1: Make runtime lifetime independent of the requesting connection

**Files:** Task 1 table row; new manager modules are root-package orchestration, preserving lower-crate dependency direction.

**Interfaces:** Consume immutable `WorkspaceBinding` and the prerequisite runtime factory. Produce `WorkspaceRuntimeManager::acquire(binding, deadline, cancellation) -> RuntimeLease`, a single-flight constructor, and owner transition notifications. Keep request-engine signatures unchanged.

**Contract inputs:** Existing `DaemonLockGuard::try_acquire`, `LeadershipState`, `JulieWorkspace`, startup recovery, and OS-lock-based exclusion. No PID-based takeover.

**File ownership:** Task 1 row. **Serialization required:** Yes. **Dependency reason:** Establishes ownership and lifecycle contracts.

**Step 1: Write the failing test.** Build a marked temporary workspace with an isolated home and the real runtime manager. An instrumented owner worker waits on a test-controlled barrier before completion; dropping a requesting lease must not cancel it.

```rust
#[tokio::test]
async fn dropping_request_lease_does_not_release_busy_owner() {
    let fixture = RuntimeFixture::with_blocked_index_commit().await;
    let lease = fixture.manager.acquire(fixture.binding(), fixture.deadline(), fixture.cancel()).await.unwrap();
    fixture.wait_until_commit_started().await;
    drop(lease);
    assert!(fixture.competing_process_lock_is_busy().await);
    fixture.release_commit();
    fixture.wait_until_idle().await;
    assert_eq!(fixture.committed_revision(), 1);
}
```

Define `RuntimeFixture` under `src/tests/runtime_lifecycle.rs`; its competing lock check launches the shared process helper, not a second task in the same process. Its committed revision reads actual SQLite through a read-only connection.

**Step 2: Verify RED.** `cargo nextest run -p julie --lib dropping_request_lease_does_not_release_busy_owner`.

**Step 3: Implement.** Manager construction is single-flight per `(workspace_id,index_root)`. Failed construction removes its entry; waiting caller cancellation does not cancel a construction still needed by another caller. Roles are a state machine, not booleans copied into handlers. Owner epoch increases on acquisition and is diagnostic/fencing metadata; the held OS guard remains the authority.

```rust
pub async fn attach_owner(runtime: &RuntimeControl) -> Result<(), RuntimeError> {
    let guard = match DaemonLockGuard::try_acquire(&runtime.leader_lock_path()) {
        Ok(guard) => guard,
        Err(AcquireError::AlreadyHeld(_)) => return Ok(()),
        Err(error) => return Err(RuntimeError::owner_lock(error)),
    };
    let owner = runtime.begin_recovery(guard).await?;
    owner.reconcile().await?;
    owner.start_single_watcher().await?;
    runtime.publish_owner(owner).await
}
```

These are new orchestration methods. `begin_recovery` retains the lock through failure cleanup; `reconcile` may not publish Owner early. If watcher startup fails, stop partial tasks, close writer handles and return Failed before releasing the lock. Graceful shutdown sequence is stop admission → drain commits → stop watcher → close writable handles → release owner guard. A follower retry never force-unlocks a live owner.

**Step 4: Verify GREEN.** `cargo check -p julie`; exact RED command. Lead additionally verifies simultaneous attach constructs once, failed attach retries, idle eviction skips work, a dead owner is acquired, and a live slow owner is never stolen.

**Step 5: Handoff.** Report runtime states, transition errors, exact ownership drop order, and process-test proof. No worker commit.

**Acceptance criteria:**
- [x] Request cancellation/disconnect cannot terminate an active owner commit or release its lock.
- [x] A follower can become owner after OS lock release without restarting its MCP session.
- [x] Exactly one watcher starts after recovery, and failed startup leaves no hidden writer.
- [x] Idle runtime eviction is bounded and does not evict active work.

### Task 2: Bound indexing across processes and worktrees

**Files:** Task 2 row; the lead applies module registrations.

**Interfaces:** Produce `IndexJobAdmission::acquire(deadline, cancellation) -> IndexJobPermit`, host slot configuration, and `DirtyQueue::{record,take_chunk}`. No semantic-provider scheduling API is introduced.

**Contract inputs:** Fixed resource admission contract above. Index owner can remain owner while queued; it must not hold a mutation guard or SQLite lock while waiting for a host slot.

**File ownership:** Task 2 row. **Serialization required:** No. **Dependency reason:** None - safe parallel batch.

**Step 1: Write the failing test.** Test overflow as an explicit rescan, not lost individual updates.

```rust
#[test]
fn dirty_queue_overflow_requests_rescan_without_unbounded_growth() {
    let mut queue = DirtyQueue::new(4096);
    for n in 0..5000 { queue.record(format!("source/file_{n}.rs")); }
    assert!(queue.rescan_required());
    assert!(queue.path_count() <= 4096);
    queue.record("source/last.rs".into());
    let chunk = queue.take_chunk();
    assert!(chunk.rescan_required);
}
```

**Step 2: Verify RED.** `cargo nextest run -p julie --lib dirty_queue_overflow_requests_rescan_without_unbounded_growth`.

**Step 3: Implement.** Record renames as old-path delete plus new-path update; duplicate events collapse by canonical relative path. Directory deletions and overflow require a reconciliation scan. A scan snapshots a queue sequence before starting and does not clear events arriving afterward.

```rust
pub fn record(&mut self, path: String) {
    if self.rescan_required { return; }
    if self.paths.contains(&path) { return; }
    if self.paths.len() == self.limit {
        self.paths.clear();
        self.rescan_required = true;
        return;
    }
    self.paths.insert(path);
}
```

Host slots use the existing cross-platform lock guard and a configuration lock, not a process-local semaphore. Create slot files once and retain them; do not unlink a lock file while other processes can hold its inode. A job checks cancellation before acquisition and between safe extraction chunks. Commit a completed chunk atomically, release the permit, then requeue unfinished paths. Source changes during extraction are requeued by task 4's precommit validation.

Add `host_slots_limit_two_processes` under julie-core tests using three child contenders and a test barrier: two acquire, third is blocked, releasing one permits exactly one next acquisition. Distinguish expected busy from permission/I/O failure. Assert no extra slot path appears under conflicting settings. Semantic readiness waits occur only after dropping `IndexJobPermit`.

**Step 4: Verify GREEN.** `cargo check -p julie`; exact RED command. The lead builds/runs the core child-process exact test sequentially with `cargo nextest run -p julie-core --lib host_slots_limit_two_processes`.

**Step 5: Handoff.** Include queue limits, permit acquisition order, configurable host limits and fault behavior.

**Acceptance criteria:**
- [x] At most the configured number of index jobs runs across OS processes.
- [x] Queue overflow, rename and deletion remain recoverable without unbounded memory.
- [x] No host permit is held during embedding initialization/inference or while idle.
- [x] Cancelled waiters leave no leaked lock slot or stale queue entry.

### Task 3: Let followers preview and safely apply source edits

**Files:** Task 3 row; all three editing tools use one source-edit coordinator. Create `src/tools/workspace/commands/recover_edit.rs` and `src/tests/edit_recovery_contract.rs` as new files. Extend the existing `ManageWorkspaceTool` schema/operation enum in `src/tools/workspace/commands/mod.rs` and existing `WorkspaceArgs`/argument mapping in `src/cli_tools/{subcommands,commands}.rs`. Do not merely delete the follower guard.

**Interfaces:** Produce `SourceEditCoordinator::{preview,apply,recover}`, `PreparedSourceChange { path, before_hash, after_bytes }`, and `EditDisposition { edit_id, applied_paths, pending_paths, conflicted_paths, index_refresh_pending }`. `recover(edit_id, RecoveryAction::{Resume,Rollback}, context)` is available through the named CLI and generic MCP/application `manage_workspace` operation `recover_edit`. Preview handles, once task 5 lands, preserve these preconditions.

**Contract inputs:** Real `EditFileTool` fields and existing `prepare_edit/call_prepared`. Preserve path traversal/symlink safety and live parser-span revalidation for rewrite/rename. Source-edit access and index-write access are separate catalog classes. The extractor prerequisite supplies `parse_source_with_options(&Path, &str, &SyntaxOptions) -> Result<ParsedSource, SyntaxError>` with `SyntaxOptions { deadline: Option<std::time::Instant>, cancelled: Option<&AtomicBool>, max_source_bytes: usize }`; request preflight must use it.

**File ownership:** Task 3 row. **Serialization required:** No. **Dependency reason:** None - safe parallel batch.

**Step 1: Write the failing test.** Fixture holds owner in a separate process, constructs a follower, previews an edit, and modifies source externally before application.

```rust
#[tokio::test]
async fn follower_preview_is_allowed_but_stale_apply_preserves_external_edit() {
    let fixture = FollowerEditFixture::new().await;
    let preview = fixture.preview("before", "after").await.unwrap();
    std::fs::write(fixture.file(), "external_change").unwrap();
    let error = fixture.apply(preview).await.unwrap_err();
    assert_eq!(error.code, "EDIT_CONFLICT");
    assert_eq!(std::fs::read_to_string(fixture.file()).unwrap(), "external_change");
    assert_eq!(fixture.follower_index_writes(), 0);
}
```

`follower_index_writes` is a test observer at the actual canonical write entry, not a count of requested operations. Add a successful follower apply fixture with byte-identical source/control and prove owner watcher eventually updates canonical and projection revisions.

**Step 2: Verify RED.** `cargo nextest run -p julie --lib follower_preview_is_allowed_but_stale_apply_preserves_external_edit`.

**Step 3: Implement.** Preview reads live source and returns a diff without acquiring owner status. Apply takes the OS source-edit lock with request deadline, revalidates containment and hashes, prepares temp files in each target directory, and commits through the journal. Preserve permissions and LF/CRLF bytes. Empty/no-match/ambiguous matches fail before any write. Reject same-path symlink swaps and files replaced between preparation and commit when the live identity/hash differs.

Thread the actual request deadline and cancellation through every source-edit parse/diagnostic preflight. Mirror `CancellationToken` into an `Arc<AtomicBool>` for `SyntaxOptions`; convert the absolute deadline rather than starting a fresh timeout at each file. On expiry/cancellation signal the parser and join its cooperative worker; retain any admission guard until that join completes. Do not claim `spawn_blocking` abort stops parsing. Check cancellation again after preparation and lock acquisition, before the journaled commit boundary. Once commit starts, finish/rollback or record precise partial disposition. Add exact `cancelled_follower_preflight_never_commits_after_request_exit`, with a root-owned test worker-entry barrier around the real bounded API proving admission was held when cancellation arrived and final source bytes unchanged. Upstream tests own parser-internal cancellation proof.

Bound source reads before allocation: default `JULIE_MAX_EDIT_SOURCE_BYTES=16777216`, configurable from 1 byte to 256 MiB. Validate configuration before I/O. Open the source and read through a bounded reader capped at limit + 1; reject `SOURCE_TOO_LARGE` immediately on excess rather than allocating the full file and checking afterward. Check deadline/cancellation between read chunks and cap the proposed replacement size as well as the original file. Pass the same cap to `SyntaxOptions`.

Add post-edit syntax validation for AST-aware rename/rewrite as explicit new hardening beyond the extractor plan's preservation work. Parse every complete proposed source file before journal creation/commit, not just the changed span. Reject newly introduced ERROR/MISSING diagnostics anywhere in the file. Compare diagnostics by kind and byte span after translating unaffected old spans through the exact edit map; counts alone are not equivalent. Old diagnostics overlapping changed bytes are not automatically exempted. Preserve rename's existing stricter precondition for erroneous source; rewrite may retain only its already-supported unrelated pre-existing diagnostics. `edit_file` remains an explicitly raw text editor, including docs/config; do not impose an AST gate on it.

Add `ast_edit_new_syntax_error_leaves_all_files_unchanged`: stage a valid two-file rename/rewrite fixture, deliberately propose malformed full source in one file, apply through the real coordinator, assert `SYNTAX_REGRESSION`, and verify both files byte-for-byte against their before snapshots and no commit journal. Add `rewrite_preserves_unrelated_existing_diagnostic`: change a valid declaration elsewhere in a file with one unrelated pre-existing diagnostic, translate that diagnostic's span through the edit, and assert successful application preserves the same diagnostic kind at the translated location. Include equal-count-but-different-location errors, an error outside the edited span, a missing node, and a proposed file exceeding the read cap in the lead's exact-test batch.

Use the verified `RewriteSymbolTool` fields for the first executable regression. `AstEditFixture::two_files` creates `src/a.rs` with `pub fn first() {}` and `src/b.rs` with `pub fn second() {}`, indexes them through the real request engine, and exposes its source snapshots and source-root journal path.

```rust
#[tokio::test]
async fn ast_edit_new_syntax_error_leaves_all_files_unchanged() {
    let fixture = AstEditFixture::two_files().await;
    let before = fixture.source_snapshots();
    let error = fixture.execute("rewrite_symbol", serde_json::json!({
        "symbol":"first", "file_path":"src/a.rs", "operation":"replace_full",
        "content":"pub fn first( {", "dry_run":false
    })).await.unwrap_err();
    assert_eq!(error.code, "SYNTAX_REGRESSION");
    assert_eq!(fixture.source_snapshots(), before);
    assert_eq!(fixture.commit_journal_count(), 0);
}
```

Run `cargo nextest run -p julie --lib ast_edit_new_syntax_error_leaves_all_files_unchanged` before adding post-edit validation, then run `cargo check -p julie` and the same exact test for GREEN. A separate coordinator test stages changes to both files and makes the second proposed file invalid, proving validation of the complete batch precedes the first replacement.

```rust
pub fn validate_preconditions(changes: &[PreparedSourceChange]) -> Result<(), EditFailure> {
    for change in changes {
        let current = std::fs::read(&change.path).map_err(EditFailure::io)?;
        if content_hash(&current) != change.before_hash {
            return Err(EditFailure::conflict(change.path.clone()));
        }
    }
    Ok(())
}
```

The coordinator holds the workspace edit lock across validation and commit. A timeout before lock acquisition returns `EDIT_BUSY`; once journaled commit begins, finish or record precise partial disposition. A source edit does not take the index mutation gate or require promotion. Owner watcher observes normal filesystem changes; enqueue a durable refresh hint under the same index root for owner wakeup/future owner when no watcher is alive. This hint is a source notification, not a follower canonical DB write. Coalesce hints, drain them only after a successful owner scan, and never trust their file paths without containment checks.

Do not promise atomicity against arbitrary uncooperative external editors. Preserve detected conflicts, use safe platform replacement primitives, and return exact partial-commit state rather than overwriting later user changes during rollback. Task 6 exercises process termination between multi-file replacements.

Implement explicit recovery in `recover_edit.rs`. Validate `edit_id` as the journal's fixed opaque identifier, never a filename/path; locate it only inside the canonical source root's journal directory. Require `recovery_action` exactly `resume` or `rollback`. Hold the same source-root edit lock across journal validation, source hash checks and recovery writes. Verify both stored payload hashes and path containment before touching source. For resume, old-hash files receive their staged new payload, new-hash files are already complete, and any other current hash becomes a conflict. For rollback, new-hash files receive their saved old payload, old-hash files are already complete, and any other hash becomes a conflict. A missing or symlink-swapped file is a conflict, never permission to recreate/overwrite it blindly.

Persist the selected recovery action before its first replacement. Repeating the same `edit_id` and action resumes incomplete matching paths; once complete it returns the persisted receipt without applying again. An opposite action after a recovery action has started returns `RECOVERY_ACTION_CONFLICT` and performs no writes. Conflicting files remain byte-identical, and the response contains `edit_id`, `recovery_action`, `applied_paths`, `pending_paths`, `conflicted_paths`, and `index_refresh_pending`. Here applied paths mean the requested recovery action reached its desired bytes; rollback therefore reports successfully restored paths there. A partial conflict returns application code `EDIT_RECOVERY_CONFLICT` with that complete disposition and CLI exit 5. Do not remove the journal until a terminal receipt is durable; retain that receipt for idempotent retries. Recovery hints name the complete commands with explicit workspace. Publish refresh hints for files actually changed, not for conflicted files.

Add a real application/CLI recovery regression. `EditRecoveryFixture::partially_applied` writes a genuine two-file journal with durable old/new payloads, one file already at its new hash and the other at its old hash. Its `recover` invokes application dispatch with `{"operation":"recover_edit","edit_id":id,"recovery_action":"resume"}` and explicit workspace; it does not call the journal helper directly.

```rust
#[tokio::test]
async fn edit_recovery_resume_is_idempotent_and_preserves_conflicts() {
    let fixture = EditRecoveryFixture::partially_applied().await;
    std::fs::write(fixture.pending_file(), "external_change").unwrap();
    let error = fixture.recover("resume").await.unwrap_err();
    assert_eq!(error.code, "EDIT_RECOVERY_CONFLICT");
    assert_eq!(std::fs::read_to_string(fixture.pending_file()).unwrap(), "external_change");
    assert_eq!(error.details["edit_id"], fixture.edit_id());
    fixture.restore_pending_old_bytes();
    let completed = fixture.recover("resume").await.unwrap();
    let before_retry = fixture.source_snapshots();
    let repeated = fixture.recover("resume").await.unwrap();
    assert_eq!(repeated, completed);
    assert_eq!(fixture.source_snapshots(), before_retry);
}
```

Run `cargo nextest run -p julie --lib edit_recovery_resume_is_idempotent_and_preserves_conflicts` for RED, then `cargo check -p julie` and the same exact test for GREEN. Add a mirrored rollback case, opposite-action refusal, malformed/unknown ID, tampered payload, and named CLI-to-generic operation/schema equivalence to the lead's exact-test batch.

**Step 4: Verify GREEN.** `cargo check -p julie-tools` and `cargo check -p julie`; exact RED command. Lead runs successful follower apply, competing same-file apply, separate-worktree apply, CRLF/permissions and multi-file journal tests once.

Add exact process regression `shared_and_standalone_edits_share_source_preconditions`: prepare two edits against the same source bytes, start one process on shared storage and the other with `--standalone`, release their apply barriers together, and assert one succeeds and the other returns `EDIT_CONFLICT`; final bytes equal one complete replacement. Repeat with distinct `JULIE_HOME` paths. Read-only source metadata directories return `SOURCE_EDIT_UNAVAILABLE` without falling back to per-index locks.

**Step 5: Handoff.** Give the lead lock-order evidence, journal recovery policy, changed schemas and all edited tool paths. Update the prerequisite follower-refusal test to assert index-write refusal and source-edit acceptance only after this real replacement passes.

**Acceptance criteria:**
- [x] Follower dry-run works and writes no source/index bytes.
- [x] Follower apply uses the same lock and hash preconditions as owner apply.
- [x] Two Julie writers cannot both apply stale previews to the same file.
- [x] Follower source edits never write canonical SQLite or Tantivy; freshness is reported honestly.
- [x] Partial multi-file application is recoverable without overwriting later external edits.
- [x] Explicit named CLI and generic `recover_edit` commands resume/rollback under source-root hash guards, report conflicts and support idempotent retries.

### Task 4: Fence every index writer and make publication recoverable

**Files:** Task 4 row and the eight-writer table above, including the named downstream proof-threading files. Reconcile relocated crate paths after prerequisites without changing the ownership invariant.

**Interfaces:** Produce `OwnerEpoch::acquire_writer() -> WriterPermit`, `PublicationStamp { epoch, canonical_revision, projected_revision, generation }`, and `WorkspaceReadSnapshot`. Existing `MutationGuard` is nested in ownership proof; no public constructor can mint an owner proof for a follower.

**Contract inputs:** Existing `SearchProjection::ensure_current_from_database`, canonical/projection state tables, and `test_leader_handoff_recovery_reconciles_tantivy_from_canonical_sqlite`.

**File ownership:** Task 4 row plus the lead's explicit eight-writer file inventory. **Serialization required:** Yes. **Dependency reason:** Integrates owner transitions and scheduler admission before touching writer entry points.

**Step 1: Write the failing test.** Fault injection stops the first child owner after canonical commit but before Tantivy commit; the replacement child must repair without a source file change.

```rust
#[tokio::test]
async fn promoted_owner_repairs_projection_gap_without_source_change() {
    let mut fixture = RecoveryProcessFixture::new().await;
    fixture.owner_pause_after_canonical_commit().await;
    let canonical = fixture.canonical_revision();
    assert!(canonical > fixture.projected_revision());
    fixture.kill_owner().await;
    fixture.wait_for_follower_promotion().await;
    let result = fixture.search("recovered_symbol").await.unwrap();
    assert!(result.contains("recovered_symbol"));
    assert_eq!(fixture.projected_revision(), fixture.canonical_revision());
    assert_eq!(fixture.canonical_revision(), canonical);
}
```

The test hook is a fixture-controlled IPC barrier in a dedicated test-helper execution mode, unavailable during normal tool use. Do not use arbitrary sleeps to guess the commit boundary.

**Step 2: Verify RED.** Build the debug binary/helper, then `JULIE_TEST_BIN=/home/murphy/source/julie/target/debug/julie-server cargo nextest run -p julie --lib promoted_owner_repairs_projection_gap_without_source_change`.

**Step 3: Implement.** `WriterPermit` is acquired only by a live owner epoch while holding its lock. A draining epoch stops new permits and waits for outstanding permits to finish before releasing ownership. Adapt all eight writers; preserve compilation failures until every writer is proven. Remove standalone write bypasses now that request-engine construction uses ownership for both storage modes.

```rust
pub fn needs_projection_recovery(canonical: u64, projected: Option<u64>) -> bool {
    match projected {
        Some(revision) => revision != canonical,
        None => true,
    }
}
```

A projected revision ahead of canonical is also invalid; discard/rebuild the derived projection under owner proof instead of treating it as fresh. On acquisition, repair missing/corrupt Tantivy, behind/ahead revision, interrupted rebuild directory, and absent projection stamp through existing projection machinery. Preserve SQLite on derived corruption. Canonical SQLite corruption returns explicit repair-required, never silently drops source truth.

Add commit payload generation/revision metadata to Tantivy and compare it with SQLite state on read open. Publication uses `<index_root>/publication.lock`, a process-shared OS shared/exclusive lock, not a Tokio-only mutex: readers acquire SQLite read transaction plus searcher under shared lock and verify their stamps; writers acquire exclusive around final publication/stamping and schema replacement/drop. Extract/build outside publication lock. Never unlink this lock while processes may hold it. A retained read snapshot keeps compatible database/search handles until the request completes, preventing schema/drop or old-artifact reclamation from invalidating an active read; retire artifacts only after reader leases permit it. If a cross-store crash creates a mismatch, reads either use a retained coherent snapshot already open or return `PROJECTION_LAG` until recovery. Never combine latest SQLite graph rows with an older search index while claiming a coherent revision.

Before canonical commit, re-read each extracted source file hash. A removed/changed file is excluded from that batch and requeued, including deletes. Emit exact `source_check_state` so a matching canonical/projection revision is not mistaken for a recent disk scan. Replay is idempotent: killing after Tantivy commit but before SQLite projection stamp may repeat projection publication, but not advance canonical revision or duplicate symbols.

**Step 4: Verify GREEN.** `cargo check -p julie-core`, `cargo check -p julie-index`, `cargo check -p julie-runtime`, `cargo check -p julie`; exact RED command. Lead runs the existing handoff regression plus before-canonical/after-Tantivy/after-stamp crash phases and retained-reader publication tests.

**Step 5: Handoff.** Include all eight writer paths, proof-token coverage, crash boundary table, and the canonical/projection revisions observed for every process test.

**Acceptance criteria:**
- [x] Every canonical/derived writer requires live owner proof and process-local serialization.
- [x] All crash windows recover idempotently; source-unchanged projection lag is repaired on promotion.
- [x] Readers never claim a mixed canonical/search generation is coherent or current.
- [x] Source changes during extraction cannot be stamped as indexed.

### Task 5: Make continuations explicit and usable across connections

**Files:** Task 5 row. Trace existing spillover backing storage and reuse its result representation where possible; replace only its lifetime/binding contract.

**Interfaces:** Produce `ContinuationStore::{create,read,expire}` and `ContinuationBinding { workspace_id, tool, arguments_hash, generation, source_hashes }`. Keep the existing `SpilloverGetTool.spillover_handle` input name, verified at `crates/julie-tools/src/spillover/mod.rs:13`; emitted bounded results use `spillover_handle` plus an exact next-call example. Add explicit optional workspace to that input and normalize it through the application binding resolver. Do not create a second incompatible continuation field name.

**Contract inputs:** Published generation stamps from task 4. Continuation is a random 256-bit identifier, strictly hex-validated, looked up in a private SQLite store at `<index_root>/continuations.db`. The token never encodes a path or supplies SQL. Stored rows contain immutable rendered pages and binding metadata, expiry and last access; no client-supplied binding is trusted.

**File ownership:** Task 5 row. **Serialization required:** Yes. **Dependency reason:** Requires stable publication generation and source-edit invalidation.

**Step 1: Write the failing test.** Create the handle in one process and read it from a second; a different workspace must refuse the same token.

```rust
#[tokio::test]
async fn continuation_survives_new_process_but_rejects_other_worktree() {
    let fixture = ContinuationProcessFixture::two_worktrees().await;
    let first = fixture.create_page_in_first_process().await;
    let continued = fixture.read_in_new_process(first.token.clone()).await.unwrap();
    assert_eq!(continued.bytes, first.expected_next_page);
    let error = fixture.read_in_other_worktree(first.token).await.unwrap_err();
    assert_eq!(error.code, "CONTINUATION_INVALID");
}
```

**Step 2: Verify RED.** `JULIE_TEST_BIN=/home/murphy/source/julie/target/debug/julie-server cargo nextest run -p julie --lib continuation_survives_new_process_but_rejects_other_worktree`.

**Step 3: Implement.** Snapshot pages once, in deterministic stable order, and bind tool/arguments/workspace. Default TTL is 15 minutes, max snapshot 8 MiB, total continuation store budget 64 MiB per workspace. Exceeding an individual limit returns a truncated result with explicit no-continuation reason; it does not write an oversized snapshot. Expired snapshots are removed first; under pressure evict least recently accessed complete snapshots. A previously evicted handle returns `CONTINUATION_EXPIRED`, never a new unrelated page. Keep small tombstones for one TTL so expired and unknown handles remain distinguishable without unbounded storage.

```rust
pub fn validate_handle_binding(
    stored: &ContinuationBinding, requested: &ContinuationBinding,
) -> Result<(), ContinuationFailure> {
    if stored.workspace_id != requested.workspace_id
        || stored.tool != requested.tool
        || stored.arguments_hash != requested.arguments_hash
    {
        return Err(ContinuationFailure::invalid());
    }
    if stored.generation != requested.generation
        || stored.source_hashes != requested.source_hashes
    {
        return Err(ContinuationFailure::stale());
    }
    Ok(())
}
```

Readers do not recompute result ranking or reuse a numeric offset against new data. A generation/source change returns `CONTINUATION_STALE` with a restart command. An immutable snapshot can remain readable only when the caller explicitly requests historical snapshot mode; do not add that mode in this plan. Pagination tokens carry no authority to edit. Preview/application preconditions remain distinct even when a diff is paginated.

SQLite continuation writes are metadata-only and use their own short transactions, allowed to any process; they never touch canonical symbols/projections. The store is private to the current user, and index-root containment is checked before opening. List/status reports continuation bytes/entry count without scanning all source files.

**Step 4: Verify GREEN.** `cargo check -p julie`; exact RED command. Lead verifies malformed token, wrong tool/arguments, source-generation change, expiry, eviction, oversized result, repeated page determinism, and concurrent readers.

**Step 5: Handoff.** Supply retention limits, schemas, conflict codes, and byte-identical cross-process page evidence.

**Acceptance criteria:**
- [x] New CLI/MCP connections can continue an existing result using explicit workspace and handle.
- [x] Wrong workspace/tool/arguments, expired handles and changed source revisions fail distinctly.
- [x] Snapshot storage and result pages are bounded and deterministic.
- [x] Handles cannot become paths, arbitrary SQL or edit authorization.

### Task 6: Run multi-process, multi-worktree, and shutdown acceptance

**Files:** Task 6 row. Extend prerequisite process helpers; no interactive MCP registration or changes to the user's existing processes.

**Interfaces:** Create a fixture repository and two real `git worktree add --detach` children under a task-owned temp directory. Each contains the same relative filename but different symbol/text. Children share one isolated `JULIE_HOME`; source roots differ. Cleanup removes only task-created worktrees after all children exit.

**Contract inputs:** Task-1 manager, task-2 slot limits, task-3 source edits, task-4 recovery and task-5 handles. Full semantic mode remains routed through the prerequisite seam; this plan does not schedule or replace inference.

**File ownership:** Task 6 row. **Serialization required:** Yes. **Dependency reason:** Exercises the integrated final binary.

**Step 1: Write the failing test.** Start an owner and follower for one worktree plus a second owner for the other. Requests use modern direct wire calls from prerequisite helpers.

```rust
#[tokio::test]
async fn concurrent_worktrees_keep_sources_indexes_and_owners_isolated() {
    let mut fixture = WorktreeProcessFixture::start_three_clients().await;
    assert_ne!(fixture.left_workspace_id(), fixture.right_workspace_id());
    fixture.left_follower_edit("left_old", "left_new").await.unwrap();
    fixture.right_owner_edit("right_old", "right_new").await.unwrap();
    fixture.wait_for_current_indexes().await;
    assert!(fixture.left_search("left_new").await.contains("left_new"));
    assert!(!fixture.right_search("left_new").await.contains("left_new"));
    assert_eq!(fixture.active_owners_for_left(), 1);
    assert_eq!(fixture.active_owners_for_right(), 1);
    fixture.stop_left_owner().await;
    fixture.wait_for_left_follower_promotion().await;
    assert_eq!(fixture.active_owners_for_left(), 1);
    assert_eq!(fixture.active_owners_for_right(), 1);
    fixture.shutdown().await;
}
```

`active_owners` reads explicit runtime diagnostics backed by held owner-epoch state and verifies contention through a separate process lock attempt. It cannot merely count process labels.

**Step 2: Verify RED.** Build `cargo build -p julie --bin julie-server`, then run `JULIE_TEST_BIN=/home/murphy/source/julie/target/debug/julie-server cargo nextest run -p julie --lib concurrent_worktrees_keep_sources_indexes_and_owners_isolated`. If all prior slices already make this pass, record added integration coverage rather than fabricating RED.

**Step 3: Close integrated defects and add the fault matrix.** Test owner normal shutdown, abrupt kill, failure during recovery, cancelled queued read, cancelled slot acquisition, open readers during publication, same-file competing edits, interrupted multi-file edit, source file deleted during extraction, filesystem watcher overflow, removed worktree root, and continuation after process restart. Use explicit barriers/condition polling with bounded deadlines, not sleeps as correctness assumptions. For removed roots, stop admission and report `WORKSPACE_MISSING`; never remap to parent repository or sibling worktree.

```rust
#[test]
fn ownership_transition_table_never_skips_recovery() {
    assert!(allowed_transition(RuntimePhaseKind::Follower, RuntimePhaseKind::Recovering));
    assert!(!allowed_transition(RuntimePhaseKind::Follower, RuntimePhaseKind::Owner));
    assert!(allowed_transition(RuntimePhaseKind::Recovering, RuntimePhaseKind::Owner));
    assert!(!allowed_transition(RuntimePhaseKind::Draining, RuntimePhaseKind::Owner));
}
```

Implement the transition table as the manager's actual state guard; this unit test complements process proofs. Update `docs/WORKSPACE_ARCHITECTURE.md` with request/owner/source-edit/index-commit lifetimes and exact storage paths. Update `docs/DEVELOPMENT.md` with shared versus standalone storage and truthful freshness examples. Update `docs/TESTING_GUIDE.md` with process commands, test fixture ownership and platform results. Preserve the protocol distinction: no initialize requirement does not eliminate runtime state.

**Step 4: Verify GREEN.** Rebuild after final changes, rerun the exact failed test, then let the lead run remaining exact fault cases and the declared dev/system/reliability gates once. Capture CPU/RSS/disk and configured slot counts under 1, 2 and 4 attached agents as report-only measurements.

**Step 5: Handoff and review.** Root Codex reviews all source-edit conflict behavior, writer proofs, generation publication and shutdown ordering. Report root/branch/HEAD/dirty state for every task checkout, ledger, process transcripts, exact platform evidence, resource configuration and remaining unverified cases. Do not call cross-platform behavior complete from Linux-only results. No push/release is part of this plan.

**Acceptance criteria:**
- [x] Concurrent agents and two real worktrees retain distinct bindings, data and owner locks.
- [x] Owner turnover and every fault boundary preserve canonical state and user edits.
- [x] No request cancellation, queue overflow or shutdown loses accepted work silently.
- [x] Modern MCP and CLI remain usable without a global daemon or fake initialization.
- [x] Root Codex review, docs and required verification ledger are complete.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Runtime lifetime independent of client connection (active suite) | `CI=1 cargo nextest run -p julie --lib tests::runtime_lifecycle::` | `m1-contract` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:08:42Z | no |
| Host indexing admission slots bounded to configured limit across OS processes | `CI=1 cargo nextest run -p julie-core --lib host_slots_` | `m2-host-slots` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:09:06Z | no |
| Follower source edits, atomic journaling, and recoverable partial application | `CI=1 cargo nextest run -p julie --lib tests::edit_recovery_contract::` | `m3-contract` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T18:00:41Z | no |
| Writer fencing with authentic OwnerEpoch and recoverable revision publication | `CI=1 cargo nextest run -p julie --lib tests::writer_fencing_contract::` | `m4-contract` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:11:56Z | no |
| Durable continuation snapshots bounded by TTL and capacity across processes | `CI=1 cargo nextest run -p julie --lib tests::runtime_continuation::` | `m5-continuations` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:12:03Z | no |
| Multi-process concurrent worktree isolation, failover, and fault matrix acceptance | `CI=1 cargo nextest run -p julie --lib tests::workspace_process_lifecycle::` | `m5-lifecycle` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:12:25Z | no |
| Workspace type checking and compilation across all targets with zero errors | `CI=1 cargo check --workspace --all-targets` | `static-check` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:08:20Z | no |
| Rustfmt code style compliance across all workspace crates | `cargo fmt --all -- --check` | `static-fmt` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:08:23Z | no |
| Agent instruction and guideline parity between CLAUDE.md and AGENTS.md | `diff -u CLAUDE.md AGENTS.md` | `static-doc-sync` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T17:08:24Z | no |
| Batch regression tier across core, tools, and CLI commands | `cargo xtask test dev` | `lead-dev-gate` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T18:39:30Z | no |
| Integration regression tier across workspace runtime, projection, and system health | `cargo xtask test system` | `lead-system-gate` | `98c23c82366d1ea601ab55898444b69a6b4829e7` | pass | 2026-09-08T18:41:08Z | no |
