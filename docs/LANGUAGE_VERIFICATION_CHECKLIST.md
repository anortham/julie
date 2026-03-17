# Language Verification Checklist

> Systematic verification that each tree-sitter extractor meets quality standards.
> Run against a **real-world open-source project** for each language — unit tests are necessary but not sufficient (the Elixir pending-relationships bug proved that).

## How to Use This Checklist

For each language, index a real-world project as a reference workspace and run the verification queries below. A language passes when all applicable checks produce correct results.

**Pick representative projects:**
- Rust: Julie itself, ripgrep, or tokio
- TypeScript: Next.js, Prisma, or tRPC
- Python: FastAPI, Django, or Flask
- Elixir: Phoenix framework
- Scala: Cats, Akka, or Play
- Go: Kubernetes, Docker, or Hugo
- Java: Spring Boot, Guava
- C#: ASP.NET Core, Newtonsoft.Json
- etc.

---

## Tier Classification

| Tier | Languages | Expected Capabilities |
|------|-----------|----------------------|
| **Full** | Rust, TypeScript, JavaScript, Python, Java, C#, PHP, Ruby, Swift, Kotlin, Scala, Go, C, C++, Elixir, Dart, Zig, Lua, GDScript | All checks below |
| **Specialized** | Bash, PowerShell, Vue, Razor, QML, R, SQL, HTML, CSS | Symbols + identifiers; relationships where applicable |
| **Data/Docs** | Markdown, JSON, TOML, YAML, Regex | Basic structure extraction only |

---

## Verification Checks

### 1. Symbol Extraction (All Tiers)

```
get_symbols(file_path="<core_file>", max_depth=1, mode="structure")
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **1.1** Top-level symbols extracted | Modules/classes/functions at file top | Missing `defmodule Phoenix.Router` |
| **1.2** Nested symbols captured | Methods inside classes, functions inside modules | Missing `def add` inside `defmodule Calculator` |
| **1.3** Correct SymbolKind | Module=Module, Class=Class, Function=Function, etc. | Elixir module stored as Import instead of Module |
| **1.4** Correct visibility | Public/private/protected where language supports it | All symbols marked `unknown` |
| **1.5** Qualified names correct | Dot-qualified names stored as-is (Elixir: `Phoenix.Router`, not just `Router`) | Name truncated to last component |
| **1.6** Import/use directives captured | `use`, `import`, `require`, `alias` as Import kind | Missing `use Phoenix.Router` |
| **1.7** Type annotations preserved | `@spec`, `: Type`, `-> ReturnType` stored | Signature field empty |
| **1.8** Reasonable symbol count | Compare against manual count or another tool | 3 symbols extracted from 500-line file |

### 2. Relationship Extraction (Full Tier)

```
# Same-file relationships
fast_refs(symbol="<function_in_file>")

# Cross-file relationship test (THE CRITICAL ONE)
fast_refs(symbol="<widely_used_module>")
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **2.1** Same-file calls detected | `multiply() → add()` creates Calls relationship | No relationships in file with function calls |
| **2.2** Cross-file uses/imports detected | `use Phoenix.Router` creates Uses relationship to Router module | Relationship points to local Import symbol, not real module |
| **2.3** Implements/extends detected | `defimpl Printable`, `extends FlatMap` creates Implements/Extends | Missing protocol implementation relationships |
| **2.4** **Pending relationships populated** | `get_pending_relationships()` returns non-empty for cross-file refs | **Empty pending (the Elixir bug)** |
| **2.5** Relationship count reasonable | Compare `manage_workspace(stats)` relationship count against expectations | 3,860 relationships for 24k-symbol project (should be ~10k+) |

### 3. Identifier Extraction (Full + Specialized Tiers)

```
fast_refs(symbol="<core_symbol>")
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **3.1** Definition found | `fast_refs` returns a "Definition:" entry | No definition line in output |
| **3.2** Import references found | `use`/`import` statements appear as imports | Missing import references |
| **3.3** Call-site references found | Function call locations appear as references | Missing call references |
| **3.4** Reference count reasonable | Core module/class has 10+ references | Only 2 references for heavily-used module |

### 4. Centrality / Reference Scores (Full Tier)

```
deep_dive(symbol="<core_module>", depth="overview")
# Check the centrality value in the output
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **4.1** Core modules have high centrality | `centrality > 0.5` for main entry points | **centrality: 0.00 for Phoenix.Router (the Elixir bug)** |
| **4.2** Utility functions have moderate centrality | Helpers have 0.1-0.5 | All symbols at 0.00 |
| **4.3** Leaf functions have low centrality | Internal-only functions near 0.00 | Every symbol at 1.00 (broken normalization) |
| **4.4** Change risk reflects centrality | High-centrality → HIGH risk | MEDIUM risk for module with 65 dependents |

