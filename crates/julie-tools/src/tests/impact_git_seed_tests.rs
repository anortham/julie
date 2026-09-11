use crate::impact::BlastRadiusTool;
use crate::impact::git_seed::changed_files_from_git_output;

#[test]
fn blast_radius_with_no_seed_uses_the_git_diff() {
    let tool: BlastRadiusTool = serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(tool.seeds_from_git());
    let explicit: BlastRadiusTool =
        serde_json::from_value(serde_json::json!({ "file_paths": ["src/a.rs"] })).unwrap();
    assert!(!explicit.seeds_from_git());
}

#[test]
fn changed_files_merges_diff_and_untracked_output() {
    let paths = changed_files_from_git_output("src/a.rs\nsrc/b.rs\n", "new.rs\n\n");
    assert_eq!(paths, vec!["src/a.rs", "src/b.rs", "new.rs"]);
}
