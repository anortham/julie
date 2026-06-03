//! RED tests for deterministic query expansion primitives.
//!
//! These tests intentionally fail until `crate::search::expansion` is added.

use std::collections::HashSet;

#[test]
fn test_phrase_alias_expands_workspace_routing() {
    let expanded = crate::search::expansion::expand_query_terms("workspace routing");

    assert!(
        expanded.alias_terms.iter().any(|term| term == "router"),
        "Expected alias expansion for 'workspace routing' to include 'router': {:?}",
        expanded.alias_terms
    );
    assert!(
        expanded.alias_terms.iter().any(|term| term == "registry"),
        "Expected alias expansion for 'workspace routing' to include 'registry': {:?}",
        expanded.alias_terms
    );
}

#[test]
fn test_alias_terms_are_deduped_against_original_terms() {
    let expanded =
        crate::search::expansion::expand_query_terms("workspace routing router registry");

    let original: HashSet<_> = expanded.original_terms.iter().cloned().collect();
    let alias: HashSet<_> = expanded.alias_terms.iter().cloned().collect();

    assert_eq!(
        expanded.alias_terms.len(),
        alias.len(),
        "Alias terms should not contain duplicates: {:?}",
        expanded.alias_terms
    );

    for alias_term in alias {
        assert!(
            !original.contains(&alias_term),
            "Alias term '{alias_term}' should not duplicate an original query term: {:?}",
            expanded.original_terms
        );
    }
}

#[test]
fn test_max_added_term_cap_is_respected() {
    let expanded = crate::search::expansion::expand_query_terms(
        "workspace routing symbol extraction dependency graph call trace index refresh semantic search reference lookup",
    );

    let added_term_count = expanded.alias_terms.len() + expanded.normalized_terms.len();
    assert_eq!(
        added_term_count,
        crate::search::expansion::MAX_ADDED_TERMS,
        "Stress query should saturate MAX_ADDED_TERMS ({}), otherwise this test does not prove cap enforcement: alias={:?}, normalized={:?}",
        crate::search::expansion::MAX_ADDED_TERMS,
        expanded.alias_terms,
        expanded.normalized_terms
    );

    assert!(
        expanded.alias_terms.iter().any(|term| term == "router"),
        "Capping should retain deterministic high-priority aliases from 'workspace routing': {:?}",
        expanded.alias_terms
    );
}

#[test]
fn test_normalization_skips_noisy_ing_and_plural_endings() {
    let expanded = crate::search::expansion::expand_query_terms("thing process class routing");

    assert!(
        expanded.normalized_terms.is_empty(),
        "Expected no noisy normalization for edge-case words, got: {:?}",
        expanded.normalized_terms
    );
}

#[test]
fn test_normalization_does_not_emit_malformed_stems() {
    let expanded = crate::search::expansion::expand_query_terms("thing process class routing");

    let malformed = ["th", "proce", "cla", "rout"];
    for stem in malformed {
        assert!(
            !expanded.normalized_terms.iter().any(|term| term == stem),
            "Normalization should not emit malformed stem '{stem}': {:?}",
            expanded.normalized_terms
        );
    }
}
