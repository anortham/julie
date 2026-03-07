//! Tests for the federation module: RRF merge and federated search.
//!
//! RRF merge tests use synthetic data (no workspace needed).
//! Federated search tests create temporary workspaces with indexed content
//! to verify parallel fan-out and result merging.

use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use crate::search::index::{
    ContentSearchResult, FileDocument, SearchFilter, SearchIndex, SymbolDocument,
    SymbolSearchResult,
};
use crate::tools::federation::rrf::{multi_rrf_merge, RrfItem, RRF_K};
use crate::tools::federation::search::{
    FederatedContentResult, FederatedSymbolResult, WorkspaceSearchEntry,
    federated_content_search, federated_symbol_search,
};

// ===========================================================================
// RRF merge with TestItem (pure algorithm tests)
// ===========================================================================

/// Simple test item for RRF unit tests.
#[derive(Debug, Clone)]
struct TestItem {
    id: String,
    score: f32,
}

impl TestItem {
    fn new(id: &str, score: f32) -> Self {
        Self {
            id: id.to_string(),
            score,
        }
    }
}

impl RrfItem for TestItem {
    fn rrf_id(&self) -> &str {
        &self.id
    }

    fn set_score(&mut self, score: f32) {
        self.score = score;
    }

    fn score(&self) -> f32 {
        self.score
    }
}

#[test]
fn test_rrf_empty_lists() {
    let lists: Vec<Vec<TestItem>> = vec![];
    let result = multi_rrf_merge(lists, RRF_K, 10);
    assert!(result.is_empty());
}

#[test]
fn test_rrf_all_empty_lists() {
    let lists: Vec<Vec<TestItem>> = vec![vec![], vec![], vec![]];
    let result = multi_rrf_merge(lists, RRF_K, 10);
    assert!(result.is_empty());
}

#[test]
fn test_rrf_single_list_passthrough() {
    let lists = vec![vec![
        TestItem::new("a", 1.0),
        TestItem::new("b", 0.8),
        TestItem::new("c", 0.6),
    ]];
    let result = multi_rrf_merge(lists, RRF_K, 10);
    assert_eq!(result.len(), 3);
    // Single list: items keep their order, scores normalized to RRF values
    assert_eq!(result[0].id, "a");
    assert_eq!(result[1].id, "b");
    assert_eq!(result[2].id, "c");
    // Scores should be RRF: 1/(k+rank) for rank 1,2,3
    let expected_scores = [1.0 / 61.0, 1.0 / 62.0, 1.0 / 63.0];
    for (i, expected) in expected_scores.iter().enumerate() {
        assert!(
            (result[i].score - expected).abs() < 1e-6,
            "Item {} expected score {}, got {}",
            result[i].id,
            expected,
            result[i].score,
        );
    }
}

