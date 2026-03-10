# TODO

## v4.0 Pre-Release Code Review Fixes

### CRITICAL (Must fix before release)

- [x] **C1.** Server binds to `0.0.0.0` — bind to `127.0.0.1` by default (`src/server.rs:174`)
- [x] **C2.** `launch_editor` accepts arbitrary executables — add allowlist (`src/api/projects.rs:519-538`)
- [x] **C3.** Permissive CORS allows cross-origin API access — restrict to localhost (`src/server.rs:172`)
- [x] **C4.** Path traversal in plan IDs — validate IDs reject `..`, `/`, `\`, null bytes (`src/memory/plan.rs:113-116`)
- [x] **C5.** Lock ordering inconsistency → potential deadlock (`src/daemon_state.rs:131-152` vs `src/daemon_indexer.rs`)

### IMPORTANT (Should fix before release)

- [x] **I6.** Concurrent Tantivy writer corruption in memory system — use file lock or singleton (`src/memory/checkpoint.rs:137-150`)
- [x] **I7.** `launch_terminal` has no Windows support (`src/api/projects.rs:562-566`)
- [x] **I8.** `DispatchManager` unbounded memory growth — add LRU eviction (`src/agent/dispatch.rs`)
- [x] **I9.** Agent stderr leaked to API consumers — sanitize errors (`src/agent/backend.rs:128-143`)
- [x] **I10.** `last_processed` HashMap grows unboundedly — periodic eviction (`src/daemon_watcher.rs:265-266`)
- [x] **I11.** Unsafe string slicing on timestamps — use `.get()` with fallback (`src/memory/storage.rs:48`, `src/memory/checkpoint.rs:71`)
- [x] **I12.** Multi-byte truncation panic in embedding text — use `floor_char_boundary` (`src/memory/embedding.rs:57-59`)
- [x] **I13.** Missing centrality boost in federated definition search (`src/tools/federation/search.rs`)
- [x] **I14.** Missing content verification in federated content search (`src/tools/federation/search.rs`)
- [x] **I15.** PID cast truncation — add bounds check (`src/daemon.rs:105,284`)
- [x] **I16.** UNC prefix leaking on Windows — create `display_path()` utility (`src/utils/paths.rs`, `src/api/common.rs`)
- [x] **I17.** Active plan not cleared on complete (`src/memory/plan.rs:235-244`)

### SUGGESTIONS (Nice to have, post-release OK)

- [x] **S1.** `DaemonState` fields are all `pub` — make `pub(crate)` (`daemon_state.rs`)
- [x] **S2.** No retry/reconnect when daemon crashes mid-bridge session (`connect.rs:378`)
- [x] **S3.** SSE parsing is hand-rolled and fragile (`connect.rs:427`)
- [x] **S4.** Test fixtures use `/tmp/project-N` — won't work on Windows (`daemon_indexer_tests.rs`)
- [x] **S5.** `text_search.rs` is 731 lines — 46% over 500-line limit (`text_search.rs`)
- [x] **S6.** Inline tests in `formatting.rs` violate project test organization rules (`navigation/formatting.rs:268`)
- [x] **S7.** `get_symbols` returns error instead of stub for `workspace="all"` (`symbols/mod.rs:90`)
- ~~**S8.** No rate limiting on dispatch endpoint~~ **Dismissed** — server binds to localhost only (C1), single-user daemon, DispatchManager has LRU eviction (I8); rate limiting a local dev tool is over-engineering

---

## Post-v4.1.4 Review Fixes (2026-03-10)

GPT review of v3.9.1→v4.1.4 delta, verified by Claude. 12/13 findings confirmed valid.

### Completed (this session)

- [x] **Restore reference-workspace `fast_refs` parity** — added `limit`, `reference_kind`, and identifier-based refs to reference workspace path (10 new tests)
- [x] **Auto-queue indexing for Registered/Stale on startup** — daemon now sends `IndexRequest` for idle projects after startup (4 new tests)
- [x] **Preserve JSON-RPC request id in connect bridge errors** — `write_jsonrpc_error` now passes through the original request `id`
- [x] **Fix federated refs alphabetical starvation** — global limit now sorts by confidence before truncating, not by project name (5 new tests)
- [x] **Stop line-mode workspace re-resolution** — `line_mode_search` accepts `WorkspaceTarget` directly, no redundant `WorkspaceRegistryService`
- [x] **Fail project registration when registry persistence fails** — `register_project`/`deregister_project` propagate file write errors (2 new tests)
- [x] **UI asset 404 instead of SPA fallback** — missing `.js`/`.css`/etc. return 404; SPA fallback only for navigation routes (8 new tests)
- [x] **PID-file atomic locking** — `fs2::try_lock_exclusive()` eliminates TOCTOU race in `daemon_start` (5 new tests)

### Completed (Windows session 2026-03-10)

- [x] **Fix Windows `stop_service()` self-kill** — replaced `taskkill /IM` (kills all processes by name) with PID-based `daemon_stop()` (`src/install.rs`)
- [x] **Make Windows uninstall robust for active executable** — rename-to-`.old` fallback when delete fails, cleaned up on next install (`src/install.rs`)
- [x] **Fix Windows UNC display-path** — `\\?\UNC\server\share` → `\\server\share` (6 new tests in `src/tests/core/paths.rs`)

### Deferred (design discussion / uncertain)

- [ ] **CORS + unauthenticated destructive endpoints** — `launch_editor`/`launch_terminal` execute system commands with no auth; localhost-only but any local process can trigger via CORS (`src/server.rs`, `src/api/projects.rs`)
- ~~**Embedding provider cross-workspace**~~ **Dismissed** — likely stale post-Tantivy migration; Tantivy already uses per-workspace indexes

---

## Open Items

- [x] **Dashboard memories tab needs project selector** (2026-03-09)
  - Added project selector dropdown with "All projects" default to the memories tab
  - "All" fetches from each project in parallel and merges results by timestamp
  - Dropdown only appears when 2+ projects are registered (single-project setups unaffected)
  - Plans sidebar also project-aware

- [x] **Daemon log location centralized to `~/.julie/logs/`** (2026-03-09)
  - Daemon and connect modes now log to `~/.julie/logs/` (global, since daemon serves multiple projects)
  - Stdio mode still logs to `{workspace}/.julie/logs/` (single-project, backward compatible)
  - Updated CLAUDE.md documentation to reflect new log location

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
1. ~~check goldfish skills~~ **DONE then REMOVED** — memory tools stripped in v4.0
2. [x] **Promoted to 4.0** — Project stats/insights for dashboard project view (language breakdown, symbol counts by kind, index health)
4. [ ] Can the dashboard also talk to a projects repo on gh or devops? what can we build with that?
5. ~~Project registration: auto on startup with julie installed, add from dashboard~~ **DONE** — auto-registration on startup + dashboard registration
6. [ ] Can we tell the agent how to use the dashboard? Can the agent open the browser to a dashboard view as part of a tool call?
7. [ ] With the advanced javascript libs available for things like graphs and diagrams, what code intelligence from julie can we surface visually?
8. ~~filewatcher: I don't think we need a filewatcher running in every project all the time, I'm not sure what the overhead of that would be. we should discuss~~ **DONE** — decided to keep current behavior (OS-native watchers have negligible idle cost), documented
9. ~~we need good documentation of the http api so other tools can integrate~~ **DONE** — OpenAPI 3.0 spec at `/api/docs` via utoipa
10. ~~validate functionality in a parallel scenario like multiple worktrees~~ **DONE** — worktree isolation validated and fixed (Task 13)
11. [ ] We should leverage gh pages to better showcase the dashboard functionality
12. [x] **Promoted to 4.0** — Project view quick-launch (copy path, open in editor, open in terminal)
13. ~~what's our most effective token optimization approach across tools? can we apply that approach to other tools?~~ **DONE** — evaluated, each tool already uses appropriate limiting; no action needed
14. ~~We need to update CLAUDE.md and README.md to properly reflect the big changes that have been made.~~ **DONE** — README.md and CLAUDE.md updated for v4.0.0
15. [x] **Promoted to 4.0** — ORT kept as sidecar fallback + embedding status dashboard card with Check Status button for on-demand initialization
16. [x] **Promoted to 4.0** — Multi-agent dispatch: support Codex, Gemini CLI, Copilot CLI alongside Claude Code
17. [x] **Bug** — Projects page Embeddings column always shows "--". Fixed: added `embedding_count` to `ProjectResponse` by querying `symbol_vectors` directly from the database, bypassing the never-initialized `embedding_runtime_status`.
18. [x] **Windows build** — `cargo build --release` fails if `ui/dist/` doesn't exist. Added `build.rs` that auto-runs `npm install && npm run build` in `ui/`, with stub fallback if Node.js isn't available.


### Multi-Agent Dispatch — CLI Research (2026-03-09)

All four CLI agents support non-interactive/headless mode with structured output.
All support subscription-based auth (OAuth/browser login) — no API key required if user has already logged in.

| Agent | Package | Headless Command | Output Flag | Streaming | Auth |
|-------|---------|-----------------|-------------|-----------|------|
| **Claude Code** | `claude` (npm: `@anthropic-ai/claude-code`) | `claude -p "prompt"` | `--output-format stream-json\|json\|text` | stream-json (JSONL) | OAuth (subscription) or `ANTHROPIC_API_KEY` |
| **Codex** | `codex` (npm: `@openai/codex`) | `codex exec "prompt"` | `--json` (JSONL events) | JSONL events | OAuth via `codex login` (subscription) or API key via `--with-api-key` |
| **Gemini CLI** | `gemini` (npm: `@google/gemini-cli`) | `gemini -p "prompt"` | `--output-format json` | stdout | Google auth (subscription) |
| **Copilot CLI** | `copilot` (npm: `@github/copilot`) | `copilot -p "prompt"` | text to stdout | `--autopilot` | GitHub auth via `gh` (subscription) |

**Key flags for full headless dispatch:**
- **Claude**: `claude -p "prompt" --output-format stream-json --allowedTools Read,Write,Bash --max-turns 10`
- **Codex**: `codex exec "prompt" --json --full-auto` (sets workspace-write sandbox + on-request approvals)
- **Gemini**: `gemini -p "prompt" --output-format json`
- **Copilot**: `copilot -p "prompt" --autopilot --allow-all-tools --max-autopilot-continues 10`

**Codex extra capabilities** (from official docs):
- `--sandbox read-only|workspace-write|danger-full-access` — file access policy
- `--ask-for-approval untrusted|on-request|never` — approval timing
- `--full-auto` — preset: workspace-write + on-request (recommended for headless)
- `--ephemeral` — skip session persistence
- `--output-last-message <path>` — write final response to file
- `--output-schema <path>` — validate response against JSON Schema
- `codex exec resume --last` — resume most recent session
- `codex cloud` — execute tasks on Codex Cloud

**Detection**: `which claude`, `which codex`, `which gemini`, `which copilot`
**Auth check**: `codex login status` (exit 0 = authenticated), `claude auth status`, `gh auth status`

**Design approach (Option B — full):**
- Backend registry with command templates, output parsers, detection logic
- Per-backend streaming output parsing (stream-json, JSONL, plain text)
- Auth detection: check if CLI is authenticated before showing in backend selector
- Status tracking + history with backend labels
- Session resume support (at least for Codex which has built-in `resume`)
- UI: backend selector in dispatch form (already shows discovered backends on Dashboard)

## Pre-4.0 Release Review

- [x] **Fix full test suite flake / order-dependent failure in embedding scheduling test** (2026-03-09)
  - Added `#[serial_test::serial(embedding_env)]` to both scheduling tests to prevent env var pollution from parallel health tests