### 5. Definition Search (Full Tier)

```
fast_search(query="<ModuleName>", search_target="definitions")
fast_search(query="<Qualified.Name>", search_target="definitions")
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **5.1** Unqualified name finds definitions | Searching "Router" surfaces `defmodule Router` | Only imports/test modules, not the real definition |
| **5.2** Qualified name finds module | Searching "Phoenix.Router" surfaces `defmodule Phoenix.Router` as #1 | Only `use` statements, not the defmodule |
| **5.3** Definition kind promoted | Actual definition (module/class/trait) ranks above imports | 5 import results before the actual class |
| **5.4** Module kind included | Searching finds Module-kind symbols | Module excluded from definition promotion |

### 6. deep_dive Resolution (Full Tier)

```
deep_dive(symbol="<Qualified.Name>", depth="overview")
deep_dive(symbol="<Qualified.Name>", depth="context", context_file="<file>")
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **6.1** Qualified name resolves to correct symbol | `Phoenix.Router` → Elixir module, not JS homonym | **JS Channel class returned instead of Elixir module** |
| **6.2** context_file disambiguates | Specifying file path narrows to correct result | context_file ignored, wrong language returned |
| **6.3** Exports/methods listed | Module children appear in output | Empty "Public exports" section |
| **6.4** Used-by shows dependents | Incoming references listed | "Used by (0)" for heavily-used module |
| **6.5** Semantic similarity works | Related symbols shown at bottom | No similar symbols section |

### 7. get_context Orientation (Full Tier)

```
get_context(query="<domain concept>")
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **7.1** Pivots are from source language | Query about "routing" returns Elixir pivots, not JS | All pivots from bundled JS files |
| **7.2** Pivots are high-centrality | Core modules selected as pivots | Helper functions chosen over core modules |
| **7.3** Neighbors provide context | Callers/callees shown as neighbors | Empty neighbor list |

### 8. Test Detection (Full Tier)

```
fast_search(query="<test_function>", exclude_tests=false)
# Check is_test field or verify exclusion with exclude_tests=true
```

| Check | What to verify | Failure example |
|-------|---------------|-----------------|
| **8.1** Test functions detected by name | `test_*`, `*_test`, `*Test`, `*Spec` marked as test | Test function not marked |
| **8.2** Test files detected by path | Files in `test/`, `tests/`, `spec/` paths marked | Test file symbols not excluded |
| **8.3** Test decorators detected | `@test`, `#[test]`, `@Test` annotation marks symbol | Decorated test not detected |
| **8.4** Auto-exclusion works | `exclude_tests=auto` hides tests for NL queries | Test results polluting definition search |

---

## Language-Specific Gotchas

| Language | Known Issue to Watch For |
|----------|------------------------|
| **Elixir** | Dot-qualified module names (`Phoenix.Router`) — must not split into parent/child |
| **Scala** | Companion objects (`object Monad`) vs traits (`trait Monad`) — both should be found |
| **PHP** | Namespace-qualified names (`App\Http\Controller`) — backslash separator |
| **C#** | `.Tests` project convention, DI patterns where constructor gets all refs |
| **Java** | `src/main/java/` vs `src/test/java/` layout detection |
| **Go** | `*_test.go` file convention, no class hierarchy |
| **Ruby** | Modules as namespaces (`Module::Class`), RSpec `describe`/`it` blocks |
| **Python** | `__init__.py` module detection, decorator-based test detection |
| **Rust** | `::` separator for qualified names, macro invocations as symbols, scoped paths (`crate::module::func()`) are implicit imports not captured by identifier extraction (see TODO.md sentrux ideas) |
| **JS/TS** | CommonJS vs ESM exports, bundled files (`.min.js`) polluting results |

---

## Running the Verification

```bash
# 1. Add reference workspace
manage_workspace(operation="add", path="/path/to/project", name="project")

# 2. Check stats
manage_workspace(operation="stats", workspace_id="project_XXXX")

# 3. Run checks 1-8 against the workspace
# Use the queries above, substituting real symbol names from the project

# 4. Record results in a tracking issue or table
```

**Passing criteria:**
- **Full Tier**: All 8 check groups pass (30 individual checks)
- **Specialized Tier**: Checks 1, 3, 5, 8 pass (where applicable)
- **Data/Docs Tier**: Check 1 passes

---

*Last updated: 2026-03-17 | Created from dogfood testing of Scala (Cats) and Elixir (Phoenix)*
