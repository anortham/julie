//! Language configuration for code-aware tokenization.
//!
//! Each language has a TOML config defining tokenizer patterns,
//! naming conventions, and scoring rules. Configs are embedded
//! in the binary via include_str!.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

const CONFIG_LANGUAGE_ALIASES: &[(&str, &str)] = &[("tsx", "typescript"), ("jsx", "javascript")];

/// Configuration for a programming language's tokenization and matching rules.
#[derive(Debug, Clone, Deserialize)]
pub struct LanguageConfig {
    pub tokenizer: TokenizerConfig,
    #[serde(default)]
    pub variants: VariantsConfig,
    #[serde(default)]
    pub scoring: ScoringConfig,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
    #[serde(default)]
    pub annotation_classes: AnnotationClassesConfig,
    #[serde(default)]
    pub test_evidence: TestEvidenceConfig,
    #[serde(default)]
    pub early_warnings: EarlyWarningConfig,
    #[serde(default)]
    pub literal_carriers: LiteralCarriersConfig,
}

/// Tokenizer configuration for code-aware text processing.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenizerConfig {
    #[serde(default)]
    pub preserve_patterns: Vec<String>,
    #[serde(default)]
    pub naming_styles: Vec<String>,
    #[serde(default)]
    pub meaningful_affixes: Vec<String>,
}

/// Configuration for generating naming variants.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VariantsConfig {
    #[serde(default)]
    pub strip_prefixes: Vec<String>,
    #[serde(default)]
    pub strip_suffixes: Vec<String>,
}

/// Configuration for search result scoring/boosting.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScoringConfig {
    #[serde(default)]
    pub important_patterns: Vec<String>,
}

/// Configuration for embedding generation per language.
///
/// Controls which additional symbol kinds get embedded and the variable budget
/// ratio. The global `EMBEDDABLE_KINDS` list always applies; `extra_kinds` adds
/// to it on a per-language basis.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EmbeddingsConfig {
    /// Additional symbol kinds to embed beyond the global EMBEDDABLE_KINDS.
    /// Valid values: "constructor", "constant", "export", "destructor", "operator"
    #[serde(default)]
    pub extra_kinds: Vec<String>,

    /// Override the variable embedding budget ratio (global default: 0.20).
    /// Languages with many function-like variables (JS/TS/Python) benefit from
    /// higher ratios (0.40-0.50).
    #[serde(default)]
    pub variable_ratio: Option<f64>,
}

/// Role-classified test annotation mappings.
///
/// Replaces the old flat `test: Vec<String>` + `fixture: Vec<String>` with
/// per-role lists so downstream quality scoring can distinguish scorable
/// test cases from non-scorable fixtures and containers.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TestAnnotationClasses {
    #[serde(default)]
    pub test_case: Vec<String>,
    #[serde(default)]
    pub parameterized_test: Vec<String>,
    #[serde(default)]
    pub fixture_setup: Vec<String>,
    #[serde(default)]
    pub fixture_teardown: Vec<String>,
    #[serde(default)]
    pub test_container: Vec<String>,
    /// Base types / inherited components that mark a container as a test
    /// container without an annotation (e.g. `unittest.TestCase`, `XCTestCase`,
    /// QML `TestCase`). Matched by last path segment in `classify_test_role`.
    #[serde(default)]
    pub test_base_types: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnnotationClassesConfig {
    #[serde(default)]
    pub entrypoint: Vec<String>,
    #[serde(default)]
    pub auth: Vec<String>,
    #[serde(default)]
    pub auth_bypass: Vec<String>,
    #[serde(default)]
    pub middleware: Vec<String>,
    #[serde(default)]
    pub scheduler: Vec<String>,
    #[serde(default)]
    pub test: TestAnnotationClasses,
}

/// Per-language carrier vocabulary for string-literal call-arg classification
/// (Miller bridge Phase 3). Each list holds the idiomatic callee texts whose
/// string-literal arguments are URLs / SQL / route templates in that language
/// (e.g. TS `fetch`, `axios.get`; C# `Query`, `ExecuteAsync`). Matching is
/// case-insensitive (lowercased when building the runtime config). Adding a
/// client library is a one-line edit here, never a hardcoded extractor change.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LiteralCarriersConfig {
    #[serde(default)]
    pub url: Vec<String>,
    #[serde(default)]
    pub sql: Vec<String>,
    #[serde(default)]
    pub route: Vec<String>,
}

