# Test-Role Enrichment: persist rich test classification into the extract

> **For agentic workers:** REQUIRED SUB-SKILL: use `razorback:subagent-driven-development` when delegation is available, else `razorback:executing-plans`. Rigid TDD — test first, every phase ends green. Use Julie's own MCP tools (`get_symbols`, `deep_dive`, `fast_refs`) to navigate before editing; **do not guess tree-sitter node names** — verify each against the grammar `node-types.json` or an `extract` probe.

> **Sibling plan:** `2026-05-29-extraction-enrichments-for-miller-bridge.md` (the `type_arguments` / `literals` / annotations batch). This plan SHARES that batch's contract epoch — read its "Critical shared knowledge: the three version dials" and "Contract coordination with Miller" sections; they govern this plan's release timing too. Both land as one `EXTRACT_CONTRACT_VERSION = 2` epoch.

**Goal:** make julie's *already-computed* rich test classification visible to external consumers (Miller, eros) by persisting it into the extract DB's `symbols.metadata`, and extend test detection so it is **cross-language complete** — every language with a real test framework is covered, not just the convenient ones. Today the extract carries a flat `is_test: true` (callable-only). Julie *internally* computes a far richer `TestRole` (`test_case` / `parameterized_test` / `fixture_setup` / `fixture_teardown` / `test_container`) but **never writes it to the extract** — so the read-only consumer can't see it. Richer persisted data ⇒ more the consumer can do (distinguish a fixture from a case, hide whole test containers, map suites, score only scorable tests).

**Why (the consumer):** Miller (`~/source/codesearch`, .NET, read-only SQLite consumer of `julie-server extract`) implements `exclude_tests` and will grow test-aware features (suite navigation, "show me the fixtures", coverage mapping). It can only use what julie persists. Today Miller must OR julie's `is_test` with a lossy language-agnostic *path* heuristic to catch the test **classes** and helper symbols julie's symbol-level detection misses. Persisting `test_role` (a) removes most of that fallback, (b) gives Miller/eros the case-vs-fixture-vs-container distinction for free across all languages. eros (commercial successor) wants the same signal at higher fidelity.

**Architecture:** This is mostly **wiring + cross-language coverage**, not new machinery. The classifier, the metadata-writing, and the per-language TOML role configs already exist and are battle-tested on julie's live daemon path. The work is: run it on the path Miller reads (and every other persistence path), then fill the language gaps.

**Tech Stack:** Rust 2024, `julie-extractors` hand-written extractors, the existing `src/analysis/test_roles.rs` classifier + language TOMLs, rusqlite/WAL, `cargo nextest`/`cargo xtask`, the `src/tests` hierarchy.

**Architecture Quality:** Low risk for Phase 1 (pure wiring of existing, tested code into more call sites — the risk is the *multi-path* completeness trap, not the logic). Medium for Phases 2–3 (new per-language detection + call-style capture; risk is grammar-node correctness, mitigated by mandatory AST verification + TDD). No schema migration (the `metadata` column already exists). Additive metadata keys only.

---

## The core finding (verified against source + a live v7.12.2 extract)

