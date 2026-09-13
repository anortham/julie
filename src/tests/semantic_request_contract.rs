//! Contract tests for semantic mode parity, SQLite vector compatibility checks,
//! and transport-independent semantic readiness across CLI and MCP.

use anyhow::Result;
use clap::Parser;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::cli::Cli;
use crate::embeddings::{DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity};
use crate::paths::RegistryPaths;
use crate::request_engine::semantic::{
    CURRENT_EMBEDDING_FORMAT_VERSION, DefaultSemanticRuntime, SemanticReadiness,
    SemanticRequirement, SemanticRuntime,
};
use crate::request_engine::types::{RequestFailure, SemanticMode, WorkspaceBinding};
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestOrigin, RuntimeFactory, ToolRequest,
};
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use julie_core::database::FactsStore;

// ---------------------------------------------------------------------------
// Mock Providers
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct MockReadyProvider {
    pub call_count: AtomicUsize,
    pub dimensions: usize,
    pub model_name: String,
    pub device: String,
}

impl MockReadyProvider {
    pub fn new(model_name: &str, dimensions: usize) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            dimensions,
            model_name: model_name.to_string(),
            device: "cpu".to_string(),
        }
    }
}

impl EmbeddingProvider for MockReadyProvider {
    fn embed_query(&self, _text: &str, _budget: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        Ok(vec![0.1_f32; self.dimensions])
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> Result<Vec<Vec<f32>>> {
        self.call_count.fetch_add(texts.len(), Ordering::SeqCst);
        Ok(vec![vec![0.1_f32; self.dimensions]; texts.len()])
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        Ok(EncoderIdentity::mock(&self.model_name, self.dimensions))
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "mock".to_string(),
            device: self.device.clone(),
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
        }
    }
}

// ---------------------------------------------------------------------------
// Test Fixture
// ---------------------------------------------------------------------------

pub struct SemanticFixture {
    pub temp_repo: TempDir,
    pub temp_home: TempDir,
    pub binding: WorkspaceBinding,
    pub runtime: Arc<dyn SemanticRuntime>,
    pub provider: Option<Arc<MockReadyProvider>>,
}

impl SemanticFixture {
    pub async fn provider_ready_without_vectors() -> Self {
        let temp_repo = tempfile::tempdir().expect("temp repo dir");
        let temp_home = tempfile::tempdir().expect("temp home dir");
        let root = make_isolated_workspace_root(temp_repo.path(), "sem_no_vectors");
        let index_root = temp_home.path().join("indexes/sem_no_vectors");
        let db_dir = index_root.join("db");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let db_path = db_dir.join("symbols.db");

        // Initialize SQLite DB with symbols but 0 vectors
        let mut db = FactsStore::new(&db_path).expect("initialize db");
        let file = crate::tests::helpers::db::file_info_builder("src/lib.rs")
            .language("rust")
            .hash("deadbeef")
            .size(100)
            .last_modified(0)
            .last_indexed(0)
            .build();
        crate::tests::helpers::db::store_file_info_if_missing(&mut db, &file)
            .expect("store file info");
        let sym =
            crate::tests::helpers::db::symbol_builder("sym_1", "probe_fn", "src/lib.rs").build();
        db.store_symbols(&[sym]).expect("store symbols");
        assert_eq!(db.embedding_count().unwrap(), 0);

        let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5", 384));
        let expected_key = provider
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .expect("storage key");
        db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
            .expect("align config");

        let runtime: Arc<dyn SemanticRuntime> = Arc::new(DefaultSemanticRuntime::new(Some(
            provider.clone() as Arc<dyn EmbeddingProvider>,
        )));

        let binding = WorkspaceBinding {
            workspace_id: "sem_no_vectors".to_string(),
            root,
            index_root,
        };

        Self {
            temp_repo,
            temp_home,
            binding,
            runtime,
            provider: Some(provider),
        }
    }

