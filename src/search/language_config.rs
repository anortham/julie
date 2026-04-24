//! Language configuration for code-aware tokenization.
//!
//! Each language has a TOML config defining tokenizer patterns,
//! naming conventions, and scoring rules. Configs are embedded
//! in the binary via include_str!.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

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
    pub early_warnings: EarlyWarningConfig,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that ALL 33 embedded language configs parse successfully.
    ///
    /// With the old warn+skip behavior, a broken TOML would silently reduce the
    /// count. With the new panic behavior, this test documents that all configs
    /// are present and valid. If this count fails, check that a new language was
    /// added to the embedded list in `load_embedded` and its .toml parses cleanly.
    #[test]
    fn test_all_embedded_language_configs_load_without_skips() {
        let configs = LanguageConfigs::load_embedded();
        assert_eq!(
            configs.len(),
            33,
            "Expected exactly 33 embedded language configs, got {}. \
             A broken TOML or missing entry would cause this count to be wrong.",
            configs.len()
        );
    }

    #[test]
    fn test_language_config_defaults_empty_annotation_sections() {
        let config: LanguageConfig = toml::from_str(
            r#"
[tokenizer]
preserve_patterns = ["@"]
naming_styles = ["snake_case"]
meaningful_affixes = []
"#,
        )
        .expect("config without annotation sections should parse");

        assert!(config.annotation_classes.entrypoint.is_empty());
        assert!(config.annotation_classes.auth.is_empty());
        assert!(config.annotation_classes.auth_bypass.is_empty());
        assert!(config.annotation_classes.middleware.is_empty());
        assert!(config.annotation_classes.scheduler.is_empty());
        assert!(config.annotation_classes.test.test_case.is_empty());
        assert!(config.annotation_classes.test.parameterized_test.is_empty());
        assert!(config.annotation_classes.test.fixture_setup.is_empty());
        assert!(config.annotation_classes.test.fixture_teardown.is_empty());
        assert!(config.annotation_classes.test.test_container.is_empty());
        assert!(config.early_warnings.review_markers.is_empty());
        assert_eq!(config.early_warnings.schema_version, 1);
    }

    #[test]
    fn test_language_config_loads_populated_annotation_sections() {
        let config: LanguageConfig = toml::from_str(
            r#"
[tokenizer]
preserve_patterns = ["@"]
naming_styles = ["snake_case"]
meaningful_affixes = []

[annotation_classes]
entrypoint = ["app.route"]
auth = ["login_required"]
auth_bypass = ["allowanonymous"]
middleware = ["middleware"]
scheduler = ["celery.task"]

[annotation_classes.test]
parameterized_test = ["pytest.mark.parametrize"]
fixture_setup = ["pytest.fixture"]

[early_warnings]
review_markers = ["allowanonymous"]
schema_version = 7
"#,
        )
        .expect("config with annotation sections should parse");

        assert_eq!(config.annotation_classes.entrypoint, vec!["app.route"]);
        assert_eq!(config.annotation_classes.auth, vec!["login_required"]);
        assert_eq!(
            config.annotation_classes.auth_bypass,
            vec!["allowanonymous"]
        );
        assert_eq!(config.annotation_classes.middleware, vec!["middleware"]);
        assert_eq!(config.annotation_classes.scheduler, vec!["celery.task"]);
        assert_eq!(
            config.annotation_classes.test.parameterized_test,
            vec!["pytest.mark.parametrize"]
        );
        assert_eq!(
            config.annotation_classes.test.fixture_setup,
            vec!["pytest.fixture"]
        );
        assert!(config.annotation_classes.test.test_case.is_empty());
        assert!(config.annotation_classes.test.fixture_teardown.is_empty());
        assert!(config.annotation_classes.test.test_container.is_empty());
        assert_eq!(config.early_warnings.review_markers, vec!["allowanonymous"]);
        assert_eq!(config.early_warnings.schema_version, 7);
    }

    #[test]
    fn test_embedded_language_configs_include_expected_annotation_classes() {
        let configs = LanguageConfigs::load_embedded();

        let python = configs.get("python").expect("python config should exist");
        assert!(
            python
                .annotation_classes
                .entrypoint
                .contains(&"app.route".into())
        );
        assert!(
            python
                .annotation_classes
                .auth
                .contains(&"login_required".into())
        );
        assert!(
            python
                .annotation_classes
                .test
                .parameterized_test
                .contains(&"pytest.mark.parametrize".into())
        );
        assert!(
            python
                .annotation_classes
                .test
                .fixture_setup
                .contains(&"pytest.fixture".into())
        );

        let java = configs.get("java").expect("java config should exist");
        assert!(
            java.annotation_classes
                .entrypoint
                .contains(&"getmapping".into())
        );
        assert!(
            java.annotation_classes
                .auth
                .contains(&"preauthorize".into())
        );
        assert!(
            java.annotation_classes
                .auth_bypass
                .contains(&"permitall".into())
        );

        let kotlin = configs.get("kotlin").expect("kotlin config should exist");
        assert!(
            kotlin
                .annotation_classes
                .entrypoint
                .contains(&"getmapping".into())
        );
        assert!(
            kotlin
                .annotation_classes
                .auth
                .contains(&"preauthorize".into())
        );
        assert!(
            kotlin
                .annotation_classes
                .auth_bypass
                .contains(&"permitall".into())
        );

        let csharp = configs.get("csharp").expect("csharp config should exist");
        assert!(
            csharp
                .annotation_classes
                .entrypoint
                .contains(&"httpget".into())
        );
        assert!(csharp.annotation_classes.auth.contains(&"authorize".into()));
        assert!(
            csharp
                .annotation_classes
                .auth_bypass
                .contains(&"allowanonymous".into())
        );

        let typescript = configs
            .get("typescript")
            .expect("typescript config should exist");
        assert!(
            typescript
                .annotation_classes
                .entrypoint
                .contains(&"controller".into())
        );
        assert!(
            typescript
                .annotation_classes
                .entrypoint
                .contains(&"get".into())
        );
        assert!(
            typescript
                .annotation_classes
                .auth
                .contains(&"useguards".into())
        );
        assert!(typescript.annotation_classes.auth_bypass.is_empty());
        assert!(typescript.early_warnings.review_markers.is_empty());
        assert_eq!(typescript.early_warnings.schema_version, 1);

        let javascript = configs
            .get("javascript")
            .expect("javascript config should exist");
        assert!(
            javascript
                .annotation_classes
                .entrypoint
                .contains(&"controller".into())
        );
        assert!(
            javascript
                .annotation_classes
                .auth
                .contains(&"useguards".into())
        );
        assert!(javascript.annotation_classes.auth_bypass.is_empty());

        let rust = configs.get("rust").expect("rust config should exist");
        assert!(rust.annotation_classes.entrypoint.is_empty());
        assert!(
            rust.annotation_classes
                .test
                .test_case
                .contains(&"test".into())
        );
        assert!(
            rust.annotation_classes
                .test
                .test_case
                .contains(&"tokio::test".into())
        );

        // Verify Java has role-classified test annotations
        assert!(
            java.annotation_classes
                .test
                .test_case
                .contains(&"test".into())
        );
        assert!(
            java.annotation_classes
                .test
                .parameterized_test
                .contains(&"parameterizedtest".into())
        );
        assert!(
            java.annotation_classes
                .test
                .fixture_setup
                .contains(&"beforeeach".into())
        );
        assert!(
            java.annotation_classes
                .test
                .fixture_teardown
                .contains(&"aftereach".into())
        );

        // Verify C# has role-classified test annotations
        assert!(
            csharp
                .annotation_classes
                .test
                .test_case
                .contains(&"fact".into())
        );
        assert!(
            csharp
                .annotation_classes
                .test
                .parameterized_test
                .contains(&"theory".into())
        );
        assert!(
            csharp
                .annotation_classes
                .test
                .fixture_setup
                .contains(&"setup".into())
        );
        assert!(
            csharp
                .annotation_classes
                .test
                .test_container
                .contains(&"testfixture".into())
        );
    }
}
