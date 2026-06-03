//! Compat marker v4 — stale-detection and acceptance tests.
//!
//! Verifies:
//! - A workspace with a `marker_version: 3` marker file is treated as stale
//!   and triggers `RecreatedIncompatible`.
//! - A freshly-created index returns `Compatible`.

use tempfile::TempDir;

use crate::search::index::{SEARCH_COMPAT_MARKER_FILE, SearchIndex, SearchIndexOpenDisposition};
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
// concurrent_rebuild_both_succeed  (Codex finding #3)
// ---------------------------------------------------------------------------

/// Two threads that both open the same stale-marker index concurrently must
/// both succeed and end up with a valid, compatible index.
///
/// The previous implementation used a `.recreating` sentinel INSIDE the index
/// directory.  `recreate_index` calls `remove_dir_all`, which deletes the
/// sentinel, so the second thread saw `AlreadyExists` on the sentinel, tried
/// to open a directory that was being torn down, and raced to an error.
///
/// The fix moves the advisory lock to the PARENT directory (a stable sibling
/// file that survives `remove_dir_all`) and makes the winner wait before the
/// loser even starts, so the loser can re-check compatibility and return early.
#[test]
fn concurrent_rebuild_both_succeed() {
    use std::sync::Arc;

    let temp_dir = tempfile::TempDir::new().unwrap();
    // The index lives INSIDE the temp dir so its parent (temp_dir.path())
    // is a stable directory that won't be deleted during recreation.
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    // Step 1: create a valid fresh index.
    let configs = LanguageConfigs::load_embedded();
    SearchIndex::create_with_language_configs(&index_path, &configs).unwrap();

    // Step 2: stomp the marker to force incompatibility on both threads.
    write_stale_v3_marker(&index_path);

    // Step 3: launch two threads that both open the incompatible index.
    // A barrier maximises overlap: both threads reach open() at the same time.
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let path = Arc::new(index_path.clone());

    let b1 = barrier.clone();
    let p1 = path.clone();
    let t1 = std::thread::spawn(move || {
        let cfgs = LanguageConfigs::load_embedded();
        b1.wait();
        SearchIndex::open_or_create_with_language_configs_outcome(&p1, &cfgs)
    });

    let b2 = barrier.clone();
    let p2 = path.clone();
    let t2 = std::thread::spawn(move || {
        let cfgs = LanguageConfigs::load_embedded();
        b2.wait();
        SearchIndex::open_or_create_with_language_configs_outcome(&p2, &cfgs)
    });

    let r1 = t1
        .join()
        .expect("thread 1 panicked")
        .expect("thread 1 returned error");
    let r2 = t2
        .join()
        .expect("thread 2 panicked")
        .expect("thread 2 returned error");

    // At least one thread must have performed a rebuild.
    let any_rebuilt = matches!(
        r1.disposition,
        SearchIndexOpenDisposition::RecreatedIncompatible
            | SearchIndexOpenDisposition::RecreatedOpenFailure
    ) || matches!(
        r2.disposition,
        SearchIndexOpenDisposition::RecreatedIncompatible
            | SearchIndexOpenDisposition::RecreatedOpenFailure
    );
    assert!(
        any_rebuilt,
        "at least one thread must have triggered a rebuild"
    );

    // The index must now be compatible when re-opened.
    let outcome =
        SearchIndex::open_or_create_with_language_configs_outcome(&index_path, &configs).unwrap();
    assert_eq!(
        outcome.disposition,
        SearchIndexOpenDisposition::Compatible,
        "index must be compatible after concurrent rebuild"
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
