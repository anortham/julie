//! Background embedding generation task
//! Generates ONNX embeddings asynchronously from SQLite data
//! Provides incremental updates to avoid reprocessing existing embeddings

use crate::database::SymbolDatabase;
use crate::handler::IndexingStatus;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};

/// Embedding generation batch size for ONNX inference
/// Tested: 256 (76s), 64 (60s), 100 (60s) - CPU bottleneck, not batch overhead
const BATCH_SIZE: usize = 100;

/// Maximum consecutive batch failures before circuit breaker activates
const MAX_CONSECUTIVE_FAILURES: usize = 5;

/// Maximum failure rate (>50% triggers abort)
const MAX_TOTAL_FAILURE_RATE: f64 = 0.5;

/// Generate embeddings from SQLite database
///
/// This runs asynchronously to provide fast indexing response times.
/// Processes symbols in batches with incremental database persistence.
pub async fn generate_embeddings_from_sqlite(
    embedding_engine: Arc<tokio::sync::RwLock<Option<crate::embeddings::EmbeddingEngine>>>,
    embedding_engine_last_used: Arc<tokio::sync::Mutex<Option<std::time::Instant>>>,
    workspace_db: Option<Arc<Mutex<SymbolDatabase>>>,
    workspace_root: Option<PathBuf>,
    workspace_id: String,
    indexing_status: Arc<IndexingStatus>,
) -> Result<()> {
    info!(
        "🐛 generate_embeddings_from_sqlite() called for workspace: {}",
        workspace_id
    );
    let start_time = std::time::Instant::now();
    debug!("Starting embedding generation from SQLite");

    // 🐛 SKIP REGISTRY UPDATE: Causes deadlock as main thread holds registry lock during statistics update
    // The registry status update is non-critical for background task operation
    info!("🐛 Skipping registry update to avoid deadlock");

    // Get database connection
    let db = match workspace_db {
        Some(db_arc) => db_arc,
        None => {
            warn!("No database available for embedding generation");
            return Ok(());
        }
    };

    // 🚀 INCREMENTAL UPDATES: Only process symbols that don't have embeddings yet
    // This fixes the performance problem where ALL symbols were reprocessed every startup
    info!("🐛 About to acquire database lock for reading symbols without embeddings...");
    let symbols = {
        let db_lock = db.lock().unwrap();
        info!("🐛 Database lock acquired successfully!");
        db_lock
            .get_symbols_without_embeddings(&workspace_id)
            .context("Failed to read symbols without embeddings from database")?
    };
    info!(
        "🐛 Read {} symbols WITHOUT embeddings (incremental update)",
        symbols.len()
    );

    if symbols.is_empty() {
        info!("✅ All symbols already have embeddings - nothing to do!");
        return Ok(());
    }

    info!(
        "🧠 Generating embeddings for {} new symbols (incremental)...",
        symbols.len()
    );

    // Initialize embedding engine if needed
    initialize_embedding_engine(&embedding_engine, &workspace_root, &db).await?;

    // Generate embeddings in batches
    let total_batches = symbols.len().div_ceil(BATCH_SIZE);
    let mut consecutive_failures = 0;
    let mut total_failures = 0;
    let mut successful_batches = 0;

    let mut model_name = String::from("bge-small");
    let mut dimensions = 384;

    for (batch_idx, chunk) in symbols.chunks(BATCH_SIZE).enumerate() {
        info!(
            "🔄 Processing embedding batch {}/{} ({} symbols)",
            batch_idx + 1,
            total_batches,
            chunk.len()
        );

        // 🔓 CRITICAL: Acquire write lock ONLY for this batch, then release
        // This allows other workspaces to interleave their batches for parallel execution
        let batch_result = {
            let mut embedding_guard = embedding_engine.write().await;
            if let Some(ref mut engine) = embedding_guard.as_mut() {
                match engine.embed_symbols_batch(chunk) {
                    Ok(batch_embeddings) => {
                        model_name = engine.model_name().to_string();
                        dimensions = engine.dimensions();

                        // 🕐 Update last_used timestamp after successful engine use
                        {
                            let mut last_used = embedding_engine_last_used.lock().await;
                            *last_used = Some(std::time::Instant::now());
                        }

                        Some(Ok(batch_embeddings))
                    }
                    Err(e) => Some(Err(e)),
                }
            } else {
                None
            }
        }; // 🔓 Write lock released here - other workspaces can now process their batches!

        // Process the batch result without holding the embedding engine lock
        match batch_result {
            Some(Ok(batch_embeddings)) => {
                // Reset consecutive failure counter on success
                consecutive_failures = 0;
                successful_batches += 1;

                // 🔥 MEMORY OPTIMIZATION: Write to DB immediately (incremental persistence)
                // This avoids accumulating embeddings in memory
                {
                    let mut db_guard = db.lock().unwrap();

                    // Use bulk insert for this batch
                    if let Err(e) = db_guard.bulk_store_embeddings(
                        &batch_embeddings,
                        dimensions,
                        &model_name,
                    ) {
                        warn!(
                            "Failed to bulk store embeddings for batch {}: {}",
                            batch_idx + 1,
                            e
                        );
                    }
                }

                debug!(
                    "✅ Generated and stored embeddings for batch {}/{} ({} embeddings)",
                    batch_idx + 1,
                    total_batches,
                    batch_embeddings.len()
                );
            }
            Some(Err(e)) => {
                consecutive_failures += 1;
                total_failures += 1;

                warn!(
                    "⚠️ Failed to generate embeddings for batch {}: {} (consecutive failures: {})",
                    batch_idx + 1,
                    e,
                    consecutive_failures
                );

                // 🚨 CIRCUIT BREAKER: Stop if too many consecutive failures
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    error!(
                        "❌ CIRCUIT BREAKER TRIGGERED: {} consecutive embedding failures. \
                         Aborting embedding generation to prevent resource waste. \
                         Successfully processed {}/{} batches.",
                        consecutive_failures,
                        successful_batches,
                        batch_idx + 1
                    );
                    return Err(anyhow::anyhow!(
                        "Embedding generation aborted after {} consecutive batch failures",
                        consecutive_failures
                    ));
                }

                // 🚨 FAILURE RATE CHECK: Stop if >50% of batches are failing
                let batches_processed = batch_idx + 1;
                if batches_processed >= 10 {
                    // Only check after reasonable sample size
                    let failure_rate = total_failures as f64 / batches_processed as f64;
                    if failure_rate > MAX_TOTAL_FAILURE_RATE {
                        error!(
                            "❌ HIGH FAILURE RATE: {:.1}% of batches failing ({}/{}). \
                             Aborting embedding generation. This indicates a systemic issue.",
                            failure_rate * 100.0,
                            total_failures,
                            batches_processed
                        );
                        return Err(anyhow::anyhow!(
                            "Embedding generation aborted due to high failure rate: {:.1}%",
                            failure_rate * 100.0
                        ));
                    }
                }
            }
            None => {
                return Err(anyhow::anyhow!("Embedding engine not available"));
            }
        }
    }

    let duration = start_time.elapsed();
    info!(
        "✅ Embedding generation complete in {:.2}s ({} total symbols)",
        duration.as_secs_f64(),
        symbols.len()
    );

    // Build and save HNSW index
    build_and_save_hnsw_index(&db, &model_name, &workspace_id, &workspace_root).await?;

    info!("✅ Background task complete - semantic search ready via lazy loading!");

    // CASCADE: Mark semantic search as ready
    indexing_status
        .semantic_ready
        .store(true, std::sync::atomic::Ordering::Release);
    debug!("🧠 CASCADE: Semantic search now available");

    // 🕐 LAZY ENGINE CLEANUP: Engine will be dropped after 5 minutes of inactivity
    // The periodic cleanup task (started at server init) checks last_used timestamp
    // and drops the engine if it's been idle for >5 minutes
    // This balances fast incremental updates during development with memory cleanup when done
    info!("✅ Embedding engine will be dropped after 5 minutes of inactivity");
    info!("🐛 Embeddings complete - registry update skipped to avoid deadlock");

    Ok(())
}

