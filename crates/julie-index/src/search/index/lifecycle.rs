use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use tantivy::Index;
use tantivy::tokenizer::TextAnalyzer;

use super::SearchIndex;
use crate::search::error::{Result, SearchError};
use crate::search::language_config::LanguageConfigs;
use crate::search::schema::{SchemaFields, create_schema};
use crate::search::tokenizer::{CodeTokenizer, SimpleCodeTokenizer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchIndexOpenDisposition {
    Compatible,
    RecreatedIncompatible,
    RecreatedOpenFailure,
}

impl SearchIndexOpenDisposition {
    pub fn repair_required(self) -> bool {
        !matches!(self, Self::Compatible)
    }
}

pub struct SearchIndexOpenOutcome {
    pub index: SearchIndex,
    pub disposition: SearchIndexOpenDisposition,
}

impl SearchIndexOpenOutcome {
    pub fn repair_required(&self) -> bool {
        self.disposition.repair_required()
    }

    pub fn into_index(self) -> SearchIndex {
        self.index
    }
}

impl SearchIndex {
    /// Create a new index at the given directory path using default patterns.
    pub fn create(path: &Path) -> Result<Self> {
        let tokenizer = CodeTokenizer::with_default_patterns();
        Self::create_with_tokenizer(path, tokenizer, None)
    }

    /// Create a new index with language-specific tokenizer patterns.
    pub fn create_with_language_configs(path: &Path, configs: &LanguageConfigs) -> Result<Self> {
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::create_with_tokenizer(path, tokenizer, Some(configs.clone()))
    }

    /// Open an existing index at the given directory path.
    pub fn open(path: &Path) -> Result<Self> {
        if !path.join("meta.json").exists() {
            return Err(SearchError::IndexNotFound(path.display().to_string()));
        }
        let tokenizer = CodeTokenizer::with_default_patterns();
        Self::open_with_tokenizer(path, tokenizer, None).map(SearchIndexOpenOutcome::into_index)
    }

    /// Open an existing index with language-specific tokenizer patterns.
    pub fn open_with_language_configs(path: &Path, configs: &LanguageConfigs) -> Result<Self> {
        if !path.join("meta.json").exists() {
            return Err(SearchError::IndexNotFound(path.display().to_string()));
        }
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_with_tokenizer(path, tokenizer, Some(configs.clone()))
            .map(SearchIndexOpenOutcome::into_index)
    }

    pub fn open_with_language_configs_outcome(
        path: &Path,
        configs: &LanguageConfigs,
    ) -> Result<SearchIndexOpenOutcome> {
        if !path.join("meta.json").exists() {
            return Err(SearchError::IndexNotFound(path.display().to_string()));
        }
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_with_tokenizer(path, tokenizer, Some(configs.clone()))
    }

    /// Open an existing index or create a new one if it doesn't exist.
    pub fn open_or_create(path: &Path) -> Result<Self> {
        let tokenizer = CodeTokenizer::with_default_patterns();
        Self::open_or_create_with_tokenizer(path, tokenizer, None)
            .map(SearchIndexOpenOutcome::into_index)
    }

    /// Open an existing index or create a new one, using language-specific tokenizer patterns.
    pub fn open_or_create_with_language_configs(
        path: &Path,
        configs: &LanguageConfigs,
    ) -> Result<Self> {
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_or_create_with_tokenizer(path, tokenizer, Some(configs.clone()))
            .map(SearchIndexOpenOutcome::into_index)
    }

    pub fn open_or_create_with_language_configs_outcome(
        path: &Path,
        configs: &LanguageConfigs,
    ) -> Result<SearchIndexOpenOutcome> {
        let tokenizer = CodeTokenizer::from_language_configs(configs);
        Self::open_or_create_with_tokenizer(path, tokenizer, Some(configs.clone()))
    }

    /// Get the total number of documents in the index.
    fn open_or_create_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<SearchIndexOpenOutcome> {
        let expected_schema = create_schema();
        let expected_marker = Self::expected_compat_marker(&expected_schema, &tokenizer);

        let (index, disposition) = if path.join("meta.json").exists() {
            match Index::open_in_dir(path) {
                Ok(existing) => {
                    if Self::index_is_compatible(
                        path,
                        &expected_schema,
                        &existing.schema(),
                        &expected_marker,
                    ) {
                        (existing, SearchIndexOpenDisposition::Compatible)
                    } else {
                        tracing::warn!(
                            "Tantivy index at {} is incompatible with Julie expectations, recreating empty index",
                            path.display()
                        );
                        drop(existing);
                        (
                            Self::recreate_index_with_lock(
                                path,
                                &expected_schema,
                                &expected_marker,
                            )?,
                            SearchIndexOpenDisposition::RecreatedIncompatible,
                        )
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        "Failed to open Tantivy index at {} ({err}), recreating empty index",
                        path.display()
                    );
                    (
                        Self::recreate_index_with_lock(path, &expected_schema, &expected_marker)?,
                        SearchIndexOpenDisposition::RecreatedOpenFailure,
                    )
                }
            }
        } else {
            let index = Index::builder()
                .schema(expected_schema.clone())
                .create_in_dir(path)?;
            Self::write_compat_marker(path, &expected_marker)?;
            (index, SearchIndexOpenDisposition::Compatible)
        };

        let search_index =
            Self::build_search_index(index, &expected_schema, tokenizer, language_configs)?;

        Ok(SearchIndexOpenOutcome {
            index: search_index,
            disposition,
        })
    }

    fn create_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<Self> {
        let schema = create_schema();
        let expected_marker = Self::expected_compat_marker(&schema, &tokenizer);
        let index = Index::create_in_dir(path, schema.clone())?;
        Self::write_compat_marker(path, &expected_marker)?;
        Self::build_search_index(index, &schema, tokenizer, language_configs)
    }

    fn open_with_tokenizer(
        path: &Path,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<SearchIndexOpenOutcome> {
        let expected_schema = create_schema();
        let expected_marker = Self::expected_compat_marker(&expected_schema, &tokenizer);

        let (index, disposition) = match Index::open_in_dir(path) {
            Ok(index) => {
                if Self::index_is_compatible(
                    path,
                    &expected_schema,
                    &index.schema(),
                    &expected_marker,
                ) {
                    (index, SearchIndexOpenDisposition::Compatible)
                } else {
                    tracing::warn!(
                        "Tantivy index at {} is incompatible with Julie expectations, recreating empty index",
                        path.display()
                    );
                    drop(index);
                    (
                        Self::recreate_index_with_lock(path, &expected_schema, &expected_marker)?,
                        SearchIndexOpenDisposition::RecreatedIncompatible,
                    )
                }
            }
            Err(err) => {
                tracing::warn!(
                    "Failed to open Tantivy index at {} ({err}), recreating empty index",
                    path.display()
                );
                (
                    Self::recreate_index_with_lock(path, &expected_schema, &expected_marker)?,
                    SearchIndexOpenDisposition::RecreatedOpenFailure,
                )
            }
        };

        let search_index =
            Self::build_search_index(index, &expected_schema, tokenizer, language_configs)?;
        Ok(SearchIndexOpenOutcome {
            index: search_index,
            disposition,
        })
    }

    fn register_tokenizer(index: &Index, tokenizer: CodeTokenizer) {
        index
            .tokenizers()
            .register("code", TextAnalyzer::builder(tokenizer).build());
        // Register the simple tokenizer for the pretokenized_code field (T3 wiring;
        // schema fields are retargeted to "simple_code" at T4/T5).
        index.tokenizers().register(
            "simple_code",
            TextAnalyzer::builder(SimpleCodeTokenizer::new()).build(),
        );
    }

    fn build_search_index(
        index: Index,
        schema: &tantivy::schema::Schema,
        tokenizer: CodeTokenizer,
        language_configs: Option<LanguageConfigs>,
    ) -> Result<Self> {
        let schema_fields = SchemaFields::new(schema);
        Self::register_tokenizer(&index, tokenizer);
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(None),
            schema_fields,
            language_configs,
            shutdown: AtomicBool::new(false),
            #[cfg(any(test, feature = "test-support"))]
            rebuild_pause_for_test: Mutex::new(None),
            #[cfg(any(test, feature = "test-support"))]
            rebuild_failure_for_test: AtomicBool::new(false),
        })
    }
}
