//! Follow `pub use` when a `crate::module::leaf` call names the re-export site.

use std::collections::HashSet;

use julie_extractors::SymbolKind;

use super::resolve::is_definition;
use super::{SymbolId, SymbolTable};

/// Import symbols paired with the module path of their file. `resolve` builds
/// it once, and every unresolved `crate::` reference reads it.
pub(super) struct ReexportIndex {
    imports: Vec<(SymbolId, Vec<String>)>,
}

impl ReexportIndex {
    pub(super) fn build(symbols: &SymbolTable) -> Self {
        let mut imports = Vec::new();
        for file in 0..symbols.files().len() as u32 {
            for id in symbols.symbols_in_file(file) {
                let id = SymbolId(id);
                let row = symbols.symbol(id);
                if row.kind == SymbolKind::Import {
                    imports.push((id, module_path(&row.path)));
                }
            }
        }
        Self { imports }
    }

    fn ids(&self) -> impl Iterator<Item = SymbolId> + '_ {
        self.imports.iter().map(|(id, _)| *id)
    }

    fn in_module<'a>(&'a self, qualifier: &'a [&str]) -> impl Iterator<Item = SymbolId> + 'a {
        self.imports
            .iter()
            .filter(move |(_, module)| path_ends_with(module, qualifier))
            .map(|(id, _)| *id)
    }
}

/// Unique definition reached through a named or glob `pub use`, else `None`.
pub fn resolve_reexport(
    symbols: &SymbolTable,
    index: &ReexportIndex,
    name: &str,
    from_file: u32,
) -> Option<SymbolId> {
    if file_language(symbols, from_file) != Some("rust") || !name.starts_with("crate::") {
        return None;
    }
    let (leaf, qualifier) = super::resolve::split_qualified(name);
    if qualifier.is_empty() {
        return None;
    }
    let mut targets = direct_targets(symbols, index, leaf, &qualifier);
    targets.extend(glob_targets(symbols, index, leaf, &qualifier));
    targets.sort();
    targets.dedup();
    match targets.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}

fn file_language(symbols: &SymbolTable, file: u32) -> Option<&str> {
    symbols.files()[file as usize]
        .symbols
        .first()
        .map(|row| row.language.as_str())
}

fn module_path(path: &str) -> Vec<String> {
    let stem = path.strip_suffix(".rs").unwrap_or(path);
    let mut parts: Vec<String> = stem
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    if matches!(parts.first().map(String::as_str), Some("src" | "Sources")) {
        parts.remove(0);
    }
    if matches!(
        parts.last().map(String::as_str),
        Some("mod" | "lib" | "main")
    ) {
        parts.pop();
    }
    parts
}

fn path_ends_with(path: &[String], namespace: &[&str]) -> bool {
    path.len() >= namespace.len()
        && path[path.len() - namespace.len()..]
            .iter()
            .zip(namespace)
            .all(|(part, want)| part.eq_ignore_ascii_case(want))
}

fn use_path(signature: &str) -> Option<String> {
    let path = signature
        .trim()
        .trim_start_matches("pub(crate) use ")
        .trim_start_matches("pub(super) use ")
        .trim_start_matches("pub use ")
        .trim_start_matches("use ")
        .trim_end_matches(';')
        .trim();
    let path = path.split(" as ").next().unwrap_or(path).trim();
    (!path.is_empty()).then(|| path.to_string())
}

fn use_segments(signature: &str) -> Option<Vec<String>> {
    let path = use_path(signature)?;
    if path.contains('{') || path.ends_with("::*") {
        return None;
    }
    let segments: Vec<String> = path
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect();
    (!segments.is_empty()).then_some(segments)
}

fn definition_namespace(segments: &[String]) -> Option<Vec<&str>> {
    let rest = match segments.first().map(String::as_str) {
        Some("crate" | "self" | "super") => &segments[1..],
        Some(_) => segments,
        None => return None,
    };
    (rest.len() >= 2).then(|| rest[..rest.len() - 1].iter().map(String::as_str).collect())
}

