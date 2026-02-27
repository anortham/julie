# Dogfood Bug Fixes: fast_search Hybrid + Rust Extractor Qualified Paths

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix two bugs discovered during semantic embedding dogfood testing: (1) fast_search NL queries never use semantic search, (2) fast_refs misses Rust qualified-path calls.

**Architecture:** Bug 1 replaces the dead bolt-on semantic fallback in `text_search_impl` with a call to `hybrid_search()` for NL definition queries. Bug 2 adds `scoped_identifier` handling to the Rust extractor's identifier and relationship extraction so `crate::module::func()` calls are indexed as `func`.

**Tech Stack:** Rust, tree-sitter (Rust grammar), Tantivy, sqlite-vec

---

## Bug 1: fast_search Uses hybrid_search() for NL Queries

### Task 1: Write failing test for NL definition search using hybrid_search

**Files:**
- Modify: `src/tests/tools/hybrid_search_tests.rs`

**Step 1: Write the failing test**

Add a test that verifies `hybrid_search` is called for NL-like queries in the definition path. Since `text_search_impl` is async and needs a full handler, test at the `hybrid_search` level instead — verify that `hybrid_search` with `None` provider still works (this already exists), and add a test that `is_nl_like_query` correctly identifies NL queries we care about.

```rust
#[test]
fn test_is_nl_like_query_examples() {
    use crate::search::scoring::is_nl_like_query;

    // NL queries that SHOULD trigger hybrid search
    assert!(is_nl_like_query("how does the server start up"));
    assert!(is_nl_like_query("find symbols similar to each other"));
    assert!(is_nl_like_query("what happens when a file is modified"));

    // Code queries that should NOT trigger hybrid search
    assert!(!is_nl_like_query("UserService"));
    assert!(!is_nl_like_query("extract_identifiers"));
    assert!(!is_nl_like_query("rrf_merge"));
}
```

**Step 2: Run test to verify it passes (this is a characterization test)**

Run: `cargo test --lib test_is_nl_like_query_examples 2>&1 | tail -5`
Expected: PASS (this confirms the NL detection works correctly for our use cases)

**Step 3: Commit**

```bash
git add src/tests/tools/hybrid_search_tests.rs
git commit -m "test: add characterization tests for is_nl_like_query detection"
```

### Task 2: Replace bolt-on fallback with hybrid_search() call

**Files:**
- Modify: `src/tools/search/text_search.rs:237-340`
- Modify: `src/search/hybrid.rs:23-30`

**Step 1: Modify `text_search_impl` to use `hybrid_search()` for NL definition queries**

In `src/tools/search/text_search.rs`, replace the definition search section (lines ~245-340). The key change:

1. Before the keyword-only search at line 254, add an NL check that routes through `hybrid_search()`
2. Remove the dead semantic fallback block (lines 307-340)

The new flow in the `"definitions"` branch:
```rust
// Inside the spawn_blocking closure, "definitions" branch:

// Check if this is an NL query and we have embeddings available
let use_hybrid = crate::search::scoring::is_nl_like_query(&query_clone)
    && embedding_provider.is_some();

if use_hybrid {
    debug!("🔍 NL query detected, using hybrid search (keyword + semantic)");

    // Get DB reference for hybrid search
    let db_guard = db_clone.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not initialized"))?;
    let db_lock = match db_guard.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("Database mutex poisoned during hybrid search, recovering");
            poisoned.into_inner()
        }
    };

    let hybrid_results = crate::search::hybrid::hybrid_search(
        &query_clone,
        &filter,
        limit_usize,
        &index,
        &db_lock,
        embedding_provider.as_deref(),
    )?;
    let relaxed = hybrid_results.relaxed;

    let mut symbols: Vec<Symbol> = hybrid_results.results
        .into_iter()
        .map(|result| tantivy_symbol_to_symbol(result))
        .collect();

    // Enrich with code_context from SQLite
    enrich_symbols_from_db(&mut symbols, &db_lock);

    Ok((symbols, relaxed))
} else {
    // Existing keyword-only path (unchanged except removing dead fallback)
    // ... (keep lines 246-305 as-is, remove 307-340)
    debug!("🔍 Searching symbols with Tantivy");
    // ... rest of keyword-only path through line 342
}
```

**Step 2: Clean up `hybrid.rs` — remove `should_use_semantic_fallback`**

In `src/search/hybrid.rs`, remove `should_use_semantic_fallback` (lines 23-30) and the `is_nl_like_query` import (line 21) since nothing uses them anymore.

**Step 3: Run tests**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: PASS (no test should reference the removed function)

If any test imports `should_use_semantic_fallback`, update it to either test `is_nl_like_query` directly or remove the test.

**Step 4: Commit**

```bash
git add src/tools/search/text_search.rs src/search/hybrid.rs
git commit -m "feat: route NL definition queries through hybrid_search instead of dead fallback"
```

### Task 3: Verify fix with manual test

**Step 1: Build and verify**

