//! src/tests/runtime_continuation.rs
//! Durable continuation snapshot contract and multi-process persistence tests.

use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio::process::Command;

use crate::paths::RegistryPaths;
use crate::tests::request_process_helpers::resolve_julie_binary;
use crate::workspace_runtime::continuation::ContinuationBinding;
use crate::workspace_runtime::continuation_store::ContinuationStore;
use julie_context::SpilloverFormat;
use julie_core::workspace::registry::generate_workspace_id;
use julie_test_support::workspace_markers::make_isolated_workspace_root;

#[derive(Debug)]
pub struct FirstPage {
    pub token: String,
    pub expected_next_page: Vec<u8>,
}

#[derive(Debug)]
pub struct ContinuedPage {
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct ContinuationError {
    pub code: String,
    pub message: String,
}

pub struct ContinuationProcessFixture {
    pub temp_repo: TempDir,
    pub primary_workspace: PathBuf,
    pub other_workspace: PathBuf,
    pub temp_home: TempDir,
    pub binary_path: PathBuf,
}

impl ContinuationProcessFixture {
    pub async fn two_worktrees() -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let _ = std::fs::create_dir_all(&tmp_base);

        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("create temp repo");
        let primary_workspace = make_isolated_workspace_root(temp_repo.path(), "cont_primary")
            .canonicalize()
            .expect("canonicalize primary");
        let other_workspace = make_isolated_workspace_root(temp_repo.path(), "cont_other")
            .canonicalize()
            .expect("canonicalize other");

        // Seed primary workspace
        std::fs::create_dir_all(primary_workspace.join("src")).expect("create primary src");
        std::fs::write(
            primary_workspace.join("Cargo.toml"),
            "[package]\nname = \"primary\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write primary Cargo.toml");
        std::fs::write(
            primary_workspace.join("src/lib.rs"),
            "pub fn item_one() {}\npub fn item_two() {}\npub fn item_three() {}\n",
        )
        .expect("write primary lib.rs");

        // Seed other workspace
        std::fs::create_dir_all(other_workspace.join("src")).expect("create other src");
        std::fs::write(
            other_workspace.join("Cargo.toml"),
            "[package]\nname = \"other\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write other Cargo.toml");
        std::fs::write(
            other_workspace.join("src/lib.rs"),
            "pub fn other_item() {}\n",
        )
        .expect("write other lib.rs");

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("create temp home");
        let binary_path = resolve_julie_binary();

        Self {
            temp_repo,
            primary_workspace,
            other_workspace,
            temp_home,
            binary_path,
        }
    }

    /// Creates a continuation page in the primary workspace index root.
    pub async fn create_page_in_first_process(&self) -> FirstPage {
        let paths = RegistryPaths::with_home(self.temp_home.path().to_path_buf());
        let ws_id = generate_workspace_id(&self.primary_workspace.to_string_lossy())
            .expect("generate ws_id");
        let index_root = paths.workspace_index_dir(&ws_id);
        std::fs::create_dir_all(&index_root).expect("create index root");

        let store = ContinuationStore::open(&index_root).expect("open store");
        let binding = ContinuationBinding {
            workspace_id: ws_id,
            tool: "get_context".to_string(),
            arguments_hash: "args_hash_context".to_string(),
            generation: 1,
            source_hashes: "rev:1".to_string(),
        };

        let rows = vec!["item_two".to_string(), "item_three".to_string()];
        let token = store
            .create(
                binding,
                "Context Items".to_string(),
                rows,
                10,
                SpilloverFormat::Compact,
                None,
            )
            .expect("create snapshot")
            .expect("token emitted");

        // Expected output formatted as compact page
        let expected_next_page = b"Context Items\nitem_two\nitem_three".to_vec();

        FirstPage {
            token,
            expected_next_page,
        }
    }

    /// Spawns a brand new process reading the token from the primary workspace.
    pub async fn read_in_new_process(
        &self,
        token: String,
    ) -> Result<ContinuedPage, ContinuationError> {
        let mut cmd = Command::new(&self.binary_path);
        let params_json = serde_json::json!({
            "spillover_handle": token,
            "workspace": self.primary_workspace.to_str().unwrap()
        })
        .to_string();

        cmd.args([
            "tool",
            "spillover_get",
            "--params",
            &params_json,
            "--workspace",
            self.primary_workspace.to_str().unwrap(),
            "--json",
        ]);
        cmd.env("JULIE_HOME", self.temp_home.path());
        cmd.env("TMPDIR", self.temp_home.path());
        cmd.env("JULIE_EMBEDDING_PROVIDER", "none");
        cmd.current_dir(&self.primary_workspace);

        let output = cmd.output().await.expect("execute second process");
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);

