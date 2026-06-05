# Autonomous Execution Report - Julie Phase 3c.3 — The Cutover (in-process leader)

**Status:** Complete
**Plan:** docs/plans/2026-06-05-julie-phase3c-inprocess-leader.md (PR 3c.3 scope: T10, T11, T12)
**Branch:** julie-rescue-phase3c3
**PR:** https://github.com/anortham/julie/pull/33
**Duration:** single autonomous session
**Phases:** 3/3 sub-PR-3c.3 tasks complete (T10, T11, T12)
**Tasks:** 3/3 complete (+ codex F1 fix)

## What shipped
- **T10 — THE CUTOVER.** No-args `julie-server` now serves `JulieServerHandler` **in-process** over rmcp stdio (no daemon fork, no adapter bridge), guarded by the per-workspace OS leader lock. Winner = sole watcher + Tantivy writer; losers = WAL/mmap readers. Closes codex F-A (split-index risk) by **pinning workspace identity to the canonical startup hint**: `run_in_process_server` canonicalizes the hint before deriving `workspace_id`, and `request_prefers_client_roots()` suppresses the daemon-era `list_roots` rebind in-process so lock-id == index-root-id == binding-id. Defense-in-depth: `run_primary_workspace_repair` early-returns for in-process followers so no follower writes through any entry path. (`src/main.rs`, `src/server_in_process.rs`, `src/handler.rs`, `src/startup.rs`; tests `src/tests/core/handler/fa_pin_hint.rs`)
- **T11 — kill-the-writer HARD GATE** (two-process). Subprocess acquires `{ws}/leader.lock`, commits a symbol to SQLite via `bulk_store_fresh_atomic` **without** Tantivy projection (simulates the crash gap), then blocks forever holding the guard. Parent asserts `AlreadyHeld` while alive, kills it, polls re-acquire, runs `ensure_current_from_database`, and asserts canonical revision == 1, status ready, ≥1 doc, symbol searchable. (`src/tests/integration/t11_kill_writer.rs`)
- **T12 — in-process boundary tripwire.** Compile-time guard that bypassed entry points still build (`run_adapter`, `start_daemon`); source-scan that `main.rs`'s `None =>` arm serves in-process (no `run_adapter`/`DaemonLauncher`); assertion that all 13 §7 DAG files still **exist** (bypassed-not-deleted — deletion is Phase 3d). (`src/tests/integration/in_process_boundary.rs`)
- **A1.8 E2E retargeted** to the in-process contract: `test_e2e_julie_server_no_args_serves_in_process` asserts no `discovery.json` is published (no daemon fork), replacing the pre-cutover daemon-spawn assertion. (`src/tests/integration/wiring_a1_8.rs`)
- **Codex F1 fix:** gated recreated-open projection repair on leadership — `may_repair_recreated_projection()` = `!is_in_process_follower()`, applied at **both** repair sites in `handler.rs` so a read-only follower can never rebuild Tantivy on a read path. (`src/tests/core/handler/follower_repair_gate.rs`)

## Judgment calls (non-blocking decisions made)
- `docs/plans/2026-06-05-julie-phase3c-inprocess-leader.md` — Classified codex F2 (no live follower promotion) as **flagged-for-human**, not dismissed. It is a real architectural limitation, but the approved T11 acceptance explicitly accepts eventual-consistency recovery via fresh-process lock reacquire; live promotion is a Phase 3d priority/architecture call. Surfaced transparently rather than silently dismissed.
- `(verification ledger)` — **Reused** the branch-gate evidence at HEAD `77ad1299` (dev+system+reliability ALL_GREEN) rather than re-running the ~28-min gate, because the recorded SHA matches HEAD exactly and the F1 fix is the HEAD commit (gate ran post-fix). Per the verification-ledger reuse contract.
- `src/tests/integration/wiring_a1_8.rs` — **Retargeted** the no-args E2E test (rename + invert assertion) instead of deleting it. The old test encoded the pre-cutover daemon-spawn contract; the cutover makes the new contract "no daemon fork", so the test stays as the contract guard.

## External review (codex, adversarial)

