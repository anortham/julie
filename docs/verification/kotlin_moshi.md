# Kotlin Verification — square/moshi

**Workspace:** moshi_c9c5a600 | **Files:** 182 | **Symbols:** 6602 | **Relationships:** 5767
**Date:** 2026-03-17

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | **PARTIAL** | `Moshi` class (87 symbols in file) extracted correctly with nested `Builder`, `LookupChain`, `Lookup`, `companion object`. Annotations (`JsonQualifier`, `Json`) extracted as classes. `Types` object extracted. `JsonAdapter` abstract class extracted. **BUG: `JsonReader` sealed class missing from symbols entirely** — its members (30+ functions, properties, constructors) and nested `Options` class are extracted, but the parent sealed class itself is absent |
| 2. Relationship Extraction | PASS | 130 refs for `Moshi` (107 defs, 22 imports, 1 call). Cross-file relationships working across Kotlin and Java files. `JsonWriter` sealed class shows 2 Extends relationships (from `-JsonUtf8Writer` and `-JsonValueWriter`). Import refs correctly tracked from Java example files |
| 3. Identifier Extraction | PASS | `fast_refs` with `type_usage` filter returns 129 refs for `Moshi` and 99 refs for `JsonAdapter`. Definitions, imports, and call-site refs all categorized. Cross-language refs (Java imports referencing Kotlin classes) working |
| 4. Centrality / Reference Scores | **MIXED** | `Moshi` class: centrality 0.30 (1 incoming ref from `build()` method). `JsonWriter` sealed class: 0.34 (2 Extends refs). `JsonAdapter` abstract class: **0.00 (0 incoming refs)** despite 99 references. `Types` object: 0.00 (0 incoming refs). Pattern: centrality only counts Extends/Calls relationships, not imports or type_usage references |
| 5. Definition Search | PASS | `fast_search(query="Moshi", search_target="definitions")` returns the `Moshi` class definition at Moshi.kt:45 plus namespace declarations and a constant. Class definition is present in results. Namespace results (`package com.squareup.moshi`) rank first, which is a known pattern |
| 6. deep_dive Resolution | PASS | `deep_dive(symbol="Moshi", depth="context")` returns full class body (45 lines shown + 334 more), lists all 5 fields, 11 methods, Builder as a semantically similar symbol (0.51). Change risk correctly computed at MEDIUM (0.45). Test coverage: 254 tests (thorough). **BUG: `deep_dive(symbol="JsonReader")` returns "No symbol found"** — consequence of the missing sealed class extraction |
| 7. get_context Orientation | PASS | Returns 2 pivots (`KotlinJsonAdapterFactory`, `ClassJsonAdapter`) + 2 neighbors across 3 files. Pivots show full code bodies. Factory pattern (adapter creation via `create()` method) correctly identified as central to "JSON type adapter" concept. Good orientation quality |
| 8. Test Detection | PASS | `exclude_tests=false`: finds `MoshiTest` class at `moshi/src/test/java/com/squareup/moshi/MoshiTest.java:53` (Java test class). `exclude_tests=true`: returns zero results (correctly excludes). Java/Kotlin `src/test/` layout properly detected |

## Issues Found

### BUG: Sealed class `JsonReader` not extracted as a symbol (Check 1, Check 6)

The `JsonReader` sealed class in `moshi/src/main/java/com/squareup/moshi/JsonReader.kt` is completely absent from the symbol table. `get_symbols` returns 46 symbols from this file including:
- 8 imports
- 1 namespace
- 6 properties (protected/public/private)
- 2 constructors
- 30+ functions (abstract and concrete)
- 1 nested class (`Options`)

But the parent `JsonReader` sealed class itself is missing. All member symbols are extracted as if they were top-level, orphaned from their enclosing class.

Evidence:
- `get_symbols(file_path="moshi/.../JsonReader.kt", max_depth=0)` — no class named `JsonReader` in output
- `deep_dive(symbol="JsonReader", context_file="JsonReader.kt")` — returns "No symbol found"
- `fast_search(query="JsonReader", search_target="definitions")` — only returns import statements, no class definition

**Impact:** Navigation to `JsonReader` is broken. Code intelligence features (callers, hierarchy, centrality) cannot reference it. Any code that extends `JsonReader` cannot be linked to the parent.

