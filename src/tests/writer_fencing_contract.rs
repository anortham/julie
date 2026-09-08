//! src/tests/writer_fencing_contract.rs
//! Contract tests for Index Writer Fencing & Recoverable Revision Publication (Milestone 4 / Task 4).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::process::{Child, Command};

use crate::paths::RegistryPaths;
use crate::tests::request_process_helpers::resolve_julie_binary;
use julie_test_support::workspace_markers::make_isolated_workspace_root;

pub struct RecoveryProcessFixture {
    pub temp_repo: TempDir,
    pub workspace_root: PathBuf,
    pub temp_home: TempDir,
    pub index_root: PathBuf,
    pub ipc_barrier_dir: PathBuf,
    pub workspace_id: String,
    pub owner_child: Option<Child>,
    pub follower_child: Option<Child>,
    pub binary_path: PathBuf,
}

impl RecoveryProcessFixture {
    pub async fn new() -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let _ = std::fs::create_dir_all(&tmp_base);

        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("create temp repo");
        let workspace_root = make_isolated_workspace_root(temp_repo.path(), "recovery_test")
            .canonicalize()
            .expect("canonicalize workspace root");

        // Seed workspace
        std::fs::create_dir_all(workspace_root.join("src")).expect("create src dir");
        std::fs::write(
            workspace_root.join("Cargo.toml"),
            "[package]\nname = \"recovery-probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write Cargo.toml");
        std::fs::write(
            workspace_root.join("src/lib.rs"),
            "pub fn recovered_symbol() -> i32 { 42 }\n",
        )
        .expect("write initial source");

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("create temp home");
        let workspace_id = julie_core::workspace::registry::generate_workspace_id(
            &workspace_root.to_string_lossy(),
        )
        .expect("generate workspace id");
        let paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
        let index_root = paths.workspace_index_dir(&workspace_id);
        std::fs::create_dir_all(&index_root).expect("create index root");

        let ipc_barrier_dir = temp_home.path().join("ipc_barriers");
        std::fs::create_dir_all(&ipc_barrier_dir).expect("create ipc barrier dir");

        let binary_path = resolve_julie_binary();

