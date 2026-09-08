//! src/tests/workspace_process_lifecycle.rs
//! Multi-process worktree acceptance, dynamic leader promotion, and fault matrix tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::Instant;

use crate::tests::request_process_helpers::{ProcessFixture, resolve_julie_binary};
use julie_core::workspace::leader_lock::{AcquireError, DaemonLockGuard};
use julie_core::workspace::ownership::{RuntimePhaseKind, allowed_transition};
use julie_core::workspace::registry::generate_workspace_id;

/// Test helper providing modern MCP request metadata.
fn modern_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {
            "name": "lifecycle-test",
            "version": "1.0.0"
        }
    })
}

/// Hermetic multi-process fixture managing a git repo, two real detached git worktrees,
/// shared isolated JULIE_HOME, and three spawned julie-server subprocesses.
pub struct WorktreeProcessFixture {
    pub temp_repo: Arc<TempDir>,
    pub temp_home: Arc<TempDir>,
    pub main_repo: PathBuf,
    pub left_worktree: PathBuf,
    pub right_worktree: PathBuf,
    pub left_workspace_id: String,
    pub right_workspace_id: String,
    pub left_owner: Option<ProcessFixture>,
    pub left_follower: ProcessFixture,
    pub right_owner: ProcessFixture,
}

impl WorktreeProcessFixture {
    /// Initialize real git repo with two detached worktrees and start three client processes.
    pub async fn start_three_clients() -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let _ = std::fs::create_dir_all(&tmp_base);

        let temp_repo = Arc::new(tempfile::tempdir_in(&tmp_base).expect("create temp repo"));
        let temp_home = Arc::new(tempfile::tempdir_in(&tmp_base).expect("create temp home"));

        let base = temp_repo.path();
        let main_repo = base.join("main_repo");
        let left_worktree = base.join("worktree_left");
        let right_worktree = base.join("worktree_right");

        // 1. Initialize main git repository
        std::fs::create_dir_all(&main_repo).expect("create main repo");
        let run_git = |args: &[&str], dir: &Path| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap_or_else(|e| panic!("failed to run git {:?}: {e}", args));
            assert!(status.success(), "git command failed: {:?}", args);
        };

        run_git(&["init", "-b", "main"], &main_repo);
        run_git(&["config", "user.email", "test@julie.local"], &main_repo);
        run_git(&["config", "user.name", "Julie Test"], &main_repo);

        // 2. Initial commit with base files
        std::fs::write(
            main_repo.join("Cargo.toml"),
            "[package]\nname = \"wt-seed\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write Cargo.toml");
        std::fs::create_dir_all(main_repo.join("src")).expect("create src dir");
        std::fs::write(
            main_repo.join("src/lib.rs"),
            "//! Base library\npub fn base_fn() -> i32 { 42 }\n",
        )
        .expect("write src/lib.rs");

        run_git(&["add", "."], &main_repo);
        run_git(&["commit", "-m", "initial commit"], &main_repo);

        // 3. Create real detached worktrees
        run_git(
            &[
                "worktree",
                "add",
                "--detach",
                left_worktree.to_str().unwrap(),
            ],
            &main_repo,
        );
        run_git(
            &[
                "worktree",
                "add",
                "--detach",
                right_worktree.to_str().unwrap(),
            ],
            &main_repo,
        );

        // 4. Seed distinct contents with identical relative path: src/service.rs
        let left_service = r#"//! Left worktree service
pub fn left_old() -> &'static str {
    "left_content_original"
}
"#;
        std::fs::write(left_worktree.join("src/service.rs"), left_service)
            .expect("write left service");

        let right_service = r#"//! Right worktree service
