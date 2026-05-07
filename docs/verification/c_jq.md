# C Verification — stedolan/jq

**Workspace:** jq_13566b9e | **Files:** 358 | **Symbols:** 12361 | **Relationships:** 3639
**Date:** 2026-03-17

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | PASS | 139 symbols from jv.h, 58 from jv.c; functions, typedefs, structs, macros (#define), imports, enums, unions all extracted |
| 2. Relationships | PASS | Cross-file refs found; `jq_next` has 15 call refs from jq_test.c, `jv_get_kind` has refs from builtin.c, compile.c, linker.c |
| 3. Identifiers | PASS | `jq_compile` refs span src/jq_test.c, tests/jq_fuzz_compile.c, tests/jq_fuzz_execute.cpp — cross-file .c/.h/.cpp refs working |
| 4. Centrality | **PARTIAL** | `jv_get_kind` in jq.h correctly gets centrality=1.00 (381 refs). But `jq_next` in execute.c gets centrality=0.00 despite 26 refs, while same symbol in jq.h gets 0.80 (see issue 1). `jv_kind` typedef gets centrality=0.00 with 0 refs despite pervasive type usage (see issue 2) |
| 5. Definition Search | PASS | `jq_next` search returns both header declaration (jq.h:30) and implementation (execute.c:340) as top results; content matches from fuzz tests and util.c also present |
| 6. deep_dive Resolution | PASS | Full body returned (640+ lines), 173 callees listed, 26 callers listed, semantic similarity found jq_util_input_next_input (0.63) |
| 7. get_context Orientation | PASS | 3 pivots (usage, process, jq_parse) + 28 neighbors; `process` correctly identified as the main value-processing loop calling jq_next, jv_get_kind, jv_string, etc. |
| 8. Test Detection | **BUG** | `exclude_tests=false` returns results from jq_test.c as expected. `exclude_tests=true` returns **identical results** — jq_test.c symbols are NOT filtered out (see issue 3) |

**Overall: 5 PASS, 1 PARTIAL, 1 BUG (test exclusion broken for jq_test.c), plus 1 type_usage gap**

## Issues Found

### Issue 1: Centrality=0.00 for .c implementation despite 26 incoming refs (Check 4, bug)

`jq_next` defined in `src/execute.c:340` reports `centrality: 0.00 (26 incoming refs)`. The same symbol declared in `src/jq.h:30` correctly reports `centrality: 0.80 (26 incoming refs)`. Both have the same number of dependents but only the header declaration gets a centrality score.

This pattern repeats: `jv_get_kind` in `jv.c:98` gets centrality=0.83 (284 refs) while the `.h` declaration gets centrality=1.00 (381 refs). The .c file gets a score here but it's lower — some refs are attributed only to the .h declaration.

**Impact:** Medium. The .c implementation definitions are undervalued in search ranking. deep_dive still finds them but centrality-boosted search will prefer the header-only declaration.

**Root cause hypothesis:** Reference resolution may be splitting callers between the .h declaration and .c definition rather than consolidating them. The .h declaration absorbs more refs (possibly from files that `#include` the header), leaving the .c definition with fewer attributed refs. In the extreme case of `jq_next` in execute.c, it somehow gets 0.00 despite showing 26 refs in the text.

### Issue 2: jv_kind typedef has zero type_usage references (Check 4, gap)

`jv_kind` is jq's central enum type, used as a return type for `jv_get_kind()` and a parameter type for `jv_kind_name()`. Yet `fast_refs(symbol="jv_kind", reference_kind="type_usage")` returns zero references, and overall `fast_refs` returns only 1 result (the definition itself).

**Impact:** Medium. The `jv_kind` type appears invisible to the reference graph despite being one of the most fundamental types in the codebase. Its centrality is 0.00 as a result.

**Root cause hypothesis:** The C extractor may not be capturing typedef/enum usages in return types and parameter types as `type_usage` relationships. When a C function is declared as `jv_kind jv_get_kind(jv)`, the `jv_kind` return type should generate a type_usage reference but apparently doesn't.

### Issue 3: exclude_tests=true does NOT filter jq_test.c (Check 8, bug)

When searching `fast_search(query="test", search_target="definitions", exclude_tests=true)`, results from `src/jq_test.c` are still returned. The file is named `jq_test.c` which follows the common C test convention (`*_test.c`). With `exclude_tests=false`, results are identical.

**Impact:** High for C codebases. Agents relying on `exclude_tests=true` to focus on production code will still get test file results polluting their results.

**Root cause hypothesis:** The test detection heuristic may not recognize `jq_test.c` as a test file. Possible patterns it checks: files under `tests/` directory, files named `test_*.c`, but maybe not `*_test.c` in `src/`. The file `src/jq_test.c` lives in the main source directory rather than a `tests/` subdirectory, so directory-based detection would miss it. The `*_test.c` naming pattern (common in C and Go) may not be in the heuristic.

### Issue 4: jq_state shows 10 separate "definitions" (Check 1, cosmetic)

`deep_dive(symbol="jq_state")` returns 10 definitions — 8 from execute.c and 2 from jq.h. In C, `jq_state` is declared once as a forward declaration (`typedef struct jq_state jq_state;` in jq.h:17) and defined once as a full struct in execute.c. The 10 hits likely include every parameter declaration that mentions `jq_state *` as a separate "definition."

**Impact:** Low. Disambiguation via `context_file` works, and deep_dive still resolves correctly. But the inflated definition count could confuse agents trying to find THE definition.

**Root cause hypothesis:** The C extractor may be treating struct type references in parameter positions as definitions rather than as type_usage references.

## Raw Evidence

### Check 1: get_symbols — src/jv.h structure
```
src/jv.h — 139 symbols
  constant #define JV_H (2-3)
  import #include <stdarg.h> (4-5)
  import #include <stdint.h> (5-6)
  import #include <stdio.h> (6-7)
  namespace extern "C" (9-275)
    type typedef struct jv_kind (19-28)         ← enum typedef extracted
    struct struct jv_refcnt (30-30)             ← struct extracted
    union typedef struct jv (34-43)             ← union extracted
    function jv_kind jv_get_kind(jv) (50-50)    ← function declaration
    function jv jv_string(const char*) (124-124)
    ...90+ more functions...
    enum enum jv_print_flags { JV_PRINT_PRETTY, JV_PRINT_ASCII, JV_PRINT_COLOR } (218-230)
    constant #define JV_PRINT_INDENT_FLAGS(n) ... (231-233)   ← macro extracted
    type typedef struct jv_nomem_handler_f (249-249)           ← function pointer typedef
    struct typedef struct jv_parser (254-254)
```
- SymbolKinds: `type` for typedefs, `struct` for structs, `union` for unions, `enum` for enums, `constant` for #define macros, `function` for declarations, `import` for #include, `namespace` for extern "C"
- 139 symbols is comprehensive for a 275-line header file

### Check 1: get_symbols — src/jv.c structure
```
src/jv.c — 58 symbols (depth=1, limit=50)
  import #include "jv.h" (45-46)
  struct typedef struct jv_refcnt (53-55)
    field int count (54-54)                   ← struct fields extracted
  function static void jvp_refcnt_inc(jv_refcnt* c) (59-61, private)  ← static = private
  constant #define KIND_MASK 0xF (73-74)
  type typedef struct payload_flags (77-80)
  constant #define JVP_MAKE_FLAGS(kind, pflags) ... (83-84)   ← parameterized macro
  function jv_kind jv_get_kind(jv x) (98-100)
  function char* jv_kind_name(jv_kind k) (102-115)
  variable static const jv JV_NULL = ... (117-117, private)   ← static const variable
  function jv jv_invalid_with_msg(jv err) (149-156)
    variable jvp_invalid* i = jv_mem_alloc(sizeof(jvp_invalid)) (150-150)  ← local vars at depth=1
```
- Private detection working: `static` functions/variables marked `(private)`
- Struct fields extracted at depth=1
- Local variables inside functions extracted at depth=1

### Check 2 & 3: fast_refs — jq_next
```
17 references to "jq_next":
  Definitions (2):
    src/execute.c:340 (function) → jv jq_next(jq_state *jq)
    src/jq.h:30 (function) → jv jq_next(jq_state *)
  References (15):
    src/jq_test.c:201 (Calls)
    src/jq_test.c:343 (Calls)
    ...13 more from jq_test.c...
```
- Both .h declaration and .c definition found
- All 15 references are call sites in jq_test.c
- Note: main.c:179 call not in this list (limit=15 may have excluded it, or it's attributed to the .h definition)

### Check 2 & 3: fast_refs — jv_kind
```
1 references to "jv_kind":
  Definition: src/jv.h:19 (type) → typedef struct jv_kind
```
- Only the definition itself — zero usage references
- No type_usage refs despite jv_kind being used as return type in jv_get_kind, jv_kind_name, etc.

### Check 2 & 3: fast_refs — jv_get_kind
```
17 references to "jv_get_kind":
  Definitions (2): src/jv.h:50, src/jv.c:98
  References (15): all Calls from src/builtin.c (lines 63-444)
```
- Cross-file .c→.h refs working for function calls

### Check 2 & 3: fast_refs — jq_compile
```
12 references to "jq_compile":
  Definitions (2): src/jq.h:26, src/execute.c:1260
  References (10): jq_test.c (5 calls), tests/jq_fuzz_compile.c, tests/jq_fuzz_execute.cpp, tests/jq_fuzz_fixed.cpp
```
- Cross-directory refs working (src/ → tests/)
- Cross-language refs working (.c → .cpp fuzz tests)

### Check 4: deep_dive overview — jv_get_kind
```
src/jv.h:50 — centrality: 1.00 (381 incoming refs) — HIGHEST in codebase
src/jv.c:98 — centrality: 0.83 (284 incoming refs)
```
- Header declaration absorbs more refs than implementation
- Both get non-zero centrality (unlike jq_next in execute.c)

### Check 4: deep_dive overview — jq_next
```
src/jq.h:30 — centrality: 0.80 (26 incoming refs)
src/execute.c:340 — centrality: 0.00 (26 incoming refs)  ← BUG
```
- Same ref count but .c gets zero centrality

### Check 5: fast_search definitions — jq_next
```
Definition found: jq_next
  src/jq.h:30 (function, public)
  src/execute.c:340 (function, public)
Other matches: src/util.c:405, tests/jq_fuzz_fixed.cpp:281, tests/jq_fuzz_execute.cpp:32, src/compile.c:24, src/jq.h:31, src/util.c:254
```
- Both .h and .c definitions returned as top results
- Content matches from usage sites also present

### Check 6: deep_dive context — jq_next (execute.c)
```
Body: 640+ lines (the main interpreter loop)
Callers (15 of 26): all from jq_test.c
Callees (15 of 173): stack_restore, dump_operation, frame_current, set_error, stack_pop, stack_push, jv_string...
Semantically Similar (4):
  jq_next                   0.91  src/jq.h:30
  jq_util_input_next_input  0.63  src/jq.h:68
  jq_util_input_next_input_cb 0.55  src/util.c:356
```
- Full function body returned with context
- 173 callees is substantial — this is jq's main execution engine
- Semantic similarity correctly identifies related input-processing functions

### Check 7: get_context — "jq value processing"
```
Context "jq value processing" | pivots=3 neighbors=28 files=11
PIVOT usage src/main.c:49 (function, centrality=medium)
PIVOT process src/main.c:175 (function, centrality=medium)
  → calls jq_next, jv_get_kind, jv_string, jv_dump, jv_free, etc.
PIVOT jq_parse src/parser.c:4162 (function, centrality=medium)
NEIGHBOR jv_get_kind, jv_copy, jv_free, jv_string, jq_start, jq_next... (28 total)
```
- `process()` is the correct central function for "jq value processing"
- Neighbors include all the key jv_* and jq_* functions
- Reasonable pivot selection covering CLI usage, processing loop, and parsing

### Check 8: Test Detection
```
exclude_tests=false: 10 results
  src/builtin.c:931 (definition: int test = ...)
  src/jq_test.c:11, 572, 95, 525, 521, 12, 277, 546, 302

exclude_tests=true: 10 results — IDENTICAL
  src/builtin.c:931
  src/jq_test.c:11, 572, 95, 525, 521, 12, 277, 546, 302
```
- jq_test.c results NOT filtered despite exclude_tests=true
- src/jq_test.c follows the *_test.c naming convention
- jq_test.c lives in src/ not tests/ — directory-based heuristic may miss it

### Check 8: get_symbols — src/jq_test.c
```
src/jq_test.c — 30 symbols (depth=0)
  function void jv_test(void) (11-11, private)
  function void run_jq_tests(...) (12-12, private)
  function void run_jq_start_state_tests(void) (13-13, private)
  function void run_jq_compile_args_tests(void) (14-14, private)
  function void run_jq_recompile_tests(void) (15-15, private)
  function void run_jq_exhaust_and_reuse_tests(void) (16-16, private)
  function int jq_testsuite(...) (21-61)
  ...test helpers and test runners...
```
- All symbols correctly extracted from the test file
- Functions named with test conventions (run_jq_tests, jv_test, etc.)
- File is clearly a test file by both name (*_test.c) and content
