# Julie Rustfmt Normalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Normalize Julie's complete Rust workspace with the repository-pinned Rust 1.97.0 formatter and close the final quality-roadmap gate without semantic changes.

**Architecture:** Treat the clean pre-normalization worktree and failing formatter check as the RED fixture. Apply only `cargo fmt`, preserve its exact 85-file output as one mechanical Rust-only commit, then prove the resulting code through formatter, compile, affected-change, release-build, dev, and full gates.

**Tech Stack:** Rust 1.97.0, rustfmt 1.9.0-stable, Cargo, cargo-nextest, Julie xtask runner, Git, Miller, Goldfish.

**Architecture Quality:** No Architecture Impact. The formatter may change layout and deterministic import/module ordering only; it must not change public interfaces, dependencies, features, supported platforms, runtime behavior, fixtures, generated artifacts, or non-Rust source files.

## Global Constraints

- Work only in `/Users/murphy/source/julie/.worktrees/julie-improvement-roadmap` on `codex/julie-improvement-roadmap`.
- The canonical formatter is the stable `rustfmt` component supplied by repository-pinned Rust `1.97.0`; do not add or change rustfmt configuration.
- The implementation command is exactly `cargo fmt`; do not mix manual cleanup, comment rewriting, refactoring, dependency changes, or opportunistic fixes into the mechanical commit.
- The mechanical commit may contain only the 85 Rust paths in Appendix A, and every changed byte must be reproducible by running `cargo fmt` from pre-normalization commit `c3f54d74596817ba420fc4097a5ab25356254db9`.
- Preserve macOS 11 support and do not suppress linker diagnostics.
- Run one test command at a time. The lead owns all broad gates because this is a no-delegation, tightly sequential plan.
- Record every result with the exact commit SHA in `docs/plans/2026-07-22-rustfmt-normalization-verification.md`.
- Do not push, merge, publish, deploy, tag, or release without separate explicit approval.

---

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `docs/DEVELOPMENT.md`, `rust-toolchain.toml`, `.cargo/config.toml`, `docs/plans/2026-07-21-julie-improvement-roadmap-design.md`, and `docs/plans/2026-07-22-rustfmt-normalization-design.md`.

**Worker red/green scope:** RED is `cargo fmt --check` failing on the 85-file Appendix A manifest at `c3f54d74596817ba420fc4097a5ab25356254db9`. GREEN is `cargo fmt --check` passing immediately after the single `cargo fmt` implementation command.

**Worker ceiling:** Not applicable; the lead executes this single tightly sequential task and owns every command.

**Worker gate invariant:** The formatter failure is removed entirely, the changed path set is exactly Appendix A, and no non-Rust path enters the mechanical diff.

**Lead affected-change scope:** Run `cargo check`, then `cargo xtask test changed`. The broad 85-file diff is expected to be OverBudget; if so, record the mapped buckets and run `cargo xtask test changed --scale` as the explicit `unique(mapped ∪ dev)` escalation.

**Branch gate:** Run `cargo xtask test dev`, `cargo build --release --bin julie-server --bin julie-embedding-host`, and `cargo xtask test full` on the exact mechanical commit. The release build must contain neither `linker stderr` nor `newer than target minimum`.

**Replay/metric evidence:** Clean formatting, the exact 85-file Rust-only manifest, successful compilation, warning-free release build, and passing test gates are hard requirements. Command duration and rustfmt line churn are report-only.

**Escalation triggers:** Any non-Rust changed file, changed path outside Appendix A, manual diff, dependency/feature/configuration change, compile/test failure, Linux/Windows workflow semantic change, macOS deployment target above 11.0, or linker warning blocks closeout and requires investigation rather than weakening the plan.

**Assigned verification failure:** The lead investigates failures through `razorback:systematic-debugging`; do not edit tests or production behavior merely to accommodate mechanical formatting.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, timestamp, warm/prebuild/cold-wall timings for xtask gates, and warning scan for the release build. Reuse evidence only when the scope label and exact commit SHA both match.

## Parallel Execution Contract