fn crate_name_matches(segment: &str, crate_name: &str) -> bool {
    segment.replace('-', "_").eq_ignore_ascii_case(crate_name)
}

fn crate_src_index(parts: &[&str], crate_name: &str) -> Option<usize> {
    if parts.len() >= 2 && crate_name_matches(parts[0], crate_name) && parts[1] == "src" {
        return Some(1);
    }
    if parts.len() >= 3
        && parts[0] == "crates"
        && crate_name_matches(parts[1], crate_name)
        && parts[2] == "src"
    {
        return Some(2);
    }
    None
}

fn path_parts(path: &str) -> Vec<&str> {
    path.split('/').filter(|part| !part.is_empty()).collect()
}

fn file_is_crate_root(path: &str, crate_name: &str) -> bool {
    let parts = path_parts(path);
    crate_src_index(&parts, crate_name)
        .is_some_and(|src| matches!(parts.get(src + 1).copied(), Some("lib.rs" | "main.rs")))
}

fn file_is_in_crate(path: &str, crate_name: &str) -> bool {
    crate_src_index(&path_parts(path), crate_name).is_some()
}

fn glob_crate_name(signature: &str) -> Option<String> {
    let path = use_path(signature)?;
    let crate_name = path.strip_suffix("::*")?;
    if crate_name.contains("::") || matches!(crate_name, "crate" | "self" | "super") {
        return None;
    }
    Some(crate_name.to_string())
}

fn definition_in_namespace(
    symbols: &SymbolTable,
    leaf: &str,
    namespace: &[&str],
    crate_name: Option<&str>,
) -> Option<SymbolId> {
    let matches: Vec<SymbolId> = symbols
        .find_by_name(leaf)
        .iter()
        .copied()
        .filter(|id| is_definition(&symbols.symbol(*id).kind))
        .filter(|id| {
            crate_name.is_none_or(|name| file_is_in_crate(&symbols.symbol(*id).path, name))
        })
        .filter(|id| path_ends_with(&module_path(&symbols.symbol(*id).path), namespace))
        .collect();
    match matches.as_slice() {
        [id] => Some(*id),
        _ => None,
    }
}

fn follow_named_use(
    symbols: &SymbolTable,
    import: SymbolId,
    leaf: &str,
    crate_name: Option<&str>,
) -> Option<SymbolId> {
    let row = symbols.symbol(import);
    if row.kind != SymbolKind::Import || row.name != leaf {
        return None;
    }
    let segments = use_segments(row.signature.as_deref()?)?;
    if segments.last().map(String::as_str) != Some(leaf) {
        return None;
    }
    let namespace = definition_namespace(&segments)?;
    definition_in_namespace(symbols, leaf, &namespace, crate_name)
}

fn direct_targets(
    symbols: &SymbolTable,
    index: &ReexportIndex,
    leaf: &str,
    qualifier: &[&str],
) -> Vec<SymbolId> {
    index
        .in_module(qualifier)
        .filter_map(|id| follow_named_use(symbols, id, leaf, None))
        .collect()
}

fn glob_targets(
    symbols: &SymbolTable,
    index: &ReexportIndex,
    leaf: &str,
    qualifier: &[&str],
) -> Vec<SymbolId> {
    let crates: HashSet<String> = index
        .in_module(qualifier)
        .filter_map(|id| glob_crate_name(symbols.symbol(id).signature.as_deref()?))
        .filter(|name| {
            index
                .ids()
                .any(|id| file_is_crate_root(&symbols.symbol(id).path, name))
        })
        .collect();
    let mut targets = Vec::new();
    for crate_name in crates {
        for import in index.ids() {
            if !file_is_crate_root(&symbols.symbol(import).path, &crate_name) {
                continue;
            }
            if let Some(id) = follow_named_use(symbols, import, leaf, Some(&crate_name)) {
                targets.push(id);
            }
        }
    }
    targets
}
