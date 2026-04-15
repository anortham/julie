use crate::extract_canonical;

#[test]
fn test_symbols_store_exact_relative_paths_across_languages() {
    let workspace_root = std::path::PathBuf::from("/test/workspace");

    let rust_results = extract_canonical(
        "src/lib.rs",
        "pub fn greet() -> &'static str { \"hi\" }",
        &workspace_root,
    )
    .expect("rust extraction should succeed");
    assert!(
        rust_results
            .symbols
            .iter()
            .all(|symbol| symbol.file_path == "src/lib.rs")
    );

    let ts_results = extract_canonical(
        "packages/app/src/service.ts",
        "export function getUserData(id: string) { return id }",
        &workspace_root,
    )
    .expect("typescript extraction should succeed");
    assert!(
        ts_results
            .symbols
            .iter()
            .all(|symbol| symbol.file_path == "packages/app/src/service.ts")
    );
}

#[test]
fn test_root_level_paths_stay_as_plain_filenames() {
    let workspace_root = std::path::PathBuf::from("/test/workspace");
    let results = extract_canonical(
        "index.ts",
        "export const VERSION = '1.0.0';",
        &workspace_root,
    )
    .expect("typescript extraction should succeed");

    let version = results
        .symbols
        .iter()
        .find(|symbol| symbol.name == "VERSION")
        .expect("should extract VERSION");
    assert_eq!(version.file_path, "index.ts");
}
