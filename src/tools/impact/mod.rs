pub mod formatting;
pub mod likely_tests;
pub mod ranking;
pub mod seed;
pub mod walk;

use anyhow::{Result, anyhow};
use rmcp::model::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::tools::navigation::resolution::{WorkspaceTarget, resolve_workspace_filter};
use crate::tools::spillover::{SpilloverFormat, SpilloverStore};

use self::formatting::{BlastRadiusHeader, format_blast_radius, impact_rows, store_list_overflow};
pub use self::likely_tests::LikelyTests;
use self::likely_tests::collect_likely_tests;
use self::ranking::RankedImpact;
use self::walk::WalkBudget;

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

/// Cap on visible paths/names under Likely tests / Related test symbols.
/// Overflow entries are stored in spillover pages.
const LIKELY_TESTS_LIMIT: usize = 10;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BlastRadiusTool {
    /// Symbol ids to seed the impact walk. Use ids from search or navigation tools.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_vec_string_lenient"
    )]
    pub symbol_ids: Vec<String>,
    /// Changed files to use as seeds. Julie resolves current symbols in each path.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_vec_string_lenient"
    )]
    pub file_paths: Vec<String>,
    /// Start Julie database revision number for a revision-range seed. Requires `to_revision`.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_i64_lenient"
    )]
    pub from_revision: Option<i64>,
    /// End Julie database revision number. Requires `from_revision` and must be greater.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_i64_lenient"
    )]
    pub to_revision: Option<i64>,
    /// Maximum relationship hops to walk outward from seed symbols.
    #[serde(
        default = "default_max_depth",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub max_depth: u32,
    /// Maximum visible impact rows in the first response. Extra rows use spillover.
    #[serde(
        default = "default_limit",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    /// Include likely tests and related test symbols when Julie can infer them.
    #[serde(
        default = "default_include_tests",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub include_tests: bool,
    /// Output format. Accepted values: `compact` and `readable`.
    #[serde(default)]
    pub format: Option<String>,
    /// Workspace target. Use `primary` or a workspace id opened through `manage_workspace`.
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
}

impl BlastRadiusTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let result = run(self, handler).await?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
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
            // Pooled DB: read-only, no mutation gate required.
            let pooled_db = handler
                .get_pooled_database_for_workspace(&target_workspace_id)
                .await?;

            tokio::task::spawn_blocking(move || {
                let pooled_db = pooled_db.into_read_snapshot()?;
                run_with_db(
                    &tool,
                    &pooled_db,
                    &target_workspace_id,
                    &spillover_store,
                    &session_id,
                )
            })
            .await?
        }
        WorkspaceTarget::Primary => {
            let db = handler.primary_pooled_database().await?;
            let workspace_id = handler.require_primary_workspace_identity()?;

            tokio::task::spawn_blocking(move || {
                let db_guard = db.into_read_snapshot()?;
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
    spillover_store: &SpilloverStore,
    session_id: &str,
) -> Result<String> {
    let seed_context = seed::resolve_seed_context(tool, db, workspace_id)?;
    let page_limit = tool.limit.max(1) as usize;
    let default_budget = WalkBudget::default();
    let walk_budget = WalkBudget {
        max_frontier_per_depth: (page_limit * 10).clamp(100, 500),
        max_identifier_fanout_per_name: default_budget.max_identifier_fanout_per_name,
    };
    let (candidates, _walk_stats) = walk::walk_impacts_with_budget(
        db,
        &seed_context.seed_symbols,
        tool.max_depth,
        walk_budget,
    )?;
    let ranked_impacts = ranking::rank_impacts(candidates, tool.include_tests);
    let likely_tests = if tool.include_tests {
        collect_likely_tests(db, &seed_context, &ranked_impacts)?
    } else {
        LikelyTests::default()
    };

    let visible_impacts: Vec<RankedImpact> =
        ranked_impacts.iter().take(page_limit).cloned().collect();
    // Keep first-page and overflow-page formats aligned. Compact is the
    // denser default for agent-mediated tool chains. Unknown values error
    // instead of silently coercing, so typos fail loudly.
    let format = match tool.format.as_deref() {
        Some(value) => SpilloverFormat::parse_strict(value).map_err(|msg| anyhow!(msg))?,
        None => SpilloverFormat::Compact,
    };
    let impact_overflow_handle = if ranked_impacts.len() > page_limit {
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
    let likely_test_paths_overflow_handle = store_list_overflow(
        spillover_store,
        session_id,
        "brltp",
        "Blast radius likely-test paths overflow",
        &likely_tests.likely_test_paths,
        LIKELY_TESTS_LIMIT,
        format,
    );
    let related_test_symbols_overflow_handle = store_list_overflow(
        spillover_store,
        session_id,
        "brlts",
        "Blast radius related test symbols overflow",
        &likely_tests.related_test_symbols,
        LIKELY_TESTS_LIMIT,
        format,
    );
    let visible_likely_tests = likely_tests.visible(LIKELY_TESTS_LIMIT);

    let header = BlastRadiusHeader {
        revision_range: match (tool.from_revision, tool.to_revision) {
            (Some(from), Some(to)) => Some((from, to)),
            _ => None,
        },
        deleted_files_path_only: !seed_context.deleted_files.is_empty(),
        impact_overflow_handle,
        likely_test_paths_overflow_handle,
        related_test_symbols_overflow_handle,
    };

    Ok(format_blast_radius(
        &seed_context,
        &visible_impacts,
        &visible_likely_tests,
        &seed_context.deleted_files,
        format,
        header,
    ))
}
