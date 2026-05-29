use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::database::{SymbolDatabase, calculate_file_hash};
use crate::external_extract::data_loss_guard::ensure_batch_preserves_known_good_symbols;
use crate::external_extract::{
    EXTRACT_CONTRACT_VERSION, ExternalExtractArgs, ExternalExtractCommand, ExternalExtractReport,
    ExternalExtractStatus, ExternalInfoSchemaState, ensure_external_extract_metadata,
    ensure_external_extract_metadata_with_root_policy, load_external_extract_metadata,
    mark_external_extract_analysis_current, normalize_deleted_external_file,
    normalize_existing_external_file, normalize_external_root,
    open_external_extract_database_for_operation,
};
use crate::indexing_core::analysis::run_sqlite_analysis;
use crate::indexing_core::discovery::{discover_external_files, is_external_file_indexable};
use crate::indexing_core::extraction::extract_files_for_indexing_with_records;
use crate::indexing_core::persistence::{
    persist_force_rebuild, persist_incremental_scan, persist_single_file_delete,
    persist_single_file_replace,
};
use crate::tools::workspace::indexing::file_policy::detect_language_for_indexing;

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

    let mut operation = open_external_extract_database_for_operation(&args.db, args.strict_schema)?;
    let metadata = ensure_external_extract_metadata_with_root_policy(
        operation.db(),
        &root,
        args.workspace_id.as_deref(),
        force,
    )?;

    let (files_to_extract, orphaned_files) = if force {
        (discovered_files, Vec::new())
    } else {
        filter_scan_delta(operation.db(), &root, discovered_files)?
    };

    let (batch, records) =
        extract_files_for_indexing_with_records(group_files_by_language(files_to_extract), &root)
            .await?;
    ensure_batch_preserves_known_good_symbols(operation.db(), &batch, &records)?;
    let symbols_extracted = batch.all_symbols.len() as u64;
    let files_updated = batch.files_processed as u64;
    let files_deleted = orphaned_files.len() as u64;

    let revision = if force {
        persist_force_rebuild(operation.db_mut(), &metadata.workspace_id, &batch)?
    } else if batch.files_to_clean.is_empty() && orphaned_files.is_empty() {
        None
    } else {
        persist_incremental_scan(
            operation.db_mut(),
            &metadata.workspace_id,
            &batch,
            &orphaned_files,
        )?
    };

    maybe_run_analysis(operation.db_mut(), &metadata.workspace_id, args.analyze)?;

    Ok(success_report(
        args,
        if force {
            ExternalExtractStatus::Rebuilt
        } else if revision.is_some() {
            ExternalExtractStatus::Scanned
        } else {
            ExternalExtractStatus::Unchanged
        },
        "scan",
        Some(root),
        Some(metadata.workspace_id),
        ReportCounts {
            files_scanned,
            files_updated,
            files_deleted,
            symbols_extracted,
        },
        Some(operation.db()),
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

    let mut operation = open_external_extract_database_for_operation(&args.db, args.strict_schema)?;
    let metadata =
        ensure_external_extract_metadata(operation.db(), &root, args.workspace_id.as_deref())?;
    let workspace_id = metadata.workspace_id.clone();

    if !is_external_file_indexable(&root, &normalized.absolute, &args.ignore_files)? {
        let revision =
            persist_single_file_delete(operation.db_mut(), &workspace_id, &normalized.relative)?;
        maybe_run_analysis(operation.db_mut(), &workspace_id, args.analyze)?;
        return Ok(success_report(
            args,
            ExternalExtractStatus::Ignored,
            "update",
            Some(root),
            Some(workspace_id),
            ReportCounts {
                files_scanned: 1,
                files_updated: 0,
                files_deleted: u64::from(revision.is_some()),
                symbols_extracted: 0,
            },
            Some(operation.db()),
        )?);
    }

    let current_hash = calculate_file_hash(&normalized.absolute)
        .with_context(|| format!("failed to hash {}", normalized.absolute.display()))?;
    if operation.db().get_file_hash(&normalized.relative)? == Some(current_hash) {
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
            Some(operation.db()),
        )?);
    }

    let (batch, records) = extract_files_for_indexing_with_records(
        group_files_by_language(vec![normalized.absolute.clone()]),
        &root,
    )
    .await?;
    ensure_batch_preserves_known_good_symbols(operation.db(), &batch, &records)?;
    let symbols_extracted = batch.all_symbols.len() as u64;
    let files_updated = batch.files_processed as u64;
    persist_single_file_replace(operation.db_mut(), &workspace_id, &batch)?;
    maybe_run_analysis(operation.db_mut(), &workspace_id, args.analyze)?;

    Ok(success_report(
        args,
        ExternalExtractStatus::Changed,
        "update",
        Some(root),
        Some(workspace_id),
        ReportCounts {
            files_scanned: 1,
            files_updated,
            files_deleted: 0,
            symbols_extracted,
        },
        Some(operation.db()),
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

    let mut operation = open_external_extract_database_for_operation(&args.db, args.strict_schema)?;
    let metadata =
        ensure_external_extract_metadata(operation.db(), &root, args.workspace_id.as_deref())?;
    let workspace_id = metadata.workspace_id.clone();
    let revision =
        persist_single_file_delete(operation.db_mut(), &workspace_id, &normalized.relative)?;
    maybe_run_analysis(operation.db_mut(), &workspace_id, args.analyze)?;

    Ok(success_report(
        args,
        if revision.is_some() {
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
            files_deleted: u64::from(revision.is_some()),
            symbols_extracted: 0,
        },
        Some(operation.db()),
    )?)
}

pub async fn run_external_analyze(args: &ExternalExtractArgs) -> Result<ExternalExtractReport> {
    if !matches!(args.command, ExternalExtractCommand::Analyze) {
        return Err(anyhow!("run_external_analyze requires an analyze command"));
    }

    let mut operation = open_external_extract_database_for_operation(&args.db, args.strict_schema)?;
    let metadata = load_external_extract_metadata(operation.db())?
        .context("external extract metadata is missing; run extract scan first")?;
    run_and_mark_analysis_current(operation.db_mut(), &metadata.workspace_id)?;

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
        Some(operation.db()),
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

fn filter_scan_delta(
    db: &SymbolDatabase,
    root: &Path,
    discovered_files: Vec<PathBuf>,
) -> Result<(Vec<PathBuf>, Vec<String>)> {
    let existing_hashes = db.get_file_hashes_for_workspace()?;
    let mut current_paths = HashSet::new();
    let mut files_to_extract = Vec::new();

    for file_path in discovered_files {
        let relative_path = crate::utils::paths::to_relative_unix_style(&file_path, root)?;
        current_paths.insert(relative_path.clone());
        let current_hash = calculate_file_hash(&file_path)
            .with_context(|| format!("failed to hash {}", file_path.display()))?;
        if existing_hashes
            .get(&relative_path)
            .is_some_and(|stored_hash| stored_hash == &current_hash)
        {
            continue;
        }
        files_to_extract.push(file_path);
    }

    let mut orphaned_files: Vec<String> = existing_hashes
        .keys()
        .filter(|path| !current_paths.contains(*path))
        .cloned()
        .collect();
    orphaned_files.sort();

    Ok((files_to_extract, orphaned_files))
}

fn group_files_by_language(files: Vec<PathBuf>) -> HashMap<String, Vec<PathBuf>> {
    let mut files_by_language: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for file_path in files {
        files_by_language
            .entry(detect_language_for_indexing(&file_path))
            .or_default()
            .push(file_path);
    }
    files_by_language
}

fn maybe_run_analysis(db: &mut SymbolDatabase, workspace_id: &str, analyze: bool) -> Result<()> {
    if analyze {
        run_and_mark_analysis_current(db, workspace_id)?;
    }
    Ok(())
}

fn run_and_mark_analysis_current(db: &mut SymbolDatabase, workspace_id: &str) -> Result<()> {
    run_sqlite_analysis(db)?;
    let revision = db.get_current_canonical_revision(workspace_id)?;
    mark_external_extract_analysis_current(db, revision)?;
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
    db: Option<&SymbolDatabase>,
) -> Result<ExternalExtractReport> {
    let context = report_context(db, workspace_id.as_deref())?;
    Ok(ExternalExtractReport {
        status,
        operation: operation.to_string(),
        workspace_id,
        db: args.db.clone(),
        root,
        julie_version: context.julie_version,
        schema_version: context.schema_version,
        schema_state: context.schema_state,
        extract_contract_version: context.extract_contract_version,
        revision: context.revision,
        analyzed_revision: context.analyzed_revision,
        analysis_state: context.analysis_state,
        missing_metadata_keys: context.missing_metadata_keys,
        files_scanned: counts.files_scanned,
        files_updated: counts.files_updated,
        files_deleted: counts.files_deleted,
        symbols_extracted: counts.symbols_extracted,
        files_total: context.files_total,
        symbols_total: context.symbols_total,
        relationships_total: context.relationships_total,
        identifiers_total: context.identifiers_total,
        types_total: context.types_total,
        type_arguments_total: context.type_arguments_total,
        literals_total: context.literals_total,
        errors: Vec::new(),
    })
}

#[derive(Default)]
struct ReportDbContext {
    schema_version: Option<i32>,
    julie_version: Option<String>,
    schema_state: Option<ExternalInfoSchemaState>,
    extract_contract_version: Option<i32>,
    revision: Option<i64>,
    analyzed_revision: Option<i64>,
    analysis_state: Option<String>,
    missing_metadata_keys: Vec<String>,
    files_total: u64,
    symbols_total: u64,
    relationships_total: u64,
    identifiers_total: u64,
    types_total: u64,
    type_arguments_total: u64,
    literals_total: u64,
}

fn report_context(
    db: Option<&SymbolDatabase>,
    workspace_id: Option<&str>,
) -> Result<ReportDbContext> {
    let Some(db) = db else {
        return Ok(ReportDbContext::default());
    };

    let stats = db.get_stats()?;
    let identifiers_total: i64 =
        db.conn
            .query_row("SELECT COUNT(*) FROM identifiers", [], |row| row.get(0))?;
    let types_total: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM types", [], |row| row.get(0))?;
    let type_arguments_total: i64 =
        db.conn
            .query_row("SELECT COUNT(*) FROM type_arguments", [], |row| row.get(0))?;
    let literals_total: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM literals", [], |row| row.get(0))?;
    let metadata = load_external_extract_metadata(db)?;
    Ok(ReportDbContext {
        julie_version: metadata
            .as_ref()
            .map(|metadata| metadata.julie_version.clone()),
        schema_version: Some(db.get_schema_version()?),
        schema_state: Some(ExternalInfoSchemaState::Current),
        extract_contract_version: metadata
            .as_ref()
            .map(|metadata| metadata.extract_contract_version)
            .or(Some(EXTRACT_CONTRACT_VERSION)),
        revision: workspace_id
            .map(|workspace_id| db.get_current_canonical_revision(workspace_id))
            .transpose()?
            .flatten(),
        analyzed_revision: metadata
            .as_ref()
            .and_then(|metadata| metadata.analyzed_revision),
        analysis_state: metadata
            .as_ref()
            .map(|metadata| metadata.analysis_state.clone()),
        missing_metadata_keys: Vec::new(),
        files_total: stats.total_files.try_into()?,
        symbols_total: stats.total_symbols.try_into()?,
        relationships_total: stats.total_relationships.try_into()?,
        identifiers_total: identifiers_total.try_into()?,
        types_total: types_total.try_into()?,
        type_arguments_total: type_arguments_total.try_into()?,
        literals_total: literals_total.try_into()?,
    })
}
