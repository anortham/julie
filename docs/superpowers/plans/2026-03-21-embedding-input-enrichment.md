# Embedding Input Enrichment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enrich the text fed to embedding models so that semantic search (`get_context`) can bridge vocabulary gaps between natural language queries and code symbols.

**Architecture:** Three changes to `src/embeddings/metadata.rs`, layered in order of increasing complexity: (1) add a new `extract_doc_excerpt` function for embedding while keeping `first_sentence` for display, (2) add callee name enrichment for functions/methods (mirroring existing child enrichment for containers), (3) raise `MAX_METADATA_CHARS` to exploit larger model token windows. The pipeline (`src/embeddings/pipeline.rs`) gains a callee map built from DB relationships and passes it to `prepare_batch_for_embedding`.

**Tech Stack:** Rust, rusqlite (relationship queries), existing test infrastructure in `src/tests/core/embedding_metadata.rs`

**Key context for implementors:**
- `src/embeddings/metadata.rs` — `format_symbol_metadata()` builds per-symbol text, `prepare_batch_for_embedding()` filters and enriches, `first_sentence()` extracts doc comments
- `src/embeddings/pipeline.rs` — `run_embedding_pipeline()` and `embed_symbols_for_file()` are the two call sites for `prepare_batch_for_embedding`. `reembed_symbols_for_file()` delegates to `embed_symbols_for_file`.
- `src/database/relationships.rs:127` — `get_outgoing_relationships_for_symbols()` batch-loads callees
- `src/tests/core/embedding_metadata.rs` — 60 existing tests, uses `make_symbol()` helper
- `MAX_METADATA_CHARS` is currently 600. Jina-code-v2 handles 8192 tokens (~32K chars). BGE-small handles 512 tokens (~2K chars).
- `CONTAINER_KINDS` (class, struct, interface, trait, enum) already get child enrichment. Functions/methods currently get nothing beyond name + signature + first doc sentence.
- **The `record_tool_call` miss:** Its doc is "Record a completed tool call. Bumps in-memory atomics synchronously, then spawns async task for source_bytes lookup + SQLite write." — `first_sentence()` returns only "Record a completed tool call." dropping all the SQLite/database signal.
- **Incremental re-embedding:** Container symbols are already force-re-embedded (pipeline.rs:204-213, but note: `Enum` is missing from that filter — fix that too). Functions with callees will need the same treatment.
- **`first_sentence` has a display caller:** `src/tools/get_context/pipeline.rs:503` calls `first_sentence` for compact output formatting. This caller needs the SHORT first-sentence behavior, NOT the expanded multi-sentence excerpt. Do NOT rename `first_sentence` — add a new function instead.
- **`embed_symbols_for_file` only has single-file symbols:** Cross-file callees won't resolve via the local symbol list. For the incremental per-file path, callee enrichment is best-effort (only same-file callees resolve). The full pipeline path uses `get_all_symbols()` so all callees resolve. This is acceptable — the full pipeline runs at workspace init and catches everything.
- **`select_budgeted_variables`** calls `format_symbol_metadata` (benefits from Task 1) but does not go through `prepare_batch_for_embedding`. Variables intentionally never get callee enrichment.
- **Test runner:** `cargo test --lib <test_name> 2>&1 | tail -10` for targeted tests. Orchestrator runs `cargo xtask test dev`.

---

### Task 1: Expand doc comment extraction for embeddings

**Files:**
- Modify: `src/embeddings/metadata.rs` (add `extract_doc_excerpt` function, update `format_symbol_metadata`)
- Test: `src/tests/core/embedding_metadata.rs`

The current `first_sentence()` takes only the first non-empty line up to `. `. This drops critical semantic context. Add a new `extract_doc_excerpt()` function that collects multiple doc lines (up to 300 chars) for embedding use. Keep `first_sentence()` unchanged for display use in `get_context/pipeline.rs:503`.

- [ ] **Step 1: Write failing test for multi-sentence doc extraction**

Add to `src/tests/core/embedding_metadata.rs` after the existing `test_first_sentence_extraction` test (line ~805):

