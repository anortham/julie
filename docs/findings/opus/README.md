# Tree-sitter Extractor Audit — Overview

**Audit scope**: All 33 concrete language extractors plus base infrastructure (`crates/julie-extractors/src/`).
**Goal**: Identify gaps and implementation errors that prevent Julie's tree-sitter layer from being world-class.
**Method**: Manual audit of base infrastructure (Opus lead) + four parallel deep audits across language tiers.

## Files in this folder

| File | Scope | Headline |
|---|---|---|
| `base-infrastructure.md` | `base/`, `language_spec/`, `registry.rs` | Solid scaffolding with seven concrete defects in the base layer that cascade into every extractor |
| `tier1-mainstream.md` | Rust, TypeScript, JavaScript, Python, Java, C#, Go, C++, C | Walker patterns inconsistent; ~half the extractors skip TypeUsage identifiers; modern features routinely missed |
| `tier2-modern-oo-functional.md` | PHP, Ruby, Swift, Kotlin, Scala, Dart, Elixir | `annotations: Vec::new()` copy-pasted everywhere outside function paths; Elixir `@doc` invisible; Ruby cross-file inheritance silently dropped |
| `tier3-specialized-scripting.md` | Bash, PowerShell, Lua, R, GDScript, Zig, VB.NET | R is the worst offender (4 class systems, only S3 detected); Bash/PowerShell create phantom Function symbols for external commands; signature bloat in Lua/GDScript |
| `tier4-markup-data.md` | HTML, CSS, Vue, QML, Razor, SQL, Regex, Markdown, JSON, TOML, YAML | SQL JOIN bug emits self-referential edges; Vue Options API loses all method/prop names; symbol-name collisions everywhere |

## Headline assessment

The architecture is sound. The directory layout, capability tier system, base extractor abstraction, and registry pattern all reflect mature thinking. Every language has tests; the capability matrix at `fixtures/extraction/capabilities.json` is enforced at compile time. **What this audit finds is not "rewrite the extractors" — it's "raise the floor."** Specific defects in the base layer, repeated patterns of incomplete coverage in language modules, and a test bar that's been calibrated to "produces something" rather than "produces what a world-class index needs."

The capability matrix's `capability_gaps` field is empty for all 36 languages. After this audit, that field should be populated with the ~80 specific gaps documented here. The matrix is currently an aspirational claim of completeness. It can become an accurate map of where the extractors actually stand.

## Cross-cutting patterns (the most valuable fixes)

These appear across multiple tiers. Fixing one of them lifts every extractor at once.

### Tier-spanning defects

