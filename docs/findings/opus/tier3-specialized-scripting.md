# Tier 3 Extractor Audit — Specialized & Scripting

## Summary

Audit of seven extractors (Bash, PowerShell, Lua, R, GDScript, Zig, VB.NET). Quality varies wildly. Bash and PowerShell share a common architectural mistake: they treat *call sites* of well-known external commands (docker, kubectl, Connect-AzAccount) as `Function` symbols, conflating definitions with usages. R is the most under-developed: no S4/R6 class detection, no Roxygen doc comments, several tests assert `>= 0` (tautological). Lua and GDScript stuff entire function bodies into the `signature` field, producing massive token bloat. Zig has a precision issue where pattern matches on `node_text.contains(...)` cause false positives, and call relationships drop everything called from inside `Method`s (only `Function` callers tracked). VB.NET ignores its own case-insensitive identity rules and maps modules to `Class` instead of using `SymbolKind::Module`. PowerShell's `is_builtin_cmdlet` knows exactly two cmdlets, generating noisy pending relationships for every standard cmdlet. Top systemic issues: signature bloat, call-site-as-definition pattern, missing class-system detection, and tautological tests.

## Per-Language Findings

### Bash
**Status**: Needs Work

**Strengths**
- Two-style function support via `find_name_node` field/word fallbacks (`bash/helpers.rs:10-24`).
- Positional parameter extraction from function bodies via `PARAM_NUMBER_RE` (`bash/functions.rs:12, 58-100`).
- Shebang detection creates an interpreter-named symbol (`bash/mod.rs:48-74`).
- `extract_command_relationships` correctly distinguishes resolved local calls from pending cross-file calls (`bash/relationships.rs:32-69`).

**Gaps & Errors**
- **Call-site-as-definition antipattern**: `extract_command` creates a `Function` symbol for any `docker`/`kubectl`/`python`/etc. invocation (`bash/commands.rs:11-67`). These are call sites, not definitions; they should be identifiers/relationships only. The base extractor will then treat every `docker run` as a function definition. Worse, the same names are listed in `is_builtin_command` (`bash/relationships.rs:88-92`) — so the extractor *creates* symbols for them but then refuses to *track* relationships against them. The two halves are inconsistent.
- **No alias support**: `alias ll='ls -la'` produces nothing. Aliases are first-class symbols.
- **No source/. tracking**: `source other.sh` and `. ./helpers.sh` produce no `Import` symbols. PowerShell tracks dot-sourcing; bash should mirror that.
- **`function name() {}` keyword-form not exercised**: `find_name_node` falls back on word/identifier children but the `function_definition` AST in tree-sitter-bash uses different structures for `function foo() {}` vs `foo() {}`. Test fixtures should cover both.
- **Multi-assignment in declarations dropped**: `extract_declaration` only extracts the *first* `variable_assignment` child of a `declare`/`export`/`readonly` (`bash/variables.rs:60-95`). `export A=1 B=2 C=3` loses B and C.
- **`is_environment_variable` regex is too eager**: any all-caps name is classified as a constant/environment variable (`bash/variables.rs:12, 118`). Locally-defined `local MAX_RETRY=5` becomes `SymbolKind::Constant`, which is misleading.
- **No here-doc capture**: variables produced via here-doc patterns are not extracted.
- **Identifier extraction is shallow**: `extract_identifier_from_node` (`bash/mod.rs:178-215`) handles `command` and `subscript` nodes but skips bare variable references (`$var`, `${var:-default}`, `${var/foo/bar}`). Per-token references are critical for `find_references`.
- **`commands.rs` and `helpers.rs` duplicate command-list constants**: `cross_language_commands` in commands.rs vs `is_builtin_command` in relationships.rs — divergent.

### PowerShell
**Status**: Needs Work

**Strengths**
- Solid module structure (10 files, each focused).
- Custom comment-based help extraction with sibling and ancestor walk (`powershell/documentation.rs:121-181`).
- Class members handled: methods, properties, enums, enum members.
- Import/Export differentiated via `SymbolKind::Import` and `SymbolKind::Export` (`powershell/imports.rs:111-115`).
- Dot-sourcing produces an `Import` symbol with the script basename.

