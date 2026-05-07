# Tree-Sitter Extractor Audit Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use razorback:subagent-driven-development when subagent delegation is available. Fall back to razorback:executing-plans for single-task, tightly-sequential, or no-delegation runs.

**Goal:** Fix the verified tree-sitter extractor audit findings from `docs/findings/COMPILED-FINDINGS.md`, then lock the corrected behavior into exact semantic tests and an honest capability matrix.

**Architecture:** Treat this as an extractor correctness program, not a rewrite. Fix high-risk shared contracts first, then run disjoint language lanes with TDD. Every worker-owned test must prove exact symbol names, kinds, relationship endpoints, pending-target payloads, or type IDs, not merely that extraction produced something.

**Tech Stack:** Rust, tree-sitter extractors, Julie extractor capability fixtures, cargo nextest, cargo xtask test buckets, Razorback subagent execution.

---

## Source Inputs

- Primary findings file: `docs/findings/COMPILED-FINDINGS.md`
- Historical fixed batch: `docs/plans/2026-05-05-treesitter-review-fixes.md`
- Verification source of truth: `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, `docs/plans/verification-ledger-template.md`
- Razorback routing source of truth: `RAZORBACK.md`

The 2026-05-05 plan is historical context from prior work that has already been fixed in this branch history. Do not reopen those items from the old plan unless `docs/findings/COMPILED-FINDINGS.md` still names the defect and a new failing test reproduces it at current HEAD.

## Execution Shape

Run this as a multi-wave Razorback plan:

1. Strategy lanes first: shared identity, relationship resolution, parent IDs, parser failure behavior, doc-comment semantics, core enums, and capability-matrix policy.
2. Worker lanes next: language-specific extractor fixes with disjoint write scopes.
3. Lead gates after coherent batches: `cargo xtask test changed`, then extractor and branch gates.
4. Capability matrix last in each wave: update `fixtures/extraction/capabilities.json` only after code and tests establish which gaps remain.

Workers must use Julie MCP tools before touching code:

- `get_context(query='<language or subsystem>')` for orientation.
- `deep_dive(symbol='<symbol to modify>')` before modifying a symbol.
- `fast_refs(symbol='<public API or shared symbol>')` before changing shared contracts.
- `get_symbols(file_path='<file>')` before reading large files.

## Current Status

- Task 10 is completed and committed as `e75facd1 fix(extractors): cover identifier and call gaps`.
- Task 11 is completed and committed as `57f59b74 fix(extractors): preserve structured pending targets`.
- Task 12 is completed and committed as `6594c4ac fix(extractors): support embedded vue html css razor`.
- Task 13 is completed and committed as `cf6c9bae fix(extractors): clean up scripting language extraction`.
- Task 14 is completed in the current working tree. Verification passed, and the Task 14 ledger records focused exact tests, golden refresh, changed-file, and one dev gate.
- Task 15 is the next unstarted task.

## Finding Coverage

- Task 1: `C1`, `H21` SQL part, `H25`, `H26` SQL part, `M9`, `M10`, `M27`.
- Task 2: `C2`, `M20`, `M21`, `M22`, `M31`, `M35`, `L2`, `L7`.
- Task 3: `C7`, `M3`, `M4`, `M5`, `M29`.
- Task 4: `C6`, `H24`, `H30`, `H37`.
- Task 5: `C4`, `C5`, Ruby part of `M32`.
- Task 6: `C9`, `C10`, `H13`, `H16`, `H32`, `H34`, `M14`, `M36`.
- Task 7: `C8`, `M1`, `CC2`, `L3`, `L4`.
- Task 8: `C11`, `C12`, `H2`, `H12`, `M19`, `M41`.
- Task 9: `H1`, `H14`, `H15`, JavaScript and TypeScript parts of `H18`, `CC4`.
- Task 10: `H17`, remaining `H18`, `M11`, `M12`, `M13`, `M17`, `M24`, `M43`.
- Task 11: `C3`, cross-file part of `C4`, `H33`, `H36`, `M15`, `M16`.
- Task 12: `H3`, `H4`, `H19`, `H20`, `H21` HTML/Razor part, `H22`, `H31`, `M30`, `M33`, `M34`, `M42`.
- Task 13: `H5`, `H6`, `H7`, `H8`, `H9`, scripting part of `H30`, `M23`, `M26`, scripting part of `M32`, `M39`, `M40`.
- Task 14: `H10`, `H11`, `H23`, `M18`, `M37`, `L5`.
- Task 15: `H27`, `H28`, `H29`, `M2`, `M7`, `M38`, `M44`, `L1`, `L6`, `L8`.
- Task 16: `CC1`, `CC3`, `CC5`, `CC6`, `CC7`, `L9`, `L10`.
- Task 17: `H35`.

Reviewed-but-not-promoted claims from the compiled document are not task drivers unless a worker touches that exact code path and finds a reproducible defect.

## File Map

### Shared Contracts

- Modify `crates/julie-extractors/src/base/types.rs` for enum, `TypeInfo`, `Visibility`, `RelationshipKind`, and conversion behavior.
- Modify `crates/julie-extractors/src/base/extractor.rs` and `crates/julie-extractors/src/base/creation_methods.rs` for ID generation, relationship IDs, symbol creation, and visibility fallback behavior.
- Modify `crates/julie-extractors/src/base/relationship_resolution.rs` for `ScopedSymbolIndex` overload and ambiguity behavior.
- Modify `crates/julie-extractors/src/base/results_normalization.rs` for `ExtractionResults::extend` type-info merge behavior.
- Modify `crates/julie-extractors/src/factory.rs` when `TypeInfo` contract changes need normalized output.
- Modify `crates/julie-extractors/src/pipeline.rs` and `crates/julie-extractors/src/manager.rs` for degraded parse failure behavior.
- Modify `crates/julie-extractors/src/language_spec/mod.rs` and `crates/julie-extractors/src/language_spec/specs.rs` for doc-comment styles, `.h` routing, and capability profiles.
- Modify `fixtures/extraction/capabilities.json` after behavior changes establish remaining gaps.
- Modify `crates/julie-extractors/src/tests/capability_matrix.rs`, `crates/julie-extractors/src/tests/golden.rs`, `crates/julie-extractors/src/tests/doc_comments.rs`, and add `crates/julie-extractors/src/tests/type_invariants.rs` if no existing test file fits the shared ID and TypeInfo invariants.

### Language Lanes

- SQL: `crates/julie-extractors/src/sql/relationships.rs`, `crates/julie-extractors/src/sql/schemas.rs`, `crates/julie-extractors/src/tests/sql/relationships.rs`.
- JSON/TOML/YAML/Markdown: `crates/julie-extractors/src/json/mod.rs`, `crates/julie-extractors/src/toml/mod.rs`, `crates/julie-extractors/src/yaml/mod.rs`, `crates/julie-extractors/src/yaml/relationships.rs`, `crates/julie-extractors/src/markdown/mod.rs`, `crates/julie-extractors/src/markdown/relationships.rs`, plus matching test modules.
- Java/C#/VB.NET/Go/Bash/Python multi-binding: `crates/julie-extractors/src/java/fields.rs`, `crates/julie-extractors/src/csharp/members.rs`, `crates/julie-extractors/src/vbnet/members.rs`, `crates/julie-extractors/src/go/specs.rs`, `crates/julie-extractors/src/bash/variables.rs`, `crates/julie-extractors/src/python/imports.rs`, plus matching tests.
- Ruby: `crates/julie-extractors/src/ruby/calls.rs`, `crates/julie-extractors/src/ruby/relationships.rs`, `crates/julie-extractors/src/tests/ruby/mod.rs`, `crates/julie-extractors/src/tests/ruby/cross_file_relationships.rs`.
- C/C++: `crates/julie-extractors/src/c/relationships.rs`, `crates/julie-extractors/src/cpp/identifiers.rs`, `crates/julie-extractors/src/cpp/relationships.rs`, `crates/julie-extractors/src/cpp/types.rs`, `crates/julie-extractors/src/cpp/visibility.rs`, and matching tests.
- Dart/R/Elixir: `crates/julie-extractors/src/dart/mod.rs`, `crates/julie-extractors/src/dart/functions.rs`, `crates/julie-extractors/src/dart/members.rs`, `crates/julie-extractors/src/dart/relationships.rs`, `crates/julie-extractors/src/r/mod.rs`, `crates/julie-extractors/src/r/relationships.rs`, `crates/julie-extractors/src/elixir/relationships.rs`, and matching tests.
- TypeScript/JavaScript: `crates/julie-extractors/src/typescript/classes.rs`, `crates/julie-extractors/src/typescript/functions.rs`, `crates/julie-extractors/src/typescript/interfaces.rs`, `crates/julie-extractors/src/typescript/imports_exports.rs`, `crates/julie-extractors/src/typescript/identifiers.rs`, `crates/julie-extractors/src/javascript/identifiers.rs`, and matching tests.
- Vue/HTML/CSS/Razor: `crates/julie-extractors/src/vue/script.rs`, `crates/julie-extractors/src/vue/identifiers.rs`, `crates/julie-extractors/src/vue/style.rs`, `crates/julie-extractors/src/html/relationships.rs`, `crates/julie-extractors/src/html/scripts.rs`, `crates/julie-extractors/src/css/at_rules.rs`, `crates/julie-extractors/src/css/identifiers.rs`, `crates/julie-extractors/src/razor/relationships.rs`, and matching tests.
- Scripting: `crates/julie-extractors/src/powershell/relationships.rs`, `crates/julie-extractors/src/powershell/classes.rs`, `crates/julie-extractors/src/powershell/commands.rs`, `crates/julie-extractors/src/bash/commands.rs`, `crates/julie-extractors/src/bash/relationships.rs`, `crates/julie-extractors/src/lua/functions.rs`, `crates/julie-extractors/src/lua/helpers.rs`, `crates/julie-extractors/src/lua/relationships.rs`, `crates/julie-extractors/src/gdscript/functions.rs`, `crates/julie-extractors/src/gdscript/identifiers.rs`, `crates/julie-extractors/src/gdscript/relationships.rs`, and matching tests.
- JVM, Swift, Scala, PHP, QML, Zig, Rust, Regex: matching language modules under `crates/julie-extractors/src/{java,csharp,swift,kotlin,scala,php,qml,zig,rust,regex}/` and tests under `crates/julie-extractors/src/tests/`.

## Task 1: SQL Graph Correctness

**Files:**
- Modify `crates/julie-extractors/src/sql/relationships.rs`
- Modify `crates/julie-extractors/src/sql/schemas.rs`
- Modify `crates/julie-extractors/src/language_spec/specs.rs`
- Test `crates/julie-extractors/src/tests/sql/relationships.rs`
- Update `fixtures/extraction/capabilities.json` after the SQL behavior is proved

**What to build:** Fix SQL relationship endpoints, line numbers, view and trigger table dependencies, and pending-capability claims. SQL must stop emitting self-edges and dead synthetic IDs.

**Approach:** Start with `extract_join_relationships`. Track the enclosing `FROM` table and emit `Uses` from that source table to each joined table. Use the AST where possible for `CREATE VIEW` and `CREATE TRIGGER`; keep regex only as a fallback with `LazyLock<Regex>`. Move SQL from `NO_PENDING_CAPABILITIES` only if real structured pending relationships are emitted for unresolved references.

**Proposed tests:**
- `test_sql_join_relationship_links_from_table_to_joined_table`
- `test_sql_join_relationship_does_not_emit_self_edge`
- `test_sql_view_and_trigger_relationships_target_real_tables`
- `test_sql_relationship_lines_are_one_based`
- `test_sql_pending_relationships_do_not_use_dead_synthetic_ids`

**Acceptance criteria:**
- JOIN relationships assert exact `from_symbol_id`, `to_symbol_id`, kind, and count.
- View and trigger tests assert exact table targets.
- SQL relationship line numbers match the project 1-based convention.
- No SQL relationship target ID is a dead placeholder such as `external_users`.
- Worker verification uses exact tests only.

## Task 2: Data Format Extraction Correctness

**Files:**
- Modify `crates/julie-extractors/src/json/mod.rs`
- Modify `crates/julie-extractors/src/toml/mod.rs`
- Modify `crates/julie-extractors/src/yaml/mod.rs`
- Modify `crates/julie-extractors/src/yaml/relationships.rs`
- Modify `crates/julie-extractors/src/markdown/mod.rs`
- Modify `crates/julie-extractors/src/markdown/relationships.rs`
- Test `crates/julie-extractors/src/tests/json/mod.rs`
- Test `crates/julie-extractors/src/tests/toml/mod.rs`
- Test `crates/julie-extractors/src/tests/yaml/mod.rs`
- Test `crates/julie-extractors/src/tests/markdown/mod.rs`
- Test `crates/julie-extractors/src/tests/markdown/relationships.rs`

**What to build:** Fix JSON UTF-8 truncation, add tested data-format relationships where the compiled findings call for them, and remove small but real Markdown/YAML data corruption.

**Approach:** Replace byte slicing in JSON doc-comment truncation with `chars().take(2000).collect::<String>()`. For YAML, resolve anchors by exact anchor name, not substring. For Markdown, preserve heading levels in metadata, extract links and footnotes, extract fenced code blocks with language metadata, and strip only the Markdown heading marker prefix in fallback heading text. JSON Schema `$ref`, TOML dependency-like references, and YAML anchors/tags should be modeled only with exact assertions and capability updates.

**Proposed tests:**
- `test_json_long_multibyte_string_doc_comment_truncates_without_panic`
- `test_yaml_alias_resolves_exact_anchor_not_prefix_match`
- `test_markdown_heading_level_is_preserved_in_metadata`
- `test_markdown_heading_fallback_preserves_csharp_heading_text`
- `test_markdown_links_footnotes_and_code_blocks_are_extracted`
- `test_data_format_relationships_have_exact_targets_or_no_edge`

**Acceptance criteria:**
- JSON extraction cannot panic when a multi-byte UTF-8 character straddles byte 2000.
- YAML `*foo` resolves only to `&foo`, not `&foobar`.
- Markdown heading text keeps meaningful leading content such as `C# Programming`.
- New data-format relationships have exact targets, or no relationship is emitted.