#[test]
fn test_rrf_single_list_respects_limit() {
    let lists = vec![vec![
        TestItem::new("a", 1.0),
        TestItem::new("b", 0.8),
        TestItem::new("c", 0.6),
    ]];
    let result = multi_rrf_merge(lists, RRF_K, 2);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_rrf_disjoint_lists() {
    // Three lists with no overlap
    let lists = vec![
        vec![TestItem::new("a", 1.0)],
        vec![TestItem::new("b", 1.0)],
        vec![TestItem::new("c", 1.0)],
    ];
    let result = multi_rrf_merge(lists, RRF_K, 10);
    assert_eq!(result.len(), 3);
    // All at rank 1 in their respective lists, so all have same RRF score
    let scores: Vec<f32> = result.iter().map(|r| r.score).collect();
    assert!((scores[0] - scores[1]).abs() < f32::EPSILON);
    assert!((scores[1] - scores[2]).abs() < f32::EPSILON);
    // Score should be 1/(k+1) = 1/61
    let expected = 1.0 / 61.0;
    assert!((scores[0] - expected).abs() < f32::EPSILON);
}

#[test]
fn test_rrf_overlapping_items_get_boosted() {
    // Item "a" appears at rank 1 in all 3 lists -- should get highest score
    // Item "b" appears at rank 2 in list 1 only
    // Item "c" appears at rank 2 in list 2 and rank 2 in list 3
    let lists = vec![
        vec![TestItem::new("a", 1.0), TestItem::new("b", 0.8)],
        vec![TestItem::new("a", 0.9), TestItem::new("c", 0.7)],
        vec![TestItem::new("a", 0.8), TestItem::new("c", 0.6)],
    ];
    let result = multi_rrf_merge(lists, RRF_K, 10);

    // "a" should be first (appears in all 3 lists at rank 1)
    assert_eq!(result[0].id, "a");
    // "a" score = 3 * 1/(60+1) = 3/61
    let expected_a = 3.0 / 61.0;
    assert!(
        (result[0].score - expected_a).abs() < 1e-6,
        "Expected a score ~{}, got {}",
        expected_a,
        result[0].score
    );

    // "c" should be second (rank 2 in list 2 + rank 2 in list 3)
    assert_eq!(result[1].id, "c");
    let expected_c = 1.0 / 62.0 + 1.0 / 62.0;
    assert!(
        (result[1].score - expected_c).abs() < 1e-6,
        "Expected c score ~{}, got {}",
        expected_c,
        result[1].score
    );

    // "b" should be last (only rank 2 in list 1)
    assert_eq!(result[2].id, "b");
    let expected_b = 1.0 / 62.0;
    assert!(
        (result[2].score - expected_b).abs() < 1e-6,
        "Expected b score ~{}, got {}",
        expected_b,
        result[2].score
    );
}

#[test]
fn test_rrf_merge_respects_limit() {
    let lists = vec![
        vec![
            TestItem::new("a", 1.0),
            TestItem::new("b", 0.9),
            TestItem::new("c", 0.8),
        ],
        vec![
            TestItem::new("d", 1.0),
            TestItem::new("e", 0.9),
            TestItem::new("f", 0.8),
        ],
    ];
    let result = multi_rrf_merge(lists, RRF_K, 3);
    assert_eq!(result.len(), 3);
}

#[test]
fn test_rrf_five_lists_merge() {
    // Simulate 5 workspace results
    let lists = vec![
        vec![TestItem::new("shared", 1.0), TestItem::new("ws1", 0.5)],
        vec![TestItem::new("shared", 0.9), TestItem::new("ws2", 0.5)],
        vec![TestItem::new("shared", 0.8), TestItem::new("ws3", 0.5)],
        vec![TestItem::new("ws4_only", 1.0)],
        vec![TestItem::new("shared", 0.7), TestItem::new("ws5", 0.5)],
    ];
    let result = multi_rrf_merge(lists, RRF_K, 10);

    // "shared" appears in 4 of 5 lists at rank 1 -- should be top
    assert_eq!(result[0].id, "shared");
    let expected_shared = 4.0 / 61.0;
    assert!(
        (result[0].score - expected_shared).abs() < 1e-6,
        "Expected shared score ~{}, got {}",
        expected_shared,
        result[0].score
    );
}

#[test]
fn test_rrf_custom_k_value() {
    let lists = vec![
        vec![TestItem::new("a", 1.0)],
        vec![TestItem::new("a", 1.0)],
    ];
    // k=10 instead of 60
    let result = multi_rrf_merge(lists, 10, 10);
    assert_eq!(result.len(), 1);
    let expected = 2.0 / 11.0; // 2 * 1/(10+1)
    assert!(
        (result[0].score - expected).abs() < 1e-6,
        "Expected score ~{}, got {}",
        expected,
        result[0].score
    );
}

// ===========================================================================
// RRF merge with FederatedResult types (real types, no I/O)
// ===========================================================================

/// Helper: create a FederatedSymbolResult for testing RRF with real types.
fn make_symbol_result(id: &str, name: &str, score: f32, ws_id: &str) -> FederatedSymbolResult {
    FederatedSymbolResult::new(
        SymbolSearchResult {
            id: id.to_string(),
            name: name.to_string(),
            signature: String::new(),
            doc_comment: String::new(),
            file_path: format!("src/{}.rs", name),
            kind: "function".to_string(),
            language: "rust".to_string(),
            start_line: 1,
            score,
        },
        ws_id.to_string(),
        format!("project-{}", ws_id),
    )
}

/// Helper: create a FederatedContentResult for testing RRF with real types.
fn make_content_result(
    file_path: &str,
    score: f32,
    ws_id: &str,
) -> FederatedContentResult {
    FederatedContentResult::new(
        ContentSearchResult {
            file_path: file_path.to_string(),
            language: "rust".to_string(),
            score,
        },
        ws_id.to_string(),
        format!("project-{}", ws_id),
    )
}

#[test]
fn test_rrf_merge_federated_symbol_results() {
    // Simulate 3 workspaces returning symbol results
    let ws1_results = vec![
        make_symbol_result("sym1", "parse_query", 0.9, "ws1"),
        make_symbol_result("sym2", "build_ast", 0.7, "ws1"),
    ];
    let ws2_results = vec![
        make_symbol_result("sym3", "parse_query", 0.85, "ws2"),
        make_symbol_result("sym4", "execute_query", 0.6, "ws2"),
    ];
    let ws3_results = vec![
        make_symbol_result("sym5", "parse_query", 0.8, "ws3"),
    ];

    let lists = vec![ws1_results, ws2_results, ws3_results];
    let merged = multi_rrf_merge(lists, RRF_K, 10);

    // All 5 unique items (different global IDs since different workspace prefixes)
    assert_eq!(merged.len(), 5);

    // All rank-1 items should score higher than rank-2 items
    // ws1:sym1 = 1/61, ws2:sym3 = 1/61, ws3:sym5 = 1/61
    // ws1:sym2 = 1/62, ws2:sym4 = 1/62
    let rank1_score = 1.0_f32 / 61.0;
    let rank2_score = 1.0_f32 / 62.0;

    // First 3 should all be rank-1 items (same score, order may vary)
    for r in &merged[0..3] {
        assert!(
            (r.score() - rank1_score).abs() < 1e-6,
            "Expected rank-1 score {}, got {} for {}",
            rank1_score,
            r.score(),
            r.result.name
        );
    }

    // Last 2 should be rank-2 items
    for r in &merged[3..5] {
        assert!(
            (r.score() - rank2_score).abs() < 1e-6,
            "Expected rank-2 score {}, got {} for {}",
            rank2_score,
            r.score(),
            r.result.name
        );
    }
}

#[test]
fn test_rrf_merge_federated_content_results() {
    // Content results from 2 workspaces with overlapping file paths
    // (but different workspace IDs, so globally unique)
    let ws1_results = vec![
        make_content_result("src/main.rs", 0.9, "ws1"),
        make_content_result("src/lib.rs", 0.7, "ws1"),
    ];
    let ws2_results = vec![
        make_content_result("src/main.rs", 0.85, "ws2"),
    ];

    let lists = vec![ws1_results, ws2_results];
    let merged = multi_rrf_merge(lists, RRF_K, 10);

    // 3 unique items (same file_path but different workspace IDs)
    assert_eq!(merged.len(), 3);
}

#[test]
fn test_rrf_preserves_workspace_attribution() {
    let results = vec![
        make_symbol_result("s1", "foo", 1.0, "project-alpha"),
        make_symbol_result("s2", "bar", 0.8, "project-alpha"),
    ];
    let lists = vec![results];
    let merged = multi_rrf_merge(lists, RRF_K, 10);

    assert_eq!(merged[0].workspace_id, "project-alpha");
    assert_eq!(merged[0].project_name, "project-project-alpha");
    assert_eq!(merged[0].result.name, "foo");
}

#[test]
fn test_global_id_format() {
    let result = make_symbol_result("sym123", "test_fn", 1.0, "ws_abc");
    assert_eq!(result.rrf_id(), "ws_abc:sym123");
}

#[test]
fn test_global_id_prevents_cross_workspace_dedup() {
    // Same symbol ID in different workspaces should NOT be deduped
    let ws1 = vec![make_symbol_result("id_1", "parse", 1.0, "ws1")];
    let ws2 = vec![make_symbol_result("id_1", "parse", 1.0, "ws2")];

    let lists = vec![ws1, ws2];
    let merged = multi_rrf_merge(lists, RRF_K, 10);

    // Should have 2 results, not 1 (different workspace prefixes)
    assert_eq!(merged.len(), 2);
}

// ===========================================================================
// Federated search integration tests (require SearchIndex)
// ===========================================================================

/// Create a temporary SearchIndex with some indexed symbols.
fn create_test_search_index(
    dir: &TempDir,
    symbols: Vec<(&str, &str, &str, &str)>, // (id, name, kind, file_path)
) -> Arc<Mutex<SearchIndex>> {
    let index_path = dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    let search_index = SearchIndex::create(&index_path).unwrap();

    // Index each symbol
    for (id, name, kind, file_path) in &symbols {
        let doc = SymbolDocument {
            id: id.to_string(),
            name: name.to_string(),
            signature: format!("fn {}()", name),
            doc_comment: String::new(),
            code_body: format!("fn {}() {{}}", name),
            file_path: file_path.to_string(),
            kind: kind.to_string(),
            language: "rust".to_string(),
            start_line: 1,
        };
        search_index.add_symbol(&doc).unwrap();
    }

    // Commit the writer so results are searchable
    search_index.commit().unwrap();

    Arc::new(Mutex::new(search_index))
}

/// Create a temporary SearchIndex with some indexed file content.
fn create_test_content_index(
    dir: &TempDir,
    files: Vec<(&str, &str, &str)>, // (file_path, language, content)
) -> Arc<Mutex<SearchIndex>> {
    let index_path = dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    let search_index = SearchIndex::create(&index_path).unwrap();

    for (file_path, language, content) in &files {
        let doc = FileDocument {
            file_path: file_path.to_string(),
            language: language.to_string(),
            content: content.to_string(),
        };
        search_index.add_file_content(&doc).unwrap();
    }

    search_index.commit().unwrap();

    Arc::new(Mutex::new(search_index))
}

#[tokio::test]
async fn test_federated_symbol_search_empty_workspaces() {
    let result = federated_symbol_search(
        "test_query",
        &SearchFilter::default(),
        10,
        &[],
    )
    .await
    .unwrap();

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_federated_symbol_search_single_workspace() {
    let dir = TempDir::new().unwrap();
    let search_index = create_test_search_index(
        &dir,
        vec![
            ("1", "parse_query", "function", "src/parser.rs"),
            ("2", "build_ast", "function", "src/ast.rs"),
        ],
    );

    let workspaces = vec![WorkspaceSearchEntry {
        workspace_id: "ws1".to_string(),
        project_name: "my-project".to_string(),
        search_index,
    }];

    let results = federated_symbol_search(
        "parse",
        &SearchFilter::default(),
        10,
        &workspaces,
    )
    .await
    .unwrap();

    // Should find at least parse_query
    assert!(!results.is_empty(), "Expected at least one result for 'parse'");
    assert_eq!(results[0].workspace_id, "ws1");
    assert_eq!(results[0].project_name, "my-project");
}

#[tokio::test]
async fn test_federated_symbol_search_two_workspaces() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();

    let index1 = create_test_search_index(
        &dir1,
        vec![
            ("1", "search_symbols", "function", "src/search.rs"),
            ("2", "index_file", "function", "src/indexer.rs"),
        ],
    );

    let index2 = create_test_search_index(
        &dir2,
        vec![
            ("1", "search_documents", "function", "src/search.rs"),
            ("2", "query_builder", "function", "src/query.rs"),
        ],
    );

    let workspaces = vec![
        WorkspaceSearchEntry {
            workspace_id: "ws_alpha".to_string(),
            project_name: "alpha-project".to_string(),
            search_index: index1,
        },
        WorkspaceSearchEntry {
            workspace_id: "ws_beta".to_string(),
            project_name: "beta-project".to_string(),
            search_index: index2,
        },
    ];

    let results = federated_symbol_search(
        "search",
        &SearchFilter::default(),
        10,
        &workspaces,
    )
    .await
    .unwrap();

    // Should find results from both workspaces
    assert!(results.len() >= 2, "Expected results from both workspaces, got {}", results.len());

    let workspace_ids: Vec<&str> = results.iter().map(|r| r.workspace_id.as_str()).collect();
    assert!(
        workspace_ids.contains(&"ws_alpha"),
        "Expected results from ws_alpha, got: {:?}",
        workspace_ids
    );
    assert!(
        workspace_ids.contains(&"ws_beta"),
        "Expected results from ws_beta, got: {:?}",
        workspace_ids
    );

    // Same symbol ID ("1") from different workspaces should NOT be deduped
    let global_ids: Vec<String> = results
        .iter()
        .map(|r| r.rrf_id().to_string())
        .collect();
    assert!(
        global_ids.contains(&"ws_alpha:1".to_string()),
        "Expected ws_alpha:1 in global IDs"
    );
    assert!(
        global_ids.contains(&"ws_beta:1".to_string()),
        "Expected ws_beta:1 in global IDs"
    );
}

#[tokio::test]
async fn test_federated_content_search_empty_workspaces() {
    let result = federated_content_search(
        "test_query",
        &SearchFilter::default(),
        10,
        &[],
    )
    .await
    .unwrap();

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_federated_content_search_two_workspaces() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();

    let index1 = create_test_content_index(
        &dir1,
        vec![(
            "src/config.rs",
            "rust",
            "pub fn load_configuration() -> Config { Config::default() }",
        )],
    );

    let index2 = create_test_content_index(
        &dir2,
        vec![(
            "src/settings.rs",
            "rust",
            "pub fn load_configuration() -> Settings { Settings::new() }",
        )],
    );

    let workspaces = vec![
        WorkspaceSearchEntry {
            workspace_id: "ws1".to_string(),
            project_name: "project-one".to_string(),
            search_index: index1,
        },
        WorkspaceSearchEntry {
            workspace_id: "ws2".to_string(),
            project_name: "project-two".to_string(),
            search_index: index2,
        },
    ];

    let results = federated_content_search(
        "configuration",
        &SearchFilter::default(),
        10,
        &workspaces,
    )
    .await
    .unwrap();

    // Should find results from both workspaces
    assert!(results.len() >= 2, "Expected results from both workspaces, got {}", results.len());

    let workspace_ids: Vec<&str> = results.iter().map(|r| r.workspace_id.as_str()).collect();
    assert!(workspace_ids.contains(&"ws1"), "Expected results from ws1");
    assert!(workspace_ids.contains(&"ws2"), "Expected results from ws2");
}

#[tokio::test]
async fn test_federated_symbol_search_respects_limit() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();

    let index1 = create_test_search_index(
        &dir1,
        vec![
            ("1", "alpha_search", "function", "src/a.rs"),
            ("2", "beta_search", "function", "src/b.rs"),
            ("3", "gamma_search", "function", "src/c.rs"),
        ],
    );

    let index2 = create_test_search_index(
        &dir2,
        vec![
            ("1", "delta_search", "function", "src/d.rs"),
            ("2", "epsilon_search", "function", "src/e.rs"),
        ],
    );

    let workspaces = vec![
        WorkspaceSearchEntry {
            workspace_id: "ws1".to_string(),
            project_name: "project-1".to_string(),
            search_index: index1,
        },
        WorkspaceSearchEntry {
            workspace_id: "ws2".to_string(),
            project_name: "project-2".to_string(),
            search_index: index2,
        },
    ];

    let results = federated_symbol_search(
        "search",
        &SearchFilter::default(),
        3, // limit to 3
        &workspaces,
    )
    .await
    .unwrap();

    assert!(
        results.len() <= 3,
        "Expected at most 3 results, got {}",
        results.len()
    );
}

