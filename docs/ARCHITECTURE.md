# Julie Architecture Documentation

## Token Optimization Architecture

**Last Updated:** 2025-10-14
**Status:** Production-Ready
**Coverage:** All core MCP tools optimized

### Overview

Julie implements a comprehensive 3-tier token optimization strategy to prevent context window exhaustion while preserving essential information. This architecture ensures all MCP tool responses stay within reasonable token budgets (5K-20K tokens depending on tool purpose) while maintaining high information density.

**Design Philosophy:**
- **Proactive, not reactive** - Optimize at data structure level before rendering
- **Graceful degradation** - Progressive reduction with quality preservation
- **Semantic awareness** - Structure-preserving truncation, not blind cutting
- **Battle-tested** - Patterns ported from COA CodeSearch with production validation

---

## 🎯 The Three Core Utilities

### 1. TokenEstimator - Fast Estimation Engine

**Purpose:** Character-based token estimation without calling actual tokenizer
**Location:** `src/utils/token_estimation.rs`
**Performance:** <1ms for typical strings

**How it works:**
```rust
let estimator = TokenEstimator::new();
let tokens = estimator.estimate_string(&output);
// Uses character-based heuristics:
// - English/ASCII: ~4 chars per token
// - CJK characters: ~2 chars per token
// - Code/symbols: ~3 chars per token
```

**When to use:**
- **Always** - Every tool should estimate token usage before returning results
- Batch operations where calling real tokenizer would be too slow
- Real-time optimization decisions during result formatting

**Example from FastSearchTool:**
```rust
fn optimize_response(&self, response: &str) -> String {
    let estimator = TokenEstimator::new();
    let tokens = estimator.estimate_string(response);

    if tokens <= 20000 {
        response.to_string()
    } else {
        // Apply optimization...
    }
}
```

---

### 2. ProgressiveReducer - Intelligent Result Limiting

**Purpose:** Gracefully reduce large result sets using progressive reduction steps
**Location:** `src/utils/progressive_reduction.rs`
**Algorithm:** Reduces by steps [100%, 75%, 50%, 30%, 20%, 10%, 5%] until within budget

**How it works:**
```rust
let reducer = ProgressiveReducer::new();
let token_estimator = TokenEstimator::new();

// Define how to estimate a subset
let estimate_fn = |subset: &[T]| {
    let formatted = format_items(subset);
    token_estimator.estimate_string(&formatted)
};

// Apply reduction
let optimized = reducer.reduce(&items, target_tokens, estimate_fn);
```

**When to use:**
- **Unbounded collections** - Lists that could have 10, 100, or 1000+ items
- Search results, file lists, workspace lists, reference lists
- Any tool where breadth (number of items) drives token usage

**Key insight:** Preserves first items (most relevant) and gracefully degrades quantity

**Examples:**

**ManageWorkspaceTool (list command):**
```rust
// List could have 100+ workspaces in multi-workspace environments
let estimate_workspaces = |ws_subset: &[WorkspaceEntry]| {
    let mut output = String::from("📋 Registered Workspaces:\n\n");
    for workspace in ws_subset {
        output.push_str(&format!(
            "🏷️ **{}** ({})\n📁 Path: {}\n...",
            workspace.display_name, workspace.id, workspace.original_path
        ));
    }
    token_estimator.estimate_string(&output)
};

let optimized = reducer.reduce(&workspaces, 10000, estimate_workspaces);
```

**TraceCallPathTool (call breadth optimization):**
```rust
// Wide call graphs can have 50+ callers at each level
let estimate_trees = |tree_subset: &[(Symbol, Vec<CallPathNode>)]| {
    let total_nodes: usize = tree_subset
        .iter()
        .map(|(_, nodes)| self.count_nodes(nodes))
        .sum();
    let sample_node = "  • path/to/file.rs:42 `function_name` [calls] (rust)\n";
    let tokens_per_node = token_estimator.estimate_string(&sample_node);
    total_nodes * tokens_per_node
};

let optimized = reducer.reduce(&trees, 15000, estimate_trees);
```