## Task 3: Identity And TypeInfo Contracts

**Files:**
- Modify `crates/julie-extractors/src/base/types.rs`
- Modify `crates/julie-extractors/src/base/extractor.rs`
- Modify `crates/julie-extractors/src/base/creation_methods.rs`
- Modify `crates/julie-extractors/src/base/results_normalization.rs`
- Modify `crates/julie-extractors/src/factory.rs`
- Modify `crates/julie-extractors/src/zig/type_inference.rs`
- Modify `crates/julie-extractors/src/dart/mod.rs`
- Test `crates/julie-extractors/src/tests/zig/types.rs`
- Test `crates/julie-extractors/src/tests/dart/types.rs`
- Add or modify `crates/julie-extractors/src/tests/type_invariants.rs`

**What to build:** Make IDs and type rows trustworthy. TypeInfo keys must be real `Symbol.id` values, generated symbol IDs need enough entropy, relationship IDs must not collide on same-line calls, and result merging must not silently overwrite type info.

**Approach:** This is a strategy or coupled implementation lane. Decide the shared `TypeInfo` representation first, including return and parameter type fields if they are added. Then update Zig and Dart to key by `symbol.id.clone()`. Replace MD5 column-only ID input with a hash over file path, name, start byte, and end byte. Include start column or a per-line counter in relationship IDs. Make `ExtractionResults::extend` merge type info deterministically or fail loudly on conflicting duplicate keys.

**Proposed tests:**
- `test_zig_typeinfo_keys_are_symbol_ids`
- `test_dart_typeinfo_keys_are_symbol_ids`
- `test_typeinfo_same_name_symbols_keep_distinct_rows`
- `test_symbol_ids_do_not_collide_for_same_row_column_different_spans`
- `test_relationship_ids_do_not_collide_for_multiple_calls_on_one_line`
- `test_extraction_results_extend_does_not_silently_overwrite_typeinfo`

**Acceptance criteria:**
- Every `TypeInfo.symbol_id` in extractor results matches an existing symbol ID.
- Same-name symbols in one file can have distinct type rows.
- Same-line calls produce distinct relationship IDs.
- Type-info merge conflicts are tested and handled intentionally.

## Task 4: Multi-Declarator And Multi-Binding Extraction

**Files:**
- Modify `crates/julie-extractors/src/java/fields.rs`
- Modify `crates/julie-extractors/src/java/classes.rs`
- Modify `crates/julie-extractors/src/csharp/members.rs`
- Modify `crates/julie-extractors/src/vbnet/members.rs`
- Modify `crates/julie-extractors/src/go/specs.rs`
- Modify `crates/julie-extractors/src/bash/variables.rs`
- Modify `crates/julie-extractors/src/python/imports.rs`
- Test `crates/julie-extractors/src/tests/java/class_tests.rs`
- Test `crates/julie-extractors/src/tests/java/modern_java_tests.rs`
- Test `crates/julie-extractors/src/tests/csharp/core.rs`
- Test `crates/julie-extractors/src/tests/vbnet/members.rs`
- Test `crates/julie-extractors/src/tests/go/types.rs`
- Test `crates/julie-extractors/src/tests/bash/mod.rs`
- Test `crates/julie-extractors/src/tests/python/imports.rs`