- [x] **Fix daemon restart + `connect` regression for already-registered projects** (2026-03-09)
  - `load_registered_projects()` now creates MCP services for ALL restored workspaces (Registered, Error, Ready)
  - Previously only the success path created services; now `/mcp/{workspace_id}` is always reachable after restart

- [x] **Make default daemon HTTP MCP endpoint daemon-aware** (2026-03-09)
  - `create_mcp_service()` now accepts `Option<DaemonState>`; production path passes `Some(daemon_state)`
  - Default `/mcp` endpoint now supports federated features (workspace="all", cross-project recall)

- [x] **Fix Agents UI project selector to send workspace IDs, not names** (2026-03-09)
  - Changed `:value="p.name"` → `:value="p.workspace_id"` + filter to ready-only projects

- [x] **Make cross-project memory features work for more than just Ready workspaces** (2026-03-09)
  - Added `resolve_workspace_any()` in common.rs for memory endpoints; Memories UI shows all registered projects

- [x] **Fix dashboard memory stats to reflect multi-project daemon mode correctly** (2026-03-09)
  - Dashboard stats now aggregate memory counts across ALL workspaces, not just the first Ready one

- [x] **Fix Memories page stale plans/active-plan state when switching projects** (2026-03-09)
  - `applyFilters()` now refetches both checkpoints AND plans, clears `activePlanId` before loading

