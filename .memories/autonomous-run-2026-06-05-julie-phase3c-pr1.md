# Autonomous Execution Report — Julie Rescue PR 3c.1 (leader-lock + hang-guard infra)

**Status:** Complete
**Plan:** docs/plans/2026-06-05-julie-phase3c-inprocess-leader.md (PR 3c.1 of 3)
**Branch:** julie-rescue-phase3c
**PR:** https://github.com/anortham/julie/pull/30
**Duration:** ~1 session (T1–T4 + T13 + codex pre-merge review + F3 fix)
**Phases:** PR 3c.1 of 3 (3c.1 / 3c.2 / 3c.3) complete
**Tasks:** 5/5 (T1, T2, T3, T4, T13) + 1 codex-finding fix (F3)

## What shipped

PR 3c.1 is the **additive foundation** for Phase 3c's cutover to an in-process MCP server. Nothing here runs at runtime yet — `main.rs` is not flipped (3c.3 does that). The daemon/adapter path is fully intact; these pieces land behind a green build.

- **T1** (`be2855a0`) — `DaemonPaths::workspace_leader_lock(workspace_id)` → `indexes/{ws}/leader.lock`, sibling of `db/`+`tantivy/`, distinct from the Tantivy rebuild lock. D7 finding: the in-process server replaces the *adapter* (not stdio), so it uses the shared `~/.julie/indexes` layout — derive index dir + lock from ONE `DaemonPaths`.
- **T2** (`c2d35c27`) — lifted `DaemonLockGuard` + `AcquireError` + `LockAlreadyHeld` + in-process dedup (`HELD_DAEMON_LOCKS`/`normalize_lock_path`) from `src/daemon/discovery.rs` into `crates/julie-core/src/workspace/leader_lock.rs`; re-exported from the old path so all 3 daemon callers compile unchanged. No behavior change. `fs2` already in julie-core.
- **T13** (`9010a71a`) — design-only: §11 dashboard persist-vs-drop table in `docs/plans/2026-06-04-julie-phase3-daemon-teardown-design.md`. Only new 3d write-site: `started_at_unix` → `registry.db.daemon_state`.
- **T3** (`d5e9ad25`) — handler per-request deadline: `dispatch_with_deadline` wraps `tool_router.call` in `tokio::time::timeout` (default 120s, env `JULIE_INPROCESS_REQUEST_TIMEOUT_SECS`, `"0"`=off). Writers exempt.
- **T4** (`d53fb00a` + `3e0d7f55`) — `JulieServerHandler::new_in_process(startup_hint, embedding_provider, leader)` + `LeadershipState` (`src/leadership.rs`). Preserves `startup_hint.source` (delegates to `new_deferred_daemon_startup_hint_with_project_log`), injects a shared embedding provider (returned first by `embedding_provider()`), carries `is_leader()`, isolates daemon deps to `None`. Fixup added the source-preservation test + `Arc::ptr_eq` provider assert + extracted `LeadershipState` to its own module.
- **codex F3 fix** (`44691692`) — operation-aware deadline exemption (see External review).

## Judgment calls (non-blocking decisions made)

- **T4 ctor — delegate, not mirror:** `new_in_process` delegates to `new_deferred_daemon_startup_hint_with_project_log` (which preserves `startup_hint.source` + gives a per-project `ProjectLog` + writes-enabled) instead of mirroring `new()`. Better base than re-implementing source preservation; aligns with T10's per-project-log plan. (lock-dev's call, accepted on review.)
- **T4 — `Arc<LeadershipState>` field, `LeadershipState::leader(bare guard)`:** the guard is wrapped in `Arc` at the field, not inside `LeadershipState`. Equivalent for `Clone`; cleaner call-site API (no `Arc::new` at the call). Kept lock-dev's shape over the plan's `Option<Arc<guard>>` spec.
- **codex F3 fix — inlined `from_arguments` logic:** `ManageWorkspaceOperation::from_arguments` is private, so `is_write_exempt` inlines the equivalent (`arguments.get("operation").as_str().and_then(parse)`) — verified byte-identical to the canonical parser at `commands/mod.rs:65`. Keeps the fix contained to `handler.rs`.
- **Right-sized post-fix gate:** did NOT re-run the full 21-min dev tier for the isolated F3 deadline-exemption change; ran the handler test subtree (47/47) @ the F3 HEAD instead. The other 35 buckets test byte-identical code already green @ `3e0d7f55`. (Owner explicitly flagged over-gating during the run.)

