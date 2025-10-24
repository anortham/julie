# Julie Development TODO

## <¯ Current Focus: Semantic Search Scoring Enhancement

**Goal:** Make doc-comment-rich symbols rank higher than generic framework boilerplate

**Problem:**
- Query: "preview MJML templates locally"
- Current: Generic Razor `<Template>` tags rank above documented `EmailTemplatePreview` class
- Root cause: Scoring doesn't consider doc comment presence or symbol quality

---

## TDD Implementation Plan: Multi-Factor Scoring

### Phase 1: Write Failing Tests   RED

**Test File:** `src/tests/tools/search/semantic_scoring_tests.rs`

#### Test 1: Doc Comment Boost
```rust
#[test]
fn test_doc_comment_boost_calculation() {
    // Symbol with rich documentation (200+ chars)
    let symbol_with_rich_docs = create_symbol_with_doc("/// ".repeat(250));
    assert_eq!(get_doc_comment_boost(&symbol_with_rich_docs), 2.0);

    // Symbol with good documentation (100-200 chars)
    let symbol_with_good_docs = create_symbol_with_doc("/// ".repeat(150));
    assert_eq!(get_doc_comment_boost(&symbol_with_good_docs), 1.5);

    // Symbol with some documentation (<100 chars)
    let symbol_with_some_docs = create_symbol_with_doc("/// Short doc");
    assert_eq!(get_doc_comment_boost(&symbol_with_some_docs), 1.3);

    // Symbol with no documentation
    let symbol_no_docs = create_symbol_with_doc(None);
    assert_eq!(get_doc_comment_boost(&symbol_no_docs), 1.0);
}
```

#### Test 2: Language Quality Boost
```rust
#[test]
fn test_language_quality_boost() {
    // Real code languages
    assert_eq!(get_language_quality_boost("csharp", false), 1.2);
    assert_eq!(get_language_quality_boost("rust", false), 1.2);

    // HTML elements get penalty
    assert_eq!(get_language_quality_boost("razor", true), 0.7);

    // Razor C# code (not HTML) is normal
    assert_eq!(get_language_quality_boost("razor", false), 1.0);
}
```

#### Test 3: Generic Symbol Detection
```rust
#[test]
fn test_generic_symbol_detection() {
    // Generic name + no docs = generic
    let template_no_docs = create_symbol("Template", None);
    assert!(is_generic_symbol(&template_no_docs));

    // Generic name + HAS docs = NOT generic
    let template_with_docs = create_symbol("Template", Some("/// Docs"));
    assert!(!is_generic_symbol(&template_with_docs));

    // Non-generic name + no docs = NOT generic
    let specific_no_docs = create_symbol("EmailTemplatePreview", None);
    assert!(!is_generic_symbol(&specific_no_docs));
}
```

#### Test 4: End-to-End Scoring
```rust
#[test]
fn test_documented_class_beats_generic_html() {
    // EmailTemplatePreview (C# class with docs)
    let documented_class = Symbol {
        name: "EmailTemplatePreview",
        language: "csharp",
        kind: SymbolKind::Class,
        doc_comment: Some("/// <summary>\n/// Simple utility class to preview MJML email templates locally\n/// Run this to generate HTML files for testing without deployment\n/// </summary>"),
        metadata: HashMap::new(),
        ..Default::default()
    };

    // Razor Template tag (HTML element, no docs)
    let mut html_metadata = HashMap::new();
    html_metadata.insert("type", "html-element");
    let generic_html = Symbol {
        name: "Template",
        language: "razor",
        kind: SymbolKind::Class,
        doc_comment: None,
        metadata: html_metadata,
        ..Default::default()
    };

    // Both start with same base semantic score
    let base_score = 0.8;

    let class_final = apply_all_boosts(&documented_class, base_score);
    let html_final = apply_all_boosts(&generic_html, base_score);

    // Documented class should score significantly higher (4x+)
    assert!(class_final > html_final * 4.0);
}
```