pub fn right_old() -> &'static str {
    "right_content_original"
}
"#;
        std::fs::write(right_worktree.join("src/service.rs"), right_service)
            .expect("write right service");

        let left_id = generate_workspace_id(&left_worktree.to_string_lossy()).expect("left id");
        let right_id = generate_workspace_id(&right_worktree.to_string_lossy()).expect("right id");

        let binary = resolve_julie_binary();

        // 5. Start Left Owner
        let mut left_owner = ProcessFixture::with_custom_roots(
            binary.clone(),
            left_worktree.clone(),
            Arc::clone(&temp_home),
            Arc::clone(&temp_repo),
        )
        .await;

        // Initially index Left worktree
        let _ = left_owner
            .rpc(json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "manage_workspace",
                    "arguments": {
                        "operation": "index",
                        "workspace": left_worktree.to_str().unwrap()
                    },
                    "_meta": modern_meta()
                }
            }))
            .await;

        // Give Left Owner a moment to settle
        tokio::time::sleep(Duration::from_millis(50)).await;

        // 6. Start Left Follower (lock busy -> runs in Follower role)
        let left_follower = ProcessFixture::with_custom_roots(
            binary.clone(),
            left_worktree.clone(),
            Arc::clone(&temp_home),
            Arc::clone(&temp_repo),
        )
        .await;

        // 7. Start Right Owner (different workspace -> acquires own lock)
        let mut right_owner = ProcessFixture::with_custom_roots(
            binary,
            right_worktree.clone(),
            Arc::clone(&temp_home),
            Arc::clone(&temp_repo),
        )
        .await;

        // Initially index Right worktree
        let _ = right_owner
            .rpc(json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "manage_workspace",
                    "arguments": {
                        "operation": "index",
                        "workspace": right_worktree.to_str().unwrap()
                    },
                    "_meta": modern_meta()
                }
            }))
            .await;

        Self {
            temp_repo,
            temp_home,
            main_repo,
            left_worktree,
            right_worktree,
            left_workspace_id: left_id,
            right_workspace_id: right_id,
            left_owner: Some(left_owner),
            left_follower,
            right_owner,
        }
    }

    pub fn left_workspace_id(&self) -> &str {
        &self.left_workspace_id
    }

    pub fn right_workspace_id(&self) -> &str {
        &self.right_workspace_id
    }

    pub fn left_root_str(&self) -> &str {
        self.left_worktree.to_str().unwrap()
    }

    pub fn right_root_str(&self) -> &str {
        self.right_worktree.to_str().unwrap()
    }

    /// Execute source edit through the Left Follower connection.
    pub async fn left_follower_edit(
        &mut self,
        old_text: &str,
        new_text: &str,
    ) -> Result<Value, String> {
        let ws = self.left_root_str().to_string();
        let res = self
            .left_follower
            .rpc(json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "edit_file",
                    "arguments": {
                        "file_path": "src/service.rs",
                        "old_text": old_text,
                        "new_text": new_text,
                        "dry_run": false,
                        "workspace": ws
                    },
                    "_meta": modern_meta()
                }
            }))
            .await;

        if let Some(err) = res.get("error") {
            Err(err.to_string())
        } else if res["result"]["isError"].as_bool().unwrap_or(false) {
            Err(res["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("error")
                .to_string())
        } else {
            Ok(res)
        }
    }

    /// Execute source edit through the Right Owner connection.
    pub async fn right_owner_edit(
        &mut self,
        old_text: &str,
        new_text: &str,
    ) -> Result<Value, String> {
        let ws = self.right_root_str().to_string();
        let res = self
            .right_owner
            .rpc(json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "edit_file",
                    "arguments": {
                        "file_path": "src/service.rs",
                        "old_text": old_text,
                        "new_text": new_text,
                        "dry_run": false,
                        "workspace": ws
                    },
                    "_meta": modern_meta()
                }
            }))
            .await;

        if let Some(err) = res.get("error") {
            Err(err.to_string())
        } else if res["result"]["isError"].as_bool().unwrap_or(false) {
            Err(res["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("error")
                .to_string())
        } else {
            Ok(res)
        }
    }

    /// Bounded polling until changes are visible in search indexes.
    pub async fn wait_for_current_indexes(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let left_text = self.left_search("left_new").await;
            let right_text = self.right_search("right_new").await;
            if left_text.contains("src/service.rs") && right_text.contains("src/service.rs") {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Fast search query on the left worktree.
    pub async fn left_search(&mut self, query: &str) -> String {
        let ws = self.left_root_str().to_string();
        let res = self
            .left_follower
            .rpc(json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "fast_search",
                    "arguments": { "query": query, "workspace": ws, "limit": 5 },
                    "_meta": modern_meta()
                }
            }))
            .await;
        res["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    /// Fast search query on the right worktree.
    pub async fn right_search(&mut self, query: &str) -> String {
        let ws = self.right_root_str().to_string();
        let res = self
            .right_owner
            .rpc(json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": "fast_search",
                    "arguments": { "query": query, "workspace": ws, "limit": 5 },
                    "_meta": modern_meta()
                }
            }))
            .await;
        res["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    /// Returns 1 if lock contention is active and verified by an external lock attempt; otherwise 0.
    pub fn active_owners_for_left(&self) -> usize {
        let lock_path = self
            .temp_home
            .path()
            .join("indexes")
            .join(&self.left_workspace_id)
            .join("leader.lock");
        match DaemonLockGuard::try_acquire(&lock_path) {
            Err(AcquireError::AlreadyHeld(_)) => 1,
            Ok(_) => 0,
            Err(_) => 0,
        }
    }

    /// Returns 1 if lock contention is active and verified by an external lock attempt; otherwise 0.
    pub fn active_owners_for_right(&self) -> usize {
        let lock_path = self
            .temp_home
            .path()
            .join("indexes")
            .join(&self.right_workspace_id)
            .join("leader.lock");
        match DaemonLockGuard::try_acquire(&lock_path) {
            Err(AcquireError::AlreadyHeld(_)) => 1,
            Ok(_) => 0,
            Err(_) => 0,
        }
    }

    /// Gracefully stop the Left Owner process to trigger failover.
    pub async fn stop_left_owner(&mut self) {
        if let Some(mut owner) = self.left_owner.take() {
            owner.shutdown().await;
        }
    }

    /// Wait until Left Follower completes dynamic leader election and promotes to Owner.
    pub async fn wait_for_left_follower_promotion(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if self.active_owners_for_left() == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("Timed out waiting for left follower promotion to owner");
    }

    /// Cleanly shut down all child processes and remove task-owned worktrees.
    pub async fn shutdown(&mut self) {
        if let Some(mut owner) = self.left_owner.take() {
            owner.shutdown().await;
        }
        self.left_follower.shutdown().await;
        self.right_owner.shutdown().await;

        let _ = std::process::Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                self.left_worktree.to_str().unwrap(),
            ])
            .current_dir(&self.main_repo)
            .status();
        let _ = std::process::Command::new("git")
            .args([
                "worktree",
                "remove",
                "--force",
                self.right_worktree.to_str().unwrap(),
            ])
            .current_dir(&self.main_repo)
            .status();
    }
}

