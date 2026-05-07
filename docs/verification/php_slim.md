# PHP Verification — slimphp/Slim

**Workspace:** slim_dce0015d | **Files:** 145 | **Symbols:** 4031 | **Relationships:** 1555

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | PASS | Classes, methods, interfaces, properties, constants, namespaces, imports all extracted correctly. Type hints preserved (e.g., `?ContainerInterface`, `ResponseFactoryInterface`). `App` class shows 46 symbols including 10 methods, 2 properties, 1 constant, 16 imports, and nested variables. |
| 2. Relationship Extraction | FAIL | `fast_refs(symbol="App")` returns 66 results but ALL are categorized as "Definitions" (variable assignments like `$app = new App(...)`). Zero actual references. `extends`/`implements` relationships not tracked as references. `reference_kind` filter completely ignored — same results for `type_usage`, `import`, and unfiltered. |
| 3. Identifier Extraction | PARTIAL | 66 identifiers found for `App` across multiple files — good cross-file coverage. But ALL classified as variable definitions, none as call/import/type_usage refs. Method-level identifiers DO work (e.g., `handle` has 10 call references). Class-level identifier classification is broken. |
| 4. Centrality / Reference Scores | FAIL | `App` class: centrality 0.00 (0 incoming refs). `Route` class: centrality 0.00. `RouteCollectorProxy`: centrality 0.00. All core PHP classes show zero centrality because `new ClassName()` constructor calls and `use` imports are not tracked as references to the class. Method-level centrality works: `add` method has 0.95 centrality (56 incoming refs). |
| 5. Definition Search | PASS | `fast_search(query="App", search_target="definitions")` returns `Slim/App.php:39` as the #1 result. Real class definition ranks above variable assignments. Namespace-qualified search `Slim\App` also works. |
| 6. deep_dive Resolution | PASS | `deep_dive(symbol="App", depth="context")` correctly resolves to `Slim/App.php:39`, lists all 10 methods with full signatures, shows extends/implements, provides code body. Change risk MEDIUM (0.50) — would be higher if centrality were correct. |
| 7. get_context Orientation | PASS | `get_context(query="HTTP routing middleware")` returns `RoutingMiddleware` as pivot with full code, `add` method from `RouteGroup` as second pivot (high centrality). Neighbors include `App.add`, `App.addRoutingMiddleware`, `App.addBodyParsingMiddleware`, `App.addErrorMiddleware`. Good conceptual navigation. |
| 8. Test Detection | PASS | `exclude_tests=false`: `AppTest` class found at `tests/AppTest.php:56`. `exclude_tests=true`: `AppTest` correctly excluded (0 results). Test methods properly extracted — 53 `test*` methods in `AppTest`. PHPUnit conventions recognized. |

## Issues Found

### BUG 1: PHP class-level references not tracked (Centrality = 0.00) — SEVERITY: HIGH

**All PHP classes have 0 incoming references and 0.00 centrality**, even core classes like `App`, `Route`, and `RouteCollectorProxy` that are instantiated dozens of times across the codebase.

**Root cause:** `new ClassName(...)` constructor calls in PHP are extracted as variable definitions (`$app = new App(...)` → variable `$app`) but the `App` class itself is NOT recorded as a reference target. Similarly, `use Slim\App` import statements and `extends RouteCollectorProxy` / `implements RequestHandlerInterface` are extracted as import/class-level metadata but NOT as incoming references to the target class.

**Impact:** Centrality-based ranking for PHP classes is completely broken. All PHP classes appear equally unimportant (centrality 0.00). Change risk assessment is undervalued — `App` shows MEDIUM risk instead of what should be HIGH/CRITICAL. `fast_refs` for a class name returns only variable assignment sites, not actual usage sites.

**Comparison:** Method-level references work correctly — `handle` has 10 call refs, `add` has 56 call refs with 0.95 centrality.

### BUG 2: `reference_kind` filter ignored for PHP — SEVERITY: MEDIUM

`fast_refs(symbol="App", reference_kind="type_usage")` and `fast_refs(symbol="App", reference_kind="import")` both return the same 66 variable definitions as an unfiltered call. The `reference_kind` parameter has no effect on PHP results. This may be a consequence of BUG 1 — if no actual references are stored, there's nothing to filter.

