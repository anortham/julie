# Watcher Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Reduce daemon log growth, duplicate watcher work, and queue overflow churn while preserving correctness after large file bursts, worktree deletion storms, atomic saves, and catch-up indexing.

**Architecture:** Move watcher queue policy into a small module that coalesces path events before they become backlog. Make the runtime drop already-covered duplicate modify/create events instead of replaying them once per second, and make overflow repair scan only changed, deleted, or new files. During catch-up indexing, drain watcher notifications without queueing them and schedule one repair pass after the catch-up completes.

**Tech Stack:** Rust, Tokio, notify, SQLite file hashes, Tantivy projection commits, Julie watcher runtime, cargo nextest, xtask tiers.

---

## Problem Frame

Recent daemon logs showed two separate patterns:

- The old `.ogg` storm on 2026-04-27 was dominated by five audio files repeatedly reaching watcher processing. That path is already mitigated by the `.ogg` blacklist.
- The current issue is queue backpressure. May 3 and May 4 had hundreds of `Watcher queue exceeded 1000 items; dropped 1 oldest events` warnings in millisecond bursts, followed by repair scans and thousands of per-file processing logs.

The code explains the shape:

- `src/watcher/events.rs:197-219` drops only one oldest event per overflow and logs every drop.
- `src/watcher/runtime.rs:396-492` requeues recent duplicate events instead of discarding or coalescing them.
- `src/watcher/mod.rs:374-484` keeps accepting and queueing watcher events while catch-up indexing has paused queue dispatch.
- `src/watcher/runtime.rs:494-622` dispatches every indexed file during overflow repair, even unchanged files.
- `src/watcher/handlers.rs:71-306` logs per-file processing at `info!`, so normal bursts inflate daemon logs.

The plan below treats these as one watcher hardening batch because the invariants overlap: no lost changes, bounded queue growth, and one eventual staleness repair after dropped or paused events.

## File Structure

**Create**
- `src/watcher/queue.rs`
  - Own queue coalescing, overflow drain policy, event merge rules, and small pure helpers.
- `src/tests/integration/watcher_queue.rs`
  - New focused tests for queue policy, duplicate dropping, overflow behavior, and paused-event behavior. Do not add more tests to `src/tests/integration/watcher.rs` except small edits to existing tests.

**Modify**
- `src/watcher/mod.rs:14-18`
  - Export the new queue module.
- `src/watcher/mod.rs:63-113`
  - Add `paused_event_count: Arc<AtomicUsize>` to `IncrementalIndexer`, clone it into the event detector, and reset it after the queued repair pass records the count.
- `src/watcher/mod.rs:374-484`
  - Pass `pause_flag` into the event detector task and suppress queueing while catch-up indexing is active.
- `src/watcher/events.rs:1-219`
  - Delegate enqueue behavior to `watcher::queue`, and return enough queue outcome data for tests and logs.
- `src/watcher/runtime.rs:1-641`
  - Use queue helpers for recent duplicate decisions.
  - Process only useful repair-scan work.
  - Emit summary logs instead of per-event info logs.
- `src/watcher/handlers.rs:71-306`
  - Move per-file `Processing file`, `Extracted`, and `Successfully indexed` logs from `info!` to `debug!`, or remove any line made redundant by runtime summaries.
- `src/tests/mod.rs:166-190`
  - Register `integration::watcher_queue`.
- `src/tests/integration/watcher.rs:740-886`
  - Adjust existing queue tests only where their old expectations conflict with the new policy.
- `src/tests/integration/stale_index_detection.rs:706`
  - Preserve the existing deleted-file cleanup assertion and add a same-test second repair check if targeted repair changes the startup repair path.

**Do Not Modify**
- `src/handler.rs`
  - `run_auto_indexing` already pauses primary workspace updates through `run_primary_workspace_repair`; this plan hardens watcher ingestion around that pause.
- `src/tools/workspace/indexing/index.rs`
  - Full indexing should not need changes; this plan changes watcher ingestion around it.

## Implementation Tasks

### Task 1: Queue Coalescing And Overflow Drain Policy

