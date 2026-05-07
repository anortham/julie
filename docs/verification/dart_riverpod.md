# Dart Verification — rrousselGit/riverpod

**Workspace:** riverpod_a7fdc041 | **Files:** 1805 | **Symbols:** 28106 | **Relationships:** 3373
**Date:** 2026-03-17

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | **PARTIAL** | 50 symbols from provider_container.dart; methods, factory constructors, properties extracted. Class `ProviderContainer` itself NOT extracted as a class symbol — only its factory constructor appears as `function`. Mixins extracted correctly (MyMixin). Abstract classes (ConsumerWidget, ConsumerState) extracted correctly. |
| 2. Relationships | PASS | 18 references to ProviderContainer across 6 files; cross-file refs to benchmarks, website docs, test packages |
| 3. Identifiers | PASS | 3 definitions found + 15 call-site references; reference kinds correctly tagged (Calls for constructor invocations) |
| 4. Centrality | PASS | Factory constructor centrality=1.00 (1364 refs), static containerOf centrality=0.47 (1256 refs) — correct gradient based on usage |
| 5. Definition Search | **BUG** | `ProviderContainer` found as `function` (factory constructor) not `class`. The class declaration itself is missing from definitions. Also: 3rd definition is a property `get providerContainer` from analyzer_utils — correctly found. |
| 6. deep_dive Resolution | PASS | Both definitions resolved; callers, callees, test locations all populated; body code returned with line numbers |
| 7. get_context Orientation | PASS | 2 pivots (watch, ProviderTransformer) + 7 neighbors; pivots are core Dart source symbols, not docs. Good orientation for "provider state management" |
| 8. Test Detection | PASS | `exclude_tests=false`: 5 results from test directories. `exclude_tests=true`: 0 results — all 5 results were correctly in `test/` paths, so filtering is accurate. No false positives or negatives detected. |

**Overall: 6 PASS, 2 PARTIAL/BUG (Dart 3 class modifier extraction)**

## Issues Found

### Issue 1: ProviderContainer class declaration not extracted as a class symbol (Check 1 & 5, significant)

The Dart class `ProviderContainer` is defined in `packages/riverpod/lib/src/core/provider_container.dart`. The `get_symbols` output for this file shows 50 symbols but **no class-level symbol for ProviderContainer**. Instead, the factory constructor `factory ProviderContainer({...})` appears as a `function` at line 811.

The class declaration itself (likely around line 700-810 based on the factory being at 811) is missing from the symbol list entirely. This means:
- `fast_search(query="ProviderContainer", search_target="definitions")` returns it as `function`, not `class`
- `deep_dive` reports `kind: function` instead of `kind: class`
- The class body, fields, and non-factory methods are not grouped under a class parent

**Compare with ConsumerWidget** which IS correctly extracted as `class` at `consumer.dart:232` with child methods `build` and `createState` nested under it. The difference is that ConsumerWidget is `abstract class ConsumerWidget extends ConsumerStatefulWidget` (standard class declaration) while ProviderContainer likely uses a more complex pattern (possibly sealed class or class with extensive doc comments/annotations).

**Impact:** Medium — agents searching for "class ProviderContainer" won't find the class definition itself, only its factory constructor. The factory constructor still provides useful entry-point info, but class-level metadata (fields, inheritance, all methods) is lost.

**Root cause hypothesis:** The Dart extractor (`crates/julie-extractors/src/dart/mod.rs`) only matches `class_definition` nodes (line 70). Riverpod uses Dart 3 class modifiers (`base class`, `final class`, `sealed class`) which the `harper-tree-sitter-dart` v0.0.5 grammar may emit as a different node type (e.g., `class_definition` with a `class_modifier` child, or an entirely different node kind). The extractor never sees the class declaration, so only the inner `factory_constructor_signature` (line 122) gets extracted — but as a standalone function with no parent class. All other methods in the class body are also extracted as standalone functions (no parent_id), which is why `get_symbols` returns them as flat `function` entries rather than `method` entries nested under a class.

**Confirmed evidence:**
- `ConsumerWidget` (`abstract class ConsumerWidget extends ...`) IS correctly extracted as `class` — standard `abstract` modifier works
- `UncontrolledProviderScope` (`class UncontrolledProviderScope extends StatefulWidget`) IS correctly extracted — plain class works
- `_UncontrolledProviderScopeState` (`final class _UncontrolledProviderScopeState`) — need to verify if `final class` is also broken
- `ProviderContainer` (likely `base class ProviderContainer`) is NOT extracted
- `AsyncValue` (likely `sealed class AsyncValue`) is NOT extracted
- `ProviderPointerManager` in same file shows `function class ProviderPointerManager(...)` — the word "class" leaks into the function signature, suggesting the tree-sitter node is being misclassified

