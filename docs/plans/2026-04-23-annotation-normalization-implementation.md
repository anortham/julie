# Annotation Normalization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Implement first-class annotation markers across extraction, SQLite storage, Tantivy search, and test detection.

**Architecture:** `AnnotationMarker` flows from tree-sitter extractors through `Symbol` into a `symbol_annotations` junction table. Search indexes both exact annotation keys and relaxed annotation text, with owner-name context for annotated methods. Test detection consumes one normalized annotation-key slice instead of decorator and attribute buckets.

**Tech Stack:** Rust, tree-sitter, rusqlite, SQLite, Tantivy, cargo nextest, cargo xtask

---

## Execution Notes

Use @razorback:test-driven-development for every task. Workers must run only the narrow test they add or change. The orchestrator runs `cargo xtask test changed` after localized batches and `cargo xtask test dev` once before handoff.

This is a light plan for same-session execution via @razorback:subagent-driven-development. Task 1 is the blocking type-contract task. After Task 1 lands, Tasks 2, 3, 5, and 6 can run in parallel if workers keep the listed write scopes.

## File Structure

- `crates/julie-extractors/src/base/annotations.rs`: shared annotation normalization helper.
- `crates/julie-extractors/src/base/types.rs`: `AnnotationMarker`, `Symbol.annotations`, `SymbolOptions.annotations`.
- `crates/julie-extractors/src/base/creation_methods.rs`: copies annotation options into symbols.
- `crates/julie-extractors/src/test_detection.rs`: normalized-key test detection API.
- `src/database/symbols/annotations.rs`: annotation row write and batch hydration helpers.
- `src/database/schema.rs` and `src/database/migrations.rs`: table and migration wiring.
- `src/search/schema.rs`, `src/search/index.rs`, `src/search/query.rs`, `src/search/projection.rs`: annotation search fields, query parsing, owner context.
- Tiered extractor files listed in Tasks 5 and 6.

## Task 1: Core Marker Contract And Normalization

**Files:**
- Create: `crates/julie-extractors/src/base/annotations.rs`
- Modify: `crates/julie-extractors/src/base/types.rs:41-92`
- Modify: `crates/julie-extractors/src/base/types.rs:466-472`
- Modify: `crates/julie-extractors/src/base/creation_methods.rs:18-67`
- Modify: `crates/julie-extractors/src/base/mod.rs:13-25`
- Modify: `crates/julie-extractors/src/lib.rs:63-71`
- Create: `crates/julie-extractors/src/tests/annotations.rs`
- Modify: `crates/julie-extractors/src/tests/mod.rs:5-43`

**What to build:** Add `AnnotationMarker` and `normalize_annotations()` as the shared contract. The helper returns display values, match keys, raw fragments, and carrier metadata for all 16 annotation-bearing languages.

**Approach:** Put `AnnotationMarker` in `base/types.rs` so `Symbol` and downstream crates share one type. Put normalization functions in `base/annotations.rs`, re-export them through `base/mod.rs` and `lib.rs`. Deduplicate by `annotation_key`, preserve declaration order, preserve source case in `annotation`, and use `carrier = Some("derive")` for Rust derive expansion. Update direct `Symbol` struct literals reported by the compiler with `annotations: Vec::new()`, while extractor paths use `SymbolOptions.annotations`.

**Acceptance criteria:**
- [ ] `normalize_annotations()` covers `@app.route("/api")`, `[Authorize, Route("api")]`, `#[derive(Debug, Clone)]`, `#[tokio::test]`, `org.junit.jupiter.api.Test`, `<TestMethodAttribute>`, `[[nodiscard, likely]]`, `@pragma('vm:prefer-inline')`, and `@moduledoc`.
- [ ] C#, VB.NET, and PowerShell strip `Attribute` only from `annotation_key`.
- [ ] Rust derive rows preserve `carrier = "derive"` and do not emit a standalone `derive` marker.
- [ ] `SymbolOptions::default()` yields an empty annotation vector.
- [ ] Manual `Symbol` constructors compile with explicit empty vectors.
- [ ] Narrow test passes: `cargo nextest run --lib annotation_normalization`.

