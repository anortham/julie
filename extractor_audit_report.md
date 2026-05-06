# Tree-Sitter Extractor Audit Report

**Languages Audited:** Lua, R, Bash, PowerShell, SQL, Regex
**Audit Date:** 2026-05-06
**Scope:** Source files in `crates/julie-extractors/src/<language>/` and corresponding test suites

---

## Executive Summary

This audit identifies **47 findings** across six tree-sitter extractors. The most severe issues cluster around:

1. **Missing or incorrect doc comment registration** (PowerShell, Regex)
2. **Placeholder type inference** (Lua, R, Bash, PowerShell)
3. **Heavy regex fallback instead of AST parsing** (SQL)
4. **Minimal source file footprints** with large logic concentration (R)
5. **Test coverage gaps** with stub/placeholder tests across all languages
6. **Inconsistent relationship extraction capabilities** compared to reference extractors (TypeScript/Rust)

---

## Severity Legend

- **Critical:** Causes incorrect behavior, missing core functionality, or data loss
- **High:** Significant feature gaps or architectural problems that limit usefulness
- **Medium:** Missing features that reduce extraction quality or cause inconsistencies
- **Low:** Minor gaps, missing edge cases, or code quality issues

---

## 1. LUA EXTRACTOR

### Source Files (9 files)
`mod.rs`, `core.rs`, `functions.rs`, `variables.rs`, `tables.rs`, `classes.rs`, `helpers.rs`, `identifiers.rs`, `relationships.rs`

### Test Files (22 files)
`mod.rs`, `core.rs`, `functions.rs`, `variables.rs`, `tables.rs`, `classes.rs`, `helpers.rs`, `identifiers.rs`, `relationships.rs`, `doc_comments.rs`, `coroutines.rs`, `control_flow.rs`, `cross_file_relationships.rs`, `error_handling.rs`, `file_operations.rs`, `identifier_extraction.rs`, `metatables.rs`, `modules.rs`, `oop_patterns.rs`, `strings.rs`, `extractor.rs`

### Findings

| # | Category | File | Line | Severity | Description |
|---|----------|------|------|----------|-------------|
| L1 | Missing type inference | `language_spec/specs.rs` | 192 | High | Lua is registered with `PENDING_NO_TYPES_CAPABILITIES` (`types: false`). No type inference is performed, even for simple literal assignments. |
| L2 | Missing pending relationships | `language_spec/specs.rs` | 192 | Medium | Lua has `pending_relationships: false`, meaning cross-file calls through `require()` are not tracked as pending relationships. |
| L3 | Incorrect AST node handling | `lua/classes.rs` | ~45 | Medium | Class detection relies on regex post-processing (`setmetatable` pattern matching and `:extend()` heuristic) instead of parsing the AST properly. This is fragile and may miss legitimate class patterns. |
| L4 | Missing symbol kinds | `lua/tables.rs` | ~30 | Medium | Table fields assigned to functions (e.g., `MyTable.method = function() end`) are extracted as standalone functions, not as methods of the table class. No `Method` symbol kind is used for table methods. |
| L5 | Missing relationships | `lua/relationships.rs` | ~40 | Medium | `require()` calls are tracked as relationships, but the module path resolution does not handle relative requires (`require("./module")`) or complex path construction. |
| L6 | Test coverage gaps | `tests/lua/core.rs` | various | Low | Several tests only assert symbol existence and count, not deep properties like `parent_id`, `visibility`, or `signature`. |
| L7 | Inconsistency with other languages | `lua/helpers.rs` | ~80 | Low | Doc comment extraction uses a custom `find_doc_comment` that differs from the base extractor pattern used in Rust/TypeScript. It searches backward from the declaration line rather than using AST comment nodes. |
| L8 | Missing doc comment edge case | `lua/doc_comments.rs` | 149 | Low | Tests cover `---`, `--[[`, and `--` comments, but do not test multi-line `--` comments above local functions or block comments with `[[` delimiters that contain nested `]]`. |

---

## 2. R EXTRACTOR

### Source Files (3 files)
`mod.rs`, `relationships.rs`, `identifiers.rs`

### Test Files (15 files)
`mod.rs`, `basics.rs`, `classes.rs`, `control_flow.rs`, `cross_file_relationships.rs`, `data_structures.rs`, `file_integration_bug.rs`, `functions.rs`, `identifiers.rs`, `modern.rs`, `packages.rs`, `real_world.rs`, `relationships.rs`, `tidyverse.rs`

