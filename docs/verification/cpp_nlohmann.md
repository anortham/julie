# C++ Verification: nlohmann-json

**Workspace:** `nlohmann-json_a5f86cd4` (1136 files, 15701 symbols, 2342 relationships)
**Date:** 2026-03-17
**Verdict:** PASS with minor issues

---

## Check 1: Symbol Extraction

**Query:** `get_symbols` on `include/nlohmann/json.hpp` (the main header)

**Result: PASS (with observations)**

Returned 71 symbols at depth=1, 50 at depth=0. Extracted symbols include:

| Symbol Kind | Examples | Count |
|-------------|----------|-------|
| class | `basic_json` (lines 77-705) | 1 |
| function | `set_parents`, `basic_json` constructors, `dump`, `type`, `is_*`, `get_impl_ptr`, `parse` | ~40+ |
| operator | `operator=` (copy/move assignment) | 1 |
| destructor | `~basic_json()` | 1 |
| variable | `using lexer`, `using serializer`, `using value_t`, `using parse_event_t` | ~5 |
| import | `using std::swap` | 1 |

**Findings:**
- The `basic_json` class itself is correctly extracted as a `class` symbol at line 77
- Constructor overloads (10 total) are all extracted as separate `function` symbols named `basic_json`
- Template parameters on functions are preserved in signatures
- The `operator=` is extracted with kind `operator` -- good C++ support
- The destructor `~basic_json()` is extracted with kind `destructor` -- good
- `using` declarations are extracted as `variable` or `import` -- reasonable mapping
- Static factory methods (`binary`, `array`, `object`, `parse`) correctly extracted
- Type query methods (`is_null`, `is_boolean`, etc.) all present

**Observation:** The class `basic_json` appears at line 77 but its symbol kind shows as "class" only at depth=1 with nested members. At depth=0 it still appears. Namespaces (`nlohmann`, `detail`) are NOT extracted as separate namespace symbols -- the class has a long combined declaration name that includes the macro `NLOHMANN_JSON_NAMESPACE_BEGIN NLOHMANN_BASIC_JSON_TPL_DECLARATION`. This is not ideal but reflects how C++ macros expand namespace blocks.

---

## Check 2-3: Relationships & Identifiers

**Query:** `fast_refs(symbol="basic_json", limit=15)`

**Result: PARTIAL PASS**

Found 26 total references:
- **11 definitions** (constructor overloads + 1 `mkdocs.yml` variable)
- **15 call references** (all in `json.hpp` lines 3732-3980)

**Positive findings:**
- All constructor overloads correctly linked as definitions
- Call references found within the same header file (static methods calling constructors)
- Definition count (11) correctly captures all constructor overloads

**Issues found:**

1. **BUG: No cross-file references detected.** All 15 call references are within `include/nlohmann/json.hpp` itself. The workspace has 1136 files including many test files that use `basic_json` extensively, yet no references from test files appear. This suggests cross-file relationship extraction is limited for C++ -- likely because test files use the `json` typedef rather than `basic_json` directly.

2. **BUG: `type_usage` references return only definitions, zero actual type usages.** When filtering by `reference_kind="type_usage"`, the result returns 11 definitions but 0 references. `basic_json` is used as a type throughout the codebase (e.g., `const basic_json& other`, template parameters), but none are captured as type_usage relationships.

3. **Spurious definition in docs/mkdocs/mkdocs.yml:102.** The word `basic_json` in a YAML navigation config is indexed as a `variable` definition. This is a false positive from the YAML extractor picking up a documentation navigation entry.

---

## Check 4: Centrality

**Query:** `deep_dive(symbol="basic_json", depth="overview")`

**Result: FAIL (disambiguation wall)**

Returns "Found 11 definitions of 'basic_json'. Use context_file to disambiguate." but does NOT return centrality scores or overview information. The tool requires disambiguation but provides no centrality data in the disambiguation listing.

When filtering by `context_file="json.hpp"`, still returns 10 definitions (all constructor overloads in the same file) with no centrality information.

**Issue:** The `basic_json` class is the single most important symbol in this codebase but has no centrality score reported. The problem is that `basic_json` as a class (line 77) is not found as a standalone definition -- only its constructors are. The tree-sitter extractor appears to extract the class declaration and its constructor functions as separate symbols, but when searching for `basic_json` by name, only constructors match (since the class declaration includes macro prefixes in its name).

**Compare with `json_pointer`:** The `json_pointer` constructor at line 62 reports centrality=0.67 (13 incoming refs). This works because there's a single constructor. The class definition at `json_fwd.hpp:56` reports centrality=0.00 despite having 9 incoming refs, suggesting the class symbol's centrality is not aggregated from its constructor usages.

---

## Check 5: Definition Search

**Query:** `fast_search(query="basic_json", search_target="definitions", limit=8)`

**Result: PASS**

Returns 8 definition matches, all constructor overloads in `json.hpp`. Signatures are complete and accurate:
- `basic_json(const value_t v)` -- value type constructor
- `basic_json(std::nullptr_t = nullptr) noexcept` -- null constructor
- `basic_json(CompatibleType && val)` -- template compatible type constructor
- `basic_json(const BasicJsonType& val)` -- cross-type constructor
- `basic_json(initializer_list_t init, ...)` -- initializer list constructor
- `basic_json(size_type cnt, const basic_json& val)` -- count constructor
- `basic_json(InputIT first, InputIT last)` -- iterator range constructor
- `basic_json(const basic_json& other)` -- copy constructor

