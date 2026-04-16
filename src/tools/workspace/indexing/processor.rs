//! File processing helpers for indexing stages.
//! Handles reading, parsing, and extracting symbols from individual files.

use crate::extractors::{PendingRelationship, Relationship, Symbol};
use crate::tools::workspace::commands::ManageWorkspaceTool;
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, trace, warn};

impl ManageWorkspaceTool {
    /// Queue cleanup and file metadata refresh after parser extraction fails.
    ///
    /// This prevents stale symbol/identifier/type rows from surviving when a file
    /// changed but extraction failed for that indexing pass.
    pub(crate) async fn queue_failed_parser_file_for_cleanup(
        &self,
        file_path: &Path,
        language: &str,
        workspace_root: &Path,
        files_to_clean: &mut Vec<String>,
        all_file_infos: &mut Vec<crate::database::FileInfo>,
    ) {
        let relative_path = if file_path.is_absolute() {
            crate::utils::paths::to_relative_unix_style(file_path, workspace_root)
                .unwrap_or_else(|_| file_path.to_string_lossy().replace('\\', "/"))
        } else {
            file_path.to_string_lossy().replace('\\', "/")
        };
        files_to_clean.push(relative_path.clone());

        let file_path_buf = file_path.to_path_buf();
        let language_owned = language.to_string();
        let workspace_root_buf = workspace_root.to_path_buf();
        match tokio::task::spawn_blocking(move || {
            crate::database::create_file_info(&file_path_buf, &language_owned, &workspace_root_buf)
        })
        .await
        {
            Ok(Ok(file_info)) => all_file_infos.push(file_info),
            Ok(Err(e)) => warn!(
                "Failed to refresh file metadata after parser failure for {}: {}",
                relative_path, e
            ),
            Err(e) => warn!(
                "File metadata refresh task panicked for {}: {}",
                relative_path, e
            ),
        }
    }

    /// Process a single file with symbol extraction
    ///
    /// Returns (symbols, relationships, file_info) for bulk storage.
    ///
    /// # Phase 2: Relative Unix-Style Path Storage
    /// Now requires workspace_root for relative path storage in extractors
    pub(crate) async fn process_file_with_parser(
        &self,
        file_path: &Path,
        language: &str,
        workspace_root: &Path, // NEW: Phase 2 - workspace root for relative paths
    ) -> Result<(
        Vec<Symbol>,
        Vec<Relationship>,
        Vec<PendingRelationship>,
        Vec<crate::extractors::Identifier>,
        HashMap<String, crate::extractors::base::TypeInfo>,
        crate::database::FileInfo,
    )> {
        // 🚨 CRITICAL FIX: Wrap ALL blocking filesystem I/O in spawn_blocking to prevent tokio deadlock
        // When processing hundreds of large files (500KB+), blocking I/O in async functions
        // starves the tokio runtime and causes silent hangs (discovered in PsychiatricIntake workspace)
        let file_path_clone = file_path.to_path_buf();
        let language_clone = language.to_string();
        let workspace_root_clone = workspace_root.to_path_buf();

        let (_canonical_file_path, content, mut file_info) =
            tokio::task::spawn_blocking(move || {
                // Blocking operation 1: canonicalize (resolves symlinks: macOS /var -> /private/var)
                let canonical = file_path_clone
                    .canonicalize()
                    .unwrap_or_else(|_| file_path_clone.clone());

                // Blocking operation 2: read file content
                let file_content = std::fs::read_to_string(&canonical)
                    .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical, e))?;

                // Blocking operation 3: create file info (does metadata, hash, etc)
                let info = crate::database::create_file_info(
                    &file_path_clone,
                    &language_clone,
                    &workspace_root_clone,
                )?;

                Ok::<_, anyhow::Error>((canonical, file_content, info))
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn blocking file I/O task: {}", e))??;

        tracing::trace!("✅ spawn_blocking completed for: {:?}", file_path);

        // Skip empty files for symbol extraction
        if content.trim().is_empty() {
            // Return empty symbol list but include file_info (already created in spawn_blocking)
            return Ok((
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                HashMap::new(),
                file_info,
            ));
        }

        // Skip symbol extraction for CSS/HTML (text search only)
        if !self.should_extract_symbols(language) {
            debug!(
                "⏭️  Skipping symbol extraction for {} file (text search only): {}",
                language,
                file_path.display()
            );

            // Return file info without symbols (file_info already created in spawn_blocking)
            return Ok((
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                HashMap::new(),
                file_info,
            ));
        }

        // 🚨 CRITICAL: Skip symbol extraction for very large files (likely data/minified)
        // These files cause exponential CPU usage in tree-sitter traversal (demo-data.js: 158KB = hang)
        // Note: Legitimate Rust files with good docs can be 100-200KB (e.g., candle-core/src/tensor.rs = 112KB)
        const MAX_FILE_SIZE_FOR_SYMBOLS: usize = 500_000; // 500KB limit
        if content.len() > MAX_FILE_SIZE_FOR_SYMBOLS {
            warn!(
                "⏭️  Skipping symbol extraction for large file ({} bytes > {}KB limit): {} - indexing for text search only",
                content.len(),
                MAX_FILE_SIZE_FOR_SYMBOLS / 1024,
                file_path.display()
            );
            return Ok((
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                HashMap::new(),
                file_info,
            ));
        }

        // 🔥 CRITICAL: Convert to relative Unix-style path for storage
        // File paths from discovery might be absolute OR relative - handle both
        let relative_path = if file_path.is_absolute() {
            // Absolute path - convert to relative
            crate::utils::paths::to_relative_unix_style(file_path, workspace_root)?
        } else {
            // Already relative - use as-is (just normalize to Unix-style)
            file_path.to_string_lossy().replace('\\', "/")
        };

        // Parsing and extraction are CPU-heavy. Run canonical extraction on the
        // blocking pool and await completion to avoid detached long-running jobs.
        let relative_path_clone = relative_path.clone();
        let content_clone = content.clone();
        let workspace_root_clone2 = workspace_root.to_path_buf();

        let extract_start = std::time::Instant::now();
        let task = tokio::task::spawn_blocking(move || {
            crate::tools::workspace::ManageWorkspaceTool::extract_symbols_static(
                &relative_path_clone,
                &content_clone,
                &workspace_root_clone2,
            )
        });

        let results = match task.await {
            Ok(result) => result?,
            Err(e) => return Err(anyhow::anyhow!("Spawn blocking task panicked: {}", e)),
        };

        let extract_elapsed = extract_start.elapsed();
        if extract_elapsed.as_millis() > 100 {
            debug!(
                "Slow file processing: {} - extraction: {:?}",
                relative_path, extract_elapsed
            );
        }

        // Destructure ExtractionResults into all fields
        let symbols = results.symbols;

        // Update file_info with actual symbol count (was initialized to 0)
        file_info.symbol_count = symbols.len() as i32;
        let relationships = results.relationships;
        let pending_relationships = results.pending_relationships;
        let identifiers = results.identifiers;
        let types = results.types;

        // Only log if there are many symbols to avoid spam
        if symbols.len() > 10 {
            debug!(
                "📊 Extracted {} symbols from {}",
                symbols.len(),
                relative_path
            );
        }

        // Log pending relationships for cross-file resolution
        if !pending_relationships.is_empty() {
            debug!(
                "📎 Found {} pending relationships in {} (need cross-file resolution)",
                pending_relationships.len(),
                relative_path
            );
        }

        // Return data for bulk operations (SQLite storage)
        Ok((
            symbols,
            relationships,
            pending_relationships,
            identifiers,
            types,
            file_info,
        ))
    }