**Files:**
- Create: `src/watcher/queue.rs`
- Modify: `src/watcher/events.rs:190-219`
- Test: `src/tests/integration/watcher_queue.rs`

**What to build:** Replace one-event-at-a-time queue overflow with a queue policy that coalesces events by affected path before counting toward the queue cap. When overflow still happens, drain to a headroom target, set `needs_rescan`, and emit one summary per drain burst instead of one warning per incoming event.

**Approach:** Add helpers such as `enqueue_file_change`, `merge_file_change`, and `affected_path`. Keep the public storage type as `VecDeque<FileChangeEvent>` for a smaller patch, but scan from the back of the capped queue for an existing event with the same affected path before pushing. With a 1000 item cap, the O(n) scan is acceptable and simpler than changing the queue storage contract across the watcher.

Merge rules must be explicit:
- `Modified` plus `Modified` becomes the newest `Modified`.
- `Created` plus `Modified` keeps `Created`.
- `Deleted` plus later `Created` or `Modified` becomes `Modified`, covering atomic saves where the file exists again.
- `Created` plus `Deleted` becomes `Deleted`, so transient files do not leave stale DB rows.
- `Renamed { from, to }` keys by `to`; later `Modified` for `to` preserves the rename so the source path is retired.
- Distinct paths are not coalesced.

Overflow policy:
- Keep `MAX_QUEUE_SIZE = 1000`.
- Add `OVERFLOW_TARGET_SIZE = 750`.
- If a distinct incoming event would exceed the cap, drain oldest events until the queue length is at or below the target, then push the new event.
- Set `needs_rescan` whenever events are drained.
- Log one warning with `max_size`, `target_size`, `dropped`, and final queue length.

**Acceptance criteria:**
- [ ] Repeated modify events for one path leave one queued event.
- [ ] Atomic save sequences do not leave both a stale delete and a fresh modify for the same path.
- [ ] Rename followed by modify still removes the old path and indexes the new path.
- [ ] Overflow drains a chunk, not one item per incoming event.
- [ ] Worker exact tests pass and the worker report states the queue invariant proved.

### Task 2: Runtime Duplicate Handling And Batch Summaries

**Files:**
- Modify: `src/watcher/runtime.rs:396-492`
- Modify: `src/watcher/handlers.rs:71-306`
- Test: `src/tests/integration/watcher_queue.rs`

**What to build:** Stop requeueing recent duplicate create/modify events. A duplicate that arrives within the debounce window after the same path was successfully processed is already covered, so it should be discarded. Deletes and renames must still be processed because they change indexed state.

**Approach:** Replace the current `should_skip` branch that pushes the skipped event back into the queue. Keep a counter for `processed`, `dropped_duplicates`, `deletes`, `renames`, and `remaining_queue_len`. Emit one summary per tick when any count is nonzero. Leave detailed per-file logs at `debug!` for diagnosis.

The runtime should still retain the head-of-line fix from `test_dedup_requeue_does_not_block_subsequent_events`: one duplicate path must not prevent later paths from processing in the same tick. The expected behavior changes from "file_a remains in the queue" to "file_a is dropped because a recent successful processing already covers it."

**Acceptance criteria:**
- [ ] A recent duplicate `Modified` event is not requeued.
- [ ] A recent duplicate `Created` event is not requeued.
- [ ] `Deleted` and `Renamed` events are processed even when the target path was recently processed.
- [ ] A duplicate event for path A does not block path B in the same tick.
- [ ] `info!` logs describe batch summaries, not every unchanged duplicate file.

### Task 3: Pause-Aware Event Ingestion During Catch-Up Indexing

**Files:**
- Modify: `src/watcher/mod.rs:374-484`
- Modify: `src/watcher/events.rs:19-188`
- Test: `src/tests/integration/watcher_queue.rs`

**What to build:** When catch-up indexing pauses watcher dispatch, the event detector should continue draining notify events but avoid filling `index_queue`. Relevant events seen while paused should set `needs_rescan`, then get dropped. After catch-up resumes, the normal queue runtime will run one repair scan.

