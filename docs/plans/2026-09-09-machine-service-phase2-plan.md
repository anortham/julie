# Machine Service Phase 2: One Writer, Disposable Indexes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development whenever delegation is available and permitted, including for one task; serialize dependent tasks. Use razorback:executing-plans only when delegation is unavailable or the user/session explicitly selected single-agent execution.

**Goal:** The service is the only writer per checkout, so every cross-process coordination path is deleted, derived indexes are deleted and rebuilt instead of migrated, a new worktree seeds its index from a sibling checkout, and `manage_workspace` gains `rebuild` and `status`. The phase ends net negative in lines with the section 5.4 deletion list empty except the two items this plan defers with a reason.

**Architecture:** Phase 1 left one `julie-server service` process that owns every `JulieServerHandler` through `RuntimeFactory`. Every handler is therefore the writer for its checkout. This plan removes the code that assumed other processes could write: leader lock, leadership state, owner epochs, writer permits, publication locks, host slots, follower refusals, primary-workspace swaps, session attachment, the continuation store, and the Python embedding host. Per-checkout serialization is the existing in-process `mutation_gate` async mutex, which stays. Storage stays on `symbols.db` plus `tantivy/` under `$JULIE_HOME/indexes/<id>/`; the blob-keyed `facts.sqlite` schema moves to phase 3 (see **Sequencing decision**).

**Tech Stack:** Rust, `rusqlite` (existing), `tokio`, `notify` (existing watcher), `git` CLI for `rev-parse --git-common-dir`, `cargo nextest`, `cargo xtask test`, `tokei`.

**Architecture Quality:** Design `docs/plans/2026-09-09-machine-service-design.md`, sections 4, 5.1, 5.4, 6.2, 7, 12, 13, 14. Module shape after this phase: `RuntimeFactory` creates one handler per `(root, index_root)`; the handler is always the writer; `mutation_gate::acquire_gate(workspace_id)` is the only serialization; `JulieWorkspace` still owns `SymbolDatabase` and `SearchIndex`. Risk: high. The handler is 2991 lines with leadership woven through it, and 3.5k lines of lifecycle tests encode the old model. Control: every task ends compilable with its own narrow suite green, tests that encode deleted behavior are deleted in the same task, and the lead runs `dev` after each batch.

## Sequencing decision (needs user approval with the plan)

Design section 14 puts the `facts.sqlite` schema in phase 2 and the read tools in phase 3. Every read tool today reads `SymbolDatabase` (21 tables keyed by path) through `JulieServerHandler`. Rewriting the schema to blob-keyed facts without moving the readers leaves no working tool between phase 2 and phase 3, which design section 4 rule 5 forbids ("parallel old and new paths"). This plan therefore splits the work by what can be deleted without a storage rewrite:

- **Phase 2 (this plan):** single writer, disposable derived indexes, sibling seed, `rebuild` and `status`. Storage schema unchanged.
- **Phase 3:** `facts.sqlite` blob-keyed schema, Tantivy projection, in-memory graph, snapshots, and all read tools, in one phase, so the old storage is deleted in the same change that replaces it.

Two deletion-list items move with their replacements, per rule 5:

- **Native sidecar broker client** (`crates/julie-pipeline/src/embeddings/native/`): deleted in phase 4 when the `serve`-mode child replaces it. Deleting it here would turn semantics off for two phases.
- **Embedding generations** (`crates/julie-core/src/database/embedding_generation*.rs`): deleted in phase 4 with the `vectors` table and encoder identity row.

The Python embedding host, the continuation store, and every lock, lease, epoch, and permit are deleted here.

## Global Constraints

- Design section 4 is normative. A worker who needs any of these words in new code stops and reports: lock, lease, fence, generation, epoch, cursor, claim, pin, coordinator, broker, journal, repair, continuation, handoff. Deleting code that contains them is the point of this phase.
- No new durable file under `~/.julie/` or under `$JULIE_HOME/indexes/<id>/`. After a full index the only entries are `indexes/<id>/db/symbols.db` (plus its `-wal` and `-shm`), `indexes/<id>/tantivy/`, `registry.db`, and the runtime `service.json`. Task 9 adds the test.
- `mutation_gate` (`crates/julie-core/src/workspace/mutation_gate.rs`, `Registry`, `MutationGuard<'a>`, `acquire_gate`) is the one serialization primitive. Gated functions take `_guard: &MutationGuard<'_>`. No new mutex, atomic flag, or channel is added to serialize writers.
- No new crate in any `Cargo.toml`. Removing crates is expected (`include_dir` goes with the Python host).
- Schema version mismatch on `symbols.db` (stored `schema_version` differs from `LATEST_SCHEMA_VERSION` in `crates/julie-core/src/database/migrations/mod.rs:15`, which moves to `schema.rs`) is handled exactly like an `index_engine_state` version mismatch today (`src/tools/workspace/indexing/index.rs:343-374`): delete the index directory and reindex. No migration code remains for `symbols.db`. `registry.db` keeps its migrations (`src/registry/database/migrations.rs`); it is not derived.
- Tool count goes from 13 to 12: `spillover_get` is deleted. `ToolCatalog` (`src/request_engine/catalog.rs`) is the single list; the test `catalog_schemas_valid_and_match_all_13_tools` in `src/tests/request_engine.rs:247` is renamed and updated to 12.
- `manage_workspace` operations after this phase: `index, list, open, remove, refresh, health, rebuild, status, recover_edit, recover-edit, dashboard`. Retired: `register` (`open` covers it), `clean` (`list` prunes), `stats` (`status` covers it). `ManageWorkspaceOperation::OPERATIONS` (`src/tools/workspace/commands/mod.rs:40-53`) is the single source of truth.
- Every test in the repo runs with `JULIE_EMBEDDING_PROVIDER=none` unless it sets the variable itself. This is set once in `.cargo/config.toml` `[env]`, which cargo applies to test binaries and which `cargo nextest` 0.9.143 honours. No test spawns a process that outlives the test; any spawned child is killed in `Drop`.
- Sibling lookup uses `git rev-parse --git-common-dir` run with `current_dir` set to the checkout root, output canonicalized. A non-zero exit or a missing `git` means "no sibling", and the checkout indexes from scratch. That is the only fallback.
- Every task ends with `cargo build` green, its worker scope green, and the tests that encoded the deleted behavior deleted (not `#[ignore]`d).
- Test file limit 1000 lines, implementation file limit 500 lines for new files. `src/handler.rs` shrinks; it does not have to reach 500 in this phase.

## Verification Strategy

**Project source of truth:** `AGENTS.md` sections "Commands" and "Test tiers"; `xtask/test_tiers.toml`; `xtask/tests/support/manifest_contract_expected.rs` (bucket inventory the manifest contract test checks).

**Worker red/green scope:** the exact test module each task names, run as `cargo nextest run --lib <module>` (or `cargo nextest run -p <crate> --lib <module>` for workspace crates).

**Worker ceiling:** `cargo build` plus the modules the task names. Workers do not run `cargo xtask test dev`, `changed`, or any tier.

**Worker gate invariant:** each task's acceptance list states the behavior its tests prove and the files its deletions remove.

**Lead affected-change scope:** `cargo xtask test changed` after each batch. Every task in this plan touches `src/handler.rs` or an unmapped path, so expect the `dev` fallback; run `cargo xtask test dev` once per batch instead.

