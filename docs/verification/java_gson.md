# Java Verification — google/gson

**Workspace:** gson_db105883 | **Files:** 305 | **Symbols:** 8511 | **Relationships:** 7327
**Date:** 2026-03-17

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | PASS | 92 symbols from Gson.java — class, constructors, methods, fields, imports, namespace all extracted with correct kinds and type annotations |
| 2. Relationship Extraction | PASS | 175 refs for `Gson` (60 defs, 109 imports, 6 call-site refs). `JsonElement` has 5 Extends relationships (JsonArray, JsonNull, JsonObject, JsonPrimitive, CustomSubclass). Cross-file relationships working |
| 3. Identifier Extraction | PASS | Definition, import, and call-site refs all found. 175 total for `Gson`, 41 for `JsonElement`, 53 for `TypeAdapter`. Counts are reasonable for a core library class |
| 4. Centrality / Reference Scores | **FAIL** | All core classes show centrality 0.00 — `Gson` (0.00), `TypeAdapter` (0.00), `GsonBuilder` (0.00). Only `JsonElement` constructor shows 0.52 (from 5 Extends refs). The class-level centrality for `Gson` should be very high given 175 references, but reports 0.00 with "0 incoming refs" |
| 5. Definition Search | PASS | `fast_search(query="Gson", search_target="definitions")` returns the `Gson` class definition (line 135, 222, 226) mixed with namespace results. The real class does appear. `package com.google.gson` namespaces rank first due to matching the query, which is arguably correct |
| 6. deep_dive Resolution | PASS | `deep_dive` with `context` depth correctly lists all 26 fields, 35 methods, constructors, and nested class `FutureTypeAdapter`. Code body shown. Change risk correctly computed. Test coverage reported as "93 tests (best: thorough, worst: stub)" |
| 7. get_context Orientation | PASS | Returns 2 pivots + 3 neighbors across 5 files. Pivots: `serialize` method from `JsonSerializationContext`, `testJsonObjectSerialization` test. Neighbors: `excludeField`, `write`, `serialize` from ProtoTypeAdapter. Reasonable orientation for "JSON serialization" query |
| 8. Test Detection | PASS | `exclude_tests=false`: finds `GsonTest` class at `gson/src/test/java/com/google/gson/GsonTest.java:41`. `exclude_tests=true`: returns zero results (correctly excludes the test class). Java `src/test/java/` layout properly detected |

## Issues Found

### BUG: Centrality scores are 0.00 for all core classes (Check 4)

This is the most significant finding. Every core class shows `centrality: 0.00 (0 incoming refs)`:

- **`Gson`** class: 0.00 centrality despite 175 references (109 imports + 60 property definitions + 6 call refs)
- **`TypeAdapter`** class: 0.00 centrality despite 53 references (47 imports + 5 defs + 1 call ref)
- **`GsonBuilder`** class: 0.00 centrality despite being used extensively
- **`JsonElement`** class: 0.00 centrality (but its *constructor* correctly shows 0.52 from 5 Extends relationships)

The pattern is clear: **class-level centrality is not being computed from import/type_usage references**. Only the `JsonElement()` constructor gets centrality (0.52) because it has 5 direct `Extends` relationships — those are relationship-based, not identifier-based.

This means the centrality computation is likely not counting import references or field-type usages toward the class symbol's incoming reference count. The `fast_refs` tool clearly finds 175 references for `Gson`, but the centrality system reports "0 incoming refs" for the same symbol.

**Impact:** Search ranking for Java codebases will not benefit from centrality boosting. The Gson class — the single most important symbol in this entire codebase — gets no centrality advantage over a leaf helper class.

### Minor: Definition search ranking (Check 5)

When searching `fast_search(query="Gson", search_target="definitions")`, namespace declarations (`package com.google.gson`) rank above the actual `Gson` class definition. This is technically correct (the query "Gson" matches the package name) but not ideal for user intent. With working centrality, the class would likely outrank these. This is a downstream consequence of the centrality bug.

### Minor: fast_search for "class Gson" returns unexpected results (Check 1)

The `fast_search(query="class Gson", search_target="definitions")` query returns content-level matches rather than the actual `Gson` class definition as the top result. The top hit is a line inside `DefaultTypeAdaptersTest.java` that happens to mention "class" and "Gson" in context. The actual `Gson` class definition does not appear in the top 8 results. This appears to be because `search_target="definitions"` with multi-word queries may not be filtering to only definition-type results, or the definition ranking is not prioritizing exact matches.

## Raw Evidence

### Check 1: get_symbols on Gson.java (92 symbols)
```
class public final class Gson (135-1265)
  constant private static final String JSON_NON_EXECUTABLE_PREFIX (137)
  property @SuppressWarnings("ThreadLocalUsage") private final ThreadLocal<...> (150-152)
  ... (26 fields total)
  constructor public Gson() (222-224)
  constructor Gson(GsonBuilder builder) (226-257, private)
  method public GsonBuilder newBuilder() (266-268)
  method public <T> TypeAdapter<T> getAdapter(TypeToken<T> type) (319-378)
  ... (35 methods total)
  class static class FutureTypeAdapter<T> extends SerializationDelegatingTypeAdapter<T> (1216-1253, private)
```
Symbol kinds: class, constructor, method, property, constant, import, namespace — all present and correct.
Type annotations preserved: generics (`TypeToken<T>`, `TypeAdapter<T>`), `@Deprecated`, `@SuppressWarnings`, `@Override`, visibility modifiers.

### Check 4: Centrality evidence
```
Gson class:        centrality: 0.00 (0 incoming refs) — but fast_refs finds 175 references
TypeAdapter class: centrality: 0.00 (0 incoming refs) — but fast_refs finds 53 references
GsonBuilder class: centrality: 0.00 (0 incoming refs) — but fast_refs finds extensive usage
JsonElement class: centrality: 0.45 (0 incoming refs) — non-zero but says "0 incoming refs" (contradictory)
JsonElement():     centrality: 0.52 (5 incoming refs) — constructor correctly gets Extends-based centrality
```

### Check 8: Test detection evidence
```
exclude_tests=false → GsonTest found at gson/src/test/java/com/google/gson/GsonTest.java:41
exclude_tests=true  → 0 results (correctly filtered out)
```
Java `src/test/java/` path convention correctly recognized.