Note: Cannot rebuild while MCP server is running. Tell the user to rebuild and restart, then test with:
- `fast_search(query="how are search results ranked and scored", search_target="definitions")`
- `fast_search(query="what happens when a file is modified", search_target="definitions")`

These NL queries should now return code symbols, not just doc file lines.

---

## Bug 2: Rust Extractor — Extract Last Segment from Scoped Identifiers

### Task 4: Write failing test for scoped identifier extraction

**Files:**
- Modify: `crates/julie-extractors/src/tests/rust/identifiers.rs`

**Step 1: Replace the placeholder with real tests**

Replace the entire file with tests for scoped identifier extraction:

```rust
// Tests for Rust identifier extraction with scoped/qualified paths
//
// Bug: `crate::module::function()` was indexed as "crate::module::function"
// instead of "function", causing fast_refs to miss the reference.

use crate::base::{Identifier, IdentifierKind, Symbol};
use crate::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    parser
}

fn extract_all(code: &str) -> (Vec<Symbol>, Vec<Identifier>) {
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let identifiers = extractor.extract_identifiers(&tree, &symbols);
    (symbols, identifiers)
}

#[test]
fn test_scoped_call_extracts_last_segment() {
    let code = r#"
fn caller() {
    crate::search::hybrid::should_use_semantic_fallback("query", 5);
}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let calls: Vec<&Identifier> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::Call)
        .collect();

    assert!(!calls.is_empty(), "Should find at least one call identifier");
    // The identifier name should be just the last segment, not the full path
    let call = calls.iter().find(|id| id.name.contains("semantic_fallback") || id.name == "should_use_semantic_fallback");
    assert!(call.is_some(), "Should find should_use_semantic_fallback call, got: {:?}", calls.iter().map(|c| &c.name).collect::<Vec<_>>());
    assert_eq!(call.unwrap().name, "should_use_semantic_fallback",
        "Should extract bare name, not qualified path");
}

#[test]
fn test_simple_call_still_works() {
    let code = r#"
fn caller() {
    do_something();
}
fn do_something() {}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let calls: Vec<&Identifier> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::Call)
        .collect();

    assert!(calls.iter().any(|c| c.name == "do_something"),
        "Simple calls should still work, got: {:?}", calls.iter().map(|c| &c.name).collect::<Vec<_>>());
}

#[test]
fn test_nested_scoped_call() {
    let code = r#"
fn example() {
    std::collections::HashMap::new();
}
"#;
    let (_symbols, identifiers) = extract_all(code);
    let calls: Vec<&Identifier> = identifiers
        .iter()
        .filter(|id| id.kind == IdentifierKind::Call)
        .collect();

    // Should extract "new" as the call name, not the full qualified path
    assert!(calls.iter().any(|c| c.name == "new"),
        "Should extract 'new' from HashMap::new(), got: {:?}", calls.iter().map(|c| &c.name).collect::<Vec<_>>());
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p julie-extractors --lib test_scoped_call_extracts_last_segment 2>&1 | tail -10`
Expected: FAIL — "Should extract bare name, not qualified path"

**Step 3: Commit failing tests**

```bash
git add crates/julie-extractors/src/tests/rust/identifiers.rs
git commit -m "test(RED): add failing tests for scoped identifier extraction in Rust"
```

### Task 5: Fix identifier extraction for scoped_identifier nodes

**Files:**
- Modify: `crates/julie-extractors/src/rust/identifiers.rs:56-73`

**Step 1: Add scoped_identifier handling in extract_identifier_from_node**

In the `call_expression` handler, after the `field_expression` check (line 62) and before the else branch (line 69), add a check for `scoped_identifier`:

```rust
// In extract_identifier_from_node, inside the "call_expression" match arm:
let name = {
    let base = extractor.get_base_mut();
    if func_node.kind() == "field_expression" {
        // Method call: extract just the field name
        if let Some(field_node) = func_node.child_by_field_name("field") {
            base.get_node_text(&field_node)
        } else {
            base.get_node_text(&func_node)
        }
    } else if func_node.kind() == "scoped_identifier" {
        // Qualified call: crate::module::function() → extract "function"
        if let Some(name_node) = func_node.child_by_field_name("name") {
            base.get_node_text(&name_node)
        } else {
            base.get_node_text(&func_node)
        }
    } else {
        // Regular function call
        base.get_node_text(&func_node)
    }
};

let identifier_node = if func_node.kind() == "field_expression" {
    if let Some(field_node) = func_node.child_by_field_name("field") {
        field_node
    } else {
        func_node
    }
} else if func_node.kind() == "scoped_identifier" {
    if let Some(name_node) = func_node.child_by_field_name("name") {
        name_node
    } else {
        func_node
    }
} else {
    func_node
};
```

**Step 2: Run the tests**

Run: `cargo test -p julie-extractors --lib test_scoped_call 2>&1 | tail -10`
Expected: PASS

Run: `cargo test -p julie-extractors --lib test_simple_call_still_works 2>&1 | tail -5`
Expected: PASS

