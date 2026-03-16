# GPT Review — Three Fixes Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three gaps identified in GPT's external review: qualified symbol lookup (`SearchIndex::search_symbols`), fixture/benchmark noise in NL search, and line-level rename_symbol dry-run previews.

**Architecture:** All fixes are localized. Task 1 wires an existing parser into two lookup paths. Task 2 tunes scoring constants and broadens a path heuristic. Task 3 enriches the rename dry-run formatter with line-level diffs.

**Tech Stack:** Rust, TDD (red-green-refactor)

---

## Chunk 1: Qualified Symbol Lookup (Tasks 1–2)

### Task 1: Wire `parse_qualified_name` into `deep_dive`'s `find_symbol`

**Problem:** `deep_dive(symbol="SearchIndex::search_symbols")` returns "No symbol found" because `find_symbol()` passes the raw string `"SearchIndex::search_symbols"` to `db.find_symbols_by_name()`, which does an exact `WHERE name = ?` match. The qualified name parser `parse_qualified_name()` exists at `src/tools/navigation/resolution.rs:15` but is never called from the lookup path — it's dead code in production (only referenced in a test).

**Fix:** When `find_symbol` receives a qualified name, split it with `parse_qualified_name`, search by the child name, and filter results by `parent_name` match against the parent.

**Files:**
- Modify: `src/tools/deep_dive/data.rs:51-74` (`find_symbol`)
- Test: `src/tests/tools/deep_dive_tests.rs` (add test near line 1555)

- [ ] **Step 1: Write failing test — qualified name resolves to correct symbol**

In `src/tests/tools/deep_dive_tests.rs`, add after the existing `test_find_symbol_not_found` test (~line 1555):

```rust
#[test]
fn test_find_symbol_qualified_name() {
    let (_tmp, mut db) = setup_db();

    // Parent struct and child method with same "process" name in different parents
    let symbols = vec![
        make_symbol(
            "sym-parent-a",
            "Engine",
            SymbolKind::Struct,
            "src/engine.rs",
            1,
            None,
            Some("pub struct Engine"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-method-a",
            "process",
            SymbolKind::Method,
            "src/engine.rs",
            10,
            Some("Engine"),      // parent_name
            Some("pub fn process(&self)"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-parent-b",
            "Pipeline",
            SymbolKind::Struct,
            "src/pipeline.rs",
            1,
            None,
            Some("pub struct Pipeline"),
            Some(Visibility::Public),
            None,
        ),
        make_symbol(
            "sym-method-b",
            "process",
            SymbolKind::Method,
            "src/pipeline.rs",
            10,
            Some("Pipeline"),    // parent_name
            Some("pub fn process(&self)"),
            Some(Visibility::Public),
            None,
        ),
    ];
    db.store_symbols(&symbols).unwrap();

    // Qualified lookup should resolve to exactly one symbol
    let found = find_symbol(&db, "Engine::process", None).unwrap();
    assert_eq!(found.len(), 1, "qualified name should resolve to exactly one symbol");
    assert_eq!(found[0].file_path, "src/engine.rs");
    assert_eq!(found[0].parent_name, Some("Engine".to_string()));

    // Dot-separated also works (for Python, JS, etc.)
    let found_dot = find_symbol(&db, "Pipeline.process", None).unwrap();
    assert_eq!(found_dot.len(), 1);
    assert_eq!(found_dot[0].file_path, "src/pipeline.rs");

    // Unqualified still returns both
    let found_all = find_symbol(&db, "process", None).unwrap();
    assert_eq!(found_all.len(), 2, "unqualified should still return both");
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib test_find_symbol_qualified_name 2>&1 | tail -10
```

Expected: FAIL — `find_symbol` searches for literal `"Engine::process"` which doesn't exist as a name.

- [ ] **Step 3: Implement qualified name resolution in `find_symbol`**

In `src/tools/deep_dive/data.rs`, modify `find_symbol` (line 51-74):

