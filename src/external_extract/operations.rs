use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use julie_core::file_policy::detect_language_for_indexing;
use julie_facts::{FactsStore, FactsWriter, PathChange};

use crate::external_extract::extract_write::{
    RootExtractor, extract_normalization, filter_scan_delta, path_blob_hash, reject_empty_replace,
    upsert_change,
};
use crate::external_extract::metadata::EXTRACT_HASH_ALGORITHM;
use crate::external_extract::{
    EXTRACT_CONTRACT_VERSION, ExternalExtractArgs, ExternalExtractCommand, ExternalExtractReport,
    ExternalExtractStatus, ExternalInfoSchemaState, ensure_external_extract_metadata,
    ensure_external_extract_metadata_with_root_policy, load_external_extract_metadata,
    mark_external_extract_analysis_current, normalize_deleted_external_file,
    normalize_existing_external_file, normalize_external_root, open_facts_store,
};
use crate::indexing_core::discovery::{discover_external_files, is_external_file_indexable};

pub async fn run_external_extract(args: &ExternalExtractArgs) -> Result<ExternalExtractReport> {
    match args.command {
        ExternalExtractCommand::Scan { .. } => run_external_scan(args).await,
        ExternalExtractCommand::Update { .. } => run_external_update(args).await,
        ExternalExtractCommand::Delete { .. } => run_external_delete(args).await,
        ExternalExtractCommand::Analyze => run_external_analyze(args).await,
        ExternalExtractCommand::Info => run_external_info(args),
    }
}

pub async fn run_external_scan(args: &ExternalExtractArgs) -> Result<ExternalExtractReport> {
    let force = match args.command {
        ExternalExtractCommand::Scan { force } => force,
        _ => return Err(anyhow!("run_external_scan requires a scan command")),
    };
    let root_arg = args
        .root
        .as_ref()
        .context("external scan requires a root path")?;
    let root = normalize_external_root(root_arg)?;
    let discovered_files = discover_external_files(&root, &args.ignore_files)?;
    let files_scanned = discovered_files.len() as u64;

    if force && args.db.exists() {
        std::fs::remove_file(&args.db)
            .with_context(|| format!("failed to replace {}", args.db.display()))?;
    }

    let mut store = open_facts_store(&args.db, args.strict_schema)?;
    let metadata = if force {
        None
    } else {
        Some(ensure_external_extract_metadata_with_root_policy(
            &store,
            &root,
            args.workspace_id.as_deref(),
            false,
        )?)
    };

    let (files_to_extract, orphaned_files) = if force {
        (discovered_files, Vec::new())
    } else {
        filter_scan_delta(&store, &root, discovered_files)?
    };

    let extractor = RootExtractor { root: root.clone() };
    let mut writer = FactsWriter::new(&mut store, &extractor, extract_normalization());
    let mut changes = Vec::new();
    for file_path in &files_to_extract {
        changes.push(upsert_change(&root, file_path)?);
    }
    for path in &orphaned_files {
        changes.push(PathChange::Remove { path: path.clone() });
    }
    let applied = writer.apply(&changes)?;
    drop(writer);

    let workspace_id = match metadata {
        Some(metadata) => metadata.workspace_id,
        None => {
            let metadata = ensure_external_extract_metadata_with_root_policy(
                &store,
                &root,
                args.workspace_id.as_deref(),
                true,
            )?;
            metadata.workspace_id
        }
    };
    if args.analyze {
        maybe_run_analysis(&store, true)?;
    } else {
        crate::external_extract::mark_external_extract_analysis_stale(&store)?;
    }

    Ok(success_report(
        args,
        if force {
            ExternalExtractStatus::Rebuilt
        } else if applied.new_blobs > 0 || applied.removed_paths > 0 || applied.reused_blobs > 0 {
            ExternalExtractStatus::Scanned
        } else {
            ExternalExtractStatus::Unchanged
        },
        "scan",
        Some(root),
        Some(workspace_id),
        ReportCounts {
            files_scanned,
            files_updated: applied.paths_now.len() as u64,
            files_deleted: applied.removed_paths as u64,
            symbols_extracted: store.reader().symbol_count().unwrap_or(0),
        },
        &store,
    )?)
}

