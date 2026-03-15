# JS/TS Test Call Expression Extraction

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Jest/Vitest/Mocha/Bun test DSL call expressions (`describe`, `it`, `test`, `beforeEach`, etc.) as named symbols so test coverage, change risk, and security risk metadata works for TypeScript and JavaScript codebases.

**Architecture:** Add a shared `test_calls` module in the extractors crate that both JS and TS extractors call. Modify `visit_node` in both extractors to handle `call_expression` for test runners. Modify `find_containing_function_in_symbols` in both extractors to resolve test call parents for relationship attribution.

**Tech Stack:** Rust, tree-sitter (typescript/javascript grammars)

---

## File Structure

| File | Responsibility | Action |
|------|---------------|--------|
| `crates/julie-extractors/src/test_calls.rs` | Shared test call extraction logic | **Create** |
| `crates/julie-extractors/src/lib.rs` | Module registration | Modify: add `mod test_calls` |
| `crates/julie-extractors/src/typescript/symbols.rs` | TS symbol visitor | Modify: add `call_expression` arm |
| `crates/julie-extractors/src/typescript/mod.rs` | TS relationship attribution | Modify: update `find_containing_function_in_symbols` |
| `crates/julie-extractors/src/javascript/mod.rs` | JS symbol visitor + relationship attribution | Modify: add call_expression arm + update find_containing |
| `crates/julie-extractors/src/tests/typescript/mod.rs` | TS extractor tests | Modify: add test call extraction tests |

---

## Chunk 1: Shared Test Call Extraction

### Task 1: Create shared test_calls module

The same test runner functions are used across JS, TS, Vue, and potentially Svelte. Extract the detection and symbol creation into shared code.

**Files:**
- Create: `crates/julie-extractors/src/test_calls.rs`
- Modify: `crates/julie-extractors/src/lib.rs`

**Test runner functions to recognize:**
- Test blocks: `it`, `test` → `is_test = true`
- Container blocks: `describe`, `context`, `suite` → `is_test = false` (containers, not tests — but still extracted as symbols for parent tracking)
- Lifecycle: `beforeEach`, `afterEach`, `beforeAll`, `afterAll`, `before`, `after` → `is_test = true`

**Tree-sitter structure of `it("name", () => { ... })`:**
```
call_expression
  function: identifier "it"       ← callee name
  arguments: arguments
    string/template_string "name" ← first arg = test name
    arrow_function/function       ← callback body
```

- [ ] **Step 1: Create the shared module**