| Fact | Evidence | Consequence |
|---|---|---|
| Rich classifier **exists** and already writes `test_role` + `is_test` into `symbol.metadata` | `src/analysis/test_roles.rs:125` `classify_symbols_by_role` (sets `metadata["test_role"]` line 134, `is_test` line 137) | The richness machinery is done — it just isn't run on the consumer's path. |
| It runs on **exactly one** path | `classify_symbols_by_role` called **only** at `src/tools/workspace/indexing/pipeline.rs:54` (grep across `src`/`crates` finds no other non-test caller) | Live daemon/MCP indexing gets `test_role`. **Nothing else does.** |
| The **extract-CLI path does NOT classify** | `src/indexing_core/persistence.rs` (`persist_force_rebuild`/`persist_incremental_scan`/`persist_single_file_replace`) receive an already-built `&ExtractedBatch` and call `canonical_write_set()` — no classify step; grep on `src/indexing_core` + `src/external_extract` for `classify_symbols_by_role`/`test_role` is **empty** | **This is the root cause.** Miller's extract DB has `is_test` (from per-extractor `is_test_symbol`) but **no `test_role`**. Confirmed by probe: a `[Fact]` method extracts as `{"is_test":true}` — no `test_role`. |
| The **watcher path does NOT classify** | grep `src/watcher` for `classify_symbols_by_role`/`test_role` is empty | Even julie's own single-file watcher updates miss `test_role` — the **multi-path trap** (sibling plan Rule 2). |
| The **TOMLs already define the rich classes** | `languages/csharp.toml:65 test_container = ["testfixture","testclass","collection","collectiondefinition"]`; `csharp.toml:63 fixture_setup = [...]`; `java.toml`/`kotlin.toml`/`python.toml` similar | Flagging `[TestClass]`/`[TestFixture]`/`@Nested` as containers is **already configured** — it only needs the classifier to run on the extract path. |
| **Class-level annotations are now populated** (sibling P1 done) | `crates/julie-extractors/src/csharp/types.rs:149` `extract_class` calls `helpers::extract_annotations`; same at interface/struct/enum/record/property/delegate (209/258/300/344) | The annotation-based container dependency (B1) is **satisfied for C#**. Verify it's on the branch you build from. |
| `is_test_symbol` gates to **callables only** | `crates/julie-extractors/src/test_detection.rs:70` `is_callable` (Function/Method/Constructor) → classes/structs return `false` | Per-extractor `is_test` can never flag a container. Containers come **only** from `classify_test_role` (which has `is_container_kind`, `test_roles.rs:51`). Don't touch the `is_test_symbol` gate. |
| Call-style capture exists but is **JS/TS-only** | `crates/julie-extractors/src/test_calls.rs` (`TEST_BLOCKS`/`CONTAINER_BLOCKS`/`LIFECYCLE_BLOCKS`); callers only in `typescript/`+`javascript/` | Jest/Vitest/Mocha `describe`/`it` already produce synthetic symbols with `is_test`/`test_container`. **Dart `test()`/`group()`, R `test_that()`, Catch2 `TEST_CASE` are NOT captured.** |
| ~7 test-capable languages fall to the **weak generic fallback** | `test_detection.rs:89` `_ => detect_generic` (name starts `test_`/`Test` + test path). Match arms cover rust/python/java/kotlin/scala/elixir/csharp/vbnet/razor/go/js/ts/php/bash/powershell/ruby/swift/dart only | **c, cpp, gdscript, lua, qml, r, zig** get only the weak heuristic — they miss their real frameworks. |

**Net:** Phase 1 (wiring) is the high-ROI centerpiece and unblocks `test_role` + class-container `is_test` for C#/Java/Kotlin immediately. Phases 2–3 are additive cross-language coverage.

---

## Contract coordination (read before shipping)