**Branch gate:** `cargo xtask test dev`, then `cargo xtask test system`, then `cargo xtask test bucket service-process`, then Task 10's measurements. `cargo xtask test full` once before handoff.

**Security scope:** `cargo audit` if installed, else record `none declared`. Secrets scan: `git diff main...HEAD | grep -iE 'token|secret|password'` reviewed by the lead.

**Replay/metric evidence:** hard gates: durable-roots test (Task 9), complexity-words script on the diff (Task 9), tokei net lines negative (Task 10), no leaked child process after `dev` (Task 6). Report-only: store size per checkout on the Julie repo, seed time for a Julie worktree, `fast` bucket wall time.

**Escalation triggers:** any need for a new lock, flag, or channel to serialize writers stops the task. Any need to keep a deleted path "for the CLI" stops the task; the CLI runs the same engine. Any test that only passes with a real repository or more than one process stops the task and reports (design rule 4).

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** `docs/plans/2026-09-09-machine-service-phase2-ledger.md`, created from `docs/plans/verification-ledger-template.md`. Record invariant, command, scope label, commit SHA, result, and timestamp. Reuse only when scope label and HEAD SHA match exactly.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Every runtime is the writer | None - serial | Modify `src/handler.rs`, `src/request_engine/runtime_factory.rs`, `src/request_engine/types.rs`, `src/request_engine/dispatch.rs`, `src/workspace_runtime/owner.rs`, `src/workspace_runtime/manager.rs`, `src/workspace_runtime/mod.rs`, `src/tools/workspace/commands/registry/register_remove.rs`, `src/tools/workspace/commands/registry/refresh_stats.rs`, `src/startup.rs`; delete `src/tests/core/handler/{follower_repair_gate,loser_refuses,leader_watcher,fa_pin_hint,t9_bounded_read,inprocess_ctor,inprocess_serve}.rs`, `src/tests/integration/t9_handoff_recovery.rs`, `src/tests/integration/t11_kill_writer.rs`; modify `src/tests/request_engine.rs`, `src/tests/mod.rs`, `src/tests/core/handler/mod.rs`, `src/tests/integration/mod.rs` | Yes | Everything after this depends on the handler having no follower branch. |
| Task 2: Delete the lock types | None - serial | Delete `src/leadership.rs`, `crates/julie-core/src/workspace/leader_lock.rs`, `crates/julie-core/src/workspace/ownership.rs`, `crates/julie-core/src/workspace/publication_lock.rs`, `crates/julie-core/src/workspace/host_slots.rs`, `src/external_extract/lock.rs`, `src/tests/writer_fencing_contract.rs`, `src/tests/runtime_lifecycle.rs`, `src/tests/workspace_process_lifecycle.rs`, `src/tests/runtime_scheduler.rs`, `src/tests/registry/lock_test.rs`, `src/tests/external_extract/locking.rs`, `crates/julie-core/src/tests/host_slots.rs`; create `crates/julie-core/src/workspace/projection_stamp.rs`; modify `src/lib.rs`, `crates/julie-core/src/workspace/mod.rs`, `crates/julie-core/src/paths.rs`, `crates/julie-core/src/tests/paths.rs`, `src/handler.rs`, `src/startup.rs`, `src/tools/workspace/commands/index.rs`, `src/tools/workspace/indexing/pipeline.rs`, `src/tools/workspace/indexing/route.rs`, `src/workspace_runtime/{owner,manager,scheduler,publication,recovery,source_edit_ops,mod}.rs`, `src/external_extract/metadata.rs`, `src/registry/discovery.rs`, `crates/julie-runtime/src/watcher/{mod,runtime,dispatch}.rs`, `crates/julie-runtime/src/watcher/runtime/{processing,projection,repairs}.rs`, `crates/julie-runtime/src/tests/helpers.rs`, `src/tests/mod.rs`, `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs` | Yes | Depends on Task 1; the types are unreferenced only after the follower branches are gone. |
| Task 3: Delete session bootstrap and primary swap | None - serial | Delete `src/registry/workspace_session_attachment.rs`, `src/tests/registry/session_workspace.rs`; modify `src/handler.rs`, `src/handler/session_workspace.rs`, `src/handler/workspace_resolution.rs`, `src/tools/workspace/commands/registry/open.rs`, `src/tools/workspace/commands/registry/index_resolution.rs`, `src/dashboard/state.rs`, `src/dashboard/mod.rs`, `src/registry/session.rs`, `src/registry/mod.rs`, `src/tests/tools/workspace/global_targeting/{primary_swap_guards,deferred_sessions,target_activation}.rs`, `src/tests/tools/workspace/{deferred_open,embedding_deferred}.rs`, `src/tests/mod.rs` | Yes | Depends on Task 2 (the swap code holds `DaemonLockGuard` values). |
| Task 4: Delete the continuation store and `spillover_get` | Batch A | Delete `src/workspace_runtime/continuation_store.rs`, `src/workspace_runtime/continuation.rs`, `src/handler/tools/spillover_get.rs`, `src/tests/runtime_continuation.rs`, `crates/julie-tools/src/spillover/` and its tests; modify `src/workspace_runtime/mod.rs`, `src/handler/tool_context_impl.rs`, `src/handler/tools/mod.rs`, `src/handler.rs` (the `spillover_store` field and `tool_router()` only), `src/request_engine/catalog.rs`, `src/tools/mod.rs`, `crates/julie-tools/src/lib.rs`, `src/cli.rs`, `src/cli_tools/{subcommands,commands}.rs`, `src/tests/request_engine.rs`, `src/tests/mod.rs`, `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/settings.local.json` | No | None - safe parallel batch. Task 4 touches only the `spillover_store` field and `tool_router()` in `src/handler.rs`; Task 5 does not touch `src/handler.rs`. |
| Task 5: Derived indexes are disposable | Batch A | Delete `crates/julie-core/src/database/migrations/` (all files), `crates/julie-core/src/tests/database/migrations*.rs` if present; modify `crates/julie-core/src/database/mod.rs`, `crates/julie-core/src/database/schema.rs`, `crates/julie-core/src/database/index_engine.rs`, `src/tools/workspace/indexing/index.rs:343-374`, `src/tools/workspace/indexing/engine_version.rs`, `crates/julie-core/src/tests/database/mod.rs` | No | None - safe parallel batch. |
| Task 6: Delete the Python embedding host and stop test process leaks | None - serial | Delete `python/embeddings_sidecar/` (see approach), `crates/julie-pipeline/src/embeddings/{sidecar_bootstrap,sidecar_supervisor,sidecar_protocol,sidecar_embedded,host_server}.rs`, `crates/julie-pipeline/src/embeddings/sidecar_provider/`, `src/embedding_host_launch.rs`, `src/bin/julie-embedding-host.rs`, `src/registry/embedding_service/`, `src/tests/registry/{embedding_host_multi_session,inprocess_embedding,embedding_service,embedding_service_shutdown}.rs`, `src/tests/core/{embedding_sidecar_provider,sidecar_embedding_tests}.rs`, `crates/julie-pipeline/src/tests/sidecar_supervisor_tests.rs` and sibling sidecar tests; modify `Cargo.toml` (root and `crates/julie-pipeline`), `.cargo/config.toml`, `src/embeddings/mod.rs`, `crates/julie-pipeline/src/embeddings/{mod,init,factory}.rs`, `src/handler.rs` (`embedding_service` field), `src/handler/embedding_init.rs`, `src/registry/mod.rs`, `src/lib.rs`, `crates/julie-core/src/paths.rs` (`embedding_host_*` fns), `crates/julie-core/src/tests/paths.rs`, `crates/julie-pipeline/src/tests/{native_broker_replacement_challenge,native_provider_challenges}.rs`, `src/tests/integration/native_semantic_lifecycle.rs`, `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs`, `src/tests/mod.rs`, `docs/DEPENDENCIES.md` | Yes | Touches `src/handler.rs` and the embeddings factory that Task 5's engine-version test reads. |
| Task 7: Sibling seed copy on `open` | None - serial | Create `crates/julie-core/src/workspace/git_identity.rs`, `src/tools/workspace/indexing/seed.rs`, `src/tests/tools/workspace/seed.rs`, `fixtures/seed/` (two small trees); modify `crates/julie-core/src/workspace/mod.rs`, `src/tools/workspace/indexing/mod.rs`, `src/tools/workspace/commands/registry/open.rs`, `src/tools/workspace/commands/index.rs`, `src/tests/mod.rs`, `xtask/test_tiers.toml` | Yes | Depends on Task 5 (a seeded copy must pass the schema-version check) and Task 3 (`open` no longer swaps). |
| Task 8: `rebuild`, `status`, retire `register`/`clean`/`stats` | None - serial | Create `src/tools/workspace/commands/registry/status.rs`, `src/tests/tools/workspace/status_rebuild.rs`; delete `src/tools/workspace/commands/registry/refresh_stats.rs` stats half (see approach), `src/tests/tools/workspace/global_targeting/list_stats.rs` stats tests; modify `src/tools/workspace/commands/mod.rs`, `src/tools/workspace/commands/registry/{mod,list_clean,register_remove,cleanup}.rs`, `src/request_engine/catalog.rs`, `src/service/status.rs`, `src/service/http.rs`, `src/cli_tools/{subcommands,commands}.rs`, `src/tests/service/http_api.rs`, `src/tests/tools/workspace/global_targeting/*.rs`, `src/tests/mod.rs`, `xtask/test_tiers.toml`, `JULIE_AGENT_INSTRUCTIONS.md` | Yes | Depends on Task 7 (`open` result shape) and Task 4 (`/status` per-checkout fields read the handler). |
| Task 9: Gates: durable roots, complexity words, tiers | None - serial | Create `scripts/complexity-words.sh`, `src/tests/service/durable_roots.rs`; modify `src/tests/service/mod.rs`, `src/tests/service/budget.rs`, `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs` | Yes | Runs against the finished tree. |
| Task 10: Measurements, docs, ledger, gate verdict (lead) | None - serial | Create `docs/findings/2026-09-DD-machine-service-phase2-gate.md`, `docs/plans/2026-09-09-machine-service-phase2-ledger.md`; modify `CLAUDE.md`, `AGENTS.md`, `docs/WORKSPACE_ARCHITECTURE.md`, `docs/ARCHITECTURE.md`, `docs/TESTING_GUIDE.md`, `README.md`, `docs/plans/2026-09-09-machine-service-design.md` (phase list only) | Yes | Lead task; needs the final tree. |