| Task | Parallel batch | File ownership | Serialization required | Dependency reason |
|---|---|---|---|---|
| Task 1: Normalize and promote the pinned formatter baseline | None - serial | The 85 Rust files in Appendix A; `docs/plans/2026-07-22-rustfmt-normalization.md`; `docs/plans/2026-07-22-rustfmt-normalization-verification.md`; `.memories/briefs/julie-quality-improvement-roadmap.md`; Phase 4 Goldfish checkpoints | Not applicable - single task. | Not applicable - single task. |

### Task 1: Normalize and promote the pinned formatter baseline

**Files:**
- Modify: The exact 85 Rust paths in Appendix A.
- Modify: `docs/plans/2026-07-22-rustfmt-normalization.md`
- Create/modify: `docs/plans/2026-07-22-rustfmt-normalization-verification.md`
- Modify: `.memories/briefs/julie-quality-improvement-roadmap.md`
- Create: Phase 4 checkpoint files under `.memories/2026-07-22/`

**Interfaces:**
- Consumes: `rust-toolchain.toml` with `channel = "1.97.0"` and `components = ["rustfmt", "clippy"]`; the clean commit `c3f54d74596817ba420fc4097a5ab25356254db9`; Cargo workspace membership; Appendix A.
- Produces: A clean canonical formatter baseline, a Rust-only mechanical commit, exact-SHA verification evidence, and a completed Julie quality-roadmap brief.

**Contract inputs:** `cargo fmt` is the only implementation command. The path manifest and warning-free release-build contract are hard gates. No manual source edits are authorized by this plan.

**File ownership:** The 85 Rust files in Appendix A; `docs/plans/2026-07-22-rustfmt-normalization.md`; `docs/plans/2026-07-22-rustfmt-normalization-verification.md`; `.memories/briefs/julie-quality-improvement-roadmap.md`; Phase 4 Goldfish checkpoints

**Serialization required:** Not applicable - single task.

**Dependency reason:** Not applicable - single task.

**Step 1: Commit the approved execution contract**

After approval, create a Goldfish checkpoint for the locked plan, then commit only this plan, the initial verification ledger, and that checkpoint as `docs(plan): define phase 4 rustfmt normalization`.

Expected: The worktree is clean, no Rust file has changed, and the plan commit becomes the exact execution HEAD.

**Step 2: Verify the RED formatter baseline**

Run: `cargo fmt --check`

Expected: FAIL with exit code 1 and rustfmt diffs across exactly the 85 paths in Appendix A. Record the baseline row against `c3f54d74596817ba420fc4097a5ab25356254db9`.

Run: `cargo fmt --check 2>&1 | awk '/^Diff in / {path=$3; sub(/:[0-9]+:$/, "", path); sub(/^.*julie-improvement-roadmap\//, "", path); seen[path]=1} END {for (path in seen) print path}' | sort`

Expected: The output matches Appendix A exactly.

**Step 3: Apply the minimal mechanical implementation**

Run: `cargo fmt`

Expected: PASS. Do not run any other source-mutating command.

**Step 4: Verify GREEN and audit the mechanical diff**

Run in order:

```bash
cargo fmt --check
git diff --check
git diff --name-only HEAD -- '*.rs' | sort
git diff --name-only HEAD | awk '$0 !~ /\.rs$/ { print; bad=1 } END { exit bad }'
git diff --stat
git diff --word-diff=porcelain
```

Expected: Formatter and whitespace checks pass; the Rust path list matches Appendix A; the non-Rust command prints nothing and exits zero; the complete review finds only formatter-owned layout/import/module-order changes. Inspect macro bodies, string literals, imports, and module declarations closely.

Use Miller `impact(git=true)` after refreshing the workspace, then inspect any formatter-changed public symbol whose signature appears in the impact output. Expected: no public interface or dependency expansion.

**Step 5: Capture the pre-commit checkpoint without polluting the mechanical commit**

Create a Goldfish checkpoint containing the RED/GREEN and diff-audit evidence. Commit only the checkpoint and current plan/ledger evidence as `docs(plan): record phase 4 normalization baseline`; leave the 85 Rust files unstaged.

Expected: The remaining working-tree diff contains exactly Appendix A and only `.rs` files.

**Step 6: Commit the formatter-owned Rust diff**

Run the Rust-only path and diff checks from Step 4 again, stage only Appendix A, and commit as `style: normalize repository formatting`.