Adding `test_role` (and newly setting `is_test` on test *containers*) changes "the meaning/shape/population of data the external reader consumes" → it is an `EXTRACT_CONTRACT_VERSION` concern (sibling plan's dial table). It is **metadata-only — no schema migration** (the `symbols.metadata TEXT` column already exists; verified in a live extract). Therefore:

- **Ride the sibling batch's single `1 → 2` contract epoch.** Do **not** introduce a separate contract bump. Miller's gate is exact-equality and Miller consumes the whole enriched epoch together when it re-pins to `(28, 2)` at M4. A standalone `(26, 2)` test-role release would be rejected by Miller's gate and stranded.
- Keep `EXTRACT_CONTRACT_VERSION = 1` across all intermediate commits; the `1 → 2` bump lands once, in the **final commit of the combined batch**, in lockstep with `schema_version` reaching 28 (driven by the sibling plan's `type_arguments`/`literals` migrations — this plan adds **no** migration).
- Bump `EXTRACTION_CONTRACT_VERSION` (the extractor-output drift dial) via the sibling plan's drift-dial procedure when Phases 2–3 change extractor output (new detect fns / call-style capture). Phase 1 (pipeline wiring, no extractor-output change) does not itself require it, but if shipped in the same batch the batch's suffix bump covers it.
- **Record the combined final triple** in the sibling plan's "Final version triple" section so Miller's D5 gate moves once.

---

## Cross-cutting rule — classify on EVERY persistence path (the multi-path trap)

This is the same trap as the sibling plan's Rule 2, applied to classification instead of new tables. `classify_symbols_by_role` must run on the **mutable `batch.all_symbols` before the DB write**, on **every** path that persists symbols:

| Path | Site | Classifies today? |
|---|---|---|
| Live MCP/daemon indexing | `src/tools/workspace/indexing/pipeline.rs:54` | ✅ yes |
| External-extract CLI (force rebuild / incremental / single-file) | `src/indexing_core/persistence.rs` (batch built upstream in the extract command flow) | ❌ **no** |
| Single-file watcher | `src/watcher/handlers.rs` (builds its own batch) | ❌ **no** |

**Preferred design (single source of truth, can't-forget-a-path):** hoist the classify step into the **shared batch-finalization point** that every path routes through — i.e. classify inside `ExtractedBatch` construction/finalization (where `all_symbols` is assembled), so persisting an unclassified batch becomes impossible. Then **remove the now-redundant explicit call at `pipeline.rs:54`** (it would double-run; `classify_symbols_by_role` is idempotent — re-inserting the same `test_role` is harmless — but a single choke point is the point). If a shared finalization point does not cleanly exist, fall back to adding the identical classify block (mirroring `pipeline.rs:51-59`) at the extract-command batch-build site **and** the watcher batch-build site, and add a test per path. Either way: **assert classification on the extract path specifically** (see Phase 1 tests) — that is the path Miller reads and the one currently broken.

`LanguageConfigs::load_embedded()` is cheap and already called this way on many paths (`pipeline.rs:52`, `route.rs:174`, `finalize.rs:109`, `watcher/mod.rs:358`); build `role_configs` once per indexing run, not per file.

---

## Phases

### Phase 1 — Wire `classify_symbols_by_role` onto every persistence path (effort 1, value 5)

**Deliverable:** the extract DB produced by `julie-server extract scan` carries `symbols.metadata.test_role` (and `is_test`) for every test symbol the classifier recognizes — including **class/struct containers** via the existing TOML `test_container` config (C# `[TestClass]`/`[TestFixture]`/`[Collection]`, Java/Kotlin `@Nested`). No schema change. No new migration.

**Edits:**
1. Implement the **multi-path rule** above (preferred: hoist into shared batch finalization; remove `pipeline.rs:54` duplicate). Mirror the exact invocation at `pipeline.rs:51-59`:
   ```rust
   let configs = crate::search::LanguageConfigs::load_embedded();
   let role_configs = configs.build_test_role_configs();
   crate::analysis::test_roles::classify_symbols_by_role(&mut batch.all_symbols, &role_configs);
   ```
2. Confirm `classify_symbols_by_role` and `build_test_role_configs` are reachable from `src/indexing_core` / the extract command (visibility); they live in `src/analysis` + `src/search`. No new `pub` should leak extractor internals — they are julie-crate-internal already.

**Tests (assert on values, not non-throw):**
- **Extract-path integration test** (the one that proves the fix) in `src/tests/external_extract/operations.rs` (uses `run_external_scan`): a tiny tmp dir with `.cs`/`.py`/`.java` test files; after `scan`, SELECT `metadata` from `symbols` and assert: a C# `[Fact]` method → `test_role == "test_case"`; a C# `[TestClass]` *class* → `test_role == "test_container"` **and** `is_test == true` (the previously-missing class-container signal); a `[SetUp]`/`[TestInitialize]` method → `fixture_setup`; a pytest `@pytest.fixture` → `fixture_setup`. **This test must read the DB the extract CLI writes** — a unit test on `classify_symbols_by_role` alone does NOT prove the path is wired.
- **Watcher-path test** if you took the fallback (non-hoisted) design: a single-file watcher update persists `test_role`. (If hoisted into batch finalization, one path test + a unit assertion that the finalization classifies is sufficient — state which design you chose.)
- **Idempotency** test if you keep two call sites: classifying twice yields the same single `test_role`.
- **Negative:** a production class with no test annotation and a production method get **no** `test_role` and no `is_test`.

**Contract impact:** none on its own (rides the sibling batch's contract bump). If shipped standalone for julie's internal benefit, note it still changes external population and must coordinate per the contract section.

**Exit:** `julie-server extract scan` over a C#/Java/Python fixture, then `SELECT id,name,kind,json_extract(metadata,'$.test_role') FROM symbols` shows `test_case`/`fixture_setup`/`test_container` rows that were absent before — including test *classes*.

---

### Phase 2 — Base-type test containers + framework gaps for the weak-fallback languages (effort ~4, value 4)

Two cross-language sub-gaps the wiring alone does not close. **Scope per the cross-language principle: cover every language with a real test framework; explicitly exclude the data/markup languages with verification.**

**2a — Base-type test containers (annotation-free).** `classify_test_role`'s container path is annotation-driven, so it cannot flag containers identified by their *base type*: Python `class X(unittest.TestCase)`, Swift `class X: XCTestCase`, Java `extends TestCase` (JUnit 3), etc. The base-type info is already extracted (Python `superclasses` is in `metadata` — verified: a probe showed `"superclasses":["unittest.TestCase"]`; `python/types.rs:16-60`). Add a base-type container rule to `classify_test_role` (or a sibling classifier step): if a container-kind symbol's recorded superclasses/base types intersect a per-language TOML `test_base_types` set (new TOML key, e.g. python `["unittest.TestCase","TestCase"]`, swift `["XCTestCase"]`), classify it `TestContainer`. Add the TOML key + per-language values. **Verify** how each extractor records base types (metadata key vs signature) before coding — do not assume `superclasses` exists for Swift; inspect via `get_symbols`/AST.

**2b — Real framework detection for the generic-fallback languages.** Replace the weak `detect_generic` for languages with established frameworks by adding a `detect_<lang>` arm in `test_detection.rs` (and TOML annotation classes where annotation-driven). **Verified inventory** (each needs grammar-node confirmation at implementation time):

| Language | Framework(s) | Shape — VERIFY nodes before coding | Notes |
|---|---|---|---|
| C++ | GoogleTest, Catch2 | `TEST`/`TEST_F`/`TEST_P`/`TYPED_TEST` macros; Catch2 `TEST_CASE("...")` (call-style → Phase 3) | gtest macros expand to a function-like symbol; confirm the symbol name/kind the extractor emits. |
| C | Unity, Criterion, CMocka | Unity `test_*`/`void test...`; Criterion `Test(suite,name)` macro | Mostly name+path; Criterion is macro/call-style. |
| Zig | `std.testing` | `test "name" { … }` — a `test` **declaration**, not a function named `test_*` | detect_generic cannot catch this; needs a dedicated path keyed on the test-declaration node. High dogfood value (julie consumers index Zig). |
| R | testthat, RUnit | testthat `test_that("desc", {…})` (call-style → Phase 3); RUnit `test.*` functions | |
| GDScript | GUT, gdUnit4 | GUT: `test_*` methods in a class extending `GutTest`; gdUnit4 annotations | base-type (2a) + name. |
| Lua | busted, luaunit | busted `describe`/`it` (call-style → Phase 3); luaunit `Test*` classes / `test*` methods | |
| QML | Qt `TestCase` | `TestCase { function test_*() {} }` | base-type container (`TestCase`) + `test_*` methods. |

**Verified n/a (do NOT implement — no test-function symbols):** CSS, HTML, JSON, Markdown, Regex, TOML, YAML, SQL (no in-language unit-test symbols in scope; `sql/routines.rs` may keep the generic fallback). State this explicitly in code comments like the sibling plan's n/a list.

**Tests:** per language, a `tests/<lang>/` case asserting a real test symbol gets `is_test`/`test_role` and a production symbol does not; the Zig `test "..."` declaration case; the base-type container cases (Python `unittest.TestCase`, QML `TestCase`). Parameterize; cover the negative path.

**Contract/drift impact:** extractor output changes → bump `EXTRACTION_CONTRACT_VERSION` per the sibling drift-dial procedure (or rely on the batch's single suffix bump if co-shipped). No schema change.

**Exit:** every test-capable language emits `is_test`/`test_role`; the 7 weak-fallback languages use real framework detection; data/markup languages are explicitly excluded with verification.

---

### Phase 3 — Call-style test capture beyond JS/TS (effort ~3, value 3)

**Deliverable:** the call-style frameworks that produce no named symbol are captured as synthetic test symbols (like `test_calls.rs` does for Jest/Vitest/Mocha today), so their `is_test`/`test_container` and the **test description string** (the first string arg) are queryable.

`test_calls.rs` is generic enough to extend; the per-language work is the dispatch hook + the carrier name set:
- **Dart** (`test('desc', () {})`, `group('desc', () {})`): hook the Dart `call_expression` arm; `test`→`is_test`/`test_case`, `group`→`test_container`. (julie already flags Dart `@isTest` annotation methods; this adds the dominant call-style path the dart `test` package uses.)
- **R** (`test_that("desc", {…})`, `describe`/`it` for testthat 3e): hook R call dispatch.
- **C++ Catch2** (`TEST_CASE("name")`, `SECTION("name")`): if pursued, capture as call-style.
- **Lua busted** (`describe`/`it`): hook lua call dispatch, reuse the `test_calls` block-name sets.

Reuse `test_calls::is_test_runner_call` / `extract_test_call` where the grammar is JS-like; for non-JS grammars add a thin per-language equivalent. Emit the description string as the synthetic symbol's name (so search/inspect can show "test: handles empty input") and set `parent_id` to the enclosing container call (as JS/TS does).

**Tests:** Dart `test('adds', () {})` → a symbol with `is_test`, name carrying the description; `group(...)` → `test_container`. R `test_that(...)`. Negative: a non-test call (`print("x")`) emits no test symbol.

**Contract/drift impact:** extractor output changes → drift-dial bump (co-ship with the batch). No schema change.

**Exit:** Dart/R (and optionally Catch2/busted) call-style tests are captured with descriptions, matching the JS/TS behavior that already ships.

---

## Out of scope (deliberately)

- **Resolved test → tested-symbol linkage** ("which test covers `Foo.Bar`"). That is consumer-side resolution (Miller/eros) over `identifiers`, not extraction — mirrors the sibling plan's "resolve-lazily" stance. Do not attempt name resolution at extract time.
- **A dedicated `test_role` column or `tests` table.** The `metadata` JSON key is sufficient and additive (no migration). Only revisit if a future consumer needs indexed querying by role at scale.
- **Assert-density / quality scoring of tests.** `is_scorable_test` (`test_roles.rs:168`) already exists for julie's internal use; persisting a score is a separate richness decision, not this plan.
- **Data/markup languages** (CSS/HTML/JSON/Markdown/Regex/TOML/YAML) — no in-language test symbols.

## Verification (whole plan)

- `cargo nextest run` — `test_detection`, `analysis::test_roles`, per-language extractor suites, and `external_extract::operations` all green.
- **The proof:** `julie-server extract --db /tmp/t.db --root <polyglot-test-fixture> scan` then
  `sqlite3 /tmp/t.db "SELECT language, kind, name, json_extract(metadata,'\$.test_role') role FROM symbols WHERE metadata LIKE '%test%' ORDER BY language;"` shows `test_case`/`fixture_setup`/`fixture_teardown`/`test_container`/`parameterized_test` across C#, Java, Python, Dart, Zig, QML, etc. — and **test classes** appear as `test_container`, which the pre-plan extract never produced.
- Re-run the sibling plan's end-to-end check so both enrichment families are present in one `(28, 2)` extract.
- Record the combined `(schema_version, extract_contract_version, EXTRACTION_CONTRACT_VERSION)` triple in the sibling plan's final-triple section; notify the Miller gate owner to re-pin `MillerExtractContract` at M4.

## Coordination note back to Miller (codesearch)

Once Phase 1 ships in the `(28,2)` epoch, Miller's `exclude_tests` can rely on `metadata.test_role`/`is_test` for test **classes** too, shrinking the language-agnostic path fallback to a thin safety net for un-classified residue. Miller's M2 ships before this lands and is unaffected (it already ORs julie's `is_test` with the path fallback at the pinned `(26,1)`). Miller gains the richer `test_role` distinctions (case vs fixture vs container) when it re-pins to `(28,2)` at M4 — enabling test-suite navigation / fixture-aware features without any Miller-side re-derivation.
