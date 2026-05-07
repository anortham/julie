# Tier 2 Extractor Audit — Modern OO + Functional

## Summary

Quality varies a lot across these seven extractors. Kotlin and Dart are the strongest; Scala and Elixir have meaningful structural gaps despite ambitious feature lists; Swift drops annotations everywhere outside `func`; Ruby has parent_id and pending-relationship correctness bugs; PHP misses several PHP 8.x features. The most pervasive systemic issues are: (1) `annotations: Vec::new()` everywhere except function bodies, (2) the doc-comment configuration treats PHP/Scala/Elixir as styleless even though their conventions are well-defined, and (3) constructor parameters / case-class fields / `attr_accessor` lists silently lose all-but-the-first member. Several of the stated language features (Swift actors, Scala 3 given/extension, Elixir defguard/defdelegate, PHP constructor promotion) are advertised but not actually implemented.

## Per-Language Findings

### PHP
**Status**: Good with notable PHP-8 gaps.

**Strengths**:
- Comprehensive class/interface/trait/enum/anonymous class extraction.
- Pending-relationship plumbing for cross-file extends/implements is solid (`php/relationships.rs:73`, `:202`).
- Namespace stripping helper `strip_php_namespace` keeps cross-namespace lookups sane (`relationships.rs:13`).
- Grouped use declarations `use App\{A, B}` correctly produce multiple Import symbols (`namespaces.rs:80`).
- PHPDoc handled via the universal `/**` fallback in `LanguageSpec::is_doc_comment`, so PHPDoc tests pass even though `language_spec/specs.rs:138` declares PHP doc styles as `EMPTY`.

**Gaps & Errors**:
- **Constructor property promotion (PHP 8.0) not extracted.** `__construct(public string $name)` declares both a parameter and a property. Nothing in `php/functions.rs` or `php/members.rs` walks the constructor's `formal_parameters` looking for visibility-prefixed parameters. They never become Property symbols. Confirmed by `grep "promoted\|simple_parameter\|property_promotion"` returning no hits.
- **Intersection types (PHP 8.1) ignored.** `find_return_type` (`php/functions.rs:154`) accepts `primitive_type | named_type | union_type | optional_type` only. `Foo&Bar` parses as `intersection_type` and is dropped from return types. No tests for it either.
- **`readonly class` modifier (PHP 8.2) is partially supported — readonly is in the `extract_modifiers` list but classes set their type kind to `class` regardless. No `readonly` flag in metadata.
- **Identifier extraction misses several common references.** `php/identifiers.rs:14-138` only handles `function_call_expression`, `member_call_expression`, `member_access_expression`, `named_type`, and `binary_expression` (instanceof). It does NOT handle:
  - `scoped_call_expression` (`Class::method()`) — produces a relationship but no identifier.
  - `class_constant_access_expression` (`Class::CONSTANT`).
  - `object_creation_expression` (`new Foo()`) — no TypeUsage identifier for the class name.
  - `variable_name` references (`$foo`) — VariableRef kind is never produced for any language in this tier; PHP is no exception.
- **PHP function signature builder uses string `replace` to inject modifiers.** `php/functions.rs:55-60` does `signature.replace(&format!("function {}{}", ref_prefix, name), &format!("{} function {}{}", ...))`. If the function name happens to also appear in attribute text (e.g., `#[Cache(name: 'foo')] public function foo`), the replace can corrupt the signature. Should build the signature linearly.
- **Trait usage (`use TraitA, TraitB;` inside a class body) is appended to the class signature as raw text** (`php/types.rs:60-68`) but never produces a Relationship. There's no `Uses` relationship from class to trait. Test fixtures don't cover this.

### Ruby
**Status**: Needs Work — multiple parent_id / cross-file bugs.

