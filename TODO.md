# TODO

## Open Items

- [ ] **Daemon log location is confusing for multi-project setups**
  - All daemon logs go to the primary workspace's `.julie/logs/` — users expect logs in each project's `.julie/logs/`
  - Options: (a) centralize logs under `~/.julie/logs/` (daemon-level, not project-level), (b) symlink per-project, (c) per-project log files in each `.julie/logs/` with routing by workspace_id
  - Related: CLAUDE.md documents the log location but only for the primary workspace

- [ ] **Re-evaluate variable kind exclusion from embeddings** (implementation landed, benchmark run pending)
  - [x] Implemented budgeted variable embedding policy (deterministic ranking + `20%` cap)
  - [x] Added stale variable vector cleanup and full/incremental policy parity
  - [x] Added dogfood metrics scaffold (`Hit@k`, `MRR@10`, `OffTopic@5`, `CrossLangRecall@5`) + JSONL fixture loader
  - [ ] Run baseline vs candidate benchmark on `LabHandbookV2` reference workspace and record quality/overhead deltas

## Recently Completed

- [x] **Cross-file inheritance extraction ported to 5 languages** (2026-03-01)
  - Java, TypeScript, JavaScript, Kotlin, Swift now create `PendingRelationship` when base types aren't found in local symbols
  - TypeScript also gained `implements_clause` support (was only handling `extends_clause`)
  - Two-phase borrow pattern (collect data → create relationships) applied to each language
  - 10 new tests across all 5 languages, 1347 extractors tests pass
  - Java test module was missing `cross_file_relationships` registration in `mod.rs` — fixed

- [x] **`fast_refs` import classification fix** (2026-03-01)
  - `format_lean_refs_results` now partitions definitions into real definitions vs imports
  - Imports shown in separate "Imports" section between Definitions and References
  - 2 new tests, all formatting tests pass


- [x] **NL query recall for C# classes with indirect naming** (2026-03-01)
  - Verified FIXED by the combination of: (1) cross-file inheritance via PendingRelationship, (2) centrality propagation from interfaces to implementations, (3) semantic search via hybrid_search
  - `get_context("Lucene search implementation")` on coa-codesearch-mcp now returns `SearchAsync` from `LuceneIndexService.cs` as pivot (ref_score: 24)
  - `get_context("LuceneIndexService")` returns both `ILuceneIndexService` (ref_score: 15) and `LuceneIndexService` (ref_score: 12) as pivots
  - `TextSearchTool` no longer invisible — appears as pivot for "text search service" (ref_score: 2)
  - `get_context("circuit breaker error handling")` correctly navigates DI graph to find `LuceneIndexService` + `ICircuitBreakerService`

- [x] **Token optimization across tools: evaluated, no action needed** (2026-03-01)
  - `TokenEstimator` is already a shared utility in `src/utils/token_estimation.rs`, used by get_context, progressive_reduction, and workspace listing
  - `truncate_to_token_budget` is get_context-specific (pivots/neighbors/summaries allocation) — not generalizable
  - Other tools have natural limiting mechanisms: deep_dive has depth levels (~200/600/1500 tokens), get_symbols has mode-based truncation, fast_search has line limits
  - No concrete problems found — design is appropriate as-is

- [x] **C# centrality propagation: interface → implementation** (2026-02-28)
  - Added Step 2 to `compute_reference_scores()`: propagates 70% of interface/base class centrality to implementations
  - `LuceneIndexService`: 0 → 9.1 (from `ILuceneIndexService` ref=13), `PathResolutionService`: 0 → 21.7 (from `IPathResolutionService` ref=31)
  - TDD: 2 new tests for propagation behavior, all existing tests pass