        Self {
            temp_repo,
            workspace_root,
            temp_home,
            index_root,
            ipc_barrier_dir,
            workspace_id,
            owner_child: None,
            follower_child: None,
            binary_path,
        }
    }

    /// Spawns owner child with pause-after-canonical-commit fault injection,
    /// and waits until the child hits the barrier.
    pub async fn owner_pause_after_canonical_commit(&mut self) {
        let reached_sentinel = self.ipc_barrier_dir.join("reached_canonical_commit");
        if reached_sentinel.exists() {
            let _ = std::fs::remove_file(&reached_sentinel);
        }

        let mut cmd = Command::new(&self.binary_path);
        cmd.args([
            "--workspace",
            self.workspace_root.to_str().unwrap(),
            "--json",
            "workspace",
            "index",
        ]);
        cmd.env("JULIE_HOME", self.temp_home.path());
        cmd.env("TMPDIR", self.temp_home.path());
        cmd.env("JULIE_EMBEDDING_PROVIDER", "none");
        cmd.env("JULIE_FAULT_INJECTION", "pause_after_canonical_commit");
        cmd.env("JULIE_IPC_BARRIER_DIR", &self.ipc_barrier_dir);
        cmd.current_dir(&self.workspace_root);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let child = cmd.spawn().expect("spawn owner child");
        self.owner_child = Some(child);

        // Deterministic wait for IPC barrier sentinel — NO arbitrary sleeps
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if reached_sentinel.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("Timed out waiting for owner to signal reached_canonical_commit");
    }

    pub fn find_symbols_db(dir: &Path) -> Option<PathBuf> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(found) = Self::find_symbols_db(&path) {
                        return Some(found);
                    }
                } else if path.file_name().and_then(|n| n.to_str()) == Some("symbols.db") {
                    return Some(path);
                }
            }
        }
        None
    }

    pub fn db_path(&self) -> PathBuf {
        let p1 = self.index_root.join("db").join("symbols.db");
        if p1.exists() {
            return p1;
        }
        if let Some(found) = Self::find_symbols_db(self.temp_home.path()) {
            return found;
        }
        if let Some(found) = Self::find_symbols_db(&self.workspace_root) {
            return found;
        }
        p1
    }

    pub fn canonical_revision(&self) -> u64 {
        let db_path = self.db_path();
        if !db_path.exists() {
            eprintln!("canonical_revision: db_path {:?} does not exist", db_path);
            return 0;
        }
        let conn = match rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("canonical_revision: open error: {e}");
                return 0;
            }
        };
        let _ = conn.busy_timeout(std::time::Duration::from_millis(5000));
        match conn.query_row(
            "SELECT revision FROM canonical_revisions ORDER BY revision DESC LIMIT 1",
            [],
            |row| row.get::<_, i64>(0),
        ) {
            Ok(rev) => rev as u64,
            Err(e) => {
                eprintln!("canonical_revision: query error: {e}");
                0
            }
        }
    }

    pub fn projected_revision(&self) -> u64 {
        let db_path = self.db_path();
        if !db_path.exists() {
            eprintln!("projected_revision: db_path {:?} does not exist", db_path);
            return 0;
        }
        let conn = match rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("projected_revision: open error: {e}");
                return 0;
            }
        };
        let _ = conn.busy_timeout(std::time::Duration::from_millis(5000));
        match conn.query_row(
            "SELECT projected_revision FROM projection_states WHERE projection = 'tantivy' ORDER BY projected_revision DESC LIMIT 1",
            [],
            |row| row.get::<_, Option<i64>>(0),
        ) {
            Ok(Some(rev)) => rev as u64,
            Ok(None) => 0,
            Err(e) => {
                eprintln!("projected_revision: query error: {e}");
                0
            }
        }
    }

    pub async fn kill_owner(&mut self) {
        if let Some(mut child) = self.owner_child.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
    }

    pub async fn wait_for_follower_promotion(&mut self) {
        let out_file = self.temp_home.path().join("follower_stdout.log");
        let err_file = self.temp_home.path().join("follower_stderr.log");
        let stdout_f = std::fs::File::create(&out_file).expect("create stdout file");
        let stderr_f = std::fs::File::create(&err_file).expect("create stderr file");

        // Spawn replacement follower child which will detect released lock and promote to Owner
        let mut cmd = Command::new(&self.binary_path);
        cmd.arg("--workspace").arg(&self.workspace_root);
        cmd.env("JULIE_HOME", self.temp_home.path());
        cmd.env("TMPDIR", self.temp_home.path());
        cmd.env("JULIE_EMBEDDING_PROVIDER", "none");
        cmd.current_dir(&self.workspace_root);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(stdout_f);
        cmd.stderr(stderr_f);

        let child = cmd.spawn().expect("spawn replacement owner child");
        self.follower_child = Some(child);

        // Wait until projected_revision matches canonical_revision (reconciliation complete)
        let target_canonical = self.canonical_revision();
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            let proj = self.projected_revision();
            if proj == target_canonical && target_canonical > 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let current_proj = self.projected_revision();
        let current_canon = self.canonical_revision();
        let db_path = self.db_path();
        let mut child_status = "running".to_string();
        if let Some(ref mut child) = self.follower_child {
            if let Ok(Some(st)) = child.try_wait() {
                child_status = format!("exited with {st}");
            }
        }
        let mut canon_rows = Vec::new();
        let mut proj_rows = Vec::new();
        if let Ok(conn) = rusqlite::Connection::open(&db_path) {
            if let Ok(mut stmt) =
                conn.prepare("SELECT revision, workspace_id, kind FROM canonical_revisions")
            {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                }) {
                    canon_rows = rows.flatten().collect();
                }
            }
            if let Ok(mut stmt) = conn.prepare("SELECT projection, workspace_id, status, canonical_revision, projected_revision FROM projection_states") {
                if let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<i64>>(3)?, r.get::<_, Option<i64>>(4)?))) {
                    proj_rows = rows.flatten().collect();
                }
            }
        }
        let follower_stderr = std::fs::read_to_string(&err_file).unwrap_or_default();
        let follower_stdout = std::fs::read_to_string(&out_file).unwrap_or_default();
        panic!(
            "Timed out waiting for follower promotion and projection recovery: target_canonical={target_canonical}, current_canon={current_canon}, current_proj={current_proj}, db_path={:?}, child_status={child_status}, canon_rows={canon_rows:?}, proj_rows={proj_rows:?}, stderr={follower_stderr}, stdout={follower_stdout}",
            db_path
        );
    }

    pub async fn search(&self, query: &str) -> anyhow::Result<String> {
        let mut cmd = Command::new(&self.binary_path);
        cmd.args([
            "--workspace",
            self.workspace_root.to_str().unwrap(),
            "--json",
            "search",
            query,
        ]);
        cmd.env("JULIE_HOME", self.temp_home.path());
        cmd.env("TMPDIR", self.temp_home.path());
        cmd.env("JULIE_EMBEDDING_PROVIDER", "none");
        let output = cmd.output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Concise helper to open DB, Tantivy index, binding, and paths with zero boilerplate.
    pub fn open_stores(
        &self,
    ) -> (
        std::sync::Arc<std::sync::Mutex<julie_core::database::SymbolDatabase>>,
        std::sync::Arc<julie_index::search::SearchIndex>,
        crate::request_engine::types::WorkspaceBinding,
        RegistryPaths,
    ) {
        let paths = RegistryPaths::with_home(self.temp_home.path().to_path_buf());
        let binding = crate::request_engine::types::WorkspaceBinding {
            workspace_id: self.workspace_id.clone(),
            root: self.workspace_root.clone(),
            index_root: self.index_root.clone(),
        };
        let db_path = self.index_root.join("db").join("symbols.db");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let db = julie_core::database::SymbolDatabase::new(&db_path).unwrap();

        let tantivy_path = self.index_root.join("tantivy");
        std::fs::create_dir_all(&tantivy_path).unwrap();
        let index = julie_index::search::SearchIndex::open_or_create(&tantivy_path).unwrap();

        (
            std::sync::Arc::new(std::sync::Mutex::new(db)),
            std::sync::Arc::new(index),
            binding,
            paths,
        )
    }
}

