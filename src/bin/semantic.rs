/// julie-semantic: Semantic code intelligence CLI (Simplified)
///
/// For now, this CLI demonstrates the embedding generation capability.
/// Full persistence and search will be handled by CodeSearch integration.
///
/// Commands:
/// - embed: Generate embeddings for symbols and output statistics
use anyhow::Result;
use clap::{Parser, Subcommand};
use julie::database::SymbolDatabase;
use julie::embeddings::EmbeddingEngine;
use julie::embeddings::vector_store::VectorStore;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "julie-semantic")]
#[command(about = "Semantic code intelligence with FastEmbed", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate embeddings for symbols database and build HNSW index
    Embed {
        /// SQLite symbols database path (from julie-codesearch)
        #[arg(long)]
        symbols_db: String,

        /// Output directory for HNSW index (e.g., .coa/codesearch/indexes/{workspace}/vectors)
        #[arg(long)]
        output: Option<String>,

        /// Write embeddings back to SQLite database (symbols_db) for cross-language access
        #[arg(long)]
        write_db: bool,

        /// Embedding model name
        #[arg(long, default_value = "bge-small")]
        model: String,

        /// Batch size for embedding generation
        #[arg(long, default_value_t = 100)]
        batch_size: usize,

        /// Maximum symbols to process (for testing)
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Update embeddings for a changed file (incremental)
    Update {
        /// File path that changed
        #[arg(long)]
        file: String,

        /// SQLite symbols database path
        #[arg(long)]
        symbols_db: String,

        /// HNSW index directory (must exist from previous embed)
        #[arg(long)]
        output: String,

        /// Write embeddings back to SQLite database (symbols_db) for cross-language access
        #[arg(long)]
        write_db: bool,

        /// Embedding model name (must match original)
        #[arg(long, default_value = "bge-small")]
        model: String,
    },

    /// Generate embedding for a search query
    Query {
        /// The search query text
        #[arg(long)]
        text: String,

        /// Embedding model name (must match indexed model)
        #[arg(long, default_value = "bge-small")]
        model: String,

        /// Output format: json or binary
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Search for similar symbols using HNSW index
    Search {
        /// The search query text
        #[arg(long)]
        text: String,

        /// Path to HNSW index directory (contains hnsw_index.hnsw.*)
        #[arg(long)]
        index: String,

        /// SQLite symbols database path (to load vector data)
        #[arg(long)]
        symbols_db: String,

        /// Embedding model name (must match indexed model)
        #[arg(long, default_value = "bge-small")]
        model: String,

        /// Number of results to return
        #[arg(long, default_value_t = 10)]
        limit: usize,

        /// Minimum similarity threshold (0.0 to 1.0)
        #[arg(long, default_value_t = 0.0)]
        threshold: f32,
    },
}

/// Embedding statistics for JSON output
#[derive(Debug, Serialize, Deserialize)]
struct EmbeddingStats {
    success: bool,
    symbols_processed: usize,
    embeddings_generated: usize,
    model: String,
    dimensions: usize,
    avg_embedding_time_ms: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Embed {
            symbols_db,
            output,
            write_db,
            model,
            batch_size,
            limit,
        } => {
            generate_embeddings(
                &symbols_db,
                output.as_deref(),
                write_db,
                &model,
                batch_size,
                limit,
            )
            .await?;
        }
        Commands::Update {
            file,
            symbols_db,
            output,
            write_db,
            model,
        } => {
            update_file_embeddings(&file, &symbols_db, &output, write_db, &model).await?;
        }
        Commands::Query {
            text,
            model,
            format,
        } => {
            generate_query_embedding(&text, &model, &format).await?;
        }
        Commands::Search {
            text,
            index,
            symbols_db,
            model,
            limit,
            threshold,
        } => {
            search_hnsw(&text, &index, &symbols_db, &model, limit, threshold).await?;
        }
    }

    Ok(())
}