Expected: The commit contains exactly 85 Rust files, `git show --check --format= HEAD` passes, and the worktree is clean.

**Step 7: Run exact-code verification gates**

Run one command at a time against the mechanical commit:

```bash
cargo fmt --check
cargo check
cargo nextest run -p xtask --test toolchain_contract_tests toolchain_contract_pins_release_build_inputs
cargo xtask test changed
cargo xtask test changed --scale
cargo xtask test dev
cargo build --release --bin julie-server --bin julie-embedding-host
cargo xtask test full
```

Expected: Formatting, compile, toolchain contract, dev, release build, and full pass. `changed` may exit non-zero only with the documented OverBudget result; in that case `changed --scale` must pass. Skip `changed --scale` only if ordinary `changed` actually runs and passes every mapped bucket. The release build output contains neither prohibited linker warning.

**Step 8: Close the roadmap records**

Update the verification ledger with exact SHAs and timings, tick this plan's criteria, mark the Goldfish brief completed, create a final checkpoint, and commit the non-code closeout as `docs(roadmap): complete Julie quality roadmap`.

Run the final worktree-state audit for the task, main, and release worktrees. Do not push or integrate.

**Step 9: Apply commit mode**

- `serial-worker-commit`: the lead owns the baseline metadata commit, the formatter-only mechanical commit, and the final roadmap closeout commit. Record every SHA in the verification ledger.

**Acceptance criteria:**
- [ ] RED reproduces exactly the 85-file Appendix A baseline.
- [ ] One `cargo fmt` command creates the complete source diff and `cargo fmt --check` passes afterward.
- [ ] The mechanical commit contains only reproducible rustfmt output in the Appendix A Rust files.
- [ ] No public interface, dependency, feature, platform, fixture, generated artifact, or runtime behavior changes are introduced.
- [ ] `cargo check`, the toolchain contract, affected-change escalation, `dev`, warning-free release build, and `full` pass on recorded exact SHAs.
- [ ] The verification ledger, this checklist, and completed Goldfish brief agree on Phase 4 and roadmap status.
- [ ] The final task worktree is clean; no push, merge, publish, deploy, tag, or release occurs.

## Appendix A: Exact Rustfmt Path Manifest

