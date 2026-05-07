# Swift Verification — Alamofire/Alamofire

**Workspace:** alamofire_3d4cceb5 | **Files:** 521 | **Symbols:** 20552 | **Relationships:** 2932
**Date:** 2026-03-17

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | **PARTIAL** | Extensions, protocols, structs, enums extracted correctly. **BUG: `Session` class declaration missing** — `open class Session: @unchecked Sendable` at Session.swift:30 is not extracted. Its ~80 methods appear as orphaned top-level functions. `DataStreamRequest` class also missing. Other classes (`Request`, `UploadRequest`, `SessionDelegate`, `MultipartUpload`) extracted fine |
| 2. Relationship Extraction | **PARTIAL** | 246 references for `Session` (231 defs, 15 refs). Cross-file relationships work. But all references link to extension declarations, never to the primary class (which is missing). Extension conformances (`RequestDelegate`, `SessionStateProvider`) correctly tracked |
| 3. Identifier Extraction | PASS | `fast_refs` returns definitions (property `let session`, method parameters), calls, and references. `type_usage` filter works — `HTTPMethod` returns 1 ref (definition only), `Request` returns 399 type_usage refs. Reference kinds properly categorized |
| 4. Centrality / Reference Scores | **BUG** | `Session` extensions: centrality ranges from 0.00 to 1.00 across 5 definitions. `extension Session` in WebSocketTests.swift has centrality 1.00 (459 dependents) — highest in codebase. But `extension Session: RequestDelegate` has centrality 0.00 despite 238 dependents. The primary class has no centrality because it doesn't exist as a symbol |
| 5. Definition Search | **PARTIAL** | `fast_search(query="Session", search_target="definitions")` returns extensions and properties but NOT the primary `open class Session` declaration. The class definition at Session.swift:30 is absent from all search results |
| 6. deep_dive Resolution | **PARTIAL** | Returns 5 Session definitions (all extensions). Shows methods, fields, change risk, test coverage. Code bodies render correctly. Semantic similarity finds `SessionDelegate` (0.54-0.57). But resolving to the PRIMARY class is impossible since it's not a symbol |
| 7. get_context Orientation | PASS | Returns 3 pivots (`RetryPolicy`, `Networking`, `DownloadRequest`) + 8 neighbors across 7 files. Full code bodies for pivots. Correctly identifies HTTP networking patterns, retry policy, and request lifecycle. Good orientation quality for the domain |
| 8. Test Detection | PASS | `exclude_tests=false`: finds `testInitializerWithDefaultArguments` in Tests/SessionTests.swift:244. `exclude_tests=true`: returns zero results (correctly excludes). `Tests/` directory properly detected as test code. Test methods (`func test*`) extracted as `method` symbols within test class hierarchy |

## Issues Found

### BUG: Primary `Session` class declaration not extracted (Check 1, 2, 4, 5, 6)

The core `Session` class declaration at `Source/Core/Session.swift:30` is completely absent from the symbol table:

```swift
open class Session: @unchecked Sendable {
```

`get_symbols(file_path="Source/Core/Session.swift", max_depth=0)` returns 5 symbols: 1 import, 4 functions. The functions are actually methods that should be children of the Session class (`webSocketRequest`, `download`), but they appear as top-level orphans.

With `max_depth=1`, 114 symbols are returned — all the class's properties, methods, nested types — but none have `Session` as their parent because the parent was never created.

**Affected symbols also missing:**
- `DataStreamRequest` at `Source/Core/DataStreamRequest.swift:28`: `public final class DataStreamRequest: Request, @unchecked Sendable` — NOT extracted. Its nested types (`Handler`, `Stream`, `Event`, `Completion`) appear as top-level.

