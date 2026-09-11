# Hybrid Auto Backend Finding

**Verdict:** passed on 2026-09-11. An omitted `backend` now runs hybrid symbol search for natural-language queries when the workspace has ready vectors, and lexical otherwise. Every other query shape and every explicit backend behave as before.

Plan: `docs/plans/2026-09-11-hybrid-auto-backend-plan.md`. Design: `docs/plans/2026-09-11-hybrid-auto-backend-design.md`. Ledger: `docs/plans/2026-09-11-hybrid-auto-backend-ledger.md`. Branch `hybrid-auto`, product commits `28a616cb..74eb5b81` over `main` at `d82cbc76`.

## What changed

- `SearchBackend::resolve` takes the query. With no backend it returns `Hybrid` when `is_nl_like_query` is true and `looks_like_file_or_path_query` is false.
- The auto path reads the already-live embedding provider and never waits for lazy init. Only an explicit `semantic` or `hybrid` request pays the init wait. The review found that the production `ensure_embedding_provider` ignores its timeout and can block up to 30 s.
- A zero-hit auto-hybrid pass falls through to the lexical pass. No note, no `backend_fallback`.
- The compact header reads `(hybrid)` when auto ran hybrid and `(auto)` otherwise.
- Telemetry rows carry `backend_auto` so phase 6b can count how often auto chose hybrid.
- The dashboard search route and `xtask-eval` pass the query to the resolver.
- Tool descriptions, `JULIE_AGENT_INSTRUCTIONS.md` (1,881 characters), the routing block (3,937 bytes), and the explore-area skill say the same thing about the omitted backend.
- `docs/eval/semantic-value/run_scorecard.py --service` calls the machine service JSON API. The standalone CLI never writes vectors, so every semantic column it produced under the machine-service architecture was a fallback.

## Gate table

| Gate | Result |
|---|---|
| Seven new tests plus the provider-wait test | pass |
| `cargo xtask test dev` | pass, 30 buckets, 140 s warm |
| `cargo xtask test dogfood` | pass, 2 buckets, 47 s warm (home-disk `TMPDIR`) |
| `NEXTEST_TEST_THREADS=6 cargo xtask test full` | pass, 47 buckets, 92 s warm |
| `cargo fmt --check`, `cargo clippy --workspace --all-targets` | pass, 0 errors |
| Scorecard hybrid vs lexical relative MRR (bar 20 percent) | pass, 93 percent |
| Head-to-head auto top-5 within one row of 6a hybrid per class | pass, exact match |
| Head-to-head `inspect.symbol` top-5 | pass, 23/23 |

## Scorecard

`python3 docs/eval/semantic-value/run_scorecard.py --service --backend lexical --backend hybrid --backend semantic` at `74eb5b81`. Result file `docs/eval/semantic-value/results/2026-09-11T13-25-15Z.md`. Twenty-three cases over the ten public repos, all with full definition-kind vector coverage.

| backend | top-1 | top-3 | top-5 | MRR | p95 latency |
|---|---:|---:|---:|---:|---:|
| lexical | 30.4% | 39.1% | 43.5% | 0.351 | 338 ms |
| hybrid | 56.5% | 73.9% | 91.3% | 0.677 | 391 ms |
| semantic | 78.3% | 91.3% | 91.3% | 0.848 | 55 ms |

Outcomes: 14 semantic wins, 9 ties, 0 lexical wins. One case (`alamofire-interceptor-selection`) misses under every backend.

## Head-to-head

`python3 docs/eval/head-to-head/run_matrix.py --skip-miller --require-semantics --julie-backends auto,hybrid` at `74eb5b81` on warm indexes. Result file `docs/eval/head-to-head/results/20260911T132525Z.md`. The warm-up pair `20260911T132116Z` was discarded. Miller columns are empty because Miller was skipped; the 6a Miller numbers stand.

| class | n | auto top-1 | auto top-5 | hybrid top-1 | hybrid top-5 | 6a auto (lexical) top-5 | 6a Miller top-5 |
|---|---:|---:|---:|---:|---:|---:|---:|
| retrieval.concept | 13 | 54% | 77% | 54% | 77% | 46% | 54% |
| retrieval.implementation | 10 | 60% | 90% | 60% | 90% | 20% | 70% |
| inspect.symbol | 23 | 87% | 100% | — | — | 100% | 100% |

All 23 auto search rows ran hybrid; none fell back to lexical. `fast_search` p50 38 ms, p95 75 ms, p50 479 bytes. The 6a lexical run measured p50 37 ms and 589 bytes, so the default costs nothing measurable on these rows once the child is warm.

## Resident memory

`service status` after the runs: process RSS 5,269 MB with 13 live checkouts and 37 dead temp checkouts from the CLI test bucket; sidecar child 204 MB. The 6a number was 3,452 MB with 11 checkouts. The dead rows and the two julie checkouts (main and this worktree, 64k symbols each) account for the difference. Re-measure after the registry hygiene item lands.

## Observations for the owner

- **Semantic alone beats hybrid** on the scorecard: MRR 0.848 vs 0.677, top-1 78 percent vs 57 percent, and p95 55 ms vs 391 ms. Hybrid's latency is the lexical half: on gson, moshi, and alamofire the lexical pass alone takes 270 to 350 ms. The head-to-head did not run a semantic column this time; in 6a semantic scored 77 and 100 percent top-5 against hybrid's 77 and 90. Auto picking `semantic` instead of `hybrid` for NL queries is a one-line change and is worth a follow-up run before any dogfood window ends.
- **Backfill is the wall.** The service embeds definition kinds at about 10 vectors per second round-robin across every open checkout. The ten corpus repos took about 45 minutes after reindex. Until a workspace's batch publishes, auto stays lexical and explicit hybrid reports unavailable, while `readiness.status` already says ready. The readiness field should not say ready before the vectors are searchable.
- **Every cold index was gone.** All ten corpus indexes were missing from `~/.julie/indexes` at the start of Task 2 even though the registry still listed them. Something between the 6a gate and this run deleted the index directories; the registry rows survived with stale `vector_count` values.
- **Standalone CLI never embeds.** Any evaluation that goes through `julie-server tool …` or `julie-server search …` is lexical-only. The scorecard now has `--service`; the head-to-head already used MCP.
- **Registry hygiene.** The full tier added 36 dead temp rows to the live registry during this branch's gate. Tracked as item 2 in the brief.
- **Dogfood on tmpfs.** `search-quality` copies the 300 MB fixture per test and needs `NEXTEST_TEST_THREADS=6` on the 32 GB tmpfs. Tracked under the test-speed item.

## Follow-ups

- Decide semantic vs hybrid for the auto NL path after one more head-to-head with `--julie-backends auto,semantic`.
- Make `readiness.status` reflect published vectors, not registry counts.
- Find what deleted the corpus index directories.