Commit mode: `serial-worker-commit` for Tasks 1, 2, 3, 6, 7, 8, 9. `parallel-lead-commit` for Batch A (Tasks 4 and 5). Task 10 is lead work.

---

### Task 1: Every runtime is the writer

**Files:**
- Modify: `src/handler.rs` (`leadership` field `:284`, `is_leader` `:1189`, `is_follower` `:1203`, `acquire_writer_permit` `:1285`, watcher-start and repair-scan gates, force-reindex `leader.lock` carve-out `:1731-1758`), `src/request_engine/runtime_factory.rs:42-59,190-264`, `src/request_engine/types.rs` (`RequestFailure::follower_read_only`, `exit_code` mapping), `src/request_engine/dispatch.rs:126-134`, `src/workspace_runtime/owner.rs`, `src/workspace_runtime/manager.rs` (`DEFAULT_PROBE_INTERVAL` and promotion probe), `src/workspace_runtime/mod.rs:55-181` (`RuntimePhase`), `src/tools/workspace/commands/registry/register_remove.rs:174-225` (follower guard), `src/tools/workspace/commands/registry/refresh_stats.rs`, `src/startup.rs`
- Delete: the nine test files listed in the contract row
- Test: `src/tests/request_engine.rs`, `src/tests/core/handler/`, `src/tests/integration/`

**Interfaces:**
- Consumes: `LeadershipState` (`src/leadership.rs`) still exists in this task; every constructor call becomes `LeadershipState::leader_in_process()` so the next task can delete the type without touching behavior.
- Produces: `RequestRuntime::check_access` (`runtime_factory.rs:50`) no longer refuses `AccessClass::IndexMutation`; `RequestFailure::follower_read_only` is gone; `RuntimePhase` has no `Follower`, `Recovering`, or `Draining` variant; `WorkspaceRuntimeManager` has no promotion probe; every `is_leader()` call site is gone.

**Contract inputs:** exit code 4 in `RequestFailure::exit_code` (`types.rs:303`) stays for `semantics_not_ready` and `busy`; only the follower case is removed. `RequestReadiness` (`types.rs:108`) keeps its fields.

**File ownership:** Modify `src/handler.rs`, `src/request_engine/runtime_factory.rs`, `src/request_engine/types.rs`, `src/request_engine/dispatch.rs`, `src/workspace_runtime/owner.rs`, `src/workspace_runtime/manager.rs`, `src/workspace_runtime/mod.rs`, `src/tools/workspace/commands/registry/register_remove.rs`, `src/tools/workspace/commands/registry/refresh_stats.rs`, `src/startup.rs`; delete `src/tests/core/handler/{follower_repair_gate,loser_refuses,leader_watcher,fa_pin_hint,t9_bounded_read,inprocess_ctor,inprocess_serve}.rs`, `src/tests/integration/t9_handoff_recovery.rs`, `src/tests/integration/t11_kill_writer.rs`; modify `src/tests/request_engine.rs`, `src/tests/mod.rs`, `src/tests/core/handler/mod.rs`, `src/tests/integration/mod.rs`

**Serialization required:** Yes

**Dependency reason:** Everything after this depends on the handler having no follower branch.

**What to build:** Remove every branch that asks "am I the leader?". In one process there is one answer. The watcher starts for every bound workspace, the startup catch-up runs for every workspace that is not indexed, `register`/`remove`/`refresh`/`index` never refuse for ownership reasons, and the runtime phase machine has one live phase.

**Approach:** Start with Miller `trace(target='is_leader')` and `trace(target='is_follower')` (19 non-test call sites). At each site keep the leader arm and delete the other. `RequestRuntime::check_access` becomes `Ok(())` and is then inlined away with its call in `dispatch.rs:126-134`. Delete `follower_rejects_manage_workspace_mutation` and `RequestFixture::as_follower` in `src/tests/request_engine.rs`. In `workspace_runtime/mod.rs` collapse `RuntimePhase` to `Owner` and `Terminal`; delete `allowed_transition` cases that no longer exist. Delete the promotion probe loop in `manager.rs` and `DEFAULT_PROBE_INTERVAL`. Do not delete `LeadershipState` or `DaemonLockGuard` yet; construct `LeadershipState::leader_in_process()` everywhere so Task 2 is a pure type deletion. Delete the nine listed test files and their `mod` lines; they test loser, follower, handoff, and kill-writer behavior that no longer exists.