```rust
use crate::tools::navigation::resolution::parse_qualified_name;

/// Look up a symbol by name, optionally disambiguated by file path.
pub fn find_symbol(
    db: &SymbolDatabase,
    name: &str,
    context_file: Option<&str>,
) -> Result<Vec<Symbol>> {
    // Try qualified name resolution first (e.g. "SearchIndex::search_symbols")
    if let Some((parent, child)) = parse_qualified_name(name) {
        let mut symbols = db.find_symbols_by_name(child)?;
        symbols.retain(|s| s.kind != SymbolKind::Import);

        // Filter to symbols whose parent matches
        let qualified: Vec<Symbol> = symbols
            .iter()
            .filter(|s| s.parent_name.as_deref() == Some(parent))
            .cloned()
            .collect();

        if !qualified.is_empty() {
            // Apply context_file disambiguation if needed
            if let Some(file) = context_file {
                let file_matches: Vec<Symbol> = qualified
                    .iter()
                    .filter(|s| s.file_path.contains(file))
                    .cloned()
                    .collect();
                if !file_matches.is_empty() {
                    return Ok(file_matches);
                }
            }
            return Ok(qualified);
        }
        // Fall through: if no parent match, try the full string as-is
        // (in case someone literally named a symbol with "::" in it)
    }

    let mut symbols = db.find_symbols_by_name(name)?;

    // Filter out imports — we want actual definitions
    symbols.retain(|s| s.kind != SymbolKind::Import);

    // Disambiguate by file if specified
    if let Some(file) = context_file {
        let file_matches: Vec<Symbol> = symbols
            .iter()
            .filter(|s| s.file_path.contains(file))
            .cloned()
            .collect();
        if !file_matches.is_empty() {
            return Ok(file_matches);
        }
    }

    Ok(symbols)
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib test_find_symbol_qualified_name 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/tools/deep_dive/data.rs src/tests/tools/deep_dive_tests.rs
git commit -m "feat(deep_dive): wire parse_qualified_name into find_symbol

Qualified names like SearchIndex::search_symbols and MyClass.method
now resolve correctly in deep_dive by splitting on :: or . and
filtering by parent_name."
```

---

### Task 2: Wire qualified name support into `fast_refs`

**Problem:** `fast_refs(symbol="SearchIndex::search_symbols")` returns "No references found" even though its schema description says "supports qualified names". The lookup in `find_references_and_definitions` (line ~265) calls `db_lock.get_symbols_by_name(&symbol)` with the raw string — same bug as deep_dive.

**Files:**
- Modify: `src/tools/navigation/fast_refs.rs:225-290` (qualified name handling in `find_references_and_definitions`)
- Test: `src/tests/tools/` (existing fast_refs test file, or new one)

- [ ] **Step 1: Write failing test — qualified name finds references**

Find the existing fast_refs test file and add a test. If no dedicated file exists, add to an appropriate test module:

```rust
#[test]
fn test_fast_refs_qualified_name() {
    // Setup: create parent struct + child method + a caller
    let (_tmp, mut db) = setup_db();

    let symbols = vec![
        make_symbol("parent", "SearchIndex", SymbolKind::Struct, "src/search/index.rs", 1, None, None, Some(Visibility::Public), None),
        make_symbol("method", "search_symbols", SymbolKind::Method, "src/search/index.rs", 50, Some("SearchIndex"), None, Some(Visibility::Public), None),
        make_symbol("caller", "do_search", SymbolKind::Function, "src/caller.rs", 10, None, None, Some(Visibility::Public), None),
    ];
    db.store_symbols(&symbols).unwrap();

    // Add a relationship: do_search calls search_symbols
    let rels = vec![make_relationship("rel-1", "caller", "method", RelationshipKind::Calls, "src/caller.rs", 15)];
    db.store_relationships(&rels).unwrap();

    // Qualified lookup should find the method and its references
    let definitions = db.get_symbols_by_name("SearchIndex::search_symbols");
    // This currently returns empty — that's the bug
    // After fix, we need the fast_refs tool to resolve this
}
```