**NOT affected (correctly extracted):**
- `Request` at `Source/Core/Request.swift:29`: `public @unchecked class Request: Sendable` — extracted
- `UploadRequest` at `Source/Core/UploadRequest.swift:28`: `public final class UploadRequest: DataRequest, @unchecked Sendable` — extracted (shown as `public @unchecked class UploadRequest`)
- `SessionDelegate` at `Source/Core/SessionDelegate.swift:28`: `open @unchecked class SessionDelegate: NSObject, Sendable` — extracted
- `MultipartUpload` at `Source/Features/MultipartUpload.swift:28`: `final class MultipartUpload: @unchecked Sendable` — extracted
- `NetworkReachabilityManager` at `Source/Features/NetworkReachabilityManager.swift:41`: `open class NetworkReachabilityManager: @unchecked Sendable` — extracted
- `UnfairLock` at `Source/Core/Protected.swift:55`: `final class UnfairLock: Lock, @unchecked Sendable` — extracted
- `JSONParameterEncoder` at `Source/Core/ParameterEncoder.swift:44`: `open class JSONParameterEncoder: @unchecked Sendable, ParameterEncoder` — extracted

**Likely cause:** The `extract_class` method in `types.rs` looks for a child node of kind `type_identifier` or `user_type` to find the class name. For `Session` and `DataStreamRequest`, this lookup returns `None`, causing the entire class to be silently dropped. The tree-sitter Swift grammar may produce a different node structure for these specific files — possibly related to file size (Session.swift is 1457+ lines) or some syntactic pattern that causes the grammar to produce an `ERROR` node or different tree structure. The `@unchecked Sendable` in the inheritance clause is common to both affected and unaffected files, so the inheritance syntax alone is not the differentiator.

**Impact:** `Session` is the most important class in Alamofire — it's the central networking API. Without it as a symbol:
- `deep_dive(symbol="Session")` cannot resolve to the primary class
- All 80+ methods lack a parent, breaking hierarchy navigation
- Centrality cannot be computed for the actual class
- Code intelligence is significantly degraded for Alamofire's core type

### BUG: Inconsistent centrality across Session extensions (Check 4)

The 5 Session extension definitions have wildly inconsistent centrality scores despite all sharing the same incoming reference pool:

| Definition | Location | Centrality | Incoming Refs |
|-----------|----------|-----------|---------------|
| `extension Session` (WebSocket) | Tests/WebSocketTests.swift:729 | **1.00** | 459 |
| `extension Session: RequestDelegate` | Source/Core/Session.swift:1358 | **0.00** | 238 |
| `extension Session: SessionStateProvider` | Source/Core/Session.swift:1415 | 0.29 | 238 |
| `extension Session` (CachedResponse) | Tests/CachedResponseHandlerTests.swift:241 | 0.29 | 238 |
| `extension Session` (TestHelpers) | Tests/TestHelpers.swift:348 | 0.00 | 238 |

The `RequestDelegate` extension has 0.00 centrality with 238 incoming refs, while the WebSocket test extension has 1.00 with 459 refs. This pattern suggests references are not being distributed correctly across extension definitions — some extensions absorb all the centrality while others get none.

### Minor: Test function search requires exact name (Check 8)

Searching for `testThatSession` or `testThat` with `search_target="definitions"` returns zero results, even though many test functions contain these substrings (e.g., `testThatSessionInitializerSucceedsWithDefaultArguments`). Only exact function names like `testInitializerWithDefaultArguments` are found. This is expected Tantivy tokenization behavior (camelCase splitting), but it means discovering test functions by partial name prefix is unreliable.

### Minor: `HTTPMethod` has very few cross-file references (Check 3)

`fast_refs(symbol="HTTPMethod")` returns only 11 references — all within `HTTPMethod.swift` itself (the static property initializers). Despite `HTTPMethod` being used extensively across Alamofire (as parameter types in `Session.request()`, `Session.download()`, etc.), these usages don't appear as identifier references. This is likely because Swift method signatures use type annotations that the identifier extractor doesn't track as references.

## Raw Evidence

