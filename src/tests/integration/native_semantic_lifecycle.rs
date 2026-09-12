//! Integration tests for Milestone N4: Native Semantic Runtime Lifecycle and Dynamic Recovery.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::handler::embedding_init::take_nl_definition_embedding_init_attempts;
use crate::paths::RegistryPaths;
use crate::request_engine::semantic::{
    DefaultSemanticRuntime, RuntimeProviderState, SemanticMode, SemanticReadiness,
    SemanticRequirement, SemanticRuntime, semantic_mode_needs_provider,
};
use crate::request_engine::types::WorkspaceBinding;
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestOrigin, RuntimeFactory, ToolRequest,
};
use crate::tests::helpers::env::EnvVarGuard;
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use julie_core::embeddings_contract::{
    DeviceInfo, EmbeddingProvider, EmbeddingRequestBudget, EncoderIdentity,
};
use julie_facts::{FactsStore, Opened};

fn open_facts(path: &std::path::Path) -> FactsStore {
    match FactsStore::open(path).expect("open facts") {
        Opened::Ready(store) => store,
        Opened::VersionMismatch { .. } => panic!("facts version mismatch"),
    }
}

fn setup_facts_db(db_path: &std::path::Path) -> rusqlite::Connection {
    let _store = open_facts(db_path);
    let conn = rusqlite::Connection::open(db_path).expect("open raw db");
    conn.execute(
        "INSERT INTO blobs (hash, language, extractor_version, byte_len) VALUES ('feedbeef', 'rust', '1.0', 100)",
        [],
    ).expect("insert blob");
    conn.execute(
        "INSERT INTO paths (path, blob_hash, language) VALUES ('src/lib.rs', 'feedbeef', 'rust')",
        [],
    )
    .expect("insert path");
    conn.execute(
        "INSERT INTO symbols (blob_hash, ordinal, name, kind, start_line, start_col, end_line, end_col, start_byte, end_byte, annotations)
         VALUES ('feedbeef', 0, 'probe', 'function', 1, 0, 1, 10, 0, 10, '[]')",
        [],
    ).expect("insert symbol");
    conn
}

fn set_facts_encoder(conn: &rusqlite::Connection, id: &str, dims: u32) {
    conn.execute("DELETE FROM encoder", [])
        .expect("delete encoder");
    conn.execute(
        "INSERT INTO encoder (id, model_checksum, dimensions, pooling, normalization, instruction_policy)
         VALUES (?1, '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef', ?2, 'cls', 'l2', 'v1')",
        rusqlite::params![id, dims],
    ).expect("insert encoder");
}

fn insert_facts_vector(conn: &rusqlite::Connection, encoder_id: &str, dims: usize) {
    let vec_bytes: Vec<u8> = vec![0.1_f32; dims]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    conn.execute(
        "INSERT OR REPLACE INTO vectors (blob_hash, symbol_ordinal, encoder_id, vector) VALUES ('feedbeef', 0, ?1, ?2)",
        rusqlite::params![encoder_id, vec_bytes],
    ).expect("insert vector");
}

// ============================================================================
// 1. Pure Unit Anchor
// ============================================================================

#[test]
fn semantic_off_requires_no_provider() {
    assert!(!semantic_mode_needs_provider(SemanticMode::Off));
    assert!(semantic_mode_needs_provider(SemanticMode::Required));
    assert!(semantic_mode_needs_provider(SemanticMode::Auto));
}

// ============================================================================
// 2. Integration Anchor Helpers
// ============================================================================

#[derive(Debug, Default)]
struct MockReadyProvider {
    pub call_count: std::sync::atomic::AtomicUsize,
    pub dimensions: usize,
    pub model_name: String,
    pub device: String,
}

impl MockReadyProvider {
    pub fn new(model_name: &str, dimensions: usize) -> Self {
        Self {
            call_count: std::sync::atomic::AtomicUsize::new(0),
            dimensions,
            model_name: model_name.to_string(),
            device: "cpu".to_string(),
        }
    }
}