#[tokio::test]
async fn test_federated_search_with_filter() {
    let dir1 = TempDir::new().unwrap();
    let search_index = create_test_search_index(
        &dir1,
        vec![
            ("1", "parse_query", "function", "src/parser.rs"),
            ("2", "build_ast", "function", "src/ast.rs"),
        ],
    );

    let workspaces = vec![WorkspaceSearchEntry {
        workspace_id: "ws1".to_string(),
        project_name: "test-project".to_string(),
        search_index,
    }];

    // Filter by language
    let filter = SearchFilter {
        language: Some("rust".to_string()),
        ..Default::default()
    };

    let results = federated_symbol_search(
        "parse",
        &filter,
        10,
        &workspaces,
    )
    .await
    .unwrap();

    // Should still find results (language matches)
    assert!(!results.is_empty());
}

// ===========================================================================
// Graceful degradation tests
// ===========================================================================

/// Poison a mutex by panicking while holding it, then return the poisoned Arc.
fn create_poisoned_search_index() -> Arc<Mutex<SearchIndex>> {
    let dir = TempDir::new().unwrap();
    let index_path = dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();
    let search_index = SearchIndex::create(&index_path).unwrap();
    let arc = Arc::new(Mutex::new(search_index));

    // Poison the mutex by panicking while holding the lock
    let arc_clone = Arc::clone(&arc);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = arc_clone.lock().unwrap();
        panic!("intentional panic to poison mutex");
    }));

    // Verify it's actually poisoned
    assert!(arc.lock().is_err(), "Mutex should be poisoned");

    // Leak the TempDir so the index files persist while the Arc is alive.
    // (The test is short-lived so this is fine.)
    std::mem::forget(dir);

    arc
}

