use crate::factory::extract_symbols_and_relationships;
use crate::tests::helpers::init_parser;
use crate::ExtractionResults;
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