    pub async fn ready_with_vectors() -> Self {
        let fixture = Self::provider_ready_without_vectors().await;
        let db_path = fixture.binding.index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).expect("open db");
        let expected_key = fixture
            .provider
            .as_ref()
            .unwrap()
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .expect("storage key");
        let rev = db
            .get_latest_facts_revision_number()
            .expect("canonical rev")
            .unwrap_or(0);
        let gen_id = db
            .begin_embedding_generation(&expected_key, rev, 384)
            .expect("begin gen");
        db.store_embeddings_for_generation(gen_id, &[("sym_1".to_string(), vec![0.1_f32; 384])])
            .expect("store embedding");
        db.publish_embedding_generation(gen_id, rev, 1, 1)
            .expect("publish gen");
        assert_eq!(db.embedding_count().unwrap(), 1);
        fixture
    }

    pub async fn ready_with_mismatched_dimensions() -> Self {
        let fixture = Self::provider_ready_without_vectors().await;
        let db_path = fixture.binding.index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).expect("open db");
        let expected_key = fixture
            .provider
            .as_ref()
            .unwrap()
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .expect("storage key");
        db.recreate_vectors_table(512).expect("recreate 512d");
        db.set_embedding_config(&expected_key, 512, CURRENT_EMBEDDING_FORMAT_VERSION)
            .expect("set config 512d");
        fixture
    }

    pub async fn ready_with_mismatched_model() -> Self {
        let fixture = Self::provider_ready_without_vectors().await;
        let db_path = fixture.binding.index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).expect("open db");
        db.set_embedding_config("other-model", 384, CURRENT_EMBEDDING_FORMAT_VERSION)
            .expect("set config other model");
        fixture
    }

    pub async fn ready_with_stale_format() -> Self {
        let fixture = Self::ready_with_vectors().await;
        let db_path = fixture.binding.index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).expect("open db");
        let expected_key = fixture
            .provider
            .as_ref()
            .unwrap()
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .expect("storage key");
        db.set_embedding_config(&expected_key, 384, 1)
            .expect("set config stale format");
        fixture
    }

    pub async fn unconfigured_provider() -> Self {
        let temp_repo = tempfile::tempdir().expect("temp repo dir");
        let temp_home = tempfile::tempdir().expect("temp home dir");
        let root = make_isolated_workspace_root(temp_repo.path(), "sem_unconfigured");
        let index_root = temp_home.path().join("indexes/sem_unconfigured");

        let runtime: Arc<dyn SemanticRuntime> = Arc::new(DefaultSemanticRuntime::new(None));
        let binding = WorkspaceBinding {
            workspace_id: "sem_unconfigured".to_string(),
            root,
            index_root,
        };

        Self {
            temp_repo,
            temp_home,
            binding,
            runtime,
            provider: None,
        }
    }

    pub async fn ready_provider_missing_database() -> Self {
        let temp_repo = tempfile::tempdir().expect("temp repo dir");
        let temp_home = tempfile::tempdir().expect("temp home dir");
        let root = make_isolated_workspace_root(temp_repo.path(), "sem_missing_db");
        let index_root = temp_home.path().join("indexes/sem_missing_db");

        let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5", 384));
        let runtime: Arc<dyn SemanticRuntime> = Arc::new(DefaultSemanticRuntime::new(Some(
            provider.clone() as Arc<dyn EmbeddingProvider>,
        )));

        let binding = WorkspaceBinding {
            workspace_id: "sem_missing_db".to_string(),
            root,
            index_root,
        };

        Self {
            temp_repo,
            temp_home,
            binding,
            runtime,
            provider: Some(provider),
        }
    }

    pub async fn ready_provider_missing_tables() -> Self {
        let temp_repo = tempfile::tempdir().expect("temp repo dir");
        let temp_home = tempfile::tempdir().expect("temp home dir");
        let root = make_isolated_workspace_root(temp_repo.path(), "sem_missing_tables");
        let index_root = temp_home.path().join("indexes/sem_missing_tables");
        let db_dir = index_root.join("db");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let db_path = db_dir.join("symbols.db");

        let conn = rusqlite::Connection::open(&db_path).expect("create raw db");
        conn.execute_batch("CREATE TABLE symbols (id TEXT PRIMARY KEY);")
            .expect("create symbols");

        let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5", 384));
        let runtime: Arc<dyn SemanticRuntime> = Arc::new(DefaultSemanticRuntime::new(Some(
            provider.clone() as Arc<dyn EmbeddingProvider>,
        )));

        let binding = WorkspaceBinding {
            workspace_id: "sem_missing_tables".to_string(),
            root,
            index_root,
        };

        Self {
            temp_repo,
            temp_home,
            binding,
            runtime,
            provider: Some(provider),
        }
    }

    pub async fn ready_provider_missing_config_row() -> Self {
        let temp_repo = tempfile::tempdir().expect("temp repo dir");
        let temp_home = tempfile::tempdir().expect("temp home dir");
        let root = make_isolated_workspace_root(temp_repo.path(), "sem_missing_config_row");
        let index_root = temp_home.path().join("indexes/sem_missing_config_row");
        let db_dir = index_root.join("db");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let db_path = db_dir.join("symbols.db");

        let conn = rusqlite::Connection::open(&db_path).expect("create raw db");
        conn.execute_batch(
            "CREATE TABLE symbols (id TEXT PRIMARY KEY);
             CREATE TABLE symbol_vectors (symbol_id TEXT, vector BLOB);
             CREATE TABLE embedding_config (id INTEGER PRIMARY KEY, model_name TEXT, dimensions INTEGER, format_version INTEGER);"
        ).expect("create tables");

        let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5", 384));
        let runtime: Arc<dyn SemanticRuntime> = Arc::new(DefaultSemanticRuntime::new(Some(
            provider.clone() as Arc<dyn EmbeddingProvider>,
        )));

        let binding = WorkspaceBinding {
            workspace_id: "sem_missing_config_row".to_string(),
            root,
            index_root,
        };

        Self {
            temp_repo,
            temp_home,
            binding,
            runtime,
            provider: Some(provider),
        }
    }

    pub async fn ensure(
        &self,
        mode: SemanticMode,
        requirement: SemanticRequirement,
    ) -> Result<SemanticReadiness, RequestFailure> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        let cancellation = tokio_util::sync::CancellationToken::new();
        self.ensure_with_deadline_and_cancel(mode, requirement, deadline, &cancellation)
            .await
    }

    pub async fn ensure_with_deadline_and_cancel(
        &self,
        mode: SemanticMode,
        requirement: SemanticRequirement,
        deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Result<SemanticReadiness, RequestFailure> {
        self.runtime
            .ensure_ready(&self.binding, requirement, mode, deadline, cancellation)
            .await
    }

    pub fn provider_call_count(&self) -> usize {
        self.provider
            .as_ref()
            .map(|p| p.call_count.load(Ordering::SeqCst))
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Step 1 RED Test (Plan 3 Task 3 Step 1 Exact Contract)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn required_semantics_refuses_ready_provider_with_missing_vectors() {
    let fixture = SemanticFixture::provider_ready_without_vectors().await;
    let error = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
    assert_eq!(error.details["coverage"], "missing");
    let off = fixture
        .ensure(SemanticMode::Off, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(off, SemanticReadiness::Disabled));
}

// ---------------------------------------------------------------------------
// Additional Contract Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn auto_semantics_degrades_when_vectors_are_missing() {
    let fixture = SemanticFixture::provider_ready_without_vectors().await;
    let readiness = fixture
        .ensure(SemanticMode::Auto, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        readiness,
        SemanticReadiness::Degraded { ref reason, retryable } if reason == "VECTORS_MISSING" && retryable
    ));
}