**What to build:** Any syntax node that declares multiple names must emit multiple symbols or imports. This covers Java, C#, VB.NET, Go, Bash, Python imports, and Java record components.

**Approach:** Where extraction helpers currently return `Option<Symbol>`, add an accumulator or a vector-returning helper at the smallest language-local call site. Do not force a shared API change unless the language visitor already needs one. Java records should emit `Property` children for record components.

**Proposed tests:**
- `test_java_field_multi_declarator_emits_all_fields`
- `test_csharp_field_and_event_multi_declarator_emits_all_members`
- `test_vbnet_dim_multi_declarator_emits_all_fields`
- `test_go_var_and_const_multi_name_specs_emit_all_symbols`
- `test_bash_export_declaration_emits_all_variables`
- `test_python_plain_import_statement_emits_every_binding`
- `test_java_record_components_emit_property_symbols`

**Acceptance criteria:**
- Tests assert exact symbol names and kinds for every declared name.
- No language loses parent IDs when changing one-symbol helpers into multi-symbol extraction.
- Existing single-declarator behavior stays unchanged.

## Task 5: Ruby Properties And Cross-File Relationships

**Files:**
- Modify `crates/julie-extractors/src/ruby/calls.rs`
- Modify `crates/julie-extractors/src/ruby/relationships.rs`
- Modify `crates/julie-extractors/src/test_detection.rs`
- Test `crates/julie-extractors/src/tests/ruby/mod.rs`
- Test `crates/julie-extractors/src/tests/ruby/cross_file_relationships.rs`
- Test `crates/julie-extractors/src/tests/ruby/identifiers.rs`

**What to build:** Ruby must emit every property in `attr_accessor`, `attr_reader`, and `attr_writer`, preserve the class parent ID, and emit structured pending relationships for cross-file inheritance and module inclusion.

**Approach:** Change the attr helper path to emit one `Property` per symbol argument. If the caller currently expects one optional symbol, use a local accumulator instead of hiding extra symbols. For relationships, resolve same-file targets as today and emit structured pending targets when the superclass, included module, or extended module is not local. Extend Ruby test framework detection for RSpec shapes.

**Proposed tests:**
- `test_ruby_attr_accessor_emits_all_property_symbols`
- `test_ruby_attr_reader_writer_preserve_class_parent_id`
- `test_ruby_cross_file_inheritance_emits_structured_pending_relationship`
- `test_ruby_include_and_extend_emit_structured_pending_relationships`
- `test_ruby_rspec_blocks_are_marked_as_tests`

**Acceptance criteria:**
- `attr_accessor :a, :b, :c` emits exactly `a`, `b`, and `c` as properties.
- Cross-file `ApplicationController` and concern targets do not disappear.
- Structured pending targets preserve terminal name and useful namespace or receiver context.

## Task 6: C And C++ Relationship Precision

**Files:**
- Modify `crates/julie-extractors/src/c/relationships.rs`
- Modify `crates/julie-extractors/src/cpp/identifiers.rs`
- Modify `crates/julie-extractors/src/cpp/relationships.rs`
- Modify `crates/julie-extractors/src/cpp/types.rs`
- Modify `crates/julie-extractors/src/cpp/visibility.rs`
- Modify `crates/julie-extractors/src/language_spec/specs.rs`
- Test `crates/julie-extractors/src/tests/c/relationships.rs`
- Test `crates/julie-extractors/src/tests/cpp/identifier_extraction.rs`
- Test `crates/julie-extractors/src/tests/cpp/cross_file_relationships.rs`
- Test `crates/julie-extractors/src/tests/cpp/type_usage.rs`
- Test `crates/julie-extractors/src/tests/cpp/classes.rs`
- Test `crates/julie-extractors/src/tests/cpp/modern.rs`

**What to build:** Fix C++ call identifier names, C++ overload relationship resolution, nested type visibility, `.h` routing, C/C++ type-use relationships, indirect C calls, and C++20 concepts.

**Approach:** First fix `obj.method()` identifier extraction to store `method`. For C++ relationship resolution, stop relying on a uniqueness-only name map. Resolve callers by node containment and resolve inheritance targets by kind-filtered candidates. `.h` handling must be content-aware or project-aware; do not route every `.h` file to C++. For type-use relationships, walk declaration, parameter, field, and return-type nodes and emit `Uses` to a local symbol or a structured pending target.

**Proposed tests:**
- `test_cpp_field_expression_call_identifier_uses_field_name`
- `test_cpp_overloaded_methods_keep_call_relationships`
- `test_cpp_constructor_name_collision_does_not_drop_inheritance`
- `test_cpp_nested_type_visibility_uses_parent_default`
- `test_h_header_with_cpp_syntax_routes_to_cpp_extractor`
- `test_h_header_with_c_syntax_stays_c_extractor`
- `test_c_and_cpp_declarations_emit_type_use_relationships`
- `test_cpp20_concept_definition_is_extracted`
- `test_c_indirect_call_emits_low_confidence_pending_relationship`

**Acceptance criteria:**
- `find_references("method")` style identifier data can find C++ method calls.
- Overloads and constructors do not disappear from relationships just because names collide.
- `.h` routing handles both C and C++ examples with explicit tests.
- C/C++ type coupling is visible in relationships.

## Task 7: Visibility And Doc-Comment Semantics

**Files:**
- Modify `crates/julie-extractors/src/base/creation_methods.rs`
- Modify `crates/julie-extractors/src/base/types.rs`
- Modify `crates/julie-extractors/src/language_spec/mod.rs`
- Modify `crates/julie-extractors/src/language_spec/specs.rs`
- Modify `crates/julie-extractors/src/rust/helpers.rs`
- Modify `crates/julie-extractors/src/rust/functions.rs`
- Modify `crates/julie-extractors/src/swift/signatures.rs`
- Test `crates/julie-extractors/src/tests/doc_comments.rs`
- Test `crates/julie-extractors/src/tests/rust/helpers.rs`
- Test `crates/julie-extractors/src/tests/swift/mod.rs`

**What to build:** Remove the base visibility text fallback and split broad line-comment matching from real doc markers. Add finer visibility values where the project chooses to model them.

**Approach:** Make `BaseExtractor::extract_visibility` stop scanning the whole node body for `public`, `private`, and `protected`. Languages must use language-specific helpers. Split doc comment styles into "any preceding comment" and "doc marker" forms so Bash, Ruby, R, Lua, SQL, and PowerShell do not inherit Go-like behavior by accident. If `Visibility::Open`, `Internal`, or `FilePrivate` are added, update conversions and tests in the same task.

**Proposed tests:**
- `test_base_visibility_does_not_read_method_body_text`
- `test_go_all_preceding_comments_can_remain_docs`
- `test_r_roxygen_comment_is_doc_but_plain_hash_comment_is_not`
- `test_lua_block_doc_precedence_over_line_comment`
- `test_rust_inner_doc_comments_are_extracted`
- `test_swift_open_visibility_is_not_flattened_without_metadata`

**Acceptance criteria:**
- A method returning `"public key"` is not classified public by base logic.
- Each affected language has explicit doc-comment behavior.
- Visibility conversions either preserve richer values or store exact literals in metadata.

## Task 8: Dart, R, And Elixir Documentation And Idioms

**Files:**
- Modify `crates/julie-extractors/src/dart/mod.rs`
- Modify `crates/julie-extractors/src/dart/functions.rs`
- Modify `crates/julie-extractors/src/dart/members.rs`
- Modify `crates/julie-extractors/src/r/mod.rs`
- Modify `crates/julie-extractors/src/r/relationships.rs`
- Modify `crates/julie-extractors/src/elixir/relationships.rs`
- Modify Elixir symbol extraction modules under `crates/julie-extractors/src/elixir/`
- Test `crates/julie-extractors/src/tests/dart/mod.rs`
- Test `crates/julie-extractors/src/tests/dart/types.rs`
- Test `crates/julie-extractors/src/tests/r/classes.rs`
- Test `crates/julie-extractors/src/tests/r/functions.rs`
- Test `crates/julie-extractors/src/tests/r/relationships.rs`
- Test `crates/julie-extractors/src/tests/elixir/mod.rs`

**What to build:** Dart and R must actually consume their configured doc-comment styles. R must extract S4, R6, reference classes, replacement functions, and `source()` imports. Elixir must extract `@doc`, `@moduledoc`, and definition forms currently skipped.