// ============================================================================
// 1. Core Transition Table Unit Test
// ============================================================================

#[test]
fn ownership_transition_table_never_skips_recovery() {
    assert!(allowed_transition(
        RuntimePhaseKind::Follower,
        RuntimePhaseKind::Recovering
    ));
    assert!(!allowed_transition(
        RuntimePhaseKind::Follower,
        RuntimePhaseKind::Owner
    ));
    assert!(allowed_transition(
        RuntimePhaseKind::Recovering,
        RuntimePhaseKind::Owner
    ));
    assert!(!allowed_transition(
        RuntimePhaseKind::Draining,
        RuntimePhaseKind::Owner
    ));
    assert!(!allowed_transition(
        RuntimePhaseKind::Failed,
        RuntimePhaseKind::Owner
    ));
    assert!(!allowed_transition(
        RuntimePhaseKind::Opening,
        RuntimePhaseKind::Owner
    ));
}

// ============================================================================
// 2. Multi-Worktree Multi-Process Acceptance Test
// ============================================================================

#[tokio::test]
async fn concurrent_worktrees_keep_sources_indexes_and_owners_isolated() {
    let mut fixture = WorktreeProcessFixture::start_three_clients().await;
    assert_ne!(fixture.left_workspace_id(), fixture.right_workspace_id());

    // Follower edit on Left, Owner edit on Right
    fixture
        .left_follower_edit("left_old", "left_new")
        .await
        .unwrap();
    fixture
        .right_owner_edit("right_old", "right_new")
        .await
        .unwrap();
    fixture.wait_for_current_indexes().await;

    // Verify search isolation: Left sees left_new in src/service.rs, Right does NOT
    assert!(
        fixture
            .left_search("left_new")
            .await
            .contains("src/service.rs")
    );
    assert!(
        !fixture
            .right_search("left_new")
            .await
            .contains("src/service.rs")
    );
    assert!(
        fixture
            .right_search("right_new")
            .await
            .contains("src/service.rs")
    );
    assert!(
        !fixture
            .left_search("right_new")
            .await
            .contains("src/service.rs")
    );

    // Exactly 1 verified owner lock per workspace
    assert_eq!(fixture.active_owners_for_left(), 1);
    assert_eq!(fixture.active_owners_for_right(), 1);

    // Stop Left Owner -> Left Follower dynamically promotes to Owner
    fixture.stop_left_owner().await;
    fixture.wait_for_left_follower_promotion().await;

    // Both workspaces still have exactly 1 active owner
    assert_eq!(fixture.active_owners_for_left(), 1);
    assert_eq!(fixture.active_owners_for_right(), 1);

    fixture.shutdown().await;
}

