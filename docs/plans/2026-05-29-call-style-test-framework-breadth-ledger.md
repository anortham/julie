# Call-style test-framework breadth ledger (all 34 languages)

Tracks **call-style test materialization** across all 34 languages for the
Miller-bridge test-role enrichment. Driven exactly like the Phase 3b literals
ledger: every language is classified **IMPLEMENTED**, **PARTIAL**, or
**VERIFIED-N/A** — no "we'll get to it" bucket. Each claim cites grammar
`node-types.json` and/or extractor source.

Owner: batch-c (task #53, research-only — no extractor edits in this pass).
Evidence collected against the working tree at the time of writing. The wave-1
adapters (#51 C/C++, #52 Lua/R) **landed mid-sweep**: all four now exist as
per-language `<lang>/test_calls.rs` files on the shared core; C/C++/R are wired
into their `mod.rs`, Lua wiring is in progress. (An earlier `LUA_VOCAB` +
`extract_lua_test_call` left in the *shared* `test_calls.rs` is now superseded by
`lua/test_calls.rs` and shows as `dead_code` until cleaned up.)

## What "call-style" means here

A framework is **call-style** when its tests are **call expressions** —
`it("name", …)`, `describe(…)`, `test "name" do`, `Describe { … }` — rather than
named function/method declarations (`func TestFoo`, `def test_foo`) or
annotations (`@Test`, `#[test]`). Call-style tests are invisible to the
declaration-walking extractor unless an adapter **materializes a `Function`
symbol** from the call node and attaches the canonical metadata:

- `is_test = true` (a test case)
- `test_container = true` (a grouping block: describe/group/context)
- `is_test = true` + `test_lifecycle = true` (a fixture hook: beforeEach/setUp)

The grammar-agnostic core lives in `crates/julie-extractors/src/test_calls.rs`
(`TestCallCategory`, `TestCallVocab`, `classify_call`, `build_test_call_symbol`).
Per-language adapters walk their own grammar (call-node kind, callee field,
string-node kind) and delegate symbol construction to that builder so the
metadata is byte-identical across languages.

The **declaration/annotation** path is the separate
`crates/julie-extractors/src/test_detection.rs::is_test_symbol` — naming +
annotation + path heuristics applied to symbols the extractor *already*
materializes. A language whose `detect_*` arm lists call-DSL vocab
(`describe`/`it`/…) but whose extractor never materializes those calls has a
**dormant** heuristic — that is the PARTIAL signature.

## Summary (34 / 34 accounted for)

| Bucket | Count | Languages |
|--------|-------|-----------|
| **IMPLEMENTED — shared core** | 3 | JavaScript, TypeScript, Dart¹ |
| **IMPLEMENTED — wave-1 in flight** | 4 | C, C++, Lua, R |
| **IMPLEMENTED — bespoke (convergence candidates)** | 2 | Ruby, Elixir² |
| **PARTIAL — vocab present, materialization dormant** | 3 | PowerShell, Bash, Scala |
| **PARTIAL — secondary call-style framework unhandled** | 4 | PHP, Kotlin, Swift, Go |
| **VERIFIED-N/A — test concept is non-call-style (handled)** | 8 | Rust, Python, Java, C#, VB.NET, GDScript, QML, Zig |
| **VERIFIED-N/A — no in-file test concept** | 10 | Vue, Razor, SQL, HTML, CSS, Regex, Markdown, JSON, TOML, YAML |

¹ Dart materializes via `dart/test_calls.rs` (#48) and is pending the fold onto
the shared `build_test_call_symbol` (lead #45).
² Ruby/Elixir materialize via **bespoke per-language code that predates the
shared core** — see the "Key corrections" section: these are *not* name-prefix
gaps; Ruby is fully wired with canonical metadata.

## IMPLEMENTED

| Lang | Framework | Adapter | Call node | Evidence |
|------|-----------|---------|-----------|----------|
| JavaScript | Jest/Vitest/Mocha/Bun | shared `extract_test_call` | `call_expression` | `javascript/mod.rs:500`, vocab `test_calls.rs:110-123` |
| TypeScript | Jest/Vitest/Mocha | shared `extract_test_call` | `call_expression` | `typescript/symbols.rs:145`, `typescript/mod.rs:186` |
| Dart | package:test | `dart/test_calls.rs` (#48) | `call_expression` (callee field `function`, name `string_literal`) | test/testWidgets→is_test, group→container, setUp/tearDown/setUpAll/tearDownAll→lifecycle. Pending fold onto shared builder (#45). |
| C | Criterion | `c/test_calls.rs` (#51) — on shared core, wired `c/mod.rs:268` | macro/`call_expression` | batch-a, landed mid-sweep |
| C++ | Catch2 | `cpp/test_calls.rs` (#51) — on shared core, wired `cpp/mod.rs:257` | `TEST_CASE(...)` call | batch-a, landed mid-sweep |
| Lua | busted | `lua/test_calls.rs` (#52) — on shared core; wiring in progress | `function_call` describe/it | batch-b, landing |
| R | testthat | `r/test_calls.rs` (#52) — on shared core, wired `r/mod.rs:444` | `call` test_that(...) | batch-b, landed mid-sweep |

## IMPLEMENTED — bespoke (CONVERGENCE candidates, not from-scratch)

These two already materialize their DSL calls with **bespoke per-language code**.
Wave-2 work is *converging them onto the shared `build_test_call_symbol`* (so
metadata stays identical and one builder owns it), not building from zero.

| Lang | Framework | Current materialization | Metadata today | Gap vs shared core |
|------|-----------|------------------------|----------------|--------------------|
| Ruby | RSpec / minitest-spec | `ruby/calls.rs:20-46` `extract_rspec_block` | **Full + canonical** (`calls.rs:210-240`): `describe`/`context`/`feature`→`test_container` (Namespace); `it`/`specify`/`example`/`scenario`→`is_test` (Function); `before`/`after`/`around`→`is_test`+`test_lifecycle` | None functionally — just not routed through the shared builder. Pure refactor/convergence. |
| Elixir | ExUnit | `elixir/calls.rs:51-52` | `test "…"`→`is_test` (`calls.rs:448`); `describe "…"`→`Namespace` symbol but **`metadata: None`** (`calls.rs:478-487`) — no `test_container` flag | (a) `describe` missing `test_container` metadata; (b) `setup`/`setup_all` absent from dispatch — **no lifecycle materialization**. Real gaps, not just convergence. |

## PARTIAL — call-style framework exists, materialization missing

The call-node + string-node columns are the wave-2 adapter scope. Vocab marked
"detect_* listed" already appears in `test_detection.rs` but is **dormant**
(nothing materializes a symbol with that name).

### Group A — primary idiom is call-style (highest wave-2 priority)

| Lang | Framework | Call node (grammar) | Name-arg node | Today | Vocab for wave 2 |
|------|-----------|---------------------|---------------|-------|------------------|
| PowerShell | Pester | `command` (callee `command_name`) | `string_literal` / `expandable_string_literal` | `is_test` only on `function` defs (`powershell/functions.rs:50-58`); Pester `Describe`/`It`/… are commands → not materialized. `detect_powershell` vocab **dormant**. | Describe/Context→container; It→test; BeforeAll/AfterAll/BeforeEach/AfterEach→lifecycle (already in `detect_powershell:248-263`) |
| Bash | shellspec / bats | `command` (callee `command_name`); bats `@test` may parse as `function_definition` — **verify in wave 2** | `string` / `raw_string` | `is_test` only on `function` defs (`bash/functions.rs:27-35`); command-form DSL not materialized. `detect_bash` vocab **dormant** for command form. | Describe/Context→container; It/Specify/Example→test; setup/teardown→lifecycle (in `detect_bash:236-246`) |
| Scala | ScalaTest / MUnit / specs2 | `call_expression` (FunSuite `test("…")`) **and** `infix_expression` (FlatSpec `"x" should "y" in {…}`) | `string` / `interpolated_string` | `is_test` only on `def` declarations via path heuristic (`scala/declarations.rs:90-98`); call/infix DSL not materialized. | test→test; describe/feature→container; ScalaTest is style-pluralized — needs a real-world fixture to lock vocab |

### Group B — secondary call-style framework; dominant idiom already handled

Lower wave-2 priority: each language's **primary** test idiom is already covered
(annotation/named/base-type), so these are additive. Framework existence is
ecosystem knowledge; the **call node is grammar-verified**, the exact DSL vocab
needs a real-world fixture before coding.

| Lang | Secondary framework | Call node (grammar-verified) | Name-arg node | Dominant idiom (handled) |
|------|--------------------|------------------------------|---------------|--------------------------|
| PHP | Pest (`test()`/`it()`/`describe()`) | `function_call_expression` | `string` / `encapsed_string` | PHPUnit: name-prefix + `@test` (`detect_php`, test_detection.rs:202-220) |
| Kotlin | Kotest / Spek (`describe`/`it`/`context`/`given`) | `call_expression` | `string_literal` | JUnit annotations (`detect_java_kotlin`) |
| Swift | Quick/Nimble (`describe`/`context`/`it`/`beforeEach`) | `call_expression` | `line_string_literal` (string body) | XCTest base-type + Swift Testing `@Test` (#48) |
| Go | Ginkgo (`Describe`/`Context`/`It`/`BeforeEach`) | `call_expression` | `interpreted_string_literal` | std `testing` `TestXxx`+`_test.go` (`detect_go`, test_detection.rs:183-188) |

## VERIFIED-N/A — test concept exists but is non-call-style (handled)

Positive verification per the breadth mandate: each has a real test idiom that is
**not** a call expression, and it is already covered by the declaration/annotation
path.

| Lang | Idiom | Why N/A for call-style | Evidence |
|------|-------|------------------------|----------|
| Rust | `#[test]` attribute on `fn` | Attribute on a named fn, not a call | `detect_rust` (test_detection.rs:106-110) |
| Python | pytest `def test_*` / unittest class + setUp/tearDown | Named-fn + class; call-style only via niche plugins (pytest-describe) | `detect_python` (112-127) |
| Java | JUnit/TestNG `@Test`; JUnit3 `extends TestCase` | Annotation + base-type; no mainstream call-style Java framework | `detect_java_kotlin` + base_types (#48) |
| C# | xUnit/NUnit/MSTest attributes | `[Fact]`/`[Test]` attributes | `detect_csharp` (163-181) |
| VB.NET | same .NET attributes | routed `"vbnet" → detect_csharp` | test_detection.rs:86 |
| GDScript | GUT `extends GutTest` + `func test_*` | Named-method + base class | `detect_gdscript` (320-322) + base_types (#47) |
| QML | Qt Quick Test `TestCase {}` + `function test_*()` | Component object + named functions | #47 |
| Zig | `test "name" { … }` | Grammar node is **`test_declaration`** — a declaration, not a call | grammar `node-types.json`; #46 |

## VERIFIED-N/A — no in-file test concept

Markup/data/template/SFC languages with no in-file test-runner construct.

| Lang | Why N/A |
|------|---------|
| Vue | SFC; tests live in external `.ts`/`.js` (Vitest/Jest), parsed by the TS/JS path |
| Razor | View templates (`.razor`/`.cshtml`); no in-file test framework |
| SQL | Query language; pgTAP is niche SQL function calls, not a materialized DSL |
| HTML | Markup |
| CSS | Stylesheet |
| Regex | Pattern language |
| Markdown | Documentation |
| JSON | Data |
| TOML | Config/data |
| YAML | Config/data |

## Key corrections to the original PARTIAL framing

The #53 brief listed `ruby / elixir / powershell / bash / scala` as PARTIAL
"call-style framework exists but only name-prefix detection." Verified against
source, two of those are wrong and need re-bucketing:

1. **Ruby is NOT name-prefix-only — it is fully bespoke-implemented.**
   `ruby/calls.rs::extract_rspec_block` already materializes the full RSpec DSL
   (container/example/lifecycle) with the exact `is_test`/`test_container`/
   `test_lifecycle` metadata the shared core emits (`calls.rs:210-240`). Ruby
   belongs in **IMPLEMENTED-bespoke** (convergence), not PARTIAL.

2. **Elixir is bespoke-implemented with specific gaps**, not name-prefix-only.
   `test "…"` is materialized with `is_test` and `describe` as a `Namespace`,
   but `describe` lacks the `test_container` flag and `setup`/`setup_all` are
   not handled at all. **IMPLEMENTED-bespoke with a 2-item gap list**, not
   PARTIAL.

3. **PowerShell / Bash / Scala are genuinely PARTIAL** — their `detect_*` vocab
   is present but **dormant** because the extractor only flags
   function/method *declarations*, never the command/call DSL. Confirmed: the
   `is_test_symbol` call sites are all on declaration paths
   (`powershell/functions.rs:50`, `bash/functions.rs:27`,
   `scala/declarations.rs:90`).

4. **Breadth sweep surfaced 4 additional PARTIAL languages** the brief did not
   list — PHP (Pest), Kotlin (Kotest), Swift (Quick), Go (Ginkgo). Each has a
   real, grammar-expressible call-style framework whose dominant sibling idiom
   is already handled. Flagged so they are not silently dropped; deprioritized
   as Group B.

## Wave-2 scoping shortlist (priority order)

1. **PowerShell, Bash** — `command`-node adapters; vocab already enumerated in
   `detect_*`. Highest value (primary idiom, dormant vocab ready).
2. **Scala** — needs both `call_expression` and `infix_expression` walking;
   vocab needs a real-world fixture.
3. **Elixir gap-fill** — add `test_container` to `describe`; add
   `setup`/`setup_all` lifecycle arms (small, in `elixir/calls.rs`).
4. **Ruby convergence** — refactor `extract_rspec_block` onto the shared builder
   (no behavior change; metadata already matches).
5. **Group B** (PHP/Kotlin/Swift/Go) — additive secondary frameworks; schedule
   after a fixture confirms each framework's exact DSL vocab.