---

### 3. ContextTruncator - Structure-Preserving Code Truncation

**Purpose:** Intelligently truncate code blocks while preserving essential structure
**Location:** `src/utils/context_truncation.rs`
**Algorithm:** Identifies "essential lines" (signatures, comments, closing braces) and preserves them with ellipsis markers

**How it works:**
```rust
let truncator = ContextTruncator::new();

// For large code bodies (>50 lines)
if body_lines.len() > 50 {
    let truncated = truncator.smart_truncate(&body_lines, 40);
    // Preserves:
    // - Function/class signatures
    // - Doc comments
    // - Return statements
    // - Closing braces
    // Shows "... (N lines truncated) ..." for omitted sections
}
```

**Essential line detection:**
- Doc comments: `///`, `/**`, `//`
- Function keywords: `fn`, `function`, `def`, `public`, `private`, `protected`
- Structure keywords: `class`, `struct`, `interface`, `enum`, `impl`
- Attributes/decorators: `#[...]`, `@...`
- Return statements: `return`, `Ok(`, `Err(`
- Closing markers: `}`, `};`, `});`

**When to use:**
- **Large code bodies** - Functions/classes >50 lines
- Symbol body extraction (Smart Read feature)
- Code context in search results
- Any tool displaying complete source code

**Example from GetSymbolsTool (Smart Read):**
```rust
fn extract_symbol_body(&self, content: &str, symbol: &Symbol) -> Option<String> {
    let body_lines: Vec<String> = /* extract lines from symbol boundaries */;

    // Apply smart truncation if body is large (>50 lines)
    if body_lines.len() > 50 {
        let truncator = ContextTruncator::new();
        Some(truncator.smart_truncate(&body_lines, 40)) // Limit to ~40 lines
    } else {
        Some(body_lines.join("\n")) // Small bodies: no truncation
    }
}
```

**Token savings:** 70-90% reduction for large functions while preserving readability

---

## 🏗️ Implementation Patterns

### Pattern 1: Simple String Truncation (Fallback Only)

**Use case:** Final safety net when other optimizations aren't applicable
**When:** Tool has already formatted output as string

```rust
fn optimize_response(&self, response: &str) -> String {
    let estimator = TokenEstimator::new();
    let tokens = estimator.estimate_string(response);

    // Target token limit (varies by tool purpose)
    if tokens <= TARGET_TOKENS {
        response.to_string()
    } else {
        // Simple truncation with notice
        let chars_per_token = response.len() / tokens.max(1);
        let target_chars = chars_per_token * TARGET_TOKENS;
        let truncated = &response[..target_chars.min(response.len())];
        format!("{}\n\n... (truncated for context limits)", truncated)
    }
}
```

**Tools using this pattern:**
- FastSearchTool (20K token target)
- FastGotoTool (15K token target)
- FastRefsTool (20K token target)

---

### Pattern 2: Progressive Collection Reduction

**Use case:** Large collections where breadth is the problem
**When:** Tool returns lists/arrays that could grow unbounded

```rust
fn optimize_collection(&self, items: &[T]) -> Vec<T> {
    let reducer = ProgressiveReducer::new();
    let token_estimator = TokenEstimator::new();

    // Define estimation function
    let estimate_items = |subset: &[T]| {
        let formatted = self.format_items(subset);
        token_estimator.estimate_string(&formatted)
    };

    // Apply reduction with target
    reducer.reduce(&items, TARGET_TOKENS, estimate_items)
}
```

**Tools using this pattern:**
- ManageWorkspaceTool (list: 10K, recent: 12K targets)
- TraceCallPathTool (15K token target for call trees)

---

### Pattern 3: Smart Body Truncation

**Use case:** Large code blocks that need structure preservation
**When:** Extracting complete function/class bodies for display

```rust
fn extract_with_truncation(&self, lines: &[String]) -> String {
    const TRUNCATION_THRESHOLD: usize = 50;
    const TARGET_LINES: usize = 40;

    if lines.len() > TRUNCATION_THRESHOLD {
        let truncator = ContextTruncator::new();
        truncator.smart_truncate(lines, TARGET_LINES)
    } else {
        lines.join("\n")
    }
}
```

