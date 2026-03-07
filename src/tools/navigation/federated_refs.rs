//! Federated reference search across all daemon-registered workspaces.
//!
//! When `workspace="all"` is passed to `fast_refs`, this module handles:
//! 1. Extracting Ready workspace DB handles from DaemonState
//! 2. Querying each workspace in parallel via spawn_blocking
//! 3. Collecting and formatting results grouped by project

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use tracing::debug;

use crate::daemon_state::WorkspaceLoadStatus;
use crate::extractors::{Relationship, RelationshipKind, Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::utils::cross_language_intelligence::generate_naming_variants;

use super::formatting::{format_federated_refs_results, ProjectTaggedResult};

/// Entry point for federated fast_refs.
///
/// Reads all Ready workspaces from daemon state, queries each in parallel,
/// and formats results with project tags.
pub async fn find_refs_federated(
    handler: &JulieServerHandler,
    symbol: &str,
    include_definition: bool,
    reference_kind: Option<&str>,
    limit: u32,
) -> Result<CallToolResult> {
    let daemon_state = handler
        .daemon_state
        .as_ref()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "workspace=\"all\" requires daemon mode. \
                 In stdio mode, use workspace=\"primary\" or a specific workspace ID."
            )
        })?;

    // Read-lock daemon state, extract DB arcs + project names for Ready workspaces.
    // We drop the lock before doing any DB queries to avoid holding it across awaits.
    let workspace_entries: Vec<(String, String, Arc<Mutex<crate::database::SymbolDatabase>>)> = {
        let state = daemon_state.read().await;
        state
            .workspaces
            .iter()
            .filter(|(_, loaded)| loaded.status == WorkspaceLoadStatus::Ready)
            .filter_map(|(ws_id, loaded)| {
                let db = loaded.workspace.db.as_ref()?.clone();
                let project_name = loaded
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| ws_id.clone());
                Some((ws_id.clone(), project_name, db))
            })
            .collect()
    };
    // Lock is dropped here

    if workspace_entries.is_empty() {
        return Ok(CallToolResult::text_content(vec![Content::text(format!(
            "No references found for \"{}\" (no Ready projects in daemon)",
            symbol
        ))]));
    }

    debug!(
        "Federated fast_refs: searching {} Ready workspace(s) for '{}'",
        workspace_entries.len(),
        symbol
    );

    // Query each workspace in parallel
    let mut join_set = tokio::task::JoinSet::new();

    for (_ws_id, project_name, db_arc) in workspace_entries {
        let symbol = symbol.to_string();
        let reference_kind = reference_kind.map(|s| s.to_string());
        let limit = limit;

        join_set.spawn(async move {
            let (defs, refs) =
                query_workspace_refs(&db_arc, &symbol, reference_kind.as_deref(), limit).await?;
            Ok::<(String, Vec<Symbol>, Vec<Relationship>), anyhow::Error>((
                project_name, defs, refs,
            ))
        });
    }

    // Collect results grouped by project
    let mut per_project: Vec<(String, Vec<Symbol>, Vec<Relationship>)> = Vec::new();

    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok(Ok((project_name, defs, refs))) => {
                if !defs.is_empty() || !refs.is_empty() {
                    per_project.push((project_name, defs, refs));
                }
            }
            Ok(Err(e)) => {
                debug!("Federated fast_refs: workspace query failed: {}", e);
            }
            Err(e) => {
                debug!("Federated fast_refs: task join failed: {}", e);
            }
        }
    }

    // Sort by project name for stable output
    per_project.sort_by(|a, b| a.0.cmp(&b.0));

    // Build tagged results for formatting
    let tagged: Vec<ProjectTaggedResult<'_>> = per_project
        .iter()
        .map(|(name, defs, refs)| {
            let (defs_slice, refs_slice): (&[Symbol], &[Relationship]) = if include_definition {
                (defs.as_slice(), refs.as_slice())
            } else {
                (&[], refs.as_slice())
            };
            ProjectTaggedResult {
                project_name: name.as_str(),
                definitions: defs_slice,
                references: refs_slice,
            }
        })
        .collect();

    let output = format_federated_refs_results(symbol, &tagged);

    Ok(CallToolResult::text_content(vec![Content::text(output)]))
}

// ---------------------------------------------------------------------------
// Internal helper
// ---------------------------------------------------------------------------