## Task 2: SQLite Persistence And Hydration

**Files:**
- Create: `src/database/symbols/annotations.rs`
- Modify: `src/database/symbols/mod.rs:1-13`
- Modify: `src/database/schema.rs:9-32`
- Modify: `src/database/schema.rs:122-195`
- Modify: `src/database/migrations.rs:16-121`
- Modify: `src/database/migrations.rs:881-902`
- Modify: `src/database/helpers.rs:11-22`
- Modify: `src/database/helpers.rs:99-175`
- Modify: `src/database/symbols/storage.rs:17-160`
- Modify: `src/database/symbols/search.rs:29-48`
- Modify: `src/database/symbols/queries.rs:169-214`
- Modify: `src/database/bulk_operations.rs:723-760`
- Modify: `src/database/bulk_operations.rs:1187-1221`
- Modify: `src/database/symbols/bulk.rs:94-214`
- Create: `src/tests/core/annotation_storage.rs`
- Modify: `src/tests/mod.rs:30-58`

**What to build:** Create and migrate `symbol_annotations`, write marker rows for every symbol storage path, and batch-hydrate annotations on symbol reads that return full symbol data.

**Approach:** Add migration 020 and bump `LATEST_SCHEMA_VERSION`. Create the table in `initialize_schema()` for new databases. Use `INSERT ... ON CONFLICT(id) DO UPDATE` for symbol writes, then delete and insert annotation rows in the same transaction. Add helper methods that can write marker rows from a transaction or connection without duplicating SQL. Hydrate markers in batch for `get_all_symbols()`, `get_symbols_for_file()`, name queries, and search enrichment paths; leave lightweight reads empty unless a caller asks for full symbols.

**Acceptance criteria:**
- [ ] New databases and migrated databases both have `symbol_annotations` with `annotation_key` and `carrier` indexes.
- [ ] Store, update, and delete operations keep annotation rows in sync with symbols.
- [ ] Re-storing a symbol with no annotations removes previous annotation rows.
- [ ] Batch hydration avoids per-symbol annotation queries.
- [ ] Storage roundtrip preserves order, display value, match key, raw text, and carrier.
- [ ] Narrow test passes: `cargo nextest run --lib annotation_storage`.

## Task 3: Tantivy Annotation Search And Owner Context

**Files:**
- Modify: `src/search/schema.rs:25-143`
- Modify: `src/search/index.rs:35-51`
- Modify: `src/search/index.rs:70-82`
- Modify: `src/search/index.rs:293-312`
- Modify: `src/search/index.rs:368-543`
- Modify: `src/search/query.rs:14-195`
- Modify: `src/search/projection.rs:68-180`
- Modify: `src/search/projection.rs:182-282`
- Modify: `src/tools/search/text_search.rs:234-415`
- Modify: `src/tools/search/text_search.rs:495-553`
- Create: `src/tests/tools/search/annotation_search_tests.rs`
- Modify: `src/tests/tools/search/mod.rs:1-40`

**What to build:** Index annotation keys, relaxed annotation text, and owner names. Route annotation-prefixed definition queries through annotation filters while keeping unprefixed terms on the existing definition path.

**Approach:** Add `annotations_exact`, `annotations_text`, and `owner_names_text` to the Tantivy schema and bump the search compatibility marker. Parse query tokens before normal tokenization: annotation tokens become required `annotation_key` filters, and remaining terms use existing name/signature/doc/body fields plus owner-name text. Annotation-only queries should match annotated symbols without searching symbol names. Disable SQLite high-centrality prepend for annotation-filtered queries so a DB rescue step cannot pollute `@Test` results.

