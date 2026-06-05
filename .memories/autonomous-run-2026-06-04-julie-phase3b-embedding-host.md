# Autonomous Execution Report - Julie Rescue Phase 3b: Resident Embedding-Host

**Status:** Complete
**Plan:** docs/plans/2026-06-04-julie-phase3b-embedding-host.md
**Branch:** julie-rescue-phase3b
**PR:** (see terminal pointer)
**Duration:** ~6h elapsed (T1–T8 implementation + codex pre-merge review + 4 fixes), multi-session
**Phases:** 1/1 complete (Phase 3b of the Julie rescue)
**Tasks:** 8/8 complete

## What shipped

Phase 3b stands up a resident **embedding-host** process that owns ONE PyTorch sidecar and serves `embed_query` / `embed_batch` / `health` to N Julie processes over a UDS (unix) / named-pipe (windows) front door. It is **additive and opt-in** (`JULIE_EMBEDDING_USE_HOST=1`), runs alongside the existing daemon, and proves "one sidecar serves N processes." It does NOT flip the daemon default or rewire stdio (that is Phase 3c).

- **T1** `DaemonPaths` embedding-host socket/lock/pipe helpers (`crates/julie-core/src/paths.rs`).
- **T2** Cross-platform IPC transport seam — blocking `HostClientConn` + async `HostListener`/`HostServerConn`; protocol ungated (`host_transport.rs`).
- **T3** `RpcEmbeddingProvider` — thin blocking RPC-client `EmbeddingProvider` with lazy connect + reconnect-once (`rpc_client.rs`).
- **T4** Embedding-host server — accept loop, per-connection dispatch, graceful shutdown, lock-first factory (`host_server.rs`).
- **T5** `julie-embedding-host` binary (`src/bin/julie-embedding-host.rs`).
- **T6** `connect_or_spawn_host` launch glue — sibling-binary locate, detached spawn, pinned child `JULIE_HOME` (`src/embedding_host_launch.rs`).
- **T7** Opt-in coexistence wiring in `spawn_embedding_init` — host path routed through the existing init match, Ready gated on a real health handshake (`src/daemon/app/helpers.rs`).
- **T8 (HARD GATE)** Lock-first factory refactor + hermetic 3-host-race acceptance test proving exactly one sidecar serves three concurrent sessions (`src/tests/daemon/embedding_host_multi_session.rs`).

## Judgment calls (non-blocking decisions made)

- `src/daemon/app/helpers.rs:~426` — F2 failure path sets `resolved_backend: Unresolved` (not `Sidecar`) because the host never resolved to a working sidecar; signals the unresolved state more accurately.
- `crates/julie-pipeline/src/embeddings/host_transport.rs:100` — F4 default RPC timeout is **120s, not 30s**: 30s could abort a legitimate slow/cold `embed_batch` on a CPU-only machine (a new bug); 120s bounds a true hang while never aborting real work. Operators tune down via `JULIE_EMBEDDING_HOST_RPC_TIMEOUT_SECS`.
- `crates/julie-pipeline/src/embeddings/host_transport.rs:110` — `parse_rpc_timeout` keeps `"0" → None` (no-timeout escape hatch) because `set_read_timeout(Some(Duration::ZERO))` errors on unix.
- `src/embedding_host_launch.rs:76` — `parse_spawn_timeout` keeps `filter(|n| *n > 0)` so `"0" → default 180s`; a 0s liveness poll would fail startup instantly.
- Restored `6d6d6ad4` timeout-helper shape (commit `6d74c0df`) over the churn commit `5c9d949e`, which regressed both `"0"` escape hatches for no benefit — chose lead-revert over another fix round to end the churn.

## External review (codex, adversarial)

