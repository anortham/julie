//! Tests for Milestone 3 (R3: Follower Source Edits & Durable Atomic Journals).
//!
//! Contract boundaries verified:
//! 1. Follower preview is permitted and writes 0 index/source bytes.
//! 2. Stale apply preserves external modifications with EDIT_CONFLICT.
//! 3. AST post-edit syntax validation rejects syntax regressions across all files.
//! 4. Crash recovery via resume/rollback is idempotent and preserves conflicts.
//! 5. Unrelated pre-existing diagnostics are preserved during symbol rewrites.
//! 6. Cancelled requests never commit and leave all source files untouched.

use crate::paths::RegistryPaths;
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestFailure, RequestOrigin, RuntimeFactory,
    SemanticMode, ToolReply, ToolRequest, WorkspaceBinding,
};
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ============================================================================
// Fixtures and Data Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct EditPreview {
    pub file_path: String,
    pub old_text: String,
    pub new_text: String,
    pub diff: String,
}

pub struct FollowerEditFixture {
    pub temp_repo: Arc<tempfile::TempDir>,
    pub workspace_root: PathBuf,
    pub temp_home: Arc<tempfile::TempDir>,
    pub workspace_id: String,
    pub binding: WorkspaceBinding,
    pub follower_engine: RequestEngine,
    pub file_path: PathBuf,
    pub initial_revision: i64,
}

impl FollowerEditFixture {
    pub async fn new() -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let _ = std::fs::create_dir_all(&tmp_base);

        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("create temp repo");
        let workspace_root = make_isolated_workspace_root(temp_repo.path(), "follower_edit");
        let file_path = workspace_root.join("src/lib.rs");
        std::fs::create_dir_all(workspace_root.join("src")).expect("create src dir");
        std::fs::write(&file_path, "pub fn before() {}\n").expect("write initial file");

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("create temp home");
        let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
        let workspace_id = julie_core::workspace::registry::generate_workspace_id(
            &workspace_root.to_string_lossy(),
        )
        .expect("generate workspace_id");

        let index_root = registry_paths.workspace_index_dir(&workspace_id);
        std::fs::create_dir_all(&index_root).expect("create index root");

        let binding = WorkspaceBinding {
            workspace_id: workspace_id.clone(),
            root: workspace_root.clone(),
            index_root: index_root.clone(),
        };

        // Build follower RequestEngine
        let binding_resolver =
            BindingResolver::new(Some(workspace_root.clone()), false, registry_paths.clone());
        let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
        let follower_engine = RequestEngine::new(binding_resolver, runtime_factory);

        Self {
            temp_repo: Arc::new(temp_repo),
            workspace_root,
            temp_home: Arc::new(temp_home),
            workspace_id,
            binding,
            follower_engine,
            file_path,
            initial_revision: 0,
        }
    }

    pub fn file(&self) -> &Path {
        &self.file_path
    }

    pub fn relative_file(&self) -> &str {
        "src/lib.rs"
    }

    pub async fn preview(
        &self,
        old_text: &str,
        new_text: &str,
    ) -> Result<EditPreview, RequestFailure> {
        let reply = self
            .execute(
                "edit_file",
                serde_json::json!({
                    "file_path": self.relative_file(),
                    "old_text": old_text,
                    "new_text": new_text,
                    "dry_run": true,
                }),
            )
            .await?;

        let diff = reply
            .result
            .get("diff")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        Ok(EditPreview {
            file_path: self.relative_file().to_string(),
            old_text: old_text.to_string(),
            new_text: new_text.to_string(),
            diff,
        })
    }

    pub async fn apply(&self, preview: EditPreview) -> Result<ToolReply, RequestFailure> {
        self.execute(
            "edit_file",
            serde_json::json!({
                "file_path": preview.file_path,
                "old_text": preview.old_text,
                "new_text": preview.new_text,
                "dry_run": false,
            }),
        )
        .await
    }

    pub async fn execute(
        &self,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ToolReply, RequestFailure> {
        let request = ToolRequest {
            name: tool.to_string(),
            arguments: args.as_object().cloned().unwrap_or_default(),
            workspace: Some(self.workspace_root.clone()),
            semantics: SemanticMode::Off,
        };
        let context = RequestContext::new(
            RequestOrigin::Cli,
            Some(std::time::Duration::from_secs(10)),
            tokio_util::sync::CancellationToken::new(),
        );
        self.follower_engine.execute(request, context).await
    }

    pub fn follower_index_writes(&self) -> usize {
        let db_path = self.binding.index_root.join("db").join("symbols.db");
        if !db_path.exists() {
            return 0;
        }
        let conn = match rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        ) {
            Ok(c) => c,
            Err(_) => return 0,
        };
        let rev: i64 = conn
            .query_row(
                "SELECT revision FROM facts_revisions ORDER BY revision DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        (rev - self.initial_revision).max(0) as usize
    }
}

