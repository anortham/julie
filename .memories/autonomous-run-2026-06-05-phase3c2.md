# Autonomous Run Report — Phase 3c.2 (in-process leader wiring)

**Status:** ✅ Complete — PR opened, awaiting human merge
**PR:** https://github.com/anortham/julie/pull/31
**Branch:** `julie-rescue-phase3c2` → `main` (base `92d37b2b`, HEAD `dab682f2`)
**Plan:** `docs/plans/2026-06-05-julie-phase3c-inprocess-leader.md` (PR 3c.2 scope: T5–T9)
**Reviewer:** codex (pre-merge, escalation tier)

---

## What shipped

PR 3c.2 of the Phase 3c daemon-teardown cutover — the **in-process leader wiring**. Still additive: `main.rs`'s no-args arm continues to use `run_adapter`; the in-process path is built and tested in isolation. The cutover (T10) lands in 3c.3.

- **T5** — watcher + 8 canonical writers gated behind leadership (`!is_in_process_follower()`).
- **T6** — in-process embeddings via the shared 3b resident host, `ensure_ready()` hard-gate, graceful keyword-only degradation.
- **T7** — loser (follower) processes refuse write-tools + drop per-call metrics writes; reads stay valid.
- **T8** — `run_in_process_server(startup_hint)` serve entry with F2 storage/lock inode coupling.
- **T9** — leader-gated handoff recovery + F1 bounded in-process read envelope (non-cancellable background repair).

Commits (base..HEAD):
```
dab682f2 docs(plan): carry codex 3c.2 F-A (election timing) into T10 cutover
b5181170 fix(rescue-3c): close F2 force-path hole + single-flight deferred repair
04091e76 test(rescue-3c): harden embedding-host opt-in timeouts against tier load
b74b3234 test(rescue-3c): repair stale src/tools paths in legacy-language guard
627134ed fix(rescue-3c): gate watcher on !is_in_process_follower, not is_leader
39e7e0e5 feat(rescue-3c): leader-gated handoff recovery + F1 bounded in-process read envelope (T9)
ae080d4c feat(rescue-3c): run_in_process_server serve entry with F2 storage/lock inode coupling (T8)
730b5da2 feat(rescue-3c): refuse write-tools + drop per-call metrics write on loser processes (T7)
32e4c921 feat(rescue-3c): in-process embeddings via resident host with ensure_ready gate (T6)
7f05633b feat(rescue-3c): gate file watcher start behind leadership (T5)
```

Tasks complete: 5/5 (T5–T9). Phases: PR 3c.2 of 3 (3c.1 merged as #30; 3c.3 next).

---

## Judgment calls

1. **T5 watcher-gate regression (caught by branch-gate, fixed `627134ed`).** T5 gated the watcher on `is_leader()`, but `restore()`/`initialize_workspace_with_force` are shared with stdio/daemon handlers (`LeadershipState::none()`, `is_leader()==false`) — silently disabling their watcher. The `system`-tier `workspace_init` test caught it. Fixed: gate on `!is_in_process_follower()`. **Lesson:** my task-level gate (narrow test + `cargo check`) was too tight for a change landing on a SHARED path; escalate the gate scope when shared code is touched.

2. **Two pre-existing failures surfaced by the `system` tier (not 3c.2 regressions).** 3c.2 is the first batch to run `system`+`reliability` (3c.1 was dev-only):
   - Stale-path test (`b74b3234`): `test_live_workspace_surface_has_no_legacy_workspace_language` hardcoded `src/tools/*` paths moved to `crates/julie-tools/src/*` in the Phase 2 crate split. Repaired the paths; assertion intent preserved.
   - Embedding-host opt-in flake (`04091e76`): `host_path_taken_when_env_set` flaked once under the 34-bucket dev-tier CPU saturation (5s `wait_until_settled` ceiling too tight; passed 3/3 in isolation). Raised 5s→30s. Not a product regression — the branch never touched `src/daemon/app.rs`.

3. **Codex F-B + F-C fixed in 3c.2; F-A carried to 3c.3.** All three findings are in the in-process path (dormant in 3c.2 production). F-B is a hole in the F2 invariant 3c.2/T8 explicitly owns → fixed. F-C is a clean, self-contained T9 hardening → fixed. F-A is genuinely cutover-election-timing design (validated live by T11) → carried, recorded as a T10 hard requirement. Did not pause to ask the user: the pre-merge-review flow authorizes the lead to classify + fix verified findings autonomously.

4. **Lead-inline fixes (not delegated).** The two fixes were surgical, same-file (`handler.rs`), and to T8/T9 code already analyzed exhaustively — the pre-merge-review skill sanctions inline lead fixes, and same-file work gives parallel workers nothing. Chose inline for lowest latency.

---

## External review (codex) — full

Verdict: **needs-attention**, 3 findings. All verified against the code before classification.

| Finding | Sev / conf | Class | Disposition |
|---|---|---|---|
| F-B — force reindex bypasses `in_process_index_root` | high 0.94 | real-bug | **Fixed** `b5181170`. Force routes through `initialize_with_index_root`, clears only `db`/`tantivy`, preserves held `leader.lock`. Test verified RED without fix. |
| F-C — per-read repair re-spawn on persistent failure | medium 0.78 | real-improvement | **Fixed** `b5181170`. `compare_exchange` in-flight claim → one outstanding repair task per cycle. |
| F-A — leader election keyed before root reconciliation | high 0.89 | real, out-of-scope | **Flagged → 3c.3.** Cutover-election-timing; T10 must elect after canonical root is known; T11 validates. codex "corruption" wording overstated (different hints → different storage AND lock; real risk = split indexes + key mismatch). |

Findings fixed: 2 (`b5181170`). Dismissed: 0. Flagged: 1 (F-A). codex does not report per-request token counts.

---

## Tests

Branch-gate **GREEN** at HEAD `b5181170`:
- `cargo xtask test dev` — 37 buckets passed (1218.6s)
- `cargo xtask test system` — 7 buckets passed (212.5s)
- `cargo xtask test reliability` — 3 buckets passed (158.5s)
- 0 FAIL / error / timeout across all three tier logs.

Targeted: handler subtree 87/87; new tests `test_inprocess_force_reindex_keeps_f2_storage_and_preserves_lock` + `test_deferred_repair_slot_is_single_flight` GREEN; F-B test confirmed RED with the fix reverted.

HEAD is now `dab682f2` (docs-only F-A carry-forward on top of the gate-tested `b5181170` — no code change, gate evidence still valid).

---

## Blockers hit

None.

---

## Files changed (base..HEAD)

30 files, +2440 / −59. Key code: `src/handler.rs` (+245), `src/server_in_process.rs` (new, +273), `src/leadership.rs` (+54), `crates/julie-runtime/src/workspace/mod.rs` (+51), `src/tools/workspace/commands/*` (loser write-refusal), `src/handler/tools/*` (write-exempt). Tests across `src/tests/core/handler/`, `src/tests/daemon/`, `src/tests/integration/`. Plus 5 `.memories/` checkpoints (version-controlled).

---

## Next steps

1. **Human merge** PR #31 (never auto-merged).
2. **PR 3c.3 — the cutover:** T10 (flip `main.rs` None arm, **closing F-A** — election after canonical root is known), T11 (kill-the-writer HARD GATE, proves single-writer-per-canonical-workspace), T12 (boundary tripwire). Small diff, high consequence, revertible in one commit.
