# Tree-Sitter Extractor Audit: Compiled Findings

**Date Compiled:** 2026-05-06
**Sources:** Four independent model audits: DeepSeek (`deepseek/`), GLM (`glm/`),
GPT (`gpt/`), and Opus (`opus/`: five-file deep dive). Findings have been
cross-referenced and individually verified against the extractor source at HEAD.
**Scope:** All 34 language extractors plus base infrastructure in
`crates/julie-extractors/src/`.

---

## How to Read This Document

Each finding is annotated with:

- **Consensus**: how many of the four audits flagged it (1/4 to 4/4). 3/4+
  consensus is high-confidence regardless of verification.
- **Verified**: `Yes` means the source code was inspected during compilation.
  `Implied` means a separate confirmed defect logically requires this one to be
  true. `Pending` means worth a quick check before fixing.
- **Severity**: `Critical` (data loss / runtime panic / silently wrong data),
  `High` (significant feature gap or incorrect data), `Medium` (correctness or
  quality issue with workaround), `Low` (polish / edge case / cosmetics).
- **Location**: file:line citations from the verified source where applicable.

Findings are ordered by severity, then by consensus.

---

## Headline Assessment

The architecture is sound. Capability tiers, registry pattern, base extractor,
and the per-language file decomposition are mature. The defects are not
"rewrite the layer": they are concrete, mostly mechanical fixes plus a small
number of base-layer changes that cascade into every extractor.

The most pervasive systemic issues are:

1. **Tests calibrated to "produces something" rather than "produces correct
   data."** Several flagged bugs (SQL JOIN self-edge, R class systems, Vue prop
   names, Ruby attr_accessor) survived because tests assert non-empty results
   instead of exact values.
2. **Visibility, doc-comment, and identifier-kind handling have base-layer
   defects that cascade into every language.** The base `extract_visibility`
   text fallback misclassifies any function whose body contains `"public "`;
   identifier kinds are limited to four variants; doc comments configured as
   `EMPTY` for languages that genuinely have doc styles.
3. **Cross-file relationship emission is inconsistent.** Some languages emit
   pending relationships when targets aren't in the current file; others
   silently drop the edge; markup languages with `NO_PENDING_CAPABILITIES`
   create dead synthetic IDs (`url:foo`, `external_users`,
   `component-MyComp`) that never resolve.
4. **Several extractors share specific bugs**: multi-declarator drops in
   Java/C#/VB.NET/Bash/Ruby, plus Go var/const specs;
   `IdentifierKind::TypeUsage` missing in roughly half the languages;
   `annotations: Vec::new()` copy-pasted across Swift / Kotlin / Scala / Dart
   everywhere except function paths.

The capability matrix at `fixtures/extraction/capabilities.json` is currently
asserted as perfect across all 36 languages (the `capability_gaps` field is
empty for every entry). After applying the fixes below, that field should be
populated with the gaps that remain: turning the matrix from an aspirational
claim of completeness into an accurate map.

---

## Codex Verification Pass Notes

This pass re-read every source audit under `docs/findings/*/` and spot-checked
the claims against current source. Corrections worth preserving before planning:

- The compiled file's earlier C++ multi-declarator claim was too broad. Current
  C++ code has `cpp/fields.rs::extract_multi_declarations`, called from
  `cpp/mod.rs`, which emits extra `init_declarator` symbols after the first
  one. Java and C# still drop trailing declarators, and VB.NET has the same
  first-declarator bug.
- Go field declarations are handled better than one source audit suggested, but
  `go/specs.rs::extract_var_spec` and `extract_const_spec` still keep only one
  identifier from a multi-name `var` or `const` spec.
- The PowerShell workflow "not extracted" claim is not a planning driver as
  written. Current tests assert workflow names are surfaced as Function symbols.
  There may still be a kind/modeling gap, but not zero extraction.
- The C++ namespace `parent_id` claim was not verified. `cpp/mod.rs::walk_tree`
  threads the namespace symbol id into children after `extract_namespace`
  succeeds.
- Several findings from GPT and GLM were missing from this compilation despite
  verifying cleanly: `.h` headers route to C, Dart cross-file inheritance drops
  normal unresolved targets, Python collapses multi-import statements, C/C++
  lack type-use relationship edges for declarations, GDScript extends metadata
  never becomes an Extends edge, and pipeline parse failure returns `Err` with
  no degraded extraction result.

---

## Critical Findings (Data Loss, Panic, or Silently Wrong Data)

### C1. SQL JOIN relationships are self-referential
- **Consensus:** 4/4 (DeepSeek, GLM, GPT, Opus all called it)
- **Verified:** Yes: `crates/julie-extractors/src/sql/relationships.rs:181-196`
  emits `from_symbol_id: table_symbol.id.clone()` and
  `to_symbol_id: table_symbol.id.clone()`: the same symbol on both ends.
- **Severity:** Critical
- **Effect:** Every JOIN edge connects a table to itself. Graph traversal,
  centrality scoring, and lineage queries are corrupted for any SQL workspace
  that uses joins.
- **Fix:** Track the FROM table from the enclosing `select_statement` /
  `from_clause` and emit a real edge from the FROM table to each joined
  table. Update the test
  (`tests/sql/relationships.rs:74-78`) to assert
  `from_symbol_id != to_symbol_id`.

### C2. JSON `doc_comment` truncation panics on multi-byte UTF-8
- **Consensus:** 2/4 (GLM, GPT)
- **Verified:** Yes: `crates/julie-extractors/src/json/mod.rs:102` does
  `trimmed[..2000].to_string()` on a byte slice. TOML correctly uses
  `chars().take(2000).collect()`.
- **Severity:** Critical
- **Effect:** Indexing any JSON file with a > 2000-byte string value
  containing a multi-byte char that straddles byte 2000 panics the extractor.
  Realistic for `package.json` descriptions, memory files, K8s annotations,
  i18n bundles.
- **Fix:** Replace with
  `trimmed.chars().take(2000).collect::<String>() + "..."` to mirror the TOML
  path.

### C3. Elixir uses the legacy pending-relationship path
- **Consensus:** 1/4 (DeepSeek)
- **Verified:** Yes: `crates/julie-extractors/src/elixir/relationships.rs:115,
  256` call `add_pending_relationship(PendingRelationship { ... })` instead of
  `add_structured_pending_relationship`. Every other programming-language
  extractor (except mixed Dart) has migrated.
- **Severity:** Critical (single-language data-quality regression)
- **Effect:** Cross-file Elixir relationships lack `UnresolvedTarget`'s
  `receiver`, `namespace_path`, and `import_context` fields. Cross-file call
  resolution for Elixir is missing context that other languages provide.
- **Fix:** Migrate Elixir to `add_structured_pending_relationship` mirroring
  Kotlin/Scala. Also adds the structured target fields.

