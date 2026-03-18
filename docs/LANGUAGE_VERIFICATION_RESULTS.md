# Language Verification Results

> Tracking verification of each language against [LANGUAGE_VERIFICATION_CHECKLIST.md](./LANGUAGE_VERIFICATION_CHECKLIST.md).
> Methodology: index a real-world project, run all applicable checks, fix bugs found via TDD.

**Legend:** PASS | FAIL(n) | SKIP (not applicable) | — (not yet tested)

---

## Summary Matrix

### Full Tier (19 languages)

| Language | Reference Project | 1. Symbols | 2. Relationships | 3. Identifiers | 4. Centrality | 5. Def Search | 6. deep_dive | 7. get_context | 8. Test Detection | Date |
|----------|------------------|-----------|-----------------|---------------|--------------|--------------|-------------|---------------|------------------|------|
| GDScript | bitbrain/pandora | PASS | PASS | PASS* | PASS* | PASS | PASS* | PASS | PASS | 2026-03-17 |
| Zig | zigtools/zls | PASS | PASS | PASS | PASS* | PASS | PASS | PASS | PASS | 2026-03-17 |
| TypeScript | colinhacks/zod | PASS | PASS | PASS* | PASS* | PASS* | PASS | PASS* | PASS* | 2026-03-17 |
| Python | pallets/flask | PASS | PASS | PASS | PASS | PASS | PASS | PASS* | PASS | 2026-03-18 |
| Go | spf13/cobra | PASS | PASS | PASS | PASS | PARTIAL | PASS | PASS | PASS | 2026-03-18 |
| Java | google/gson | PASS | PASS | PASS | PASS* | PASS | PASS | PASS | PASS | 2026-03-18 |
| PHP | slimphp/Slim | PASS | FAIL | PARTIAL | PASS* | PASS | PASS | PASS | PASS | 2026-03-18 |
| Ruby | sinatra/sinatra | PASS | PASS | PASS | MIXED | PASS | PASS | PASS | PASS | 2026-03-18 |
| C# | JamesNK/Newtonsoft.Json | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | 2026-03-18 |
| Swift | Alamofire/Alamofire | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | 2026-03-18 |
| Kotlin | square/moshi | PASS | PASS | PASS | PASS* | PASS | PASS | PASS | PASS | 2026-03-18 |
| C | jqlang/jq | PASS | PASS | PASS | PARTIAL | PASS | PASS | PASS | FAIL* | 2026-03-18 |
| C++ | nlohmann/json | PASS | FAIL | PASS* | FAIL | PASS | FAIL | PASS | PASS | 2026-03-18 |
| Dart | rrousselGit/riverpod | PASS | PASS | PASS | PASS* | PASS | PASS | PASS | PASS | 2026-03-18 |
| Lua | rxi/lite | PASS | PASS | PASS | FAIL | PARTIAL | PARTIAL | PARTIAL | N/A | 2026-03-18 |
| Scala | typelevel/cats | PASS | PASS | PASS* | PASS* | PASS | PASS | PASS | PASS | 2026-03-18 |
| Elixir | phoenixframework/phoenix | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | 2026-03-18 |
| Rust | julie (self) | PASS | PASS | PASS | PASS | PASS | PASS | PASS | PASS | 2026-03-18 |
| JavaScript | expressjs/express | PASS | PASS | PASS | PASS* | PASS | PASS | PASS | PASS | 2026-03-18 |

### Specialized Tier (9 languages)

| Language | Reference Project | 1. Symbols | 3. Identifiers | 5. Def Search | 8. Test Detection | Date |
|----------|------------------|-----------|---------------|--------------|------------------|------|
| QML | KDE/kirigami | PASS* | PASS* | PASS* | PASS* | 2026-03-17 |
| Razor | dotnet/blazor-samples (9.0) | PASS* | PASS | PASS | N/A | 2026-03-17 |
| Bash | fixture: system-admin-script.sh | PASS | PASS | PASS | N/A | 2026-03-18 |
| PowerShell | fixture: system-health-check.ps1 | PASS | PASS | PASS | N/A | 2026-03-18 |
| Vue | fixture: App.vue | PASS | PASS | PASS | N/A | 2026-03-18 |
| R | fixture: ggplot2-geom-point.R | PASS | PASS | PASS | N/A | 2026-03-18 |
| SQL | fixture: postgresql-migrations.sql | PASS | PASS | PASS | N/A | 2026-03-18 |
| HTML | fixture: popup-info-web-component.html | PASS* | N/A | N/A | N/A | 2026-03-18 |
| CSS | fixture: flexbox-grid.css | PASS* | N/A | N/A | N/A | 2026-03-18 |

