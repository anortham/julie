use std::path::{Path, PathBuf};

const MANIFESTS: &[&str] = &[
    "Cargo.toml",
    "crates/julie-core/Cargo.toml",
    "crates/julie-index/Cargo.toml",
    "crates/julie-pipeline/Cargo.toml",
    "crates/julie-runtime/Cargo.toml",
    "crates/julie-tools/Cargo.toml",
];

fn repo_file(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(path)
}

#[test]
fn extractor_dependency_release_is_v2_16_0() {
    for manifest in MANIFESTS {
        let contents = std::fs::read_to_string(repo_file(manifest)).unwrap();
        let parsed: toml::Value = toml::from_str(&contents).unwrap();
        let dependency = parsed
            .get("dependencies")
            .and_then(|value| value.get("julie-extractors"))
            .unwrap_or_else(|| panic!("{manifest} has no julie-extractors dependency"));

        assert_eq!(
            dependency.get("tag").and_then(toml::Value::as_str),
            Some("v2.16.0"),
            "{manifest} must pin v2.16.0"
        );
        assert_eq!(
            dependency.get("git").and_then(toml::Value::as_str),
            Some("https://github.com/anortham/julie-extractors"),
            "{manifest} must use the canonical upstream"
        );
    }
}