impl EmbeddingProvider for MockReadyProvider {
    fn embed_query(
        &self,
        _text: &str,
        _budget: &EmbeddingRequestBudget,
    ) -> anyhow::Result<Vec<f32>> {
        self.call_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(vec![0.1_f32; self.dimensions])
    }

    fn embed_batch(
        &self,
        texts: &[String],
        _budget: &EmbeddingRequestBudget,
    ) -> anyhow::Result<Vec<Vec<f32>>> {
        self.call_count
            .fetch_add(texts.len(), std::sync::atomic::Ordering::SeqCst);
        Ok(vec![vec![0.1_f32; self.dimensions]; texts.len()])
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn encoder_identity(&self) -> anyhow::Result<EncoderIdentity> {
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

#[cfg(unix)]
fn compile_mock_sidecar(dir: &std::path::Path) -> std::path::PathBuf {
    let src_path = dir.join("mock_sidecar.rs");
    let bin_path = dir.join("mock_sidecar_bin");
    if bin_path.exists() {
        return bin_path;
    }
    let src = r#"
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

fn main() {
    if env::args().any(|arg| arg == "prepare") {
        let cache_dir = env::var("JULIE_EMBEDDING_CACHE_DIR").unwrap();
        if env::var("REQUIRE_BARRIER").is_ok() && !Path::new(&cache_dir).join("barrier").exists() {
            eprintln!("model_not_prepared");
            std::process::exit(7);
        }
        std::fs::create_dir_all(&cache_dir).unwrap();
        let count_path = Path::new(&cache_dir).join("prepare-count");
        let count = std::fs::read_to_string(&count_path).ok().and_then(|count| count.parse::<usize>().ok()).unwrap_or(0);
        std::fs::write(count_path, (count + 1).to_string()).unwrap();
        if let Ok(error) = env::var("PREPARE_FAIL") {
            eprintln!("{error}");
            std::process::exit(7);
        }
        if let Ok(delay) = env::var("PREPARE_DELAY_MS") {
            std::thread::sleep(std::time::Duration::from_millis(delay.parse().unwrap()));
        }
        std::fs::write(Path::new(&cache_dir).join("prepared"), "ready").unwrap();
        return;
    }
    if env::var("REQUIRE_BARRIER").is_ok() {
        if let Ok(cache_dir) = env::var("JULIE_EMBEDDING_CACHE_DIR") {
            let barrier = Path::new(&cache_dir).join("barrier");
            if !barrier.exists() {
                std::process::exit(1);
            }
        }
    }
    let prepared = env::var("JULIE_EMBEDDING_CACHE_DIR").ok().is_none_or(|cache_dir| Path::new(&cache_dir).join("prepared").exists());
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout();
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        if line.trim().is_empty() {
            line.clear();
            continue;
        }
        let req_id = if let Some(pos) = line.find("\"request_id\":") {
            let rest = &line[pos + 13..];
            if let Some(start) = rest.find('"') {
                if let Some(end) = rest[start + 1..].find('"') {
                    &rest[start + 1..start + 1 + end]
                } else { "req-1" }
            } else { "req-1" }
        } else { "req-1" };

        let reply = if line.contains("\"health\"") && !prepared {
            format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":{{\"ready\":false,\"degraded_reason\":\"model_not_prepared\"}},\"error\":null}}\n")
        } else if line.contains("\"health\"") {
            format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":{{\"ready\":true,\"dims\":384,\"device\":\"cpu\",\"runtime\":\"llama.cpp\",\"model_id\":\"bge-small-en-v1.5-f32\",\"model_sha256\":\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\",\"pooling\":\"cls\",\"normalization\":\"l2\",\"instruction_policy_version\":1,\"llama_cpp_build\":\"b3560\"}},\"error\":null}}\n")
        } else if line.contains("\"embed_query\"") {
            let mut s = format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":{{\"dims\":384,\"vector\":[");
            for i in 0..384 {
                if i > 0 { s.push(','); }
                s.push_str("0.1");
            }
            s.push_str("]},\"error\":null}\n");
            s
        } else if line.contains("\"embed_batch\"") {
            let mut s = format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":{{\"dims\":384,\"vectors\":[[");
            for i in 0..384 {
                if i > 0 { s.push(','); }
                s.push_str("0.1");
            }
            s.push_str("]]}},\"error\":null}\n");
            s
        } else if line.contains("\"shutdown\"") {
            let reply = format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":{{\"status\":\"ok\"}},\"error\":null}}\n");
            let _ = stdout.write_all(reply.as_bytes());
            let _ = stdout.flush();
            std::process::exit(0);
        } else {
            format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":null,\"error\":null}}\n")
        };
        let _ = stdout.write_all(reply.as_bytes());
        let _ = stdout.flush();
        line.clear();
    }
}