    /// Process a file without a tree-sitter parser (no symbol extraction)
    ///
    /// Files without parsers are still indexed for full-text search via database.
    pub(crate) async fn process_file_without_parser(
        &self,
        file_path: &Path,
        language: &str,
        workspace_root: &Path, // NEW: Required for relative path conversion
    ) -> Result<(Vec<Symbol>, Vec<Relationship>, crate::database::FileInfo)> {
        tracing::trace!(
            "📂 Processing file without parser: {:?} (language: {})",
            file_path,
            language
        );

        // 🚨 CRITICAL FIX: Wrap ALL blocking filesystem I/O in spawn_blocking to prevent tokio deadlock
        let file_path_clone = file_path.to_path_buf();
        let language_clone = language.to_string();
        let workspace_root_clone = workspace_root.to_path_buf();

        let (_canonical_file_path, content, file_info) = tokio::task::spawn_blocking(move || {
            tracing::trace!(
                "🔄 Inside spawn_blocking (no parser) for: {:?}",
                file_path_clone
            );
            // Blocking operation 1: canonicalize (resolves symlinks: macOS /var -> /private/var)
            let canonical = file_path_clone
                .canonicalize()
                .unwrap_or_else(|_| file_path_clone.clone());

            // Blocking operation 2: read file content
            let file_content = std::fs::read_to_string(&canonical)
                .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical, e))?;

            // Blocking operation 3: create file info (does metadata, hash, etc)
            let info = crate::database::create_file_info(
                &file_path_clone,
                &language_clone,
                &workspace_root_clone,
            )?;

            Ok::<_, anyhow::Error>((canonical, file_content, info))
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to spawn blocking file I/O task: {}", e))??;

        trace!("Read {} bytes from file without parser", content.len());

        // No symbols extracted (no parser available), but file_info created in spawn_blocking above
        Ok((Vec::new(), Vec::new(), file_info))
    }
}