/// Initialize embedding engine with double-checked locking pattern
async fn initialize_embedding_engine(
    embedding_engine: &Arc<tokio::sync::RwLock<Option<crate::embeddings::EmbeddingEngine>>>,
    workspace_root: &Option<PathBuf>,
    db: &Arc<Mutex<SymbolDatabase>>,
) -> Result<()> {
    // Fast path: check with read lock first
    let needs_init = {
        let read_guard = embedding_engine.read().await;
        read_guard.is_none()
    };

    if !needs_init {
        return Ok(());
    }

    // Slow path: acquire write lock only for initialization
    let mut write_guard = embedding_engine.write().await;

    // Double-check: another task might have initialized while we waited
    if write_guard.is_none() {
        info!("🔧 Initializing embedding engine for background generation...");

        // 🔧 FIX: Use workspace .julie/cache directory instead of polluting CWD
        let cache_dir = if let Some(ref root) = workspace_root {
            root.join(".julie").join("cache").join("embeddings")
        } else {
            // Fallback to temp directory if workspace root not available
            std::env::temp_dir().join("julie_cache").join("embeddings")
        };

        std::fs::create_dir_all(&cache_dir)?;
        info!(
            "📁 Using embedding cache directory: {}",
            cache_dir.display()
        );

        // 🚨 CRITICAL: ONNX model loading is BLOCKING and can take seconds (download + init)
        // Must run on blocking thread pool to avoid deadlocking the tokio runtime
        // Same fix as workspace/mod.rs:458
        let db_clone = db.clone();
        let cache_dir_clone = cache_dir.clone();
        match tokio::task::spawn_blocking(move || {
            crate::embeddings::EmbeddingEngine::new("bge-small", cache_dir_clone, db_clone)
        })
        .await
        {
            Ok(Ok(engine)) => {
                *write_guard = Some(engine);
                info!("✅ Embedding engine initialized for background task");
            }
            Ok(Err(e)) => {
                error!("❌ Failed to initialize embedding engine: {}", e);
                return Err(anyhow::anyhow!(
                    "Embedding engine initialization failed: {}",
                    e
                ));
            }
            Err(join_err) => {
                error!(
                    "❌ Embedding engine initialization task panicked: {}",
                    join_err
                );
                return Err(anyhow::anyhow!(
                    "Embedding engine initialization task failed: {}",
                    join_err
                ));
            }
        }
    }

    Ok(())
}