**Acceptance criteria:**
- [ ] `@Test` searches `annotations_exact` and does not search symbol names.
- [ ] `@GetMapping UserController` matches methods owned by `UserController`.
- [ ] `Test` without annotation syntax does not search annotation fields.
- [ ] Native pasted syntax normalizes for query input: `[Authorize]`, `#[tokio::test]`, and `@app.route("/x")`.
- [ ] OR fallback relaxes normal terms while annotation filters remain required.
- [ ] Narrow test passes: `cargo nextest run --lib annotation_search`.

## Task 4: Test Detection API Migration

**Files:**
- Modify: `crates/julie-extractors/src/test_detection.rs:61-251`
- Modify: `crates/julie-extractors/src/bash/functions.rs:27`
- Modify: `crates/julie-extractors/src/c/declarations.rs:173`
- Modify: `crates/julie-extractors/src/cpp/functions.rs:130`
- Modify: `crates/julie-extractors/src/cpp/functions.rs:214`
- Modify: `crates/julie-extractors/src/csharp/members.rs:75`
- Modify: `crates/julie-extractors/src/csharp/members.rs:139`
- Modify: `crates/julie-extractors/src/csharp/members.rs:191`
- Modify: `crates/julie-extractors/src/dart/functions.rs:94`
- Modify: `crates/julie-extractors/src/dart/functions.rs:196`
- Modify: `crates/julie-extractors/src/dart/functions.rs:292`
- Modify: `crates/julie-extractors/src/elixir/calls.rs:106`
- Modify: `crates/julie-extractors/src/gdscript/functions.rs:108`
- Modify: `crates/julie-extractors/src/go/functions.rs:61`
- Modify: `crates/julie-extractors/src/go/functions.rs:164`
- Modify: `crates/julie-extractors/src/java/methods.rs:86`
- Modify: `crates/julie-extractors/src/java/methods.rs:159`
- Modify: `crates/julie-extractors/src/javascript/functions.rs:65`
- Modify: `crates/julie-extractors/src/javascript/functions.rs:139`
- Modify: `crates/julie-extractors/src/kotlin/declarations.rs:111`
- Modify: `crates/julie-extractors/src/kotlin/declarations.rs:193`
- Modify: `crates/julie-extractors/src/lua/functions.rs:98`
- Modify: `crates/julie-extractors/src/php/functions.rs:94`
- Modify: `crates/julie-extractors/src/powershell/functions.rs:39`
- Modify: `crates/julie-extractors/src/python/functions.rs:71`
- Modify: `crates/julie-extractors/src/qml/mod.rs:216`
- Modify: `crates/julie-extractors/src/r/mod.rs:292`
- Modify: `crates/julie-extractors/src/razor/csharp.rs:255`
- Modify: `crates/julie-extractors/src/razor/stubs.rs:144`
- Modify: `crates/julie-extractors/src/ruby/symbols.rs:158`
- Modify: `crates/julie-extractors/src/rust/functions.rs:125`
- Modify: `crates/julie-extractors/src/scala/declarations.rs:92`
- Modify: `crates/julie-extractors/src/sql/routines.rs:47`
- Modify: `crates/julie-extractors/src/swift/callables.rs:76`
- Modify: `crates/julie-extractors/src/typescript/functions.rs:58`
- Modify: `crates/julie-extractors/src/typescript/functions.rs:180`
- Modify: `crates/julie-extractors/src/vbnet/members.rs:40`
- Modify: `crates/julie-extractors/src/vue/script.rs:94`
- Modify: `crates/julie-extractors/src/vue/script_setup.rs:121`
- Modify: `crates/julie-extractors/src/vue/script_setup.rs:201`
- Modify: `crates/julie-extractors/src/zig/functions.rs:40`
- Modify: `crates/julie-extractors/src/tests/test_detection.rs:12-30`
- Modify: `crates/julie-extractors/src/tests/test_detection.rs:41-1806`

**What to build:** Replace decorator and attribute input buckets with a single `annotation_keys: &[String]` argument.

**Approach:** Update `is_test_symbol()` and language-specific detectors to consume normalized keys. For unmigrated call sites, pass an empty slice until the extractor task supplies real markers. Keep existing path and name fallback behavior. Update tests to use lowercased keys such as `test`, `parameterizedtest`, `fact`, `pytest.mark.parametrize`, and `istest`.