**Gaps & Errors**
- **`EMPTY` doc comment styles in language spec**: `language_spec/specs.rs:226` registers PowerShell with `EMPTY` doc styles, meaning `LanguageSpec::is_doc_comment` always returns false for PS. The PowerShell module compensates with `extract_powershell_doc_comment`, but anything calling the base `find_doc_comment` (or any cross-cutting code that uses `LanguageSpec`) gets nothing. The spec should contain `HashLine` and an explicit `<#` block style.
- **Method signatures are malformed**: `extract_method_signature` returns `format!("{}{} {}()", prefix, suffix, name)` (`powershell/classes.rs:197`). When `static` and a return type are both present, you get `"static  string MyMethod()"` (double space). Worse, the parameter list is *always* `()` — methods with parameters render with empty parentheses. This makes signatures actively misleading.
- **Method parameters not extracted as symbols**: `walk_tree_for_symbols` only extracts parameters when `symbol.kind == Function` (`powershell/mod.rs:67-72`). PS class methods have parameter blocks too; they're silently dropped.
- **Call-site-as-definition antipattern**: `extract_command` creates a `Function` symbol for `Connect-AzAccount`, `New-AzResourceGroup`, `docker`, etc. (`powershell/commands.rs:11-92`). Every script that calls `Connect-AzAccount` produces a fake Function symbol named `Connect-AzAccount`. This is a call site, not a definition.
- **`is_builtin_cmdlet` is laughably narrow**: `matches!(command_name, "Write-Output" | "Get-ChildItem")` (`powershell/relationships.rs:100-102`). Every other standard cmdlet (`Get-Date`, `Set-Location`, `Out-File`, `Where-Object`, `Select-Object`, ...) generates noisy pending relationships that will never resolve. PS ships with hundreds of built-in cmdlets; an exclusion list is the only way to keep this signal-to-noise tolerable.
- **`extract_function_from_error` regex** assumes well-formed `function Name {`, but real PS may have `function Verb-Noun [<#help#>]` with the comment between `function` and the name. The regex fails silently.
- **Variable extraction for global/script scope**: `is_global = full_text.contains("Global:")` (`powershell/variables.rs:31`). If `Global:` appears in a string literal, it's a false positive. Should test the `scope` field of the variable AST node instead.
- **No support for `[Parameter()]` typed scalars in the spec**: type brackets like `[ValidateSet(...)]`, `[ValidateRange(...)]` aren't currently captured as annotations.
- **Configuration extraction depends on ERROR nodes** (`powershell/mod.rs:168-218`). DSC syntax parses cleanly in many cases; relying on ERROR nodes is fragile.

### Lua
**Status**: Needs Work

**Strengths**
- Distinguishes local vs global functions (`lua/functions.rs:127-154` for locals).
- Handles `obj:method` colon syntax and `obj.method` dot syntax for methods (`lua/functions.rs:39-72`).
- Class detection via post-processing for setmetatable, `:extend()`, and `__index` patterns (`lua/classes.rs:23-155`).
- `require()` calls produce `SymbolKind::Import` via type inference (`lua/helpers.rs:35-43`).
- Good identifier coverage including `dot_index_expression` and `method_index_expression` (`lua/identifiers.rs:46-153`).

**Gaps & Errors**
- **Function signatures are entire function bodies**: `let signature = base.get_node_text(&node)` (`lua/functions.rs:81, 137`) captures the full text from `function foo()` through `end`. A 100-line function gets a 100-line `signature`. This is the single biggest token-waste in this extractor. Should build a synthetic signature from name + parameters + return hint, like Zig does.
- **Method paths longer than two parts dropped silently**: `obj.sub.method` returns `None` (`lua/functions.rs:51-72`) because the parts.len() != 2 guard. Lua module patterns commonly nest like `M.utils.format = function() ... end`.
- **Class detection sets metadata but doesn't emit `Extends` relationships**: `detect_lua_classes` writes `metadata["baseClass"]` (`lua/classes.rs:120, 148`) but doesn't push to a relationships vector. Means callers asking for `find_references` on a base class miss subclass uses.
- **`require()` symbol uses the receiving variable's name, not the module path**: `local foo = require("path.to.module")` produces an Import symbol named "foo" with no record of "path.to.module" anywhere queryable. Cross-file resolution can't match against the actual module name.
- **Containing-function lookup uses name-only match**: `find_enclosing_function` in `lua/relationships.rs:111-134` does `symbol_map.get(caller_name.as_str())` — but two functions in the same file with the same name (overload-style) collapse to one. Position-based scoping would be safer.
- **No metatable method tracking**: `setmetatable(t, {__index = function() ... end})` captures none of the metamethods.
- **`extract_assignment_statement` swallows multi-RHS tuple assignments**: `x, y = a, b` works for matched pairs but `x, y = unpack(t)` (single-call returning multiple values) doesn't get inferred types for either symbol.
- **Visibility for upper-case underscore prefix**: A `local _M = {}` is marked Private by `_` rule but is the canonical Lua *public* module return; should be Public.

