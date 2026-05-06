# Julie Tree-Sitter Extractor Audit

**Date**: 2026-05-06
**Scope**: Cross-language extractor quality audit for all 34 supported languages
**Focus**: Missing functionality, implementation errors, data quality degradation

---

## Executive Summary

This audit covers Julie's tree-sitter extractors across 34 languages, reviewing symbol extraction, relationship extraction, identifier tracking, type inference, visibility, signature extraction, documentation handling, and test detection. Findings are organized by severity and then by category.

**Key themes**:
1. **Import/Use relationship graphs are incomplete across most languages** - Import symbols are created but dependency edges (RelationshipKind::Imports) are not, making cross-file resolution unreliable
2. **SymbolKind enum is too narrow** - Missing kinds for Macro, Component, TypeAlias, Protocol, Actor, and Decorator force important constructs into inappropriate categories
3. **Visibility collapses nuance** - Internal, fileprivate, package-private, and open all map to Public or Private, losing critical access-level information
4. **Type inference is regex-based across all extractors** - Tree-sitter ASTs contain structured type information that should be parsed instead
5. **Regression risk** - JSON extractor has a UTF-8 panic bug, several extractors have hard-coded indices that will break on language additions

---

## CRITICAL Findings

### C1: JSON UTF-8 panic on long string truncation
- **File**: `json/mod.rs:102`
- **Category**: Bug - runtime panic
- `trimmed[..2000].to_string()` slices at a byte boundary, not a char boundary. Will panic on JSON files with long string values containing multi-byte UTF-8 (CJK descriptions in package.json, memory files). TOML correctly uses `chars().take(2000).collect()` but JSON does not.
- **Fix**: Change to `trimmed.chars().take(2000).collect::<String>() + "..."`

### C2: C++ no type-usage relationships for variables/fields
- **File**: `cpp/relationships.rs`
- Only `call_expression` and `class_specifier`/`struct_specifier` are handled. No relationship edges for when a struct/union/enum is used as a type in a variable declaration, parameter, or field. This means the graph has no "X uses Y" edges for C++ type references, which is one of the most important relationship types in C++ codebases.

### C3: C no type-usage relationships for variables/fields
- **File**: `c/relationships.rs`
- Same as C++ but worse: only `call_expression` and `preproc_include` are handled. No relationship for struct/union/enum type usage in declarations.

### C4: Zig missing `usingnamespace` and `@import` patterns
- **File**: `zig/variables.rs:22-24`
- Only `@import` calls within `const_declaration`/`variable_declaration` are detected. The broader `usingnamespace` statement is not extracted at all, and `@import` used outside const declarations is missed.

### C5: R no S4/R6 class extraction
- **File**: `r/mod.rs:182-186`
- No extraction for `setClass`, `setRefClass`, `setGeneric`, `setMethod` (S4 OOP), or `R6Class`/`R6::R6Class` (R6 OOP). These are R's primary OOP constructors.

### C6: TypeScript/TSX no JSX element extraction
- **File**: `typescript/symbols.rs:30-139`
- JSX self-closing elements (`<MyComponent />`), opening elements, and fragments are never extracted. All React/Preact component references in TSX files are invisible.

### C7: Pipeline total parse failure returns error with zero data
- **File**: `pipeline.rs:119-122`
- When `parser.parse()` returns None (timeout, memory limit), the pipeline returns `Err`, giving the caller nothing. A degraded result with `parse_diagnostics` would let the caller still index file metadata.

---

## HIGH Findings

### H1: SymbolKind enum missing critical kinds
- **File**: `base/types.rs:273-292`
- Missing kinds:
  - `Macro` - Rust `macro_rules!`, C `#define`, Elixir macros, Scala macros all forced into `Function` or `Variable`
  - `Protocol` - Swift protocols, Elixir behaviours/protocols, Go interfaces conflate with `Interface`
  - `TypeAlias` - Kotlin `typealias`, Swift `typealias`, Rust `type`, TypeScript `type` aliases forced into `Type`
  - `Component` - Vue SFC, React, Angular, Svelte components forced into `Class` or `Function`
  - `Decorator` - Python decorators, TypeScript decorators are metadata, not first-class symbols