**Acceptance criteria:**
- [ ] `grep -rn 'is_follower\|follower_read_only\|as_follower' src crates --include='*.rs'` returns nothing.
- [ ] `RuntimePhase` has no `Follower`, `Recovering`, or `Draining` variant and `manager.rs` has no probe interval.
- [ ] `cargo nextest run --lib tests::request_engine` and `cargo nextest run --lib tests::core::handler` pass; the nine deleted test files are gone from `src/tests/mod.rs` and submodule lists.
- [ ] `cargo build` green; worker-scope verification passes and the change is committed per commit mode.

---

### Task 2: Delete the lock types

**Files:**
- Delete: `src/leadership.rs` (153), `crates/julie-core/src/workspace/leader_lock.rs` (224), `crates/julie-core/src/workspace/ownership.rs` (371), `crates/julie-core/src/workspace/publication_lock.rs` (221), `crates/julie-core/src/workspace/host_slots.rs` (499), `src/external_extract/lock.rs` (138), `src/tests/writer_fencing_contract.rs` (980), `src/tests/runtime_lifecycle.rs` (600), `src/tests/workspace_process_lifecycle.rs` (617), `src/tests/runtime_scheduler.rs` (186), `src/tests/registry/lock_test.rs` (164), `src/tests/external_extract/locking.rs` (103), `crates/julie-core/src/tests/host_slots.rs` (348)
- Create: `crates/julie-core/src/workspace/projection_stamp.rs` (the survivors of `ownership.rs`: `PublicationStamp`, `TantivyCommitPayload`, `SourceCheckState`, `needs_projection_recovery`; under 150 lines)
- Modify: every file in the contract row; `RegistryPaths::workspace_leader_lock` (`crates/julie-core/src/paths.rs:373-382`) is deleted
- Test: `crates/julie-runtime/src/tests/watcher_mutation_gate.rs` (280, stays), `src/tests/request_engine.rs`, `crates/julie-core/src/tests/paths.rs`

**Interfaces:**
- Consumes: Task 1's tree, where every `LeadershipState` is `leader_in_process()`.
- Produces: `JulieServerHandler::acquire_writer_permit` is replaced by `pub(crate) async fn acquire_mutation_guard(&self, workspace_id: &str) -> MutationGuard<'static>` that calls `julie_core::workspace::mutation_gate::acquire_gate`. Every function that took `permit: &WriterPermit<'_>` takes `_guard: &MutationGuard<'_>` (`startup.rs`, `tools/workspace/commands/index.rs:33-73,433-438`, `workspace_runtime/recovery.rs`, `crates/julie-runtime/src/watcher/dispatch.rs:19-29`, `watcher/runtime/{processing,projection,repairs}.rs`). `IncrementalIndexer::start_watching_with_epoch` and `set_owner_epoch` are deleted; `start_watching` (`watcher/mod.rs:213`) takes no epoch and does not touch any lock. `IndexScheduler` (`workspace_runtime/scheduler.rs`) has no admission pool; it runs jobs directly under the gate.

**Contract inputs:** `WorkspaceReadSnapshot` (`src/workspace_runtime/publication.rs`) keeps its name and fields but is built without a shared lock; `SnapshotError::Lock` is deleted. `mutation_gate` is unchanged.

**File ownership:** as listed in the Parallel Execution Contract row for Task 2 (verbatim).

**Serialization required:** Yes

**Dependency reason:** Depends on Task 1; the types are unreferenced only after the follower branches are gone.

**What to build:** Delete the six lock/epoch/permit modules and the tests that exist to prove cross-process fencing. Thread `MutationGuard` where `WriterPermit` was. Remove `leader.lock` from the on-disk layout.

**Approach:** Use Miller `trace` on `DaemonLockGuard`, `OwnerEpoch`, `WriterPermit`, `PublicationLock`, `IndexJobPermit`, `ExternalExtractOperationLock` before deleting each; delete callers top-down so the tree compiles at each step. Keep `mutation_gate` and its test. The force-reindex carve-out that preserved the held `leader.lock` inode (`handler.rs:1731-1758`) goes away entirely: force-reindex now deletes the whole index directory under the gate. Remove the buckets whose only tests are deleted from `xtask/test_tiers.toml` and the expected list in `xtask/tests/support/manifest_contract_expected.rs`; run `cargo nextest run -p xtask` to prove the manifest contract test agrees. `crates/julie-runtime/src/tests/helpers.rs` loses its epoch helpers.

**Acceptance criteria:**
- [ ] The thirteen listed files no longer exist and `grep -rn 'DaemonLockGuard\|OwnerEpoch\|WriterPermit\|PublicationLock\|IndexJobPermit\|leader\.lock\|workspace_leader_lock' src crates xtask --include='*.rs'` returns nothing.
- [ ] `grep -rn 'WriterPermit' crates/julie-runtime/src` returns nothing and `dispatch_file_event` takes `&MutationGuard<'_>`.
- [ ] After `manage_workspace index` on a temp workspace, `indexes/<id>/` contains only `db/` and `tantivy/` (asserted in the updated `crates/julie-core/src/tests/paths.rs` layout test or a new test in `src/tests/tools/workspace/isolation.rs`).
- [ ] `cargo nextest run -p julie-runtime --lib tests::watcher_mutation_gate`, `cargo nextest run --lib tests::request_engine`, `cargo nextest run -p xtask` pass; `cargo build` green; committed per commit mode.

---

### Task 3: Delete session bootstrap and primary swap

**Files:**
- Delete: `src/registry/workspace_session_attachment.rs` (130), `src/tests/registry/session_workspace.rs`
- Modify: `src/handler.rs` (`PrimarySwapRollback` `:62-…`, primary swap/rebind `:1377-1553`, `switch_primary_workspace_with_root` `:2336`, `activate_workspace*` `:2310-2315`, `deferred_auto_index_pending`/`_gate`/`_in_flight` `:246-262`, `ref_db_cache`), `src/handler/session_workspace.rs`, `src/handler/workspace_resolution.rs`, `src/tools/workspace/commands/registry/open.rs:33-59`, `src/tools/workspace/commands/registry/index_resolution.rs`, `src/dashboard/state.rs`, `src/dashboard/mod.rs:119-140`, `src/registry/session.rs`, `src/registry/mod.rs`
- Test: `src/tests/tools/workspace/global_targeting/open_lifecycle.rs`, `src/tests/tools/workspace/global_targeting/rebind_index.rs`, `src/tests/dashboard/`

**Interfaces:**
- Consumes: `RuntimeFactory::acquire` (`runtime_factory.rs:116`) creates one handler per `(root, index_root)`; a handler never changes workspace after construction.
- Produces: `manage_workspace open` returns the same text as today (`opened_message`, `open.rs:25`) and either binds an existing index or runs `index` on the path; it never swaps a handler's workspace. `handle_open_command` no longer returns the "Primary workspace swap in progress" error. `SessionTracker` (`src/registry/session.rs`) is deleted or reduced to the fields the dashboard still renders; the dashboard's session-phase panel shows the service `StatusLog` in-flight count instead (`DashboardState::new` signature changes accordingly and `src/dashboard/mod.rs:dashboard_router` is updated).

