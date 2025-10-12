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
            generate_embeddings(&symbols_db, output.as_deref(), write_db, &model, batch_size, limit).await?;
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
        Commands::Query { text, model, format } => {
            generate_query_embedding(&text, &model, &format).await?;
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

    let db_arc = std::sync::Arc::new(tokio::sync::Mutex::new(db));
    let mut engine = EmbeddingEngine::new(model, cache_dir, db_arc.clone())?;

    eprintln!("🚀 Model: {} ({}D embeddings)", model, engine.dimensions());
    eprintln!("⚡ Batch size: {}", batch_size);

    // 3. Create VectorStore for collecting embeddings
    let mut vector_store = VectorStore::new(engine.dimensions())?;

    // 4. Process in batches and collect embeddings into VectorStore
    let start_time = Instant::now();
    let mut total_embedded = 0;
    let batch_count = (symbols.len() + batch_size - 1) / batch_size;

    for (i, batch) in symbols.chunks(batch_size).enumerate() {
        let batch_start = Instant::now();

        // Generate embeddings for batch
        let embeddings = engine.embed_symbols_batch(batch)?;
        total_embedded += embeddings.len();

        // Store embeddings in VectorStore
        for (symbol_id, vector) in &embeddings {
            vector_store.store_vector(symbol_id.clone(), vector.clone())?;
        }

        let batch_time = batch_start.elapsed();

        eprintln!(
            "⚡ Batch {}/{}: {} embeddings in {:.2}ms ({:.0} emb/sec)",
            i + 1,
            batch_count,
            embeddings.len(),
            batch_time.as_secs_f64() * 1000.0,
            embeddings.len() as f64 / batch_time.as_secs_f64()
        );

        // Sample output for first batch (show embedding dimensions)
        if i == 0 && !embeddings.is_empty() {
            let (sample_id, sample_vec) = &embeddings[0];
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
        let mut db = db_arc.lock().await;

        // Convert VectorStore's HashMap to Vec for bulk_store_embeddings
        // We need to access the internal vectors HashMap - add a getter method
        let all_embeddings: Vec<(String, Vec<f32>)> = vector_store
            .get_all_vectors()
            .into_iter()
            .collect();

        // Store in database using the existing bulk method
        db.bulk_store_embeddings(&all_embeddings, engine.dimensions(), model)?;

        let db_write_time = db_write_start.elapsed();
        eprintln!(
            "✅ {} embeddings written to database in {:.2}s ({:.0} embeddings/sec)",
            all_embeddings.len(),
            db_write_time.as_secs_f64(),
            all_embeddings.len() as f64 / db_write_time.as_secs_f64()
        );
    }

    // 6. Build and save HNSW index if output directory specified
    if let Some(output_path) = output_dir {
        eprintln!("\n🏗️  Building HNSW index...");
        let hnsw_start = Instant::now();

        vector_store.build_hnsw_index()?;

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
    let absolute_path_str = absolute_path.to_str()
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

    let db_arc = std::sync::Arc::new(tokio::sync::Mutex::new(db));
    let mut engine = EmbeddingEngine::new(model, cache_dir, db_arc.clone())?;

    eprintln!("🚀 Model: {} ({}D embeddings)", model, engine.dimensions());

    // 3. Load existing HNSW index
    let index_path = std::path::Path::new(output_dir);
    if !index_path.exists() {
        anyhow::bail!("HNSW index directory does not exist: {}", output_dir);
    }

    let mut vector_store = VectorStore::new(engine.dimensions())?;

    eprintln!("📂 Loading existing HNSW index from {}...", output_dir);
    let load_start = Instant::now();
    vector_store.load_hnsw_index(index_path)?;
    eprintln!("✅ Index loaded in {:.2}ms", load_start.elapsed().as_secs_f64() * 1000.0);

    // 4. Remove old vectors for this file's symbols
    eprintln!("🗑️  Removing old vectors for {} symbols...", symbols.len());
    for symbol in &symbols {
        vector_store.remove_vector(&symbol.id)?;
    }

    // 5. Generate new embeddings
    eprintln!("⚡ Generating new embeddings...");
    let embed_start = Instant::now();
    let embeddings = engine.embed_symbols_batch(&symbols)?;
    let embed_time = embed_start.elapsed();

    eprintln!(
        "✅ Generated {} embeddings in {:.2}ms ({:.0} emb/sec)",
        embeddings.len(),
        embed_time.as_secs_f64() * 1000.0,
        embeddings.len() as f64 / embed_time.as_secs_f64()
    );

    // 6. Add new vectors to index
    for (symbol_id, vector) in &embeddings {
        vector_store.store_vector(symbol_id.clone(), vector.clone())?;
    }

    // 7. Write embeddings to SQLite if requested (for cross-language access)
    if write_db {
        eprintln!("\n💾 Writing embeddings to SQLite database...");
        let db_write_start = Instant::now();

        // Get database connection (Arc is cloned, so engine still has its reference)
        let mut db = db_arc.lock().await;

        // Store only the new embeddings for this file
        db.bulk_store_embeddings(&embeddings, engine.dimensions(), model)?;

        let db_write_time = db_write_start.elapsed();
        eprintln!(
            "✅ {} embeddings written to database in {:.2}s ({:.0} embeddings/sec)",
            embeddings.len(),
            db_write_time.as_secs_f64(),
            embeddings.len() as f64 / db_write_time.as_secs_f64()
        );
    }

    // 8. Rebuild HNSW index with updated vectors
    eprintln!("🏗️  Rebuilding HNSW index...");
    let rebuild_start = Instant::now();
    vector_store.build_hnsw_index()?;
    eprintln!("✅ Index rebuilt in {:.2}s", rebuild_start.elapsed().as_secs_f64());

    // 8. Save updated index
    eprintln!("💾 Saving updated index...");
    let save_start = Instant::now();
    vector_store.save_hnsw_index(index_path)?;
    eprintln!("✅ Index saved in {:.2}s", save_start.elapsed().as_secs_f64());

    // Output JSON statistics
    let avg_time_ms = (embed_time.as_secs_f64() * 1000.0) / symbols.len() as f64;
    let stats = EmbeddingStats {
        success: true,
        symbols_processed: symbols.len(),
        embeddings_generated: embeddings.len(),
        model: model.to_string(),
        dimensions: engine.dimensions(),
        avg_embedding_time_ms: avg_time_ms,
    };

    println!("{}", serde_json::to_string_pretty(&stats)?);

    Ok(())
}

/// Generate embedding for a search query (for query-time semantic search)
async fn generate_query_embedding(text: &str, model: &str, format: &str) -> Result<()> {
    // Initialize embedding engine without requiring database
    let cache_dir = std::env::temp_dir().join("julie-embeddings");
    std::fs::create_dir_all(&cache_dir)?;

    // Create a temporary dummy database (required by EmbeddingEngine API)
    // Note: This is a design limitation - the engine requires a DB but query doesn't need one
    let temp_dir = std::env::temp_dir().join("julie-query-temp");
    std::fs::create_dir_all(&temp_dir)?;
    let dummy_db_path = temp_dir.join(format!("query_dummy_{}.db", std::process::id()));
    let dummy_db = SymbolDatabase::new(dummy_db_path.to_str().unwrap())?;
    let db_arc = std::sync::Arc::new(tokio::sync::Mutex::new(dummy_db));

    // Initialize embedding engine
    let mut engine = EmbeddingEngine::new(model, cache_dir, db_arc)?;

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
            anyhow::bail!("Unknown format '{}'. Supported formats: json, binary", format);
        }
    }

    Ok(())
}
