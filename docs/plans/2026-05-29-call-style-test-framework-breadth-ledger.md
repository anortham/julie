# Call-style test-framework breadth ledger (all 34 languages)

Tracks **call-style test materialization** across all 34 languages for the
Miller-bridge test-role enrichment. Driven exactly like the Phase 3b literals
ledger: every language is classified **IMPLEMENTED**, **PARTIAL**, or
**VERIFIED-N/A** — no "we'll get to it" bucket. Each claim cites grammar
`node-types.json` and/or extractor source.

Owner: implementation ledger for the Miller-bridge extractor updates.
Evidence reflects the current branch after the Wave-3 adapters landed.

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

## Summary (34 / 34 accounted for) — COMPLETE as of 2026-05-30

Every PARTIAL bucket from the original audit has been driven to IMPLEMENTED. No
"we'll get to it" bucket remains.

| Bucket | Count | Languages |
|--------|-------|-----------|
| **IMPLEMENTED — shared `test_calls` core** | 14 | JavaScript, TypeScript, Dart, C, C++, Lua, R, PHP, Kotlin, Swift, Go, Scala, PowerShell, Bash |
| **IMPLEMENTED — bespoke** | 2 | Ruby, Elixir² |
| **VERIFIED-N/A — test concept is non-call-style (handled)** | 8 | Rust, Python, Java, C#, VB.NET, GDScript, QML, Zig |
| **VERIFIED-N/A — no in-file test concept** | 10 | Vue, Razor, SQL, HTML, CSS, Regex, Markdown, JSON, TOML, YAML |

16 languages materialize call-style tests; 18 are positively verified N/A. The
former PARTIAL set — PowerShell/Bash/Scala (primary idiom) and PHP/Kotlin/Swift/Go
(secondary framework) — all shipped Wave-3 adapters on the shared core (PowerShell
Pester, Bash shellspec/bats, Scala ScalaTest/MUnit, PHP Pest, Kotlin Kotest/Spek,
Swift Quick/Nimble, Go Ginkgo).

² Ruby/Elixir materialize via **bespoke per-language code that predates the
shared core**. Both are fully wired with canonical metadata. Convergence onto
the shared `build_test_call_symbol` stays deferred because that builder only
emits `Function`, while Ruby/Elixir `describe` produce a `Namespace` symbol for
parent tracking. Functionally complete, not a breadth gap.

## Qualified-callee false-positive guard (2026-05-30 audit)

`classify_call` keys on the segment before the first `.` (for JS `it.only` /
`describe.skip` modifier chains). That JS-ism is a footgun for every other
language: a member/qualified callee whose **leading** segment is a vocab word
(`it.register("x")`, R S3 `describe.default`, PowerShell `Context.Helper`) would
false-positive. Fixed centrally: added `classify_call_exact` (exact membership,
no split) used by all 12 non-JS adapters; JS/TS keep `classify_call`. Two
mechanisms were closed — **A** member access in function position, and **B**
dotted *bare* identifiers (R/Bash/PowerShell) that a node-kind guard cannot
catch. Every adapter carries a `qualified_callee_is_not_materialized`-style
negative lock. Full SAFE/VULNERABLE positive-verification matrix below.

| Lang | Verdict | Evidence |
|------|---------|----------|
| C, C++, Dart, Scala, Lua | VULN→fixed (mech A) | member/`field_expression` callee text reached `classify_call`; now `classify_call_exact` |
| R, PowerShell, Bash | VULN→fixed (mech B) | dotted bare `identifier`/`command_name`/`word` (S3 names, dotted barewords) |
| PHP | SAFE | only fires on `function_call_expression`; member/static calls excluded; fn names dotless |
| Swift | SAFE | callee = first DIRECT `simple_identifier`; member receiver nests in `navigation_expression` → not found |
| Go | SAFE | `function.kind() != "identifier"` rejects `selector_expression` |
| Kotlin | VULN→fixed | dropped `navigation_expression` from callee kinds (bare-identifier only) |

## IMPLEMENTED