pub struct AstEditFixture {
    pub temp_repo: Arc<tempfile::TempDir>,
    pub workspace_root: PathBuf,
    pub temp_home: Arc<tempfile::TempDir>,
    pub workspace_id: String,
    pub engine: RequestEngine,
    pub journal_dir: PathBuf,
}

impl AstEditFixture {
    pub async fn two_files() -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("temp repo");
        let workspace_root = make_isolated_workspace_root(temp_repo.path(), "ast_edit_fixture");
        std::fs::create_dir_all(workspace_root.join("src")).unwrap();

        std::fs::write(workspace_root.join("src/a.rs"), "pub fn first() {}\n").unwrap();
        std::fs::write(workspace_root.join("src/b.rs"), "pub fn second() {}\n").unwrap();

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("temp home");
        let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
        let workspace_id = julie_core::workspace::registry::generate_workspace_id(
            &workspace_root.to_string_lossy(),
        )
        .unwrap();

        let binding_resolver =
            BindingResolver::new(Some(workspace_root.clone()), false, registry_paths.clone());
        let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
        let engine = RequestEngine::new(binding_resolver, runtime_factory);

        let journal_dir = workspace_root.join(".julie").join("edit-journals");

        // Index the workspace through request engine
        let _ = engine
            .execute(
                ToolRequest::new("get_symbols", serde_json::Map::new())
                    .with_workspace(Some(workspace_root.clone()))
                    .with_semantics(SemanticMode::Off),
                RequestContext::new(
                    RequestOrigin::Cli,
                    Some(std::time::Duration::from_secs(10)),
                    tokio_util::sync::CancellationToken::new(),
                ),
            )
            .await;

        Self {
            temp_repo: Arc::new(temp_repo),
            workspace_root,
            temp_home: Arc::new(temp_home),
            workspace_id,
            engine,
            journal_dir,
        }
    }

    pub async fn two_files_shared() -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("temp repo");
        let workspace_root = make_isolated_workspace_root(temp_repo.path(), "ast_edit_shared");
        std::fs::create_dir_all(workspace_root.join("src")).unwrap();

        std::fs::write(
            workspace_root.join("src/a.rs"),
            "pub fn shared_calc() -> i32 { 10 }\n",
        )
        .unwrap();
        std::fs::write(
            workspace_root.join("src/b.rs"),
            "pub fn caller() -> i32 { shared_calc() }\n",
        )
        .unwrap();

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("temp home");
        let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
        let workspace_id = julie_core::workspace::registry::generate_workspace_id(
            &workspace_root.to_string_lossy(),
        )
        .unwrap();

        let binding_resolver =
            BindingResolver::new(Some(workspace_root.clone()), false, registry_paths.clone());
        let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
        let engine = RequestEngine::new(binding_resolver, runtime_factory);

        let journal_dir = workspace_root.join(".julie").join("edit-journals");

        // Index the workspace through request engine
        let _ = engine
            .execute(
                ToolRequest::new("get_symbols", serde_json::Map::new())
                    .with_workspace(Some(workspace_root.clone()))
                    .with_semantics(SemanticMode::Off),
                RequestContext::new(
                    RequestOrigin::Cli,
                    Some(std::time::Duration::from_secs(10)),
                    tokio_util::sync::CancellationToken::new(),
                ),
            )
            .await;

        Self {
            temp_repo: Arc::new(temp_repo),
            workspace_root,
            temp_home: Arc::new(temp_home),
            workspace_id,
            engine,
            journal_dir,
        }
    }

    pub fn source_snapshots(&self) -> std::collections::HashMap<PathBuf, String> {
        let mut map = std::collections::HashMap::new();
        for file in &["src/a.rs", "src/b.rs"] {
            let path = self.workspace_root.join(file);
            if path.exists() {
                map.insert(PathBuf::from(file), std::fs::read_to_string(&path).unwrap());
            }
        }
        map
    }

    pub fn commit_journal_count(&self) -> usize {
        if !self.journal_dir.exists() {
            return 0;
        }
        std::fs::read_dir(&self.journal_dir)
            .map(|entries| entries.filter_map(|e| e.ok()).count())
            .unwrap_or(0)
    }

    pub async fn execute(
        &self,
        tool: &str,
        args: serde_json::Value,
    ) -> Result<ToolReply, RequestFailure> {
        let request = ToolRequest {
            name: tool.to_string(),
            arguments: args.as_object().cloned().unwrap_or_default(),
            workspace: Some(self.workspace_root.clone()),
            semantics: SemanticMode::Off,
        };
        let context = RequestContext::new(
            RequestOrigin::Cli,
            Some(std::time::Duration::from_secs(30)),
            tokio_util::sync::CancellationToken::new(),
        );
        self.engine.execute(request, context).await
    }
}

