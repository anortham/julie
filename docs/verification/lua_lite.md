# Lua Verification: lite (lite_f7e95a20)

**Workspace:** lite text editor — 404 files, 27858 symbols, 9167 relationships
**Date:** 2026-03-17
**Status:** PASS with observations

---

## Check 1: Symbol Extraction

**File tested:** `data/core/init.lua` (92 symbols), `data/core/doc/init.lua` (80 symbols), `data/core/view.lua` (50 symbols), `data/core/command.lua` (20 symbols)

| Symbol Kind | Extracted? | Examples |
|-------------|-----------|----------|
| Local functions | YES | `local function diff_files(a, b)`, `local function split_lines(text)` |
| Module methods | YES | `function core.init()`, `function Doc:save(filename)`, `function View:update()` |
| Method (colon syntax) | YES | `function Doc:new(filename)`, `function Doc:insert(line, col, text)` |
| Variables | YES | `local core = {}`, `Doc = Object:extend()` |
| Fields (self.x) | YES | `self.lines`, `self.filename`, `self.scroll`, `core.redraw` |
| Imports (require) | YES | `local common = require "core.common"`, `local Doc = require "core.doc"` |
| Anonymous functions | YES | `function() return core.active_view:is(class) end` |
| Tables as symbols | PARTIAL | Tables initialized as `{}` are captured as variables (e.g., `core = {}`). Table constructors with fields are not individually decomposed — fields assigned via `self.x = ...` or `core.x = ...` are captured as field symbols. |

**Verdict:** PASS — Lua extractor handles the full range of Lua idioms well. Functions (global, local, method with `:`, method with `.`), imports via `require`, fields, and variables all extracted correctly. 92 symbols from `init.lua` is comprehensive for a ~465-line file.

---

## Check 2: Relationships

**Symbol tested:** `core`

| Relationship Kind | Found? | Count | Examples |
|-------------------|--------|-------|----------|
| Definition | YES | 1 | `data/core/init.lua:12` — `local core = {}` |
| Imports (require) | YES | 13 | `core = require "core"` across 13 files (command.lua, logview.lua, statusview.lua, docview.lua, rootview.lua, commandview.lua, view.lua, etc.) |
| Cross-file refs | YES | — | All 13 imports are cross-file `require()` references |

**Verdict:** PASS — Cross-file `require()` relationships are detected correctly. The Lua module system (table-based, using `require`) is well understood by the extractor. All 13 files that `require "core"` are captured as import relationships.

---

## Check 3: Identifiers (Doc)

**Symbol tested:** `Doc`

| Category | Count | Examples |
|----------|-------|----------|
| Definitions | 9 | `Doc = Object:extend()` (doc/init.lua:8), `local function doc()` (commands/doc.lua:14), field `self.doc = assert(doc)` (docview.lua:57) |
| Imports | 2 | `Doc = require "core.doc"` in init.lua:83, commandview.lua:4 |
| References (calls) | 15 | Calls across commands/doc.lua (lines 68-104) — `doc():save()`, `doc():insert()`, etc. |
| **Total** | **26** | |

**Verdict:** PASS — Good identifier coverage. Both the `Doc` class definition and the helper `local function doc()` are tracked. Call-site references are correctly classified.

---

## Check 4: Centrality

| Symbol | File | Centrality | Incoming Refs | Risk |
|--------|------|-----------|---------------|------|
| `core` | data/core/init.lua:12 | **0.00** | 0 | MEDIUM (0.58) |
| `Doc` | data/core/doc/init.lua:8 | **0.00** | 1 | MEDIUM (0.58) |
| `View` | data/core/view.lua:8 | **0.00** | 0 | MEDIUM (0.58) |
| `command` | data/core/command.lua:2 | **0.00** | 2 | MEDIUM (0.58) |
| `CommandView` | data/core/commandview.lua:16 | **0.00** | 1 | MEDIUM (0.58) |

**Observation: ALL centrality scores are 0.00.** This is unexpected for core symbols in a 27858-symbol, 9167-relationship codebase.

`fast_refs` found 13 import references to `core` and 26 references to `Doc`, yet `deep_dive` reports `core` has "0 incoming refs" and `Doc` has "1 incoming refs." There is a clear mismatch between what `fast_refs` finds and what centrality sees.

**Possible bug:** The centrality computation may not be counting `import`-kind references toward centrality scores. Alternatively, the Lua extractor may be storing relationships in a way that the centrality pipeline doesn't pick up (e.g., the `require()` calls may not create proper identifier-to-definition links in the graph).

**Verdict:** FAIL — Centrality is non-functional for Lua. Core symbols like `core` (imported by 13 files) and `Doc` (26 total references) should have significant centrality scores, but all show 0.00.

---

## Check 5: Definition Search

**Query:** `Doc` (search_target=definitions)

| # | File | Line | Kind | Snippet |
|---|------|------|------|---------|
| 1 | data/core/commands/doc.lua | 14 | function | `local function doc()` |
| 2 | data/core/commands/findreplace.lua | 10 | function | `local function doc()` |
| 3 | data/core/init.lua | 262 | variable | `local doc = Doc(filename)` |
| 4 | data/core/init.lua | 394 | variable | `local doc = core.docs[i]` |
| 5 | data/core/doc/highlighter.lua | 11 | field | `self.doc = doc` |
| 6 | data/core/doc/init.lua | 8 | variable | `Doc = Object:extend()` |

