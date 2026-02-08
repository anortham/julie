# Extractor Quality Audit

**Started:** 2026-02-08
**Goal:** Systematic per-extractor audit of all 31 language extractors for correctness, completeness, edge cases, and code quality.

## Audit Criteria

Each extractor is evaluated on:

1. **Completeness** — Does it extract all meaningful symbol types for the language? (classes, functions, methods, imports, exports, types, constants, etc.)
2. **Symbol Kinds** — Are `SymbolKind` values correct? (e.g., a Python decorator shouldn't be `Class`)
3. **Signatures** — Are signatures accurate and useful? Do they include parameter types, return types, generics?
4. **Relationships** — Are parent-child relationships correct? (methods inside classes, nested types, etc.)
5. **Edge Cases** — Does it handle unusual but valid syntax? (unicode identifiers, deeply nested constructs, empty bodies, etc.)
6. **Noise** — Does it extract things that aren't real symbols? (HTML `<div>`, CSS `0%`, etc.)
7. **Sentinel Residue** — Any remaining hardcoded fallback names? (`"unknown"`, `"Anonymous"`, etc.)
8. **Code Quality** — Clean code, no dead branches, proper Option/Result handling?

### Rating Scale
- **A** — Production quality, no issues found
- **B** — Minor issues only, works well in practice
- **C** — Notable gaps or issues that affect usefulness
- **D** — Significant problems, needs rework

---

## Extractor Status

### Group 1: Core High-Level Languages

| Extractor | Files | Rating | Audited By | Notes |
|-----------|-------|--------|------------|-------|
| **Rust** | 7 | B | Claude Opus 4.6 | Solid extraction; enum/struct mapped to Class instead of Enum/Struct; no field/variant extraction |
| **TypeScript** | 10 | B | Claude Opus 4.6 | Good coverage; inference.rs uses wrong parser; class signatures missing; no enum members |
| **JavaScript** | 11 | B | Claude Opus 4.6 | Comprehensive; sentinel "unknown" in helpers.rs:194; extensive destructuring support |
| **Python** | 10 | B | Claude Opus 4.6 | Clean design; no parent_id on assignments; no nested class support; lambda naming is noisy |

### Group 2: JVM & .NET Languages

| Extractor | Files | Rating | Audited By | Notes |
|-----------|-------|--------|------------|-------|
| **Java** | 10 | A | Claude Opus 4.6 | Clean extraction; all files under 500 lines; zero sentinels; annotation_type uses Interface (no Decorator variant); package-private mapped to Private; only first field declarator extracted |
| **C#** | 8 | A | Claude Opus 4.6 | Comprehensive C# support; operators, indexers, delegates, events, records; zero sentinels; all files under 500 lines; clean where-clause and generic handling; internal mapped to Private with metadata |
| **Kotlin** | 6 | B | Claude Opus 4.6 | Good Kotlin-specific features; types.rs at 532 lines (over 500 limit); companion objects always named "Companion" even with custom name; internal visibility falls through to Public; rich metadata |
| **Swift** | 10 | A | Claude Opus 4.6 | Excellent Swift coverage; protocols, extensions, associated types, subscripts; zero sentinels; all files under 500 lines; protocol members extracted separately; comprehensive signatures |

### Group 3: Systems Languages

| Extractor | Files | Rating | Audited By | Notes |
|-----------|-------|--------|------------|-------|
| **C** | 7 | B | Claude Opus 4.6 | Good coverage; no union extraction; structs mapped to Class; 3 sentinel "unknown" in types.rs; declarations.rs at 736 lines (over 500 limit); hardcoded AtomicCounter fix |
| **C++** | 8 | B | Claude Opus 4.6 | Comprehensive extraction; template_declaration stub returns None; declarations.rs at 790 lines (over 500 limit); no sentinel values; clean visibility handling |
| **Go** | 8 | A | Claude Opus 4.6 | Excellent coverage; zero sentinels; correct visibility; type definitions use wrong `=` syntax in signatures; embedding relationships unimplemented stub |
| **Zig** | 9 | B | Claude Opus 4.6 | Good Zig-specific handling; type aliases mapped to Interface (questionable); 1 sentinel "unknown" in types.rs; error types mapped to Class; parameters extracted as symbols (noisy) |

### Group 4: Scripting Languages

| Extractor | Files | Rating | Audited By | Notes |
|-----------|-------|--------|------------|-------|
| **PHP** | 8 | B | Claude Opus 4.6 | Well-structured; methods use Function instead of Method; no sentinel residue; good signatures |
| **Ruby** | 8 | B | Claude Opus 4.6 | Comprehensive; 2 sentinel fallbacks (alias_method, delegated_method); good visibility tracking |
| **Lua** | 9 | B | Claude Opus 4.6 | Clever class detection via metatable patterns; "unknown" in type inference metadata; solid overall |
| **Bash** | 8 | B | Claude Opus 4.6 | Good DevOps focus; control flow blocks as Method is questionable; no sentinel residue |
| **PowerShell** | 11 | B | Claude Opus 4.6 | Most comprehensive scripting extractor; "ModuleMember" fallback; regex-heavy helpers |
| **R** | 3 | D | Claude Opus 4.6 | Minimal: only assignments extracted; no signatures; no type inference; no class/S4/R6 support |

### Group 5: Web & UI Languages

| Extractor | Files | Rating | Audited By | Notes |
|-----------|-------|--------|------------|-------|
| **HTML** | 9 | B | Claude Opus 4.6 | Good noise filtering; comments extracted as symbols with name "comment"; DOCTYPE as Variable |
| **CSS** | 8 | B | Claude Opus 4.6 | Clean extraction; keyframe percentages extracted as noise; class selectors mapped to Class |
| **Vue** | 7 | B | Claude Opus 4.6 | Regex-based script parsing (no tree-sitter); "VueComponent" sentinel fallback; no Composition API |
| **Razor** | 8 | B | Claude Opus 4.6 | Comprehensive C# extraction; 2 files over 500-line limit; "unknown" type sentinel; "expression" name sentinel |
| **QML** | 3 | B | Claude Opus 4.6 | Clean and focused; only root objects extracted as definitions; no enum/id/alias support |
| **GDScript** | 10 | A | Claude Opus 4.6 | Thorough extraction; proper @export/@onready handling; "unknown" in type inference only (acceptable) |

### Group 6: Data & Specialized

| Extractor | Files | Rating | Audited By | Notes |
|-----------|-------|--------|------------|-------|
| **SQL** | 7 | B | Claude Opus 4.6 | Very comprehensive; mod.rs at 662 lines (over 500 limit); 4 sentinel "unknown" values; extensive ERROR node recovery |
| **Dart** | 7 | C | Claude Opus 4.6 | mod.rs at 706 lines (over 500 limit); hardcoded test values in ERROR handler ("green","blue","Color"); no import extraction; thread-local content cache workaround |
| **Regex** | 8 | B | Claude Opus 4.6 | Thorough regex construct coverage; 3 sentinel "unknown" in flags.rs; very noisy (extracts every literal/anchor); niche use case |
| **JSON** | 1 | A | Claude Opus 4.6 | Clean, focused; proper key-value pair extraction; string values as doc_comments for search; no sentinels |
| **TOML** | 1 | B | Claude Opus 4.6 | Only extracts table headers, not individual key-value pairs; clean code; no sentinels |
| **YAML** | 1 | B | Claude Opus 4.6 | Synthetic names "document"/"flow_mapping"; no anchor/alias support; no sequence extraction |
| **Markdown** | 1 | A | Claude Opus 4.6 | Excellent frontmatter + section content for RAG; clean heading extraction; proper body capture |

---

## Detailed Findings

### Group 1: Core High-Level Languages

#### Rust

**Rating: B**

**What it extracts:** structs, enums, traits, unions, functions, methods (via two-phase impl block processing), modules, constants, statics, macro definitions (`macro_rules!`), macro invocations (struct-generating only), type aliases, use declarations (imports), associated types, function signatures (extern), doc comments (`///`, `//!`, `/** */`, `#[doc = "..."]`), derive attributes, generic type parameters, where clauses, async/unsafe/extern modifiers.

**Strengths:**
- Two-phase impl block processing is well-engineered: phase 1 collects all types, phase 2 links methods to parent types via byte-range reconstruction. Avoids unsafe lifetime issues.
- Comprehensive signature building: includes visibility, async, unsafe, extern, generics, parameters, return type, and where clauses.
- Proper visibility handling: distinguishes `pub`, `pub(crate)`, `pub(super)`, etc.
- Derive trait extraction embedded in struct/enum signatures provides useful context.
- Doc comment extraction handles all four Rust doc formats plus multi-line blocks.
- Cross-file pending relationships for imports and unknown call targets.
- Clean module decomposition: 7 files, all under 500 lines (largest is types.rs at 418 lines).
- No sentinel values found anywhere in the extractor.

**Issues:**
- [P1] Rust enums mapped to `SymbolKind::Class` (types.rs:99) instead of `SymbolKind::Enum`. The `SymbolKind` enum has an `Enum` variant. Makes it impossible to distinguish structs from enums in search results.
- [P1] Rust structs mapped to `SymbolKind::Class` (types.rs:53) instead of `SymbolKind::Struct`. The `SymbolKind` enum has a `Struct` variant. Same distinguishability problem.
- [P1] No extraction of struct fields or enum variants as child symbols. The relationship extractor walks field_declaration_list/enum_variant_list for type references, but individual fields/variants are not extracted as `SymbolKind::Field`/`SymbolKind::EnumMember` symbols. Cannot search for or navigate to fields.
- [P2] Macro invocation extraction (signatures.rs:120-163) only triggers when the macro name contains "struct" or "generate". This narrow heuristic misses most real-world macro invocations (`lazy_static!`, `bitflags!`, etc.).
- [P2] `extract_macro_invocation` uses `unwrap_or_default()` on the macro name (signatures.rs:131), silently producing an empty string if no identifier is found rather than returning `None` early.
- [P2] Use declaration extraction (signatures.rs:167-219) uses regex instead of tree-sitter node traversal. Grouped imports (`use foo::{bar, baz}`) and glob imports (`use foo::*`) are not handled.
- [P2] `infer_types` in mod.rs:146-186 compiles regex on every call per symbol. Should be compiled once (e.g., `OnceLock`).
- [P2] Static items mapped to `SymbolKind::Variable` (types.rs:327). Statics are semantically closer to constants.

**Missing extractions:**
- Struct fields (`field_declaration`) as `SymbolKind::Field`
- Enum variants as `SymbolKind::EnumMember`
- `let` bindings / local variables (intentional for noise reduction, acceptable)
- `extern` blocks and their contained FFI declarations
- Glob imports (`use foo::*`) and grouped imports (`use foo::{bar, baz}`)

**Test coverage:** Good. 2,248 lines across 9 test files. Covers struct, enum, trait, impl block, function, module, constant, generic type, and cross-file relationship extraction. Main test file (mod.rs) at 1,715 lines is over the 1,000-line test limit by a notable margin.

#### TypeScript

**Rating: B**

**What it extracts:** classes (with inheritance, abstract modifier), functions, methods, constructors, arrow functions (assigned to variables), variables, interfaces, type aliases, enums, namespaces/modules, properties (class and interface), imports, exports, doc comments. Also extracts identifiers (calls, member access) and relationships (calls, extends). Includes type inference module.

**Strengths:**
- Clean modular architecture: 10 well-focused files, all under 500 lines (largest is functions.rs at 313 lines).
- Arrow function detection correctly traces through variable_declarator parent to extract name.
- Method extraction properly identifies constructors by name and sets `SymbolKind::Constructor`.
- ID regeneration in functions.rs ensures symbol IDs are based on name position rather than body start, improving containment logic.
- Parent class ID lookup for methods tries multiple ID generation strategies before falling back to linear search.
- No sentinel values found in the extractor.
- `SymbolKind` mappings are correct: interfaces -> Interface, type aliases -> Type, enums -> Enum, namespaces -> Namespace, properties -> Property.

**Issues:**
- [P1] `inference.rs:30` uses `tree_sitter_javascript::LANGUAGE` to parse TypeScript content. This means TypeScript-specific syntax (interfaces, type annotations, generics, decorators) will fail to parse during type inference, causing silently incomplete results.
- [P1] Class extraction (classes.rs) does not generate a signature string. The `signature` field is set to `None`. All other symbol types include signatures. Class symbols appear without a human-readable description.
- [P1] No extraction of enum members. `extract_enum` (interfaces.rs:55-73) creates the enum declaration but does not walk into the enum body to extract individual members as `SymbolKind::EnumMember`.
- [P2] Interface extraction (interfaces.rs:11-30) does not capture `extends` clauses or interface members (method signatures, property signatures). Only the name is extracted.
- [P2] Property extraction (interfaces.rs:99-120) does not capture type annotations, readonly modifier, optional modifier, or access modifiers. Only the name is extracted.
- [P2] Export extraction (imports_exports.rs:45-83) only extracts the first export specifier from `export { a, b, c }` blocks (via `named_child(0)` at line 66). Subsequent specifiers are silently dropped.
- [P2] `find_containing_function` in relationships.rs:138-159 only matches `SymbolKind::Function`, not `SymbolKind::Method`. Method calls inside methods won't have their caller resolved for relationships.
- [P2] Tests overwhelmingly use `tree_sitter_javascript` parser. Only 3 test functions use the actual TypeScript parser. TypeScript-specific syntax (decorators, access modifiers, generics) is undertested.

**Missing extractions:**
- Enum members as `SymbolKind::EnumMember`
- Interface method signatures and property signatures as child symbols
- TypeScript decorators (`decorator` nodes not handled)
- TypeScript access modifiers on class members (`private`, `protected`, `public`, `readonly`)
- Ambient declarations (`declare function`, `declare class`, etc.)
- Type guards, conditional types, mapped types, utility types

**Test coverage:** Good. 2,524 lines across 12 test files. Covers function declarations, class declarations, arrow functions, methods, imports/exports, identifiers, relationships, cross-file relationships, type inference, and relative paths. However, TypeScript-specific tests are sparse -- most tests use JavaScript parser.

#### JavaScript

**Rating: B**

**What it extracts:** classes (with extends), functions (declarations, expressions, arrow, generators), methods (with static, getter, setter, async, generator detection), constructors, variables (const/let/var with initializer tracking), destructuring assignments (object and array patterns, rest parameters), imports (ES6 named, default, namespace, CommonJS require), exports (named, default, re-exports), properties (class fields, object properties with function-as-method promotion), prototype method assignments, static method assignments, doc comments (JSDoc), identifiers (calls, member access), relationships (calls, extends).

**Strengths:**
- Most comprehensive extractor of the four. 11 well-focused files, all under 500 lines (largest is mod.rs at 293 lines).
- Destructuring variable extraction is thorough: handles object patterns, array patterns, rest parameters, and nested patterns.
- Assignment expression handling correctly identifies prototype method assignments (`Constructor.prototype.method = function()`) and static assignments (`Class.method = function()`).
- Function-as-property promotion: when a property value is an arrow function or function expression, the symbol kind is upgraded from Property to Method.
- CommonJS require detection correctly classifies `const x = require('module')` as Import.
- Signature building is detailed: includes async, generator, static, getter/setter modifiers.
- Visibility inference from naming conventions (#private, _protected, public) is appropriate for JavaScript.
- JSDoc type inference extracts `@returns {Type}` and `@type {Type}` annotations.

**Issues:**
- [P1] Sentinel value `"unknown"` returned in `extract_require_source` (helpers.rs:194) when require arguments cannot be parsed. Should return an empty string or `Option<String>` instead. This value can propagate into metadata and confuse search results.
- [P2] Import specifier extraction (imports.rs:57-100) may extract both original name AND alias for aliased imports (lines 69-74: pushes both `name` and `alias`), creating duplicate import symbols.
- [P2] `get_declaration_type` (helpers.rs:144-168) defaults to `"var"` when no declaration keyword is found. Symbols inside non-standard contexts will be labeled as `var` declarations.
- [P2] `build_class_signature` (signatures.rs:12-43) uses `unwrap_or_default()` for the class name, silently producing "class " with no name if the name node is missing.
- [P2] No extraction of object literal keys as properties when the object is assigned to a named variable (e.g., `const config = { port: 3000, host: 'localhost' }` keys are not extracted).

**Missing extractions:**
- Enum-like patterns (frozen objects, Symbol-based enums)
- Class static blocks (`static { ... }`)
- Optional chaining expressions as identifiers
- Template literal tag functions

**Test coverage:** Excellent. 2,516 lines across 10 test files. Dedicated test files for error handling (309 lines), identifier extraction (273 lines), JSDoc comments (355 lines), legacy patterns (248 lines), modern features (591 lines), relationships, scoping (229 lines), and types. The modern_features test file covers async/await, generators, destructuring, classes, modules, private fields, and more.

#### Python

**Rating: B**

**What it extracts:** classes (with inheritance, metaclass, Enum/Protocol detection), functions (with decorators, async, type hints, return type annotations), methods (__init__ -> Constructor, others -> Method), lambdas, variables (with type annotations, constant detection via uppercase naming), enum members (uppercase names inside Enum subclasses), imports (import, from...import, aliased imports), self.attribute assignments as Properties, __slots__ as Property, docstrings (triple-quoted strings in function/class body), decorators (@property, @staticmethod, @classmethod, custom), identifiers (calls, attribute access), relationships (inheritance with Implements/Extends distinction, calls with pending cross-file resolution).

**Strengths:**
- Clean, focused architecture: 10 files, all under 500 lines (largest is mod.rs at 172 lines). Smallest total codebase of the four extractors (1,408 lines).
- Smart Protocol detection: classes inheriting from `Protocol` are correctly mapped to `SymbolKind::Interface`.
- Smart Enum detection: classes inheriting from `Enum` are mapped to `SymbolKind::Enum`, and uppercase assignments inside enum classes are mapped to `SymbolKind::EnumMember`.
- Visibility inference follows Python conventions: `__dunder__` -> Public, `_private` -> Private, everything else -> Public.
- Docstring extraction correctly handles triple-quoted strings inside expression_statement nodes.
- Parameter extraction handles all Python parameter types: simple, default, typed, typed_default.
- Decorator extraction walks up to `decorated_definition` parent and strips `@` prefix and parameters.
- Multiple assignment targets (tuple unpacking `a, b = 1, 2`) correctly produce separate symbols.
- No sentinel values found anywhere in the extractor.

**Issues:**
- [P1] Assignment extraction (assignments.rs:83) never sets `parent_id`. The comment acknowledges this: "Parent tracking not yet implemented for assignments." This means `self.x = value` inside `__init__` won't be linked to its parent class, and class-level constants won't be linked either. Breaks parent-child navigation for all variable/property/constant symbols.
- [P2] Lambda naming uses `<lambda:row>` format (functions.rs:113) which produces non-searchable names like `<lambda:15>`. These angle-bracket names may cause issues in search indexing.
- [P2] No nested class support. `extract_class` does not track parent class context, so inner classes (common in Python, e.g., Django Meta classes) are extracted as top-level symbols with no parent_id.
- [P2] `extract_class` (types.rs:12) finds the class name by `node.children().nth(1)` instead of using `child_by_field_name("name")`. This positional approach is fragile.
- [P2] `has_async_keyword` (signatures.rs:87-96) only checks direct children for an "async" node kind. For `async_function_definition` nodes, the async flag might not be reliably set via this check.
- [P2] Wildcard imports (`from module import *`) are not handled.
- [P2] `infer_type_from_signature` in mod.rs:115-139 compiles regex on every call. Should be compiled once.

**Missing extractions:**
- Nested classes (inner classes with parent_id)
- `__all__` declarations (module-level export list)
- Wildcard imports (`from module import *`)
- Global/nonlocal declarations
- `@property` decorated methods as Property kind (currently extracted as Method)
- Named expressions (walrus operator `:=`)

**Test coverage:** Good. 1,939 lines across 12 test files. Main test file (mod.rs) at 1,030 lines covers classes, functions, imports, assignments, decorators, docstrings, and edge cases. Cross-file relationship tests (278 lines) and relationship tests (279 lines) are well-structured. Several sub-module test files are stubs (7 lines each: assignments.rs, decorators.rs, identifiers.rs, imports.rs) that only declare the module -- actual tests are in mod.rs.

---

### Group 2: JVM & .NET Languages

#### Java

**Rating: A**

**What it extracts:** Classes, interfaces, enums (with constants), records (Java 16+), methods, constructors, fields (with constant detection via static+final), annotation type declarations, imports (including static and wildcard), package declarations, JavaDoc comments.

**Strengths:**
- Well-organized into 10 focused files, all well under 500-line limit (largest is `classes.rs` at 280 lines)
- Zero sentinel values -- all name extraction uses `?` operator to return `None` on missing names
- Comprehensive signature construction including generics, throws clauses, inheritance, permits (sealed classes)
- Smart field classification: `static final` fields detected as `SymbolKind::Constant`
- Record support with formal parameters and interface implementations
- Enum constants extracted with argument lists when present
- Cross-file relationship support via `PendingRelationship` pattern
- Identifier extraction handles both `method_invocation` and `field_access` with proper deduplication
- Static regex compilation via `LazyLock` in type inference

**Issues:**
- [P2] `annotation_type_declaration` uses `SymbolKind::Interface` -- acceptable since no `Decorator`/`Annotation` variant exists, and the `@interface` signature makes intent clear
- [P2] Java's default visibility (package-private) is mapped to `Visibility::Private` (helpers.rs:28) -- package-private is actually broader than private but there's no package-private variant in the enum
- [P2] Multi-field declarations (`int x, y, z;`) only extract the first declarator (fields.rs:42) -- the comment acknowledges this
- [P2] Method return type defaults to `"void"` when not found (methods.rs:41) -- could theoretically misidentify a method with an unrecognized type as void, though unlikely in practice

**Missing extractions:**
- Lambda expressions (not extracted as symbols, though tested as part of method bodies)
- Local variable declarations (by design -- not useful for code intelligence)
- Static initializer blocks
- Annotation members (elements within `@interface` declarations)

**Test coverage:** Excellent -- 12 test files covering classes, interfaces, enums, methods, generics, annotations, modern Java features (records, text blocks, streams), JavaDoc extraction, identifiers, cross-file relationships, and package/import handling. Total: ~2,303 lines of tests.

#### C# (csharp)

**Rating: A**

**What it extracts:** Namespaces, using directives (regular, static, aliased), classes, interfaces, structs, enums (with members), records (class and struct variants), methods, constructors, destructors, properties (with accessor lists), fields (with constant detection via `const` or `static readonly`), events, delegates, operators (arithmetic, comparison, logical, bit-shift, true/false), conversion operators (implicit/explicit), indexers, XML doc comments.

**Strengths:**
- Exceptionally comprehensive C# coverage -- handles operators, indexers, delegates, events, conversion operators which most extractors skip
- All 8 files under 500-line limit (largest is `members.rs` at 407 lines)
- Zero sentinel values -- clean `?` operator usage throughout
- `internal` visibility mapped to `Visibility::Private` with actual C# visibility stored in metadata (`csharp_visibility` key) for downstream use
- Record declarations properly detect `record struct` vs `record class` variants
- Where clauses on generics extracted and appended to signatures
- Arrow expression clauses (`=>`) included in method/operator signatures
- Using directives support aliases (`using Alias = Namespace.Type`)
- Base list extraction properly filters out `:` and `,` punctuation tokens
- Destructor names prefixed with `~` for clarity

**Issues:**
- [P2] Constructor default visibility assumes `Public` when no modifier present (helpers.rs:49-51) -- this is an oversimplification; C# constructors default to private when in a class without an explicit access modifier, though in practice most constructors in code have explicit modifiers
- [P2] `extract_property_type` iterates children checking text against modifier list (helpers.rs:136-168) -- slightly fragile approach but works in practice due to AST structure
- [P2] `event_field_declaration` type defaults to `"EventHandler"` when type node is not found (members.rs:315) -- minor fallback value

**Missing extractions:**
- Local functions (`local_function_statement`) -- not extracted as symbols, though call relationships from them are detected
- File-scoped namespaces (`namespace Foo;` without braces) -- handled if tree-sitter parser produces `namespace_declaration` node
- `readonly struct` modifier detection in signatures
- Partial classes/methods (no `partial` keyword handling beyond being included in modifiers)

**Test coverage:** Very thorough -- 9 test files covering core types, language features (generics, LINQ, async/await, nullable), metadata extraction, runtime behavior, identifiers, cross-file relationships. Total: ~2,699 lines of tests.

#### Kotlin

**Rating: B**

**What it extracts:** Classes (regular, data, sealed, abstract, inner), enum classes (with members), interfaces (including fun interfaces), objects, companion objects, functions (top-level and methods), extension functions, properties (val/var with delegation and initializers), constructor parameters (as properties), type aliases, imports, package declarations, operator functions, KDoc comments.

**Strengths:**
- Rich Kotlin-specific feature support: data classes, sealed classes, companion objects, extension functions, property delegation (`by lazy`), infix/operator functions
- Zero sentinel name values -- all name extraction uses `?` operator properly
- Extension function signatures correctly include receiver type (`String.functionName`)
- Enum class detection handles both `enum_declaration` node type and `enum` keyword in modifiers
- Constructor parameters (`val`/`var` in primary constructor) extracted as property symbols with type info
- Property delegation (`by lazy`, `by Delegates.notNull()`) included in signatures
- Type aliases capture the aliased type after `=` sign, including complex types like `suspend (T) -> Unit`
- Where clauses extracted for generic constraints
- Fun interface detection and proper signature formatting
- Metadata includes modifiers, return type, property type for downstream type inference

**Issues:**
- [P1] `types.rs` is 532 lines -- exceeds the 500-line limit by 32 lines
- [P2] Companion objects always get the symbol name `"Companion"` even when they have a custom name (types.rs:215) -- the custom name only appears in the signature. A companion object declared as `companion object Factory` should have the name `Factory`
- [P2] `internal` visibility modifier is extracted as a modifier string but not handled in `determine_visibility()` -- it falls through to the default `Visibility::Public` (helpers.rs:335-343). While Kotlin does default to public, `internal` should arguably map to something other than `Public`
- [P2] Constructor parameter type defaults to empty string `""` when not found (properties.rs:168) -- used as a sentinel-like value, though the code checks `if !param_type.is_empty()` before using it
- [P2] `extract_function` calls `extract_return_type` twice -- once for the signature and again for metadata (types.rs:264, 316)

**Missing extractions:**
- Secondary constructors (only primary constructor parameters are extracted)
- Annotation declarations (`annotation class`)
- Destructuring declarations
- Property accessors (custom get/set)
- `init` blocks
- `value class` / inline class declarations (treated as regular classes)

**Test coverage:** Good -- 3 test files with comprehensive coverage of classes, objects, functions, interfaces, generics, type aliases, KDoc, identifiers, and cross-file relationships. Tests total ~2,194 lines. The main test file at 1,791 lines is large but acceptable for tests.

#### Swift

**Rating: A**

**What it extracts:** Classes, structs, protocols (as Interface), enums (with associated values and raw values), enum cases, functions/methods, initializers (`init`), deinitializers (`deinit`), variables (let/var), properties, subscripts, extensions, imports, type aliases, associated types, protocol function requirements, protocol property requirements (with getter/setter), Swift doc comments.

**Strengths:**
- Excellent module organization -- 10 focused files, all well under 500 lines (largest is `relationships.rs` at 383 lines)
- Zero sentinel values -- clean `?` operator usage with proper `Option` returns
- Comprehensive Swift-specific feature support: protocols mapped correctly to `SymbolKind::Interface`, extensions as `SymbolKind::Class`, associated types as `SymbolKind::Type`
- Protocol members extracted via dedicated `protocol_function_declaration` and `protocol_property_declaration` handlers -- not just generic function/property extraction
- Subscript declarations extracted with parameters, return type, and accessor requirements
- Enum cases handle both `enum_case_declaration` (multiple cases) and `enum_entry` (single case) nodes
- Extension declarations include conformance clauses and extended type name in metadata
- Generic parameters and where clauses properly extracted and included in signatures
- Initializer parameters extracted separately from function parameters (different AST structure)
- Visibility handling correctly maps `fileprivate` to Private and `internal` to Protected
- Property extraction distinguishes `let` vs `var` and includes type annotations
- Inheritance extraction handles both `type_inheritance_clause` and `inheritance_specifier` nodes (parser version compatibility)
- `infer_types` method uses metadata-based type inference with proper priority (returnType > propertyType > variableType > generic "type")

**Issues:**
- [P2] `extract_class` handles enum/struct/extension via `class_declaration` node kind (types.rs:19-48) -- suggests the tree-sitter Swift grammar may not always produce distinct node kinds for these, so the fallback is necessary but makes the code harder to follow
- [P2] Protocol function/property doc comments are set to `None` (protocol.rs:59, 145) instead of calling `find_doc_comment` -- minor inconsistency
- [P2] `extract_where_clause` has a text-scanning fallback (signatures.rs:119-131) that scans child nodes for text containing `"where "` -- fragile but unlikely to produce false matches
- [P2] Extension declarations use `SymbolKind::Class` -- could be more semantically accurate with a dedicated kind, but Class is the best available option

**Missing extractions:**
- Computed properties (willSet/didSet/get/set bodies are not extracted as separate symbols)
- Operator declarations (only operator functions within types are extracted via `function_declaration`)
- Nested function declarations within other functions
- `@propertyWrapper` struct detection (attributes are extracted as modifiers but not semantically analyzed)
- `@resultBuilder` and macro declarations
- Global constants/variables outside of types

**Test coverage:** Comprehensive -- 3 test files covering class/struct extraction, protocol/extension extraction, enum/associated values, generics/type constraints, closures/function types, property wrappers/attributes, identifier extraction, type inference, relationships, and doc comments. Total: ~1,876 lines of tests.

---

### Group 3: Systems Languages

#### C

**Rating: B**

**What it extracts:** Functions (definitions and declarations), structs, enums (with enum values as separate Constant symbols), typedefs (including function pointer typedefs), macros (#define, function-like macros), variables (global/static/extern/const/volatile/array), includes (#include), linkage specifications (extern "C"), expression statement typedefs (edge case recovery).

**Strengths:**
- Thorough handling of C's complex declaration syntax including function pointers, multi-variable declarations, and typedef struct patterns
- Separate extraction of enum values as child Constant symbols with proper parent_id linkage
- Good visibility detection via `static` keyword (private) vs default (public)
- Rich metadata on every symbol (type, isStatic, isExtern, isConst, isVolatile, isArray, initializer, returnType, parameters, isDefinition)
- Function pointer typedef names correctly fixed via post-processing regex pass
- Variadic parameters handled correctly
- Doc comment extraction for all declaration types
- Cross-file relationship support via PendingRelationship for unresolved function calls
- Include relationships tracked with isSystemHeader detection

**Issues:**
- [P1] No union extraction: `union_specifier` is completely absent from `visit_node` match arms in `c/mod.rs`. C unions are a significant language feature used in systems programming.
- [P1] Structs mapped to `SymbolKind::Class` instead of `SymbolKind::Struct` (`declarations.rs:328`). There is a `Struct` variant available in the enum.
- [P2] Three sentinel `"unknown".to_string()` fallbacks in `types.rs` (lines 129, 287, 332) for variable types and underlying types when extraction fails. Should return `Option<String>` or empty string.
- [P2] `declarations.rs` is 736 lines, exceeding the 500-line limit. Should be split (e.g., separate typedef handling into its own module).
- [P2] Hardcoded `AtomicCounter` fix in `reconstruct_struct_signature_with_alignment` (`declarations.rs:731-735`) -- this is a test-specific hack baked into production code.
- [P2] Macros mapped to `SymbolKind::Constant` -- reasonable given no `Macro` variant exists, but the metadata correctly records `type: "macro"`.
- [P2] Function declarations (prototypes without bodies) are extracted as `Function` with `isDefinition: false` -- this is correct behavior but could produce noise in header-heavy codebases.

**Missing extractions:**
- Unions (`union_specifier`)
- Struct fields as separate child symbols (only counted in metadata as "N fields")
- Bit fields
- `_Atomic`, `_Bool`, `_Complex` type qualifiers
- `#pragma` directives
- `#ifdef`/`#ifndef` conditional compilation blocks
- Anonymous structs/enums (skipped via `?` on name extraction)

**Test coverage:** 29 tests across 9 test files covering basics, advanced, preprocessor, pointers, doxygen comments, types, identifiers, relationships, and cross-file relationships. Good breadth.

---

#### C++ (cpp)

**Rating: B**

**What it extracts:** Classes (with template parameters and inheritance), structs (with alignas), unions (including anonymous), enums (regular and `enum class` with underlying type), enum members, functions, methods, constructors, destructors, operators (overloaded and conversion), namespaces, using declarations/namespace aliases, friend declarations, fields (including multi-declarator), variables, constants (const/constexpr/static members), template declarations.

**Strengths:**
- Comprehensive C++ feature coverage including templates, operator overloading, conversion operators, friend declarations, and access specifiers
- Correct visibility handling: walks class body to find most recent `access_specifier` before each member; defaults to `private` for classes, `public` for structs/unions
- Template parameters included in signatures via parent node traversal
- Inheritance chains correctly parsed from `base_class_clause` with access specifiers
- Distinguishes constructors (name matches enclosing class), destructors (`~` prefix), operators (`operator` prefix), and regular methods
- Handles `= delete` and `= default` method specifiers
- Robust handling of multiple fields on same line (`size_t rows, cols;`)
- Zero sentinel values -- clean extraction throughout
- Handles `noexcept` specifier and `const` qualifier on methods
- ERROR node recovery for malformed class/struct declarations
- Clean modular architecture with good separation of concerns

**Issues:**
- [P1] `extract_template` is a stub that returns `None` (`declarations.rs:109-117`). Template declarations themselves produce no symbol; they rely on the inner declaration being extracted during tree walking. This works because the tree walker recurses into children, but means template parameters are only captured when the inner function/class explicitly looks for them via parent traversal. Template variable declarations (e.g., `template<class T> constexpr T pi = T(3.14)`) would be missed.
- [P2] `declarations.rs` is 790 lines, exceeding the 500-line limit. The visibility extraction logic (lines 700-790) and field extraction (lines 254-407) could be separate modules.
- [P2] Anonymous unions use a generated name `<anonymous_union_N>` (`types.rs:144`). This is a reasonable approach but the angle brackets in the name could cause issues with search/display. Anonymous structs and enums are silently skipped (return None when no name found) -- inconsistent with union handling.
- [P2] Only the first `init_declarator` is processed in variable declarations (`declarations.rs:219`) -- `int x = 1, y = 2;` would only extract `x`.
- [P2] Trailing return type extraction (`functions.rs:346-373`) has redundant logic checking both `->` token and `trailing_return_type` node.

**Missing extractions:**
- Template variable declarations
- `static_assert` declarations
- `concept` declarations (C++20)
- `requires` clauses on functions/methods
- `consteval` and `constinit` specifiers (C++20)
- Structured bindings (`auto [a, b] = ...`)
- Anonymous structs (silently skipped, unlike unions)
- Nested class forward declarations
- `typedef` declarations (no `type_definition` handler)

**Test coverage:** 38 tests across 13 test files covering classes, templates, namespaces, functions, types, modern C++ features, concurrency, exceptions, robustness, doxygen comments, identifiers, cross-file relationships, and testing patterns. Excellent breadth and depth.

---

#### Go

**Rating: A**

**What it extracts:** Packages, imports (with alias detection, blank import filtering), structs, interfaces (with union type bodies), type aliases, type definitions, functions (with generics), methods (with receiver types), variables (`var`), constants (`const`), fields (including multi-name patterns like `X, Y float64`), field tags (`json:"id"`).

**Strengths:**
- Zero sentinel values -- completely clean extraction
- Correct Go visibility model: uppercase first letter = public, lowercase = private; `main` and `init` always private
- Proper receiver handling for methods: extracts receiver type, distinguishes pointer vs value receivers
- Multi-name field declarations handled correctly (`X, Y float64` produces two Field symbols)
- Generic type parameter support on functions, methods, and type declarations
- Multiple return types correctly formatted: single `string`, multiple `(int, error)`
- Import alias detection with blank import (`_`) filtering
- Interface body extraction including type element unions
- Deduplication via `prioritize_functions_over_fields` to handle name collisions
- Good signature quality matching Go convention: `func (r *Repo) Get(id int64) (*User, error)`
- ERROR node recovery for partial function signatures

**Issues:**
- [P2] Type definitions (e.g., `type UserID int64`) incorrectly use `=` in signature: `type UserID = int64` (`types.rs:175`). In Go, `type X Y` is a type definition and `type X = Y` is a type alias -- these have different semantics. The code has a comment acknowledging this: "For type definition (no equals sign) - formats these like aliases".
- [P2] `extract_embedding_relationships` is an unimplemented stub (`relationships.rs:78-87`). Go struct embedding is a core Go feature for composition.
- [P2] `find_containing_function` for method declarations looks for `identifier` but Go methods use `field_identifier` for the method name (`relationships.rs:193`). This means call relationships inside methods might not find the caller correctly.
- [P2] Two `#[allow(dead_code)]` functions in `signatures.rs` (lines 3, 52) -- `build_function_signature` and `build_method_signature` are unused.
- [P2] Go test `mod.rs` is 2401 lines (over 1000-line test file limit).

**Missing extractions:**
- Struct embedding relationships (stub exists but unimplemented)
- Short variable declarations (`:=`) inside function bodies
- `iota` constant groups (individual iota values not tracked)
- Go `//go:` directives (build constraints, compiler directives)
- Interface method declarations as child symbols (only body type elements extracted)

**Test coverage:** 45 tests across 10 test files covering packages, structs, interfaces, functions, methods, generics, imports, constants, variables, edge cases, concurrency, error handling, build tags, type assertions, cross-file relationships, and integration tests. Excellent comprehensive coverage.

---

#### Zig

**Rating: B**

**What it extracts:** Functions (pub/export/inline/extern modifiers), methods (inside structs), test declarations, structs (regular/packed/extern), unions (regular/union(enum)), enums (with backing type), enum variants, struct/container fields, variables (`var`/`const`), type aliases, error types, error sets (with union support), function type aliases, generic type constructors (`fn(comptime T: type) type` pattern), parameters.

**Strengths:**
- Good coverage of Zig-specific constructs: comptime parameters, error sets, error unions, packed structs, extern structs
- Test declarations properly extracted with `isTest` metadata
- Generic type constructor detection via pattern matching on `(comptime` + `= struct`
- Correct handling of `pub`/`export`/`inline` visibility modifiers with fallback to prev_sibling check
- Function type aliases detected and extracted with `isFunctionType` metadata
- Error set assignments extracted with error union detection (`||` operator)
- Good type inference via `type_inference.rs` using regex + metadata
- ERROR node recovery for partial generic type constructor patterns
- Clean modular architecture with focused files all under 500 lines

**Issues:**
- [P1] Type aliases mapped to `SymbolKind::Interface` (`types.rs:239`). Zig type aliases are not interfaces. `SymbolKind::Type` would be the correct mapping, consistent with how Go handles the same concept.
- [P1] Function type aliases also mapped to `SymbolKind::Interface` (`variables.rs:297`). Same issue -- a function type typedef is not an interface.
- [P1] Error types mapped to `SymbolKind::Class` (`types.rs:198`). Zig errors are not classes. There's no perfect mapping, but `Type` or `Enum` would be more accurate since error sets are enumerated values.
- [P2] One sentinel `"unknown".to_string()` in `types.rs:163` when struct field type cannot be determined.
- [P2] Parameters are extracted as individual `Variable` symbols (`functions.rs:94-133`). This is noisy -- function parameters are not typically standalone definitions. They inflate the symbol count and can pollute search results.
- [P2] Union and struct types both map to `SymbolKind::Class` (`types.rs:28, 70`). Zig structs and unions could use `Struct` and `Union` variants that exist in the enum.
- [P2] `extract_function_signature` has a heuristic that checks raw function text for `"..."` and adds variadic parameter (`functions.rs:228-231`). This could false-positive on string literals containing `"..."`.
- [P2] Zig test `mod.rs` is 1962 lines (over 1000-line test file limit).

**Missing extractions:**
- `usingnamespace` declarations
- `@import` as Import symbols (currently only extracted as const assignments)
- Comptime blocks (`comptime { ... }`)
- Assembly blocks (`asm volatile (...)`)
- Labeled blocks and loops
- Tagged union payloads
- `threadlocal` variables

**Test coverage:** 25 tests across 4 test files covering structs, unions, enums, functions, variables, constants, generic type constructors, error sets, cross-file relationships, and extractor integration. Moderate coverage -- fewer edge case tests compared to C/C++/Go.

---

### Group 4: Scripting Languages

#### PHP

**Rating: B**

**What it extracts:** Classes (with extends, implements, trait usage), interfaces (with extends), traits, enums (with backing type and implements), enum cases (with values), functions, methods (including `__construct`/`__destruct`), properties (with typed properties, promoted constructor params), constants (class and standalone), namespaces, use/import declarations (with aliases), variable assignments, PHPDoc comments.

**Strengths:**
- Well-organized into 8 focused files, all under 500 lines (largest is `types.rs` at 324 lines)
- Zero sentinel values -- all name extraction uses `?` operator to return `None` on missing names
- Good signature construction: class signatures include `extends`, `implements`, and trait `use` clauses
- Enum support is comprehensive: backing types (`: string`), implements clauses, case values
- PHPDoc extraction on all symbol types via `find_doc_comment`
- Cross-file relationship support via `PendingRelationship` for unresolved calls
- Visibility handling covers `public`, `private`, `protected` with proper defaults
- Identifier extraction handles `function_call_expression`, `member_call_expression`, `scoped_call_expression`, `member_access_expression`, `name`

**Issues:**
- [P1] All methods use `SymbolKind::Function` instead of `SymbolKind::Method` (`functions.rs:35`). Only `__construct` gets `Constructor` and `__destruct` gets `Destructor`. Regular class methods like `public function getName()` are tagged as `Function`, which is semantically wrong and prevents method-vs-function queries.
- [P2] `namespace_use_declaration` handler extracts only the first `namespace_use_clause` (`namespaces.rs:50-51`). PHP grouped use declarations like `use App\{Controller, Model, Service}` would only extract `Controller`.
- [P2] Variable assignment extraction is at the top level only (`assignment_expression` in `visit_node`). Variables inside function bodies are also extracted as top-level symbols, which could be noisy.
- [P2] `extract_modifiers` scans for specific string tokens (`abstract`, `final`, `static`, `readonly`) via text matching rather than AST node kinds (`helpers.rs:29-42`).

**Missing extractions:**
- Anonymous classes (`new class { ... }`)
- Arrow functions (`fn($x) => $x + 1`)
- Named arguments in function calls
- Match expressions
- First-class callable syntax (`strlen(...)`)
- PHP attributes (`#[Route('/api')]`) as separate symbols (they appear in class signatures but aren't independently extractable)
- Closure/lambda declarations

**Test coverage:** 6 test files totaling ~3,323 lines covering classes, functions, methods, namespaces, enums, interfaces, traits, properties, constants, visibility, PHPDoc, identifiers, and cross-file relationships. Good coverage.

---

#### Ruby

**Rating: B**

**What it extracts:** Modules, classes (with inheritance), singleton classes, methods, singleton methods, variables (instance, class, global, local, constants), constants, aliases, require/require_relative/include/extend calls, attr_accessor/attr_reader/attr_writer, define_method, def_delegator/def_delegators, parallel assignments, RDoc/YARD comments.

**Strengths:**
- Comprehensive Ruby-specific feature support: singleton methods, attr_accessor family, dynamic method definition via `define_method`, delegation via `def_delegator`
- Visibility tracking via `current_visibility` field that tracks `public`/`private`/`protected` sections within classes
- Good signature construction for modules, classes (with inheritance), and methods (with parameters)
- Parallel assignment support (`a, b, c = 1, 2, 3`) extracts individual variables
- Rest assignment support (`first, *rest = array`)
- Cross-file relationship support via `PendingRelationship`
- Identifier extraction covers method calls, member access, constant references

**Issues:**
- [P1] Sentinel fallback `"delegated_method"` in `calls.rs:134` when `def_delegator` target method name cannot be extracted. Should return `None` instead.
- [P1] Sentinel fallback `"alias_method"` in `helpers.rs:176` used as default name for alias method calls. Should return `None` instead.
- [P2] Dead code in `helpers.rs`: `extract_assignment_symbols` and `extract_parallel_assignment_fallback` are marked `#[allow(dead_code)]` -- these should be removed or used.
- [P2] `extract_call` in `calls.rs` does a lot of string matching on method names (`attr_accessor`, `include`, `extend`, `require`, etc.) which could miss variants or false-positive on user methods with the same names.
- [P2] Block parameters (`|x, y|`) are not extracted as symbols, which means Ruby block-local variables are invisible.

**Missing extractions:**
- `module_function` declarations
- `Struct.new` class definitions (`Person = Struct.new(:name, :age)`)
- Refinements (`refine`)
- `prepend` module inclusion
- Method visibility per-method (`private :method_name`)
- `protected` method declarations (section tracking exists but per-method form is not handled)
- Proc/Lambda as named symbols
- `method_missing` / `respond_to_missing?` (dynamic dispatch patterns)

**Test coverage:** 5 test files totaling ~2,486 lines covering classes, modules, methods, variables, constants, aliases, attr_accessor, visibility, inheritance, identifiers, and cross-file relationships. Good breadth.

---

#### Lua

**Rating: B**

**What it extracts:** Functions (regular, local, colon-method, dot-method), variables (local and global, with table field detection), table fields, metatable-based class detection (via `setmetatable` pattern), function calls, method calls, member access identifiers.

**Strengths:**
- Clever metatable-based class detection in `classes.rs`: scans for `setmetatable` calls and promotes the target table variable to `SymbolKind::Class` -- this correctly identifies OOP patterns in Lua despite the language having no native class syntax
- Good function variant coverage: regular `function foo()`, local `local function foo()`, method `function Obj:method()`, dot-style `function Obj.staticMethod()`
- Colon-method functions correctly add `self` as implicit first parameter in metadata
- Table field extraction handles nested tables
- Identifier extraction covers function calls, method calls (`:`), and member access (`.`)
- All files under 500 lines (largest is `variables.rs` at 460 lines, close to limit)

**Issues:**
- [P1] `"unknown"` sentinel in type inference: `helpers.rs` lines 55 and 57 return `"unknown"` as default data type when type cannot be determined. Six total occurrences across `variables.rs` (4 places setting `dataType` to `"unknown"`) and `helpers.rs` (2 places).
- [P2] `variables.rs` at 460 lines is close to the 500-line limit and has significant code duplication between `extract_local_variable` and `extract_global_variable` -- the logic for detecting table fields, function values, and building metadata is nearly identical between the two functions.
- [P2] Class detection regex in `classes.rs:28` (`r"setmetatable\(\s*(\w+)\s*,"`) is fragile -- won't match `setmetatable(self, {__index = Base})` with complex second arguments or multiline calls, though it handles the common case.
- [P2] No visibility model -- all symbols are `Visibility::Public`. Lua has no native visibility, but `local` could reasonably map to `Private`.

**Missing extractions:**
- Metatables beyond `setmetatable` (e.g., `getmetatable`, `__index` chain analysis)
- Module patterns (`return M` at end of file)
- `require` calls as Import symbols
- Vararg functions (`...` parameter)
- Coroutine definitions (`coroutine.create`/`coroutine.wrap`)
- Table constructors as named types
- Numeric and generic `for` loop variables

**Test coverage:** 21 test files totaling ~3,385 lines covering functions, variables, tables, metatable classes, method calls, identifiers, relationships, and edge cases. Excellent breadth with many real-world Lua patterns tested.

---

#### Bash

**Rating: B**

**What it extracts:** Functions (POSIX and bash-style), variables (declarations, assignments, exports, readonly, local, environment), commands (source, eval, exec, trap, alias), control flow blocks (if/for/while/case as symbols), positional parameters, command relationships, function signatures with parameter detection.

**Strengths:**
- Good DevOps/scripting focus: tracks `source`, `eval`, `exec`, `trap`, `alias` as significant commands
- Positional parameter detection inside functions (scans for `$1`, `$2`, `${1}`, etc.)
- Variable declaration handling covers `declare`, `local`, `export`, `readonly`, `typeset`
- Environment variable detection via regex pattern matching (ALL_CAPS names)
- Function signatures include detected positional parameters
- Cross-language command tracking (source/eval are important for understanding script relationships)
- All files well under 500 lines (largest is `mod.rs` at 224 lines)
- Zero sentinel values

**Issues:**
- [P1] Control flow blocks (`if`, `for`, `while`, `case`) are extracted as `SymbolKind::Method` with synthetic names like `"for block"`, `"while block"`, `"if block"` (`commands.rs:78-91`). These are not methods -- they should either not be extracted or use a more appropriate kind. This creates noise in search results and misrepresents the code structure.
- [P2] `is_environment_variable` in `variables.rs:114` creates a new `Regex` on every call instead of using `LazyLock`. This function is called for every variable encountered.
- [P2] `relationships.rs` has empty stub methods: `extract_command_substitution_relationships` and `extract_file_relationships` are defined but contain no logic (lines 52-117) -- just comments describing what they should do.
- [P2] `types.rs` type inference is extremely basic (51 lines) -- only checks for integer, array, and associative array patterns.
- [P2] No heredoc content extraction -- heredocs are common in bash scripts for configuration/templates.

**Missing extractions:**
- Array declarations (`declare -a`, `declare -A`)
- Heredoc content and delimiters
- Subshell blocks (`(...)` and `$(...)`)
- Arithmetic expressions (`$((...))`)
- Bash-specific features: `select` loops, `coproc`, process substitution
- Shebang line detection
- `set -e`, `set -o pipefail` and other option settings
- Function `return` value patterns

**Test coverage:** 4 test files totaling ~1,880 lines covering functions, variables, commands, control flow, environment variables, identifiers, and cross-file relationships. Moderate coverage -- could use more edge case tests for complex scripts.

---

#### PowerShell

**Rating: B**

**What it extracts:** Functions (simple and advanced with CmdletBinding), parameters (with types and attributes), classes (with inheritance), class methods, class properties, enums (with members), variables (with scope prefixes: script, global, local, private), Import-Module, Export-ModuleMember, using statements, dot sourcing, DSC configurations, pipelines, well-known DevOps commands, comment-based help, ERROR node recovery.

**Strengths:**
- Most comprehensive scripting language extractor at 11 files and 2,038 total lines
- Advanced function support: detects `[CmdletBinding()]`, extracts `[Parameter()]` attributes, handles `begin/process/end` blocks
- Comment-based help extraction (`.SYNOPSIS`, `.DESCRIPTION`, `.PARAMETER`, `.EXAMPLE`) as doc comments
- DSC configuration extraction (`Configuration`, `Node`, resource blocks)
- Well-known DevOps command tracking: `Invoke-RestMethod`, `Start-Job`, `New-Object`, `Get-WmiObject`, etc.
- ERROR node recovery in `commands.rs` -- attempts to extract useful information from malformed AST nodes
- Scope-aware variable extraction: correctly handles `$script:var`, `$global:var`, `$local:var`, `$private:var`
- Dot sourcing detection with path extraction
- Cross-file relationship support via `PendingRelationship`
- Good identifier extraction covering command calls, method invocations, and member access

**Issues:**
- [P1] `"ModuleMember"` sentinel fallback in `imports.rs:99` when `Export-ModuleMember` parameter type cannot be determined. Should return `None`.
- [P2] `helpers.rs` lines 155-177 create regex patterns on every call without `LazyLock` caching. Multiple regex compilations per node visit for parameter attribute extraction.
- [P2] `types.rs` correctly uses `LazyLock` for its regexes (good), but `helpers.rs` and `imports.rs` do not (inconsistent).
- [P2] Pipeline extraction creates symbols for each pipeline segment, which can be noisy for long pipelines like `Get-Process | Where-Object | Select-Object | Export-Csv`.
- [P2] `extract_import_module_name` and `extract_import_command` have duplicated regex patterns for Import-Module (`imports.rs:63` and `imports.rs:182`).

**Missing extractions:**
- Workflow functions (`workflow { ... }`)
- DSC resource property declarations
- PowerShell classes: interface implementation (no `implements` equivalent in PS, but could detect patterns)
- Splatting variables (`@params`)
- Script blocks as named symbols (`$sb = { ... }`)
- `#Requires` statements
- Module manifest (`.psd1`) key-value pairs
- PowerShell 7 features: ternary operator, null-coalescing, pipeline chain operators

**Test coverage:** 3 test files totaling ~2,093 lines covering functions, classes, enums, variables, commands, pipelines, DSC configurations, comment-based help, identifiers, and cross-file relationships. Reasonable coverage but could benefit from more edge case tests given the extractor's breadth.

---

#### R

**Rating: D**

**What it extracts:** Function definitions (via `<-` and `=` assignment of `function()` expressions), variable assignments (via `<-`, `=`, `<<-`), function call identifiers, pipe operator relationships (`|>` and `%>%`), member access via `$` and `@` operators.

**Strengths:**
- Handles both `<-` and `=` assignment operators for function detection
- Pipe operator (`|>` and `%>%`) relationship tracking -- important for tidyverse-style R code
- Member access via `$` (list/data.frame columns) and `@` (S4 slot access) in identifiers
- Relationship extraction covers function calls, pipe chains, and member access
- All 3 files total only 634 lines -- compact codebase

**Issues:**
- [P0] **No signatures at all**: Every symbol is created with `SymbolOptions::default()`, meaning no signature, no visibility, no metadata, no doc_comment on any symbol. This is the only extractor in the entire codebase that produces symbols with zero metadata.
- [P0] **No `infer_types` method**: The `RExtractor` struct has no type inference capability. All other extractors implement at least basic type inference.
- [P1] **Only `binary_operator` nodes processed** (`mod.rs:50`): The `visit_node` method only matches on `binary_operator` (for `<-`/`=`/`<<-` assignments). All other R constructs are completely ignored -- no `if`, `for`, `while`, `repeat`, `switch`, no expression-level analysis.
- [P1] **No class support**: R has 4 major class systems (S3, S4, R5/Reference Classes, R6) and none are detected. `setClass()`, `setGeneric()`, `setMethod()`, `R6Class$new()` are all missed.
- [P1] **No library/require imports**: `library()` and `require()` calls are not extracted as Import symbols. These are fundamental to understanding R package dependencies.
- [P1] **No roxygen2 documentation**: R's standard documentation system (`#' @param`, `#' @return`, `#' @export`) is completely ignored.
- [P2] Identifier extraction uses line-number matching (`identifiers.rs:108-126`) instead of `find_containing_symbol` from BaseExtractor -- fragile and inconsistent with other extractors.
- [P2] Relationship extraction creates synthetic IDs like `builtin_print`, `piped_filter`, `member_data` (`relationships.rs:95,164,236`) -- these won't match any real symbol IDs and create dead-end references.

**Missing extractions:**
- S3 classes (structure + UseMethod dispatch)
- S4 classes (`setClass`, `setGeneric`, `setMethod`)
- R5/Reference classes (`setRefClass`)
- R6 classes (`R6Class$new()`)
- `library()`/`require()` imports
- `source()` file inclusion
- roxygen2 documentation comments
- Formula objects (`y ~ x`)
- Function signatures (parameter names, defaults)
- Namespace-qualified calls (`dplyr::filter()`)
- Package exports (`@export` roxygen tag)
- `for`/`if`/`while`/`repeat` control flow
- Named list creation (`list(a = 1, b = 2)`)
- Data frame column references in formulas and tidyverse pipelines

**Test coverage:** 14 test files totaling ~3,910 lines. Despite the large test suite, the tests primarily cover function assignment detection and variable extraction -- the two things the extractor actually does. The test count is high relative to the extractor's capabilities, suggesting tests were written to validate the minimal functionality rather than driving development of missing features.

---

### Group 5: Web & UI Languages

#### HTML

**Rating: B**

**What it extracts:** Semantic HTML elements (header, nav, main, footer, section, article, etc.), form elements, media elements, headings, script/style tags, DOCTYPE, custom elements (contain hyphen), elements with id/name attributes, HTML comments (3+ chars), SVG elements.

**Strengths:**
- Excellent noise filtering: generic containers (div, span, p, ul, ol, li, table) are skipped unless they have `id` or `name` attributes, reducing noise by 90-95%
- Custom element detection via hyphen-in-tag-name heuristic is clever and correct
- Thorough attribute handling with per-tag priority attributes (e.g., img gets src/alt/width/height, form gets action/method)
- UTF-8 safe string truncation via `BaseExtractor::truncate_string`
- Robust fallback extraction via regex when tree-sitter parsing fails
- All files well under 500-line limit (max: 240 lines)
- Comprehensive relationship extraction for href/src/action attributes
- File-scoped identifier filtering (critical fix applied correctly)

**Issues:**
- [P2] Comments are extracted as symbols with the literal name `"comment"` and kind `Property` (elements.rs:208-209). Every comment becomes a symbol named "comment". While the signature includes the actual text, the symbol name is useless for search.
- [P2] DOCTYPE is extracted as `SymbolKind::Variable` (elements.rs:164). `Variable` is a somewhat arbitrary choice for a document type declaration.
- [P2] The fallback regex extractor (fallback.rs:78-156) does not apply `should_extract_element` filtering, so it will extract every element including generic containers like `<div>` and `<p>`. This partially defeats the noise filtering when the fallback path is taken.
- [P2] Hardcoded custom element attribute priorities for specific elements like `custom-video-player` and `image-gallery` (attributes.rs:134-136) -- these are examples, not real elements.

**Missing extractions:**
- `<template>` element is not in the semantic element list (relevant for HTML template elements)
- `<slot>` element (Web Components) is not extracted
- `<output>`, `<progress>`, and `<meter>` form elements are not extracted
- No extraction of `data-*` attributes as separate identifiers

**Sentinel residue:** None. The `"comment"` name is a generic placeholder but technically intentional, not a sentinel fallback.

**Test coverage:** Comprehensive -- 9 test files covering structure, forms, media, script/style, edge cases, identifiers, doc comments, and types.

---

#### CSS

**Rating: B**

**What it extracts:** CSS rule sets (selectors), @keyframes rules and individual keyframe blocks, @media queries, @import/@charset/@namespace at-rules, @supports rules, CSS custom properties (--variables), animation names.

**Strengths:**
- Clean modular design with focused files (rules, animations, at_rules, media, properties, helpers, identifiers)
- All files well under 500-line limit (max: 182 lines)
- CSS custom properties (`--var-name`) extracted with proper `SymbolKind::Property` and value in signature
- Good key property extraction for signatures: prioritizes important CSS properties (display, position, etc.) and unique properties (calc, var, gradients)
- Special handling for `:root` selector to include all CSS variables
- Identifier extraction for function calls (calc, var, rgb), class selectors, and ID selectors
- No sentinel values anywhere

**Issues:**
- [P1] Keyframe percentages like `0%`, `50%`, `100%`, `from`, `to` are extracted as individual symbols (animations.rs:82-138) with `SymbolKind::Variable`. These are noise -- having a symbol named "50%" is not useful for code intelligence.
- [P2] Class selectors (`.button`) are mapped to `SymbolKind::Class` (rules.rs:40). CSS class selectors are not classes in the OOP sense.
- [P2] ID selectors (`#header`) and most other selectors are mapped to `SymbolKind::Variable` (rules.rs:42-47). This is a catch-all with no semantic meaning.
- [P2] `@keyframes` animation name is extracted twice: once as `@keyframes fadeIn` (the rule) and once as `fadeIn` (the animation name). Creates duplicate symbols for the same concept.
- [P2] `Regex::new()` is called inline in `extract_keyframes_name` and `extract_supports_condition` (animations.rs:144, properties.rs:96) rather than using static `LazyLock` patterns.

**Missing extractions:**
- No relationship extraction at all (the module has no relationships file)
- No extraction of CSS `@layer` rules (modern CSS)
- No extraction of CSS `@container` queries
- No extraction of CSS `@font-face` rules
- No extraction of CSS `@property` registered custom properties

**Sentinel residue:** None.

**Test coverage:** Excellent -- 13 test files covering basic rules, advanced features, animations, at-rules, custom properties, doc comments, identifiers, media queries, modern CSS, pseudo-elements, responsive design, and utilities.

---

#### Vue

**Rating: B**

**What it extracts:** Vue SFC component name (from `export default { name: ... }` or filename), script section options (data, methods, computed, props), function definitions within script, CSS class names from style section.

**Strengths:**
- Does not use tree-sitter for Vue SFC parsing (correctly uses regex-based section splitting since tree-sitter-html cannot parse `.vue` files)
- Identifier extraction delegates to JavaScript tree-sitter parser for the `<script>` section, with correct TypeScript support when `lang="ts"`
- Correct line offset adjustment for identifiers extracted from script sections
- Template section correctly returns no symbols (component usages in templates are references, not definitions)
- File-scoped identifier filtering applied correctly
- All files well under 500-line limit (max: 245 lines)

**Issues:**
- [P1] Regex-based script parsing (script.rs) only extracts Options API patterns (`data()`, `methods:`, `computed:`, `props:`, and raw function definitions). The Composition API (`setup()`, `ref()`, `computed()`, `watch()`, `onMounted()`, `defineComponent()`, `defineProps()`, `defineEmits()`) is completely unhandled. In modern Vue 3 codebases, this means most meaningful symbols are missed.
- [P1] `"VueComponent"` sentinel fallback in component.rs:26 (`unwrap_or("VueComponent")`) and component.rs:43 (`Some("VueComponent".to_string())`). If the file path has no stem and no script-section name, the component gets the generic sentinel name "VueComponent".
- [P2] Style section only extracts class selectors via regex (`.className {`), missing ID selectors, keyframes, custom properties, and nested selectors (SCSS/Less when `lang="scss"`)
- [P2] The `create_identifier_with_offset` function (identifiers.rs:185-226) re-parses the entire Vue SFC with `parse_vue_sfc` on every identifier creation. It also does a `std::mem::take` content swap trick that is fragile.
- [P2] No extraction from `<script setup>` syntax (Vue 3's recommended pattern). The `setup` attribute is not checked.

**Missing extractions:**
- Vue 3 Composition API: `ref()`, `reactive()`, `computed()`, `watch()`, `defineProps()`, `defineEmits()`, `defineExpose()`
- `<script setup>` variable declarations (top-level bindings auto-exposed)
- Template `v-slot` definitions, template refs (`ref="..."`)
- Emits declarations, watchers (`watch:` option)
- Lifecycle hooks (`created`, `mounted`, `beforeDestroy`, etc.)
- Mixins and plugin usage

**Sentinel residue:** `"VueComponent"` in component.rs:26,43 -- used as fallback component name when filename cannot be determined.

**Test coverage:** Moderate -- 3 test files (mod.rs at 31k is substantial, plus parsing.rs and types.rs). Tests cover Options API well but likely do not cover Composition API or `<script setup>`.

---

#### Razor

**Rating: B**

**What it extracts:** Razor directives (@page, @model, @using, @inject, @inherits, @implements, @namespace, @attribute, @addTagHelper), code blocks (@code, @functions, @{...}), Razor expressions, sections (@section), C# symbols within code blocks (classes, methods, properties, fields, local functions, local variables, variable declarations, assignments, invocations, element access), using directives, namespace/class declarations.

**Strengths:**
- Very comprehensive C# code block extraction -- handles method parameters, return types, modifiers, attributes, visibility determination, explicit interface implementations, accessor lists (get/set)
- Relationship extraction covers component usage, data bindings (@bind-Value), event bindings (@onclick), using directive relationships, and invocation relationships with deduplication
- ERROR node fallback: when tree-sitter produces ERROR nodes, falls back to regex-based extraction for @inherits and @rendermode directives
- Template component references correctly skipped (not definitions)
- Identifier extraction handles invocation_expression and member_access_expression with proper call/member-access distinction
- Good metadata tagging (ViewData, ViewBag, Layout detection, Component.InvokeAsync, Html.Raw, RenderSectionAsync, RenderBody)

**Issues:**
- [P1] `stubs.rs` is 606 lines, exceeding the 500-line limit. `relationships.rs` is 554 lines, also exceeding the limit. These files should be refactored into smaller modules.
- [P1] `"expression"` sentinel fallback in directives.rs:324: `extract_variable_from_expression` falls back to `"expression".to_string()` when regex fails. This creates a symbol with the generic name "expression" which is meaningless for search.
- [P1] `"unknown"` sentinel in type_inference.rs:23: `let mut inferred_type = "unknown".to_string()` is the initial value for all inferred types. If no type information is found from metadata or signature regex, the type stays "unknown".
- [P2] Invocation expressions are extracted as symbols (stubs.rs:460-536) with `SymbolKind::Function`. Method calls like `Html.Raw(...)` or `RenderBody()` are usages/references, not definitions. Extracting them as symbols creates noise.
- [P2] Assignment expressions are extracted as symbols (stubs.rs:387-458). `ViewData["Title"] = "Home"` creates a Variable symbol named "ViewData", which is a usage, not a definition.
- [P2] Element access expressions like `ViewData["Title"]` are extracted as symbols (stubs.rs:538-606), which are again usages, not definitions.
- [P2] `content[..content.len().min(200)]` in directives.rs:292 is not UTF-8 boundary safe -- it could panic on multi-byte characters. The `BaseExtractor::truncate_string` helper is used elsewhere but not here.

**Missing extractions:**
- No enum extraction within @code blocks
- No interface extraction within @code blocks
- No struct/record extraction
- No event handler definitions (delegate-based)
- No Blazor `@typeparam` directive
- No `@preservewhitespace` directive

**Sentinel residue:** `"expression"` in directives.rs:324, `"unknown"` in type_inference.rs:23.

**Test coverage:** Substantial -- mod.rs at 72k lines is very thorough, plus relationships.rs and types.rs test files.

---

#### QML

**Rating: B**

**What it extracts:** Root QML object definitions (the file's component base type), QML properties (property declarations), QML signals, JavaScript function declarations.

**Strengths:**
- Clean, focused design: only 3 source files (mod.rs, identifiers.rs, relationships.rs), all under 500 lines
- Smart root-vs-nested object distinction: only the root `ui_object_definition` is extracted as a Class (the component definition), nested objects are correctly treated as instantiations (usages), not definitions
- Comprehensive relationship extraction: call relationships, component instantiation relationships (with `RelationshipKind::Instantiates`), and property binding relationships
- Pending relationship support for cross-file function call resolution
- Identifier extraction covers call_expression, member_expression, and standalone identifiers with proper parent filtering
- Signal handler detection (`ui_script_binding`, `ui_binding`) correctly routes to containing component for relationship resolution
- No sentinel values

**Issues:**
- [P2] Nested QML objects (e.g., `Rectangle {}` inside `Window {}`) produce no symbols at all since only root objects get Class symbols. This means nested custom components like `MyCustomWidget {}` inside a layout are invisible to code intelligence. The decision is architecturally sound (they are usages, not definitions), but it means relationship extraction for instantiation (relationships.rs:80-129) will often find no matching symbol.
- [P2] `find_containing_symbol_id` in identifiers.rs:147-168 uses a manual line-number-matching approach instead of the standard `base.find_containing_symbol()` pattern used by all other extractors. It searches all symbols by line match rather than by byte position containment, which could produce incorrect results for multi-line symbols.
- [P2] `match symbol_map.get(...) { None => {...} _ => {} }` in mod.rs:166-183 -- the catch-all `_ => {}` arm silently ignores the `Some` case. Cleaner pattern: `if symbol_map.get(...).is_none() { ... }`.

**Missing extractions:**
- QML `id:` property declarations (e.g., `id: myButton`) -- critical for QML referencing
- QML `enum` declarations (available in Qt 5.10+)
- QML `alias` property declarations (`property alias text: label.text`)
- QML `required` property declarations
- QML `Connections` blocks
- QML `Component` inline definitions
- QML `ListModel` / `ListElement` definitions
- Nested object definitions are not extracted at all
- No type inference

**Sentinel residue:** None.

**Test coverage:** Excellent -- 13 test files covering basics, animations, bindings, components, cross-file relationships, functions, identifiers, layouts, modern QML, real-world scenarios, relationships, and signals.

---

#### GDScript

**Rating: A**

**What it extracts:** Classes (explicit `class_name` and inner `class` definitions), implicit classes from `extends` statements (named after filename), functions and methods (with lifecycle detection), constructors (`_init`), variables with `@export`/`@onready` annotations, constants, enums and enum members, signals.

**Strengths:**
- Thorough and well-designed extraction covering all major GDScript constructs
- Smart class context tracking: `current_class_context` and `determine_effective_parent_id` correctly handle `class_name` classes, inner classes (indentation-based scoping), and implicit classes (lifecycle callbacks and setget functions become methods)
- Proper annotation handling: `extract_variable_annotations` collects `@export`, `@onready`, and other annotations from both children and sibling positions, including them in signatures
- Position-based deduplication (`processed_positions`) prevents double-extraction when `var`/`func` keywords appear in both standalone and statement-wrapped positions
- Inheritance information collection in a separate first pass before symbol extraction
- Type inference from both explicit annotations (`var x: int`) and implicit assignment (`var x = 42` infers `int`)
- GDScript-specific type inference: recognizes Godot types like Vector2, Vector3, Color, etc.
- Signal extraction with `SymbolKind::Event` (correct for GDScript signals)
- Constants extracted with `SymbolKind::Constant` (not Variable)
- Visibility determined by naming convention (underscore prefix = private)
- Pending relationship support for cross-file call resolution
- All files under 500-line limit (max: 369 lines in mod.rs)
- No sentinel values in symbol names

**Issues:**
- [P2] `"unknown"` appears in type inference (types.rs:67,80,82) as a fallback when type cannot be inferred from expression. Since this is type inference output (not a symbol name), it is an acceptable semantic value meaning "type unknown".
- [P2] `"ImplicitClass"` fallback in mod.rs:75 (`unwrap_or("ImplicitClass")`) -- used if file_path has no segments, which should be extremely rare. Practically unreachable.
- [P2] Enum member extraction (enums.rs:63) skips lowercase-starting identifiers, which could miss enum members that start with lowercase (valid but unconventional GDScript).

**Missing extractions:**
- No extraction of `@tool` annotation (marks a script for editor execution)
- No extraction of `@icon` annotation
- No extraction of `setget` property getters/setters as separate symbols
- No extraction of `preload()` / `load()` as import-like symbols

**Sentinel residue:** `"unknown"` in types.rs (type inference only, acceptable), `"ImplicitClass"` in mod.rs:75 (practically unreachable).

**Test coverage:** Excellent -- 12 test files covering classes, functions, signals, types, cross-file relationships, identifiers, modern GDScript patterns, resources, scenes, UI scripts, and comprehensive patterns.

---

### Group 6: Data & Specialized

#### SQL

**Rating: B**

**What it extracts:** Tables (Class), columns (Field), views (Interface), indexes (Property), triggers (Method), stored procedures (Method), functions (Function), CTEs (Interface), schemas (Namespace), sequences (Variable), domains (Class), custom types/enums (Class), constraints (Interface/Property), SELECT aliases (Field), DECLARE variables (Variable), ALTER TABLE constraints (Property), aggregate functions (Function)

**Strengths:**
- Exceptionally comprehensive SQL coverage -- handles tables, views, indexes, triggers, procedures, functions, CTEs, schemas, sequences, domains, and custom types
- Robust ERROR node recovery (`error_handling.rs`) extracts symbols from 9 different SQL construct types that tree-sitter-sql fails to parse, including procedures, functions, views, triggers, constraints, domains, types, and aggregates
- Well-structured modular decomposition: 7 files with clear separation of concerns
- Detailed signatures including column counts for tables, USING/INCLUDE/WHERE clauses for indexes, parameter lists for procedures, and return types for functions
- Window function handling in SELECT aliases preserves the OVER clause in signatures
- Foreign key relationship extraction with proper source/target table resolution and confidence scoring
- Static lazy regex patterns in helpers.rs compiled once for performance
- Comprehensive test suite: 12 test files (~2,262 lines)

**Issues:**
- [P1] `mod.rs` at 662 lines exceeds the 500-line limit. The `extract_select_aliases`, `extract_view_columns_from_error_node`, and `extract_identifier_from_node` methods could be extracted to submodules
- [P1] `error_handling.rs` at 503 lines is right at the limit. Contains significant code duplication with `constraints.rs` for ALTER TABLE constraint extraction (nearly identical regex patterns)
- [P2] Sentinel "unknown" at `schemas.rs:66` -- table name fallback when name_node is None
- [P2] Sentinel "unknown" at `constraints.rs:120` -- data type fallback when type node not found for column
- [P2] Sentinel "unknown" at `constraints.rs:153` -- constraint_type defaults to "unknown"
- [P2] Sentinel "unknown" at `routines.rs:175` -- variable type fallback
- [P2] `mod.rs:459-471` has a hardcoded skip list of table alias names ("u", "ae", "users", "analytics_events", "id", "username", "email") that is test-data-specific
- [P2] Regex objects recompiled on every call in `constraints.rs` instead of using `LazyLock`
- [P2] `extract_table_references` at `relationships.rs:155-175` finds table references but does nothing with them (assigns to `_table_symbol`)

**Missing extractions:**
- DROP TABLE/VIEW/INDEX statements
- INSERT INTO table references as identifiers
- Materialized views
- GRANT/REVOKE permission statements

**Test coverage:** Excellent -- 12 test files covering DDL, DML, procedures, indexes, schemas, relationships, doc comments, identifiers, security, transactions, and types.

---

#### Dart

**Rating: C**

**What it extracts:** Classes (Class), functions (Function), methods (Method), constructors (Constructor), getters/setters (Property), fields (Field), enums (Enum), enum constants (EnumMember), mixins (Interface), extensions (Module), type aliases (Class), variables/constants (Variable/Constant), Flutter widget detection

**Strengths:**
- Good coverage of Dart-specific constructs including mixins, extensions, factory/const/named constructors, and typedefs
- Flutter-aware: detects StatelessWidget/StatefulWidget subclasses and lifecycle methods
- Proper Dart visibility handling using `_` prefix convention for private members
- Rich method signatures with modifiers (static, async, @override) and return types with generics
- Cross-file relationship support via PendingRelationship mechanism
- Good class signature extraction including abstract, generics, extends, with, and implements
- Comprehensive test suite (~2,479 lines)

**Issues:**
- [P0] Hardcoded test values in ERROR node handler at `mod.rs:176-180` -- extraction only triggers if error text contains "green", "blue", "const ", "Color", or "Blue". Enum constants from ERROR nodes will NEVER be extracted for any other enum.
- [P0] `extract_enum_constants_from_error` at `mod.rs:496` only extracts identifiers named "green", "blue", or "Color" -- completely non-generic
- [P0] `extract_enum_constants_from_text` at `mod.rs:523-582` has hardcoded patterns like `"blue('Blue')"`, `"Blue')"`, `"const Color"` -- entirely test-data-specific
- [P1] `mod.rs` at 706 lines significantly exceeds the 500-line limit
- [P1] Doc comment claims "Imports and library dependencies" support but zero import extraction implemented
- [P1] Thread-local content cache (`helpers.rs:23-45`) is a fragile borrow-checker workaround
- [P2] `extract_enum_constants_from_error_recursive` extracts ANY identifier from ERROR nodes as EnumMember -- extremely noisy
- [P2] No sealed/base/final/interface class modifiers (Dart 3.0)
- [P2] Typedef uses `SymbolKind::Class` instead of `SymbolKind::Type`

**Missing extractions:**
- Import/export statements (import, export, part, part of, library)
- Extension types (Dart 3.3)
- Sealed/base/final/interface class modifiers (Dart 3.0)
- Pattern matching declarations
- Record types

**Test coverage:** Good breadth but ERROR handling tests are tautologically tied to hardcoded values, giving false confidence. Main test file at 2,046 lines exceeds 1,000-line guideline.

---

#### Regex

**Rating: B**

**What it extracts:** Patterns (Variable/Function/Class/Constant/Method), character classes (Class), groups including named/non-capturing (Class), quantifiers (Function), anchors (Constant), lookaheads/lookbehinds (Method), alternations (Variable), predefined character classes (Constant), unicode properties (Constant), backreferences (Variable), conditionals (Method), literals (Variable), generic patterns

**Strengths:**
- Comprehensive regex construct coverage: groups, quantifiers, anchors, lookaround assertions, alternations, character classes, unicode properties, backreferences, and conditionals
- Dual extraction: tree-sitter parsing AND text-based pattern extraction
- Good metadata: each symbol includes type, pattern text, and computed complexity score
- Named group extraction for both `(?<name>...)` and `(?P<name>...)` syntax
- Backreference resolution (numeric `\1` and named `\k<name>`)
- UTF-8 boundary safety checks throughout
- Clean dead code removal with documented reasoning
- All files well under 500-line limit (largest: patterns.rs at 473 lines)

**Issues:**
- [P1] Very noisy: extracts individual literal characters, anchors, and every quantified expression as separate symbols. A simple regex produces 10+ symbols, flooding the index.
- [P2] Sentinel "unknown" at `flags.rs:11` -- anchor type fallback
- [P2] Sentinel "unknown" at `flags.rs:76` -- unicode property name fallback
- [P2] Sentinel "unknown" at `flags.rs:128` -- condition text fallback
- [P2] SymbolKind mapping is unconventional (groups as Class, quantifiers as Function, etc.)
- [P2] `is_valid_regex_pattern` is too permissive -- any alphanumeric string passes
- [P2] Duplicate `extract_group_name` in both `groups.rs` and `identifiers.rs`

**Missing extractions:**
- Atomic groups (`(?>...)`) -- tree-sitter limitation
- Inline comments (`(?#...)`) -- tree-sitter limitation
- Subroutine references, mode modifiers

**Test coverage:** Good -- 10 test files, ~1,216 lines.

---

#### JSON

**Rating: A**

**What it extracts:** Key-value pairs from JSON objects. Object/array values as Module (containers), primitive values as Variable. String values captured as doc_comment for semantic search.

**Strengths:**
- Clean, focused 134-line implementation
- Recursive tree walking with proper parent_id tracking
- String values stored as doc_comment enabling semantic search over config values
- Long string truncation at 2000 characters
- No sentinel values, no hardcoded fallbacks
- Proper quote stripping from key names

**Issues:**
- None significant

**Missing extractions:**
- No signature generation (could show value type)
- Array elements not individually extractable

**Test coverage:** Comprehensive -- 1,266 lines of tests.

---

#### TOML

**Rating: B**

**What it extracts:** Table headers (`[table]` and `[[array_table]]`) as Module symbols. Only table-level structure, not individual key-value pairs.

**Strengths:**
- Clean, focused 132-line implementation
- Handles regular tables and array tables
- Supports bare keys, quoted keys, and dotted keys
- No sentinel values

**Issues:**
- [P1] Does not extract individual key-value pairs -- only table headers. Config keys like `name = "julie"` are invisible to the symbol index. The JSON extractor handles pairs; TOML should too for consistency.
- [P2] No differentiation between regular and array tables (the `_is_array` parameter is unused at line 72)
- [P2] Dotted keys like `[parent.child]` extracted as flat name without parent-child relationships

**Missing extractions:**
- Individual key-value pairs
- Inline tables
- Array of tables relationship tracking
- Value type metadata

**Test coverage:** Good -- 845 lines.

---

#### YAML

**Rating: B**

**What it extracts:** Documents (Module), block mapping pairs as keys (Variable), flow mappings (Module).

**Strengths:**
- Handles block mapping pairs correctly
- Key extraction supports plain, single-quoted, and double-quoted scalars
- Recursive tree walking with parent_id tracking
- Clean 196-line implementation

**Issues:**
- [P1] Synthetic names: "document" (line 100) and "flow_mapping" (line 152) are generic -- every YAML file has a "document" symbol; every inline mapping is "flow_mapping". These are noise.
- [P1] No anchor (`&name`) or alias (`*name`) support -- core YAML features used in CI/CD and Kubernetes configs
- [P2] Sequences not extracted despite being mentioned in module doc comment
- [P2] All mapping pairs are Variable regardless of value type (JSON differentiates containers vs primitives)
- [P2] Multi-document YAML each gets generic "document" name

**Missing extractions:**
- YAML anchors and aliases (`&name` / `*name`)
- Merge keys (`<<: *alias`)
- Sequences/arrays
- Flow sequences, tags, comments

**Test coverage:** Adequate -- 394 lines. Thinnest test coverage in the group.

---

#### Markdown

**Rating: A**

**What it extracts:** Sections/headings (Module), frontmatter metadata (Property). Section content (paragraphs, lists, code blocks, tables, block quotes) captured as doc_comment for RAG embedding.

**Strengths:**
- Excellent RAG design: heading text as name, full section body as doc_comment for semantic search
- Frontmatter extraction for both YAML (`---`) and TOML (`+++`) delimiters
- Body content after frontmatter but before first heading is captured -- critical for memory checkpoint files
- `is_content_node` comprehensively covers paragraphs, lists, code blocks, block quotes, tables, thematic breaks, HTML blocks
- Clean heading text extraction
- No sentinel values
- Clean 340-line file

**Issues:**
- [P2] Heading level computed but not stored in metadata or signature
- [P2] No hierarchical parent-child relationships between headings based on level
- [P2] Link extraction not implemented

**Missing extractions:**
- Heading level metadata
- Links as identifiers/references
- Image references
- Code block language tags
- Footnotes

**Test coverage:** Comprehensive -- 1,168 lines.