**Contract inputs:** `BindingResolver` (`src/request_engine/binding.rs`) is unchanged; `process_workspace` stays for the CLI.

**File ownership:** as listed in the Parallel Execution Contract row for Task 3 (verbatim).

**Serialization required:** Yes

**Dependency reason:** Depends on Task 2 (the swap code holds `DaemonLockGuard` values).

**What to build:** A handler is bound once by the factory. Delete the machinery that let one handler swap its primary workspace, attach and detach sessions, and defer auto-index behind a claim flag.

**Approach:** Miller `inspect(target='PrimarySwapRollback', depth=overview)` and `trace` on `switch_primary_workspace_with_root`, `attach_workspace_once`, `deferred_auto_index_pending`. The deferred auto-index flags exist because a session could attach before the leader indexed; with one writer, `RuntimeFactory::acquire` already runs `initialize_workspace_with_force` and the catch-up, so the flags and their gate are deleted and the tests in `deferred_open.rs`, `embedding_deferred.rs`, `deferred_sessions.rs`, `target_activation.rs`, `primary_swap_guards.rs` are deleted or reduced to the "open binds and indexes" case. Keep `initialize_workspace_with_force` and `startup::run_primary_workspace_repair` (catch-up on start) unchanged.

**Acceptance criteria:**
- [ ] `grep -rn 'PrimarySwapRollback\|switch_primary_workspace\|attach_workspace_once\|deferred_auto_index' src --include='*.rs'` returns nothing.
- [ ] `cargo nextest run --lib tests::tools::workspace::global_targeting` and `cargo nextest run --lib tests::dashboard` pass with the deleted cases removed.
- [ ] `src/handler.rs` is at least 400 lines shorter than at Task 1's start (record both numbers in the commit message).
- [ ] `cargo build` green; committed per commit mode.

---

### Task 4: Delete the continuation store and `spillover_get`

**Files:**
- Delete: `src/workspace_runtime/continuation_store.rs` (469), `src/workspace_runtime/continuation.rs` (171), `src/handler/tools/spillover_get.rs` (224), `src/tests/runtime_continuation.rs` (425), `crates/julie-tools/src/spillover/` and its tests
- Modify: `src/workspace_runtime/mod.rs:5-6,24-29`, `src/handler/tool_context_impl.rs` (spillover writes), `src/handler/tools/mod.rs`, `src/handler.rs:216` (`spillover_store`) and `tool_router()` `:2656-2671`, `src/request_engine/catalog.rs:121-248` (remove the `spillover_get` row), `src/tools/mod.rs:23,34`, `crates/julie-tools/src/lib.rs`, `src/cli.rs` (`spillover-get` command), `src/cli_tools/{subcommands,commands}.rs`, `src/tests/request_engine.rs:247`, `src/tests/mod.rs`, `JULIE_AGENT_INSTRUCTIONS.md`, `.claude/settings.local.json`
- Test: `src/tests/request_engine.rs`, `src/tests/mcp_protocol_contract.rs`, `src/tests/cli_tests.rs`

**Interfaces:**
- Consumes: `ToolCatalog` macro table (`catalog.rs:15-119`).
- Produces: `AVAILABLE_TOOLS` has 12 entries. A tool whose output exceeds its size limit ends with one line: `Output truncated at <N> results; narrow the query or pass a smaller limit.` and no handle. The `Command::SpilloverGet` CLI arm and its alias are gone.

**Contract inputs:** design section 5.3: results are bounded, no continuation tokens, no durable continuation pages.

**File ownership:** as listed in the Parallel Execution Contract row for Task 4 (verbatim).

**Serialization required:** No

**Dependency reason:** None - safe parallel batch. Task 4 touches only the `spillover_store` field and `tool_router()` in `src/handler.rs`; Task 5 does not touch `src/handler.rs`.

**What to build:** Delete the third SQLite database per checkout (`continuation_snapshots`, `continuation_pages`, `continuation_tombstones`) and the tool that read it.

**Approach:** Miller `trace(target='spillover_store')` to find the writers in `tool_context_impl.rs`; replace each write with the truncation line. Rename `catalog_schemas_valid_and_match_all_13_tools` to `..._12_tools`. Update the 13-tool expectations in `src/tests/mcp_protocol_contract.rs` and the host-gate finding is not edited (it is a record). Remove the allowlist entry from `.claude/settings.local.json` and the tool description from `JULIE_AGENT_INSTRUCTIONS.md`.

**Acceptance criteria:**
- [ ] `grep -rni 'continuation\|spillover' src crates --include='*.rs'` returns nothing outside `docs/`.
- [ ] `ToolCatalog::list().len() == 12` is asserted in `src/tests/request_engine.rs`.
- [ ] `cargo nextest run --lib tests::request_engine`, `cargo nextest run --lib tests::mcp_protocol_contract`, `cargo nextest run --lib tests::cli_tests` pass; `cargo build` green; handed to the lead per commit mode.

---

### Task 5: Derived indexes are disposable

**Files:**
- Delete: `crates/julie-core/src/database/migrations/` (`mod.rs` 223, `v1_to_v8.rs` 408, `v9_to_v20.rs` 334, `v21_to_v30.rs` 138, `receiver_type.rs` 15, `embedding_generation.rs` 12) and any `crates/julie-core/src/tests/database/migration*` tests
- Modify: `crates/julie-core/src/database/mod.rs:123-160` (`SymbolDatabase::new`), `crates/julie-core/src/database/schema.rs` (`initialize_schema` `:9-39` gains `schema_version`, `embedding_config`, `tool_calls`, `symbol_vectors`, `memory_vectors` creation; `LATEST_SCHEMA_VERSION` moves here), `crates/julie-core/src/database/index_engine.rs`, `src/tools/workspace/indexing/index.rs:343-374`, `src/tools/workspace/indexing/engine_version.rs:22`
- Test: `crates/julie-core/src/tests/database/`, `src/tests/integration/stale_index_detection/upgrade.rs`, `src/tests/tools/workspace/index_embedding_tests.rs`

**Interfaces:**
- Consumes: `get_schema_version` (`migrations/mod.rs:72`) semantics: one row in `schema_version`.
- Produces: `pub fn schema_version_matches(&self) -> Result<bool>` on `SymbolDatabase` in `crates/julie-core/src/database/schema.rs`; `SymbolDatabase::new` on a fresh file writes `LATEST_SCHEMA_VERSION`; on an existing file it opens without altering tables. The drift check in `index.rs:343-374` treats `!schema_version_matches()` exactly like an engine-version mismatch: delete the index directory, recreate, reindex. `SEMANTIC_INDEX_ENGINE_VERSION` gains a `+schema=<LATEST_SCHEMA_VERSION>` component so one comparison covers both.

**Contract inputs:** design section 6.1: "Never migrated." The `sqlite-vec` `vec0` tables are created with the default 384 dimensions as migration 010 did; `recreate_vectors_table` (`vectors.rs:284`) still handles dimension changes.

**File ownership:** as listed in the Parallel Execution Contract row for Task 5 (verbatim).

**Serialization required:** No

**Dependency reason:** None - safe parallel batch.

