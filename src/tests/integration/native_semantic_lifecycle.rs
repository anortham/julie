//! Integration tests for Milestone N4: Native Semantic Runtime Lifecycle and Dynamic Recovery.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::handler::embedding_init::take_nl_definition_embedding_init_attempts;
use crate::paths::RegistryPaths;
use crate::request_engine::semantic::{
    CURRENT_EMBEDDING_FORMAT_VERSION, DefaultSemanticRuntime, RuntimeProviderState, SemanticMode,
    SemanticReadiness, SemanticRequirement, SemanticRuntime, semantic_mode_needs_provider,
};
use crate::request_engine::types::WorkspaceBinding;
use crate::request_engine::{
    BindingResolver, RequestContext, RequestEngine, RequestOrigin, RuntimeFactory, ToolRequest,
};
use crate::tests::helpers::env::EnvVarGuard;
use crate::tests::helpers::workspace::make_isolated_workspace_root;
use crate::tests::semantic_request_contract::MockReadyProvider;
use julie_core::database::SymbolDatabase;
use julie_core::embeddings_contract::EmbeddingProvider;
use julie_pipeline::embeddings::native::launch::{
    derive_broker_paths, find_and_hash_sidecar_binary,
};

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

/// Owns a spawned mock broker and kills it on drop, including on panic.
#[cfg(unix)]
struct KillOnDrop(std::process::Child);

