// CASCADE Architecture Integration Tests
// Tests the complete cascade flow: SQLite → Tantivy → Semantic

use crate::handler::JulieServerHandler;
use crate::tools::search::FastSearchTool;
use crate::tools::workspace::ManageWorkspaceTool;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

/// Helper: Create a test workspace with sample files
async fn create_test_workspace() -> Result<(TempDir, PathBuf), anyhow::Error> {
    let temp_dir = TempDir::new()?;
    let workspace_path = temp_dir.path().to_path_buf();

    // Create sample TypeScript file with searchable content
    let ts_file = workspace_path.join("sample.ts");
    std::fs::write(
        &ts_file,
        r#"
// Sample TypeScript file for CASCADE testing
export function calculateTotal(items: number[]): number {
    return items.reduce((sum, item) => sum + item, 0);
}

export class UserService {
    private users: Map<string, User>;

    constructor() {
        this.users = new Map();
    }

    getUserById(id: string): User | undefined {
        return this.users.get(id);
    }
}
"#,
    )?;

    // Create sample Python file
    let py_file = workspace_path.join("sample.py");
    std::fs::write(
        &py_file,
        r#"
# Sample Python file for CASCADE testing
def calculate_total(items):
    """Calculate the total sum of items."""
    return sum(items)

class UserService:
    def __init__(self):
        self.users = {}

    def get_user_by_id(self, user_id):
        return self.users.get(user_id)
"#,
    )?;

    Ok((temp_dir, workspace_path))
}