#[tokio::test]
async fn test_federated_symbol_search_skips_failed_workspace() {
    // Workspace 1: healthy, has searchable symbols
    let dir1 = TempDir::new().unwrap();
    let healthy_index = create_test_search_index(
        &dir1,
        vec![
            ("1", "search_handler", "function", "src/handler.rs"),
        ],
    );

    // Workspace 2: poisoned mutex -- should fail gracefully
    let poisoned_index = create_poisoned_search_index();

    let workspaces = vec![
        WorkspaceSearchEntry {
            workspace_id: "healthy_ws".to_string(),
            project_name: "healthy-project".to_string(),
            search_index: healthy_index,
        },
        WorkspaceSearchEntry {
            workspace_id: "broken_ws".to_string(),
            project_name: "broken-project".to_string(),
            search_index: poisoned_index,
        },
    ];

    // Should NOT error -- the broken workspace is skipped
    let results = federated_symbol_search(
        "search",
        &SearchFilter::default(),
        10,
        &workspaces,
    )
    .await
    .unwrap();

    // Should still get results from the healthy workspace
    assert!(!results.is_empty(), "Expected results from healthy workspace");
    assert_eq!(results[0].workspace_id, "healthy_ws");
    assert_eq!(results[0].result.name, "search_handler");

    // No results from the broken workspace
    let broken_results: Vec<_> = results
        .iter()
        .filter(|r| r.workspace_id == "broken_ws")
        .collect();
    assert!(broken_results.is_empty(), "Broken workspace should have been skipped");
}