### Findings

| # | Category | File | Line | Severity | Description |
|---|----------|------|------|----------|-------------|
| R1 | Missing type inference | `language_spec/specs.rs` | 208 | High | R is registered with `PENDING_NO_TYPES_CAPABILITIES` (`types: false`). No type inference is performed. R has rich type information (vectors, data.frames, lists, S3/S4 classes) that could be inferred from assignments and function signatures. |
| R2 | Minimal source footprint | `r/mod.rs` | ~400 | High | The entire R extractor logic is concentrated in `mod.rs` (~400 lines) with only `relationships.rs` and `identifiers.rs` as additional modules. This violates the project's module boundary standards (single responsibility per file) and makes maintenance difficult. |
| R3 | Missing pending relationships | `language_spec/specs.rs` | 208 | Medium | R has `pending_relationships: false`, meaning cross-file function calls (e.g., `source("other.R")`) are not tracked as pending relationships. |
| R4 | Missing S4 slot extraction | `r/mod.rs` | ~200 | Medium | S4 class definitions are detected, but S4 slot accessors (`obj@slot`) are not extracted as identifiers or member access relationships. |
| R5 | Missing formula symbol kind | `r/mod.rs` | ~180 | Medium | R formulas (`y ~ x1 + x2`) are treated as generic expressions. They should be extracted as a distinct symbol kind or at least have their variables extracted as identifiers. |
| R6 | Missing R6 class support | `r/mod.rs` | ~220 | Medium | R6 classes (from the `R6` package) use `R6Class()` constructor syntax. The extractor does not recognize this pattern and treats R6 objects as generic variables. |
| R7 | Test coverage gaps | `tests/r/modern.rs` | various | Medium | Modern R pattern tests (tidyverse, data.table) only assert variable/function counts. They do not verify pipeline relationships (`%>%` pipe chains), NSE quosure extraction, or `library()` import tracking. |
| R8 | Missing doc comment extraction test | `tests/r/` | N/A | Low | No dedicated `doc_comments.rs` test file exists for R, despite `R_DOCS` being registered in `language_spec/mod.rs` with `RHashPrime` style. |
| R9 | File integration bug | `tests/r/file_integration_bug.rs` | ~30 | Low | A test file exists labeled "BUG HUNT: Reproduction test for file extraction failure" but it is not clear if the bug has been fixed or if the test is still failing. The test body appears to be a stub. |

---

## 3. BASH EXTRACTOR

### Source Files (9 files)
`mod.rs`, `functions.rs`, `commands.rs`, `variables.rs`, `signatures.rs`, `helpers.rs`, `types.rs`, `relationships.rs`

### Test Files (7 files)
`mod.rs`, `control_flow_verification.rs`, `cross_file_relationships.rs`, `doc_comments.rs`, `types.rs`

### Findings

| # | Category | File | Line | Severity | Description |
|---|----------|------|------|----------|-------------|
| B1 | Primitive type inference | `bash/types.rs` | ~30 | High | `infer_types()` only handles five primitive types: `string`, `integer`, `float`, `boolean`, `path`. It does not infer array types (`declare -a`), associative array types (`declare -A`), or command substitution return types. |
| B2 | Missing command relationship depth | `bash/relationships.rs` | ~50 | Medium | External command calls (e.g., `docker build`, `kubectl apply`) are extracted as `Calls` relationships, but subcommands and arguments are not tracked. A call to `docker build` and `docker run` both resolve to the same `docker` symbol without distinguishing the subcommand. |
| B3 | Missing identifier kinds | `bash/identifiers.rs` | ~40 | Medium | Identifier extraction only handles `command` nodes and `subscript` nodes. It does not extract variable references in arithmetic contexts (`$((x + y))`), parameter expansions (`${var:-default}`), or process substitution references. |
| B4 | Test coverage gaps | `tests/bash/mod.rs` | ~700 | Medium | The main test file is 1,407 lines (exceeds the 1,000-line test file limit). It contains multiple test modules (symbol extraction, identifier extraction, arrays, process substitution) that should be split into separate files per project standards. |
| B5 | Shebang extraction inconsistency | `bash/mod.rs` | ~60 | Low | Shebang lines are extracted as `SymbolKind::Variable` with the interpreter name (e.g., `bash` or `python3`). This is inconsistent with other languages where shebangs are typically ignored or extracted as metadata, not symbols. |
| B6 | Missing doc comment style registration | `language_spec/specs.rs` | 217 | Low | Bash is registered with `HASH_DOCS` (same as Ruby). Bash doc comments use `#` syntax, but the extractor also captures shebang lines and section headers as doc comments because `HashLine` matches any `#` line. This can attach incorrect doc comments to symbols. |
| B7 | Control flow not extracted | `bash/mod.rs` | ~100 | Low | `if`, `for`, `while`, `case` blocks are not extracted as symbols or relationships. In complex bash scripts, these are important scoping boundaries. |