### R
**Status**: Significant Gaps

**Strengths**
- S3 method detection: dotted names like `print.person` produce `Method` symbols with `s3_method`/`s3_class` metadata (`r/mod.rs:251-304`).
- Generic detection via `body_contains_usemethod` (`r/mod.rs:393-410`).
- `library()` and `require()` calls produce Import symbols (`r/mod.rs:413-444`).
- `NON_S3_DOT_FUNCTIONS` exclusion list keeps `data.frame`, `as.numeric`, etc. classified as Functions, not S3 methods (`r/mod.rs:17-142`).
- Relationship extraction handles `pkg::fn` namespace operator and `obj$method` extract operator (`r/relationships.rs:48-66`).

**Gaps & Errors**
- **No Roxygen doc comments anywhere**: `R_DOCS` is wired correctly in `language_spec/specs.rs:208-211`, but the R extractor never calls `find_doc_comment` or its base helper. `grep -c "find_doc_comment\|extract_documentation"` returns 0 across all R files. The `doc_comment` field on every R symbol is `None`. This is a critical, easy-to-fix gap; Roxygen is the standard R documentation system.
- **No S4 class detection (`setClass`)**: A `setClass("Student", slots = c(...))` call leaves no `Class` symbol behind. The test in `tests/r/classes.rs:117` literally asserts `symbols.len() >= 0` for an S4 class — a tautology. Real users searching for "Student class" will find nothing.
- **No S4 method detection (`setMethod`)**: `setMethod("display", "Student", function() ...)` doesn't produce a `Method` symbol bound to the `Student` class. Test `test_s4_methods` (`tests/r/classes.rs:139-141`) again asserts `>= 0`.
- **No S4 generic detection (`setGeneric`)**: Same gap.
- **No R6 class detection (`R6Class`)**: `Person <- R6Class("Person", public = list(initialize = ..., greet = ...))` parses as a binary_operator with a generic call on the right; methods inside the `public` list are invisible. Test asserts only that *variables* exist (`tests/r/classes.rs:176-180`), not classes.
- **No Reference class detection (`setRefClass`)**: same shape as R6, same blind spot.
- **No replacement function detection**: `names(x) <- c("a", "b")` is the canonical R replacement-function pattern; `extract_from_binary_op` requires `left.kind() == "identifier"` (`r/mod.rs:210-211`) so `names(x)` (a `call` node) is rejected — both as a definition site and as an assignment.
- **`->` and `->>` right-to-left assignments only handle plain identifier targets**: `value -> obj$field` is dropped (`r/mod.rs:230-246`).
- **`mod.rs` is 523 lines** — over the 500-line target.

### GDScript
**Status**: Good (with caveats)

**Strengths**
- Correctly handles implicit class from top-level `extends` statement (`gdscript/mod.rs:62-105`).
- Class hierarchy: `class_name`, inner `class`, and implicit (extends-only) classes all distinguished.
- Lifecycle callbacks (`_ready`, `_process`, `_input`, etc.) classified as Methods even in implicit classes (`gdscript/functions.rs:8-21, 181-195`).
- Annotations (`@export`, `@onready`) captured in metadata (`gdscript/variables.rs:46-87`).
- Signals as `SymbolKind::Event` (`gdscript/signals.rs:8-33`).
- Reasonable type inference from explicit `-> Type` and `: Type` annotations (`gdscript/mod.rs:139-162`).