- **Findings:** 2 (verdict: needs-attention)
- **Verified real, fixed:** 1 (commits: 77ad1299)
  - **F1 (high, 0.88) — "Read-only followers can still rebuild Tantivy on a read path"** (`src/handler.rs`). Real: the recreated-open repair (`repair_recreated_open_if_needed`, which does `clear_all` + `apply_documents` = Tantivy writes) ran unconditionally on read paths, so an in-process follower could write the index. Fixed by gating both repair sites on `may_repair_recreated_projection()` (`!is_in_process_follower()`); a follower now logs and skips. Covered by `follower_repair_gate.rs` (truth table + follower-no-op + leader-repairs).
- **Dismissed:** 0
- **Flagged for your review:** 1
  - **F2 (high, 0.84) — "Leader crash does not promote surviving followers"** (`src/server_in_process.rs`) — why flagged: this is **by design for 3c.3**. The approved T11 acceptance gate is "kill the leader → freshness degrades only (~500ms eventual consistency, never an error) → a fresh process wins the lock and reconciles Tantivy to canonical" — the 3-host-race analog from Phase 3b. Promoting an **already-running** follower to writer is a dynamic-leadership feature explicitly deferred to **Phase 3d**, not a 3c.3 regression. Needs a human call on whether that's acceptable for merge (it matches the plan, but the reviewer is right that production recovery relies on a fresh process, not live promotion).

## Tests
- **Branch-gate ALL_GREEN @ `77ad1299`** (post-F1-fix HEAD): `cargo xtask test dev` exit=0 (37 buckets), `cargo xtask test system` exit=0 (8 buckets — incl. integration 95.7s, lifecycle 72.7s), `cargo xtask test reliability` exit=0 (3 buckets — daemon 61.8s, workspace-init, integration 90.9s). Daemon/adapter test trees still present and green (in-process path did not regress `src/tests/daemon/**` or `src/tests/adapter/**`).

## Blockers hit
- **None** for the automated gate. **One REQUIRED MANUAL step before relying on the cutover in production** (it is part of the plan's T11 acceptance and can only be done by the user, since it requires rebuilding release + restarting the live MCP client): rebuild `--release`, restart the live MCP client, confirm via `ps` / absence of `~/.julie/.../discovery.json` that **no daemon forked**, and that a live `fast_search` + `edit_file` + re-search round-trip works in-process. This session cannot perform it without disrupting itself.

## Files changed
```
 .memories/2026-06-05/102437_df05.md                |  41 +++
 .memories/2026-06-05/105114_2f46.md                |  46 +++
 docs/plans/2026-06-05-julie-phase3c-inprocess-leader.md |   2 +
 src/handler.rs                                     |  69 ++++-
 src/main.rs                                        |  39 +--
 src/server_in_process.rs                           |  17 +-
 src/startup.rs                                     |  10 +
 src/tests/core/handler.rs                          |   2 +
 src/tests/core/handler/fa_pin_hint.rs              | 146 +++++++++
 src/tests/core/handler/follower_repair_gate.rs     | 239 +++++++++++++++
 src/tests/integration/in_process_boundary.rs       | 148 +++++++++
 src/tests/integration/t11_kill_writer.rs           | 331 +++++++++++++++++++++
 src/tests/integration/wiring_a1_8.rs               |  44 ++-
 src/tests/mod.rs                                   |   2 +
 14 files changed, 1095 insertions(+), 41 deletions(-)
```

## Next steps
- Review PR: https://github.com/anortham/julie/pull/33
- **Manual dogfood smoke (user-only, REQUIRED):** rebuild release + restart live MCP, confirm no daemon fork (`ps` / no `discovery.json`) and a live `fast_search` + `edit_file` + re-search round-trip works in-process.
- **Decide on codex F2** (no live follower promotion, `server_in_process.rs`): confirm the by-design eventual-consistency recovery is acceptable for merge, or pull live-promotion forward from Phase 3d.
- After merge: proceed to **Phase 3d** (delete the bypassed daemon/adapter DAG that T12 currently asserts still exists; live follower promotion if F2 is pulled forward).