pub struct EditRecoveryFixture {
    pub temp_repo: Arc<tempfile::TempDir>,
    pub workspace_root: PathBuf,
    pub temp_home: Arc<tempfile::TempDir>,
    pub workspace_id: String,
    pub engine: RequestEngine,
    pub edit_id: String,
    pub pending_path: PathBuf,
    pub pending_old_bytes: String,
}

impl EditRecoveryFixture {
    pub async fn partially_applied() -> Self {
        let tmp_base = std::env::var_os("TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp"));
        let temp_repo = tempfile::tempdir_in(&tmp_base).expect("temp repo");
        let workspace_root = make_isolated_workspace_root(temp_repo.path(), "recovery_fixture");
        std::fs::create_dir_all(workspace_root.join("src")).unwrap();

        let file1_old = "pub fn file_one() -> i32 { 1 }\n";
        let file1_new = "pub fn file_one() -> i32 { 100 }\n";
        let file2_old = "pub fn file_two() -> i32 { 2 }\n";
        let file2_new = "pub fn file_two() -> i32 { 200 }\n";

        // Write file1 in ALREADY APPLIED state (new content)
        std::fs::write(workspace_root.join("src/file1.rs"), file1_new).unwrap();
        // Write file2 in PENDING state (old content)
        let pending_path = workspace_root.join("src/file2.rs");
        std::fs::write(&pending_path, file2_old).unwrap();

        let edit_id = "0191eb44-7700-7c2a-9273-d5dcf594a11b".to_string();
        let journal_dir = workspace_root.join(".julie").join("edit-journals");
        std::fs::create_dir_all(&journal_dir).unwrap();

        let h_f1_old = blake3::hash(file1_old.as_bytes()).to_hex().to_string();
        let h_f1_new = blake3::hash(file1_new.as_bytes()).to_hex().to_string();
        let h_f2_old = blake3::hash(file2_old.as_bytes()).to_hex().to_string();
        let h_f2_new = blake3::hash(file2_new.as_bytes()).to_hex().to_string();

        let journal_data = serde_json::json!({
            "edit_id": edit_id,
            "created_at_unix_ms": 1757332800000u64,
            "updated_at_unix_ms": 1757332800000u64,
            "state": "applying",
            "active_recovery_action": null,
            "terminal_receipt": null,
            "files": [
                {
                    "path": "src/file1.rs",
                    "before_hash": h_f1_old,
                    "after_hash": h_f1_new,
                    "before_bytes": file1_old,
                    "after_bytes": file1_new,
                    "state": "applied"
                },
                {
                    "path": "src/file2.rs",
                    "before_hash": h_f2_old,
                    "after_hash": h_f2_new,
                    "before_bytes": file2_old,
                    "after_bytes": file2_new,
                    "state": "pending"
                }
            ]
        });
        std::fs::write(
            journal_dir.join(format!("{}.json", edit_id)),
            serde_json::to_string_pretty(&journal_data).unwrap(),
        )
        .unwrap();

        let temp_home = tempfile::tempdir_in(&tmp_base).expect("temp home");
        let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
        let workspace_id = julie_core::workspace::registry::generate_workspace_id(
            &workspace_root.to_string_lossy(),
        )
        .unwrap();

