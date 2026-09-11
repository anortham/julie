# Machine Service Phase 6a Verification Ledger

HEAD at last recorded product row: `3d218f4c` (prior to this docs commit). Reuse only when SHA matches current HEAD exactly.

Branch gates were run in `.worktrees/contract-gate` with shared `CARGO_TARGET_DIR`. Most rows are detached `c2486db3` at 2026-09-11T07:30Z–08:00Z. `dev` and `full` were rerun at `3d218f4c` about 08:10Z.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Task 3 compact shape: workspace lib | `cargo nextest run --workspace --lib` (lead: workspace lib 2177/2177) | lead-task | e46aba79 | pass (2177/2177) | 2026-09-11T03:07:09Z | no |
| Task 3 dogfood | `cargo xtask test dogfood` | lead-task | e46aba79 | pass | 2026-09-11T03:07:09Z | no |
| Task 3 full | `cargo xtask test full` | lead-task | e46aba79 | pass (47/47) | 2026-09-11T03:07:09Z | no |
| Task 4 fmt | `cargo fmt --check` | lead-task | 992a4d37 | pass | 2026-09-11T03:29:55Z | no |
| Task 4 workspace lib | `cargo nextest run --workspace --lib` (lead: workspace lib 2179/2179) | lead-task | 992a4d37 | pass (2179/2179) | 2026-09-11T03:29:55Z | no |
| Task 4 full | `cargo xtask test full` | lead-task | 992a4d37 | pass (47/47) | 2026-09-11T03:29:55Z | no |
| Task 5 docs contract | `cargo test -p xtask --test docs_contract_tests` | lead-task | c443e598 | pass (14/14) | 2026-09-11T03:40:15Z | no |
| Task 5 instructions paths | `cargo nextest run --lib instructions_paths` | lead-task | c443e598 | pass | 2026-09-11T03:40:15Z | no |
| Task 5 hook envelope, kill switch, sizes, CLAUDE.md == AGENTS.md | lead verification at Task 5 acceptance | lead-task | c443e598 | pass | 2026-09-11T03:40:15Z | no |
| Task 5 routing block names extractor-dep-integration | `cargo test -p xtask --test docs_contract_tests docs_contract_tests_routing_block_carries_the_full_guidance` | worker-exact | 9672bd57 | pass | 2026-09-11T03:42:19Z | no |
| Task 6 harness self-check | `python3 docs/eval/head-to-head/run_matrix.py --self-check` | worker-exact | c2486db3 | pass (`self-check ok`) | 2026-09-11T05:08:26Z | no |
| Task 6 harness validate | `python3 docs/eval/head-to-head/run_matrix.py --validate` | worker-exact | c2486db3 | pass (`46 rows valid`) | 2026-09-11T05:08:26Z | no |
| Task 6 accepted | lead acceptance of head-to-head at `c2486db3` | lead-task | c2486db3 | pass | 2026-09-11T05:08:26Z | no |
| Instructions budget | `wc -c JULIE_AGENT_INSTRUCTIONS.md` | worker-exact | c2486db3 | 1847 characters (≤ 1,900) | 2026-09-11T05:08:26Z | no |
| Standalone guidance lives in the routing block | `cargo nextest run --lib test_agent_instructions_recommend_standalone_for_quick_dogfood_checks` | worker-exact | 3d218f4c | pass | 2026-09-11T05:23:33Z | no |
| Branch fmt | `cargo fmt --check` | lead-fmt | c2486db3 | pass | 2026-09-11T07:30:00Z | no |
| Branch clippy | `cargo clippy --workspace --all-targets` | lead-clippy | c2486db3 | pass (0 errors, 326 warnings; collapsible if, map_or, too many arguments; pre-existing) | 2026-09-11T07:32:00Z | no |
| Branch system | `cargo xtask test system` | lead-system | c2486db3 | pass (4 buckets, 8.6s warm) | 2026-09-11T07:40:00Z | no |
| Branch dogfood | `cargo xtask test dogfood` | lead-dogfood | c2486db3 | pass (2 buckets, 41.7s warm) | 2026-09-11T07:45:00Z | no |
| Branch fast (three runs, median) | `cargo xtask test fast` ×3 | lead-fast | c2486db3 | pass (3.6s, 3.5s, 3.5s warm; median 3.5s) | 2026-09-11T07:50:00Z | no |
| Branch docs contract | `cargo test -p xtask --test docs_contract_tests` | lead-docs-contract | c2486db3 | pass (14/14) | 2026-09-11T07:52:00Z | no |
| Net lines vs `5eafea53` | `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` at `5eafea53` and `c2486db3` | lead-tokei | c2486db3 | pass (540 files / 98,056 lines / 84,551 code → 530 / 95,082 / 81,989; net −2,974 lines, −2,562 code) | 2026-09-11T07:55:00Z | no |
| Resident memory | `julie-server service status` with julie checkout plus ten corpus workspaces open, semantics on | lead-status | c2486db3 | pass. RSS 3,452,176 KB (~3.3 GiB); sidecar child 172,736 KB (`bge-small`) | 2026-09-11T07:58:00Z | no |
| Branch dev at Task 6 HEAD | `cargo xtask test dev` | lead-dev | c2486db3 | FAIL (cli: `test_agent_instructions_recommend_standalone_for_quick_dogfood_checks`) | 2026-09-11T07:35:00Z | no |
| Branch full at Task 6 HEAD | `cargo xtask test full` | lead-full | c2486db3 | FAIL (cli: `test_agent_instructions_recommend_standalone_for_quick_dogfood_checks`) | 2026-09-11T07:36:00Z | no |
| Branch dev at cli-bucket fix | `cargo xtask test dev` | lead-dev | 3d218f4c | pass (30 buckets, 52.4s warm, cold 82.2s) | 2026-09-11T08:10:00Z | no |
| Branch full at cli-bucket fix | `cargo xtask test full` | lead-full | 3d218f4c | pass (47 buckets, 93.4s warm, cold 98.2s) | 2026-09-11T08:10:00Z | no |
| Workspace lib at cli-bucket fix | `cargo nextest run --workspace --lib --no-fail-fast` | lead-lib | 3d218f4c | pass (2179/2179) | 2026-09-11T08:10:00Z | no |