**Expected:** All tests FAIL (functions don't exist yet)

---

### Phase 2: Implement Helper Functions  GREEN

**File:** `src/tools/search/semantic_search.rs`

#### Step 2.1: Add `get_doc_comment_boost()`
```rust
/// Boost symbols with documentation
fn get_doc_comment_boost(symbol: &Symbol) -> f32 {
    match &symbol.doc_comment {
        None => 1.0,
        Some(doc) if doc.is_empty() => 1.0,
        Some(doc) => {
            let doc_len = doc.len();
            if doc_len > 200 {
                2.0  // Rich documentation
            } else if doc_len > 100 {
                1.5  // Good documentation
            } else {
                1.3  // Some documentation
            }
        }
    }
}
```

**Test:** `cargo test test_doc_comment_boost_calculation` ’ should PASS

#### Step 2.2: Add `is_html_element()` helper
```rust
/// Check if symbol is an HTML element (not real code)
fn is_html_element(symbol: &Symbol) -> bool {
    symbol.kind == SymbolKind::Class
        && symbol.metadata
            .get("type")
            .and_then(|v| v.as_str())
            .map(|s| s == "html-element")
            .unwrap_or(false)
}
```

#### Step 2.3: Add `get_language_quality_boost()`
```rust
/// Boost real code over markup/templates
fn get_language_quality_boost(symbol: &Symbol) -> f32 {
    match symbol.language.as_str() {
        // Real code languages - high signal
        "csharp" | "rust" | "typescript" | "java" | "kotlin" => 1.2,

        // Scripting languages - good signal
        "javascript" | "python" | "ruby" | "php" => 1.1,

        // Markup/templating - context dependent
        "razor" | "vue" | "html" => {
            if is_html_element(symbol) {
                0.7  // HTML tag penalty
            } else {
                1.0  // Razor C# code is normal
            }
        }

        _ => 1.0
    }
}
```

**Test:** `cargo test test_language_quality_boost` ’ should PASS

#### Step 2.4: Add `is_generic_symbol()` and `get_generic_penalty()`
```rust
/// Check if symbol is generic framework boilerplate
fn is_generic_symbol(symbol: &Symbol) -> bool {
    const GENERIC_NAMES: &[&str] = &[
        "Template", "Container", "Wrapper", "Item",
        "Data", "Value", "Component", "Element"
    ];

    // Only penalize if BOTH generic name AND no documentation
    symbol.doc_comment.is_none()
        && GENERIC_NAMES.contains(&symbol.name.as_str())
}

/// Penalize generic undocumented symbols
fn get_generic_penalty(symbol: &Symbol) -> f32 {
    if is_generic_symbol(symbol) {
        0.5  // 50% penalty
    } else {
        1.0
    }
}
```

**Test:** `cargo test test_generic_symbol_detection` ’ should PASS

---

### Phase 3: Integrate into Scoring  GREEN

**File:** `src/tools/search/semantic_search.rs` (lines 328-352)

#### Current Code:
```rust
// Apply quality scoring to rerank results
let mut scored_symbols: Vec<(Symbol, f32)> = symbols
    .into_iter()
    .zip(semantic_results.iter())
    .map(|(symbol, result)| {
        let mut score = result.similarity_score;

        // Apply symbol kind boosting
        score *= get_symbol_kind_boost(&symbol);

        // Heavily downrank vendor symbols (95% penalty)
        if is_vendor_symbol(&symbol.file_path) {
            score *= 0.05;
        }

        (symbol, score)
    })
    .collect();
```

#### Updated Code:
```rust
// Apply multi-factor quality scoring to rerank results
let mut scored_symbols: Vec<(Symbol, f32)> = symbols
    .into_iter()
    .zip(semantic_results.iter())
    .map(|(symbol, result)| {
        let mut score = result.similarity_score;

        // Factor 1: Symbol kind boosting (existing)
        score *= get_symbol_kind_boost(&symbol);

        // Factor 2: Doc comment boost (NEW)
        score *= get_doc_comment_boost(&symbol);

        // Factor 3: Language quality boost (NEW)
        score *= get_language_quality_boost(&symbol);

        // Factor 4: Generic symbol penalty (NEW)
        score *= get_generic_penalty(&symbol);

        // Factor 5: Vendor penalty (existing)
        if is_vendor_symbol(&symbol.file_path) {
            score *= 0.05;
        }

        (symbol, score)
    })
    .collect();

// Re-sort by adjusted scores
scored_symbols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
```

**Test:** `cargo test test_documented_class_beats_generic_html` ’ should PASS

---

### Phase 4: Integration Testing >ê

#### Test with Real Workspace
```bash
# Build release
cargo build --release

# Restart Claude Code

# Re-index coa-intranet workspace

# Test queries:
# 1. "email templates" ’ EmailTemplatePreview should be #1
# 2. "preview MJML templates" ’ EmailTemplatePreview should be #1
# 3. "user authentication" ’ Documented auth classes should be top
```

**Expected Results:**
-  Documented C# classes rank above generic HTML tags
-  Confidence scores improve (0.5 ’ 0.6-0.7+)
-  No false negatives (legitimate HTML still findable)

---

### Phase 5: Refactor & Document ='

**After all tests pass:**

1. **Extract scoring helpers** to separate module
   - `src/tools/search/scoring/multi_factor.rs`
   - Clean separation of concerns

2. **Add configuration** for tuning
   ```rust
   pub struct ScoringConfig {
       pub rich_doc_boost: f32,      // Default: 2.0
       pub good_doc_boost: f32,      // Default: 1.5
       pub some_doc_boost: f32,      // Default: 1.3
       pub html_element_penalty: f32, // Default: 0.7
       pub generic_penalty: f32,      // Default: 0.5
   }
   ```

3. **Document pattern** for other languages
   - Comment each factor with rationale
   - Explain how to replicate for TypeScript, Python, etc.

---

## Progress Checklist

- [ ] Write failing tests (Phase 1)
- [ ] Implement `get_doc_comment_boost()` + test
- [ ] Implement `get_language_quality_boost()` + test
- [ ] Implement `is_generic_symbol()` + test
- [ ] Integrate all factors into scoring
- [ ] Run end-to-end test
- [ ] Integration test with real workspace
- [ ] Verify EmailTemplatePreview ranks #1
- [ ] Refactor & document
- [ ] Create pattern template for other languages

---

## Next Languages to Apply Pattern

After C# scoring is perfected:
1. **TypeScript/JavaScript** (JSDoc comments `/** ... */`)
2. **Python** (docstrings `"""..."""`)
3. **Rust** (doc comments `///...`)
4. **Java** (JavaDoc `/** ... */`)
5. Remaining 21 languages...

---

## Success Metrics

**Before:**
- Query: "email templates" ’ Generic Razor tags rank #1, #2
- EmailTemplatePreview (documented) ranks #3
- Confidence: 0.5

**After:**
- Query: "email templates" ’ EmailTemplatePreview ranks #1
- Generic HTML tags rank below documented classes
- Confidence: 0.6-0.7+
- 4x+ score difference between documented vs generic symbols

---

## Notes

- **TDD discipline:** RED ’ GREEN ’ REFACTOR cycle
- **No false negatives:** HTML elements still findable, just deprioritized
- **Replicable pattern:** Same formula works for all 25 languages
- **Evidence-based:** Doc comments signal developer intent and domain value
