# GPT Findings: Tree-sitter Extractor Data Quality

Date: 2026-05-06

Scope: `crates/julie-extractors`, with emphasis on whether each language extracts data rich enough to support search, navigation, relationships, type lookup, and graph quality. This is not a parser dependency audit. It is an extracted-data audit.

Confidence: 88/100. The highest-severity findings are direct code-reading findings with line evidence. Some medium findings should still get small regression tests before fixing, because tree-sitter node shapes vary by grammar.

## Summary

The biggest quality risks are not broad parser failures. They are plausible-looking outputs with the wrong identity, wrong relationship kind, missing cross-file pending edges, or incomplete language idioms. That is worse for Julie than an obvious parse failure because downstream tools trust the data.

Highest priority fixes:

1. Fix symbol identity bugs in type extraction, especially Zig and Dart.
2. Stop C++ relationship extraction from dropping ordinary overload and constructor cases.
3. Fix SQL relationship corruption, especially JOIN self-edges.
4. Add missing cross-file pending edges for inheritance and conformance in Dart and Ruby.
5. Bring TypeScript import extraction up to the JavaScript extractor's binding-level fidelity.
6. Add tests that assert exact relationship kind, symbol IDs, duplicate counts, and binding names, not just non-empty output.

## Findings

### 1. High: Zig and Dart type maps are keyed by symbol name, not symbol ID

Evidence:

- `convert_types_map` treats each `infer_types()` key as a symbol ID and copies it into `TypeInfo.symbol_id`: `crates/julie-extractors/src/factory.rs:13`.
- Zig inserts `symbol.name.clone()` as the key: `crates/julie-extractors/src/zig/type_inference.rs:18`, `:30`, `:40`, `:54`, `:58`.
- Dart does the same: `crates/julie-extractors/src/dart/mod.rs:347`, `:354`, `:361`.

Impact:

- Type rows can attach to non-existent symbol IDs.
- Same-name symbols across scopes collide.
- Any downstream lookup that expects `TypeInfo.symbol_id` to match a real `Symbol.id` becomes unreliable.

Fix direction:

- Make every `infer_types()` implementation return `HashMap<symbol_id, type_string>`.
- Add regression tests that assert every key in `results.types` exists in `results.symbols`.
- Add exact type assertions for duplicate-name symbols in different scopes.

### 2. High: C++ drops call and inheritance edges when names are duplicated

Evidence:

- C++ relationship extraction builds a `unique_symbol_map`: `crates/julie-extractors/src/cpp/relationships.rs:21`.
- `unique_symbol_map` removes all names with more than one candidate: `crates/julie-extractors/src/base/relationship_resolution.rs:141`.
- C++ caller lookup returns early if the containing function name is not unique: `crates/julie-extractors/src/cpp/relationships.rs:237`.
- C++ inheritance lookup also relies on that map for the derived class and base class: `crates/julie-extractors/src/cpp/relationships.rs:80`, `:103`.
- Constructors are emitted as symbols with the same name as the class: `crates/julie-extractors/src/cpp/functions.rs:173`.

Impact:

- Overloaded functions, common method names like `init` and `size`, and classes with constructors can lose relationships.
- A class with a constructor can make both the class name and constructor name non-unique, which blocks inheritance extraction.
- This degrades call graph, centrality, blast radius, and inheritance navigation in normal C++ code.

Fix direction:

- Resolve callers by node containment or symbol span, not by name uniqueness.
- Resolve inheritance targets with kind-aware candidates, for example `Class` or `Struct` only.
- Add tests for inheritance when both base and derived classes have constructors.
- Add tests for calls inside overloaded methods with the same terminal name.

### 3. High: `.h` headers are always parsed as C, not C++

Evidence:

- C owns extension `h`: `crates/julie-extractors/src/language_spec/specs.rs:13`.
- C++ does not include `h`: `crates/julie-extractors/src/language_spec/specs.rs:21`.

Impact:

- C++ projects commonly put classes, templates, namespaces, and inline methods in `.h` files.
- Those files are parsed with `tree-sitter-c`, so C++ symbols and relationships are missed or degraded.

Fix direction:

- Add a content-based fallback for `.h`, for example detect `class`, `namespace`, `template`, `public:`, `private:`, and C++ includes.
- Prefer compile database hints when available.
- Add fixture tests for `.h` files containing C++ classes and plain C declarations.

### 4. High: TypeScript import extraction is statement-level, while JavaScript is binding-level

Evidence:

- TypeScript uses the module source as the import symbol name: `crates/julie-extractors/src/typescript/imports_exports.rs:13`.
- TypeScript creates one import symbol per import statement: `crates/julie-extractors/src/typescript/imports_exports.rs:30`.
- JavaScript creates binding-level import symbols with metadata for source, specifier, default import, and namespace import: `crates/julie-extractors/src/javascript/imports.rs:27`, `:42`.