**Approach:** Pass `pause_flag` into the event detector task. Add a lightweight classification path in `events.rs` that applies the same filtering rules as `process_file_system_event` without queueing. If the event has at least one relevant path and the watcher is paused, set `needs_rescan` and increment a paused-event counter. Suppress repeated paused-event info logs; emit a summary when the queue processor resumes or when a repair scan starts.

This is the key fix for the May 4 shape: full indexing of `hermes-agent` started, the watcher was effectively paused for processing, but the event detector still queued enough events to overflow.

**Acceptance criteria:**
- [ ] While `pause_flag` is true, relevant notify events do not increase `index_queue.len()`.
- [ ] While `pause_flag` is true, relevant notify events set `needs_rescan`.
- [ ] Ignored files remain ignored during paused ingestion.
- [ ] Catch-up indexing can complete without watcher queue overflow caused by its own paused window.

### Task 4: Targeted Overflow Repair Scan

**Files:**
- Modify: `src/watcher/runtime.rs:494-622`
- Test: `src/tests/integration/watcher_queue.rs`
- Test: `src/tests/integration/stale_index_detection.rs:706`

**What to build:** Make overflow repair scan dispatch only real work: deleted indexed files, changed indexed files, and new supported files. It should not reprocess every indexed file after every overflow.

**Approach:** Use the database file hash map, `crate::database::calculate_file_hash`, and `startup::scan_workspace_files`. For each indexed file:
- If the path no longer exists, dispatch `Deleted`.
- If the path exists and its hash differs from the stored hash, dispatch `Modified`.
- If the path exists and its hash matches, skip it.

For each workspace file not in the indexed set:
- Dispatch `Created` only if it passes the same supported-file rules used by normal indexing.

The repair scan should log one summary: checked indexed files, skipped unchanged files, deleted files, modified files, new files, failed hash reads, and elapsed time. Commit Tantivy only if at least one dispatched event touched the search index.

**Acceptance criteria:**
- [ ] Overflow repair skips unchanged indexed files.
- [ ] Overflow repair deletes missing indexed files.
- [ ] Overflow repair indexes changed files.
- [ ] Overflow repair indexes new supported files.
- [ ] Overflow repair does not dispatch unsupported extensionless files like `.dockerignore` or unsupported extensions like `.nix`.
- [ ] Existing startup stale-index tests still pass.

### Task 5: Repair Retry Filtering For Unsupported Files

**Files:**
- Modify: `src/watcher/runtime.rs:157-286`
- Test: `src/tests/integration/watcher_queue.rs`
- Test: `src/tests/integration/watcher.rs:960-1023`

**What to build:** Align persisted repair retry with actual extractor support. The current code clears unsupported extensions such as `.ogg`, but it allows extensionless files through. Logs show extensionless or unsupported-name files such as `.dockerignore`, `Dockerfile`, and `.nix` reaching extraction failure during watcher repair.

**Approach:** Add a helper that asks whether a repair path is indexable by the watcher and extractable by the current extractor registry. Clear repair rows for unsupported files instead of dispatching them every retry cycle. Do not rely on a language fallback that maps unknown extensionless files to `text` unless there is an extractor capable of handling that language.

**Acceptance criteria:**
- [ ] Existing `.ogg` repair retry regression still passes.
- [ ] Persisted repair for `.dockerignore` is cleared without extraction.
- [ ] Persisted repair for `Dockerfile` is cleared unless an extractor for that detected language exists.
- [ ] Persisted repair for `.nix` is cleared without extraction.
- [ ] Supported files with extractor failures are still retried after the configured retry interval.

### Task 6: Verification Ledger And Runtime Evidence

**Files:**
- Modify: `docs/plans/2026-05-04-watcher-hardening.md`

**What to build:** Record all worker and lead verification in the ledger below. After implementation, capture one concise before/after log-shape check using a synthetic burst test or a local daemon dogfood check, if the lead can do it without broad manual setup.

