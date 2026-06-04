use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use futures::stream::{self, StreamExt};
use julie_extractors::base::{ParseDiagnostic, StructuredPendingRelationship};
use tracing::{debug, info, trace, warn};

use julie_extractors::{
    ExtractionResults, Identifier, Literal, PendingRelationship, Relationship, Symbol,
};
use crate::indexing_core::batch::ExtractedBatch;
use crate::indexing_core::paths::relative_path_for_storage;
use julie_core::file_policy::{
    ExtractionMode, detect_language_for_indexing_with_content, determine_extraction_mode,
};

pub enum ExtractedFileDisposition {
    Parsed,
    TextOnly,
    RepairNeeded { detail: String },
}

pub struct ExtractedFileRecord {
    pub relative_path: String,
    pub language: String,
    pub disposition: ExtractedFileDisposition,
}

type ParserFileProcessResult = (
    Vec<Symbol>,
    Vec<Relationship>,
    Vec<PendingRelationship>,
    Vec<StructuredPendingRelationship>,
    Vec<Identifier>,
    HashMap<String, julie_extractors::base::TypeInfo>,
    Vec<julie_extractors::base::TypeArgumentUsage>,
    Vec<Literal>,
    Vec<ParseDiagnostic>,
    julie_core::database::FileInfo,
);

type TextFileProcessResult = (Vec<Symbol>, Vec<Relationship>, julie_core::database::FileInfo);

enum ExtractOutcome {
    WithParser(Result<ParserFileProcessResult>),
    WithoutParser(Result<TextFileProcessResult>),
}

pub async fn extract_files_for_indexing(
    files_by_language: HashMap<String, Vec<PathBuf>>,
    workspace_root: &Path,
) -> Result<ExtractedBatch> {
    extract_files_for_indexing_with_records(files_by_language, workspace_root)
        .await
        .map(|(batch, _records)| batch)
}

