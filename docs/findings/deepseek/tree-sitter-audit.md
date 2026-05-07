# Tree-Sitter Extractor Audit

**Auditor:** DeepSeek v4 (via OpenCode)  
**Date:** 2026-05-06  
**Commit:** Current HEAD  
**Scope:** All 36 language entries (34 extractor modules + 2 aliases)  

---

## Summary

The tree-sitter extraction layer is **solid but uneven**. The strongest extractors (Rust, C++, C#, PHP, Kotlin) extract symbols, relationships, identifiers, types, doc_comments, and visibility with high coverage and correct structured pending relationships. Several extractors have significant gaps ranging from missing entire public methods to complete absence of doc_comment extraction.

### Confidence: 85/100
High confidence in the gap identification. A few edge cases (e.g., exact AST node names for QML enum field access, TOML commented syntax scoping) could shift from "gap" to "correct" with grammar inspection, but the core findings (missing methods, missing doc_comments) are confirmed from source code.

---

## Grade-by-Language Matrix

| Language | Symbols | Relationships | Identifiers | Types | Doc Comments | Visibility | Pending | Overall |
|---|---|---|---|---|---|---|---|---|
| Rust | A | A | A | B | A | A | Structured | **A** |
| C | A | B | B | C | A | B | Structured | **B+** |
| C++ | A | A | B | B | A | A | Structured | **A-** |
| Go | B | B | C | C | A | A | Structured | **B** |
| Zig | A | B | B | C | A | A | Structured | **B+** |
| TypeScript | B | A | B | B | A | A | Structured | **B+** |
| JavaScript | A | A | B | C | A | A | Structured | **B+** |
| Python | A | A | B | C | A | B | Structured | **B+** |
| Java | A | A | A | C | A | A | Structured | **A-** |
| C# | A | A | C | C | A | A | Structured | **B+** |
| VB.NET | A | A | C | B | A | A | Structured | **B+** |
| PHP | A | A | A | A | A | A | Structured | **A** |
| Ruby | A | A | B | B | A | A | Structured | **B+** |
| Swift | A | A | C | A | A | A | Structured | **B+** |
| Kotlin | A | A | A | A | A | A | Structured | **A** |
| Scala | A | B | A | B | A | A | Structured | **B+** |
| Dart | A | B | A | C | **F** | A | Mixed | **C+** |
| Elixir | B | C | B | C | B | B | **Legacy** | **C+** |
| Lua | B | B | B | **F** | C | B | Structured | **C** |
| R | **D** | C | C | **F** | **F** | **F** | Structured | **D+** |
| Bash | B | B | B | C | A | B | Structured | **B** |
| PowerShell | A | B | B | B | A | B | Structured | **B** |
| GDScript | A | B | B | C | A | B | Structured | **B** |
| QML | B | C | C | **F** | **F** | **F** | Structured | **D** |
| Razor | A | B | B | B | A | A | Structured | **B+** |
| SQL | A | C | C | B | A | B | Structured | **B-** |
| Regex | C | C | C | D | **F** | **F** | None | **D** |
| Markdown | B | C | F | **F** | A | N/A | None | **C** |
| JSON | C | **F** | F | **F** | B | N/A | None | **D+** |
| TOML | C | **F** | F | **F** | B | N/A | None | **D+** |
| YAML | B | C | C | **F** | **F** | N/A | None | **C** |

---

## Critical Gaps

### 1. Elixir: Legacy Pending Relationships (CRITICAL)

**File:** `crates/julie-extractors/src/elixir/relationships.rs`

Elixir is the **only** extractor still using the legacy `add_pending_relationship` path. Every other programming language extractor has migrated to `StructuredPendingRelationship`. This means Elixir call and use relationships lack `UnresolvedTarget` with `receiver`, `namespace_path`, and `import_context` fields. Cross-file call resolution for Elixir modules is missing context that other languages provide.

```rust
// Current (wrong):
base.add_pending_relationship(PendingRelationship::legacy(...));

// Should use:
base.add_structured_pending_relationship(
    base.create_pending_relationship(from_id, unresolved_target, ...)
);
```

### 2. Dart: Zero Doc Comment Extraction

**Files:** `crates/julie-extractors/src/dart/*.rs`

No extraction function calls `base.find_doc_comment()`. All 15+ extraction paths pass `doc_comment: None`. Dart supports `///` triple-slash doc comments (registered in `specs.rs` as `DART_DOCS`), but the extractor never retrieves them. Every Dart symbol in the index has `doc_comment: None`.

### 3. QML: Missing Three Core Methods

**File:** `crates/julie-extractors/src/qml/mod.rs`

- No `infer_types` method
- No `find_doc_comment` calls
- No visibility extraction

QML has ~10 node types extracted as symbols, but zero doc comments, zero types, and zero visibility markers.

### 4. R Extractor: Severely Underdeveloped

**File:** `crates/julie-extractors/src/r/mod.rs`

Only two AST node types produce symbols (`binary_operator` for assignments, `call` for `library()/require()`). Missing: S3/S4/R6 class systems, `function_definition` standalone, `formula` nodes, `setGeneric()/setMethod()`, `Roxygen2` comments, `infer_types`, visibility, and `source()/load()` imports. Right-to-left assignment (`->`) always creates `Variable` instead of `Function`.

### 5. JSON & TOML: Missing `extract_relationships` Public Method

**Files:** `crates/julie-extractors/src/json/mod.rs`, `crates/julie-extractors/src/toml/mod.rs`

The registry calls `ext.extract_relationships(tree, &symbols)` on every registered extractor. JSON and TOML use the `define_data_only_extractors!` macro, which explicitly does NOT call `extract_relationships`. This is by design (data-only), but the relationship column is always empty for these languages. JSON key-value hierarchy and TOML table nesting produce no relationships.

### 6. Multiple Extractors Missing `infer_types`

| Language | Status |
|---|---|
| Lua | Method does not exist. `dataType` metadata is set but never surfaced. |
| QML | Method does not exist. |
| Markdown | Method does not exist. |
| JSON | Method does not exist. |
| TOML | Method does not exist. |
| YAML | Method does not exist. |
| R | Method does not exist. |

The registry uses `define_*_extractors!` macros that only call `infer_types` when the capabilities include `types: true`. Lua and QML have `PENDING_NO_TYPES_CAPABILITIES` which sets `types: false`. The data languages correctly skip it. But Lua's extractor still computes data during extraction and stores it in metadata, making the information inaccessible.

---

## High-Severity Gaps

### 7. Missing TypeUsage Identifiers

| Language | Has TypeUsage? |
|---|---|
| C# | **No** |
| VB.NET | **No** |
| Go | **No** |
| Swift | **No** |
| JavaScript/JSX | **No** |

Type references in code are not tracked as identifiers in these languages. Java, Kotlin, Scala, Rust, C, C++, Dart, and PHP all extract `IdentifierKind::TypeUsage`. The five listed extractors only produce `Call` and `MemberAccess`.

### 8. No VariableRef Identifiers Anywhere

`IdentifierKind::VariableRef` is defined in the type system but not extracted by any programming language extractor. Variable reads (`let x = foo + bar`) produce no identifiers for `foo` or `bar`. This limits the ability to find all usages of a variable across a codebase.

### 9. Registry Macro Inconsistency

14 extractors use manual inline functions in `registry.rs` while only 7 use the `define_*_extractors!` macros:

**Macro-based:** Rust, Dart, Go, C, Zig, VB.NET, GDScript, Python, C++, Ruby, SQL, HTML, Razor, Regex, CSS, Markdown, YAML, JSON, TOML

**Inline:** Elixir, TypeScript, TSX, JavaScript, JSX, Java, C#, Kotlin, Swift, PHP, Scala, Bash, PowerShell, Lua, QML, R, Vue

This isn't a functional gap since both paths produce identical `ExtractionResults`, but the inline path is ~20 lines of boilerplate per language. The `define_structured_full_language_extractors!` macro should be extended to all `StructuredPendingRelationship` users.

### 10. Python/Ruby Constructor Inconsistency

Python and Ruby extractors take `new(file_path, content, workspace_root)` (no `language` param). All other extractors except C++ take `new(language, file_path, content, workspace_root)`. C++ takes `new(file_path, content, workspace_root)` (same as Py/Rb but hardcodes "cpp").

---

## Medium-Severity Gaps

### 11. Go: Struct Embedding Stub

**File:** `crates/julie-extractors/src/go/relationships.rs`

`extract_embedding_relationships` is a stub (`_node, _symbol_map, _relationships` parameters). Go embedded struct fields like `type Foo struct { Bar }` should create a relationship indicating `Foo` embeds `Bar`.

### 12. Go: Stdlib Filter Applies Only to `fmt`

The import-based pending relationship filter only recognizes `"fmt"` as a standard library import. Calls like `strings.Split()`, `os.Open()`, `io.ReadAll()` all generate pending relationships because their package imports aren't recognized as stdlib.

### 13. TypeScript: Single Import Symbol Per Statement

`import { a, b, c } from './foo'` produces only ONE `Import` symbol. The JavaScript extractor correctly uses `extract_import_specifiers` to produce one symbol per named import.

### 14. Vue: Options API Is Regex-Only

The `<script>` section for Options API (non-`<script setup>`) uses regex pattern matching. Nested functions, async methods, and computed get/set pairs are missed. CSS in `<style>` is also regex-based instead of delegating to the CSS extractor.

### 15. CSS: No `infer_types` Support

The CSS extractor lacks an `infer_types` method. The `define_relationship_data_extractors!` macro sets `types: false` in capabilities, so the registry wrapper only calls `extract_symbols`, `extract_relationships`, and `extract_identifiers`. This works but means CSS types are never surfaced.

### 16. Zig: `infer_types` Uses Symbol Name as Key

**File:** `crates/julie-extractors/src/zig/type_inference.rs`

The returned `HashMap` uses `symbol.name` as key instead of `symbol.id`. Every other extractor uses `symbol.id`. Since names aren't unique within a file, this can cause type collisions.

### 17. Java/C#/VB.NET: Only First Declarator Extracted

`int x, y, z;` produces only one symbol for `x`. `y` and `z` are silently dropped. This affects Java field declarations, C# field declarations, and VB.NET field declarations.

### 18. Markdown: Links and Footnotes Not Extracted

`[text](url)`, `[text][ref]`, reference definitions, and `[^footnote]` syntax are invisible to the extractor. For a documentation language, this is a significant missing capability.

---

## Low-Severity Gaps / Edge Cases

### 19. SQL: FROM/JOIN Table References Not Tracked

Table references in SELECT, FROM, JOIN, WHERE, and EXEC statements produce no identifiers or relationships. `SELECT COUNT(*)` function calls are similarly invisible.

### 20. Bash: `source`/`.` Commands Excluded

Sourced files are listed as builtins and excluded from relationship creation. Cross-file tracing through sourced scripts is lost.

### 21. C: No VariableRef Identifiers

Only `Call`, `TypeUsage`, and `MemberAccess`. Variable reads/writes are not tracked.

### 22. CPP: Duplicate MemberAccess on Method Calls

`obj.method()` produces both a `Call` identifier (correct) and a `MemberAccess` identifier (duplicate). Rust and Go check for this and skip the duplicate; C++ does not.

### 23. Scala: `val` Is Always `Constant`

All `val` definitions map to `SymbolKind::Constant`. Local `val` inside a method body is an immutable variable, not a constant. Only top-level or object-level `val` should be `Constant`.

### 24. Regex: All Symbols Are `Variable`

Capture groups, character classes, lookaround assertions, unicode properties all get `SymbolKind::Variable`. There's no differentiation in SymbolKind.

### 25. YAML: Anchors Are Metadata, Not Symbols

`&anchor` definitions exist only as metadata on their containing pair, not as independent Symbol entries. Aliases `*anchor` are correctly tracked as VariableRef identifiers with resolution.

---

## Pending Relationship Migration Status

| Type | Languages |
|---|---|
| Structured | Rust, C, C++, Go, Zig, TypeScript/TSX, JavaScript/JSX, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Scala, Python, Lua, QML, R, Bash, PowerShell, GDScript, Razor, SQL, Vue |
| Legacy | Elixir |
| Mixed | Dart (uses both paths) |
| None | HTML, CSS, Regex, Markdown, JSON, TOML, YAML |

Elixir is the sole remaining holdout on legacy `PendingRelationship`. Dart mixes both in the same extractor (`same_file_calls` uses legacy; call_expression resolution uses structured).

---

## `infer_types` Implementation Quality

| Method | Languages |
|---|---|
| AST/Node-based | Kotlin, Scala, VB.NET |
| Metadata-based | PHP, Swift |
| Regex on signature string | Python, Rust, C, C++, Go, Zig, TypeScript, Java, C#, GDScript |
| Value-literal inference | Ruby |
| Annotation-based | Elixir (from @spec) |
| Not implemented | Lua*, QML*, Markdown*, JSON*, TOML*, YAML*, R* |

\* Lua stores `dataType` in metadata but has no public `infer_types` method.

---

## Doc Comment Extraction Gap

| Status | Languages |
|---|---|
| Full support | Rust, C, C++, Go, Zig, TypeScript/TSX, JavaScript/JSX, Java, C#, VB.NET, PHP, Ruby, Swift, Kotlin, Scala, Python, Bash, PowerShell, GDScript, HTML, CSS, SQL, Markdown, JSON, TOML, Razor |
| Partial (some functions only) | Elixir (@doc/@moduledoc content discarded), Lua (inconsistent code paths) |
| None | Dart, QML, Regex, YAML, R |

---

## Recommendations

### Immediate Fixes (Bug-tier)

1. **Dart doc_comments**: Add `base.find_doc_comment()` calls in all 15 extraction functions.
2. **Elixir structured pending**: Migrate from `add_pending_relationship` to `add_structured_pending_relationship` with proper `UnresolvedTarget`.
3. **Zig `infer_types`**: Change map key from `symbol.name` to `symbol.id`.
4. **Go stdlib filter**: Expand from `matches!("fmt")` to a known-stdlib-package set.
5. **CPP duplicate MemberAccess**: Add the parent-check guard present in Rust/Go identifier extractors.

### Near-Term Improvements (Week-tier)

6. **R extractor**: Implement S3 class detection, Roxygen2 comment extraction, function_definition standalone handling.
7. **QML**: Add `infer_types`, `find_doc_comment`, and visibility extraction.
8. **TypeScript import granularity**: Use `extract_import_specifiers` pattern from JavaScript.
9. **Vue**: Delegate `<style>` to CSS extractor; add tree-sitter parsing for Options API.
10. **SQL**: Extract FROM/JOIN table references as relationships; track function calls.
11. **Multiple declarators**: Handle all declarators in Java, C#, VB.NET field declarations.
12. **Markdown links**: Extract link targets and reference definitions.

### Architecture Improvements (Month-tier)

13. **Unify registry macros**: Move all inline extractor functions to `define_structured_full_language_extractors!` or equivalent.
14. **VariableRef identifiers**: Add to all programming language extractors that currently only do Call/MemberAccess.
15. **`infer_types` contract**: Every extractor should have the public method, even if it returns an empty HashMap for data/documentation languages.
16. **Constructor consistency**: Standardize on `new(language, file_path, content, workspace_root)` for all extractors.