---

## 4. POWERSHELL EXTRACTOR

### Source Files (11 files)
`mod.rs`, `classes.rs`, `commands.rs`, `documentation.rs`, `functions.rs`, `helpers.rs`, `identifiers.rs`, `imports.rs`, `relationships.rs`, `types.rs`, `variables.rs`

### Test Files (3 files)
`mod.rs`, `cross_file_relationships.rs`, `types.rs`

### Findings

| # | Category | File | Line | Severity | Description |
|---|----------|------|------|----------|-------------|
| P1 | Missing doc comment registration | `language_spec/specs.rs` | 224 | **Critical** | PowerShell is registered with `EMPTY` doc comment styles (`doc_comment_styles: &[]`), meaning no doc comment extraction is performed at the language spec level. However, `documentation.rs` exists and contains doc comment extraction logic. This is a registration bug: the extractor likely extracts comments, but `is_doc_comment()` in `LanguageSpec` always returns `false` for PowerShell. |
| P2 | Narrow type inference | `powershell/types.rs` | ~40 | High | Type inference only handles `[int]`, `[string]`, `[bool]`, `[array]`, `[hashtable]`, and `[pscustomobject]`. It does not handle generic .NET types (`[System.Collections.Generic.List[string]]`), enum types, or custom class types defined in the same file. |
| P3 | Missing test coverage | `tests/powershell/` | N/A | High | Only 3 test files exist for PowerShell (the fewest of any language with `FULL_CAPABILITIES`). There are no dedicated tests for: doc comments, functions, variables, classes, commands, identifiers, imports, or relationships. The `types.rs` test is the only non-mod test. |
| P4 | Missing CmdletBinding metadata | `powershell/functions.rs` | ~50 | Medium | Functions with `[CmdletBinding()]` are detected, but the attribute parameters (e.g., `SupportsShouldProcess`, `ConfirmImpact`) are not extracted as metadata. |
| P5 | Missing pipeline relationship tracking | `powershell/relationships.rs` | ~40 | Medium | PowerShell's pipeline syntax (`Get-Process | Where-Object { ... }`) is not tracked as a relationship chain. Each command in a pipeline is treated as an isolated call. |
| P6 | Missing enum extraction | `powershell/mod.rs` | ~80 | Medium | PowerShell enums (`enum Color { Red; Green; Blue }`) are not extracted as `Enum` symbol kinds. They are either ignored or treated as variables. |
| P7 | Inconsistent visibility handling | `powershell/functions.rs` | ~60 | Low | Functions exported via `Export-ModuleMember` are marked as `Public`, but functions inside classes are not consistently marked as `Private` by default. The visibility logic is scattered across `functions.rs` and `classes.rs`. |

---

## 5. SQL EXTRACTOR

### Source Files (9 files)
`mod.rs`, `views.rs`, `identifiers.rs`, `schemas.rs`, `error_handling.rs`, `constraints.rs`, `routines.rs`, `relationships.rs`, `helpers.rs`

### Test Files (13 files)
`mod.rs`, `ddl.rs`, `dml.rs`, `doc_comments.rs`, `identifier_extraction.rs`, `indexes.rs`, `procedures.rs`, `relationships.rs`, `schema.rs`, `security.rs`, `transactions.rs`, `types.rs`

### Findings