pub async fn extract_files_for_indexing_with_records(
    files_by_language: HashMap<String, Vec<PathBuf>>,
    workspace_root: &Path,
) -> Result<(ExtractedBatch, Vec<ExtractedFileRecord>)> {
    let concurrency = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);

    let mut per_language_counts: HashMap<String, (usize, bool)> = HashMap::new();
    let work: Vec<(String, PathBuf, bool)> = files_by_language
        .into_iter()
        .filter(|(_, paths)| !paths.is_empty())
        .flat_map(|(language, file_paths)| {
            let has_parser = julie_extractors::language::get_tree_sitter_language(&language).is_ok();
            per_language_counts
                .entry(language.clone())
                .or_insert((file_paths.len(), has_parser));
            file_paths
                .into_iter()
                .map(move |path| (language.clone(), path, has_parser))
        })
        .collect();

    if work.is_empty() {
        return Ok((ExtractedBatch::new(), Vec::new()));
    }

    info!(
        "🚀 Extracting {} files in parallel (concurrency={}, languages={})",
        work.len(),
        concurrency,
        per_language_counts.len()
    );
    for (language, (count, has_parser)) in &per_language_counts {
        debug!(
            "Extraction plan: {} {} files ({})",
            count,
            language,
            if *has_parser {
                "tree-sitter parser"
            } else {
                "text-only"
            }
        );
    }

    let extract_start = std::time::Instant::now();
    let outcomes: Vec<(String, PathBuf, ExtractOutcome)> = stream::iter(work)
        .map(|(language, file_path, has_parser)| async move {
            let outcome = if has_parser {
                ExtractOutcome::WithParser(
                    process_file_with_parser(&file_path, &language, workspace_root).await,
                )
            } else {
                ExtractOutcome::WithoutParser(
                    process_file_without_parser(&file_path, &language, workspace_root).await,
                )
            };
            (language, file_path, outcome)
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    info!(
        "⏱️  parallel extraction complete: {:.2}s ({} files)",
        extract_start.elapsed().as_secs_f64(),
        outcomes.len()
    );

    let mut batch = ExtractedBatch::new();
    let mut records = Vec::new();
    for (language, file_path, outcome) in outcomes {
        let relative_path = relative_path_for_storage(&file_path, workspace_root);
        match outcome {
            ExtractOutcome::WithParser(Ok((
                symbols,
                relationships,
                pending_rels,
                structured_pending_rels,
                identifiers,
                types,
                type_argument_usages,
                literals,
                parse_diagnostics,
                file_info,
            ))) => {
                records.push(ExtractedFileRecord {
                    relative_path: relative_path.clone(),
                    language: file_info.language.clone(),
                    disposition: ExtractedFileDisposition::Parsed,
                });
                batch.files_processed += 1;
                trace!(
                    "File {} extracted {} symbols, {} pending relationships",
                    file_path.display(),
                    symbols.len(),
                    pending_rels.len()
                );
                batch.files_to_clean.push(relative_path.clone());
                batch.all_symbols.extend(symbols);
                batch.all_relationships.extend(relationships);
                batch.all_pending_relationships.extend(pending_rels);
                batch
                    .all_structured_pending_relationships
                    .extend(structured_pending_rels);
                batch.all_identifiers.extend(identifiers);
                batch.all_types.extend(types.into_values());
                batch.all_type_argument_rows.extend(
                    julie_core::database::bulk::type_arguments::flatten_type_argument_usages(
                        &type_argument_usages,
                    ),
                );
                batch.all_literals.extend(literals);
                batch
                    .parse_diagnostics_by_file
                    .push((relative_path, parse_diagnostics));
                batch.all_file_infos.push(file_info);
                if batch.files_processed.is_multiple_of(50) {
                    debug!(
                        "Progress: {} files processed, {} symbols collected",
                        batch.files_processed,
                        batch.all_symbols.len()
                    );
                }
            }
            ExtractOutcome::WithParser(Err(error)) => {
                warn!("Failed to process file {:?}: {}", file_path, error);
                queue_failed_parser_file_for_cleanup(
                    &file_path,
                    &language,
                    workspace_root,
                    &mut batch.files_to_clean,
                    &mut batch.all_file_infos,
                )
                .await;
                let detail = error.to_string();
                records.push(ExtractedFileRecord {
                    relative_path: relative_path.clone(),
                    language,
                    disposition: ExtractedFileDisposition::RepairNeeded {
                        detail: detail.clone(),
                    },
                });
                batch.repair_entries.push((relative_path, detail));
            }
            ExtractOutcome::WithoutParser(Ok((symbols, relationships, file_info))) => {
                debug!("📄 Processed file without parser: {:?}", file_path);
                records.push(ExtractedFileRecord {
                    relative_path: relative_path.clone(),
                    language,
                    disposition: ExtractedFileDisposition::TextOnly,
                });
                batch.files_processed += 1;
                batch.files_to_clean.push(relative_path);
                batch.all_symbols.extend(symbols);
                batch.all_relationships.extend(relationships);
                batch.all_file_infos.push(file_info);
            }
            ExtractOutcome::WithoutParser(Err(error)) => {
                warn!(
                    "Failed to process file without parser {:?}: {}",
                    file_path, error
                );
                queue_failed_parser_file_for_cleanup(
                    &file_path,
                    &language,
                    workspace_root,
                    &mut batch.files_to_clean,
                    &mut batch.all_file_infos,
                )
                .await;
                let detail = error.to_string();
                records.push(ExtractedFileRecord {
                    relative_path: relative_path.clone(),
                    language,
                    disposition: ExtractedFileDisposition::RepairNeeded {
                        detail: detail.clone(),
                    },
                });
                batch.repair_entries.push((relative_path, detail));
            }
        }
    }

    // Carrier classification (Miller bridge Phase 3) + test-role classification
    // (test-role enrichment Phase 1). This is the shared chokepoint for BOTH the
    // live indexing pipeline and the external-extract CLI (`operations.rs`) —
    // both route through this function — so Miller's extract DB gets gated
    // literals AND `test_role`/container `is_test` exactly like the live daemon.
    // (The single-file watcher path runs the same gates separately.) The language
    // configs are loaded once and reused for both gates; skip the load entirely
    // when nothing was captured.
    if !batch.all_literals.is_empty() || !batch.all_symbols.is_empty() {
        let configs = julie_index::search::LanguageConfigs::load_embedded();
        // Non-carrier literals are dropped here; only recognized url/sql/route
        // literals survive into the batch.
        if !batch.all_literals.is_empty() {
            let carrier_configs = configs.build_literal_carrier_configs();
            julie_index::analysis::literals::classify_literals_by_carrier(
                &mut batch.all_literals,
                &carrier_configs,
            );
        }
        // Annotation/convention-driven; sets metadata.test_role and is_test —
        // including class/struct containers ([TestClass], @Nested) that the
        // callable-only per-extractor is_test can never flag.
        if !batch.all_symbols.is_empty() {
            let role_configs = configs.build_test_role_configs();
            julie_index::analysis::test_roles::classify_symbols_by_role(
                &mut batch.all_symbols,
                &role_configs,
            );
        }
    }

    Ok((batch, records))
}

pub async fn queue_failed_parser_file_for_cleanup(
    file_path: &Path,
    language: &str,
    workspace_root: &Path,
    files_to_clean: &mut Vec<String>,
    all_file_infos: &mut Vec<julie_core::database::FileInfo>,
) {
    let relative_path = relative_path_for_storage(file_path, workspace_root);
    files_to_clean.push(relative_path.clone());

    let file_path_buf = file_path.to_path_buf();
    let language_owned = language.to_string();
    let workspace_root_buf = workspace_root.to_path_buf();
    match tokio::task::spawn_blocking(move || {
        julie_core::database::create_file_info(&file_path_buf, &language_owned, &workspace_root_buf)
    })
    .await
    {
        Ok(Ok(file_info)) => all_file_infos.push(file_info),
        Ok(Err(error)) => warn!(
            "Failed to refresh file metadata after parser failure for {}: {}",
            relative_path, error
        ),
        Err(error) => warn!(
            "File metadata refresh task panicked for {}: {}",
            relative_path, error
        ),
    }
}

pub async fn process_file_with_parser(
    file_path: &Path,
    language: &str,
    workspace_root: &Path,
) -> Result<ParserFileProcessResult> {
    process_file_with_parser_using(
        file_path,
        language,
        workspace_root,
        |relative_path, content, workspace_root_path| {
            julie_extractors::extract_canonical(&relative_path, &content, &workspace_root_path)
        },
    )
    .await
}

pub async fn process_file_with_parser_for_test<F>(
    file_path: &Path,
    language: &str,
    workspace_root: &Path,
    extract: F,
) -> Result<ParserFileProcessResult>
where
    F: FnOnce(String, String, PathBuf) -> Result<ExtractionResults> + Send + 'static,
{
    process_file_with_parser_using(file_path, language, workspace_root, extract).await
}

async fn process_file_with_parser_using<F>(
    file_path: &Path,
    _language: &str,
    workspace_root: &Path,
    extract: F,
) -> Result<ParserFileProcessResult>
where
    F: FnOnce(String, String, PathBuf) -> Result<ExtractionResults> + Send + 'static,
{
    let file_path_clone = file_path.to_path_buf();
    let workspace_root_clone = workspace_root.to_path_buf();

    let (_canonical_file_path, content, mut file_info) = tokio::task::spawn_blocking(move || {
        let canonical = file_path_clone
            .canonicalize()
            .unwrap_or_else(|_| file_path_clone.clone());
        let file_content = std::fs::read_to_string(&canonical)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical, e))?;
        let detected_language =
            detect_language_for_indexing_with_content(&file_path_clone, &file_content);
        let info = julie_core::database::create_file_info(
            &file_path_clone,
            &detected_language,
            &workspace_root_clone,
        )?;
        Ok::<_, anyhow::Error>((canonical, file_content, info))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to spawn blocking file I/O task: {}", e))??;

    tracing::trace!("✅ spawn_blocking completed for: {:?}", file_path);

    let language = file_info.language.as_str();
    if determine_extraction_mode(language, &content) == ExtractionMode::TextOnly {
        debug!(
            "⏭️  Switching to text-only indexing for {} ({})",
            file_path.display(),
            language
        );
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            HashMap::new(),
            Vec::new(), // type_argument_usages
            Vec::new(), // literals
            Vec::new(),
            file_info,
        ));
    }

    let relative_path = relative_path_for_storage(file_path, workspace_root);
    let relative_path_clone = relative_path.clone();
    let content_clone = content.clone();
    let workspace_root_clone2 = workspace_root.to_path_buf();

    let extract_start = std::time::Instant::now();
    let task = tokio::task::spawn_blocking(move || {
        extract(relative_path_clone, content_clone, workspace_root_clone2)
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

    let symbols = results.symbols;
    file_info.symbol_count = symbols.len() as i32;
    let relationships = results.relationships;
    let pending_relationships = results.pending_relationships;
    let structured_pending_relationships = results.structured_pending_relationships;
    let identifiers = results.identifiers;
    let types = results.types;
    let type_argument_usages = results.type_argument_usages;
    let literals = results.literals;
    let parse_diagnostics = results.parse_diagnostics;

    if symbols.len() > 10 {
        debug!(
            "📊 Extracted {} symbols from {}",
            symbols.len(),
            relative_path
        );
    }

    if !pending_relationships.is_empty() {
        debug!(
            "📎 Found {} pending relationships in {} (need cross-file resolution)",
            pending_relationships.len(),
            relative_path
        );
    }

    Ok((
        symbols,
        relationships,
        pending_relationships,
        structured_pending_relationships,
        identifiers,
        types,
        type_argument_usages,
        literals,
        parse_diagnostics,
        file_info,
    ))
}

pub async fn process_file_without_parser(
    file_path: &Path,
    language: &str,
    workspace_root: &Path,
) -> Result<TextFileProcessResult> {
    tracing::trace!(
        "📂 Processing file without parser: {:?} (language: {})",
        file_path,
        language
    );

    let file_path_clone = file_path.to_path_buf();
    let workspace_root_clone = workspace_root.to_path_buf();

    let (_canonical_file_path, content, file_info) = tokio::task::spawn_blocking(move || {
        tracing::trace!(
            "🔄 Inside spawn_blocking (no parser) for: {:?}",
            file_path_clone
        );
        let canonical = file_path_clone
            .canonicalize()
            .unwrap_or_else(|_| file_path_clone.clone());
        let file_content = std::fs::read_to_string(&canonical)
            .map_err(|e| anyhow::anyhow!("Failed to read file {:?}: {}", canonical, e))?;
        let detected_language =
            detect_language_for_indexing_with_content(&file_path_clone, &file_content);
        let info = julie_core::database::create_file_info(
            &file_path_clone,
            &detected_language,
            &workspace_root_clone,
        )?;
        Ok::<_, anyhow::Error>((canonical, file_content, info))
    })
    .await
    .map_err(|e| anyhow::anyhow!("Failed to spawn blocking file I/O task: {}", e))??;

    trace!("Read {} bytes from file without parser", content.len());
    Ok((Vec::new(), Vec::new(), file_info))
}
