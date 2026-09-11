# Autonomous Execution Report - Test speed (owner sequence item 3)

**Status:** Awaiting publication approval
**Plan:** docs/plans/2026-09-11-test-speed-plan.md
**Branch:** test-speed (worktree `.worktrees/test-speed`, 10 commits over main `ab6b8077`)
**PR:** pending — filled in after PR creation
**Publication authority:** local commit=authorized (approved plan, 2026-09-11); push=missing (brief: "Push to origin is still the owner's call"); PR=missing (no instruction)
**Duration:** about 1 h 35 min (baseline 18:20Z, gate 19:54Z)
**Phases:** 1/1 complete
**Tasks:** 6/6 complete, plus one fix found during the gate
**External-model policy:** no external model received the diff (reviewer: none)

## What shipped
- Three test tiers, each one `cargo nextest run --workspace` call: `dev` 12.2 s median (was 50.5 s + prebuild), `dogfood` 14.6 s (was 124 s), `full` 26.4 s with dogfood included (was 94 s without). Bucket manifest, `changed`, `inventory`, budgets, prebuild accounting deleted: xtask 11,470 to 2,233 lines.
- Every test runs: 2275 listed = 2206 dev + 69 dogfood. 319 tests ran in no bucket before.
- Dogfood store open 8.4 s to 1.4 s per test: re-export import index built once per graph load, plus `opt-level = 1` for `julie-index` in dev builds. The live service benefits too.
- Three 10 s fixed waits removed: `connect_or_start_within(deadline)`, readiness poll in the race test.
- The uncovered scenario test fixed (`--standalone`), the fixture tests moved to the dogfood set.
- Process-fixture service leak fixed: each parity or scenario test left a detached service for 30 minutes (129 processes, 8.2 GB after seven runs). Fixture sets `JULIE_SERVICE_IDLE_SECS=2`.
- CLAUDE.md, AGENTS.md, README, TESTING_GUIDE, five other docs, two hooks, one skill, and the ledger template name only the new commands; xtask docs-contract test enforces it.
- Finding: `docs/findings/2026-09-11-test-speed.md`. Ledger: `docs/plans/2026-09-11-test-speed-ledger.md`.

## Judgment calls (non-blocking decisions made)
- `src/tests/tools/search/race_condition.rs:183` — The test now calls `manage_workspace index` before polling `search_ready` because `initialize_workspace_with_force` does not index; the old test searched an empty index after a dead 10 s wait.
- `src/tests/request_scenarios.rs:250` — Added `--standalone` (plan outcome a) because the CLI without it opened the store the fixture's MCP child was still creating; the flags were correct.
- `crates/julie-index/src/tests/graph/reexports.rs` — Tests go through `resolve` because `mod reexports` is private; the plan asked for a test through `resolve_reexport`.
- `xtask/src/runner.rs` — Folded execution and rendering modules into one file; children inherit stdio instead of capturing.
- `src/tests/request_process_helpers.rs:269,482` — Chose the idle env var over a `service stop` in `Drop` because `Drop` is synchronous and the env var is two lines.
- `docs/TREE_SITTER_QUALITY_BAR.md:200-245` — Historical ledger rows keep the retired command names; only the commands table changed.
- `.config/nextest.toml` — No `max-threads = 1` group for `service::process`; the tests passed at 24 threads in every run. Add one if they flake.

External review: none (not requested for this run).

## Review campaign
- **State:** not run
- **Evidence:** lead-only
- **Round:** 0/0
- **External invocations:** 0/0
- **Open critical/high:** 0
- **Open medium/low:** 0
- **Open at/above floor:** 0

## Tests
- Branch gate at `dd0ebe7a`: `cargo fmt --check` clean; `cargo clippy --workspace --all-targets` 0 errors; `cargo xtask test full` 5/5 commands in 26.4 s (2206 + 13 ignored cli + 69 dogfood tests, 0 failing); 0 leaked services after 4 s. Three timed `dev` runs 19.3, 12.2, 11.9 s; three `dogfood` runs 14.5, 14.6, 14.6 s. Security scope: none declared. The last commit `db3a1a60` is docs and `.memories` only, so the evidence holds.

## Blockers hit
- None. Push and PR authority are missing, so the run stops here for the owner's decision (merge locally as with items 1 and 2, or push / open a PR).

## Files changed
- `git diff --stat main..HEAD`: 75 files, +1,489 / -9,955. Product: `src/service/client.rs` (+28), `crates/julie-index/src/graph/reexports.rs` (90), `resolve.rs` (15), `Cargo.toml` (+3), `.config/nextest.toml` (+6). Runner: `xtask/**` (about -9,240). Tests: `src/tests/service/client.rs`, `race_condition.rs`, `request_scenarios.rs`, `request_process_helpers.rs`, `crates/julie-index/src/tests/graph/reexports.rs` (+135). Docs and instructions: 15 files. Memories: 8 checkpoints.

## Source control
- **Outstanding:** None — all commits ride on `test-speed`. The main checkout still has the owner's uncommitted `.codex/config.toml` change and the Codex checkpoint `.memories/2026-09-11/133457_b2d2.md`; untouched.
- **Worktrees left in place:** `.worktrees/test-speed` on `test-speed` (this run's; clean except the untracked `fixtures/databases/julie-snapshot` symlink to the main checkout's fixture, which worktree removal deletes).

## Next steps
- Owner: merge `test-speed` into main locally, or say "push" / "open a PR".
- After the merge: rebuild main's release binary, copy the sidecar, restart the service, update the brief, start item 4 (deployment story).
- Deferred (in the finding): `workspace_isolation_smoke` 5 to 14 s floor, `store_open` 5 s lock waits, dogfood thread cap trade, Alamofire graph load 3 s in release, CLI calls without `--standalone` in other parity tests.