/// Build HNSW index from database embeddings and save to disk
async fn build_and_save_hnsw_index(
    db: &Arc<Mutex<SymbolDatabase>>,
    model_name: &str,
    workspace_id: &str,
    workspace_root: &Option<PathBuf>,
) -> Result<()> {
    info!("🏗️ Building HNSW index from database embeddings...");
    let hnsw_start = std::time::Instant::now();

    let mut vector_store = crate::embeddings::vector_store::VectorStore::new(384)?;

    // Load all embeddings from database for HNSW building
    let embeddings_result = {
        let db_lock = db.lock().unwrap();
        db_lock.load_all_embeddings(model_name)
    }; // Drop lock before HNSW build

    match embeddings_result {
        Ok(embeddings) => {
            let count = embeddings.len();
            info!("📥 Loaded {} embeddings from database for HNSW", count);

            // Store in VectorStore for HNSW building
            for (symbol_id, vector) in embeddings {
                if let Err(e) = vector_store.store_vector(symbol_id.clone(), vector) {
                    warn!("Failed to store vector {}: {}", symbol_id, e);
                }
            }

            // Build HNSW index
            match vector_store.build_hnsw_index() {
                Ok(_) => {
                    info!(
                        "✅ HNSW index built in {:.2}s",
                        hnsw_start.elapsed().as_secs_f64()
                    );

                    // Save to disk for lazy loading on next startup
                    let vectors_path = if let Some(ref root) = workspace_root {
                        root.join(".julie")
                            .join("indexes")
                            .join(workspace_id)
                            .join("vectors")
                    } else {
                        std::path::PathBuf::from("./.julie/indexes")
                            .join(workspace_id)
                            .join("vectors")
                    };

                    if let Err(e) = vector_store.save_hnsw_index(&vectors_path) {
                        warn!("Failed to save HNSW index to disk: {}", e);
                    } else {
                        info!("💾 HNSW index saved to {}", vectors_path.display());

                        // 🔥 CRITICAL MEMORY FIX: Immediately release memory after save
                        // VectorStore + HNSW graph are no longer needed - data is on disk
                        vector_store.clear();
                        info!("🧹 VectorStore memory released after successful save");
                    }
                }
                Err(e) => {
                    warn!("Failed to build HNSW index: {}", e);
                }
            }
        }
        Err(e) => {
            warn!("Could not load embeddings for HNSW: {}", e);
        }
    }

    Ok(())
}