        let ok = json["ok"].as_bool().unwrap_or(false);
        if output.status.success() && ok {
            let text = json["reply"]["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .trim_end();
            Ok(ContinuedPage {
                bytes: text.as_bytes().to_vec(),
            })
        } else {
            let code = json["error"]["code"]
                .as_str()
                .or_else(|| json["reply"]["error"]["code"].as_str())
                .unwrap_or("UNKNOWN_ERROR")
                .to_string();
            let message = json["error"]["message"]
                .as_str()
                .or_else(|| json["reply"]["error"]["message"].as_str())
                .unwrap_or("")
                .to_string();
            Err(ContinuationError { code, message })
        }
    }

    /// Spawns a process attempting to read the token from a different workspace root.
    pub async fn read_in_other_worktree(
        &self,
        token: String,
    ) -> Result<ContinuedPage, ContinuationError> {
        let mut cmd = Command::new(&self.binary_path);
        let params_json = serde_json::json!({
            "spillover_handle": token,
            "workspace": self.other_workspace.to_str().unwrap()
        })
        .to_string();

        cmd.args([
            "tool",
            "spillover_get",
            "--params",
            &params_json,
            "--workspace",
            self.other_workspace.to_str().unwrap(),
            "--json",
        ]);
        cmd.env("JULIE_HOME", self.temp_home.path());
        cmd.env("TMPDIR", self.temp_home.path());
        cmd.env("JULIE_EMBEDDING_PROVIDER", "none");
        cmd.current_dir(&self.other_workspace);

        let output = cmd.output().await.expect("execute third process");
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);

        let code = json["error"]["code"]
            .as_str()
            .or_else(|| json["reply"]["error"]["code"].as_str())
            .unwrap_or("UNKNOWN_ERROR")
            .to_string();
        let message = json["error"]["message"]
            .as_str()
            .or_else(|| json["reply"]["error"]["message"].as_str())
            .unwrap_or("")
            .to_string();
        Err(ContinuationError { code, message })
    }
}

// ----------------------------------------------------------------------------
// Task 5 Primary Failing RED Test
// ----------------------------------------------------------------------------

#[tokio::test]
async fn continuation_survives_new_process_but_rejects_other_worktree() {
    let fixture = ContinuationProcessFixture::two_worktrees().await;
    let first = fixture.create_page_in_first_process().await;
    let continued = fixture
        .read_in_new_process(first.token.clone())
        .await
        .unwrap();
    assert_eq!(continued.bytes, first.expected_next_page);
    let error = fixture
        .read_in_other_worktree(first.token)
        .await
        .unwrap_err();
    assert_eq!(error.code, "CONTINUATION_INVALID");
}

// ----------------------------------------------------------------------------
// Contract Test 1: Invalidation on Source Change / Generation Bump
// ----------------------------------------------------------------------------

#[tokio::test]
async fn continuation_fails_on_source_change_with_stale_code() {
    let fixture = ContinuationProcessFixture::two_worktrees().await;
    let first = fixture.create_page_in_first_process().await;

    // Mutate generation in SQLite database to simulate source invalidation
    let paths = RegistryPaths::with_home(fixture.temp_home.path().to_path_buf());
    let ws_id = generate_workspace_id(&fixture.primary_workspace.to_string_lossy()).unwrap();
    let db_path = paths.workspace_index_dir(&ws_id).join("continuations.db");

    if db_path.exists() {
        let conn = rusqlite::Connection::open(&db_path).expect("open continuations db");
        conn.execute("UPDATE continuation_snapshots SET generation = 999", [])
            .expect("bump generation");
    }

    let err = fixture.read_in_new_process(first.token).await.unwrap_err();
    assert_eq!(err.code, "CONTINUATION_STALE");
}

// ----------------------------------------------------------------------------
// Contract Test 2: Invalidation after TTL Expiry & Tombstone Distinguishability
// ----------------------------------------------------------------------------

#[tokio::test]
async fn continuation_expires_after_ttl_with_expired_code() {
    let fixture = ContinuationProcessFixture::two_worktrees().await;
    let first = fixture.create_page_in_first_process().await;

    // Manually advance expiry in SQLite database to simulate TTL expiration
    let paths = RegistryPaths::with_home(fixture.temp_home.path().to_path_buf());
    let ws_id = generate_workspace_id(&fixture.primary_workspace.to_string_lossy()).unwrap();
    let db_path = paths.workspace_index_dir(&ws_id).join("continuations.db");

    if db_path.exists() {
        let conn = rusqlite::Connection::open(&db_path).expect("open continuations db");
        conn.execute("UPDATE continuation_snapshots SET expires_at = 0", [])
            .expect("expire rows");
        conn.execute("UPDATE continuation_pages SET expires_at = 0", [])
            .expect("expire rows");
    }

    let err = fixture.read_in_new_process(first.token).await.unwrap_err();
    assert_eq!(err.code, "CONTINUATION_EXPIRED");

    // Compare with an arbitrary non-existent token (must return CONTINUATION_INVALID)
    let unknown = "0000000000000000000000000000000000000000000000000000000000000000".to_string();
    let unknown_err = fixture.read_in_new_process(unknown).await.unwrap_err();
    assert_eq!(unknown_err.code, "CONTINUATION_INVALID");
}

