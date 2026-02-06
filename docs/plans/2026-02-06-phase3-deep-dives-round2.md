# Phase 3 Deep-Dives Round 2: Priorities 5-9 Done Right

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete the substantive Phase 3 work that was skipped during the parallel cleanup sprint — data utilization unlocks, Bucket C decisions, output enrichment, and dogfood testing for Priorities 5-9.

**Architecture:** Each task targets a specific tool with focused data utilization improvements. Tasks are grouped by tool but independently executable. All code changes follow TDD (failing test → minimal implementation → verify).

**Tech Stack:** Rust, SQLite (symbols/identifiers/relationships tables), Tantivy, tree-sitter, serde, toon_format

---

## Context

Round 1 (parallel team) handled code cleanup: dead code, anti-patterns, file size limits. This round addresses the 4 checklist items that were skipped:

1. **Data utilization** — Which tables are ignored that could improve results?
2. **Output optimization** — Is the output agent-friendly and complete?
3. **Test coverage** — TDD for all new behavior
4. **Dogfood test** — Verify against Julie's own codebase

### Data Utilization Scorecard (current)

| Data | Usage | Target |
|------|-------|--------|
| Identifiers | 40% (fast_refs, trace_call_path) | 60% (+fast_explore, +fast_goto) |
| Types | 10% (fast_explore types mode only) | 10% (no change this round) |
| Visibility | 0% | 20% (+fast_explore logic, +fast_goto output) |
| Confidence | ~5% | ~5% (no change this round) |
| Metadata | 0% | 0% (extractors don't populate — not worth chasing) |

### Scope Decisions

**IN scope:** Identifiers integration, visibility usage, qualified names, find_logic kill, editing tool discoverability, MCP descriptions.

**OUT of scope (and why):**
- Confidence score filtering everywhere — adds complexity, marginal agent benefit
- Metadata JSON integration — most extractors don't populate it, so filtering would return nothing
- Types table in more tools — only fast_explore types mode needs it, and it already uses it
- Identifiers for fast_goto ranking — interesting but complex, low ROI vs qualified names

---

## Group A: fast_explore (Priority 5)

### Task 1: Kill find_logic MCP tool registration

**Why:** Phase 2 decided "KILL — redirect to fast_explore(mode='logic')". find_logic is confirmed 100% duplicate — fast_explore delegates to FindLogicTool. Remove the MCP registration so agents can't call it directly.

**Files:**
- Modify: `src/handler.rs` — remove find_logic tool registration
- Modify: `src/tests/tools/exploration/find_logic.rs` — check if tests call via MCP or directly; keep direct tests
- Keep: `src/tools/exploration/find_logic/mod.rs` — implementation stays (fast_explore depends on it)

**Step 1: Verify fast_explore delegates to FindLogicTool**

Run: `cargo test --lib fast_explore -- --test-threads=1 2>&1 | tail -5`
Expected: All fast_explore tests pass (confirms delegation works)

**Step 2: Remove find_logic from handler.rs tool list**

In `src/handler.rs`, find the `find_logic` tool registration block (around line 433-450) and remove it. Keep the `use` import if fast_explore needs it.

Also search for any `"find_logic"` string matches in handler.rs routing (the match arm in `call_tool` that dispatches to FindLogicTool).

**Step 3: Update find_logic tests**

Check `src/tests/tools/exploration/find_logic.rs` — if tests construct FindLogicTool directly and call methods, keep them (they test the implementation fast_explore depends on). If tests go through the MCP handler, update or remove them.

**Step 4: Run tests**

Run: `cargo test --lib 2>&1 | tail -5`
Expected: All tests pass (find_logic unit tests still work, just no MCP registration)

**Step 5: Commit**

```bash
git add src/handler.rs src/tests/tools/exploration/
git commit -m "refactor: remove find_logic MCP registration (use fast_explore mode=logic)"
```

---

### Task 2: Integrate identifiers into fast_explore logic mode

**Why:** Logic mode uses Tantivy + relationships for discovery but ignores the identifiers table (63K+ records). The same pattern that improved fast_refs and trace_call_path applies here — identifier usage sites reveal which symbols are actively called/referenced.

**Files:**
- Modify: `src/tools/exploration/find_logic/search.rs` — add identifiers query in Tier 4 (graph analysis)
- Test: `src/tests/tools/exploration/find_logic.rs` — new test

**Step 1: Write failing test**

Add a test in `src/tests/tools/exploration/find_logic.rs` that creates:
- A symbol `process_payment` with no relationships pointing to it
- An identifier with `kind=Call` and `containing_symbol_id` pointing to another symbol
- Verify that logic mode finds `process_payment` and boosts its confidence based on identifier usage

The test should fail because current logic mode only checks the relationships table for centrality analysis.

**Step 2: Run test to verify it fails**

Run: `cargo test --lib find_logic::test_identifier_usage_boosts_confidence -- --nocapture`
Expected: FAIL

**Step 3: Implement identifier-based usage counting in Tier 4**

In `src/tools/exploration/find_logic/search.rs`, in the `analyze_business_importance` function (or equivalent Tier 4 method):

1. After relationship-based centrality analysis, add identifier-based counting:
   - Generate naming variants: `generate_naming_variants(&symbol.name)`
   - Query: `db.get_identifiers_by_names_and_kind(&variants, "call")`
   - Count unique `containing_symbol_id` values per target symbol
   - Apply logarithmic boost similar to relationship centrality

2. Merge with existing relationship counts (don't double-count)

**Step 4: Run test to verify it passes**

Run: `cargo test --lib find_logic -- --nocapture`
Expected: All tests pass including new one

**Step 5: Commit**

```bash
git add src/tools/exploration/find_logic/search.rs src/tests/tools/exploration/find_logic.rs
git commit -m "feat(fast_explore): integrate identifiers into logic mode centrality analysis"
```

---

### Task 3: Add visibility-aware ranking to logic mode

**Why:** Logic mode currently treats public API methods and private helpers equally. Public business logic (controllers, services, handlers) is what agents care about. Private helpers should be deprioritized, not removed.

**Files:**
- Modify: `src/tools/exploration/find_logic/search.rs` — add visibility boost in Tier 3 (path intelligence) or as new step
- Test: `src/tests/tools/exploration/find_logic.rs` — new test

**Step 1: Write failing test**

Create two symbols with identical names and paths but different visibility:
- `process_order` with `visibility: Some("public")` — should rank higher
- `process_order_internal` with `visibility: Some("private")` — should rank lower

Test that the public symbol has higher confidence in results.

**Step 2: Run test to verify it fails**

Expected: FAIL (both have same confidence currently)

**Step 3: Implement visibility boost**

In the ranking pipeline (after Tier 3 path intelligence), add a visibility adjustment:
- `visibility == "public"` or `visibility == "pub"` → +0.1 confidence boost
- `visibility == "private"` or `visibility == "priv"` → -0.15 confidence penalty
- `visibility == "protected"` → -0.05 (slight penalty)
- `visibility == None` → no change (don't penalize missing data)

**Step 4: Run tests**

Run: `cargo test --lib find_logic -- --nocapture`
Expected: All pass

**Step 5: Commit**

```bash
git add src/tools/exploration/find_logic/search.rs src/tests/tools/exploration/find_logic.rs
git commit -m "feat(fast_explore): visibility-aware ranking in logic mode"
```

---

## Group B: fast_goto (Priority 6)

### Task 4: Implement qualified name resolution

**Why:** The tool description documents support for `MyClass::method` but the implementation does a literal string search for that exact name, which never matches. This is a documented-but-not-implemented feature — essentially a bug.

**Files:**
- Modify: `src/tools/navigation/resolution.rs` — add qualified name splitting and parent_id resolution
- Test: `src/tests/tools/navigation/` — new test file or add to existing

**Step 1: Write failing test**

Create test data:
- Symbol `MyClass` (kind: Class, id: "class_1")
- Symbol `my_method` (kind: Method, parent_id: "class_1")
- Symbol `my_method` (kind: Function, parent_id: None) — different symbol, same name

Search for `"MyClass::my_method"` — should find only the method with parent_id pointing to MyClass, not the standalone function.

**Step 2: Run test to verify it fails**

Expected: FAIL (current code searches for literal "MyClass::my_method" which matches nothing)

**Step 3: Implement qualified name resolution**

In `src/tools/navigation/resolution.rs` (or fast_goto.rs), before the existing symbol lookup:

1. Check if symbol name contains `::` (or `.` for non-Rust languages)
2. If yes, split into `(parent_name, child_name)`
3. Query: `db.get_symbols_by_name(child_name)`
4. Filter results: keep only symbols where `parent_id` resolves to a symbol with `name == parent_name`
5. If no results after filtering, fall through to normal resolution (maybe the `::` is part of the actual name in some language)

**Step 4: Run tests**

Run: `cargo test --lib fast_goto -- --nocapture`
Expected: All pass

**Step 5: Commit**

```bash
git add src/tools/navigation/resolution.rs src/tests/tools/navigation/
git commit -m "feat(fast_goto): implement qualified name resolution (MyClass::method)"
```

---

### Task 5: Enrich fast_goto output with parent and visibility

**Why:** Agents get file:line but no context about WHERE in the hierarchy the symbol lives. Adding parent name and visibility costs almost nothing in tokens but gives agents much better context for disambiguation.

**Files:**
- Modify: `src/tools/navigation/fast_goto.rs` — add parent_name and visibility to DefinitionResult and lean output
- Modify: `src/tools/navigation/mod.rs` — update types if DefinitionResult is defined here
- Test: `src/tests/tools/navigation/` — verify enriched output

**Step 1: Write failing test**

Create a symbol with `parent_id` pointing to a class and `visibility: Some("public")`. Verify fast_goto output includes:
- `parent: ClassName` (resolved from parent_id)
- `visibility: public`

In lean format, this would appear as:
```
src/service.rs:42 (method, public)
  in ClassName
  fn process_order(order: Order) -> Result
```

**Step 2: Run test to verify it fails**

Expected: FAIL (current output doesn't include parent or visibility)

**Step 3: Implement output enrichment**

1. After resolving symbols, batch-fetch parent symbols for any with `parent_id`
2. Add `parent_name: Option<String>` and `visibility: Option<String>` to the output struct
3. Update lean format rendering to include parent and visibility
4. Update TOON/JSON format to include these fields

**Step 4: Run tests**

Run: `cargo test --lib fast_goto -- --nocapture`
Expected: All pass

**Step 5: Commit**

```bash
git add src/tools/navigation/fast_goto.rs src/tools/navigation/mod.rs src/tests/tools/navigation/
git commit -m "feat(fast_goto): enrich output with parent name and visibility"
```

---

## Group C: Editing Tools Discoverability (Priority 7)

### Task 6: Rewrite editing tool MCP descriptions

**Why:** The audit concluded all three tools are well-built with genuine unique capabilities, but agents don't discover them because the MCP descriptions are vague. This is the highest-ROI fix for editing tool adoption.

**Files:**
- Modify: `src/handler.rs` — update tool descriptions for edit_lines, fuzzy_replace, edit_symbol

**Step 1: Read current descriptions**

Read `src/handler.rs` and find the description strings for all three editing tools.

**Step 2: Write improved descriptions**

**edit_lines** — current: "Surgical line editing: insert, replace, or delete specific lines in a file."
```
New: "Edit file content by line number: insert, replace, or delete specific line ranges.
Use when you know the exact line numbers (e.g., from get_symbols output).
Supports dry-run preview. Paths are relative to workspace root."
```

**fuzzy_replace** — current: "Fuzzy search and replace using diff-match-patch algorithm. Tolerant of whitespace changes."
```
New: "Find and replace with fuzzy matching (tolerates whitespace and minor variations).
Two modes: single-file (file_path) or multi-file (file_pattern with glob like '**/*.rs').
Multi-file mode is ideal for codebase-wide refactoring. Supports dry-run preview and structural validation."
```

**edit_symbol** — current: "Edit a symbol's body (function, class, etc.) with fuzzy matching."
```
New: "AST-aware symbol editing: replace function/method bodies, insert code before/after symbols, or extract symbols to other files.
Three operations: 'replace_body' (update implementation), 'insert_relative' (add code adjacent to symbol), 'extract_to_file' (move symbol).
Finds symbols by name using tree-sitter — works even if code has minor variations. Supports dry-run preview."
```

**Step 3: Update handler.rs**

Replace the description strings in the tool registration blocks.

**Step 4: Verify compile**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors

**Step 5: Commit**

```bash
git add src/handler.rs
git commit -m "docs(tools): rewrite editing tool MCP descriptions for agent discoverability"
```

---

### Task 7: Expand editing tools section in agent instructions

**Why:** JULIE_AGENT_INSTRUCTIONS.md has 4 lines for all three editing tools. Agents need concrete examples showing WHEN to use each tool vs Claude Code's native Edit/Write.

**Files:**
- Modify: `JULIE_AGENT_INSTRUCTIONS.md` — expand refactoring tools section with examples and decision guidance

**Step 1: Read current instructions**

Read `JULIE_AGENT_INSTRUCTIONS.md` and find the refactoring/editing tools section.

**Step 2: Write expanded section**

Replace the brief listing with:

```markdown
### Editing Tools — When to Use Which

Julie provides three editing tools. Each has a unique capability that Claude Code's native Edit/Write tools lack.

**edit_lines** - Line-number-based editing with dry-run preview
- Best when: You know exact line numbers (from get_symbols or fast_search output)
- Operations: insert (add lines at position), replace (swap line range), delete (remove lines)
- Example: Insert import at line 3, delete dead code at lines 45-52

**fuzzy_replace** - Fuzzy matching + multi-file refactoring
- Best when: Pattern has whitespace variations, OR you need to change multiple files at once
- Unique: `file_pattern="**/*.rs"` applies the same replacement across all matching files
- Example: Rename `getUserData` to `fetchUserData` across all .ts files in one call
- Tip: Start with `dry_run=true` to preview, then `dry_run=false` to apply

**edit_symbol** - AST-aware semantic editing
- Best when: You want to edit a function/class by NAME, not by line number or string match
- Operations: `replace_body` (rewrite implementation), `insert_relative` (add before/after), `extract_to_file` (move to another file)
- Example: Replace the body of `calculate_total()` without touching its signature
- Tip: Uses tree-sitter for symbol detection — finds symbols reliably even in complex code

**ALWAYS use `dry_run=true` first** for all three tools. Review the preview, then apply with `dry_run=false`.
```

**Step 3: Apply the edit**

**Step 4: Commit**

```bash
git add JULIE_AGENT_INSTRUCTIONS.md
git commit -m "docs: expand editing tools guidance in agent instructions"
```

---

## Group D: Quick Fixes (Priorities 8-9)

### Task 8: Fix MCP description inaccuracies

**Why:** recall tool says "semantic search" but uses Tantivy BM25 (not embeddings). checkpoint doesn't mention git context or type options. These are quick, high-clarity fixes.

**Files:**
- Modify: `src/handler.rs` — fix recall and checkpoint descriptions

**Step 1: Read current descriptions**

Find recall and checkpoint tool registration blocks in handler.rs.

**Step 2: Fix descriptions**

**recall** — change "Retrieve development memories using semantic search." to:
```
"Retrieve development memories using text search with code-aware tokenization."
```

**checkpoint** — change "Save development memory checkpoint to .memories/ directory." to:
```
"Save development memory checkpoint to .memories/ directory. Captures git context (branch, commit, changed files) automatically. Supports types: checkpoint, decision, learning, observation."
```

**Step 3: Update and verify**

Run: `cargo check 2>&1 | tail -5`
Expected: No errors

**Step 4: Commit**

```bash
git add src/handler.rs
git commit -m "docs(tools): fix recall and checkpoint MCP descriptions"
```

---

## Group E: Dogfood Testing

### Task 9: Dogfood test all improved tools against Julie's codebase

**Why:** The Phase 3 checklist requires dogfood testing — verify improvements against real-world data.

**Tests to run (these are manual verification, not automated tests):**

**fast_explore logic mode:**
```
fast_explore(mode="logic", domain="search indexing")
```
Verify: Results include symbols found via identifier usage (not just relationships). Public symbols rank higher than private helpers.

**fast_goto with qualified names:**
```
fast_goto(symbol="TraceCallPathTool::call_tool")
```
Verify: Returns the `call_tool` method inside TraceCallPathTool, not standalone functions named `call_tool`.

**fast_goto output enrichment:**
```
fast_goto(symbol="trace_upstream")
```
Verify: Output includes parent module/struct name and visibility information.

**Editing tool descriptions:**
- Start a fresh MCP session
- Check that tool descriptions are clear and explain unique capabilities
- Verify agents can distinguish when to use each tool

**find_logic removal:**
```
find_logic(domain="search")
```
Verify: Returns "unknown tool" error (no longer registered).

---

## Task Order & Dependencies

```
Group A (fast_explore): Tasks 1 → 2 → 3 (sequential — Task 1 modifies handler.rs, Tasks 2-3 modify find_logic)
Group B (fast_goto): Tasks 4 → 5 (sequential — Task 4 adds resolution, Task 5 adds output fields)
Group C (editing): Tasks 6 → 7 (sequential — descriptions first, then instructions)
Group D (quick fixes): Task 8 (independent)
Group E (dogfood): Task 9 (depends on all above)
```

Groups A, B, C, D are independent of each other and can be parallelized.

**Estimated effort:** 2 focused sessions, or 1 session with 2-3 parallel agents.

---

## Verification Criteria

Before marking Phase 3 complete:

- [ ] `cargo test --lib` — all tests pass (762+ with new tests)
- [ ] find_logic is NOT in MCP tool list
- [ ] fast_goto resolves `MyClass::method` qualified names
- [ ] fast_goto output includes parent name and visibility
- [ ] fast_explore logic mode uses identifiers for centrality
- [ ] fast_explore logic mode boosts public symbols
- [ ] Editing tool MCP descriptions explain unique capabilities and operations
- [ ] Agent instructions have concrete examples for each editing tool
- [ ] recall description says "text search" not "semantic search"
- [ ] Dogfood tests pass against Julie's own codebase