impl Drop for RecoveryProcessFixture {
    fn drop(&mut self) {
        if let Some(mut child) = self.owner_child.take() {
            let _ = child.start_kill();
        }
        if let Some(mut child) = self.follower_child.take() {
            let _ = child.start_kill();
        }
    }
}

#[tokio::test]
async fn promoted_owner_repairs_projection_gap_without_source_change() {
    let mut fixture = RecoveryProcessFixture::new().await;
    fixture.owner_pause_after_canonical_commit().await;
    let canonical = fixture.canonical_revision();
    assert!(canonical > fixture.projected_revision());
    fixture.kill_owner().await;
    fixture.wait_for_follower_promotion().await;
    let result = fixture.search("recovered_symbol").await.unwrap();
    assert!(result.contains("recovered_symbol"));
    assert_eq!(fixture.projected_revision(), fixture.canonical_revision());
    assert_eq!(fixture.canonical_revision(), canonical);
}

#[tokio::test]
async fn publication_lock_blocks_concurrent_writers() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let lock_path = tmp.path().join("publication.lock");

    // 1. Multiple shared readers can acquire simultaneously without blocking
    let shared_1 =
        julie_core::workspace::publication_lock::PublicationLock::try_acquire_shared(&lock_path)
            .expect("first shared lock must succeed");
    let shared_2 =
        julie_core::workspace::publication_lock::PublicationLock::try_acquire_shared(&lock_path)
            .expect("concurrent shared lock must succeed");

    // While shared locks are held, exclusive writer must return WouldBlock
    let exclusive_attempt =
        julie_core::workspace::publication_lock::PublicationLock::try_acquire_exclusive(&lock_path);
    assert!(
        matches!(
            exclusive_attempt,
            Err(julie_core::workspace::publication_lock::PublicationLockError::WouldBlock)
        ),
        "exclusive acquire must return WouldBlock while shared locks are held: {exclusive_attempt:?}"
    );

    drop(shared_1);
    drop(shared_2);

    // 2. Writer acquires exclusive lock
    let exclusive_writer =
        julie_core::workspace::publication_lock::PublicationLock::try_acquire_exclusive(&lock_path)
            .expect("exclusive acquire must succeed when no holders exist");

    // 3. While exclusive writer holds lock, try_acquire_shared must return WouldBlock
    let reader_attempt =
        julie_core::workspace::publication_lock::PublicationLock::try_acquire_shared(&lock_path);
    assert!(
        matches!(
            reader_attempt,
            Err(julie_core::workspace::publication_lock::PublicationLockError::WouldBlock)
        ),
        "shared acquire must return WouldBlock while exclusive writer holds lock"
    );

    // Another exclusive writer must also return WouldBlock
    let second_writer_attempt =
        julie_core::workspace::publication_lock::PublicationLock::try_acquire_exclusive(&lock_path);
    assert!(
        matches!(
            second_writer_attempt,
            Err(julie_core::workspace::publication_lock::PublicationLockError::WouldBlock)
        ),
        "concurrent exclusive acquire must return WouldBlock"
    );

    // 4. Release exclusive lock: reader can acquire immediately
    drop(exclusive_writer);
    let shared_after =
        julie_core::workspace::publication_lock::PublicationLock::try_acquire_shared(&lock_path)
            .expect("shared acquire must succeed after exclusive writer drops");
    drop(shared_after);

    // Invariant: Lock file is never unlinked
    assert!(
        lock_path.exists(),
        "publication.lock must never be unlinked"
    );
}