**Acceptance criteria:**
- [ ] `is_test_symbol()` has one annotation-key input.
- [ ] Java, Kotlin, Scala, C#, VB.NET, Rust, Python, Dart, and PHP test cases pass through normalized keys.
- [ ] Path-only languages such as Go keep existing behavior.
- [ ] False-positive guards in production paths still pass.
- [ ] Narrow test passes: `cargo nextest run --lib test_detection`.

## Task 5: Tier 1 Extractor Migration

**Files:**
- Modify: `crates/julie-extractors/src/python/decorators.rs:7-53`
- Modify: `crates/julie-extractors/src/python/functions.rs:11-95`
- Modify: `crates/julie-extractors/src/python/types.rs:10-111`
- Modify: `crates/julie-extractors/src/typescript/helpers.rs:17-89`
- Modify: `crates/julie-extractors/src/typescript/classes.rs:76-83`
- Modify: `crates/julie-extractors/src/typescript/functions.rs:58-180`
- Modify: `crates/julie-extractors/src/java/helpers.rs`
- Modify: `crates/julie-extractors/src/java/methods.rs:11-189`
- Modify: `crates/julie-extractors/src/csharp/helpers.rs:8-30`
- Modify: `crates/julie-extractors/src/csharp/members.rs:67-191`
- Modify: `crates/julie-extractors/src/rust/helpers.rs:96-130`
- Modify: `crates/julie-extractors/src/rust/functions.rs:42-149`
- Modify: `crates/julie-extractors/src/kotlin/helpers.rs:1-60`
- Modify: `crates/julie-extractors/src/kotlin/declarations.rs:14-198`
- Test: `crates/julie-extractors/src/tests/python/decorators.rs`
- Test: `crates/julie-extractors/src/tests/typescript/helpers.rs`
- Test: `crates/julie-extractors/src/tests/java/annotation_tests.rs`
- Test: `crates/julie-extractors/src/tests/csharp/metadata.rs`
- Test: `crates/julie-extractors/src/tests/rust/functions.rs`
- Test: `crates/julie-extractors/src/tests/kotlin/mod.rs`

**What to build:** Migrate existing partial extraction paths to the shared marker contract for Python, TypeScript, Java, C#, Rust, and Kotlin.

**Approach:** Extract raw annotation text as close to the AST as possible, feed it to `normalize_annotations()`, pass markers into `SymbolOptions`, and pass marker keys into `is_test_symbol()`. Keep signature rendering unchanged where decorators or attributes are already shown, but stop using signature text as the only persistence path.

**Acceptance criteria:**
- [ ] Python functions and classes preserve `app.route`, `pytest.mark.parametrize`, and class decorators.
- [ ] TypeScript class and method decorators persist without relying on signature text.
- [ ] Java and Kotlin modifier annotations persist on methods and constructors.
- [ ] C# multi-attribute lists expand into separate markers.
- [ ] Rust `#[test]`, `#[tokio::test]`, and `#[derive(Debug, Clone)]` normalize with correct keys and carriers.
- [ ] Narrow tests pass: `cargo nextest run --lib python_decorator`, `cargo nextest run --lib java_annotation`, `cargo nextest run --lib csharp_attribute`, `cargo nextest run --lib rust_test_attribute`, and `cargo nextest run --lib kotlin`.

## Task 6: Tier 2 Extractor Migration