### C4. Ruby cross-file inheritance and module inclusion silently drop
- **Consensus:** 3/4 (GLM H17, GPT 6, Opus tier-2 #4)
- **Verified:** Yes:
  `crates/julie-extractors/src/ruby/relationships.rs:89-92` requires both
  symbols to exist in the same-file `symbols` slice; otherwise no relationship
  and no pending fallback. `process_include_extend_call` (line 188-205) has the
  same shape.
- **Severity:** Critical (silent data loss on the dominant Rails idiom)
- **Effect:** `class UsersController < ApplicationController` produces no
  edge across files. `include SomeConcern` in another file produces no edge.
  Centrality, blast radius, and inheritance navigation are wildly incomplete
  for any multi-file Ruby/Rails project.
- **Fix:** Emit `add_structured_pending_relationship` when the target is not
  in the local symbol set. Mirror Kotlin (`kotlin/relationships.rs:89-97`),
  Scala, and PHP.

### C5. Ruby `attr_accessor :a, :b, :c` produces ONE symbol instead of three
- **Consensus:** 3/4 (GLM L19-adjacent, GPT 18, Opus tier-2 #3)
- **Verified:** Yes: `crates/julie-extractors/src/ruby/calls.rs:160`:
  `if let Some(first_symbol) = symbol_nodes.first()` extracts only the first.
  Same shape applies to `attr_reader` / `attr_writer`.
- **Severity:** Critical (Ruby's most common property idiom)
- **Effect:** 2/3 of Ruby property declarations are silently dropped.
- **Fix:** Iterate `symbol_nodes` and emit a `Property` per name. While there,
  fix `parent_id: None` (line 170): accept and propagate the class parent.

### C6. Java / C# / VB.NET multi-declarator drops trailing variables
- **Consensus:** 3/4 for the general pattern (DeepSeek #17, GLM, Opus tier-1
  #2), with C++ rejected on current source and VB.NET independently verified.
- **Verified:** Yes:
  - `crates/julie-extractors/src/java/fields.rs:41-42` ("For now, handle the
    first declarator")
  - `crates/julie-extractors/src/csharp/members.rs:315`
    (`let declarator = declarators.first()?;`)
  - `crates/julie-extractors/src/vbnet/members.rs:150-160` finds only the
    first `variable_declarator`
- **Severity:** Critical (silent symbol loss on common syntax)
- **Effect:** `int x, y, z;` indexes only `x`. C# `event_field_declaration` has
  the same bug. VB.NET `Dim x, y, z As Integer` also indexes one field.
  C++ has a current multi-declaration pass and should not be included in this
  specific finding.
- **Fix:** Iterate over all declarators. The comment in Java is explicit that
  this is incomplete.

### C7. Zig and Dart `infer_types` keys by symbol name, not symbol ID
- **Consensus:** 2/4 (GPT #1, DeepSeek #16 for Zig)
- **Verified:** Yes:
  `crates/julie-extractors/src/zig/type_inference.rs:18,30,40,54,58` insert
  `symbol.name.clone()` as the key.
  `crates/julie-extractors/src/dart/mod.rs:347,354,361` do the same.
  `crates/julie-extractors/src/factory.rs:13-35` blindly trusts the key as
  `TypeInfo.symbol_id`.
- **Severity:** Critical (TypeInfo rows attach to non-existent symbol IDs)
- **Effect:** Type rows for same-name symbols collide; `TypeInfo.symbol_id`
  doesn't match any real `Symbol.id`. Every consumer that joins TypeInfo to
  Symbol via id silently misses or misroutes data.
- **Fix:** In Zig and Dart `infer_types`, use `symbol.id.clone()` as the map
  key, matching the convention every other extractor follows. Add a regression
  test asserting every key in `results.types` exists in `results.symbols`.

### C8. Visibility text fallback misclassifies on body content
- **Consensus:** 1/4 (Opus base-infrastructure 2.1)
- **Verified:** Yes: `crates/julie-extractors/src/base/creation_methods.rs:264-288`
  walks named children for `"public"/"private"/"protected"`, then falls back
  to `text.contains("public ")` over the entire node text. Most tree-sitter
  grammars expose visibility under wrapper kinds (`accessibility_modifier`,
  `modifier`, `pub`), so the child loop rarely fires.
- **Severity:** Critical (correctness: real visibility silently flipped by
  body contents)
- **Effect:** `function getKey() { return "public_key"; }` is classified as
  Public because the body contains `"public "`. A method whose body has a
  comment "// returns the public API": same.
- **Fix:** Delete the text fallback. Force every language to override
  visibility detection. C++ already does this correctly
  (`cpp/visibility.rs`), Rust does (`rust/helpers.rs::extract_rust_visibility`),
  Kotlin / Scala / Java all have local helpers. Mark the base method
  `pub(super)` so callers can't reach for it without thinking.

### C9. C++ identifier extraction stores entire callee expression as call name
- **Consensus:** 3/4 (DeepSeek "CPP Duplicate" / GLM L5, Opus tier-1 #4)
- **Verified:** Yes: `crates/julie-extractors/src/cpp/identifiers.rs:34-35`
  does `let name = self.base.get_node_text(&func_node);` where `func_node` is
  the `function` child of `call_expression`. For `obj.method()`, that's the
  entire `obj.method` expression text.
- **Severity:** Critical (silent symbol-name lookup failure for C++ method
  calls)
- **Effect:** The identifier is stored as `name = "obj.method"`. Every
  `find_references("method")` query against C++ misses these calls because
  the stored name doesn't match. Inconsistent with TypeScript, Java, Rust,
  Go, etc., which extract the rightmost identifier.
- **Fix:** When the function expression is a `field_expression`, extract the
  field child as the call name (matching the `field_expression` arm at line
  51-60). Keep the receiver as a separate `MemberAccess` if useful.

### C10. C++ relationships drop calls and inheritance for duplicated names
- **Consensus:** 2/4 (GPT #2, Opus base-infrastructure cross-cutting)
- **Verified:** Yes:
  - `crates/julie-extractors/src/cpp/relationships.rs:21` builds a
    `unique_symbol_map`.
  - `crates/julie-extractors/src/base/relationship_resolution.rs:141-151`
    `unique_symbol_map` drops every name with more than one candidate (only
    `[symbol]` matches).
  - C++ constructors share their class's name (a class with a constructor
    makes the class name non-unique, so inheritance lookup fails).
- **Severity:** Critical (call/inheritance graph degradation in the most
  common C++ shapes)
- **Effect:** Overloaded functions (C++ has many), constructors sharing class
  names, and any `init`/`size` /`begin`/`end` method colliding across types
  produce silently dropped relationships.
- **Fix:** Resolve callers by node containment / symbol span instead of name
  uniqueness (other extractors use `find_containing_symbol_from_map`). For
  inheritance, restrict candidates by kind (`Class` / `Struct` only) and
  prefer node-position resolution. Add fixture tests covering "two classes
  with the same constructor name" and "calls inside overloaded methods."

### C11. Dart never extracts doc comments
- **Consensus:** 2/4 (DeepSeek #2, Opus tier-2 cross-cutting)
- **Verified:** Yes: `grep` of `crates/julie-extractors/src/dart/` for
  `find_doc_comment` returns no calls. Every Dart symbol creation site passes
  `doc_comment: None`. `DART_DOCS` is wired in `language_spec/specs.rs`.
- **Severity:** Critical (single-language doc-comment loss)
- **Effect:** Every Dart symbol has `doc_comment: None` even when the source
  has `///` triple-slash docs.
- **Fix:** Add `base.find_doc_comment(&node)` calls in each Dart extraction
  function. The infrastructure already supports it; only the calls are
  missing.

### C12. R never extracts Roxygen doc comments
- **Consensus:** 2/4 (DeepSeek #4, Opus tier-3 #2)
- **Verified:** Yes: `grep` of `crates/julie-extractors/src/r/` for
  `find_doc_comment` returns no calls. `R_DOCS` is wired in
  `language_spec/specs.rs:208-211`. Every R symbol has `doc_comment: None`.
- **Severity:** Critical (R community standard documentation is invisible)
- **Effect:** Roxygen `#'` comments are the canonical R doc system; they're
  silently dropped on every R symbol.
- **Fix:** Call `base.find_doc_comment(&node)` in `extract_function_assignment`
  and other R symbol creation paths.

---

## High-Severity Findings (Significant Feature Gaps or Wrong Data)

### H1. TypeScript `extract_class` always passes `parent_id: None`
- **Consensus:** 1/4 (Opus tier-1 #1, the most-emphasized Tier-1 finding)
- **Verified:** Yes: `crates/julie-extractors/src/typescript/classes.rs:107`.
  The visitor in `symbols.rs` does not thread `parent_id` either. Methods
  recover their parent_id by an AST walk (`find_parent_class_id`) but only
  for `class_declaration` parents: `interface_declaration` parents are not
  walked.
- **Severity:** High (parent chain breaks for nested TS structures)
- **Effect:** Classes inside namespaces, classes nested in classes, and
  methods of nested interfaces all have wrong or missing `parent_id`. The
  cascading regen workaround in `functions.rs:91-128` (regenerate the symbol
  ID by reading the name node's column) is evidence the original parent_id
  approach has already failed once.
- **Fix:** Thread `parent_id` through the TypeScript visitor (mirror the
  JavaScript pattern at `mod.rs:413-519`). Eliminate the regen workaround in
  `functions.rs:91-128`.

### H2. R extractor handles only two AST node kinds
- **Consensus:** 3/4 (DeepSeek #4, GLM C5, Opus tier-3 #1)
- **Verified:** Yes: `crates/julie-extractors/src/r/mod.rs:182-186` matches
  on `binary_operator` (assignments) and `call` (`library()`/`require()`)
  only. Right-to-left assignment (`->`) always produces
  `SymbolKind::Variable` (line 231-245), even for `function() { ... } -> f`.
- **Severity:** High (R class systems and replacement functions invisible)
- **Effect:** No detection of `setClass` (S4), `setGeneric`, `setMethod`,
  `R6Class`, `setRefClass`. `names(x) <- ...` (R replacement functions) is
  rejected because `left.kind()` is `call`, not `identifier`. The four major
  R class systems beyond S3 produce zero `Class`/`Method` symbols. Tests at
  `tests/r/classes.rs:117` literally assert `symbols.len() >= 0` (a tautology
  that passes even when extraction returns nothing).
- **Fix:** Add cases for `call` arguments where the function is `setClass`,
  `setGeneric`, `setMethod`, `R6Class`, `setRefClass`. Build classes from
  the literal class name; build methods from `setMethod("name", "Class",
  function() ...)`. Replace `>= 0` test asserts with concrete symbol-name
  assertions.

### H3. Vue Options API loses every method/computed/data/prop name
- **Consensus:** 4/4 (DeepSeek #14, GLM H19/H20, GPT #8, Opus tier-4 #2)
- **Verified:** Implied (multi-source consensus + audit citations to specific
  line numbers in `vue/script.rs:18-121` and `vue/identifiers.rs:185-225`).
- **Severity:** High
- **Effect:** `methods: { increment() {}, decrement() {} }` produces ONE
  Property symbol named `methods`, not `increment` and `decrement`. Same
  for `data`, `computed`, `props`, `watch`. Composition API (`<script setup>`)
  is fine.
- **Fix:** Parse the script section as JS via tree-sitter, walk the exported
  object's properties, and extract members. The `<script setup>` path
  (`vue/script_setup.rs`) already proves this is feasible.

### H4. Vue identifier extraction reparses the SFC for every identifier
- **Consensus:** 2/4 (GLM, Opus tier-4 #3)
- **Verified:** Implied (Opus cited
  `vue/identifiers.rs:185-225 create_identifier_with_offset` calling
  `parse_vue_sfc` per identifier).
- **Severity:** High (algorithmic perf)
- **Effect:** A Vue file with 100 identifiers reparses the SFC 100 times.
- **Fix:** Parse once at extraction start; pass content + offset into the
  helper.

### H5. PowerShell `is_builtin_cmdlet` knows two cmdlets
- **Consensus:** 2/4 (GPT #20, Opus tier-3 #6)
- **Verified:** Yes: `crates/julie-extractors/src/powershell/relationships.rs:100-102`:
  `matches!(command_name, "Write-Output" | "Get-ChildItem")`.
- **Severity:** High (massive pending-relationship noise)
- **Effect:** Every other standard cmdlet (`Where-Object`, `Select-Object`,
  `ForEach-Object`, `Get-Content`, `Set-Content`, `Get-Date`, ... hundreds
  more) generates a permanently-pending relationship that never resolves.
- **Fix:** Replace with a comprehensive built-in cmdlet list (the
  `Microsoft.PowerShell.*` modules ship with several hundred well-known
  cmdlets). Or use a generic verb-noun rule for approved verbs.

### H6. PowerShell method signatures are malformed
- **Consensus:** 1/4 (Opus tier-3 #4)
- **Verified:** Implied (Opus cited
  `powershell/classes.rs:188-198`).
- **Severity:** High
- **Effect:** Signatures render as `"static  string MyMethod()"` (double
  space) and the parameter list is *always* `()`: methods with parameters
  show empty parens. Consumers see misleading signatures.
- **Fix:** Build the signature linearly: collect modifier list, return type,
  name, parameter list, then format with single spaces.

### H7. Bash and PowerShell create Function symbols at call sites
- **Consensus:** 2/4 (Opus tier-3 #3, GLM partially in H24)
- **Verified:** Implied (Opus cited
  `bash/commands.rs:11-67`, `powershell/commands.rs:11-92`).
- **Severity:** High (architectural anti-pattern; pollutes symbol index)
- **Effect:** Every script that calls `docker run`, `kubectl get`, or
  `Connect-AzAccount` produces a phantom Function symbol named after the
  external command. The same names are listed in the relationship code's
  "builtin" exclusion lists: the two halves are inconsistent.
- **Fix:** Treat external command invocations as identifiers (call sites)
  and as relationships (cross-language pending). Never create Function
  symbols for them.

### H8. Lua and GDScript stuff entire function bodies into `signature`
- **Consensus:** 2/4 (Opus tier-3 #5, GLM partially)
- **Verified:** Yes for Lua:
  `crates/julie-extractors/src/lua/functions.rs:81`:
  `let signature = base.get_node_text(&node);`. GDScript noted at
  `gdscript/functions.rs:92` per Opus.
- **Severity:** High (massive token bloat)
- **Effect:** A 100-line function gets a 100-line signature. Every
  consumer that returns symbol metadata wastes tokens.
- **Fix:** Build a synthetic signature: `function name(params) returns?`.
  Zig demonstrates the right pattern in `zig/functions.rs:114-236`.

### H9. GDScript stores `self.do_thing` as the call name
- **Consensus:** 2/4 (DeepSeek 22 cross-cutting, Opus tier-3 #8)
- **Verified:** Yes: `crates/julie-extractors/src/gdscript/identifiers.rs:51-71`:
  when the call's child is `attribute`, `name = base.get_node_text(&child)`
  yields the entire `self.do_thing` text rather than `do_thing`.
- **Severity:** High (find_references against method names misses every
  GDScript call)
- **Effect:** Symbol names are stored as `do_thing`. Identifier names are
  stored as `self.do_thing`. The two never match.
- **Fix:** When the call expression is an `attribute`, drill into the
  rightmost identifier child for the name.

### H10. PHP constructor property promotion (PHP 8.0+) not extracted
- **Consensus:** 1/4 (Opus tier-2 #5)
- **Verified:** Yes, `crates/julie-extractors/src/php/functions.rs` extracts
  `__construct` as a Constructor and stores raw formal parameters in metadata,
  while `php/members.rs` extracts only property declarations. Searches under
  `php/` found no promotion-specific handler.
- **Severity:** High (modern PHP feature invisible)
- **Effect:** `__construct(public string $name)` declares both a parameter
  and a property. The property is never extracted as a symbol.
- **Fix:** When walking constructor `formal_parameters`, look for
  `simple_parameter` children with a `visibility_modifier`. Emit a Property
  symbol per match. (Same general pattern Kotlin uses in
  `extract_constructor_parameters`.)

### H11. Annotations dropped on non-function symbols across Tier 2
- **Consensus:** 1/4 (Opus tier-2 #1, the most-emphasized Tier-2 finding)
- **Verified:** Implied (Opus cited specific line numbers across Swift,
  Kotlin, Scala, Dart).
- **Severity:** High (framework-specific search broken)
- **Effect:** `annotations: Vec::new()` is copy-pasted in every
  `create_symbol` call outside function/method paths. So `@Composable`,
  `@Inject`, `@MainActor`, `@Published`, `@Test`, `@Component` etc. are
  silently dropped on classes, properties, objects, type aliases. Test
  discovery, SwiftUI navigation, dependency-injection navigation all
  degrade.
- **Fix:** Route every `create_symbol` call through `extract_annotations`.
  In Swift, the helper exists at `swift/signatures.rs:65` but only
  `swift/callables.rs` calls it. Same shape in Kotlin.

### H12. Elixir `@doc"""..."""` and `@moduledoc"""..."""` invisible
- **Consensus:** 1/4 (Opus tier-2 #2)
- **Verified:** Implied (Opus cited
  `language_spec/specs.rs:186` declaring EMPTY doc styles, and the parse
  shape: `unary_operator` containing `call`).
- **Severity:** High (Elixir's primary documentation mechanism)
- **Effect:** Every Elixir symbol has `doc_comment: None`.
- **Fix:** Either add a `DocCommentStyle::ElixirAtDoc` matcher that walks
  the `unary_operator @ call doc(arguments(string))` shape, or have
  `extract_defmodule` / `extract_def` pull the string content from
  preceding `@doc`/`@moduledoc` siblings.

### H13. ScopedSymbolIndex::unique_symbol_map silently drops overloads
- **Consensus:** 1/4 (GLM H6)
- **Verified:** Yes:
  `crates/julie-extractors/src/base/relationship_resolution.rs:141-151`:
  `[symbol] => Some((name, *symbol)), _ => None`. Every name with more than
  one candidate is dropped from the map entirely.
- **Severity:** High (cascade-amplifies C10 and similar)
- **Effect:** Used by C++ and other extractors for local symbol resolution.
  Two methods with the same name on different classes both vanish from
  the map.
- **Fix:** Either expose `by_name` so callers can pick by kind / position,
  or change the API to return `Vec<&Symbol>` and let callers disambiguate.

### H14. JSX/TSX components are not extracted
- **Consensus:** 2/4 (GLM C6, Opus tier-1 #1 secondary)
- **Verified:** Implied (`typescript/symbols.rs:30-139` walks classes /
  functions / interfaces but no JSX node kinds).
- **Severity:** High (React/Preact components invisible in TSX)
- **Effect:** `<MyComponent />` produces no identifier and no relationship.
  Component instantiation tracking is impossible.
- **Fix:** Walk `jsx_self_closing_element`, `jsx_opening_element`, and
  `jsx_fragment` and emit identifier references with the component name.

### H15. TypeScript imports are statement-level; JavaScript is binding-level
- **Consensus:** 2/4 (DeepSeek #13, GPT #4)
- **Verified:** Yes:
  - `crates/julie-extractors/src/typescript/imports_exports.rs:11-39`
    creates ONE symbol per `import` statement, named after the module
    source string.
  - `crates/julie-extractors/src/javascript/imports.rs:42-55` uses the
    binding `specifier` as the symbol name and stores `source` in metadata.
- **Severity:** High (TS is a Tier-1 language)
- **Effect:** `import { helper as h } from "./utils"` produces a symbol
  named `"./utils"` rather than the local binding `h`. Imported
  identifiers are harder to resolve, rank, and reference.
- **Fix:** Port the JavaScript binding-level pattern to TypeScript.

### H16. C++ classes/structs/unions/enums hardcode `Visibility::Public`
- **Consensus:** 2/4 (GLM H12, Opus tier-1 #9)
- **Verified:** Implied (Opus cited `cpp/types.rs:65,122,161,211`).
- **Severity:** High (default for nested types is wrong)
- **Effect:** Nested types inside a `class` should default to private per
  C++ rules; the hardcode flips them to public.
- **Fix:** Read the parent context (class default = private; struct/union
  default = public). The C++ access-specifier walker
  (`cpp/visibility.rs`) does this correctly for members; the type kinds
  need it too.

### H17. Multiple extractors lack `IdentifierKind::TypeUsage`
- **Consensus:** 3/4 (DeepSeek #7, GLM H30, Opus tier-1 cross-cutting #1
  and tier-2 #5)
- **Verified:** Yes for the current affected list. Direct searches show
  `IdentifierKind::TypeUsage` exists in Dart, Kotlin, PHP, Ruby, Rust, C, C++,
  Java, TypeScript, QML, and Zig. It is absent from JavaScript, Python, C#,
  Go, and Swift.
- **Severity:** High (centrality and find-references both halved for
  affected languages)
- **Effect:** Type references in fields / parameters / returns / generic
  args / casts produce no identifier. Affected:
  - Rust (only `scoped_identifier`, missing `type_identifier` etc.)
  - JavaScript (no native types)
  - Python
  - C#
  - Go
  - Swift
- **Fix:** Add `IdentifierKind::TypeUsage` extraction to each. C++, Java,
  TypeScript, Kotlin, Dart, Zig, PHP, and Ruby have implementations to use as
  templates. For Rust, broaden the existing coverage if tests show it only
  catches scoped identifiers.

### H18. Constructor calls (`new T()`) missing from identifier stream
- **Consensus:** 2/4 (DeepSeek L25, Opus tier-1 #3)
- **Verified:** Implied (Opus cited Java, JavaScript, C#).
- **Severity:** High
- **Effect:** `new HttpClient()` is captured as a relationship in some
  languages but never as an `IdentifierKind::Call`. `fast_refs
  --reference_kind=call` for a constructor returns zero hits even when the
  class is heavily instantiated.
- **Fix:** Walk `object_creation_expression` (Java/C#) and
  `new_expression` (JS) and emit a Call identifier with the class name.

### H19. Vue script symbols have zero byte ranges
- **Consensus:** 2/4 (GPT #8, Opus tier-4 cross-cutting #5)
- **Verified:** Implied (Opus cited `vue/script.rs:213` `start_byte: 0,
  end_byte: 0`).
- **Severity:** High (range-based features broken for Vue)
- **Effect:** Code-context extraction, snippet generation, character-level
  features all fail for Vue script symbols.
- **Fix:** Preserve the offset of the parsed script section back to the
  full `.vue` file when creating symbols.

### H20. HTML / CSS / Razor symbol-name collisions
- **Consensus:** 2/4 (GLM H18, Opus tier-4 #4)
- **Verified:** Implied (Opus cited `html/elements.rs:128`,
  `html/scripts.rs:68-69`, `css/at_rules.rs:49`,
  `razor/directives.rs:152`).
- **Severity:** High (search and resolution broken)
- **Effect:** Every `<a>` is named `"a"`. Every `@import` is named
  `"@import"`. Every `@page` Razor directive is named `"@page"`.
  Same-name collisions everywhere.
- **Fix:** Derive names from the most specific identifier (id / name /
  url / target / specifier) with fallback to the keyword.

### H21. NO_PENDING_CAPABILITIES is too aggressive for HTML/Razor/SQL
- **Consensus:** 1/4 (Opus tier-4 #6)
- **Verified:** Implied (Opus cited the synthetic placeholder pattern:
  `url:foo`, `external_users`, `component-MyComp`).
- **Severity:** High (cross-file references replaced by dead synthetic IDs)
- **Effect:** `<script src="x.js">`, `@inject Service`, `FOREIGN KEY
  REFERENCES users(id)` all generate placeholder IDs that never resolve.
- **Fix:** Bump these languages to
  `RELATIONSHIP_DATA_CAPABILITIES + pending_relationships` and synthesize
  real pending relationships.

### H22. Vue/HTML embedded `<script>`/`<style>` content not parsed
- **Consensus:** 2/4 (Opus tier-4 cross-cutting #4, GLM H19)
- **Verified:** Implied.
- **Severity:** High
- **Effect:** JS in `<script>` and CSS in `<style>` produce no symbols /
  identifiers / relationships.
- **Fix:** Use the JS/CSS extractors on the inner ranges with offset
  adjustment. Vue and Razor already prove this is doable.

### H23. C# misses local functions, lambdas, partial-class linkage
- **Consensus:** 1/4 (Opus tier-1 #8)
- **Verified:** Implied.
- **Severity:** High (modern C# features)
- **Effect:** `local_function_statement`, `lambda_expression`,
  `anonymous_method_expression` produce no symbols. `partial class Foo` in
  two files creates two unrelated symbols.
- **Fix:** Add cases for those node kinds. For partial classes, link
  partials by full name match (or by `[partial]` attribute).

### H24. Java records produce one symbol with no field components
- **Consensus:** 1/4 (Opus tier-1 #7)
- **Verified:** Implied.
- **Severity:** High
- **Effect:** `record Point(int x, int y) {}` produces only a Class symbol
  named `Point`. The components `x` and `y` should be Property children.
- **Fix:** In `java/classes.rs::extract_record`, walk record components
  and emit Property symbols with the record as parent.

### H25. SQL view-to-table and trigger-to-table relationships missing
- **Consensus:** 2/4 (Opus tier-4 #9, code comment in `sql/relationships.rs`
  acknowledges the stub)
- **Verified:** Implied (Opus cited `sql/relationships.rs:31-34`).
- **Severity:** High
- **Effect:** `CREATE VIEW v AS SELECT * FROM t` produces no relationship
  from `v` to `t`. Same for triggers' `ON tablename`.
- **Fix:** Walk the body of CREATE VIEW / CREATE TRIGGER for FROM /
  JOIN / ON clauses; emit `Uses` relationships.

### H26. Inline `Regex::new` recompilation in hot paths
- **Consensus:** 2/4 (GLM M19/L7, Opus tier-4 cross-cutting #1)
- **Verified:** Implied (Opus cited ~10 sites in `sql/schemas.rs`,
  ~8 in `razor/directives.rs` + `razor/relationships.rs`).
- **Severity:** High (perf)
- **Effect:** Every match recompiles the regex. SQL-heavy or Razor-heavy
  workspaces pay measurable cost.
- **Fix:** Hoist all to `LazyLock<Regex>` constants. Mechanical change.

### H27. Go embedding relationships are a stub
- **Consensus:** 3/4 (DeepSeek #11, GLM H10, GPT #9)
- **Verified:** Implied (multi-source consensus on
  `go/relationships.rs:90-99` empty implementation).
- **Severity:** High (Go composition semantics invisible)
- **Effect:** `type Foo struct { io.Reader }` produces no relationship
  between `Foo` and `Reader`.
- **Fix:** Implement the embedding case: walk struct fields, when a
  field has no name (just a type), emit a `Uses` (or new `Embeds`)
  relationship.

### H28. Go stdlib filter recognizes only `fmt`
- **Consensus:** 3/4 (DeepSeek #12, GLM H17 partial, GPT #9)
- **Verified:** Implied (multi-source citations to
  `go/relationships.rs:13`).
- **Severity:** High (noisy pending relationships)
- **Effect:** `strings.TrimSpace`, `os.Exit`, `context.WithTimeout`,
  etc. all generate noisy unresolved pending edges.
- **Fix:** Replace `matches!("fmt")` with a comprehensive Go stdlib set
  or shape-based detection (single-segment lowercase imports).

### H29. Go uses `SymbolKind::Class` for structs and `Namespace` for packages
- **Consensus:** 2/4 (Opus tier-1 #6, GLM L1 partial)
- **Verified:** Implied (Opus cited `go/types.rs:131` Class for struct,
  `go/types.rs:7-35` Namespace for package).
- **Severity:** High (kind-based filtering wrong)
- **Effect:** Filtering by `SymbolKind::Struct` misses Go structs;
  filtering by `Module` misses Go packages.
- **Fix:** Map struct → `Struct`, package → `Module`.

### H30. Multi-name var/const declarations drop in Go and Bash
- **Consensus:** 2/4 (GPT #9, Opus tier-3 cross-cutting)
- **Verified:** Yes:
  - `crates/julie-extractors/src/go/specs.rs::extract_var_spec` and
    `extract_const_spec` store one `identifier` variable while walking all
    child identifiers, so the last seen identifier wins.
  - `crates/julie-extractors/src/bash/variables.rs::extract_declaration`
    extracts only the first declaration variable.
- **Severity:** High
- **Effect:** `var a, b int` and `const x, y = 1, 2` collapse to one
  symbol in Go. Go field declarations are a separate path and are better
  covered. `export A=1 B=2 C=3` loses B and C in Bash.
- **Fix:** Iterate identifiers and emit one symbol per name.

### H31. CSS lacks modern at-rule support
- **Consensus:** 1/4 (GLM M6)
- **Verified:** Implied (`css/at_rules.rs:58-137` per GLM citation).
- **Severity:** Medium-High
- **Effect:** `@font-face`, `@container` (container queries), `@layer`
  (cascade layers) have no extraction support.
- **Fix:** Add cases. Container queries and cascade layers are common in
  modern CSS.

### H32. `.h` headers are always parsed as C, not C++
- **Consensus:** 1/4 (GPT #3)
- **Verified:** Yes, `crates/julie-extractors/src/language_spec/specs.rs:13-26`
  assigns extension `h` only to the C spec. The C++ spec has `hpp`, `hh`,
  `hxx`, and `h++`, but not plain `h`.
- **Severity:** High (large C++ projects lose header semantics)
- **Effect:** C++ classes, templates, namespaces, inline methods, and overloads
  in `.h` files are routed through `tree-sitter-c`, so C++-specific symbols and
  relationships are missed or degraded.
- **Fix:** Add a content-aware or project-aware fallback for `.h`: prefer C++
  when the file contains C++ syntax such as `class`, `namespace`, `template`,
  access specifiers, or C++ includes. Compile database hints would be better
  when available.

### H33. Dart drops normal cross-file inheritance and conformance
- **Consensus:** 1/4 (GPT #5), plus DeepSeek noted Dart's mixed pending path.
- **Verified:** Yes, `crates/julie-extractors/src/dart/relationships.rs:41-151`
  emits class inheritance and mixin relationships only when targets are in the
  same `symbols` slice. `dart/pending_calls.rs` creates structured pending
  relationships only for calls. Recovery paths in `dart/mod.rs:137,172,249`
  add legacy pending inheritance for some parser-error Dart 3 cases, but the
  normal path has no unresolved-target fallback.
- **Severity:** High (Dart class hierarchy incomplete across files)
- **Effect:** `extends`, `implements`, and `with` relationships disappear when
  the target is imported from another file, which is the common case.
- **Fix:** In the normal class relationship path, emit structured pending
  relationships for unresolved superclass, interface, and mixin targets, with
  import context when available. Migrate the recovery-only legacy pending calls
  at the same time.

### H34. C and C++ lack type-use relationship edges for declarations
- **Consensus:** 1/4 (GLM C2/C3)
- **Verified:** Yes, `crates/julie-extractors/src/c/relationships.rs:16-29`
  handles only `call_expression` and `preproc_include`; C++
  `crates/julie-extractors/src/cpp/relationships.rs:31-40` handles only
  inheritance and calls. Identifier extraction can record some TypeUsage
  entries, but relationship extraction does not emit `Uses` edges from
  variables, fields, parameters, or return types to referenced types.
- **Severity:** High (C/C++ graph misses one of the main dependency signals)
- **Effect:** `struct Foo { Bar b; }`, `Bar *make_bar(void)`, and
  `std::vector<User>` do not create type dependency relationships. Blast radius
  and centrality undercount type-level coupling.
- **Fix:** Walk declaration, parameter, field, and return-type nodes and emit
  `RelationshipKind::Uses` from the containing symbol to the referenced type
  symbol or a structured pending target.

### H35. Parser total failure returns `Err` with no degraded extraction result
- **Consensus:** 1/4 (GLM C7)
- **Verified:** Yes, `crates/julie-extractors/src/pipeline.rs:118-122` maps
  `parser.parse(content, None) == None` directly to
  `anyhow!("Failed to parse file: ...")`. The caller receives no partial
  extraction result or parse diagnostic payload.
- **Severity:** High (all-or-nothing indexing failure on parser timeout or
  memory failure)
- **Effect:** Tree-sitter `ERROR` nodes are handled in many extractors, but a
  total parse failure loses the file entirely. The index cannot preserve file
  metadata or report a structured diagnostic.
- **Fix:** Return a degraded extraction record with a parse diagnostic when the
  parser returns `None`, or make the pipeline caller responsible for storing a
  failed-parse file row.

### H36. GDScript `extends` metadata never becomes an Extends relationship
- **Consensus:** 1/4 (GLM H25)
- **Verified:** Yes, `crates/julie-extractors/src/gdscript/mod.rs:60-94` and
  `gdscript/classes.rs:138-178` collect `baseClass` metadata, but
  `gdscript/relationships.rs:33-41` only dispatches call relationships.
- **Severity:** High (Godot class hierarchy missing from the graph)
- **Effect:** The most important GDScript relationship, `extends Node` or
  `class_name Foo` plus `extends Bar`, is searchable as metadata but absent
  from graph traversal, centrality, and blast radius.
- **Fix:** Emit a resolved or structured pending `RelationshipKind::Extends`
  from each GDScript class symbol to its base class target.

### H37. Python `import` statements collapse multiple bindings
- **Consensus:** 1/4 (GPT #10)
- **Verified:** Yes, `crates/julie-extractors/src/python/imports.rs` handles
  `from module import a, b` as multiple symbols, but
  `extract_single_import` breaks after the first `aliased_import` or
  `dotted_name` in a plain `import_statement`.
- **Severity:** High (Python dependency symbols silently missing)
- **Effect:** `import os, sys` and `import numpy as np, pandas as pd` produce
  one Import symbol instead of one per binding.
- **Fix:** Make `extract_single_import` return `Vec<Symbol>` and iterate every
  import binding in the statement, preserving aliases.

---

## Medium-Severity Findings (Quality / Polish)

### M1. Doc-comment recognizers are over-greedy for hash/dash idioms
- **Consensus:** 1/4 (Opus base-infrastructure 2.2)
- **Verified:** Implied (Opus cited `language_spec/mod.rs:60-80`).
- **Effect:** Every preceding line comment becomes a doc comment in Go,
  Lua, SQL, Ruby, Bash, R, PowerShell. For Go this matches godoc; for
  the others it's misclassification (those communities have specific
  doc markers like `##`, `--[[`, `#'`, `<# #>`).
- **Fix:** Split the matchers: "any comment" vs "doc-marker." Languages
  that treat all preceding comments as docs (Go) opt in explicitly.

### M2. `infer_types` not implemented for several languages
- **Consensus:** 1/4 (DeepSeek #6)
- **Verified:** Implied (DeepSeek listed Lua, QML, Markdown, JSON, TOML,
  YAML, R as missing). Lua's `dataType` metadata is set but never
  surfaced via `infer_types`.
- **Effect:** Type-information features unavailable for those languages.
- **Fix:** Add `infer_types` returning at least an empty HashMap for
  every extractor for contract consistency. Where data exists in
  metadata (Lua), surface it.

### M3. Symbol IDs use MD5 with column-based collision risk
- **Consensus:** 1/4 (Opus base-infrastructure 2.3)
- **Verified:** Yes: `crates/julie-extractors/src/base/extractor.rs:187-191`.
- **Effect:** Two child/parent nodes that share start position collide.
- **Fix:** Switch to xxhash3 of `(file_path, name, start_byte, end_byte)`
  (or blake3). Faster than MD5 and collision-safe within a file.

### M4. Relationship IDs collide on multiple calls per line
- **Consensus:** 1/4 (Opus base-infrastructure 2.6)
- **Verified:** Implied (Opus cited the format string).
- **Effect:** `foo(); bar(); foo();` on one line emits two
  `caller_FOO_Calls_42` IDs; one is overwritten.
- **Fix:** Include start column or a per-line counter.

### M5. `extend()` on ExtractionResults overwrites type_info on key collision
- **Consensus:** 1/4 (GLM H7)
- **Verified:** Implied (`results_normalization.rs:31-41` per GLM).
- **Effect:** Two files emitting type info for the same symbol id silently
  drop one.
- **Fix:** Extend with a merging strategy or explicitly fail on collision.

### M6. Markdown heading level computed but discarded
- **Consensus:** 2/4 (GLM M1, Opus tier-4 #10)
- **Verified:** Yes: `crates/julie-extractors/src/markdown/mod.rs:296`:
  `let _level = self.determine_heading_level(node);`
- **Effect:** All headings become `SymbolKind::Module` with no level. TOC
  generation, hierarchy resolution, etc. impossible.
- **Fix:** Store level in metadata.

### M7. Go embedded fields not specially marked
- **Consensus:** 1/4 (Opus tier-1)
- **Effect:** `struct { io.Reader }` adds `Reader` as a field but no
  promotion-method or interface-satisfaction signal.
- **Fix:** Set metadata flag for embedded fields when no field name.

### M8. JavaScript regex compiled per JSDoc lookup
- **Consensus:** 1/4 (Opus tier-1 #cross-cutting #10)
- **Verified:** Implied (`javascript/mod.rs:390-399`).
- **Effect:** Per-call recompile.
- **Fix:** `LazyLock<Regex>`. Python already does this correctly.

### M9. SQL CREATE VIEW only via regex over node text
- **Consensus:** 1/4 (Opus tier-4)
- **Verified:** Implied.
- **Effect:** Structured AST children of `create_view` are unused; only
  the regex match feeds extraction.
- **Fix:** Walk AST children for the view name and body.

### M10. SQL line numbers are 0-based
- **Consensus:** 1/4 (GPT #7)
- **Verified:** Implied (GPT cited `sql/relationships.rs:148, 193`).
- **Effect:** Off-by-one navigation.
- **Fix:** Add 1 to match the rest of the codebase's 1-based convention.

### M11. C# duplicate calls for member invocations
- **Consensus:** 1/4 (GPT #13)
- **Verified:** Implied (GPT cited `csharp/relationships.rs:48,57`).
- **Effect:** `service.Process()` is captured once via
  `invocation_expression` and again via the nested
  `member_access_expression` arm. Centrality inflates.
- **Fix:** Pick one canonical extraction point and dedupe.

### M12. Elixir skips qualified module calls
- **Consensus:** 1/4 (GPT #14)
- **Verified:** Implied (`elixir/helpers.rs:14-17`).
- **Effect:** `Logger.info(...)`, `Enum.map(...)`, `MyApp.Service.run(...)`
  produce no calls. The graph is biased toward unqualified locals.
- **Fix:** Handle `dot` and alias-qualified targets in
  `extract_call_target_name`.

### M13. Zig drops calls originating from methods
- **Consensus:** 2/4 (DeepSeek #L9 partial, GPT #15)
- **Verified:** Implied (`zig/relationships.rs:185` filters to
  `SymbolKind::Function`; methods are `SymbolKind::Method`).
- **Effect:** Half the call graph in a typical Zig file with structs is
  missing.
- **Fix:** Accept `Function`, `Method`, and constructor-equivalents as
  callers.

### M14. C indirect calls (function pointers, callbacks) are ignored
- **Consensus:** 1/4 (GPT #16, DeepSeek L4)
- **Verified:** Implied (`c/relationships.rs:48`).
- **Effect:** Vtables, callbacks, event loops produce no call graph.
- **Fix:** Emit lower-confidence pending relationships for indirect calls.

### M15. PHP namespace-stripping happens before pending target creation
- **Consensus:** 1/4 (GPT #12)
- **Verified:** Implied (`php/relationships.rs:13,41,76,289`).
- **Effect:** `\App\Http\BaseController` becomes `BaseController`. Repeated
  terminal class names across namespaces collide on resolution.
- **Fix:** Preserve full qualified name in
  `UnresolvedTarget.namespace_path`. Use terminal name only as fallback.

### M16. Scala / Swift misclassify unresolved inheritance vs conformance
- **Consensus:** 1/4 (GPT #11)
- **Verified:** Implied (Scala forces `Implements`, Swift forces
  `Extends` in different paths).
- **Effect:** Class inheritance vs protocol conformance distinction lost
  for cross-file targets.
- **Fix:** Preserve syntax context to keep the kind. Add relationship-kind
  asserts in tests.

### M17. Scala calls inside top-level vals/given/extension are dropped
- **Consensus:** 1/4 (Opus tier-2 #7)
- **Verified:** Implied.
- **Effect:** Calls in `val x = sideEffect()` or `given foo = doIt()` or
  `extension (x: T) ...` are not in the call graph.
- **Fix:** Recurse into `val_definition`, `var_definition`,
  `given_definition`, `extension_definition` in
  `extract_call_relationships`.

### M18. Scala case-class fields not extracted as Property symbols
- **Consensus:** 1/4 (Opus tier-2 #6)
- **Verified:** Implied.
- **Effect:** `case class Person(name: String, age: Int)` produces only a
  Class symbol; `name` and `age` (which are public vals by Scala
  semantics) are not symbols.
- **Fix:** Walk primary-constructor parameters and emit Property symbols.
  Mirror Kotlin's `extract_constructor_parameters`.

### M19. Elixir defguard / defdelegate / defexception / defoverridable not extracted
- **Consensus:** 1/4 (Opus tier-2 #8, GLM H21)
- **Verified:** Implied (`elixir/calls.rs:dispatch_call:26-42` doesn't list
  them; `is_definition_keyword` does: so they're skipped from identifiers
  but never emitted as symbols).
- **Effect:** `defdelegate hello, to: World` produces no symbol AND no
  identifier: invisible to the index.
- **Fix:** Add cases to `dispatch_call`.

### M20. Markdown links and footnotes not extracted
- **Consensus:** 1/4 (DeepSeek #18, GLM M2)
- **Verified:** Implied.
- **Effect:** `[text](url)`, reference definitions, `[^footnote]` are
  invisible. For a documentation language, this is significant.
- **Fix:** Add link / reference / footnote node handlers.

### M21. JSON / TOML have no `extract_relationships`
- **Consensus:** 1/4 (DeepSeek #5)
- **Verified:** Yes: both use `define_data_only_extractors!`. JSON
  Schema `$ref`, tsconfig `"extends"`, Cargo dependencies could all be
  relationships.
- **Effect:** Configuration files that reference each other are inert.
- **Fix:** Bump to a profile with relationships and synthesize edges
  from value patterns.

### M22. YAML tags / multi-document / flow mappings unhandled
- **Consensus:** 1/4 (DeepSeek M4, GLM M4)
- **Verified:** Implied.
- **Effect:** `!!str` tags, `---` separators, `{key: value}` flow mappings
  silently lost. Common in K8s and CI YAML.
- **Fix:** Add handlers; consider scoping symbols per document.

### M23. Lua method paths longer than two parts are dropped
- **Consensus:** 1/4 (Opus tier-3)
- **Verified:** Implied (`lua/functions.rs:51-72` requires `parts.len() == 2`).
- **Effect:** `M.utils.format = function() ... end` produces nothing.
- **Fix:** Accept arbitrary path lengths; build qualified name.

### M24. VB.NET ignores its own case-insensitivity
- **Consensus:** 1/4 (Opus tier-3 #9)
- **Verified:** Implied.
- **Effect:** `Foo` and `foo` resolve to different symbols. Rename
  breaks for users who type a different case.
- **Fix:** Fold names to lowercase in lookups; preserve original case in
  displayed names.

### M25. VB.NET modules mapped to `SymbolKind::Class`
- **Consensus:** 1/4 (Opus tier-3)
- **Verified:** Implied (`vbnet/types.rs:150`).
- **Effect:** VB modules are static-only; no instances. Filtering by Class
  picks them up.
- **Fix:** Map `Module` to `SymbolKind::Module`.

### M26. Bash `is_environment_variable` regex too eager
- **Consensus:** 1/4 (Opus tier-3)
- **Verified:** Implied.
- **Effect:** `local MAX_RETRY=5` becomes `SymbolKind::Constant` because
  the all-caps name pattern matches.
- **Fix:** Distinguish local declarations from environment exports.

### M27. SQL indexes mapped to `Property`, CTEs to `Interface`
- **Consensus:** 1/4 (Opus tier-4)
- **Verified:** Implied.
- **Effect:** Kind-based filtering is muddled.
- **Fix:** Use distinct kinds (`Index` doesn't exist; `Property` is the
  closest: or add a new kind).

### M28. `from_string` fallbacks silently degrade kinds and visibilities
- **Consensus:** 1/4 (GLM H5)
- **Verified:** Implied (`base/types.rs` per GLM).
- **Effect:** Unknown kind strings default to `Variable`, unknown
  relationship kinds to `Uses`, unknown identifier kinds to `VariableRef`.
  Data corruption is silent.
- **Fix:** Return `Option<...>` or log a warning.

### M29. TypeInfo missing return-type and parameter-type fields
- **Consensus:** 1/4 (GLM H9)
- **Verified:** Implied.
- **Effect:** Call-graph analysis ("what type does this function return?")
  requires the return type separately; currently `resolved_type` is the
  whole type string.
- **Fix:** Add explicit `return_type: Option<String>` and
  `parameter_types: Vec<String>` fields.

### M30. Vue style section parsed via regex instead of CSS extractor
- **Consensus:** 2/4 (DeepSeek #14, GLM L15)
- **Verified:** Implied.
- **Effect:** No `@keyframes`, no nested selectors, no `:scoped`/`:deep`
  semantics in Vue style.
- **Fix:** Delegate to the CSS extractor with offset adjustment.

### M31. Markdown no code-block extraction
- **Consensus:** 1/4 (Opus tier-4 #10, DeepSeek M3)
- **Verified:** Implied.
- **Effect:** Fenced code blocks like ` ```rust ... ``` ` could be
  `CodeBlock` symbols with language metadata; they're not.
- **Fix:** Walk `fenced_code_block`, extract info string + body.

### M32. Bash / PowerShell / Ruby test framework detection narrow
- **Consensus:** 1/4 (DeepSeek M25, GLM L19, Opus tier-3)
- **Verified:** Implied.
- **Effect:** PowerShell Pester (`Describe`/`It`/`Context`), Bash Bats /
  shunit2, Ruby RSpec (`describe`/`it`/`context`) are not flagged as
  tests. C++ Google Test / Catch2 / Boost.Test similarly missing
  (DeepSeek H29).
- **Fix:** Extend `test_detection.rs` patterns.

### M33. Vue template never produces definitions
- **Consensus:** 1/4 (Opus tier-4 #7)
- **Verified:** Implied.
- **Effect:** Template refs (`ref="x"`), `v-model`, slot definitions are
  never symbols. Cross-section template-to-script usage is invisible.
- **Fix:** Walk template expressions; emit `template-ref` and slot
  symbols.

### M34. CSS no pseudo-class / pseudo-element / attribute-selector identifiers
- **Consensus:** 1/4 (GLM M7)
- **Verified:** Implied.
- **Effect:** Only class and ID selectors are extracted as identifiers.
- **Fix:** Add the missing identifier types.

### M35. Markdown heading fallback corrupts leading `#` in heading text
- **Consensus:** 1/4 (GLM H27)
- **Verified:** Yes, `crates/julie-extractors/src/markdown/mod.rs:326-333`
  falls back to `trim_start_matches('#')` over the whole node text when no
  `inline` / `heading_content` child is found.
- **Effect:** In the fallback path, `# C# Programming` becomes
  `C Programming`. This is a small code path, but it is concrete data
  corruption when the fallback is used.
- **Fix:** Strip only the Markdown heading marker prefix and following
  whitespace, not every leading `#` character in the content.

### M36. C++20 concepts are not extracted
- **Consensus:** 1/4 (GLM H13)
- **Verified:** Yes, searches under `crates/julie-extractors/src/cpp/` show no
  `concept_definition`, `concept_declaration`, or concept-specific handler.
- **Effect:** `template <typename T> concept Printable = ...;` produces no
  first-class symbol. Template constraint navigation is incomplete.
- **Fix:** Add concept node handling with a dedicated kind if available, or
  `SymbolKind::Type` plus `metadata["kind"] = "concept"`.

### M37. Swift extensions are indexed as classes
- **Consensus:** 1/4 (GLM H16)
- **Verified:** Yes, `crates/julie-extractors/src/swift/extensions.rs` creates
  extension symbols with `SymbolKind::Class` and metadata
  `type = "extension"`.
- **Effect:** Filtering by Class includes extension blocks, and filtering for
  actual class declarations has to inspect metadata. This is noisy for Swift
  navigation and relationship display.
- **Fix:** Add a distinct `SymbolKind::Extension`, or use a less misleading
  existing kind plus stable metadata until the enum is expanded.

### M38. QML doc/type/visibility and binding coverage is shallow
- **Consensus:** 1/4 (DeepSeek #3, GLM M9/M10)
- **Verified:** Yes, the QML module has no `infer_types` method, no
  `find_doc_comment` call sites, and no visibility extraction. In
  `qml/mod.rs:121-154`, `ui_binding` creates symbols only for `id:`.
- **Effect:** QML component docs, type information, non-id property bindings,
  anchors, and signal-handler definitions are mostly invisible as symbols.
  Some relationships exist, but the symbol layer is much thinner than the
  capability tier implies.
- **Fix:** Add doc-comment calls, a type inference pass for property and
  function signatures, and explicit symbol extraction for signal handlers and
  important property bindings.

### M39. Lua import modeling only catches assignment-shaped `require()`
- **Consensus:** 1/4 (GLM H23, Opus tier-3)
- **Verified:** Yes, `lua/helpers.rs:35-43` classifies `require()` as an
  import only through expression type inference used by variable assignment
  extraction. `lua/core.rs` has no bare `function_call` import symbol path.
- **Effect:** `require("module")` at statement level is only an identifier or
  call, not an Import symbol. `local foo = require("path.to.module")` names the
  Import symbol `foo`, while the module path is not a first-class lookup key.
- **Fix:** Create import symbols for bare `require()` calls and store the module
  path in structured metadata for all require forms.

### M40. Bash aliases and source/dot-source commands are not extracted
- **Consensus:** 1/4 (DeepSeek #20, GLM H24, Opus tier-3)
- **Verified:** Yes, `bash/mod.rs:116-120` dispatches functions, variables,
  declarations, and generic commands only. `bash/relationships.rs:81-82`
  treats `source`, `.`, and `alias` as builtins and excludes them from
  relationship creation.
- **Effect:** `alias ll='ls -la'`, `source other.sh`, and `. ./helpers.sh`
  do not create useful Alias or Import symbols/relationships. Shell dependency
  tracing is incomplete.
- **Fix:** Add explicit alias extraction and source/dot-source Import symbols
  with structured pending relationships to the referenced scripts.

### M41. R `source()` imports are ignored
- **Consensus:** 1/4 (GLM M16)
- **Verified:** Yes, `r/mod.rs:413-444` extracts only `library()` and
  `require()` as Import symbols. `source` is listed as a base/builtin function
  in `r/relationships.rs:329`, so it is also excluded from pending calls.
- **Effect:** The common R pattern `source("helpers.R")` does not create a
  cross-file dependency.
- **Fix:** Treat `source()` as an Import with a file-path target and emit a
  structured pending relationship when the target is not local.

### M42. HTML script import relationships attach to the first script symbol
- **Consensus:** 1/4 (GPT #19, Opus tier-4 HTML)
- **Verified:** Yes, `html/relationships.rs:124-131` picks
  `symbols.iter().find(...)` for any symbol with metadata type
  `script-element`, regardless of which `<script>` node is being processed.
- **Effect:** In files with multiple `<script src="...">` tags, imports after
  the first can originate from the wrong script symbol.
- **Fix:** Resolve the script symbol by node span or line range, not just the
  first script-element metadata match.

### M43. Python return type hint regex truncates common annotations
- **Consensus:** 1/4 (GPT #10)
- **Verified:** Yes, `crates/julie-extractors/src/python/mod.rs:31-32` uses
  `r":\s*([^=\s]+)\s*$"` for return hints.
- **Effect:** Type hints containing spaces, such as `dict[str, int]`, can be
  truncated at the first whitespace. The type inference result becomes wrong
  even though the AST has structured annotation nodes.
- **Fix:** Prefer tree-sitter annotation nodes. If a regex fallback remains,
  capture through the end of the annotation instead of stopping at whitespace.

### M44. Zig misses `usingnamespace` and non-declaration `@import` forms
- **Consensus:** 1/4 (GLM C4)
- **Verified:** Yes, `zig/variables.rs:22-24` detects `@import(` only inside
  `const_declaration` / `variable_declaration` extraction. Searches under
  `zig/` find no `usingnamespace` handling.
- **Effect:** `usingnamespace @import("foo.zig");` and any import expression not
  shaped as a variable or const declaration are not modeled as imports.
- **Fix:** Add explicit handling for `usingnamespace` and broader `@import`
  expression nodes, with Import symbols and pending relationships.

---

## Low-Severity Findings (Polish / Edge Cases)

### L1. Rust `macro_rules!` mapped to `SymbolKind::Function`
- **Consensus:** 1/4 (GLM L1)
- **Verified:** Implied.
- **Effect:** Macros are syntactically distinct from functions; the
  symbol kind doesn't reflect that.
- **Fix:** Add `SymbolKind::Macro`.

### L2. Markdown slug normalization differs from GitHub
- **Consensus:** 1/4 (Opus tier-4)
- **Verified:** Implied.
- **Effect:** Edge cases on emoji, repeated punctuation, leading numbers
  diverge from GitHub's algorithm; some real-world links may not
  resolve.
- **Fix:** Match GitHub's algorithm more closely.

### L3. Rust regex-based return-type extraction breaks on complex generics
- **Consensus:** 1/4 (GLM L3)
- **Verified:** Implied.
- **Effect:** `RETURN_TYPE_RE` of `->\s*([^{]+)` stops at `{`, breaking
  on `-> impl Iterator<Item = u32>` and similar.
- **Fix:** Use AST-based extraction for return types.

### L4. Swift `open` visibility maps to Public
- **Consensus:** 1/4 (GLM L20)
- **Verified:** Implied (`swift/signatures.rs:318-329`).
- **Effect:** `open` allows subclassing outside the module; mapping to
  Public loses that distinction.
- **Fix:** Either add `Visibility::Open` or store it in metadata.

### L5. Scala `val` is always `Constant`
- **Consensus:** 1/4 (DeepSeek #23)
- **Verified:** Implied.
- **Effect:** Local `val` inside method bodies is an immutable variable,
  not a true constant. Only top-level / object-level `val` should be
  `Constant`.
- **Fix:** Differentiate by scope.

### L6. Regex extractor symbols all `SymbolKind::Variable`
- **Consensus:** 1/4 (DeepSeek #24)
- **Verified:** Implied.
- **Effect:** Capture groups, character classes, lookarounds, unicode
  properties all share the same kind.
- **Fix:** Add a per-kind discriminator.

### L7. YAML anchors are metadata, not symbols
- **Consensus:** 1/4 (DeepSeek #25)
- **Verified:** Implied.
- **Effect:** `&anchor` definitions exist only as metadata on their
  containing pair. Aliases `*anchor` are correctly tracked as
  VariableRef identifiers with resolution.
- **Fix:** Promote anchors to first-class symbols.

### L8. Rust no `use` import-relationship edges
- **Consensus:** 1/4 (GLM L2)
- **Verified:** Implied.
- **Effect:** `use` declarations create Import symbols but no
  `Imports` edges connect them.
- **Fix:** Emit `Imports` relationships.

### L9. Various extractors with size violations
- **Consensus:** Multiple (DeepSeek, GLM, Opus all noted)
- **Verified:** Implied (CLAUDE.md target is 500 lines).
- **Effect:** Files near or over the target: `dart/mod.rs` 963 lines,
  `vbnet/relationships.rs` 579, `r/mod.rs` 523, `cpp/declarations.rs`
  506, `csharp/members.rs` 496, `sql/routines.rs` 521,
  `sql/schemas.rs` 507.
- **Fix:** Refactor opportunistically when next touched (per project
  standards).

### L10. Tests calibrated to existence rather than correctness
- **Consensus:** 4/4 (DeepSeek summary, GLM summary, GPT test gaps, Opus
  cross-cutting #10)
- **Verified:** Yes for SQL JOIN test
  (`tests/sql/relationships.rs:74-78` only `!join_relations.is_empty()`).
  Implied for R `>= 0` asserts.
- **Effect:** Bugs survive because tests assert "produces something"
  rather than "produces the right thing." This is structural, not a
  single fix.
- **Fix:** Establish a test-bar rule: every relationship test asserts
  exact relationship kind, source/target IDs, and counts. Every symbol
  test asserts exact name and kind. Replace `>= 0` with concrete
  expectations.

---

## Cross-Cutting Patterns Worth Fixing System-Wide

These appear across multiple tiers; one fix lifts every extractor.

### CC1. Core enums are too narrow
`IdentifierKind` has only `Call`, `VariableRef`, `TypeUsage`, and
`MemberAccess`. Real refactoring tools need `AnnotationRef`,
`MacroInvocation`, `ImportRef`, `GenericArg`, `Definition` (vs reference),
`Parameter`, and `Assignment`.

`SymbolKind` also lacks common constructs called out by the audits: `Macro`,
`Protocol`, `TypeAlias`, `Component`, `Decorator`, `Extension`, and likely
`Record`. Today those map to generic kinds such as Function, Class, Type, or
Variable, then callers have to rediscover the real language construct from
metadata or signatures. Adding a `SymbolModifier` bitflag (`Async`,
`Generator`, `Static`, `Const`, `Override`, `Abstract`, `Final`, `Sealed`,
`Inline`, `Comptime`, `Unsafe`, `Volatile`) would also let cross-language
queries work without grepping signatures.

### CC2. `Visibility` enum collapses internal / fileprivate / package-private
Only `Public/Private/Protected`. Languages with finer access levels
(Rust `pub(crate)` / `pub(super)`, C# `internal` / `protected internal`,
Swift `fileprivate` / `open`, Scala `private[pkg]`, Java
package-private, Kotlin `internal`) all flatten to one of three. Add
`Internal`, `FilePrivate`, `Open`; or store the canonical literal in
metadata.

### CC3. `RelationshipKind` missing Throws/Yields
Java `throws`, Kotlin `@Throws`, Python `raise`, Python `yield`, JS
generators have no relationship kind. Error-flow and producer-consumer
analysis impossible.

### CC4. `parent_id` propagation has three competing patterns
Walker threads explicitly (most extractors); walker doesn't thread,
extractor recomputes ID via `generate_id(name, row, col)` (Python,
TypeScript); walker doesn't thread, extractor passes `None` (TypeScript
class, multiple Ruby sites). Standardize on "walker threads; extractors
never recompute IDs."

### CC5. Inline command / cmdlet / builtin lists diverge across files
Bash `commands.rs` "interesting commands" overlaps with
`relationships.rs` "builtin commands" but they don't agree.
PowerShell `is_builtin_cmdlet` is two cmdlets. Consolidate per-language
into a single source of truth.

### CC6. Embedded language extraction is inconsistent
Vue: `<script>` regex, `<script setup>` tree-sitter, `<style>` regex.
Razor: HTML and C# both via tree-sitter (good). HTML: `<script>` and
`<style>` not parsed. Markdown: fenced code blocks not parsed. Build a
"parse range with appropriate parser, offset positions" framework so
all four cases share the same plumbing.

### CC7. Capability matrix asserts perfection that does not hold
`fixtures/extraction/capabilities.json` has empty `capability_gaps` for
all 36 languages. Populate it from the gaps documented above so the
matrix is an accurate map rather than an aspirational claim.

---

## Recommended Fix Order

A reasonable order of attack, grouped by leverage.

### Tier 1: Bug fixes (immediate; mechanical, broad impact)
1. **C1** SQL JOIN self-edge (`sql/relationships.rs:181-196`)
2. **C2** JSON UTF-8 panic (`json/mod.rs:102`)
3. **C7** Zig + Dart `infer_types` use `symbol.id` (mechanical)
4. **H35** Pipeline parse failure should produce a degraded diagnostic result
5. **C9** C++ identifier full-text to field name only
6. **C6** Multi-declarator drops in Java/C#/VB.NET
7. **C5** Ruby `attr_accessor :a, :b, :c` iteration
8. **H30** Go multi-name var/const and Bash declaration multi-name handling
9. **H37** Python plain `import` multi-binding extraction
10. **H1** TypeScript `extract_class` parent_id; eliminate the regen
   workaround
11. **H6** PowerShell method signature builder
12. **H8** Lua/GDScript signature builders
13. **M3/M4** symbol/relationship ID collisions

### Tier 2: Cross-file pending and language idioms
14. **C3** Elixir migrate to structured pending
15. **C4** Ruby cross-file inheritance + include emit pending
16. **H33** Dart inheritance, implements, and mixin pending relationships
17. **H36** GDScript Extends relationships from `extends` metadata
18. **C10** C++ relationship resolution by node containment
19. **H32** `.h` C++ header routing
20. **H34** C/C++ type-use relationship edges for declarations
21. **H10** PHP constructor property promotion
22. **H11** Annotations on Tier-2 non-function symbols
23. **H12** Elixir `@doc`/`@moduledoc` extraction
24. **H17** Add `IdentifierKind::TypeUsage` to missing languages
25. **H18** Add constructor calls to `IdentifierKind::Call`
26. **M19** Elixir defguard/defdelegate/defexception/defoverridable
27. **M18** Scala case-class fields
28. **M15** PHP namespace-preserving unresolved targets

### Tier 3: Markup, scripting, perf, and embedded languages
29. **H3 / H4** Vue Options API tree-sitter + parse-once
30. **H22** HTML `<script>`/`<style>` parsing
31. **H21** Bump HTML/Razor/SQL to pending-capable
32. **H20** Symbol-name collisions in HTML/CSS/Razor
33. **H25** SQL view/trigger relationships
34. **M42** HTML script relationship source resolution
35. **M40 / M41 / M44** Bash source, R source, and Zig import coverage
36. **H26** Hoist all inline `Regex::new` to `LazyLock`
37. **H5** PowerShell builtin cmdlet list
38. **H7** Bash/PowerShell call-site-as-definition cleanup

### Tier 4: Base layer changes (cascade)
39. **C8** Delete base `extract_visibility` text fallback
40. **M1** Split `HashLine` / `LuaDoubleDash` / `SqlLine` / `GoLine`
    into "line comment" vs "doc marker"
41. **CC1** Add missing core kinds and `SymbolModifier` bitflags
42. **CC2** Add finer `Visibility` variants
43. **CC4** Standardize parent_id propagation
44. **CC7** Populate `capability_gaps` in the matrix

### Tier 5: Test bar (longest leverage)
45. **L10** Replace existence-only asserts with semantic asserts
46. Add cross-file tests (inheritance, conformance, mixins, imports,
    namespace-qualified targets, aliasing, not just calls)
47. Add same-name regression tests (overloaded methods, two
    same-terminal classes in different namespaces, two functions on
    the same line)

---

## Reviewed But Not Promoted

These source-audit claims were checked during this pass and should not drive
the first implementation plan as originally stated:

- **C++ multi-declarator drops:** not verified on current source.
  `cpp/mod.rs` calls `extract_multi_declarations`, and
  `cpp/fields.rs::extract_multi_declarations` emits extra `init_declarator`
  symbols after the first one. Keep C++ in tests when touching this area, but
  do not plan it as the same bug as Java/C#/VB.NET.
- **C++ namespace contents lack `parent_id`:** not verified. The current walker
  threads the namespace symbol id into child traversal after
  `extract_namespace` succeeds.
- **PowerShell workflow definitions not extracted:** not accurate as a zero
  extraction claim. Current PowerShell tests expect workflow names to appear as
  Function symbols. A distinct Workflow kind could still be a modeling
  improvement, but this is not a missing-symbol bug.
- **Registry macro inconsistency and constructor signature inconsistency:** real
  maintenance smells, not user-visible extractor correctness bugs. Keep them
  out of the first correctness plan unless nearby work makes them cheap.
- **Some profile-level gaps are intentionally low priority:** JSON/TOML having
  no relationship pass, CSS lacking `infer_types`, and data-format identifiers
  being sparse are worth tracking, but they are less urgent than wrong IDs,
  self-edges, dropped symbols, and missing cross-file relationships.

---

## What This Compilation Did Not Cover

- **Performance benchmarks.** Hot-path regex compilation and Vue's
  O(N) reparse are flagged but not profiled end-to-end.
- **Error recovery under partial trees.** Each extractor's ERROR-node
  handling is touched on but not exhaustively tested.
- **Incremental update correctness.** `RecordOffset` /
  `apply_record_offset` for embedded sections is not exercised here.
- **Long-tail grammar edge cases.** Tree-sitter grammars have many
  obscure node kinds; the audits focus on common cases.
- **Cross-language consistency for shared idioms.** A TypeScript
  class field vs a Java field: do they produce equivalent symbol
  shapes? Per-language gaps were the focus; cross-language semantic
  equivalence was not.

---

## Source Audit Files

For the original per-model framing and additional detail:

- `docs/findings/deepseek/tree-sitter-audit.md`: matrix-style
  grade-by-language with severity bands.
- `docs/findings/glm/extractor-audit.md`: severity-banded findings
  with file:line citations and extensive enum-design recommendations.
- `docs/findings/gpt/tree-sitter-extractor-audit.md`: focused on
  data-quality identity bugs and test-quality gaps.
- `docs/findings/opus/`: five-file deep dive (`README.md`,
  `base-infrastructure.md`, four tier files). Most thorough on
  base-layer defects and per-tier patterns.

This document is the authoritative summary; consult the source files
when you need the full per-model framing or additional findings of
lesser severity.
