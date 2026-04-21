use serde::Serialize;

use crate::extractors::Symbol;

use super::types::LineMatch;

#[derive(Debug, Clone)]
pub enum SearchHitBacking {
    Symbol(Symbol),
    LineMatch(LineMatch),
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub name: String,
    pub file: String,
    pub line: Option<u32>,
    pub kind: String,
    pub language: String,
    pub score: f32,
    pub snippet: Option<String>,
    pub workspace: String,
    pub symbol_id: Option<String>,
    #[serde(skip_serializing)]
    pub backing: SearchHitBacking,
}

impl SearchHit {
    pub fn from_symbol(symbol: Symbol, workspace: String) -> Self {
        let line = Some(symbol.start_line);
        let score = symbol.confidence.unwrap_or(0.0);
        let snippet = symbol
            .signature
            .clone()
            .or(symbol.doc_comment.clone())
            .or(symbol.code_context.clone());
        let symbol_id = Some(symbol.id.clone());
        let name = symbol.name.clone();
        let file = symbol.file_path.clone();
        let kind = symbol.kind.to_string();
        let language = symbol.language.clone();

        Self {
            name,
            file,
            line,
            kind,
            language,
            score,
            snippet,
            workspace,
            symbol_id,
            backing: SearchHitBacking::Symbol(symbol),
        }
    }

    pub fn from_line_match(
        line_match: LineMatch,
        workspace: String,
        language: String,
        score: f32,
    ) -> Self {
        let filename = line_match
            .file_path
            .rsplit('/')
            .next()
            .unwrap_or(&line_match.file_path)
            .to_string();
        let file = line_match.file_path.clone();
        let line = Some(line_match.line_number as u32);
        let snippet = Some(line_match.line_content.clone());

        Self {
            name: filename,
            file,
            line,
            kind: "line".to_string(),
            language,
            score,
            snippet,
            workspace,
            symbol_id: None,
            backing: SearchHitBacking::LineMatch(line_match),
        }
    }

    pub fn as_symbol(&self) -> Option<&Symbol> {
        match &self.backing {
            SearchHitBacking::Symbol(symbol) => Some(symbol),
            SearchHitBacking::LineMatch(_) => None,
        }
    }

    pub fn as_line_match(&self) -> Option<&LineMatch> {
        match &self.backing {
            SearchHitBacking::Symbol(_) => None,
            SearchHitBacking::LineMatch(line_match) => Some(line_match),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHitSummary {
    pub rank: usize,
    pub symbol_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line: Option<u32>,
    pub score: f32,
    pub workspace: String,
}

/// Attribution for a zero-hit search outcome. Populated by per-stage
/// instrumentation (Task 4) and surfaced through `SearchTrace.zero_hit_reason`
/// so telemetry can classify empty results without log-scraping.
///
/// `Promoted` is set on the original (content) leg of a content→definitions
/// auto-promotion: the content search returned zero hits, which triggered the
/// promotion into the `Promoted` composite.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ZeroHitReason {
    /// Tantivy returned no candidate documents before any filtering.
    TantivyNoCandidates,
    /// Candidates existed but were eliminated by the `file_pattern` filter.
    FilePatternFiltered,
    /// Candidates existed but were eliminated by the `language` filter.
    LanguageFiltered,
    /// Candidates existed but were eliminated by the `exclude_tests` filter.
    TestFiltered,
    /// File content could not be loaded (blob missing, storage unavailable).
    FileContentUnavailable,
    /// Line-level post-processing did not match any content lines.
    LineMatchMiss,
    /// Zero hits on the requested leg were promoted to a fallback leg; the
    /// original zero-hit branch carries this reason.
    Promoted,
}

/// Categorizes the kind of hint prepended to an agent-facing search response.
/// Persisted through `SearchTrace.hint_kind` so the "without-recourse" rate is
/// measurable from `tool_calls.metadata`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HintKind {
    /// Multi-token content search produced zero hits; formatter prepended a
    /// per-token breakdown hint (Task 8).
    MultiTokenHint,
    /// Definitions search produced hits only outside the requested file scope.
    OutOfScopeDefinitionHint,
    /// `file_pattern` contains commas; formatter prepended a glob-syntax hint.
    CommaGlobHint,
}

/// Summarizes a content→definitions auto-promotion for trace consumers and
/// telemetry. The effective result count is `SearchTrace::result_count` on the
/// enclosing trace; storing it here would be redundant.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PromotionInfo {
    pub requested_target: String,
    pub effective_target: String,
    pub requested_result_count: usize,
    pub promotion_reason: String,
}