#[tokio::test]
async fn off_semantics_performs_zero_provider_work() {
    let fixture = SemanticFixture::unconfigured_provider().await;
    let readiness = fixture
        .ensure(SemanticMode::Off, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(readiness, SemanticReadiness::Disabled));
    assert_eq!(fixture.provider_call_count(), 0);
}

#[tokio::test]
async fn required_semantics_succeeds_when_vectors_present_and_provider_ready() {
    let fixture = SemanticFixture::ready_with_vectors().await;
    let readiness = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    match readiness {
        SemanticReadiness::Ready {
            model_id,
            dimensions,
            device,
            ..
        } => {
            assert_eq!(model_id, "bge-small-en-v1.5");
            assert_eq!(dimensions, 384);
            assert_eq!(device, "cpu");
        }
        other => panic!("Expected SemanticReadiness::Ready, got {other:?}"),
    }
}

#[tokio::test]
async fn requirement_none_succeeds_without_checking_vectors() {
    let fixture = SemanticFixture::provider_ready_without_vectors().await;
    let readiness = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::None)
        .await
        .unwrap();
    assert!(matches!(readiness, SemanticReadiness::Disabled));
}

#[tokio::test]
async fn query_only_requirement_succeeds_with_missing_symbol_vectors() {
    let fixture = SemanticFixture::provider_ready_without_vectors().await;
    let readiness = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::Query)
        .await
        .unwrap();
    assert!(matches!(
        readiness,
        SemanticReadiness::Ready { ref model_id, dimensions: 384, .. } if model_id == "bge-small-en-v1.5"
    ));
}

