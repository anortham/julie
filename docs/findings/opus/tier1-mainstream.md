# Tier 1 Extractor Audit — Mainstream OO/Imperative

## Summary

The Tier 1 extractors (Rust, TypeScript, JavaScript, Python, Java, C#, Go, C++, C) are functional and cover the major shapes of each language, but several systemic gaps prevent them from feeling "world class." The biggest cross-cutting issues are: (1) inconsistent identifier coverage — only TypeScript, Java, C++, and C extract `IdentifierKind::TypeUsage`, leaving Rust, JavaScript, Python, C#, and Go without type-reference tracking; (2) incomplete multi-declarator handling — Java, C#, and C++ silently drop trailing names in `int a, b, c;`-style declarations; (3) constructor calls (`new T()`) are missing from the identifier stream in Java, JavaScript, and C#, even though they're the most common form of "Calls" between modules in those ecosystems; (4) several extractors (TypeScript class extraction, C++ class/struct types, Python lambdas, Java records) hardcode visibility or kind in ways that lose real signal; (5) parent_id propagation has subtle bugs across the suite — TypeScript top-level passes `None` and reconstructs IDs by scanning, Python reconstructs by row-recomputation, Go does it via walk_tree but several class-types use a different path. The Go, C, and C++ extractors have the strongest type/relationship coverage; TypeScript has the strongest identifier coverage but the weakest parent_id story; Python has the most missing language features (no type aliases, no match patterns, no `TypeAlias`).

## Per-Language Findings

### Rust
**Status**: Good

**Strengths**:
- Two-phase impl block processing is sound; methods correctly link to their parent struct/enum/trait through `impl_type_name` metadata, and cross-file impls degrade gracefully via `impl_parent_id_resolved` flag (functions.rs:208-216).
- Excellent doc-comment handling: scans `///`, `//!`, `/** */`, *and* `#[doc = "..."]` attributes, with attribute siblings skipped (helpers.rs:207-317).
- Item-position macro filter via `NOISE_MACROS` constant is solid (signatures.rs:126-167); avoids polluting symbols with `vec!`, `format!`, `println!`, etc.
- Test detection wired through `is_test_symbol` with annotation keys (functions.rs:118).
- Trait-implementation relationships built from `extract_impl_target_names` correctly handle `for` token splitting (helpers.rs:41-74).

**Gaps & Errors**:
- `rust/identifiers.rs:115` admits the limitation: "We're conservative — only extract clear variable usages, not all identifiers." `IdentifierKind::VariableRef` is never produced. Bare `let x = foo;` reads of `foo` produce no identifier, breaking find-references for variables and constants.
- `lifetime` parameters on functions/types aren't extracted as part of the signature distinct from `type_parameters`; they ride along inside the `type_parameters` text but aren't separable.
- `where` clauses are appended verbatim including embedded whitespace from the source (functions.rs:74-78), which can produce ugly multi-line signatures.
- `extract_struct` (types.rs:13-61) doesn't extract trait bounds on type parameters (`struct Foo<T: Bar>`); they're only inside the `type_params` text.
- `extract_macro_invocation` only filters expression-position macros via `parent_kind != "source_file" && parent_kind != "declaration_list"`. This means macros inside `extern` blocks may be miscategorized; the filter should include `extern_block` and friends.
- Unused regex `VAR_TYPE_RE` in `mod.rs:21` matches `: Type` but `infer_types` uses it on signatures that already have `pub` prefix, which can mismatch on `pub fn f() -> i32`.

### TypeScript
**Status**: Needs Work

**Strengths**:
- Module is decomposed cleanly into `classes/functions/interfaces/imports_exports/identifiers`. Scaffolding is sound.
- Identifier extraction picks up `type_identifier` references with smart filtering of declaration sites and TS noise types (`Record`, `Partial`, single-letter generics) — see `identifiers.rs:124-235`. This is the most thorough identifier extractor in the suite.
- Decorator handling supports both child decorators (classes) and preceding-sibling decorators (methods) via `extract_decorator_names` and `extract_preceding_decorator_names` (helpers.rs:22-67).
- Pending relationship resolution handles imports and constructor-receiver context (`find_receiver_import_context`, mod.rs:265-320).

**Gaps & Errors**:
- **`extract_class` always passes `parent_id: None`** (classes.rs:107). Classes nested inside namespaces, modules, or other classes will not have `parent_id` set; the visitor in `symbols.rs:visit_node` does not pass parent_id to the class extractor either. Methods recover their parent_id by scanning ancestors via `find_parent_class_id` — but that only walks `class_declaration` parents, not `interface_declaration` parents (functions.rs:316-354), so methods of nested interfaces have no parent. This breaks `parent_id` chains for any class or method that is not at the top level.
- The same is true for `extract_function` (functions.rs:85), `extract_method` (functions.rs:212 — has it), `extract_property`, `extract_namespace`, `extract_enum`, `extract_type_alias` — none of these accept or propagate parent_id; only `extract_method` reconstructs a parent_class via tree walk.
- After symbol creation, both `extract_function` and `extract_method` regenerate the symbol ID by recomputing it from the name node's position and mutating `symbol_map` (functions.rs:91-128). This duplicates IDs already inserted, leaks a stale entry if the lookup fails, and is brittle. The fix is to pass the right node to `create_symbol` in the first place.
- `enum_member` extraction (interfaces.rs:139-167) treats `property_identifier` as a node kind to walk, but the fallback for getting the name (interfaces.rs:140-149) returns `child` itself when `child.kind() == "property_identifier"`, then loops through `c.kind() == "property_identifier" || c.kind() == "identifier"` — confused logic that may double-insert the same name.
- No detection of `abstract` methods specifically — `is_abstract` is only set on classes (classes.rs:73-74).
- `extract_property` does not detect `?` (optional) markers or `!` (definitely-assigned) modifiers in TypeScript.
- No `JSX_self_closing_element` / `JSX_element` extraction. Components are only extracted if declared as `function ComponentName()` or `const ComponentName = () =>`. Dedicated JSX/TSX component detection is absent.

### JavaScript
**Status**: Good

**Strengths**:
- `visit_node` correctly threads `parent_id` through recursion (mod.rs:413-519), unlike TypeScript.
- Handles arrow functions in many contexts: `variable_declarator`, `assignment_expression`, `pair`, plus regular declarations (functions.rs:23-39).
- Destructuring patterns produce multiple symbols via `extract_destructuring_variables` (mod.rs:441-444).
- JSDoc-based type inference for `@returns`/`@type` (mod.rs:380-410).
- Visibility convention: `#name` → Private, `_name` → Protected, otherwise Public (visibility.rs:11-29). Reasonable JS idiom.

**Gaps & Errors**:
- Same identifier limitation as Rust: no `TypeUsage`, no `VariableRef`. JS doesn't have native types, but JSDoc types `{Foo}` aren't parsed for cross-references.
- `extract_method` builds `isStatic` metadata by scanning children for `c.kind() == "static"` (functions.rs:131-133), but tree-sitter-javascript usually puts `static` in the modifier slot. This needs verification against the actual grammar.
- Member access skipping in identifiers.rs:88-97 only checks if the immediate parent is `call_expression`; chained `a.b.c.d()` may double-extract intermediate property accesses as both `MemberAccess` and `Call`.
- `extract_export` returns `SymbolKind::Export` but the `signature` is the *entire* node text (types.rs:157), which can balloon if the export is `export class X { ... entire body ... }`.
- The JSDoc regex in mod.rs:390 uses `regex::Regex::new(...).ok()?` for every type lookup — compiles a regex per call, per symbol. Should be a `LazyLock`.

### Python
**Status**: Significant Gaps

**Strengths**:
- Docstring extraction works correctly via `extract_docstring` looking for the first string literal in the body (types.rs:116-147). Strips triple quotes properly via `strip_string_delimiters` (helpers.rs:77-89).
- `class_definition` correctly classifies `Enum` subclasses as `SymbolKind::Enum` and `Protocol` as `SymbolKind::Interface` (types.rs:36-87).
- Decorator extraction handles both `@property`/`@staticmethod` and parameterized decorators like `@lru_cache(maxsize=128)` (decorators.rs:49-63).

**Gaps & Errors**:
- **`PEP 695` type aliases (`type X = ...`) are not extracted.** No grammar node `type_alias_statement` is matched. Searching for `type_alias` in `crates/julie-extractors/src/python/` returns no hits. Modern Python type aliases are silently dropped.
- **`match` statements / structural pattern matching are ignored.** No `match_statement` or `case_clause` handling. Names bound by `case Point(x=x, y=y):` produce no symbols and no identifiers.
- **Dataclass/`__init_subclass__` semantics are not surfaced.** `@dataclass` is recognized as a decorator string but doesn't add `init`-position fields as Properties.
- **`PythonExtractor::traverse_tree` does not pass parent_id during recursion** (mod.rs:62-100). Methods rely on `determine_function_kind` / `find_parent_class_id` to walk back up the AST and recompute the parent class ID via `generate_id(class_name, row+1, column)` (functions.rs:170-179). This works for direct class methods, but:
  - **Nested function parents are lost.** A function defined inside another function gets `parent_id = None` instead of the enclosing function's ID.
  - The reconstruction is fragile: if `BaseExtractor::create_symbol` ever changes how it computes the ID (e.g., the start_byte vs start_line vs start_column input), this code will silently desync from the actual class symbol's ID.
- `extract_lambda` (functions.rs:111-151) generates the name `lambda_{row}` and always passes `parent_id: None`. Multiple lambdas on the same row (rare but possible) collide. They're also not nested under their containing function.
- `extract_async_function` (functions.rs:104-108) is a thin wrapper that just calls `extract_function`. The signature relies on `signatures::has_async_keyword(&node)` — but for `async_function_definition` nodes, the keyword location is different than for regular `function_definition`. Verify this works.
- Python identifiers don't extract `IdentifierKind::TypeUsage`. Reference annotations like `def foo(x: MyClass)` produce no identifier for `MyClass` (identifiers.rs:121-125 has the explicit "Skip other node types for now" comment).
- `extract_imports` treats `from x import a, b, c` as multiple symbols correctly, but `import x.y.z` only produces a single symbol with name `x.y.z` — the dotted path becomes the symbol name verbatim. References to `x` or `x.y` aren't tracked.
- `helpers::find_parent_class_id` (helpers.rs:8-34) walks the chain of class_definition parents but uses the *innermost* one, which is correct for nesting but the comment claims "nearest enclosing." OK, that's the same thing — minor.
- Python has no equivalent to `is_test_symbol` for module-level testing markers (e.g., `pytest.fixture`); but it does pass annotation_keys to `is_test_symbol` (functions.rs:77-86), so framework-level `@pytest.fixture` would work.

### Java
**Status**: Good

**Strengths**:
- Records (Java 14+) are recognized via `record_declaration` → `extract_record` (mod.rs:102, classes.rs:216-281).
- Sealed-class `permits` clause is appended to the signature (classes.rs:50-58).
- `walk_tree` correctly threads `parent_id` (mod.rs:73-88).
- Inheritance and call relationships are extracted with correct `Implements`/`Extends`/`Calls` distinction in `relationships.rs`. `object_creation_expression` produces `Instantiates` relationships (relationships.rs:175-183).
- `dedupe_relationships` (mod.rs:171-174) protects against duplicate relationship IDs.

**Gaps & Errors**:
- **Multi-declarator field declarations only extract the first variable** (fields.rs:41-46): `int a, b, c;` only produces a symbol for `a`. The comment explicitly says "For now, handle the first declarator (we could extend to handle multiple)." This is a real bug; the others are silently dropped.
- **Java records produce `SymbolKind::Class`** (classes.rs:280) but should be `SymbolKind::Struct` or a dedicated kind. The record components (the parameters) are *not* extracted as `Property` or `Field` symbols of the record — so `record Point(int x, int y) {}` produces only one symbol named `Point`.
- `extract_class` doesn't read or propagate annotations (classes.rs:11-76 — no call to `helpers::extract_annotations`). `@SpringBootApplication` etc. on classes are lost from the `annotations` field, even though `extract_method` does extract them (methods.rs:22).
- `extract_interface` and `extract_enum` similarly miss annotations.
- Java identifier extraction (identifiers.rs) handles `method_invocation`, `field_access`, and `type_identifier` references correctly, but **does NOT extract `object_creation_expression`** as a `Call` identifier. `new Foo()` is captured for relationships but not as an identifier — so `fast_refs` for constructor calls won't work via `IdentifierKind::Call`.
- Anonymous classes (`new Runnable() { @Override public void run() { ... } }`) are not extracted as a separate Class symbol; their methods are also not extracted (the walker doesn't descend into `object_creation_expression` looking for class bodies).
- Lambda expressions (`x -> x * 2`) produce no symbol. They're inert in the index.
- `extract_field` (fields.rs:32) defaults `field_type` to `"Object"` if no type is found, which is misleading for primitive defaults.
- `extract_method` (methods.rs:46-48) defaults return type to `"void"` even for `var`-style auto-typed locals. For Java this is fine because methods always declare returns, but worth noting.

### C#
**Status**: Needs Work

**Strengths**:
- Comprehensive coverage of C# constructs: namespaces, classes, interfaces, structs, enums, records, methods, constructors, destructors, properties, fields, events, delegates, operators, conversion operators, indexers (mod.rs:160-188).
- Modern C# features: `record_declaration`, top-level statements via `ensure_file_scope_symbol` synthesizing a Module symbol (mod.rs:46-86), nullable types, attribute extraction.
- `walk_tree` threads parent_id properly (mod.rs:133-152).
- `extract_destructor` handles `~ClassName` (members.rs:163-211) and gives it `Visibility::Protected` per CLR semantics.
- Operators, conversion operators, and indexers are kept distinct via `SymbolKind::Operator` (operators.rs).

**Gaps & Errors**:
- **Multi-declarator field declarations only extract the first variable** (members.rs:315): same bug as Java. `int a, b, c;` drops `b` and `c`. `event_field_declaration` has the same bug (members.rs:386-390).
- **No extraction of `local_function_statement`** — local functions inside methods produce no symbol. Common in modern C# code.
- **No extraction of `lambda_expression` or `anonymous_method_expression`.**
- **No extraction of `partial` declaration awareness.** A `partial class Foo` and another `partial class Foo` in a different file create two separate symbols with no link. The `partial` modifier is captured in the `modifiers` list but not surfaced.
- **Properties don't decompose into get/set accessors.** A property `public int X { get; private set; }` gets a single `Property` symbol with the accessor block in the signature — consumers can't distinguish a getter call from a setter call. C# uses different visibility per accessor; that's lost.
- **No `using static` / `global using` distinction.** `extract_using` (types.rs:37-90) tracks `is_static` but doesn't emit it differently. `global using` (C# 10+) isn't surfaced specially.
- **C# identifier extraction is incomplete** (identifiers.rs:30-85): no `TypeUsage` extraction. Type references in parameters, return types, generic args, and casts produce no identifier — meaning `fast_refs` for class names doesn't include their usage sites.
- **`object_creation_expression` is not extracted as a `Call`**. `new HttpClient()` produces no identifier. Same as Java.
- `extract_method` finds the name by reverse-iterating from the parameter_list looking for the last `identifier` (members.rs:18-21). For methods returning a generic type like `Task<MyType> Foo()`, `MyType` is also an `identifier` and could be confused — but the position-based filter mostly handles this. Still, the logic is fragile.
- `extract_property` has a fallback that walks all children with bool flags `found_type` (members.rs:227-247) — this is a code smell. Fragile if the grammar changes.
- `member_type_relationships.rs` is 384 lines, larger than the project's 500-line target but OK; `members.rs` is 496 lines, right at the edge.

### Go
**Status**: Good

**Strengths**:
- Generics (Go 1.18+) are extracted via `type_parameter_list` in functions.rs:114-115 and types.rs.
- Multi-name field declarations handled correctly: `field_declaration` produces multiple Field symbols (mod.rs:169-174).
- `var_declaration` and `const_declaration` produce multiple symbols via dedicated extractors (mod.rs:161-168).
- ERROR-node recovery via `extract_from_error_node` and `recover_function_symbols_from_source` (mod.rs:206, 61).
- `prioritize_functions_over_fields` (mod.rs:120-151) is a thoughtful tiebreaker for name collisions.
- Methods with receivers are extracted with the receiver type captured in metadata.
- Doc-comment lookup falls back to parent `type_declaration` if not found on `type_spec` (types.rs:60-70).

**Gaps & Errors**:
- **Go structs use `SymbolKind::Class`** (types.rs:131), not `SymbolKind::Struct`. Inconsistent with Rust/C/C++ where struct kind is preserved. Type info loss.
- **No identifier `TypeUsage` extraction** (identifiers.rs:97-100 explicit "Skip other node types for now"). Type references in field types, parameter types, return types are not in the identifier index.
- **Embedded types in struct fields are not specially marked.** `struct { io.Reader }` adds `Reader` as a field but the embedding semantics (promoted methods, interface satisfaction) are not flagged in metadata.
- **Channel types and goroutine launches are not surfaced.** `go func() { ... }()` produces no relationship. `chan int` types in field positions are just text.
- `prioritize_functions_over_fields` reorders symbols, which can be surprising for downstream code that expects extraction order to match source order. Document this.
- `extract_type_spec` (types.rs:54-160) has stateful flag `has_equals` to distinguish aliases from definitions — works but is convoluted; using `child_by_field_name("alias_eq")` would be cleaner.
- `extract_package` (types.rs:7-35) returns a `Namespace` symbol but `Go` uses package as the *module* concept, not namespace. Use `SymbolKind::Module` instead.

### C++
**Status**: Good

**Strengths**:
- Broadest type coverage: classes, structs, unions, enums, scoped enums, namespaces, friend declarations, typedefs, template declarations, operators, destructors, conversion operators (mod.rs:187-234).
- Handles function pointer typedefs and alignment attributes via post-processing (mod.rs:78-79).
- Visibility correctly tracks `access_specifier` nodes (`public:` / `private:` / `protected:`) walking siblings backward (visibility.rs). Defaults `class` to private and `struct`/`union` to public — correct C++ semantics.
- Multi-variable declarations: `extract_multi_declarations` produces additional symbols for `int x = 1, y = 2;` (mod.rs:130-139, declarations.rs).
- Template parameters extracted from parent `template_declaration` and prepended to signatures (functions.rs:66-69, types.rs:42-44).
- `processed_nodes` set prevents double extraction of `function_declarator` vs `function_definition` (mod.rs:174-238).
- ERROR node recovery for malformed source (mod.rs:244-297).
- Identifier extraction includes `TypeUsage` with declaration-site filtering and noise-type filter (identifiers.rs:70-89).

**Gaps & Errors**:
- **`extract_class`, `extract_struct`, `extract_union`, `extract_enum` all hardcode `Visibility::Public`** (types.rs:65, 122, 161, 211). The actual visibility of a *nested* class inside an enclosing class depends on the enclosing access specifier — but at the top level of a translation unit, classes have no visibility (translation-unit linkage). The hardcoded `Public` is a missed opportunity for nested types.
- **C++20 concepts (`concept`, `requires`)** are not extracted.
- **Lambda expressions** (`[](){ ... }`) produce no symbol.
- **Structured bindings** (`auto [x, y] = pair;`) likely don't decompose.
- `extract_class` reads `template_type` if the class name is a partial specialization (`Vector<bool>`), strips the angle-bracket part, and uses just `Vector` as the name (types.rs:20-31). This means `template <> class Vector<bool> { ... };` collides with the primary `Vector` symbol. Specializations need a distinguishing suffix or metadata flag.
- The C++ identifier extractor for `call_expression` extracts the *entire function expression text* as the name (identifiers.rs:34-35). For `obj.method()` you get name = `"obj.method"`, not `"method"`. This breaks symbol-name lookups; compare to TypeScript/Java which extract just the method name. This is a significant difference and likely a bug.
- `field_declaration` extraction returns `Vec<Symbol>` early and doesn't recurse into children (mod.rs:117-127), which is correct for plain fields but means a nested `class` inside a `struct` field declaration would be missed (rare).
- `extract_friend_declaration` exists but I didn't audit its kind selection in detail; verify it produces a meaningful symbol vs polluting with synthetic ones.
- `cpp/declarations.rs` is 506 lines, just over the 500-line target.

### C
**Status**: Good

**Strengths**:
- Broad coverage: includes, macros (`#define` and `#define name(...)`), declarations, function definitions, structs, unions, enums, typedefs, linkage specifications (mod.rs:166-253).
- Enum values from anonymous typedef'd enums (`typedef enum { ... } Name;`) are extracted as constants with the typedef name as parent (mod.rs:215-222).
- `typedef struct { ... } Name;` correctly extracts inner fields with the typedef as their parent (mod.rs:222-243).
- Function-pointer typedef name fixup post-pass (mod.rs:78).
- Multi-variable declarations handled via `find_variable_declarators` (declarations.rs:124-131).
- `expression_statement` recovery for trailing typedef syntax like `} PACKED Name;` (mod.rs:248-252) — clever.
- C identifier extraction includes `TypeUsage` with proper definition-site filtering (identifiers.rs:70-106).

**Gaps & Errors**:
- **Anonymous structs/unions** (without typedef) embedded in other structs do not produce a synthetic name, so their fields could be lost. Verify with a test.
- **`_Generic` selections** are not surfaced.
- **`__attribute__((...))`** handling is partial — `fix_struct_alignment_attributes` exists for one specific case; arbitrary attributes are textually merged into signatures rather than tracked separately.
- `extract_struct` and `extract_union` treat the case of `struct X { ... };` (with body) and skip extracting fields when the parent is a `type_definition`. The conditional uses `node.parent().map_or(false, |p| p.kind() == "type_definition")` (mod.rs:187-191) — a bare `struct X { ... };` outside a typedef will have its parent be `translation_unit` or `declaration`, so this branch correctly extracts. But a `struct X` declared inside a function body (block scope) might miss since the parent would be `compound_statement` — verify.
- The C extractor uses a regex (`Regex::new` in typedefs.rs imports) for typedef post-processing. A regex over names is fragile if the source has unusual whitespace.
- No extraction of `static_assert` or `_Static_assert`.

## Cross-Cutting Patterns

These appear across multiple extractors and represent the highest-leverage improvements:

1. **Inconsistent `IdentifierKind::TypeUsage` coverage.**  
   Languages WITH TypeUsage: TypeScript, Java, C++, C.  
   Languages WITHOUT TypeUsage: Rust (only `Call`/`MemberAccess`/`scoped_identifier` as TypeUsage but limited), JavaScript, Python, C#, Go.  
   Effect: `fast_refs` on a class/struct/type returns only the definition + import sites, not the usage sites in fields, parameters, returns, casts, generics. Severely limits the centrality signal and find-references quality for half the languages.

2. **`object_creation` / `new T()` constructor calls are missing from the identifier stream in Java, JavaScript, and C#.** They are partially captured in `relationships.rs` for those languages, but `IdentifierKind::Call` doesn't cover them. This means `fast_refs --reference_kind=call` on a constructor returns zero hits even if the class is instantiated frequently. C++ also lumps the entire callee text into one `Call` identifier name (e.g., `MyClass()` is fine but `obj.method()` becomes name = `"obj.method"`).

3. **Multi-declarator field/variable declarations are dropped in Java, C#, and (partially) C++.** Pattern is consistent: `for (declarator in declarators) { ... break; }` or `declarators.first()?`. Comments admit this is "for now." Source files with `int x, y, z;` lose 2/3 of the symbols. C and Go handle this correctly.

4. **Parent_id propagation is inconsistent.** Three patterns coexist:
   - **Walker threads parent_id** (Rust, Java, JavaScript, Go, C, C#, C++): `walk_tree(child, symbols, current_parent_id.clone())`.
   - **Walker doesn't thread, extractor recomputes parent ID by AST walk + name-based ID generation** (Python, TypeScript): `find_parent_class_id` walks ancestors and calls `generate_id(name, row, col)`.
   - **Walker doesn't thread, extractor passes `None`** (TypeScript class extraction, top-level functions, properties).  
   The recompute approach (#2) is fragile because it duplicates the ID-generation logic from `BaseExtractor::create_symbol` — if either changes, IDs desynchronize silently. The pass-None approach (#3) loses parent information entirely.

5. **Visibility hardcoding across languages.** C++ types hardcode `Visibility::Public`; Python uses `Visibility::Public` for all classes; many extractors don't surface `pub(crate)` / `internal` / `protected internal` / `pub(super)` finely — they collapse to `Public` or `Private`. The `Visibility` enum has only `Public/Private/Protected`, so subtler distinctions aren't representable. Consider an enum extension or a metadata field.

6. **`SymbolKind::Class` is overloaded.** Used for: Rust impl's parent (could be Struct, Enum, Trait, Union), Java records, Go structs, Python classes (correct), C# records, C++ classes. Code that filters by `SymbolKind::Class` will silently match too much for some languages and not enough for others. Map carefully:
   - Java records → `Struct` (or new `Record` variant)
   - Go struct → `Struct`
   - C# record → `Struct` (or new variant)

7. **Nested function and lambda support is weak.** Nested functions (Python, JS arrow), local functions (C#), lambdas (Java, C#, C++, Python), block expressions (Rust closures) — none of these consistently produce symbols with proper parent_id. Find-references for "what's inside this lambda?" doesn't work.

8. **Annotations not propagated to all symbol kinds.** Java extracts annotations on methods and constructors but not on classes/interfaces/enums. C# extracts them everywhere. Python extracts decorators but they're stored as raw text in metadata, not parsed. TypeScript extracts both child decorators and preceding-sibling decorators but the logic differs between class and method paths.

9. **Doc comments pickup is uneven.** Rust, Java, C#, TypeScript handle their idiomatic doc style. Python uses a custom path. JavaScript uses base `find_doc_comment` for JSDoc, which works because `/** */` looks like a comment to the base. Imports often skip doc-comment extraction. Some languages (Go) fall back to the parent node's doc comment, others don't.

10. **Regex compilation in hot paths.** `javascript/mod.rs:390-399` compiles a regex per JSDoc lookup. `python/mod.rs:33-38` uses `LazyLock` correctly. Audit for one-shot `Regex::new` calls in extraction loops.

## Top 10 Highest-Impact Findings (ranked)

1. **TypeScript `extract_class` always passes `parent_id: None`** (typescript/classes.rs:107) and the visitor doesn't thread parent_id either. Nested classes, classes inside namespaces, and most structural relationships are broken. The cascade of post-creation ID regeneration in `extract_function`/`extract_method` (functions.rs:91-128) compounds the fragility.

2. **Multi-declarator drops in Java (fields.rs:41-46), C# (members.rs:315), and C++ (declarations.rs comment "For now, handle the first declarator").** Real source code regularly has `int a, b, c;`. Today, only `a` is indexed. Each comment explicitly admits this is incomplete.

3. **`object_creation_expression` / `new T()` not in identifier stream for Java, C#, JavaScript.** Constructor calls are the primary form of cross-module dependency in OO languages; missing them in `IdentifierKind::Call` means `fast_refs` for constructors is unreliable.

4. **C++ identifier extraction uses the entire function expression as the call name** (cpp/identifiers.rs:34-35). For `obj.method()`, it stores `name = "obj.method"`. This is inconsistent with every other language extractor and breaks symbol-name lookups for C++ method calls.

5. **Python doesn't extract PEP 695 type aliases (`type X = ...`) or `match` statements.** Modern Python (3.10+ for match, 3.12+ for type aliases) is increasingly common; both are silently dropped.

6. **Half the language identifier extractors omit `TypeUsage`.** Rust (only via scoped_identifier in some cases), JavaScript (no native types), Python, C#, Go. This halves the value of centrality scoring and find-references for type names in those languages.

7. **Java records get `SymbolKind::Class` and don't decompose into properties.** `record Point(int x, int y)` produces one symbol; the `x` and `y` components should be `SymbolKind::Property` children. Same issue with Java's lack of annotations on classes (classes.rs:11-76 doesn't call `extract_annotations`).

8. **C# misses local functions, lambdas, and partial class linkage.** Three big modern C# features that produce no symbols today. `partial class Foo` in two files creates two unrelated symbols.

9. **C++ class/struct/union/enum visibility is hardcoded to `Public`** (types.rs:65, 122, 161, 211). For nested types inside a `class`, default visibility is `private` per C++ rules — but the hardcode makes it always `Public`. Wrong default for nested types.

10. **Parent_id inconsistency between extractors creates silent index corruption.** Python and TypeScript regenerate parent IDs by recomputing `generate_id(name, row, col)` independently of `create_symbol`. If `BaseExtractor::create_symbol` changes its ID-generation logic (which it has — see the regen workaround in `typescript/functions.rs:91-128`), the relationship stitching silently breaks. A single contract — "the walker passes parent_id; extractors never recompute IDs" — would eliminate a class of bugs.

### Honorable Mentions

- **Python lambda naming `lambda_{row}`** collides on the same row and never has a parent_id (functions.rs:131-145). Lambdas inside `map(lambda x: ..., list)` are common.
- **JavaScript `extract_export` signature is the entire node text** (types.rs:157), which can be enormous for `export class X { ... 500 lines ... }`.
- **Rust `extract_macro_invocation` filter** is item-position only via parent_kind whitelist; this might miss macros in certain attribute positions or `extern` blocks.
- **Go uses `SymbolKind::Class` for structs and `SymbolKind::Namespace` for packages**; both should be more specific (`Struct` and `Module` respectively).
- **C++ template specializations collide with the primary template** by name (types.rs:20-31 strips angle brackets for the symbol name).
- **TypeScript `extract_property` doesn't surface `?` (optional) or `!` (definite assignment)** modifiers.
