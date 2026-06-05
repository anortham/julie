//! Follower repair-gate tests (codex 3c.3 pre-merge, Finding 1).
//!
//! `repair_recreated_open_if_needed` runs `clear_all` + `apply_documents` — a
//! Tantivy WRITE that rebuilds the projection from canonical SQLite. It is
//! reached on the search-index OPEN/read path from
//! `primary_workspace_snapshot_from_binding_paths` and
//! `get_search_index_for_workspace`. Before the fix, neither site checked
//! leadership, so an in-process FOLLOWER serving a read tool could become a
//! Tantivy writer when the on-disk index reported `repair_required` — violating
//! the single-writer invariant the cutover establishes (T7, Risk #2).
//!
//! Both sites now gate the rebuild on `may_repair_recreated_projection()`. These
//! tests pin that gate:
//!   1. Predicate truth table — follower → false; leader + daemon/stdio
//!      (`none()`) → true.
//!   2. Behavioral — driving the EXACT call-site decision
//!      (`repair_required && handler.may_repair_recreated_projection()`) against
//!      a REAL recreated-open index: a follower leaves it empty (no write); a
//!      leader rebuilds it from canonical SQLite.

use tempfile::TempDir;

use crate::daemon::discovery::DaemonLockGuard;
use crate::database::types::FileInfo;
use crate::database::SymbolDatabase;
use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::leadership::LeadershipState;
use crate::search::{SearchIndex, SearchProjection};
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

const WS: &str = "ws_repair_gate";

// ---------------------------------------------------------------------------
// Handler builders (mirror loser_refuses.rs)
// ---------------------------------------------------------------------------

