//! T9 handoff-recovery gate tests.
//!
//! Two invariants:
//!
//! 1. **Leader recovery** — when `canonical_revision > projected_revision` (SQLite
//!    has a symbol that Tantivy does not know about), the reconcile path
//!    `ensure_current_from_database` (called by `reconcile_projection_lag_if_needed`
//!    inside `run_primary_workspace_repair`) rebuilds Tantivy from canonical SQLite
//!    state so the promoted leader can serve searches. Modeled directly on
//!    `test_search_projection_rebuilds_empty_index_from_canonical_sqlite` in
//!    `projection_repair.rs`.
//!
//! 2. **Follower structural gate** — an in-process follower handler reports
//!    `is_in_process_follower() == true`, which is the guard condition for the
//!    early-return no-op added to `complete_deferred_auto_index_if_needed` (T9/Part A).
//!    The follower therefore never runs the writing SQLite/Tantivy repair, preventing
//!    cross-process data races (Risk #2).

use anyhow::Result;
use tempfile::TempDir;

use crate::database::types::FileInfo;
use crate::database::{SymbolDatabase};
use crate::extractors::{Symbol, SymbolKind};
use crate::search::{SearchIndex, SearchProjection};

// ---------------------------------------------------------------------------
// Helpers (mirrors projection_repair.rs to keep the test self-contained)
// ---------------------------------------------------------------------------

fn handoff_file(path: &str, content: &str) -> FileInfo {
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

fn handoff_symbol(id: &str, name: &str, file_path: &str) -> Symbol {
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

// ---------------------------------------------------------------------------
// Test 1: Leader recovery reconciles Tantivy from canonical SQLite
// ---------------------------------------------------------------------------

/// T9 gate — Leader handoff recovery: when SQLite has a symbol
/// (canonical_revision=1) but Tantivy is empty (no projection state →
/// projected_revision==null → lag detected), `ensure_current_from_database`
/// rebuilds Tantivy from canonical SQLite state. A search for the symbol must
/// return a result afterwards.
///
/// This is the mechanism exercised by `reconcile_projection_lag_if_needed`
/// inside `run_primary_workspace_repair` on leader promotion.
#[test]
fn test_leader_handoff_recovery_reconciles_tantivy_from_canonical_sqlite() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("symbols.db");
    let index_path = temp_dir.path().join("tantivy");
    std::fs::create_dir_all(&index_path)?;

    let mut db = SymbolDatabase::new(&db_path)?;
    let index = SearchIndex::open_or_create(&index_path)?;
    let projection = SearchProjection::tantivy("ws_handoff");

    // Step 1: Write symbol to SQLite (advances canonical_revision to 1).
    // Tantivy is intentionally left empty — simulates the crash/restart gap
    // where the SQLite commit succeeded but Tantivy apply did not.
    db.bulk_store_fresh_atomic(
        &[handoff_file("src/leader.rs", "fn promoted_leader_fn() {}\n")],
        &[handoff_symbol("sym_leader", "promoted_leader_fn", "src/leader.rs")],
        &[],
        &[],
        &[],
        "ws_handoff",
    )?;

    // Verify the projection lag: Tantivy is empty, SQLite has canonical_revision=1.
    assert_eq!(
        index.num_docs(),
        0,
        "Tantivy must be empty before reconcile (no projection state yet)"
    );

    // Step 2: Call ensure_current_from_database — the reconcile path that
    // run_primary_workspace_repair → reconcile_projection_lag_if_needed invokes.
    let state = projection.ensure_current_from_database(&mut db, &index)?;

    // Step 3: Verify Tantivy has been rebuilt from canonical SQLite state.
    assert!(
        index.num_docs() >= 1,
        "reconcile must have populated Tantivy (at least 1 doc for the indexed symbol/file)"
    );
    assert_eq!(
        state.canonical_revision,
        Some(1),
        "canonical_revision must be 1 after first bulk_store"
    );
    assert_eq!(state.status.as_str(), "ready", "projection status must be ready");

    // Step 4: Search finds the promoted leader's symbol.
    let results = index.search_symbols("promoted_leader_fn", &Default::default(), 10)?;
    assert_eq!(
        results.results.len(),
        1,
        "search must find the symbol after reconcile"
    );
    assert_eq!(results.results[0].name, "promoted_leader_fn");

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 2: Follower structural gate — is_in_process_follower() == true
// ---------------------------------------------------------------------------

/// T9 gate — Follower structural gate: an in-process follower handler reports
/// `is_in_process_follower() == true` and `is_leader() == false`.
///
/// `complete_deferred_auto_index_if_needed` has an early-return guard on
/// `is_in_process_follower()` (T9/Part A). This test proves the structural
/// condition holds for a follower handler, confirming the guard will fire and
/// prevent the follower from running the writing SQLite/Tantivy repair.
#[tokio::test]
async fn test_follower_structural_gate_is_in_process_follower() {
    use crate::handler::JulieServerHandler;
    use crate::leadership::LeadershipState;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    let workspace_dir = tempfile::tempdir().unwrap();
    let hint = WorkspaceStartupHint {
        path: workspace_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let follower =
        JulieServerHandler::new_in_process(hint, None, LeadershipState::follower(), None)
            .await
            .expect("follower handler must build");

    // Structural gate: is_in_process_follower() must be true so the early-return
    // guard in complete_deferred_auto_index_if_needed fires.
    assert!(
        follower.is_in_process_follower(),
        "follower must report is_in_process_follower() == true \
         (guard condition for the T9 writing-recovery skip)"
    );
    assert!(
        !follower.is_leader(),
        "follower must not claim leadership"
    );
    assert!(
        follower.is_in_process(),
        "follower must report is_in_process() == true (participates in election)"
    );
}

/// T9 gate — Leader structural gate: a leader handler reports `is_leader() == true`
/// and `is_in_process_follower() == false`, so the repair guard does NOT skip
/// recovery for leaders.
#[tokio::test]
async fn test_leader_structural_gate_is_leader_not_follower() {
    use crate::daemon::discovery::DaemonLockGuard;
    use crate::handler::JulieServerHandler;
    use crate::leadership::LeadershipState;
    use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

    let workspace_dir = tempfile::tempdir().unwrap();
    let lock_path = workspace_dir.path().join(".leader.lock");
    let guard = DaemonLockGuard::try_acquire(&lock_path)
        .expect("lock must be acquirable on fresh path");

    let hint = WorkspaceStartupHint {
        path: workspace_dir.path().to_path_buf(),
        source: Some(WorkspaceStartupSource::Cli),
    };

    let leader =
        JulieServerHandler::new_in_process(hint, None, LeadershipState::leader(guard), None)
            .await
            .expect("leader handler must build");

    assert!(
        leader.is_leader(),
        "leader must report is_leader() == true"
    );
    assert!(
        !leader.is_in_process_follower(),
        "leader must NOT report is_in_process_follower() (repair must NOT be skipped for leaders)"
    );
    assert!(
        leader.is_in_process(),
        "leader must report is_in_process() == true"
    );
}
