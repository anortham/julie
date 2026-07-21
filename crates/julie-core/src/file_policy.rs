use crate::shared::{BLACKLISTED_EXTENSIONS, BLACKLISTED_FILENAMES};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::OnceLock;

const HARD_SIZE_CAP: usize = 5_000_000; // 5 MiB absolute safety rail
const MINIFIED_AVG_LINE_LEN: usize = 200;
const MINIFIED_MAX_LINE_LEN: usize = 20_000;
const MINIFIED_LONG_LINE_RATIO: f64 = 0.20;
const LONG_LINE_THRESHOLD: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractionMode {
    ParserBacked,
    TextOnly,
}

pub fn detect_language_for_indexing(path: &Path) -> String {
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        if let Some(lang) = julie_extractors::language::detect_language_from_extension(ext) {
            return lang.to_string();
        }
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");

    match file_name.to_lowercase().as_str() {
        "dockerfile" | "containerfile" => "dockerfile".to_string(),
        "makefile" | "gnumakefile" => "makefile".to_string(),
        "cargo.toml" | "cargo.lock" => "toml".to_string(),
        "package.json" | "tsconfig.json" | "jsconfig.json" => "json".to_string(),
        name if name.starts_with("bash") || name.contains("bashrc") || name.contains("bash_") => {
            "bash".to_string()
        }
        _ => "text".to_string(),
    }
}

pub fn detect_language_for_indexing_with_content(path: &Path, content: &str) -> String {
    let path_str = path.to_string_lossy();
    julie_extractors::language::detect_language_for_source(&path_str, content)
        .map(str::to_string)
        .unwrap_or_else(|| detect_language_for_indexing(path))
}

pub fn supported_extensions_for_indexing() -> &'static HashSet<String> {
    static SUPPORTED_EXTENSIONS: OnceLock<HashSet<String>> = OnceLock::new();
    SUPPORTED_EXTENSIONS.get_or_init(|| {
        julie_extractors::language::supported_extensions()
            .iter()
            .map(|ext| ext.to_lowercase())
            .collect()
    })
}

pub fn allows_blacklisted_extension(file_name: &str) -> bool {
    file_name.eq_ignore_ascii_case("Cargo.lock")
}

pub fn determine_extraction_mode(language: &str, content: &str) -> ExtractionMode {
    if content.trim().is_empty()
        || julie_extractors::language::get_tree_sitter_language(language).is_err()
    {
        return ExtractionMode::TextOnly;
    }

    let skip_minified_check = matches!(language, "markdown");
    let too_large = content.len() > HARD_SIZE_CAP;
    let minified = !skip_minified_check
        && is_likely_minified_or_generated(
            content,
            MINIFIED_AVG_LINE_LEN,
            MINIFIED_MAX_LINE_LEN,
            MINIFIED_LONG_LINE_RATIO,
            LONG_LINE_THRESHOLD,
        );

    if too_large || minified {
        ExtractionMode::TextOnly
    } else {
        ExtractionMode::ParserBacked
    }
}

pub fn should_index_path_candidate(path: &Path, supported_extensions: &HashSet<String>) -> bool {
    if is_project_local_julie_state(path) {
        return false;
    }

    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        if BLACKLISTED_FILENAMES.contains(&file_name) {
            return false;
        }
        if allows_blacklisted_extension(file_name) {
            return true;
        }
    }

    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext = ext.to_lowercase();
        let dotted_ext = format!(".{ext}");
        if BLACKLISTED_EXTENSIONS.contains(&dotted_ext.as_str()) {
            return false;
        }
        if supported_extensions.contains(ext.as_str()) {
            return true;
        }
        return crate::file_utils::is_likely_text_file(path);
    }

    crate::file_utils::is_likely_text_file(path)
}

fn is_project_local_julie_state(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == OsStr::new(".julie"))
}

pub fn should_watch_path(path: &Path, supported_extensions: &HashSet<String>) -> bool {
    should_index_path_candidate(path, supported_extensions)
}

pub fn should_process_deleted_path(path: &Path, _supported_extensions: &HashSet<String>) -> bool {
    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        return !BLACKLISTED_FILENAMES.contains(&file_name);
    }
    true
}

fn is_likely_minified_or_generated(
    content: &str,
    avg_threshold: usize,
    max_threshold: usize,
    long_ratio_threshold: f64,
    long_line_len: usize,
) -> bool {
    let mut line_count: usize = 0;
    let mut long_lines: usize = 0;
    let mut max_line: usize = 0;

    for line in content.lines() {
        let len = line.len();
        line_count += 1;
        if len > max_line {
            max_line = len;
        }
        if len > long_line_len {
            long_lines += 1;
        }
    }

    if line_count == 0 {
        return false;
    }

    if max_line > max_threshold {
        return true;
    }

    let avg_line = content.len() / line_count;
    if avg_line > avg_threshold {
        return true;
    }

    let ratio = long_lines as f64 / line_count as f64;
    ratio > long_ratio_threshold
}
