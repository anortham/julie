# Autonomous Execution Report - Machine Service Phase 2 (one writer, disposable indexes)

**Status:** Awaiting publication approval
**Plan:** docs/plans/2026-09-09-machine-service-phase2-plan.md
**Branch:** machine-service (worktree `.worktrees/machine-service`; phase 1 commits ride on the same branch)
**PR:** pending — filled in after PR creation
**Publication authority:** local commit=authorized (user asked for the phase 1 review and the phase 2 plan on the machine-service branch, then approved the plan); push=missing (no user instruction to push); PR=missing (no user instruction to open a PR)
**Duration:** about 6 h across two context windows (2026-09-09 23:00 UTC to 2026-09-10 03:15 UTC for phase 2)
**Phases:** 2/6 of the machine-service design complete (phase 1 service, phase 2 one writer)
**Tasks:** 10/10 complete
**External-model policy:** no external dispatch in this run (reviewer choice none)

## What shipped
- Task 1: every runtime is the writer; follower branches, promotion probe, and follower refusals deleted (7caa96e7).
- Task 2: leader lock, owner epochs, writer permits, publication lock, host slots deleted as pure type removal (386f49d6, 3482ccc0).
- Task 3: primary swap, session attachment, deferred auto-index, and MCP roots deleted; one handler per `(root, index_root)` (20160dfe, ebdc4050); `src/handler.rs` 2991 -> 1890 lines.
- Task 4: continuation store and `spillover_get` deleted; 12 tools; over-limit output ends with a truncation line (7209380a, 50cd2ae7).
- Task 5: `symbols.db` migrations deleted; schema or engine drift deletes `indexes/<id>/` and reindexes (6f069f49).
- Task 6: Python embedding host deleted (-14,565 lines); tests default to `JULIE_EMBEDDING_PROVIDER=none`; dev tier 195 s -> 55 s; mock brokers killed on drop (b2a4dfb2, 967394eb).
- Task 7: sibling seed copy on `manage_workspace open` (779a01ed); Julie worktree seeded in 14.0 s vs 35.9 s from scratch.
- Task 8: `manage_workspace rebuild` and `status` (`CheckoutStatus`, `/status.checkouts`); `register`, `clean`, `stats` retired (98c72a9d).
- Task 9: durable-roots test, `scripts/complexity-words.sh`, `.symbols.db.init.lock` and `fs2` locking deleted (7944312f).
- Task 10: docs rewritten to the one-writer model; gate finding `docs/findings/2026-09-10-machine-service-phase2-gate.md`; ledger `docs/plans/2026-09-09-machine-service-phase2-ledger.md`. Rust lines (plan tokei command) 159,488 -> 145,367.

## Judgment calls (non-blocking decisions made)
- `docs/plans/2026-09-09-machine-service-phase2-plan.md` (Sequencing decision) — `facts.sqlite` and read tools moved to phase 3; native broker client and embedding generations to phase 4, because phase 2 already deleted 14k lines and the broker client is the seam phase 4 replaces.
- `fixtures/databases/julie-snapshot/` — Made the search-quality snapshot git-ignored and built on demand by `ensure_julie_fixture` (4f82d55a), because the committed schema-9 snapshot cannot open without migrations and a rebuilt one is about 300 MB, over GitHub's 100 MB file limit. Reversible with Git LFS or a smaller snapshot.
- `src/tests/core/workspace_init/handler_binding.rs`, `crates/julie-core/src/tests/database_init_race.rs`, `src/tests/integration/projection_repair.rs` — Deleted tests whose state cannot exist after the deletions (root swap, twelve concurrent initializers of one path, legacy schema upgrade), following the Task 2 precedent.
- `src/tests/tools/refactoring/*` — Eight rename tests now call the handler's `execute_rename_symbol` (6b19c37e) because the tools-crate entry only prepares; they already failed on main 8b07074b. A dry run with zero matches reports the no-references message (product change, small).
- `src/startup_repair_plan.rs` — No missing-embeddings catch-up when no provider can exist, unless one is injected (e508994d, 7576ba08).
- `src/tests/tools/search/fast_search_regression_tests.rs` — Content enrichment fixtures use string literals because julie-extractors v2.41 extracts local `let` bindings as symbols.
- `.julieignore` — `fixtures/search-quality/` excluded from indexing so the OR-fallback sentinel token is not found in the snapshot.
- Task 9 — 15 complexity-word hits accepted (4 moved lines, 11 pre-existing names); the bucket stays on-demand.

External review: none (not requested for this run).

## Review campaign
- **State:** not run
- **Evidence:** lead-only
- **Round:** 0/0
- **External invocations:** 0
- **Open critical/high:** 0
- **Open medium/low:** 0
- **Open at/above floor:** 0

## Tests
- `cargo xtask test dev` 29 buckets pass (53.7 s) at bc650bc2; `system` 5 buckets, `bucket service-process`, and `full` 51 buckets (471 s) pass at 46bb53da (bc650bc2 is docs-only on top).
- `cargo audit`: 4 vulnerabilities and 8 warnings, identical to main's lockfile; none introduced here. Secrets grep over the diff: clean.

## Blockers hit
- None. Push and PR are waiting on authority (see Status).

## Files changed
- `git diff --stat 8b07074b..HEAD`: 434 files changed, 8,977 insertions, 69,052 deletions (40 commits since main; 28 in phase 2).

## Source control
- **Outstanding:** None — all commits ride on machine-service. The SDD workspace for this plan was deleted; the phase 1 SDD workspace (`.razorback/sdd/2026-09-09-machine-service-phase1-plan-1fd9dc7b83ba`, git-ignored) remains.
- **Worktrees left in place:** `/home/murphy/source/julie/.worktrees/machine-service` — kept, branch not yet published. `/home/murphy/source/julie` (main, ahead of origin by 5) is the user's; it holds one untracked file `.memories/2026-09-09/222842_f3b4.md` that predates this run and was not touched.

## Next steps
- Review PR: pending — filled in after PR creation
- Decide whether the search-quality snapshot should return to git via LFS or stay build-on-demand.
- Phase 3 plan: `facts.sqlite` blob-keyed schema, derived indexes, read tools; fix `BindingResolver::is_unknown_selector` in the engine (Important deferral).
- Run `cargo xtask sync-plugin` before the next release; investigate the `fast-refs` duplicate `pub use` line and the non-identifier call names from julie-extractors.
