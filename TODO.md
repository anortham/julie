# TODO

## Bugs

- [ ] **workspace_init is pre-existing red + pathological** — `tests::core::workspace_init::test_find_workspace_root_rejects_home_julie_dir` fails on both `main` and `feat/test-runner-tiering`, and the `workspace-init` bucket still times out even with a `480s` budget; this currently blocks treating `cargo xtask test system` / `full` as green-by-default (`src/tests/core/workspace_init.rs`)
- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)
- [x] **Common method names can resolve to the wrong callee symbol** — fixed via import-constrained call-edge filtering: resolver now queries identifiers table to check if caller file references candidate's parent type (+200 score boost), dominating all other heuristics. Two extra batch queries per resolve_batch call, short-circuits when no candidates have parent_id. (`src/tools/workspace/indexing/resolver.rs`, `src/database/identifiers.rs`)

## Tech Debt

- [x] **Watcher doesn't respect `.gitignore`** — fixed: replaced hardcoded `glob::Pattern` list with `ignore` crate's `Gitignore` matcher built from root `.gitignore` + `.julieignore` + synthetic patterns. Uses `matched_path_or_any_parents()` for directory-aware matching. `BLACKLISTED_DIRECTORIES` kept as independent safety net. Added `.gradle` and `.dart_tool` to blacklist. (`src/watcher/filtering.rs`, `src/watcher/events.rs`, `src/watcher/mod.rs`)
- [x] **Elixir extractor doesn't extract ExUnit `test` blocks as symbols** — fixed: added `"test"` and `"describe"` arms to `dispatch_call`. `test "description"` extracts as `SymbolKind::Function` with `is_test: true` metadata. `describe "context"` extracts as `SymbolKind::Namespace` with children traversal. Added `extract_first_string_arg` helper. (`crates/julie-extractors/src/elixir/calls.rs`, `crates/julie-extractors/src/elixir/helpers.rs`)
- [x] **Test quality regexes count comments/strings as evidence** — fixed: added `strip_comments_and_strings()` state machine that replaces comment/string contents with spaces before regex scanning. Handles `//`, `#`, `--`, `/* */`, `"..."`, `'...'` with escape support. Also fixed 2 existing tests that were themselves false positives (`should_err` only appeared in comments). (`src/analysis/test_quality.rs`, `src/tests/analysis/test_quality_tests.rs`)
- [x] **PHP/Swift test detection lacks path guard** — fixed: `detect_php` and `detect_swift` now take `file_path` and gate name-prefix detection on `is_test_path()`. PHP `@test` doc annotation still works regardless of path (genuine test marker). (`crates/julie-extractors/src/test_detection.rs`)
- [x] **Line-mode `exclude_tests` has no test coverage** — fixed: added `test_fast_search_line_mode_exclude_tests` that indexes a production file + test file, verifies `exclude_tests: None` returns both, and `exclude_tests: Some(true)` filters the test file. Discovered exclude_tests filtering was already implemented (triple-layered: Tantivy, post-filter, defense-in-depth). (`src/tests/tools/search/line_mode.rs`)
- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas
- [x] **Scala extractor missing `type_usage` identifiers** — already fixed: `extract_identifier_from_node` has a `"type_identifier"` arm that creates `IdentifierKind::TypeUsage` identifiers, with `is_type_declaration_name` filtering and `is_scala_noise_type` noise rejection. (`crates/julie-extractors/src/scala/identifiers.rs`)
- [x] **Code-health regression coverage (4/4 done)** — `get_context` `SignatureOnly` label rendering, compact no-results formatting, `deep_dive` change/security output, and line-mode `exclude_tests` all covered.
- [x] **Broaden and normalize cross-language test detection** — Scala detection broadened from JUnit-only to JUnit + `is_test_path` + `test*` name prefix (2026-03-17). Java/Kotlin/C# exact-annotation matching is correct — those languages use annotations as the canonical test marker. Remaining PHP/Swift concern broken out as separate item above. (`crates/julie-extractors/src/test_detection.rs`)

## Performance

(No open items)

## Ideas from Sentrux (tree-sitter code analysis tool)

Reviewed `/Users/murphy/source/sentrux` on 2026-03-17. Sentrux is a code metrics/analysis tool using tree-sitter with a plugin architecture. Different goals than Julie (shallow metrics vs deep code intelligence), but several ideas worth stealing:

- [ ] **AST-based complexity metrics (cyclomatic + cognitive)** — Sentrux computes CC (1 + branch_nodes + logic_nodes) and cognitive complexity (SonarSource 2016: nesting-weighted branch count) by walking the tree-sitter AST. Per-language config defines which node kinds are `branch_nodes`, `logic_nodes`, `nesting_nodes`. Julie already has the AST — adding CC/CoG to `FuncInfo`/`Symbol` metadata would be cheap and high-value for code quality insights. Reference: `sentrux-core/src/analysis/parser/mod.rs` (complexity counting), plugin.toml `[semantics.complexity]` sections.