**Observation:** The actual class definition `Doc = Object:extend()` at `data/core/doc/init.lua:8` appears as result #6 (last). The top results are local helper functions and variable assignments. The class definition — the most important result for a "Doc" search — is ranked lowest.

**Verdict:** PARTIAL — The definition is found but ranking is suboptimal. The canonical class definition should rank higher than local helper functions and variable assignments. This is likely a consequence of the zero centrality scores (check 4) — if centrality were working, the class-level `Doc` definition would be boosted above the local helpers.

---

## Check 6: deep_dive Resolution

**Query:** `deep_dive(symbol="Doc", depth="context")`

Resolved to `data/core/doc/init.lua:8` — `Doc = Object:extend()` with body showing lines 5-11.

| Field | Value |
|-------|-------|
| Referenced by | 1 (data/core/init.lua:262, Calls) |
| Centrality | 0.00 (1 incoming ref) |
| Visibility | public |
| Test coverage | untested |
| Kind | variable |

**Observation:** `deep_dive` correctly resolves to the class definition. However, it reports only 1 reference (the `Doc(filename)` call in init.lua:262), while `fast_refs` found 26 references. The `deep_dive` "Referenced by" section at `context` depth appears to severely undercount.

**Verdict:** PARTIAL — Resolution is correct but reference count in deep_dive output is incomplete compared to fast_refs.

---

## Check 7: get_context

**Query:** `text editor document`

Returned 2 pivots, 10 neighbors across 7 files.

| Pivots | File | Centrality | Notes |
|--------|------|-----------|-------|
| `doc()` | data/core/commands/doc.lua:14 | high | Helper function returning `core.active_view.doc` |
| `insert()` | data/core/commandview.lua:11 | high | `SingleLineDoc:insert` override |

| Neighbors (10) | File | Kind |
|-----------------|------|------|
| `core.log` | data/core/init.lua:296 | method |
| `get_files` | data/core/init.lua:30 | function |
| `push_previous_find` | data/core/commands/findreplace.lua:20 | function |
| `save` | data/core/commands/doc.lua:60 | function |
| `fuzzy_match_items` | data/core/common.lua:61 | function |
| `insert_at_start_of_selected_lines` | data/core/commands/doc.lua:27 | function |
| `append_line_if_last_line` | data/core/commands/doc.lua:53 | function |
| `remove_from_start_of_selected_lines` | data/core/commands/doc.lua:39 | function |
| `mouse_selection` | data/core/docview.lua:195 | function |
| `push_token` | data/core/tokenizer.lua:4 | function |

**Observation:** The context response is reasonable for the query. The `doc()` helper and document editing functions are surfaced. However, the `Doc` class itself (the actual document model at `data/core/doc/init.lua`) is absent from both pivots and neighbors, which is surprising for a "text editor document" query. `get_files` and `push_token` seem tangential.

**Verdict:** PARTIAL — Useful results but misses the core `Doc` class definition. The document model (Doc:new, Doc:insert, Doc:load, Doc:save) would be the most relevant symbols for this query.

---

## Check 8: Test Detection

**With `exclude_tests=false`:** 5 results — all from C headers in `winlib/SDL2-2.0.10/test/` and `src/lib/lua52/`. No Lua test files found.

**With `exclude_tests=true`:** 5 results — similar C headers. The `winlib/SDL2-2.0.10/test/testautomation_video.c` file was NOT excluded even with `exclude_tests=true`.

**Observation:** The lite project has no formal test suite, which is expected for a small Lua text editor project. However, the test exclusion filter did not remove `winlib/SDL2-2.0.10/test/testautomation_video.c` — a file clearly in a `test/` directory — when `exclude_tests=true`. This suggests the test path detection heuristic may not be catching the `test/` directory pattern within vendored/library paths.

**Verdict:** N/A for test content (no Lua tests exist), but **observation** that `exclude_tests=true` does not filter `winlib/.../test/` paths.

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Symbol Extraction | **PASS** | Functions, methods, fields, imports, variables all extracted correctly |
| 2. Relationships | **PASS** | Cross-file `require()` imports detected (13 for `core`) |
| 3. Identifiers | **PASS** | 26 references for `Doc` across definitions, imports, and calls |
| 4. Centrality | **FAIL** | All core symbols show 0.00 centrality despite many references |
| 5. Definition Search | **PARTIAL** | Definitions found but class definition ranked last (centrality gap) |
| 6. deep_dive Resolution | **PARTIAL** | Correct resolution but reference count underreported vs fast_refs |
| 7. get_context | **PARTIAL** | Good coverage but misses the Doc class itself for "document" query |
| 8. Test Detection | **N/A** | No Lua tests in project; `exclude_tests` doesn't filter vendored test dirs |

### Bugs / Issues Found

1. **Centrality is zero for all Lua symbols** — Despite `fast_refs` finding 13+ imports and 26+ references, centrality scores are uniformly 0.00. This likely cascades into poor search ranking (check 5) and incomplete `get_context` results (check 7). The relationship data exists but may not be feeding into the centrality graph computation correctly for Lua.

2. **deep_dive reference undercount** — `deep_dive(Doc, context)` reports 1 reference; `fast_refs(Doc)` finds 26. The `deep_dive` "Referenced by" section at `context` depth may be applying an aggressive limit or using a different query path.

3. **Definition search ranking** — `Doc = Object:extend()` (the actual class) ranks last behind local variable assignments and helper functions. Direct consequence of zero centrality.

4. **Test exclusion gap** — Files in `winlib/SDL2-2.0.10/test/` are not excluded by `exclude_tests=true`.
