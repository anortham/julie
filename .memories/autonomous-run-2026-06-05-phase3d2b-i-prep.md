# Autonomous Run — Phase 3d.2b-i (app-independent prep)

**Status:** Complete — PR opened, awaiting human merge
**Plan:** `docs/plans/2026-06-05-julie-phase3d-delete-daemon-adapter.md` (§3d.2b-i)
**Branch:** `julie-rescue-phase3d2b-i` (base `main`, merge-base `b13b1d40`)
**PR:** https://github.com/anortham/julie/pull/36
**Date:** 2026-06-05
**Phases complete:** 1/1 (3d.2b-i) · **Tasks complete:** 2/2 (i1, i2)
**Reviewer:** codex (gpt-5.5, escalation tier) — 1 finding, fixed in-scope

---

## What shipped

3d.2b-i is the **prep half** of 3d.2b (server-core kill), split out (owner-approved, 2a/2b pattern) so the big atomic delete (3d.2b-ii) lands clean. **Zero server-core deletions**; only the two genuinely app-INDEPENDENT couplings severed. No product/runtime code changed.

### i1 — gut xtask `dev-restart` daemon-control path (commit 99c68a2c)
- Removed `run_dev_restart`'s `lifecycle::{check_status,stop_daemon,DaemonStatus}` + `DaemonPaths` usage — the **only** xtask-crate consumers of those symbols (they stay in `lifecycle.rs` until 3d.2b-ii; severing the consumer first means -ii's deletion causes no cross-crate break).
- `dev-restart` is now **advisory** (post-3c.3 there's no shared daemon to restart/SIGTERM — each MCP session runs its own in-process `julie-server`, leader-locked). Prints guidance, performs no process control. `--force` removed; `DevRestartReport` removed (never consumed).
- cli.rs parse + main.rs dispatch updated; cli tests → takes-no-args + rejects `--force`.
- `changed.rs` lifecycle path→bucket mapping intentionally **left for 3d.2b-ii** (those files + the `lifecycle` bucket survive until then; removing the mapping now would break `changed` routing for still-present files).

### i2 — delete redundant `embedding_host_optin.rs` (commit 5c6d45a0)
- It was the sole kept-code consumer of the daemon-only `spawn_embedding_init` + `WatcherPool` (deleted in -ii).
- **Deleted, not repointed** — lead-verified zero coverage loss: its host-opt-in + `ensure_ready` invariant is already covered against the surviving `acquire_in_process_embedding_provider` by `inprocess_embedding.rs` (3 tests: ready=true→Some, ready=false→None, PROVIDER=none→None), and the `EmbeddingService` settle machine by `embedding_service.rs` units (Ready/Unavailable/Timeout/multi-waiter). The only unique behavior (`JULIE_EMBEDDING_USE_HOST` routing) is daemon-only with no in-process equivalent. Repointing would only duplicate `inprocess_embedding.rs`.
- After deletion, `spawn_embedding_init` has zero kept-code callers (all remaining refs in the `app/**` delete-set).

### docs (commits 99c68a2c + 1eeb87bf)
- Advisory `dev-restart` in CLAUDE.md/AGENTS.md maintainer dev-loop sections.
- "⚠️ in transition (Phase 3d)" banners on the legacy daemon/adapter/log sections (codex F1 fix).

---

## Plan corrections made before execution (the real value of this pass)

The reshaped plan I inherited still had **three premature items** that I caught + corrected via direct grep before writing any code:

1. **Pool de-typing is atomic with the app.rs deletion** — `app.rs:291` is the sole live `DashboardState::new_with_watcher_pool` caller and `app.rs:298/299/326` (+ `app/helpers.rs`, `app/handle.rs`) the sole live `Some(pool)` threaders into the handler ctors. Removing pool fields can't compile while app.rs is present → moved dashboard/handler/health/test-helper de-type from 3d.2b-i into the 3d.2b-ii atomic block.
2. **i2 should be a delete, not a repoint** — the "in-process equivalent" the plan wanted to repoint to already exists as a separate file (`inprocess_embedding.rs`). Repoint → duplication. Changed to delete-as-redundant.
3. **changed.rs lifecycle mapping is premature in -i** — the mapped files + the `lifecycle` bucket survive until -ii; moved to -ii step 1.

So 3d.2b-i shrank to exactly the two app-independent severs (the xtask cross-crate coupling + the kept-3b-test coupling — the two "headline misses" the earlier mapping workflow flagged).

---

## Tests (branch gate GREEN @ 5c6d45a0; HEAD delta is docs-only → code gate reused)

| Invariant | Command | SHA | Result |
|---|---|---|---|
| i1 dev-restart parse contract | `cargo nextest run -p xtask dev_restart` | 99c68a2c | 3/3 pass |
| cross-crate compile | `cargo build --workspace --bins` | 5c6d45a0 | pass |
| compile authority | `cargo nextest run -p julie --no-run` | 5c6d45a0 | pass |
| i2 surviving coverage | `cargo nextest run -p julie --lib inprocess_embedding` | 5c6d45a0 | 3/3 pass |
| whole-crate regression | `cargo nextest run -p julie --no-fail-fast -- --skip search_quality` | 5c6d45a0 | 1654/1656; 2 pre-existing |
| exit invariant | grep `lifecycle::{stop_daemon,check_status,DaemonStatus}` + `spawn_embedding_init` | 5c6d45a0 | zero live kept-code callers |

**The 2 superset failures — both pre-existing, isolate-verified, unreachable by this PR:**
1. `harness::in_process::test_in_process_daemon_starts_and_shuts_down` — deterministic (FAILS in isolation): `in_process.rs:231` asserts a bearer token at `daemon.token` that the post-3c.3 rmcp-stdio server never writes. Self-resolves in 3d.2b-ii (deletes the `InProcessDaemon` harness). Matches 3d.2a ledger #3.
2. `daemon::restart_listener::daemon_reaps_idle_session...` — load flake (PASSES single-threaded in isolation). Matches 3d.2a ledger #2.

Both are logically unreachable by 3d.2b-i: i1 is xtask-only (the julie test crate doesn't depend on xtask); i2 deleted an unrelated test.

---

## External review (codex, gpt-5.5, escalation @ 5c6d45a0)

Verdict **needs-attention**, 1 medium finding. Codex confirmed the positives: xtask `dev-restart` parse/dispatch coherent (no dangling `--force`/lifecycle consumer); deleted-test refs confined to the doomed `app/**` path. (It couldn't run `inprocess_embedding` — Unix-socket bind EPERM in its sandbox — that coverage is locally verified 3/3.)

- **F1 (medium) — real-improvement, partially in-scope, FIXED (commit 1eeb87bf):** my i1 dev-restart edit (now "no daemon") sharpened an internal contradiction with CLAUDE.md/AGENTS.md sections still describing daemon auto-start, `~/.julie/daemon.log`, stale-binary restart, and the wrong "Adapter mode (default): auto-starts the daemon ... bridges stdio to HTTP". Fixed in-scope (codex's own "label as legacy" recommendation): added "⚠️ in transition (Phase 3d)" banners to the Mode key-fact, the LOG-LOCATIONS daemon block, and the Architecture-#3 bullets in both files. The full prose rewrite (against the final post-daemon state) is **tracked to 3d.3** — doing it now would describe a transient half-deleted state that -ii/3d.3 re-touch.

Docs-only fix → no re-run (confirmed `git diff 5c6d45a0..HEAD` touches only `*.md`).

---

## Process notes / hazards hit

- **`cargo fmt` is poison here:** a bare `cargo fmt` reformatted **160+ files** of pre-existing workspace drift (this repo's CI does not enforce `cargo fmt --check`, or the local rustfmt differs from the canonical one). Recovered by reverting everything to HEAD except machine-local `.codex/config.toml`, then re-applying i1 by hand. **Do NOT run `cargo fmt` in this repo** — hand-format edits instead.
- `.git/index.lock` contention during recovery was Git's benign `fsmonitor--daemon`, not another session.
- CLAUDE.md↔AGENTS.md sync pre-commit hook is **non-executable** here, so both files were edited directly (kept byte-identical; verified `diff -q`).
- Staged every commit by explicit path; `.codex/config.toml` + `.claude/scheduled_tasks.lock` never committed.

## Commits (base..HEAD)
- `1d00ddfc` docs(plan): reshape 3d.2b-i to app-independent prep only
- `99c68a2c` refactor(rescue-3d2b-i): gut xtask dev-restart daemon-control path
- `5c6d45a0` test(rescue-3d2b-i): delete redundant embedding_host_optin test
- `8ba27070` docs(plan): record 3d.2b-i verification ledger
- `1eeb87bf` docs(rescue-3d2b-i): label legacy daemon/adapter sections in transition (codex F1)

## Next
**3d.2b-ii** — the atomic delete + pool de-type. app/** deletion is indivisible from de-typing handler/dashboard/health/test-helper off the pools (sole live threaders `app.rs:291` + `app.rs:298-326`). This is the substantial half → execute with the full TeamCreate visible Sonnet team.