/// Framework-specific test evidence identifiers.
///
/// Used by downstream test quality scoring to recognize assertion calls,
/// error-path assertions, and mock/stub setups within test bodies. All
/// identifiers are stored lowercase for case-insensitive matching.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TestEvidenceConfig {
    #[serde(default)]
    pub assertion_identifiers: Vec<String>,
    #[serde(default)]
    pub error_assertion_identifiers: Vec<String>,
    #[serde(default)]
    pub mock_identifiers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EarlyWarningConfig {
    #[serde(default)]
    pub review_markers: Vec<String>,
    #[serde(default = "default_early_warning_schema_version")]
    pub schema_version: u32,
}

impl Default for EarlyWarningConfig {
    fn default() -> Self {
        Self {
            review_markers: Vec::new(),
            schema_version: default_early_warning_schema_version(),
        }
    }
}

fn default_early_warning_schema_version() -> u32 {
    1
}

/// Registry of all language configurations.
#[derive(Clone)]
pub struct LanguageConfigs {
    configs: HashMap<String, LanguageConfig>,
}

impl LanguageConfigs {
    /// Load all embedded language configurations.
    pub fn load_embedded() -> Self {
        let mut configs = HashMap::new();

        // Each entry: (language_name, toml_content)
        let embedded: &[(&str, &str)] = &[
            ("bash", include_str!("../../languages/bash.toml")),
            ("c", include_str!("../../languages/c.toml")),
            ("cpp", include_str!("../../languages/cpp.toml")),
            ("csharp", include_str!("../../languages/csharp.toml")),
            ("css", include_str!("../../languages/css.toml")),
            ("dart", include_str!("../../languages/dart.toml")),
            ("elixir", include_str!("../../languages/elixir.toml")),
            ("gdscript", include_str!("../../languages/gdscript.toml")),
            ("go", include_str!("../../languages/go.toml")),
            ("html", include_str!("../../languages/html.toml")),
            ("java", include_str!("../../languages/java.toml")),
            (
                "javascript",
                include_str!("../../languages/javascript.toml"),
            ),
            ("json", include_str!("../../languages/json.toml")),
            ("kotlin", include_str!("../../languages/kotlin.toml")),
            ("scala", include_str!("../../languages/scala.toml")),
            ("lua", include_str!("../../languages/lua.toml")),
            ("markdown", include_str!("../../languages/markdown.toml")),
            ("php", include_str!("../../languages/php.toml")),
            (
                "powershell",
                include_str!("../../languages/powershell.toml"),
            ),
            ("python", include_str!("../../languages/python.toml")),
            ("qml", include_str!("../../languages/qml.toml")),
            ("r", include_str!("../../languages/r.toml")),
            ("razor", include_str!("../../languages/razor.toml")),
            ("regex", include_str!("../../languages/regex.toml")),
            ("ruby", include_str!("../../languages/ruby.toml")),
            ("rust", include_str!("../../languages/rust.toml")),
            ("sql", include_str!("../../languages/sql.toml")),
            ("swift", include_str!("../../languages/swift.toml")),
            ("toml", include_str!("../../languages/toml.toml")),
            (
                "typescript",
                include_str!("../../languages/typescript.toml"),
            ),
            ("vbnet", include_str!("../../languages/vbnet.toml")),
            ("vue", include_str!("../../languages/vue.toml")),
            ("yaml", include_str!("../../languages/yaml.toml")),
            ("zig", include_str!("../../languages/zig.toml")),
        ];

        for (name, content) in embedded {
            // These configs are compiled-in via include_str! -- a parse failure means
            // a broken build was shipped. Panic immediately rather than silently
            // skipping the language and running with incomplete tokenization.
            let config: LanguageConfig = toml::from_str(content).unwrap_or_else(|e| {
                panic!("Failed to parse embedded language config for '{name}': {e}")
            });
            configs.insert(name.to_string(), config);
        }

        Self { configs }
    }

    pub fn get(&self, language: &str) -> Option<&LanguageConfig> {
        self.configs.get(language)
    }