#[tokio::test]
async fn reader_snapshot_detects_projection_lag() {
    use crate::request_engine::types::WorkspaceBinding;
    use crate::workspace_runtime::publication::{SnapshotError, WorkspaceReadSnapshot};
    use julie_core::database::{ProjectionStatus, SymbolDatabase};
    use julie_index::search::SearchIndex;

    let fixture = RecoveryProcessFixture::new().await;
    let paths = RegistryPaths::with_home(fixture.temp_home.path().to_path_buf());
    let binding = WorkspaceBinding {
        workspace_id: fixture.workspace_id.clone(),
        root: fixture.workspace_root.clone(),
        index_root: fixture.index_root.clone(),
    };

    let db_path = fixture.index_root.join("db").join("symbols.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = SymbolDatabase::new(&db_path).unwrap();

    let tantivy_path = fixture.index_root.join("tantivy");
    std::fs::create_dir_all(&tantivy_path).unwrap();
    let index = SearchIndex::open_or_create(&tantivy_path).unwrap();

    // Setup: Seed file info and record 2 canonical revisions via SQLite
    let file = julie_core::database::FileInfo {
        path: "src/probe.rs".to_string(),
        language: "rust".to_string(),
        hash: hex::encode(blake3::hash(b"pub fn probe() {}").as_bytes()),
        size: 17,
        last_modified: 1000,
        last_indexed: 1000,
        symbol_count: 1,
        line_count: 1,
        content: Some("pub fn probe() {}".to_string()),
    };
    db.store_file_info(&file).unwrap();

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO canonical_revisions (revision, workspace_id, kind, cleaned_file_count, file_count, symbol_count, relationship_count, identifier_count, type_count, created_at)
         VALUES (1, ?1, 'fresh', 0, 1, 1, 0, 0, 0, 1000)",
        rusqlite::params![&fixture.workspace_id],
    ).unwrap();
    conn.execute(
        "INSERT INTO canonical_revisions (revision, workspace_id, kind, cleaned_file_count, file_count, symbol_count, relationship_count, identifier_count, type_count, created_at)
         VALUES (2, ?1, 'incremental', 0, 1, 1, 0, 0, 0, 2000)",
        rusqlite::params![&fixture.workspace_id],
    ).unwrap();

    // Simulate lag: canonical is 2, but projected is 1
    db.upsert_projection_state(
        "tantivy",
        &fixture.workspace_id,
        ProjectionStatus::Ready,
        Some(2),
        Some(1),
        None,
    )
    .unwrap();

    // Reader attempts snapshot acquisition and must detect PROJECTION_LAG
    let err = WorkspaceReadSnapshot::acquire(&binding, &paths).await;
    assert!(
        matches!(
            err,
            Err(SnapshotError::ProjectionLag {
                canonical: 2,
                projected: Some(1),
                ..
            })
        ),
        "Reader must detect projection lag when canonical (2) > projected (1)"
    );

    // Reconcile projection: update projected_revision to 2 and commit Tantivy payload
    let payload = serde_json::to_string(&julie_core::workspace::ownership::TantivyCommitPayload {
        epoch: 1,
        revision: 2,
        generation: 1,
    })
    .unwrap();
    index.commit_with_payload(&payload).unwrap();
    db.upsert_projection_state(
        "tantivy",
        &fixture.workspace_id,
        ProjectionStatus::Ready,
        Some(2),
        Some(2),
        None,
    )
    .unwrap();

    // Now reader snapshot acquisition succeeds cleanly
    let snapshot = WorkspaceReadSnapshot::acquire(&binding, &paths)
        .await
        .expect("Snapshot acquisition must succeed once projection is caught up");
    assert_eq!(snapshot.stamp().canonical_revision, 2);
    assert_eq!(snapshot.stamp().projected_revision, 2);
}