**Likely cause:** The Kotlin extractor may not handle `public sealed class` declarations, or the specific declaration pattern in this file (which has a large KDoc comment and is followed by a `Closeable` interface implementation) isn't matched by the tree-sitter query.

**Note:** `JsonWriter` (also a sealed class) IS extracted correctly — `deep_dive(symbol="JsonWriter")` works and returns the class with 12 fields and 24 methods. The difference may be in the exact syntax or file structure. Both files use `public sealed class ... : Closeable, Flushable` pattern but JsonReader.kt has additional complexity (the `Token` enum, `Options` nested class).

### BUG: Centrality 0.00 for `JsonAdapter` despite 99 references (Check 4)

Same pattern as seen in Java/Gson verification. `JsonAdapter` has 99 references (77 property definitions, 22 imports) but reports `centrality: 0.00 (0 incoming refs)`. Only relationship-based references (Extends, Calls) contribute to centrality, not identifier-based references (imports, type_usage).

Centrality summary:
- **`Moshi`** class: 0.30 (1 incoming ref — from `build()` Calls relationship)
- **`JsonWriter`** sealed class: 0.34 (2 incoming refs — from Extends relationships)
- **`JsonAdapter`** abstract class: 0.00 (0 incoming refs — 99 fast_refs references ignored)
- **`Types`** object: 0.00 (0 incoming refs — 26 test references ignored)

### Minor: `Moshi` class signature has formatting artifact (Check 1)

The extracted class signature reads `public class Moshiprivate constructor(builder: Builder)` — the space between `Moshi` and `private` is missing. Should be `public class Moshi private constructor(builder: Builder)`. This is a cosmetic extraction issue in the signature text.

### Minor: Companion object extraction (Check 1)

Companion objects are extracted as `class companion object` entries in `get_symbols`. The `Moshi` file's companion object (lines 363-404) containing `BUILT_IN_FACTORIES` and factory methods is correctly extracted with its members. The `Json` annotation's companion object with `UNSET_NAME` constant is also correctly extracted. This pattern works well.

### Minor: Annotation class extraction (Check 1)

Kotlin annotation classes (`@annotation class JsonQualifier`, `@annotation class Json`) are extracted as `class` kind symbols, which is reasonable. Their properties (like `Json.name` and `Json.ignore`) and companion objects are correctly nested. The `@Target`, `@Retention`, and `@MustBeDocumented` meta-annotations are included in the signature.

## Raw Evidence

### Check 1: get_symbols on Moshi.kt (87 symbols)
```
class public class Moshiprivate constructor(builder: Builder) (45-405)
  property val builder: Builder (45-45)
  property private val factories = buildList { ... } (46-49, private)
  property private val lastOffset = builder.lastOffset (50-50, private)
  property private val lookupChainThreadLocal (51-51, private)
  property private val adapterCache (52-52, private)
  method adapter (x8 overloads) — lines 55-168
  method nextAdapter — line 170
  method newBuilder — line 190
  method private cacheKey — line 203
  class public class Builder (207-251)
    method addAdapter, add (x5 overloads), addLast (x4), build
  class private inner class LookupChain (271-346)
  class private class Lookup<T> (349-361)
  class companion object (363-404)
    property BUILT_IN_FACTORIES
    method newAdapterFactory (x2 overloads)
```

### Check 1: Missing JsonReader — get_symbols evidence
```
get_symbols(JsonReader.kt, max_depth=0) returns 46 symbols:
  namespace, 8 imports, 6 properties, 2 constructors,
  20+ functions, 1 nested class (Options)
  NO class entry for JsonReader itself
```

### Check 4: Centrality evidence
```
Moshi class:       centrality: 0.30 (1 incoming ref)  — only build() Calls relationship counted
JsonAdapter class: centrality: 0.00 (0 incoming refs) — 99 fast_refs references ignored
JsonWriter class:  centrality: 0.34 (2 incoming refs) — 2 Extends relationships counted
Types object:      centrality: 0.00 (0 incoming refs) — 26 test references ignored
```

### Check 8: Test detection evidence
```
exclude_tests=false → MoshiTest found at moshi/src/test/java/com/squareup/moshi/MoshiTest.java:53
exclude_tests=true  → 0 results (correctly filtered out)
```
Java/Kotlin `src/test/` path convention correctly recognized for both `.java` and `.kt` files.