/// Trace describing the executed search strategy, result count, and diagnostic
/// metadata.
///
/// The following fields are populated by downstream stages and default to
/// `None`:
///
/// - `promoted` is set by the single-identifier auto-promotion constructor
///   (`SearchExecutionResult::new_promoted`, Task 7).
/// - `zero_hit_reason` is set by per-stage attribution (Task 4) when
///   `result_count == 0`. Paths that intentionally leave it `None`:
///     * single-identifier definitions search where the symbol genuinely does
///       not exist anywhere in the index,
///     * content hit with `result_count > 0`,
///     * definitions search that returned hits.
/// - `hint_kind` is set by the multi-token zero-hit hint formatter (Task 8),
///   the out-of-scope definition hint, and the comma-glob hint. It stays
///   `None` for responses that carry no prepended hint.
#[derive(Debug, Clone, Serialize)]
pub struct SearchTrace {
    pub strategy_id: String,
    pub result_count: usize,
    pub top_hits: Vec<SearchHitSummary>,
    pub promoted: Option<PromotionInfo>,
    pub zero_hit_reason: Option<ZeroHitReason>,
    pub hint_kind: Option<HintKind>,
}

impl SearchTrace {
    pub fn from_hits(strategy_id: impl Into<String>, hits: &[SearchHit]) -> Self {
        let strategy_id = strategy_id.into();
        let top_hits = hits
            .iter()
            .take(3)
            .enumerate()
            .map(|(idx, hit)| SearchHitSummary {
                rank: idx + 1,
                symbol_id: hit.symbol_id.clone(),
                name: hit.name.clone(),
                kind: hit.kind.clone(),
                file: hit.file.clone(),
                line: hit.line,
                score: hit.score,
                workspace: hit.workspace.clone(),
            })
            .collect();

        Self {
            strategy_id,
            result_count: hits.len(),
            top_hits,
            promoted: None,
            zero_hit_reason: None,
            hint_kind: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SearchExecutionKind {
    Definitions,
    Content {
        workspace_label: Option<String>,
        file_level: bool,
    },
    /// Composite variant for content→definitions auto-promotions. The caller
    /// originally requested `requested_target` (typically `"content"`) but the
    /// first leg returned zero hits, so the executor transparently ran a
    /// second leg against `effective_target` (typically `"definitions"`) and
    /// returned those hits instead. Both inner kinds are preserved so
    /// downstream consumers (formatter, dashboard, telemetry) can inspect or
    /// render either side.
    Promoted {
        requested_target: String,
        effective_target: String,
        requested_result_count: usize,
        effective_result_count: usize,
        promotion_reason: String,
        inner_content: Box<SearchExecutionKind>,
        inner_definitions: Box<SearchExecutionKind>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchExecutionResult {
    pub hits: Vec<SearchHit>,
    pub relaxed: bool,
    pub total_results: usize,
    pub trace: SearchTrace,
    #[serde(skip_serializing)]
    pub kind: SearchExecutionKind,
}

impl SearchExecutionResult {
    pub fn new(
        hits: Vec<SearchHit>,
        relaxed: bool,
        total_results: usize,
        strategy_id: impl Into<String>,
        kind: SearchExecutionKind,
    ) -> Self {
        let trace = SearchTrace::from_hits(strategy_id, &hits);
        Self {
            hits,
            relaxed,
            total_results,
            trace,
            kind,
        }
    }

    /// Construct a `SearchExecutionResult` for a content→definitions
    /// auto-promotion. Wraps the inner content and definitions execution kinds
    /// in a composite `Promoted` variant and records the promotion metadata on
    /// `trace.promoted`. `hits` are the effective hits that will be shown to
    /// the agent (typically the definitions leg results). The effective result
    /// count is derived from `hits.len()`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_promoted(
        hits: Vec<SearchHit>,
        relaxed: bool,
        total_results: usize,
        strategy_id: impl Into<String>,
        requested_target: impl Into<String>,
        effective_target: impl Into<String>,
        requested_result_count: usize,
        promotion_reason: impl Into<String>,
        inner_content: SearchExecutionKind,
        inner_definitions: SearchExecutionKind,
    ) -> Self {
        let requested_target = requested_target.into();
        let effective_target = effective_target.into();
        let promotion_reason = promotion_reason.into();
        let effective_result_count = hits.len();

        let mut trace = SearchTrace::from_hits(strategy_id, &hits);
        trace.promoted = Some(PromotionInfo {
            requested_target: requested_target.clone(),
            effective_target: effective_target.clone(),
            requested_result_count,
            promotion_reason: promotion_reason.clone(),
        });

        let kind = SearchExecutionKind::Promoted {
            requested_target,
            effective_target,
            requested_result_count,
            effective_result_count,
            promotion_reason,
            inner_content: Box::new(inner_content),
            inner_definitions: Box::new(inner_definitions),
        };

        Self {
            hits,
            relaxed,
            total_results,
            trace,
            kind,
        }
    }

    pub fn definition_symbols(&self) -> Vec<Symbol> {
        self.hits
            .iter()
            .filter_map(|hit| hit.as_symbol().cloned())
            .collect()
    }
}
