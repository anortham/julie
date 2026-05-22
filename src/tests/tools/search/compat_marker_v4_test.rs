//! Compat marker v4 — stale-detection and acceptance tests.
//!
//! Verifies:
//! - A workspace with a `marker_version: 3` marker file is treated as stale
//!   and triggers `RecreatedIncompatible`.
//! - A freshly-created index returns `Compatible`.

use tempfile::TempDir;

use crate::search::index::{SearchIndex, SearchIndexOpenDisposition, SEARCH_COMPAT_MARKER_FILE};
use crate::search::language_config::LanguageConfigs;

// ---------------------------------------------------------------------------
// Helper: write a hand-crafted v3 marker (stale) over an existing marker file.
// ---------------------------------------------------------------------------

fn write_stale_v3_marker(dir: &std::path::Path) {
    // Write a structurally-valid SearchCompatMarker JSON with marker_version 3
    // and empty signatures. The version mismatch (3 != 4) causes
    // `index_is_compatible` to return false → RecreatedIncompatible.
    let stale = serde_json::json!({
        "marker_version": 3,
        "schema_signature": { "fields": [] },
        "tokenizer_signature": {
            "preserve_patterns": [],
            "meaningful_affixes": [],
            "strip_prefixes": [],
            "strip_suffixes": [],
            "ablate_stemming": false,
            "ablate_camel_emit": false
        }
    });
    let path = dir.join(SEARCH_COMPAT_MARKER_FILE);
    std::fs::write(&path, serde_json::to_string_pretty(&stale).unwrap()).unwrap();
}

// ---------------------------------------------------------------------------
// detects_v3_marker_as_stale
// ---------------------------------------------------------------------------

#[test]
fn detects_v3_marker_as_stale() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    // Step 1: create a fresh index using the language-configs path so the
    // tokenizer signature is consistent with the re-open below.
    let configs = LanguageConfigs::load_embedded();
    SearchIndex::create_with_language_configs(path, &configs).unwrap();

    // Step 2: stomp the marker file with a v3 marker.
    write_stale_v3_marker(path);

    // Step 3: re-open with the same configs — must detect the marker mismatch
    // and return RecreatedIncompatible.
    let outcome =
        SearchIndex::open_or_create_with_language_configs_outcome(path, &configs).unwrap();
    assert_eq!(
        outcome.disposition,
        SearchIndexOpenDisposition::RecreatedIncompatible,
        "opening an index with a v3 marker must return RecreatedIncompatible after bump to v4"
    );
}

// ---------------------------------------------------------------------------
// accepts_v4_marker
// ---------------------------------------------------------------------------

#[test]
fn accepts_v4_marker() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path();

    // A freshly-created index writes the current marker version (v4 after the
    // bump). Re-opening with the same tokenizer must return Compatible.
    let configs = LanguageConfigs::load_embedded();
    SearchIndex::create_with_language_configs(path, &configs).unwrap();

    let outcome =
        SearchIndex::open_or_create_with_language_configs_outcome(path, &configs).unwrap();
    assert_eq!(
        outcome.disposition,
        SearchIndexOpenDisposition::Compatible,
        "opening a freshly-created index must return Compatible"
    );
}