**What to build:** Delete 1130 lines of migrations. A `symbols.db` whose version is not current is deleted and rebuilt.

**Approach:** Read `initialize_schema` with Miller `inspect(target='initialize_schema', depth=full)`. Copy the `CREATE TABLE` statements that only migrations created (`schema_version`, `embedding_config`, `tool_calls`, `symbol_vectors`, `memory_vectors`) into `schema.rs` as `create_*_table` methods. Write the version row in `initialize_schema`. The test `src/tests/integration/stale_index_detection/upgrade.rs` is rewritten to: create a DB, set `schema_version` to `LATEST_SCHEMA_VERSION - 1`, run `index`, assert the directory was recreated and reindexed. Delete tests that assert migration steps.

**Acceptance criteria:**
- [ ] `crates/julie-core/src/database/migrations/` does not exist and `grep -rn 'run_migrations\|apply_migration\|migration_0' crates src --include='*.rs'` returns nothing.
- [ ] A fresh `SymbolDatabase::new` creates every table `initialize_schema` lists plus the five moved tables; asserted by a test that enumerates `sqlite_master`.
- [ ] An out-of-date `schema_version` triggers directory recreation and reindex (`tests::integration::stale_index_detection::upgrade`).
- [ ] `cargo nextest run -p julie-core --lib tests::database` and `cargo nextest run --lib tests::integration::stale_index_detection` pass; `cargo build` green; handed to the lead per commit mode.

---

### Task 6: Delete the Python embedding host and stop test process leaks

**Files:**
- Delete: `python/embeddings_sidecar/` (see approach for the evaluation-harness check), `crates/julie-pipeline/src/embeddings/sidecar_bootstrap.rs` (666), `sidecar_supervisor.rs` (255), `sidecar_protocol.rs` (339), `sidecar_embedded.rs` (113), `host_server.rs` (347), `sidecar_provider/`, `src/embedding_host_launch.rs` (261), `src/bin/julie-embedding-host.rs`, `src/registry/embedding_service/`, the test files in the contract row
- Modify: root `Cargo.toml` (`default = ["embeddings-sidecar"]` `:34-35`, feature line `:52`, `include_dir` `:111`, `[[bin]] julie-embedding-host`), `crates/julie-pipeline/Cargo.toml`, `.cargo/config.toml` (`[env] JULIE_EMBEDDING_PROVIDER = "none"`), `src/embeddings/mod.rs` (`acquire_in_process_embedding_provider` keeps only the `none` and `native` arms), `crates/julie-pipeline/src/embeddings/{mod,init,factory}.rs` (`parse_provider_preference` accepts `auto`, `native`, `none`; `auto` means native if the sidecar binary is found, else none), `src/handler.rs:243` (`embedding_service`), `src/handler/embedding_init.rs`, `src/registry/mod.rs`, `src/lib.rs:29`, `crates/julie-core/src/paths.rs:387-416` (`embedding_host_socket`, `embedding_host_lock`, `embedding_host_pipe_name`), `crates/julie-core/src/tests/paths.rs`, `crates/julie-pipeline/src/tests/{native_broker_replacement_challenge,native_provider_challenges}.rs`, `src/tests/integration/native_semantic_lifecycle.rs`, `xtask/test_tiers.toml` (`core-embeddings` bucket commands `:76-77`), `xtask/tests/support/manifest_contract_expected.rs`, `src/tests/mod.rs`, `docs/DEPENDENCIES.md`
- Test: `crates/julie-pipeline/src/tests/native_provider.rs`, `src/tests/core/embedding*`, `src/tests/semantic_request_contract.rs`

**Interfaces:**
- Consumes: `create_embedding_provider` (`crates/julie-pipeline/src/embeddings/init.rs:24`).
- Produces: `EmbeddingBackend` has no Python/sidecar variant. `RegistryPaths` has no `embedding_host_*` functions. `~/.julie/` never contains `embedding-host.sock`, `embedding-host.lock`, or `embedding-host.<date>` logs. Tests default to no embeddings.

**Contract inputs:** design section 8: "The Python embedding host is removed from the product. It remains in the evaluation harness only as the quality baseline." Verify with `grep -rn 'embeddings_sidecar\|sidecar' xtask-eval/ scripts/bakeoff scripts/benchmarks`. If a harness script invokes `python/embeddings_sidecar/` directly (not through the Rust launcher), keep only `python/embeddings_sidecar/sidecar/` and its `pyproject.toml`, delete `python/embeddings_sidecar/tests/`, and record the decision in the commit message. If nothing invokes it, delete the directory.

**File ownership:** as listed in the Parallel Execution Contract row for Task 6 (verbatim).

**Serialization required:** Yes

**Dependency reason:** Touches `src/handler.rs` and the embeddings factory that Task 5's engine-version test reads.

**What to build:** Native semantics only. The product no longer provisions Python, `uv`, a venv, or a shared host process. Tests stop spawning embedding processes by default, and the two native challenge suites kill their mock brokers.

**Approach:** Evidence for the leak, recorded 2026-09-09: after `cargo xtask test dev` on this branch, `ps` showed about fifty `mock-broker.py` and `mock-broker-bin` processes from `/tmp/.tmp*` directories, some four hours old, plus a `julie-embedding-host` with a `python -m sidecar.main` child. Their spawners are `crates/julie-pipeline/src/tests/native_broker_replacement_challenge.rs` and `native_provider_challenges.rs` (grep `mock-broker`). Wrap each spawned `Child` in a guard whose `Drop` calls `kill()` and `wait()`. The `[env]` default in `.cargo/config.toml` is the reason only ten test files needed `JULIE_EMBEDDING_PROVIDER=none` before; after this task those ten set nothing. Delete the `embeddings-sidecar` feature entirely rather than leaving it off. Delete `include_dir` from `Cargo.toml` and `Cargo.lock`.

**Acceptance criteria:**
- [ ] `grep -rn 'sidecar_bootstrap\|sidecar_supervisor\|julie-embedding-host\|embedding_host\|embeddings-sidecar\|include_dir' src crates xtask Cargo.toml crates/*/Cargo.toml --include='*.rs' --include='*.toml'` returns nothing.
- [ ] `.cargo/config.toml` sets `JULIE_EMBEDDING_PROVIDER = "none"` under `[env]`, and a test in `src/tests/core/` asserts `create_embedding_provider()` returns `(None, None)` with no variable set by the test.
- [ ] After `cargo nextest run -p julie-pipeline --lib tests::native_broker_replacement_challenge` and `tests::native_provider_challenges`, `ps -eo cmd | grep -c '[m]ock-broker'` prints `0` (record the output in the commit message).
- [ ] `cargo nextest run -p julie-pipeline --lib tests::native_provider`, `cargo nextest run --lib tests::semantic_request_contract`, `cargo nextest run -p xtask` pass; `cargo build` green; committed per commit mode.

---

### Task 7: Sibling seed copy on `open`