pub async fn run_external_update(args: &ExternalExtractArgs) -> Result<ExternalExtractReport> {
    let file_arg = match &args.command {
        ExternalExtractCommand::Update { file } => file,
        _ => return Err(anyhow!("run_external_update requires an update command")),
    };
    let root_arg = args
        .root
        .as_ref()
        .context("external update requires a root path")?;
    let root = normalize_external_root(root_arg)?;
    let normalized = normalize_existing_external_file(&root, file_arg).with_context(|| {
        format!(
            "failed to update {}; if the file was deleted, run extract delete",
            file_arg.display()
        )
    })?;

    let mut store = open_facts_store(&args.db, args.strict_schema)?;
    let metadata = ensure_external_extract_metadata(&store, &root, args.workspace_id.as_deref())?;
    let workspace_id = metadata.workspace_id.clone();
    let extractor = RootExtractor { root: root.clone() };

    if !is_external_file_indexable(&root, &normalized.absolute, &args.ignore_files)? {
        let applied =
            FactsWriter::new(&mut store, &extractor, extract_normalization()).apply(&[
                PathChange::Remove {
                    path: normalized.relative.clone(),
                },
            ])?;
        if args.analyze {
            maybe_run_analysis(&store, true)?;
        } else {
            crate::external_extract::mark_external_extract_analysis_stale(&store)?;
        }
        return Ok(success_report(
            args,
            ExternalExtractStatus::Ignored,
            "update",
            Some(root),
            Some(workspace_id),
            ReportCounts {
                files_scanned: 1,
                files_updated: 0,
                files_deleted: applied.removed_paths as u64,
                symbols_extracted: 0,
            },
            &store,
        )?);
    }

    let bytes = std::fs::read(&normalized.absolute)
        .with_context(|| format!("failed to read {}", normalized.absolute.display()))?;
    let current_hash = blake3::hash(&bytes).to_hex().to_string();
    if path_blob_hash(&store, &normalized.relative)? == Some(current_hash) {
        return Ok(success_report(
            args,
            ExternalExtractStatus::Unchanged,
            "update",
            Some(root),
            Some(workspace_id),
            ReportCounts {
                files_scanned: 1,
                files_updated: 0,
                files_deleted: 0,
                symbols_extracted: 0,
            },
            &store,
        )?);
    }

    let language = detect_language_for_indexing(&normalized.absolute);
    reject_empty_replace(&store, &extractor, &normalized.relative, &bytes, &language)?;
    let applied = FactsWriter::new(&mut store, &extractor, extract_normalization()).apply(&[
        PathChange::Upsert {
            path: normalized.relative,
            bytes,
            language,
        },
    ])?;
    if args.analyze {
        maybe_run_analysis(&store, true)?;
    } else {
        crate::external_extract::mark_external_extract_analysis_stale(&store)?;
    }
    Ok(success_report(
        args,
        ExternalExtractStatus::Changed,
        "update",
        Some(root),
        Some(workspace_id),
        ReportCounts {
            files_scanned: 1,
            files_updated: applied.paths_now.len() as u64,
            files_deleted: 0,
            symbols_extracted: store.reader().symbol_count().unwrap_or(0),
        },
        &store,
    )?)
}

pub async fn run_external_delete(args: &ExternalExtractArgs) -> Result<ExternalExtractReport> {
    let file_arg = match &args.command {
        ExternalExtractCommand::Delete { file } => file,
        _ => return Err(anyhow!("run_external_delete requires a delete command")),
    };
    let root_arg = args
        .root
        .as_ref()
        .context("external delete requires a root path")?;
    let root = normalize_external_root(root_arg)?;
    let normalized = normalize_deleted_external_file(&root, file_arg)?;
    let mut store = open_facts_store(&args.db, args.strict_schema)?;
    let metadata = ensure_external_extract_metadata(&store, &root, args.workspace_id.as_deref())?;
    let workspace_id = metadata.workspace_id.clone();
    let extractor = RootExtractor { root: root.clone() };
    let existed = path_blob_hash(&store, &normalized.relative)?.is_some();
    let applied = FactsWriter::new(&mut store, &extractor, extract_normalization()).apply(&[
        PathChange::Remove {
            path: normalized.relative,
        },
    ])?;
    if args.analyze {
        maybe_run_analysis(&store, true)?;
    } else {
        crate::external_extract::mark_external_extract_analysis_stale(&store)?;
    }
    Ok(success_report(
        args,
        if existed {
            ExternalExtractStatus::Deleted
        } else {
            ExternalExtractStatus::NotFound
        },
        "delete",
        Some(root),
        Some(workspace_id),
        ReportCounts {
            files_scanned: 0,
            files_updated: 0,
            files_deleted: applied.removed_paths as u64,
            symbols_extracted: 0,
        },
        &store,
    )?)
}

pub async fn run_external_analyze(args: &ExternalExtractArgs) -> Result<ExternalExtractReport> {
    if !matches!(args.command, ExternalExtractCommand::Analyze) {
        return Err(anyhow!("run_external_analyze requires an analyze command"));
    }
    let store = open_facts_store(&args.db, args.strict_schema)?;
    let metadata = load_external_extract_metadata(&store)?
        .context("external extract metadata is missing; run extract scan first")?;
    mark_external_extract_analysis_current(&store, Some(store.reader().symbol_count()? as i64))?;
    Ok(success_report(
        args,
        ExternalExtractStatus::Analyzed,
        "analyze",
        None,
        Some(metadata.workspace_id),
        ReportCounts {
            files_scanned: 0,
            files_updated: 0,
            files_deleted: 0,
            symbols_extracted: 0,
        },
        &store,
    )?)
}