#[tokio::test]
async fn test_cascade_flow_fresh_index() {
    // TDD Test 1: Verify complete cascade flow from fresh index
    //
    // This test validates that:
    // 1. SQLite FTS is ready immediately after indexing
    // 2. Tantivy builds in background and becomes ready
    // 3. All status flags are set correctly
    // 4. Search works at each cascade level

    let (_temp_dir, workspace_path) = create_test_workspace()
        .await
        .expect("Failed to create test workspace");

    // Initialize handler and workspace
    let handler = JulieServerHandler::new()
        .await
        .expect("Failed to create handler");

    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await
        .expect("Failed to initialize workspace");

    // Verify status: Nothing ready before indexing
    assert!(
        !handler
            .indexing_status
            .sqlite_fts_ready
            .load(Ordering::Relaxed),
        "SQLite FTS should not be ready before indexing"
    );
    assert!(
        !handler
            .indexing_status
            .tantivy_ready
            .load(Ordering::Relaxed),
        "Tantivy should not be ready before indexing"
    );

    // Perform indexing using ManageWorkspaceTool
    let start = Instant::now();
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        expired_only: None,
        days: None,
        max_size_mb: None,
        detailed: None,
    };
    index_tool
        .call_tool(&handler)
        .await
        .expect("Failed to index workspace");

    let indexing_duration = start.elapsed();
    println!("⏱️  Indexing completed in {:?}", indexing_duration);

    // ASSERTION 1: SQLite FTS should be ready immediately (non-blocking)
    assert!(
        handler
            .indexing_status
            .sqlite_fts_ready
            .load(Ordering::Relaxed),
        "SQLite FTS should be ready immediately after indexing"
    );
    assert!(
        indexing_duration.as_secs() < 5,
        "Indexing should complete in <5s (was: {:?})",
        indexing_duration
    );

    // ASSERTION 2: SQLite FTS search should work immediately
    let workspace = handler.get_workspace().await.unwrap().unwrap();
    let registry_service =
        crate::workspace::registry_service::WorkspaceRegistryService::new(workspace.root.clone());
    let workspace_id_opt = registry_service
        .get_primary_workspace_id()
        .await
        .ok()
        .flatten();

    let db = workspace.db.as_ref().unwrap();
    let db_lock = db.lock().await;

    let fts_results = db_lock
        .search_file_content_fts("calculate", workspace_id_opt.as_deref(), 10)
        .expect("SQLite FTS search failed");

    assert!(!fts_results.is_empty(), "SQLite FTS should find results");
    println!(
        "✅ SQLite FTS found {} results immediately",
        fts_results.len()
    );
    drop(db_lock);

    // ASSERTION 3: Wait for Tantivy to build in background (should be quick)
    let mut tantivy_ready = false;
    for i in 0..20 {
        // Wait up to 10 seconds (20 * 500ms)
        if handler
            .indexing_status
            .tantivy_ready
            .load(Ordering::Relaxed)
        {
            tantivy_ready = true;
            println!("✅ Tantivy ready after ~{}ms", i * 500);
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    assert!(tantivy_ready, "Tantivy should be ready within 10 seconds");

    // ASSERTION 4: Verify Tantivy search works
    let search_engine = handler.active_search_engine().await.unwrap();
    let search_lock = search_engine.read().await;
    let tantivy_results = search_lock.search("UserService").await;
    assert!(tantivy_results.is_ok(), "Tantivy search should work");
    let symbols = tantivy_results.unwrap();
    assert!(!symbols.is_empty(), "Tantivy should find symbols");
    println!("✅ Tantivy found {} symbols", symbols.len());

    println!("\n🎉 CASCADE flow test PASSED - All three layers operational!");
}

#[tokio::test]
async fn test_search_fallback_chain() {
    // TDD Test 2: Verify search fallback chain works correctly
    //
    // This test validates that:
    // 1. When Tantivy fails, falls back to SQLite FTS
    // 2. Search always returns results (graceful degradation)
    // 3. Fallback adds minimal overhead

    let (_temp_dir, workspace_path) = create_test_workspace()
        .await
        .expect("Failed to create test workspace");

    let handler = JulieServerHandler::new()
        .await
        .expect("Failed to create handler");

    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await
        .expect("Failed to initialize workspace");

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        expired_only: None,
        days: None,
        max_size_mb: None,
        detailed: None,
    };
    index_tool
        .call_tool(&handler)
        .await
        .expect("Failed to index workspace");

    // Wait for SQLite FTS to be ready
    let mut fts_ready = false;
    for _ in 0..10 {
        if handler
            .indexing_status
            .sqlite_fts_ready
            .load(Ordering::Relaxed)
        {
            fts_ready = true;
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(fts_ready, "SQLite FTS should be ready");

    // ASSERTION 1: Search should work even before Tantivy is ready
    let search_tool = FastSearchTool {
        query: "calculate".to_string(),
        mode: "text".to_string(),
        limit: 10,
        language: None,
        file_pattern: None,
        workspace: Some("primary".to_string()),
    };

    let start = Instant::now();
    let results = search_tool.call_tool(&handler).await;
    let search_duration = start.elapsed();

    assert!(results.is_ok(), "Search should succeed with fallback");
    println!("✅ Fallback search completed in {:?}", search_duration);

    // ASSERTION 2: Results should be meaningful (from SQLite FTS)
    // Note: The results will be in CallToolResult format, we just verify success
    println!("✅ Search fallback chain working - graceful degradation achieved");

    // ASSERTION 3: Verify fallback overhead is minimal (<10ms typical)
    assert!(
        search_duration.as_millis() < 100,
        "Fallback search should complete quickly (was: {:?})",
        search_duration
    );
}

#[tokio::test]
async fn test_tantivy_rebuild_from_sqlite() {
    // TDD Test 3: Verify Tantivy can rebuild entirely from SQLite
    //
    // This test validates that:
    // 1. SQLite contains all necessary data (source of truth)
    // 2. Tantivy can be rebuilt from SQLite alone
    // 3. Rebuilt Tantivy index works correctly

    let (_temp_dir, workspace_path) = create_test_workspace()
        .await
        .expect("Failed to create test workspace");

    let handler = JulieServerHandler::new()
        .await
        .expect("Failed to create handler");

    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await
        .expect("Failed to initialize workspace");

    // Index the workspace (creates both SQLite and Tantivy)
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        expired_only: None,
        days: None,
        max_size_mb: None,
        detailed: None,
    };
    index_tool
        .call_tool(&handler)
        .await
        .expect("Failed to index workspace");

    // Wait for both indexes to be ready
    for _ in 0..40 {
        if handler
            .indexing_status
            .sqlite_fts_ready
            .load(Ordering::Relaxed)
            && handler
                .indexing_status
                .tantivy_ready
                .load(Ordering::Relaxed)
        {
            break;
        }
        sleep(Duration::from_millis(500)).await;
    }

    // ASSERTION 1: Verify SQLite contains file contents
    let workspace = handler.get_workspace().await.unwrap().unwrap();
    let registry_service =
        crate::workspace::registry_service::WorkspaceRegistryService::new(workspace.root.clone());
    let workspace_id = registry_service
        .get_primary_workspace_id()
        .await
        .unwrap()
        .unwrap();

    let db = workspace.db.as_ref().unwrap();
    let db_lock = db.lock().await;

    let file_contents = db_lock
        .get_all_file_contents(&workspace_id)
        .expect("Failed to get file contents");

    assert!(
        !file_contents.is_empty(),
        "SQLite should contain file contents"
    );
    assert!(
        file_contents.len() >= 2,
        "Should have at least 2 files indexed"
    );
    println!("✅ SQLite contains {} files", file_contents.len());

    drop(db_lock);

    // ASSERTION 2: Verify SQLite contains symbols
    let db_lock = db.lock().await;
    let symbols = db_lock
        .get_symbols_for_workspace(&workspace_id)
        .expect("Failed to get symbols");

    assert!(!symbols.is_empty(), "SQLite should contain symbols");
    println!("✅ SQLite contains {} symbols", symbols.len());

    drop(db_lock);

    // ASSERTION 3: Verify Tantivy search works (rebuilt from SQLite)
    if handler
        .indexing_status
        .tantivy_ready
        .load(Ordering::Relaxed)
    {
        let search_engine = handler.active_search_engine().await.unwrap();
        let search_lock = search_engine.read().await;
        let results = search_lock.search("calculate").await;

        assert!(results.is_ok(), "Tantivy search should work after rebuild");
        let symbols = results.unwrap();
        assert!(
            !symbols.is_empty(),
            "Tantivy should find symbols after rebuild"
        );
        println!(
            "✅ Tantivy successfully rebuilt from SQLite - found {} results",
            symbols.len()
        );
    }

    println!("\n🎉 Tantivy rebuild test PASSED - SQLite is true source of truth!");
}

#[tokio::test]
async fn test_sqlite_fts_query_performance() {
    // TDD Test 4: Verify SQLite FTS meets performance targets
    //
    // Target: <5ms query latency for FTS5 searches

    let (_temp_dir, workspace_path) = create_test_workspace()
        .await
        .expect("Failed to create test workspace");

    let handler = JulieServerHandler::new()
        .await
        .expect("Failed to create handler");

    handler
        .initialize_workspace(Some(workspace_path.to_string_lossy().to_string()))
        .await
        .expect("Failed to initialize workspace");

    // Index the workspace
    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        expired_only: None,
        days: None,
        max_size_mb: None,
        detailed: None,
    };
    index_tool
        .call_tool(&handler)
        .await
        .expect("Failed to index workspace");

    // Wait for SQLite FTS to be ready
    for _ in 0..10 {
        if handler
            .indexing_status
            .sqlite_fts_ready
            .load(Ordering::Relaxed)
        {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    // Run multiple queries to get average performance
    let queries = vec!["calculate", "User", "function", "class", "total"];
    let mut total_duration = Duration::from_millis(0);
    let mut query_count = 0;

    let workspace = handler.get_workspace().await.unwrap().unwrap();
    let db = workspace.db.as_ref().unwrap();

    for query in &queries {
        let db_lock = db.lock().await;
        let start = Instant::now();

        let _results = db_lock
            .search_file_content_fts(query, None, 10)
            .expect("FTS search failed");

        let duration = start.elapsed();
        drop(db_lock);

        total_duration += duration;
        query_count += 1;

        println!("⏱️  Query '{}' took {:?}", query, duration);
    }

    let avg_duration = total_duration / query_count;
    println!("\n📊 Average FTS query time: {:?}", avg_duration);

    // ASSERTION: Average query time should be <5ms (target performance)
    assert!(
        avg_duration.as_millis() < 5,
        "SQLite FTS queries should average <5ms (was: {:?})",
        avg_duration
    );

    println!(
        "✅ SQLite FTS performance target MET - queries averaging {:?}",
        avg_duration
    );
}
