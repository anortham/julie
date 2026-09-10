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

use julie_context::ToolContext;
use julie_index::snapshot::Snapshot;

use self::formatting::{BlastRadiusFormat, BlastRadiusHeader, format_blast_radius};
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
const LIKELY_TESTS_LIMIT: usize = 10;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct BlastRadiusTool {
    /// Symbol names or ids to seed the impact walk. Every definition with a matching name seeds the walk.
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
    /// Maximum relationship hops to walk outward from seed symbols.
    #[serde(
        default = "default_max_depth",
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub max_depth: u32,
    /// Maximum visible impact rows. Extra rows are dropped and reported by the truncation line.
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
    let target = handler
        .resolve_workspace_target(tool.workspace.as_deref())
        .await?;
    debug!("blast_radius: using workspace {:?}", target);
    let snapshot = handler.snapshot(&target).await?;
    run_with_snapshot(tool, &snapshot)
}

fn run_with_snapshot(tool: &BlastRadiusTool, snapshot: &Snapshot) -> Result<String> {
    match tool.mode.as_deref() {
        None | Some("default") | Some("web") => {}
        Some(other) => return Ok(format!("mode must be 'default' or 'web'; got '{other}'")),
    }
    let graph = snapshot.graph();
    let seed_context = seed::resolve_seed_context(tool, graph)?;
    let page_limit = tool.limit.max(1) as usize;
    let walk_budget = WalkBudget {
        max_frontier_per_depth: (page_limit * 10).clamp(100, 500),
    };
    let traversal_policy = if tool.mode.as_deref() == Some("web") {
        ImpactTraversalPolicy::Web
    } else {
        ImpactTraversalPolicy::Default
    };
    let (candidates, _walk_stats) = walk::walk_impacts_with_policy(
        graph,
        &seed_context.seed_symbols,
        tool.max_depth,
        walk_budget,
        traversal_policy,
    );
    let web_callers = if tool.mode.as_deref() == Some("web") {
        walk::walk_web_callers(snapshot, &seed_context.seed_symbols)?
    } else {
        Vec::new()
    };
    let ranked_impacts = ranking::rank_impacts(candidates, tool.include_tests);
    let likely_tests = if tool.include_tests {
        collect_likely_tests(graph, &seed_context, &ranked_impacts)
    } else {
        LikelyTests::default()
    };

    let visible_impacts: Vec<RankedImpact> =
        ranked_impacts.iter().take(page_limit).cloned().collect();
    // Unknown format values error instead of silently coercing, so typos fail loudly.
    let format = match tool.format.as_deref() {
        Some(value) => BlastRadiusFormat::parse_strict(value).map_err(|msg| anyhow!(msg))?,
        None => BlastRadiusFormat::Compact,
    };
    let impact_overflow = ranked_impacts.len() > page_limit;
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
    web_caller_rows.truncate(page_limit);

    let header = BlastRadiusHeader {
        impact_overflow,
        web_callers: web_caller_rows,
        web_callers_total,
    };

    Ok(format_blast_radius(
        &seed_context,
        &visible_impacts,
        &visible_likely_tests,
        format,
        header,
    ))
}
