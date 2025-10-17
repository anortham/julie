# TODO

 • Agent Takeaways
   - Codex agent surfaced operational gaps: semantic search ignoring requested workspace (src/tools/search.rs), FTS fallback returning non-actionable pseudo-symbols,
   line-mode search bypassing filters, and missing telemetry.
   - Claude agent emphasized structural search tooling, richer symbol filtering, scoped queries, and enriching result context to boost developer ergonomics.
   - Gemini agent reinforced the need for structured query syntax, improved ranking signals, disambiguation UX, search history, and stronger natural-language handling.
   - Cloud and Qwen agents failed (missing credentials); no input gathered from them.

   Integrated Plan
   - Audit Current Pipeline: Re-run CASCADE readiness checks, text/semantic/hybrid modes, and line-mode behavior to confirm existing guarantees and to reproduce
   Codex-identified edge cases in src/tools/search.rs and src/tools/navigation.rs.
   - Fix Workspace Semantics: Patch semantic search to honor explicit workspace filters before vector lookups, aligning all tiers with single-workspace policy.
   - Harden Fallback Paths: Map FTS fallback hits back onto real symbols with precise ranges, and ensure line-mode respects language/glob filters; add regression tests
   once behavior is corrected.
   - Expand Filtering & Scoping: Extend FastSearchTool to accept symbol kind, visibility, documentation, include/exclude glob lists, and “within symbol” scoping so
   results stay code-specific.
   - Introduce Structural Search: Ship a dedicated structural/relationship search tool leveraging relationships data for call graphs, implementations, and dependency
   exploration; plan phased AST/regex pattern search on stored code_context as follow-up.
   - Elevate Ranking Signals: Incorporate file importance, symbol kind boosts, and recency into ranking, and expose mode-level telemetry (latency, fallback counts,
   confidence) for future tuning.
   - Enhance Result UX: Enrich results with hierarchy breadcrumbs, reference counts, configurable context windows, and mode-aware next actions; add interactive
   disambiguation when multiple definitions surface.
   - Improve Ergonomics & Memory: Provide structured query syntax, optional search history, and quick suggestions drawn from prior queries and workspace metadata;
   document the syntax for users.
   - Natural Language Evolution: Evaluate upgrading semantic tier with better embeddings or fine-tuned models to close Gemini’s NL gap once telemetry validates baseline
   stability.

/e

   # GET_SYMBOLS
   This tool can't help reduce tokens if by default it blows through the hard token limit. 
   We have to figure out a better way to make this tool work. Either with summary modes or paging or idk what.
   We're trying to refactor to get this codebase into more manageable sized files but other user will need this tool to work no matter what conidtion their files are in.


   # MANAGE_WORKSPACE
   I think we still have an issue with reference workspaces being deleted when they shouldn't be. If you look at the workspace registry right now and compare it to the files on disk, you'll see they're missing. I think maybe they are being deleted at startup.

#FAST_SEARCH
   ⏺ julie - fast_search (MCP)(query: "get_symbols", mode: "text", limit: 20)
  ⎿  ⚠ Large MCP response (~14.0k tokens), this can fill up context quickly
  ⎿  {
       "confidence": 1,
       "insights": "Mostly Methods (14 of 20)",
     … +723 lines (ctrl+o to expand)

     we fixed the context issues with fast_search, I think we need to spend some time find tuning our results. The better the results are, the fewer we need to return by default.

# missing implementations
  - look for "coming soon", "TODO", "Stub" , etc



⏺ julie - get_symbols (MCP)(file_path: "src/tools/refactoring/extract_function.rs", max_depth: 1, target: "apply_extraction", include_body: "true", mode:
                           "minimal")
  ⎿  No symbols found in: src/tools/refactoring/extract_function.rs