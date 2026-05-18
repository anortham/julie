use std::path::Path;

use serde::Serialize;

#[derive(Serialize)]
struct CorpusRoots<'a> {
    roots: &'a [String],
}

pub fn toml_roots(roots: &[String]) -> String {
    toml::to_string(&CorpusRoots { roots }).expect("serialize TOML roots")
}

pub fn toml_roots_from_paths(roots: &[&Path]) -> String {
    let roots = roots
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    toml_roots(&roots)
}
