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
            ("gdscript", include_str!("../../languages/gdscript.toml")),
            ("go", include_str!("../../languages/go.toml")),
            ("html", include_str!("../../languages/html.toml")),
            ("java", include_str!("../../languages/java.toml")),
            ("javascript", include_str!("../../languages/javascript.toml")),
            ("json", include_str!("../../languages/json.toml")),
            ("kotlin", include_str!("../../languages/kotlin.toml")),
            ("lua", include_str!("../../languages/lua.toml")),
            ("markdown", include_str!("../../languages/markdown.toml")),
            ("php", include_str!("../../languages/php.toml")),
            ("powershell", include_str!("../../languages/powershell.toml")),
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
            ("typescript", include_str!("../../languages/typescript.toml")),
            ("vue", include_str!("../../languages/vue.toml")),
            ("yaml", include_str!("../../languages/yaml.toml")),
            ("zig", include_str!("../../languages/zig.toml")),
        ];

        for (name, content) in embedded {
            match toml::from_str::<LanguageConfig>(content) {
                Ok(config) => {
                    configs.insert(name.to_string(), config);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse language config for {}: {}", name, e);
                }
            }
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
}