async fn make_leader(dir: &TempDir) -> JulieServerHandler {
    let guard = DaemonLockGuard::try_acquire(&dir.path().join(".leader.lock"))
        .expect("lock acquirable on fresh path");
    let hint = WorkspaceStartupHint {
        path: dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    JulieServerHandler::new_in_process(hint, None, LeadershipState::leader(guard), None)
        .await
        .expect("leader handler")
}

async fn make_follower(dir: &TempDir) -> JulieServerHandler {
    let hint = WorkspaceStartupHint {
        path: dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    JulieServerHandler::new_in_process(hint, None, LeadershipState::follower(), None)
        .await
        .expect("follower handler")
}

async fn make_none(dir: &TempDir) -> JulieServerHandler {
    let hint = WorkspaceStartupHint {
        path: dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };
    JulieServerHandler::new_in_process(hint, None, LeadershipState::none(), None)
        .await
        .expect("none() handler")
}

// ---------------------------------------------------------------------------
// Fixture helpers (mirror projection_repair.rs)
// ---------------------------------------------------------------------------

fn gate_file(path: &str, content: &str) -> FileInfo {
    FileInfo {
        path: path.to_string(),
        language: "rust".to_string(),
        hash: format!("hash_{path}"),
        size: content.len() as i64,
        last_modified: 1000,
        last_indexed: 0,
        symbol_count: 1,
        line_count: content.lines().count() as i32,
        content: Some(content.to_string()),
    }
}

fn gate_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
    Symbol {
        id: id.to_string(),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: file_path.to_string(),
        start_line: 1,
        start_column: 0,
        end_line: 3,
        end_column: 0,
        start_byte: 0,
        end_byte: 32,
        signature: Some(format!("fn {}()", name)),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: None,
        code_context: Some(format!("fn {}() {{}}", name)),
        content_type: None,
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

/// Build a workspace whose Tantivy index reports `repair_required` on reopen:
/// store one symbol + project it, then delete the compat marker so the next open
/// recreates the index empty. Returns the fixture dir, db path, and index path.
fn setup_repair_required_fixture() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("symbols.db");
    let index_path = temp.path().join("tantivy");
    std::fs::create_dir_all(&index_path).unwrap();

    let mut db = SymbolDatabase::new(&db_path).unwrap();
    let projection = SearchProjection::tantivy(WS);
    {
        let index = SearchIndex::open_or_create(&index_path).unwrap();
        db.bulk_store_fresh_atomic(
            &[gate_file("src/lib.rs", "fn gated_symbol() {}\n")],
            &[gate_symbol("sym_gate", "gated_symbol", "src/lib.rs")],
            &[],
            &[],
            &[],
            WS,
        )
        .unwrap();
        projection.ensure_current_from_database(&mut db, &index).unwrap();
        assert_eq!(index.num_docs(), 2, "fixture setup should create docs");
    }
    // Delete the compat marker → next open is a recreated-empty (repair_required) open.
    std::fs::remove_file(index_path.join("julie-search-compat.json")).unwrap();

    (temp, db_path, index_path)
}

/// Reopen the fixture index (forcing the recreated-empty path) and run the EXACT
/// gated decision both call sites use:
/// `if repair_required && handler.may_repair_recreated_projection() { rebuild }`.
/// Returns the resulting doc count: 0 = no rebuild (follower), 2 = rebuilt.
fn open_and_maybe_repair(
    handler: &JulieServerHandler,
    db_path: &std::path::Path,
    index_path: &std::path::Path,
) -> u64 {
    let configs = crate::search::LanguageConfigs::load_embedded();
    let open_outcome =
        SearchIndex::open_or_create_with_language_configs_outcome(index_path, &configs).unwrap();
    let repair_required = open_outcome.repair_required();
    assert!(
        repair_required,
        "fixture must present a recreated-open (repair_required) index"
    );
    let index = open_outcome.into_index();
    assert_eq!(index.num_docs(), 0, "recreated open must start empty");

    // The exact call-site gate (handler.rs primary_workspace_snapshot_from_binding_paths
    // / get_search_index_for_workspace), driven by the REAL handler predicate.
    if repair_required && handler.may_repair_recreated_projection() {
        let mut db = SymbolDatabase::new(db_path).unwrap();
        let projection = SearchProjection::tantivy(WS);
        projection
            .repair_recreated_open_if_needed(&mut db, &index, repair_required, None)
            .unwrap();
    }

    index.num_docs()
}

// ---------------------------------------------------------------------------
// Test 1: predicate truth table
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_may_repair_recreated_projection_truth_table() {
    let d1 = TempDir::new().unwrap();
    let leader = make_leader(&d1).await;
    assert!(
        leader.may_repair_recreated_projection(),
        "leader OWNS writes — must be allowed to repair"
    );

    let d2 = TempDir::new().unwrap();
    let follower = make_follower(&d2).await;
    assert!(
        !follower.may_repair_recreated_projection(),
        "in-process follower must NOT repair (single-writer invariant)"
    );

    let d3 = TempDir::new().unwrap();
    let none = make_none(&d3).await;
    assert!(
        none.may_repair_recreated_projection(),
        "daemon/stdio (none()) is not follower-gated — pre-3c repair path unchanged"
    );
}

// ---------------------------------------------------------------------------
// Test 2: behavioral — follower leaves the index empty, leader rebuilds it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_follower_skips_recreated_open_repair() {
    let hint_dir = TempDir::new().unwrap();
    let follower = make_follower(&hint_dir).await;

    let (_fix, db_path, index_path) = setup_repair_required_fixture();
    let docs = open_and_maybe_repair(&follower, &db_path, &index_path);

    assert_eq!(
        docs, 0,
        "in-process follower must NOT rebuild the projection on a read-path open — \
         it stays a pure reader (degrades to freshness-only; the leader owns the rebuild)"
    );
}

#[tokio::test]
async fn test_leader_repairs_recreated_open() {
    let hint_dir = TempDir::new().unwrap();
    let leader = make_leader(&hint_dir).await;

    let (_fix, db_path, index_path) = setup_repair_required_fixture();
    let docs = open_and_maybe_repair(&leader, &db_path, &index_path);

    assert_eq!(
        docs, 2,
        "leader OWNS writes — it must rebuild the recreated-open projection from canonical SQLite"
    );
}