#[tokio::test]
async fn required_semantics_refuses_incompatible_dimensions() {
    let fixture = SemanticFixture::ready_with_mismatched_dimensions().await;
    let error = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
    assert_eq!(error.details["coverage"], "incompatible");
    assert_eq!(error.details["stored_dimensions"], 512);
    assert_eq!(error.details["provider_dimensions"], 384);
}

#[tokio::test]
async fn required_semantics_refuses_incompatible_model() {
    let fixture = SemanticFixture::ready_with_mismatched_model().await;
    let error = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
    assert_eq!(error.details["coverage"], "incompatible");
    assert_eq!(error.details["stored_model"], "other-model");
}

#[tokio::test]
async fn required_semantics_refuses_stale_format_version() {
    let fixture = SemanticFixture::ready_with_stale_format().await;
    let error = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
    assert_eq!(error.details["coverage"], "stale");
}

#[tokio::test]
async fn required_semantics_refuses_missing_database() {
    let fixture = SemanticFixture::unconfigured_provider().await;
    let error = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
}

struct MockProviderMissingIdentity {
    dimensions: usize,
    model_name: String,
}

impl EmbeddingProvider for MockProviderMissingIdentity {
    fn embed_query(&self, _t: &str, _b: &EmbeddingRequestBudget) -> Result<Vec<f32>> {
        Ok(vec![0.1_f32; self.dimensions])
    }
    fn embed_batch(&self, t: &[String], _b: &EmbeddingRequestBudget) -> Result<Vec<Vec<f32>>> {
        Ok(vec![vec![0.1_f32; self.dimensions]; t.len()])
    }
    fn dimensions(&self) -> usize {
        self.dimensions
    }
    fn encoder_identity(&self) -> Result<EncoderIdentity> {
        anyhow::bail!("IdentityUnavailable: missing pooling")
    }
    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            runtime: "mock".to_string(),
            device: "cpu".to_string(),
            model_name: self.model_name.clone(),
            dimensions: self.dimensions,
        }
    }
}

#[tokio::test]
async fn challenge_required_semantics_refuses_missing_identity_without_fallback() {
    let temp_repo = tempfile::tempdir().expect("temp repo dir");
    let temp_home = tempfile::tempdir().expect("temp home dir");
    let root = make_isolated_workspace_root(temp_repo.path(), "sem_no_fallback");
    let index_root = temp_home.path().join("indexes/sem_no_fallback");
    let db_dir = index_root.join("db");
    std::fs::create_dir_all(&db_dir).expect("create db dir");
    let db_path = db_dir.join("symbols.db");

    let mut db = FactsStore::new(&db_path).expect("initialize db");
    let file = crate::tests::helpers::db::file_info_builder("src/lib.rs")
        .language("rust")
        .hash("deadbeef")
        .size(100)
        .last_modified(0)
        .last_indexed(0)
        .build();
    crate::tests::helpers::db::store_file_info_if_missing(&mut db, &file).expect("store file info");
    let sym = crate::tests::helpers::db::symbol_builder("sym_1", "probe_fn", "src/lib.rs").build();
    db.store_symbols(&[sym]).expect("store symbols");

    // In the old code, if encoder_identity failed, check_sqlite_vectors fell back to dev_info.model_name!
    // We populate DB with a published generation using encoder_key = "bge-small-en-v1.5" (matching model_name).
    let model_name = "bge-small-en-v1.5";
    let rev = db
        .get_latest_facts_revision_number()
        .expect("rev")
        .unwrap_or(0);
    let gen_id = db
        .begin_embedding_generation(model_name, rev, 384)
        .expect("begin gen");
    db.store_embeddings_for_generation(gen_id, &[("sym_1".to_string(), vec![0.1_f32; 384])])
        .expect("store");
    db.publish_embedding_generation(gen_id, rev, 1, 1)
        .expect("publish");
    db.set_embedding_config(model_name, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
        .expect("config");

    let provider = Arc::new(MockProviderMissingIdentity {
        dimensions: 384,
        model_name: model_name.to_string(),
    });
    let runtime: Arc<dyn SemanticRuntime> = Arc::new(DefaultSemanticRuntime::new(Some(
        provider.clone() as Arc<dyn EmbeddingProvider>,
    )));
    let binding = WorkspaceBinding {
        workspace_id: "sem_no_fallback".to_string(),
        root,
        index_root,
    };

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let cancel = tokio_util::sync::CancellationToken::new();

    // 1. SemanticMode::Required: MUST return error with missing_identity and NEVER fall back to model_name
    let err = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "SEMANTICS_NOT_READY");
    assert_eq!(err.details["coverage"], "missing_identity");
    assert!(
        err.message.contains("Encoder identity unavailable"),
        "got: {}",
        err.message
    );

    // 2. SemanticMode::Auto: returns Degraded with ENCODER_IDENTITY_UNAVAILABLE
    let auto_res = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Auto,
            deadline,
            &cancel,
        )
        .await
        .unwrap();
    assert!(matches!(
        auto_res,
        SemanticReadiness::Degraded { ref reason, retryable: true } if reason == "ENCODER_IDENTITY_UNAVAILABLE"
    ));

    // 3. SemanticMode::Off: returns Disabled
    let off_res = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Off,
            deadline,
            &cancel,
        )
        .await
        .unwrap();
    assert!(matches!(off_res, SemanticReadiness::Disabled));
}