**Gaps & Errors**
- **Function signatures are entire function bodies**: `let signature = base.get_node_text(&parent_node)` (`gdscript/functions.rs:92`) captures the whole function block. Same bloat issue as Lua. Worth reducing to `func name(params) -> ReturnType:`.
- **Identifier extraction stores full attribute chain as the call name**: in `gdscript/identifiers.rs:53`, when the call's child is an `attribute`, `name = base.get_node_text(&child)` yields `"self.do_thing"` rather than `"do_thing"`. Subsequent reference matching against symbol names (which store `"do_thing"`) will miss every method call.
- **`get_node` special-case is questionable**: `gdscript/identifiers.rs:75-80` treats `get_node` as its own node kind and creates an identifier with literal name `"get_node"`. tree-sitter-gdscript doesn't have a `get_node` node type — this branch may be dead code or relying on an older grammar.
- **No constructor for `_ready` vs `_init` distinction enforced**: `_init` is hardcoded as Constructor (`gdscript/functions.rs:146-148`), but Godot 4 also has `_static_init` for static class init. Not handled.
- **`determine_effective_parent_id` uses column-based heuristics** (`gdscript/mod.rs:314-357`) — fragile against tab-vs-space mixes and against the Godot 4 `static func` keyword which can break the indentation expectation.
- **No `enum NamedEnum { A, B, C }` member ordering**: enum members are extracted but order/value information isn't preserved beyond signature text.
- **`signal` extracted with no parameter list parsing**: `signal hit(damage: int, source: Node)` produces an Event symbol but the parameter shape isn't recorded as identifier targets.
- **`Composition` / `Returns` relationships not emitted for typed parameters**: a function `func update(player: Player) -> Result:` only yields a Calls relationship via the body, not a `Uses` or `Parameter` relationship for the `Player` type.

### Zig
**Status**: Good (with caveats)

**Strengths**
- Excellent function signature builder (`zig/functions.rs:114-236`) with proper handling of `pub`/`export`/`inline`/`extern`, `comptime` parameters, variadic `...`, and return type detection.
- Strong type-usage identifier coverage including `pointer_type`, `optional_type`, `error_union_type` (`zig/identifiers.rs:113-154`) — distinguishes type position from name position via colon detection.
- Skips Zig builtin types via `is_zig_builtin_type` to keep noise low (`zig/identifiers.rs:188-219`).
- Discriminates `struct`/`union`/`enum`/`error_set`/function-type/generic constructor in `extract_variable` (`zig/variables.rs:8-64`).
- Test extraction (`test "name" {...}`) produces a Function with `is_test` metadata.
- Composition relationships emitted from struct fields with named types (`zig/relationships.rs:52-150`).

**Gaps & Errors**
- **`is_const` detection uses substring match**: `node.kind() == "const_declaration" || base.get_node_text(&node).contains("const")` (`zig/variables.rs:16-17`). A `var x = constants.MAX` is misclassified as `Constant`. Should rely on the AST node kind alone.
- **Many type-discriminator branches use `node_text.contains(...)`**: `"union(enum)"`, `"packed struct"`, `"extern struct"`, `"error{"` all use string searching (`zig/variables.rs:53, 161-167, 199-203`). False positives are likely when these tokens appear in *comments* or *string literals* inside the same node text.
- **Method calls drop call relationships**: `extract_function_call_relationships` filters callers with `symbol.kind == SymbolKind::Function` (`zig/relationships.rs:185`), missing every call made *from inside a method*. Methods are first-class call sites.
- **Return type lookup returns parameter types**: `extract_function_signature` does `find_child_by_type(&node, "type_expression")` for the return type (`zig/functions.rs:212-220`). If `find_child_by_type` traverses into the parameters list (which contains `type_expression` nodes), the return type ends up being the *first* parameter's type. Need to scope to the right-of-`)` region or use a `return_type` field if the grammar provides one.
- **Error sets mapped to `SymbolKind::Enum`** (`zig/types.rs:188-216`, `zig/variables.rs:264-312`). Zig error sets are conceptually distinct (closer to a type-level set). Better mapping is `SymbolKind::Type` with `metadata["isErrorSet"] = true`. Note: `extract_error_set_assignment` already sets this metadata, but the kind mismatch makes downstream filtering fragile.
- **`extract_test` sets the symbol *name* to the string content of the test** (`zig/functions.rs:80-87`). If the test name contains spaces or special chars (`test "all sorts of edge: cases"`) the name becomes a search-unfriendly token.
- **Compound declarations in ERROR nodes**: only one partial-generic regex is recognized (`zig/error_handling.rs:8-49`); incomplete struct/union/enum bodies in editor-state code aren't recovered.
- **`is_inside_struct` walks all the way up the tree** (`zig/helpers.rs:53-67`) — meaning a `fn` defined at file-scope but nested inside an unrelated `container_declaration` higher up is mistakenly classified as Method.

### VB.NET
**Status**: Good (with caveats)