"#;
    std::fs::write(&src_path, src).unwrap();
    let status = std::process::Command::new("rustc")
        .args([src_path.to_str().unwrap(), "-o", bin_path.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "failed to compile mock sidecar");
    bin_path
}

fn semantic_binding(root: &std::path::Path) -> WorkspaceBinding {
    WorkspaceBinding {
        workspace_id: "semantic-cold-test".to_string(),
        root: root.to_path_buf(),
        index_root: root.join("index"),
    }
}

async fn semantic_readiness(
    runtime: &DefaultSemanticRuntime,
    root: &std::path::Path,
    mode: SemanticMode,
    deadline: Duration,
) -> Result<SemanticReadiness, crate::request_engine::types::RequestFailure> {
    runtime
        .ensure_ready(
            &semantic_binding(root),
            None,
            &julie_index::search::language_config::LanguageConfigs::load_embedded(),
            SemanticRequirement::Query,
            mode,
            tokio::time::Instant::now() + deadline,
            &CancellationToken::new(),
        )
        .await
}

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn cold_model_prepare_does_not_block_lexical_search() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let mut env = EnvVarGuard::new();
    env.set("JULIE_EMBEDDING_PROVIDER", "native");
    env.set(
        "JULIE_NATIVE_SIDECAR_PROGRAM",
        compile_mock_sidecar(temp.path()),
    );
    env.set("JULIE_EMBEDDING_CACHE_DIR", &cache);
    env.set("PREPARE_DELAY_MS", "500");
    let root = make_isolated_workspace_root(temp.path(), "cold_lexical_workspace");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn cold_lexical_probe() {}\n").unwrap();
    let registry_paths = RegistryPaths::with_home(temp.path().join("home"));
    let runtime = Arc::new(DefaultSemanticRuntime::from_registry_paths(
        registry_paths.clone(),
    ));
    let engine = RequestEngine::with_semantic_runtime(
        BindingResolver::new(Some(root.clone()), false, registry_paths.clone()),
        Arc::new(RuntimeFactory::new(registry_paths)),
        runtime.clone(),
    );
    engine
        .execute(
            ToolRequest::new(
                "manage_workspace",
                json!({ "operation": "index", "path": root })
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .with_semantics(SemanticMode::Off),
            RequestContext::new(
                RequestOrigin::Cli,
                Some(Duration::from_secs(10)),
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let reply = engine
        .execute(
            ToolRequest::new(
                "fast_search",
                json!({ "query": "cold_lexical_probe" })
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .with_semantics(SemanticMode::Auto),
            RequestContext::new(
                RequestOrigin::Cli,
                Some(Duration::from_secs(2)),
                CancellationToken::new(),
            ),
        )
        .await
        .unwrap();
    assert!(!reply.is_error());
    assert!(reply.result.to_string().contains("cold_lexical_probe"));
    assert!(
        matches!(semantic_readiness(&runtime, temp.path(), SemanticMode::Auto, Duration::from_secs(2)).await.unwrap(), SemanticReadiness::Degraded { reason, .. } if reason == "MODEL_PREPARING")
    );
    assert!(started.elapsed() < Duration::from_millis(250));
    assert!(
        matches!(semantic_readiness(&runtime, temp.path(), SemanticMode::Auto, Duration::from_secs(2)).await.unwrap(), SemanticReadiness::Degraded { reason, .. } if reason == "MODEL_PREPARING")
    );
    assert!(
        semantic_readiness(
            &runtime,
            temp.path(),
            SemanticMode::Required,
            Duration::from_secs(2)
        )
        .await
        .unwrap()
        .is_ready()
    );
    assert_eq!(
        std::fs::read_to_string(cache.join("prepare-count")).unwrap(),
        "1"
    );

    let off_cache = temp.path().join("off-cache");
    env.set("JULIE_EMBEDDING_CACHE_DIR", &off_cache);
    let off_runtime = DefaultSemanticRuntime::from_registry_paths(RegistryPaths::with_home(
        temp.path().join("off-home"),
    ));
    assert!(matches!(
        semantic_readiness(
            &off_runtime,
            temp.path(),
            SemanticMode::Off,
            Duration::from_secs(1)
        )
        .await
        .unwrap(),
        SemanticReadiness::Disabled
    ));
    assert!(!off_cache.join("prepare-count").exists());

    let none_cache = temp.path().join("none-cache");
    env.set("JULIE_EMBEDDING_PROVIDER", "none");
    env.set("JULIE_EMBEDDING_CACHE_DIR", &none_cache);
    let none_runtime = DefaultSemanticRuntime::from_registry_paths(RegistryPaths::with_home(
        temp.path().join("none-home"),
    ));
    assert!(matches!(
        semantic_readiness(
            &none_runtime,
            temp.path(),
            SemanticMode::Auto,
            Duration::from_secs(1)
        )
        .await
        .unwrap(),
        SemanticReadiness::Disabled
    ));
    assert!(!none_cache.join("prepare-count").exists());
}

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn offline_missing_model_degrades_with_actionable_status() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    let mut env = EnvVarGuard::new();
    env.set("JULIE_EMBEDDING_PROVIDER", "native");
    env.set(
        "JULIE_NATIVE_SIDECAR_PROGRAM",
        compile_mock_sidecar(temp.path()),
    );
    env.set("JULIE_EMBEDDING_CACHE_DIR", &cache);
    env.set("PREPARE_FAIL", "offline cache unavailable");
    let runtime = DefaultSemanticRuntime::from_registry_paths(RegistryPaths::with_home(
        temp.path().join("home"),
    ));
    assert!(matches!(
        semantic_readiness(
            &runtime,
            temp.path(),
            SemanticMode::Auto,
            Duration::from_secs(1)
        )
        .await
        .unwrap(),
        SemanticReadiness::Degraded { .. }
    ));
    let error = semantic_readiness(
        &runtime,
        temp.path(),
        SemanticMode::Required,
        Duration::from_secs(2),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "SEMANTICS_NOT_READY");
    assert_eq!(error.details["model_id"], "bge-small-en-v1.5-f32");
    assert_eq!(error.details["cache_path"], cache.display().to_string());
    assert!(
        error.details["reason"]
            .as_str()
            .unwrap()
            .contains("offline cache unavailable")
    );
    assert_eq!(
        error.details["recovery"],
        "julie-semantic-sidecar prepare --model bge-small-en-v1.5-f32"
    );
    assert!(matches!(
        runtime.runtime_state().await,
        RuntimeProviderState::Degraded {
            retryable: true,
            ..
        }
    ));
    assert!(matches!(
        semantic_readiness(
            &runtime,
            temp.path(),
            SemanticMode::Auto,
            Duration::from_secs(1)
        )
        .await
        .unwrap(),
        SemanticReadiness::Degraded { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(cache.join("prepare-count")).unwrap(),
        "1"
    );
}

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn offline_warm_cache_reaches_semantic_ready() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path().join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("prepared"), "ready").unwrap();
    let mut env = EnvVarGuard::new();
    env.set("JULIE_EMBEDDING_PROVIDER", "native");
    env.set(
        "JULIE_NATIVE_SIDECAR_PROGRAM",
        compile_mock_sidecar(temp.path()),
    );
    env.set("JULIE_EMBEDDING_CACHE_DIR", &cache);
    env.set("PREPARE_FAIL", "offline must not prepare");
    let runtime = DefaultSemanticRuntime::from_registry_paths(RegistryPaths::with_home(
        temp.path().join("home"),
    ));
    assert!(
        semantic_readiness(
            &runtime,
            temp.path(),
            SemanticMode::Required,
            Duration::from_secs(2)
        )
        .await
        .unwrap()
        .is_ready()
    );
}

// ============================================================================
// 3. Integration Anchor Scenario
// ============================================================================

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn native_semantics_becomes_ready_without_client_restart() {
    let temp_repo = tempfile::tempdir().expect("temp repo dir");
    let root = make_isolated_workspace_root(temp_repo.path(), "native_recovery_ws");
    let temp_home = tempfile::tempdir().expect("temp home dir");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let cache_dir = temp_home.path().join("cache");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");

    let bin_path = compile_mock_sidecar(temp_home.path());

    let mut env = EnvVarGuard::new();
    env.set("JULIE_EMBEDDING_PROVIDER", "native");
    env.set("JULIE_NATIVE_SIDECAR_MODEL", "bge-small-en-v1.5-f32");
    env.set("JULIE_NATIVE_SIDECAR_PROGRAM", &bin_path);
    env.set("JULIE_EMBEDDING_CACHE_DIR", &cache_dir);
    env.set("REQUIRE_BARRIER", "1");

    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    std::fs::write(src_dir.join("lib.rs"), "pub fn recovery_probe() {}\n").expect("write file");

    let semantic_runtime = Arc::new(DefaultSemanticRuntime::from_registry_paths(
        registry_paths.clone(),
    ));
    let binding_resolver = BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths.clone()));
    let engine = RequestEngine::with_semantic_runtime(
        binding_resolver,
        runtime_factory.clone(),
        semantic_runtime,
    );

    let dummy_ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(30)),
        CancellationToken::new(),
    );
    let req_index = ToolRequest::new(
        "manage_workspace",
        json!({ "operation": "index", "path": root.to_string_lossy() })
            .as_object()
            .unwrap()
            .clone(),
    );
    let _ = engine.execute(req_index, dummy_ctx).await.unwrap();

    let req_auto = ToolRequest::new(
        "fast_search",
        json!({ "query": "recovery_probe" })
            .as_object()
            .unwrap()
            .clone(),
    )
    .with_semantics(SemanticMode::Auto);
    let ctx1 = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let reply_auto = engine
        .execute(req_auto, ctx1)
        .await
        .expect("auto semantics must succeed in lexical fallback");

    assert!(
        reply_auto.readiness.status.starts_with("degraded"),
        "expected degraded status while sidecar unavailable, got: {}",
        reply_auto.readiness.status
    );
    assert_eq!(reply_auto.readiness.mode, SemanticMode::Auto);

    let req_off = ToolRequest::new(
        "fast_search",
        json!({ "query": "recovery_probe" })
            .as_object()
            .unwrap()
            .clone(),
    )
    .with_semantics(SemanticMode::Off);
    let ctx_off = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let reply_off = engine
        .execute(req_off, ctx_off)
        .await
        .expect("off mode must succeed");
    assert_eq!(reply_off.readiness.status, "disabled");
    assert_eq!(reply_off.readiness.mode, SemanticMode::Off);

    // Release barrier so sidecar child will succeed
    let barrier_file = cache_dir.join("barrier");
    std::fs::write(&barrier_file, "ready").expect("write barrier");

    // Populate vectors in facts.sqlite matching the sidecar's expected identity
    let binding = engine
        .bindings
        .resolve(None, None, false)
        .unwrap()
        .expect("workspace binding resolved");
    let db_path = binding
        .index_root
        .join(julie_index::checkout_store::FACTS_FILE);
    let expected_identity = EncoderIdentity {
        schema: 1,
        model_id: "bge-small-en-v1.5-f32".to_string(),
        weights_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            .to_string(),
        dimensions: 384,
        pooling: "cls".to_string(),
        normalization: "l2".to_string(),
        instruction_policy: "v1".to_string(),
        text_format: 1,
        runtime_build: "llama.cpp-b3560".to_string(),
    };
    let expected_key = expected_identity.storage_key().expect("storage key");
    let conn = rusqlite::Connection::open(&db_path).expect("open facts db");
    set_facts_encoder(&conn, &expected_key, 384);
    let blob_hash: String = conn
        .query_row("SELECT blob_hash FROM paths LIMIT 1", [], |r| r.get(0))
        .expect("get indexed blob hash");
    let vec_bytes: Vec<u8> = vec![0.1_f32; 384]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    conn.execute(
        "INSERT OR REPLACE INTO vectors (blob_hash, symbol_ordinal, encoder_id, vector) VALUES (?1, 0, ?2, ?3)",
        rusqlite::params![blob_hash, expected_key, vec_bytes],
    )
    .expect("insert vector");
    drop(conn);
    let publish_context = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let runtime = runtime_factory
        .acquire(Some(&binding), &publish_context)
        .await
        .expect("acquire existing runtime");
    let store = runtime
        .handler()
        .checkout_store_for_workspace(&binding.workspace_id, &binding.root)
        .await
        .expect("open checkout store");
    store.publish_vectors().expect("publish inserted vector");

    // Second request with Required semantics in the SAME session
    let req_required = ToolRequest::new(
        "fast_search",
        json!({ "query": "recovery_probe" })
            .as_object()
            .unwrap()
            .clone(),
    )
    .with_semantics(SemanticMode::Required);
    let ctx2 = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );
    let reply_required = engine
        .execute(req_required, ctx2)
        .await
        .expect("required semantics must now succeed after dynamic recovery");

    assert_eq!(
        reply_required.readiness.status, "ready",
        "expected readiness status to be 'ready' after recovery"
    );
    assert_eq!(
        reply_required.readiness.mode,
        SemanticMode::Required,
        "expected mode to be Required"
    );
}

