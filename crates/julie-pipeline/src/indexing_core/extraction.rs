use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use futures::stream::{self, StreamExt};
use tracing::{debug, info, trace, warn};

use crate::indexing_core::batch::ExtractedBatch;
use crate::indexing_core::normalized::{NormalizedExtractionData, normalize_extraction_results};
use crate::indexing_core::paths::relative_path_for_storage;
use julie_core::file_policy::{
    ExtractionMode, detect_language_for_indexing_with_content, determine_extraction_mode,
};
use julie_extractors::{ExtractionResults, Relationship, Symbol};

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

#[derive(Debug)]
pub struct ParserFileProcessResult {
    pub normalized: NormalizedExtractionData,
    pub file_info: julie_core::database::FileInfo,
}

type TextFileProcessResult = (Vec<Symbol>, Vec<Relationship>, julie_core::database::FileInfo);

enum ExtractOutcome {
    WithParser(Result<Box<ParserFileProcessResult>>),
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
    let configs = Arc::new(julie_index::search::LanguageConfigs::load_embedded());
    let outcomes: Vec<(String, PathBuf, ExtractOutcome)> = stream::iter(work)
        .map(|(language, file_path, has_parser)| {
            let configs = Arc::clone(&configs);
            async move {
                let outcome = if has_parser {
                    ExtractOutcome::WithParser(
                        process_file_with_parser_using_configs(
                            &file_path,
                            &language,
                            workspace_root,
                            configs,
                        )
                        .await
                        .map(Box::new),
                    )
                } else {
                    ExtractOutcome::WithoutParser(
                        process_file_without_parser(&file_path, &language, workspace_root).await,
                    )
                };
                (language, file_path, outcome)
            }
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
            ExtractOutcome::WithParser(Ok(result)) => {
                let ParserFileProcessResult {
                    normalized,
                    file_info,
                } = *result;
                records.push(ExtractedFileRecord {
                    relative_path: relative_path.clone(),
                    language: file_info.language.clone(),
                    disposition: ExtractedFileDisposition::Parsed,
                });
                batch.files_processed += 1;
                trace!(
                    "File {} extracted {} symbols, {} pending relationships",
                    file_path.display(),
                    normalized.symbols.len(),
                    normalized.pending_relationships.len()
                );
                batch.files_to_clean.push(relative_path.clone());
                batch.all_symbols.extend(normalized.symbols);
                batch.all_relationships.extend(normalized.relationships);
                batch
                    .all_pending_relationships
                    .extend(normalized.pending_relationships);
                batch
                    .all_structured_pending_relationships
                    .extend(normalized.structured_pending_relationships);
                batch.all_identifiers.extend(normalized.identifiers);
                batch.all_types.extend(normalized.types);
                batch
                    .all_type_argument_rows
                    .extend(normalized.type_argument_rows);
                batch.all_literals.extend(normalized.literals);
                batch.all_source_regions.extend(normalized.source_regions);
                batch
                    .all_structural_facts
                    .extend(normalized.structural_facts);
                batch
                    .all_complexity_metrics
                    .extend(normalized.complexity_metrics);
                batch
                    .parse_diagnostics_by_file
                    .push((relative_path, normalized.parse_diagnostics));
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
    process_file_with_parser_using_configs(
        file_path,
        language,
        workspace_root,
        Arc::new(julie_index::search::LanguageConfigs::load_embedded()),
    )
    .await
}

async fn process_file_with_parser_using_configs(
    file_path: &Path,
    language: &str,
    workspace_root: &Path,
    configs: Arc<julie_index::search::LanguageConfigs>,
) -> Result<ParserFileProcessResult> {
    process_file_with_parser_using(
        file_path,
        language,
        workspace_root,
        |relative_path, content, workspace_root_path| {
            julie_extractors::extract_canonical(&relative_path, &content, &workspace_root_path)
        },
        configs,
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
    process_file_with_parser_using(
        file_path,
        language,
        workspace_root,
        extract,
        Arc::new(julie_index::search::LanguageConfigs::load_embedded()),
    )
    .await
}

async fn process_file_with_parser_using<F>(
    file_path: &Path,
    _language: &str,
    workspace_root: &Path,
    extract: F,
    configs: Arc<julie_index::search::LanguageConfigs>,
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
        return Ok(ParserFileProcessResult {
            normalized: normalize_extraction_results(ExtractionResults::empty(), &configs),
            file_info,
        });
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

    let normalized = normalize_extraction_results(results, &configs);
    file_info.symbol_count = normalized.symbols.len() as i32;

    if normalized.symbols.len() > 10 {
        debug!(
            "📊 Extracted {} symbols from {}",
            normalized.symbols.len(),
            relative_path
        );
    }

    if !normalized.pending_relationships.is_empty() {
        debug!(
            "📎 Found {} pending relationships in {} (need cross-file resolution)",
            normalized.pending_relationships.len(),
            relative_path
        );
    }

    Ok(ParserFileProcessResult {
        normalized,
        file_info,
    })
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
