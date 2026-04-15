use crate::extract_canonical;
use crate::manager::ExtractorManager;
use std::path::PathBuf;

#[test]
fn test_public_api_surface_projects_canonical_results() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file_path = "src/app.ts";
    let content = r#"
import { externalHelper } from "./external";

export function localHelper(input: number): number {
    return input + 1;
}

export function processData(): number {
    return localHelper(externalHelper(41));
}
"#;

    let manager = ExtractorManager::new();
    let canonical = extract_canonical(file_path, content, &workspace_root)
        .expect("canonical extraction should succeed");
    let all_results = manager
        .extract_all(file_path, content, &workspace_root)
        .expect("manager extract_all should use the canonical pipeline");

    assert!(
        !canonical.structured_pending_relationships.is_empty(),
        "parity coverage should exercise a canonical result with structured unresolved relationships"
    );
    assert_eq!(
        canonical.pending_relationships,
        canonical
            .structured_pending_relationships
            .clone()
            .into_iter()
            .map(|pending| pending.into_pending_relationship())
            .collect::<Vec<_>>(),
        "canonical extraction should keep the degraded compatibility payload aligned with structured unresolved entries"
    );

    assert_eq!(all_results.symbols, canonical.symbols);
    assert_eq!(all_results.identifiers, canonical.identifiers);
    assert_eq!(all_results.relationships, canonical.relationships);
    assert_eq!(
        all_results.pending_relationships,
        canonical.pending_relationships
    );
    assert_eq!(
        all_results.structured_pending_relationships,
        canonical.structured_pending_relationships
    );
    assert_eq!(all_results.types, canonical.types);

    let symbols = manager
        .extract_symbols(file_path, content, &workspace_root)
        .expect("symbol projection should succeed");
    let identifiers = manager
        .extract_identifiers(file_path, content, &workspace_root)
        .expect("identifier projection should succeed");
    let relationships = manager
        .extract_relationships(file_path, content, &workspace_root)
        .expect("relationship projection should succeed");

    assert_eq!(symbols, canonical.symbols);
    assert_eq!(identifiers, canonical.identifiers);
    assert_eq!(relationships, canonical.relationships);
}

#[test]
fn test_public_api_surface_preserves_structured_pending_for_remaining_registry_wave() {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let cases = [
        (
            "src/main.c",
            r#"
int main_function() {
    int result = helper_function(21);
    return result;
}
"#,
            "helper_function",
            None,
        ),
        (
            "src/main.cpp",
            r#"
int main_function() {
    int result = helper_function(21);
    return result;
}
"#,
            "helper_function",
            None,
        ),
        (
            "src/processor.rs",
            r#"
pub fn process() -> i32 {
    let calc = Calculator::new(21);
    calc.double()
}
"#,
            "calc.double",
            Some("calc"),
        ),
        (
            "src/main.zig",
            r#"
const util = @import("util.zig");

fn main_function() i32 {
    const result = util.helper_function(21);
    return result;
}
"#,
            "util.helper_function",
            Some("util"),
        ),
        (
            "main.go",
            r#"
package main

import "myapp/utils"

func MainFunction() int {
    result := utils.HelperFunction(21)
    return result
}
"#,
            "utils.HelperFunction",
            Some("utils"),
        ),
        (
            "lib/processor.py",
            r#"
def process():
    calc = Calculator(21)
    return calc.double()
"#,
            "calc.double",
            Some("calc"),
        ),
        (
            "lib/processor.rb",
            r#"
def process
  calc = Calculator.new(21)
  result = calc.double()
  result
end
"#,
            "Calculator.new",
            Some("Calculator"),
        ),
        (
            "lib/processor.gd",
            r#"
func process():
    var result = external_helper(21)
    return result
"#,
            "external_helper",
            None,
        ),
        (
            "lib/processor.dart",
            r#"
import 'utils.dart';

int mainFunction() {
    final result = helperFunction(21);
    return result;
}
"#,
            "helperFunction",
            None,
        ),
    ];

    let manager = ExtractorManager::new();

    for (file_path, content, expected_display_name, expected_receiver) in cases {
        let canonical =
            extract_canonical(file_path, content, &workspace_root).unwrap_or_else(|err| {
                panic!("canonical extraction should succeed for {file_path}: {err}")
            });
        let all_results = manager
            .extract_all(file_path, content, &workspace_root)
            .unwrap_or_else(|err| {
                panic!("manager extract_all should succeed for {file_path}: {err}")
            });

        let structured_pending = canonical
            .structured_pending_relationships
            .iter()
            .find(|pending| pending.target.display_name == expected_display_name)
            .unwrap_or_else(|| {
                panic!(
                    "canonical extraction should preserve structured pending target {expected_display_name} for {file_path}; found {:?}",
                    canonical
                        .structured_pending_relationships
                        .iter()
                        .map(|pending| pending.target.display_name.as_str())
                        .collect::<Vec<_>>()
                )
            });

        assert_eq!(
            structured_pending.target.receiver.as_deref(),
            expected_receiver,
            "canonical extraction should preserve receiver context for {file_path}"
        );
        assert_eq!(
            canonical.pending_relationships,
            canonical
                .structured_pending_relationships
                .clone()
                .into_iter()
                .map(|pending| pending.into_pending_relationship())
                .collect::<Vec<_>>(),
            "canonical extraction should keep degraded compatibility payload aligned for {file_path}"
        );
        assert_eq!(
            all_results.structured_pending_relationships,
            canonical.structured_pending_relationships,
            "manager extract_all should match canonical structured pending output for {file_path}"
        );
        assert_eq!(
            all_results.pending_relationships, canonical.pending_relationships,
            "manager extract_all should match canonical compatibility pending output for {file_path}"
        );
    }
}
