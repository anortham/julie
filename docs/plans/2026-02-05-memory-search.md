# Memory Search Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add fuzzy text search to the `recall` tool using an ephemeral in-memory Tantivy index, so agents can query memories by content (e.g., "what did I learn about Tantivy scoring?").

**Architecture:** When `recall` receives a `query` parameter, load all memory files from disk (already done), build a throwaway Tantivy `RamDirectory` index with a simple 2-field schema (id + body), index each memory's description, run the query, and return results ranked by BM25 score. No persistent index — keeps the code search index clean and avoids sync/maintenance overhead.

**Tech Stack:** Tantivy 0.22 (already a dependency), `Index::create_in_ram()`, `CodeTokenizer` (reuse from `src/search/tokenizer.rs`)

---

## Context for Implementer

### Key files
- `src/tools/memory/mod.rs` — `recall_memories()` loads all memories from disk, `RecallOptions` struct, `Memory` struct, `read_memory_file()`
- `src/tools/memory/recall.rs` — `RecallTool` MCP tool struct (has `limit`, `since`, `until`, `type` params), `call_tool()` orchestrates recall and formats output
- `src/search/index.rs` — existing `SearchIndex` (for reference, not reuse — too coupled to code schema)
- `src/search/tokenizer.rs` — `CodeTokenizer` (reusable for memory search)
- `src/search/schema.rs` — existing code schema (for reference on how to build schemas)
- `src/tests/memory_recall_tests.rs` — existing recall tests

### How Tantivy in-memory works
```rust
use tantivy::{Index, schema::*, doc, collector::TopDocs, query::QueryParser};

let mut builder = Schema::builder();
let body = builder.add_text_field("body", TEXT | STORED);
let id = builder.add_text_field("id", STRING | STORED);
let schema = builder.build();

let index = Index::create_in_ram(schema);
// Register custom tokenizer if needed:
// index.tokenizers().register("code", TextAnalyzer::builder(CodeTokenizer::new()).build());
let mut writer = index.writer(15_000_000)?; // 15MB heap for tiny index
writer.add_document(doc!(id => "abc", body => "some text"))?;
writer.commit()?;

let reader = index.reader()?;
let searcher = reader.searcher();
let query_parser = QueryParser::for_index(&index, vec![body]);
let query = query_parser.parse_query("search terms")?;
let top_docs = searcher.search(&query, &TopDocs::with_limit(10))?;
```

### Current RecallTool params
```rust
pub struct RecallTool {
    pub limit: Option<u32>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub memory_type: Option<String>,
    // NEW: pub query: Option<String>,  // <-- we're adding this
}
```

### Behavior when query is provided
1. All existing filters (type, since, until) still apply first
2. After filtering, build in-memory Tantivy index from remaining memories
3. Run query against the index
4. Return results ordered by relevance score (not chronological)
5. `limit` caps the number of results (default 10)

### Behavior when query is NOT provided
- Unchanged — return memories in reverse chronological order as today

---

## Task 1: Add `search_memories()` function to mod.rs

**Files:**
- Modify: `src/tools/memory/mod.rs`
- Test: `src/tests/memory_recall_tests.rs`

**Step 1: Write the failing test**

Add to `src/tests/memory_recall_tests.rs`:

```rust
#[test]
fn test_search_memories_by_query() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    // Create memories with distinct descriptions
    let m1 = crate::tools::memory::Memory::new(
        "mem_tantivy".to_string(), 1000, "checkpoint".to_string(),
    ).with_extra(serde_json::json!({
        "description": "Fixed Tantivy tokenizer bug with hyphenated identifiers",
        "tags": ["tantivy", "bugfix"]
    }));

    let m2 = crate::tools::memory::Memory::new(
        "mem_sql".to_string(), 2000, "checkpoint".to_string(),
    ).with_extra(serde_json::json!({
        "description": "Added SQL migration to drop FTS5 tables",
        "tags": ["sql", "migration"]
    }));

    let m3 = crate::tools::memory::Memory::new(
        "mem_auth".to_string(), 3000, "decision".to_string(),
    ).with_extra(serde_json::json!({
        "description": "Decided to use JWT tokens for authentication",
        "tags": ["auth", "decision"]
    }));

    crate::tools::memory::save_memory(&workspace_root, &m1)?;
    crate::tools::memory::save_memory(&workspace_root, &m2)?;
    crate::tools::memory::save_memory(&workspace_root, &m3)?;

    // Search for "tantivy" — should find m1
    let results = crate::tools::memory::search_memories(
        &workspace_root,
        "tantivy",
        Default::default(),
    )?;
    assert!(!results.is_empty(), "Should find at least one result");
    assert_eq!(results[0].0.id, "mem_tantivy", "Top result should be the Tantivy memory");

    // Search for "migration" — should find m2
    let results = crate::tools::memory::search_memories(
        &workspace_root,
        "migration",
        Default::default(),
    )?;
    assert!(!results.is_empty());
    assert_eq!(results[0].0.id, "mem_sql");

    // Search for "authentication" — should find m3
    let results = crate::tools::memory::search_memories(
        &workspace_root,
        "authentication",
        Default::default(),
    )?;
    assert!(!results.is_empty());
    assert_eq!(results[0].0.id, "mem_auth");

    Ok(())
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_search_memories_by_query 2>&1 | tail -5`
Expected: FAIL — `search_memories` doesn't exist

