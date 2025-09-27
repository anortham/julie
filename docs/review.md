# Julie Project Review — September 27, 2025

## Overview
- Reviewed indexing, search, editing, and workspace subsystems looking for stubbed code, regressions, and performance issues.
- The findings below focus on behavior gaps that would break advertised features or erode trust in safety guarantees.

## Key Findings

1. **Incremental indexing currently erases symbol data**
   - Locations: `src/extractors/mod.rs:52`, `src/extractors/mod.rs:67`, `src/watcher/mod.rs:320`, `src/watcher/mod.rs:331`
   - `ExtractorManager::extract_symbols` always returns `Ok(Vec::new())`, yet `IncrementalIndexer::handle_file_created_or_modified` treats the result as authoritative. Whenever the watcher sees a change, it deletes the existing rows and writes the empty list back, so the modified file loses all symbols and relationships.
   - Impact: After the first watched edit, the database, Tantivy index, and in-memory caches no longer contain anything for that file; follow-up searches and goto fall back to nothing. This defeats incremental indexing entirely and risks silently erasing workspace data.
   - Recommendation: Either wire `ExtractorManager` to the real language extractors, or guard the watcher so it refuses to overwrite when extraction yields zero symbols for a file that previously had entries. Until then, disable watcher-driven deletions.

2. **Persistent search index is never adopted by the handler**
   - Location: `src/handler.rs:117`
   - When a workspace with a Tantivy index is loaded, the code opens the persistent search (`workspace.search`) but never swaps it into `self.search_engine`; the comment notes a TODO and leaves the in-memory fallback in place.
   - Impact: All fast search calls continue to hit the temporary RAM index, so the persisted Tantivy data is unused and every restart requires re-indexing. Latency and resource numbers advertised in the README are therefore inaccurate.
   - Recommendation: Replace the handler's search engine with the workspace's `Arc<RwLock<SearchEngine>>`, or otherwise delegate every search to `workspace.search` when present.

3. **Workspace indexing only extracts four languages**
   - Location: `src/tools/indexing.rs:392`
   - Even though `detect_language` recognizes dozens of extensions, `extract_symbols_for_language` only dispatches Rust, TypeScript, JavaScript, and Python. Every other file is logged and skipped.
   - Impact: Claims of “26/26 language support” are incorrect, search/indexing coverage is limited to four languages, and tests for other extractors never run under the main indexing flow.
   - Recommendation: Extend the match to call the appropriate extractor for every supported language (or plug the `ExtractorManager` in), and add regression tests so new languages cannot regress silently.

4. **Search-and-replace mode depends on brittle Debug parsing and unbounded filesystem scans**
   - Locations: `src/tools/editing.rs:274`, `src/tools/editing.rs:298`
   - The tool pulls file paths by formatting the `CallToolResult` with `{:?}` and scraping for the `📁` emoji. Any change to the result formatter breaks parsing.
   - If the fast search call produces no hits, the fallback walks the current directory, the system temp directory, `/tmp`, and `/var/folders` recursively looking for matches. There is no workspace boundary, so large machines and shared environments will be scanned, which is both slow and risky.
   - Recommendation: Parse the actual `TextContent` payload instead of the struct Debug output, and restrict the fallback search to the active workspace (or drop the fallback entirely until a bounded implementation exists).

5. **“Semantic” search path never touches embeddings**
   - Locations: `src/tools/search.rs:221`, `src/search/mod.rs:493`
   - `FastSearchTool::semantic_search` simply calls `text_search`, and `SearchEngine::semantic_search` is just a Tantivy query over the existing text fields. No embeddings or vector similarity are used despite the API claiming semantic capability.
   - Impact: Users selecting semantic mode receive identical results with extra latency expectations; the advertised “semantic bridge” is not implemented.
   - Recommendation: Either wire the call through `EmbeddingEngine`/vector store or clearly downgrade the feature flag until semantic search exists.

6. **LineEdit test suite is a placeholder**
   - Location: `src/tests/line_edit_tests.rs:85` and throughout the module
   - Almost every test ends with `assert!(true)` and a TODO, so the suite never validates the `LineEditTool`. Any regression (off-by-one ranges, missing backups, etc.) would still pass.
   - Recommendation: Replace the placeholders with real assertions that exercise each operation; until then these tests should be removed or marked `#[ignore]` to avoid a false sense of coverage.

## Additional Notes
- File watcher embedding updates are currently skipped (`src/watcher/mod.rs:350`) because `EmbeddingEngine` is stored behind `Arc<EmbeddingEngine>`; once incremental indexing is repaired you will still need a concurrency-safe way to refresh embeddings.