**Approach:** Add `base.find_doc_comment(&node)` at every Dart symbol creation path and R symbol creation path. Replace tautological R class tests with exact assertions. Add R handlers for `setClass`, `setGeneric`, `setMethod`, `R6Class`, `setRefClass`, replacement functions, and `source()`. For Elixir, either add a doc-comment style that understands `@doc`/`@moduledoc` AST shapes or extract adjacent doc attributes in Elixir-specific code. Add `defguard`, `defdelegate`, `defexception`, and `defoverridable` symbol handling where `is_definition_keyword` already knows they are definitions.

**Proposed tests:**
- `test_dart_doc_comment_attaches_to_classes_methods_and_fields`
- `test_r_roxygen_comment_attaches_to_function_and_class_symbols`
- `test_r_s4_class_and_method_have_exact_symbols`
- `test_r_r6_class_members_have_exact_symbols`
- `test_r_source_call_emits_import_and_pending_relationship`
- `test_elixir_doc_and_moduledoc_attach_to_symbols`
- `test_elixir_defguard_defdelegate_defexception_are_extracted`

**Acceptance criteria:**
- Dart and R doc comment tests assert exact doc text.
- R class tests contain no `>= 0` or non-semantic assertions.
- Elixir definition forms are not skipped as identifiers without becoming symbols.

## Task 9: TypeScript And JavaScript Structure

**Files:**
- Modify `crates/julie-extractors/src/typescript/classes.rs`
- Modify `crates/julie-extractors/src/typescript/functions.rs`
- Modify `crates/julie-extractors/src/typescript/interfaces.rs`
- Modify `crates/julie-extractors/src/typescript/imports_exports.rs`
- Modify `crates/julie-extractors/src/typescript/identifiers.rs`
- Modify `crates/julie-extractors/src/javascript/identifiers.rs`
- Test `crates/julie-extractors/src/tests/typescript/relationships.rs`
- Test `crates/julie-extractors/src/tests/typescript/identifiers.rs`
- Test `crates/julie-extractors/src/tests/typescript/functions.rs`
- Test `crates/julie-extractors/src/tests/typescript/types.rs`
- Test `crates/julie-extractors/src/tests/javascript/relationships.rs`
- Test `crates/julie-extractors/src/tests/javascript/identifier_extraction.rs`

**What to build:** Standardize TypeScript parent ID propagation, remove ID-regeneration workarounds, extract JSX/TSX component identifiers, port binding-level imports from JavaScript to TypeScript, and capture constructor calls.

**Approach:** Strategy lane first for `parent_id`: choose walker-threaded parent IDs and remove the `find_parent_class_id` regeneration path once tests prove nested classes, namespaces, and interfaces work. TypeScript imports should produce one symbol per local binding with module source metadata. JSX/TSX should emit component identifiers from `jsx_self_closing_element`, `jsx_opening_element`, and fragments when there is a real component name. JavaScript and TypeScript `new` expressions should emit `IdentifierKind::Call`.

**Proposed tests:**
- `test_typescript_nested_class_parent_id_is_threaded`
- `test_typescript_interface_method_parent_id_is_threaded`
- `test_typescript_import_named_alias_creates_binding_symbol`
- `test_tsx_component_usage_emits_identifier_reference`
- `test_javascript_new_expression_emits_constructor_call_identifier`
- `test_typescript_new_expression_emits_constructor_call_identifier`

**Acceptance criteria:**
- TypeScript extractors never recompute parent IDs from generated row and column guesses.
- Imported TypeScript bindings are searchable by local name.
- JSX/TSX and constructor identifiers are present with exact names and containing symbol IDs where applicable.

## Task 10: Identifier And Call Coverage Across Languages

**Files:**
- Modify identifiers and relationships modules for `csharp`, `go`, `swift`, `python`, `rust`, `elixir`, `scala`, `zig`, and `c`
- Modify `crates/julie-extractors/src/vbnet/relationships.rs`
- Modify `crates/julie-extractors/src/python/mod.rs`
- Test matching files under `crates/julie-extractors/src/tests/{csharp,go,swift,python,rust,elixir,scala,zig,c,vbnet}/`

**What to build:** Fill missing `IdentifierKind::TypeUsage`, constructor-call identifiers, duplicate-call dedupe, qualified calls, method callers, Scala call recursion, VB.NET case-insensitive lookup, and Python return type hint parsing.

**Approach:** Use languages with existing `TypeUsage` extraction as templates, especially C++, Java, TypeScript, Kotlin, Dart, Zig, PHP, and Ruby. Do not add type-use extraction to JavaScript as if it had native type syntax; keep it limited to actual typed dialects and parse shapes. For constructor calls, emit `IdentifierKind::Call` with the class name from object creation nodes. For Python return types, prefer annotation nodes over regex.

**Proposed tests:**
- `test_csharp_type_usage_identifiers_cover_fields_params_returns_and_generics`
- `test_go_type_usage_identifiers_cover_fields_params_returns_and_generics`
- `test_swift_type_usage_identifiers_cover_properties_params_returns_and_generics`
- `test_python_type_usage_identifiers_cover_annotations`
- `test_rust_type_usage_identifiers_cover_type_identifier_nodes`
- `test_java_and_csharp_object_creation_emits_constructor_call_identifier`
- `test_csharp_member_invocation_is_not_double_counted`
- `test_elixir_qualified_module_call_is_extracted`
- `test_zig_method_call_relationship_uses_method_caller`
- `test_scala_calls_inside_vals_given_and_extensions_are_extracted`
- `test_vbnet_relationship_lookup_is_case_insensitive`
- `test_python_return_type_hint_uses_annotation_node`

**Acceptance criteria:**
- Reference searches by type name are backed by exact identifier data in every typed language named here.
- Constructor call searches find object creation sites.
- Duplicate relationship extraction is deduped by tested identity, not by a broad "unique" pass that hides real duplicates.

## Task 11: Structured Pending Relationships

**Status:** Completed on 2026-05-06. Stop here for the requested break.

**Files:**
- Modify `crates/julie-extractors/src/elixir/relationships.rs`
- Modify `crates/julie-extractors/src/ruby/relationships.rs`
- Modify `crates/julie-extractors/src/dart/relationships.rs`
- Modify `crates/julie-extractors/src/gdscript/relationships.rs`
- Modify `crates/julie-extractors/src/php/relationships.rs`
- Add `crates/julie-extractors/src/php/call_relationships.rs`
- Modify `crates/julie-extractors/src/php/mod.rs`
- Modify `crates/julie-extractors/src/scala/relationships.rs`
- Modify `crates/julie-extractors/src/swift/relationships.rs`
- Test matching cross-file relationship files under `crates/julie-extractors/src/tests/`

**What to build:** Languages must not silently drop cross-file inheritance, conformance, mixins, includes, or namespace-qualified pending targets. Legacy pending usage should be migrated where structured pending exists.

**Approach:** Use `add_structured_pending_relationship` with `UnresolvedTarget` fields filled from the language context: terminal name, receiver, namespace path, and import context. Keep relationship kind exact: extends vs implements vs uses must come from the syntax, not a guessed fallback. PHP must preserve fully qualified names in `namespace_path` while keeping terminal name as fallback.

**Proposed tests:**
- `test_elixir_use_and_behaviour_emit_structured_pending_targets`
- `test_ruby_cross_file_inheritance_and_include_emit_structured_pending_targets`
- `test_dart_extends_implements_and_with_emit_structured_pending_targets`
- `test_gdscript_extends_metadata_emits_extends_relationship_or_pending_target`
- `test_php_pending_relationship_preserves_namespace_path`
- `test_scala_unresolved_inheritance_and_conformance_keep_relationship_kind`
- `test_swift_unresolved_inheritance_and_conformance_keep_relationship_kind`

**Acceptance criteria:**
- Cross-file targets are either resolved locally or represented as structured pending data.
- No worker adds a legacy pending path where structured pending is available.
- Pending payload tests assert receiver, namespace path, import context, and relationship kind where the language can know them.

**Completion notes:**
- Elixir `use` and behaviour relationships now preserve structured pending targets without resolving imports as fake local behaviour targets.
- Dart inheritance, `with`, and `implements` clauses now emit structured pending targets from parser clause nodes, with import context preserved when unambiguous.
- GDScript `extends` metadata now emits real `Extends` relationships or structured pending targets, while common engine base classes such as `Node` stay out of cross-file pending output.
- PHP preserves namespace components for unresolved type targets and splits call relationship extraction into `php/call_relationships.rs` to keep implementation files below the 500-line cap.
- Scala and Swift keep exact `Extends` versus `Implements` relationship kinds for unresolved inheritance and conformance targets.
- Ruby keeps structured pending inheritance/include coverage and fixes the adjacent receiver-qualified instance call terminal-name bug found during focused verification.

