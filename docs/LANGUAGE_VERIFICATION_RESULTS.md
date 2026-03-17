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
| TypeScript | — | — | — | — | — | — | — | — | — | — |
| Python | — | — | — | — | — | — | — | — | — | — |
| Go | — | — | — | — | — | — | — | — | — | — |
| Java | — | — | — | — | — | — | — | — | — | — |
| PHP | — | — | — | — | — | — | — | — | — | — |
| Ruby | — | — | — | — | — | — | — | — | — | — |
| C# | — | — | — | — | — | — | — | — | — | — |
| Swift | — | — | — | — | — | — | — | — | — | — |
| Kotlin | — | — | — | — | — | — | — | — | — | — |
| C | — | — | — | — | — | — | — | — | — | — |
| C++ | — | — | — | — | — | — | — | — | — | — |
| Dart | — | — | — | — | — | — | — | — | — | — |
| Lua | — | — | — | — | — | — | — | — | — | — |
| Scala | — | — | — | — | — | — | — | — | — | — |
| Elixir | — | — | — | — | — | — | — | — | — | — |

### Specialized Tier (9 languages)

| Language | Reference Project | 1. Symbols | 3. Identifiers | 5. Def Search | 8. Test Detection | Date |
|----------|------------------|-----------|---------------|--------------|------------------|------|
| QML | KDE/kirigami | PASS* | PASS* | PASS* | PASS* | 2026-03-17 |
| Razor | dotnet/blazor-samples (9.0) | PASS* | PASS | PASS | N/A | 2026-03-17 |
| Bash | — | — | — | — | — | — |
| PowerShell | — | — | — | — | — | — |
| Vue | — | — | — | — | — | — |
| R | — | — | — | — | — | — |
| SQL | — | — | — | — | — | — |
| HTML | — | — | — | — | — | — |
| CSS | — | — | — | — | — | — |

### Data/Docs Tier (5 languages)

| Language | Reference Project | 1. Symbols | Date |
|----------|------------------|-----------|------|
| Markdown | — | — | — |
| JSON | — | — | — |
| TOML | — | — | — |
| YAML | — | — | — |
| Regex | — | — | — |

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