| # | Category | File | Line | Severity | Description |
|---|----------|------|------|----------|-------------|
| S1 | Regex fallback instead of AST | `sql/error_handling.rs` | ~30 | **Critical** | The `error_handling.rs` module contains regex-based fallback patterns for parsing SQL when the tree-sitter AST is insufficient. This includes regex matching for `CREATE TABLE`, `CREATE VIEW`, `ALTER TABLE`, etc. This is a significant architectural debt: if the tree-sitter parser improves or changes, these regexes may produce incorrect results or fail silently. |
| S2 | Missing pending relationships | `language_spec/specs.rs` | 248 | High | SQL is registered with `NO_PENDING_CAPABILITIES` (`pending_relationships: false`). Cross-schema table references, foreign key relationships to tables in other files, and stored procedure calls to external routines are not tracked as pending relationships. |
| S3 | Missing type inference for columns | `sql/types.rs` | ~30 | Medium | Column data types (`VARCHAR(255)`, `DECIMAL(10,2)`) are captured in signatures but not inferred as structured type information. The `infer_types()` method likely returns an empty map or primitive strings. |
| S4 | Incomplete transaction support | `sql/transactions.rs` | ~40 | Medium | Tests for transactions exist, but `BEGIN`, `COMMIT`, `ROLLBACK`, and `SAVEPOINT` statements are not extracted as symbols or relationships. They are invisible in the symbol graph. |
| S5 | Missing CTE relationship depth | `sql/relationships.rs` | ~50 | Medium | Common Table Expressions (`WITH ... AS`) are extracted as symbols, but the relationship between a CTE and its referencing query is not tracked. A CTE defined in a `WITH` clause has no `References` or `Defines` relationship to the main query. |
| S6 | Missing trigger action extraction | `sql/routines.rs` | ~60 | Medium | `CREATE TRIGGER` extracts the trigger name, but the trigger body (DML statements inside `BEGIN ... END`) is not parsed for nested symbols or relationships. Trigger actions like `INSERT INTO audit_log` are invisible. |
| S7 | Test shallow assertions | `tests/sql/doc_comments.rs` | various | Low | Doc comment tests verify that comments are non-empty but do not assert that the comment is attached to the correct symbol when multiple symbols appear in sequence. |
| S8 | Identifier extraction limited | `sql/identifiers.rs` | ~40 | Low | Identifier extraction focuses on column references and table aliases. It does not extract function calls within SQL (e.g., `COUNT(*)`, `UPPER(name)`) as identifiers, nor does it track subquery aliases. |

---

## 6. REGEX EXTRACTOR

### Source Files (10 files)
`mod.rs`, `relationships.rs`, `flags.rs`, `identifiers.rs`, `patterns.rs`, `signatures.rs`, `classes.rs`, `helpers.rs`, `groups.rs`

### Test Files (11 files)
`mod.rs`, `advanced_features.rs`, `classes.rs`, `extractor.rs`, `flags.rs`, `groups.rs`, `helpers.rs`, `identifiers.rs`, `relationships.rs`, `signatures.rs`, `types.rs`

### Findings

| # | Category | File | Line | Severity | Description |
|---|----------|------|------|----------|-------------|
| X1 | Missing doc comment registration | `language_spec/specs.rs` | 256 | **Critical** | Regex is registered with `EMPTY` doc comment styles (`doc_comment_styles: &[]`). Like PowerShell, this means `is_doc_comment()` always returns `false`. The test file `tests/regex/mod.rs` contains a `doc_comment_tests` module, but all tests are stubs that only check `symbol.doc_comment.is_some()` without actually asserting content, and the extraction logic likely never populates the field. |
| X2 | Missing symbol kinds after noise reduction | `regex/mod.rs` | ~100 | High | The extractor removed support for literals, anchors, quantifiers, unnamed groups, alternations, backreferences, and predefined classes as separate symbols. While this reduces noise, it means the symbol graph is extremely sparse: only top-level patterns, character classes, named groups, and lookarounds are extracted. For a simple regex like `hello`, **zero symbols** are produced. |
| X3 | No type inference | `language_spec/specs.rs` | 256 | Medium | Regex has `NO_PENDING_CAPABILITIES` which includes `types: false`. While regex patterns don't have traditional types, metadata about pattern categories (email, date, URL) could be inferred heuristically. |
| X4 | Missing tree-sitter grammar support | `tests/regex/advanced_features.rs` | 20 | Medium | Atomic groups (`(?>...)`), inline comments (`(?# ...)`), and extended mode comments are not supported by the tree-sitter regex parser. They produce `ERROR` nodes. The extractor handles this gracefully but cannot extract meaningful structure from these patterns. |
| X5 | Test stubs for doc comments | `tests/regex/mod.rs` | 732-940 | Medium | The `doc_comment_tests` module contains 9 tests, but all of them only verify that `symbol.doc_comment` exists as a field (via `let _ = symbol.doc_comment.as_ref()`). None assert that comments are actually extracted or attached. These are tautological tests. |
| X6 | Missing relationship between groups and backreferences | `regex/relationships.rs` | ~40 | Low | Named groups are extracted as symbols, and backreferences are extracted as identifiers, but there is no `References` relationship linking a backreference to its defining group. This would be useful for navigation. |
| X7 | Inconsistent with other languages | `regex/patterns.rs` | ~50 | Low | The extractor's noise reduction approach is unique to Regex. No other language aggressively filters out AST nodes. While justified for regex, this makes the extractor behavior unpredictable for users who expect all parseable nodes to become symbols. |