**Files:**
- Modify: `crates/julie-extractors/src/javascript/functions.rs:65-139`
- Modify: `crates/julie-extractors/src/scala/helpers.rs:1-40`
- Modify: `crates/julie-extractors/src/scala/declarations.rs:13-100`
- Modify: `crates/julie-extractors/src/php/functions.rs:10-104`
- Modify: `crates/julie-extractors/src/php/members.rs:21-40`
- Modify: `crates/julie-extractors/src/php/types.rs:19-32`
- Modify: `crates/julie-extractors/src/swift/signatures.rs:17-57`
- Modify: `crates/julie-extractors/src/swift/callables.rs:2-90`
- Modify: `crates/julie-extractors/src/dart/helpers.rs:183-235`
- Modify: `crates/julie-extractors/src/dart/functions.rs:90-201`
- Modify: `crates/julie-extractors/src/cpp/functions.rs:130-214`
- Modify: `crates/julie-extractors/src/vbnet/helpers.rs:130-145`
- Modify: `crates/julie-extractors/src/vbnet/members.rs:37-46`
- Modify: `crates/julie-extractors/src/powershell/helpers.rs:126-169`
- Modify: `crates/julie-extractors/src/powershell/functions.rs:30-264`
- Modify: `crates/julie-extractors/src/gdscript/helpers.rs:8-79`
- Modify: `crates/julie-extractors/src/gdscript/variables.rs:41-60`
- Modify: `crates/julie-extractors/src/elixir/attributes.rs:11-170`
- Modify: `crates/julie-extractors/src/elixir/calls.rs:91-112`
- Test: language test modules under `crates/julie-extractors/src/tests/{javascript,scala,php,swift,dart,cpp,vbnet,powershell,gdscript,elixir}/`

**What to build:** Add or finish annotation marker extraction for the remaining annotation-bearing languages.

**Approach:** Use focused grammar tests for each language before changing implementation. PowerShell must treat `[string]`, `[int]`, and similar type brackets as type annotations, not command attributes. Swift must keep declaration attributes separate from type attributes such as `@unchecked`. PHP namespace display may differ from key matching; preserve display text and match with `annotation_key`. C++ standard attributes such as `[[nodiscard]]` and multi-attribute lists should expand like C# carrier lists.

**Acceptance criteria:**
- [ ] JavaScript decorator syntax reaches markers when the grammar exposes decorator nodes.
- [ ] Scala annotations use Java-like rightmost display and key handling.
- [ ] PHP native attributes preserve namespaced display and usable keys.
- [ ] Swift declaration attributes persist without swallowing type-only attributes.
- [ ] Dart annotations keep `isTest`, `override`, and `pragma` keys.
- [ ] C++ standard attributes expand from `[[X, Y]]`.
- [ ] VB.NET angle-bracket attributes share C# suffix-key behavior.
- [ ] PowerShell command and parameter attributes persist while type brackets stay out.
- [ ] GDScript and Elixir module annotations persist on the symbols they describe.
- [ ] Narrow language tests pass for every changed module.

## Task 7: End-To-End Validation And Dogfood Checks

**Files:**
- Modify: `src/tests/integration/indexing_pipeline.rs`
- Modify: `src/tests/integration/search_regression_tests.rs`
- Modify: `src/tests/tools/filtering_tests.rs`
- Modify: `docs/plans/2026-04-23-annotation-normalization-design.md` only if implementation discoveries require a spec correction

**What to build:** Prove the extraction, storage, search, and test filtering path works through Julie’s normal indexing and CLI-accessible search flow.

**Approach:** Add integration coverage that indexes a mixed-language fixture with annotated symbols, verifies SQLite annotation rows, checks `fast_search("@...")` behavior, and confirms `exclude_tests` excludes annotation-detected tests. Use the new CLI to dogfood at least one standalone `fast_search` query after the binary builds.

**Acceptance criteria:**
- [ ] `fast_search("@app.route", search_target="definitions")` finds a Python route handler.
- [ ] `fast_search("@GetMapping UserController", search_target="definitions")` finds an owned Java method.
- [ ] `fast_search("@Fact", search_target="definitions")` finds C# tests.
- [ ] `fast_search("Test", search_target="definitions")` does not match solely through annotation fields.
- [ ] `exclude_tests` filters annotation-detected tests.
- [ ] `cargo xtask test changed` passes after localized work.
- [ ] `cargo xtask test dev` passes before completion.
- [ ] CLI dogfood command succeeds: `cargo build && ./target/debug/julie-server search "@app.route" --workspace . --standalone --json`.