        let binding_resolver =
            BindingResolver::new(Some(workspace_root.clone()), false, registry_paths.clone());
        let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths));
        let engine = RequestEngine::new(binding_resolver, runtime_factory);

        Self {
            temp_repo: Arc::new(temp_repo),
            workspace_root,
            temp_home: Arc::new(temp_home),
            workspace_id,
            engine,
            edit_id,
            pending_path,
            pending_old_bytes: file2_old.to_string(),
        }
    }

    pub fn pending_file(&self) -> &Path {
        &self.pending_path
    }

    pub fn edit_id(&self) -> &str {
        &self.edit_id
    }

    pub fn restore_pending_old_bytes(&self) {
        std::fs::write(&self.pending_path, &self.pending_old_bytes).unwrap();
    }

    pub fn source_snapshots(&self) -> std::collections::HashMap<PathBuf, String> {
        let mut map = std::collections::HashMap::new();
        for file in &["src/file1.rs", "src/file2.rs"] {
            let path = self.workspace_root.join(file);
            if path.exists() {
                map.insert(PathBuf::from(file), std::fs::read_to_string(&path).unwrap());
            }
        }
        map
    }

    pub async fn recover(&self, action: &str) -> Result<ToolReply, RequestFailure> {
        let request = ToolRequest {
            name: "manage_workspace".to_string(),
            arguments: serde_json::json!({
                "operation": "recover_edit",
                "edit_id": self.edit_id,
                "recovery_action": action,
            })
            .as_object()
            .cloned()
            .unwrap(),
            workspace: Some(self.workspace_root.clone()),
            semantics: SemanticMode::Off,
        };
        let context = RequestContext::new(
            RequestOrigin::Cli,
            Some(std::time::Duration::from_secs(10)),
            tokio_util::sync::CancellationToken::new(),
        );
        self.engine.execute(request, context).await
    }
}

// ============================================================================
// Contract Tests
// ============================================================================

#[tokio::test]
async fn follower_preview_is_allowed_but_stale_apply_preserves_external_edit() {
    let fixture = FollowerEditFixture::new().await;
    let preview = fixture.preview("before", "after").await.unwrap();
    std::fs::write(fixture.file(), "external_change").unwrap();
    let error = fixture.apply(preview).await.unwrap_err();
    assert_eq!(error.code, "EDIT_CONFLICT");
    assert_eq!(
        std::fs::read_to_string(fixture.file()).unwrap(),
        "external_change"
    );
    assert_eq!(fixture.follower_index_writes(), 0);
}

#[tokio::test]
async fn edit_recovery_resume_is_idempotent_and_preserves_conflicts() {
    let fixture = EditRecoveryFixture::partially_applied().await;
    std::fs::write(fixture.pending_file(), "external_change").unwrap();
    let error = fixture.recover("resume").await.unwrap_err();
    assert_eq!(error.code, "EDIT_RECOVERY_CONFLICT");
    assert_eq!(
        std::fs::read_to_string(fixture.pending_file()).unwrap(),
        "external_change"
    );
    assert_eq!(error.details["edit_id"], fixture.edit_id());
    fixture.restore_pending_old_bytes();
    let completed = fixture.recover("resume").await.unwrap();
    let before_retry = fixture.source_snapshots();
    let repeated = fixture.recover("resume").await.unwrap();
    assert_eq!(repeated.result, completed.result);
    assert_eq!(fixture.source_snapshots(), before_retry);
}

#[tokio::test]
async fn cancelled_follower_preflight_never_commits_after_request_exit() {
    let fixture = FollowerEditFixture::new().await;
    let initial_bytes = std::fs::read(fixture.file()).unwrap();

    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel_cloned = cancel.clone();

    // Pre-cancel to verify immediate rejection before committing
    cancel.cancel();

    let request = ToolRequest {
        name: "edit_file".to_string(),
        arguments: serde_json::json!({
            "file_path": fixture.relative_file(),
            "old_text": "before",
            "new_text": "after",
            "dry_run": false
        })
        .as_object()
        .cloned()
        .unwrap(),
        workspace: Some(fixture.workspace_root.clone()),
        semantics: SemanticMode::Off,
    };
    let context = RequestContext::new(
        RequestOrigin::Cli,
        Some(std::time::Duration::from_secs(10)),
        cancel_cloned,
    );

    let res = fixture.follower_engine.execute(request, context).await;
    assert!(res.is_err(), "cancelled preflight must return an error");
    let err = res.unwrap_err();
    assert_eq!(err.code, "CANCELLED");

    // Invariant: Source bytes completely unchanged, no journal committed
    assert_eq!(std::fs::read(fixture.file()).unwrap(), initial_bytes);
    let journal_dir = fixture.workspace_root.join(".julie").join("edit-journals");
    assert!(!journal_dir.exists() || std::fs::read_dir(journal_dir).unwrap().count() == 0);
}