1. `crates/julie-core/src/database/bulk/write_set.rs`
2. `crates/julie-core/src/database/files.rs`
3. `crates/julie-core/src/database/helpers.rs`
4. `crates/julie-core/src/glob.rs`
5. `crates/julie-core/src/test_support/cleanup.rs`
6. `crates/julie-core/src/test_support/mod.rs`
7. `crates/julie-core/src/tests/database/basic_storage.rs`
8. `crates/julie-core/src/tests/mod.rs`
9. `crates/julie-index/src/analysis/early_warnings.rs`
10. `crates/julie-index/src/analysis/test_quality.rs`
11. `crates/julie-index/src/search/scoring.rs`
12. `crates/julie-index/src/tests/analysis/early_warning_report_tests.rs`
13. `crates/julie-index/src/tests/analysis/literals_tests.rs`
14. `crates/julie-index/src/tests/analysis/quality_pipeline_tests.rs`
15. `crates/julie-index/src/tests/analysis/test_quality_tests.rs`
16. `crates/julie-index/src/tests/search/projection_search_doc_test.rs`
17. `crates/julie-index/src/tests/search/reranker_ordering_tests.rs`
18. `crates/julie-index/src/tests/search/reranker_tests.rs`
19. `crates/julie-index/src/tests/search/tantivy_cross_process_reload_test.rs`
20. `crates/julie-index/src/tests/search/unified_reranker_test.rs`
21. `crates/julie-pipeline/src/embeddings/host_server.rs`
22. `crates/julie-pipeline/src/embeddings/init.rs`
23. `crates/julie-pipeline/src/embeddings/mod.rs`
24. `crates/julie-pipeline/src/embeddings/pipeline.rs`
25. `crates/julie-pipeline/src/embeddings/rpc_client.rs`
26. `crates/julie-pipeline/src/finalize.rs`
27. `crates/julie-pipeline/src/indexing_core/discovery.rs`
28. `crates/julie-pipeline/src/indexing_core/extraction.rs`
29. `crates/julie-runtime/src/tests/mod.rs`
30. `crates/julie-runtime/src/tests/watcher.rs`
31. `crates/julie-runtime/src/tests/watcher/event_queue.rs`
32. `crates/julie-runtime/src/tests/watcher_observability.rs`
33. `crates/julie-runtime/src/tests/watcher_queue.rs`
34. `crates/julie-runtime/src/tests/workspace/mod.rs`
35. `crates/julie-tools/src/deep_dive/data.rs`
36. `crates/julie-tools/src/deep_dive/formatting.rs`
37. `crates/julie-tools/src/deep_dive/mod.rs`
38. `crates/julie-tools/src/editing/rewrite_symbol.rs`
39. `crates/julie-tools/src/get_context/entries.rs`
40. `crates/julie-tools/src/get_context/task_signals.rs`
41. `crates/julie-tools/src/navigation/fast_refs.rs`
42. `crates/julie-tools/src/navigation/target_workspace.rs`
43. `crates/julie-tools/src/refactoring/mod.rs`
44. `crates/julie-tools/src/refactoring/rename.rs`
45. `crates/julie-tools/src/search/hint_formatter.rs`
46. `crates/julie-tools/src/search/mod.rs`
47. `crates/julie-tools/src/search/nl_embeddings.rs`
48. `crates/julie-tools/src/symbols/mod.rs`
49. `crates/julie-tools/src/symbols/primary.rs`
50. `crates/julie-tools/src/symbols/target_workspace.rs`
51. `crates/julie-tools/src/tests/blast_radius_formatting_tests.rs`
52. `crates/julie-tools/src/tests/deep_dive_regression_tests.rs`
53. `crates/julie-tools/src/tests/deep_dive_tests/data_tests.rs`
54. `crates/julie-tools/src/tests/deep_dive_tests/formatting_tests.rs`
55. `crates/julie-tools/src/tests/filtering_tests.rs`
56. `crates/julie-tools/src/tests/formatting_tests.rs`
57. `crates/julie-tools/src/tests/get_context_graph_expansion_tests.rs`
58. `crates/julie-tools/src/tests/get_context_pipeline_relevance_tests.rs`
59. `crates/julie-tools/src/tests/get_context_pipeline_tests.rs`
60. `crates/julie-tools/src/tests/get_context_quality_tests.rs`
61. `crates/julie-tools/src/tests/get_context_relevance_tests.rs`
62. `crates/julie-tools/src/tests/get_context_scoring_tests.rs`
63. `crates/julie-tools/src/tests/get_context_task_inputs_tests.rs`
64. `crates/julie-tools/src/tests/get_context_tests.rs`
65. `crates/julie-tools/src/tests/hybrid_search_tests/knn_conversion.rs`
66. `crates/julie-tools/src/tests/hybrid_search_tests/lock_free_embed.rs`
67. `crates/julie-tools/src/tests/hybrid_search_tests/orchestrator.rs`
68. `crates/julie-tools/src/tests/hybrid_search_tests/weight_profile_wiring.rs`
69. `crates/julie-tools/src/tests/phase4_token_savings.rs`
70. `crates/julie-tools/src/tests/search_annotation_search_tests.rs`
71. `crates/julie-tools/src/tests/search_nl_path_prior_pipeline_tests.rs`
72. `crates/julie-tools/src/tests/search_nl_symbol_query_latency_tests.rs`
73. `crates/julie-tools/src/tests/search_title_exact_boost_tests.rs`
74. `crates/julie-tools/src/tests/tantivy_index_tests/schema_compat.rs`
75. `src/cli_tools/commands.rs`
76. `src/cli_tools/mod.rs`
77. `src/handler/tools/fast_search.rs`
78. `src/handler/workspace_resolution.rs`
79. `src/leadership.rs`
80. `src/tests/core/handler/metrics_recording.rs`
81. `src/tests/tools/search/race_condition.rs`
82. `src/tests/tools/search_quality/dogfood_tests.rs`
83. `src/tools/workspace/indexing/file_policy.rs`
84. `src/tools/workspace/indexing/state.rs`
85. `src/utils/mod.rs`
