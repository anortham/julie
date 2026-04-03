# Embedding Enrichment: Behavioral Fingerprints Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enrich embedding input text with behavioral context (callees, file paths, implementors), add query classification for smarter hybrid search weighting, and tune tool consumers for better semantic results.

**Architecture:** Enrichment happens in `src/embeddings/metadata.rs` (formatting) and `src/embeddings/pipeline.rs` (data gathering). Query classification is a lightweight heuristic in `src/search/weights.rs`. Consumer tuning touches `get_context` pivot scoring. All changes are additive; no storage or model changes.

**Tech Stack:** Rust, tree-sitter symbol data, sqlite-vec KNN, Tantivy BM25, CodeRankEmbed (unchanged)

**Spec:** `docs/superpowers/specs/2026-04-03-embedding-enrichment-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `benchmarks/embedding_enrichment/baseline.md` | Create | Before-state benchmark results |
| `benchmarks/embedding_enrichment/after.md` | Create | After-state benchmark results (Task 8) |
| `src/embeddings/metadata.rs` | Modify | `format_symbol_metadata` gains file_path; `prepare_batch_for_embedding` gains implementor enrichment |
| `src/embeddings/pipeline.rs` | Modify | `build_implementor_map` helper; pass implementors + child signatures to metadata |
| `src/search/weights.rs` | Modify | `QueryIntent` enum, `classify_query` function, intent-to-profile mapping |
| `src/search/hybrid.rs` | Modify | Use `classify_query` when no explicit weight profile |
| `src/tools/get_context/scoring.rs` | Modify | Scoring formula adjustment in `select_pivots` |
| `src/tests/core/embedding_metadata.rs` | Modify | Tests for enriched format |
| `src/tests/core/embedding_metadata_enrichment.rs` | Modify | Tests for implementor/signature enrichment |
| `src/tests/tools/query_classification_tests.rs` | Create | Tests for classify_query |
| `src/tests/tools/hybrid_search_tests.rs` | Modify | Tests for query-classified hybrid search |

---

### Task 1: Capture Benchmark Baseline

**Files:**
- Create: `benchmarks/embedding_enrichment/baseline.md`

This task captures the current state of search/semantic results across 4 workspaces before any changes. Run each query manually via Julie MCP tools and paste the results. This is a manual data-collection task, not code.

- [ ] **Step 1: Create benchmark directory**

Run: `mkdir -p benchmarks/embedding_enrichment`

- [ ] **Step 2: Run exact symbol lookup queries and record results**

Run these via Julie MCP tools, copy full output to the baseline file:

```
# Julie (Rust)
fast_search(query="hybrid_search", search_target="definitions", limit=5)
fast_search(query="prepare_batch_for_embedding", search_target="definitions", limit=5)

# Zod (TypeScript) - adjust workspace param
fast_search(query="ZodType", search_target="definitions", limit=5, workspace="<zod_id>")
fast_search(query="parse", search_target="definitions", limit=5, workspace="<zod_id>")

# Flask (Python)
fast_search(query="Flask", search_target="definitions", limit=5, workspace="<flask_id>")
fast_search(query="route", search_target="definitions", limit=5, workspace="<flask_id>")

# Cobra (Go)
fast_search(query="Command", search_target="definitions", limit=5, workspace="<cobra_id>")
fast_search(query="Execute", search_target="definitions", limit=5, workspace="<cobra_id>")
```

- [ ] **Step 3: Run conceptual/natural language queries and record results**

```
# Julie
fast_search(query="error handling and retry logic", limit=10)
fast_search(query="search scoring and ranking", limit=10)
fast_search(query="symbol extraction from source code", limit=10)

# Zod
fast_search(query="input validation and type checking", limit=10, workspace="<zod_id>")

# Flask
fast_search(query="request routing and middleware", limit=10, workspace="<flask_id>")

# Cobra
fast_search(query="command line argument parsing", limit=10, workspace="<cobra_id>")
```

- [ ] **Step 4: Run deep_dive similar-symbols queries and record results**

```
# Julie - well-known symbols
deep_dive(symbol="hybrid_search", depth="context")  # capture Similar section
deep_dive(symbol="format_symbol_metadata", depth="context")
deep_dive(symbol="SymbolDatabase", depth="context")

# Zod
deep_dive(symbol="ZodType", depth="context", workspace="<zod_id>")