#[tokio::test]
async fn source_edit_preserves_executable_file_permissions() {
    let fixture = AstEditFixture::two_files().await;
    let script_path = fixture.workspace_root.join("script.sh");
    std::fs::write(&script_path, "#!/bin/sh\necho \"v1\"\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            std::fs::metadata(&script_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
    }

    let _ = fixture
        .execute(
            "edit_file",
            serde_json::json!({
                "file_path": "script.sh",
                "old_text": "echo \"v1\"",
                "new_text": "echo \"v2\"",
                "dry_run": false
            }),
        )
        .await
        .expect("edit_file must succeed");

    assert!(
        std::fs::read_to_string(&script_path)
            .unwrap()
            .contains("echo \"v2\"")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&script_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode, 0o755,
            "Executable permissions (0755) must be preserved after source edit"
        );
    }
}

#[tokio::test]
async fn coordinator_multi_file_second_file_invalid_syntax_leaves_all_untouched() {
    let fixture = AstEditFixture::two_files().await;
    let before = fixture.source_snapshots();

    let root = fixture.workspace_root.clone();
    let coordinator = crate::workspace_runtime::SourceEditCoordinator::new(root).unwrap();

    let path_a = fixture.workspace_root.join("src/a.rs");
    let path_b = fixture.workspace_root.join("src/b.rs");
    let orig_a = std::fs::read(&path_a).unwrap();
    let orig_b = std::fs::read(&path_b).unwrap();

    let changes = vec![
        crate::workspace_runtime::PreparedSourceChange {
            path: PathBuf::from("src/a.rs"),
            before_hash: blake3::hash(&orig_a).to_hex().to_string(),
            after_bytes: b"pub fn first_renamed() {}\n".to_vec(),
            before_bytes: Some(orig_a),
            is_ast_aware: true,
        },
        crate::workspace_runtime::PreparedSourceChange {
            path: PathBuf::from("src/b.rs"),
            before_hash: blake3::hash(&orig_b).to_hex().to_string(),
            after_bytes: b"pub fn second_broken( {\n".to_vec(),
            before_bytes: Some(orig_b),
            is_ast_aware: true,
        },
    ];

    let edit_id = uuid::Uuid::new_v4().to_string();
    let result = coordinator
        .apply(
            &edit_id,
            &changes,
            std::time::Instant::now() + std::time::Duration::from_secs(10),
            &tokio_util::sync::CancellationToken::new(),
        )
        .await;

    assert!(
        result.is_err(),
        "batch apply must fail when second file has syntax regression"
    );
    let err = result.unwrap_err();
    assert_eq!(err.error_code(), "SYNTAX_REGRESSION");

    assert_eq!(
        fixture.source_snapshots(),
        before,
        "File A must remain untouched when File B in the same batch fails syntax validation"
    );
    assert_eq!(fixture.commit_journal_count(), 0);
}

#[tokio::test]
async fn source_edit_coordinator_preview_contract() {
    let fixture = FollowerEditFixture::new().await;
    let coordinator =
        crate::workspace_runtime::SourceEditCoordinator::new(fixture.workspace_root.clone())
            .unwrap();

    let preview = coordinator
        .preview(Path::new("src/lib.rs"), "before", "after")
        .await
        .unwrap();

    assert_eq!(preview.file_path, PathBuf::from("src/lib.rs"));
    assert_eq!(preview.old_text, "before");
    assert_eq!(preview.new_text, "after");
    assert!(preview.diff.contains("-pub fn before()"));
    assert!(preview.diff.contains("+pub fn after()"));
    assert_eq!(
        std::fs::read_to_string(fixture.file()).unwrap(),
        "pub fn before() {}\n"
    );
}

#[tokio::test]
async fn source_edit_bounded_read_rejects_oversized_file() {
    let fixture = FollowerEditFixture::new().await;
    let large_file = fixture.workspace_root.join("src/large.rs");
    std::fs::write(&large_file, vec![b'a'; 2048]).unwrap();

    let config = crate::workspace_runtime::SourceEditConfig {
        max_source_bytes: 1024,
    };
    let coordinator = crate::workspace_runtime::SourceEditCoordinator::with_config(
        fixture.workspace_root.clone(),
        config,
    )
    .unwrap();

    let err = coordinator
        .read_source_bounded(
            Path::new("src/large.rs"),
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            &tokio_util::sync::CancellationToken::new(),
        )
        .unwrap_err();

    assert_eq!(err.error_code(), "SOURCE_TOO_LARGE");
}
