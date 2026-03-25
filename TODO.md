# TODO

## Open Issues

- [ ] **[S-H4] RRF merge always keeps keyword metadata** -- If symbol appears in both keyword and semantic results, keyword version always kept. Metadata could be stale if enrichment paths diverge. (`hybrid.rs:81-86`)

## Bugs

(No known open bugs.)

## Performance

- [~] **ORT VRAM management for larger models** -- Discovered 2026-03-21 when switching to Jina-code-v2 (768d, ~270MB model) from BGE-small (384d, ~33MB). Partially fixed 2026-03-24: ORT sub-batch size set to 32 in `embed_batch()` to prevent VRAM overflow from sequence padding.
  - **Root cause found (2026-03-24)**: fastembed's `embed(texts, None)` with default batch_size=256 processes ALL texts in one ORT inference call. One long text (up to 8192 tokens for Jina-code-v2) forces padding for every text in the batch, creating tensors that exceed 6GB VRAM. DirectML silently falls back to CPU for oversized batches. Fixed by passing `Some(32)` as the ORT sub-batch size.
  - **Multiple instances are the real risk**: 2+ Claude Code sessions = 2+ Julie processes = 2+ full model loads in VRAM. On a 12GB RTX 4080 Laptop GPU, two Jina-code-v2 instances (~540MB weights alone) plus activation memory during a 250-text batch could push close to limits. BGE-small was small enough this was never a concern.
  - CodeRankEmbed (768d, sidecar) is similarly large. As we move toward larger models as defaults, this needs addressing.
  - **Remaining work**: (a) VRAM query via DirectML/DXGI before loading -- check available VRAM and degrade gracefully. (b) Multi-instance VRAM risk documentation. (c) Consider model-level singleton or shared ORT session across Julie instances (hard -- separate processes).
  - Key files: `src/embeddings/pipeline.rs` (batch size), `src/embeddings/ort_provider.rs` (model init, CPU fallback, sub-batch size), Miller project at `c:\source\miller` has GPU memory detection patterns via WMI.

- [ ] **Upgrade ORT to rc12** -- Current: `ort = "2.0.0-rc.10"` (resolves to rc.11 in lockfile). rc.12 is available. May include DirectML improvements, bug fixes, and new operator support. Update Cargo.toml and test embedding pipeline on both Windows (DirectML) and macOS (CPU/sidecar).

- [ ] **Incomplete embedding backfill not resumed on daemon restart** -- Discovered 2026-03-24. When the daemon is killed mid-embedding (e.g., to stop CPU thrashing), the partial progress (e.g., 1000 of 4858 vectors) is persisted in SQLite, but the remaining symbols are never re-embedded on the next daemon startup. The embedding pipeline only runs during `index` or `refresh` operations, not on session connect. Need either: (a) detect incomplete embedding state on workspace pool init and auto-resume, or (b) add an explicit `manage_workspace(operation="embed")` command, or (c) trigger embedding pipeline on session connect if vectors are below expected count.

## Future Ideas

- [ ] **AST-based complexity metrics** -- Add cyclomatic complexity calculation during AST extraction. Store as symbol metadata. Enables a `/hotspots` skill (complexity x centrality = refactoring targets). Deferred because it requires per-language node-kind mapping across 33 extractors -- needs a language-agnostic approach.
- [ ] **Function body hashing for duplication detection** -- Hash normalized function bodies during extraction to detect near-duplicate functions across a codebase. Low priority -- useful during refactoring but the need arises rarely in practice.
- [ ] **Scoped path extraction for Rust** -- Capture `crate::module::func()` qualified paths as implicit import edges. Currently these don't appear in `use` statements, so the call graph misses callers that use qualified paths. Would improve call graph quality for Rust codebases specifically.
- [ ] **Data-driven language config for semantic constants** -- Move per-language constant tables (public keywords, method parent kinds, test decorators) from Rust match arms to config files. Would reduce boilerplate across 33 extractors without touching extraction logic. Big refactor with regression risk -- future consideration.

## Enhancements

