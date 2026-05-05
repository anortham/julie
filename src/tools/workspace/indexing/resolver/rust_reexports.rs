use super::{
    namespace,
    scoring::{is_resolvable_target, language_of},
};
use julie_extractors::base::{PendingRelationship, Symbol, SymbolKind, UnresolvedTarget};
use std::collections::HashSet;

pub(super) fn select_definition<'a>(
    candidates: &'a [Symbol],
    reexport_imports: &[Symbol],
    pending: &PendingRelationship,
    target: Option<&UnresolvedTarget>,
) -> Option<&'a Symbol> {
    let target = target?;
    if language_of(&pending.file_path) != Some("rust") {
        return None;
    }
    if target.namespace_path.first().map(String::as_str) != Some("crate") {
        return None;
    }

    let reexport_namespace = &target.namespace_path[1..];
    let mut reexport_target_paths =
        direct_reexport_target_paths(reexport_imports, reexport_namespace, target);
    reexport_target_paths.extend(workspace_glob_reexport_target_paths(
        reexport_imports,
        reexport_namespace,
        target,
    ));

    let mut resolved_targets = Vec::new();
    for target_path in reexport_target_paths {
        let Some(definition_namespace) = definition_namespace_from_use_path(&target_path.segments)
        else {
            continue;
        };
        let matches = candidates
            .iter()
            .filter(|candidate| {
                candidate.name == target.terminal_name && is_resolvable_target(&candidate.kind)
            })
            .filter(|candidate| {
                target_path
                    .workspace_crate
                    .as_deref()
                    .is_none_or(|crate_name| {
                        file_is_in_workspace_crate(&candidate.file_path, crate_name)
                    })
            })
            .filter(|candidate| {
                let module_path = namespace::rust_module_path_from_file(&candidate.file_path);
                namespace::path_ends_with_segments(&module_path, &definition_namespace)
            })
            .collect::<Vec<_>>();

        if matches.len() == 1 {
            resolved_targets.push(matches[0]);
        }
    }

    resolved_targets.sort_by(|left, right| left.id.cmp(&right.id));
    resolved_targets.dedup_by(|left, right| left.id == right.id);
    match resolved_targets.as_slice() {
        [symbol] => Some(*symbol),
        _ => None,
    }
}

struct ReexportTargetPath {
    segments: Vec<String>,
    workspace_crate: Option<String>,
}

fn direct_reexport_target_paths(
    reexport_imports: &[Symbol],
    reexport_namespace: &[String],
    target: &UnresolvedTarget,
) -> Vec<ReexportTargetPath> {
    reexport_imports
        .iter()
        .filter(|candidate| {
            candidate.kind == SymbolKind::Import && candidate.name == target.terminal_name
        })
        .filter(|candidate| {
            let module_path = namespace::rust_module_path_from_file(&candidate.file_path);
            namespace::path_ends_with_segments(&module_path, reexport_namespace)
        })
        .filter_map(|candidate| {
            candidate
                .signature
                .as_deref()
                .and_then(use_signature_path_segments)
        })
        .filter(|segments| {
            segments
                .last()
                .is_some_and(|name| name == &target.terminal_name)
        })
        .map(|segments| ReexportTargetPath {
            segments,
            workspace_crate: None,
        })
        .collect()
}

fn definition_namespace_from_use_path(segments: &[String]) -> Option<Vec<String>> {
    let path_without_root = match segments.first().map(String::as_str) {
        Some("crate" | "self" | "super") => &segments[1..],
        Some(_) => segments,
        None => return None,
    };
    if path_without_root.len() < 2 {
        return None;
    }
    Some(path_without_root[..path_without_root.len() - 1].to_vec())
}

fn workspace_glob_reexport_target_paths(
    reexport_imports: &[Symbol],
    reexport_namespace: &[String],
    target: &UnresolvedTarget,
) -> Vec<ReexportTargetPath> {
    let workspace_crates = reexport_imports
        .iter()
        .filter(|candidate| candidate.kind == SymbolKind::Import)
        .filter(|candidate| {
            let module_path = namespace::rust_module_path_from_file(&candidate.file_path);
            namespace::path_ends_with_segments(&module_path, reexport_namespace)
        })
        .filter_map(|candidate| {
            candidate
                .signature
                .as_deref()
                .and_then(workspace_crate_glob_from_use_signature)
        })
        .filter(|crate_name| workspace_has_crate_root(reexport_imports, crate_name))
        .collect::<HashSet<_>>();

    let mut target_paths = Vec::new();
    for crate_name in workspace_crates {
        for candidate in reexport_imports {
            if candidate.kind != SymbolKind::Import || candidate.name != target.terminal_name {
                continue;
            }
            if !file_is_workspace_crate_root(&candidate.file_path, &crate_name) {
                continue;
            }
            let Some(segments) = candidate
                .signature
                .as_deref()
                .and_then(use_signature_path_segments)
            else {
                continue;
            };
            if segments
                .last()
                .is_some_and(|name| name == &target.terminal_name)
            {
                target_paths.push(ReexportTargetPath {
                    segments,
                    workspace_crate: Some(crate_name.clone()),
                });
            }
        }
    }

    target_paths
}

fn workspace_crate_glob_from_use_signature(signature: &str) -> Option<String> {
    let path = normalized_use_path(signature)?;
    let crate_name = path.strip_suffix("::*")?;
    if crate_name.contains("::") || matches!(crate_name, "crate" | "self" | "super") {
        return None;
    }
    Some(crate_name.to_string())
}

fn workspace_has_crate_root(candidates: &[Symbol], crate_name: &str) -> bool {
    candidates
        .iter()
        .any(|candidate| file_is_workspace_crate_root(&candidate.file_path, crate_name))
}

fn file_is_workspace_crate_root(path: &str, crate_name: &str) -> bool {
    let parts = path_parts(path);
    parts.windows(3).any(|window| {
        crate_name_matches(&window[0], crate_name)
            && window[1] == "src"
            && matches!(window[2].as_str(), "lib.rs" | "main.rs")
    })
}

fn file_is_in_workspace_crate(path: &str, crate_name: &str) -> bool {
    let parts = path_parts(path);
    parts
        .windows(2)
        .any(|window| crate_name_matches(&window[0], crate_name) && window[1] == "src")
}

fn crate_name_matches(path_segment: &str, crate_name: &str) -> bool {
    path_segment
        .replace('-', "_")
        .eq_ignore_ascii_case(crate_name)
}

fn path_parts(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn use_signature_path_segments(signature: &str) -> Option<Vec<String>> {
    let path = normalized_use_path(signature)?;
    let path = path.split(" as ").next().unwrap_or(path).trim();
    if path.is_empty() || path.contains('{') || path.ends_with("::*") {
        return None;
    }

    let segments = path
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!segments.is_empty()).then_some(segments)
}

fn normalized_use_path(signature: &str) -> Option<&str> {
    let path = signature
        .trim()
        .trim_start_matches("pub(crate) use ")
        .trim_start_matches("pub(super) use ")
        .trim_start_matches("pub use ")
        .trim_start_matches("use ")
        .trim_end_matches(';')
        .trim();
    (!path.is_empty()).then_some(path)
}