## Task 12: Vue, HTML, CSS, And Razor Embedded Extraction

**Status:** Completed on 2026-05-06 in the current working tree. Stop before Task 13.

**Files:**
- Modify `crates/julie-extractors/src/vue/mod.rs`
- Modify `crates/julie-extractors/src/vue/script.rs`
- Modify `crates/julie-extractors/src/vue/script_setup.rs`
- Add `crates/julie-extractors/src/vue/template.rs`
- Modify `crates/julie-extractors/src/vue/style.rs`
- Modify `crates/julie-extractors/src/vue/helpers.rs`
- Modify `crates/julie-extractors/src/html/mod.rs`
- Modify `crates/julie-extractors/src/html/relationships.rs`
- Modify `crates/julie-extractors/src/html/scripts.rs`
- Modify `crates/julie-extractors/src/css/at_rules.rs`
- Modify `crates/julie-extractors/src/css/identifiers.rs`
- Modify `crates/julie-extractors/src/razor/relationships.rs`
- Test `crates/julie-extractors/src/tests/vue/mod.rs`
- Test `crates/julie-extractors/src/tests/vue/script_setup.rs`
- Test `crates/julie-extractors/src/tests/html/script_style.rs`
- Test `crates/julie-extractors/src/tests/css/mod.rs`
- Test `crates/julie-extractors/src/tests/razor/mod.rs`
- Update `fixtures/extraction/html/basic/expected.json`
- Update `fixtures/extraction/vue/basic/expected.json`

**What to build:** Options API members, script/style offsets, embedded JS/CSS parsing, modern CSS, symbol-name collisions, HTML/Razor pending relationships, and Vue template definitions.

**Approach:** Parse Vue Options API script with the JavaScript or TypeScript parser once per section and walk exported object properties. Preserve byte and line offsets when creating symbols and identifiers. Reuse real JS/CSS extractor paths for embedded ranges instead of adding more regex parsing. HTML, Razor, and SQL-style dead IDs should become real pending relationships only when the language spec advertises pending support. CSS should add modern at-rules and selector identifier types with parser-aware extraction.

**Proposed tests:**
- `test_vue_options_api_methods_computed_data_props_emit_member_symbols`
- `test_vue_identifier_extraction_parses_script_section_once`
- `test_vue_script_symbols_have_full_file_byte_ranges`
- `test_vue_style_delegates_to_css_extractor_with_offsets`
- `test_html_script_and_style_ranges_delegate_to_js_and_css_extractors`
- `test_html_script_import_relationship_uses_matching_script_symbol`
- `test_html_css_razor_symbol_names_use_specific_targets`
- `test_css_modern_at_rules_and_pseudo_selectors_are_extracted`
- `test_vue_template_refs_slots_and_v_model_emit_template_symbols`

**Acceptance criteria:**
- Vue Options API methods such as `increment` and `decrement` are real symbols.
- Embedded ranges preserve full-file positions.
- HTML and Vue embedded scripts/styles produce real language-specific output.
- HTML/CSS/Razor symbols no longer collapse to generic names when a specific target exists.

**Completion notes:**
- Vue Options API extraction now parses script sections through the JavaScript or TypeScript parser and emits real props, emits, data-return properties, computed members, and methods.
- Vue template extraction now emits template-owned `ref`, named slot, and `v-model` symbols while preserving component usages as non-definition references.
- Vue style extraction and HTML inline script/style extraction now delegate to CSS, JavaScript, or TypeScript extractors and preserve full-file line, column, and byte ranges.
- CSS modern at-rules now keep specific names, and pseudo selector call identifiers are parser-scoped so comments and broad selector text do not create bogus calls.
- Razor `@using` relationships now target namespace-specific IDs, while C# `@using (...)` blocks are filtered out.
- `language_spec/specs.rs` was not changed because HTML and Razor pending-capability policy stayed unchanged; no legacy pending path was added.
- Dev verification exposed a downstream Vue `get_symbols` span regression; script setup function spans now cover full function declarations so minimal mode can return function bodies.

## Task 13: Scripting Language Extraction

**Status:** Completed on 2026-05-07 in the current working tree.

**Files:**
- Modify `crates/julie-extractors/src/powershell/relationships.rs`
- Modify `crates/julie-extractors/src/powershell/classes.rs`
- Modify `crates/julie-extractors/src/powershell/commands.rs`
- Modify `crates/julie-extractors/src/powershell/documentation.rs`
- Modify `crates/julie-extractors/src/powershell/helpers.rs`
- Modify `crates/julie-extractors/src/bash/commands.rs`
- Modify `crates/julie-extractors/src/bash/relationships.rs`
- Modify `crates/julie-extractors/src/bash/signatures.rs`
- Modify `crates/julie-extractors/src/bash/variables.rs`
- Modify `crates/julie-extractors/src/lua/core.rs`
- Modify `crates/julie-extractors/src/lua/functions.rs`
- Modify `crates/julie-extractors/src/lua/relationships.rs`
- Modify `crates/julie-extractors/src/gdscript/functions.rs`
- Modify `crates/julie-extractors/src/gdscript/identifiers.rs`
- Modify `crates/julie-extractors/src/gdscript/mod.rs`
- Modify `crates/julie-extractors/src/gdscript/relationships.rs`
- Modify `crates/julie-extractors/src/test_detection.rs`
- Test matching files under `crates/julie-extractors/src/tests/{powershell,bash,lua,gdscript}/`
- Update `fixtures/extraction/{gdscript,lua,powershell}/basic/expected.json`

**What to build:** Clean up command modeling, built-in filtering, signatures, member-call names, imports, aliases, source commands, and test framework detection for scripting languages.

**Approach:** Centralize Bash and PowerShell command or builtin lists before widening them. Calls to external commands should be identifiers and relationships, not fake `Function` definitions. PowerShell method signatures should be assembled from real modifiers, return type, name, and parameters. Lua and GDScript signatures must be synthetic signatures, not full bodies. GDScript attribute calls should store the rightmost method name. Lua `require`, Bash `source`, dot-source, and alias should become import or alias symbols with tested metadata.

**Proposed tests:**
- `test_powershell_builtin_cmdlets_do_not_emit_noisy_pending_relationships`
- `test_powershell_method_signature_includes_parameters_without_double_spaces`
- `test_bash_and_powershell_external_commands_are_not_function_symbols`
- `test_lua_and_gdscript_function_signatures_do_not_include_bodies`
- `test_gdscript_attribute_call_identifier_uses_rightmost_name`
- `test_lua_bare_require_emits_import_symbol`
- `test_bash_alias_and_source_emit_symbols_and_pending_relationships`
- `test_bash_powershell_and_ruby_test_framework_detection_covers_common_frameworks`
- `test_bash_local_all_caps_variable_is_not_environment_constant`

**Acceptance criteria:**
- Command calls no longer pollute the symbol index as definitions.
- Built-in filters are one source of truth per language.
- Signatures fit on one logical declaration line.
- Import/source constructs are searchable and relationship-backed.

**Completion notes:**
- Bash external commands now remain call identifiers plus pending call relationships instead of fake `Function` symbols, while `alias`, `source`, and dot-source commands emit searchable import symbols and structured pending import relationships.
- Bash builtin filtering is centralized in `bash/commands.rs`, and local all-caps variables no longer become environment constants just because their names shout.
- PowerShell command invocations no longer become function symbols, builtin cmdlet filtering is shared by command relationships, and stale command-documentation helpers were removed.
- PowerShell method signatures are assembled from AST children before the body, preserving modifiers, return types, names, and parameters without doubled spaces.
- Lua function signatures now stop at the declaration line, and bare `require("...")` emits a real import symbol plus a structured pending import relationship from that symbol ID.
- GDScript function signatures now stop before the body, attribute-call identifiers use the rightmost method name, and relationship targets keep the full receiver prefix.
- Test framework detection for Bash, PowerShell, and Ruby now covers common framework names while path-gating generic DSL words to avoid production false positives.

## Task 14: JVM, Swift, Scala, And PHP Idioms