```rust
// crates/julie-extractors/src/test_calls.rs
//! Shared extraction of test DSL call expressions (Jest, Vitest, Mocha, Bun).
//!
//! Recognizes `describe()`, `it()`, `test()`, `beforeEach()`, etc. and creates
//! named symbols from them. Used by both TypeScript and JavaScript extractors.

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use crate::test_detection::is_test_symbol;
use serde_json::json;
use tree_sitter::Node;

/// Test runner function names that should be extracted as symbols.
const TEST_BLOCKS: &[&str] = &["it", "test"];
const CONTAINER_BLOCKS: &[&str] = &["describe", "context", "suite"];
const LIFECYCLE_BLOCKS: &[&str] = &[
    "beforeEach", "afterEach", "beforeAll", "afterAll", "before", "after",
];

/// Check if a function name is a test runner call expression.
pub fn is_test_runner_call(name: &str) -> bool {
    // Also handle `.skip`, `.only`, `.todo` variants (e.g., `it.skip`, `describe.only`)
    let base_name = name.split('.').next().unwrap_or(name);
    TEST_BLOCKS.contains(&base_name)
        || CONTAINER_BLOCKS.contains(&base_name)
        || LIFECYCLE_BLOCKS.contains(&base_name)
}

/// Extract a test call expression as a Symbol.
///
/// Returns `None` if the node is not a recognized test runner call.
pub fn extract_test_call(
    base: &BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    // Get the callee name
    let function_node = node.child_by_field_name("function")?;
    let callee_name = match function_node.kind() {
        "identifier" => base.get_node_text(&function_node),
        // Handle `describe.only(...)`, `it.skip(...)` etc.
        "member_expression" => {
            let obj = function_node.child_by_field_name("object")?;
            base.get_node_text(&obj)
        }
        _ => return None,
    };

    let base_callee = callee_name.split('.').next().unwrap_or(&callee_name);

    let is_test_block = TEST_BLOCKS.contains(&base_callee);
    let is_container = CONTAINER_BLOCKS.contains(&base_callee);
    let is_lifecycle = LIFECYCLE_BLOCKS.contains(&base_callee);

    if !is_test_block && !is_container && !is_lifecycle {
        return None;
    }

    // Extract the symbol name
    let args_node = node.child_by_field_name("arguments")?;
    let symbol_name = if is_lifecycle {
        // Lifecycle functions don't have a string name arg — use the function name
        callee_name.clone()
    } else {
        // describe/it/test: first argument is the test name string
        let mut cursor = args_node.walk();
        let first_arg = args_node.children(&mut cursor)
            .find(|c| c.kind() == "string" || c.kind() == "template_string")?;

        let raw = base.get_node_text(&first_arg);
        // Strip surrounding quotes
        raw.trim_matches(|c| c == '"' || c == '\'' || c == '`').to_string()
    };

    if symbol_name.is_empty() {
        return None;
    }

    // Build signature
    let signature = format!("{}(\"{}\")", callee_name, symbol_name);

    // Determine metadata
    let mut metadata = serde_json::Map::new();
    if is_test_block || is_lifecycle {
        metadata.insert("is_test".to_string(), json!(true));
    }
    if is_lifecycle {
        metadata.insert("test_lifecycle".to_string(), json!(true));
    }
    if is_container {
        metadata.insert("test_container".to_string(), json!(true));
    }

    let language = base.language.clone();
    let file_path = base.file_path.clone();

    // Use is_test_symbol for consistency (it checks file path + name)
    // But override for explicit test blocks
    let should_mark_test = is_test_block || is_lifecycle;
    if should_mark_test && !metadata.contains_key("is_test") {
        metadata.insert("is_test".to_string(), json!(true));
    }

    let sym = base.create_symbol(SymbolOptions {
        name: symbol_name,
        kind: SymbolKind::Function,
        node,
        parent_id: parent_id.map(String::from),
        metadata: if metadata.is_empty() { None } else { Some(metadata) },
        signature: Some(signature),
        ..Default::default()
    });

    Some(sym)
}
```

- [ ] **Step 2: Register the module**

In `crates/julie-extractors/src/lib.rs`, add:
```rust
pub mod test_calls;
```

- [ ] **Step 3: Commit**

```bash
git add crates/julie-extractors/src/test_calls.rs crates/julie-extractors/src/lib.rs
git commit -m "feat(extractors): add shared test call extraction module