#[tokio::test]
async fn owner_epoch_drains_permits_before_demotion() {
    use julie_core::workspace::leader_lock::DaemonLockGuard;
    use julie_core::workspace::mutation_gate::Registry;
    use julie_core::workspace::ownership::{OwnerEpoch, OwnershipError};
    use std::time::Duration;

    let tmp = tempfile::tempdir().expect("create tempdir");
    let lock_path = tmp.path().join("leader.lock");
    let guard = DaemonLockGuard::try_acquire(&lock_path).expect("acquire leader lock");

    let epoch = std::sync::Arc::new(OwnerEpoch::new(1, "test_ws".to_string(), guard));
    let registry = Registry::new();

    // 1. Acquire writer permit
    let permit = epoch
        .acquire_writer(&registry)
        .await
        .expect("acquire writer permit");
    assert_eq!(epoch.active_permits_count(), 1);
    assert!(!epoch.is_draining());

    // 2. Spawn task to drain permits
    let epoch_clone = std::sync::Arc::clone(&epoch);
    let drain_handle =
        tokio::spawn(async move { epoch_clone.drain_permits(Duration::from_secs(2)).await });

    // Small yield so drain_permits marks draining
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(epoch.is_draining());

    // 3. While draining, new permit requests are immediately rejected
    let second_permit = epoch.acquire_writer(&registry).await;
    assert!(
        matches!(second_permit, Err(OwnershipError::Draining)),
        "New permits must be rejected while draining"
    );

    // 4. Dropping active permit allows drain to finish successfully
    drop(permit);
    let drain_result = drain_handle.await.expect("join drain task");
    assert!(
        drain_result.is_ok(),
        "drain_permits must succeed after active permits hit 0"
    );
    assert_eq!(epoch.active_permits_count(), 0);
}

#[tokio::test]
async fn pre_commit_source_hash_recheck_catches_concurrent_modification() {
    use crate::tools::workspace::indexing::pipeline::verify_source_hashes_before_commit;
    use julie_core::workspace::ownership::SourceCheckState;

    let tmp = tempfile::tempdir().expect("create tempdir");
    let root = tmp.path();
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let file_path = src_dir.join("racy.rs");

    let initial_content = b"pub fn clean_fn() {}\n";
    std::fs::write(&file_path, initial_content).unwrap();

    let initial_hash = hex::encode(blake3::hash(initial_content).as_bytes());
    let file_info = julie_core::database::FileInfo {
        path: "src/racy.rs".to_string(),
        language: "rust".to_string(),
        hash: initial_hash,
        size: initial_content.len() as i64,
        last_modified: 1000,
        last_indexed: 1000,
        symbol_count: 1,
        line_count: 1,
        content: Some(String::from_utf8_lossy(initial_content).to_string()),
    };

    // 1. File matches hash on disk -> Verified
    let state = verify_source_hashes_before_commit(root, &[file_info.clone()]);
    assert_eq!(state, SourceCheckState::Verified { files_checked: 1 });

    // 2. File modified concurrently before commit -> HashMismatch
    std::fs::write(&file_path, b"pub fn modified_concurrently() {}\n").unwrap();
    let state_mod = verify_source_hashes_before_commit(root, &[file_info.clone()]);
    assert_eq!(state_mod, SourceCheckState::HashMismatch);

    // 3. File deleted concurrently before commit -> HashMismatch
    std::fs::remove_file(&file_path).unwrap();
    let state_del = verify_source_hashes_before_commit(root, &[file_info.clone()]);
    assert_eq!(state_del, SourceCheckState::HashMismatch);
}

fn seed_coherent_revision(
    workspace_id: &str,
    db: &std::sync::Arc<std::sync::Mutex<julie_core::database::SymbolDatabase>>,
    index: &std::sync::Arc<julie_index::search::SearchIndex>,
    revision: u64,
) {
    let db_guard = db.lock().unwrap();
    db_guard
        .conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, cleaned_file_count, file_count, symbol_count, relationship_count, identifier_count, type_count, created_at)
             VALUES (?1, ?2, 'fresh', 0, 1, 1, 0, 0, 0, 1000)",
            rusqlite::params![revision as i64, workspace_id],
        )
        .unwrap();
    db_guard
        .upsert_projection_state(
            "tantivy",
            workspace_id,
            julie_core::database::ProjectionStatus::Ready,
            Some(revision as i64),
            Some(revision as i64),
            None,
        )
        .unwrap();
    let payload = serde_json::to_string(&julie_core::workspace::ownership::TantivyCommitPayload {
        epoch: 1,
        revision,
        generation: revision,
    })
    .unwrap();
    index.commit_with_payload(&payload).unwrap();
}