**1. Visibility detection is fragile and inconsistent.**
- The base `BaseExtractor::extract_visibility` (creation_methods.rs:264-288) has a `text.contains("public ")` fallback that scans entire function bodies — function bodies containing `"public_key"` strings are misclassified as public. C++ correctly overrides; TypeScript/JavaScript still call the base version with bare arguments.
- C++ types hardcode `Visibility::Public` everywhere (cpp/types.rs:65, 122, 161, 211). Default for nested types in `class` is `private` per C++ rules.
- Python uses `Visibility::Public` for all classes regardless of `_`-prefix convention.
- The `Visibility` enum has only `Public/Private/Protected`; languages with finer distinctions (Rust `pub(crate)`/`pub(super)`, C# `internal`/`protected internal`, Swift `fileprivate`/`open`, Scala `private[pkg]`) collapse to one of three buckets.

**2. Doc-comment recognition is over-greedy or missing entirely.**
- `LuaDoubleDash`, `GoLine`, `HashLine`, `SqlLine` matchers (language_spec/mod.rs:60-80) treat **every** preceding line comment as a doc comment for Lua, Bash, Ruby, R, PowerShell, SQL.
- TypeScript, JavaScript, Python, PHP, Scala, Elixir, PowerShell, QML, Regex, JSON, TOML, YAML are all configured with `EMPTY` doc styles. Most accidentally work because the universal `/**` heuristic in `is_doc_comment` covers JSDoc/Javadoc/PHPDoc/Scaladoc.
- **Elixir does not work that way.** `@doc"""..."""` and `@moduledoc"""..."""` parse as `unary_operator` tree nodes, not as comments. Every Elixir symbol has `doc_comment: None`.
- **R Roxygen comments are completely unreached.** `R_DOCS` is wired up in the language spec but the R extractor never calls `find_doc_comment`.
- **PowerShell** has `EMPTY` doc styles in `language_spec/specs.rs:226` despite a custom local extractor.

**3. Annotations / decorators / attributes drop on most call sites.**
- Tier 2 finds `annotations: Vec::new()` copy-pasted across Swift, Kotlin, Scala, Dart everywhere except function/method extraction. Property wrappers (`@MainActor`, `@Published`, `@Composable`, `@Inject`) are silently lost.
- Java doesn't extract annotations on classes/interfaces/enums (Tier 1).
- Python decorators get stored as raw text in metadata, not parsed.
- TypeScript decorator handling differs between class and method paths.

**4. Identifier kind coverage is uneven.**
- `IdentifierKind::TypeUsage` missing or partial in: Rust (limited to scoped_identifier), JavaScript (no native types), Python, C#, Go, Ruby (Tier 2), Swift (Tier 2). **Half the languages skip it entirely.** Centrality scoring and find-references for type names are halved in those languages.
- `IdentifierKind::VariableRef` is **not emitted by any extractor in Tier 2** (PHP, Ruby, Swift, Kotlin, Scala, Dart, Elixir). The few Tier 1 extractors that emit it do so inconsistently. This means identifier-stream queries against variables fail across most of the language coverage.
- `IdentifierKind` enum has only 4 variants. No `AnnotationRef`, `MacroInvocation`, `ImportRef`, `GenericArg` — these all collapse to `Call` or are skipped.

**5. Constructor calls missing from identifier stream.**
- `object_creation_expression` / `new T()` is captured in `relationships.rs` for Java, JavaScript, C# but **not** as `IdentifierKind::Call` (Tier 1). `fast_refs --reference_kind=call` for a constructor returns zero hits even when the class is heavily instantiated.
- C++ stores the entire callee expression text as the call name (cpp/identifiers.rs:34-35) — `obj.method()` becomes `name = "obj.method"` rather than `"method"`. Inconsistent with every other language.
- GDScript has the same bug: `self.do_thing` stored as the call name (gdscript/identifiers.rs:51-71).

**6. Multi-declarator declarations drop all-but-first.**
- Java fields.rs:41-46: comment "For now, just the first."
- C# members.rs:315: same
- C++ declarations.rs:268: same
- Bash `extract_declaration` only takes first variable in `export A=1 B=2` (Tier 3).
- Ruby `attr_accessor :a, :b, :c` produces ONE symbol (Tier 2) — and this is Ruby's most common property idiom.
- C and Go correctly handle this.

**7. Parent_id propagation has three competing patterns.**
- Walker threads parent_id explicitly (most extractors) — correct
- Walker doesn't thread; extractor recomputes ID via `generate_id(name, row, col)` (Python, TypeScript) — duplicates `BaseExtractor::create_symbol` logic and breaks silently when either side changes
- Walker doesn't thread; extractor passes `None` (TypeScript class extraction at typescript/classes.rs:107, multiple Ruby sites) — orphans nested symbols
- The recompute pattern shows up as the regen workaround in typescript/functions.rs:91-128, evidence that this approach has already failed once.

**8. Cross-file relationship emission is inconsistent.**
- Ruby silently drops cross-file inheritance and `include` (ruby/relationships.rs:89-92, :188-205) instead of emitting pending relationships like Kotlin/Scala/PHP do.
- HTML, Razor, SQL are configured with `NO_PENDING_CAPABILITIES` despite having genuine cross-file relationships (`<script src>`, `@inject`, `FOREIGN KEY REFERENCES`). Synthetic placeholders (`url:foo`, `external_users`, `component-MyComp`) get emitted instead.

**9. Symbol-name collisions in markup languages.**
- HTML elements all named by tag (every `<a>` collides with every other `<a>`).
- CSS at-rules all named by keyword (`@import`, `@media`).
- Razor directives all named by directive (`@page`, `@layout`).
- The base relationship_id format also collides on multiple calls per line (`foo(); bar(); foo();` produces two relationships with the same ID).

**10. Tests calibrated to existence, not correctness.**
- R: `assert!(symbols.len() >= 0)` for S4/R6/Reference classes — passes even when extraction produces zero.
- SQL JOIN test (tests/sql/relationships.rs:74-78): `!join_relations.is_empty()` — does not verify FROM and TO are different tables. The actual code produces self-referential edges.
- Vue tests check that the `props`/`methods` wrapper exists — not the inner names that are silently dropped.
- The capability matrix's parity test enforces "implementation matches declaration" — but when the declaration is below the world-class bar, parity is too easy.

## Per-tier headline issues

### Tier 1 (Mainstream)
- TypeScript `extract_class` always passes `parent_id: None` (typescript/classes.rs:107). Cascading regen workaround at functions.rs:91-128.
- Multi-declarator drops in Java/C#/C++ — explicit comments admit it's incomplete.
- Constructor calls missing from `IdentifierKind::Call` for Java/JavaScript/C#.
- C++ stores `"obj.method"` as call name instead of `"method"`.
- Python misses PEP 695 type aliases (`type X = ...`) and `match` statements entirely.
- Java records stored as `Class` with no Property components.
- C# has no local functions, no lambdas, no partial-class linkage.

### Tier 2 (Modern OO/Functional)
- `annotations: Vec::new()` copy-pasted in Swift/Kotlin/Scala/Dart non-function paths.
- Elixir `@doc"""..."""`/`@moduledoc"""..."""` invisible (parsed as `unary_operator`, no special handler).
- Ruby `attr_accessor :a, :b, :c` → 1 symbol (should be 3).
- Ruby cross-file `<` inheritance and `include` drop silently.
- PHP constructor property promotion (PHP 8.0+) not extracted.
- Swift actors detected by comment but never branched on.
- Scala 3 given/extension/opaque untested.
- Multiple Ruby `parent_id: None` hardcodes orphaning aliases, variables, attr_accessors.

### Tier 3 (Specialized/Scripting)
- R: Only S3 of four R class systems detected. No Roxygen extraction. `>= 0` tests hide the gap.
- Bash + PowerShell: phantom Function symbols for external commands (`docker`, `kubectl`, `Connect-AzAccount`...). Internally inconsistent with relationship code's "builtin" lists.
- PowerShell method signatures malformed: `"static  Method()"` with double spaces and always-empty parameters.
- PowerShell's `is_builtin_cmdlet` knows two cmdlets — every other standard cmdlet generates a permanently-pending relationship.
- Lua + GDScript: entire function body stuffed into `signature` field.
- GDScript: `self.do_thing` stored as call name instead of `do_thing`.
- VB.NET: case-sensitive matching despite VB being case-insensitive.
- Zig: call relationships dropped from inside methods (caller filter is `Function` only).
- Zig: substring matchers (`node_text.contains("const")`) misclassify when token appears in strings/comments.

### Tier 4 (Markup/Data)
- **SQL JOIN bug**: every JOIN produces `from_symbol_id == to_symbol_id` (sql/relationships.rs:181-196).
- Vue Options API: `data`/`methods`/`computed`/`props` extracted as 5-byte container symbols with regex; individual member names never extracted.
- Vue identifier extraction: re-runs `parse_vue_sfc` per identifier — O(N) full reparses.
- Symbol name collisions in HTML/CSS/Razor.
- `NO_PENDING_CAPABILITIES` mistuned for HTML/Razor/SQL (genuine cross-file refs become dead synthetic IDs).
- Embedded `<script>`/`<style>` in HTML never parsed (Vue/Razor prove the technique works).
- SQL view-to-table, trigger-to-table relationships missing — code comment admits the original implementation was a stub.
- Markdown discards heading level; ignores fenced code blocks (cheap wins).
- Inline `Regex::new` recompilation in SQL (~10 sites in schemas.rs alone) and Razor.

## Recommended priorities

A reasonable order of attack, grouped by leverage:

### Quick wins — single-PR fixes with broad impact
1. **Hoist all inline `Regex::new` to `LazyLock<Regex>`** (SQL ~10 sites, Razor ~8, JavaScript JSDoc lookup). Mechanical, measurable perf.
2. **Fix multi-declarator drops** in Java, C#, C++, Bash, Ruby attr_accessor. Iterate over all declarators.
3. **Fix the SQL JOIN bug** (track FROM-table from `select_statement`).
4. **Fix C++ and GDScript call-name extraction** to use just the method name.
5. **Add Roxygen `#'` extraction to R.**
6. **Wire Elixir `@doc`/`@moduledoc` extraction** in `extract_def`/`extract_defmodule`.

### Medium — touches one or two extractors significantly
7. **Replace base `extract_visibility` text fallback** with a hard-failure stub; require each language to override.
8. **Fix Ruby cross-file inheritance** to emit pending relationships (mirror Kotlin/Scala/PHP).
9. **Re-route every `create_symbol` call in Swift/Kotlin/Scala/Dart through `extract_annotations`** to stop dropping decorators.
10. **Replace Vue Options API regex extractor** with tree-sitter walking the JS/TS section, mirroring the Composition API path.
11. **Add PHP 8.0+ constructor property promotion**, Java records → Properties, Swift actors detection.
12. **Add Scala 3 given/extension/opaque, Elixir defguard/defdelegate/defexception.**

### Larger — touches base or many extractors
13. **Standardize parent_id propagation** to "walker passes; extractors never recompute IDs." Eliminate the recompute paths in Python and TypeScript.
14. **Add `IdentifierKind::TypeUsage` extraction** to Rust (full), JavaScript, Python, C#, Go, Ruby, Swift.
15. **Add constructor calls (`new T()`) to `IdentifierKind::Call`** for Java, JavaScript, C#.
16. **Bump HTML/Razor/SQL to `RELATIONSHIP_DATA_CAPABILITIES + pending_relationships`** and synthesize real pending relationships instead of dead `external_*` IDs.
17. **Add embedded language extraction** to HTML (`<script>`, `<style>`) and Markdown (fenced code blocks). Use Vue/Razor as reference.

### Foundational — schema / type-level
18. **Add `modifiers: BitFlags<SymbolModifier>` to `Symbol`** for canonical async/static/const/abstract/etc., replacing string-grep on signatures.
19. **Add `import_source: Option<String>` to import-kind symbols** so cross-file resolvers don't re-parse signatures.
20. **Switch symbol IDs to xxhash3 of `(file_path, name, start_byte, end_byte)`** — eliminates collision risk and is faster than MD5.
21. **Populate `capability_gaps` in `fixtures/extraction/capabilities.json`** with the gaps documented here. Today the matrix asserts perfection across 36 languages, hiding the real coverage map.

## What this audit didn't cover

- **Performance benchmarks**: the tier audits noted regex-recompilation hot paths and Vue's O(N) reparse, but no end-to-end profiling was done.
- **Error recovery**: tree-sitter produces error nodes when parsing fails. The audit didn't deeply inspect how each extractor handles `ERROR` nodes or partial trees.
- **Incremental update correctness**: each extractor's behavior under file edits, especially `RecordOffset` / `apply_record_offset` for embedded sections, is touched on but not exhaustively tested.
- **Cross-language consistency for shared idioms**: e.g., does a TypeScript class field and a Java field produce equivalent symbol shapes? This audit found per-language issues but didn't compare semantically equivalent constructs across languages.
- **Coverage of grammar edge cases**: tree-sitter grammars have many obscure node kinds; this audit focused on the common cases. Long tail of grammar quirks remains.

## Final verdict

Julie's tree-sitter extractor layer is good enough to power a useful code intelligence server today. It is not yet world-class. The path from here to world-class is well-defined: the base infrastructure needs five concrete fixes, every extractor benefits from raising the test bar above "produces non-empty output," and the per-language audits enumerate roughly 80 specific gaps that can be fixed incrementally. None of this is a rewrite; all of it is tightening the bolts on a sound chassis.