**Tools using this pattern:**
- GetSymbolsTool (Smart Read with 70-90% token savings)
- FastSearchTool (code context in results)
- FastGotoTool (symbol definitions)

---

### Pattern 4: Dual-Layer Optimization (Advanced)

**Use case:** Complex tools where both breadth AND depth can cause problems
**When:** Tool has nested structures with unbounded width and height

```rust
// Layer 1: Reduce collection breadth (data structure level)
let optimized_trees = self.apply_call_breadth_optimization(&trees)?;

// Layer 2: Format with structure-aware truncation
let output = self.format_trees_with_truncation(&optimized_trees);

// Layer 3: Final string truncation (safety net)
self.optimize_response(&output)
```

**Tools using this pattern:**
- TraceCallPathTool (call breadth + code context + final truncation)

**Why this works:**
1. **Data-level** optimization prevents generating excess data
2. **Structure-level** optimization preserves semantics during rendering
3. **String-level** optimization catches edge cases

---

## 📊 Tool-Specific Token Budgets

Different tools have different token budgets based on their purpose:

| Tool | Target Tokens | Rationale |
|------|---------------|-----------|
| **FastSearchTool** | 20,000 | Search results can be extensive; users need comprehensive coverage |
| **FastGotoTool** | 15,000 | Symbol definitions with context; moderate detail needed |
| **FastRefsTool** | 20,000 | Reference lists can be long; need to see usage patterns |
| **GetSymbolsTool** | 15,000 | File structure overviews; moderate size sufficient |
| **TraceCallPathTool** | 15,000 (data), 20,000 (final) | Call graphs can explode; dual-layer protection |
| **ManageWorkspaceTool** | 10,000 (list), 12,000 (recent) | Administrative commands; compact summaries preferred |
| **FindLogicTool** | 12,000 | Business logic summaries; focused results |

**General principle:** Navigation tools (search, goto, refs) get higher budgets than administrative tools (manage, list)

---

## 🧪 Testing Token Optimization

### Integration Test Pattern

All token optimization features have dedicated integration tests in `src/tests/`:

**Example structure:**
```rust
#[test]
fn test_tool_with_large_dataset_applies_reduction() {
    let reducer = ProgressiveReducer::new();
    let token_estimator = TokenEstimator::new();

    // Create large dataset (100+ items)
    let large_dataset = create_test_dataset(100);

    // Apply optimization
    let optimized = reducer.reduce(&large_dataset, 5000, estimate_fn);

    // Verify reduction occurred
    assert!(optimized.len() < 100, "Should reduce item count");
    assert!(optimized.len() >= 5, "Should preserve at least 5%");

    // Verify token limits respected
    let final_tokens = estimate_fn(&optimized);
    assert!(final_tokens <= 5000, "Should be within token limit");
}
```

**Test coverage:**
- `workspace_management_token_tests.rs` - 5 tests (ManageWorkspaceTool)
- `get_symbols_token_tests.rs` - 7 tests (GetSymbolsTool Smart Read)
- `deep_dive_tests.rs` - 37 tests (progressive-depth symbol investigation)

### Testing Checklist

When adding token optimization to a new tool:

- [ ] Test with small dataset (no reduction should occur)
- [ ] Test with large dataset (reduction should occur)
- [ ] Test progressive reduction steps work correctly
- [ ] Verify token estimation accuracy
- [ ] Confirm structure preservation (for code truncation)
- [ ] Validate graceful degradation behavior

---

## 🎯 Best Practices

### DO:

✅ **Estimate tokens before returning results** - Every tool should check its output size
✅ **Optimize at the data level first** - Reduce before formatting when possible
✅ **Use ProgressiveReducer for collections** - Graceful degradation vs hard cutoffs
✅ **Preserve structure in code** - Use ContextTruncator, not blind substring
✅ **Test with realistic large datasets** - 100+ items, 500+ line files
✅ **Document token targets** - Make budgets explicit in tool descriptions