### BUG 3: `get_symbols` returns duplicate methods with `target` filter — SEVERITY: LOW

`get_symbols(file_path="tests/AppTest.php", target="test")` returns 106 symbols where every test method appears twice. Without the `target` filter (or with `target="AppTest"`), the same file correctly returns 55 symbols with no duplicates. The duplication appears to be a `target` filter issue that matches symbols in two passes.

## Raw Evidence

### Check 1: Symbol Extraction — `get_symbols` on `Slim/App.php`
```
Slim/App.php — 46 symbols
  namespace namespace Slim (11-11)
  import use Psr\Container\ContainerInterface (13-13)
  import use Psr\Http\Message\ResponseFactoryInterface (14-14)
  ...16 total imports...
  class class App extends RouteCollectorProxy implements RequestHandlerInterface (39-226)
    constant public const VERSION = '4.15.1' (46-46)
    property protected RouteResolverInterface $routeResolver (48-48, protected)
    property protected MiddlewareDispatcherInterface $middlewareDispatcher (50-50, protected)
    constructor public function __construct(...) (55-80)
    method public function getRouteResolver(): RouteResolverInterface (85-88)
    method public function getMiddlewareDispatcher(): MiddlewareDispatcherInterface (93-96)
    method public function add($middleware): self (102-106)
    method public function addMiddleware(MiddlewareInterface $middleware): self (112-116)
    method public function addRoutingMiddleware(): RoutingMiddleware (125-133)
    method public function addErrorMiddleware(...): ErrorMiddleware (145-161)
    method public function addBodyParsingMiddleware(array $bodyParsers = []): BodyParsingMiddleware (170-175)
    method public function run(?ServerRequestInterface $request = null): void (186-196)
    method public function handle(ServerRequestInterface $request): ResponseInterface (207-225)
```

Interfaces also extracted correctly:
```
Slim/Interfaces/RouteCollectorProxyInterface.php — 21 symbols
  namespace namespace Slim\Interfaces (11-11)
  interface interface RouteCollectorProxyInterface (21-126)
    method public function getResponseFactory(): ResponseFactoryInterface (23-23)
    ...16 interface methods...
```

### Check 2: Relationship Extraction — `fast_refs(symbol="App")`
```
66 references to "App":
Definitions (66):
  tests/AppTest.php:1443 (variable) → $app = new App(...)
  tests/AppTest.php:732 (variable) → $app = new App(...)
  ... all 66 are variable definitions, ZERO actual references ...
  Slim/App.php:39 (class) → class App extends RouteCollectorProxy implements RequestHandlerInterface
```

Compare with method-level refs that DO work:
```
21 references to "handle":
Definitions (11): ... method definitions across files ...
References (10): ... call sites in test files ...
```

### Check 4: Centrality Scores
```
App:                  centrality: 0.00 (0 incoming refs)  ← WRONG
Route:                centrality: 0.00 (0 incoming refs)  ← WRONG
RouteCollectorProxy:  centrality: 0.00 (0 incoming refs)  ← WRONG
add (method):         centrality: 0.95 (56 incoming refs) ← CORRECT
```

### Check 5: Definition Search
```
fast_search(query="App", search_target="definitions"):
  #1: Slim/App.php:39 (class) — CORRECT ranking
  #2-8: variable assignments in test files
```

### Check 7: get_context
```
get_context(query="HTTP routing middleware"):
  PIVOT: RoutingMiddleware (Slim/Middleware/RoutingMiddleware.php:25) — full code
  PIVOT: add (Slim/Routing/RouteGroup.php:74) — high centrality
  NEIGHBORS: App.add, App.addRoutingMiddleware, App.addBodyParsingMiddleware,
             App.addErrorMiddleware, RouteGroup.appendMiddlewareToDispatcher,
             Route.add
```

### Check 8: Test Detection
```
fast_search(query="AppTest", exclude_tests=false): Found tests/AppTest.php:56 (class AppTest)
fast_search(query="AppTest", exclude_tests=true):  No results (correctly excluded)
fast_search(query="ErrorMiddlewareTest", exclude_tests=false): Found tests/Middleware/ErrorMiddlewareTest.php:28
fast_search(query="ErrorMiddlewareTest", exclude_tests=true):  No results (correctly excluded)
```