### Issue 2: AsyncValue not extracted as a class from Dart source (Check 1, moderate)

`deep_dive(symbol="AsyncValue")` found 3 definitions — all from **CHANGELOG.md** files (as `module` kind), not from the actual Dart source file `packages/riverpod/lib/src/core/async_value.dart`. The `get_symbols` call on that file shows 26 symbols (methods like `map`, `when`, `whenData`, extensions) but no top-level `AsyncValue` class symbol.

This is likely the same root cause as Issue 1 — the `AsyncValue` class uses Dart 3 `sealed class` syntax which the tree-sitter grammar may not fully support, causing the class declaration to be skipped during extraction.

**Impact:** Searching for `AsyncValue` returns only markdown changelog entries as definitions, not the actual sealed class. The methods within the class ARE extracted (map, when, whenData, etc.) but lack the parent class container.

### Issue 3: No type_usage references for ProviderContainer (Check 3, minor)

`fast_refs(symbol="ProviderContainer", reference_kind="type_usage")` returned only the 3 definitions (no actual type_usage references). In Dart, `ProviderContainer` is heavily used as a type annotation (parameter types, field types, return types). These type_usage references appear to be missing — all 15 non-definition references were categorized as `Calls` (constructor invocations).

**Impact:** Low — call-site references are the most important for navigation. But type annotation usages (e.g., `final ProviderContainer container;`) are not captured as type_usage, which could affect "find all type usages" workflows.

## Raw Evidence

### Check 1: get_symbols — provider_container.dart structure
```
packages/riverpod/lib/src/core/provider_container.dart — 50 symbols
  property get container (10-10)
  function String toString() (231-231)
  function class ProviderPointerManager(...) (274-280)    ← nested class, extracted as function
  function factory ProviderPointerManager(...) (284-288)
  function void _initializeProviderOverride(...) (321-321, private)
  ...many methods...
  function factory ProviderContainer({...}) (811-816)      ← factory, not class
  function static defaultRetry(...) (831-837)
  function Future<void> pump() async (895-895)
  function StateT read<StateT>(...) (916-916)
  function void dispose() (1150-1150)
  function String toString() (1153-1153)
```
- ProviderContainer appears only as `factory ProviderContainer({...})` — kind=function, not class
- ProviderPointerManager also appears as function, not class
- Methods (read, dispose, pump, listen, invalidate, refresh) all extracted as standalone functions
- No parent-child class hierarchy visible

### Check 1: get_symbols — consumer.dart (GOOD extraction for comparison)
```
packages/flutter_riverpod/lib/src/core/consumer.dart — 28 symbols
  type typedef ConsumerBuilder = Widget Function(...) (7-8)
  class abstract class ConsumerWidget extends ConsumerStatefulWidget (232-279)
    method Widget build(BuildContext context, WidgetRef ref) (274-274)
    method @override _ConsumerState createState() (278-278)
  class class _ConsumerState extends ConsumerState<ConsumerWidget> (281-284)
  class abstract class ConsumerStatefulWidget extends StatefulWidget (316-325)
  class abstract class ConsumerState<WidgetT> extends State<WidgetT> (357-361)
```
- ConsumerWidget correctly extracted as `class` with child methods
- Abstract classes, typedefs, generic type params all handled
- Private classes (_ConsumerState) identified

### Check 1: get_symbols — ref.dart (mixed results)
```
packages/riverpod/lib/src/core/ref.dart — 36 symbols
  module extension $RefArg on Ref (5-11)          ← extension extracted as module
    property get $arg (7-7)
    property get $element (10-10)
  class class UnmountedRefException implements Exception (14-30)  ← class correct
    constructor UnmountedRefException() (15-15)
    field final ProviderBase origin (17-17)
    method @override String toString() (20-20)
  class class $Ref<StateT, ValueT> extends Ref (730-767)   ← class correct
    constructor $Ref() (732-732)
    property get element (735-735)
    field final ProviderElement _element (738-738, private)
  class class KeepAliveLink (771-780)              ← class correct
```
- Regular classes (UnmountedRefException, $Ref, KeepAliveLink) extracted correctly with children
- Extensions extracted as `module` — reasonable mapping
- Standalone functions in the file extracted correctly

### Check 1: get_symbols — sync.g.dart (generated code)
```
packages/riverpod_generator/test/integration/sync.g.dart — 20 symbols
  function String toString() (32-32)
  function Override overrideWithValue(...) (53-53)
  function List<ValueT> build(ValueT param) (337-337)
  function void runBuild() (340-340)
```
- Generated .g.dart files ARE indexed (symbols extracted)
- Functions and methods inside generated code captured

