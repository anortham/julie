# Token Efficiency Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce token consumption of Julie MCP tool outputs by 30-60% across the most common tool calls, without changing tool descriptions or server instructions.

**Architecture:** Seven independent changes to server-side formatting and defaults. Six are transparent output optimizations (agents don't need to change behavior). One is a new plugin skill. All changes are backward-compatible; agents that explicitly pass current defaults continue to get the same behavior.

**Tech Stack:** Rust (server-side formatting), Markdown (skill)

---

## File Map

| Change | Files to Modify | Files to Create | Test Files |
|--------|----------------|-----------------|------------|
| Task 1: get_symbols default | `src/tools/symbols/mod.rs` | | `src/tests/tools/get_symbols.rs`, `src/tests/tools/get_symbols_token.rs` |
| Task 2: fast_search locations mode | `src/tools/search/mod.rs`, `src/tools/search/formatting.rs` | | `src/tests/tools/search/lean_format_tests.rs` |
| Task 3: deep_dive token cap | `src/tools/deep_dive/formatting.rs` | | `src/tests/tools/deep_dive_tests.rs` |
| Task 4: group-by-file search output | `src/tools/search/formatting.rs` | | `src/tests/tools/search/lean_format_tests.rs` |
| Task 5: kind prefix dedup | `src/tools/symbols/formatting.rs` | | `src/tests/tools/get_symbols.rs` |
| Task 6: get_context compact tightening | `src/tools/get_context/formatting.rs`, `src/tools/get_context/pipeline.rs` | | `src/tests/tools/get_context_formatting_tests.rs` |
| Task 7: /efficient skill | | `julie-plugin/skills/efficient/SKILL.md` | (manual testing) |

---

### Task 1: get_symbols default mode from "minimal" to "structure"

The highest-impact single change. When agents call `get_symbols` without specifying `mode`, they currently get code bodies (minimal mode). Most calls are for orientation ("what's in this file?"), not code extraction. Changing the default to "structure" (names/signatures only) saves 50-80% of output tokens per call.

**Files:**
- Modify: `src/tools/symbols/mod.rs:31` (default_mode function)
- Test: `src/tests/tools/get_symbols.rs`, `src/tests/tools/get_symbols_token.rs`

- [ ] **Step 1: Write failing test for new default behavior**

In `src/tests/tools/get_symbols.rs`, add a test that verifies the default mode is "structure":

```rust
#[test]
fn test_default_mode_is_structure() {
    let default = super::super::tools::symbols::default_mode();
    assert_eq!(default, Some("structure".to_string()),
        "Default mode should be 'structure' for token efficiency. \
         Agents that need code bodies should explicitly request mode='minimal'");
}
```

Note: `default_mode` is `fn default_mode() -> Option<String>` at `src/tools/symbols/mod.rs:31`. It's `pub(crate)` scope via the module, so test access depends on existing test patterns. Check how existing tests in `get_symbols.rs` access the tool. If they go through `GetSymbolsTool` deserialization, write the test that way instead:

```rust
#[test]
fn test_default_mode_is_structure() {
    let tool: GetSymbolsTool = serde_json::from_str(r#"{"file_path": "test.rs"}"#).unwrap();
    assert_eq!(tool.mode, Some("structure".to_string()),
        "Default mode should be 'structure' for token efficiency");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_default_mode_is_structure 2>&1 | tail -10`
Expected: FAIL with assertion error showing `Some("minimal")` != `Some("structure")`

- [ ] **Step 3: Change the default**

In `src/tools/symbols/mod.rs`, change the `default_mode` function:

```rust
fn default_mode() -> Option<String> {
    Some("structure".to_string()) // Default to structure for token efficiency; use "minimal" for code bodies
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_default_mode_is_structure 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Check for broken tests that assumed minimal default**

Run: `cargo test --lib tests::tools::get_symbols 2>&1 | tail -30`

Some existing tests may construct `GetSymbolsTool` without specifying `mode` and expect code bodies. These tests need to explicitly pass `mode: Some("minimal".to_string())` to preserve their behavior. Fix any failures.

- [ ] **Step 6: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green. Fix any regressions.

- [ ] **Step 7: Commit**

```bash
git add src/tools/symbols/mod.rs src/tests/tools/get_symbols.rs
git commit -m "feat(symbols): change get_symbols default mode to structure

Default mode was 'minimal' (returns code bodies). Most get_symbols calls
are for file orientation, not code extraction. Changing to 'structure'
(names/signatures only) saves 50-80% output tokens per call.

Agents that need code bodies can explicitly pass mode='minimal'."
```

---

### Task 2: fast_search locations-only mode

Add a `return` parameter to `fast_search` that supports `"full"` (default, current behavior) and `"locations"` (file:line pairs only, no code context). When agents just need to find where something is defined, code snippets are wasted tokens.

**Files:**
- Modify: `src/tools/search/mod.rs:43-78` (FastSearchTool struct, add `return` field)
- Modify: `src/tools/search/mod.rs:94-222` (call_tool method, branch on return mode)
- Modify: `src/tools/search/formatting.rs` (add `format_locations_only` function)
- Test: `src/tests/tools/search/lean_format_tests.rs`

- [ ] **Step 1: Write failing test for locations-only format**

In `src/tests/tools/search/lean_format_tests.rs`, add:

```rust
#[test]
fn test_locations_only_format() {
    use crate::tools::search::formatting::format_locations_only;
    use crate::extractors::base::{Symbol, SymbolKind, Visibility};
    use crate::tools::shared::OptimizedResponse;

    let symbols = vec![
        Symbol {
            name: "process".to_string(),
            kind: SymbolKind::Function,
            file_path: "src/handler.rs".to_string(),
            start_line: 42,
            end_line: 80,
            // NOTE: Symbol has no Default impl. Use full construction (see make_test_symbol in lean_format_tests.rs for pattern)
            id: format!("test_{}_{}", file_path, start_line), language: "rust".to_string(),
            start_column: 0, end_column: 0, start_byte: 0, end_byte: 0,
            parent_id: None, signature: None, doc_comment: None, visibility: None,
            metadata: None, semantic_group: None, confidence: None, code_context: None, content_type: None
        },
        Symbol {
            name: "process".to_string(),
            kind: SymbolKind::Function,
            file_path: "src/worker.rs".to_string(),
            start_line: 15,
            end_line: 30,
            // NOTE: Symbol has no Default impl. Use full construction (see make_test_symbol in lean_format_tests.rs for pattern)
            id: format!("test_{}_{}", file_path, start_line), language: "rust".to_string(),
            start_column: 0, end_column: 0, start_byte: 0, end_byte: 0,
            parent_id: None, signature: None, doc_comment: None, visibility: None,
            metadata: None, semantic_group: None, confidence: None, code_context: None, content_type: None
        },
    ];
    let response = OptimizedResponse::with_total(symbols, 2);
    let output = format_locations_only("process", &response);

    assert!(output.contains("src/handler.rs:42"), "Should contain file:line");
    assert!(output.contains("src/worker.rs:15"), "Should contain file:line");
    // No code context in output
    assert!(!output.contains("fn "), "Should not contain code snippets");
    // Check it's compact
    assert!(output.lines().count() <= 5, "Locations-only should be very compact");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_locations_only_format 2>&1 | tail -10`
Expected: FAIL (function doesn't exist yet)

- [ ] **Step 3: Add the `format_locations_only` function**

In `src/tools/search/formatting.rs`, add:

```rust
/// Format search results as file:line locations only (no code context).
///
/// Output format:
/// ```text
/// 2 locations for "process":
///   src/handler.rs:42 (function)
///   src/worker.rs:15 (function)
/// ```
///
/// Use when the agent only needs to know WHERE a symbol is, not what it looks like.
/// Saves 70-90% tokens compared to full format.
pub fn format_locations_only(query: &str, response: &OptimizedResponse<Symbol>) -> String {
    let mut output = String::new();
    let count = response.results.len();
    let total = response.total_found;

    if count == total {
        output.push_str(&format!("{} locations for \"{}\":\n", count, query));
    } else {
        output.push_str(&format!(
            "{} locations for \"{}\" (showing {} of {}):\n",
            count, query, count, total
        ));
    }

    for symbol in &response.results {
        let kind = symbol.kind.to_string();
        output.push_str(&format!("  {}:{} ({})\n", symbol.file_path, symbol.start_line, kind));
    }

    output.trim_end().to_string()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_locations_only_format 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Add the `return` field to `FastSearchTool`**

In `src/tools/search/mod.rs`, add the field to the struct (after `exclude_tests`):

```rust
    /// Return format: "full" (default, code context included) or "locations" (file:line only, 70-90% fewer tokens)
    #[serde(default = "default_return_format")]
    pub return_format: String,
```

Add the default function:

```rust
fn default_return_format() -> String {
    "full".to_string()
}
```

- [ ] **Step 6: Wire up return_format in call_tool**

In the `call_tool` method of `FastSearchTool` (`src/tools/search/mod.rs`), after the line that calls `format_definition_search_results`, add a branch for locations mode. Find the section around line 187 where definition search results are formatted:

```rust
// Before the existing format_definition_search_results call, add:
if self.return_format == "locations" {
    let lean_output = formatting::format_locations_only(&self.query, &optimized);
    return Ok(CallToolResult::text_content(vec![Content::text(lean_output)]));
}
```

This should go right after the empty-results check and before the existing `format_definition_search_results` call. Also apply the same branch in the line_mode path if it makes sense (line_mode already outputs file:line:content, but locations mode would strip the content).

For line_mode, in the `line_mode_search` function in `src/tools/search/line_mode.rs`, the return_format param needs to be threaded through. The simplest approach: pass `return_format` as a parameter to `line_mode_search`, and if it's "locations", format as grouped file:line pairs instead of file:line:content.

- [ ] **Step 7: Write integration test for the full tool**

```rust
#[test]
fn test_fast_search_return_format_deserialization() {
    use crate::tools::search::FastSearchTool;

    // Default should be "full"
    let tool: FastSearchTool = serde_json::from_str(r#"{"query": "test"}"#).unwrap();
    assert_eq!(tool.return_format, "full");

    // Explicit "locations"
    let tool: FastSearchTool = serde_json::from_str(
        r#"{"query": "test", "return_format": "locations"}"#
    ).unwrap();
    assert_eq!(tool.return_format, "locations");
}
```

- [ ] **Step 8: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green.

- [ ] **Step 9: Commit**

```bash
git add src/tools/search/mod.rs src/tools/search/formatting.rs src/tools/search/line_mode.rs src/tests/tools/search/lean_format_tests.rs
git commit -m "feat(search): add return_format='locations' for file-only results

Agents that only need to find where a symbol lives can now use
return_format='locations' to get file:line pairs without code context.
Saves 70-90% output tokens for location lookups.

Default remains 'full' for backward compatibility."
```

---

### Task 3: deep_dive(full) enforce hard token cap

The `deep_dive` tool description claims `full` depth produces ~1500 tokens, but the actual caps allow 50+50 refs with up to 10 code lines each, plus a 100-line body. Worst case is 4000-6000 tokens. Add hard truncation to enforce the documented budget.

**Files:**
- Modify: `src/tools/deep_dive/formatting.rs` (add token-budgeted truncation)
- Modify: `src/tools/deep_dive/mod.rs` (import token estimator)
- Test: `src/tests/tools/deep_dive_tests.rs`

- [ ] **Step 1: Write failing test for output cap**

In `src/tests/tools/deep_dive_tests.rs`, add:

```rust
#[test]
fn test_deep_dive_full_output_respects_token_cap() {
    // Build a SymbolContext with maximum refs to trigger worst-case output
    use crate::tools::deep_dive::formatting::format_symbol_context;
    use crate::tools::deep_dive::data::{SymbolContext, RefEntry};
    use crate::extractors::base::{Symbol, SymbolKind, RelationshipKind};
    use crate::utils::token_estimation::TokenEstimator;

    // Create a symbol with a long body
    let mut symbol = Symbol::default();
    symbol.name = "big_function".to_string();
    symbol.kind = SymbolKind::Function;
    symbol.file_path = "src/big.rs".to_string();
    symbol.start_line = 1;
    symbol.signature = Some("pub fn big_function(a: i32, b: i32) -> Result<()>".to_string());
    symbol.code_context = Some("fn big_function() {\n".to_string() + &"    let x = 1;\n".repeat(100));

    // Create 50 incoming refs with code bodies
    let incoming: Vec<RefEntry> = (0..50).map(|i| {
        let mut s = Symbol::default();
        s.name = format!("caller_{}", i);
        s.kind = SymbolKind::Function;
        s.file_path = format!("src/callers/caller_{}.rs", i);
        s.start_line = 10;
        s.code_context = Some("fn caller() {\n".to_string() + &"    do_stuff();\n".repeat(10));
        RefEntry {
            file_path: s.file_path.clone(),
            line_number: 10,
            kind: RelationshipKind::Calls,
            symbol: Some(s),
        }
    }).collect();

    let ctx = SymbolContext {
        symbol,
        incoming,
        incoming_total: 50,
        outgoing: vec![],
        outgoing_total: 0,
        children: vec![],
        implementations: vec![],
        test_refs: vec![],
        similar: vec![],
    };

    let output = format_symbol_context(&ctx, "full");
    let estimator = TokenEstimator::new();
    let estimated_tokens = estimator.estimate(&output);

    // The documented budget for "full" is ~1500 tokens.
    // Allow some headroom but enforce a hard ceiling.
    assert!(
        estimated_tokens <= 2000,
        "deep_dive(full) output should stay under 2000 estimated tokens, got {}.\n\
         Output length: {} chars",
        estimated_tokens, output.len()
    );
}
```

Note: `Symbol` does NOT implement `Default`. Tests must construct all fields explicitly. See `src/tests/tools/search/lean_format_tests.rs:14` for the `make_test_symbol` helper pattern. The `SymbolContext` struct is in `src/tools/deep_dive/data.rs`; check its exact field names. The test above shows the expected shape; adjust field names and use full Symbol construction (all fields explicit) rather than `..Symbol::default()`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_deep_dive_full_output_respects_token_cap 2>&1 | tail -10`
Expected: FAIL with estimated_tokens >> 2000

- [ ] **Step 3: Add token-budgeted truncation to format_symbol_context**

In `src/tools/deep_dive/formatting.rs`, modify `format_symbol_context` to enforce a hard cap:

```rust
use crate::utils::token_estimation::TokenEstimator;

/// Maximum estimated tokens per depth level.
const TOKEN_CAPS: &[(&str, usize)] = &[
    ("overview", 300),
    ("context", 800),
    ("full", 1800),
];

fn token_cap_for_depth(depth: &str) -> usize {
    TOKEN_CAPS.iter()
        .find(|(d, _)| *d == depth)
        .map(|(_, cap)| *cap)
        .unwrap_or(300) // Default to overview cap for unknown depths
}
```

Then at the end of `format_symbol_context`, before the final `out.trim_end().to_string()`, add truncation:

```rust
    let cap = token_cap_for_depth(depth);
    let estimator = TokenEstimator::default();
    let estimated = estimator.estimate(&out);
    if estimated > cap {
        // Truncate: keep the header and first portion, add truncation notice
        let target_chars = (cap as f64 * 4.0) as usize; // rough chars-per-token inverse
        if out.len() > target_chars {
            out.truncate(target_chars);
            // Find last newline to avoid cutting mid-line
            if let Some(pos) = out.rfind('\n') {
                out.truncate(pos + 1);
            }
            out.push_str(&format!("  ... (truncated to ~{} token budget)\n", cap));
        }
    }

    out.trim_end().to_string()
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_deep_dive_full_output_respects_token_cap 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green.

- [ ] **Step 6: Commit**

```bash
git add src/tools/deep_dive/formatting.rs src/tests/tools/deep_dive_tests.rs
git commit -m "fix(deep_dive): enforce token cap on full depth output

deep_dive(full) claimed ~1500 tokens but could produce 4000-6000 in
worst case (50 refs x 10 code lines each + 100-line body). Now enforces
a hard cap of ~1800 estimated tokens, truncating with a notice when
the budget is exceeded."
```

---

### Task 4: Group-by-file output in search and refs results

When multiple search results or references appear in the same file, the file path is repeated for each one. Grouping by file saves 5-15% on multi-match results.

**Files:**
- Modify: `src/tools/search/formatting.rs` (modify `format_lean_search_results` and `format_definition_search_results`)
- Modify: `src/tools/navigation/formatting.rs` (modify References section of `format_lean_refs_results`)
- Test: `src/tests/tools/search/lean_format_tests.rs`, `src/tests/tools/formatting_tests.rs`

- [ ] **Step 1: Write failing test for grouped search output**

In `src/tests/tools/search/lean_format_tests.rs`, add:

```rust
#[test]
fn test_lean_format_groups_same_file_results() {
    use crate::tools::search::formatting::format_lean_search_results;
    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::tools::shared::OptimizedResponse;

    let symbols = vec![
        Symbol {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            file_path: "src/handler.rs".to_string(),
            start_line: 42,
            end_line: 50,
            code_context: Some("  42: fn foo() {".to_string()),
            // NOTE: Symbol has no Default impl. Use full construction (see make_test_symbol in lean_format_tests.rs for pattern)
            id: format!("test_{}_{}", file_path, start_line), language: "rust".to_string(),
            start_column: 0, end_column: 0, start_byte: 0, end_byte: 0,
            parent_id: None, signature: None, doc_comment: None, visibility: None,
            metadata: None, semantic_group: None, confidence: None, code_context: None, content_type: None
        },
        Symbol {
            name: "bar".to_string(),
            kind: SymbolKind::Function,
            file_path: "src/handler.rs".to_string(),
            start_line: 100,
            end_line: 110,
            code_context: Some("  100: fn bar() {".to_string()),
            // NOTE: Symbol has no Default impl. Use full construction (see make_test_symbol in lean_format_tests.rs for pattern)
            id: format!("test_{}_{}", file_path, start_line), language: "rust".to_string(),
            start_column: 0, end_column: 0, start_byte: 0, end_byte: 0,
            parent_id: None, signature: None, doc_comment: None, visibility: None,
            metadata: None, semantic_group: None, confidence: None, code_context: None, content_type: None
        },
        Symbol {
            name: "baz".to_string(),
            kind: SymbolKind::Function,
            file_path: "src/other.rs".to_string(),
            start_line: 5,
            end_line: 10,
            code_context: Some("  5: fn baz() {".to_string()),
            // NOTE: Symbol has no Default impl. Use full construction (see make_test_symbol in lean_format_tests.rs for pattern)
            id: format!("test_{}_{}", file_path, start_line), language: "rust".to_string(),
            start_column: 0, end_column: 0, start_byte: 0, end_byte: 0,
            parent_id: None, signature: None, doc_comment: None, visibility: None,
            metadata: None, semantic_group: None, confidence: None, code_context: None, content_type: None
        },
    ];

    let response = OptimizedResponse::with_total(symbols, 3);
    let output = format_lean_search_results("test", &response);

    // File path "src/handler.rs" should appear only once as a group header
    assert_eq!(
        output.matches("src/handler.rs").count(), 1,
        "Same-file results should be grouped under one file header. Output:\n{}",
        output
    );
    // Both line numbers should appear under that header
    assert!(output.contains(":42"), "Should contain line 42");
    assert!(output.contains(":100"), "Should contain line 100");
    // Different file should have its own header
    assert!(output.contains("src/other.rs"), "Should contain other file");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_lean_format_groups_same_file_results 2>&1 | tail -10`
Expected: FAIL (file path appears twice)

- [ ] **Step 3: Implement group-by-file in format_lean_search_results**

In `src/tools/search/formatting.rs`, rewrite `format_lean_search_results` to group results:

```rust
pub fn format_lean_search_results(query: &str, response: &OptimizedResponse<Symbol>) -> String {
    let mut output = String::new();

    let count = response.results.len();
    let total = response.total_found;
    if count == total {
        output.push_str(&format!("{} matches for \"{}\":\n\n", count, query));
    } else {
        output.push_str(&format!(
            "{} matches for \"{}\" (showing {} of {}):\n\n",
            count, query, count, total
        ));
    }

    // Group results by file path (preserving order of first appearance)
    let mut file_groups: Vec<(&str, Vec<&Symbol>)> = Vec::new();
    for symbol in &response.results {
        if let Some(group) = file_groups.iter_mut().find(|(path, _)| *path == symbol.file_path) {
            group.1.push(symbol);
        } else {
            file_groups.push((&symbol.file_path, vec![symbol]));
        }
    }

    for (file_path, symbols) in &file_groups {
        output.push_str(&format!("{}:\n", file_path));
        for symbol in symbols {
            if let Some(ctx) = &symbol.code_context {
                for line in ctx.lines() {
                    output.push_str(&format!("  {}\n", line));
                }
            } else {
                output.push_str(&format!("  :{}\n", symbol.start_line));
            }
        }
        output.push('\n');
    }

    output.trim_end().to_string()
}
```

Note: The code_context already includes line numbers and arrows (e.g., `42→ fn foo() {`). The file path becomes the group header, and each match appears indented under it.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_lean_format_groups_same_file_results 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Fix any broken lean_format tests**

Run: `cargo test --lib lean_format 2>&1 | tail -30`

Existing tests like `test_lean_format_single_result` and `test_lean_format_multiple_results` will need updating to match the new grouped output format. The key change: tests that assert `format!("{}:{}\n", file_path, line)` now need to assert the file path appears as a group header followed by indented matches.

- [ ] **Step 6: Apply same pattern to refs "References" section**

In `src/tools/navigation/formatting.rs`, modify the References section of `format_lean_refs_results` (around line 131-149) to group by file:

```rust
    if !references.is_empty() {
        output.push_str(&format!("References ({}):\n", references.len()));

        // Group by file path
        let mut ref_groups: Vec<(&str, Vec<&Relationship>)> = Vec::new();
        for rel in references {
            if let Some(group) = ref_groups.iter_mut().find(|(path, _)| *path == rel.file_path) {
                group.1.push(rel);
            } else {
                ref_groups.push((&rel.file_path, vec![rel]));
            }
        }

        for (file_path, rels) in &ref_groups {
            if rels.len() == 1 {
                // Single ref in file: inline format
                let rel = rels[0];
                let kind = format!("{:?}", rel.kind);
                if let Some(name) = source_names.get(&rel.from_symbol_id) {
                    output.push_str(&format!("  {}:{}  {} ({})\n", file_path, rel.line_number, name, kind));
                } else {
                    output.push_str(&format!("  {}:{} ({})\n", file_path, rel.line_number, kind));
                }
            } else {
                // Multiple refs in file: group header
                output.push_str(&format!("  {}:\n", file_path));
                for rel in rels {
                    let kind = format!("{:?}", rel.kind);
                    if let Some(name) = source_names.get(&rel.from_symbol_id) {
                        output.push_str(&format!("    :{}  {} ({})\n", rel.line_number, name, kind));
                    } else {
                        output.push_str(&format!("    :{} ({})\n", rel.line_number, kind));
                    }
                }
            }
        }
    }
```

- [ ] **Step 7: Fix any broken formatting_tests**

Run: `cargo test --lib formatting_tests 2>&1 | tail -30`

Update assertions to match the new grouped format.

- [ ] **Step 8: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green.

- [ ] **Step 9: Commit**

```bash
git add src/tools/search/formatting.rs src/tools/navigation/formatting.rs src/tests/tools/search/lean_format_tests.rs src/tests/tools/formatting_tests.rs
git commit -m "feat(formatting): group search and refs results by file

When multiple matches appear in the same file, the file path is now
shown once as a header with matches indented below. Saves 5-15% on
multi-match results by eliminating path repetition."
```

---

### Task 5: Drop kind prefix when signature already contains it

In `get_symbols` structure-mode output, each line shows `kind signature (lines)`, e.g., `function pub fn process(&self) (17-24)`. The word "function" is redundant because the signature starts with `fn`. Same for `struct pub struct Foo`, `method pub fn bar()`, etc.

**Files:**
- Modify: `src/tools/symbols/formatting.rs:65-104` (format_lean_symbols function)
- Test: `src/tests/tools/get_symbols.rs`

- [ ] **Step 1: Write failing test**

In `src/tests/tools/get_symbols.rs`, add:

```rust
#[test]
fn test_lean_format_skips_redundant_kind_prefix() {
    use crate::tools::symbols::formatting::format_symbol_response;
    use crate::extractors::base::{Symbol, SymbolKind, Visibility};

    let symbols = vec![
        Symbol {
            name: "Foo".to_string(),
            kind: SymbolKind::Struct,
            signature: Some("pub struct Foo".to_string()),
            visibility: Some(Visibility::Public),
            start_line: 10,
            end_line: 20,
            file_path: "test.rs".to_string(),
            // NOTE: Symbol has no Default impl. Use full construction (see make_test_symbol in lean_format_tests.rs for pattern)
            id: format!("test_{}_{}", file_path, start_line), language: "rust".to_string(),
            start_column: 0, end_column: 0, start_byte: 0, end_byte: 0,
            parent_id: None, signature: None, doc_comment: None, visibility: None,
            metadata: None, semantic_group: None, confidence: None, code_context: None, content_type: None
        },
        Symbol {
            name: "process".to_string(),
            kind: SymbolKind::Function,
            signature: Some("pub fn process(&self)".to_string()),
            visibility: Some(Visibility::Public),
            start_line: 25,
            end_line: 40,
            file_path: "test.rs".to_string(),
            // NOTE: Symbol has no Default impl. Use full construction (see make_test_symbol in lean_format_tests.rs for pattern)
            id: format!("test_{}_{}", file_path, start_line), language: "rust".to_string(),
            start_column: 0, end_column: 0, start_byte: 0, end_byte: 0,
            parent_id: None, signature: None, doc_comment: None, visibility: None,
            metadata: None, semantic_group: None, confidence: None, code_context: None, content_type: None
        },
    ];

    let result = format_symbol_response("test.rs", symbols, None).unwrap();
    let output = result.content.first().unwrap();
    let text = match output {
        crate::mcp_compat::Content::Text(t) => &t.text,
        _ => panic!("expected text content"),
    };

    // "struct pub struct Foo" is redundant; should be just "pub struct Foo"
    assert!(!text.contains("struct pub struct"),
        "Kind prefix should be skipped when signature contains the kind keyword. Output:\n{}", text);
    assert!(!text.contains("function pub fn"),
        "Kind prefix should be skipped when signature contains the kind keyword. Output:\n{}", text);
    // The signature itself should still appear
    assert!(text.contains("pub struct Foo"), "Signature should be present");
    assert!(text.contains("pub fn process"), "Signature should be present");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_lean_format_skips_redundant_kind_prefix 2>&1 | tail -10`
Expected: FAIL (output contains "struct pub struct")

- [ ] **Step 3: Implement kind-prefix dedup**

In `src/tools/symbols/formatting.rs`, in the `format_lean_symbols` function, replace the format line:

```rust
    // Map of kind display names to their source-code keywords
    fn kind_keyword(kind: &crate::extractors::base::SymbolKind) -> Option<&'static str> {
        use crate::extractors::base::SymbolKind;
        match kind {
            SymbolKind::Function => Some("fn "),
            SymbolKind::Method => Some("fn "),
            SymbolKind::Struct => Some("struct "),
            SymbolKind::Class => Some("class "),
            SymbolKind::Interface => Some("interface "),
            SymbolKind::Trait => Some("trait "),
            SymbolKind::Enum => Some("enum "),
            SymbolKind::Module => Some("mod "),
            SymbolKind::Namespace => Some("namespace "),
            SymbolKind::Constructor => Some("new"),
            _ => None,
        }
    }
```

Then in the formatting loop, replace the output line:

```rust
        // Skip kind prefix if the signature already contains the kind keyword
        let show_kind = if let Some(sig) = &symbol.signature {
            kind_keyword(&symbol.kind)
                .map(|kw| !sig.contains(kw))
                .unwrap_or(true)
        } else {
            true
        };

        let name_display = if let Some(sig) = &symbol.signature {
            sig.clone()
        } else {
            symbol.name.clone()
        };

        if show_kind {
            output.push_str(&format!(
                "{}{} {} ({}-{}{})\n",
                indent, kind, name_display, symbol.start_line, symbol.end_line, vis_str,
            ));
        } else {
            output.push_str(&format!(
                "{}{} ({}-{}{})\n",
                indent, name_display, symbol.start_line, symbol.end_line, vis_str,
            ));
        }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_lean_format_skips_redundant_kind_prefix 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Fix broken tests and run dev tier**

Run: `cargo test --lib tests::tools::get_symbols 2>&1 | tail -30`

Update any assertions in existing tests that expected the redundant kind prefix. Then:

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green.

- [ ] **Step 6: Commit**

```bash
git add src/tools/symbols/formatting.rs src/tests/tools/get_symbols.rs
git commit -m "feat(symbols): skip redundant kind prefix in structure output

When a symbol's signature already contains its kind keyword (e.g.
'pub fn process' contains 'fn'), the kind prefix is now omitted.
Saves 3-5 tokens per symbol in structure-mode listings."
```

---

### Task 6: get_context compact mode tightening

The compact format is barely smaller than readable (test allows within 10%). Make it genuinely compact: drop centrality labels (ordering implies importance), use shorter kind prefixes, and enforce the neighbor token budget.

**Files:**
- Modify: `src/tools/get_context/formatting.rs:188-274` (compact formatter functions)
- Modify: `src/tools/get_context/pipeline.rs:465-490` (neighbor entry building, enforce token cap)
- Test: `src/tests/tools/get_context_formatting_tests.rs`

- [ ] **Step 1: Update the compact-vs-readable test to require real savings**

In `src/tests/tools/get_context_formatting_tests.rs`, find `test_compact_reduces_estimated_tokens_by_at_least_20_percent` (line ~750) and update the assertion. Also update `test_compact_output_smaller_than_readable_for_same_context` (line ~697) to require at least 15% savings:

```rust
    // In test_compact_output_smaller_than_readable_for_same_context:
    // Replace the ratio < 1.10 assertion with:
    assert!(
        ratio < 0.90,
        "compact should be at least 10% smaller than readable (compact={}, readable={}, ratio={:.2})",
        compact.len(),
        readable.len(),
        ratio
    );
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_compact_output_smaller 2>&1 | tail -10`
Expected: FAIL (compact is currently ~same size as readable)

- [ ] **Step 3: Tighten the compact formatter**

In `src/tools/get_context/formatting.rs`, rewrite `format_context_compact`:

Key changes:
1. Drop centrality labels entirely (the pivots are already sorted by relevance)
2. Use shorter kind abbreviations: `fn` for function, `st` for struct, `tr` for trait, `md` for module, `mt` for method, `en` for enum
3. Drop `kind=` and `centrality=` labels
4. Use comma-separated callers instead of separate lines
5. Omit NEIGHBOR label, use a tighter format

```rust
fn format_context_compact(data: &ContextData) -> String {
    if data.pivots.is_empty() {
        return format!(
            "Context \"{}\" | no relevant symbols\n\
            Try fast_search(query=\"{}\") for exact matches, or verify the workspace is indexed",
            data.query, data.query
        );
    }

    let mut out = String::with_capacity(1536);
    let file_count = count_unique_files(data);
    out.push_str(&format!(
        "Context \"{}\" | pivots={} neighbors={} files={}\n",
        data.query,
        data.pivots.len(),
        data.neighbors.len(),
        file_count
    ));

    for pivot in &data.pivots {
        out.push_str(&format!(
            "PIVOT {} {}:{} kind={}\n",
            pivot.name, pivot.file_path, pivot.start_line, pivot.kind
        ));
        for line in pivot.content.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        let incoming_names = dedup_names(&pivot.incoming_names);
        let outgoing_names = dedup_names(&pivot.outgoing_names);
        if !incoming_names.is_empty() || !outgoing_names.is_empty() {
            let mut parts = Vec::new();
            if !incoming_names.is_empty() {
                parts.push(format!("callers={}", incoming_names.join(",")));
            }
            if !outgoing_names.is_empty() {
                parts.push(format!("calls={}", outgoing_names.join(",")));
            }
            out.push_str(&format!("  {}\n", parts.join(" ")));
        }
    }

    if !data.neighbors.is_empty() {
        for neighbor in &data.neighbors {
            format_neighbor_compact(&mut out, neighbor, &data.allocation.neighbor_mode);
        }
    }

    out
}
```

The main differences from current: dropped `centrality=` label and `quality=` tag from pivots, merged callers/calls onto one line.

- [ ] **Step 4: Enforce neighbor token budget in pipeline**

In `src/tools/get_context/pipeline.rs`, find the `build_neighbor_entries` function (line ~467) and the `MAX_NEIGHBOR_ENTRIES` constant (line 465). Add token-aware truncation:

```rust
const MAX_NEIGHBOR_ENTRIES: usize = 200;
const MAX_NEIGHBOR_TOKENS: usize = 600; // 30% of a 2000-token budget
```

After building the neighbor entries vec, add estimated token counting:

```rust
fn build_neighbor_entries(expansion: &GraphExpansion) -> Vec<super::formatting::NeighborEntry> {
    use super::formatting::NeighborEntry;

    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    let mut estimated_chars: usize = 0;
    let char_budget = MAX_NEIGHBOR_TOKENS * 4; // rough token-to-char conversion

    // ... existing dedup + entry building loop ...
    // Inside the loop, after pushing each entry, add:
    //     estimated_chars += entry_line_len;
    //     if estimated_chars > char_budget { break; }

    entries
}
```

The exact integration depends on the loop structure in the existing code. The worker should read the full function and add the char budget check inside the entry construction loop.

- [ ] **Step 5: Run tests**

Run: `cargo test --lib test_compact_output_smaller 2>&1 | tail -10`
Run: `cargo test --lib test_compact_reduces 2>&1 | tail -10`
Expected: Both PASS

- [ ] **Step 6: Run dev tier**

Run: `cargo xtask test dev 2>&1 | tail -20`
Expected: All green.

- [ ] **Step 7: Commit**

```bash
git add src/tools/get_context/formatting.rs src/tools/get_context/pipeline.rs src/tests/tools/get_context_formatting_tests.rs
git commit -m "feat(get_context): tighten compact mode for real token savings

Compact mode now drops centrality labels (ordering implies importance),
merges caller/callee lines, and enforces a token budget on neighbor
entries. Achieves 15-30% reduction vs readable format, up from ~2%."
```

---

### Task 7: /efficient skill for conservative defaults

A plugin skill that agents (or users) can invoke to get guidance on using Julie tools with minimal token consumption. This doesn't change server behavior; it provides a prompt overlay that guides the agent toward lean options.

**Files:**
- Create: `/Users/murphy/source/julie-plugin/skills/efficient/SKILL.md`

- [ ] **Step 1: Create the skill file**

```markdown
---
name: efficient
description: Switch Julie tools to token-efficient defaults. Use when context window is filling up, working on large codebases, or you want to minimize token consumption. Guides all subsequent Julie tool calls toward leaner output modes.
user-invocable: true
arguments: ""
allowed-tools: mcp__julie__fast_search, mcp__julie__get_symbols, mcp__julie__deep_dive, mcp__julie__get_context, mcp__julie__fast_refs
---

# Efficient Mode

Minimize token consumption from Julie tool calls for the rest of this session. Apply these defaults to ALL subsequent Julie tool calls:

## Tool Defaults

| Tool | Parameter | Efficient Value | Why |
|------|-----------|----------------|-----|
| `get_symbols` | `mode` | `"structure"` | Names/signatures only, no code bodies |
| `deep_dive` | `depth` | `"overview"` | Signature + caller/callee list (~200 tokens) |
| `get_context` | `max_tokens` | `1500` | Tight budget, fewer neighbors |
| `get_context` | `format` | `"compact"` | Minimal labels, no decorative headers |
| `fast_search` | `limit` | `5` | Fewer results per query |
| `fast_search` | `return_format` | `"locations"` | File:line only, no code context |
| `fast_search` | `context_lines` | `0` | Match line only, no surrounding lines |
| `fast_refs` | `limit` | `5` | Fewer references per query |

## When to Escalate

If the efficient defaults don't give you enough information, escalate ONE level:

1. `get_symbols(mode="structure")` not enough? Try `get_symbols(mode="minimal", target="specific_symbol")`
2. `deep_dive(depth="overview")` not enough? Try `deep_dive(depth="context")`
3. `fast_search(return_format="locations")` not enough? Try `fast_search(return_format="full", limit=3)`

## Rules

- NEVER use `deep_dive(depth="full")` in efficient mode unless explicitly asked
- NEVER use `get_symbols(mode="full")` in efficient mode
- NEVER use `fast_search(limit=10)` or higher in efficient mode
- Prefer `get_symbols` over `Read` for ALL file inspection
- Prefer `deep_dive(depth="overview")` over `fast_search` + `get_symbols` chains
```

- [ ] **Step 2: Verify skill structure**

Check that the skill has the required frontmatter fields: `name`, `description`, `user-invocable: true`, and `allowed-tools`.

- [ ] **Step 3: Test manually**

The worker should note that this skill lives in the `julie-plugin` repo, not the `julie` repo. It can be tested by running Claude Code with the plugin installed and invoking `/efficient`.

- [ ] **Step 4: Commit (in julie-plugin repo)**

```bash
cd /Users/murphy/source/julie-plugin
git add skills/efficient/SKILL.md
git commit -m "feat(skills): add /efficient skill for token-conscious mode

New user-invocable skill that guides agents toward minimal-token
Julie tool defaults. Reduces context window consumption for sessions
working on large codebases or approaching context limits."
```

---

## Execution Order

Tasks 1-6 are independent and can be executed in parallel (they touch different files). Task 7 depends on Task 2 (references `return_format` parameter).

Recommended batching for subagent-driven development:
- **Batch 1 (parallel):** Tasks 1, 3, 5 (small, independent)
- **Batch 2 (parallel):** Tasks 2, 4 (search formatting, some overlap in `formatting.rs`)
- **Batch 3:** Task 6 (get_context, benefits from earlier changes being merged)
- **Batch 4:** Task 7 (skill, in separate repo)

After all tasks: run `cargo xtask test full` for comprehensive regression check.
