# Phase 3: `get_context` Tool — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a new MCP tool that returns a token-budgeted subgraph of relevant code for a given query — search → rank by centrality → expand graph → adaptive token allocation → formatted output.

**Architecture:** New tool module `src/tools/get_context/` with pipeline stages. Composes existing primitives: `search_symbols` (with OR fallback from Phase 1), `reference_score` (from Phase 2), relationship queries, and `TokenEstimator`. Output formatted as pivots + neighbors + file map.

**Tech Stack:** Existing Tantivy search, rusqlite relationships/symbols, TokenEstimator, rmcp tool macros

**Depends on:** Phase 1 (OR fallback) and Phase 2 (centrality scoring) must be complete.

---

### Task 1: Tool Scaffolding and Registration

**Files:**
- Create: `src/tools/get_context/mod.rs`
- Modify: `src/tools/mod.rs` (add pub mod get_context)
- Modify: `src/handler.rs:341-468` (register the new tool with #[tool(...)] attribute)

**Step 1: Create the module structure**

Create `src/tools/get_context/mod.rs` with the tool struct and parameter parsing:

```rust
//! get_context tool — returns token-budgeted context subgraph for a query.
//!
//! Pipeline: search → rank by centrality → expand graph → adaptive allocation → format

mod pipeline;
mod allocation;
mod formatting;

use anyhow::Result;
use serde::Deserialize;

use crate::handler::JulieServerHandler;

#[derive(Debug, Deserialize)]
pub struct GetContextTool {
    /// Search query — concept, task description, or symbol pattern
    pub query: String,
    /// Maximum tokens to return (optional, adaptive default)
    pub max_tokens: Option<u32>,
    /// Workspace filter
    pub workspace: Option<String>,
    /// Language filter
    pub language: Option<String>,
    /// File pattern filter (glob)
    pub file_pattern: Option<String>,
}

impl GetContextTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<String> {
        pipeline::run(self, handler).await
    }
}
```

Create stub files for the submodules:

`src/tools/get_context/pipeline.rs`:
```rust
//! Main pipeline: search → rank → expand → allocate → format
use anyhow::Result;
use super::GetContextTool;
use crate::handler::JulieServerHandler;

pub async fn run(tool: &GetContextTool, handler: &JulieServerHandler) -> Result<String> {
    todo!("Implement pipeline")
}
```

`src/tools/get_context/allocation.rs`:
```rust
//! Adaptive token budget allocation
```

`src/tools/get_context/formatting.rs`:
```rust
//! Output formatting for get_context results
```

**Step 2: Register in handler.rs**

Add the tool to the `#[tool_router]` block in `src/handler.rs`, following the pattern of existing tools:

```rust
#[tool(
    name = "get_context",
    description = "Get token-budgeted context for a concept or task. Returns relevant code subgraph with full code for pivots and signatures for neighbors. Use at the start of a task for orientation.",
    read_only_hint = true,
    destructive_hint = false
)]
async fn get_context(&self, #[tool(aggr)] tool: GetContextTool) -> Result<CallToolResult, McpError> {
    match tool.call_tool(self).await {
        Ok(result) => Ok(CallToolResult::success(vec![Content::text(result)])),
        Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Error: {}", e))])),
    }
}
```

**Step 3: Add mod declaration**

In `src/tools/mod.rs`, add: `pub mod get_context;`

**Step 4: Verify it compiles**

Run: `cargo build 2>&1 | tail -10`
Expected: Compiles (pipeline::run is todo!() but that's fine — it only panics at runtime)

**Step 5: Commit**

```bash
git add src/tools/get_context/ src/tools/mod.rs src/handler.rs
git commit -m "feat: scaffold get_context tool with MCP registration"
```

---

### Task 2: Search + Pivot Selection

**Files:**
- Modify: `src/tools/get_context/pipeline.rs`
- Test: `src/tests/tools/get_context_tests.rs` (new file)

**Step 1: Write the failing test**

Create `src/tests/tools/get_context_tests.rs`:

```rust
//! Tests for the get_context tool pipeline.

use tempfile::TempDir;
use crate::database::SymbolDatabase;
use crate::search::index::{SearchIndex, SymbolDocument};

#[test]
fn test_pivot_selection_prefers_high_centrality() {
    let temp_dir = TempDir::new().unwrap();
    let db = SymbolDatabase::new(temp_dir.path().join("test.db")).unwrap();
    let index = SearchIndex::create(temp_dir.path().join("tantivy")).unwrap();

    // Setup: two symbols both matching "process", different reference_scores
    // ... insert files, symbols with reference_score, add to tantivy ...

    // Symbol with ref_score=50 should be selected as pivot over ref_score=1
    let pivots = select_pivots(&search_results, &reference_scores);
    assert_eq!(pivots[0].name, "process_payment",
        "Highest centrality symbol should be first pivot");
}

#[test]
fn test_pivot_count_adapts_to_score_distribution() {
    // When top result is 2x above second → 1 pivot
    // When top 3 are within 30% of each other → 3 pivots
    // ... test the adaptive pivot count logic ...
}
```

Register in `src/tests/mod.rs`.

**Step 2: Run test to verify it fails**

Run: `cargo test test_pivot_selection 2>&1 | tail -20`
Expected: FAIL — functions don't exist

**Step 3: Implement search + pivot selection in pipeline.rs**

```rust
use crate::search::index::{SearchFilter, SymbolSearchResult};
use crate::search::scoring::apply_centrality_boost;
use std::collections::HashMap;

/// Select pivot symbols from search results based on centrality-weighted scores.
pub fn select_pivots(
    results: &[SymbolSearchResult],
    reference_scores: &HashMap<String, f64>,
) -> Vec<&SymbolSearchResult> {
    if results.is_empty() {
        return Vec::new();
    }

    // Apply centrality to determine combined scores
    let mut scored: Vec<(f32, &SymbolSearchResult)> = results
        .iter()
        .map(|r| {
            let ref_score = reference_scores.get(&r.id).copied().unwrap_or(0.0);
            let boost = 1.0 + (1.0 + ref_score as f32).ln() * 0.3;
            (r.score * boost, r)
        })
        .collect();
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Determine pivot count from score distribution
    let top_score = scored[0].0;
    let pivot_count = if scored.len() == 1 {
        1
    } else if top_score > scored[1].0 * 2.0 {
        1  // Clear winner
    } else if scored.len() >= 3
        && scored[2].0 >= top_score * 0.7
    {
        3  // Cluster of similar scores
    } else {
        2  // Default
    };

    scored.into_iter().take(pivot_count).map(|(_, r)| r).collect()
}
```

**Step 4: Run tests**

Run: `cargo test test_pivot_selection 2>&1 | tail -20`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tools/get_context/pipeline.rs src/tests/tools/get_context_tests.rs src/tests/mod.rs
git commit -m "feat: implement pivot selection with centrality-weighted scoring"
```

---

### Task 3: Graph Expansion

**Files:**
- Modify: `src/tools/get_context/pipeline.rs` (add expand_graph function)
- Test: `src/tests/tools/get_context_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_graph_expansion_finds_callers_and_callees() {
    // Given a pivot symbol with relationships, expand_graph should return
    // incoming (callers) and outgoing (callees) neighbors.
    // ... setup db with symbols and relationships ...

    let neighbors = expand_graph(&pivot_ids, &db).unwrap();
    assert!(neighbors.incoming.iter().any(|n| n.name == "caller_fn"));
    assert!(neighbors.outgoing.iter().any(|n| n.name == "callee_fn"));
}

#[test]
fn test_graph_expansion_deduplicates_across_pivots() {
    // If two pivots both call the same function, it should appear once in neighbors
    // ... setup two pivots both calling shared_fn ...

    let neighbors = expand_graph(&pivot_ids, &db).unwrap();
    let shared_count = neighbors.outgoing.iter()
        .filter(|n| n.name == "shared_fn")
        .count();
    assert_eq!(shared_count, 1, "Shared neighbor should appear once");
}
```

**Step 2: Run test, verify fail**

**Step 3: Implement expand_graph**

Uses existing `db.get_outgoing_relationships()` and `db.get_relationships_to_symbols()` from `src/database/relationships.rs`. Deduplicate by symbol ID across pivots. Sort neighbors by their own reference_score.

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git commit -m "feat: implement graph expansion with deduplication"
```

---

### Task 4: Adaptive Token Allocation

**Files:**
- Modify: `src/tools/get_context/allocation.rs`
- Test: `src/tests/tools/get_context_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_adaptive_allocation_single_pivot_goes_deep() {
    // With 1 pivot, allocation should give it 60% of budget as full code body
    let budget = TokenBudget::new(2000);
    let allocation = budget.allocate(1, 5);  // 1 pivot, 5 neighbors
    assert!(allocation.pivot_tokens > 1000, "Single pivot should get majority of budget");
}

#[test]
fn test_adaptive_allocation_many_pivots_goes_broad() {
    // With 7+ pivots, allocation should use signatures only
    let budget = TokenBudget::new(4000);
    let allocation = budget.allocate(8, 20);  // 8 pivots, 20 neighbors
    assert!(allocation.pivot_tokens < 2000, "Many pivots should share budget thinly");
}

#[test]
fn test_allocation_respects_max_tokens() {
    let budget = TokenBudget::new(1000);
    let allocation = budget.allocate(3, 10);
    assert!(
        allocation.pivot_tokens + allocation.neighbor_tokens + allocation.summary_tokens <= 1000,
        "Total allocation must not exceed budget"
    );
}
```

**Step 2: Run test, verify fail**

**Step 3: Implement TokenBudget**

```rust
use crate::utils::token_estimation::TokenEstimator;

pub struct TokenBudget {
    pub max_tokens: u32,
}

pub struct Allocation {
    pub pivot_tokens: u32,
    pub neighbor_tokens: u32,
    pub summary_tokens: u32,
    pub pivot_mode: PivotMode,
    pub neighbor_mode: NeighborMode,
}

pub enum PivotMode {
    FullBody,           // 1-3 pivots: full code
    SignatureAndKey,    // 4-6 pivots: signature + first/last 5 lines
    SignatureOnly,      // 7+: signature only
}

pub enum NeighborMode {
    SignatureAndDoc,    // 1-3 pivots: signature + doc comment + 1-line context
    SignatureOnly,      // 4-6 pivots: signature only
    NameAndLocation,   // 7+: just name + file:line
}

impl TokenBudget {
    pub fn new(max_tokens: u32) -> Self { Self { max_tokens } }

    /// Adaptive default budget based on result count
    pub fn adaptive(pivot_count: usize) -> Self {
        let budget = match pivot_count {
            0..=2 => 2000,
            3..=5 => 3000,
            _ => 4000,
        };
        Self { max_tokens: budget }
    }

    pub fn allocate(&self, pivot_count: usize, neighbor_count: usize) -> Allocation {
        let (pivot_mode, neighbor_mode) = match pivot_count {
            0..=3 => (PivotMode::FullBody, NeighborMode::SignatureAndDoc),
            4..=6 => (PivotMode::SignatureAndKey, NeighborMode::SignatureOnly),
            _ => (PivotMode::SignatureOnly, NeighborMode::NameAndLocation),
        };

        let total = self.max_tokens;
        Allocation {
            pivot_tokens: (total as f32 * 0.6) as u32,
            neighbor_tokens: (total as f32 * 0.3) as u32,
            summary_tokens: (total as f32 * 0.1) as u32,
            pivot_mode,
            neighbor_mode,
        }
    }
}
```

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git commit -m "feat: implement adaptive token budget allocation"
```

---

### Task 5: Output Formatting

**Files:**
- Modify: `src/tools/get_context/formatting.rs`
- Test: `src/tests/tools/get_context_tests.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_format_output_contains_sections() {
    // Output should contain: header, pivot sections, neighbors section, files section
    let output = format_context(&context_data);
    assert!(output.contains("Pivot:"), "Should have pivot sections");
    assert!(output.contains("Neighbors"), "Should have neighbors section");
    assert!(output.contains("Files"), "Should have files section");
}

#[test]
fn test_format_output_shows_centrality_hint() {
    // Pivots should show centrality level (high/medium/low)
    let output = format_context(&context_data);
    assert!(
        output.contains("ref_score:") || output.contains("Centrality:"),
        "Should show centrality information"
    );
}
```

**Step 2: Implement formatter**

Follow the output format from the design doc:
```
═══ Context: "payment processing" ═══
Found 2 pivot symbols, 8 neighbors across 4 files

── Pivot: process_payment ──────────────────────
src/payment/processor.rs:42 (function, public)
  Centrality: high (ref_score: 47)
  [code body or signature based on allocation mode]
  Callers (5): ...
  Calls: ...

── Neighbors ───────────────────────────────────
  [signatures or names based on allocation mode]

── Files ───────────────────────────────────────
  [file map]
```

**Step 3: Run tests, verify pass**

**Step 4: Commit**

```bash
git commit -m "feat: implement get_context output formatting"
```

---

### Task 6: Wire Up the Full Pipeline

**Files:**
- Modify: `src/tools/get_context/pipeline.rs` (replace todo!() with full pipeline)
- Test: `src/tests/tools/get_context_tests.rs` (integration test)

**Step 1: Write the integration test**

```rust
#[test]
fn test_full_pipeline_returns_context() {
    // End-to-end: create a workspace with symbols and relationships,
    // run the pipeline, verify output contains expected sections.
    // ... setup temp workspace, db, tantivy index ...
    // ... insert symbols with relationships ...
    // ... run pipeline with query "process" ...
    // ... assert output contains pivot, neighbors, files ...
}
```

**Step 2: Wire up pipeline::run**

```rust
pub async fn run(tool: &GetContextTool, handler: &JulieServerHandler) -> Result<String> {
    let workspace = handler.resolve_workspace(&tool.workspace)?;
    let db = workspace.database();
    let search_index = workspace.search_index();

    // Step 1: Search
    let filter = SearchFilter {
        language: tool.language.clone(),
        kind: None,
        file_pattern: tool.file_pattern.clone(),
    };
    let search_results = search_index.search_symbols(&tool.query, &filter, 30)?;

    if search_results.is_empty() {
        return Ok(format!("No results found for: {}", tool.query));
    }

    // Step 2: Get reference scores and select pivots
    let ids: Vec<&str> = search_results.iter().map(|r| r.id.as_str()).collect();
    let ref_scores = db.get_reference_scores(&ids)?;
    let pivots = select_pivots(&search_results, &ref_scores);

    // Step 3: Expand graph
    let pivot_ids: Vec<&str> = pivots.iter().map(|p| p.id.as_str()).collect();
    let neighbors = expand_graph(&pivot_ids, &db)?;

    // Step 4: Allocate token budget
    let budget = match tool.max_tokens {
        Some(max) => TokenBudget::new(max),
        None => TokenBudget::adaptive(pivots.len()),
    };
    let allocation = budget.allocate(pivots.len(), neighbors.total_count());

    // Step 5: Format output
    let output = format_context(&tool.query, &pivots, &neighbors, &allocation, &db)?;
    Ok(output)
}
```

**Step 3: Run integration test**

Run: `cargo test test_full_pipeline_returns_context 2>&1 | tail -20`
Expected: PASS

**Step 4: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All pass

**Step 5: Commit**

```bash
git add src/tools/get_context/
git commit -m "feat: wire up complete get_context pipeline"
```

---

### Task 7: Update Agent Instructions

**Files:**
- Modify: Agent instructions text (JULIE_AGENT_INSTRUCTIONS or equivalent constant in handler.rs)

**Step 1: Add get_context to tool descriptions**

Add a section describing `get_context` in the agent instructions:

```
### get_context — Contextual Code Understanding
**Always use BEFORE:** Starting a new task, investigating an unfamiliar area, or needing orientation.

Combines search + graph traversal + token budgeting in one call:
```javascript
get_context(query="payment processing")
// → Pivots (full code), neighbors (signatures), file map — all token-budgeted
```

**When to use which tool:**
- `get_context` → "I need to understand this area" (start of task)
- `deep_dive` → "Tell me about this specific symbol" (during task)
- `fast_search` → "Where is X defined?" (quick lookup)
- `fast_refs` → "Who uses X?" (before modifying)
```

**Step 2: Commit**

```bash
git commit -m "docs: add get_context to agent instructions"
```

---

### Task 8: Live Testing and Tuning

**Step 1: Build debug binary**

Run: `cargo build 2>&1 | tail -5`

**Step 2: Restart and test with real queries**

Test queries against Julie's own codebase:

1. `get_context(query="search ranking")` — should surface scoring.rs, index.rs, query.rs
2. `get_context(query="file watcher")` — should surface watcher module
3. `get_context(query="workspace management")` — should surface workspace tools
4. `get_context(query="symbol extraction")` — should surface extractors

For each, verify:
- Pivots are the most relevant/central symbols
- Neighbors provide useful surrounding context
- Token count is reasonable (not too much, not too little)
- File map accurately reflects the code area

**Step 3: Compare with manual tool chain**

For one query, compare:
- `get_context(query="search ranking")` output
- vs. manually calling: `fast_search("search ranking")` → `deep_dive("apply_important_patterns_boost")` → `fast_refs("search_symbols")`

Is `get_context` returning equivalent or better context in one call?

**Step 4: Tune and commit**

Adjust CENTRALITY_WEIGHT, budget defaults, pivot count thresholds based on results.

```bash
git commit -m "tune: adjust get_context parameters from live testing"
```