/// Generate embeddings for symbols and optionally build HNSW index
async fn generate_embeddings(
    db_path: &str,
    output_dir: Option<&str>,
    write_db: bool,
    model: &str,
    batch_size: usize,
    limit: Option<usize>,
) -> Result<()> {
    use std::time::Instant;

    eprintln!("🧠 Loading symbols from {}...", db_path);

    // 1. Load symbols from SQLite
    let db = SymbolDatabase::new(db_path)?;
    let mut symbols = db.get_all_symbols()?;

    // Apply limit if specified
    if let Some(limit_count) = limit {
        symbols.truncate(limit_count);
    }

    eprintln!("📊 Processing {} symbols", symbols.len());

    if symbols.is_empty() {
        eprintln!("⚠️  No symbols found in database");
        println!(
            "{}",
            serde_json::to_string_pretty(&EmbeddingStats {
                success: false,
                symbols_processed: 0,
                embeddings_generated: 0,
                model: model.to_string(),
                dimensions: 0,
                avg_embedding_time_ms: 0.0,
            })?
        );
        return Ok(());
    }

    // 2. Initialize embedding engine
    let cache_dir = std::env::temp_dir().join("julie-embeddings");
    std::fs::create_dir_all(&cache_dir)?;

    let db_arc = std::sync::Arc::new(std::sync::Mutex::new(db));
    let mut engine = EmbeddingEngine::new(model, cache_dir, db_arc.clone()).await?;

    eprintln!("🚀 Model: {} ({}D embeddings)", model, engine.dimensions());
    eprintln!("⚡ Batch size: {}", batch_size);

    // 3. Process in batches and collect embeddings into HashMap (NEW ARCHITECTURE)
    let start_time = Instant::now();
    let mut total_embedded = 0;
    let batch_count = symbols.len().div_ceil(batch_size);
    let mut all_embeddings: std::collections::HashMap<String, Vec<f32>> =
        std::collections::HashMap::new();

    for (i, batch) in symbols.chunks(batch_size).enumerate() {
        let batch_start = Instant::now();

        // Generate embeddings for batch
        let embeddings = engine.embed_symbols_batch(batch)?;
        total_embedded += embeddings.len();

        // 🔧 REFACTOR: Collect embeddings into HashMap instead of VectorStore
        for (symbol_id, vector) in embeddings {
            all_embeddings.insert(symbol_id, vector);
        }

        let batch_time = batch_start.elapsed();

        eprintln!(
            "⚡ Batch {}/{}: {} embeddings in {:.2}ms ({:.0} emb/sec)",
            i + 1,
            batch_count,
            total_embedded,
            batch_time.as_secs_f64() * 1000.0,
            total_embedded as f64 / batch_time.as_secs_f64()
        );

        // Sample output for first batch (show embedding dimensions)
        if i == 0 && !all_embeddings.is_empty() {
            let (sample_id, sample_vec) = all_embeddings.iter().next().unwrap();
            eprintln!(
                "   📝 Sample: symbol_id={}, vector_len={}",
                sample_id,
                sample_vec.len()
            );
        }
    }

    let total_time = start_time.elapsed();
    let avg_time_ms = (total_time.as_secs_f64() * 1000.0) / symbols.len() as f64;

    eprintln!("✅ Embedding generation complete!");
    eprintln!("   Total time: {:.2}s", total_time.as_secs_f64());
    eprintln!(
        "   Rate: {:.0} embeddings/sec",
        total_embedded as f64 / total_time.as_secs_f64()
    );

    // 5. Write embeddings to SQLite if requested (for cross-language access)
    if write_db {
        eprintln!("\n💾 Writing embeddings to SQLite database...");
        let db_write_start = Instant::now();

        // Get database connection back from Arc<Mutex<>>
        let mut db = match db_arc.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Warning: Database mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };

        // 🔧 REFACTOR: Use all_embeddings HashMap directly instead of VectorStore
        let embeddings_vec: Vec<(String, Vec<f32>)> = all_embeddings
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Store in database using the existing bulk method
        db.bulk_store_embeddings(&embeddings_vec, engine.dimensions(), model)?;

        let db_write_time = db_write_start.elapsed();
        eprintln!(
            "✅ {} embeddings written to database in {:.2}s ({:.0} embeddings/sec)",
            embeddings_vec.len(),
            db_write_time.as_secs_f64(),
            embeddings_vec.len() as f64 / db_write_time.as_secs_f64()
        );
    }

    // 6. Build and save HNSW index if output directory specified
    if let Some(output_path) = output_dir {
        eprintln!("\n🏗️  Building HNSW index...");
        let hnsw_start = Instant::now();

        // 🔧 REFACTOR: Create VectorStore as pure HNSW Index Manager
        let mut vector_store = VectorStore::new(engine.dimensions())?;

        // Build HNSW index from collected embeddings HashMap
        vector_store.build_hnsw_index(&all_embeddings)?;

        let hnsw_time = hnsw_start.elapsed();
        eprintln!("✅ HNSW index built in {:.2}s", hnsw_time.as_secs_f64());

        // Create output directory if it doesn't exist
        std::fs::create_dir_all(output_path)?;
        let index_path = std::path::Path::new(output_path);

        eprintln!("💾 Saving HNSW index to {}...", output_path);
        let save_start = Instant::now();

        vector_store.save_hnsw_index(index_path)?;

        let save_time = save_start.elapsed();
        eprintln!("✅ HNSW index saved in {:.2}s", save_time.as_secs_f64());
    }

    // Output JSON statistics
    let stats = EmbeddingStats {
        success: true,
        symbols_processed: symbols.len(),
        embeddings_generated: total_embedded,
        model: model.to_string(),
        dimensions: engine.dimensions(),
        avg_embedding_time_ms: avg_time_ms,
    };

    println!("{}", serde_json::to_string_pretty(&stats)?);

    Ok(())
}