#[tokio::test]
async fn test_federated_content_search_skips_failed_workspace() {
    // Workspace 1: healthy
    let dir1 = TempDir::new().unwrap();
    let healthy_index = create_test_content_index(
        &dir1,
        vec![("src/main.rs", "rust", "fn main() { println!(\"hello\"); }")],
    );

    // Workspace 2: poisoned
    let poisoned_index = create_poisoned_search_index();

    let workspaces = vec![
        WorkspaceSearchEntry {
            workspace_id: "healthy_ws".to_string(),
            project_name: "healthy-project".to_string(),
            search_index: healthy_index,
        },
        WorkspaceSearchEntry {
            workspace_id: "broken_ws".to_string(),
            project_name: "broken-project".to_string(),
            search_index: poisoned_index,
        },
    ];

    let results = federated_content_search(
        "main",
        &SearchFilter::default(),
        10,
        &workspaces,
    )
    .await
    .unwrap();

    // Should still get results from the healthy workspace
    assert!(!results.is_empty(), "Expected results from healthy workspace");
    assert_eq!(results[0].workspace_id, "healthy_ws");

    // No results from the broken workspace
    let broken_results: Vec<_> = results
        .iter()
        .filter(|r| r.workspace_id == "broken_ws")
        .collect();
    assert!(broken_results.is_empty(), "Broken workspace should have been skipped");
}