Impact:

- `import { helper as h } from "./utils"` produces a symbol for `"./utils"` rather than local binding `h`.
- Imported identifiers are harder to resolve, rank, and reference accurately.
- TypeScript is a core language for Julie, so this is not a minor richness gap.

Fix direction:

- Port the JavaScript binding-level import model to TypeScript.
- Preserve module source and original specifier in metadata.
- Add tests for named imports, aliases, default imports, namespace imports, type-only imports, and mixed import clauses.

### 5. High: Dart drops normal cross-file inheritance and conformance edges

Evidence:

- Dart emits an `Extends` relationship only when the superclass is found in the same file: `crates/julie-extractors/src/dart/relationships.rs:74`.
- There is no pending fallback after the local lookup misses: `crates/julie-extractors/src/dart/relationships.rs:74`.
- Dart's pending pass is call-only: `crates/julie-extractors/src/dart/mod.rs:335`.

Impact:

- `extends`, `implements`, and mixin-based relationships disappear across files.
- Class hierarchy, centrality, and blast-radius quality suffer for normal Dart projects.

Fix direction:

- Emit structured pending relationships when local inheritance or interface targets are unresolved.
- Include import context when the target is imported.
- Add cross-file tests for `extends`, `implements`, `with`, and generic superclass references.

### 6. High: Ruby inheritance and mixins do not emit pending relationships for cross-file targets

Evidence:

- Ruby class inheritance only pushes a relationship if both symbols exist in the same file: `crates/julie-extractors/src/ruby/relationships.rs:89`.
- Ruby `include`, `extend`, `prepend`, and `using` do the same: `crates/julie-extractors/src/ruby/relationships.rs:188`.
- There is no pending fallback in either branch.

Impact:

- Rails-style inheritance like `UsersController < ApplicationController` is commonly cross-file and will often be missing.
- Concern and module inclusion relationships are underreported.
- Ruby call extraction has structured pending support, so this gap is inconsistent with the rest of the language implementation.

Fix direction:

- Emit structured pending relationships for unresolved superclasses and mixin modules.
- Preserve namespace-qualified constants like `Admin::BaseController`.
- Add tests for Rails-style controller inheritance and `include SomeConcern` across files.

### 7. High: SQL JOIN relationships are self-edges and SELECT/FROM table usage is skipped

Evidence:

- SQL explicitly skips `select_statement` and `from_clause` relationship extraction: `crates/julie-extractors/src/sql/relationships.rs:31`.
- JOIN extraction creates both `from_symbol_id` and `to_symbol_id` from the same table symbol: `crates/julie-extractors/src/sql/relationships.rs:181`, `:189`.
- Two SQL relationship paths use 0-based line numbers: `crates/julie-extractors/src/sql/relationships.rs:148`, `:193`.

Impact:

- JOIN edges corrupt the graph by adding table self-edges.
- Read/query lineage is mostly absent because `SELECT/FROM` table references are ignored.
- Off-by-one line numbers degrade navigation and diagnostics.

Fix direction:

- Represent query-to-table usage or source-table-to-target-table relationships consistently.
- Remove JOIN self-edges unless there is a deliberate self-join with aliases.
- Normalize all relationship line numbers to 1-based.

### 8. High: Vue script symbols have zero byte ranges, and template identifiers are ignored

Evidence:

- Vue `create_symbol_manual` hardcodes `start_byte: 0` and `end_byte: 0`: `crates/julie-extractors/src/vue/script.rs:213`.
- The helper is used for script and script-setup extraction: `crates/julie-extractors/src/vue/script.rs:177`.
- Vue identifier extraction processes only sections with `section_type == "script"`: `crates/julie-extractors/src/vue/identifiers.rs:20`.

Impact:

- Range-based navigation, containment, overlap resolution, and precise jump targets degrade for Vue script symbols.
- Template references like `{{ user.name }}`, `@click="save"`, and `:items="items"` do not contribute identifiers.
- Vue's most important cross-section data, template-to-script usage, is missing.

Fix direction:

- Preserve byte offsets when mapping parsed script sections back to the full `.vue` file.
- Extract template expression identifiers and connect them to script symbols when possible.
- Add tests for script setup bindings used only from the template.

### 9. Medium-high: Go loses common language idioms

Evidence:

- Go struct embedding is explicitly skipped: `crates/julie-extractors/src/go/relationships.rs:90`.
- Go stdlib suppression recognizes only `fmt`: `crates/julie-extractors/src/go/relationships.rs:13`.
- Go `var` and `const` spec extraction stores a single identifier while walking possible multiple identifiers: `crates/julie-extractors/src/go/specs.rs:170`, `:240`.

