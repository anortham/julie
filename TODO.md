# TODO

## Bugs

- [ ] **F4. Embedding KNN smoke test may be red** — `test_pipeline_knn_works_after_embedding` asserts `authenticate_user` ranks above `DatabaseConnection` for an auth query; needs verification run (`src/tests/integration/embedding_pipeline.rs`)
- [ ] **workspace_init is pre-existing red + pathological** — `tests::core::workspace_init::test_find_workspace_root_rejects_home_julie_dir` fails on both `main` and `feat/test-runner-tiering`, and the `workspace-init` bucket still times out even with a `480s` budget; this currently blocks treating `cargo xtask test system` / `full` as green-by-default (`src/tests/core/workspace_init.rs`)
- [x] **Common method names can resolve to the wrong callee symbol** — fixed via import-constrained call-edge filtering: resolver now queries identifiers table to check if caller file references candidate's parent type (+200 score boost), dominating all other heuristics. Two extra batch queries per resolve_batch call, short-circuits when no candidates have parent_id. (`src/tools/workspace/indexing/resolver.rs`, `src/database/identifiers.rs`)

## Tech Debt

- [ ] **Run embedding benchmark** — baseline vs candidate on `LabHandbookV2` reference workspace, record quality/overhead deltas
- [ ] **Watcher doesn't respect `.gitignore`** — The file walker uses `ignore::WalkBuilder` with `.gitignore` support, but the filewatcher uses hardcoded `glob::Pattern` via `build_ignore_patterns()`. Any `.gitignore` pattern not also in `build_ignore_patterns()` leaks through the watcher. Fix: use `ignore` crate's gitignore parsing at event-filter time, or sync patterns from `.gitignore` at startup. Key files: `src/watcher/filtering.rs`, `src/watcher/events.rs`, `src/utils/walk.rs`
- [ ] **Test quality regexes count comments/strings as evidence** — `analyze_test_body()` scans raw source, so comment text like `// should_err` can incorrectly upgrade a test to `thorough` (`src/analysis/test_quality.rs`, `src/tests/analysis/test_quality_tests.rs`)
- [ ] **Broaden and normalize cross-language test detection** — Java/Kotlin/C# annotation matching is exact-text only, while PHP/Swift `test*` detection is broad enough to risk production false positives (`crates/julie-extractors/src/test_detection.rs`, `crates/julie-extractors/src/java/methods.rs`, `crates/julie-extractors/src/csharp/helpers.rs`)
- [ ] **Elixir extractor doesn't extract ExUnit `test` blocks as symbols** — `test "description" do` is a macro call with target `"test"`, but `dispatch_call` in `calls.rs` sends it to `_ => None`. These are the primary test definitions in Elixir projects. Need to add a `"test"` arm that extracts a Function symbol with `is_test: true` metadata. Currently test detection only works for `def`/`defp` functions inside test directories. (`crates/julie-extractors/src/elixir/calls.rs:37`)
- [x] **Scala extractor missing `type_usage` identifiers** — already fixed: `extract_identifier_from_node` has a `"type_identifier"` arm that creates `IdentifierKind::TypeUsage` identifiers, with `is_type_declaration_name` filtering and `is_scala_noise_type` noise rejection. (`crates/julie-extractors/src/scala/identifiers.rs`)
- [ ] **Add regression coverage for code-health output plumbing** — missing tests for line-mode `exclude_tests`, `get_context` label rendering in `SignatureOnly`, compact no-results formatting, and `deep_dive` change/security output let the current regressions slip through (`src/tests/tools/search/line_mode.rs`, `src/tests/tools/get_context_formatting_tests.rs`, `src/tests/tools/deep_dive_tests.rs`)

## Performance

(No open items)

## Ideas from Sentrux (tree-sitter code analysis tool)

Reviewed `/Users/murphy/source/sentrux` on 2026-03-17. Sentrux is a code metrics/analysis tool using tree-sitter with a plugin architecture. Different goals than Julie (shallow metrics vs deep code intelligence), but several ideas worth stealing:

- [ ] **AST-based complexity metrics (cyclomatic + cognitive)** — Sentrux computes CC (1 + branch_nodes + logic_nodes) and cognitive complexity (SonarSource 2016: nesting-weighted branch count) by walking the tree-sitter AST. Per-language config defines which node kinds are `branch_nodes`, `logic_nodes`, `nesting_nodes`. Julie already has the AST — adding CC/CoG to `FuncInfo`/`Symbol` metadata would be cheap and high-value for code quality insights. Reference: `sentrux-core/src/analysis/parser/mod.rs` (complexity counting), plugin.toml `[semantics.complexity]` sections.

- [ ] **Function body hashing for duplication detection** — Sentrux hashes normalized function bodies (`bh: Option<u64>` on `FuncInfo`) to detect near-duplicate functions across a codebase. Normalize whitespace, hash content, compare. Trivial to add during symbol extraction, useful signal for code quality and refactoring recommendations. Reference: `sentrux-core/src/analysis/parser/mod.rs` body hash computation.

- [ ] **Data-driven language config for semantic constants** — Sentrux defines `public_keywords`, `method_parent_kinds`, `test_decorators`, `entry_point_patterns`, `dot_is_module_separator` etc. in per-language TOML files instead of Rust match arms. Julie's core extraction logic rightly stays in Rust (too stateful for .scm queries), but the *constant tables* — which keywords mean public, which node kinds are method parents, which decorators mark tests — could move to config. Would reduce boilerplate across 31 language extractors without touching the extraction logic itself. Reference: `sentrux-core/src/analysis/plugin/profile.rs`, any `plugin.toml`.

- [ ] **Scoped path extraction for implicit imports (Rust)** — Sentrux captures `crate::module::func()` calls and extracts `crate/module` as an implicit import via a `@call.scoped_path` query. In Rust, qualified paths are common and don't appear in `use` statements. Julie's identifier extraction would miss these as import edges. Worth adding to the Rust extractor's relationship/identifier logic. Reference: `sentrux-core/src/queries/rust/tags.scm`.

- [x] **Sound call-edge filtering via import constraint** — implemented as identifier-constrained scoring in the resolver. Instead of Sentrux's hard filter (caller must import callee's file), Julie uses a soft +200 score boost when the caller file's identifiers reference the candidate's parent type. More flexible — works even without explicit imports (same-package, implicit imports). Reference: `src/tools/workspace/indexing/resolver.rs` `ParentReferenceContext`.

### Not worth borrowing

- **Runtime grammar loading (libloading)** — Sentrux dynamically loads .so/.dylib grammar plugins. Adds complexity, fragile on user machines. Julie's compiled-in grammars are simpler and more reliable.
- **Tree-sitter .scm query files for extraction** — Sentrux uses .scm queries because it extracts shallow data (name + location + complexity). Julie's extraction is 70-80% stateful multi-step logic (parent-child linking, two-phase impl processing, doc comment search, relationship inference) that .scm fundamentally cannot express. The manual AST walking approach is correct for Julie's depth of extraction.
- **Content-hash parse caching** — Sentrux caches parse results keyed by SHA256(content+language). Julie uses incremental indexing with MD5 change detection + SQLite persistence, which is a better fit for a persistent index.

## Enhancements

- [ ] **Windows Python launcher `py -3.12` / `py -3.13` probing** — `python_interpreter_candidates()` doesn't try `py -3.12` syntax, which is the standard way to request a specific version on Windows (`src/embeddings/sidecar_bootstrap.rs:197-208`)
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