**Files:**
- Create: `crates/julie-core/src/workspace/git_identity.rs` (`pub fn git_common_dir(root: &Path) -> Option<PathBuf>`, under 40 lines), `src/tools/workspace/indexing/seed.rs` (under 200 lines), `src/tests/tools/workspace/seed.rs`, `fixtures/seed/a/` and `fixtures/seed/b/` (two trees of about eight files sharing six)
- Modify: `crates/julie-core/src/workspace/mod.rs`, `src/tools/workspace/indexing/mod.rs`, `src/tools/workspace/commands/registry/open.rs`, `src/tools/workspace/commands/index.rs` (`index_workspace_inner` `:436` entry), `src/tests/mod.rs`, `xtask/test_tiers.toml` (`tools-workspace-indexing` bucket)
- Test: `src/tests/tools/workspace/seed.rs`

**Interfaces:**
- Consumes: `WorkspaceRegistryStore` listing (`src/registry/workspace_registry_store.rs`) for candidate siblings; `files.hash` (blake3, `crates/julie-core/src/database/files.rs:557`) and `symbols.file_hash`; `filter_changed_files` (`src/tools/workspace/indexing/incremental.rs:21`) and `clean_orphaned_files` (`:158`); `persist_incremental_scan` (`crates/julie-pipeline/src/indexing_core/persistence.rs:25`).
- Produces: `pub(crate) async fn seed_from_sibling(handler: &JulieServerHandler, root: &Path, index_root: &Path, _guard: &MutationGuard<'_>) -> Result<Option<SeedReport>>` in `seed.rs`, where `SeedReport { sibling_root: PathBuf, copied_files: usize, reextracted_files: usize, removed_files: usize, elapsed_ms: u128 }`. `open` and `index` on a checkout with no `symbols.db` call it before the full scan; `Some` means the full scan is skipped and the incremental scan ran instead. The `open` message gains one line: `Seeded from <sibling_root>: <copied_files> files copied, <reextracted_files> re-extracted, <removed_files> removed in <elapsed_ms> ms`.

**Contract inputs:** design section 7. The sibling is any registered workspace whose `git_common_dir` equals this checkout's, excluding itself, preferring the one with the newest `last_indexed`. Symbol ids from `julie-extractors` (`generate_id` at `base/extractor.rs:340`) derive from the relative path, name, line, and column, so rows for an unchanged file are identical across checkouts and copy verbatim.

**File ownership:** as listed in the Parallel Execution Contract row for Task 7 (verbatim).

**Serialization required:** Yes

**Dependency reason:** Depends on Task 5 (a seeded copy must pass the schema-version check) and Task 3 (`open` no longer swaps).

**What to build:** A new worktree of an indexed repo copies its sibling's `symbols.db` and `tantivy/` directory, rewrites the one `workspaces` row (`id`, `path`, `name`), then runs the existing incremental scan, which re-extracts the files whose hash differs, adds new files, and removes files that are not in the new tree. Tantivy is then brought current by the same projection call `index` uses (`backfill_tantivy_if_needed`, `index.rs:313`).

**Approach:** Under the mutation guard: `std::fs::copy` the sibling `symbols.db` (open the sibling read-only first and run `PRAGMA wal_checkpoint(TRUNCATE)` so the copy is one file), copy the `tantivy/` directory with `fs_extra`-free recursive copy (a 15-line helper), open the copy, `UPDATE workspaces SET id=?, path=?, name=?`. Then reuse the incremental path exactly as `manage_workspace refresh` does. The test builds `fixtures/seed/a`, indexes it into temp home, then `git init` is not needed: inject the sibling lookup as a function parameter (`sibling: Option<&Path>`) so the unit test passes the sibling root directly and only one small test covers `git_common_dir` with a real `git worktree add` in a temp dir (design rule 4: under two seconds, in-process). Assert `copied_files == 6`, `reextracted_files == 2`, `removed_files == 0`, and that `fast_search` on the new workspace finds a symbol that lives only in a shared file.

**Acceptance criteria:**
- [ ] `git_common_dir` returns the same canonical path for a main checkout and a linked worktree created in the test, and `None` for a non-git directory.
- [ ] Seeding `fixtures/seed/b` from an indexed `fixtures/seed/a` copies 6 files, re-extracts 2, removes 0, and a search on `b` finds a shared-file symbol.
- [ ] Seed time for a real worktree of the Julie repo is recorded in the commit message (report-only).
- [ ] `cargo nextest run --lib tests::tools::workspace::seed` and `cargo nextest run --lib tests::tools::workspace::global_targeting::open_lifecycle` pass; `cargo build` green; committed per commit mode.

---

### Task 8: `rebuild`, `status`, and retiring `register`, `clean`, `stats`

**Files:**
- Create: `src/tools/workspace/commands/registry/status.rs` (`handle_status_command`, `CheckoutStatus` struct, under 200 lines), `src/tests/tools/workspace/status_rebuild.rs`
- Modify: `src/tools/workspace/commands/mod.rs:40-53,123-227,302-351`, `src/tools/workspace/commands/registry/mod.rs`, `src/tools/workspace/commands/registry/list_clean.rs` (`handle_clean_command` deleted; `handle_list_command` keeps `run_cleanup_sweep`), `src/tools/workspace/commands/registry/register_remove.rs` (`handle_register_command` deleted), `src/tools/workspace/commands/registry/refresh_stats.rs` (`handle_stats_command` `:281-372` deleted; file renamed `refresh.rs`), `src/request_engine/catalog.rs:201-212` (access map: `list|health|dashboard|status` are `Read`), `src/service/status.rs` (`StatusDocument` gains `checkouts: Vec<CheckoutStatus>`), `src/service/http.rs` (`status` handler asks the engine for `CheckoutStatus` per known workspace), `src/cli_tools/subcommands.rs:261-281`, `src/cli_tools/commands.rs:281-321` (`validate_standalone` allows `open`, `remove`, `refresh`, `rebuild`, `status`), `src/tests/service/http_api.rs`, `src/tests/tools/workspace/global_targeting/list_stats.rs`, `src/tests/mod.rs`, `xtask/test_tiers.toml`, `JULIE_AGENT_INSTRUCTIONS.md`
- Test: `src/tests/tools/workspace/status_rebuild.rs`, `src/tests/service/http_api.rs`

**Interfaces:**
- Consumes: `HealthChecker::system_snapshot` (`src/tools/workspace/commands/registry/health.rs:10-36`) stays as `health`. `WorkspaceRegistryStore` for the checkout list. `SeedReport` from Task 7 is not consumed here.
- Produces: `CheckoutStatus { workspace_id, root, root_exists, watcher: "running" | "stopped", last_file_event_at: Option<String>, file_count, symbol_count, db_bytes, tantivy: "present" | "building" | "absent", tantivy_age_seconds: Option<u64>, vector_count, last_write_at: Option<String> }`, `Serialize`, returned by `manage_workspace status` as `structured_content` (plus a text rendering) and embedded in `/status` as `checkouts`. `manage_workspace rebuild` (`IndexMutation` access; `workspace_id` or `path` required) deletes `indexes/<id>/` under the mutation guard and runs `index --force`; it returns the `index` message prefixed with `Rebuilt <root>`.

**Contract inputs:** design section 10 per-checkout fields; design section 14.2 operation list; Miller's `workspace` contract keeps `refresh` and `health`.

**File ownership:** as listed in the Parallel Execution Contract row for Task 8 (verbatim).

**Serialization required:** Yes

**Dependency reason:** Depends on Task 7 (`open` result shape) and Task 4 (`/status` per-checkout fields read the handler).

**What to build:** Two new operations, three retired, and the service status page shows every checkout.