- [ ] **Function body hashing for duplication detection** — Sentrux hashes normalized function bodies (`bh: Option<u64>` on `FuncInfo`) to detect near-duplicate functions across a codebase. Normalize whitespace, hash content, compare. Trivial to add during symbol extraction, useful signal for code quality and refactoring recommendations. Reference: `sentrux-core/src/analysis/parser/mod.rs` body hash computation.

- [ ] **Data-driven language config for semantic constants** — Sentrux defines `public_keywords`, `method_parent_kinds`, `test_decorators`, `entry_point_patterns`, `dot_is_module_separator` etc. in per-language TOML files instead of Rust match arms. Julie's core extraction logic rightly stays in Rust (too stateful for .scm queries), but the *constant tables* — which keywords mean public, which node kinds are method parents, which decorators mark tests — could move to config. Would reduce boilerplate across 33 language extractors without touching the extraction logic itself. Reference: `sentrux-core/src/analysis/plugin/profile.rs`, any `plugin.toml`.

- [ ] **Scoped path extraction for implicit imports (Rust)** — Sentrux captures `crate::module::func()` calls and extracts `crate/module` as an implicit import via a `@call.scoped_path` query. In Rust, qualified paths are common and don't appear in `use` statements. Julie's identifier extraction would miss these as import edges. Worth adding to the Rust extractor's relationship/identifier logic. Reference: `sentrux-core/src/queries/rust/tags.scm`.

- [x] **Sound call-edge filtering via import constraint** — implemented as identifier-constrained scoring in the resolver. Instead of Sentrux's hard filter (caller must import callee's file), Julie uses a soft +200 score boost when the caller file's identifiers reference the candidate's parent type. More flexible — works even without explicit imports (same-package, implicit imports). Reference: `src/tools/workspace/indexing/resolver.rs` `ParentReferenceContext`.

### Not worth borrowing

- **Runtime grammar loading (libloading)** — Sentrux dynamically loads .so/.dylib grammar plugins. Adds complexity, fragile on user machines. Julie's compiled-in grammars are simpler and more reliable.
- **Tree-sitter .scm query files for extraction** — Sentrux uses .scm queries because it extracts shallow data (name + location + complexity). Julie's extraction is 70-80% stateful multi-step logic (parent-child linking, two-phase impl processing, doc comment search, relationship inference) that .scm fundamentally cannot express. The manual AST walking approach is correct for Julie's depth of extraction.
- **Content-hash parse caching** — Sentrux caches parse results keyed by SHA256(content+language). Julie uses incremental indexing with MD5 change detection + SQLite persistence, which is a better fit for a persistent index.

## Enhancements

- [ ] **Windows Python launcher versioned probing** — `python_interpreter_candidates()` now lists `py` first on Windows, but doesn't try `py -3.12` / `py -3.13` syntax (the standard way to request a specific Python version via the Windows launcher). These require passing args, not just a binary name, so the current `Vec<OsString>` approach needs rework. (`src/embeddings/sidecar_bootstrap.rs:196-213`)
- [ ] **Verify reference workspace coverage** — Test quality metrics run per-workspace during indexing via `process_files_optimized`, which handles both primary and reference workspaces. Verify with an integration test that indexes a reference workspace and confirms `is_test` metadata and `test_quality` metrics are present. Key files: `src/tools/workspace/indexing/processor.rs`, `src/tests/integration/reference_workspace.rs`

## Review Notes

- 2026-03-15 static review only — findings above come from code/test inspection; runtime verification is still pending.
- Post-indexing analysis order looks sane: reference scores -> test quality -> test coverage -> change risk -> security risk (`src/tools/workspace/indexing/processor.rs`).
- `get_context` batching is a solid improvement and avoids the usual N+1 nonsense (`src/tools/get_context/pipeline.rs`).
- Security sink detection deduplicates evidence across identifiers and relationships before scoring, which is the right shape for this feature (`src/analysis/security_risk.rs`).
- 2026-03-15 bugfix session — validated and fixed 7/7 code bugs, 4 tech debt items from GPT review. GPT's review quality was high — only overclaim was "collapsed identifier buckets" in test coverage.
- 2026-03-16 dogfood pass (primary + `LabHandbookV2`) — `deep_dive` test/risk metadata is already useful, but `get_context` still under-serves test-centric workflows and the current security/callee heuristics can produce misleading output.
- 2026-03-16 bugfix session — validated and fixed 4 more bugs from GPT review: `filter` false positive in sinks, `protected`→`public` mislabel, `get_context` test_quality omission, exact test-name queries hidden by TEST_FILE_PENALTY. All 8 xtask dev buckets green.
- 2026-03-17 dogfood session (Scala/Elixir) — found and fixed: language detection lists not synced (5 files), vendor detection false positive on `lib/`, duplicate Elixir routing arms, Elixir test detection not wired up, Scala test detection JUnit-only. Consolidated all language detection to single source of truth in `crates/julie-extractors/src/language.rs`.
- 2026-03-18 TODO audit — verified all open items against current code. Marked code-health regression tests (3/4) and cross-language test detection as mostly done; broke out remaining gaps (line-mode `exclude_tests`, PHP/Swift path guard) as focused items. Updated Windows launcher description. Reordered tech debt by impact.