**Step 3: Implement `search_memories()` in `src/tools/memory/mod.rs`**

Add after `recall_memories()`:

```rust
use tantivy::{Index, doc, collector::TopDocs, query::QueryParser};
use tantivy::schema::{Schema as TantivySchema, TEXT, STRING, STORED};
use tantivy::tokenizer::TextAnalyzer;
use crate::search::tokenizer::CodeTokenizer;

/// Search memories by text query using an ephemeral in-memory Tantivy index.
/// Returns Vec<(Memory, f32)> — memories with their relevance scores, highest first.
pub fn search_memories(
    workspace_root: &Path,
    query_str: &str,
    options: RecallOptions,
) -> Result<Vec<(Memory, f32)>> {
    // Load and filter memories using existing logic
    let memories = recall_memories(workspace_root, options)?;

    if memories.is_empty() || query_str.trim().is_empty() {
        // No query or no memories — return all with score 0.0
        return Ok(memories.into_iter().map(|m| (m, 0.0)).collect());
    }

    // Build ephemeral Tantivy schema — just id + body
    let mut schema_builder = TantivySchema::builder();
    let id_field = schema_builder.add_text_field("id", STRING | STORED);
    let body_field = schema_builder.add_text_field("body", TEXT | STORED);
    let schema = schema_builder.build();

    // Create in-memory index
    let index = Index::create_in_ram(schema);

    // Register CodeTokenizer for consistent code-aware matching
    index.tokenizers().register(
        "default",
        TextAnalyzer::builder(CodeTokenizer::new()).build(),
    );

    // Index all memories
    let mut writer = index.writer(15_000_000)?; // 15MB heap — plenty for ~100 memories
    for memory in &memories {
        let description = memory
            .extra
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let tags = memory
            .extra
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        // Combine description + tags + type into searchable body
        let body = format!("{} {} {}", description, tags, memory.memory_type);
        writer.add_document(doc!(
            id_field => memory.id.as_str(),
            body_field => body.as_str(),
        ))?;
    }
    writer.commit()?;

    // Search
    let reader = index.reader()?;
    let searcher = reader.searcher();
    let query_parser = QueryParser::for_index(&index, vec![body_field]);
    let query = query_parser.parse_query(query_str)
        .unwrap_or_else(|_| {
            // Fallback: treat entire query as a single term
            Box::new(tantivy::query::TermQuery::new(
                tantivy::Term::from_field_text(body_field, query_str),
                tantivy::schema::IndexRecordOption::Basic,
            ))
        });

    let top_docs = searcher.search(&query, &TopDocs::with_limit(memories.len()))?;

    // Map results back to memories by id
    let memory_map: std::collections::HashMap<&str, &Memory> =
        memories.iter().map(|m| (m.id.as_str(), m)).collect();

    let mut results = Vec::new();
    for (score, doc_address) in top_docs {
        let doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;
        if let Some(id_val) = doc.get_first(id_field) {
            if let Some(id_str) = id_val.as_str() {
                if let Some(memory) = memory_map.get(id_str) {
                    results.push(((*memory).clone(), score));
                }
            }
        }
    }

    Ok(results)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_search_memories_by_query 2>&1 | tail -5`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tools/memory/mod.rs src/tests/memory_recall_tests.rs
git commit -m "feat(memory): add search_memories() with in-memory Tantivy index"
```

---

## Task 2: Add more search tests (edge cases)

**Files:**
- Test: `src/tests/memory_recall_tests.rs`

**Step 1: Write additional tests**

```rust
#[test]
fn test_search_memories_respects_type_filter() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let checkpoint = crate::tools::memory::Memory::new(
        "mem_cp".to_string(), 1000, "checkpoint".to_string(),
    ).with_extra(serde_json::json!({"description": "Fixed authentication bug"}));

    let decision = crate::tools::memory::Memory::new(
        "mem_dec".to_string(), 2000, "decision".to_string(),
    ).with_extra(serde_json::json!({"description": "Decided on authentication approach"}));

    crate::tools::memory::save_memory(&workspace_root, &checkpoint)?;
    crate::tools::memory::save_memory(&workspace_root, &decision)?;

    // Search with type filter — should only find the decision
    let options = crate::tools::memory::RecallOptions {
        memory_type: Some("decision".to_string()),
        ..Default::default()
    };
    let results = crate::tools::memory::search_memories(&workspace_root, "authentication", options)?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.id, "mem_dec");

    Ok(())
}

