//! Line-mode (grep-style) search
//!
//! Provides grep-style line-by-line searching with file paths and line numbers.
//! Used when output format is set to "lines".

use anyhow::Result;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use tracing::{debug, warn};

use crate::handler::JulieServerHandler;
use crate::search::{SearchFilter, SearchIndex};

use super::query::{line_match_strategy, line_matches, matches_glob_pattern};
use super::types::{LineMatch, LineMatchStrategy};

/// Line-level search mode (grep-style output with line numbers)
///
/// Returns every line matching the query with file:line_number:line_content format.
/// Supports language and file pattern filtering for targeted searches.
pub async fn line_mode_search(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    workspace: &Option<String>,
    handler: &JulieServerHandler,
) -> Result<CallToolResult> {
    debug!("📄 Line-level search for: '{}'", query);

    // Get workspace and database
    let workspace_struct = handler
        .get_workspace()
        .await?
        .ok_or_else(|| anyhow::anyhow!("No workspace initialized for line search"))?;

    let db = workspace_struct
        .db
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No database available for line search"))?;

    let registry_service = crate::workspace::registry_service::WorkspaceRegistryService::new(
        workspace_struct.root.clone(),
    );

    let primary_workspace_id = registry_service
        .get_primary_workspace_id()
        .await?
        .unwrap_or_else(|| "primary".to_string());

    // Resolve workspace filter for singular workspace search
    let target_workspace_id = match workspace {
        Some(ws_id) if ws_id == "primary" => {
            // "primary" is a filter keyword, not an ID - use the resolved primary workspace ID
            primary_workspace_id.clone()
        }
        Some(ws_id) if ws_id != "all" => {
            // Specific workspace ID - validate it exists
            match registry_service.get_workspace(ws_id).await? {
                Some(_) => ws_id.clone(),
                None => return Err(anyhow::anyhow!("Workspace '{}' not found", ws_id)),
            }
        }
        _ => primary_workspace_id.clone(),
    };

    let match_strategy = line_match_strategy(query);
    let base_limit = limit.max(1) as usize;
    let has_file_filter = file_pattern.is_some();
    let fetch_limit = if has_file_filter {
        // When file_pattern is active, most Tantivy results will be filtered out.
        // Fetch more candidates so matching files aren't missed by the limit cap.
        base_limit.saturating_mul(100).max(500).min(1000)
    } else {
        // 20x: compound token boosting broadens matching, so we need more candidates
        // to surface precise matches after line-level re-ranking.
        base_limit.saturating_mul(20)
    };
    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.clone(),
    };

    // Search the single target workspace
    let all_line_matches: Vec<LineMatch> = if target_workspace_id == primary_workspace_id {
        // Search primary workspace using Tantivy index + shared DB for content
        let search_index = workspace_struct
            .search_index
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Search index not initialized. Run 'manage_workspace index' first."
                )
            })?;

        let search_index = search_index.clone();
        let db = db.clone();
        let query = query.to_string();
        let match_strategy = match_strategy.clone();
        let file_pattern_clone = file_pattern.clone();
        let language_clone = language.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<LineMatch>> {
            let index = match search_index.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Search index mutex poisoned, recovering: {}", poisoned);
                    poisoned.into_inner()
                }
            };
            let file_results = index.search_content(&query, &filter, fetch_limit)?;
            drop(index);

            let db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Database mutex poisoned, recovering: {}", poisoned);
                    poisoned.into_inner()
                }
            };

            let mut matches = Vec::new();
            for file_result in file_results {
                if matches.len() >= base_limit {
                    break;
                }

                // Apply file_pattern filter BEFORE expensive DB content retrieval
                if let Some(ref pattern) = file_pattern_clone {
                    if !matches_glob_pattern(&file_result.file_path, pattern) {
                        continue;
                    }
                }

                // Apply language filter BEFORE DB lookup (defense-in-depth; Tantivy also filters)
                if let Some(ref lang) = language_clone {
                    if !file_matches_language(&file_result.file_path, lang) {
                        continue;
                    }
                }

                if let Some(content) = db_lock.get_file_content(&file_result.file_path)? {
                    collect_line_matches(
                        &mut matches,
                        &content,
                        &file_result.file_path,
                        &match_strategy,
                        base_limit,
                    );
                }
            }

            Ok(matches)
        })
        .await??
    } else {
        // Search reference workspace with isolated Tantivy index + DB for content
        let tantivy_path = workspace_struct.workspace_tantivy_path(&target_workspace_id);
        let ref_db_path = workspace_struct.workspace_db_path(&target_workspace_id);
        let query_clone = query.to_string();
        let strategy = match_strategy.clone();
        let ref_file_pattern = file_pattern.clone();
        let ref_language = language.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<LineMatch>> {
            if !tantivy_path.join("meta.json").exists() {
                debug!("No Tantivy index for reference workspace, skipping");
                return Ok(Vec::new());
            }

            let ref_index = SearchIndex::open(&tantivy_path)?;
            let ref_filter = SearchFilter {
                language: ref_language.clone(),
                kind: None,
                file_pattern: ref_file_pattern.clone(),
            };
            let file_results =
                ref_index.search_content(&query_clone, &ref_filter, fetch_limit)?;

            if file_results.is_empty() || !ref_db_path.exists() {
                return Ok(Vec::new());
            }

            let ref_db = crate::database::SymbolDatabase::new(&ref_db_path)?;
            let mut matches = Vec::new();
            for file_result in file_results {
                if matches.len() >= base_limit {
                    break;
                }

                // Apply file_pattern filter BEFORE expensive DB content retrieval
                if let Some(ref pattern) = ref_file_pattern {
                    if !matches_glob_pattern(&file_result.file_path, pattern) {
                        continue;
                    }
                }

                // Apply language filter BEFORE DB lookup
                if let Some(ref lang) = ref_language {
                    if !file_matches_language(&file_result.file_path, lang) {
                        continue;
                    }
                }

                if let Some(content) = ref_db.get_file_content(&file_result.file_path)? {
                    collect_line_matches(
                        &mut matches,
                        &content,
                        &file_result.file_path,
                        &strategy,
                        base_limit,
                    );
                }
            }

            Ok(matches)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to spawn reference workspace search: {}", e))??
    };

    // Defense-in-depth: post-filter by language and file_pattern
    // (primary filtering now happens inside the collection loop above)
    let filtered_matches: Vec<LineMatch> = all_line_matches
        .into_iter()
        .filter(|line_match| {
            let language_match = language
                .as_ref()
                .map(|lang| file_matches_language(&line_match.file_path, lang))
                .unwrap_or(true);

            let file_match = file_pattern
                .as_ref()
                .map(|pattern| matches_glob_pattern(&line_match.file_path, pattern))
                .unwrap_or(true);

            language_match && file_match
        })
        .collect();

    if filtered_matches.is_empty() {
        let message = format!(
            "🔍 No lines found matching: '{}'\n\
            💡 Try a broader search term or different query",
            query
        );
        return Ok(CallToolResult::text_content(vec![Content::text(
            message,
        )]));
    }

    // Format results (single workspace search)
    let header = match &match_strategy {
        LineMatchStrategy::FileLevel { .. } => {
            let file_count = filtered_matches
                .iter()
                .map(|m| &m.file_path)
                .collect::<std::collections::HashSet<_>>()
                .len();
            format!(
                "📄 File-level search in [{}]: '{}' (found {} lines across {} files)",
                target_workspace_id, query, filtered_matches.len(), file_count
            )
        }
        _ => format!(
            "📄 Line-level search in [{}]: '{}' (found {} lines)",
            target_workspace_id, query, filtered_matches.len()
        ),
    };
    let mut lines = vec![header];
    lines.push(String::new());

    for entry in &filtered_matches {
        lines.push(format!(
            "{}:{}:{}",
            entry.file_path, entry.line_number, entry.line_content
        ));
    }

    Ok(CallToolResult::text_content(vec![Content::text(
        lines.join("\n"),
    )]))
}