**Approach:** `db_bytes` is `metadata(symbols.db).len()` plus the `-wal` file if present; `tantivy_age_seconds` is now minus the newest `meta.json` mtime under `tantivy/`; `vector_count` is `embedding_count` (`vectors.rs:201`); `last_file_event_at` and `watcher` come from `IncrementalIndexer` (add two getters if absent; no new state). Delete the `stats` tests in `list_stats.rs` and the `register`/`clean` cases wherever they live (Miller `search(query='operation: "register"')` across `src/tests`). Update the `validate_standalone` refusal list so the CLI reaches every remaining operation; the reason the CLI refused them (per-session primary) is gone after Task 3.

**Acceptance criteria:**
- [ ] `ManageWorkspaceOperation::OPERATIONS` equals the Global Constraints list and `valid_operations_help()` prints it.
- [ ] `manage_workspace status` on an indexed temp workspace returns every `CheckoutStatus` field with the expected values, and `GET /status` carries the same object under `checkouts`.
- [ ] `manage_workspace rebuild` recreates `indexes/<id>/` and the symbol count after equals the count before.
- [ ] `julie-server workspace status` and `julie-server workspace rebuild --path <root>` work from the CLI (asserted in `src/tests/cli_execution_tests.rs`).
- [ ] `cargo nextest run --lib tests::tools::workspace::status_rebuild`, `cargo nextest run --lib tests::service::http_api`, `cargo nextest run --lib tests::cli_execution_tests` pass; `cargo build` green; committed per commit mode.

---

### Task 9: Gates: durable roots, complexity words, tiers

**Files:**
- Create: `scripts/complexity-words.sh`, `src/tests/service/durable_roots.rs`
- Modify: `src/tests/service/mod.rs`, `src/tests/service/budget.rs` (the coordination-words test is extended to `src/tools/workspace/indexing/seed.rs` and `src/tools/workspace/commands/registry/status.rs`), `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs` (the `service` bucket gains the command `sh scripts/complexity-words.sh main`; there is no CI test workflow, only `pages.yml` and `release.yml`)
- Test: `src/tests/service/durable_roots.rs`, `src/tests/service/budget.rs`

**Interfaces:**
- Consumes: the finished tree.
- Produces: `scripts/complexity-words.sh [<base>]` prints every added line in `git diff <base>...HEAD -- '*.rs'` (excluding `/tests/`) that contains a banned word as a whole token and exits 1 if any; `# complexity-exception: <reason>` on the same line allowlists it. `durable_roots` test: index a temp workspace through the service `/api/manage_workspace`, then assert `$JULIE_HOME` contains exactly `registry.db`, `service.json`, `indexes/`, and `indexes/<id>/` contains exactly `db/` and `tantivy/`, with `db/` containing only `symbols.db`, `symbols.db-wal`, `symbols.db-shm`.

**Contract inputs:** design section 12 budget table rows "Durable roots" and "Coordination words in new code". Tier membership: `service` stays in `fast`; every bucket deleted in Tasks 2 and 6 is removed from every tier; the `reliability` tier that `CLAUDE.md` names does not exist in `xtask/test_tiers.toml`; Task 10 removes it from the docs.

**File ownership:** Create `scripts/complexity-words.sh`, `src/tests/service/durable_roots.rs`; modify `src/tests/service/mod.rs`, `src/tests/service/budget.rs`, `xtask/test_tiers.toml`, `xtask/tests/support/manifest_contract_expected.rs`

**Serialization required:** Yes

**Dependency reason:** Runs against the finished tree.

**What to build:** The two section-12 gates this phase can measure, plus tier cleanup.

**Approach:** The script is POSIX `sh` with `git diff --unified=0` and `grep -Ewi`. The word list is the fourteen words from design section 4. The test lives in `tests::service` so it runs in `fast`. Run `cargo xtask test list` and `cargo nextest run -p xtask` to prove the manifest contract agrees after tier edits; record `cargo xtask test fast` wall time in the commit message (report-only).

**Acceptance criteria:**
- [ ] `scripts/complexity-words.sh main` exits 0 on this branch, and exits 1 on a scratch commit that adds `let lease = 1;` (demonstrated in the commit message).
- [ ] `durable_roots` passes and lists nothing beyond the Global Constraints layout.
- [ ] `cargo nextest run -p xtask` passes with the updated bucket inventory; `cargo xtask test list` shows no bucket whose test module no longer exists.
- [ ] `cargo build` green; committed per commit mode.

---

### Task 10: Measurements, docs, ledger, gate verdict (lead)

**Files:**
- Create: `docs/findings/2026-09-DD-machine-service-phase2-gate.md`, `docs/plans/2026-09-09-machine-service-phase2-ledger.md`
- Modify: `CLAUDE.md` and `AGENTS.md` ("Filewatcher Mutation Gate" section: the eight writers list and the `leadership` text; "Per-Workspace Isolation" bullet 3), `docs/WORKSPACE_ARCHITECTURE.md`, `docs/ARCHITECTURE.md`, `docs/TESTING_GUIDE.md` (embedding default, no multi-process lifecycle suites), `README.md`, `docs/plans/2026-09-09-machine-service-design.md` section 14 (record the phase 2/3 split from **Sequencing decision**)

**Interfaces:**
- Consumes: the finished tree at the final code commit.
- Produces: the gate verdict at the top of the finding: `tokei` Rust lines for `src crates xtask` excluding tests before (`151101` at `3e2a6505`) and after; the deletion-list ledger with each section 5.4 item marked `deleted` or `deferred to phase N because <reason>`; store size of `symbols.db` and `tantivy/` for the Julie repo; seed time for a Julie worktree; `fast` wall time; leaked-process check after `dev` (`ps -eo cmd | grep -c '[m]ock-broker\|[j]ulie-embedding-host'` prints `0`).

**Contract inputs:** design section 14 phase 2 gate: "the deletion list in section 5.4 is empty, and the phase is net negative in lines."

**File ownership:** as listed in the Parallel Execution Contract row for Task 10 (verbatim).

**Serialization required:** Yes

**Dependency reason:** Lead task; needs the final tree.

**What to build:** The evidence that the gate holds, and docs that describe one writer per checkout instead of leader and follower.

**Approach:** Run `cargo xtask test dev`, `system`, `bucket service-process`, then `full`, recording each in the ledger with the HEAD SHA. Run `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` and compare with the baseline. If lines are not net negative, the phase does not pass; report which task fell short rather than adjusting the gate.

**Acceptance criteria:**
- [ ] The finding states the verdict, the before/after line counts, the deletion-list ledger, and the report-only numbers.
- [ ] `CLAUDE.md`, `AGENTS.md`, `docs/WORKSPACE_ARCHITECTURE.md` no longer mention leader, follower, `leader.lock`, writer fencing, or the Python sidecar as product behavior (`grep -n` in the finding).
- [ ] The ledger has rows for `dev`, `system`, `service-process`, and `full` at the final HEAD, all `pass`.
- [ ] `cargo xtask test dev` green at the final commit.

## Execution handoff

- `local_commit_authority: authorized — the user asked for the phase 1 review and the phase 2 plan on the machine-service branch; phase 1 execution committed per task under the same authority.`
- `push_authority: missing — no user instruction to push.`
- `pr_authority: missing — no user instruction to open a PR.`
- Reviewer choice: `none` unless the approval names one.
