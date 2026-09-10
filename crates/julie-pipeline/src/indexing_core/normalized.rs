use anyhow::Result;
use julie_core::Symbol;
use julie_extractors::{
    ComplexityMetric, ExtractionResults, Identifier, Literal, ParseDiagnostic, PendingRelationship,
    Relationship, SourceRegion, StructuralFact, StructuredPendingRelationship, TypeInfo,
};

#[derive(Debug)]
pub struct NormalizedExtractionData {
    pub symbols: Vec<Symbol>,
    pub relationships: Vec<Relationship>,
    pub pending_relationships: Vec<PendingRelationship>,
    pub structured_pending_relationships: Vec<StructuredPendingRelationship>,
    pub identifiers: Vec<Identifier>,
    pub types: Vec<TypeInfo>,
    pub type_argument_rows: Vec<julie_facts::rows::FlatTypeArgument>,
    pub literals: Vec<Literal>,
    pub source_regions: Vec<SourceRegion>,
    pub structural_facts: Vec<StructuralFact>,
    pub complexity_metrics: Vec<ComplexityMetric>,
    pub parse_diagnostics: Vec<ParseDiagnostic>,
}

pub fn normalize_extraction_results(
    mut results: ExtractionResults,
    source: &str,
    configs: &julie_index::search::LanguageConfigs,
) -> Result<NormalizedExtractionData> {
    if !results.literals.is_empty() {
        let carriers = configs.build_literal_carrier_configs();
        julie_index::analysis::literals::classify_literals_by_carrier(
            &mut results.literals,
            &carriers,
        );
    }
    let mut symbols = results
        .symbols
        .into_iter()
        .map(|extracted| Symbol::from_extracted(extracted, source))
        .collect::<Result<Vec<_>>>()?;

    if !symbols.is_empty() {
        let roles = configs.build_test_role_configs();
        julie_index::analysis::test_roles::classify_symbols_by_role(&mut symbols, &roles);
    }

    Ok(NormalizedExtractionData {
        symbols,
        relationships: results.relationships,
        pending_relationships: results.pending_relationships,
        structured_pending_relationships: results.structured_pending_relationships,
        identifiers: results.identifiers,
        types: results.types.into_values().collect(),
        type_argument_rows: julie_facts::rows::flatten_type_argument_usages(
            &results.type_argument_usages,
        ),
        literals: results.literals,
        source_regions: results.source_regions,
        structural_facts: results.structural_facts,
        complexity_metrics: results.complexity_metrics,
        parse_diagnostics: results.parse_diagnostics,
    })
}