pub fn run_external_info(args: &ExternalExtractArgs) -> Result<ExternalExtractReport> {
    if !matches!(args.command, ExternalExtractCommand::Info) {
        return Err(anyhow!("run_external_info requires an info command"));
    }
    let info = crate::external_extract::read_external_extract_info(&args.db)?;
    Ok(ExternalExtractReport {
        status: ExternalExtractStatus::Unchanged,
        operation: "info".to_string(),
        workspace_id: info
            .metadata
            .as_ref()
            .map(|metadata| metadata.workspace_id.clone()),
        db: args.db.clone(),
        root: info
            .metadata
            .as_ref()
            .map(|metadata| PathBuf::from(&metadata.root_path)),
        julie_version: info
            .metadata
            .as_ref()
            .map(|metadata| metadata.julie_version.clone()),
        schema_version: info.schema_version,
        schema_state: Some(info.schema_state),
        extract_contract_version: info
            .metadata
            .as_ref()
            .map(|metadata| metadata.extract_contract_version),
        hash_algorithm: info
            .metadata
            .as_ref()
            .map(|metadata| metadata.hash_algorithm.clone()),
        revision: info.latest_revision,
        analyzed_revision: info
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.analyzed_revision),
        analysis_state: info
            .metadata
            .as_ref()
            .map(|metadata| metadata.analysis_state.clone()),
        missing_metadata_keys: info.missing_metadata_keys,
        files_scanned: 0,
        files_updated: 0,
        files_deleted: 0,
        symbols_extracted: 0,
        files_total: info.counts.files,
        symbols_total: info.counts.symbols,
        relationships_total: info.counts.relationships,
        identifiers_total: info.counts.identifiers,
        types_total: info.counts.types,
        type_arguments_total: info.counts.type_arguments,
        literals_total: info.counts.literals,
        errors: Vec::new(),
    })
}

fn maybe_run_analysis(store: &FactsStore, analyze: bool) -> Result<()> {
    if analyze {
        mark_external_extract_analysis_current(store, Some(store.reader().symbol_count()? as i64))?;
    }
    Ok(())
}

struct ReportCounts {
    files_scanned: u64,
    files_updated: u64,
    files_deleted: u64,
    symbols_extracted: u64,
}

fn success_report(
    args: &ExternalExtractArgs,
    status: ExternalExtractStatus,
    operation: &str,
    root: Option<PathBuf>,
    workspace_id: Option<String>,
    counts: ReportCounts,
    store: &FactsStore,
) -> Result<ExternalExtractReport> {
    let info = crate::external_extract::read_external_extract_info(&args.db).ok();
    Ok(ExternalExtractReport {
        status,
        operation: operation.to_string(),
        workspace_id,
        db: args.db.clone(),
        root,
        julie_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        schema_version: info.as_ref().and_then(|info| info.schema_version),
        schema_state: Some(ExternalInfoSchemaState::Current),
        extract_contract_version: Some(EXTRACT_CONTRACT_VERSION),
        hash_algorithm: Some(EXTRACT_HASH_ALGORITHM.to_string()),
        revision: info.as_ref().and_then(|info| info.latest_revision),
        analyzed_revision: info
            .as_ref()
            .and_then(|info| info.metadata.as_ref())
            .and_then(|metadata| metadata.analyzed_revision),
        analysis_state: info
            .as_ref()
            .and_then(|info| info.metadata.as_ref())
            .map(|metadata| metadata.analysis_state.clone()),
        missing_metadata_keys: info
            .as_ref()
            .map(|info| info.missing_metadata_keys.clone())
            .unwrap_or_default(),
        files_scanned: counts.files_scanned,
        files_updated: counts.files_updated,
        files_deleted: counts.files_deleted,
        symbols_extracted: counts.symbols_extracted,
        files_total: store
            .reader()
            .paths()
            .map(|paths| paths.len() as u64)
            .unwrap_or(0),
        symbols_total: store.reader().symbol_count().unwrap_or(0),
        relationships_total: count_table(store, "relationships"),
        identifiers_total: count_table(store, "identifiers"),
        types_total: count_table(store, "types"),
        type_arguments_total: count_table(store, "type_arguments"),
        literals_total: count_table(store, "literals"),
        errors: Vec::new(),
    })
}

fn count_table(store: &FactsStore, table: &str) -> u64 {
    store
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0)
        .max(0) as u64
}
