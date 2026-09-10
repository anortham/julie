//! `find_references` over a snapshot: limits, `reference_kind` filters,
//! identifier sites, and qualified names, on trees the real extractors index.

#[cfg(test)]
mod tests {
    use std::fs;

    use julie_test_support::SnapshotFixture;
    use tempfile::TempDir;

    use crate::tools::navigation::{FoundReferences, find_references};

    fn refs(
        files: &[(&str, &str)],
        symbol: &str,
        limit: u32,
        reference_kind: Option<&str>,
    ) -> FoundReferences {
        let dir = TempDir::new().unwrap();
        for (path, content) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, content).unwrap();
        }
        let fixture = SnapshotFixture::from_tree(dir.path()).unwrap();
        find_references(&fixture.snapshot(), symbol, limit, reference_kind)
    }

    fn caller(name: &str, callee: &str) -> String {
        format!("fn {name}() {{\n    {callee}();\n}}\n")
    }

    mod async_target_workspace;
    mod identifier_refs;
    mod kind_filtering;
    mod limits;
    mod qualified_names;
}