**Files:**
- Modify `crates/julie-extractors/src/php/functions.rs`
- Modify `crates/julie-extractors/src/php/members.rs`
- Modify `crates/julie-extractors/src/php/relationships.rs`
- Modify Swift, Kotlin, Scala, Dart annotation extraction paths under `crates/julie-extractors/src/{swift,kotlin,scala,dart}/`
- Modify `crates/julie-extractors/src/csharp/members.rs`
- Modify `crates/julie-extractors/src/csharp/relationships.rs`
- Modify `crates/julie-extractors/src/java/classes.rs`
- Modify `crates/julie-extractors/src/scala/properties.rs`
- Modify `crates/julie-extractors/src/swift/extensions.rs`
- Test matching files under `crates/julie-extractors/src/tests/{php,swift,kotlin,scala,csharp,java,dart}/`

**What to build:** Add PHP constructor property promotion, annotations on non-function symbols, C# local functions/lambdas/partial class linkage, Java record components, Scala case-class fields, Swift extension modeling, and Scala `val` kind semantics.

**Approach:** Route non-function `create_symbol` calls through each language's annotation helper. PHP constructor parameter visibility should emit promoted property symbols. C# local functions and lambdas should become symbols only when they have stable names or useful synthetic names tested by behavior. Partial-class linkage needs an explicit invariant before implementation: full name plus `partial` marker should connect related declarations without merging unrelated classes. Scala case-class constructor params should mirror Kotlin constructor parameter handling. Swift extensions should use a distinct kind if the shared enum task adds one; otherwise use stable metadata with a less misleading existing kind.

**Proposed tests:**
- `test_php_constructor_property_promotion_emits_property_symbols`
- `test_php_pending_target_preserves_namespace_path`
- `test_annotations_attach_to_classes_properties_objects_and_type_aliases`
- `test_csharp_local_functions_lambdas_and_partial_classes_are_modeled`
- `test_java_record_components_emit_properties`
- `test_scala_case_class_fields_are_property_symbols`
- `test_swift_extensions_are_not_plain_class_symbols`
- `test_scala_val_kind_depends_on_scope`

**Acceptance criteria:**
- Framework annotations on non-function symbols are searchable.
- PHP 8 promoted properties are first-class symbols.
- JVM and Swift/Scala language constructs are modeled with exact names, kinds, and parent IDs.

**Status:** Completed on 2026-05-07 in the current working tree.

**Completion notes:**
- Added PHP 8 constructor property promotion with class parent IDs, visibility, signatures, and property type metadata. PHP qualified pending targets now keep namespace-qualified display and callee names.
- Added C# local function symbols, stable-name lambda symbols, and partial-class linkage using full-name plus `partial` marker metadata. Lead review split the new C# code into focused modules to keep touched implementation files under the 500-line limit.
- Added Java record component property symbols with parent IDs and field-based type extraction for qualified, generic, and array component types.
- Added Scala case-class constructor property symbols, scope-sensitive `val` kind semantics, and non-function annotation capture for classes, objects, properties, and type aliases.
- Added Swift extension modeling as `SymbolKind::Module` with `symbol_role = extension` metadata, plus non-function annotation metadata for extensions, type aliases, enum cases, and properties.
- Added Kotlin and Dart non-function annotation capture for classes, objects, properties/fields, type aliases, and related type declarations. Lead review fixed the Dart field path after the first exact test pass exposed the worker's missed `dart/members.rs` path.

## Task 15: Remaining Language Modeling Corrections

**Files:**
- Modify `crates/julie-extractors/src/go/types.rs`
- Modify `crates/julie-extractors/src/go/relationships.rs`
- Modify `crates/julie-extractors/src/qml/mod.rs`
- Modify `crates/julie-extractors/src/zig/variables.rs`
- Modify `crates/julie-extractors/src/rust/relationships.rs`
- Modify `crates/julie-extractors/src/rust/macros.rs` or the Rust macro extraction module if split
- Modify `crates/julie-extractors/src/regex/mod.rs`
- Modify language `infer_types` methods for Lua, QML, Markdown, JSON, TOML, YAML, and R
- Test matching files under `crates/julie-extractors/src/tests/{go,qml,zig,rust,regex,lua,json,toml,yaml,r,markdown}/`

**What to build:** Fix Go kind mapping, Go embeddings and stdlib filtering, QML coverage, Zig imports, Rust import edges and macro kinds, Regex symbol kinds, and minimum type-inference contracts for languages with no implementation.

**Approach:** Map Go structs to `SymbolKind::Struct` and packages to `SymbolKind::Module`. Emit Go embedding relationships or metadata for anonymous fields, and replace the `fmt`-only stdlib filter with a tested stdlib set or a conservative import-shape filter. QML should add doc-comment calls, type inference for property/function signatures, visibility metadata, and binding/signal symbols. Zig should model `usingnamespace` and broader `@import` forms. Rust should emit `Imports` relationships from `use` declarations and use a macro kind if the shared enum task adds one. Regex should distinguish capture groups, character classes, lookarounds, and unicode properties.

**Proposed tests:**
- `test_go_structs_are_struct_symbols_and_packages_are_modules`
- `test_go_embedded_field_emits_relationship_or_embedding_metadata`
- `test_go_stdlib_filter_avoids_noisy_pending_relationships`
- `test_qml_docs_types_visibility_bindings_and_signal_handlers_are_extracted`
- `test_zig_usingnamespace_and_non_declaration_imports_are_extracted`
- `test_rust_use_declarations_emit_import_relationships`
- `test_rust_macro_rules_uses_macro_kind_when_available`
- `test_regex_constructs_have_distinct_symbol_kinds`
- `test_missing_infer_types_languages_return_contract_consistent_results`

**Acceptance criteria:**
- Kind-based filters work for Go, Rust macros, Regex constructs, and QML symbols.
- Import/dependency constructs produce searchable symbols and relationships.
- Languages with no type inference return a tested, contract-consistent result instead of accidental absence.

## Task 16: Core Enums, Embedded Extraction Framework, Capability Matrix, And Test Bar

**Files:**
- Modify `crates/julie-extractors/src/base/types.rs`
- Modify `crates/julie-extractors/src/language_spec/mod.rs`
- Modify `crates/julie-extractors/src/language_spec/specs.rs`
- Modify or add shared embedded-range helpers under `crates/julie-extractors/src/base/` or a language-neutral module
- Modify `fixtures/extraction/capabilities.json`
- Modify `crates/julie-extractors/src/tests/capability_matrix.rs`
- Modify `crates/julie-extractors/src/tests/golden.rs`
- Modify weak tests named by earlier tasks, especially SQL and R

**What to build:** Make core enums and capability data honest enough that language workers are not forced to hide constructs in metadata forever. Establish the test bar that would have caught these findings.

**Approach:** Strategy lane only. Add enum variants only when at least one implementation task uses them in the same wave. Candidate additions from the findings: identifier kinds for annotations, macros, imports, generic args, assignments, parameters; symbol kinds for macro, protocol, type alias, component, decorator, extension, and record; relationship kinds for throws and yields. Add `SymbolModifier` only if at least one language worker lands real modifier data in this plan. Build or extract a shared "parse embedded range with offset" helper only after Vue/HTML/CSS/Razor tests define the offset invariant. Populate `capability_gaps` with exact remaining gaps after each wave.

**Proposed tests:**
- `test_capability_matrix_records_known_gaps_for_languages_with_unfixed_findings`
- `test_capability_matrix_pending_claim_requires_pending_output_in_fixtures`
- `test_relationship_tests_assert_exact_source_target_kind_and_count`
- `test_symbol_tests_assert_exact_name_kind_parent_and_range`
- `test_embedded_range_helper_preserves_offsets`
- `test_core_kind_conversion_rejects_or_reports_unknown_values`

**Acceptance criteria:**
- `capability_gaps` is not an empty aspirational field for every language.
- Tests with `assert symbols.len() >= 0`, `assert True`, or non-empty-only relationship checks are replaced when touched.
- Unknown kind, visibility, relationship, or identifier strings do not silently degrade to a generic value without a tested warning or failure path.

## Task 17: Degraded Parser Failure Results

**Files:**
- Modify `crates/julie-extractors/src/pipeline.rs`
- Modify `crates/julie-extractors/src/manager.rs`
- Modify `crates/julie-extractors/src/base/types.rs` if parse diagnostics need a result field
- Test `crates/julie-extractors/src/tests/pipeline.rs`

**What to build:** A total tree-sitter parse failure should not lose the file without structured evidence. The caller should receive either a degraded extraction result with a diagnostic or a tested failure record path.

**Approach:** Escalation lane. First decide the contract: degraded `ExtractionResults` with a parse diagnostic is preferred if callers can accept it; otherwise manager/indexing code must store a failed-parse file row. The test should simulate `parser.parse(content, None) == None` or a controlled parser failure without relying on huge files or timeouts.