- **Findings:** 4
- **Verified real, fixed:** 4 (commits: `03fde74a`, `db8ea61b`, `6d6d6ad4`)
  - **F1 (HIGH)** — shutdown released the fs2 singleton lock BEFORE dropping the listener socket, leaving a window for a second host to win the lock while the old socket was still bound. Fixed: `drop(listener)` before `drop(lock_file)` + invariant comment (`03fde74a`).
  - **F2 (HIGH)** — `RpcEmbeddingProvider` trait getters (`device_info`/`accelerated`/`degraded_reason`/`dimensions`) swallow a failed health handshake, so the daemon host path could publish Ready against a connectable-but-broken host. Fixed: added fallible `ensure_ready()`, gate the host path on it inside `spawn_blocking`, Err → `publish_unavailable`; new test `host_unavailable_when_health_not_ready` (`db8ea61b`).
  - **F3 (MED)** — 10s spawn-liveness timeout shorter than a ~120–180s cold sidecar init → spurious first-start failure. Fixed: 180s default + `JULIE_EMBEDDING_HOST_SPAWN_TIMEOUT_SECS` + pure parse unit test (`6d6d6ad4`).
  - **F4 (MED)** — no RPC socket timeout → a stalled host hangs `round_trip` forever. Fixed: `connect_with_timeout(Option<Duration>)` sets read/write timeouts, 120s default, env override, deterministic timeout-fires test + parse unit test (`6d6d6ad4`).
- **Dismissed:** 0
- **Flagged for your review:** 0

codex/claude do not surface per-request token counts in their JSON output, so no reviewer token cost is recorded.

## Tests

- **Branch-gate:** `cargo xtask test dev` → **37/37 buckets PASS, 1264.9s** @ `6d74c0df` (incl. `xtask-runner`, `core-pipeline`, `daemon`, `dashboard`). HEAD advanced one commit to `96d30f8e` (`.memories`-only, zero code/test delta), so the gate evidence holds.
- **Acceptance HARD GATE:** `one_sidecar_serves_three_sessions` (hermetic 3-host race) PASS — exactly one sidecar, two losers fail on the singleton lock.
- **Narrow re-verify @ HEAD:** pipeline `host_transport` + `host_server` 10/10; `rpc_client` green; `daemon::embedding_host_optin` 3/3 (incl. the new Unavailable test + the unchanged ready=true path); `parse_rpc_timeout`/`parse_spawn_timeout` unit tests green.

## Blockers hit

- None. One process incident (non-blocking, resolved): an un-frozen worker (`host-server-dev`) committed `5c9d949e` after its work was accepted, re-shaping the timeout helpers to match the example code in my fix-prompt; the mid-edit half-states broke the branch-gate mid-run and the reshape regressed both `"0"` escape hatches. Resolved by restoring the verified `6d6d6ad4` shape as lead-revert `6d74c0df`, then re-running the branch-gate green. Lesson recorded to memory (freeze every worker after acceptance; treat fix-prompt example code as load-bearing).

## Files changed

37 files, +3799 / -7. Highlights:
- New: `host_server.rs` (+323), `rpc_client.rs` (+335), `host_transport.rs` (+298), `src/bin/julie-embedding-host.rs` (+76), `src/embedding_host_launch.rs` (+258), `embedding_host_multi_session.rs` (+423), `embedding_host_optin.rs` (+247), `host_server_test.rs` (+313), `rpc_client_test.rs` (+196), `host_transport_test.rs` (+156), `crates/julie-core/src/tests/paths.rs` (+49).
- Modified: `src/daemon/app/helpers.rs` (+84/-x), `crates/julie-pipeline/src/embeddings/mod.rs`, `crates/julie-core/src/paths.rs` (+29), `Cargo.toml`/`Cargo.lock`/`.gitignore`, `src/daemon/app.rs`, `src/lib.rs`.
- Docs/state: plan doc + `.memories/` checkpoint trail.

## Next steps

- Review PR and human-merge-gate it (rescue-plan phases are merged by a human, like #22–#26).
- After merge: **Phase 3c** — flip the daemon default / rewire stdio onto the resident host (the Phase 3 decomposition's next step after 3b proves "one sidecar serves N").
- Optional cleanup: branch history includes the `5c9d949e` churn + `6d74c0df` revert; net tree is correct. Squash-merge will collapse it.