// ----------------------------------------------------------------------------
// Contract Test 3: LRU Eviction Under Storage Pressure
// ----------------------------------------------------------------------------

#[tokio::test]
async fn continuation_eviction_under_storage_pressure() {
    let tmp = tempfile::tempdir().unwrap();
    let store_path = tmp.path().join("continuations.db");

    // Low capacity budget: 64 KiB
    let max_budget_bytes = 64 * 1024;
    let store = ContinuationStore::with_max_snapshot(
        &store_path,
        max_budget_bytes,
        Duration::from_secs(900),
        32 * 1024,
    )
    .expect("open store");

    let b1 = ContinuationBinding::new("ws1", "tool1", "arg1", 1, "h1");
    let b2 = ContinuationBinding::new("ws1", "tool1", "arg2", 1, "h1");
    let b3 = ContinuationBinding::new("ws1", "tool1", "arg3", 1, "h1");

    let payload_25k = vec![b'x'; 25 * 1024];
    let t1 = store.create_raw(&b1, &payload_25k).expect("create t1");
    let t2 = store.create_raw(&b2, &payload_25k).expect("create t2");

    // Access t1 to update last_access
    let _ = store.read_raw(&t1, &b1).expect("access t1");

    // Create t3 (25 + 25 + 25 = 75 KiB > 64 KiB) -> triggers LRU eviction of t2
    let t3 = store.create_raw(&b3, &payload_25k).expect("create t3");

    let err_t2 = store.read_raw(&t2, &b2).unwrap_err();
    assert_eq!(err_t2.code(), "CONTINUATION_EXPIRED");

    assert!(store.read_raw(&t1, &b1).is_ok());
    assert!(store.read_raw(&t3, &b3).is_ok());
}

// ----------------------------------------------------------------------------
// Contract Test 4: Oversized Payload Rejection & Bounded Growth
// ----------------------------------------------------------------------------

#[tokio::test]
async fn continuation_oversized_payload_returns_truncated_result() {
    let tmp = tempfile::tempdir().unwrap();
    let store_path = tmp.path().join("continuations.db");

    // Max snapshot size: 100 KiB
    let max_snapshot_bytes = 100 * 1024;
    let store = ContinuationStore::with_max_snapshot(
        &store_path,
        64 * 1024 * 1024,
        Duration::from_secs(900),
        max_snapshot_bytes,
    )
    .expect("open store");

    let b = ContinuationBinding::new("ws1", "tool1", "arg1", 1, "h1");
    let oversized_payload = vec![b'y'; 150 * 1024]; // 150 KiB > 100 KiB

    let res = store.create_raw(&b, &oversized_payload);
    assert!(res.is_err(), "oversized snapshot write must fail");
    let err = res.unwrap_err();
    assert_eq!(err.code(), "PAYLOAD_OVERSIZED");

    assert_eq!(store.entry_count().expect("entry count"), 0);
}

// ----------------------------------------------------------------------------
// Contract Test 5: Lowercase Hex Continuation Token Validation
// ----------------------------------------------------------------------------

#[test]
fn continuation_token_validation_strictly_enforces_lowercase_hex() {
    use crate::workspace_runtime::continuation::is_valid_continuation_token;

    let valid = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(is_valid_continuation_token(valid));

    // Uppercase characters A-F must be rejected
    let uppercase = "0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef";
    assert!(!is_valid_continuation_token(uppercase));

    // Non-hex character
    let non_hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg";
    assert!(!is_valid_continuation_token(non_hex));

    // Wrong length
    let short = "0123456789abcdef";
    assert!(!is_valid_continuation_token(short));
}

// ----------------------------------------------------------------------------
// Contract Test 6: validate_handle_binding allows spillover_get retrieval
// ----------------------------------------------------------------------------

#[test]
fn validate_handle_binding_allows_spillover_get_for_originating_query_tools() {
    use crate::workspace_runtime::continuation::{ContinuationBinding, validate_handle_binding};

    let stored = ContinuationBinding::new("ws_alpha", "fast_search", "hash_query_123", 1, "rev:1");

    // spillover_get consumer matches stored snapshot across distinct tool name and empty args
    let requested_spillover = ContinuationBinding::new("ws_alpha", "spillover_get", "", 1, "rev:1");
    assert!(validate_handle_binding(&stored, &requested_spillover).is_ok());

    // Cross-workspace read via spillover_get must fail
    let cross_ws = ContinuationBinding::new("ws_beta", "spillover_get", "", 1, "rev:1");
    let cross_err = validate_handle_binding(&stored, &cross_ws).unwrap_err();
    assert_eq!(cross_err.code, "CONTINUATION_INVALID");

    // Non-spillover_get tool with mismatched tool name must fail
    let other_tool =
        ContinuationBinding::new("ws_alpha", "get_context", "hash_query_123", 1, "rev:1");
    let tool_err = validate_handle_binding(&stored, &other_tool).unwrap_err();
    assert_eq!(tool_err.code, "CONTINUATION_INVALID");
}