**Note:** The exact test structure depends on whether `fast_refs` tests use the full tool `call_tool` path (which needs a handler) or test the `find_references_and_definitions` method directly. Check existing test patterns in `src/tests/tools/` and follow the same approach.

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib test_fast_refs_qualified_name 2>&1 | tail -10
```

- [ ] **Step 3: Implement qualified name handling in `find_references_and_definitions`**

In `src/tools/navigation/fast_refs.rs`, at the start of `find_references_and_definitions` (~line 260, after the reference workspace early return), add qualified name resolution before the primary workspace lookup:

```rust
use crate::tools::navigation::resolution::parse_qualified_name;

// ... inside find_references_and_definitions, before the SQLite lookup block ...

// Resolve qualified names: "SearchIndex::search_symbols" → search for "search_symbols" filtered by parent
let (effective_symbol, parent_filter) = match parse_qualified_name(&self.symbol) {
    Some((parent, child)) => (child.to_string(), Some(parent.to_string())),
    None => (self.symbol.clone(), None),
};

// Then use `effective_symbol` instead of `self.symbol` in the db lookup:
// db_lock.get_symbols_by_name(&effective_symbol)
//
// After getting definitions, filter by parent_name if parent_filter is Some:
// if let Some(ref parent) = parent_filter {
//     definitions.retain(|s| s.parent_name.as_deref() == Some(parent.as_str()));
// }
```

Apply the same `effective_symbol` to the variant generation and identifier lookup blocks too.

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib test_fast_refs_qualified_name 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```bash
git add src/tools/navigation/fast_refs.rs src/tests/tools/
git commit -m "feat(fast_refs): support qualified symbol names

SearchIndex::search_symbols now resolves correctly in fast_refs
by splitting on :: and filtering definitions by parent_name."
```

---

## Chunk 2: Fixture/Benchmark Noise in NL Search (Task 3)

### Task 3: Harsher fixture penalty + add `benchmarks` to path detection

**Problem:** NL queries return benchmark fixture files because: (a) `NL_PATH_PENALTY_FIXTURES` is only 0.95 (5% reduction — too gentle), and (b) `is_fixture_path` doesn't match `benchmarks` as a path segment. The dogfood query file `fixtures/benchmarks/labhandbookv2_dogfood_queries.jsonl` contains exact NL query text and ranks highly.

**Files:**
- Modify: `src/search/scoring.rs:25` (penalty constant)
- Modify: `src/search/scoring.rs:227-238` (`is_fixture_path` — add `benchmarks`)
- Test: `src/tests/tools/search/` (scoring tests)

- [ ] **Step 1: Write failing tests**

Find the existing scoring test file (search for `test.*is_fixture_path` or `test.*nl_path`). Add:

```rust
#[test]
fn test_is_fixture_path_matches_benchmarks() {
    assert!(is_fixture_path("fixtures/benchmarks/queries.jsonl"));
    assert!(is_fixture_path("benchmarks/perf_data.json"));
    assert!(is_fixture_path("src/benchmarks/load_test.rs"));
}