// ---------------------------------------------------------------------------
// CLI Flag Mapping Contract Tests
// ---------------------------------------------------------------------------

#[test]
fn cli_flags_map_semantics_correctly() {
    let cli_off =
        Cli::try_parse_from(["julie-server", "--semantics", "off", "search", "probe"]).unwrap();
    assert_eq!(cli_off.tool_flags.semantics, Some(SemanticMode::Off));

    let cli_auto =
        Cli::try_parse_from(["julie-server", "--semantics", "auto", "search", "probe"]).unwrap();
    assert_eq!(cli_auto.tool_flags.semantics, Some(SemanticMode::Auto));

    let cli_required =
        Cli::try_parse_from(["julie-server", "--semantics", "required", "search", "probe"])
            .unwrap();
    assert_eq!(
        cli_required.tool_flags.semantics,
        Some(SemanticMode::Required)
    );

    let cli_default = Cli::try_parse_from(["julie-server", "search", "probe"]).unwrap();
    assert_eq!(cli_default.tool_flags.semantics, None);
}

#[test]
fn cli_flags_reject_invalid_semantics() {
    let err =
        match Cli::try_parse_from(["julie-server", "--semantics", "invalid", "search", "probe"]) {
            Ok(_) => panic!("Expected invalid semantics parse error"),
            Err(e) => e,
        };
    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
}

// ---------------------------------------------------------------------------
// Adversarial Stress Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn adversarial_missing_database_with_ready_provider() {
    let fixture = SemanticFixture::ready_provider_missing_database().await;

    // Required mode must fail with SEMANTICS_NOT_READY and coverage = "missing"
    let err = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(err.code, "SEMANTICS_NOT_READY");
    assert_eq!(err.details["coverage"], "missing");

    // Auto mode must degrade to DATABASE_MISSING without failing the request
    let auto = fixture
        .ensure(SemanticMode::Auto, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        auto,
        SemanticReadiness::Degraded { ref reason, retryable: true } if reason == "DATABASE_MISSING"
    ));

    // Off mode must return Disabled
    let off = fixture
        .ensure(SemanticMode::Off, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert_eq!(off, SemanticReadiness::Disabled);

    // Requirement None must return Disabled without checking database
    let none = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::None)
        .await
        .unwrap();
    assert_eq!(none, SemanticReadiness::Disabled);
}