Template parameters and SFINAE constraints are preserved in the output -- good for C++ template-heavy code.

**Observation:** The class definition itself (`class basic_json`) does not appear in definition search results for the query "basic_json". Only constructors are returned. This is because the class declaration name includes macro prefixes that make it not match cleanly as "basic_json".

---

## Check 6: deep_dive Resolution

**Query:** `deep_dive(symbol="basic_json", depth="context", context_file="json.hpp")`

**Result: FAIL (same disambiguation wall)**

Returns the same 10-definition disambiguation list even with `context_file="json.hpp"`. The `context_file` parameter doesn't help because all 10 definitions are in the same file.

**Root cause:** C++ constructor overloads all share the class name. Julie's deep_dive cannot disambiguate between them since they're all in `json.hpp`. There's no way to select "the class definition" vs "a specific constructor overload" because the class itself isn't indexed with the plain name `basic_json`.

**Workaround tested:** `deep_dive(symbol="json_pointer")` works when `context_file` disambiguates between files, but fails the same way for multiple definitions in the same file. The `json_pointer` deep_dive at `overview` depth successfully returned caller/callee information and centrality scores for all 4 definitions.

---

## Check 7: get_context

**Query:** `get_context(query="JSON parsing serialization")`

**Result: PASS**

Returns 3 pivots + 1 neighbor across 4 files:

| Symbol | File | Kind | Quality |
|--------|------|------|---------|
| `CAPTURE` (CBOR roundtrip) | `tests/src/unit-cbor.cpp:1959` | function | Good -- shows `json::from_cbor` / `json::to_cbor` roundtrip |
| `LLVMFuzzerTestOneInput` | `tests/src/fuzzer-parse_json.cpp:30` | function | Good -- shows `json::parse` / `j.dump()` parse-serialize cycle |
| `roundtrip` | `tests/src/unit-unicode1.cpp:230` | function | Good -- shows JSON parse/dump/roundtrip with Unicode |
| `parse` (neighbor) | `include/nlohmann/json.hpp:4075` | function | Good -- the actual `parse` static method signature |

**Assessment:** The results are semantically relevant -- all three pivots demonstrate JSON parsing and serialization workflows. The `parse` function from the main header appears as a neighbor with its signature. The token budget is well used.

**Minor observation:** Pivots are all from test/fuzzer files rather than the core implementation. This makes sense for a header-only library where the "implementation" is all in headers and the tests are the primary `.cpp` files.

---

## Check 8: Test Detection

**Query 1:** `fast_search(query="TEST_CASE", search_target="definitions", exclude_tests=false, limit=5)`
**Result:** 5 matches found, all in `tests/src/` directory:
- `tests/src/unit-allocator.cpp:19`
- `tests/src/unit-udt.cpp:874`
- `tests/src/unit-merge_patch.cpp:14`
- `tests/src/unit-allocator.cpp:114`
- `tests/src/unit-regression2.cpp:422`

**Query 2:** `fast_search(query="TEST_CASE", search_target="definitions", exclude_tests=true, limit=5)`
**Result:** 0 matches (empty result)

**Result: PASS**

Test detection works correctly:
- With `exclude_tests=false`: finds TEST_CASE macros in test files
- With `exclude_tests=true`: correctly filters out ALL test file results, returning nothing
- The `tests/` directory is properly recognized as a test directory
- The filter is binary and complete -- no test results leak through

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | PASS | 71 symbols extracted; classes, functions, operators, destructors all captured |
| 2-3. Relationships & Identifiers | PARTIAL PASS | Cross-file refs missing; type_usage returns 0 refs; YAML false positive |
| 4. Centrality | FAIL | Disambiguation wall; no centrality for `basic_json` |
| 5. Definition Search | PASS | All constructor overloads found with full template signatures |
| 6. deep_dive Resolution | FAIL | Cannot disambiguate same-file constructor overloads |
| 7. get_context | PASS | Semantically relevant pivots showing parse/serialize workflows |
| 8. Test Detection | PASS | `exclude_tests` correctly filters `tests/` directory |

### Bugs Found

1. **[Medium] deep_dive disambiguation fails for same-file overloads.** When a symbol has multiple definitions in the same file (C++ constructor overloads), `context_file` cannot disambiguate and the tool returns a listing instead of analysis. This blocks centrality reporting and context-depth exploration for the most important symbol in the codebase.

2. **[Medium] No cross-file references for `basic_json`.** All 15 references are intra-file. Test files use `json` (the typedef) not `basic_json`, so this may be "correct" behavior, but it means the core class template has zero detected external usage despite being the entire API surface.

3. **[Low] `type_usage` reference kind returns 0 results for `basic_json`.** The type is used extensively in parameter types, template arguments, and variable declarations, but no type_usage relationships are captured.

4. **[Low] YAML false positive.** `docs/mkdocs/mkdocs.yml:102` is indexed as a `variable` definition of `basic_json`, which is just a YAML navigation entry for documentation.

5. **[Low] Class definition not searchable by plain name.** The `basic_json` class at line 77 has a compound declaration name including C preprocessor macros (`NLOHMANN_JSON_NAMESPACE_BEGIN NLOHMANN_BASIC_JSON_TPL_DECLARATION`), making it unsearchable as simply "basic_json" in definition search.
