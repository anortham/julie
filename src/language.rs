//! Language Support - Re-exports from julie-extractors crate
//!
//! This module re-exports the language utilities from julie-extractors for backward compatibility.

pub use julie_extractors::{detect_language_for_path, detect_language_for_source};

pub fn detect_language_from_extension(ext: &str) -> Option<&'static str> {
    let dummy_path = std::path::PathBuf::from(format!("probe.{ext}"));
    detect_language_for_path(&dummy_path, "")
}

pub fn get_tree_sitter_language(lang: &str) -> anyhow::Result<tree_sitter::Language> {
    let snapshot = julie_extractors::capability_snapshot();
    let entry = snapshot
        .languages()
        .find(|l| l.language.eq_ignore_ascii_case(lang));
    if let Some(entry) = entry {
        if let Some(ext) = entry.extensions.first() {
            let dummy_path = std::path::PathBuf::from(format!("probe.{ext}"));
            if let Ok(parsed) = julie_extractors::syntax::parse_source(&dummy_path, "") {
                return Ok(parsed.tree.language().clone());
            }
        }
    }
    anyhow::bail!("unsupported language: {lang}")
}