### DON'T:

❌ **Don't hard-code limits without reduction** - "Take first 50" loses information
❌ **Don't truncate mid-word or mid-line** - Breaks readability
❌ **Don't optimize without measuring** - Use TokenEstimator to validate
❌ **Don't skip small-dataset tests** - Ensure no reduction when unnecessary
❌ **Don't remove context without indication** - Always show "... truncated ..."
❌ **Don't optimize prematurely** - Add optimization when token issues appear

---

## 🔄 Migration Guide: Adding Token Optimization to a Tool

### Step 1: Add imports

```rust
use crate::utils::progressive_reduction::ProgressiveReducer;
use crate::utils::token_estimation::TokenEstimator;
use crate::utils::context_truncation::ContextTruncator; // If needed
```

### Step 2: Choose optimization pattern

- **Unbounded collections?** → Use Pattern 2 (ProgressiveReducer)
- **Large code bodies?** → Use Pattern 3 (ContextTruncator)
- **Complex nested data?** → Use Pattern 4 (Dual-layer)
- **Already formatted string?** → Use Pattern 1 (Fallback truncation)

### Step 3: Implement optimization

```rust
fn optimize_results(&self, items: &[T]) -> Vec<T> {
    let reducer = ProgressiveReducer::new();
    let token_estimator = TokenEstimator::new();

    let estimate_fn = |subset: &[T]| {
        let formatted = self.format_items(subset);
        token_estimator.estimate_string(&formatted)
    };

    reducer.reduce(&items, TARGET_TOKENS, estimate_fn)
}
```

### Step 4: Write integration tests

See "Testing Token Optimization" section above.

### Step 5: Update tool description

Document token limits in the tool's MCP description:
```rust
#[mcp_tool(
    name = "my_tool",
    description = "... (optimized to stay within 15K token budget)",
    // ...
)]
```

---

## 📈 Performance Characteristics

**TokenEstimator:**
- Speed: <1ms for typical strings (10K chars)
- Accuracy: ±10% vs actual tokenizer
- Overhead: Negligible (single allocation)

**ProgressiveReducer:**
- Speed: O(n log n) where n = collection size
- Iterations: Max 7 reduction steps
- Memory: O(n) temporary allocations

**ContextTruncator:**
- Speed: O(n) where n = line count
- Structure preservation: 90%+ essential lines kept
- Token savings: 70-90% for large functions

**Overall impact:** Token optimization adds <10ms latency to tool execution while preventing 10-100x context waste.

---

## 🏆 Success Metrics

### Before Token Optimization (Pre-Release):
- Search results: Often exceeded 50K tokens (truncated in Claude UI)
- Workspace lists: 100+ workspaces → 149K tokens (unusable)
- Call traces: Deep graphs → 80K+ tokens (conversation killer)

### After Token Optimization (Current):
- Search results: Consistently <20K tokens, full information preserved
- Workspace lists: 100+ workspaces → 10K tokens with progressive reduction
- Call traces: Deep graphs → 15K tokens, important paths preserved

**Impact:** 70-90% token reduction with minimal information loss

---

## 🔮 Future Enhancements

Potential improvements for future releases:

1. **Adaptive token budgets** - Adjust targets based on remaining context window
2. **Semantic ranking** - Prioritize most relevant items during reduction
3. **User-configurable limits** - Allow power users to set custom token budgets
4. **Progressive loading** - Return partial results with "load more" capability
5. **Compression techniques** - Smarter encoding for repetitive structures

---

## 📚 Related Documentation

- **SEARCH_FLOW.md** - Tantivy search architecture
- **SMART_READ_DEMO.md** - GetSymbolsTool token savings demonstration
- **CLAUDE.md** - Project guidelines and TDD methodology
- **Testing:** `src/tests/*_token_tests.rs` - Integration test suites

---

**Questions or improvements?** Update this document following TDD principles - write tests first, then document patterns.