/// Check if a file path matches the given language filter by extension.
fn file_matches_language(file_path: &str, lang: &str) -> bool {
    let path = std::path::Path::new(file_path);
    let Some(ext) = path.extension() else {
        return false;
    };
    let ext_str = ext.to_string_lossy().to_lowercase();
    match lang.to_lowercase().as_str() {
        "rust" => ext_str == "rs",
        "typescript" => ext_str == "ts" || ext_str == "tsx",
        "javascript" => ext_str == "js" || ext_str == "jsx" || ext_str == "mjs",
        "python" => ext_str == "py",
        "java" => ext_str == "java",
        "csharp" | "c#" => ext_str == "cs",
        "cpp" | "c++" => ext_str == "cpp" || ext_str == "cc" || ext_str == "cxx",
        "c" => ext_str == "c" || ext_str == "h",
        _ => ext_str == lang.to_lowercase(),
    }
}

/// Collect line matches from file content using the given strategy
fn collect_line_matches(
    destination: &mut Vec<LineMatch>,
    content: &str,
    file_path: &str,
    strategy: &LineMatchStrategy,
    max_results: usize,
) {
    for (line_idx, line) in content.lines().enumerate() {
        if line_matches(strategy, line) {
            destination.push(LineMatch {
                file_path: file_path.to_string(),
                line_number: line_idx + 1,
                line_content: line.trim_end_matches('\r').to_string(),
            });

            if destination.len() >= max_results {
                break;
            }
        }
    }
}
