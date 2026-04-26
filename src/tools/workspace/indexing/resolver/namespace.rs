use super::{ParentReferenceContext, language_of};
use julie_extractors::base::{PendingRelationship, Symbol, UnresolvedTarget};

pub(super) fn score(
    candidate: &Symbol,
    pending: &PendingRelationship,
    target: Option<&UnresolvedTarget>,
    parent_ctx: &ParentReferenceContext,
) -> Option<u32> {
    let Some(target) = target else {
        return Some(0);
    };
    if target.namespace_path.is_empty() {
        return Some(0);
    }

    let root = target.namespace_path.first().map(String::as_str);
    if matches!(root, Some("std" | "core" | "alloc")) {
        let candidate_module_path = rust_module_path_from_file(&candidate.file_path);
        return path_ends_with_segments(&candidate_module_path, &target.namespace_path)
            .then_some(500);
    }

    if root == Some("crate") && language_of(&pending.file_path) == Some("rust") {
        let namespace_without_root = &target.namespace_path[1..];
        if namespace_without_root.is_empty() {
            return Some(0);
        }

        let candidate_module_path = rust_module_path_from_file(&candidate.file_path);
        if path_ends_with_segments(&candidate_module_path, namespace_without_root) {
            return Some(500);
        }

        if candidate_parent_matches_namespace(candidate, target, parent_ctx) {
            return Some(450);
        }

        return None;
    }

    Some(0)
}

fn rust_module_path_from_file(path: &str) -> Vec<String> {
    let without_ext = path.strip_suffix(".rs").unwrap_or(path);
    let mut parts: Vec<String> = without_ext
        .split('/')
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
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

fn path_ends_with_segments(path_segments: &[String], namespace_segments: &[String]) -> bool {
    path_segments.len() >= namespace_segments.len()
        && path_segments[path_segments.len() - namespace_segments.len()..]
            .iter()
            .zip(namespace_segments)
            .all(|(path, namespace)| path.eq_ignore_ascii_case(namespace))
}

fn candidate_parent_matches_namespace(
    candidate: &Symbol,
    target: &UnresolvedTarget,
    parent_ctx: &ParentReferenceContext,
) -> bool {
    let Some(parent_id) = candidate.parent_id.as_deref() else {
        return false;
    };
    let Some(parent_name) = parent_ctx.parent_names.get(parent_id) else {
        return false;
    };
    target
        .namespace_path
        .last()
        .is_some_and(|name| name == parent_name)
}