**Proposed tests:**
- `test_pipeline_parse_none_returns_degraded_result_with_diagnostic`
- `test_manager_records_failed_parse_without_losing_file_identity`

**Acceptance criteria:**
- A parser `None` result has a structured diagnostic visible to callers.
- Indexing can preserve file identity even when extraction cannot produce symbols.
- This task is not assigned to a cheap worker because it affects pipeline and caller contracts.

## Verification Strategy

**Project source of truth:** `AGENTS.md`, `RAZORBACK.md`, `docs/TESTING_GUIDE.md`, and `docs/plans/verification-ledger-template.md`.

**Worker red/green scope:** Workers run the exact test they wrote or changed:

```bash
cargo nextest run --lib <exact_test_name> 2>&1 | tail -10
```

Where package selection is useful and confirmed locally, workers may use the narrower equivalent:

```bash
cargo nextest run -p julie-extractors <exact_test_name> 2>&1 | tail -10
```

**Worker ceiling:** Workers do not run `cargo xtask test changed`, `cargo xtask test dev`, `cargo xtask test full`, `cargo xtask test bucket extractors`, or broad unfiltered `cargo nextest`.

**Worker gate invariant:** Each assigned exact test must prove the behavior named in the task: exact symbol names, kinds, parent IDs, relationship source/target IDs, pending payload fields, type IDs, ranges, or diagnostics.

**Lead affected-change scope:** After a coherent batch lands, run:

```bash
cargo xtask test changed
```

**Branch gate:** Before handoff, run:

```bash
cargo xtask test dev
```

**Specialist gates:**
- After extractor-wide or fixture-wide changes, run:

```bash
cargo xtask test bucket extractors
```

- If parser dependencies, parser routing, grammar fixtures, or `.h` parser selection changes enough to affect parser compatibility evidence, run:

```bash
cargo xtask test bucket parser-upgrade
```

- If search ranking, centrality, or dogfood search quality changes as a direct result of graph semantics, run:

```bash
cargo xtask test dogfood
```

**Replay/metric evidence:** No replay metric is a hard gate for this plan unless a task explicitly adds one. Extractor exact tests, capability/golden tests, changed gate, extractor bucket, and dev gate are hard gates.

**Escalation triggers:** Escalate to strategy or escalation tier when a worker needs to change shared enums, public result shapes, parser pipeline contracts, relationship resolver semantics, parent ID policy, capability policy, or when an exact test passes but the lead sees a plausible wrong graph edge or data-loss path.

**Assigned verification failure:** Workers stop and report when assigned verification fails unless the plan explicitly says to update that gate.

**Verification ledger:** Record invariant, command, scope label, commit SHA, result, timestamp, and evidence reuse. Reuse evidence only when the scope label and commit SHA match the current HEAD exactly and the existing result is pass.

## Model Routing

**Project source of truth:** `RAZORBACK.md`.

**Strategy tier:** Planning, architecture, decomposition, lead review, finding triage, and shared contract decisions.
- Codex mapping: `gpt-5.5` medium or high.

**Implementation tier:** Bounded worker tasks from this plan with narrow file ownership and exact tests.
- Codex mapping: `gpt-5.4-mini` xhigh.

**Mechanical tier:** Rote fixture updates, formatting, docs updates, or manifest edits after evidence is already interpreted.
- Codex mapping: `gpt-5.4-mini` low or medium.

**Coupled implementation tier:** Cross-file edits after the strategy contract is named.
- Codex mapping: `gpt-5.3-codex` high for shared invariants, `gpt-5.3-codex` xhigh for parser, concurrency, or terminal-heavy diagnosis.

**Gate-interpretation reviewer:** Reading a failing exact test, replay, metric, or diff to decide whether the test or implementation is wrong.
- Codex mapping: `gpt-5.3-codex` high.

**Escalation tier:** Subtle correctness, public result shape, parser failure behavior, relationship resolver semantics, weak tests, repeated worker failure, or plan mismatch.
- Codex mapping: `gpt-5.3-codex` high for first escalation, `gpt-5.5` high or xhigh for top-risk correctness or planning failure.

**Worker eligibility:** Implementation-tier workers may own Tasks 1, 2, 4, 5, 8, 12, 13, 14, and 15 only when their assigned files do not overlap and the task prompt names exact tests. Tasks 3, 6, 7, 9, 10, 11, 16, and 17 require strategy, coupled implementation, or escalation routing until the lead names the shared invariant.

**Mechanical exclusion:** Mechanical workers cannot own failing tests, replay evidence, metrics, capability interpretation, or acceptance gates.

**Unsupported harness behavior:** If a harness cannot choose per-agent models, use `inherit`, record that limitation in the worker report, and continue.

## Commit And Review Rhythm

- Use one branch for this plan, normally `codex/tree-sitter-extractor-audit-remediation`.
- Commit after each coherent task or tightly coupled task pair once exact worker tests pass and lead review accepts the diff.
- Lead runs `cargo xtask test changed` after each wave, not after every worker.
- Before final handoff, run the branch gate and record the ledger rows.
- If external review is requested at approval time, run `razorback:pre-merge-review` after verification passes and before finishing the branch.

## Verification Ledger