Impact:

- Embedded struct relationships are absent, even though embedding is a core Go composition pattern.
- Calls like `strings.TrimSpace`, `os.Exit`, and `context.WithTimeout` can create noisy unresolved pending edges.
- `var a, b int` and `const x, y = ...` collapse to one symbol.

Fix direction:

- Emit composition or uses relationships for embedded fields.
- Replace the `fmt` special case with generic stdlib detection, probably based on import path shape plus a known stdlib set.
- Emit one symbol per declared identifier in multi-name var and const specs.

### 10. Medium-high: Python drops multi-import bindings and truncates common type hints

Evidence:

- `extract_single_import` breaks after the first `aliased_import` or `dotted_name`: `crates/julie-extractors/src/python/imports.rs:83`, `:90`.
- The return hint regex captures only non-whitespace text: `crates/julie-extractors/src/python/mod.rs:32`.

Impact:

- `import os, sys` emits only one import symbol.
- `import numpy as np, pandas as pd` emits only one alias.
- Return hints like `dict[str, int]` can be truncated because the regex stops at whitespace.

Fix direction:

- Return a `Vec<Symbol>` for import statements that declare multiple bindings.
- Parse import child nodes structurally instead of stopping at the first match.
- Replace regex-only return type extraction with tree-sitter annotation nodes where available.

### 11. Medium-high: Scala and Swift misclassify unresolved inheritance and conformance

Evidence:

- Scala unresolved inheritance chooses `Implements` for every non-trait source: `crates/julie-extractors/src/scala/relationships.rs:69`.
- Swift unresolved class bases force the first entry to `Extends`: `crates/julie-extractors/src/swift/relationships.rs:164`.
- Swift relationship traversal ignores `protocol_declaration`, even though symbol extraction supports protocols: `crates/julie-extractors/src/swift/relationships.rs:31`.

Impact:

- `class Worker extends ExternalService` can become an unresolved `Implements` edge in Scala.
- `class UserModel: Codable` can become `Extends` instead of protocol `Implements` in Swift.
- Swift protocol-to-protocol inheritance is missed.

Fix direction:

- Preserve enough syntax context to distinguish class inheritance from protocol or trait conformance.
- Add relationship-kind assertions to tests, not just target-name assertions.
- Include Swift protocol declarations in relationship traversal.

### 12. Medium-high: PHP strips namespaces before unresolved targets are created

Evidence:

- `strip_php_namespace` returns only the terminal component: `crates/julie-extractors/src/php/relationships.rs:13`.
- Class `extends` uses the stripped name for both same-file lookup and pending relationship targets: `crates/julie-extractors/src/php/relationships.rs:41`, `:76`.
- Object creation does the same: `crates/julie-extractors/src/php/relationships.rs:289`.

Impact:

- `\App\Http\BaseController` becomes `BaseController`.
- Framework code commonly has repeated terminal class names across namespaces.
- Cross-file resolution loses the namespace information it needs to choose the right target.

Fix direction:

- Keep the full qualified name in `UnresolvedTarget.namespace_path`.
- Use terminal names only as a fallback.
- Add tests with two same-terminal classes in different namespaces.

### 13. Medium: C# can emit duplicate call relationships for member calls

Evidence:

- C# extracts calls for `invocation_expression`: `crates/julie-extractors/src/csharp/relationships.rs:48`.
- It also extracts calls for a nested `member_access_expression` when the next sibling is `argument_list`: `crates/julie-extractors/src/csharp/relationships.rs:57`.

Impact:

- A call like `service.Process()` can be visited once as an invocation and once as member access.
- If no deduplication catches it, centrality and reference counts inflate.

Fix direction:

- Choose one tree-sitter node kind as the canonical call extraction point.
- Add tests that assert exactly one relationship for a member call.

### 14. Medium: Elixir skips qualified module calls

Evidence:

- Elixir call target extraction returns a name only when the target node is `identifier`: `crates/julie-extractors/src/elixir/helpers.rs:14`.
- Non-identifier targets return `None`: `crates/julie-extractors/src/elixir/helpers.rs:17`.

Impact:

- Common calls like `Logger.info(...)`, `Enum.map(...)`, and `MyApp.Service.run(...)` can be skipped.
- The call graph is biased toward local unqualified functions.

Fix direction:

- Handle `dot` and alias-qualified targets.
- Emit structured pending relationships with module receiver context.
- Add tests for remote calls, imported calls, and aliased module calls.

### 15. Medium: Zig relationship extraction ignores calls originating from methods

Evidence:

- Zig only accepts containing symbols whose kind is `Function`: `crates/julie-extractors/src/zig/relationships.rs:183`.
- Zig method symbols are emitted as `SymbolKind::Method`: `crates/julie-extractors/src/zig/functions.rs:23`.

Impact:

- Calls inside methods are skipped even when function calls are otherwise supported.
- Zig code using struct-associated functions and methods gets a partial call graph.

Fix direction:

- Accept `Function`, `Method`, and likely `Constructor` equivalents where the language has them.
- Add a regression test for a method that calls another local function.

### 16. Medium: C indirect calls are ignored

Evidence:

- C relationship extraction returns unless the callee node kind is `identifier`: `crates/julie-extractors/src/c/relationships.rs:48`.

Impact:

- Function pointer calls and callback dispatch are absent from the call graph.
- This matters in C codebases that use vtables, callbacks, and event loops.

Fix direction:

- Emit lower-confidence pending relationships for indirect call expressions.
- Preserve the expression text and receiver/context metadata when direct resolution is impossible.

### 17. Medium: Rust type relationships skip unions

Evidence:

- Rust relationship traversal extracts type relationships for `struct_item` and `enum_item`: `crates/julie-extractors/src/rust/relationships.rs:48`.
- `union_item` is not included there, even though union symbols are extracted elsewhere.

Impact:

- Unsafe and FFI-heavy Rust code can miss field type relationships for unions.
- The type graph is less complete than the symbol graph.

Fix direction:

- Include `union_item` in type relationship traversal.
- Add a union fixture with field types that should create `Uses` relationships.

### 18. Medium: Ruby synthetic symbols lose richness

Evidence:

- `attr_reader`, `attr_writer`, and `attr_accessor` use only the first symbol argument: `crates/julie-extractors/src/ruby/calls.rs:160`.
- Synthetic properties and dynamic methods are emitted with `parent_id: None`: `crates/julie-extractors/src/ruby/calls.rs:170`, `:209`.

Impact:

- `attr_reader :name, :age` emits only `name`.
- Class-level methods created through `define_method` or `attr_accessor` appear top-level.
- Ruby hierarchy and navigation become misleading.

Fix direction:

- Return multiple symbols for multi-argument attribute macros.
- Pass class or module parent context into call-derived symbol extraction.

### 19. Medium-low: HTML script import relationships can attach to the wrong script symbol

Evidence:

- HTML script relationship extraction selects the first symbol with metadata type `script-element`: `crates/julie-extractors/src/html/relationships.rs:124`.

Impact:

- In documents with multiple `<script src="...">` tags, relationships can originate from the first script symbol rather than the current script node.

Fix direction:

- Resolve the script symbol by node span or line range.
- Add a two-script fixture that asserts each relationship source ID.

### 20. Medium-low: PowerShell built-in cmdlet suppression is too narrow

Evidence:

- PowerShell treats only `Write-Output` and `Get-ChildItem` as built-ins: `crates/julie-extractors/src/powershell/relationships.rs:100`.

Impact:

- Common commands like `Where-Object`, `Select-Object`, `ForEach-Object`, `Get-Content`, and `Set-Content` can become unresolved pending calls.
- Pending relationship noise reduces precision.

Fix direction:

- Use a broader built-in cmdlet list or a generic rule for approved common cmdlets.
- Add tests for common pipeline commands.

## Test Gaps

Several bugs survive because tests assert that extraction produced something, not that it produced the right thing.

Examples:

- Dart and Zig type tests need to assert that type keys match real symbol IDs.
- TypeScript extractor tests should use the TypeScript grammar for TypeScript-specific syntax. Some current TypeScript tests use `tree_sitter_javascript::LANGUAGE`, for example `crates/julie-extractors/src/tests/typescript/symbols.rs:34`.
- Relationship tests should assert exact relationship kind, exact count, and exact source and target IDs.
- Cross-file tests should cover inheritance, implementation, mixins, imports, namespace-qualified targets, and aliasing, not just calls.
- SQL tests should assert there are no accidental self-edges except deliberate self-joins.

## Triage Order

Fix first:

1. Zig and Dart type map keys.
2. C++ relationship resolution for overloaded names and constructors.
3. SQL JOIN self-edges and line numbering.
4. TypeScript binding-level imports.
5. Dart and Ruby unresolved inheritance pending relationships.

Fix next:

1. Vue byte ranges and template identifiers.
2. Go multi-name declarations, embedding, and stdlib filtering.
3. Scala and Swift relationship kind classification.
4. PHP namespace-preserving unresolved targets.
5. Python multi-import extraction.

Then tighten tests across all touched languages so these failures cannot hide behind non-empty assertions again.