#[test]
fn test_search_memories_empty_query_returns_all() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let m1 = crate::tools::memory::Memory::new(
        "mem_1".to_string(), 1000, "checkpoint".to_string(),
    ).with_extra(serde_json::json!({"description": "First memory"}));

    crate::tools::memory::save_memory(&workspace_root, &m1)?;

    // Empty query returns all memories with score 0.0
    let results = crate::tools::memory::search_memories(&workspace_root, "", Default::default())?;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, 0.0);

    Ok(())
}

#[test]
fn test_search_memories_no_results() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let m1 = crate::tools::memory::Memory::new(
        "mem_1".to_string(), 1000, "checkpoint".to_string(),
    ).with_extra(serde_json::json!({"description": "Fixed a database bug"}));

    crate::tools::memory::save_memory(&workspace_root, &m1)?;

    // Search for something not in any memory
    let results = crate::tools::memory::search_memories(
        &workspace_root, "xylophone", Default::default(),
    )?;
    assert!(results.is_empty(), "Should find nothing for unrelated query");

    Ok(())
}

#[test]
fn test_search_memories_finds_by_tags() -> Result<()> {
    let temp = TempDir::new()?;
    let workspace_root = temp.path().to_path_buf();

    let m1 = crate::tools::memory::Memory::new(
        "mem_tagged".to_string(), 1000, "checkpoint".to_string(),
    ).with_extra(serde_json::json!({
        "description": "Some work was done",
        "tags": ["performance", "optimization"]
    }));

    crate::tools::memory::save_memory(&workspace_root, &m1)?;

    // Search by tag content
    let results = crate::tools::memory::search_memories(
        &workspace_root, "performance", Default::default(),
    )?;
    assert!(!results.is_empty(), "Should find memory by tag");
    assert_eq!(results[0].0.id, "mem_tagged");

    Ok(())
}
```

**Step 2: Run all tests**

Run: `cargo test "memory" 2>&1 | tail -10`
Expected: All pass

**Step 3: Commit**

```bash
git add src/tests/memory_recall_tests.rs
git commit -m "test(memory): add edge case tests for search_memories"
```

---

## Task 3: Wire `query` param into RecallTool

**Files:**
- Modify: `src/tools/memory/recall.rs`

**Step 1: Write failing test**

This is an integration-level change. The test is verifying the plumbing works at the MCP tool level, but since `call_tool` requires a `JulieServerHandler`, we'll rely on the unit tests from Tasks 1-2 and do a manual smoke test.

**Step 2: Add `query` field to `RecallTool` struct**

In `src/tools/memory/recall.rs`, add to the struct:

```rust
pub struct RecallTool {
    /// Search query to find specific memories by content (uses fuzzy matching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Maximum results (default: 10)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    // ... rest unchanged
}
```

**Step 3: Update `call_tool()` to use `search_memories()` when query is provided**

In the `call_tool` method, after building `options`, replace the direct `recall_memories` call:

```rust
// Recall or search memories
let mut memories_with_scores: Vec<(Memory, Option<f32>)> = if let Some(ref query) = self.query {
    // Query provided — use search (returns ranked by relevance)
    let results = search_memories(&workspace_root, query, options)?;
    results.into_iter().map(|(m, s)| (m, Some(s))).collect()
} else {
    // No query — chronological recall
    let mut memories = recall_memories(&workspace_root, options)?;
    memories.reverse(); // Most recent first
    memories.into_iter().map(|m| (m, None)).collect()
};
```

Then update the formatting loop to show scores when present.

**Step 4: Update the empty-results message**

When query is provided but no results, show:
```
No memories matched query "your query".
Try broader terms or use recall without a query to browse chronologically.
```

**Step 5: Update the footer tip**

Remove the stale `fast_search` tip. Replace with:
```
💡 TIP: Use `query` parameter to search memory content (e.g., recall(query="tantivy scoring"))
```

**Step 6: Run full test suite**

Run: `cargo test "memory" 2>&1 | tail -10`
Expected: All pass

**Step 7: Commit**

```bash
git add src/tools/memory/recall.rs
git commit -m "feat(memory): wire query param into RecallTool for content search"
```

---

## Task 4: Update slash command and verify live

**Files:**
- Modify: `.claude/commands/recall.md`

**Step 1: Update recall command**

Add mention of query parameter to the recall slash command.

**Step 2: Manual smoke test**

After rebuild + restart:
- `recall(query="tantivy")` — should return Tantivy-related memories ranked by relevance
- `recall(query="markdown format")` — should return the format migration checkpoint
- `recall(limit=5)` — should still work chronologically (no query)
- `recall(query="tantivy", type="checkpoint")` — combined filters

**Step 3: Commit**

```bash
git add .claude/commands/recall.md
git commit -m "docs: update recall command with query parameter"
```

---

## Summary

| Task | Description | Estimated Complexity |
|------|-------------|---------------------|
| 1 | `search_memories()` with in-memory Tantivy | Medium — core logic |
| 2 | Edge case tests | Easy — just tests |
| 3 | Wire into RecallTool | Easy — plumbing |
| 4 | Update docs + smoke test | Easy — polish |

**No new dependencies.** Uses Tantivy 0.22 (already in Cargo.toml) and `CodeTokenizer` (already in codebase).
