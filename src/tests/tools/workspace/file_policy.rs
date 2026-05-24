use crate::tools::workspace::indexing::file_policy::{
    ExtractionMode, detect_language_for_indexing_with_content, determine_extraction_mode,
    should_watch_path,
};
use crate::watcher::filtering::build_supported_extensions;
use std::fs;

#[test]
fn test_determine_extraction_mode_parser_backed_source_uses_parser() {
    let mode = determine_extraction_mode("rust", "fn main() { helper(); }\n");
    assert_eq!(mode, ExtractionMode::ParserBacked);
}

#[test]
fn test_detect_language_for_indexing_with_content_prefers_cpp_h_header() {
    let content = r#"
#pragma once
namespace app {
class Widget {
public:
    void run() const;
};
}
"#;

    assert_eq!(
        detect_language_for_indexing_with_content(
            std::path::Path::new("include/widget.h"),
            content,
        ),
        "cpp",
        "indexing language detection should use source-aware parser language for C++ .h headers"
    );
}

#[test]
fn test_determine_extraction_mode_data_languages_use_parser() {
    let css = determine_extraction_mode("css", ".button { color: red; }\n");
    let html = determine_extraction_mode("html", "<main><h1>Worker</h1></main>\n");
    assert_eq!(css, ExtractionMode::ParserBacked);
    assert_eq!(html, ExtractionMode::ParserBacked);
}

#[test]
fn test_determine_extraction_mode_text_language_is_text_only() {
    let mode = determine_extraction_mode("text", "plain text\n");
    assert_eq!(mode, ExtractionMode::TextOnly);
}

#[test]
fn test_determine_extraction_mode_oversized_parser_file_falls_back_to_text_only() {
    let oversized = "a".repeat(5_000_001);
    let mode = determine_extraction_mode("rust", &oversized);
    assert_eq!(mode, ExtractionMode::TextOnly);
}

#[test]
fn test_determine_extraction_mode_minified_parser_file_falls_back_to_text_only() {
    let minified = format!("function x(){{return 1;}}{}\n", "a".repeat(25_000));
    let mode = determine_extraction_mode("javascript", &minified);
    assert_eq!(mode, ExtractionMode::TextOnly);
}

#[test]
fn test_determine_extraction_mode_markdown_long_lines_stays_parser_backed() {
    let mut content = String::from("# Heading\n\n");
    for _ in 0..8 {
        content.push_str(&"a ".repeat(300));
        content.push('\n');
    }
    let mode = determine_extraction_mode("markdown", &content);
    assert_eq!(mode, ExtractionMode::ParserBacked);
}

#[test]
fn test_should_watch_path_accepts_extensionless_text_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("README");
    fs::write(&file_path, "plain text content\n").unwrap();
    let supported = build_supported_extensions();

    assert!(
        should_watch_path(&file_path, &supported),
        "extensionless text files should be watched/indexed for text-only parity"
    );
}

#[test]
fn test_should_watch_path_accepts_unsupported_text_extension() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("notes.txt");
    fs::write(&file_path, "plain text content\n").unwrap();
    let supported = build_supported_extensions();

    assert!(
        should_watch_path(&file_path, &supported),
        "unsupported-but-text files should be watched/indexed for text-only parity"
    );
}

#[test]
fn test_should_watch_path_rejects_blacklisted_filename() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("pnpm-lock.yaml");
    fs::write(&file_path, "lockfileVersion: '9.0'\n").unwrap();
    let supported = build_supported_extensions();

    assert!(
        !should_watch_path(&file_path, &supported),
        "blacklisted lockfiles must remain excluded"
    );
}

#[test]
fn test_should_watch_path_rejects_blacklisted_extension_even_when_text() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("diagram.svg");
    fs::write(&file_path, "<svg><text>source-looking text</text></svg>\n").unwrap();
    let supported = build_supported_extensions();

    assert!(
        !should_watch_path(&file_path, &supported),
        "blacklisted text formats must not slip through unsupported text fallback"
    );
}
