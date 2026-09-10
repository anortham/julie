use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use julie_core::file_policy::{
    ExtractionMode, detect_language_for_indexing, determine_extraction_mode,
};
use julie_extractors::ExtractionResults;
use julie_facts::rows::{Normalization, TestRoleConfig};
use julie_facts::{Extractor, FactsStore, PathChange};

use crate::utils::paths::to_relative_unix_style;

pub(crate) struct RootExtractor {
    pub root: PathBuf,
}

impl Extractor for RootExtractor {
    fn extract(&self, path: &str, content: &str, language: &str) -> ExtractionResults {
        match determine_extraction_mode(language, content) {
            ExtractionMode::TextOnly => ExtractionResults::empty(),
            ExtractionMode::ParserBacked => {
                julie_extractors::extract_canonical(path, content, &self.root)
                    .unwrap_or_else(|_| ExtractionResults::empty())
            }
        }
    }
}

pub(crate) fn extract_normalization() -> Normalization {
    let test_roles = crate::search::LanguageConfigs::load_embedded()
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

pub(crate) fn reject_empty_replace(
    store: &FactsStore,
    extractor: &RootExtractor,
    path: &str,
    bytes: &[u8],
    language: &str,
) -> Result<()> {
    let existing = store.reader().symbols_for_paths(&[path])?;
    if existing.is_empty() {
        return Ok(());
    }
    let results = extractor.extract(path, &String::from_utf8_lossy(bytes), language);
    if results.symbols.is_empty() {
        return Err(anyhow!(
            "extraction for '{path}' would remove existing symbols ({})",
            existing.len()
        ));
    }
    Ok(())
}

pub(crate) fn upsert_change(root: &Path, file_path: &Path) -> Result<PathChange> {
    let relative = to_relative_unix_style(file_path, root)?;
    let bytes = std::fs::read(file_path)
        .with_context(|| format!("failed to read {}", file_path.display()))?;
    Ok(PathChange::Upsert {
        path: relative,
        language: detect_language_for_indexing(file_path),
        bytes,
    })
}

pub(crate) fn path_blob_hash(store: &FactsStore, path: &str) -> Result<Option<String>> {
    Ok(store
        .reader()
        .paths()?
        .into_iter()
        .find(|row| row.path == path)
        .map(|row| row.blob_hash))
}

pub(crate) fn filter_scan_delta(
    store: &FactsStore,
    root: &Path,
    discovered_files: Vec<PathBuf>,
) -> Result<(Vec<PathBuf>, Vec<String>)> {
    let existing: HashSet<(String, String)> = store
        .reader()
        .paths()?
        .into_iter()
        .map(|row| (row.path, row.blob_hash))
        .collect();
    let existing_paths: HashSet<String> = existing.iter().map(|(path, _)| path.clone()).collect();
    let mut current_paths = HashSet::new();
    let mut files_to_extract = Vec::new();
    for file_path in discovered_files {
        let relative_path = to_relative_unix_style(&file_path, root)?;
        current_paths.insert(relative_path.clone());
        let bytes = std::fs::read(&file_path)
            .with_context(|| format!("failed to hash {}", file_path.display()))?;
        let current_hash = blake3::hash(&bytes).to_hex().to_string();
        if existing.contains(&(relative_path, current_hash)) {
            continue;
        }
        files_to_extract.push(file_path);
    }
    let mut orphaned_files: Vec<String> = existing_paths
        .into_iter()
        .filter(|path| !current_paths.contains(path))
        .collect();
    orphaned_files.sort();
    Ok((files_to_extract, orphaned_files))
}
