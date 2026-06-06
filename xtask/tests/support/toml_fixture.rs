use serde::Serialize;

#[derive(Serialize)]
struct CorpusRoots<'a> {
    roots: &'a [String],
}

pub fn toml_roots(roots: &[String]) -> String {
    toml::to_string(&CorpusRoots { roots }).expect("serialize TOML roots")
}
