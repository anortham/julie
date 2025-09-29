# Julie Project Review — Codex Agent Summary

## Collected Findings

### Critical Breakages
- Incremental indexing writes empty symbol lists back to storage, erasing metadata after file changes (`src/extractors/mod.rs`, `src/watcher/mod.rs`).
- `JulieServerHandler::active_search_engine` panics when no workspace is active yet `fast_search` still calls it (`src/handler.rs`, `src/tools/search.rs`).
- Semantic search depends on downloading FastEmbed models at runtime and keeps vectors only in memory; failures silently downgrade to text heuristics (`src/handler.rs`, `src/embeddings/mod.rs`, `src/embeddings/vector_store.rs`).

### Missing / Placeholder Features
- `FindLogicTool` still returns a "coming soon" banner and `SmartRefactorTool` uses `String::replace`, so the promised AST-aware refactoring does not exist (`src/tools/exploration.rs`, `src/tools/refactoring.rs`).
- Editing tools advertise zero-corruption safeguards but lack real backup/rollback flows; multi-file edits parse emoji output and fall back to full-workspace scans (`src/tools/editing.rs`).
- Several TDD suites are placeholders (`assert!(true)` or empty tests), so regressions slip through (`src/tests/line_edit_tests.rs`, `src/tests/watcher_tests.rs`, `src/tests/editing_tests.rs`).

### Platform & UX Risks
- Tantivy persistence is never adopted by the handler, so every restart reindexes and advertised latency numbers are unverified.
- Large files and outstanding compiler warnings persist (see `TODO.md`), reducing maintainability and professionalism.
- Tool output exceeds token budgets (observed 149k-token `fast_search` response) because pagination and summarization are missing.

## Consolidated Improvement Plan
1. **Indexing Integrity**: Wire real extractor outputs into incremental indexing, guard against zero-symbol overwrites, and refresh relationships with symbols.
2. **Persistent Engines**: Swap handler search to the workspace Tantivy index when ready and persist embedding vectors/ANN structures so cold starts meet latency claims.
3. **Finish Core Tools**: Implement true logic discovery and AST-aware rename/extract operations; add the missing `search_and_replace` facade and other high-value investigation tools.
4. **Editing Safety**: Build transactional backups and restore paths for both single- and multi-file edits before advertising "zero corruption" guarantees.
5. **Token Guardrails**: Add strict limits, summary/detail modes, and pagination to every high-volume tool response, borrowing patterns from `coa-mcp-framework` and `coa-codesearch-mcp`.
6. **TDD Enforcement**: Replace placeholder tests with SOURCE/CONTROL suites that hit MCP entry points, add integration coverage for result quality, and keep tests failing until features ship.
7. **Runtime Resilience**: Make search degrade gracefully when no workspace is loaded, preload/cache embeddings with clear error messaging, and resolve compiler warnings plus oversized modules noted in `TODO.md`.
8. **Expectation Reset**: Update README claims once metrics, persistence, and safety nets are proven; until then, align marketing with current capabilities.

## Capability Assessment
- **Can Julie hit its stated goals today?** No. Missing persistence, incomplete tools, and fragile tests keep the project below the advertised "production-ready" bar.
- **What is required to get there?** The eight steps above are the minimum to close the gap: durable storage, completed tooling, rigorous guardrails, and validated tests/metrics.
- **Would I adopt Julie now?** Not yet. The architecture is promising, but I would wait for the plan items to land and for results to be confirmed by automated tests and performance data.

_Document authored by Code (GPT-5 Codex) on 2025-09-29._
