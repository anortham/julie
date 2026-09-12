# Plan 1 execution report

Status: implemented and verified locally. Publication was not requested or authorized.

Plan: [Index freshness and request isolation](../docs/plans/2026-09-11-revival-correctness-plan.md).
Worktree: `/home/murphy/source/julie/.worktrees/revival-correctness`.
Branch: `fix/revival-correctness`; base: local `main`, `eabf93cbc0359fdee4644442d48c87a8fe107958`.
Verified implementation commit: `9faf6fb4a8a3b180b6a17304586d86e8c789b089`.
All three implementation tasks are complete. Work began September 11; verification finished September 12 after a session pause.

## Changes

- Watcher cycles consume overflow/rescan obligations under the existing mutation gate. Distinct rapid writes are retained. Read and metadata failures preserve prior data, failed recovery is paced, and reconciliation repairs facts/graph/Tantivy publication gaps.
- Semantic mode belongs to each request; Off cannot disable a concurrent Required request. HTTP and MCP share validated mode parsing, including nullable defaults and explicit invalid-value errors.
- Pipeline eligibility and coverage use current canonical blob/ordinal keys, metadata filters and variable budgets. Shared blobs count once, current source paths remain eligible even when identical test paths exist, and container enrichment remains intact.
- Partial embeddings resume through the existing task map after restart or late provider readiness. Valid vectors survive; duplicate scheduling preserves the existing job. Required, Auto and health report compatible eligible coverage consistently.
- MCP metadata preserves request readiness without overwriting the tool's structured payload. A reusable native-service probe lives at `src/tests/helpers/revival_live_probe.py`.

No additional dependency, schema, broker, recovery journal or background subsystem was introduced. Plans 2–5 remain proposals.

## Verification

At the implementation commit above:

| Check | Result |
|---|---|
| `cargo fmt --check` | Pass |
| `cargo clippy --workspace --all-targets` | Pass, with warnings; no warning-free claim |
| `cargo xtask test full` | Pass: 2,245 development tests, 13 CLI tests, 69 dogfood tests |
| Isolated native service probe | Pass: 50/51 to 51/51 embeddings, all 50 stored vectors unchanged; Off/Required overlap, rapid updates, checkout isolation, stdio MCP readiness |
| Owned process cleanup | No probe processes remained; temporary service home removed |

[Live JSON evidence](../docs/findings/2026-09-12-revival-correctness-live.json) and the plan's verification ledger record the exact source commit. To satisfy the exact-HEAD rule, the report commit receives a fresh formatting, clippy, full and live gate before the final session handoff. These historical rows are not reused as evidence for a different commit.

Commands use `CARGO_TARGET_DIR=/home/murphy/source/julie/target` and `JULIE_TEST_BIN=/home/murphy/source/julie/target/debug/julie-server`. Fixed-path CLI tests additionally use a worktree-local `target/debug/julie-server` symlink to that same debug binary. No release build or maintainer service restart is needed.

Run the live check with:

```sh
python3 src/tests/helpers/revival_live_probe.py --binary /home/murphy/source/julie/target/debug/julie-server
```

The probe uses a valid fake native sidecar: one successful batch followed by an intentional batch error. It proves process/protocol/restart behavior on this Linux host, not real-model quality, model downloads, Windows/macOS installation or release packaging. Those belong to Plan 4. No blanket security/dependency audit was declared or run.

## Review and corrections

Astra reviewed Sol's changes inline. No external CLI reviewer was selected. Review caught unreadable-path deletion risks, partial-publication recovery, duplicate scheduling, canonical-key/path eligibility, lost container enrichment, and health parity.

The full gate required two fixture corrections: raw SQL vector writes now publish into the existing snapshot, and overflow filler events no longer manufacture separate queued-read failures. The live probe then found the HTTP semantic-mode bug; its regression failed before the adapter fix and passes afterward.

Some initial coverage production edits preceded their negative tests. The worker explicitly restored the original raw-count/nonzero behavior for negative evidence; the plan records this deviation. Existing invariant tests and zero-selected runs are not represented as original test-first repairs.

## Source control and authority

Local commits are authorized by the implementation request and roadmap contract. Push, PR publication, tagging, release and live installation have no authorization and were not performed. Integration into main remains a separate action.

- Task changes, the earlier assessment/plans, and task memory are together on this branch. No task work was stashed or stranded elsewhere.
- Original Julie checkout remains `main` at `eabf93cb`, with the user's modified `.codex/config.toml` and untracked `.memories/2026-09-11/133457_b2d2.md` preserved.
- The already-landed `deployment-story` worktree remains at `cda68bed` with its pre-existing untracked `fixtures/databases/`.
- `julie-plugin` remains at `db4dd894`, ahead 7/behind 1, with its pre-existing untracked v8 Linux archive.
- Implementation snapshot versus base: 54 changed files, 3,976 insertions and 394 deletions, including assessment/plans, tests, the live harness and memory.

No implementation blocker remains. Miller replacement, production installation and publishing are not implied by completing Plan 1.