### Check 1: get_symbols on Session.swift (max_depth=0, 5 symbols)
```
import Foundation (25-25)
function open func webSocketRequest(...) -> WebSocketRequest (30-528)
function open func webSocketRequest<Parameters>(...) -> WebSocketRequest (530-559)
function open func webSocketRequest(performing:...) -> WebSocketRequest (561-578)
function open func download(...) -> DownloadRequest (602-619)
```
Note: No `class Session` symbol. The `webSocketRequest` function at line 30 occupies the same line as the class declaration.

### Check 1: get_symbols on Session.swift (max_depth=1, 114 symbols)
Key symbols extracted as children of the orphaned functions:
- `static let default` (32)
- `enum RequestSetup` (35-45)
- `let session: URLSession` (53), `let delegate: SessionDelegate` (55)
- `init(session: URLSession)` (129-160), `convenience init(configuration:)` (197-229)
- `deinit` (231-234)
- `func request(...)` (308-324, 357-373, 383-397)
- `func streamRequest(...)` (421-441, 459-477, 491-507)
- `func download(...)` (602-619, 639-656, 668-684, 705-721)
- `func upload(...)` (769-787, 799-809, etc.)
- `extension Session: RequestDelegate` (1358-1411)
- `extension Session: SessionStateProvider` (1415-1457)

### Check 2: fast_refs for Session (246 total)
- 231 definitions (property declarations `let session`, extensions)
- 15 references (calls and references in Source/ and Tests/)
- No definition for the primary `open class Session` itself

### Check 4: deep_dive overview for Session
- 5 definitions found (all extensions, no primary class)
- Best centrality: 1.00 on WebSocket test extension (459 dependents)
- Worst centrality: 0.00 on RequestDelegate extension (238 dependents)
- Test coverage ranges: 0 tests (untested) to 192 tests (thorough)

### Check 5: Definition search for Session
Returns 8 results:
1. Tests/WebSocketTests.swift:729 — `extension Session`
2. Tests/CachedResponseHandlerTests.swift:201 — `private func session(...)`
3. Tests/CachedResponseHandlerTests.swift:241 — `extension Session`
4. Source/Core/Session.swift:1415 — `extension Session: SessionStateProvider`
5. Tests/TestHelpers.swift:348 — `extension Session`
6. Source/Core/Session.swift:1358 — `extension Session: RequestDelegate`
7. Source/Features/RequestInterceptor.swift:33 — `let session: Session`
8. Tests/SessionTests.swift:1400 — `let session`

No primary class definition in results.

### Check 7: get_context for "HTTP request networking"
3 pivots:
- `RetryPolicy` (Source/Features/RetryPolicy.swift:29) — retry policy with exponential backoff
- `Networking` (watchOS Example/.../Networking.swift:28) — example usage
- `DownloadRequest` (Source/Core/DownloadRequest.swift:28) — download request class

8 neighbors including `ConnectionLostRetryPolicy`, `RequestInterceptor`, `download` methods

### Check 8: Test detection
- `testInitializerWithDefaultArguments` found at Tests/SessionTests.swift:244 with `exclude_tests=false`
- Same search with `exclude_tests=true` returns zero results
- `SessionTestCase` class found at Tests/SessionTests.swift:29 with `exclude_tests=false`, filtered with `exclude_tests=true`
- Test file contains 64 symbols: 3 imports, 3 test classes, 4 helper classes, 54 test methods

### Correctly extracted Swift patterns
- **Protocols:** `URLConvertible` (interface), `URLRequestConvertible` (interface), `RequestDelegate` (interface) — all extracted with methods
- **Extensions:** `extension String: URLConvertible`, `extension URL: URLConvertible`, `extension URLRequest: URLRequestConvertible` — conformance extensions extracted
- **Structs:** `HTTPMethod` with static properties (`.get`, `.post`, etc.) — fully extracted
- **Enums:** `Uploadable`, `NetworkReachabilityStatus` — extracted
- **Nested types:** `RequestSetup` enum inside Session area, `Options` struct inside `DownloadRequest`
- **Test classes:** `SessionTestCase: BaseTestCase`, `SessionMassActionTestCase`, `SessionConfigurationHeadersTestCase` — all extracted with test methods