### H2: Visibility collapses internal/fileprivate/package-private
- **File**: `base/types.rs:306-312`
- `Visibility` only has `Public/Private/Protected`. C# `internal`, Kotlin `internal`, Swift `internal`, Swift `fileprivate`, Java package-private, and Python module-level visibility all map to `Private`, losing critical access-level distinctions. The real visibility is not stored in metadata either.

### H3: RelationshipKind missing Throws/Yields
- **File**: `base/types.rs:264-272`
- Exception declarations (Java `throws`, Kotlin `@Throws`, Python `raise`) and generator yields (Python `yield`, JS generators) have no relationship kind. Understanding error flow and producer-consumer patterns is impossible.

### H4: IdentifierKind missing Import/Definition/Parameter/Assignment
- **File**: `base/types.rs:180-196`
- Import references, definition sites, function parameters, and variable assignments are all collapsed into `VariableRef`. This makes it impossible to distinguish reads from writes, imports from uses, or definitions from references.

### H5: `from_string` fallbacks silently degrade data
- **File**: `base/types.rs:195-209`, `base/types.rs:272-292`, `base/types.rs:335-346`
- `SymbolKind::from_string` defaults to `Variable`, `RelationshipKind::from_string` defaults to `Uses`, `IdentifierKind::from_string` defaults to `VariableRef`. These silent defaults make it impossible to detect data corruption or version mismatches. Should return `None` or log a warning.

### H6: `ScopedSymbolIndex::unique_symbol_map` silently drops overloaded methods
- **File**: `relationship_resolution.rs:141-151`
- When two symbols have the same name (overloaded methods), `unique_symbol_map` discards ALL of them. Overloaded methods become invisible to local resolution.

### H7: `extend()` on ExtractionResults silently overwrites type_info
- **File**: `results_normalization.rs:31-41`
- `self.types.extend(other.types)` uses `HashMap::extend`, which silently drops entries when keys collide. If two files extract type info for the same symbol, one is lost.

### H8: No deduplication of symbols across files
- **File**: `results_normalization.rs:31-41`
- Cross-file deduplication is absent. Two files can emit the same symbol ID for different constructs, creating duplicates in the final `symbols` vec.

### H9: TypeInfo missing return type and parameter type fields
- **File**: `base/types.rs:248-262`
- `TypeInfo` stores `resolved_type` but has no separate field for return types or parameter types of functions. Call graph analysis ("what type does this function return?") requires the return type separately.

### H10: Go embedding relationships are a stub
- **File**: `go/relationships.rs:90-99`
- `extract_embedding_relationships` has an empty implementation (all parameters unused with `_` prefix). Go struct embedding is one of the most important composition patterns and produces zero relationship edges.

### H11: Go only uses name-based symbol lookup (no ScopedSymbolIndex)
- **File**: `go/relationships.rs:68-69`
- Uses `HashMap<String, &Symbol>` keyed by name. Method calls like `obj.Method()` can only resolve if there's exactly one symbol with that name in the entire file. Fails for common patterns like multiple types having a `String()` method.

### H12: C all structs/unions/enums hardcoded as Public
- **File**: `c/structs.rs:34,55,83`
- C file-scope `static` declarations should be `Private`, but all struct/union/enum symbols get `Visibility::Public`.

### H13: C++ concepts not extracted
- **File**: `cpp/mod.rs:174-234`
- C++20 concepts (`concept` nodes) are not extracted. Important for template constraint navigation.

### H14: C++ namespace contents lack parent_id
- **File**: `cpp/declarations.rs:50-115`
- Namespace aliases are extracted as `Import`, but symbols inside `namespace_definition` nodes don't get a `parent_id` linking them to their namespace.

### H15: C++ call resolution uses fragile string matching
- **File**: `cpp/relationships.rs:287-342`
- `find_containing_function_name` uses string matching on symbol_map, which is fragile for overloaded functions. Other extractors use position-based `find_containing_symbol_from_map`.

### H16: Swift extensions use `SymbolKind::Class`
- **File**: Swift extractor
- Extensions should have a distinct kind or at minimum relationships to the extended type. Using `Class` makes them indistinguishable from actual class definitions.