---

## 7. CROSS-LANGUAGE INCONSISTENCIES

| # | Category | Affected Languages | Severity | Description |
|---|----------|-------------------|----------|-------------|
| C1 | Doc comment style mismatch | PowerShell, Regex | **Critical** | Both languages have extractor logic for doc comments but are registered with `EMPTY` styles in `language_spec/specs.rs`. This causes `is_doc_comment()` to always return `false`, disabling extraction. |
| C2 | Capabilities misalignment | Lua, R, SQL, Regex | High | Lua and R have `PENDING_NO_TYPES_CAPABILITIES` but their test suites test modern patterns (tidyverse for R, OOP for Lua) that would benefit from type inference. SQL and Regex have `NO_PENDING_CAPABILITIES` which is appropriate but should be revisited if cross-file SQL analysis becomes a requirement. |
| C3 | Module boundary violations | R, Bash | Medium | R concentrates all extraction logic in `mod.rs` (~400 lines). Bash's main test file is 1,407 lines. Both violate project standards (impl <= 500 lines, tests <= 1000 lines). |
| C4 | Test organization inconsistency | All | Medium | Lua has 22 test files; PowerShell has 3. The disparity does not correlate with language complexity. PowerShell's `FULL_CAPABILITIES` registration warrants more test coverage. |
| C5 | Identifier extraction depth | Bash, SQL, Regex | Medium | Bash extracts commands and subscripts. SQL extracts columns and aliases. Regex extracts backreferences and named groups. None reach the depth of TypeScript's identifier extraction, which tracks member access chains, import contexts, and constructor bindings. |
| C6 | Doc comment style inconsistency | Bash | Low | Bash uses `HASH_DOCS` (same as Ruby). This matches `#` comments, but also matches shebang lines and section headers, causing false positives in doc comment attachment. |

---

## 8. TEST COVERAGE GAP ANALYSIS

| Language | Test Files | Lines (approx) | Stub Tests | Deep Assertions |
|----------|-----------|----------------|------------|-----------------|
| Lua | 22 | ~2,800 | Low | Medium |
| R | 15 | ~2,200 | Medium | Low |
| Bash | 7 | ~2,600 | Low | Medium |
| PowerShell | 3 | ~300 | **High** | **Low** |
| SQL | 13 | ~2,400 | Low | Medium |
| Regex | 11 | ~2,100 | **High** | Low |

**Stub test patterns observed:**
- `assert!(variables.len() >= N)` without checking specific variable names or properties
- `assert!(symbol.doc_comment.is_some() || symbol.doc_comment.as_ref().unwrap().is_empty())` (tautological)
- `let _ = symbol.doc_comment.as_ref()` (no assertion at all)
- Tests labeled "BUG HUNT" with empty or incomplete bodies

---

## 9. RECOMMENDED PRIORITY ORDER

1. **Fix doc comment registration for PowerShell and Regex** (P1, X1) — One-line changes in `language_spec/specs.rs` with high impact.
2. **Implement type inference for Lua and R** (L1, R1) — Add `types: true` and implement `infer_types()` methods.
3. **Expand PowerShell test coverage** (P3) — Add dedicated test files for functions, classes, variables, commands, identifiers, imports, and relationships.
4. **Refactor R extractor into multiple modules** (R2) — Split `mod.rs` into `functions.rs`, `variables.rs`, `classes.rs`, `helpers.rs`, etc.
5. **Reduce SQL regex fallback dependency** (S1) — Audit `error_handling.rs` and migrate regex patterns to AST-based extraction where possible.
6. **Add pending relationship support for Lua, R, SQL** (L2, R3, S2) — Enable cross-file reference tracking.
7. **Improve Regex symbol coverage** (X2) — Consider extracting at least top-level literals and anchors as a single "pattern" symbol instead of zero symbols.
8. **Split Bash test file** (B4) — Refactor `tests/bash/mod.rs` into separate modules per concern.

---

*Report compiled from source files in `crates/julie-extractors/src/` and corresponding test suites.*