#[tokio::test]
async fn adversarial_missing_tables_in_sqlite() {
    let fixture = SemanticFixture::ready_provider_missing_tables().await;

    // Required mode must fail with SEMANTICS_NOT_READY and coverage = "missing"
    let err = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(err.code, "SEMANTICS_NOT_READY");
    assert_eq!(err.details["coverage"], "missing");

    // Auto mode must degrade to VECTORS_MISSING without failing
    let auto = fixture
        .ensure(SemanticMode::Auto, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        auto,
        SemanticReadiness::Degraded { ref reason, retryable: true } if reason == "VECTORS_MISSING"
    ));

    // Off mode returns Disabled
    let off = fixture
        .ensure(SemanticMode::Off, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert_eq!(off, SemanticReadiness::Disabled);
}

#[tokio::test]
async fn adversarial_missing_config_row_in_sqlite() {
    let fixture = SemanticFixture::ready_provider_missing_config_row().await;

    // Required mode must fail with SEMANTICS_NOT_READY and coverage = "missing"
    let err = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap_err();
    assert_eq!(err.code, "SEMANTICS_NOT_READY");
    assert_eq!(err.details["coverage"], "missing");

    // Auto mode must degrade to CONFIG_MISSING without failing
    let auto = fixture
        .ensure(SemanticMode::Auto, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        auto,
        SemanticReadiness::Degraded { ref reason, retryable: true } if reason == "CONFIG_MISSING"
    ));
}

#[tokio::test]
async fn adversarial_vector_state_matrix_auto_degradation_and_required_rejection() {
    // 1. Incompatible dimensions
    let fixture_dims = SemanticFixture::ready_with_mismatched_dimensions().await;
    let auto_dims = fixture_dims
        .ensure(SemanticMode::Auto, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        auto_dims,
        SemanticReadiness::Degraded { ref reason, retryable: true } if reason == "VECTORS_INCOMPATIBLE"
    ));

    // 2. Incompatible model
    let fixture_model = SemanticFixture::ready_with_mismatched_model().await;
    let auto_model = fixture_model
        .ensure(SemanticMode::Auto, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        auto_model,
        SemanticReadiness::Degraded { ref reason, retryable: true } if reason == "VECTORS_INCOMPATIBLE"
    ));

    // 3. Stale format version
    let fixture_stale = SemanticFixture::ready_with_stale_format().await;
    let auto_stale = fixture_stale
        .ensure(SemanticMode::Auto, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        auto_stale,
        SemanticReadiness::Degraded { ref reason, retryable: true } if reason == "VECTORS_STALE"
    ));

    // 4. Ready vectors
    let fixture_ready = SemanticFixture::ready_with_vectors().await;
    let ready_res = fixture_ready
        .ensure(SemanticMode::Required, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert!(matches!(
        ready_res,
        SemanticReadiness::Ready {
            ref model_id,
            dimensions: 384,
            ..
        } if model_id == "bge-small-en-v1.5"
    ));
}

#[tokio::test]
async fn adversarial_off_and_none_perform_strictly_zero_work() {
    let fixture = SemanticFixture::ready_provider_missing_database().await;

    // Mode Off with QueryAndSymbols: zero work, returns Disabled
    let off_res = fixture
        .ensure(SemanticMode::Off, SemanticRequirement::QueryAndSymbols)
        .await
        .unwrap();
    assert_eq!(off_res, SemanticReadiness::Disabled);
    assert_eq!(fixture.provider_call_count(), 0);

    // Requirement None with Required mode: zero work, returns Disabled
    let none_req_res = fixture
        .ensure(SemanticMode::Required, SemanticRequirement::None)
        .await
        .unwrap();
    assert_eq!(none_req_res, SemanticReadiness::Disabled);
    assert_eq!(fixture.provider_call_count(), 0);

    // Requirement None with Auto mode: zero work, returns Disabled
    let none_auto_res = fixture
        .ensure(SemanticMode::Auto, SemanticRequirement::None)
        .await
        .unwrap();
    assert_eq!(none_auto_res, SemanticReadiness::Disabled);
    assert_eq!(fixture.provider_call_count(), 0);
}

#[tokio::test]
async fn adversarial_cooperative_cancellation_and_deadline() {
    let fixture = SemanticFixture::ready_with_vectors().await;

    // Cancellation check
    let cancelled_token = CancellationToken::new();
    cancelled_token.cancel();
    let err = fixture
        .ensure_with_deadline_and_cancel(
            SemanticMode::Required,
            SemanticRequirement::QueryAndSymbols,
            Instant::now() + Duration::from_secs(5),
            &cancelled_token,
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, "CANCELLED");

    // Deadline check
    let not_cancelled = CancellationToken::new();
    let expired_deadline = Instant::now() - Duration::from_millis(10);
    let err2 = fixture
        .ensure_with_deadline_and_cancel(
            SemanticMode::Required,
            SemanticRequirement::QueryAndSymbols,
            expired_deadline,
            &not_cancelled,
        )
        .await
        .unwrap_err();
    assert_eq!(err2.code, "DEADLINE_EXCEEDED");
}