**Approach:** Exact unit or integration tests are the hard gates. Runtime log comparison is report-only unless a deterministic test is added for it. If a synthetic CLI dogfood check is used, use the debug binary path from `AGENTS.md`: `./target/debug/julie-server <tool-command> --workspace . --standalone --json`.

**Acceptance criteria:**
- [ ] Verification ledger contains worker exact-test evidence.
- [ ] Lead `cargo xtask test changed` evidence is recorded after the coherent batch.
- [ ] Lead `cargo xtask test dev` evidence is recorded before handoff.
- [ ] Any runtime log-shape check clearly labels hard-gate assertions versus report-only metrics.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, and `xtask/test_tiers.toml`.

**Worker red/green scope:** Workers run exact tests only, for example `cargo nextest run --lib test_queue_coalesces_latest_event_per_path 2>&1 | tail -10`. Each worker writes or updates a failing test first, verifies it fails, implements the change, then reruns the exact test.

**Worker ceiling:** Workers must not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test system`, `cargo xtask test reliability`, or broad `cargo nextest run --lib`. Workers may run at most exact tests assigned to their task.

**Worker gate invariant:** Each worker report must state the watcher invariant proved:
- Queue policy worker: queue coalescing and overflow chunk-drain preserve eventual repair.
- Runtime worker: recent duplicate create/modify events do not replay forever.
- Pause worker: catch-up indexing does not fill the watcher queue while dispatch is paused.
- Repair worker: overflow repair dispatches only changed, deleted, and new supported files.
- Repair-retry worker: unsupported persisted repairs are cleared instead of retried forever.

**Lead affected-change scope:** After a coherent batch, the lead runs `cargo xtask test changed`.

**Branch gate:** Before handoff, the lead runs `cargo xtask test dev` once.

**Replay/metric evidence:** Exact tests are hard gates. Queue length, overflow warning count, and daemon log growth from synthetic or dogfood runs are report-only unless encoded as assertions in a deterministic test.

**Escalation triggers:** Escalate to strategy or escalation tier if a change touches daemon watcher sharing, workspace identity, Tantivy projection state, catch-up indexing lifecycle, or if queue coalescing breaks rename/delete correctness. Also escalate if two worker attempts fail review or if broad verification reports database lock regressions.

**Assigned verification failure:** Workers stop and report when assigned verification fails, unless this plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, and timestamp. For replay or metric evidence, also record hard-gate metrics and report-only metrics. If the same HEAD already has a passing ledger entry for the required scope, reuse that evidence instead of rerunning the same expensive gate.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Queue overflow drains to headroom and schedules repair | `cargo nextest run --lib test_queue_overflow_drains_to_headroom 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 1/1 | 2026-05-04T14:19:37Z | No |
| Paused watcher ingestion drops queued work and sets rescan | `cargo nextest run --lib test_paused_event_ingestion_sets_rescan_without_queueing 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 1/1 | 2026-05-04T14:19:37Z | No |
| Recent duplicate create/modify events are dropped without blocking delete or rename | `cargo nextest run --lib test_runtime_drops_recent_duplicates_and_processes_delete_and_rename 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 1/1 | 2026-05-04T14:19:37Z | No |
| Overflow repair skips unchanged indexed files | `cargo nextest run --lib test_overflow_repair_skips_unchanged_indexed_files 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 1/1 | 2026-05-04T14:19:37Z | No |
| Overflow repair processes changed, deleted, and new supported files only | `cargo nextest run --lib test_overflow_repair_processes_changed_deleted_and_new_supported_only 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 1/1 | 2026-05-04T14:19:37Z | No |
| Unsupported extensionless and unsupported-name repairs are cleared | `cargo nextest run --lib test_repair_retry_clears_unsupported_extensionless_and_unsupported_names 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | RED failed on `.dockerignore` and `Dockerfile`; GREEN PASS, 1/1 | 2026-05-04T14:19:37Z | No |
| Supported extractor failures remain retry candidates | `cargo nextest run --lib test_repair_retry_keeps_supported_extractor_failures_due_for_retry 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 1/1 | 2026-05-04T14:19:37Z | No |
| Existing `.ogg` repair regression still clears unsupported extensions | `cargo nextest run --lib test_repair_retry_clears_unsupported_extension 2>&1 \| tail -10` | worker-red-green | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 2/2 because the filter also matched the new unsupported-name test | 2026-05-04T14:19:37Z | No |
| Changed files satisfy the affected gate | `cargo xtask test changed` | affected-change | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, fell back to dev tier, 22 buckets passed in 468.3s | 2026-05-04T14:19:37Z | No |
| Branch gate passes before handoff | `cargo xtask test dev` | branch-gate | `0e8009f0e840a3ab0e08b421c29a4a6f8cf36732 + worktree` | PASS, 22 buckets passed in 440.3s | 2026-05-04T14:19:37Z | No |

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** Planning, architecture, decomposition, lead review, and finding triage.
- Harness mapping: Codex `gpt-5.5` medium or high.