// ============================================================================
// 4. Adversarial Challenges
// ============================================================================

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn challenge_single_flight_concurrency_and_cancellation_isolation() {
    let temp_repo = tempfile::tempdir().expect("temp repo dir");
    let root = make_isolated_workspace_root(temp_repo.path(), "challenge_single_flight_ws");
    let temp_home = tempfile::tempdir().expect("temp home dir");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let cache_dir = temp_home.path().join("cache");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");

    let bin_path = compile_mock_sidecar(temp_home.path());

    let mut env = EnvVarGuard::new();
    env.set("JULIE_EMBEDDING_PROVIDER", "native");
    env.set("JULIE_NATIVE_SIDECAR_MODEL", "bge-small-en-v1.5-f32");
    env.set("JULIE_NATIVE_SIDECAR_PROGRAM", &bin_path);
    env.set("JULIE_EMBEDDING_CACHE_DIR", &cache_dir);

    let index_root = temp_home.path().join("indexes/ws1");
    std::fs::create_dir_all(&index_root).expect("create index dir");
    let db_path = index_root.join(julie_index::checkout_store::FACTS_FILE);
    {
        let expected_identity = EncoderIdentity {
            schema: 1,
            model_id: "bge-small-en-v1.5-f32".to_string(),
            weights_sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
            dimensions: 384,
            pooling: "cls".to_string(),
            normalization: "l2".to_string(),
            instruction_policy: "v1".to_string(),
            text_format: 1,
            runtime_build: "llama.cpp-b3560".to_string(),
        };
        let expected_key = expected_identity.storage_key().expect("storage key");
        let conn = setup_facts_db(&db_path);
        set_facts_encoder(&conn, &expected_key, 384);
        insert_facts_vector(&conn, &expected_key, 384);
    }

    let binding = WorkspaceBinding {
        workspace_id: "ws1".to_string(),
        root: root.clone(),
        index_root: index_root.clone(),
    };

    let runtime = Arc::new(DefaultSemanticRuntime::from_registry_paths(registry_paths));
    assert_eq!(
        runtime.runtime_state().await,
        RuntimeProviderState::Starting
    );

    // Concurrency test:
    // Caller 1 is cancelled immediately
    let cancel1 = CancellationToken::new();
    cancel1.cancel();

    // Caller 2 and Caller 3 have active tokens
    let cancel2 = CancellationToken::new();
    let cancel3 = CancellationToken::new();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    let r1 = Arc::clone(&runtime);
    let b1 = binding.clone();
    let t1 = tokio::spawn(async move {
        r1.ensure_ready(
            &b1,
            None,
            &crate::search::language_config::LanguageConfigs::load_embedded(),
            SemanticRequirement::Query,
            SemanticMode::Required,
            deadline,
            &cancel1,
        )
        .await
    });

    let r2 = Arc::clone(&runtime);
    let b2 = binding.clone();
    let cancel2_clone = cancel2.clone();
    let t2 = tokio::spawn(async move {
        r2.ensure_ready(
            &b2,
            None,
            &crate::search::language_config::LanguageConfigs::load_embedded(),
            SemanticRequirement::Query,
            SemanticMode::Required,
            deadline,
            &cancel2_clone,
        )
        .await
    });

    let r3 = Arc::clone(&runtime);
    let b3 = binding.clone();
    let cancel3_clone = cancel3.clone();
    let t3 = tokio::spawn(async move {
        r3.ensure_ready(
            &b3,
            None,
            &crate::search::language_config::LanguageConfigs::load_embedded(),
            SemanticRequirement::Query,
            SemanticMode::Required,
            deadline,
            &cancel3_clone,
        )
        .await
    });

    let (res1, res2, res3) = tokio::join!(t1, t2, t3);

    // Caller 1 MUST fail with cancelled
    let res1 = res1
        .expect("task 1 join")
        .expect_err("caller 1 must be cancelled");
    assert!(
        res1.code.contains("CANCELLED") || res1.message.contains("cancelled"),
        "expected cancelled, got: {:?}",
        res1
    );

    // Caller 2 and Caller 3 MUST succeed with Ready
    let res2 = res2.expect("task 2 join").expect("caller 2 must succeed");
    let res3 = res3.expect("task 3 join").expect("caller 3 must succeed");

    assert!(res2.is_ready());
    assert!(res3.is_ready());

    // Single-flight verification: both callers observe the same provider and state is Ready
    let p = runtime.provider().expect("provider cached");
    assert_eq!(p.dimensions(), 384);
    assert_eq!(runtime.runtime_state().await, RuntimeProviderState::Ready);
}

