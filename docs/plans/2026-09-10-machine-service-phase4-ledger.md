# Machine Service Phase 4 Verification Ledger

HEAD at last recorded row: see table. Reuse only when SHA matches current HEAD exactly.

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Sidecar health and query round-trip | `cargo test -p julie-pipeline child_answers_health_and_embeds_over_stdio` | worker-ceiling | 90558c0a | pass | 2026-09-10T19:30:00Z | no |
| Sidecar child transparent respawn | `cargo test -p julie-pipeline provider_respawns_the_child_after_it_exits` | worker-ceiling | 90558c0a | pass | 2026-09-10T19:35:00Z | no |
| Sidecar deadline enforcement | `cargo test -p julie-pipeline child_request_respects_the_deadline` | worker-ceiling | 90558c0a | pass | 2026-09-10T19:37:00Z | no |
| Shared embedding child across checkouts | `cargo test --lib two_checkouts_share_one_embedding_child` | worker-ceiling | f0abe71f | pass | 2026-09-10T19:55:00Z | no |
| Challenge required mode fails closed | `cargo test --lib challenge_required_mode_fails_closed_when_generation_unready` | worker-ceiling | da021069 | pass | 2026-09-10T20:25:00Z | no |
| Model mismatch rejection at spawn | `cargo test -p julie-pipeline fake_sidecar_reporting_other_model_fails_try_new` | worker-ceiling | da021069 | pass | 2026-09-10T20:26:00Z | no |
| Lexical-only mode never spawns child | `cargo test --lib lexical_only_mode_never_spawns_the_child` | worker-ceiling | da021069 | pass | 2026-09-10T20:27:00Z | no |
| Status reports child and vector scan | `cargo test --lib service_status_reports_embedding_child_and_vector_scan_latency` | worker-ceiling | da021069 | pass | 2026-09-10T20:28:00Z | no |
| Durable roots minimal under JULIE_HOME | `cargo test --lib durable_roots_under_julie_home_are_minimal` | worker-ceiling | da021069 | pass | 2026-09-10T20:29:00Z | no |
| Model scorecard evaluation | `python3 docs/eval/semantic-value/run_scorecard.py` (via `eval_model.py`) | worker-scorecard | 9393d9be | pass (bge-small 91.3% top-5 vs qwen3 26.1% top-5) | 2026-09-10T21:18:30Z | no |
| Net lines vs `eacfc98f` | `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` | worker-tokei | 9393d9be | 164,870 → 98,043 (net −66,827 lines, −56,508 code) | 2026-09-10T21:20:05Z | no |
| Julie semantics measurements | temp `JULIE_HOME`; index `/home/murphy/source/julie`; 5 searches; `/status` | worker-status | 9393d9be | pass. rss 817,823,744; graph 86,043,992; vectors 50; child VmRSS 329,340 kB | 2026-09-10T21:37:47Z | no |
| Miller semantics measurements | temp `JULIE_HOME`; index `/home/murphy/source/miller`; 5 searches; `/status` | worker-status | 9393d9be | pass. rss 3,624,103,936; graph 330,214,728; child VmRSS 394,220 kB | 2026-09-10T21:37:47Z | no |
| Durable roots check | `find $JULIE_HOME -maxdepth 1` | worker-durable-roots | 9393d9be | pass. `registry.db` (+wal/shm), `service.json`, `indexes` | 2026-09-10T21:37:47Z | no |
| Non-blocking PID check under in-flight batch | `cargo nextest run --lib child_pid_returns_without_blocking_while_batch_is_in_flight` | worker-exact | 5b44eb13 | pass (2.02s) | 2026-09-10T21:56:55Z | no |
| NL definition deferred embedding init | `cargo nextest run --lib test_nl_definition_search_can_enable_hybrid_without_prior_index_embedding` | worker-exact | 5b44eb13 | pass (0.06s) | 2026-09-10T21:56:43Z | no |
| Incremental index catch-up embedding | `cargo nextest run --lib test_incremental_index_triggers_catch_up_embedding_when_none_exist` | worker-exact | 07451b40 | pass (0.09s) | 2026-09-10T22:12:59Z | no |
| Workspace health initialized status | `cargo nextest run --lib test_manage_workspace_health_reports_initialized_when_not_degraded` | worker-exact | 967a0918 | pass (0.02s) | 2026-09-10T22:18:36Z | no |
| Workspace mod_tests bucket verification | `cargo nextest run --lib tests::tools::workspace::mod_tests` | worker-bucket | 967a0918 | pass (29 passed, 0 failed, 1.13s) | 2026-09-10T22:18:40Z | no |
| Focused bucket: core-embeddings | `cargo xtask test bucket core-embeddings` | lead-focused-bucket | 9393d9be | pass | 2026-09-10T21:40:00Z | no |
| Focused bucket: core-pipeline | `cargo xtask test bucket core-pipeline` | lead-focused-bucket | 9393d9be | pass | 2026-09-10T21:40:00Z | no |
| Focused bucket: service | `cargo xtask test bucket service` | lead-focused-bucket | 9393d9be | pass | 2026-09-10T21:40:00Z | no |
| Dev tier regression gate | `cargo xtask test dev` | lead-dev | 967a0918 | pass (30 buckets, 48.2s warm) | 2026-09-10T22:22:00Z | no |
| System tier gate | `cargo xtask test system` | lead-system | 967a0918 | pass (4 buckets, 6.5s warm) | 2026-09-10T22:22:00Z | no |
| Full suite gate | `cargo xtask test full` | lead-full | 967a0918 | pass (48 buckets, 97.4s warm, cold wall 142.3s) | 2026-09-10T22:22:00Z | no |
| Fast confidence gate | `cargo xtask test fast` ×3 | lead-fast | 967a0918 | pass (3.3s, 3.3s, 3.3s warm; median 3.3s) | 2026-09-10T22:22:00Z | no |