- [x] **C# cross-file inheritance extraction via PendingRelationship** (2026-02-28)
  - `extract_inheritance_relationships` only resolved against same-file symbols, silently dropping cross-file interfaces (nearly ALL C# inheritance)
  - Added `else` branch creating `PendingRelationship` with `is_interface_name()` heuristic for Implements vs Extends
  - Phase 1/Phase 2 restructure to satisfy borrow checker (collect data → create relationships)
  - coa relationships: 2,067 → 2,088 (+21 new cross-file inheritance)
  - TDD: 3 new tests (cross-file interface, cross-file base class, same-file still works)

- [x] **`get_context` NL path prior made language-agnostic** (2026-02-28)
  - `is_test_path` now handles C# `.Tests` dirs, Go `_test.go`, JS/TS `.test.ts`/`.spec.ts`, Python `test_*.py`, Ruby `spec/`, generic `test`/`tests`/`__tests__` segments
  - `is_docs_path` and `is_fixture_path` similarly generic
  - Tests cover C#, Python, Java, Go, JS/TS, Ruby project layouts
  - Fixed: coa NL queries no longer return test files/stubs; Program.cs no longer gravitates as pivot

- [x] **`get_context` Program.cs gravity eliminated** (2026-02-28)
  - Verified: "how does text search work", "Lucene search implementation", "circuit breaker error handling", "symbol extraction pipeline" — none return Program.cs as pivot
  - Root cause was NL path prior being a no-op on C# layouts (now fixed)

- [x] **`get_context` off-topic `estimate_words` for "search scoring"** (2026-02-28)
  - "how does search scoring work" now returns `calculate_score` (path_relevance) and `calculate_search_confidence` (search scoring) — both relevant

- [x] **`get_context` empty Lua test pivot for "symbol extraction pipeline"** (2026-02-28)
  - Now returns `spawn_workspace_embedding`, `extract_symbols`, `process_files_optimized` — all relevant, no empty tests

- [x] **`get_context` compact format dumping massive code bodies** (2026-02-28)
  - Was caused by Program.cs gravity (fixed) — low-value pivots no longer consume entire token budget

- [x] **Definition search over-fetch + kind-aware promotion** (2026-02-28)
  - Over-fetch floor bumped from 50 to 200; three-tier promotion (definition kinds → non-definition → rest)
  - Removed premature `.take(limit)` truncation before promotion in file_pattern code path
  - `LuceneIndexService` definition search with limit=5 now promotes correctly
  - `EmbeddingManager` on miller now shows class definition as first result

- [x] **Deduplicated `is_nl_like_query`** (2026-02-28)
  - Deleted weaker private copy in `expansion.rs`, replaced with import of canonical version from `scoring.rs`

- [x] **C# field/property type relationships** (2026-02-28)
  - Added `field_declaration` and `property_declaration` relationship extraction
  - Shared helpers: `extract_type_name_from_node`, `find_containing_class`
  - 7 new tests, all passing

- [x] **File delete events**: Handle when a delete event occurs in the filewatcher but the file isn't deleted, just edited (atomic save pattern). Fixed with `path.exists()` guard + `should_process_deletion()` for real deletions. (2026-02-28)
- [x] **Filewatcher validation**: Validated watcher keeps index fresh with Tantivy + embeddings sidecar. Incremental pipeline confirmed (52 new / 19,899 skipped on restart). (2026-02-28)
- [x] **CPU usage at idle**: Fixed — 0.0% CPU at idle with new sidecar setup. (2026-02-28)
- [x] **Search-layer relevance for natural-language queries**: Shipped deterministic NL query expansion (original/alias/normalized groups), weighted query builders, and conservative NL-only `src/` path prior with regression coverage for identifier-query stability.

**Post Platform Tasks**
1. ~~check goldfish skills, we'll need a refocused version of those for the memory tools. Also check goldfish server instructions and tool descriptions, they are effective at agent adoption and we need that too.~~ **DONE** — Julie plugin shipped with 5 skills, 3 hooks, enhanced tool descriptions
2. [ ] *(Deferred to 4.1)* what kind of project stats/insights make sense for the project view in the dashboard? git status? index stats? language stats? dependencies? test counts?
3. [ ] *(Deferred to 4.1)* with tantivy and embeddings available to memories now, what advanced memory features does that open up? Could we link memories to code/commits?
4. [ ] *(Deferred to 4.1)* Can the dashboard also talk to a projects repo on gh or devops? what can we build with that?
5. ~~Project registration: auto on startup with julie installed, add from dashboard~~ **DONE** — auto-registration on startup + dashboard registration
6. [ ] *(Deferred to 4.1)* Can we tell the agent how to use the dashboard? Can the agent open the browser to a dashboard view as part of a tool call?
7. [ ] *(Deferred to 4.1)* With the advanced javascript libs available for things like graphs and diagrams, what code intelligence from julie can we surface visually?
8. ~~filewatcher: I don't think we need a filewatcher running in every project all the time, I'm not sure what the overhead of that would be. we should discuss~~ **DONE** — decided to keep current behavior (OS-native watchers have negligible idle cost), documented
9. ~~we need good documentation of the http api so other tools can integrate~~ **DONE** — OpenAPI 3.0 spec at `/api/docs` via utoipa
10. ~~validate functionality in a parallel scenario like multiple worktrees~~ **DONE** — worktree isolation validated and fixed (Task 13)
11. [ ] *(Deferred to 4.1)* we should leverage gh pages to better showcase the dashboard functionality
12. [ ] *(Deferred to 4.1)* project view in dashboard should have info to help quickly get into a project in a devs preferred tools
13. ~~what's our most effective token optimization approach across tools? can we apply that approach to other tools?~~ **DONE** — evaluated, each tool already uses appropriate limiting; no action needed
14. ~~We need to update CLAUDE.md and README.md to properly reflect the big changes that have been made.~~ **IN PROGRESS** — CLAUDE.md updated for v4.0.0