/// Update embeddings for a single changed file (incremental)
async fn update_file_embeddings(
    file_path: &str,
    db_path: &str,
    output_dir: &str,
    write_db: bool,
    model: &str,
) -> Result<()> {
    use std::time::Instant;

    // Resolve file path to absolute (database stores absolute paths)
    let absolute_path = std::fs::canonicalize(file_path)?;
    let absolute_path_str = absolute_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 in file path"))?;

    eprintln!("🔄 Updating embeddings for {}...", absolute_path_str);

    // 1. Load symbols for the changed file from SQLite
    let db = SymbolDatabase::new(db_path)?;
    let symbols = db.get_symbols_for_file(absolute_path_str)?;

    if symbols.is_empty() {
        eprintln!("⚠️  No symbols found for file (file may have been deleted or has no symbols)");
        println!(
            "{}",
            serde_json::to_string_pretty(&EmbeddingStats {
                success: true,
                symbols_processed: 0,
                embeddings_generated: 0,
                model: model.to_string(),
                dimensions: 0,
                avg_embedding_time_ms: 0.0,
            })?
        );
        return Ok(());
    }

    eprintln!("📊 Processing {} symbols from file", symbols.len());

    // 2. Initialize embedding engine
    let cache_dir = std::env::temp_dir().join("julie-embeddings");
    std::fs::create_dir_all(&cache_dir)?;

    let db_arc = std::sync::Arc::new(std::sync::Mutex::new(db));
    let mut engine = EmbeddingEngine::new(model, cache_dir, db_arc.clone()).await?;

    eprintln!("🚀 Model: {} ({}D embeddings)", model, engine.dimensions());

    // 3. Verify HNSW index directory exists
    let index_path = std::path::Path::new(output_dir);
    if !index_path.exists() {
        anyhow::bail!("HNSW index directory does not exist: {}", output_dir);
    }

    // 4. Generate new embeddings for changed file
    eprintln!("⚡ Generating new embeddings...");
    let embed_start = Instant::now();
    let new_embeddings = engine.embed_symbols_batch(&symbols)?;
    let embed_time = embed_start.elapsed();

    eprintln!(
        "✅ Generated {} embeddings in {:.2}ms ({:.0} emb/sec)",
        new_embeddings.len(),
        embed_time.as_secs_f64() * 1000.0,
        new_embeddings.len() as f64 / embed_time.as_secs_f64()
    );

    // 5. Write embeddings to SQLite if requested (for cross-language access)
    if write_db {
        eprintln!("\n💾 Writing embeddings to SQLite database...");
        let db_write_start = Instant::now();

        // Get database connection (Arc is cloned, so engine still has its reference)
        let mut db = match db_arc.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                eprintln!("Warning: Database mutex poisoned, recovering");
                poisoned.into_inner()
            }
        };

        // Store only the new embeddings for this file
        db.bulk_store_embeddings(&new_embeddings, engine.dimensions(), model)?;

        let db_write_time = db_write_start.elapsed();
        eprintln!(
            "✅ {} embeddings written to database in {:.2}s ({:.0} embeddings/sec)",
            new_embeddings.len(),
            db_write_time.as_secs_f64(),
            new_embeddings.len() as f64 / db_write_time.as_secs_f64()
        );
    }

    // 6. 🔧 REFACTOR: Load ALL embeddings from SQLite for HNSW rebuild
    eprintln!("📖 Loading all embeddings from database for HNSW rebuild...");
    let load_all_start = Instant::now();
    let db = match db_arc.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("Warning: Database mutex poisoned, recovering");
            poisoned.into_inner()
        }
    };
    let all_embeddings = db.load_all_embeddings(model)?;
    drop(db); // Release lock
    eprintln!(
        "✅ Loaded {} embeddings in {:.2}ms",
        all_embeddings.len(),
        load_all_start.elapsed().as_secs_f64() * 1000.0
    );

    // 7. Rebuild HNSW index with ALL embeddings (new architecture)
    eprintln!("🏗️  Rebuilding HNSW index...");
    let rebuild_start = Instant::now();
    let mut vector_store = VectorStore::new(engine.dimensions())?;
    vector_store.build_hnsw_index(&all_embeddings)?;
    eprintln!(
        "✅ Index rebuilt in {:.2}s",
        rebuild_start.elapsed().as_secs_f64()
    );

    // 8. Save updated index
    eprintln!("💾 Saving updated index...");
    let save_start = Instant::now();
    vector_store.save_hnsw_index(index_path)?;
    eprintln!(
        "✅ Index saved in {:.2}s",
        save_start.elapsed().as_secs_f64()
    );

    // Output JSON statistics
    let avg_time_ms = (embed_time.as_secs_f64() * 1000.0) / symbols.len() as f64;
    let stats = EmbeddingStats {
        success: true,
        symbols_processed: symbols.len(),
        embeddings_generated: new_embeddings.len(),
        model: model.to_string(),
        dimensions: engine.dimensions(),
        avg_embedding_time_ms: avg_time_ms,
    };

    println!("{}", serde_json::to_string_pretty(&stats)?);

    Ok(())
}

