pub mod formatting;
pub mod likely_tests;
pub mod ranking;
pub mod seed;
pub mod walk;

use anyhow::{Result, anyhow};
use julie_core::mcp_compat::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

use crate::navigation::resolution::WorkspaceTarget;
use crate::spillover::{SpilloverFormat, SpilloverStore};
use julie_context::ToolContext;
use julie_core::database::SymbolDatabase;

use self::formatting::{BlastRadiusHeader, format_blast_radius, impact_rows, store_list_overflow};
pub use self::likely_tests::LikelyTests;
use self::likely_tests::collect_likely_tests;
use self::ranking::RankedImpact;
use self::walk::{ImpactTraversalPolicy, WalkBudget};

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
        deserialize_with = "julie_core::serde_lenient::deserialize_vec_string_lenient"
    )]
    pub symbol_ids: Vec<String>,
    /// Changed files to use as seeds. Julie resolves current symbols in each path.
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_vec_string_lenient"
    )]
    pub file_paths: Vec<String>,
    /// Start Julie database revision number for a revision-range seed. Requires `to_revision`.
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_option_i64_lenient"
    )]
    pub from_revision: Option<i64>,
    /// End Julie database revision number. Requires `from_revision` and must be greater.
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_option_i64_lenient"
    )]
    pub to_revision: Option<i64>,
    /// Maximum relationship hops to walk outward from seed symbols.
    #[serde(
        default = "default_max_depth",
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub max_depth: u32,
    /// Maximum visible impact rows in the first response. Extra rows use spillover.
    #[serde(
        default = "default_limit",
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    /// Include likely tests and related test symbols when Julie can infer them.
    #[serde(
        default = "default_include_tests",
        deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient"
    )]
    pub include_tests: bool,
    /// Output format. Accepted values: `compact` and `readable`.
    #[serde(default)]
    pub format: Option<String>,
    /// Workspace target. Use `primary` or a workspace id opened through `manage_workspace`.
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Traversal mode. `default` (omitted) walks the stored relationship +
    /// identifier graph only — output is byte-identical to the legacy tool.
    /// `web` additionally surfaces reverse `http_call` edges so the blast
    /// radius of a route handler lists the frontend symbols that call it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Default for BlastRadiusTool {
    fn default() -> Self {
        Self {
            symbol_ids: Vec::new(),
            file_paths: Vec::new(),
            from_revision: None,
            to_revision: None,
            max_depth: default_max_depth(),
            limit: default_limit(),
            include_tests: default_include_tests(),
            format: None,
            workspace: default_workspace(),
            mode: None,
        }
    }
}

impl BlastRadiusTool {
    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        let result = run(self, handler).await?;
        Ok(CallToolResult::success(vec![Content::text(result)]))
    }
}

pub async fn run(tool: &BlastRadiusTool, handler: &dyn ToolContext) -> Result<String> {
    let workspace_target = handler
        .resolve_workspace_target(tool.workspace.as_deref())
        .await?;
    let spillover_store = handler.spillover_store();
    let session_id = handler.session_id().to_string();
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
    match tool.mode.as_deref() {
        None | Some("default") | Some("web") => {}
        Some(other) => return Ok(format!("mode must be 'default' or 'web'; got '{other}'")),
    }
    let seed_context = seed::resolve_seed_context(tool, db, workspace_id)?;
    let page_limit = tool.limit.max(1) as usize;
    let default_budget = WalkBudget::default();
    let walk_budget = WalkBudget {
        max_frontier_per_depth: (page_limit * 10).clamp(100, 500),
        max_identifier_fanout_per_name: default_budget.max_identifier_fanout_per_name,
    };
    let traversal_policy = if tool.mode.as_deref() == Some("web") {
        ImpactTraversalPolicy::Web
    } else {
        ImpactTraversalPolicy::Default
    };
    let (candidates, _walk_stats) = walk::walk_impacts_with_policy(
        db,
        &seed_context.seed_symbols,
        tool.max_depth,
        walk_budget,
        traversal_policy,
    )?;
    let web_callers = if tool.mode.as_deref() == Some("web") {
        let seed_ids: Vec<String> = seed_context
            .seed_symbols
            .iter()
            .map(|symbol| symbol.id.clone())
            .collect();
        walk::walk_web_callers(db, &seed_ids)?
    } else {
        Vec::new()
    };
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

    let mut web_caller_rows: Vec<String> = web_callers
        .iter()
        .map(|caller| {
            format!(
                "- {}  {}:{}  via {} {}",
                caller.impact.symbol.name,
                caller.impact.symbol.file_path,
                caller.impact.symbol.start_line,
                caller.via,
                caller.endpoint
            )
        })
        .collect();
    let web_callers_total = web_caller_rows.len();
    let web_callers_overflow_handle = store_list_overflow(
        spillover_store,
        session_id,
        "brwc",
        "Blast radius web callers overflow",
        &web_caller_rows,
        page_limit,
        format,
    );
    web_caller_rows.truncate(page_limit);

    let header = BlastRadiusHeader {
        revision_range: match (tool.from_revision, tool.to_revision) {
            (Some(from), Some(to)) => Some((from, to)),
            _ => None,
        },
        deleted_files_path_only: !seed_context.deleted_files.is_empty(),
        impact_overflow_handle,
        likely_test_paths_overflow_handle,
        related_test_symbols_overflow_handle,
        web_callers: web_caller_rows,
        web_callers_overflow_handle,
        web_callers_total,
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