- [ ] **Upgrade to ORT rc.12 and test auto-device on Mac** -- `ort` crate 2.0.0-rc.12 adds `SessionBuilder::with_auto_device` (ONNX Runtime 1.22+) which auto-selects NPU when available. On Apple Silicon, the Neural Engine is an NPU. If this routes to CoreML/ANE without the 13GB memory bloat we saw before, it would give us GPU-class acceleration via ORT natively, eliminating the Mac sidecar dependency for ONNX models. Also ships CUDA 13 builds. Caveat: maintainer says "expect little to no macOS support" after losing Hackintosh VM.
- [ ] **Evaluate CodeRankEmbed ONNX export** -- Track [fastembed issue #587](https://github.com/qdrant/fastembed/issues/587). Once an ONNX export exists, CodeRankEmbed (currently sidecar-only, best quality) could run via ORT natively on all platforms with DirectML/CoreML/CUDA acceleration. This would make it viable as the default model everywhere without requiring the Python sidecar.
- [ ] **Embedding model selection** -- A/B tested 2026-03-21 on LabHandbookV2 (C# + TypeScript + Vue). Jina-code-v2 beats BGE-small on cross-language queries (auth A+ vs B-), BGE wins on English-concept-to-code bridging (rich text B+ vs B-). Overall: Jina-code-v2 is the better default for multi-language codebases. BGE-small viable fallback for single-language or resource-constrained. CodeRankEmbed (768d, nomic-ai) still best overall in benchmarks (+10% namespace, +20% cross-language vs BGE-small) but sidecar-only. Decision: keep Jina-code-v2 as ORT default on Windows, BGE-small elsewhere until CodeRankEmbed gets ONNX export or ORT rc.12 auto-device works on Mac.
- [ ] **Windows Python launcher versioned probing** -- `python_interpreter_candidates()` now lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs:196-213`)
- [ ] **Worktree agent metrics are lost on cleanup** -- Worktree agents spawn their own Julie MCP server instance with a separate `.julie/` directory. When the worktree is cleaned up, those metrics are deleted. Even if the worktree persists, metrics don't merge back (`.julie/` is gitignored). Fix: route metrics writes to the primary workspace's database regardless of which worktree Julie is running in, or consolidate metrics post-merge.
- [ ] **Verify reference workspace coverage** -- Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`
- [ ] **Claude Code plugin distribution** -- Investigated 2026-03-20. Viable via a separate `julie-plugin` repo that bundles pre-built binaries + sidecar + skills. Key findings:
  - **Separate repo required**: Julie's source repo is 33GB; users need only the ~79MB binary, 484KB sidecar, and plugin metadata. The plugin repo is a distribution artifact, not a dev repo.
  - **Binary bundling**: Include all 3 platform binaries (`bin/{platform}/julie-server`) directly in the repo. ~75-100MB total. Use force-push on release to avoid git history bloat.
  - **Launcher script**: `.mcp.json` calls `bash ${CLAUDE_PLUGIN_ROOT}/scripts/launch.sh` which detects platform and `exec`s the right binary. MCP server defined inline in `plugin.json` (like goldfish pattern).
  - **Sidecar bundling**: Include the Python sidecar source (484KB) in the plugin repo. Point `JULIE_EMBEDDING_SIDECAR_SCRIPT` at it via env in the MCP config. Julie's existing `uv` bootstrapping handles venv creation.
  - **Skills bundled**: search-debug, explore-area, call-trace, impact-analysis, type-flow, dependency-graph, logic-flow all ship with the plugin. Manual users would still need to copy skills separately.
  - **Hooks**: SessionStart for auto-recall/indexing, PreCompact for checkpointing, etc.
  - **CI integration**: Extend release.yml to copy binaries + sidecar + skills into `julie-plugin/` and push.
  - **Windows launcher**: Needs `.cmd` or PowerShell equivalent since bash isn't guaranteed. Or rely on Git Bash / WSL.
  - **No PostInstall hooks exist** in Claude Code plugin system (open feature request #9394/#11240). SessionStart can't download binaries because MCP connects before hooks run. Bundling is the only reliable approach.
  - **Manual path unchanged**: Non-Claude-Code users still download binary, add to PATH, register MCP, copy skills. Plugin is additive, not a replacement.
  - Reference: https://code.claude.com/docs/en/plugins, goldfish plugin at ~/source/goldfish as working example
- [ ] **Embedding format versioning** -- When embedding enrichment format changes (e.g., adding field accesses), symbols need re-embedding. Currently requires `force=true` on reindex. Add a format version to the pipeline so changes trigger automatic re-embedding.
- [ ] **Self-improvement skill** -- Julie could identify symbols with high centrality but poor searchability: functions whose names and docs don't overlap with the concepts they implement. Would help developers improve code discoverability.

## Review Notes

### 2026-03-21 LabHandbook Embedding Model Dogfood -- Jina-Code-v2 (ONNX/DirectML, RTX 4080)

Exercised all 8 MCP tools against the LabHandbook V2 codebase (434 files, 7306 symbols, 1471 embedding vectors) using Jina-Code-v2 via ORT on DirectML. 30 tool calls in 4 minutes.

#### Session Performance

| Tool | Calls | Avg Latency | Output |
|---|---|---|---|
| `get_context` | 9 | 120.2ms | 51.0KB |
| `deep_dive` | 3 | 69.6ms | 11.2KB |
| `fast_refs` | 2 | 29.9ms | 2.9KB |
| `rename_symbol` | 1 | 26.3ms | 2.7KB |
| `query_metrics` | 5 | 26.9ms | 6.4KB |
| `fast_search` | 6 | 3.3ms | 6.7KB |
| `get_symbols` | 2 | 0.6ms | 3.4KB |
| `manage_workspace` | 2 | 3.3ms | 1.2KB |

Context efficiency: 372KB examined -> 86KB returned -> **77% context savings**.

#### Semantic Similarity Quality -- get_context (Embedding-Powered)

| Query | Result Quality | Notes |
|---|---|---|
| "authentication and user roles" | **A+** | `IUserService`, `UserService`, `hasRole` composable as pivots. `AuthController`, `UsersController`, router guard as neighbors. Full cross-stack (C# + Vue). |
| "lab test validation rules" | **A** | Found both `LabTestCreateValidator` and `LabTestUpdateValidator` -- FluentValidation classes that `fast_search` completely missed. |
| "full text search indexing" | **A** | `ISearchIndexer`, `SearchIndexer`, `ISearchService` as pivots. `ILuceneIndexManager`, `SearchService`, `SearchController` as neighbors. |
| "calendar event recurrence" | **A** | `RecurrenceService` (iCal logic), `ICalendarEventDto`, `CalendarEventDto`. Neighbors: DTOs, calendar store actions. |
| "error handling and exception mapping to HTTP status codes" | **A** | `ErrorHandlingMiddleware` with full exception->status code mapping + `ApiError` DTO. |
| "migrating legacy data from old database" | **A** | Found `MigrateDocumentsAsync`, `MigrateLabTestsAsync` AND the EF `AddLabTestEntities` migration. Understood "migrating" in both senses. |
| "how does the frontend communicate with the backend API" | **A+** | Best result. `useApi` composable with **46 callers** (every Pinia store action) + `ApiResponse<T>` (C#). Complete API surface map in one call. |
| "storing and retrieving images with thumbnails" | **A** | `ThumbnailService` (SkiaSharp), `IThumbnailService`, `IMediaStorage`. Both storage implementations as neighbors. |
| "content management rich text editing" | **B-** | Weakest. Found `CmsDocumentList.vue` and `RichTextField.vue` but missed `useContentEditor`, `ContentService`, `ContentController`. Embedding didn't connect "rich text editing" to the content block abstraction. |

#### Semantic Similarity Quality -- fast_search (Text-Based, No Embeddings)

| Query | Result Quality | Notes |
|---|---|---|
| `LabTestService` (definition mode) | **A+** | Instant, correct -- class, constructor, interface, controller, tests. |
| "authentication and authorization" | **C+** | Found `Program.cs` import lines with those exact words. Missed `DevAuthHandler`, `RoleClaimsMiddleware`, `AuthController`. |
| "how are lab tests validated before saving" | **D** | Found README, DTOs, TestHelpers. Missed `Validators/` entirely. |
| "search indexing with Lucene" | **D** | Found deployment checklist, missed `SearchIndexer.cs` and `LuceneIndexManager`. |
| "upload file to storage" | **D** | Found README, missed `MediaController`, `DatabaseMediaStorage`, `IMediaStorage`. |
| "paginated list with filtering" | **C** | Scattered results. Found `PaginationGuard` and `IUserService` but missed `LabTestsController.GetAll`. |

#### Key Findings

1. **Embedding model is strong for semantic retrieval.** `get_context` consistently finds the right code for conceptual queries, even across C#/TypeScript/Vue boundaries. The cross-language semantic understanding is the standout capability.
2. **`fast_search` NL queries are the weak spot.** Text tokenization matches words, not concepts. Definition mode is excellent; content mode with natural language queries regularly misses relevant code. This is expected -- `fast_search` is designed for keyword/exact matching, not semantic search. Consider: should `fast_search` content mode fall back to embedding similarity when text results score below a threshold?
3. **One soft miss in embeddings**: "content management rich text editing" found Vue components but missed the core content subsystem. The embedding vectors for `useContentEditor`, `ContentService`, and `ContentController` may not strongly associate with "rich text editing" -- worth investigating whether the symbol names or their code bodies drive the embedding and whether content-domain symbols need richer context in their embedding input.
4. **Risk metrics are actionable.** `DatabaseMediaStorage.DeleteAsync` (untested file deletion) correctly flagged as highest security risk. `useApi` (141 refs, untested) correctly flagged as highest centrality risk. Interfaces ranking HIGH for change risk correctly reflects cascade impact.
5. **Performance is excellent.** GPU-accelerated ONNX on DirectML keeps `get_context` under 150ms average despite embedding similarity across 1471 vectors + graph traversal.

#### Action Items

- [ ] **Investigate `fast_search` NL fallback** -- When text search returns low-confidence results for natural language queries, consider falling back to or blending with embedding similarity. The quality gap between `fast_search` and `get_context` for NL queries is significant.
- [ ] **Debug "rich text editing" embedding miss** -- Investigate why `ContentService`, `useContentEditor`, and `ContentController` didn't rank for "content management rich text editing". Check what text is fed to the embedding model for these symbols -- if it's just the symbol name + signature without code body context, the semantic connection to "rich text" may be too weak.
- [ ] **Cross-language embedding quality looks great** -- The "frontend communicates with backend" query producing a complete API surface map across C# and TypeScript validates that Jina-Code-v2 handles multi-language semantic similarity well. No action needed, just confirmation.

### 2026-03-21 LabHandbook Embedding Model Comparison -- BGE-small vs Jina-Code-v2

Reran the same 9 `get_context` queries from the Jina-Code-v2 dogfood against BGE-small-en-v1.5 (384d) to compare semantic similarity quality. Same codebase (434 files, 7306 symbols, 1471 vectors), same ORT/DirectML backend.

#### Performance Comparison

| Metric | Jina-Code-v2 | BGE-small | Delta |
|---|---|---|---|
| `get_context` avg latency | 120.2ms | 70.3ms | **-42% (BGE faster)** |
| Source examined | 372KB | 149KB | **-60% (BGE tighter)** |
| Output returned | 86KB | 52KB | **-40% (BGE smaller)** |

#### Query-by-Query Comparison

| Query | Jina-Code | BGE-small | Verdict | Detail |
|---|---|---|---|---|
| "authentication and user roles" | **A+** | **B-** | **REGRESSION** | Jina: `IUserService`, `UserService`, `hasRole` -- full cross-stack (C# + Vue). BGE: `fetchUsers`, `UserDto`, `assignRole` -- frontend admin store only. Lost all backend auth infrastructure. |
| "lab test validation rules" | **A** | **A** | **SAME** | Both: `LabTestCreateValidator` + `LabTestUpdateValidator` |
| "full text search indexing" | **A** | **A-** | **SLIGHT REGRESSION** | Jina: 3 pivots incl. `ISearchService`. BGE: 2 pivots -- dropped search service interface. Core indexing code still found. |
| "calendar event recurrence" | **A** | **A** | **LATERAL** | Jina: `RecurrenceService`, `ICalendarEventDto`, `CalendarEventDto`. BGE: `RecurrenceService`, `CalendarEventCreateDto`, `CalendarEventUpdateDto`. Different DTOs, equally relevant. |
| "error handling and exception mapping to HTTP status codes" | **A** | **A** | **SAME** | Identical pivots: `ErrorHandlingMiddleware` + `ApiError` |
| "migrating legacy data from old database" | **A** | **A** | **LATERAL** | Jina found EF migration `AddLabTestEntities`. BGE found `MigrateUrlsAsync` -- all 3 data migration methods surfaced but lost the schema migration angle. |
| "how does the frontend communicate with the backend API" | **A+** | **A+** | **SAME** | Identical: `useApi` (46 callers) + `ApiResponse<T>`. Complete API surface map. |
| "storing and retrieving images with thumbnails" | **A** | **A** | **SAME** | Identical: `ThumbnailService`, `IThumbnailService`, `IMediaStorage`. Same neighbors. |
| "content management rich text editing" | **B-** | **B+** | **IMPROVEMENT** | Jina: `editingLinkId`, `RichTextField`, `isEditing` + 0 neighbors. BGE: `editingLinkId`, **`ContentBlockDto`**, `RichTextField` + **6 neighbors** (`getBlock`, `saveBlock`, `fetchBlock`, `fetchSection`, `blocks`). Found the content data model and store. |

#### Recommendation

- [ ] **Keep Jina-Code-v2 as default for multi-language codebases.** Cross-stack semantic understanding is the differentiating capability.
- [ ] **Investigate Jina-Code weakness on "content management rich text editing"** -- BGE found `ContentBlockDto` and content store neighbors that Jina missed. This may be addressable by enriching the embedding input for content-domain symbols rather than switching models.
- [ ] **BGE-small remains a viable fallback** for single-language codebases or resource-constrained environments where the 42% latency improvement matters.

### 2026-03-21 Dogfood Issue: Too Many Queries for Simple Question

**Question asked:** "What triggers metrics to be saved to the DB?"

**Expected:** 1-2 queries max. **Actual:** 6 tool calls + 1 file read.

**Root cause:** Vocabulary mismatch. User thinks "save to database" but code says "record_tool_call" / "insert_tool_call". Field access enrichment bridged "metrics" and "database" but "save" and "persist" remain unmatched. This is a code naming issue, not a search algorithm bug.

**Action items:**
- [ ] **Consider: should `get_context` boost functions that call DB write operations?** -- Functions containing `INSERT`, `execute`, `conn.execute` etc. could get a "write path" signal that helps queries about persistence.
- [ ] **Document recommended query patterns** -- The MCP instructions tell agents to use `get_context` for orientation, but don't guide them on query formulation.

### Historical Notes

- 2026-03-15 static review only -- findings above come from code/test inspection; runtime verification is still pending.
- Post-indexing analysis order looks sane: reference scores -> test quality -> test coverage -> change risk -> security risk (`src/tools/workspace/indexing/processor.rs`).
- `get_context` batching is a solid improvement and avoids the usual N+1 nonsense (`src/tools/get_context/pipeline.rs`).
- Security sink detection deduplicates evidence across identifiers and relationships before scoring, which is the right shape for this feature (`src/analysis/security_risk.rs`).
- 2026-03-15 bugfix session -- validated and fixed 7/7 code bugs, 4 tech debt items from GPT review.
- 2026-03-16 dogfood pass (primary + `LabHandbookV2`) -- `deep_dive` test/risk metadata is already useful, but `get_context` still under-serves test-centric workflows.
- 2026-03-16 bugfix session -- validated and fixed 4 more bugs from GPT review. All 8 xtask dev buckets green.
- 2026-03-17 dogfood session (Scala/Elixir) -- found and fixed language detection sync, vendor detection, Elixir routing, test detection issues. Consolidated language detection to single source of truth.
- 2026-03-18 watcher `.gitignore` support -- replaced hardcoded glob patterns with `ignore` crate's `Gitignore` matcher.
- 2026-03-18 added `query_metrics` MCP tool and 3 report skills (`/codehealth`, `/security-audit`, `/architecture`). Skills leverage existing analysis data via the new metadata query tool.
- 2026-03-18 codehealth-driven test coverage -- 96 new tests targeting the highest-risk untested code identified by `/codehealth`: extractor critical path (`get_node_text`, `create_symbol`, `create_identifier`, `find_containing_symbol`, `find_doc_comment`), test detection dispatch (`is_test_symbol`), database write paths (`incremental_update_atomic`, `bulk_store_types`), and type conversion (`convert_types_map`).