### H17: No import/Uses relationship edges in most languages
- **Category**: Cross-cutting
- Only C creates import relationships. Rust, C++, Go, Zig, JS, TS, Python, Java, C#, PHP, Ruby, Elixir, Scala, and Bash all extract import symbols but do NOT create `RelationshipKind::Imports` edges. Cross-file resolution for imported symbols has no structural basis in the graph.

### H18: HTML script/style symbols always named "script"/"style"
- **File**: `html/scripts.rs:68-69`
- Multiple `<script>` or `<style>` tags produce symbols with the same name, making them impossible to distinguish. The `src` or `type` attribute should be incorporated.

### H19: Vue Options API regex-only extraction is extremely limited
- **File**: `vue/script.rs:16-120`
- Only extracts section headers (`methods`, `computed`, `data`, `props`), not individual methods/properties inside those blocks. A `methods: { foo() {}, bar() {} }` only creates a `methods` Property symbol.

### H20: Vue template section produces zero symbols
- **File**: `vue/mod.rs:172-176`
- Important template constructs like `v-slot` named slots, `v-for` iterator variables, and template refs are all lost.

### H21: Elixir defguard/defdelegate/defexception/defoverridable silently dropped
- **File**: `elixir/calls.rs:41`
- These definition keywords are in the skip list but have no extraction handler. Guard definitions, delegation functions, and exception modules are all silently dropped.

### H22: Scala missing match types, opaque types, implicit classes, self-types
- **File**: `scala/mod.rs:73-117`
- `match_type_definition` (Scala 3), `opaque_type_definition`, `implicit_class_definition`, and `self_type` annotations are not extracted.

### H23: Lua `require()` calls not treated as imports
- **File**: `lua/core.rs:20-58`
- Bare `require("module")` calls at statement level are missed. Only `local x = require(...)` assignments create Import symbols.

### H24: Bash missing alias, array, and source/dot-source extraction
- **File**: `bash/mod.rs:115-123`
- Shell aliases (`alias ll='ls -la'`), array definitions, and `source`/`.` commands are not extracted.

### H25: GDScript no `extends` relationship edge
- **File**: `gdscript/relationships.rs:40-44`
- When a class extends another (the most important GDScript relationship), only `baseClass` metadata is set, but no `Extends` relationship edge is created.

### H26: SQL JOIN relationships self-reference
- **File**: `sql/relationships.rs:156-199`
- `extract_join_relationships` uses the same table symbol for both source and target (`from_symbol_id == to_symbol_id`), making the relationship useless for graph traversal.

### H27: Markdown heading text fallback corrupts content with `#`
- **File**: `markdown/mod.rs:331-333`
- Strips ALL leading `#` chars from heading text. `# C# Programming` becomes `C Programming`.

### H28: YAML no leaf-value doc_comment extraction
- **File**: `yaml/mod.rs:119-126`
- Unlike JSON/TOML which extract string values into `doc_comment`, YAML doesn't extract any value content. `description: Fixed the auth bug` produces a symbol `description` with no searchable text.

### H29: C++ test frameworks not handled
- **File**: `test_detection.rs:87`
- Google Test (`TEST()`, `TEST_F()`, `TEST_P()`), Catch2 (`TEST_CASE`, `SECTION`), and Boost.Test (`BOOST_AUTO_TEST_CASE`) are all missed.

### H30: No type usage (TypeUsage) identifier tracking in 5/8 backend languages
- **Category**: Cross-cutting
- PHP, Ruby, Swift, Kotlin, Dart don't track type references as identifiers, so `fast_refs` can't find type reference sites.

---

## MEDIUM Findings

### M1: Markdown heading level computed but discarded
- **File**: `markdown/mod.rs:296`
- `determine_heading_level` returns the level but the result is bound to `_level` and thrown away. Heading level is essential for hierarchy resolution.

### M2: Markdown no cross-file link relationships
- **File**: `markdown/relationships.rs:9-10`
- Only local anchor links produce relationships. Cross-file links like `[API](./api.md)` are completely ignored.