Run: `cargo test -p julie-extractors --lib test_nested_scoped_call 2>&1 | tail -5`
Expected: PASS

**Step 3: Run the full extractor test suite**

Run: `cargo test -p julie-extractors --lib 2>&1 | tail -5`
Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/julie-extractors/src/rust/identifiers.rs
git commit -m "fix: extract last segment from scoped_identifier in Rust call expressions"
```

### Task 6: Fix relationship extraction for scoped_identifier nodes

**Files:**
- Modify: `crates/julie-extractors/src/rust/relationships.rs:182-216`
- Modify: `crates/julie-extractors/src/tests/rust/relationships.rs`

**Step 1: Write failing test for scoped call relationships**

Replace `crates/julie-extractors/src/tests/rust/relationships.rs` with:

```rust
// Tests for Rust relationship extraction with scoped/qualified paths

use crate::base::Symbol;
use crate::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");
    parser
}

#[test]
fn test_scoped_call_creates_relationship_with_bare_name() {
    let code = r#"
fn target_function() {}

fn caller() {
    crate::module::target_function();
}
"#;
    let mut parser = init_parser();
    let tree = parser.parse(code, None).unwrap();
    let workspace_root = PathBuf::from("/tmp/test");
    let mut extractor = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
        code.to_string(),
        &workspace_root,
    );
    let symbols = extractor.extract_symbols(&tree);
    let relationships = extractor.extract_relationships(&tree, &symbols);

    // Should find a Calls relationship to target_function
    // The relationship resolver uses the bare name to match against symbol_map
    assert!(!relationships.is_empty(),
        "Should create at least one relationship for the scoped call");
}
```

**Step 2: Run test**

Run: `cargo test -p julie-extractors --lib test_scoped_call_creates_relationship 2>&1 | tail -10`
Expected: FAIL — no relationship created because `extract_call_relationships` only handles `identifier` and `field_expression`, not `scoped_identifier`.

**Step 3: Fix `extract_call_relationships` in `relationships.rs:182-216`**

Add `scoped_identifier` handling alongside the existing `identifier` check:

```rust
fn extract_call_relationships(
    extractor: &mut RustExtractor,
    node: Node,
    symbol_map: &HashMap<String, &Symbol>,
    relationships: &mut Vec<Relationship>,
) {
    let function_node = node.child_by_field_name("function");
    if let Some(func_node) = function_node {
        // Handle method calls (receiver.method())
        if func_node.kind() == "field_expression" {
            let method_node = func_node.child_by_field_name("field");
            if let Some(method_node) = method_node {
                let method_name = extractor.get_base_mut().get_node_text(&method_node);
                handle_call_target(
                    extractor, node, &method_name, symbol_map, relationships,
                );
            }
        }
        // Handle direct function calls
        else if func_node.kind() == "identifier" {
            let function_name = extractor.get_base_mut().get_node_text(&func_node);
            handle_call_target(
                extractor, node, &function_name, symbol_map, relationships,
            );
        }
        // Handle qualified/scoped calls: crate::module::function()
        else if func_node.kind() == "scoped_identifier" {
            let function_name = if let Some(name_node) = func_node.child_by_field_name("name") {
                extractor.get_base_mut().get_node_text(&name_node)
            } else {
                extractor.get_base_mut().get_node_text(&func_node)
            };
            handle_call_target(
                extractor, node, &function_name, symbol_map, relationships,
            );
        }
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p julie-extractors --lib test_scoped_call_creates_relationship 2>&1 | tail -5`
Expected: PASS

Run: `cargo test -p julie-extractors --lib 2>&1 | tail -5`
Expected: All tests pass

**Step 5: Commit**

```bash
git add crates/julie-extractors/src/rust/relationships.rs crates/julie-extractors/src/tests/rust/relationships.rs
git commit -m "fix: handle scoped_identifier in Rust relationship extraction"
```

### Task 7: Run full fast test suite and verify

**Step 1: Run fast tier tests across both crates**

Run: `cargo test --lib -- --skip search_quality 2>&1 | tail -5`
Expected: All tests pass

**Step 2: Commit any remaining changes and verify clean state**

Run: `git status -s`
Expected: No uncommitted changes

---

## Summary

| Task | Bug | What | Files |
|------|-----|------|-------|
| 1 | #1 | Characterization test for `is_nl_like_query` | `hybrid_search_tests.rs` |
| 2 | #1 | Replace fallback with `hybrid_search()` | `text_search.rs`, `hybrid.rs` |
| 3 | #1 | Manual verification (rebuild required) | — |
| 4 | #2 | Failing test for scoped identifier extraction | `tests/rust/identifiers.rs` |
| 5 | #2 | Fix identifier extraction | `rust/identifiers.rs` |
| 6 | #2 | Fix relationship extraction + test | `rust/relationships.rs` |
| 7 | — | Full test suite verification | — |

Tasks 1-3 (Bug 1) and Tasks 4-6 (Bug 2) are independent and can be implemented in parallel.