# Flask
deep_dive(symbol="Flask", depth="context", workspace="<flask_id>")
```

- [ ] **Step 5: Run get_context orientation queries and record results**

```
get_context(query="embedding pipeline and vector search")
get_context(query="how does search scoring work")
get_context(query="type validation", workspace="<zod_id>")
get_context(query="HTTP request handling", workspace="<flask_id>")
```

- [ ] **Step 6: Save all results to baseline file**

Write all collected outputs to `benchmarks/embedding_enrichment/baseline.md` with clear section headers per query category and workspace. Include the date and current commit hash.

- [ ] **Step 7: Commit**

```bash
git add benchmarks/embedding_enrichment/baseline.md
git commit -m "chore(benchmarks): capture embedding enrichment baseline"
```

---

### Task 2: Add File Path to format_symbol_metadata

**Files:**
- Modify: `src/embeddings/metadata.rs:110-140`
- Modify: `src/tests/core/embedding_metadata.rs`

The simplest enrichment: append the symbol's relative file path to the embedding text. This provides module-level context ("where does this live?") that helps the model distinguish same-named symbols in different areas.

- [ ] **Step 1: Write failing test for file path inclusion**

Add to `src/tests/core/embedding_metadata.rs` after the existing `test_format_with_all_fields` test:

```rust
#[test]
fn test_format_includes_file_path() {
    let mut sym = make_symbol(
        "id_fp",
        "process_payment",
        SymbolKind::Function,
        Some("fn process_payment(amount: f64) -> Result<Receipt>"),
        None,
    );
    sym.file_path = "src/billing/processor.rs".to_string();
    let text = format_symbol_metadata(&sym);
    assert!(
        text.contains("in: src/billing/processor.rs"),
        "Expected file path in output, got: {text}"
    );
}