| Lang | Framework | Adapter | Call node | Evidence |
|------|-----------|---------|-----------|----------|
| JavaScript | Jest/Vitest/Mocha/Bun | shared `extract_test_call` | `call_expression` | `javascript/mod.rs:500`, vocab `test_calls.rs:110-123` |
| TypeScript | Jest/Vitest/Mocha | shared `extract_test_call` | `call_expression` | `typescript/symbols.rs:145`, `typescript/mod.rs:186` |
| Dart | package:test | `dart/test_calls.rs` (#48) | `call_expression` (callee field `function`, name `string_literal`) | test/testWidgets→is_test, group→container, setUp/tearDown/setUpAll/tearDownAll→lifecycle. Pending fold onto shared builder (#45). |
| C | Criterion | `c/test_calls.rs` (#51) — on shared core, wired `c/mod.rs:268` | macro/`call_expression` | batch-a, landed mid-sweep |
| C++ | Catch2 | `cpp/test_calls.rs` (#51) — on shared core, wired `cpp/mod.rs:257` | `TEST_CASE(...)` call | batch-a, landed mid-sweep |
| Lua | busted | `lua/test_calls.rs` (#52) — on shared core, wired through `lua/core.rs:33` | `function_call` describe/it | container/test/lifecycle metadata |
| R | testthat | `r/test_calls.rs` (#52) — on shared core, wired `r/mod.rs:444` | `call` test_that(...) | batch-b, landed mid-sweep |
| PHP | Pest | `php/test_calls.rs` — on shared core, wired `php/mod.rs:136` | `function_call_expression` | test/it/describe/lifecycle |
| Kotlin | Kotest / Spek | `kotlin/test_calls.rs` — on shared core, wired `kotlin/mod.rs:151` | `call_expression` | test/container/lifecycle |
| Swift | Quick/Nimble | `swift/test_calls.rs` — on shared core, wired `swift/mod.rs:139` | `call_expression` | test/container/lifecycle |
| Go | Ginkgo | `go/test_calls.rs` — on shared core, wired `go/mod.rs:217` | `call_expression` | Describe/Context/It/BeforeEach |
| Scala | ScalaTest / MUnit / specs2 | `scala/test_calls.rs` — on shared core, wired `scala/mod.rs:134,137` | `call_expression`, `infix_expression` | FunSuite + FlatSpec styles |
| PowerShell | Pester | `powershell/test_calls.rs` — on shared core, wired `powershell/mod.rs:135` | `command` | Describe/Context/It/lifecycle |
| Bash | shellspec / bats | `bash/test_calls.rs` — on shared core, wired `bash/mod.rs:140` | `command`, `simple_command` | shellspec/bats DSL |

## IMPLEMENTED — bespoke (CONVERGENCE candidates, not from-scratch)

These two already materialize their DSL calls with **bespoke per-language code**.
They are complete for behavior; the only remaining difference from the shared
core is implementation shape.

| Lang | Framework | Current materialization | Metadata today | Gap vs shared core |
|------|-----------|------------------------|----------------|--------------------|
| Ruby | RSpec / minitest-spec | `ruby/calls.rs:20-46` `extract_rspec_block` | **Full + canonical** (`calls.rs:210-240`): `describe`/`context`/`feature`→`test_container` (Namespace); `it`/`specify`/`example`/`scenario`→`is_test` (Function); `before`/`after`/`around`→`is_test`+`test_lifecycle` | None functionally — just not routed through the shared builder. Pure refactor/convergence. |
| Elixir | ExUnit | `elixir/calls.rs:51-53` | `test "…"`→`is_test`; `describe "…"`→`test_container`; `setup`/`setup_all`→`is_test` + `test_lifecycle` (`calls.rs:481`, `calls.rs:524`) | None functionally — bespoke path is complete. |

## Former PARTIAL closure evidence

The original PARTIAL set has no remaining open rows. Each former gap now has a
per-language adapter wired into symbol extraction.

| Former gap | Closure evidence |
|------------|------------------|
| PowerShell Pester | `powershell/test_calls.rs`, `powershell/mod.rs:135` |
| Bash shellspec / bats | `bash/test_calls.rs`, `bash/mod.rs:140` |
| Scala ScalaTest / MUnit / specs2 | `scala/test_calls.rs`, `scala/mod.rs:134,137` |
| PHP Pest | `php/test_calls.rs`, `php/mod.rs:136` |
| Kotlin Kotest / Spek | `kotlin/test_calls.rs`, `kotlin/mod.rs:151` |
| Swift Quick/Nimble | `swift/test_calls.rs`, `swift/mod.rs:139` |
| Go Ginkgo | `go/test_calls.rs`, `go/mod.rs:217` |

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
