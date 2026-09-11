# Hybrid Auto Backend Verification Ledger

Plan: `docs/plans/2026-09-11-hybrid-auto-backend-plan.md`. Reuse a row only when its scope label and commit SHA match the current HEAD exactly.

Worker rows come from the Opus implementer and fix agents (workflow `wf_b0226104-a63`). Lead rows ran in the clean `hybrid-auto` worktree; no separate gate checkout was needed because the tree was clean at every gate.

Environment note: the `search-quality` bucket copies the 300 MB fixture into a temp dir per test. With 64 tests in parallel on the 32 GB tmpfs the run failed with `Disk quota exceeded`, so the lead ran `dogfood` with `TMPDIR` on the home disk and `full` on tmpfs with `NEXTEST_TEST_THREADS=6`. A `TMPDIR` under `$HOME` breaks the root-detection tests in `workspace-init`, so tmpfs plus a thread cap is the working combination. Tracked under the test-speed item in the brief.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Seven Task 1 tests red | file did not compile before `auto_prefers_hybrid` existed (E0599) | worker-exact | 9c1a70cf | red as expected | 2026-09-11T12:40Z | no |
| Seven Task 1 tests green | `cargo nextest run --lib <name>` ×7 | worker-exact | 28a616cb | pass (7/7) | 2026-09-11T12:47Z | no |
| Repaired pre-existing test | `cargo nextest run --lib lexical_zero_hits_skip_semantic_fallback_for_plain_language_noise` | worker-exact | 28a616cb | pass after query became a single word | 2026-09-11T12:47Z | no |
| Auto path never waits on provider init | `cargo nextest run --lib auto_nl_query_never_waits_for_embedding_provider_init` | worker-exact | cc81ce87 | red (1 call, expected 0) then pass | 2026-09-11T12:58Z | no |
| Instructions budget | `wc -c JULIE_AGENT_INSTRUCTIONS.md` | worker-exact | 28a616cb | 1881 characters (≤ 1,900) | 2026-09-11T12:47Z | no |
| Routing block budget | `wc -c .claude/hooks/julie-routing-block.md` | worker-exact | 28a616cb | 3937 bytes (≤ 4,000) | 2026-09-11T12:47Z | no |
| Hybrid bucket after review follow-ups | `cargo xtask test bucket tools-search-hybrid` | lead-bucket | 8dbd61d4 (uncommitted at run, identical tree) | pass (2 commands, 0.9s warm) | 2026-09-11T13:00Z | no |
| Lead fmt | `cargo fmt --check` | lead-fmt | 8dbd61d4 | pass | 2026-09-11T13:00Z | no |
| Lead dev | `cargo xtask test dev` | lead-dev | 8dbd61d4 | pass (30 buckets, 140.1s warm, cold 186.5s) | 2026-09-11T13:03Z | no |
| Lead dogfood on tmpfs | `cargo xtask test dogfood` | lead-dogfood | 8dbd61d4 | FAIL: `Disk quota exceeded` copying the fixture (environment, not product) | 2026-09-11T13:05Z | no |
| Lead dogfood on home disk | `TMPDIR=/home/murphy/tmp cargo xtask test dogfood` | lead-dogfood | 8dbd61d4 | pass (2 buckets, 46.7s warm) | 2026-09-11T13:06Z | no |
| Lead full prebuild | `cargo xtask test full` | lead-full | 8dbd61d4 | FAIL: `xtask-eval` did not compile (`resolve` caller missed by Task 1) | 2026-09-11T13:07Z | no |
| Workspace type-check | `cargo check --workspace --all-targets` | lead-check | 1e069ee9 | pass | 2026-09-11T13:08Z | no |
| Lead full on home disk | `TMPDIR=/home/murphy/tmp cargo xtask test full` | lead-full | 1e069ee9 | FAIL: three `workspace-init` root-detection tests reject a temp dir under `$HOME` (environment) | 2026-09-11T13:10Z | no |
| Lead full on tmpfs, six threads | `NEXTEST_TEST_THREADS=6 cargo xtask test full` | lead-full | 1e069ee9 | pass (47 buckets, 91.6s warm, cold 94.9s) | 2026-09-11T13:12Z | no |
| Lead clippy | `cargo clippy --workspace --all-targets` | lead-clippy | 1e069ee9 | pass (0 errors; pre-existing warnings only) | 2026-09-11T13:14Z | no |
| Scorecard service smoke | `run_scorecard.py --service --case express-lazy-router --backend lexical` | lead-eval | 74eb5b81 (harness uncommitted at run) | pass (rank 1, 102 ms) | 2026-09-11T13:21Z | no |
| Corpus vectors complete | `sqlite3 … count(*) from vectors` vs registry `vector_count`, ten repos | lead-eval | 74eb5b81 | pass (10/10 at target) | 2026-09-11T13:24Z | no |
| Scorecard hybrid vs lexical | `run_scorecard.py --service --backend lexical --backend hybrid --backend semantic` | lead-eval | 74eb5b81 | pass (MRR 0.677 vs 0.351, +93% relative; semantic 0.848) | 2026-09-11T13:25Z | no |
| Head-to-head auto vs 6a hybrid | `run_matrix.py --skip-miller --require-semantics --julie-backends auto,hybrid` | lead-eval | 74eb5b81 | pass (auto 77%/90% top-5 = hybrid; inspect 23/23; 23/23 auto rows ran hybrid; p50 38 ms) | 2026-09-11T13:26Z | no |
| Resident memory | `julie-server service status` | lead-status | 74eb5b81 | report-only: RSS 5,269 MB, 13 live + 37 dead checkouts, sidecar 204 MB | 2026-09-11T13:27Z | no |
| Semantic auto: worker tests | `cargo nextest run --lib <name>` ×7 | worker-exact | 78553133 | pass (red was E0599 on the renamed helper) | 2026-09-11T13:50Z | no |
| Semantic auto: hybrid bucket | `cargo xtask test bucket tools-search-hybrid` | lead-bucket | dcfe855f | pass (0.9s warm) | 2026-09-11T13:56Z | no |
| Semantic auto: dev | `cargo xtask test dev` | lead-dev | dcfe855f | pass (30 buckets, 54.2s warm) | 2026-09-11T13:58Z | no |
| Semantic auto: full | `NEXTEST_TEST_THREADS=6 cargo xtask test full` | lead-full | dcfe855f | pass (47 buckets, 91.7s warm) | 2026-09-11T14:02Z | no |
| Semantic auto: fmt, clippy | `cargo fmt --check`; `cargo clippy --workspace --all-targets` | lead-clippy | dcfe855f | pass (0 errors) | 2026-09-11T14:05Z | no |
| Semantic auto: scorecard, first attempt | `run_scorecard.py --service …` | lead-eval | dcfe855f | DISCARDED: service was rebuilding every corpus index after restart (see finding) | 2026-09-11T14:13Z | no |
| Semantic auto: scorecard | `run_scorecard.py --service --backend lexical --backend hybrid --backend semantic` | lead-eval | dcfe855f | pass (semantic MRR 0.848, hybrid 0.677, lexical 0.351) | 2026-09-11T14:18Z | no |
| Semantic auto: head-to-head | `run_matrix.py --skip-miller --require-semantics --julie-backends auto,hybrid,semantic` | lead-eval | dcfe855f | pass (auto 18/23 top-1, 20/23 top-5, p50 15 ms, 23/23 rows semantic; inspect 23/23) | 2026-09-11T14:18Z | no |