## External review (codex, adversarial)

- **Findings:** 3 (verdict: needs-attention). All verified real against the code via Julie MCP (`deep_dive ManageWorkspaceOperation`, `get_symbols ensure_primary_workspace_for_request`). **Zero dismissed.**
- **Verified real, fixed:** 1 (commit `44691692`)
  - **F3 (medium)** — `EXEMPT_WRITER_TOOLS` exempted the whole `manage_workspace` tool, but its enum has read-only ops (List/Stats/Health) → a hung stats/health read escaped the hang guard. Fixed: `is_write_exempt(tool_name, arguments)` is operation-aware — exempt only Index/Register/Remove/Clean/Refresh/Open; bound List/Stats/Health + unparseable. `dispatch_with_deadline` now takes explicit `exempt: bool`. +10 unit tests (16/16 GREEN).
- **Dismissed:** 0
- **Flagged for your review (carried into 3c.2 via plan commit `72963ad7`):** 2
  - **F1 (high)** — the T3 deadline wraps only `tool_router.call`; the pre-dispatch `ensure_primary_workspace_for_request` (verified: `list_roots_from_peer` client round-trip + `complete_deferred_auto_index_if_needed` indexing) is unbounded, so a Cwd-path first read can still hang. *Why flagged, not fixed:* the correct fix (bound the whole read request, push repair to a non-cancellable background task) needs the in-process resolution/repair structure that lands in 3c.2 (T9); a naive outward wrap would cancel an in-flight index write (codex concurs). Now a hard T9 acceptance gate + Risk #4 updated.
  - **F2 (high)** — `new_in_process` (daemon_db/workspace_pool=None) resolves to project-local `.julie/indexes` storage while the leader lock is `DaemonPaths`(`~/.julie`)-based → different dirs (the inode mismatch D7/Risk #6 warned of). *Why flagged, not fixed:* the lock isn't acquired until 3c.2 (T8 wires serve + acquisition); T8 must thread one shared `index_root` so storage+lock share `{indexes}/{workspace_id}`. Now a hard T8 acceptance gate + Risk #6 raised to high.
- codex does not surface per-request token counts; no reviewer cost recorded.

## Tests

- **Branch-gate:** `cargo xtask test dev` → **37/37 buckets GREEN, 1281.6s** @ `3e0d7f55` (incl. daemon 61.7s + dashboard 14.7s — the lock-lift regression surface).
- **Post-F3-fix:** handler test subtree (`tests::core::handler::`) **47/47 GREEN** @ `44691692`. F3 is isolated to the deadline-exemption decision; the other buckets test byte-identical code.
- **Per-task narrow (lead-verified at HEAD):** julie-core leader_lock+paths 2/2; julie re-export `test_daemon_lock_try_acquire_fails_when_held`; deadline 6→16; inprocess_ctor 3→4.

## Blockers hit

- None. One process note: workers occasionally swept `.memories/` checkpoints into commits via broad `git add` — harmless (project rule commits `.memories/`), and no forbidden files (`.codex/config.toml`, `.miller/`) ever leaked. Reminded workers to stage by explicit path.

## Files changed

13 files, +1071 / -212 (code+docs; excludes `.memories/`):
- New: `crates/julie-core/src/workspace/leader_lock.rs` (+224), `src/leadership.rs` (+42), `src/tests/core/handler/deadline.rs` (+184), `src/tests/core/handler/inprocess_ctor.rs` (+145), plan doc (+174).
- Modified: `src/handler.rs` (+186/-…), `src/daemon/discovery.rs` (-213, lifted), `crates/julie-core/src/paths.rs` (+14), `crates/julie-core/src/tests/paths.rs` (+51), Phase 3 design doc (+46 §11), `src/lib.rs` (+1), `crates/julie-core/src/workspace/mod.rs` (+1), `src/tests/core/handler.rs` (+2).

## Next steps

- Review PR: https://github.com/anortham/julie/pull/30
- **Confirm F1/F2 scoping** — both flagged as 3c.2 (T8/T9) work, baked into the plan as hard acceptance gates; pull either forward if desired.
- After merge: **PR 3c.2** (T5–T9: watcher/writer leadership gating, host-backed embeddings via the 3b resident host, loser write-refusal, the `run_in_process_server` serve entry, handoff recovery) — where F1 and F2 are fixed.
- Then **PR 3c.3** (T10 flip `main.rs` + T11 kill-the-writer acceptance HARD GATE + T12 boundary tripwire) — the actual cutover.