```rust
#[test]
fn test_format_includes_multiple_doc_sentences() {
    let sym = make_symbol(
        "id_multi_doc",
        "record_tool_call",
        SymbolKind::Method,
        Some("pub(crate) fn record_tool_call(&self, tool_name: &str, duration: Duration, report: &ToolCallReport)"),
        Some("/// Record a completed tool call. Bumps in-memory atomics synchronously, then spawns async task for source_bytes lookup + SQLite write."),
    );
    let text = format_symbol_metadata(&sym);
    assert!(
        text.contains("SQLite write"),
        "Should include second sentence with database signal: {text}"
    );
    assert!(
        text.contains("Record a completed tool call"),
        "Should still include first sentence: {text}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_format_includes_multiple_doc_sentences 2>&1 | tail -10`
Expected: FAIL — "Should include second sentence with database signal"

- [ ] **Step 3: Add `extract_doc_excerpt` and update `format_symbol_metadata`**

In `src/embeddings/metadata.rs`, add the new function AFTER `first_sentence` (keeping `first_sentence` unchanged):

```rust
/// Maximum characters for the doc excerpt in embedding input.
const MAX_DOC_EXCERPT_CHARS: usize = 300;

/// Extract a multi-sentence doc comment excerpt for embedding input.
///
/// Unlike `first_sentence()` (used for compact display), this collects multiple
/// doc lines to capture richer semantic context for the embedding model.
/// Stops at `MAX_DOC_EXCERPT_CHARS` on a word boundary.
///
/// Cleans doc comment prefixes (`///`, `//!`, `/** */`, `# `, etc.) and XML tags.
pub fn extract_doc_excerpt(doc: &str) -> String {
    let cleaned: String = doc
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let stripped = trimmed
                .strip_prefix("///")
                .or_else(|| trimmed.strip_prefix("//!"))
                .or_else(|| trimmed.strip_prefix("/**"))
                .or_else(|| trimmed.strip_prefix("*/"))
                .or_else(|| trimmed.strip_prefix("* "))
                .or_else(|| trimmed.strip_prefix("*"))
                .or_else(|| trimmed.strip_prefix("# "))
                .or_else(|| trimmed.strip_prefix("## "))
                .or_else(|| trimmed.strip_prefix("### "))
                .unwrap_or(trimmed)
                .trim();

            let without_tags = strip_xml_tags(stripped);
            let content = without_tags.trim();

            if content.is_empty() {
                None
            } else {
                Some(content.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    truncate_on_word_boundary(&cleaned, MAX_DOC_EXCERPT_CHARS)
}
```

Update `format_symbol_metadata` to use the new function (line 130):
```rust
// Change:  doc_excerpt = first_sentence(doc);
// To:      doc_excerpt = extract_doc_excerpt(doc);
doc_excerpt = extract_doc_excerpt(doc);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_format_includes_multiple_doc_sentences 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Update existing `test_first_sentence_extraction` test**

The existing test at line 788 asserts the second sentence is NOT included. Update it to expect multi-sentence behavior, and rename for clarity:

```rust
#[test]
fn test_format_includes_multi_sentence_doc_excerpt() {
    let sym = make_symbol(
        "id11",
        "foo",
        SymbolKind::Function,
        None,
        Some("/// Handles authentication. Also does authorization and logging."),
    );
    let text = format_symbol_metadata(&sym);
    assert!(
        text.contains("Handles authentication."),
        "Should include first sentence: {text}"
    );
    assert!(
        text.contains("Also does authorization"),
        "Should now include subsequent sentences: {text}"
    );
}
```

- [ ] **Step 6: Run updated test**

Run: `cargo test --lib test_format_includes_multi_sentence_doc_excerpt 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/embeddings/metadata.rs src/tests/core/embedding_metadata.rs
git commit -m "feat(embeddings): add extract_doc_excerpt for multi-sentence embedding input"
```

---

### Task 2: Add callee enrichment for functions and methods

**Files:**
- Modify: `src/embeddings/metadata.rs:152-214` (`prepare_batch_for_embedding` — add callees param and enrichment)
- Modify: `src/embeddings/pipeline.rs:138` (pass callee map from DB in full pipeline)
- Modify: `src/embeddings/pipeline.rs:328` (pass callee map in `embed_symbols_for_file`)
- Test: `src/tests/core/embedding_metadata.rs`

Functions and methods currently get no enrichment beyond name + signature + doc. Container symbols get child methods/properties/variants appended. This task adds callee name enrichment: `" calls: insert_tool_call, get_total_file_sizes"`.

**Design decisions:**
- The callee map is `HashMap<String, Vec<String>>` mapping `from_symbol_id → [callee_name, ...]`
- Built at the pipeline level from `get_outgoing_relationships_for_symbols()`, then passed to `prepare_batch_for_embedding`
- Only `RelationshipKind::Calls` relationships are included (not imports, type refs, etc.)
- Callee enrichment applies to `Function` and `Method` kinds only (not containers — they already have child enrichment)
- **Cross-file callee resolution:** `build_callee_map` resolves callee names from the provided `symbols` slice. In `run_embedding_pipeline` (full pipeline), this is `get_all_symbols()` — all callees resolve. In `embed_symbols_for_file` (incremental), only same-file callees resolve. This is acceptable: the full pipeline runs at workspace init and catches everything; the incremental path is best-effort.
- **`select_budgeted_variables`** does not go through `prepare_batch_for_embedding` and intentionally does not get callee enrichment.

- [ ] **Step 1: Write failing test for callee enrichment**

Add to `src/tests/core/embedding_metadata.rs`:

```rust
#[test]
fn test_prepare_batch_enriches_function_with_callees() {
    let func = make_symbol(
        "f1",
        "record_tool_call",
        SymbolKind::Function,
        Some("pub fn record_tool_call(&self, tool_name: &str)"),
        Some("/// Record a completed tool call."),
    );
    let callee_func = make_symbol(
        "f2",
        "insert_tool_call",
        SymbolKind::Function,
        None,
        None,
    );
    let callee_func2 = make_symbol(
        "f3",
        "get_total_file_sizes",
        SymbolKind::Function,
        None,
        None,
    );

    let symbols = vec![func, callee_func, callee_func2];

    // Build callee map: f1 calls f2 and f3
    let mut callees_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
    callees_by_symbol.insert(
        "f1".to_string(),
        vec!["insert_tool_call".to_string(), "get_total_file_sizes".to_string()],
    );

    let batch = prepare_batch_for_embedding(&symbols, None, &callees_by_symbol);
    assert_eq!(batch.len(), 3);

    let (_, text) = batch.iter().find(|(id, _)| id == "f1").unwrap();
    assert!(
        text.contains("calls:"),
        "Function should have callee enrichment: {text}"
    );
    assert!(
        text.contains("insert_tool_call"),
        "Should contain callee name: {text}"
    );
    assert!(
        text.contains("get_total_file_sizes"),
        "Should contain second callee name: {text}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails (compilation error)**

Run: `cargo test --lib test_prepare_batch_enriches_function_with_callees 2>&1 | tail -10`
Expected: FAIL — compilation error because `prepare_batch_for_embedding` doesn't accept a `callees_by_symbol` parameter yet.

- [ ] **Step 3: Add callee parameter to `prepare_batch_for_embedding`**

Change the function signature in `src/embeddings/metadata.rs`:

```rust
pub fn prepare_batch_for_embedding(
    symbols: &[Symbol],
    lang_configs: Option<&LanguageConfigs>,
    callees_by_symbol: &HashMap<String, Vec<String>>,
) -> Vec<(String, String)> {
```

Add callee enrichment after the container enrichment block (after line 209), before the final `(s.id.clone(), text)`:

```rust
            // Enrich functions/methods with callee names.
            // This bridges the vocabulary gap: "record_tool_call" calling
            // "insert_tool_call" makes it findable for "database insert" queries.
            if matches!(s.kind, SymbolKind::Function | SymbolKind::Method) {
                if let Some(callees) = callees_by_symbol.get(&s.id) {
                    if !callees.is_empty() {
                        let suffix = format!(" calls: {}", callees.join(", "));
                        text.push_str(&suffix);
                        text = truncate_on_word_boundary(&text, MAX_METADATA_CHARS);
                    }
                }
            }
```

Note: No redundant `!CONTAINER_KINDS.contains` check — `Function` and `Method` are not in `CONTAINER_KINDS`, so the `matches!` guard is sufficient.

- [ ] **Step 4: Update all existing test call sites**

All tests that call `prepare_batch_for_embedding` need `&HashMap::new()` as the third arg. There are ~18 occurrences in `src/tests/core/embedding_metadata.rs`. Update them all:

```rust
// Every existing call like:
let batch = prepare_batch_for_embedding(&symbols, None);
// becomes:
let batch = prepare_batch_for_embedding(&symbols, None, &HashMap::new());
```

Also rename `test_prepare_batch_no_enrichment_for_functions` (line 427) to `test_prepare_batch_no_enrichment_for_functions_without_callees` since functions now CAN get enrichment when they have callees.

- [ ] **Step 5: Update pipeline call sites and add `build_callee_map` helper**

**`src/embeddings/pipeline.rs:138`** (full pipeline — add before `prepare_batch_for_embedding` call):
```rust
let callees_by_symbol = {
    let db_guard = db
        .lock()
        .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
    build_callee_map(&db_guard, &symbols)
};

let base_prepared = prepare_batch_for_embedding(&symbols, lang_configs, &callees_by_symbol);
```

**`src/embeddings/pipeline.rs:328`** (incremental per-file in `embed_symbols_for_file`):
```rust
let callees_by_symbol = {
    let db_guard = db
        .lock()
        .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
    build_callee_map(&db_guard, &symbols)
};

let prepared = prepare_batch_for_embedding(&symbols, lang_configs, &callees_by_symbol);
```

Note: `reembed_symbols_for_file` (pipeline.rs:386) delegates to `embed_symbols_for_file`, so it inherits the callee map automatically.

Add the `build_callee_map` helper in `src/embeddings/pipeline.rs`:

```rust
use crate::extractors::RelationshipKind;

/// Build a map of symbol_id → callee names from the relationship graph.
/// Only includes `Calls` relationships to avoid noise from imports/type refs.
///
/// Callee names are resolved from the provided `symbols` slice. In the full
/// pipeline, this is all workspace symbols (complete resolution). In the
/// per-file incremental path, only same-file callees resolve (best-effort).
fn build_callee_map(
    db: &SymbolDatabase,
    symbols: &[Symbol],
) -> HashMap<String, Vec<String>> {
    let func_ids: Vec<String> = symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|s| s.id.clone())
        .collect();

    if func_ids.is_empty() {
        return HashMap::new();
    }

    let relationships = match db.get_outgoing_relationships_for_symbols(&func_ids) {
        Ok(rels) => rels,
        Err(err) => {
            tracing::warn!("Failed to load callees for embedding enrichment: {err:#}");
            return HashMap::new();
        }
    };

    // Build symbol_id → name lookup for resolving callee names
    let id_to_name: HashMap<&str, &str> = symbols
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();

    let mut callees: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &relationships {
        if rel.kind == RelationshipKind::Calls {
            if let Some(name) = id_to_name.get(rel.to_symbol_id.as_str()) {
                callees
                    .entry(rel.from_symbol_id.clone())
                    .or_default()
                    .push(name.to_string());
            }
        }
    }

    // Deduplicate callee lists
    for names in callees.values_mut() {
        names.sort();
        names.dedup();
    }

    callees
}
```

- [ ] **Step 6: Run callee test to verify it passes**

Run: `cargo test --lib test_prepare_batch_enriches_function_with_callees 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 7: Write test for method callee enrichment**

```rust
#[test]
fn test_prepare_batch_enriches_method_with_callees() {
    let method = make_symbol(
        "m1",
        "process",
        SymbolKind::Method,
        Some("pub fn process(&self)"),
        None,
    );
    let symbols = vec![method];
    let mut callees = HashMap::new();
    callees.insert("m1".to_string(), vec!["save".to_string(), "validate".to_string()]);

    let batch = prepare_batch_for_embedding(&symbols, None, &callees);
    let (_, text) = &batch[0];
    assert!(
        text.contains("calls: save, validate"),
        "Method should have sorted callee enrichment: {text}"
    );
}
```

- [ ] **Step 8: Run test**

Run: `cargo test --lib test_prepare_batch_enriches_method_with_callees 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 9: Write test that containers do NOT get callee enrichment**

```rust
#[test]
fn test_prepare_batch_container_no_callee_enrichment() {
    let class = make_symbol_with_lang("c1", "MyService", SymbolKind::Class, "csharp");
    let symbols = vec![class];
    let mut callees = HashMap::new();
    callees.insert("c1".to_string(), vec!["something".to_string()]);

    let batch = prepare_batch_for_embedding(&symbols, None, &callees);
    let (_, text) = &batch[0];
    assert!(
        !text.contains("calls:"),
        "Container symbols should NOT get callee enrichment (they have child enrichment): {text}"
    );
}
```

- [ ] **Step 10: Run test**

Run: `cargo test --lib test_prepare_batch_container_no_callee_enrichment 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 11: Commit**

```bash
git add src/embeddings/metadata.rs src/embeddings/pipeline.rs src/tests/core/embedding_metadata.rs
git commit -m "feat(embeddings): enrich functions/methods with callee names for semantic bridging"
```

---

### Task 3: Increase MAX_METADATA_CHARS and fix force-re-embed

**Files:**
- Modify: `src/embeddings/metadata.rs:17` (`MAX_METADATA_CHARS` constant)
- Modify: `src/embeddings/pipeline.rs:204-213` (fix force-re-embed: add `Enum`, scope functions to those with callees)
- Test: `src/tests/core/embedding_metadata.rs`

- [ ] **Step 1: Write test that verifies enriched text is not truncated at 600 chars**

Construct a string that will exceed 600 chars with doc + signature + callees:

```rust
#[test]
fn test_enriched_function_with_callees_uses_expanded_budget() {
    let long_doc = "/// Orchestrates a complex multi-stage data processing pipeline that coordinates extraction from multiple sources. Manages transformation rules, validates intermediate results against business constraints, and loads final output into the target database system. Implements comprehensive retry logic for transient failures with exponential backoff.";
    let func = make_symbol(
        "f1",
        "orchestrate_complex_pipeline",
        SymbolKind::Function,
        Some("pub async fn orchestrate_complex_pipeline(handler: &JulieServerHandler, config: &PipelineConfig, options: &ProcessingOptions) -> Result<PipelineOutput>"),
        Some(long_doc),
    );
    let symbols = vec![func];
    let mut callees = HashMap::new();
    callees.insert("f1".to_string(), vec![
        "connect_to_source_database".to_string(),
        "extract_source_records".to_string(),
        "transform_with_business_rules".to_string(),
        "validate_intermediate_output".to_string(),
        "load_into_target_database".to_string(),
        "retry_with_exponential_backoff".to_string(),
    ]);

    let batch = prepare_batch_for_embedding(&symbols, None, &callees);
    let (_, text) = &batch[0];

    // This text should exceed 600 chars but fit within the new budget.
    // Verify the last callee is present (would be truncated at 600).
    assert!(
        text.contains("retry_with_exponential_backoff"),
        "Last callee should not be truncated with expanded budget: {text}"
    );
    assert!(
        text.contains("loads final output"),
        "Multi-sentence doc should survive within budget: {text}"
    );
    assert!(
        text.len() > 600,
        "Text should exceed old 600-char limit: len={}, text: {text}",
        text.len()
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_enriched_function_with_callees_uses_expanded_budget 2>&1 | tail -10`
Expected: FAIL — text truncated at 600 chars, missing last callees.

- [ ] **Step 3: Increase MAX_METADATA_CHARS**

In `src/embeddings/metadata.rs`, line 13-17:

```rust
/// Maximum characters for the embedding input text.
/// Jina-code-v2 and CodeRankEmbed handle up to 8192 tokens (~32K chars).
/// BGE-small handles up to 512 tokens (~2000 chars).
/// 1200 chars ≈ 240-300 tokens — safe for all supported models, and 2x the
/// previous budget. Gives room for multi-sentence docs + callee names
/// without approaching any model's limit.
const MAX_METADATA_CHARS: usize = 1200;
```

Why 1200 and not higher:
- BGE-small's 512-token limit is ~2000 chars. 1200 chars ≈ 300 tokens — still safe.
- Embedding models have diminishing returns past ~200 tokens for semantic signal.
- Larger input = larger VRAM usage per batch. We have VRAM concerns with Jina-code-v2.
- We can increase later if 1200 proves too tight.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib test_enriched_function_with_callees_uses_expanded_budget 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Update truncation test expectations**

The existing `test_child_enrichment_truncates_within_budget` (line 438) and `test_enriched_container_preserves_more_content_within_limit` (line 698) may need assertion updates since they test truncation at the old 600-char limit. Read these tests and adjust the expected truncation behavior for 1200 chars. If any test constructs a string that fit within 600 but should test truncation, make the test string longer so it still tests the truncation boundary.

- [ ] **Step 6: Fix force-re-embed: add `Enum`, scope functions to those with callees**

In `src/embeddings/pipeline.rs`, replace the block at lines 202-220.

**Pre-existing bug fix:** The current `container_ids` filter omits `Enum` even though `CONTAINER_KINDS` includes it. Fix this.

**Performance fix:** Instead of force-re-embedding ALL functions/methods (which would re-embed ~70-80% of symbols every run), only force-re-embed functions that actually have callee entries. The `callees_by_symbol` map is available at this point.

```rust
    // Symbols with relationship-based enrichment must be re-embedded when
    // relationships change. Containers get child enrichment, functions/methods
    // get callee enrichment.
    //
    // For containers: always re-embed (children may have changed).
    // For functions/methods: only re-embed if they have callees in the map
    //   (most functions have no callees, so this avoids re-embedding ~70% of symbols).
    let enriched_ids: HashSet<&str> = symbols
        .iter()
        .filter(|s| {
            match s.kind {
                // Containers always get child enrichment — re-embed unconditionally
                SymbolKind::Class
                | SymbolKind::Struct
                | SymbolKind::Interface
                | SymbolKind::Trait
                | SymbolKind::Enum => true,
                // Functions/methods only if they have callees
                SymbolKind::Function | SymbolKind::Method => {
                    callees_by_symbol.contains_key(&s.id)
                }
                _ => false,
            }
        })
        .map(|s| s.id.as_str())
        .collect();

    // Skip symbols that already have embeddings (incremental),
    // EXCEPT enriched symbols which always get re-embedded.
    let prepared: Vec<_> = all_prepared
        .into_iter()
        .filter(|(id, _)| !already_embedded.contains(id) || enriched_ids.contains(id.as_str()))
        .collect();
```

Note: `callees_by_symbol` must be in scope at this point — it was built earlier in the pipeline (Task 2 Step 5).

- [ ] **Step 7: Run all embedding metadata tests**

Run: `cargo test --lib embedding_metadata 2>&1 | tail -20`
Expected: All pass (may need to fix assertion values for new budget)

- [ ] **Step 8: Commit**

```bash
git add src/embeddings/metadata.rs src/embeddings/pipeline.rs src/tests/core/embedding_metadata.rs
git commit -m "feat(embeddings): raise metadata budget to 1200 chars, fix force-re-embed for Enum + callee functions"
```

---

### Task 4: Integration verification

**Files:**
- No new code — this is a verification task

- [ ] **Step 1: Run xtask dev tier**

Run: `cargo xtask test dev`
Expected: All green. If any failures, investigate — these are real regressions.

- [ ] **Step 2: Rebuild release binary and restart**

The user must exit Claude Code, run `cargo build --release`, and restart. This triggers re-indexing with enriched embeddings.

- [ ] **Step 3: Verify the `record_tool_call` miss is fixed**

Run: `get_context(query="metrics save database persist")`
Expected: `record_tool_call` appears as a pivot or neighbor. Its embedding text should now include "SQLite write" (from expanded doc) and "calls: insert_tool_call, get_total_file_sizes" (from callee enrichment).

- [ ] **Step 4: Verify LabHandbook "rich text editing" query**

Run: `get_context(query="content management rich text editing", workspace="labhandbookv2_...")`
Expected: Improved results — `ContentService`, `ContentController`, or content store symbols should appear, not just `editingLinkId` variables.

- [ ] **Step 5: Spot-check that existing good queries haven't regressed**

Run the known-good queries from the dogfood: "authentication and user roles", "how does the frontend communicate with the backend API", "lab test validation rules". All should maintain A/A+ quality.

- [ ] **Step 6: Commit version bump if all passes**

```bash
git commit -m "chore: bump version for embedding enrichment release"
```