    pub fn len(&self) -> usize {
        self.configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    /// Collect all preserve_patterns from all languages into a single union set.
    pub fn all_preserve_patterns(&self) -> Vec<String> {
        let mut patterns: HashSet<String> = HashSet::new();
        for config in self.configs.values() {
            for pattern in &config.tokenizer.preserve_patterns {
                patterns.insert(pattern.clone());
            }
        }
        let mut result: Vec<String> = patterns.into_iter().collect();
        // Sort by length descending so longer patterns match first
        result.sort_by_key(|b| std::cmp::Reverse(b.len()));
        result
    }

    /// Collect all meaningful_affixes from all languages into a single union set.
    pub fn all_meaningful_affixes(&self) -> Vec<String> {
        let mut affixes: HashSet<String> = HashSet::new();
        for config in self.configs.values() {
            for affix in &config.tokenizer.meaningful_affixes {
                affixes.insert(affix.clone());
            }
        }
        // Sort by length descending so longer affixes match first
        let mut result: Vec<String> = affixes.into_iter().collect();
        result.sort_by_key(|b| std::cmp::Reverse(b.len()));
        result
    }

    /// Collect all strip_prefixes and strip_suffixes from all languages.
    pub fn all_strip_rules(&self) -> (Vec<String>, Vec<String>) {
        let mut prefixes: HashSet<String> = HashSet::new();
        let mut suffixes: HashSet<String> = HashSet::new();
        for config in self.configs.values() {
            for p in &config.variants.strip_prefixes {
                prefixes.insert(p.clone());
            }
            for s in &config.variants.strip_suffixes {
                suffixes.insert(s.clone());
            }
        }
        // Sort by length descending so longer patterns match first
        let mut prefix_vec: Vec<String> = prefixes.into_iter().collect();
        prefix_vec.sort_by_key(|b| std::cmp::Reverse(b.len()));
        let mut suffix_vec: Vec<String> = suffixes.into_iter().collect();
        suffix_vec.sort_by_key(|b| std::cmp::Reverse(b.len()));
        (prefix_vec, suffix_vec)
    }

    /// Get the embeddings config for a specific language.
    pub fn embeddings_config(&self, language: &str) -> Option<&EmbeddingsConfig> {
        self.configs.get(language).map(|c| &c.embeddings)
    }

    /// Build per-language test role configs from the annotation classes in each
    /// language TOML. Used by `classify_symbols_by_role` in the indexing pipeline.
    pub fn build_test_role_configs(
        &self,
    ) -> HashMap<String, crate::analysis::test_roles::TestRoleConfig> {
        let configs = self
            .configs
            .iter()
            .map(|(lang, config)| {
                let tc = &config.annotation_classes.test;
                let role_config = crate::analysis::test_roles::TestRoleConfig {
                    test_case: tc.test_case.iter().cloned().collect(),
                    parameterized_test: tc.parameterized_test.iter().cloned().collect(),
                    fixture_setup: tc.fixture_setup.iter().cloned().collect(),
                    fixture_teardown: tc.fixture_teardown.iter().cloned().collect(),
                    test_container: tc.test_container.iter().cloned().collect(),
                    test_base_types: tc.test_base_types.iter().cloned().collect(),
                };
                (lang.clone(), role_config)
            })
            .collect();
        with_config_language_aliases(configs)
    }

    /// Build per-language literal carrier configs from the `[literal_carriers]`
    /// section in each language TOML. Carrier sets are lowercased here so
    /// `classify_literals_by_carrier` can match case-insensitively. Used by the
    /// literal classification + gate in the indexing pipeline / extract path.
    pub fn build_literal_carrier_configs(
        &self,
    ) -> HashMap<String, crate::analysis::literals::LiteralCarrierConfig> {
        let configs = self
            .configs
            .iter()
            .map(|(lang, config)| {
                let lc = &config.literal_carriers;
                let carrier_config = crate::analysis::literals::LiteralCarrierConfig {
                    url: lc.url.iter().map(|s| s.to_lowercase()).collect(),
                    sql: lc.sql.iter().map(|s| s.to_lowercase()).collect(),
                    route: lc.route.iter().map(|s| s.to_lowercase()).collect(),
                };
                (lang.clone(), carrier_config)
            })
            .collect();
        with_config_language_aliases(configs)
    }
}

fn with_config_language_aliases<T: Clone>(mut configs: HashMap<String, T>) -> HashMap<String, T> {
    for (alias, source) in CONFIG_LANGUAGE_ALIASES {
        if let Some(config) = configs.get(*source).cloned() {
            configs.insert((*alias).to_string(), config);
        }
    }
    configs
}