- [x] **Clarify or fix Search debug mode behavior for "All projects"** (2026-03-09)
  - Added visible warning banner when debug mode + "All projects" are combined

- [x] **Fix `get_symbols` target+minimal mode returning empty results for Vue files** (2026-03-09)
  - Root cause: `create_symbol_manual` (Vue extractor) sets `start_byte=0, end_byte=0` — no tree-sitter node available
  - `extract_code_bodies` then sliced `source[0..0]` = empty string, producing `Some("")` code_context
  - Fix: Added line-based fallback in `body_extraction.rs` — when byte offsets are both 0, extracts by `start_line`/`end_line`
  - Fallback is generic: works for ANY extractor that lacks byte offsets, not just Vue
  - TDD: new test `test_vue_target_minimal_extracts_code_body`, 1541 tests pass

- [x] **Show embeddings status on the Projects page** (2026-03-09)
  - Added `EmbeddingStatusResponse` to `ProjectResponse` API with backend, accelerated, degraded_reason
  - Projects table shows new Embeddings column with backend badge, GPU bolt icon, degraded warning

- [x] **Improve mobile/responsive polish in the management UI** (2026-03-09)
  - Nav: icon-only mode at ≤768px (hide text labels and brand name, keep icons)
  - Projects table: `overflow-x: auto` + `min-width: 700px` for horizontal scroll
  - Projects page header + register form: stack at ≤600px
  - Search form: input + button stack at ≤600px
  - Memories filters: full-width stacking at ≤600px (fixed clipped Project dropdown)
  - Main padding: reduced to 1rem at ≤600px
  - Verified at 320px, 375px, and 1280px — no horizontal page scrollbar, no regressions