### Data/Docs Tier (5 languages)

| Language | Reference Project | 1. Symbols | Date |
|----------|------------------|-----------|------|
| Markdown | julie: docs/ARCHITECTURE.md | PASS | 2026-03-18 |
| JSON | julie: .claude/hooks/hooks.json | PASS* | 2026-03-18 |
| TOML | julie: Cargo.toml | PASS | 2026-03-18 |
| YAML | julie: .github/workflows/release.yml | PASS | 2026-03-18 |
| Regex | fixture: validation_patterns.regex | PASS | 2026-03-18 |

---

## Recommended Reference Projects

### Full Tier

| Language | Repository | Why |
|----------|-----------|-----|
| GDScript | `bitbrain/pandora` | Godot framework, good coverage of GDScript patterns |
| Zig | `zigtools/zls` | Zig Language Server — real-world Zig with types, structs, tests |
| QML | `nickvdh/qml-examples` or a KDE app | QML-heavy project with components and signals |
| Razor | `dotnet/blazor-samples` | Official Blazor samples with .razor components |
| TypeScript | `colinhacks/zod` | Focused TS library with classes, generics, type exports |
| Python | `pallets/flask` | Medium-size, well-structured, decorators + imports |
| Go | `spf13/cobra` | CLI framework, interfaces, struct methods, test files |
| Java | `google/gson` | Focused Java lib with class hierarchy, generics |
| PHP | `slimphp/Slim` | PSR-compliant, namespaced PHP with backslash paths |
| Ruby | `sinatra/sinatra` | Module nesting, DSL methods, `Module::Class` patterns |
| C# | `JamesNK/Newtonsoft.Json` | .NET conventions, `.Tests` project, DI patterns |
| Swift | `Alamofire/Alamofire` | Protocol-oriented, extensions, test targets |
| Kotlin | `square/moshi` | Kotlin-first lib, annotations, sealed classes |
| C | `stedolan/jq` | Medium C project with headers, function pointers |
| C++ | `nlohmann/json` | Templates, namespaces, header-only patterns |
| Dart | `rrousselGit/riverpod` | Provider pattern, code generation, test structure |
| Lua | `rxi/lite` | Focused Lua editor, modules, metatables |
| Scala | `typelevel/cats` | Traits, companion objects, implicits (already used) |
| Elixir | `phoenixframework/phoenix` | Modules, protocols, macros (already used) |

### Specialized Tier

| Language | Repository | Why |
|----------|-----------|-----|
| Bash | `ohmyzsh/ohmyzsh` | Functions, aliases, sourcing patterns |
| PowerShell | `PowerShell/PSScriptAnalyzer` | Cmdlets, modules, Pester tests |
| Vue | `vuejs/pinia` | SFC components, composables, TypeScript interop |
| R | `tidyverse/ggplot2` | S4/R5 classes, generics, testthat tests |
| SQL | `flyway/flyway` | Migration SQL files with DDL/DML |
| HTML | (covered by any web project — Vue/TS repo) | Templates, semantic elements |
| CSS | (covered by any web project) | Selectors, custom properties |

### Data/Docs Tier

