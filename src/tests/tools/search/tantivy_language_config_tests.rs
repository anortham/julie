//! Tests for language configuration loading.

use crate::search::language_config::{LanguageConfig, LanguageConfigs};

#[test]
fn test_load_embedded_configs() {
    let configs = LanguageConfigs::load_embedded();
    // We have 31 supported languages
    assert!(
        configs.len() >= 28,
        "Expected at least 28 languages, got {}",
        configs.len()
    );
}

#[test]
fn test_rust_config_has_expected_patterns() {
    let configs = LanguageConfigs::load_embedded();
    let rust = configs.get("rust").expect("rust config should exist");
    assert!(rust.tokenizer.preserve_patterns.contains(&"::".to_string()));
    assert!(rust.tokenizer.preserve_patterns.contains(&"->".to_string()));
    assert!(
        rust.tokenizer
            .naming_styles
            .contains(&"snake_case".to_string())
    );
}

#[test]
fn test_typescript_config_has_expected_patterns() {
    let configs = LanguageConfigs::load_embedded();
    let ts = configs
        .get("typescript")
        .expect("typescript config should exist");
    assert!(ts.tokenizer.preserve_patterns.contains(&"?.".to_string()));
    assert!(ts.variants.strip_prefixes.contains(&"I".to_string()));
}

#[test]
fn test_config_defaults_for_optional_sections() {
    let toml_str = r#"
[tokenizer]
preserve_patterns = ["::"]
naming_styles = ["snake_case"]
"#;
    let config: LanguageConfig = toml::from_str(toml_str).unwrap();
    assert!(config.variants.strip_prefixes.is_empty());
    assert!(config.variants.strip_suffixes.is_empty());
    assert!(config.scoring.important_patterns.is_empty());
}

#[test]
fn test_all_preserve_patterns_collected() {
    let configs = LanguageConfigs::load_embedded();
    let all_patterns = configs.all_preserve_patterns();
    // Should include patterns from multiple languages
    assert!(
        all_patterns.contains(&"::".to_string()),
        "Missing Rust ::"
    );
    assert!(
        all_patterns.contains(&"?.".to_string()),
        "Missing TS ?."
    );
    assert!(
        all_patterns.contains(&":=".to_string()),
        "Missing Go :="
    );
}
