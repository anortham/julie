# Impact Mixed Traversal Verification

## Promotion Contract

- Hard gate: every expected internal symbol in the curated corpus is found.
- Hard gate: no unexpected internal symbol links are emitted.
- Hard gate: default-mode compact output remains byte-identical to the pre-change snapshot.
- Report only: per-case recall and default/web latency p50 and p95.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Curated corpus covers all approved categories and validates its references | `cargo nextest run --lib phase3_mixed_traversal_corpus_is_complete` | worker-exact-baseline | `1d516b9b5af2bbebbab5dd99686148eba3ae9b1a` | pass: 1 passed | 2026-07-22T18:24:50Z | no |
| Default compact output is locked before production behavior changes | `cargo nextest run --lib phase3_default_mode_matches_legacy_snapshot` | worker-exact-baseline | `1d516b9b5af2bbebbab5dd99686148eba3ae9b1a` | pass: 1 passed | 2026-07-22T18:24:50Z | no |
| Pre-change scorecard records gaps without unexpected internal links | `cargo nextest run --lib phase3_mixed_traversal_scorecard --no-capture` | evaluation-baseline | `1d516b9b5af2bbebbab5dd99686148eba3ae9b1a` | pass harness; hard gates: default unchanged, unexpected internal 0; HTTP missing fetch_user/page_loader; SQL missing report_page; report-only p50/p95 us default 1456/1587, web 1315/1459 | 2026-07-22T18:24:50Z | no |
| HTTP mixed traversal test proves the direct-only implementation is incomplete | `cargo nextest run --lib phase3_web_mode_traverses_http_mixed_path --no-capture` | worker-exact-red | `924512977d4470f094ee2a64e820301010e15ec8` plus uncommitted behavior test | expected fail: `fetch_user` absent | 2026-07-22T18:32:17Z | no |
| Web mode traverses the HTTP mixed path | `cargo nextest run --lib phase3_web_mode_traverses_http_mixed_path` | worker-exact-green | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Web mode traverses the SQL mixed path | `cargo nextest run --lib phase3_web_mode_traverses_sql_mixed_path` | worker-exact-green | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| External web edges remain terminal | `cargo nextest run --lib phase3_web_mode_keeps_external_edges_terminal` | worker-exact-green | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Cycles and duplicate routes deduplicate at shortest distance | `cargo nextest run --lib phase3_web_mode_deduplicates_cycles_at_shortest_distance` | worker-exact-green | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| One depth limit applies across ordinary and web edges | `cargo nextest run --lib phase3_web_mode_applies_one_combined_depth_limit` | worker-exact-green | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Mixed traversal output is deterministic | `cargo nextest run --lib phase3_web_mode_is_deterministic` | worker-exact-green | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Default mode remains byte-identical to the locked snapshot | `cargo nextest run --lib phase3_default_mode_matches_legacy_snapshot` | worker-exact-compatibility | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Existing HTTP web-caller provenance remains intact | `cargo nextest run --lib impact_web_mode_lists_calling_frontend_symbols` | worker-exact-compatibility | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Existing SQL web-caller provenance remains intact | `cargo nextest run --lib impact_web_mode_lists_routines_querying_table` | worker-exact-compatibility | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Identifier fanout budget remains intact | `cargo nextest run --lib test_walk_impacts_caps_identifier_fanout_for_common_names` | worker-exact-compatibility | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Depth-frontier budget remains intact | `cargo nextest run --lib test_blast_radius_limit_bounds_depth_frontier` | worker-exact-compatibility | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass: 1 passed | 2026-07-22T18:32:17Z | no |
| Final scorecard satisfies every promotion hard gate | `cargo nextest run --lib phase3_mixed_traversal_scorecard --no-capture` | evaluation-final-code | `8ca200a1bbac4273ba33ea1c6a6b38b22250b0a0` | pass; hard gates: expected 7/7, unexpected internal 0, default unchanged; per-case recall HTTP 5/5, SQL 2/2; report-only p50/p95 us default 1357/1685, web 1570/1715 | 2026-07-22T18:32:17Z | no |
| Phase 3 Rust files match the pinned formatter | `rustfmt --edition 2024 --check crates/julie-tools/src/impact/mod.rs crates/julie-tools/src/impact/walk.rs src/tests/tools/blast_radius_mixed_traversal.rs src/tests/tools/blast_radius_mixed_traversal/behavior.rs src/tests/tools/blast_radius_mixed_traversal/corpus.rs src/tests/tools/blast_radius_mixed_traversal/scorecard.rs` | lead-format-targeted | `6cb130efd316d17f842e58a4d3c58a5351499d22` | pass | 2026-07-22T19:01:54Z | no |
| Repository-wide formatter baseline is diagnosed without an unrelated rewrite | `cargo fmt --check` | branch-format | `aa153f1823c0433506f236f5a38c93f4801247e8` | baseline fail: broad pre-existing rustfmt drift under pinned Rust 1.97.0; Phase 3-owned drift corrected in `6cb130ef`; normalization remains the roadmap's final Phase 4 item | 2026-07-22T19:01:54Z | no |
| Workspace compiles on final code HEAD | `cargo check` | lead-compile | `6cb130efd316d17f842e58a4d3c58a5351499d22` | pass | 2026-07-22T19:01:54Z | no |
| Focused blast-radius and spillover coverage passes | `cargo xtask test bucket tools-blast-spillover` | lead-focused | `6cb130efd316d17f842e58a4d3c58a5351499d22` | pass: 1 bucket; warm 2.9s, prebuild 18.3s, cold wall 21.2s | 2026-07-22T19:01:54Z | no |
| Changed-file runner outcome is recorded honestly after the implementation commit | `cargo xtask test changed` | lead-changed-routing | `6cb130efd316d17f842e58a4d3c58a5351499d22` | no-op pass: clean worktree had no local changes to map; focused bucket supplies the intended blast/spillover coverage | 2026-07-22T19:01:54Z | no |
| Final code impact preserves the public/default boundary and remains language-agnostic | Miller `impact(git=true, base=61072763)` plus full inspection of `walk_impacts_with_policy` and `run_with_db` | lead-impact | `6cb130efd316d17f842e58a4d3c58a5351499d22` | pass: policy is parent-private; default wrapper unchanged; no language branch; one shared walk owns depth, budget, deduplication, and ordering | 2026-07-22T19:01:54Z | no |
| Development branch gate passes | `cargo xtask test dev` | branch-dev | `6cb130efd316d17f842e58a4d3c58a5351499d22` | pass: 27 buckets; warm 242.2s, prebuild 20.2s, cold wall 262.4s | 2026-07-22T19:01:54Z | no |
| Full branch gate passes | `cargo xtask test full` | branch-full | `6cb130efd316d17f842e58a4d3c58a5351499d22` | pass: 49 buckets; warm 1265.0s, prebuild 10.1s, cold wall 1275.1s | 2026-07-22T19:01:54Z | no |

## Promotion Decision

Phase 3 mixed traversal is promoted for the existing opt-in `web` mode. The curated corpus found every expected internal symbol, emitted no unexpected internal links, and preserved default-mode output. Latency remains report-only and did not influence promotion.

The repository-wide `cargo fmt --check` baseline is not a Phase 3 regression. It is the already documented one-time normalization item remaining in roadmap Phase 4; all Phase 3 Rust files pass the pinned formatter directly.