#[cfg(unix)]
impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(unix)]
fn expected_mock_sidecar_identity() -> julie_core::embeddings_contract::EncoderIdentity {
    julie_core::embeddings_contract::EncoderIdentity {
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
use std::os::unix::net::UnixListener;
use std::path::Path;

fn main() {
    if env::var("REQUIRE_BARRIER").is_ok() {
        if let Ok(cache_dir) = env::var("JULIE_EMBEDDING_CACHE_DIR") {
            let barrier = Path::new(&cache_dir).join("barrier");
            if !barrier.exists() {
                std::process::exit(1);
            }
        }
    }
    let mut endpoint = env::var("MOCK_ENDPOINT").ok();
    let args: Vec<String> = env::args().collect();
    for i in 0..args.len() {
        if args[i] == "--endpoint" && i + 1 < args.len() {
            endpoint = Some(args[i + 1].clone());
        }
    }
    let endpoint = match endpoint {
        Some(ep) => ep,
        None => std::process::exit(0),
    };
    if args.iter().any(|a| a == "--lock") {
        std::thread::spawn(|| {
            let mut byte = [0u8; 1];
            let _ = std::io::Read::read(&mut std::io::stdin(), &mut byte);
            std::process::exit(0);
        });
    }
    let ep_path = Path::new(&endpoint);
    let _ = std::fs::remove_file(ep_path);
    let listener = match UnixListener::bind(ep_path) {
        Ok(l) => l,
        Err(_) => std::process::exit(0),
    };
    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
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

                let reply = if line.contains("\"health\"") {
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
                } else {
                    format!("{{\"schema\":\"julie.embedding.sidecar\",\"version\":1,\"request_id\":\"{req_id}\",\"result\":null,\"error\":null}}\n")
                };
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.flush();
                line.clear();
            }
        }
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

// ============================================================================
// 3. Integration Anchor Scenario
// ============================================================================

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn native_semantics_becomes_ready_without_client_restart() {
    // 1. Setup isolated directories and paths
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

    let (_bin, mock_sha) = find_and_hash_sidecar_binary(Some(&bin_path)).expect("hash sidecar");
    let broker_paths =
        derive_broker_paths(&cache_dir, &mock_sha, "bge-small-en-v1.5-f32").expect("broker paths");

    // Seed workspace source files
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).expect("create src dir");
    std::fs::write(src_dir.join("lib.rs"), "pub fn recovery_probe() {}\n").expect("write file");

    // 2. Initialize long-running RequestEngine
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

    // Warm up runtime factory so the workspace binding is registered
    let dummy_ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(30)),
        CancellationToken::new(),
    );
    let binding = engine
        .bindings
        .resolve(None, None, false)
        .unwrap()
        .expect("workspace binding resolved");
    let _ = runtime_factory
        .acquire(Some(&binding), &dummy_ctx)
        .await
        .unwrap();

    let db_path = binding.index_root.join("db/symbols.db");

    // Initialize symbols in symbols.db
    {
        let mut db = SymbolDatabase::new(&db_path).expect("open db");
        let file = crate::tests::helpers::db::file_info_builder("src/lib.rs")
            .language("rust")
            .hash("feedbeef")
            .size(100)
            .last_modified(0)
            .last_indexed(0)
            .build();
        crate::tests::helpers::db::store_file_info_if_missing(&mut db, &file)
            .expect("store file info");
        let sym =
            crate::tests::helpers::db::symbol_builder("sym_1", "recovery_probe", "src/lib.rs")
                .build();
        db.store_symbols(&[sym]).expect("store symbols");
    }

    // 3. Step 1: Initial request with Auto semantics while broker is NOT yet available.
    // Broker socket does not exist yet -> should gracefully degrade to keyword-only search.
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
        "expected degraded status while broker unavailable, got: {}",
        reply_auto.readiness.status
    );
    assert_eq!(
        reply_auto.readiness.coverage,
        Some("missing".to_string()),
        "expected coverage to be 'missing' while degraded"
    );
    assert_eq!(reply_auto.readiness.mode, SemanticMode::Auto);

    // Also verify zero-overhead Off mode
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

    // 4. Step 2: Release broker barrier by creating barrier file and spawning child broker.
    let barrier_file = cache_dir.join("barrier");
    std::fs::write(&barrier_file, "ready").expect("write barrier");

    let _child = KillOnDrop(
        std::process::Command::new(&bin_path)
            .env("REQUIRE_BARRIER", "1")
            .env("JULIE_EMBEDDING_CACHE_DIR", &cache_dir)
            .env("MOCK_ENDPOINT", &broker_paths.endpoint_path)
            .spawn()
            .expect("spawn mock broker"),
    );

    for _ in 0..100 {
        if broker_paths.endpoint_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    // 5. Step 3: Populate compatible SQLite embedding generation & vectors.
    {
        let mut db = SymbolDatabase::new(&db_path).expect("open db");
        let expected_key = expected_mock_sidecar_identity()
            .storage_key()
            .expect("storage key");
        let rev = db
            .get_latest_canonical_revision_number()
            .expect("canonical rev")
            .unwrap_or(0);
        let gen_id = db
            .begin_embedding_generation(&expected_key, rev, 384)
            .expect("begin generation");
        assert!(gen_id > 0);
        db.store_embeddings_for_generation(gen_id, &[("sym_1".to_string(), vec![0.1; 384])])
            .expect("store embedding");
        db.publish_embedding_generation(gen_id, rev, 1, 1)
            .expect("publish generation");
    }

    // 6. Step 4: Second request with Required semantics in the SAME session.
    // The runtime dynamically re-probes, attaches to the live host socket,
    // verifies generation readiness, and transitions to Ready without client restart.
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
        .expect("required semantics must now succeed after dynamic broker recovery");

    assert_eq!(
        reply_required.readiness.status, "ready",
        "expected readiness status to be 'ready' after recovery"
    );
    assert_eq!(
        reply_required.readiness.mode,
        SemanticMode::Required,
        "expected mode to be Required"
    );
    assert_eq!(
        reply_required.readiness.coverage,
        Some("full".to_string()),
        "expected coverage to be 'full' after recovery"
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

    let (_bin, mock_sha) = find_and_hash_sidecar_binary(Some(&bin_path)).expect("hash sidecar");
    let broker_paths =
        derive_broker_paths(&cache_dir, &mock_sha, "bge-small-en-v1.5-f32").expect("broker paths");

    let _child = KillOnDrop(
        std::process::Command::new(&bin_path)
            .env("MOCK_ENDPOINT", &broker_paths.endpoint_path)
            .spawn()
            .expect("spawn mock broker"),
    );

    for _ in 0..100 {
        if broker_paths.endpoint_path.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }

    let index_root = temp_home.path().join("indexes/ws1");
    std::fs::create_dir_all(index_root.join("db")).expect("create db dir");
    let db_path = index_root.join("db/symbols.db");
    {
        let mut db = SymbolDatabase::new(&db_path).expect("open db");
        let file = crate::tests::helpers::db::file_info_builder("src/lib.rs")
            .language("rust")
            .hash("feedbeef")
            .size(100)
            .last_modified(0)
            .last_indexed(0)
            .build();
        crate::tests::helpers::db::store_file_info_if_missing(&mut db, &file)
            .expect("store file info");
        let sym = crate::tests::helpers::db::symbol_builder("sym_1", "probe", "src/lib.rs").build();
        db.store_symbols(&[sym]).expect("store symbols");
        let expected_key = expected_mock_sidecar_identity()
            .storage_key()
            .expect("storage key");
        let rev = db
            .get_latest_canonical_revision_number()
            .expect("canonical rev")
            .unwrap_or(0);
        let gen_id = db
            .begin_embedding_generation(&expected_key, rev, 384)
            .expect("begin gen");
        db.store_embeddings_for_generation(gen_id, &[("sym_1".to_string(), vec![0.1; 384])])
            .expect("store emb");
        db.publish_embedding_generation(gen_id, rev, 1, 1)
            .expect("publish gen");
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
            SemanticRequirement::QueryAndSymbols,
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
            SemanticRequirement::QueryAndSymbols,
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
            SemanticRequirement::QueryAndSymbols,
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
    std::fs::create_dir_all(index_root.join("db")).expect("create db dir");
    let db_path = index_root.join("db/symbols.db");

    let binding = WorkspaceBinding {
        workspace_id: "ws2".to_string(),
        root,
        index_root,
    };

    let mock_provider = Arc::new(MockReadyProvider::new("bge-small-en-v1.5-f32", 384));
    let runtime = Arc::new(DefaultSemanticRuntime::new(Some(mock_provider.clone())));
    let cancel = CancellationToken::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);

    // Case 0: DB file missing
    let res_missing = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await;
    let err_missing = res_missing.expect_err("must fail closed when db file is missing");
    assert_eq!(err_missing.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_missing.details["coverage"], "missing");

    // Case 1: DB exists with default config (bge-small-en-v1.5) incompatible with provider (bge-small-en-v1.5-f32)
    {
        let _db = SymbolDatabase::new(&db_path).expect("open db");
    }
    let res_incompatible = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await;
    let err_incompatible =
        res_incompatible.expect_err("must fail closed when model name is incompatible");
    assert_eq!(err_incompatible.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_incompatible.details["coverage"], "incompatible");

    // Case 1b: Symbols exist and config matches, but 0 generations recorded
    let (expected_key, rev) = {
        let mut db = SymbolDatabase::new(&db_path).expect("open db");
        let file = crate::tests::helpers::db::file_info_builder("src/lib.rs")
            .language("rust")
            .hash("feedbeef")
            .size(100)
            .last_modified(0)
            .last_indexed(0)
            .build();
        crate::tests::helpers::db::store_file_info_if_missing(&mut db, &file)
            .expect("store file info");
        let sym = crate::tests::helpers::db::symbol_builder("sym_1", "probe", "src/lib.rs").build();
        db.store_symbols(&[sym]).expect("store symbols");

        let expected_key = mock_provider
            .encoder_identity()
            .and_then(|id| id.storage_key())
            .expect("storage key");
        let rev = db
            .get_latest_canonical_revision_number()
            .expect("canonical rev")
            .unwrap_or(0);
        db.set_embedding_config(&expected_key, 384, CURRENT_EMBEDDING_FORMAT_VERSION)
            .expect("align config");
        (expected_key, rev)
    };

    let res_no_gen = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await;
    let err_no_gen = res_no_gen.expect_err("must fail closed when 0 generations recorded");
    assert_eq!(err_no_gen.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_no_gen.details["coverage"], "missing");

    // Case 2: Tables exist, symbols exist, but generation is building
    let gen_id = {
        let mut db = SymbolDatabase::new(&db_path).expect("open db");
        let gen_id = db
            .begin_embedding_generation(&expected_key, rev, 384)
            .expect("begin gen");
        db.store_embeddings_for_generation(gen_id, &[("sym_1".to_string(), vec![0.1; 384])])
            .expect("store emb");
        gen_id
    };

    let res_building = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await;
    let err_building = res_building.expect_err("must fail closed when generation is building");
    assert_eq!(err_building.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_building.details["coverage"], "building");

    // In Auto mode, building should report Degraded { reason: "GENERATION_BUILDING", retryable: true }
    let res_auto = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Auto,
            deadline,
            &cancel,
        )
        .await
        .expect("auto mode succeeds degraded");
    assert_eq!(res_auto.reason(), Some("GENERATION_BUILDING"));

    // Case 3: Publish generation -> Required mode now succeeds!
    {
        let mut db = SymbolDatabase::new(&db_path).expect("open db");
        db.publish_embedding_generation(gen_id, rev, 1, 1)
            .expect("publish gen");
    }

    let res_ready = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect("must succeed after publish");
    assert!(res_ready.is_ready());
    if let SemanticReadiness::Ready {
        vector_generation,
        eligible_symbols,
        embedded_symbols,
        ..
    } = res_ready
    {
        assert_eq!(vector_generation, Some(gen_id));
        assert_eq!(eligible_symbols, 1);
        assert_eq!(embedded_symbols, 1);
    } else {
        panic!("expected Ready variant");
    }

    // Case 4: format_version == 1 fails closed (coverage: stale)
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open db raw");
        conn.execute(
            "UPDATE embedding_config SET format_version = 1 WHERE id = 1",
            [],
        )
        .expect("set format 1");
    }
    let err_fmt = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect_err("must fail closed when format_version == 1");
    assert_eq!(err_fmt.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_fmt.details["coverage"], "stale");
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open db raw");
        conn.execute(
            "UPDATE embedding_config SET format_version = ?1 WHERE id = 1",
            rusqlite::params![CURRENT_EMBEDDING_FORMAT_VERSION],
        )
        .expect("restore format");
    }

    // Case 5: embedded_symbols < eligible_symbols fails closed (coverage: incomplete)
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open db raw");
        conn.execute(
            "UPDATE embedding_generations SET embedded_symbols = 0 WHERE id = ?1",
            rusqlite::params![gen_id],
        )
        .expect("set incomplete");
    }
    let err_inc = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect_err("must fail closed when embedded < eligible");
    assert_eq!(err_inc.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_inc.details["coverage"], "incomplete");
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open db raw");
        conn.execute(
            "UPDATE embedding_generations SET embedded_symbols = 1 WHERE id = ?1",
            rusqlite::params![gen_id],
        )
        .expect("restore complete");
    }

    // Case 6: gen_source_revision < canonical_revisions.revision fails closed (coverage: stale)
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open db raw");
        conn.execute(
            "INSERT INTO canonical_revisions (revision, workspace_id, kind, created_at)
             VALUES (999, 'ws2', 'incremental', 123456789)",
            [],
        )
        .expect("insert advanced rev");
    }
    let err_stale_rev = runtime
        .ensure_ready(
            &binding,
            SemanticRequirement::QueryAndSymbols,
            SemanticMode::Required,
            deadline,
            &cancel,
        )
        .await
        .expect_err("must fail closed when generation is stale");
    assert_eq!(err_stale_rev.code, "SEMANTICS_NOT_READY");
    assert_eq!(err_stale_rev.details["coverage"], "stale");
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

#[tokio::test]
#[serial_test::serial(embedding_env)]
async fn challenge_required_mode_fails_closed_on_unstarted_broker() {
    let temp_repo = tempfile::tempdir().expect("temp repo dir");
    let root = make_isolated_workspace_root(temp_repo.path(), "challenge_unstarted_broker_ws");
    let temp_home = tempfile::tempdir().expect("temp home dir");
    let registry_paths = RegistryPaths::with_home(temp_home.path().to_path_buf());

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

    let req_required = ToolRequest::new(
        "fast_search",
        json!({ "query": "auth_token_probe" })
            .as_object()
            .unwrap()
            .clone(),
    )
    .with_semantics(SemanticMode::Required);

    let ctx = RequestContext::new(
        RequestOrigin::Cli,
        Some(Duration::from_secs(5)),
        CancellationToken::new(),
    );

    let result = engine.execute(req_required, ctx).await;
    let err = result.expect_err("Required mode on unstarted broker must fail closed");
    assert_eq!(
        err.code, "SEMANTICS_NOT_READY",
        "error code must strictly be SEMANTICS_NOT_READY"
    );
}