#[test]
fn test_fixture_penalty_is_meaningful() {
    // Fixture penalty should be at least 20% reduction (≤ 0.80)
    // to meaningfully suppress noise over source code boost (1.08)
    assert!(NL_PATH_PENALTY_FIXTURES <= 0.80,
        "fixture penalty {} is too gentle — should be ≤ 0.80 to suppress noise vs source boost {}",
        NL_PATH_PENALTY_FIXTURES, NL_PATH_BOOST_SRC);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib test_is_fixture_path_matches_benchmarks 2>&1 | tail -10
cargo test --lib test_fixture_penalty_is_meaningful 2>&1 | tail -10
```

Expected: FAIL — `benchmarks` not matched, penalty is 0.95.

- [ ] **Step 3: Implement fixes**

In `src/search/scoring.rs`:

**3a.** Change line 25 — harsher penalty:
```rust
pub(crate) const NL_PATH_PENALTY_FIXTURES: f32 = 0.75;
```

**3b.** Add `"benchmarks"` to `is_fixture_path` (line ~230):
```rust
pub(crate) fn is_fixture_path(path: &str) -> bool {
    for segment in path.split('/') {
        match segment {
            "fixtures" | "fixture" | "Fixtures" | "Fixture" | "testdata" | "test_data"
            | "test-data" | "__fixtures__" | "snapshots" | "Snapshots" | "__snapshots__"
            | "benchmarks" | "Benchmarks" | "benchmark" | "Benchmark" => {
                return true;
            }
            _ => {}
        }
    }
    false
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --lib test_is_fixture_path_matches_benchmarks 2>&1 | tail -10
cargo test --lib test_fixture_penalty_is_meaningful 2>&1 | tail -10
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/scoring.rs src/tests/
git commit -m "fix(scoring): harsher fixture penalty (0.95→0.75) + match benchmarks paths

NL queries were surfacing benchmark fixture files because the 5%
penalty was too gentle and 'benchmarks' wasn't recognized as a
fixture path segment."
```

---

## Chunk 3: Line-Level Rename Preview (Task 4)

### Task 4: Add line-level diff preview to `rename_symbol` dry-run

**Problem:** `rename_symbol` dry-run output shows only `"src/search/index.rs (1 changes)"` — no line numbers, no before/after context. For a risky rename, users need to see what's changing to evaluate safety.

**Current flow:** `handle_rename_symbol` → `rename_in_file` → `smart_text_replace` → compares old/new content → returns change count. In dry-run mode, the file is NOT written but changes are computed. We have both `content` and `updated_content` available — we just don't diff them.

**Fix:** Create a `RenameChange` struct that captures line-level diffs. Modify `rename_in_file` to return `Vec<RenameChange>` instead of `usize`. Format these into a readable preview in the dry-run output.

**Files:**
- Modify: `src/tools/refactoring/mod.rs:162-203` (`rename_in_file` return type)
- Modify: `src/tools/refactoring/rename.rs:189-215` (dry-run formatting)
- Test: `src/tests/tools/` (refactoring tests)

- [ ] **Step 1: Write failing test — dry-run output contains line-level detail**

Find or create the rename tests file. Add:

```rust
#[test]
fn test_rename_dry_run_shows_line_preview() {
    // Create a temp file with known content
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("example.rs");
    std::fs::write(&file_path, "fn foo() {\n    let x = foo();\n}\n").unwrap();

    let tool = SmartRefactorTool {
        operation: "rename_symbol".to_string(),
        params: String::new(),
        dry_run: true,
    };

    let content = std::fs::read_to_string(&file_path).unwrap();
    let updated = tool.smart_text_replace(&content, "foo", "bar", "example.rs", false).unwrap();

    // Compute line-level changes
    let changes = compute_line_changes(&content, &updated);
    assert!(!changes.is_empty(), "should detect line-level changes");

    // Each change should have line number and before/after
    for change in &changes {
        assert!(change.line_number > 0);
        assert!(change.old_line.contains("foo"));
        assert!(change.new_line.contains("bar"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test --lib test_rename_dry_run_shows_line_preview 2>&1 | tail -10
```

Expected: FAIL — `compute_line_changes` doesn't exist yet.

- [ ] **Step 3: Implement `RenameChange` struct and `compute_line_changes`**

In `src/tools/refactoring/mod.rs`, add a struct and diff helper:

```rust
/// A single line-level change from a rename operation.
#[derive(Debug, Clone)]
pub(crate) struct RenameChange {
    pub line_number: usize,
    pub old_line: String,
    pub new_line: String,
}

/// Compare old and new content line-by-line, returning changed lines.
pub(crate) fn compute_line_changes(old_content: &str, new_content: &str) -> Vec<RenameChange> {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    let mut changes = Vec::new();

    for (i, (old, new)) in old_lines.iter().zip(new_lines.iter()).enumerate() {
        if old != new {
            changes.push(RenameChange {
                line_number: i + 1, // 1-indexed
                old_line: old.to_string(),
                new_line: new.to_string(),
            });
        }
    }
    changes
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cargo test --lib test_rename_dry_run_shows_line_preview 2>&1 | tail -10
```

- [ ] **Step 5: Modify `rename_in_file` to return `Vec<RenameChange>`**

Change the return type from `Result<usize>` to `Result<Vec<RenameChange>>`:

```rust
async fn rename_in_file(
    &self, workspace_root: &Path, file_path: &str,
    old_name: &str, new_name: &str,
) -> Result<Vec<RenameChange>> {
    let absolute_path = if Path::new(file_path).is_absolute() {
        file_path.to_string()
    } else {
        workspace_root.join(file_path).to_string_lossy().to_string()
    };

    let content = fs::read_to_string(&absolute_path)?;
    let updated_content = self.smart_text_replace(&content, old_name, new_name, file_path, false)?;

    if updated_content == content {
        return Ok(Vec::new());
    }

    if !self.dry_run {
        let tx = EditingTransaction::begin(&absolute_path)?;
        tx.commit(&updated_content)?;
    }

    Ok(compute_line_changes(&content, &updated_content))
}
```

- [ ] **Step 6: Update `handle_rename_symbol` to use line-level changes in dry-run output**

In `src/tools/refactoring/rename.rs`, update the dry-run formatting section (~line 189-215):

```rust
// Replace the current file_summary with line-level preview:
let mut file_previews: Vec<String> = Vec::new();
for (file_path, changes) in &renamed_files {
    file_previews.push(format!("  {} ({} changes):", file_path, changes.len()));
    for change in changes.iter().take(5) { // Cap at 5 lines per file
        file_previews.push(format!("    L{}: - {}", change.line_number, change.old_line.trim()));
        file_previews.push(format!("    L{}: + {}", change.line_number, change.new_line.trim()));
    }
    if changes.len() > 5 {
        file_previews.push(format!("    ... and {} more changes", changes.len() - 5));
    }
}
```

Note: `renamed_files` type changes from `Vec<(String, usize)>` to `Vec<(String, Vec<RenameChange>)>`. Update the type and all downstream references (the total_changes computation becomes `.iter().map(|(_, c)| c.len()).sum::<usize>()`).

- [ ] **Step 7: Run test to verify the formatted output**

```bash
cargo test --lib test_rename_dry_run 2>&1 | tail -10
```

- [ ] **Step 8: Commit**

```bash
git add src/tools/refactoring/mod.rs src/tools/refactoring/rename.rs src/tests/
git commit -m "feat(rename): line-level diff preview in dry-run output

rename_symbol dry-run now shows line numbers with before/after for
each changed line, capped at 5 per file. Makes it possible to
evaluate rename safety without applying changes."
```

---

## Final Verification

- [ ] **Step 1: Run `cargo xtask test dev`**

```bash
cargo xtask test dev 2>&1 | tail -20
```

Expect: all buckets pass (ignore known pre-existing `core-embeddings` failure).

- [ ] **Step 2: Manual smoke test (requires rebuild)**

Ask user to exit Claude Code, then:
```bash
cargo build --release
```

Restart Claude Code and test:
```
deep_dive(symbol="SearchIndex::search_symbols")
fast_refs(symbol="SearchIndex::search_symbols")
rename_symbol(old_name="search_symbols", new_name="query_symbols", dry_run=true)
```

- [ ] **Step 3: Final commit with .memories/**

```bash
git add .memories/
git commit -m "chore: checkpoint goldfish state after GPT review fixes"
```
