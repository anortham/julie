# Ruby Verification — sinatra/sinatra

**Workspace:** sinatra_86eed2fe | **Files:** 289 | **Symbols:** 6919 | **Relationships:** 1661

## Results

| Check | Result | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | PASS | Modules, classes, methods, imports, properties, constants all extracted. Nesting correct (Sinatra > Base > methods). 69 symbols in base.rb at depth=1. |
| 2. Relationship Extraction | PASS | 153 references found for `Base`. Cross-file refs, includes (Implements), extends, imports (`require 'sinatra/base'`), and call sites all detected. |
| 3. Identifier Extraction | PASS | 112 definitions, 21 imports, 20 references across the codebase. Includes variable assignments (`base = Class.new(Sinatra::Base)`), constants, and class definitions. |
| 4. Centrality / Reference Scores | MIXED | `route` method has centrality 1.00 (22 refs) -- correct, it is the most-connected routing symbol. `Base` constant at line 379 has centrality 0.55 (1 ref). However, the `Base` **class** at line 971 shows centrality 0.00 despite being the core class. See Issue 1. |
| 5. Definition Search | PASS | `fast_search(query="Base", search_target="definitions")` returns `Sinatra::Base` class at line 971 and constant at line 379 as top results. Rack::Protection::Base also surfaces. Ranking is reasonable. |
| 6. deep_dive Resolution | PASS | `deep_dive(symbol="Base", context_file="lib/sinatra/base.rb", depth="context")` returns 5 definitions from the file with full method listings (21 methods), field listings (48), includes/extends relationships, and code body. Disambiguates correctly when multiple `Base` symbols exist across files. |
| 7. get_context Orientation | PASS | Returns 3 pivots (`route`, `route` (multi_route), `process_route`) and 32 neighbors. Covers HTTP verbs (get/post/put/delete/head/patch/link/unlink/options), route compilation, pattern matching, and WebDAV extensions. Excellent orientation for understanding Sinatra routing. |
| 8. Test Detection | PASS | `exclude_tests=false` returns 5 results from `test/contest.rb`. `exclude_tests=true` returns 0 results -- all correctly excluded. Test files in `test/` and `spec/` directories are properly classified. |

## Issues Found

### Issue 1: Class definition centrality=0.00 despite being the most-referenced symbol (MEDIUM)

`Sinatra::Base` (class, line 971) shows `centrality: 0.00 (0 incoming refs)` in deep_dive, yet `fast_refs` finds 153 references to "Base" including 20 cross-file reference relationships (Extends) and 21 imports. The **constant** `Base` at line 379 has centrality 0.55, but the **class** definition at line 971 -- which is the actual core symbol -- shows 0.00.

This appears to be a centrality calculation issue where the class definition does not accumulate the reference counts that its constant identifier does. The references are landing on the `constant` version of `Base` rather than the `class` version. Both exist at line 971 (the class definition creates both a class symbol and a constant symbol), but only the constant at line 379 (a usage site inside `mime_type`) has centrality 0.55.

**Impact:** The core `Sinatra::Base` class doesn't get the search ranking boost it deserves from centrality. It still appears in search results because the text match is strong, but the ranking metadata is wrong.

### Issue 2: Qualified name lookup returns wrong definition (LOW)

`fast_refs(symbol="Sinatra::Base")` returns only 1 result: `test/test_helper.rb:37` (a class reopening), not the original definition at `lib/sinatra/base.rb:971`. The qualified name resolution finds the test helper's reopened class instead of the primary definition. Searching the unqualified name `Base` works fine.

### Issue 3: Duplicate symbols at same line (COSMETIC)

`deep_dive` shows both a `class` and a `constant` symbol at `lib/sinatra/base.rb:971` for `Base`. Similarly `lib/sinatra/base.rb:2085` has both `class` and `constant` for `Application`. This is technically correct for Ruby (class definition creates a constant binding), but it creates noise in results -- 21 disambiguation candidates for "Base" when the user likely wants the class.

## Raw Evidence

### Check 1: get_symbols on lib/sinatra/base.rb (depth=1, structure mode)
```
69 symbols extracted including:
- module Sinatra (28-2173) with includes (Rack::Utils, Helpers, Templates)
- class Sinatra::Request < Rack::Request (31-165) with alias, attr_accessor, attr_reader
- class Sinatra::Response < Rack::Response (171-218)
- module Sinatra::Helpers (286-722) with alias url/uri, alias to/uri, alias errback/callback
- module Sinatra::Templates (742-939) with attr_accessor
- class Sinatra::Base (971-2076) with attr_accessor, attr_reader, 21+ methods
- class Sinatra::Application < Base (2085-2096)
- module Sinatra::Delegator (2101-2127)
- class Sinatra::Wrapper (2129-2150)
- Top-level factory methods: self.new, self.register, self.helpers, self.use
```

### Check 2: fast_refs for Base
```
153 total references:
- 112 definitions (class defs, constant refs, variable assignments)
- 21 imports (require 'sinatra/base' across sinatra-contrib and test files)
- 20 references (16 Extends in rack-protection, 2 Calls in base.rb, 1 References in settings_test)
```

### Check 4: Centrality scores
```
Base constant (line 379): centrality 0.55 (1 incoming ref)
Base class (line 971): centrality 0.00 (0 incoming refs) -- BUG
route method (line 1776): centrality 1.00 (22 incoming refs) -- correct, highest
Application constant (line 2035): centrality 0.46 (0 incoming refs)
```

### Check 5: Definition search ranking
```
Top results for "Base" definitions:
1. lib/sinatra/base.rb:379 (constant) -- Base.mime_type usage
2. lib/sinatra/base.rb:971 (class) -- class Sinatra::Base
3. rack-protection xss_header.rb:16 (constant)
4. rack-protection cookie_tossing.rb:19 (constant)
5-8. More rack-protection Base subclass constants
```

### Check 7: get_context for "HTTP request routing"
```
3 pivots:
- route (lib/sinatra/base.rb:1776) - centrality=high, risk=HIGH
- route (sinatra-contrib/lib/sinatra/multi_route.rb:69) - centrality=medium
- process_route (lib/sinatra/base.rb:1098) - centrality=medium

32 neighbors covering: settings, options, params, call, force_encoding, delete,
error_block!, compile!, flatten, merge, pattern, captures, filter!, conditions,
verb, routes, invoke_hook, route!, enable, head, copy, put, unlink, move, post,
link, proppatch, unlock, patch, propfind, mkcol, options
```

### Check 8: Test detection
```
exclude_tests=false: 5 results from test/contest.rb (test_name, test helper methods)
exclude_tests=true: 0 results -- all test/ files correctly excluded

RSpec describe blocks also found in spec/ dirs:
- sinatra-contrib/spec/namespace_spec.rb
- sinatra-contrib/spec/content_for_spec.rb
- sinatra-contrib/spec/respond_with_spec.rb
- test/helpers_test.rb (Minitest test framework, uses describe)
```