// ============================================================================
// 3. Integrated Fault Matrix Tests
// ============================================================================

#[tokio::test]
async fn test_fault_matrix_abrupt_kill_promotes_surviving_follower() {
    let mut fixture = WorktreeProcessFixture::start_three_clients().await;
    assert_eq!(fixture.active_owners_for_left(), 1);

    // Abrupt kill without graceful drain
    fixture.left_owner = None;
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Follower must promote automatically
    fixture.wait_for_left_follower_promotion().await;
    assert_eq!(fixture.active_owners_for_left(), 1);
    assert_eq!(fixture.active_owners_for_right(), 1);

    fixture.shutdown().await;
}

#[tokio::test]
async fn test_fault_matrix_competing_edits_rejected_with_conflict() {
    let mut fixture = WorktreeProcessFixture::start_three_clients().await;
    let ws_str = fixture.left_root_str().to_string();

    let res1 = fixture
        .left_follower_edit(
            "pub fn left_old() -> &'static str",
            "pub fn concurrent_edit_1() -> &'static str",
        )
        .await;
    let edit2_args = json!({
        "file_path": "src/service.rs",
        "old_text": "pub fn left_old() -> &'static str",
        "new_text": "pub fn concurrent_edit_2() -> &'static str",
        "dry_run": false,
        "workspace": ws_str
    });
    let res2 = fixture
        .left_owner
        .as_mut()
        .unwrap()
        .cli_tool("edit_file", &edit2_args)
        .await;

    let success_count = (res1.is_ok() as usize) + (res2.is_success() as usize);
    assert_eq!(success_count, 1, "Exactly one competing edit must succeed");

    if let Err(ref err) = res1 {
        assert!(
            err.contains("CONFLICT") || err.contains("conflict") || err.contains("EDIT_CONFLICT")
        );
    } else {
        assert!(res2.stderr.contains("CONFLICT") || res2.stdout.contains("EDIT_CONFLICT"));
    }

    fixture.shutdown().await;
}

#[tokio::test]
async fn test_fault_matrix_removed_worktree_returns_workspace_missing() {
    let mut fixture = WorktreeProcessFixture::start_three_clients().await;

    // Delete left worktree directory
    let left_path = fixture.left_worktree.clone();
    let left_str = fixture.left_root_str().to_string();
    std::fs::remove_dir_all(&left_path).expect("remove left worktree dir");

    // Query deleted worktree
    let out = fixture
        .right_owner
        .cli_tool(
            "fast_search",
            &json!({
                "query": "anything",
                "workspace": left_str
            }),
        )
        .await;

    assert!(!out.is_success());
    let json = out.stdout_json();
    let error_code = json["error"]["code"].as_str().unwrap_or("");
    assert_eq!(
        error_code, "WORKSPACE_MISSING",
        "Must reject non-existent workspace root with WORKSPACE_MISSING (got {error_code})"
    );

    fixture.shutdown().await;
}