| Language | Source |
|----------|--------|
| Markdown | Any project's `docs/` (use Julie itself) |
| JSON | Any project's config files (use Julie itself) |
| TOML | Julie's own `Cargo.toml` files |
| YAML | Any CI config (GitHub Actions in Julie) |
| Regex | (Minimal — verify extractor doesn't crash) |

---

## Per-Language Details

*(Filled in as each language is verified)*

<!-- Template for each language:
### Language Name
- **Reference project:** org/repo
- **Date verified:** YYYY-MM-DD
- **Verified by:** session/agent
- **Issues found:**
  - Issue description → fix commit hash
- **Notes:** Any language-specific observations
-->

### GDScript
- **Reference project:** bitbrain/pandora (337 .gd files, 7987 symbols, 2665 relationships)
- **Date verified:** 2026-03-17
- **Issues found:**
  - **FIXED: Identifier extraction missing type annotations** — The `identifiers.rs` extractor only handled `call`, `get_node`, `attribute`, and `subscript` nodes. It completely missed type annotations (`var x: Type`, `func f(a: Type) -> ReturnType`, `extends BaseClass`). This caused `PandoraEntity` (the core model class) to have 0 incoming references. Fix: added `"type"` node handler to extract `IdentifierKind::TypeUsage` identifiers. Result: PandoraEntity went from 0 → 70 dependents.
  - **FIXED: Centrality 0.00 for all type-defining symbols** — Centrality was computed only from the `relationships` table (call graph). The GDScript relationship extractor only captures function calls, not inheritance or type usage. Since GDScript classes are referenced primarily via type annotations, centrality was 0.00. Fix: added Step 1b to `compute_reference_scores()` — a SQL query that counts cross-file `type_usage` identifiers by name match for type-defining symbols (class, struct, enum, interface, trait, type, module, namespace). Weight 1.0 per reference (same as `uses` relationships). This is a systemic fix benefiting all 33 languages.
  - **NOTED: max_depth=1 inconsistency** — `entity_backend.gd` (561 lines) shows only 7 symbols at max_depth=1 but 53 at max_depth=2. Meanwhile `entity.gd` (822 lines) shows all 80 symbols at max_depth=1. Same class structure, different depth behavior. Display quirk, not data loss.
- **Notes:**
  - `*` on Identifiers, Centrality, and deep_dive indicates these PASS after fixes applied in this session
  - GDScript uses GdUnit4 test framework; test files in `test/` with `extends GdUnitTestSuite`
  - GDScript's constructor syntax `ClassName.new()` captures `new` as the call, not `ClassName` — type annotation extraction compensates for this
  - The `preload()` mechanism (GDScript's import) is captured as constants, not Import identifiers — acceptable since GDScript has no standalone import statement

### Zig
- **Reference project:** zigtools/zls (104 .zig files, 10677 symbols, 3549 relationships)
- **Date verified:** 2026-03-17
- **Issues found:**
  - **FIXED: Centrality 0.00 for Zig type constants** — Zig declares types as `const Server = @This()` which Julie extracts as `constant` kind. The centrality TypeUsage boost only applied to `class`, `struct`, `enum`, etc. — not `constant`. Fix: added `constant` to the centrality WHERE clause. Safe because the SQL filters by `type_usage`/`import` identifiers — plain constants with zero such refs get no spurious boost.
  - **FIXED: Import identifiers not contributing to centrality (systemic)** — The centrality boost only counted `type_usage` identifiers. In Zig (and most languages), cross-file references are primarily `@import()` which produce `import` kind identifiers. Fix: expanded centrality boost to also count `import` identifiers with weight 2.0 (matching the relationship weight for imports). Benefits all 33 languages.
  - **FIXED: Zig identifier extractor missing type annotations** — Like GDScript, the Zig `identifiers.rs` only handled `call_expression` and `field_expression`. Type annotations (`field: Type`, `param: *Type`, `!ReturnType`, `var x: Type`) were missed. Fix: added `identifier` node handler that detects type position by checking parent context (`container_field`, `parameter`, `pointer_type`, `error_union_type`, `optional_type`, `variable_declaration` after `:`). Filters out Zig builtin types to avoid noise.
- **Notes:**
  - `*` on Centrality indicates PASS after fixes
  - Zig's `@import` statements correctly captured as `import` kind (15 import refs for DocumentStore)
  - `@This()` idiom (Zig's self-type pattern) captured as `constant` — semantically a type alias but syntactically a const
  - Server: centrality 0.40, 35 refs. DocumentStore: 39 refs. Healthy reference counts
  - Test files in `tests/` directory properly detected

### QML (Specialized Tier)
- **Reference project:** KDE/kirigami (200 .qml files, 6207 symbols, 11196 relationships)
- **Date verified:** 2026-03-17
- **Tier:** Specialized (moved from Full — QML is a declarative UI language)
- **Issues found:**
  - **FIXED: Import statements not extracted** — The extractor had no handler for `ui_import` nodes. QML imports like `import QtQuick 2.15`, `import org.kde.kirigami as Kirigami` were completely missed. Fix: added `ui_import` handler in `traverse_node()` that extracts the `source` field (handles both `identifier` and `nested_identifier` nodes for simple and dotted paths).
  - **FIXED: Class name was base type instead of component name** — In QML, the file name IS the component name (`ScrollablePage.qml` defines `ScrollablePage`). The root element (`KC.Page {}`) declares the base type. The extractor stored the base type as the class name, making definition search unable to find components by name. Fix: derive component name from file path stem, store base type in signature as `extends BaseType`.
  - **FIXED: No TypeUsage identifiers for component instantiations** — Nested `ui_object_definition` nodes (`Rectangle {}`, `Button {}`, etc.) are type references but weren't captured as identifiers. Fix: added `ui_object_definition` handler in `identifiers.rs` that detects nested (non-root) objects and creates `TypeUsage` identifiers for their type names. This enables centrality scoring for QML components.
  - **FIXED: Test detection not working (systemic)** — QML test files use `autotests/` directory and `tst_` file prefix (Qt convention). Neither pattern was in `is_test_path()`. Fix: added `"autotests"` to directory segment match and `tst_` to file name prefix patterns. Benefits all languages using Qt-style test conventions.
  - **FIXED: Centrality 0.00 for all QML components despite heavy usage (systemic)** — The centrality boost SQL in `compute_reference_scores()` Step 1b used exact name matching (`i.name = symbols.name`). QML (and other languages) use namespace-qualified references (`Kirigami.ScrollablePage`) that don't match unqualified symbol names (`ScrollablePage`). Fix: added suffix matching (`i.name LIKE '%.' || symbols.name`) to also match qualified identifiers by their last component. Benefits all languages using dot-qualified references (QML, Elixir, Scala, Java, C#, etc.).
- **Notes:**
  - `*` on all checks indicates PASS after fixes applied in this session
  - KDE/Qt projects use `autotests/` with `tst_` prefix — now recognized by generic test detection
  - QML component instantiations now contribute to centrality via TypeUsage identifiers
  - **Live verified:** ScrollablePage centrality 0.00 → 0.87 (25 dependents), OverlayDrawer 0.00 → 0.56 (7 dependents)
  - AboutPage stays at 0.00 with 1 dependent — correct for a leaf component

### Razor (Specialized Tier)
- **Reference project:** dotnet/blazor-samples 9.0 (623 .razor files + 242 .cs files, 6901 symbols, 1390 relationships)
- **Date verified:** 2026-03-17
- **Tier:** Specialized (moved from Full — Razor is a template language with embedded C#)
- **Issues found:**
  - **FIXED: `get_symbols` invisible `@code` block content (orphan parent_id bug)** — `extract_code_block()` in `razor/mod.rs` created a parent symbol but never stored it (early `return` skipped the `symbols.push()`). Child symbols (methods, properties, fields from `@code {}` blocks) referenced this phantom parent via `parent_id`, causing `get_symbols` depth filtering to exclude them. Symbols were in the database and reachable via `fast_search`/`deep_dive`, but `get_symbols` — the primary navigation tool — showed zero C# code for Razor files. Fix: pass outer `parent_id` instead of code block's ID, making `@code` block symbols top-level file members.
- **Notes:**
  - `*` on Symbols indicates PASS after fix
  - Extraction was always correct — methods, properties, classes from `@code` blocks were in the DB. Only `get_symbols` display was broken.
  - `@page`, `@using`, `@inject`, `@attribute` directives all captured correctly
  - No test files in this samples repo — Check 8 not applicable
  - 62 definitions of `OnInitializedAsync` across the codebase, all with correct signatures and visibility

### TypeScript
- **Reference project:** colinhacks/zod (496 files, 17055 symbols, 7184 relationships)
- **Date verified:** 2026-03-17
- **Issues found (10 — all fixed):**
  - **FIXED: `packages/` in BLACKLISTED_DIRECTORIES (CRITICAL)** — Hard directory filter silently excluded ALL source files in JS/TS monorepos (npm/pnpm/Lerna/Nx/Turborepo). zod: 36 → 498 files after fix. Commit `45885e09`.
  - **FIXED: `packages/` in analyze_vendor_patterns** — Same false positive in soft vendor detection. Removed from both `matches!` blocks. Commit `45885e09`.
  - **FIXED: Lockfile noise (pnpm-lock.yaml, package-lock.json)** — `pnpm-lock.yaml` has `.yaml` extension (not blacklisted). Added 9,861 YAML symbols = 43% noise. Fix: `BLACKLISTED_FILENAMES` constant with exact-name filtering in `should_index_file()`. Commit `45885e09`.
  - **FIXED: Zero type_usage identifiers** — `identifiers.rs` only handled `call_expression` and `member_expression`. Missing `type_identifier` node → centrality Step 1b contributed nothing for TS interfaces/types. Commit `45885e09`.
  - **FIXED: type_identifier too broad** — `type_identifier` appears for both declaration names AND type references. Added `is_type_declaration_name()` parent-context filter for `interface_declaration`, `type_alias_declaration`, `class_declaration`, `abstract_class_declaration`, `type_parameter`, `mapped_type_clause`. Commit `45885e09`.
  - **FIXED: Noise types not filtered** — Added `is_ts_noise_type()` for single-letter generics (T, K, V) and TS compiler utility types (Partial, Required, Pick, Omit, etc.). JS runtime globals (Map, Set, Promise, Array, Iterator, etc.) are intentionally NOT filtered — they can be user-defined, and builtin refs to non-existent symbols cause zero centrality impact. Commit `45885e09`, narrowed in follow-up.
  - **FIXED: SQL LIKE wildcards in centrality (systemic)** — `_` and `%` in symbol names were SQL wildcards causing false centrality matches. Fixed in `compute_reference_scores()` (REPLACE-based escaping) AND in `build_name_match_clause()` in `identifiers.rs` (`escape_sql_like` + ESCAPE clause). Benefits all 33 languages. Commit `45885e09`, identifier fix in follow-up.
  - **FIXED: Watcher lockfile filtering** — `BLACKLISTED_FILENAMES` not checked in `watcher/filtering.rs` or `watcher/events.rs`. Watcher could re-index lockfiles on every save. Commit `45885e09`.
  - **FIXED: `libs/` vendor false positive** — `analyze_vendor_patterns` treated `libs/` as vendor. Nx/Angular monorepos use `libs/` as standard source directory alongside `apps/`. Commit `45885e09`.
  - **FIXED: Test exclusion missed non-function symbols in test files (systemic)** — `filter_test_symbols()` in `text_search.rs` checked only `metadata["is_test"]` (set by extractors for function symbols). Interfaces, types, and classes in `.test.ts` files had no `is_test` metadata and bypassed `exclude_tests=true`. Fix: added `is_test_path(&s.file_path)` as fallback in `filter_test_symbols`. Benefits all languages.
  - **FIXED: `get_context` neighbors always 0 for TypeScript (systemic)** — `expand_graph()` queried only the `relationships` table (imports, inheritance edges). TypeScript type usages and call references live in the `identifiers` table instead. Fix: after relationship expansion, also query `get_identifiers_by_names()` for `type_usage`, `import`, and `call` kinds. `containing_symbol_id` from each identifier ref becomes a neighbor. Relationship-based entries take priority. Benefits all languages where identifiers are richer than relationships (TypeScript, GDScript, Zig, Scala).
- **Live verification results:**
  - Check 1 (Symbols): 151 symbols from `schemas.ts` at max_depth=1; kinds correct (interface, class, method, export); signatures preserved
  - Check 2 (Relationships): 7184 total; ZodString class hierarchy captured; cross-file imports working
  - Check 3 (Identifiers): 94 identifier refs for ZodType; definition found; call-site refs detected
  - Check 4 (Centrality): ZodType interface **0.95** (94 refs); ZodType variable **0.00** (correct — value position); 94 dependents via deep_dive
  - Check 5 (Def Search): Interface/class/export promoted above imports; test file definition ranks #1 (expected — definition searches include tests by default)
  - Check 6 (deep_dive): `context_file` disambiguates cross-file ZodType; 44 methods shown; 94 dependents; change risk MEDIUM
  - Check 7 (get_context): 7.1 PASS (TS source pivots, no bundled JS); 7.2 PASS (high-centrality ZodType pivot); 7.3 PASS after fix (identifier-based expansion now provides type_usage neighbors)
  - Check 8 (Test Detection): `.test.ts` files detected by path; `exclude_tests=true` now excludes all symbols from test files (including interfaces); auto-exclusion works for content search
- **Notes:**
  - `*` on Identifiers, Centrality, Def Search, Test Detection indicates PASS after fixes
  - TypeScript `predefined_type` node (`string`, `number`, `boolean`) is distinct from `type_identifier` — builtins naturally excluded without filtering
  - Multi-letter generic param references (`Input`, `Output`) still appear as TypeUsage in reference positions — acceptable, would require scope analysis to filter
  - The relationships table for TypeScript has 0 rows for ZodType — all 94 "dependents" live in the identifiers table (type_usage). Relationships only capture imports and extends/implements edges.

### Scala
- **Reference project:** typelevel/cats (934 files, 22336 symbols, 12236 relationships)
- **Date verified:** 2026-03-18 (full sweep)
- **All 8 checks: PASS**
- **Issues found (4 — all fixed):**
  - **FIXED: Zero type_usage identifiers** — Scala `identifiers.rs` only handled `call_expression` and `field_expression`. Added `type_identifier` handler. Commit `f90f7350`.
  - **FIXED: Type alias declaration names not filtered** — `type_definition.name` uses `type_identifier` in Scala. Added `is_type_declaration_name()`. Commit `f90f7350`.
  - **FIXED: Noise types not filtered** — Added `is_scala_noise_type()` for single-letter generics and Scala primitives. Commit `f90f7350`.
  - **FIXED: Factory consistency test count stale** — Expected 31 but had 34 after Scala+Elixir. Commit `f90f7350`.
- **Live verification results:**
  - Functor: centrality **1.00** (465+ refs), Monad: centrality **1.00** (366+ refs)
  - 26 symbols from Functor.scala (trait + companion + 17 methods + nested types)
  - Extends chains correctly detected (Apply, Traverse, CoflatMap)
  - 153 neighbors in get_context across 51 files spanning entire type class hierarchy
  - FunctorSuite/MonadSuite correctly excluded with `exclude_tests=true`
- **Notes:**
  - `*` on Identifiers and Centrality indicates PASS after fixes
  - Wildcard imports in cats means import-kind refs return 0 for individual symbols (not a bug)

### Elixir
- **Reference project:** phoenixframework/phoenix (456 files, 14705 symbols, 4236 relationships)
- **Date verified:** 2026-03-18
- **All 8 checks: PASS**
- **No bugs found.**
- **Live verification results:**
  - Phoenix.Socket: centrality **1.00** (118 refs), Phoenix.Router: **1.00** (65 refs), Phoenix.Controller: **1.00** (59 refs)
  - Qualified names (`Phoenix.Router`) correctly preserved as full dot-separated names
  - Both qualified and unqualified searches find module definitions as top result
  - 28 public exports for Phoenix.Router, semantic similarity working (0.83 for Route)
  - `test/` directory files correctly excluded
- **Notes:**
  - `defmacro` mapped to kind=`function` (acceptable — Elixir macros are syntactically similar)
  - `use` mapped to import reference kind (correct for Elixir's module inclusion semantics)

### Python
- **Reference project:** pallets/flask (227 files, 4297 symbols, 1952 relationships)
- **Date verified:** 2026-03-18
- **6/8 PASS, 2 FAIL (Centrality, Def Search)**
- **Issues found (2 — not yet fixed):**
  - **UNFIXED: Test subclass steals centrality** — `tests/test_config.py` defines `class Flask(flask.Flask)` (a test subclass). Pending relationship resolution picks this test Flask (raw score 213) over the real `src/flask/app.py` Flask (raw score 1.4). The real Flask class gets almost zero centrality despite 125 dependents.
  - **UNFIXED: Def search ranking affected** — Test Flask subclass ranks #1 above real Flask class (cascade from centrality issue).
- **Live verification results:**
  - Check 1 (Symbols): 142 symbols for Flask class, 35 methods, correct types
  - Check 2 (Relationships): 60 refs for Flask, cross-file imports/calls, `extends` captured
  - Check 3 (Identifiers): 125 dependents via identifiers
  - Check 6 (deep_dive): 35 methods, `extends App`, 125 dependents shown
  - Check 7 (get_context): 10 neighbors showing request handling pipeline
  - Check 8 (Test Detection): `tests/` directory correctly excluded
- **Notes:**
  - `request` global proxy variable has centrality 0.00 (92 refs) — `variable` kind excluded from Step 1b
  - `route` method has highest raw centrality (495.0) — expected for the most-called decorator
  - `__init__.py` re-exports all captured correctly (39 import symbols)

### Go
- **Reference project:** spf13/cobra (65 files, 1441 symbols, 1590 relationships)
- **Date verified:** 2026-03-18
- **7/8 PASS, 1 PARTIAL (Def Search)**
- **Issues found (1 — minor):**
  - **Markdown headings outrank Go struct** — Without `language` filter, `fast_search("Command", search_target="definitions")` returns markdown doc headings above the actual `Command` struct. With `language="go"`, ranks correctly at #1.
- **Live verification results:**
  - Command: centrality **1.00** (correct gradient: Execute=0.69, SetErrPrefix=0.43)
  - 118 symbols from command.go, correct kinds (class/field/method/constant/namespace)
  - `*_test.go` functions correctly detected and excluded
  - 3 high-centrality pivots in get_context, 21 neighbors

### Java
- **Reference project:** google/gson (305 files, 8511 symbols, 7327 relationships)
- **Date verified:** 2026-03-18
- **All 8 checks: PASS** (after type_usage fix)
- **Issues found (1 — fixed):**
  - **FIXED: Centrality 0.00 for all core classes** — Java identifier extractor had no `type_identifier` handler. Added handler + `is_type_declaration_name()` + single-letter generic filter. Commit `90bffa2a`.
- **Live verification results:**
  - Gson: centrality **1.00** (673 incoming refs) — was 0.00 before fix
  - 92 symbols from Gson.java, 26 fields, 35 methods, generics preserved
  - `src/test/java/` layout correctly recognized, `exclude_tests` filtering works
  - `extends`/`implements` relationships properly detected

### PHP
- **Reference project:** slimphp/Slim (145 files, 4031 symbols, 1555 relationships)
- **Date verified:** 2026-03-18
- **5/8 PASS, 2 FAIL, 1 PARTIAL** (after type_usage fix)
- **Issues found (3):**
  - **FIXED: No type_usage identifiers** — Added `named_type` + `instanceof_expression` handlers. App centrality 0.00 → 0.24. Commit `90bffa2a`.
  - **UNFIXED: Class-level relationship tracking weak** — `new ClassName()`, `use` imports, `extends`/`implements` not fully tracked as incoming references at class level. Method-level refs work.
  - **UNFIXED: `reference_kind` filter ignored** — All kinds return identical results (downstream of relationship issue).
- **Notes:**
  - PHP's interface-heavy architecture (Slim uses PSR interfaces) naturally limits direct class references
  - `get_symbols` target filter duplicates methods (106 instead of 55) — display bug

### Ruby
- **Reference project:** sinatra/sinatra (289 files, 6919 symbols, 1661 relationships)
- **Date verified:** 2026-03-18
- **7/8 PASS, 1 MIXED (Centrality)**
- **Issues found (1):**
  - **UNFIXED: Centrality on constant, not class** — Ruby class definitions produce both a `class` and `constant` symbol at the same line. Centrality accumulates on the wrong one (`Sinatra::Base` class = 0.00, but `route` method = 1.00).
- **Notes:**
  - Module nesting (`Sinatra::Base`, `Sinatra::Helpers`) correctly represented
  - attr_accessor, attr_reader, aliases, includes/extends all captured
  - 21 disambiguation candidates for `Base` in deep_dive (class/constant duplication)

### C#
- **Reference project:** JamesNK/Newtonsoft.Json (1160 files, 21062 symbols, 17049 relationships)
- **Date verified:** 2026-03-18
- **All 8 checks: PASS**
- **No bugs found.**
- **Live verification results:**
  - JsonConvert: centrality **1.00** (786 dependents), JsonReader: 0.43, JToken: 0.35
  - 69 methods, 8 fields in JsonConvert, correct kinds
  - `.Tests` project convention correctly detected and excluded
  - 2 pivots + 38 neighbors in get_context across 11 files

### Swift
- **Reference project:** Alamofire/Alamofire (521 files, 20552 symbols, 2932 relationships)
- **Date verified:** 2026-03-18
- **5/8 PASS, 3 PARTIAL**
- **Issues found (1 — not yet fixed):**
  - **UNFIXED: Primary `Session` class missing from symbols** — `open class Session: @unchecked Sendable` at `Source/Core/Session.swift:30` is absent from the symbol table. Its ~80 methods are extracted as orphaned top-level functions. `extract_class()` in `swift/types.rs` looks for `type_identifier`/`user_type` child but returns `None` for this file. Other classes with `@unchecked Sendable` ARE extracted correctly.
- **Notes:**
  - Protocol/extension extraction works well (URLConvertible, URLRequestConvertible)
  - Session extensions have wildly different centrality (0.00 to 1.00)
  - Test detection works correctly for Swift `Tests/` directory

### Kotlin
- **Reference project:** square/moshi (182 files, 6602 symbols, 5767 relationships)
- **Date verified:** 2026-03-18
- **6/8 PASS, 2 PARTIAL** (after type_usage fix)
- **Issues found (2):**
  - **FIXED: Centrality 0.00 for all core classes** — Added `user_type` handler for Kotlin type annotations. Commit `90bffa2a`.
  - **UNFIXED: Sealed class `JsonReader` not extracted** — `sealed class JsonReader` completely absent from symbols. 30+ member functions extracted as orphaned top-level symbols. `JsonWriter` (also sealed) IS extracted correctly.
- **Notes:**
  - Nested class extraction works well (Builder, LookupChain, companion objects)
  - Cross-language references (Java importing Kotlin) tracked correctly
  - Missing space in `Moshi` class signature: `Moshiprivate constructor(builder: Builder)`

### C
- **Reference project:** jqlang/jq (358 files, 12361 symbols, 3639 relationships)
- **Date verified:** 2026-03-18
- **5/8 PASS, 1 PARTIAL, 1 FAIL** (after type_usage + test detection fix)
- **Issues found (2):**
  - **FIXED: No type_usage identifiers for typedefs** — Added `type_identifier` handler with struct/enum/typedef declaration filter. Commit `90bffa2a`.
  - **FIXED: `*_test.c` not detected** — Added `_test.c`/`_test.cc`/`_test.cpp` to `is_test_path()`. Commit `90bffa2a`.
  - **UNFIXED: Centrality split between header and implementation** — `jq_next` in `execute.c` has centrality 0.00 (26 refs) while the `jq.h` declaration gets 0.80. Ref attribution goes to header, not implementation.

### C++
- **Reference project:** nlohmann/json (1136 files, 15701 symbols, 2342 relationships)
- **Date verified:** 2026-03-18
- **5/8 PASS, 1 PARTIAL, 2 FAIL** (after type_usage fix)
- **Issues found (3):**
  - **FIXED: No type_usage identifiers** — Added `type_identifier` handler + template_type_parameter filter. Commit `90bffa2a`.
  - **UNFIXED: `deep_dive` can't disambiguate constructor overloads** — `basic_json` has 10+ constructors in same file. `context_file` only helps across files, not within.
  - **UNFIXED: Zero cross-file references** — Tests use `json` typedef, not `basic_json` directly. Header-only architecture means most code is in one file.
- **Notes:**
  - YAML false positive: `mkdocs.yml` nav entries indexed as variable definitions
  - Template functions with SFINAE constraints have full signatures preserved

### Dart
- **Reference project:** rrousselGit/riverpod (1805 files, 28106 symbols, 3373 relationships)
- **Date verified:** 2026-03-18
- **6/8 PASS, 2 PARTIAL** (after type_usage fix)
- **Issues found (2):**
  - **FIXED: No type_usage identifiers** — Added `type_identifier` handler with type_alias declaration filter. Commit `90bffa2a`.
  - **UNFIXED: Dart 3 class modifiers dropped** — `ProviderContainer` (`base class`) and `AsyncValue` (`sealed class`) not extracted as class symbols. Extractor only matches `class_definition`; Dart 3's `base`/`sealed`/`final`/`interface` modifiers produce different AST node types.
- **Notes:**
  - Standard `abstract class` and plain `class` declarations extracted correctly
  - Test detection correctly handles `test/` directory for Dart

### Lua
- **Reference project:** rxi/lite (404 files, 27858 symbols, 9167 relationships)
- **Date verified:** 2026-03-18
- **4/8 PASS, 3 PARTIAL, 1 N/A**
- **Issues found (1):**
  - **UNFIXED: Class-like tables stored as `variable` kind** — Lua has no `class` keyword; classes are `Doc = Object:extend()`. These are stored as `variable` kind, which is excluded from centrality Step 1b. The `exit` method (132.0) and `doc` function (36.0) DO get centrality via Step 1a relationships.
- **Notes:**
  - lite project includes C/SDL source code — C symbols dominate centrality (SDL_SCANCODE_TO_KEYCODE: 507.0)
  - No test framework in this project — Check 8 is N/A
  - Lua function extraction works well (`:` and `.` method syntax both captured)

---

## Known Limitations (Accepted)

These are unfixed issues that are either language-inherent or low-severity. They do not block usage but may affect specific features for these languages.

| Language | Limitation | Workaround |
|----------|-----------|------------|
| **C++** | Zero cross-file references in header-only projects (e.g., nlohmann/json) | Most C++ projects with separate `.cpp` files work correctly |
| **C** | Centrality splits between header declaration and implementation | Header gets the references; use `context_file` parameter to reach the implementation |
| **PHP** | Class-level relationship tracking weak for namespace-heavy codebases | Method-level references work; use `language` filter for better results |
| **Ruby** | Centrality accumulates on `constant` symbol instead of `class` | Class is still found via search; centrality ranking is affected |
| **Lua** | Class-like tables stored as `variable` kind (no `class` keyword) | Lua metatables are semantically classes but syntactically variables |
| **Go** | Markdown headings can outrank Go structs in def search without language filter | Use `language="go"` for accurate definition search results |
| **HTML** | Structural language — no navigable symbols extracted | HTML elements are indexed for full-text search but not symbol navigation |
| **CSS** | Structural language — no navigable symbols extracted | CSS selectors are indexed for full-text search but not symbol navigation |
| **JavaScript** | CommonJS `require()` patterns produce fewer relationship edges than ES modules | Centrality may be lower than expected; symbol extraction works correctly |
| **JSON** | Flat config files may produce no symbols | Structured JSON with nested objects extracts correctly |