**Strengths**
- Comprehensive symbol kinds: namespaces, classes, modules, structures, interfaces, enums, delegates, methods, constructors, properties, fields, events, operators, constants, declares (`vbnet/mod.rs:83-107`).
- Good signature builders with modifier prefixes, type parameters, inheritance, implements clauses (`vbnet/types.rs:91-105` for class).
- VB.NET-specific visibility metadata (`Public`/`Private`/`Friend`/`Protected Friend`) preserved via `vb_visibility_metadata`.
- Generic `Imports` aliasing handled (`vbnet/types.rs:36-57`).
- Custom `Declare` statements (P/Invoke) produce symbols.

**Gaps & Errors**
- **Modules mapped to `SymbolKind::Class`** (`vbnet/types.rs:150`). VB.NET `Module` is its own concept (static-like, no instances). The framework defines `SymbolKind::Module` — it should be used here. Right now consumers can't distinguish a module from a class without inspecting metadata.
- **Case-insensitive identity not honored**: VB.NET treats `Foo` and `foo` as the same identifier. `vbnet/identifiers.rs` extracts raw-cased names, and the `symbol_map` lookup in relationships does case-sensitive matching. Renaming uses break: `Public.Foo()` and `public.foo()` resolve as different symbols.
- **`extract_field` only takes the first `variable_declarator`** (`vbnet/members.rs:156-160`). VB allows `Dim x, y, z As Integer` and `Public a As Integer, b As String` — only `x`/`a` becomes a symbol.
- **`Dim` keyword used in field signatures** (`vbnet/members.rs:164`). Inside a class body, fields aren't `Dim`; they're `Public/Private/Friend`. Generated signatures look unidiomatic and misleading.
- **No Optional / ByRef / ByVal parameter modifiers in signatures**: `extract_parameters` (in `vbnet/helpers.rs`) returns the raw tree-sitter parameter text; if the grammar exposes them as separate child nodes, the modifiers may be lost.
- **`relationships.rs` is 579 lines** — over the 500-line target.
- **Operator name `"operator +"`** (`vbnet/members.rs:233`) — useful but uses a space, which clashes with most search tokenizers. `op_Addition` (the CLR convention) might be a better lookup-friendly name.
- **`extract_event` doesn't capture event handler signature**: a `Public Event Click(sender As Object, e As EventArgs)` becomes `"Public Event Click As ?"` where the `As` clause is the *type* (a delegate), but the parameter list is not surfaced. Useful for find_references on `sender`/`e`.

## Cross-Cutting Patterns

1. **Call-site-as-definition antipattern (Bash + PowerShell)**. Both extractors create Function symbols when they encounter `docker`, `kubectl`, `Connect-AzAccount`, etc. These should be tracked as identifiers (call sites) and as relationships (calls, possibly cross-language) — never as definitions. The current design pollutes search results with phantom symbols and breaks every "find definition" query.

2. **Signature bloat (Lua + GDScript)**. `let signature = base.get_node_text(&node)` for a function captures the whole body. Both Zig and Bash demonstrate the right pattern: build a synthetic `name(params) -> RetType` string. Lua and GDScript should follow.

3. **`node_text.contains(...)` discriminators (Zig prominently, also Lua/GDScript)**. Substring tests against the entire node text fail when the looked-for token appears in a string or comment within the node. Prefer AST-node-kind checks.

4. **Tautological tests (R)**. `assert!(symbols.len() >= 0)` for S4/R6 features means broken extraction passes the test. R tests for setClass/setMethod/R6Class should assert specific Class/Method symbols with the right names.

5. **Missing class systems (R)**. R has *four* class systems (S3, S4, R6, Reference). Only S3 is detected. This is a single-language gap but it's a multi-system gap.

6. **Missing doc comments (R, parts of PowerShell config)**. R has Roxygen (`#'`) registered but never extracted. PowerShell's spec has `EMPTY` doc styles even though a custom extractor exists — anything outside the PS module sees no docs.

7. **Caller-kind filtering drops methods (Zig)**. Filtering call relationships to `SymbolKind::Function` only excludes methods as caller sites. Anywhere in this audit where call extraction filters on kind, methods should be included.

8. **Inconsistent local naming (Bash)**. `commands.rs`'s cross-language list and `relationships.rs`'s builtin list overlap and disagree. The same name is "interesting enough to symbolize" and "boring enough to ignore." Consolidate.

## Top 10 Highest-Impact Findings (ranked)

1. **R has zero detection of S4 classes, S4 methods, S4 generics, R6 classes, or Reference classes.** All four R class systems beyond S3 produce no Class/Method symbols. Tests assert `>= 0` so the gap is invisible. (`r/mod.rs:182-249`, `tests/r/classes.rs:117/140`)