### Check 1: Mixin extraction
```
fast_search(query="mixin", search_target="definitions", language="dart"):
  packages/riverpod/test/src/matrix/notifier_mixin.dart:8
    mixin MyMixin<StateT, ValueT, RandomT> on AnyNotifier<StateT, ValueT> {}
```
- Dart mixins found via search, appear in definition results

### Check 2 & 3: fast_refs — ProviderContainer
```
18 references to "ProviderContainer":
  Definitions (3):
    provider_container.dart:811 (function) → factory ProviderContainer({...})
    provider_container.dart:9 (property) → get providerContainer
    provider_scope.dart:84 (function) → static ProviderContainer(...)
  References (15): ALL in benchmarks/ directory, ALL tagged as (Calls)
    benchmarks/lib/add_listener.dart:15,22,26,42,63
    benchmarks/lib/create_bench.dart:21,28,45,62
    benchmarks/lib/read_bench.dart:15,22,27,42,60
    benchmarks/lib/remove_listener.dart:15
```
Cross-file references work correctly. Call sites properly identified.

### Check 2 & 3: fast_refs — ConsumerWidget
```
12 references to "ConsumerWidget":
  Definitions (2):
    consumer.dart:232 (class) ← correctly typed as class
    convert_to_widget_utils.dart:27 (enummember)
  References (10): mix of Uses and References in consumer_test.dart
```

### Check 4: Centrality gradient
```
ProviderContainer (factory):  centrality=1.00, 1364 incoming refs, risk=HIGH (1.00)
ProviderContainer (static):  centrality=0.47, 1256 incoming refs, risk=HIGH (0.81)
ConsumerWidget:               centrality=0.42,    1 incoming refs, risk=HIGH (0.77)
```
- Factory constructor has highest centrality in the workspace — makes sense as the main entry point
- ConsumerWidget centrality seems low (0.42 with only 1 incoming ref) despite being a widely-used base class — may indicate that most usages are via `extends` in user code outside the indexed workspace

### Check 5: Definition search — ProviderContainer
```
Definition found: ProviderContainer
  provider_container.dart:811 (function, public) — factory ProviderContainer({...})
  provider_scope.dart:84 (function, public) — static ProviderContainer(...)
  provider_container.dart:9 (property, public) — get providerContainer

Other matches:
  provider_scope.dart:412 (content match)
  visit_states_test.dart:295 (content match)
  provider_scope.dart:178 (content match)
  create_bench.dart:97 (content match)
  read_bench.dart:99 (content match)
```
Factory constructor ranks #1 (highest centrality) — correct priority. But labeled as function, not class.

### Check 6: deep_dive context — ProviderContainer
```
Two definitions disambiguated:
1. provider_scope.dart:84 (function) — static containerOf method
   Callers: 8 shown of 1256; includes initState, website examples
   Referenced by: 7 testing files
   Test locations: 10 files across riverpod_generator tests
   Body: 7 lines returned (lines 81-90)

2. provider_container.dart:811 (function) — factory constructor
   Callers: 15 shown of 1359; includes website docs, test files
   Callees: 1 (container property)
   Test locations: 10 files
   Body: 10 lines returned (lines 808-819)
```
Resolution works well — callers, callees, test locations all populated. Disambiguation prompt shown when multiple matches exist.

### Check 7: get_context — "provider state management"
```
Context "provider state management" | pivots=2 neighbors=7 files=6
PIVOT watch (ref.dart:652) — kind=function centrality=medium risk=HIGH
PIVOT ProviderTransformer (provider_listenable_transformer.dart:26) — kind=class centrality=low

NEIGHBORS: _throwIfInvalidUsage, _element, invalidateSelf, main (x4 benchmark files)
```
- Pivots are core Dart source symbols (watch method, ProviderTransformer class)
- No markdown/doc files in pivots — good
- The `watch` method is a highly relevant result for "provider state management"
- ProviderTransformer is a reasonable secondary pivot

### Check 8: Test detection
```
exclude_tests=false: 5 results
  riverpod_lint_flutter_test/test/lints/provider_dependencies/missing_dependencies...fix.dart:287
  riverpod_generator/test/integration/sync.dart:234
  flutter_riverpod/test/providers/change_notifier/change_notifier_provider_test.dart:10
  riverpod_lint_flutter_test/test/lints/provider_dependencies/missing_dependencies.dart:286
  riverpod_lint_flutter_test/test/lints/provider_dependencies/missing_dependencies...fix.dart:287

exclude_tests=true: 0 results
```
All 5 results are in `test/` directories — correctly identified as test files. With `exclude_tests=true`, all filtered out. This is correct behavior since all matching definitions happened to be in test paths.
