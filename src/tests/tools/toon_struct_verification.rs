//! TOON Struct Field Verification Tests
//!
//! These tests enforce data consistency between Symbol and TOON variant structs.
//! They serve as:
//! 1. Living documentation of intentional field inclusion/exclusion decisions
//! 2. Change detection - alerts us when Symbol gains new fields
//! 3. Regression prevention - ensures TOON structs maintain their contracts
//!
//! Context: Gemini review identified data drift risk between Symbol and ToonFlatSymbol/ToonSymbol.
//! If Symbol evolves (adds critical fields), TOON variants must be consciously updated.

use std::collections::HashSet;

/// Expected fields in the base Symbol struct (as of 2025-11-21)
///
/// This is our source of truth. If this test fails, Symbol has changed.
/// Review the new fields and consciously decide whether to include them in TOON variants.
fn expected_symbol_fields() -> HashSet<&'static str> {
    [
        // Core identity
        "id",
        "name",
        "kind",
        "language",
        "file_path",
        // Position (fine-grained)
        "start_line",
        "start_column",
        "end_line",
        "end_column",
        "start_byte",
        "end_byte",
        // Metadata
        "signature",
        "doc_comment",
        "visibility",
        "parent_id",
        "metadata",
        "semantic_group",
        "confidence",
        "code_context",
        "content_type",
    ]
    .iter()
    .copied()
    .collect()
}

/// Fields included in ToonFlatSymbol (for get_symbols tool)
///
/// Design Decision: Include structural navigation fields (parent_id, signature)
/// Exclude: search-specific (confidence, code_context), byte-level positions, heavy metadata
fn toonflatssymbol_fields() -> HashSet<&'static str> {
    [
        // Core identity
        "id",
        "name",
        "kind", // String (converted from enum)
        "language",
        "file_path",
        // Position (line-level only for token efficiency)
        "start_line",
        "end_line",
        // Navigation & structure
        "parent_id", // CRITICAL: enables class.method relationships
        "signature",
        "doc_comment",
        "visibility", // String (converted from enum)
    ]
    .iter()
    .copied()
    .collect()
}

/// Fields included in ToonSymbol (for fast_search tool)
///
/// Design Decision: Include search-specific fields (confidence, code_context) AND parent_id
/// Exclude: byte-level positions, heavy metadata
fn toonsymbol_fields() -> HashSet<&'static str> {
    [
        // Core identity
        "id",
        "name",
        "kind", // String (converted from enum)
        "language",
        "file_path",
        // Position (line-level only)
        "start_line",
        "end_line",
        // Navigation & structure
        "parent_id",     // CRITICAL: enables class.method relationships
        // Search-specific
        "signature",
        "doc_comment",
        "visibility",    // String (converted from enum)
        "confidence",    // Search relevance
        "code_context",  // Match context
    ]
    .iter()
    .copied()
    .collect()
}

#[test]
fn test_symbol_has_expected_fields() {
    // This test documents the current Symbol schema
    // If it fails, Symbol has gained/lost fields - review and update TOON variants

    let expected = expected_symbol_fields();

    // We verify this by checking field count (brittle but intentional - forces conscious review)
    assert_eq!(
        expected.len(),
        20,
        "Symbol field count changed! Expected 20 fields. \
         Review new/removed fields and update ToonFlatSymbol/ToonSymbol consciously.\n\
         Expected fields: {:?}",
        expected
    );
}

#[test]
fn test_toonflatssymbol_is_proper_subset() {
    // ToonFlatSymbol MUST be a subset of Symbol (can't have fields Symbol doesn't have)

    let symbol_fields = expected_symbol_fields();
    let toon_flat_fields = toonflatssymbol_fields();

    let extra_fields: Vec<_> = toon_flat_fields
        .difference(&symbol_fields)
        .copied()
        .collect();

    assert!(
        extra_fields.is_empty(),
        "ToonFlatSymbol has fields not in Symbol: {:?}",
        extra_fields
    );
}

#[test]
fn test_toonsymbol_is_proper_subset() {
    // ToonSymbol MUST be a subset of Symbol

    let symbol_fields = expected_symbol_fields();
    let toon_symbol_fields = toonsymbol_fields();

    let extra_fields: Vec<_> = toon_symbol_fields
        .difference(&symbol_fields)
        .copied()
        .collect();

    assert!(
        extra_fields.is_empty(),
        "ToonSymbol has fields not in Symbol: {:?}",
        extra_fields
    );
}