### M3: Markdown no code block language metadata
- **File**: `markdown/mod.rs:268-280`
- Language tags from fenced code blocks (e.g., `rust` from ` ```rust `) are not extracted as structured metadata.

### M4: YAML flow mapping pairs not extracted
- **File**: `yaml/mod.rs:73-80`
- Only `block_mapping_pair` is handled. `flow_mapping_pair` (inline YAML like `config: {host: localhost}`) is skipped.

### M5: TOML array table kind not distinguished
- **File**: `toml/mod.rs:73`
- `[[servers]]` (array table) and `[servers]` (regular table) produce identical symbols with no metadata distinction.

### M6: CSS no `@container`, `@font-face`, or `@layer` extraction
- **File**: `css/mod.rs:58-137`
- Modern CSS features (container queries, font-face, cascade layers) have no extraction support.

### M7: CSS no pseudo-class/pseudo-element/attribute selector identifiers
- **File**: `css/identifiers.rs:49-112`
- Only class and ID selectors are extracted as identifiers. Pseudo-classes, pseudo-elements, and attribute selectors are lost.

### M8: CSS `@keyframes` body is dead code
- **File**: `css/animations.rs:59-66`
- `extract_keyframes` is an intentional no-op, but `mod.rs:86-91` calls it AND creates the `@keyframes` parent symbol separately. The function call is dead code.

### M9: QML only `id:` bindings extracted from `ui_binding`
- **File**: `qml/mod.rs:122-154`
- All property bindings (`width: 200`, `color: "red"`, `anchors.fill: parent`) are silently skipped.

### M10: QML no signal handler extraction
- **Category**: QML
- Signal handlers (`onClicked`, `onPressed`) are `ui_binding`/`ui_script_binding` nodes that are completely lost.

### M11: Elixir import/alias/require calls skipped in relationship walk
- **File**: `elixir/relationships.rs:48`
- These composition directives create Import symbols but generate no relationship edges.

### M12: GDScript no `@onready` reference tracking
- **File**: `gdscript/variables.rs:47`
- `@onready var label = $Label` node path references are not tracked as identifiers.

### M13: GDScript static functions not distinguished
- **File**: `gdscript/functions.rs:50-137`
- GDScript 4.0 `static func` declarations are extracted as regular functions.

### M14: Razor component relationships use synthetic IDs
- **File**: `razor/relationships.rs:75-96`
- `component-ComponentName`, `property-PropertyName`, `event-EventName` IDs will never resolve to real symbols during cross-file resolution.

### M15: SQL materialized views not handled
- **Category**: SQL
- `CREATE MATERIALIZED VIEW` is not extracted. Materialized views are distinct database objects.

### M16: R no `source()` import extraction
- **File**: `r/mod.rs:413-444`
- `source("file.R")` is the R equivalent of `require`/`import` but only `library`/`require` are handled.

### M17: PowerShell workflow definitions not extracted
- **File**: `powershell/mod.rs:90-138`
- `workflow` definitions are a distinct PowerShell construct that's missed.

### M18: `content_type` only set for markdown
- **File**: `base/creation_methods.rs:36-39`
- JSON, TOML, and YAML are also data/configuration languages but don't get `content_type` values.

### M19: Registry has 17 near-identical hand-written extraction functions
- **File**: `registry.rs:235-726`
- Java, C#, Kotlin, Swift, PHP, Scala, TS, TSX, JS, JSX, Bash, PowerShell, Lua, QML, R, Vue all have custom functions that differ only in the language name string. Should use a macro.

### M20: Registry hard-coded language count
- **File**: `registry.rs:59,828`
- `assert_eq!(supported_languages().len(), 36)` will break on any language addition/removal.

### M21: `containing_symbol_at_line` uses line-only containment
- **File**: `base/mod.rs:37-42`
- Only checks `start_line <= line_number && end_line >= line_number` without column checking. Wrong result when two symbols overlap on the same line.

### M22: `extract_visibility` base implementation uses string matching
- **File**: `base/creation_methods.rs:264-288`
- Does `text.contains("public ")` on the entire node text, producing false positives like `return public_value;`.

### M23: Zig type regex too simplistic
- **File**: `zig/type_inference.rs:7`
- `:\s*([\w\[\]!?*]+)` misses compound types like `*const u8`, `[]const u8`, `?*ArrayList`, `error{NotFound}!u32`.

### M24: R no type inference at all
- **Category**: R
- No type inference module beyond S3 classification. Return types could be inferred from the last expression in function bodies.

### M25: Bash/PowerShell test frameworks not specifically handled
- **File**: `test_detection.rs:87`
- PowerShell Pester (`Describe`/`It`/`Context`) and Bash bats/shunit2 frameworks are not specifically handled.

### M26: `confidence` and `Identifier::confidence` never populated by extractors
- **File**: `creation_methods.rs:61,107`
- These fields default to `None` for symbols and `1.0` for identifiers, but no extractor sets meaningful values. Downstream tools can't use confidence for ranking.

### M27: `symbol_map` silently overwrites duplicates within a file
- **File**: `creation_methods.rs:66`
- `create_symbol` uses `HashMap::insert`, silently overwriting if the key already exists.

### M28: `normalize_file_path` calls `canonicalize()` which resolves symlinks
- **File**: `base/span.rs:48-77`
- If the workspace root or file is accessed via a symlink, `canonicalize()` may produce a path outside the workspace root, causing incorrect relative paths.

### M29: Multi-document YAML not scoped
- **File**: `yaml/mod.rs:39-43`
- Duplicate key names across `---` document boundaries share the same symbol namespace.

### M30: TOML dotted key parent_id may be wrong
- **File**: `toml/mod.rs:103-179`
- `server.port = 8080` inside `[server]` gets `server`'s ID as parent, but TOML semantics define a subtable.

---

## LOW Findings

### L1: Rust `macro_rules!` mapped to `SymbolKind::Function`
- **File**: `rust/types.rs:443`
- Macros are syntactically and semantically distinct from functions. Should have a dedicated kind.

### L2: Rust no `use` import relationship edges
- **File**: `rust/relationships.rs`
- `use` declarations create Import symbols but no `Imports` relationship edges connect them.

### L3: Rust regex-based return type extraction is fragile
- **File**: `rust/mod.rs:162-196`
- `RETURN_TYPE_RE` regex `->\s*([^{]+)` stops at `{`, breaking on complex generic return types.

### L4: C function call resolution only handles bare `identifier` calls
- **File**: `c/relationships.rs:45-49`
- Function pointer calls and struct member calls are silently dropped.

### L5: C++ `function_call` node kind not handled in identifiers
- **File**: `cpp/identifiers.rs:31-33`
- Some tree-sitter C++ grammars produce `function_call` instead of `call_expression`. The relationships module handles both but the identifiers module only handles `call_expression`.

### L6: Go `init` and `main` marked as Private
- **File**: `go/functions.rs:132-135`
- These are special Go functions called by the runtime; they should be Public or flagged.

### L7: Go regex-based function recovery from source
- **File**: `go/functions.rs:11-97`
- `recover_function_symbols_from_source` uses regex with `confidence: 0.8` and no `parent_id`. This compensates for presumed tree-sitter gaps.

### L8: ZSH Zig opaque types not extracted
- **Category**: Zig
- `opaque {}` declarations have no extraction handler.

### L9: Zig `is_inside_struct` doesn't check for `union_declaration`
- **File**: `zig/helpers.rs:53-66`
- Methods inside union declarations would be classified as `Function` instead of `Method`.

### L10: TypeScript generic type parameters missing from signatures
- **File**: `typescript/classes.rs:83-95`
- `class Foo<T extends Bar>` gets signature `class Foo` without the generics.

### L11: TypeScript no type usage relationships for implements/extends
- **File**: `typescript/relationships.rs:143-214`
- Implements/extends metadata is stored in the class symbol's metadata, but no `TypeUsage` identifier is created.

### L12: JavaScript no async generator handling
- **File**: `javascript/mod.rs`
- `async_arrow_function` and `async_function_expression` may not be handled as distinct node types.

### L13: JavaScript no CommonJS require() relationship
- **File**: `javascript/imports.rs`
- `require()` calls get `isCommonJS: true` metadata but no `Imports` relationship.

### L14: HTML incomplete SVG element coverage
- **File**: `html/elements.rs:35-57`
- Missing common SVG elements: `g`, `use`, `clipPath`, `mask`, `pattern`, `symbol`, `image`, `line`, `polygon`, `polyline`, `ellipse`, `filter`.

### L15: Vue style section parsed with regex instead of CSS extractor
- **File**: `vue/style.rs:14-86`
- All rich CSS extraction (at-rules, keyframes, selectors) is lost for Vue style sections.

### L16: Vue `<script setup>` missing `defineModel` and `defineSlots`
- **File**: `vue/script_setup.rs:377-380`
- Vue 3.4+ compiler macro `defineModel` and `defineSlots` are not extracted.

### L17: QML instantiation relationship matching is fragile
- **File**: `qml/relationships.rs:155-158`
- Uses `name.contains(&component_type)`, which can match `RoundedRectangle` when looking for `Rectangle`.

### L18: Python `match`/`case` not extracted
- **Category**: Python
- Python 3.10+ match/case patterns are a distinct control flow construct.

### L19: Ruby RSpec test detection nonexistent
- **Category**: Ruby
- Only `test_` prefix + path is checked. The dominant Ruby testing framework (RSpec `describe`/`it`/`context`) produces zero test annotations.

### L20: Swift `open` visibility maps to `Public`
- **File**: `swift/signatures.rs:318-329`
- `open` in Swift is strictly more permissive than `public` (allows subclassing outside the module).

### L21: Scala `given` definitions use `SymbolKind::Variable`
- **File**: `scala/declarations.rs:239`
- Given instances are implicit providers, not variables.

### L22: JSONC tests exercise unsupported grammar
- **Category**: Testing
- JSONC tests use the standard JSON parser which doesn't support comments. Tree has ERROR nodes but extraction happens to work accidentally.

### L23: JSON no value extraction for non-string types
- **File**: `json/mod.rs:92-109`
- Number, boolean, and null values are not captured in `doc_comment`. `"port": 5432` produces a symbol with no searchable value.

### L24: `is_callable_or_import` only recognizes 4 kinds
- **File**: `relationship_resolution.rs:248-252`
- Function, Method, Constructor, and Import are the only callable kinds. Static methods, operators, delegates, and destructors are excluded, preventing local call resolution for these.

### L25: No `IdentifierKind::Import` entries in any extractor
- **Category**: Cross-cutting
- Import declarations create `SymbolKind::Import` symbols but no `IdentifierKind::Import` references linking usage sites.

---

## Priority Recommendations

### Immediate (P0 - data loss or runtime errors)
1. Fix JSON UTF-8 panic (`trimmed[..2000].to_string()` -> char-boundary-safe)
2. Add type-usage relationships for C and C++ (most impactful for those ecosystems)
3. Fix markdown heading `#` stripping bug
4. Add import relationship edges to at least Rust, Go, Python, TypeScript, Java, C#

### High Priority (P1 - significant feature gaps)
5. Add `Macro`, `Protocol`, `TypeAlias`, `Component` to `SymbolKind`
6. Add `Internal` and `FilePrivate` to `Visibility`
7. Add `Throws` and `Yields` to `RelationshipKind`
8. Fix Go embedding relationship stub (currently empty)
9. Add JSX/TSX element extraction
10. Fix C++ test detection (Google Test, Catch2, Boost.Test)
11. Add `TypeUsage` identifier extraction to PHP, Ruby, Swift, Kotlin, Dart
12. Fix Elixir defguard/defdelegate/defexception extraction

### Medium Priority (P2 - quality improvements)
13. Replace regex-based type inference with AST-based parsing
14. Add cross-file link relationships for Markdown
15. Improve Vue Options API extraction beyond regex stubs
16. Add YAML leaf value extraction to `doc_comment`
17. Deduplicate call-site `from_string` fallbacks with logging
18. Add R S4/R6 class extraction
19. Refactor registry.rs to use macros for the 17 near-identical extraction functions