#[tokio::test]
async fn challenge_required_mode_fails_closed_when_generation_unready() {
    let temp_repo = tempfile::tempdir().expect("temp repo dir");
    let root = make_isolated_workspace_root(temp_repo.path(), "challenge_fail_closed_ws");
    let index_root = temp_repo.path().join("indexes/ws2");
    std::fs::create_dir_all(&index_root).expect("create index dir");
    let db_path = index_root.join(julie_index::checkout_store::FACTS_FILE);

    let binding = WorkspaceBinding {
        workspace_id: "ws2".to_string(),
        root: root.clone(),
        index_root: index_root.clone(),
    };

    let mock_provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5-f32", 384));
    let runtime = Arc::new(DefaultSemanticRuntime::new(Some(mock_provider.clone())));
    let cancel = CancellationToken::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    let conn = setup_facts_db(&db_path);
    let store = julie_index::checkout_store::CheckoutStore::open(&index_root, &root).unwrap();
    let configs = crate::search::language_config::LanguageConfigs::load_embedded();
    let snapshot = store.current();
    let res_no_encoder = runtime
        .ensure_ready(
            &binding,
            Some(snapshot.as_ref()),
            &configs,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await;
    let err_no_encoder =
        res_no_encoder.expect_err("must fail closed when symbols exist but no encoder");
    assert_eq!(err_no_encoder.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_no_encoder.details["coverage"], "missing");

    set_facts_encoder(&conn, "other-model", 384);
    store.publish_vectors().unwrap();
    let snapshot = store.current();
    let res_incompatible = runtime
        .ensure_ready(
            &binding,
            Some(snapshot.as_ref()),
            &configs,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await;
    let err_incompatible =
        res_incompatible.expect_err("must fail closed when model is incompatible");
    assert_eq!(err_incompatible.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_incompatible.details["coverage"], "incompatible");

    let expected_identity = mock_provider.encoder_identity().expect("encoder identity");
    let expected_key = expected_identity.storage_key().expect("storage key");
    set_facts_encoder(&conn, &expected_key, 384);
    store.publish_vectors().unwrap();
    let snapshot = store.current();
    let res_no_vectors = runtime
        .ensure_ready(
            &binding,
            Some(snapshot.as_ref()),
            &configs,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await;
    let err_no_vectors =
        res_no_vectors.expect_err("must fail closed when symbols exist but 0 vectors");
    assert_eq!(err_no_vectors.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_no_vectors.details["coverage"], "missing");

    let res_auto = runtime
        .ensure_ready(
            &binding,
            Some(snapshot.as_ref()),
            &configs,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Auto,
            deadline,
            &cancel,
        )
        .await
        .expect("auto mode succeeds degraded");
    assert_eq!(res_auto.reason(), Some("VECTORS_MISSING"));

    insert_facts_vector(&conn, &expected_key, 384);
    store.publish_vectors().unwrap();
    let snapshot = store.current();

    let res_ready = runtime
        .ensure_ready(
            &binding,
            Some(snapshot.as_ref()),
            &configs,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect("must succeed after vectors stored");
    assert!(res_ready.is_ready());
    if let SemanticReadiness::Ready {
        eligible_symbols,
        embedded_symbols,
        ..
    } = res_ready
    {
        assert_eq!(eligible_symbols, 1);
        assert_eq!(embedded_symbols, 1);
    } else {
        panic!("expected Ready variant");
    }
}

#[tokio::test]
async fn challenge_dynamic_recovery_state_transitions() {
    // 1. Initialized with None provider -> Disabled
    let runtime = DefaultSemanticRuntime::new(None);
    assert_eq!(
        runtime.runtime_state().await,
        RuntimeProviderState::Disabled
    );

    // 2. Updated to Some provider -> Ready
    let provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5-f32", 384));
    runtime.update_provider(Some(provider.clone())).await;
    assert_eq!(runtime.runtime_state().await, RuntimeProviderState::Ready);

    // 3. Provider becomes unavailable -> Degraded (retryable: true)
    runtime.update_provider(None).await;
    assert_eq!(
        runtime.runtime_state().await,
        RuntimeProviderState::Degraded {
            reason: "PROVIDER_UNAVAILABLE".to_string(),
            retryable: true,
        }
    );
    assert!(runtime.runtime_state().await.is_retryable());

    // 4. Provider recovers -> Ready
    runtime.update_provider(Some(provider)).await;
    assert_eq!(runtime.runtime_state().await, RuntimeProviderState::Ready);
    assert!(runtime.runtime_state().await.is_ready());
}

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn challenge_off_mode_nl_query_performs_zero_provider_acquisition() {
    let temp_repo = tempfile::tempdir().expect("temp repo dir");
    let root = make_isolated_workspace_root(temp_repo.path(), "challenge_off_nl_ws");
    let temp_home = tempfile::tempdir().expect("temp home dir");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());
    let cache_dir = temp_home.path().join("cache");
    std::fs::create_dir_all(&cache_dir).expect("create cache dir");

    let semantic_runtime = Arc::new(DefaultSemanticRuntime::from_registry_paths(
        registry_paths.clone(),
    ));
    let binding_resolver = BindingResolver::new(Some(root.clone()), false, registry_paths.clone());
    let runtime_factory = Arc::new(RuntimeFactory::new(registry_paths.clone()));
    let engine = RequestEngine::with_semantic_runtime(
        binding_resolver,
        runtime_factory.clone(),
        semantic_runtime,
    );

    let req_off_nl = ToolRequest::new(
        "fast_search",
        json!({ "query": "how does the natural language authentication flow work" })
            .as_object()
            .unwrap()
            .clone(),
    )
    .with_semantics(SemanticMode::Off);

    let ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(10)),
        CancellationToken::new(),
    );

    let _ = take_nl_definition_embedding_init_attempts(&root);

    let reply = engine
        .execute(req_off_nl, ctx)
        .await
        .expect("off mode query must succeed");

    assert_eq!(reply.readiness.status, "disabled");
    assert_eq!(reply.readiness.mode, SemanticMode::Off);

    let attempts_after = take_nl_definition_embedding_init_attempts(&root);
    assert_eq!(
        attempts_after, 0,
        "NL query in Off mode must not trigger deferred embedding init attempt"
    );
}