**Implementation tier:** Bounded worker tasks from this clear plan.
- Harness mapping: Use Codex `gpt-5.3-codex` high for watcher, queue, catch-up, and repair behavior because `RAZORBACK.md` treats watcher and lifecycle work as shared-invariant work.

**Mechanical tier:** Docs, fixture-only edits, formatting, and rote manifest changes with no test, replay, metric, or acceptance-gate ownership.
- Harness mapping: Codex `gpt-5.4-mini` low or medium.

**Gate-interpretation reviewer:** Review the plan, failing test, diff, and gate result when deciding whether the test or implementation is wrong.
- Harness mapping: Codex `gpt-5.3-codex` high.

**Escalation tier:** Subtle correctness, concurrency behavior, repeated failures, broad lifecycle risk, or weak tests.
- Harness mapping: Codex `gpt-5.3-codex` xhigh for terminal-heavy watcher debugging; Codex `gpt-5.5` high or xhigh if the plan or architecture is wrong.

**Worker eligibility:** Implementation-tier workers are eligible only for narrow, non-overlapping file scopes with exact tests: queue policy, runtime duplicate handling, paused ingestion, targeted repair scan, or repair retry filtering.

**Escalation triggers:** Escalate if a task changes watcher pool ownership, workspace routing, session lifecycle, public MCP behavior, or cross-workspace indexing contracts. Escalate on repeated database lock errors, rename/delete data loss risk, or ambiguous repair-scan semantics.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, or acceptance gates. Split docs-only updates from evidence interpretation.

**Unsupported harness behavior:** If the harness cannot choose models per agent, use `inherit`, note it in the worker report, and continue.

## Task Decomposition

- Worker A: Queue policy module and queue-policy tests. Owns `src/watcher/queue.rs`, `src/watcher/events.rs`, `src/tests/integration/watcher_queue.rs`, and the `src/tests/mod.rs` registration for that new test file.
- Worker B: Runtime duplicate handling and log summary changes. Owns `src/watcher/runtime.rs` duplicate branch and `src/watcher/handlers.rs` log level changes.
- Worker C: Pause-aware ingestion. Owns `src/watcher/mod.rs` event detector changes and the paused-event tests in `src/tests/integration/watcher_queue.rs`.
- Worker D: Targeted repair scan. Owns `src/watcher/runtime.rs` repair scan changes and repair-scan tests.
- Worker E: Persisted repair filtering. Owns `src/watcher/runtime.rs` repair retry filtering and unsupported repair tests.
- Lead: Review merge semantics, run affected-change and branch gates, and verify the final log shape against the original daemon-log smell.

## Risks

- Rename coalescing can cause data loss if the source path is not retired. Preserve `Renamed { from, to }` through later target modifies.
- Delete/create atomic saves can be misclassified. If the file exists by dispatch time, prefer indexing the live file over deleting valid rows.
- Paused ingestion must not discard changes silently. Every relevant event seen while paused must set `needs_rescan`.
- Repair scan must not reintroduce full-workspace reindex behavior. Hash-matching indexed files should be skipped.
- Logging changes must not hide real failures. Keep warnings and errors visible; only demote per-file success chatter.