#[test]
fn test_format_file_path_empty_when_missing() {
    // file_path is always set on Symbol, but test empty string case
    let mut sym = make_symbol(
        "id_fp2",
        "helper",
        SymbolKind::Function,
        Some("fn helper()"),
        None,
    );
    sym.file_path = String::new();
    let text = format_symbol_metadata(&sym);
    assert!(
        !text.contains("in:"),
        "Should not include 'in:' prefix for empty path, got: {text}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_format_includes_file_path 2>&1 | tail -10`
Expected: FAIL (format_symbol_metadata doesn't include file path yet)

Run: `cargo test --lib test_format_file_path_empty_when_missing 2>&1 | tail -10`
Expected: PASS (no "in:" in current output, which is correct)

- [ ] **Step 3: Implement file path inclusion in format_symbol_metadata**

Modify `src/embeddings/metadata.rs` function `format_symbol_metadata` (line 110). Change the function to accept the file path and append it as a newline-separated suffix:

```rust
pub fn format_symbol_metadata(symbol: &Symbol) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(4);

    // Kind as lowercase word
    let kind_str = kind_to_str(&symbol.kind);
    parts.push(kind_str);

    // Symbol name
    parts.push(&symbol.name);

    // Signature excerpt (first line only, trimmed)
    let sig_excerpt;
    if let Some(ref sig) = symbol.signature {
        sig_excerpt = first_line_trimmed(sig);
        if !sig_excerpt.is_empty() {
            parts.push(&sig_excerpt);
        }
    }

    // Doc comment excerpt (multi-sentence for embedding richness)
    let doc_excerpt;
    if let Some(ref doc) = symbol.doc_comment {
        doc_excerpt = extract_doc_excerpt(doc);
        if !doc_excerpt.is_empty() {
            parts.push(&doc_excerpt);
        }
    }

    let mut joined = parts.join(" ");

    // File path context (newline-separated for clear signal boundary)
    if !symbol.file_path.is_empty() {
        joined.push_str("\nin: ");
        joined.push_str(&symbol.file_path);
    }

    truncate_on_word_boundary(&joined, MAX_METADATA_CHARS)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib test_format_includes_file_path 2>&1 | tail -10`
Expected: PASS

Run: `cargo test --lib test_format_file_path_empty_when_missing 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Update existing tests that assert on exact output**

Some existing tests like `test_format_with_all_fields`, `test_format_name_only`, etc. assert on exact string output. Update them to account for the new `\nin: <path>` suffix. Since `make_symbol` sets `file_path` to an empty string, most tests should pass unchanged. Verify:

Run: `cargo test --lib test_format_with_all_fields 2>&1 | tail -10`
Run: `cargo test --lib test_format_name_only 2>&1 | tail -10`

If any fail, update the `make_symbol` helper or the assertions to account for the file_path. The `make_symbol` helper currently sets `file_path: String::new()` (or `"test.rs"` depending on the helper), so check what it produces.

- [ ] **Step 6: Commit**

```bash
git add src/embeddings/metadata.rs src/tests/core/embedding_metadata.rs
git commit -m "feat(embeddings): include file path in symbol metadata for embedding"
```

---

### Task 3: Enrich Trait/Interface with Implementor Names

**Files:**
- Modify: `src/embeddings/pipeline.rs:45-99`
- Modify: `src/embeddings/metadata.rs:154-244`
- Modify: `src/tests/core/embedding_metadata_enrichment.rs`

Add a `build_implementor_map` helper that queries `Implements`/`Extends` relationships pointing TO trait/interface symbols, then pass the implementor names to `prepare_batch_for_embedding` for enrichment.

- [ ] **Step 1: Write failing test for implementor enrichment**

Add to `src/tests/core/embedding_metadata_enrichment.rs`:

```rust
#[test]
fn test_prepare_batch_enriches_trait_with_implementors() {
    let trait_sym = make_symbol_with_lang(
        "t1",
        "EmbeddingProvider",
        SymbolKind::Trait,
        "rust",
    );

    // Child methods (existing enrichment)
    let mut method1 = make_symbol_with_lang("m1", "embed_query", SymbolKind::Method, "rust");
    method1.parent_id = Some("t1".to_string());

    let mut method2 = make_symbol_with_lang("m2", "embed_batch", SymbolKind::Method, "rust");
    method2.parent_id = Some("t1".to_string());

    let symbols = vec![trait_sym, method1, method2];

    // Implementor names passed via the new parameter
    let mut implementors_by_symbol: HashMap<String, Vec<String>> = HashMap::new();
    implementors_by_symbol.insert(
        "t1".to_string(),
        vec!["SidecarEmbeddingProvider".to_string(), "PartialProvider".to_string()],
    );

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &implementors_by_symbol,
    );

    assert_eq!(batch.len(), 1); // Only the trait is embeddable
    let (_, text) = &batch[0];
    assert!(
        text.contains("implemented_by: SidecarEmbeddingProvider, PartialProvider"),
        "Expected implementor names in trait embedding text, got: {text}"
    );
    assert!(
        text.contains("methods: embed_query, embed_batch"),
        "Expected child methods preserved, got: {text}"
    );
}

#[test]
fn test_prepare_batch_enriches_interface_with_implementors() {
    let iface = make_symbol_with_lang("i1", "ISearchService", SymbolKind::Interface, "csharp");

    let mut method = make_symbol_with_lang("m1", "Search", SymbolKind::Method, "csharp");
    method.parent_id = Some("i1".to_string());

    let symbols = vec![iface, method];

    let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
    implementors.insert(
        "i1".to_string(),
        vec!["LuceneSearchService".to_string()],
    );

    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &implementors,
    );

    assert_eq!(batch.len(), 1);
    let (_, text) = &batch[0];
    assert!(
        text.contains("implemented_by: LuceneSearchService"),
        "Expected implementor name, got: {text}"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_prepare_batch_enriches_trait_with_implementors 2>&1 | tail -10`
Expected: FAIL (compile error, signature doesn't match yet)

- [ ] **Step 3: Add implementors_by_symbol parameter to prepare_batch_for_embedding**

Modify `src/embeddings/metadata.rs` function signature at line 154:

```rust
pub fn prepare_batch_for_embedding(
    symbols: &[Symbol],
    lang_configs: Option<&LanguageConfigs>,
    callees_by_symbol: &HashMap<String, Vec<String>>,
    fields_by_symbol: &HashMap<String, Vec<String>>,
    implementors_by_symbol: &HashMap<String, Vec<String>>,
) -> Vec<(String, String)> {
```

Add implementor enrichment inside the container enrichment block (after variants, before the closing `text = truncate_on_word_boundary`):

```rust
            if CONTAINER_KINDS.contains(&s.kind) {
                // ... existing methods, properties, variants enrichment ...

                // Implementor names for traits/interfaces
                if matches!(s.kind, SymbolKind::Trait | SymbolKind::Interface) {
                    if let Some(impls) = implementors_by_symbol.get(&s.id) {
                        if !impls.is_empty() {
                            let suffix = format!(" implemented_by: {}", impls.join(", "));
                            text.push_str(&suffix);
                        }
                    }
                }

                text = truncate_on_word_boundary(&text, MAX_METADATA_CHARS);
            }
```

- [ ] **Step 4: Fix all callers of prepare_batch_for_embedding**

The function signature changed. Update all call sites to pass the new parameter.

In `src/embeddings/pipeline.rs` at the call site (~line 218):
```rust
    let base_prepared = prepare_batch_for_embedding(
        &symbols,
        lang_configs,
        &callees_by_symbol,
        &fields_by_symbol,
        &HashMap::new(), // implementors_by_symbol - populated in Task 4
    );
```

In `src/embeddings/pipeline.rs` at the `embed_symbols_for_file` call site (~line 458):
```rust
    let prepared = prepare_batch_for_embedding(
        &symbols,
        lang_configs,
        &callees_by_symbol,
        &fields_by_symbol,
        &HashMap::new(), // implementors - not available in per-file path
    );
```

Update ALL test call sites in `src/tests/core/embedding_metadata.rs` and `src/tests/core/embedding_metadata_enrichment.rs` to pass `&HashMap::new()` as the fifth argument. There are approximately 20+ call sites. Use find-and-replace: change `&HashMap::new(), &HashMap::new())` to `&HashMap::new(), &HashMap::new(), &HashMap::new())` at every `prepare_batch_for_embedding` call.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib test_prepare_batch_enriches_trait_with_implementors 2>&1 | tail -10`
Expected: PASS

Run: `cargo test --lib test_prepare_batch_enriches_interface_with_implementors 2>&1 | tail -10`
Expected: PASS

Run: `cargo test --lib test_prepare_batch 2>&1 | tail -10`
Expected: All existing prepare_batch tests still PASS

- [ ] **Step 6: Commit**

```bash
git add src/embeddings/metadata.rs src/embeddings/pipeline.rs \
    src/tests/core/embedding_metadata.rs src/tests/core/embedding_metadata_enrichment.rs
git commit -m "feat(embeddings): enrich trait/interface embeddings with implementor names"
```

---

### Task 4: Build Implementor Map in Pipeline

**Files:**
- Modify: `src/embeddings/pipeline.rs`

Wire up the actual implementor data from the relationships table into the embedding pipeline.

- [ ] **Step 1: Write the build_implementor_map helper**

Add to `src/embeddings/pipeline.rs` after `build_field_access_map` (~line 99):

```rust
/// Build a map of symbol_id -> implementor names from the relationship graph.
/// Finds `Implements` and `Extends` relationships pointing TO trait/interface symbols.
fn build_implementor_map(db: &SymbolDatabase, symbols: &[Symbol]) -> HashMap<String, Vec<String>> {
    let trait_interface_ids: Vec<String> = symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Trait | SymbolKind::Interface))
        .map(|s| s.id.clone())
        .collect();

    if trait_interface_ids.is_empty() {
        return HashMap::new();
    }

    let relationships = match db.get_relationships_to_symbols(&trait_interface_ids) {
        Ok(rels) => rels,
        Err(err) => {
            tracing::warn!("Failed to load implementors for embedding enrichment: {err:#}");
            return HashMap::new();
        }
    };

    let id_to_name: HashMap<&str, &str> = symbols
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();

    let mut implementors: HashMap<String, Vec<String>> = HashMap::new();
    for rel in &relationships {
        if matches!(rel.kind, RelationshipKind::Implements | RelationshipKind::Extends) {
            let impl_name = id_to_name
                .get(rel.from_symbol_id.as_str())
                .unwrap_or(&rel.from_symbol_id.as_str());
            implementors
                .entry(rel.to_symbol_id.clone())
                .or_default()
                .push(impl_name.to_string());
        }
    }

    // Deduplicate and cap at 8 per trait/interface
    for names in implementors.values_mut() {
        names.sort();
        names.dedup();
        names.truncate(8);
    }

    implementors
}
```

- [ ] **Step 2: Wire build_implementor_map into run_embedding_pipeline_cancellable**

In `src/embeddings/pipeline.rs`, in the `run_embedding_pipeline_cancellable` function, after `build_field_access_map` (~line 213):

```rust
    let (callees_by_symbol, fields_by_symbol, implementors_by_symbol) = {
        let db_guard = db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB mutex poisoned: {e}"))?;
        (
            build_callee_map(&db_guard, &symbols),
            build_field_access_map(&db_guard),
            build_implementor_map(&db_guard, &symbols),
        )
    };
```

Update the `prepare_batch_for_embedding` call (~line 218) to pass `&implementors_by_symbol` instead of `&HashMap::new()`:

```rust
    let base_prepared = prepare_batch_for_embedding(
        &symbols,
        lang_configs,
        &callees_by_symbol,
        &fields_by_symbol,
        &implementors_by_symbol,
    );
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build 2>&1 | tail -10`
Expected: Compiles without errors

- [ ] **Step 4: Run the full embedding metadata test suite**

Run: `cargo test --lib tests::core::embedding_metadata 2>&1 | tail -20`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/embeddings/pipeline.rs
git commit -m "feat(embeddings): wire implementor map into embedding pipeline"
```

---

### Task 5: Enrich Class/Struct with Child Field Signatures

**Files:**
- Modify: `src/embeddings/metadata.rs:154-244`
- Modify: `src/tests/core/embedding_metadata_enrichment.rs`

Currently container enrichment includes child property/field *names*. Upgrade to include their *signatures* (which capture type info like `name: String`), falling back to just the name if no signature exists.

- [ ] **Step 1: Write failing test**

Add to `src/tests/core/embedding_metadata_enrichment.rs`:

```rust
#[test]
fn test_prepare_batch_enriches_struct_with_field_signatures() {
    let struct_sym = make_symbol_with_lang("s1", "UserRecord", SymbolKind::Struct, "rust");

    let mut field1 = make_symbol_with_lang("f1", "name", SymbolKind::Field, "rust");
    field1.parent_id = Some("s1".to_string());
    field1.signature = Some("pub name: String".to_string());

    let mut field2 = make_symbol_with_lang("f2", "age", SymbolKind::Field, "rust");
    field2.parent_id = Some("s1".to_string());
    field2.signature = Some("pub age: u32".to_string());

    let mut field3 = make_symbol_with_lang("f3", "active", SymbolKind::Field, "rust");
    field3.parent_id = Some("s1".to_string());
    // No signature - should fall back to name
    field3.signature = None;

    let symbols = vec![struct_sym, field1, field2, field3];
    let batch = prepare_batch_for_embedding(
        &symbols,
        None,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
    );

    assert_eq!(batch.len(), 1);
    let (_, text) = &batch[0];
    // Should use signatures where available, names as fallback
    assert!(
        text.contains("fields: pub name: String, pub age: u32, active"),
        "Expected field signatures in embedding, got: {text}"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib test_prepare_batch_enriches_struct_with_field_signatures 2>&1 | tail -10`
Expected: FAIL (current output has `properties: name, age, active` without type info)

- [ ] **Step 3: Implement field signature enrichment**

In `src/embeddings/metadata.rs`, change the `properties_by_parent` map to store signature-or-name instead of just names. Replace the current `properties_by_parent` construction (lines 161-178):

```rust
    // Build parent_id -> child property/field signatures for container enrichment.
    // Uses signature (type info) where available, falls back to name.
    let mut field_sigs_by_parent: HashMap<&str, Vec<String>> = HashMap::new();
```

And in the loop where child symbols are categorized:

```rust
                SymbolKind::Property | SymbolKind::Field => {
                    let display = match &sym.signature {
                        Some(sig) => first_line_trimmed(sig).to_string(),
                        None => sym.name.clone(),
                    };
                    field_sigs_by_parent
                        .entry(parent_id.as_str())
                        .or_default()
                        .push(display);
                }
```

Update the enrichment section that uses `properties_by_parent` to use `field_sigs_by_parent` instead, and rename the label from `properties:` to `fields:`:

```rust
                if let Some(field_sigs) = field_sigs_by_parent.get(s.id.as_str()) {
                    let suffix = format!(" fields: {}", field_sigs.join(", "));
                    text.push_str(&suffix);
                }
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib test_prepare_batch_enriches_struct_with_field_signatures 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Update existing tests that assert on "properties:" label**

The label changed from `properties:` to `fields:`. Update assertions in these existing tests:
- `test_prepare_batch_enriches_class_with_child_properties`
- `test_prepare_batch_enriches_interface_with_fields`
- `test_prepare_batch_enriches_with_both_methods_and_properties`
- `test_prepare_batch_struct_enriched_with_fields`
- `test_prepare_batch_no_field_enrichment_for_containers`
- `test_enriched_container_preserves_more_content_within_limit`

Change `contains("properties:")` to `contains("fields:")` in each. Also verify the tests still hold since field_sigs_by_parent now stores signatures where available (the test fixtures use `make_symbol_with_lang` which sets `signature: None`, so they should still produce just names).

Run: `cargo test --lib test_prepare_batch_enriches_class_with_child_properties 2>&1 | tail -10`
Run: `cargo test --lib test_prepare_batch_enriches_with_both_methods_and_properties 2>&1 | tail -10`
Expected: PASS after label update

- [ ] **Step 6: Commit**

```bash
git add src/embeddings/metadata.rs src/tests/core/embedding_metadata.rs \
    src/tests/core/embedding_metadata_enrichment.rs
git commit -m "feat(embeddings): enrich container embeddings with field signatures instead of just names"
```

---

### Task 6: Query Classification for Hybrid Search

**Files:**
- Modify: `src/search/weights.rs`
- Create: `src/tests/tools/query_classification_tests.rs`
- Modify: `src/search/hybrid.rs:174-267`
- Modify: `src/tests/tools/hybrid_search_tests.rs`

Add a lightweight heuristic that classifies queries as symbol lookups vs conceptual queries and maps them to different weight profiles.

- [ ] **Step 1: Write tests for query classification**

Create `src/tests/tools/query_classification_tests.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::search::weights::{classify_query, QueryIntent};

    #[test]
    fn test_classify_snake_case_as_symbol() {
        assert_eq!(classify_query("hybrid_search"), QueryIntent::SymbolLookup);
        assert_eq!(classify_query("prepare_batch_for_embedding"), QueryIntent::SymbolLookup);
    }

    #[test]
    fn test_classify_camel_case_as_symbol() {
        assert_eq!(classify_query("SearchWeightProfile"), QueryIntent::SymbolLookup);
        assert_eq!(classify_query("SymbolDatabase"), QueryIntent::SymbolLookup);
    }

    #[test]
    fn test_classify_qualified_name_as_symbol() {
        assert_eq!(classify_query("std::collections::HashMap"), QueryIntent::SymbolLookup);
        assert_eq!(classify_query("Phoenix.Router"), QueryIntent::SymbolLookup);
    }

    #[test]
    fn test_classify_natural_language_as_conceptual() {
        assert_eq!(classify_query("error handling and retry logic"), QueryIntent::Conceptual);
        assert_eq!(classify_query("how does authentication work"), QueryIntent::Conceptual);
        assert_eq!(classify_query("search scoring and ranking"), QueryIntent::Conceptual);
    }

    #[test]
    fn test_classify_short_natural_language_as_mixed() {
        // 2-3 word NL queries are ambiguous
        assert_eq!(classify_query("error handling"), QueryIntent::Mixed);
        assert_eq!(classify_query("payment validation"), QueryIntent::Mixed);
    }

    #[test]
    fn test_classify_single_lowercase_word_as_mixed() {
        // Could be a symbol or a concept
        assert_eq!(classify_query("search"), QueryIntent::Mixed);
        assert_eq!(classify_query("database"), QueryIntent::Mixed);
    }

    #[test]
    fn test_classify_mixed_code_and_nl() {
        assert_eq!(classify_query("SymbolDatabase query methods"), QueryIntent::Mixed);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib test_classify_snake_case_as_symbol 2>&1 | tail -10`
Expected: FAIL (compile error, classify_query doesn't exist yet)

- [ ] **Step 3: Implement QueryIntent and classify_query**

Add to `src/search/weights.rs`:

```rust
/// Query intent classification for dynamic weight profile selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    /// Exact symbol lookup (snake_case, CamelCase, qualified names)
    SymbolLookup,
    /// Natural language / conceptual query (4+ words, no code tokens)
    Conceptual,
    /// Ambiguous mix of code and natural language
    Mixed,
}

/// Classify a search query to determine optimal keyword/semantic weighting.
///
/// Uses lightweight heuristics (no ML):
/// - snake_case, CamelCase, `::`, `.` separators -> SymbolLookup
/// - 4+ space-separated words with no code-like tokens -> Conceptual
/// - Everything else -> Mixed
pub fn classify_query(query: &str) -> QueryIntent {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return QueryIntent::Mixed;
    }

    // Check for code-like patterns
    let has_snake_case = trimmed.contains('_') && trimmed.chars().any(|c| c.is_lowercase());
    let has_qualified = trimmed.contains("::") || (trimmed.contains('.') && !trimmed.ends_with('.'));
    let has_camel_case = trimmed.chars().any(|c| c.is_uppercase())
        && trimmed.chars().any(|c| c.is_lowercase())
        && !trimmed.contains(' ');

    if has_snake_case || has_qualified || has_camel_case {
        // If it also has spaces and multiple words, it's mixed
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.len() >= 3 {
            return QueryIntent::Mixed;
        }
        return QueryIntent::SymbolLookup;
    }

    // Count space-separated words
    let words: Vec<&str> = trimmed.split_whitespace().collect();

    if words.len() >= 4 {
        return QueryIntent::Conceptual;
    }

    QueryIntent::Mixed
}

impl QueryIntent {
    /// Map query intent to a search weight profile.
    pub fn to_weight_profile(&self) -> SearchWeightProfile {
        match self {
            QueryIntent::SymbolLookup => SearchWeightProfile {
                keyword_weight: 1.0,
                semantic_weight: 0.3,
            },
            QueryIntent::Conceptual => SearchWeightProfile {
                keyword_weight: 0.5,
                semantic_weight: 1.0,
            },
            QueryIntent::Mixed => SearchWeightProfile {
                keyword_weight: 0.8,
                semantic_weight: 0.8,
            },
        }
    }
}
```

- [ ] **Step 4: Register the test module**

Add `mod query_classification_tests;` to the appropriate test module file in `src/tests/tools/mod.rs`.

- [ ] **Step 5: Run classification tests**

Run: `cargo test --lib test_classify_ 2>&1 | tail -20`
Expected: All 7 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/search/weights.rs src/tests/tools/query_classification_tests.rs src/tests/tools/mod.rs
git commit -m "feat(search): add query intent classification for dynamic weight profiles"
```

---

### Task 7: Integrate Query Classification into Hybrid Search

**Files:**
- Modify: `src/search/hybrid.rs:174-267`
- Modify: `src/tests/tools/hybrid_search_tests.rs`

When `hybrid_search` is called with `weight_profile: None`, use `classify_query` to pick a profile dynamically instead of defaulting to uniform RRF.

- [ ] **Step 1: Write failing test**

Add to `src/tests/tools/hybrid_search_tests.rs`:

```rust
#[test]
fn test_hybrid_search_none_profile_uses_query_classification() {
    // When no explicit profile is given, hybrid_search should use
    // classify_query to pick weights. A conceptual query should
    // produce weighted merge, not uniform.
    let (index, db) = build_test_index_and_db();
    let provider = MockEmbeddingProvider::new();
    let filter = SearchFilter::default();

    // Conceptual query - should get semantic-heavy weights
    let result = hybrid_search(
        "error handling and retry logic",
        &filter,
        10,
        &index,
        &db,
        Some(&provider),
        None, // no explicit profile
    );

    assert!(result.is_ok());
    // The key assertion: results should exist (semantic search activated with real weight)
    // We can't directly assert on weights, but we verify the code path runs without error
}
```

Note: The exact test setup depends on the existing test infrastructure in `hybrid_search_tests.rs`. Use the same `build_test_index_and_db` or `MockEmbeddingProvider` patterns already present in that file.

- [ ] **Step 2: Implement classification integration in hybrid_search**

Modify `src/search/hybrid.rs` in the `hybrid_search` function. Change the merge step (~line 245):

```rust
    use crate::search::weights::classify_query;

    let merged = match weight_profile {
        Some(profile) => {
            debug!(
                "  weight profile (explicit): keyword={:.2}, semantic={:.2}",
                profile.keyword_weight, profile.semantic_weight
            );
            weighted_rrf_merge(
                tantivy_results.results,
                semantic_results,
                60,
                limit,
                profile.keyword_weight,
                profile.semantic_weight,
            )
        }
        None => {
            // Dynamic: classify query and pick weights
            let intent = classify_query(query);
            let profile = intent.to_weight_profile();
            debug!(
                "  weight profile (classified {:?}): keyword={:.2}, semantic={:.2}",
                intent, profile.keyword_weight, profile.semantic_weight
            );
            weighted_rrf_merge(
                tantivy_results.results,
                semantic_results,
                60,
                limit,
                profile.keyword_weight,
                profile.semantic_weight,
            )
        }
    };
```

- [ ] **Step 3: Run existing hybrid search tests**

Run: `cargo test --lib tests::tools::hybrid_search 2>&1 | tail -20`
Expected: All existing tests still pass. The `test_hybrid_search_none_provider_returns_keyword_results` test should be unaffected since it has no embedding provider (classification only matters when semantic results exist).

- [ ] **Step 4: Run the new test**

Run: `cargo test --lib test_hybrid_search_none_profile_uses_query_classification 2>&1 | tail -10`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/search/hybrid.rs src/tests/tools/hybrid_search_tests.rs
git commit -m "feat(search): integrate query classification into hybrid search merge"
```

---

### Task 8: Re-index and Capture After Benchmark

**Files:**
- Create: `benchmarks/embedding_enrichment/after.md`

Re-embed all test workspaces with the enriched metadata, then run the exact same benchmark queries from Task 1.

- [ ] **Step 1: Build release binary**

Ask the user to exit Claude Code, then:
Run: `cargo build --release`

- [ ] **Step 2: Re-index all test workspaces**

Start the daemon and force re-index each workspace:

```
manage_workspace(operation="index", force=true)
manage_workspace(operation="index", workspace_id="<zod_id>", force=true)
manage_workspace(operation="index", workspace_id="<flask_id>", force=true)
manage_workspace(operation="index", workspace_id="<cobra_id>", force=true)
```

- [ ] **Step 3: Run the exact same queries from Task 1**

Run all queries from Steps 2-5 of Task 1 against the re-indexed workspaces.

- [ ] **Step 4: Save results to after.md**

Write to `benchmarks/embedding_enrichment/after.md` with the same section structure as baseline.md.

- [ ] **Step 5: Compare and document findings**

Add a "Comparison" section at the bottom of after.md noting:
- Which conceptual queries returned better results
- Whether similar-symbols sections in deep_dive improved
- Whether get_context pivots changed
- Any regressions in exact symbol lookups

- [ ] **Step 6: Commit**

```bash
git add benchmarks/embedding_enrichment/after.md
git commit -m "chore(benchmarks): capture embedding enrichment after-state results"
```

---

### Task 9: Tune Thresholds and get_context Scoring

**Files:**
- Modify: `src/tools/get_context/scoring.rs` (if benchmark shows pivot selection needs improvement)
- Modify: `src/search/similarity.rs` (if thresholds need adjustment)

This task is benchmark-driven. Based on Task 8 results, make targeted tuning adjustments.

- [ ] **Step 1: Evaluate threshold impact from benchmark**

Compare similarity scores in the before/after deep_dive results. If the enriched embeddings produce higher similarity scores for genuinely related symbols, the thresholds may need tightening (raising MIN_SIMILARITY_SCORE from 0.5 to e.g. 0.55) to reduce noise. If scores spread out more, they may need loosening.

If no threshold change is needed, skip to Step 4.

- [ ] **Step 2: Adjust thresholds (if needed)**

In `src/search/similarity.rs`, adjust `MIN_SIMILARITY_SCORE` based on observed data.

In `src/tools/navigation/fast_refs.rs`, adjust `QUERY_SIMILARITY_THRESHOLD` if the semantic fallback results changed.

- [ ] **Step 3: Run affected tests**

Run: `cargo test --lib test_find_similar 2>&1 | tail -10`
Expected: Tests may need threshold updates to match new values.

- [ ] **Step 4: Evaluate get_context pivot quality**

If the benchmark shows get_context pivots improved just from better hybrid search (via query classification), no scoring change is needed.

If pivots are still suboptimal for conceptual queries, add embedding similarity as a tiebreaker in `select_pivots` (`src/tools/get_context/scoring.rs`). The existing scoring uses `result.score * (1.0 + centrality_boost)`. The tiebreaker would be: when two candidates have scores within 10% of each other, prefer the one with higher search score (which now includes semantic signal from query classification).

This may not require code changes since query classification already biases the hybrid search results toward semantically relevant symbols.

- [ ] **Step 5: Commit (if changes were made)**

```bash
git add src/search/similarity.rs src/tools/get_context/scoring.rs \
    src/tools/navigation/fast_refs.rs
git commit -m "fix(search): tune similarity thresholds after embedding enrichment"
```

---

### Task 10: Update Workflow Instructions

**Files:**
- Modify: Julie plugin server instructions / SessionStart hook
- Modify: Julie plugin skills (explore-area, logic-flow, call-trace)

Update agent-facing documentation to leverage improved semantic search.

- [ ] **Step 1: Update fast_search tool description**

In Julie's MCP tool description for `fast_search`, add guidance about conceptual queries. The current description says "Supports multi-word queries with AND/OR logic." Expand to mention that natural language queries like "error handling retry logic" now work well for discovering relevant code by concept, not just by name.

- [ ] **Step 2: Update SessionStart hook content**

Add a line to the Julie server instructions (SessionStart hook) noting:
- For exact symbols: `fast_search(query="SymbolName", search_target="definitions")`
- For concepts: `fast_search(query="what does error handling look like")` (semantic search handles this)

- [ ] **Step 3: Update explore-area skill**

The explore-area skill recommends `get_context` for orientation. Add a note that conceptual queries work well for get_context too, not just symbol names.

- [ ] **Step 4: Update logic-flow and call-trace skills**

If users describe behavior ("how does payment processing work") rather than naming symbols, suggest starting with a conceptual `fast_search` to find the entry point before tracing.

- [ ] **Step 5: Commit**

```bash
git add <modified skill files and server instruction files>
git commit -m "docs(plugin): update workflow instructions for improved semantic search"
```

---

### Task 11: Run Dev Test Tier

**Files:** None (validation only)

- [ ] **Step 1: Run cargo xtask test dev**

Run: `cargo xtask test dev`
Expected: All green. No regressions.

- [ ] **Step 2: If failures, fix and re-run**

Diagnose any failures. The most likely source is test assertions that expect exact embedding text format and weren't updated.

- [ ] **Step 3: Final commit if any fixes**

```bash
git add -u
git commit -m "fix(tests): resolve test regressions from embedding enrichment"
```
