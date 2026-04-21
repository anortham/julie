pub mod formatting;
pub mod ranking;
pub mod seed;
pub mod walk;

use std::collections::{HashMap, HashSet};

use anyhow::{Result, anyhow};
use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

use crate::analysis::test_linkage::test_linkage_entry;
use crate::database::{IdentifierRef, SymbolDatabase};
use crate::handler::JulieServerHandler;
use crate::search::scoring::is_test_path;
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};
use crate::tools::spillover::SpilloverFormat;

use self::formatting::{format_blast_radius, impact_rows};
use self::ranking::RankedImpact;
use self::seed::SeedContext;

fn default_max_depth() -> u32 {
    2
}

fn default_limit() -> u32 {
    12
}

fn default_include_tests() -> bool {
    true
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

/// Cap on how many paths/names we surface under Likely tests / Related test
/// symbols. Shared across both collections so formatter output stays bounded.
const LIKELY_TESTS_LIMIT: usize = 10;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BlastRadiusTool {
    #[serde(default)]
    pub symbol_ids: Vec<String>,
    #[serde(default)]
    pub file_paths: Vec<String>,
    #[serde(default)]
    pub from_revision: Option<i64>,
    #[serde(default)]
    pub to_revision: Option<i64>,
    #[serde(
        default = "default_max_depth",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub max_depth: u32,
    #[serde(
        default = "default_limit",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    #[serde(default = "default_include_tests")]
    pub include_tests: bool,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl BlastRadiusTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let result = run(self, handler).await?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}

/// Bundle of test evidence surfaced next to a blast radius report.
///
/// Two collections instead of one mixed list: paths drive navigation,
/// symbol names are supplementary context. Keeps the formatter honest about
/// what "Likely tests" means.
#[derive(Debug, Default, Clone)]
pub struct LikelyTests {
    pub likely_test_paths: Vec<String>,
    pub related_test_symbols: Vec<String>,
}

impl LikelyTests {
    pub fn is_empty(&self) -> bool {
        self.likely_test_paths.is_empty() && self.related_test_symbols.is_empty()
    }
}

pub async fn run(tool: &BlastRadiusTool, handler: &JulieServerHandler) -> Result<String> {
    let workspace_target = resolve_workspace_filter(tool.workspace.as_deref(), handler).await?;
    let spillover_store = handler.spillover_store.clone();
    let session_id = handler.session_metrics.session_id.clone();
    let tool = tool.clone();

    match workspace_target {
        WorkspaceTarget::Target(target_workspace_id) => {
            debug!("blast_radius: using workspace {}", target_workspace_id);
            let db = handler
                .get_database_for_workspace(&target_workspace_id)
                .await?;

            tokio::task::spawn_blocking(move || {
                let db_guard = db
                    .lock()
                    .map_err(|e| anyhow!("Database lock error: {}", e))?;
                run_with_db(
                    &tool,
                    &db_guard,
                    &target_workspace_id,
                    &spillover_store,
                    &session_id,
                )
            })
            .await?
        }
        WorkspaceTarget::Primary => {
            let db = handler.primary_database().await?;
            let workspace_id = handler.require_primary_workspace_identity()?;

            tokio::task::spawn_blocking(move || {
                let db_guard = db
                    .lock()
                    .map_err(|e| anyhow!("Database lock error: {}", e))?;
                run_with_db(
                    &tool,
                    &db_guard,
                    &workspace_id,
                    &spillover_store,
                    &session_id,
                )
            })
            .await?
        }
    }
}

fn run_with_db(
    tool: &BlastRadiusTool,
    db: &SymbolDatabase,
    workspace_id: &str,
    spillover_store: &crate::tools::spillover::SpilloverStore,
    session_id: &str,
) -> Result<String> {
    let seed_context = seed::resolve_seed_context(tool, db, workspace_id)?;
    let candidates = walk::walk_impacts(db, &seed_context.seed_symbols, tool.max_depth)?;
    let ranked_impacts = ranking::rank_impacts(candidates, tool.include_tests);
    let likely_tests = if tool.include_tests {
        collect_likely_tests(db, &seed_context, &ranked_impacts)?
    } else {
        LikelyTests::default()
    };

    let page_limit = tool.limit.max(1) as usize;
    let visible_impacts: Vec<RankedImpact> =
        ranked_impacts.iter().take(page_limit).cloned().collect();
    // Default blast_radius to compact when caller left format unset. Compact
    // is the denser output and the better fit for agent-mediated tool chains;
    // spillover_get keeps its readable default via SpilloverFormat::from_option
    // (used by other tools).
    let format = match tool.format.as_deref() {
        Some(value) => SpilloverFormat::from_option(Some(value)),
        None => SpilloverFormat::Compact,
    };
    let overflow_handle = if ranked_impacts.len() > page_limit {
        spillover_store.store_rows(
            session_id,
            "br",
            "Blast radius overflow",
            impact_rows(&ranked_impacts[page_limit..], page_limit + 1),
            0,
            page_limit,
            format,
        )
    } else {
        None
    };

    Ok(format_blast_radius(
        &seed_context,
        &visible_impacts,
        &likely_tests,
        &seed_context.deleted_files,
        overflow_handle.as_deref(),
        format,
    ))
}

fn collect_likely_tests(
    db: &SymbolDatabase,
    seed_context: &SeedContext,
    impacts: &[RankedImpact],
) -> Result<LikelyTests> {
    let mut tests = LikelyTests::default();

    let relevant_symbols: Vec<_> = seed_context
        .seed_symbols
        .iter()
        .chain(impacts.iter().map(|impact| &impact.symbol))
        .collect();
    let seed_ids: HashSet<String> = seed_context
        .seed_symbols
        .iter()
        .map(|symbol| symbol.id.clone())
        .collect();

    // Tier 1: metadata-declared linkage from test_linkage / test_coverage.
    // Paths go into likely_test_paths, bare names into related_test_symbols.
    let mut seen_paths = HashSet::new();
    let mut seen_names = HashSet::new();
    for symbol in &relevant_symbols {
        if let Some(linkage) = symbol.metadata.as_ref().and_then(|metadata| {
            let value = serde_json::to_value(metadata).ok()?;
            test_linkage_entry(&value).cloned()
        }) {
            if let Some(linked_test_paths) = linkage
                .get("linked_test_paths")
                .and_then(|value| value.as_array())
            {
                for linked_test_path in linked_test_paths.iter().filter_map(|value| value.as_str())
                {
                    push_unique(
                        &mut tests.likely_test_paths,
                        &mut seen_paths,
                        linked_test_path.to_string(),
                    );
                }
            }
            if let Some(linked_tests) = linkage
                .get("linked_tests")
                .and_then(|value| value.as_array())
            {
                for linked_test in linked_tests.iter().filter_map(|value| value.as_str()) {
                    push_unique(
                        &mut tests.related_test_symbols,
                        &mut seen_names,
                        linked_test.to_string(),
                    );
                }
            }
        }
    }

    if !tests.is_empty() {
        truncate(&mut tests);
        return Ok(tests);
    }

    // Tier 2: relationships table — any test symbol that calls/uses the
    // relevant symbols. Yields test file paths.
    let symbol_ids: Vec<String> = relevant_symbols
        .iter()
        .map(|symbol| symbol.id.clone())
        .collect();
    let relationship_tests = db.get_relationships_to_symbols(&symbol_ids)?;
    let mut from_ids: Vec<String> = relationship_tests
        .iter()
        .map(|relationship| relationship.from_symbol_id.clone())
        .collect();
    from_ids.sort();
    from_ids.dedup();
    let mut from_symbols = db.get_symbols_by_ids(&from_ids)?;
    from_symbols.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then_with(|| a.id.cmp(&b.id))
    });
    for symbol in from_symbols {
        if is_test_symbol(&symbol) {
            push_unique(
                &mut tests.likely_test_paths,
                &mut seen_paths,
                symbol.file_path.clone(),
            );
        }
    }

    if !tests.likely_test_paths.is_empty() {
        truncate(&mut tests);
        return Ok(tests);
    }

    // Tier 3: identifiers table. First pass — resolved matches where
    // target_symbol_id points at a seed. Those are much higher signal than
    // name-only matches. If any resolved matches exist, we use ONLY them so
    // the result stays tight.
    let relevant_names: Vec<String> = relevant_symbols
        .iter()
        .map(|symbol| symbol.name.clone())
        .collect();
    let mut identifier_refs = db.get_identifiers_by_names(&relevant_names)?;

    // Drop rows whose container is a seed — a seed "calling itself" via its
    // own name is noise.
    identifier_refs.retain(|iref| {
        iref.containing_symbol_id
            .as_ref()
            .is_none_or(|id| !seed_ids.contains(id))
    });

    let resolved_refs: Vec<IdentifierRef> = identifier_refs
        .iter()
        .filter(|iref| {
            iref.target_symbol_id
                .as_ref()
                .is_some_and(|target| seed_ids.contains(target))
        })
        .cloned()
        .collect();

    let mut working_refs = if resolved_refs.is_empty() {
        identifier_refs
    } else {
        resolved_refs
    };
    sort_identifier_refs(&mut working_refs);

    let containing_ids: Vec<String> = working_refs
        .iter()
        .filter_map(|identifier| identifier.containing_symbol_id.clone())
        .collect();
    let containing_symbols = db.get_symbols_by_ids(&containing_ids)?;
    let containing_map: HashMap<String, crate::extractors::Symbol> = containing_symbols
        .into_iter()
        .map(|symbol| (symbol.id.clone(), symbol))
        .collect();

    for identifier in working_refs {
        let containing_symbol = identifier
            .containing_symbol_id
            .as_ref()
            .and_then(|id| containing_map.get(id));
        if containing_symbol.is_some_and(is_test_symbol) || is_test_path(&identifier.file_path) {
            let test_path = containing_symbol
                .map(|symbol| symbol.file_path.clone())
                .unwrap_or_else(|| identifier.file_path.clone());
            push_unique(&mut tests.likely_test_paths, &mut seen_paths, test_path);
            if let Some(symbol) = containing_symbol {
                push_unique(
                    &mut tests.related_test_symbols,
                    &mut seen_names,
                    symbol.name.clone(),
                );
            }
        }
    }

    if !tests.is_empty() {
        truncate(&mut tests);
        return Ok(tests);
    }

    // Tier 4: stem-matching fallback. Walk the file index in deterministic
    // order and flag test files whose name shares a stem with any relevant
    // symbol's source file. Paths only (no symbol names).
    let mut stmt = db.conn.prepare("SELECT path FROM files ORDER BY path")?;
    let file_rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let file_stems: HashSet<String> = relevant_symbols
        .iter()
        .filter_map(|symbol| symbol.file_path.rsplit('/').next())
        .filter_map(|file_name| file_name.split('.').next())
        .map(|stem| stem.to_ascii_lowercase())
        .collect();

    for row in file_rows {
        let path = row?;
        if !is_test_path(&path) {
            continue;
        }
        let matches_stem = path
            .rsplit('/')
            .next()
            .map(|file_name| file_name.to_ascii_lowercase())
            .is_some_and(|file_name| file_stems.iter().any(|stem| file_name.contains(stem)));
        if matches_stem {
            push_unique(&mut tests.likely_test_paths, &mut seen_paths, path);
        }
    }

    truncate(&mut tests);
    Ok(tests)
}

fn push_unique(values: &mut Vec<String>, seen: &mut HashSet<String>, candidate: String) {
    if seen.insert(candidate.clone()) {
        values.push(candidate);
    }
}

fn truncate(tests: &mut LikelyTests) {
    tests.likely_test_paths.truncate(LIKELY_TESTS_LIMIT);
    tests.related_test_symbols.truncate(LIKELY_TESTS_LIMIT);
}

fn sort_identifier_refs(refs: &mut [IdentifierRef]) {
    refs.sort_by(|a, b| {
        // Confidence descending, then file_path ascending, then
        // containing_symbol_id ascending (break ties deterministically).
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| {
                let left = a.containing_symbol_id.as_deref().unwrap_or("");
                let right = b.containing_symbol_id.as_deref().unwrap_or("");
                left.cmp(right)
            })
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
}

fn is_test_symbol(symbol: &crate::extractors::Symbol) -> bool {
    symbol
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("is_test"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || is_test_path(&symbol.file_path)
}