Recognizes Jest/Vitest/Mocha/Bun test DSL call expressions (describe,
it, test, beforeEach, afterEach, etc.) and creates named symbols from
them. Shared by both TypeScript and JavaScript extractors."
```

---

## Chunk 2: TypeScript Integration + Relationship Attribution

### Task 2: Add test call extraction to TypeScript visitor

**Files:**
- Modify: `crates/julie-extractors/src/typescript/symbols.rs:26-110` (visit_node)

- [ ] **Step 1: Write failing test**

In `crates/julie-extractors/src/tests/typescript/mod.rs`, add a test that extracts symbols from a test file with `describe`/`it`/`beforeEach`:

```rust
#[test]
fn test_extract_jest_test_calls() {
    let code = r#"
import { describe, it, expect, beforeEach } from 'bun:test';

describe("payment handler", () => {
    beforeEach(() => {
        setupDatabase();
    });

    it("should process payment", async () => {
        const result = await processPayment(100);
        expect(result).toBeDefined();
    });

    it("should reject negative amount", () => {
        expect(() => processPayment(-1)).toThrow();
    });
});
"#;
    let tree = parse_typescript(code);
    let workspace_root = PathBuf::from("/tmp/test");
    let result = extract_symbols_and_relationships(
        &tree, "tests/payment.test.ts", code, "typescript", &workspace_root
    ).unwrap();

    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();

    // Should extract describe, it, and beforeEach as symbols
    assert!(names.contains(&"payment handler"), "describe block should be extracted. Got: {:?}", names);
    assert!(names.contains(&"should process payment"), "it block should be extracted. Got: {:?}", names);
    assert!(names.contains(&"should reject negative amount"), "second it block should be extracted. Got: {:?}", names);
    assert!(names.contains(&"beforeEach"), "beforeEach should be extracted. Got: {:?}", names);

    // it blocks should be marked as tests
    let test_sym = result.symbols.iter().find(|s| s.name == "should process payment").unwrap();
    let meta = test_sym.metadata.as_ref().unwrap();
    assert_eq!(meta.get("is_test").and_then(|v| v.as_bool()), Some(true));

    // describe should NOT be marked as test
    let desc_sym = result.symbols.iter().find(|s| s.name == "payment handler").unwrap();
    let desc_meta = desc_sym.metadata.as_ref();
    let is_test = desc_meta.and_then(|m| m.get("is_test")).and_then(|v| v.as_bool());
    assert_ne!(is_test, Some(true), "describe should not be is_test");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib test_extract_jest_test_calls 2>&1 | tail -10`
Expected: FAIL — "describe block should be extracted"

- [ ] **Step 3: Add call_expression handling to visit_node**

In `crates/julie-extractors/src/typescript/symbols.rs`, add to the match:

```rust
// Test call expressions (describe, it, test, beforeEach, etc.)
"call_expression" => {
    // Check if this is a test runner call before extracting
    if let Some(function_node) = node.child_by_field_name("function") {
        let callee = match function_node.kind() {
            "identifier" => extractor.base().get_node_text(&function_node),
            "member_expression" => {
                if let Some(obj) = function_node.child_by_field_name("object") {
                    extractor.base().get_node_text(&obj)
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        };
        if crate::test_calls::is_test_runner_call(&callee) {
            // Find parent describe symbol for nesting
            let parent = symbols.iter().rev()
                .find(|s| {
                    s.metadata.as_ref()
                        .and_then(|m| m.get("test_container"))
                        .and_then(|v| v.as_bool()) == Some(true)
                    && s.start_byte <= node.start_byte() as u64
                    && s.end_byte >= node.end_byte() as u64
                })
                .map(|s| s.id.as_str());
            symbol = crate::test_calls::extract_test_call(
                extractor.base(), node, parent
            );
        }
    }
}
```

Also add `use crate::test_calls;` at the top of the file.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib test_extract_jest_test_calls 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/julie-extractors/src/typescript/symbols.rs
git commit -m "feat(typescript): extract test call expressions as symbols

describe/it/test/beforeEach/afterEach/beforeAll/afterAll are now
extracted as named symbols in TypeScript test files. it/test blocks
get is_test=true, lifecycle blocks get is_test=true, describe blocks
are containers for parent tracking."
```

### Task 3: Update find_containing_function_in_symbols for test calls

**Files:**
- Modify: `crates/julie-extractors/src/typescript/mod.rs:149-177`

The current implementation walks up tree parents looking for `function_declaration`, `method_definition`, or `arrow_function` with a name field. Inside an `it()` callback, the arrow function is anonymous — no name field. We need to also check for `call_expression` parents with test runner names.

- [ ] **Step 1: Write failing test for relationship attribution**

```rust
#[test]
fn test_jest_test_creates_relationships_to_called_functions() {
    let code = r#"
import { describe, it } from 'bun:test';
import { processPayment } from './payments';

function helper() { return 42; }

describe("payments", () => {
    it("should process", async () => {
        helper();
        processPayment(100);
    });
});
"#;
    let tree = parse_typescript(code);
    let workspace_root = PathBuf::from("/tmp/test");
    let result = extract_symbols_and_relationships(
        &tree, "tests/payment.test.ts", code, "typescript", &workspace_root
    ).unwrap();

    // The it block should have a relationship to helper() (local call)
    let test_sym = result.symbols.iter().find(|s| s.name == "should process").unwrap();
    let has_call_from_test = result.relationships.iter().any(|r|
        r.from_symbol_id == test_sym.id && r.kind == RelationshipKind::Calls
    );
    assert!(has_call_from_test, "it block should have call relationships. Rels: {:?}",
        result.relationships.iter().map(|r| (&r.from_symbol_id, &r.to_symbol_id, &r.kind)).collect::<Vec<_>>());

    // Should also have a pending relationship to processPayment (imported)
    let has_pending = result.pending_relationships.iter().any(|p|
        p.callee_name == "processPayment"
    );
    assert!(has_pending, "Should have pending relationship to imported processPayment");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p julie-extractors --lib test_jest_test_creates_relationships 2>&1 | tail -10`
Expected: FAIL — no relationships from test symbols

- [ ] **Step 3: Update find_containing_function_in_symbols**

In `crates/julie-extractors/src/typescript/mod.rs`, modify `find_containing_function_in_symbols`:

```rust
fn find_containing_function_in_symbols<'a>(
    &self,
    node: tree_sitter::Node,
    symbol_map: &'a std::collections::HashMap<String, &'a Symbol>,
) -> Option<&'a Symbol> {
    let mut current = node.parent();

    while let Some(current_node) = current {
        // Check for function declarations
        if current_node.kind() == "function_declaration"
            || current_node.kind() == "method_definition"
            || current_node.kind() == "arrow_function"
        {
            if let Some(name_node) = current_node.child_by_field_name("name") {
                let func_name = self.base.get_node_text(&name_node);
                if let Some(symbol) = symbol_map.get(&func_name) {
                    if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) {
                        return Some(symbol);
                    }
                }
            }
        }

        // Check for test call expressions (it, test, describe, beforeEach, etc.)
        // The arrow_function inside it("name", () => {...}) has no name field,
        // so we look one level up at the call_expression and use the test name.
        if current_node.kind() == "call_expression" {
            if let Some(function_node) = current_node.child_by_field_name("function") {
                let callee = match function_node.kind() {
                    "identifier" => self.base.get_node_text(&function_node),
                    "member_expression" => {
                        if let Some(obj) = function_node.child_by_field_name("object") {
                            self.base.get_node_text(&obj)
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                };

                if crate::test_calls::is_test_runner_call(&callee) {
                    // Get the test name from the first string argument
                    if let Some(args) = current_node.child_by_field_name("arguments") {
                        let mut cursor = args.walk();
                        if let Some(first_str) = args.children(&mut cursor)
                            .find(|c| c.kind() == "string" || c.kind() == "template_string")
                        {
                            let name = self.base.get_node_text(&first_str)
                                .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                                .to_string();
                            if let Some(symbol) = symbol_map.get(&name) {
                                return Some(symbol);
                            }
                        }
                        // For lifecycle (no string arg), look up by callee name
                        if let Some(symbol) = symbol_map.get(&callee) {
                            return Some(symbol);
                        }
                    }
                }
            }
        }

        current = current_node.parent();
    }

    None
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p julie-extractors --lib test_jest_test_creates_relationships 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/julie-extractors/src/typescript/mod.rs
git commit -m "fix(typescript): attribute relationships inside test callbacks to test symbols

find_containing_function_in_symbols now recognizes call_expression
parents with test runner names (it, test, describe, beforeEach) and
resolves the containing test symbol by its string argument name.
Previously, arrow functions inside it() were anonymous and relationship
attribution fell through."
```

---

## Chunk 3: JavaScript Integration

### Task 4: Apply same changes to JavaScript extractor

The JS extractor has the same structure (`visit_node`, `find_containing_function_in_symbols`). Apply identical changes.

**Files:**
- Modify: `crates/julie-extractors/src/javascript/mod.rs:225-299` (visit_node) and `:143-174` (find_containing_function_in_symbols)

- [ ] **Step 1: Add call_expression handling to JS visit_node**

Same pattern as TypeScript — add a match arm for `"call_expression"` that checks `is_test_runner_call` and calls `extract_test_call`.

- [ ] **Step 2: Update JS find_containing_function_in_symbols**

Same pattern as TypeScript — add the `call_expression` check with test name resolution.

- [ ] **Step 3: Write and run a JS-specific test**

Similar to the TS test but with JavaScript file path and `"javascript"` language.

- [ ] **Step 4: Commit**

```bash
git add crates/julie-extractors/src/javascript/mod.rs
git commit -m "feat(javascript): extract test call expressions as symbols

Same changes as TypeScript: describe/it/test/beforeEach etc. extracted
as named symbols, relationship attribution works inside test callbacks."
```

---

## Chunk 4: Verification

### Task 5: Run tests and dogfood

- [ ] **Step 1: Run xtask dev tier**

Run: `cargo xtask test dev`
Expected: All buckets pass

- [ ] **Step 2: Build release and re-index**

```bash
cargo build --release
# Restart Claude Code, then:
manage_workspace(operation="remove", workspace_id="goldfish_5ed767a5")
manage_workspace(operation="add", path="/Users/murphy/source/goldfish", name="Goldfish (TypeScript)")
```

- [ ] **Step 3: Dogfood verification**

```
deep_dive(symbol="saveCheckpoint", workspace="goldfish_...")
```
Expected: test coverage shows (no longer "untested"), change risk recalculated with test data

```
deep_dive(symbol="handleCheckpoint", workspace="goldfish_...")
```
Expected: test locations with quality tiers
