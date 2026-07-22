use std::collections::{BTreeSet, HashMap};

use anyhow::{Context, Result, bail, ensure};
use julie_core::database::bulk::atomic::{AtomicPersistenceMetadata, CanonicalWriteSet};
use julie_core::database::{SymbolDatabase, WebEdge, WebEdgeKind};
use julie_extractors::RelationshipKind;
use julie_test_support::FakeToolContext;
use julie_test_support::db::{file_info_builder, relationship_builder, symbol_builder};
use serde::Deserialize;
use tempfile::TempDir;

use crate::tests::helpers::mcp::call_tool_result_text;
use crate::tools::BlastRadiusTool;

const CORPUS_TEXT: &str =
    include_str!("../../../../fixtures/eval/blast_radius_mixed_traversal.toml");
const WORKSPACE_ID: &str = "phase3-mixed-traversal";

#[derive(Debug, Deserialize)]
pub(super) struct Corpus {
    pub version: u32,
    pub families: Vec<String>,
    pub features: Vec<String>,
    pub symbols: Vec<CorpusSymbol>,
    pub relationships: Vec<CorpusRelationship>,
    pub web_edges: Vec<CorpusWebEdge>,
    pub cases: Vec<CorpusCase>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CorpusSymbol {
    pub id: String,
    pub name: String,
    pub file_path: String,
    pub language: String,
    pub line: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct CorpusRelationship {
    pub id: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct CorpusWebEdge {
    pub from: String,
    pub to: Option<String>,
    pub external: Option<String>,
    pub kind: String,
    pub method: Option<String>,
    pub path: Option<String>,
    pub table: Option<String>,
    pub file_path: String,
    pub line: u32,
}

#[derive(Debug, Deserialize)]
pub(super) struct CorpusCase {
    pub id: String,
    pub seed: String,
    pub max_depth: u32,
    pub expected_default: Vec<String>,
    pub expected_web: Vec<String>,
    pub terminal_external: Vec<String>,
}

pub(super) struct CorpusFixture {
    _temp: TempDir,
    pub corpus: Corpus,
    pub context: FakeToolContext,
}

pub(super) fn load_corpus() -> Result<Corpus> {
    toml::from_str(CORPUS_TEXT).context("parsing Phase 3 mixed traversal corpus")
}

impl Corpus {
    pub fn case(&self, id: &str) -> Result<&CorpusCase> {
        self.cases
            .iter()
            .find(|case| case.id == id)
            .with_context(|| format!("missing corpus case {id}"))
    }

    pub fn symbol(&self, id: &str) -> Result<&CorpusSymbol> {
        self.symbols
            .iter()
            .find(|symbol| symbol.id == id)
            .with_context(|| format!("missing corpus symbol {id}"))
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "unsupported corpus version {}",
            self.version
        );
        ensure!(
            self.families.len() >= 2,
            "corpus needs at least two language/framework families"
        );

        let required_features = [
            "ordinary-web-ordinary",
            "http-internal",
            "sql-internal",
            "external-terminal",
            "cycle",
            "self-call",
            "duplicate-edge",
            "combined-depth",
            "deterministic-order",
        ];
        for feature in required_features {
            ensure!(
                self.features.iter().any(|candidate| candidate == feature),
                "missing corpus feature {feature}"
            );
        }

        let symbol_ids: BTreeSet<&str> = self
            .symbols
            .iter()
            .map(|symbol| symbol.id.as_str())
            .collect();
        ensure!(
            symbol_ids.len() == self.symbols.len(),
            "corpus symbol ids must be unique"
        );
        for relationship in &self.relationships {
            ensure!(
                symbol_ids.contains(relationship.from.as_str()),
                "relationship {} has missing source",
                relationship.id
            );
            ensure!(
                symbol_ids.contains(relationship.to.as_str()),
                "relationship {} has missing target",
                relationship.id
            );
        }
        for edge in &self.web_edges {
            ensure!(
                symbol_ids.contains(edge.from.as_str()),
                "web edge has missing source {}",
                edge.from
            );
            ensure!(
                edge.to.is_some() ^ edge.external.is_some(),
                "web edge from {} must have exactly one target",
                edge.from
            );
            if let Some(target) = edge.to.as_deref() {
                ensure!(
                    symbol_ids.contains(target),
                    "web edge has missing target {target}"
                );
            }
        }
        for case in &self.cases {
            ensure!(
                symbol_ids.contains(case.seed.as_str()),
                "case {} has missing seed",
                case.id
            );
            ensure!(
                !case.terminal_external.is_empty(),
                "case {} needs a terminal external target",
                case.id
            );
            let default: BTreeSet<_> = case.expected_default.iter().collect();
            let web: BTreeSet<_> = case.expected_web.iter().collect();
            ensure!(
                default.is_subset(&web),
                "case {} default expectations must be a web subset",
                case.id
            );
            for id in web {
                ensure!(
                    symbol_ids.contains(id.as_str()),
                    "case {} expects missing symbol {id}",
                    case.id
                );
                ensure!(id != &case.seed, "case {} must not emit its seed", case.id);
            }
        }
        Ok(())
    }
}

pub(super) fn build_fixture() -> Result<CorpusFixture> {
    let corpus = load_corpus()?;
    corpus.validate()?;
    let temp = TempDir::new()?;
    let db_path = temp.path().join("mixed-traversal.db");
    let mut db = SymbolDatabase::new(&db_path)?;

    let mut languages = HashMap::new();
    for symbol in &corpus.symbols {
        languages.insert(symbol.file_path.clone(), symbol.language.clone());
    }
    let files = languages
        .iter()
        .map(|(path, language)| file_info_builder(path).language(language).build())
        .collect::<Vec<_>>();
    let symbols = corpus
        .symbols
        .iter()
        .map(|symbol| {
            symbol_builder(&symbol.id, &symbol.name, &symbol.file_path)
                .language(&symbol.language)
                .span(symbol.line, 0, symbol.line, 20)
                .bytes(symbol.line * 10, symbol.line * 10 + 20)
                .build()
        })
        .collect::<Vec<_>>();
    let relationships = corpus
        .relationships
        .iter()
        .map(|relationship| {
            let source = corpus.symbol(&relationship.from)?;
            Ok(
                relationship_builder(&relationship.id, &relationship.from, &relationship.to)
                    .kind(RelationshipKind::Calls)
                    .file_path(&source.file_path)
                    .line_number(source.line)
                    .build(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let write_set = CanonicalWriteSet {
        files: &files,
        symbols: &symbols,
        relationships: &relationships,
        ..Default::default()
    };
    let files_to_clean = files
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    db.incremental_update_atomic_with_metadata(
        &files_to_clean,
        &write_set,
        WORKSPACE_ID,
        AtomicPersistenceMetadata::default(),
    )?;

    let web_edges = corpus
        .web_edges
        .iter()
        .map(to_web_edge)
        .collect::<Result<Vec<_>>>()?;
    db.replace_all_web_edges(&web_edges)?;
    drop(db);

    let context = FakeToolContext::new()
        .with_workspace_id(WORKSPACE_ID)
        .with_primary_root(temp.path())
        .with_primary_db_path(&db_path);
    Ok(CorpusFixture {
        _temp: temp,
        corpus,
        context,
    })
}

fn to_web_edge(edge: &CorpusWebEdge) -> Result<WebEdge> {
    let kind = match edge.kind.as_str() {
        "http" => WebEdgeKind::HttpCall,
        "sql" => WebEdgeKind::SqlQuery,
        other => bail!("unsupported corpus web edge kind {other}"),
    };
    Ok(WebEdge {
        from_symbol_id: edge.from.clone(),
        to_symbol_id: edge.to.clone(),
        to_external: edge.external.clone(),
        kind,
        method: edge.method.clone(),
        path: edge.path.clone(),
        table: edge.table.clone(),
        file_path: edge.file_path.clone(),
        line_number: edge.line,
        confidence: 0.95,
        metadata: None,
    })
}

pub(super) async fn call_case(
    fixture: &CorpusFixture,
    case: &CorpusCase,
    mode: Option<&str>,
    max_depth: Option<u32>,
) -> Result<String> {
    let result = BlastRadiusTool {
        symbol_ids: vec![case.seed.clone()],
        max_depth: max_depth.unwrap_or(case.max_depth),
        limit: 100,
        include_tests: false,
        mode: mode.map(str::to_string),
        ..Default::default()
    }
    .call_tool(&fixture.context)
    .await?;
    Ok(call_tool_result_text(&result))
}
