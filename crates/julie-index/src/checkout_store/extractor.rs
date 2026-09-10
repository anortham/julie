//! The real extractor and normalization the store feeds to `FactsWriter`.

use std::collections::HashMap;
use std::path::PathBuf;

use julie_core::file_policy::{ExtractionMode, determine_extraction_mode};
use julie_extractors::ExtractionResults;
use julie_facts::Extractor;
use julie_facts::rows::{Normalization, TestRoleConfig};
use tracing::warn;

use crate::search::LanguageConfigs;

pub struct FactsExtractor {
    root: PathBuf,
}

impl FactsExtractor {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl Extractor for FactsExtractor {
    fn extract(&self, path: &str, content: &str, language: &str) -> ExtractionResults {
        match determine_extraction_mode(language, content) {
            ExtractionMode::TextOnly => ExtractionResults::empty(),
            ExtractionMode::ParserBacked => julie_extractors::extract_canonical(
                path, content, &self.root,
            )
            .unwrap_or_else(|err| {
                warn!(path, %err, "extraction failed; storing the blob without facts");
                ExtractionResults::empty()
            }),
        }
    }
}

/// Test-role classifiers from the embedded language configs.
pub fn normalization() -> Normalization {
    let test_roles: HashMap<String, TestRoleConfig> = LanguageConfigs::load_embedded()
        .build_test_role_configs()
        .into_iter()
        .map(|(language, config)| {
            (
                language,
                TestRoleConfig {
                    test_case: config.test_case,
                    parameterized_test: config.parameterized_test,
                    fixture_setup: config.fixture_setup,
                    fixture_teardown: config.fixture_teardown,
                    test_container: config.test_container,
                    test_base_types: config.test_base_types,
                },
            )
        })
        .collect();
    Normalization { test_roles }
}