/// Generate embedding for a search query (for query-time semantic search)
async fn generate_query_embedding(text: &str, model: &str, format: &str) -> Result<()> {
    // Initialize embedding engine in standalone mode (no database required)
    let cache_dir = std::env::temp_dir().join("julie-embeddings");
    std::fs::create_dir_all(&cache_dir)?;

    // Use standalone engine - no dummy database needed!
    let mut engine = EmbeddingEngine::new_standalone(model, cache_dir).await?;

    // Generate embedding for query text
    let embedding = engine.embed_text(text)?;

    // Output based on format
    match format {
        "json" => {
            // Output as JSON array (default, for C# consumption)
            println!("{}", serde_json::to_string(&embedding)?);
        }
        "binary" => {
            // Output as binary (for future optimization)
            use std::io::Write;
            for value in &embedding {
                std::io::stdout().write_all(&value.to_le_bytes())?;
            }
        }
        _ => {
            anyhow::bail!(
                "Unknown format '{}'. Supported formats: json, binary",
                format
            );
        }
    }

    Ok(())
}

/// Search for similar symbols using HNSW index
async fn search_hnsw(
    text: &str,
    index_path: &str,
    db_path: &str,
    model: &str,
    limit: usize,
    threshold: f32,
) -> Result<()> {
    use std::time::Instant;

    // 1. Generate query embedding
    eprintln!("🧠 Generating embedding for query: '{}'", text);
    let cache_dir = std::env::temp_dir().join("julie-embeddings");
    std::fs::create_dir_all(&cache_dir)?;

    // Use standalone engine - no dummy database needed!
    let mut engine = EmbeddingEngine::new_standalone(model, cache_dir).await?;

    let embed_start = Instant::now();
    let query_vector = engine.embed_text(text)?;
    eprintln!(
        "✅ Query embedding generated in {:.2}ms ({}D)",
        embed_start.elapsed().as_secs_f64() * 1000.0,
        query_vector.len()
    );

    // 2. Open database connection (will be used for on-demand vector fetching)
    eprintln!("📖 Opening database: {}...", db_path);
    let db = SymbolDatabase::new(db_path)?;

    // 3. 🔧 REFACTOR: Load HNSW index structure only (new architecture)
    let index_path_obj = std::path::Path::new(index_path);
    eprintln!("📂 Loading HNSW index from {}...", index_path);
    let load_start = Instant::now();

    let mut vector_store = VectorStore::new(engine.dimensions())?;
    match vector_store.load_hnsw_index(index_path_obj) {
        Ok(_) => {
            eprintln!(
                "✅ HNSW index loaded in {:.2}ms",
                load_start.elapsed().as_secs_f64() * 1000.0
            );
        }
        Err(e) => {
            anyhow::bail!("Failed to load HNSW index: {}", e);
        }
    };

    // 4. Search using HNSW with on-demand SQLite vector fetching
    eprintln!(
        "🔍 Searching for top {} results (threshold: {})...",
        limit, threshold
    );
    let search_start = Instant::now();
    let results = vector_store.search_similar_hnsw(&db, &query_vector, limit, threshold, model)?;
    let search_time = search_start.elapsed();

    eprintln!(
        "✅ Found {} results in {:.2}ms (HNSW + SQLite on-demand)",
        results.len(),
        search_time.as_secs_f64() * 1000.0
    );

    // 5. Fetch complete symbol data for results
    let symbol_ids: Vec<String> = results.iter().map(|r| r.symbol_id.clone()).collect();
    eprintln!(
        "📥 Fetching {} symbol records from database...",
        symbol_ids.len()
    );
    let fetch_start = Instant::now();
    let symbols = db.get_symbols_by_ids(&symbol_ids)?;
    eprintln!(
        "✅ Fetched symbol data in {:.2}ms",
        fetch_start.elapsed().as_secs_f64() * 1000.0
    );

    // 6. Create lookup map for similarity scores
    let similarity_map: std::collections::HashMap<_, _> = results
        .iter()
        .map(|r| (r.symbol_id.clone(), r.similarity_score))
        .collect();

    // 7. Output complete symbol data with similarity scores as JSON
    let output: Vec<_> = symbols
        .iter()
        .map(|symbol| {
            let similarity = similarity_map.get(&symbol.id).copied().unwrap_or(0.0);
            serde_json::json!({
                "symbol_id": symbol.id,
                "similarity": similarity,
                "name": symbol.name,
                "kind": symbol.kind.to_string(),
                "language": symbol.language,
                "file_path": symbol.file_path,
                "start_line": symbol.start_line,
                "start_col": symbol.start_column,
                "end_line": symbol.end_line,
                "end_col": symbol.end_column,
                "signature": symbol.signature,
                "doc_comment": symbol.doc_comment,
                "visibility": symbol.visibility.as_ref().map(|v| v.to_string()),
                "parent_id": symbol.parent_id,
            })
        })
        .collect();

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}