fn make_test_symbol(id: &str, name: &str, file_path: &str) -> julie_core::Symbol {
    julie_core::Symbol {
        extracted: julie_extractors::Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: julie_extractors::SymbolKind::Function,
            language: "rust".to_string(),
            file_path: file_path.to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 20,
            start_byte: 0,
            end_byte: 20,
            parent_id: None,
            signature: None,
            doc_comment: None,
            visibility: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            content_type: None,
            body_span: None,
            body_hash: None,
            annotations: Vec::new(),
        },
        code_context: None,
    }
}

#[tokio::test]
async fn pre_commit_hash_mismatch_excludes_and_requeues_modified_file() {
    use crate::tools::workspace::indexing::pipeline::verify_source_hashes_before_commit;
    use crate::tools::workspace::indexing::route::IndexRoute;
    use crate::tools::workspace::indexing::state::IndexingOperation;
    use julie_core::workspace::ownership::SourceCheckState;
    use std::collections::HashMap;

    let fixture = RecoveryProcessFixture::new().await;
    let root = &fixture.workspace_root;
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();

    let intact_path = src.join("intact.rs");
    let racy_path = src.join("racy.rs");
    std::fs::write(&intact_path, "pub fn intact_symbol() -> i32 { 1 }\n").unwrap();
    std::fs::write(&racy_path, "pub fn stale_racy_symbol() -> i32 { 2 }\n").unwrap();

    let (db, _index, _binding, _paths) = fixture.open_stores();

    // 1. Extract batch containing both files
    let files_by_lang = HashMap::from([(
        "rust".to_string(),
        vec![intact_path.clone(), racy_path.clone()],
    )]);
    let (batch, _records) =
        crate::indexing_core::extraction::extract_files_for_indexing_with_records(
            files_by_lang,
            root,
        )
        .await
        .unwrap();

    // Verify extraction captured both symbols initially
    assert!(batch.all_symbols.iter().any(|s| s.name == "intact_symbol"));
    assert!(
        batch
            .all_symbols
            .iter()
            .any(|s| s.name == "stale_racy_symbol")
    );

    // 2. Simulate concurrent edit to racy.rs on disk BEFORE commit
    std::fs::write(&racy_path, "pub fn updated_racy_symbol() -> i32 { 99 }\n").unwrap();

    // 3. Pre-commit check detects mismatch
    let check = verify_source_hashes_before_commit(root, &batch.all_file_infos);
    assert_eq!(check, SourceCheckState::HashMismatch);

    // 4. Filter batch: exclude mismatched file(s) and collect for requeue
    let (clean_batch, requeued) =
        crate::tools::workspace::indexing::pipeline::filter_mismatched_batch_files(root, batch);
    assert_eq!(requeued, vec![racy_path.clone()]);
    assert_eq!(clean_batch.all_file_infos.len(), 1);
    assert_eq!(clean_batch.all_file_infos[0].path, "src/intact.rs");

    // 5. Persist clean batch to SQLite
    let route = IndexRoute::for_test(&fixture.workspace_id, root, &fixture.index_root);
    let persist_res = crate::tools::workspace::indexing::pipeline::persist_batch_for_test(
        &db,
        &route,
        IndexingOperation::Incremental,
        &clean_batch,
    )
    .unwrap();
    assert_eq!(persist_res.canonical_revision, Some(1));

    // Assert: intact symbols committed; racy symbols NOT committed
    let db_guard = db.lock().unwrap();
    let symbols = db_guard.get_all_symbols().unwrap();
    assert!(
        symbols.iter().any(|s| s.name == "intact_symbol"),
        "intact_symbol must be committed"
    );
    assert!(
        !symbols.iter().any(|s| s.name == "stale_racy_symbol"),
        "stale_racy_symbol must NOT be committed"
    );
    assert!(
        !symbols.iter().any(|s| s.name == "updated_racy_symbol"),
        "updated_racy_symbol not committed yet"
    );
    drop(db_guard);

    // 6. Subsequent indexing picks up the requeued file with its new content
    let requeued_by_lang = HashMap::from([("rust".to_string(), requeued)]);
    let (batch_2, _) = crate::indexing_core::extraction::extract_files_for_indexing_with_records(
        requeued_by_lang,
        root,
    )
    .await
    .unwrap();

    let persist_res_2 = crate::tools::workspace::indexing::pipeline::persist_batch_for_test(
        &db,
        &route,
        IndexingOperation::Incremental,
        &batch_2,
    )
    .unwrap();
    assert_eq!(persist_res_2.canonical_revision, Some(2));

    // Assert: updated racy symbol is now committed at revision 2
    let db_guard = db.lock().unwrap();
    let symbols_after = db_guard.get_all_symbols().unwrap();
    assert!(
        symbols_after
            .iter()
            .any(|s| s.name == "updated_racy_symbol"),
        "updated_racy_symbol must be committed"
    );
    assert!(
        !symbols_after.iter().any(|s| s.name == "stale_racy_symbol"),
        "stale symbol never entered database"
    );
}

