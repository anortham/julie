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
| Net lines vs `eacfc98f` | `tokei src crates xtask --exclude 'src/tests' --exclude '*/tests/*' -t Rust` | worker-tokei | 9393d9be | 164,870 → 97,978 (net −66,892 lines, −56,573 code) | 2026-09-10T21:20:05Z | no |
| Julie semantics measurements | temp `JULIE_HOME`; index `/home/murphy/source/julie`; 5 searches; `/status` | worker-status | 9393d9be | pass. rss 817,823,744; graph 86,043,992; vectors 8,033; child VmRSS 329,340 kB | 2026-09-10T21:37:47Z | no |
| Miller semantics measurements | temp `JULIE_HOME`; index `/home/murphy/source/miller`; 5 searches; `/status` | worker-status | 9393d9be | pass. rss 3,624,103,936; graph 330,214,728; child VmRSS 394,220 kB | 2026-09-10T21:37:47Z | no |
| Durable roots check | `find $JULIE_HOME -maxdepth 1` | worker-durable-roots | 9393d9be | pass. `registry.db`, `service.json`, `indexes` | 2026-09-10T21:37:47Z | no |