**Strengths**:
- Module/class/method/singleton method/alias all covered with the basic semantics right.
- Visibility tracking via `current_visibility` mutable state (`ruby/mod.rs:30, :253-259, :273-285`) correctly models Ruby's "subsequent methods" semantics for bare `private` calls.
- `Struct.new(...)` recognized and rewritten to a Class with field Properties (`ruby/calls.rs:32-112`), nice touch.
- Ruby has the universal `/**` doc fallback off (PHP/Scala/Elixir all match `/**`, but Ruby doesn't use it; Ruby uses `HASH_DOCS` so `#`-line doc comments work — this is correct for RDoc/YARD).

**Gaps & Errors**:
- **`attr_accessor :name, :age, :email` only produces ONE Property symbol.** `ruby/calls.rs:160-178`'s `extract_attr_accessor` extracts `symbol_nodes.first()` and returns. The 2nd, 3rd, etc. names are silently dropped. Same shape for `attr_reader` and `attr_writer`. This breaks the most common Ruby idiom for declaring properties.
- **`define_method :foo`, `def_delegator`, and `attr_accessor` all hardcode `parent_id: None`** (`ruby/calls.rs:170, :209, :240`). When these calls happen inside a `class Foo` body, the resulting Property/Method symbol is unparented. Children fall outside the class subtree.
- **`extract_alias` and `extract_variable` hardcode `parent_id: None`** (`ruby/symbols.rs:235, :285`). Aliases inside a class are orphaned. Same for instance/class variables.
- **Inheritance and module inclusion never produce pending relationships.** `ruby/relationships.rs:89-92` only creates an Extends relationship if BOTH `class_name` AND `superclass_name` are found in `symbols` (same-file). Cross-file `class Bar < Foo` produces zero relationship. The same applies to `include Helpers` (`process_include_extend_call` line 188-205): if the module isn't in this file, the relationship is silently dropped. Compare with Kotlin/Scala which emit `add_structured_pending_relationship` for cross-file targets.
- **Ruby `private :foo, :bar` (method-level inline visibility) not handled.** `parse_visibility` (`ruby/helpers.rs:200-207`) handles bare `private` and `module_function` only. The call form with explicit symbol arguments doesn't update those methods' visibility post-hoc. There are no tests for this.
- **`extract_singleton_method` skips test detection** (`ruby/symbols.rs:191-218`) even though `extract_method` runs `is_test_symbol`. So `def self.test_something` is never marked `is_test`.
- **`extract_call_relationships` finds containing symbol via `find_containing_symbol(&node, symbols)` without filtering by symbol kind** (`ruby/relationships.rs:227`). For Ruby this can attribute a call to the enclosing class symbol rather than a method, when no method is found. Probably benign but worth verifying.

### Swift
**Status**: Good for OO, Significant Gaps for modern Swift.

**Strengths**:
- Class/struct/protocol/enum/extension/typealias all extracted.
- `enum_case` and indirect enum recognized.
- Initializer/deinitializer/subscript handled.
- Inheritance relationships use both `type_inheritance_clause` and `inheritance_specifier` shapes.

**Gaps & Errors**:
- **Swift actors are not extracted.** The comment in `swift/types.rs:12` ("class_declaration for class, struct, enum, extension, and actor") is aspirational. Search for `actor_declaration` or `\"actor\"` returns nothing. An `actor` declaration parses to a `class_declaration` with an `actor` keyword child — `extract_class` sees only `is_enum`/`is_struct`/`is_extension` and falls through to `("class", SymbolKind::Class)` (line 49). Actors lose their distinguishing kind/metadata.
- **Annotations / property wrappers / attributes dropped EVERYWHERE except `func`.** `swift/extensions.rs:168` (`create_symbol_options`) hardcodes `annotations: Vec::new()`. Every callsite in `types.rs`, `properties.rs`, `protocol.rs`, `enum_cases.rs`, `extensions.rs` does the same. `extract_annotations` (`swift/signatures.rs:65`) exists and works, but only `swift/callables.rs` actually calls it (lines 23, 111, 158). Result: `@MainActor`, `@objc`, `@available`, `@Published`, `@State`, `@StateObject`, `@Environment`, `@FetchRequest`, custom property wrappers, etc. all vanish from class/struct/protocol/enum/extension/property/enum_case symbols. This kills both test detection (e.g., `@Test` for swift-testing) and SwiftUI navigation.
- **Identifier extraction limited to Calls and MemberAccess.** `swift/identifiers.rs:101-104` says "Future: type usage, constructor calls, etc." Type references in `let x: Foo`, `func bar(a: Foo)`, generic args, conformance lists are not recorded as identifiers. Centrality scoring suffers.
- **Result builders (`@resultBuilder`), property wrappers (declarations), macros (`@freestanding`/`@attached`)** — none have specific handling. Anything decorated with these types is treated as a regular class/struct/property.
- **`async`/`throws`/`rethrows` / `async throws` modifiers** are picked up by `extract_modifiers` (`swift/signatures.rs:18-44`) only if the grammar produces a `modifiers` container around them, but in practice tree-sitter-swift puts these as adjacent keyword nodes on the `function_declaration`. Verify by reading callables.rs — the signature builder uses `extract_modifiers` and the test would only pass if `async` ends up inside `modifiers`. Worth a regression test for `func foo() async throws -> Int`.
- **Doc comments**: Swift uses both `///` (TripleSlash) and `/** */`. SWIFT_DOCS includes only `TripleSlash`; `/** */` works via universal fallback. Headerdoc-style `/// - Parameters:` should parse fine.

### Kotlin
**Status**: Good — best of the seven for symbol kinds and inheritance pending.

**Strengths**:
- Data class, sealed class, enum class, object, companion object, fun interface, value class all surfaced via `determine_class_kind` (`kotlin/helpers.rs`).
- Extension functions get receiver type baked into signature (`kotlin/declarations.rs:46-51`).
- Inheritance produces proper pending relationships when target not in same file (`kotlin/relationships.rs:89-97`), with constructor invocation distinction for Extends vs Implements.
- Identifiers cover Calls, MemberAccess, AND TypeUsage with noise filtering for builtins.
- ERROR-node recovery for misshapen `class Foo \n private constructor(...)` (`kotlin/mod.rs:140-156`).
- `secondary_constructor` extraction (`kotlin/mod.rs:111-124`).
- Operator functions get `SymbolKind::Operator` (`kotlin/declarations.rs:76-77`).

**Gaps & Errors**:
- **`annotations: Vec::new()` on `extract_class`, `extract_interface`, `extract_object`, `extract_companion_object`, `extract_enum_members`, `extract_property`, `extract_package`, `extract_import`, `extract_type_alias`** (`kotlin/types.rs:113, 165, 212, 254, 300`, `kotlin/properties.rs:123, 254`, `kotlin/declarations.rs:243, 276, 343`). `helpers::extract_annotations` exists, but only `extract_function` uses it. So `@Composable`, `@Inject`, `@Test`, `@Deprecated`, `@Serializable`, `@JvmStatic`, etc. are dropped on classes/properties/objects. Same systemic bug as Swift.
- **Extension function metadata not flagged.** Line 76-82 of `declarations.rs` decides between `Operator`, `Method`, `Function`. An extension function at top level is a `Function` with no metadata indicating it's an extension. Receiver type is in the signature string, not in metadata.
- **VariableRef identifiers absent** — same Tier 2 systemic gap.
- **`when` expressions / smart casts / sealed-when exhaustiveness** are not analyzed (out of scope for symbol extraction, but worth noting for future).
- **Top-level type extraction skips `enum_class_body` recursion**: enum members are extracted by walking `enum_class_body` but the parent enum's `parent_id` gets lost if `extract_enum_members` is called from a nested context. Worth checking via test that nested enums are parented correctly.

### Scala
**Status**: Significant Gaps despite being one of the most ambitious extractors.

**Strengths**:
- Classes, traits, objects, enums (Scala 3), enum_case, val/var, given, extension, type alias, package, import all dispatched in `scala/mod.rs:73-118`.
- Companion-object detection by name match (`scala/types.rs:143-149`).
- Inheritance with proper Implements vs Extends distinction, and pending relationships for cross-file (`scala/relationships.rs:67-85`).
- Doc comments work via `/**` universal fallback even though `language_spec/specs.rs:170` says EMPTY.

**Gaps & Errors**:
- **Case class constructor parameters NOT extracted as Field/Property symbols.** `case class Person(name: String, age: Int)` declares two public `val`s automatically by Scala semantics. The current code (`scala/helpers.rs:97-102`) extracts the parameter list as a string for the signature only. There's no equivalent of Kotlin's `extract_constructor_parameters`. Same for primary class constructors. The test `test_scala_case_class_signature` (`tests/scala/mod.rs:476-500`) only verifies the signature contains "case", not that `name` and `age` are symbols.
- **Top-level vals/vars/given/extension call relationships are missed.** `scala/mod.rs:161-175` only invokes `extract_call_relationships` when visiting a `class_definition`/`trait_definition`/`object_definition`/`enum_definition`/`function_definition`/`function_declaration`. Calls inside `val foo = doSomething()` at file level, or inside a `given` instance body, or inside a Scala 3 `extension` block, are silently dropped. Verify with: `def topLevel = sideEffect()` works (function_definition), but `val x = sideEffect()` does not.
- **Scala 3 `given`, `extension`, `using` clauses have NO test coverage.** `grep "given_definition\|extension_definition\|implicit"` against `tests/scala/` returns zero. The code claims to support them; nobody verifies.
- **Opaque types untested.** `helpers.rs:66` accepts `opaque` as a modifier keyword, but `tests/scala/` has zero `opaque` test files.
- **Package object (`package object Foo { ... }`) — Scala 2 idiom** parses as `object_definition` and is treated as a regular object. There's no special-casing to distinguish a package-level object from a regular companion-style object.
- **`self_type` declarations** (cake pattern: `self: Service =>`) are not captured.
- **`extract_extension` falls back to a literal name "extension"** when the type can't be extracted (`scala/declarations.rs:270`). Multiple extensions on the same type yield multiple symbols all named "extension" with random IDs. Either don't emit a symbol, or use a deterministic name like `extension_<lineno>`.
- **`Scala 3 enum case` parameter list** is just appended to the signature (`scala/types.rs:248-251`); fields of enum cases (`case Point(x: Int, y: Int)`) aren't promoted to Field symbols.
- **`extract_call_relationships` only handles `call_expression`** (`scala/relationships.rs:160`), missing infix operators like `a + b` (parses as `infix_expression`) and `a.method` accessed as `field_expression` without parens. The receiver-aware pending-target builder in `unresolved_call_target` is well-structured but never sees infix calls.
- **Modifier extraction has `extract_modifiers` which conflates direct modifier nodes and `modifiers` container children, sometimes duplicating** (`scala/helpers.rs:11-31`). The `is_modifier_keyword` check on the text + the second loop checking `child.kind()` can both fire on the same node text. Worth cleaning up.

### Dart
**Status**: Good — strongest in identifier coverage; a few OO-specific gaps.

**Strengths**:
- Comprehensive coverage: classes, mixins, extensions, enums, enhanced enum bodies, factory constructors, named/positional params, getters/setters, async, generics, typedefs.
- Dart 3 sealed/base/final/interface class recovery via ERROR-node pattern matching (`dart/mod.rs:393-555`) — heroic engineering against an incomplete grammar.
- Identifier extraction covers Calls, MemberAccess, AND TypeUsage with noise filter (`dart/identifiers.rs:88-108`).
- `is_async`, `is_static`, `is_override`, `is_flutter_lifecycle` metadata on methods (`dart/functions.rs:133-198`).
- Flutter widget detection (`dart/functions.rs:26-46`).

**Gaps & Errors**:
- **`extract_enum`, `extract_mixin`, `extract_extension` ignore `_` privacy convention.** `dart/types.rs:25, :105, :160` all hardcode `Visibility::Public`. Compare with `extract_typedef` (line 195) which respects `name.starts_with('_')`. So `enum _InternalState` is reported public.
- **Enum `signature` is bare `enum Foo`** (`dart/types.rs:24`) — missing the `implements` clause and any constructor params for enhanced enums.
- **Mixin `on` constraint** is detected via `source.contains(" on ")` string match (`dart/types.rs:82`). String matching where AST kinds should be used. `mixin Foo on Service` and `mixin onlyOnSomething on Service` could collide; the latter is unusual but possible.
- **Extension `on` clause** uses the same string-matching approach (`dart/types.rs:137`).
- **Dart class definitions use `find_child_by_type(node, "identifier")` to get the name** (`dart/functions.rs:21, :57`). For classes with type parameters or before/after specific tokens, this picks the FIRST identifier in tree order, which usually is the class name, but it's not robust against grammar quirks. Should use `child_by_field_name("name")`.
- **`mod.rs:963 lines** — Dart's `mod.rs` is by far the largest file in the tier-2 set, exceeding the 500-line guideline by nearly 2x. Mostly Dart 3 ERROR recovery code. Consider extracting the recovery functions into `dart/recovery.rs`.

### Elixir
**Status**: Significant Gaps. Most distinctive Elixir features missing.

**Strengths**:
- `defmodule`/`def`/`defp`/`defmacro`/`defmacrop`/`defprotocol`/`defimpl`/`defstruct` all dispatched in `calls.rs:dispatch_call`.
- `@type`/`@typep`/`@opaque`/`@callback`/`@spec`/`@behaviour` module attributes parsed (`elixir/attributes.rs`).
- ExUnit `test` and `describe` recognized as test/namespace symbols (`calls.rs:39-40, :427-488`).
- Module name stack for qualified names (`elixir/mod.rs:28`).

**Gaps & Errors**:
- **`@doc"""..."""` and `@moduledoc"""..."""` are NEVER captured as doc_comments.** Elixir's doc style is `EMPTY` (`language_spec/specs.rs:186`). `find_doc_comment` looks for prev_named_sibling that "contains 'comment'"; but `@doc` parses as a sibling `unary_operator` containing a `call`, not a comment. So `extract_defmodule` calls `find_doc_comment(node)` (calls.rs:58) and always gets None for the moduledoc. Same for `extract_def` (line 109). The body of `@doc` strings — which IS the canonical Elixir docstring — is silently dropped. There IS a partial workaround: `collect_module_annotations` (`attributes.rs:221`) collects raw `@moduledoc` lines as annotation text, but they're stored as annotation keys, not as the symbol's `doc_comment` field. Tools that read `symbol.doc_comment` won't see them.
- **`defguard`, `defguardp`, `defdelegate`, `defexception`, `defoverridable` are NOT extracted as symbols.** `dispatch_call` (`calls.rs:26-42`) doesn't list them. But `is_definition_keyword` in `identifiers.rs:96-117` DOES list them as definition keywords (so they're skipped from being treated as identifiers). Net effect: `defdelegate hello, to: World` produces no symbol AND no identifier — invisible to the index.
- **`defstruct` field locations are wrong.** `helpers.rs:148-156` collects field names with byte offsets, but `extract_defstruct` (`calls.rs:340-355`) ignores the offsets and creates each field symbol using the parent `defstruct` node's location. Every field has identical line/column. Find_references on `:name` will report the `defstruct` line, not the field's own atom location. The Vec returned by `extract_struct_fields` even returns `start_byte`/`end_byte` per field — they're collected then discarded.
- **Pipeline operator `|>` not recognized for call resolution.** `a |> foo() |> bar()` is `binary_operator` nodes, not `call` nodes that contain the receiver as `a`. The current call extraction (`relationships.rs:222-264`) uses `extract_call_target_name` which expects the standard `call` shape. Pipeline-style code, which is idiomatic Elixir, has its calls discovered but the receiver context (`a` is an arg of `foo`, etc.) is lost. The `extract_pending_target` shape doesn't model this.
- **No support for `@derive`, `@before_compile`, `@after_compile`, `@external_resource`** as symbols — they are listed in `collect_module_annotations` (line 233-237) for annotation-tagging, but not as symbols. `@derive Jason.Encoder` is a real semantic dependency on Jason.
- **Struct usage `%MyStruct{...}` is not extracted as TypeUsage.** Elixir's `identifiers.rs:33-93` handles `call`, `dot`, and `alias` but not `struct` or `map` literal nodes. So instantiating a struct doesn't increment the struct's reference count.
- **Macro expansion is not modeled.** Tree-sitter sees `defmacro foo` as a macro definition but cannot follow expansions of `foo(...)` callsites. This is a fundamental tree-sitter limitation, but it should at least be flagged in metadata so users know reference counts on macros are call-site only.
- **Scope-aware visibility via `defp`** works correctly, BUT the `parent_id` chain may break for nested modules. `extract_defmodule` pushes `module_name` onto `module_stack` (line 81) but the parent_id passed to children is `Some(&sym_id)`, not derived from the stack. If `defmodule Outer do defmodule Inner do def foo` is encountered, `foo`'s parent should be `Inner.id`. Need to verify this is correct by reading the test suite, which I didn't fully exercise.

## Cross-Cutting Patterns

These appear across multiple languages and are the most valuable to fix.

1. **`annotations: Vec::new()` is a copy-paste anti-pattern.** Affects Swift (every callsite outside `callables.rs`), Kotlin (every callsite outside `extract_function`), Scala, Dart's create_symbol calls in `extract_class`/`extract_method`/`extract_typedef`, etc. The fix is uniform: route every `create_symbol` call through `extract_annotations`. Decorators/attributes are first-class symbol metadata for SwiftUI, Compose, Spring, Test discovery, framework heuristics — losing them silently is a serious search-quality regression.

2. **Doc-style configuration is incomplete for three languages.** `language_spec/specs.rs` declares `EMPTY` for PHP (line 138), Scala (line 170), and Elixir (line 186). PHP and Scala accidentally work because of the universal `/**` prefix fallback in `LanguageSpec::is_doc_comment` (`language_spec/mod.rs:52`). Elixir does NOT recover this way — its `@doc"""..."""` form is parsed as a tree node, not a comment. Either:
   - Add a `DocCommentStyle::ElixirAtDoc` that knows to walk `unary_operator @ call doc(arguments(string))` and extract the string value, or
   - Have `extract_defmodule` / `extract_def` actively look at preceding `@doc`/`@moduledoc` siblings and pull the string content into `doc_comment`.

3. **Cross-file inheritance/inclusion sometimes drops silently instead of producing pending relationships.** Ruby's `extract_inheritance_relationship` and `process_include_extend_call` (`ruby/relationships.rs:89-92, :188-205`) require both ends to be in the same file's symbol set; otherwise, no relationship and no pending. Compare to Kotlin (`kotlin/relationships.rs:89-97`), Scala (`scala/relationships.rs:75-84`), and PHP (`php/relationships.rs:73-82`), which all emit pending relationships. Ruby should match.

4. **Constructor parameters and DSL-declared properties lose all-but-first.** Ruby's `attr_accessor :a, :b, :c` makes one symbol (`ruby/calls.rs:160-178`). Scala's case class fields are zero symbols. PHP constructor promotion is zero symbols. These are all the language's primary mechanism for declaring properties; missing them is a real coverage gap.

5. **Identifier kinds skewed toward Call/MemberAccess; TypeUsage and VariableRef inconsistent.** None of the seven extractors emit `IdentifierKind::VariableRef`. Swift (and to a lesser extent Ruby) doesn't emit `TypeUsage` either. Centrality scoring depends on these reference counts.

6. **Test coverage gaps for declared-but-untested features.** Scala 3 given/using/extension/opaque, Swift actors, Elixir defguard/defdelegate/defexception. The code recognizes the keywords but no test verifies the resulting symbols. This is how regressions sneak in. Recommend adding at minimum one fixture test per claimed feature.

7. **Several `parent_id: None` hardcodes.** Ruby `extract_alias`, `extract_variable`, `extract_attr_accessor`, `extract_define_method`, `extract_def_delegator`, `extract_require` (`ruby/symbols.rs:235, :285`, `ruby/calls.rs:144, :170, :209, :240`). When these constructs are inside a class/module, parent_id should propagate via the traversal. The current shape orphans them.

8. **Hand-rolled signature builders use `String::replace` / string concatenation in places where assembly should be linear.** PHP `functions.rs:55-60` is the worst offender. Scala's `extract_modifiers` filtering pattern is duplicated five times across `types.rs`, `properties.rs`, and `declarations.rs`. Worth a shared helper.

## Top 10 Highest-Impact Findings (ranked)

1. **Annotations dropped on all non-function symbols in Swift and Kotlin.** Single-line fix per call site; biggest impact on framework-specific search (SwiftUI, Compose, dependency injection, test discovery). `swift/extensions.rs:168`, all Kotlin `types.rs`/`properties.rs`/`declarations.rs` callsites.

2. **Elixir `@doc"""..."""` and `@moduledoc"""..."""` not stored in `doc_comment`.** Elixir's primary documentation mechanism is silently invisible. Requires special-case handling because tree-sitter parses them as `unary_operator`, not as comments. Fix in `elixir/calls.rs::extract_defmodule` and `extract_def` to walk preceding `@doc`/`@moduledoc` siblings and pull the string content.

3. **Ruby `attr_accessor :a, :b, :c` produces ONE symbol instead of THREE.** `ruby/calls.rs:160-178`. Iterate over `symbol_nodes`, emit a Property per name. This is the most common Ruby property idiom; missing 2/3 of them is severe.

4. **Ruby cross-file inheritance and `include` produce NO relationship.** `ruby/relationships.rs:89-92, :188-205`. Mirror the pending-relationship pattern from Kotlin/Scala. Without this, Ruby ref graphs are wildly incomplete in any multi-file project.

5. **PHP constructor property promotion (PHP 8.0+) not extracted.** `__construct(public string $name)` should produce a Property symbol named `name`. Iterate over the constructor's `formal_parameters` and look for `simple_parameter` children with a `visibility_modifier`. No current code path attempts this.

6. **Swift actors and Scala case-class fields not extracted as first-class symbols.** Two distinct fixes:
   - Swift: detect `actor` keyword in `class_declaration` and choose `SymbolKind::Class` with `metadata["kind"] = "actor"`, plus fix the comment that currently lies about coverage.
   - Scala: add a constructor-parameter walker similar to Kotlin's `extract_constructor_parameters` for primary constructors of `class_definition`, marked `case` if present.

7. **Scala calls inside top-level vals/given/extension are dropped.** `scala/mod.rs:161-175` should also recurse into `val_definition`, `var_definition`, `given_definition`, `extension_definition` (or simply walk all children unconditionally, since `extract_call_relationships` itself walks).

8. **Elixir `defguard`, `defguardp`, `defdelegate`, `defexception`, `defoverridable` not extracted.** Add cases to `dispatch_call` (`elixir/calls.rs:26-42`). `defdelegate` is especially important because it's a common library pattern.

9. **Ruby `parent_id: None` hardcodes break the parent chain.** Multiple sites in `ruby/symbols.rs` and `ruby/calls.rs` hardcode None instead of accepting parent_id from traversal. Pass parent_id through and use it.

10. **Doc-style declared `EMPTY` for PHP / Scala / Elixir is misleading.** PHP and Scala accidentally work via the `/**` fallback. Elixir does not. Update specs.rs to declare proper styles (and add an Elixir-specific extraction path for `@doc` strings). Even though PHP/Scala work today, the EMPTY declaration suggests the author thought there was no doc style at all, which is wrong; future maintainers should see the truth.