#[tokio::test]
async fn reader_snapshot_and_writer_publication_lock_mutual_exclusion() {
    use crate::workspace_runtime::publication::{
        SnapshotError, WorkspaceReadSnapshot, acquire_exclusive_publication_lock,
        try_acquire_exclusive_publication_lock,
    };
    use julie_core::workspace::publication_lock::PublicationLockError;

    let fixture = RecoveryProcessFixture::new().await;
    let (db, index, binding, paths) = fixture.open_stores();

    // Seed matched canonical and projected state at revision 1
    seed_coherent_revision(&fixture.workspace_id, &db, &index, 1);

    // --- Scenario A: Active Reader Blocks Publishing Writer ---
    let reader_snapshot = WorkspaceReadSnapshot::acquire(&binding, &paths)
        .await
        .expect("reader snapshot acquisition must succeed");

    // While reader snapshot is retained, writer cannot acquire exclusive publication lock
    let writer_attempt = try_acquire_exclusive_publication_lock(&fixture.index_root);
    assert!(
        matches!(writer_attempt, Err(PublicationLockError::WouldBlock)),
        "Active reader holding snapshot must cause writer exclusive acquire to return WouldBlock: {writer_attempt:?}"
    );

    // Dropping reader snapshot releases shared publication lock
    drop(reader_snapshot);

    // Writer exclusive publication lock succeeds immediately after reader drops
    let writer_guard = acquire_exclusive_publication_lock(&fixture.index_root)
        .expect("Writer exclusive publication lock must succeed after reader drops snapshot");

    // --- Scenario B: Active Publishing Writer Blocks Reader Snapshot ---
    // Reader with short deadline must fail while writer holds exclusive publication lock
    let deadline = Instant::now() + Duration::from_millis(50);
    let reader_attempt =
        WorkspaceReadSnapshot::acquire_with_deadline(&binding, &paths, deadline).await;
    assert!(
        matches!(
            reader_attempt,
            Err(SnapshotError::Lock(PublicationLockError::Timeout(_)))
                | Err(SnapshotError::Lock(PublicationLockError::WouldBlock))
        ),
        "Reader snapshot must fail with Lock Timeout or WouldBlock while writer holds exclusive lock: {reader_attempt:?}"
    );

    // Dropping writer guard permits reader snapshot acquisition to succeed
    drop(writer_guard);
    let snapshot_after = WorkspaceReadSnapshot::acquire(&binding, &paths)
        .await
        .expect("Reader snapshot acquisition must succeed after writer releases exclusive lock");
    assert_eq!(snapshot_after.stamp().canonical_revision, 1);
}

