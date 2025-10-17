//! Line-mode (grep-style) search
//!
//! Provides grep-style line-by-line searching with file paths and line numbers.
//! Used when output format is set to "lines".

use anyhow::Result;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use tracing::debug;

use crate::handler::JulieServerHandler;

use super::query::{line_match_strategy, line_matches, matches_glob_pattern, preprocess_fallback_query};
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
                None => {
                    return Err(anyhow::anyhow!(
                        "Workspace '{}' not found",
                        ws_id
                    ))
                }
            }
        }
        _ => primary_workspace_id.clone(),
    };

    let processed_query = preprocess_fallback_query(query);
    let match_strategy = line_match_strategy(query);
    let base_limit = limit.max(1) as usize;
    let fetch_limit = base_limit.saturating_mul(5);

    // Search the single target workspace
    let all_line_matches: Vec<LineMatch> = if target_workspace_id == primary_workspace_id {
        // Search primary workspace using shared connection
        tokio::task::block_in_place(|| -> Result<Vec<LineMatch>> {
            let db_lock = db.lock().unwrap();
            let file_results = db_lock.search_file_content_fts(
                &processed_query,
                Some(&primary_workspace_id),
                fetch_limit,
            )?;

            let mut matches = Vec::new();
            for file_result in file_results {
                if matches.len() >= base_limit {
                    break;
                }

                if let Some(content) = db_lock
                    .get_file_content(&file_result.path, Some(&primary_workspace_id))?
                {
                    collect_line_matches(
                        &mut matches,
                        &content,
                        &file_result.path,
                        &primary_workspace_id,
                        &match_strategy,
                        base_limit,
                    );
                }
            }

            Ok(matches)
        })?
    } else {
        // Search reference workspace with isolated connection
        let ref_db_path = workspace_struct.workspace_db_path(&target_workspace_id);
        let query_clone = processed_query.clone();
        let strategy = match_strategy.clone();
        let workspace_id_clone = target_workspace_id.clone();

        tokio::task::spawn_blocking(move || -> Result<Vec<LineMatch>> {
            if !ref_db_path.exists() {
                return Ok(Vec::new());
            }

            let ref_db = crate::database::SymbolDatabase::new(&ref_db_path)?;
            let file_results = ref_db.search_file_content_fts(
                &query_clone,
                Some(&workspace_id_clone),
                fetch_limit,
            )?;

            let mut matches = Vec::new();
            for file_result in file_results {
                if matches.len() >= base_limit {
                    break;
                }

                if let Some(content) = ref_db
                    .get_file_content(&file_result.path, Some(&workspace_id_clone))?
                {
                    collect_line_matches(
                        &mut matches,
                        &content,
                        &file_result.path,
                        &workspace_id_clone,
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

    // Apply language and file pattern filtering
    let filtered_matches: Vec<LineMatch> = all_line_matches
        .into_iter()
        .filter(|line_match| {
            // Apply language filter if specified
            let language_match = if let Some(ref lang) = language {
                // Extract file extension and match against language
                let path = std::path::Path::new(&line_match.file_path);
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    // Simple extension matching - could be enhanced with language detection
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
                } else {
                    false
                }
            } else {
                true // No language filter, accept all
            };

            // Apply file pattern filter if specified
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
        return Ok(CallToolResult::text_content(vec![TextContent::from(
            message,
        )]));
    }

    // Format results (single workspace search)
    let mut lines = vec![format!(
        "📄 Line-level search in [{}]: '{}' (found {} lines)",
        target_workspace_id,
        query,
        filtered_matches.len()
    )];
    lines.push(String::new());

    for entry in &filtered_matches {
        lines.push(format!(
            "{}:{}:{}",
            entry.file_path, entry.line_number, entry.line_content
        ));
    }

    Ok(CallToolResult::text_content(vec![TextContent::from(
        lines.join("\n"),
    )]))
}

/// Collect line matches from file content using the given strategy
fn collect_line_matches(
    destination: &mut Vec<LineMatch>,
    content: &str,
    file_path: &str,
    _workspace_id: &str,
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
