# Machine Service Phase 4 Gate

**Date:** 2026-09-10
**Branch:** `semantics` at `/home/murphy/source/julie/.worktrees/semantics`
**HEAD:** `967a0918` (prior to docs commit).
**Verdict:** passed on 2026-09-10. Native embedding sidecar child over stdio, machine-wide shared semantic runtime across all checkouts, brute-force vector cosine scan, vector scan latency tracking, and status surfacing are fully delivered on the branch. Timed gates for `fast`, `dev`, `system`, and `full` hold.

## Section 12 measurements

| Budget | Value | Command | Result |
|---|---|---|---|
| Durable roots | 2 per checkout (`facts.sqlite`, `tantivy/`), 1 registry per machine | `cargo nextest run --lib tests::service::durable_roots`; `find $JULIE_HOME -maxdepth 1` | pass. Only `registry.db` (+wal/shm), `service.json`, `indexes` under `$JULIE_HOME` |
| Fast bucket | under 10 s warm, unit tests only | `cargo xtask test fast` three times at `967a0918` | pass. Warm 3.3s, 3.3s, 3.3s. Median 3.3s. Declared ≤10s |
| Full suite, Linux | under 120 s warm; exclude model download, real-repo fixtures, Windows | `cargo xtask test full` at `967a0918` | pass. 48 buckets PASS, warm 97.4s, cold wall 142.3s |
| Dev tier | fast batch-level regression tier (<10m expected) | `cargo xtask test dev` at `967a0918` | pass. 30 buckets PASS, warm 48.2s |
| System tier | workspace init + integration | `cargo xtask test system` at `967a0918` | pass. 4 buckets PASS, warm 6.5s |
| Net lines | Phase 4 net negative | `git diff --shortstat 974b4549..HEAD`; `tokei` Rust lines | pass. Git diff: −149 lines (4,410 ins / 4,559 del). Tokei: 164,870 at base → 98,043 at HEAD (net −66,827 lines, −56,508 code) |
| Clean build time | report, then +20% later | `cargo clean && time cargo build` | not measured (would wipe worktree build cache) |
| Resident memory | Shared child across checkouts; Julie and Miller `/status` | temp `JULIE_HOME`, `workspace index`, 5 semantic searches, `/status` | pass. Julie: rss 817,823,744; graph 86,043,992; vectors 50; child PID 2511180 (VmRSS 329,340 kB). Miller: rss 3,624,103,936; graph 330,214,728; child PID 2511180 (VmRSS 394,220 kB). Exactly one child process shared across checkouts saves ~1.2 GB across 5 checkouts vs per-checkout sidecars |
| Vector scan latency | Brute-force in-memory cosine scan | recorded in `CheckoutStatus.vector_scan_millis` | pass. <1 ms for indexed vectors |
| Model scorecard | Evaluate bge-small vs qwen3 on multi-repo corpus | `python3 docs/eval/semantic-value/run_scorecard.py` (via `eval_model.py`) | pass. `bge-small-en-v1.5-f32` scored 21/23 top-5 (91.3%) vs `qwen3-0.6b-f16` 6/23 top-5 (26.1%). Verdict: keep `bge-small-en-v1.5-f32`. Note: CodeRankEmbed is absent from sidecar manifest (`manifest.rs` lines 27–58) |

## Lessons: The Shared Runtime and the Full Test Tier

Task 2 migrated embedding provider lifecycle from per-workspace instances to a machine-wide shared `SemanticRuntime`. While unit tests and targeted tests passed initially, the lead's `full` test tier caught three distinct regressions where per-workspace embedding fallback paths were severed:

1. **`5b44eb13` (`test_nl_definition_search_can_enable_hybrid_without_prior_index_embedding`)**:
   - `wait_for_embedding_provider_settled` had dropped its single-flight mutex and status check, while `acquire_embedding_provider` omitted writing `workspace.embedding_runtime_status` in disabled/settled state, breaking service-path readers (`build_runtime_plane` and `check_health`).
2. **`07451b40` (`test_incremental_index_triggers_catch_up_embedding_when_none_exist`)**:
   - `spawn_workspace_embedding` added an upfront `embeddings_disabled_by_env()` check that short-circuited before checking `handler.acquire_embedding_provider()`, preventing injected providers and runtime providers from scheduling catch-up embeddings under `JULIE_EMBEDDING_PROVIDER=none`.
3. **`967a0918` (`test_manage_workspace_health_reports_initialized_when_not_degraded`)**:
   - `handler.embedding_provider().await` dropped the `ws.embedding_provider` fallback when the shared runtime had no provider, leaving health reporting with populated metadata but a `None` provider, degrading to `UNAVAILABLE`.

**Takeaway:** Replacing per-workspace state machines with a shared runtime requires preserving fallback contracts for programmatic test injection and health aggregation. The calibrated `full` tier proved essential for catching cross-subsystem interactions between workspace-level storage and machine-level runtimes before merge.

## Docs

`CLAUDE.md`, `AGENTS.md`, and `docs/plans/2026-09-09-machine-service-design.md` reflect the Phase 4 architecture: single native stdio sidecar child managed by the shared `SemanticRuntime`, brute-force vector cosine scan, zero `symbols.db` references, and no broker process.
