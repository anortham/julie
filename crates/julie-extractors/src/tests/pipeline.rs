use crate::ExtractionResults;
use crate::base::{ParseDiagnosticKind, SymbolKind};
use crate::factory::extract_symbols_and_relationships;
use crate::tests::helpers::init_parser;
use std::path::{Path, PathBuf};

fn extract_legacy(
    file_path: &str,
    language: &str,
    content: &str,
    workspace_root: &Path,
) -> ExtractionResults {
    let tree = init_parser(content, language);
    extract_symbols_and_relationships(&tree, file_path, content, language, workspace_root)
        .expect("legacy extraction should succeed")
}

fn assert_paths_are_normalized(results: &ExtractionResults, expected_file_path: &str) {
    assert!(!results.symbols.is_empty(), "expected extracted symbols");
    assert!(
        results
            .symbols
            .iter()
            .all(|symbol| symbol.file_path == expected_file_path),
        "expected all symbols to use normalized path {expected_file_path:?}, got {:?}",
        results
            .symbols
            .iter()
            .map(|symbol| symbol.file_path.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_extract_canonical_matches_legacy_factory_for_representative_languages() {
    let workspace_root = PathBuf::from("/test/workspace");
    let cases = [
        (
            "rust",
            "src/lib.rs",
            r#"
use crate::external::external_helper;

pub fn local_helper(input: i32) -> i32 {
    input + 1
}

pub fn process_data() -> Result<Vec<u8>, std::io::Error> {
    let value = local_helper(external_helper(41));
    Ok(vec![value as u8])
}
"#,
            vec!["local_helper", "process_data"],
            true,
            true,
            true,
            true,
        ),
        (
            "typescript",
            "src/app.ts",
            r#"
import { externalHelper } from "./external";

export function localHelper(input: number): number {
    return input + 1;
}

export function caller(): number {
    return localHelper(externalHelper(41));
}
"#,
            vec!["localHelper", "caller"],
            true,
            true,
            true,
            true,
        ),
        (
            "python",
            "src/app.py",
            r#"
from external import external_helper

def local_helper(input: int) -> int:
    return input + 1

def caller() -> int:
    return local_helper(external_helper(41))
"#,
            vec!["local_helper", "caller"],
            true,
            true,
            true,
            true,
        ),
    ];

    for (
        language,
        file_path,
        content,
        expected_symbol_names,
        expect_identifiers,
        expect_relationships,
        expect_pending_relationships,
        expect_types,
    ) in cases
    {
        let legacy = extract_legacy(file_path, language, content, &workspace_root);
        let canonical = crate::pipeline::extract_canonical(file_path, content, &workspace_root)
            .expect("canonical extraction should succeed");

        let legacy_names: Vec<_> = legacy
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect();
        let canonical_names: Vec<_> = canonical
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect();

        assert_eq!(
            canonical_names, legacy_names,
            "symbol names should stay in parity for {language}"
        );
        for expected_symbol_name in expected_symbol_names {
            assert!(
                canonical_names.contains(&expected_symbol_name),
                "expected symbol {expected_symbol_name:?} for {language}, got {:?}",
                canonical_names
            );
        }

        assert_paths_are_normalized(&canonical, file_path);

        assert_eq!(
            !canonical.identifiers.is_empty(),
            expect_identifiers,
            "identifier presence mismatch for {language}"
        );
        assert_eq!(
            !canonical.relationships.is_empty(),
            expect_relationships,
            "relationship presence mismatch for {language}"
        );
        assert_eq!(
            !canonical.pending_relationships.is_empty(),
            expect_pending_relationships,
            "pending relationship presence mismatch for {language}"
        );
        assert_eq!(
            !canonical.types.is_empty(),
            expect_types,
            "type presence mismatch for {language}"
        );

        assert_eq!(
            !legacy.identifiers.is_empty(),
            !canonical.identifiers.is_empty(),
            "legacy and canonical identifiers presence should match for {language}"
        );
        assert_eq!(
            !legacy.relationships.is_empty(),
            !canonical.relationships.is_empty(),
            "legacy and canonical relationships presence should match for {language}"
        );
        assert_eq!(
            !legacy.pending_relationships.is_empty(),
            !canonical.pending_relationships.is_empty(),
            "legacy and canonical pending relationship presence should match for {language}"
        );
        assert_eq!(
            !legacy.types.is_empty(),
            !canonical.types.is_empty(),
            "legacy and canonical types presence should match for {language}"
        );
    }
}

#[test]
fn test_extract_canonical_records_parse_diagnostics_without_dropping_recovered_symbols() {
    let workspace_root = PathBuf::from("/test/workspace");
    let content = r#"
package main

type Empty struct{}

type EmbeddedStruct struct {
    Empty
    value int
}

type MissingBrace struct {
    field int

func VariadicFunction(format string, args ...interface{}) {
    fmt.Printf(format, args...)
}
"#;

    let results = crate::pipeline::extract_canonical("src/recovery.go", content, &workspace_root)
        .expect("canonical extraction should recover partial Go syntax");

    assert!(
        !results.parse_diagnostics.is_empty(),
        "malformed recovered parse should record parse diagnostics"
    );
    assert!(
        results
            .parse_diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == ParseDiagnosticKind::Error),
        "malformed recovered parse should include an error diagnostic: {:?}",
        results.parse_diagnostics
    );

    let names: Vec<_> = results
        .symbols
        .iter()
        .map(|symbol| symbol.name.as_str())
        .collect();
    for expected_name in ["Empty", "EmbeddedStruct", "VariadicFunction"] {
        assert!(
            names.contains(&expected_name),
            "recovered parse should keep symbol {expected_name:?}; got {names:?}"
        );
    }
}

#[test]
fn test_extract_canonical_parse_none_returns_degraded_result_with_diagnostic() {
    let workspace_root = PathBuf::from("/test/workspace");
    let content = "fn main() {\n    println!(\"unterminated parse\")";

    let results = crate::pipeline::extract_canonical_with_parse(
        "src/broken.rs",
        content,
        &workspace_root,
        |_language, _file_path, _content| Ok(None),
    )
    .expect("parser None should return a degraded extraction result");

    assert!(results.symbols.is_empty());
    assert!(results.relationships.is_empty());
    assert!(results.identifiers.is_empty());
    assert_eq!(results.parse_diagnostics.len(), 1);

    let diagnostic = &results.parse_diagnostics[0];
    assert_eq!(diagnostic.kind, ParseDiagnosticKind::Error);
    assert_eq!(diagnostic.start_line, 1);
    assert_eq!(diagnostic.start_column, 0);
    assert_eq!(diagnostic.start_byte, 0);
    assert_eq!(diagnostic.end_byte, content.len() as u32);
    assert_eq!(diagnostic.end_line, 2);
    assert_eq!(
        diagnostic.end_column,
        "    println!(\"unterminated parse\")".len() as u32
    );
}

#[test]
fn test_h_header_with_cpp_syntax_routes_to_cpp_extractor() {
    let workspace_root = PathBuf::from("/test/workspace");
    let content = r#"
#pragma once

namespace app {
class Widget {
public:
    void run() const;
};
}
"#;

    let results = crate::pipeline::extract_canonical("include/widget.h", content, &workspace_root)
        .expect("C++ header extraction should succeed");

    let widget = results
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Widget" && symbol.kind == SymbolKind::Class)
        .unwrap_or_else(|| {
            panic!(
                "expected C++ class symbol in .h header: {:?}",
                results.symbols
            )
        });
    assert_eq!(widget.language, "cpp");
    assert!(
        results.symbols.iter().any(|symbol| {
            symbol.name == "run" && symbol.kind == SymbolKind::Method && symbol.language == "cpp"
        }),
        "expected C++ method symbol in .h header: {:?}",
        results.symbols
    );
}

#[test]
fn test_h_header_with_c_syntax_stays_c_extractor() {
    let workspace_root = PathBuf::from("/test/workspace");
    let content = r#"
#ifndef WIDGET_H
#define WIDGET_H

typedef struct widget {
    int id;
} widget_t;

void widget_init(widget_t *widget);

#endif
"#;

    let results = crate::pipeline::extract_canonical("include/widget.h", content, &workspace_root)
        .expect("C header extraction should succeed");

    assert!(
        results.symbols.iter().any(|symbol| {
            symbol.name == "widget_t" && symbol.kind == SymbolKind::Struct && symbol.language == "c"
        }),
        "expected C struct typedef symbol in .h header: {:?}",
        results.symbols
    );
    assert!(
        results.symbols.iter().any(|symbol| {
            symbol.name == "widget_init"
                && symbol.kind == SymbolKind::Function
                && symbol.language == "c"
        }),
        "expected C function declaration symbol in .h header: {:?}",
        results.symbols
    );
}

#[test]
fn test_detect_language_for_source_routes_cpp_h_header_and_preserves_c_header() {
    let cpp_header = r#"
#pragma once

namespace app {
class Widget {
public:
    void run() const;
};
}
"#;
    let c_header = r#"
#ifndef WIDGET_H
#define WIDGET_H

typedef struct widget {
    int id;
} widget_t;

void widget_init(widget_t *widget);

#endif
"#;

    assert_eq!(
        crate::language::detect_language_for_source("include/widget.h", cpp_header),
        Some("cpp"),
        "public source-aware language detection should route C++ .h headers to cpp"
    );
    assert_eq!(
        crate::language::detect_language_for_source("include/widget.h", c_header),
        Some("c"),
        "public source-aware language detection should preserve path-only C default for C .h headers"
    );
}
