pub mod formatting;
pub mod git_seed;
pub mod likely_tests;
pub mod ranking;
pub mod seed;
pub mod walk;

use anyhow::{Result, anyhow};
use julie_core::mcp_compat::{CallToolResult, Content};
use schemars::JsonSchema;
use serde::Deserialize;
use tracing::debug;

use julie_context::{ToolContext, WorkspaceTarget};
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
    /// Maximum visible impact rows. Extra rows are dropped and reported by a next: line.
    #[serde(
        default = "default_limit",
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    /// Skip this many impact rows before keeping `limit` rows (default: 0)
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_u32_lenient"
    )]
    pub offset: u32,
    /// Include likely tests and related test symbols when Julie can infer them.
    #[serde(
        default = "default_include_tests",
        deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient"
    )]
    pub include_tests: bool,
    /// Output format. Accepted values: `compact` and `readable`.
    #[serde(default)]
    pub format: Option<String>,
    /// Required workspace target: a workspace ID or absolute workspace path for MCP calls.
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Traversal mode. `default` (omitted) walks the stored relationship +
    /// identifier graph only — output is byte-identical to the legacy tool.
    /// `web` additionally surfaces reverse `http_call` edges so the blast
    /// radius of a route handler lists the frontend symbols that call it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Seed from the working-tree git diff. Default when symbol_ids and file_paths are both empty.
    #[serde(
        default,
        deserialize_with = "julie_core::serde_lenient::deserialize_bool_lenient"
    )]
    pub git: bool,
}

impl Default for BlastRadiusTool {
    fn default() -> Self {
        Self {
            symbol_ids: Vec::new(),
            file_paths: Vec::new(),
            max_depth: default_max_depth(),
            limit: default_limit(),
            offset: 0,
            include_tests: default_include_tests(),
            format: None,
            workspace: default_workspace(),
            mode: None,
            git: false,
        }
    }
}

impl BlastRadiusTool {
    pub fn seeds_from_git(&self) -> bool {
        self.git || (self.symbol_ids.is_empty() && self.file_paths.is_empty())
    }

    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        self.call_tool_counted(handler)
            .await
            .map(|(result, _)| result)
    }

    pub async fn call_tool_counted(
        &self,
        handler: &dyn ToolContext,
    ) -> Result<(CallToolResult, u32)> {
        let (text, count) = run(self, handler).await?;
        Ok((CallToolResult::success(vec![Content::text(text)]), count))
    }
}

pub async fn run(tool: &BlastRadiusTool, handler: &dyn ToolContext) -> Result<(String, u32)> {
    let target = handler
        .resolve_workspace_target(tool.workspace.as_deref())
        .await?;
    debug!("blast_radius: using workspace {:?}", target);
    let mut seeded = tool.clone();
    if seeded.seeds_from_git() {
        let root = match &target {
            WorkspaceTarget::Primary => handler.require_primary_workspace_root()?,
            WorkspaceTarget::Target(id) => handler.get_workspace_root_for_target(id).await?,
        };
        seeded.file_paths = git_seed::changed_files(&root)?;
        seeded.git = true;
        if seeded.file_paths.is_empty() {
            return Ok(("No changed files in the working tree.".to_string(), 0));
        }
    }
    let snapshot = handler.snapshot(&target).await?;
    run_with_snapshot(&seeded, &snapshot)
}

fn run_with_snapshot(tool: &BlastRadiusTool, snapshot: &Snapshot) -> Result<(String, u32)> {
    match tool.mode.as_deref() {
        None | Some("default") | Some("web") => {}
        Some(other) => {
            return Ok((format!("mode must be 'default' or 'web'; got '{other}'"), 0));
        }
    }
    let graph = snapshot.graph();
    let seed_context = seed::resolve_seed_context(tool, graph)?;
    let page_limit = tool.limit.max(1) as usize;
    let offset = tool.offset as usize;
    let walk_budget = WalkBudget {
        max_frontier_per_depth: ((page_limit + offset) * 10).clamp(100, 500),
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

    let visible_impacts: Vec<RankedImpact> = ranked_impacts
        .iter()
        .skip(offset)
        .take(page_limit)
        .cloned()
        .collect();
    // Unknown format values error instead of silently coercing, so typos fail loudly.
    let format = match tool.format.as_deref() {
        Some(value) => BlastRadiusFormat::parse_strict(value).map_err(|msg| anyhow!(msg))?,
        None => BlastRadiusFormat::Compact,
    };
    let impact_overflow = ranked_impacts.len() > offset + page_limit;
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
    web_caller_rows = web_caller_rows
        .into_iter()
        .skip(offset)
        .take(page_limit)
        .collect();

    let file_paths = tool.file_paths.join(",");
    let symbol_ids = tool.symbol_ids.join(",");
    let seed_args: Vec<(&str, &str)> = if tool.seeds_from_git() {
        vec![("git", "true")]
    } else if !tool.file_paths.is_empty() {
        vec![("file_paths", file_paths.as_str())]
    } else if !tool.symbol_ids.is_empty() {
        vec![("symbol_ids", symbol_ids.as_str())]
    } else {
        vec![]
    };
    let next = impact_overflow.then(|| {
        crate::shared::next_line("blast_radius", &seed_args, offset + visible_impacts.len())
    });

    let header = BlastRadiusHeader {
        impact_overflow,
        web_callers: web_caller_rows,
        web_callers_total,
        next,
    };

    let count = visible_impacts.len() as u32;
    Ok((
        format_blast_radius(
            &seed_context,
            &visible_impacts,
            &visible_likely_tests,
            format,
            header,
        ),
        count,
    ))
}