#[tokio::test]
async fn tantivy_commit_payload_matches_canonical_revision_in_production() {
    use crate::workspace_runtime::publication::WorkspaceReadSnapshot;
    use julie_core::workspace::ownership::TantivyCommitPayload;

    let fixture = RecoveryProcessFixture::new().await;
    let (db, index, binding, paths) = fixture.open_stores();

    // 1. Perform production-path projection via SearchProjection
    let file = julie_core::database::FileInfo {
        path: "src/lib.rs".to_string(),
        language: "rust".to_string(),
        hash: hex::encode(blake3::hash(b"pub fn payload_test() {}").as_bytes()),
        size: 25,
        last_modified: 1000,
        last_indexed: 1000,
        symbol_count: 1,
        line_count: 1,
        content: Some("pub fn payload_test() {}".to_string()),
    };
    db.lock().unwrap().store_file_info(&file).unwrap();

    let symbol = make_test_symbol("payload_test", "payload_test", "src/lib.rs");

    // Record canonical revision 1 in SQLite
    db.lock()
        .unwrap()
        .conn
        .execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, cleaned_file_count, file_count, symbol_count, relationship_count, identifier_count, type_count, created_at)
             VALUES (1, ?1, 'fresh', 0, 1, 1, 0, 0, 0, 1000)",
            rusqlite::params![fixture.workspace_id],
        )
        .unwrap();

    // Execute production project_documents_with_locks with canonical revision 1
    crate::search::SearchProjection::tantivy(fixture.workspace_id.clone())
        .project_documents_with_locks(&db, &index, &[symbol], &[file], &[], Some(1))
        .expect("production project_documents_with_locks must succeed");

    // 2. Inspect Tantivy commit metadata from disk
    let searcher = index.reader().searcher();
    let metas = searcher
        .index()
        .load_metas()
        .expect("must load Tantivy index metas");

    assert!(
        metas.payload.is_some(),
        "Tantivy commit metadata must contain payload in production"
    );
    let payload_str = metas.payload.as_deref().unwrap();
    let payload: TantivyCommitPayload =
        serde_json::from_str(payload_str).expect("Tantivy payload must be valid JSON");

    // 3. Assert Tantivy payload matches SQLite canonical revision
    assert_eq!(
        payload.revision, 1,
        "Tantivy payload revision must match canonical revision 1"
    );
    assert!(payload.epoch > 0, "Tantivy payload epoch must be > 0");
    assert!(
        payload.generation > 0,
        "Tantivy payload generation must be > 0"
    );

    // 4. Assert WorkspaceReadSnapshot accepts matching payload without ProjectionLag
    let snapshot = WorkspaceReadSnapshot::acquire(&binding, &paths)
        .await
        .expect("Snapshot acquire must succeed with matching Tantivy payload");
    assert_eq!(snapshot.stamp().canonical_revision, 1);
    assert_eq!(snapshot.stamp().projected_revision, 1);
}

#[tokio::test]
async fn writer_permit_enforcement_across_writer_apis() {
    use julie_core::workspace::leader_lock::DaemonLockGuard;
    use julie_core::workspace::mutation_gate::Registry as GateRegistry;
    use julie_core::workspace::ownership::{OwnerEpoch, OwnershipError};

    let fixture = RecoveryProcessFixture::new().await;
    let (db, index, _binding, _paths) = fixture.open_stores();
    let leader_lock_path = fixture.temp_home.path().join("leader.lock");

    // 1. Owner acquires leader lock and constructs OwnerEpoch
    let lock_guard =
        DaemonLockGuard::try_acquire(&leader_lock_path).expect("Owner acquires leader lock");
    let epoch = std::sync::Arc::new(OwnerEpoch::new(1, fixture.workspace_id.clone(), lock_guard));
    let gate_registry = GateRegistry::new();

    // 2. Permitted execution: Acquire valid WriterPermit
    let permit = epoch
        .acquire_writer(&gate_registry)
        .await
        .expect("Owner must be able to acquire WriterPermit");

    // Call writer API with valid &WriterPermit<'_> -> Succeeds
    let reconciler = crate::workspace_runtime::recovery::ProjectionRecoveryCoordinator::new(
        fixture.workspace_id.clone(),
    );
    let reconcile_res = reconciler.reconcile_if_needed(&db, &index, &permit).await;
    assert!(
        reconcile_res.is_ok(),
        "reconcile_if_needed must succeed with valid permit"
    );

    // 3. Draining rejection: Demoting or draining epoch blocks permit issuance
    let epoch_clone = std::sync::Arc::clone(&epoch);
    tokio::spawn(async move { epoch_clone.drain_permits(Duration::from_secs(1)).await });
    tokio::time::sleep(Duration::from_millis(25)).await;

    let blocked_permit = epoch.acquire_writer(&gate_registry).await;
    assert!(
        matches!(blocked_permit, Err(OwnershipError::Draining)),
        "Draining epoch must reject writer permit requests: {blocked_permit:?}"
    );

    // 4. Follower rejection: Process without leader lock cannot obtain WriterPermit
    // Leader lock is currently held by owner lock_guard inside epoch
    let follower_lock_attempt = DaemonLockGuard::try_acquire(&leader_lock_path);
    assert!(
        follower_lock_attempt.is_err(),
        "Follower must be unable to acquire leader lock while owner is active"
    );

    // Drop active permit to let drain complete cleanly
    drop(permit);
    assert_eq!(epoch.active_permits_count(), 0);
}
