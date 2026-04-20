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

#[derive(Debug, Clone, Serialize)]
pub struct SearchTrace {
    pub strategy_id: String,
    pub result_count: usize,
    pub top_hits: Vec<SearchHitSummary>,
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

    pub fn definition_symbols(&self) -> Vec<Symbol> {
        self.hits
            .iter()
            .filter_map(|hit| hit.as_symbol().cloned())
            .collect()
    }
}