#[tokio::test]
async fn adversarial_request_engine_dispatch_integration() {
    // Create an isolated workspace
    let temp_repo = tempfile::tempdir().expect("temp repo dir");
    let root = make_isolated_workspace_root(temp_repo.path(), "adv_dispatch");
    std::fs::create_dir_all(root.join("src")).expect("create src dir");
    std::fs::write(root.join("src/lib.rs"), "pub fn adv_probe() {}\n").expect("write file");

    let temp_home = tempfile::tempdir().expect("temp home dir");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let binding_resolver = BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths.clone()));

    // Provider is ready, but vectors are missing
    let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5", 384));
    let semantic_runtime = Arc::new(DefaultSemanticRuntime::new(Some(
        provider.clone() as Arc<dyn EmbeddingProvider>
    )));

    let engine = RequestEngine::with_semantic_runtime(
        binding_resolver,
        runtime_factory.clone(),
        semantic_runtime,
    );

    // Warm up the runtime factory so the workspace exists
    let dummy_ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(30)),
        CancellationToken::new(),
    );
    let binding = engine.bindings.resolve(None, None, false).unwrap();
    let _ = runtime_factory
        .acquire(binding.as_ref(), &dummy_ctx)
        .await
        .unwrap();

    {
        let db_path = binding.as_ref().unwrap().index_root.join("db/symbols.db");
        let mut db = FactsStore::new(&db_path).expect("open db");
        let expected_key = provider
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .expect("storage key");
        db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
            .expect("align config");
    }

    // 1. fast_search (requires QueryAndSymbols) with Required mode MUST fail with SEMANTICS_NOT_READY
    let req_required = ToolRequest::new(
        "fast_search",
        json!({ "query": "adv_probe" }).as_object().unwrap().clone(),
    )
    .with_semantics(SemanticMode::Required);
    let ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let err = engine.execute(req_required, ctx).await.unwrap_err();
    assert_eq!(err.code, "SEMANTICS_NOT_READY");
    assert_eq!(err.details["coverage"], "missing");

    // 2. fast_search with Auto mode MUST succeed with degraded readiness
    let req_auto = ToolRequest::new(
        "fast_search",
        json!({ "query": "adv_probe" }).as_object().unwrap().clone(),
    )
    .with_semantics(SemanticMode::Auto);
    let ctx_auto = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let reply_auto = engine.execute(req_auto, ctx_auto).await.unwrap();
    assert_eq!(reply_auto.readiness.mode, SemanticMode::Auto);
    assert_eq!(reply_auto.readiness.status, "degraded: VECTORS_MISSING");
    assert_eq!(reply_auto.readiness.coverage, Some("missing".to_string()));

    // 3. fast_search with Off mode MUST succeed with disabled readiness
    let req_off = ToolRequest::new(
        "fast_search",
        json!({ "query": "adv_probe" }).as_object().unwrap().clone(),
    )
    .with_semantics(SemanticMode::Off);
    let ctx_off = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let reply_off = engine.execute(req_off, ctx_off).await.unwrap();
    assert_eq!(reply_off.readiness.mode, SemanticMode::Off);
    assert_eq!(reply_off.readiness.status, "disabled");
    assert_eq!(reply_off.readiness.coverage, None);

    let workspace_id = binding.as_ref().unwrap().workspace_id.clone();
    let req_stats = ToolRequest::new(
        "manage_workspace",
        json!({ "operation": "health", "workspace_id": workspace_id })
            .as_object()
            .unwrap()
            .clone(),
    )
    .with_semantics(SemanticMode::Required);
    let ctx_stats = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let reply_stats = engine.execute(req_stats, ctx_stats).await.unwrap();
    assert_eq!(reply_stats.readiness.status, "disabled");
    assert_eq!(reply_stats.readiness.coverage, None);
}