| Invariant | Command | Scope Label | Commit SHA | Result | Timestamp (UTC) | Evidence Reused |
|---|---|---|---|---|---|---|
| Task 10 focused Go malformed recovery and type usage regressions pass | `cargo nextest run -p julie-extractors --lib tests::go::identifiers` | task-10-go-identifiers | `4fbdd4b7f9a2a0458d9f06aa03a16db2be978fa6` | PASS, 2 tests passed | 2026-05-06T20:55:51Z | No |
| Task 10 canonical extractor golden fixtures are current | `UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden` | task-10-golden | `4fbdd4b7f9a2a0458d9f06aa03a16db2be978fa6` | PASS, 3 tests passed | 2026-05-06T20:55:51Z | No |
| Task 10 diff has no whitespace errors | `git diff --check` | task-10-diff-check | `4fbdd4b7f9a2a0458d9f06aa03a16db2be978fa6` | PASS | 2026-05-06T20:55:51Z | No |
| Task 10 changed-file buckets pass | `cargo xtask test changed` | task-10-changed | `4fbdd4b7f9a2a0458d9f06aa03a16db2be978fa6` | PASS, extractors and parser-upgrade buckets passed | 2026-05-06T20:55:51Z | No |
| Task 10 batch regression tier passes | `cargo xtask test dev` | task-10-dev | `4fbdd4b7f9a2a0458d9f06aa03a16db2be978fa6` | PASS, 22 buckets passed in 351.9s | 2026-05-06T20:55:51Z | No |
| Task 11 Dart structured pending inheritance, implements, and mixins pass | `cargo nextest run -p julie-extractors --lib tests::dart::cross_file_relationships` | task-11-dart-cross-file | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 6 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 Elixir structured pending use and behaviour relationships pass | `cargo nextest run -p julie-extractors --lib tests::elixir` | task-11-elixir | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 23 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 PHP namespace-aware pending relationship coverage passes | `cargo nextest run -p julie-extractors --lib tests::php::cross_file_relationships` | task-11-php-cross-file | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 12 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 Ruby structured pending and receiver-qualified call coverage passes | `cargo nextest run -p julie-extractors --lib tests::ruby::cross_file_relationships` | task-11-ruby-cross-file | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 6 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 Scala unresolved inheritance and conformance relationship kinds pass | `cargo nextest run -p julie-extractors --lib tests::scala` | task-11-scala | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 30 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 Swift unresolved inheritance and conformance relationship kinds pass | `cargo nextest run -p julie-extractors --lib tests::swift::cross_file_relationships` | task-11-swift-cross-file | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 10 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 GDScript metadata inheritance and built-in base-class regression pass | `cargo nextest run -p julie-extractors --lib tests::gdscript::cross_file_relationships` | task-11-gdscript-cross-file | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 8 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 formatting is current | `cargo fmt --check` | task-11-fmt-check | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS | 2026-05-06T21:41:52Z | No |
| Task 11 diff has no whitespace errors | `git diff --check` | task-11-diff-check | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS | 2026-05-06T21:41:52Z | No |
| Task 11 canonical extractor golden fixtures are current | `UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden` | task-11-golden | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 3 tests passed | 2026-05-06T21:41:52Z | No |
| Task 11 changed-file buckets pass | `cargo xtask test changed` | task-11-changed | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, extractors bucket passed in 0.8s | 2026-05-06T21:41:52Z | No |
| Task 11 batch regression tier passes | `cargo xtask test dev` | task-11-dev | `e75facd1c3dda1bcbbd9c5765b737fcee1994a9e` | PASS, 22 buckets passed in 348.7s | 2026-05-06T21:41:52Z | No |
| Task 12 Vue template, Options API, style, script range, and script setup line regressions pass | `cargo nextest run -p julie-extractors --lib test_vue_template_refs_slots_and_v_model_emit_template_symbols`; `cargo nextest run -p julie-extractors --lib test_vue_template_symbol_ranges_use_template_section_when_content_repeats`; `cargo nextest run -p julie-extractors --lib test_vue_component_symbol_keeps_broad_span_when_name_appears_in_script`; `cargo nextest run -p julie-extractors --lib test_vue_options_api_methods_computed_data_props_emit_member_symbols`; `cargo nextest run -p julie-extractors --lib test_vue_script_symbols_have_full_file_byte_ranges`; `cargo nextest run -p julie-extractors --lib test_vue_style_delegates_to_css_extractor_with_offsets`; `cargo nextest run -p julie-extractors --lib test_script_setup_line_numbers_are_file_relative` | task-12-vue-focused | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, 7 exact tests passed | 2026-05-06T23:50:56Z | No |
| Task 12 HTML, CSS, and Razor exact regressions pass | `cargo nextest run -p julie-extractors --lib test_html_script_and_style_ranges_delegate_to_js_and_css_extractors`; `cargo nextest run -p julie-extractors --lib test_html_script_import_relationship_uses_matching_script_symbol`; `cargo nextest run -p julie-extractors --lib test_css_modern_at_rules_and_pseudo_selectors_are_extracted`; `cargo nextest run -p julie-extractors --lib test_html_css_razor_symbol_names_use_specific_targets`; `cargo nextest run -p julie-extractors --lib test_razor_using_blocks_do_not_emit_fake_namespace_relationships` | task-12-embedded-css-razor-focused | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, 5 exact tests passed | 2026-05-06T23:50:56Z | No |
| Task 12 existing Vue and HTML regressions remain covered | `cargo nextest run -p julie-extractors --lib test_template_usages_not_extracted_as_symbols`; `cargo nextest run -p julie-extractors --lib test_extract_external_script_and_style_references` | task-12-existing-regressions | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, 2 exact tests passed | 2026-05-06T23:50:56Z | No |
| Task 12 Vue spans still satisfy get_symbols target minimal mode | `cargo nextest run --lib test_vue_target_minimal_extracts_code_body` | task-12-get-symbols-vue-span | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, 1 exact test passed | 2026-05-06T23:50:56Z | No |
| Task 12 canonical extractor golden fixtures are current | `UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden` | task-12-golden | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, 3 tests passed | 2026-05-06T23:50:56Z | No |
| Task 12 formatting is current | `cargo fmt --check` | task-12-fmt-check | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS | 2026-05-06T23:50:56Z | No |
| Task 12 diff has no whitespace errors | `git diff --check` | task-12-diff-check | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS | 2026-05-06T23:50:56Z | No |
| Task 12 changed-file buckets pass | `cargo xtask test changed` | task-12-changed | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, extractors and parser-upgrade buckets passed in 8.8s | 2026-05-06T23:50:56Z | No |
| Task 12 extractor specialist bucket passes | `cargo xtask test bucket extractors` | task-12-extractors | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, extractors bucket passed in 0.8s | 2026-05-06T23:50:56Z | No |
| Task 12 batch regression tier passes | `cargo xtask test dev` | task-12-dev | `57f59b748850e373c0541f9acb7f6ba7f0a4ba67 + dirty Task 12 working tree` | PASS, 22 buckets passed in 356.0s; not rerun after warning-only helper cleanup, final `changed` gate passed | 2026-05-06T23:50:56Z | No |
| Task 13 focused scripting extractor regressions pass | sequential loop over `cargo nextest run -p julie-extractors --lib <exact_test>` for `test_gdscript_function_signature_does_not_include_body`, `test_gdscript_attribute_call_identifier_uses_rightmost_name`, `test_attribute_call_relationship_uses_rightmost_method_name`, `test_bash_external_command_is_identifier_not_function_symbol`, `test_bash_alias_and_source_emit_symbols_and_pending_relationships`, `test_bash_local_all_caps_variable_is_not_environment_constant`, `test_powershell_external_command_call_is_identifier_not_function_symbol`, `test_powershell_method_signature_includes_parameters_without_double_spaces`, `test_powershell_builtin_cmdlets_do_not_emit_noisy_pending_relationships`, `test_lua_function_signature_does_not_include_body`, `test_lua_bare_require_emits_import_symbol`, and `test_bash_powershell_and_ruby_test_framework_detection_covers_common_frameworks` | task-13-focused | `6594c4acbac9d579ad5621d9ace6096efd91a034 + dirty Task 13 working tree` | PASS, 12 exact tests passed | 2026-05-07T00:48:53Z | No |
| Task 13 canonical extractor golden fixtures are current | `UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden` | task-13-golden | `6594c4acbac9d579ad5621d9ace6096efd91a034 + dirty Task 13 working tree` | PASS, 3 tests passed | 2026-05-07T00:48:53Z | No |
| Task 13 formatting is current | `cargo fmt --check` | task-13-fmt-check | `6594c4acbac9d579ad5621d9ace6096efd91a034 + dirty Task 13 working tree` | PASS | 2026-05-07T00:48:53Z | No |
| Task 13 diff has no whitespace errors | `git diff --check` | task-13-diff-check | `6594c4acbac9d579ad5621d9ace6096efd91a034 + dirty Task 13 working tree` | PASS | 2026-05-07T00:48:53Z | No |
| Task 13 changed-file buckets pass | `cargo xtask test changed` | task-13-changed | `6594c4acbac9d579ad5621d9ace6096efd91a034 + dirty Task 13 working tree` | PASS, extractors and parser-upgrade buckets passed in 3.3s | 2026-05-07T00:48:53Z | No |
| Task 13 batch regression tier passes | `cargo xtask test dev` | task-13-dev | `6594c4acbac9d579ad5621d9ace6096efd91a034 + dirty Task 13 working tree` | PASS, 22 buckets passed in 354.4s | 2026-05-07T00:48:53Z | No |
| Task 14 focused JVM, Swift, Scala, PHP, Kotlin, and Dart idiom regressions pass | sequential loop over `cargo nextest run -p julie-extractors --lib <exact_test>` for `test_php_constructor_property_promotion_emits_property_symbols`, `test_php_pending_target_preserves_namespace_path`, `test_csharp_local_functions_lambdas_and_partial_classes_are_modeled`, `test_java_record_components_emit_properties`, `test_scala_case_class_fields_are_property_symbols`, `test_scala_val_kind_depends_on_scope`, `test_scala_annotations_attach_to_classes_properties_objects_and_type_aliases`, `test_swift_extensions_are_not_plain_class_symbols`, `test_swift_annotations_attach_to_extensions_type_aliases_and_enum_cases`, `test_kotlin_annotations_attach_to_classes_properties_objects_and_type_aliases`, and `test_dart_annotations_attach_to_classes_properties_and_type_aliases` | task-14-focused | `cf6c9baea45442eb8d90bc9fd3cc074b627b70ce + dirty Task 14 working tree` | PASS, 11 exact tests passed | 2026-05-07T01:20:08Z | No |
| Task 14 canonical extractor golden fixtures are current | `UPDATE_GOLDEN=1 cargo nextest run -p julie-extractors golden` | task-14-golden | `cf6c9baea45442eb8d90bc9fd3cc074b627b70ce + dirty Task 14 working tree` | PASS, 3 tests passed | 2026-05-07T01:20:08Z | No |
| Task 14 formatting is current | `cargo fmt --check` | task-14-fmt-check | `cf6c9baea45442eb8d90bc9fd3cc074b627b70ce + dirty Task 14 working tree` | PASS | 2026-05-07T01:20:08Z | No |
| Task 14 diff has no whitespace errors | `git diff --check` | task-14-diff-check | `cf6c9baea45442eb8d90bc9fd3cc074b627b70ce + dirty Task 14 working tree` | PASS | 2026-05-07T01:20:08Z | No |
| Task 14 changed-file buckets pass | `cargo xtask test changed` | task-14-changed | `cf6c9baea45442eb8d90bc9fd3cc074b627b70ce + dirty Task 14 working tree` | PASS, extractors bucket passed in 0.9s | 2026-05-07T01:20:08Z | No |
| Task 14 batch regression tier passes | `cargo xtask test dev` | task-14-dev | `cf6c9baea45442eb8d90bc9fd3cc074b627b70ce + dirty Task 14 working tree` | PASS, 22 buckets passed in 371.1s | 2026-05-07T01:20:08Z | No |