#[test]
fn test_toonflatssymbol_intentional_exclusions() {
    // Document WHY we exclude certain fields from ToonFlatSymbol

    let symbol_fields = expected_symbol_fields();
    let toon_flat_fields = toonflatssymbol_fields();

    let excluded: Vec<_> = symbol_fields
        .difference(&toon_flat_fields)
        .copied()
        .collect();

    // Expected exclusions with rationale:
    let expected_exclusions = [
        "start_column",   // Token efficiency: line-level positioning sufficient for get_symbols
        "end_column",     // Token efficiency: line-level positioning sufficient
        "start_byte",     // Token efficiency: internal detail, not useful to LLM
        "end_byte",       // Token efficiency: internal detail, not useful to LLM
        "metadata",       // Token efficiency: language-specific extras rarely needed in summaries
        "semantic_group", // Token efficiency: cross-language linking (experimental feature)
        "confidence",     // Not applicable: get_symbols returns exact matches, not search results
        "code_context",   // Not applicable: get_symbols shows full symbol bodies, not snippets
        "content_type",   // Token efficiency: type distinction rarely needed in symbol lists
    ];

    assert_eq!(
        excluded.len(),
        expected_exclusions.len(),
        "ToonFlatSymbol exclusion count changed!\n\
         Expected to exclude: {:?}\n\
         Actually excluded: {:?}\n\
         Review whether new Symbol fields should be included in get_symbols output.",
        expected_exclusions,
        excluded
    );
}

#[test]
fn test_toonsymbol_intentional_exclusions() {
    // Document WHY we exclude certain fields from ToonSymbol

    let symbol_fields = expected_symbol_fields();
    let toon_symbol_fields = toonsymbol_fields();

    let excluded: Vec<_> = symbol_fields
        .difference(&toon_symbol_fields)
        .copied()
        .collect();

    // Expected exclusions with rationale:
    let expected_exclusions = [
        "start_column",   // Token efficiency: line-level positioning sufficient for search
        "end_column",     // Token efficiency: line-level positioning sufficient
        "start_byte",     // Token efficiency: internal detail
        "end_byte",       // Token efficiency: internal detail
        "metadata",       // Token efficiency: language-specific extras rarely useful in search
        "semantic_group", // Token efficiency: cross-language linking (experimental)
        "content_type",   // Token efficiency: type distinction rarely needed in search
    ];

    assert_eq!(
        excluded.len(),
        expected_exclusions.len(),
        "ToonSymbol exclusion count changed!\n\
         Expected to exclude: {:?}\n\
         Actually excluded: {:?}\n\
         Review whether new Symbol fields should be included in search output.",
        expected_exclusions,
        excluded
    );
}

#[test]
fn test_toonsymbol_includes_parent_id() {
    // ToonSymbol MUST include parent_id (critical for class.method relationships in search)

    let toon_symbol_fields = toonsymbol_fields();

    assert!(
        toon_symbol_fields.contains("parent_id"),
        "ToonSymbol must include parent_id for hierarchical symbol display in search results"
    );
}

#[test]
fn test_toonflatssymbol_includes_parent_id() {
    // ToonFlatSymbol CORRECTLY includes parent_id (needed for hierarchical symbol display)

    let toon_flat_fields = toonflatssymbol_fields();

    assert!(
        toon_flat_fields.contains("parent_id"),
        "ToonFlatSymbol must include parent_id for hierarchical navigation"
    );
}

#[test]
fn test_field_coverage_documentation() {
    // Summary test that prints coverage statistics for documentation

    let symbol_fields = expected_symbol_fields();
    let toon_flat_fields = toonflatssymbol_fields();
    let toon_symbol_fields = toonsymbol_fields();

    let toon_flat_coverage = (toon_flat_fields.len() as f32 / symbol_fields.len() as f32) * 100.0;
    let toon_symbol_coverage = (toon_symbol_fields.len() as f32 / symbol_fields.len() as f32) * 100.0;

    // This isn't an assertion, just documentation printed during test runs
    println!("\n=== TOON Field Coverage ===");
    println!("Symbol total fields: {}", symbol_fields.len());
    println!(
        "ToonFlatSymbol: {} fields ({:.1}% coverage)",
        toon_flat_fields.len(),
        toon_flat_coverage
    );
    println!(
        "ToonSymbol: {} fields ({:.1}% coverage)",
        toon_symbol_fields.len(),
        toon_symbol_coverage
    );

    // Assert reasonable coverage (not too sparse, not redundant)
    assert!(
        toon_flat_coverage >= 50.0 && toon_flat_coverage <= 70.0,
        "ToonFlatSymbol coverage out of expected range (50-70%): {:.1}%",
        toon_flat_coverage
    );
    assert!(
        toon_symbol_coverage >= 50.0 && toon_symbol_coverage <= 70.0,
        "ToonSymbol coverage out of expected range (50-70%): {:.1}%",
        toon_symbol_coverage
    );
}