/// Query a single workspace's database for all references to a symbol.
///
/// Runs all DB operations inside `spawn_blocking` to avoid blocking the
/// tokio runtime. Uses the same multi-strategy approach as the primary
/// workspace search: exact name -> naming variants -> relationships -> identifiers.
async fn query_workspace_refs(
    db_arc: &Arc<Mutex<crate::database::SymbolDatabase>>,
    symbol: &str,
    reference_kind: Option<&str>,
    limit: u32,
) -> Result<(Vec<Symbol>, Vec<Relationship>)> {
    let db = db_arc.clone();
    let symbol = symbol.to_string();
    let reference_kind = reference_kind.map(|s| s.to_string());
    let limit = limit as usize;

    tokio::task::spawn_blocking(move || {
        let db_lock = super::lock_db(&db, "federated fast_refs");

        // Strategy 1: Exact name lookup
        let mut definitions = db_lock.get_symbols_by_name(&symbol).unwrap_or_default();

        // Strategy 2: Cross-language naming variants
        let variants = generate_naming_variants(&symbol);
        for variant in &variants {
            if *variant != symbol {
                if let Ok(variant_symbols) = db_lock.get_symbols_by_name(variant) {
                    for s in variant_symbols {
                        if s.name == *variant {
                            definitions.push(s);
                        }
                    }
                }
            }
        }

        // Deduplicate
        definitions.sort_by(|a, b| a.id.cmp(&b.id));
        definitions.dedup_by(|a, b| a.id == b.id);

        // Separate imports into references
        let mut import_refs: Vec<Relationship> = Vec::new();
        definitions.retain(|sym| {
            if sym.kind == SymbolKind::Import {
                import_refs.push(Relationship {
                    id: format!("import_{}_{}", sym.file_path, sym.start_line),
                    from_symbol_id: sym.id.clone(),
                    to_symbol_id: String::new(),
                    kind: RelationshipKind::Imports,
                    file_path: sym.file_path.clone(),
                    line_number: sym.start_line,
                    confidence: 1.0,
                    metadata: None,
                });
                false
            } else {
                true
            }
        });

        // Filter synthetic import refs if reference_kind is set and isn't "import"
        let mut references: Vec<Relationship> = match &reference_kind {
            Some(kind) if kind != "import" => Vec::new(),
            _ => import_refs,
        };

        // Strategy 3: Relationships to definition symbols
        let definition_ids: Vec<String> = definitions.iter().map(|d| d.id.clone()).collect();
        let symbol_references = if let Some(kind) = &reference_kind {
            db_lock.get_relationships_to_symbols_filtered_by_kind(&definition_ids, kind)
        } else {
            db_lock.get_relationships_to_symbols(&definition_ids)
        };
        if let Ok(refs) = symbol_references {
            references.extend(refs);
        }

        // Strategy 4: Identifier-based reference discovery
        let mut all_names = vec![symbol.clone()];
        for v in &variants {
            if *v != symbol {
                all_names.push(v.clone());
            }
        }
        let first_def_id = definitions.first().map(|d| d.id.clone()).unwrap_or_default();
        let identifier_refs = if let Some(kind) = &reference_kind {
            db_lock.get_identifiers_by_names_and_kind(&all_names, kind)
        } else {
            db_lock.get_identifiers_by_names(&all_names)
        };
        if let Ok(ident_refs) = identifier_refs {
            let mut existing_refs: HashSet<(String, u32)> = references
                .iter()
                .map(|r| (r.file_path.clone(), r.line_number))
                .collect();
            for def in &definitions {
                existing_refs.insert((def.file_path.clone(), def.start_line));
            }
            for ident in ident_refs {
                let key = (ident.file_path.clone(), ident.start_line);
                if existing_refs.contains(&key) {
                    continue;
                }
                let rel_kind = match ident.kind.as_str() {
                    "call" => RelationshipKind::Calls,
                    "import" => RelationshipKind::Imports,
                    _ => RelationshipKind::References,
                };
                references.push(Relationship {
                    id: format!("ident_{}_{}", ident.file_path, ident.start_line),
                    from_symbol_id: ident.containing_symbol_id.unwrap_or_default(),
                    to_symbol_id: first_def_id.clone(),
                    kind: rel_kind,
                    file_path: ident.file_path,
                    line_number: ident.start_line,
                    confidence: ident.confidence,
                    metadata: None,
                });
            }
        }

        // Sort by file path, then line number
        references.sort_by(|a, b| {
            let file_cmp = a.file_path.cmp(&b.file_path);
            if file_cmp != std::cmp::Ordering::Equal {
                return file_cmp;
            }
            a.line_number.cmp(&b.line_number)
        });

        // Apply limit
        references.truncate(limit);

        Ok((definitions, references))
    })
    .await
    .map_err(|e| anyhow::anyhow!("spawn_blocking join error: {}", e))?
}
