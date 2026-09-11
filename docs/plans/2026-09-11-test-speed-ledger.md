# Test Speed Verification Ledger

Plan: `docs/plans/2026-09-11-test-speed-plan.md`. Finding: `docs/findings/2026-09-11-test-speed.md`.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Task 1: client tests finish under 1 s; race test polls readiness and asserts the result | `cargo nextest run --lib -p julie <each Task 1 test>` | worker-red-green | b94f5b98 | pass, 0.212 s, 0.210 s, 0.040 s | 2026-09-11T19:30:00Z | no |
| Task 2: re-export resolution through the index returns the same targets; graph tests pass | `cargo nextest run -p julie-index crate_call_through_reexport_resolves_to_the_definition`; `cargo nextest run -p julie-index tests::graph` | worker-red-green | 723ae550 | pass, 39 graph tests | 2026-09-11T19:32:00Z | no |
| Task 2: dogfood store open cost on the fixture | `cargo nextest run --lib -p julie test_stemming_estimation_finds_estimator` (three walls per stage) | worker-red-green | 723ae550 | baseline 8.37, 9.72, 8.69 s; index only 3.13, 3.15, 3.09 s; index plus profile 1.41, 1.43, 1.39 s | 2026-09-11T19:32:00Z | no |
| Task 3: the scenario test passes | `cargo nextest run --lib -p julie test_scenario_2_refactoring_pipeline`; `cargo nextest run --lib -p julie tests::request_scenarios` | worker-red-green | a17af8cb | pass, 3 tests | 2026-09-11T19:30:13Z | no |
| Batch A: whole workspace in one nextest call | `cargo nextest run --workspace -E 'not (test(/search_quality/) \| test(/fixtures::julie_db/) \| test(/dogfood/))'` | dev | 723ae550 | pass, 2354 tests, 14.67 s tests, 34.98 s wall with the post-profile rebuild | 2026-09-11T19:35:00Z | no |
| Task 4: runner runs the fixed command lists and stops on the first failure | `cargo nextest run -p xtask` | worker-red-green | edb6d1e5 | 30 passed, 2 failed (docs contract, handed to Task 5) | 2026-09-11T19:38:00Z | no |
| Task 4: tiers exist and unknown tiers exit 2 | `cargo xtask test nano`; `cargo xtask test dev`; `cargo xtask test dogfood` | worker-red-green | edb6d1e5 | exit 2; dev 1/3 (docs contract only); dogfood 2/2 in 14.4 s | 2026-09-11T19:38:00Z | no |
| Task 5: docs name only the new commands; CLAUDE.md equals AGENTS.md | `cargo nextest run -p xtask docs_contract`; `diff CLAUDE.md AGENTS.md` | worker-red-green | 82d99134 | pass, 9 tests, empty diff | 2026-09-11T19:43:00Z | no |
| Formatting and lints | `cargo fmt --check`; `cargo clippy --workspace --all-targets` | full | 82d99134 | pass, 0 errors, clippy 17.7 s | 2026-09-11T19:45:00Z | no |
| Dev tier, three warm runs | `cargo xtask test dev` | dev | 82d99134 | pass 3/3 commands: 19.3 s (rebuild after clippy), 12.2 s, 11.9 s; median 12.2 s | 2026-09-11T19:47:00Z | no |
| Dogfood tier, three warm runs with the fixture present | `cargo xtask test dogfood` | dogfood | 82d99134 | pass 2/2 commands: 14.5 s, 14.6 s, 14.6 s; median 14.6 s; median test 1.76 s | 2026-09-11T19:48:00Z | no |
| Full tier | `cargo xtask test full` | full | 82d99134 | pass 5/5 commands in 26.3 s | 2026-09-11T19:49:00Z | no |
| Zero uncovered tests | `cargo nextest list --workspace` against the dev and dogfood filtersets | full | 82d99134 | 2275 listed = 2206 dev + 69 dogfood; 29 ignored, of which dev runs 13 (`tests::cli::`) and dogfood runs `ensure_julie_fixture` | 2026-09-11T19:50:00Z | no |
| Service graph load on the branch release binary (report-only) | `cargo build --release`; `julie-server service restart`; `julie-server service status` | live | 82d99134 | julie 638 ms for 76,541 symbols; Alamofire 3,040 ms for 104,082; nlohmann-json 416 ms for 32,720 | 2026-09-11T19:51:00Z | no |
| Test services exit after the fixture's idle timeout | `cargo nextest run --lib -p julie test_parity_generic_tool_subcommand`; `pgrep -fc '^…/target/debug/julie-server service$'` after 4 s | worker-red-green | dd0ebe7a | pass; 0 services left (129 before the fix) | 2026-09-11T19:53:00Z | no |
| Branch gate after the fixture fix | `cargo fmt --check`; `cargo clippy --workspace --all-targets`; `cargo xtask test full`; leak check | full | dd0ebe7a | pass, 0 clippy errors, full 5/5 commands in 26.4 s (2206 + 13 + 69 tests), 0 services left | 2026-09-11T19:54:13Z | no |
| Security scope | none declared | full | dd0ebe7a | not applicable | 2026-09-11T19:54:13Z | no |