## 4.0 Release Readiness Review (since `v3.9.1`)

### Fixed

- [x] **R2.** `launch_editor` allowlist bypass — execute validated basename, not user-supplied path (`src/api/projects.rs`)
- [x] **R4.** Stale `Mcp-Session-Id` after daemon restart — clear session on error, always re-capture from headers (`src/connect.rs`)
- [x] **R5.** SSE stream `done` event hardcoded as "completed" — now queries actual dispatch status (`src/api/agents.rs`)
- [x] **R6.** Cross-project recall per-workspace limit=5 — changed to 1000, global limit applied after merge (`src/memory/recall.rs`)
- [x] **R10.** Dispatches recorded as fake "default" project — now resolves actual workspace ID (`src/api/agents.rs`)
- [x] **R11.** Federated `fast_refs` no post-merge truncation — added global limit enforcement (`src/tools/navigation/federated_refs.rs`)

### Dismissed (not bugs / by design / low severity)

- **R3.** PID file reuse — **standard Unix daemon pattern**: connect does health checks, risk is theoretical
- **R8.** Debug endpoint ignores content_type — **real but irrelevant**: debug is a developer diagnostic, not user-facing (post-release)
- **R9.** Re-index TOCTOU race — **real but harmless**: indexing is idempotent, worst case is redundant work (post-release)