2. **R never extracts Roxygen doc comments.** `R_DOCS` is wired in the language spec but the extractor never calls `find_doc_comment`. Every R symbol has `doc_comment: None`. (`r/mod.rs` whole file)

3. **Bash and PowerShell create Function symbols for call sites of well-known external commands** (docker, kubectl, Connect-AzAccount, Set-AzContext, ...). This pollutes the symbol index with phantom definitions and is internally inconsistent with the relationship code that treats those same names as builtins. (`bash/commands.rs:11-67`, `powershell/commands.rs:11-92`)

4. **PowerShell method signatures are malformed**: `"{}{} {}()"` with `static`, return type, and an *always-empty* parameter list. Methods with parameters render with `()`. Consumers see misleading signatures. (`powershell/classes.rs:188-198`)

5. **Lua and GDScript stuff entire function bodies into `signature`.** A 100-line function gets a 100-line `signature`. Massive token waste in any tool that returns symbol metadata. (`lua/functions.rs:81/137`, `gdscript/functions.rs:92`, `gdscript/variables.rs:41`)

6. **PowerShell's `is_builtin_cmdlet` knows two cmdlets** (`Write-Output`, `Get-ChildItem`). Every other standard cmdlet generates a pending relationship that will never resolve. Hundreds of false-positive pending edges per script. (`powershell/relationships.rs:100-102`)

7. **Zig drops call relationships from inside methods.** Caller filter is `SymbolKind::Function`; methods are excluded. Half the call graph in any zig file with structs is missing. (`zig/relationships.rs:185`)

8. **GDScript identifier extraction stores `self.do_thing` instead of `do_thing` as the call name**. When a `call` node's child is `attribute`, the entire dotted text is used as the identifier name. find_references against method names will fail consistently. (`gdscript/identifiers.rs:51-71`)

9. **VB.NET ignores its own case-insensitivity.** Identifier matching is case-sensitive; `Foo` and `foo` resolve to different symbols. Rename and find_references break for users who type a different case than the definition. (`vbnet/identifiers.rs` plus relationships)

10. **PowerShell's language spec has `EMPTY` doc comment styles** (`language_spec/specs.rs:226`). The extractor compensates locally but anything that goes through `LanguageSpec::is_doc_comment` (cross-cutting code, future generic flows) gets nothing for PS. The spec should declare the actual `<#` block and `#` line styles.

## Notable secondary issues (not in top 10 but worth tracking)

- **VB.NET `Module` mapped to `Class`** instead of `SymbolKind::Module`.
- **Lua `_M = {}` (canonical public module return) marked Private** because of underscore convention.
- **R `mod.rs` is 523 lines**, VB.NET `relationships.rs` is 579 — over the 500-line file-size target.
- **Bash `extract_declaration` only extracts the first variable in `export A=1 B=2`** — partial data.
- **Zig error sets mapped to `Enum`** rather than `Type` despite already setting `metadata["isErrorSet"]`.
- **GDScript `get_node` special-case in identifiers** appears to be dead code (no such tree-sitter node kind in current grammar).
- **Lua method paths longer than two parts (`obj.sub.method`) silently dropped.**

## File path index (absolute)

- `/Users/murphy/source/julie/crates/julie-extractors/src/bash/{mod,commands,functions,helpers,relationships,signatures,types,variables}.rs`
- `/Users/murphy/source/julie/crates/julie-extractors/src/powershell/{mod,classes,commands,documentation,functions,helpers,identifiers,imports,relationships,types,variables}.rs`
- `/Users/murphy/source/julie/crates/julie-extractors/src/lua/{mod,classes,core,functions,helpers,identifiers,relationships,tables,variables}.rs`
- `/Users/murphy/source/julie/crates/julie-extractors/src/r/{mod,identifiers,relationships}.rs`
- `/Users/murphy/source/julie/crates/julie-extractors/src/gdscript/{mod,classes,enums,functions,helpers,identifiers,relationships,signals,types,variables}.rs`
- `/Users/murphy/source/julie/crates/julie-extractors/src/zig/{mod,error_handling,functions,helpers,identifiers,relationships,type_inference,types,variables}.rs`
- `/Users/murphy/source/julie/crates/julie-extractors/src/vbnet/{mod,helpers,identifiers,members,relationships,type_inference,types}.rs`
- `/Users/murphy/source/julie/crates/julie-extractors/src/language_spec/{mod,specs}.rs` — central language registry; PowerShell `EMPTY` doc styles bug at `specs.rs:226`.
