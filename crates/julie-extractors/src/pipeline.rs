use crate::ExtractionResults;
use std::path::Path;
use tree_sitter::{Parser, Tree};

use crate::base::RecordOffset;

pub fn extract_canonical(
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    if file_path.ends_with(".jsonl") {
        return extract_jsonl_canonical(file_path, content, workspace_root);
    }

    let (language, tree) = parse_file(file_path, content)?;
    crate::registry::extract_for_language(language, &tree, file_path, content, workspace_root)
}

fn extract_jsonl_canonical(
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    let mut results = ExtractionResults::empty();

    for (line_delta, byte_delta, line) in jsonl_records(content) {
        let tree = parse_for_language("json", file_path, line)?;
        let mut record_results =
            crate::registry::extract_for_language("json", &tree, file_path, line, workspace_root)?;
        record_results.apply_record_offset(RecordOffset {
            line_delta,
            byte_delta,
        });
        record_results.rekey_normalized_locations();
        results.extend(record_results);
    }

    Ok(results)
}

fn jsonl_records(content: &str) -> Vec<(u32, u32, &str)> {
    let mut records = Vec::new();
    let mut byte_offset = 0u32;
    let mut line_offset = 0u32;

    for chunk in content.split_inclusive('\n') {
        let line = chunk.strip_suffix('\n').unwrap_or(chunk);
        let line = line.strip_suffix('\r').unwrap_or(line);

        if !line.trim().is_empty() {
            records.push((line_offset, byte_offset, line));
        }

        byte_offset += chunk.len() as u32;
        line_offset += 1;
    }

    if !content.ends_with('\n') && !content.is_empty() {
        return records;
    }

    records
}

pub(crate) fn parse_file(
    file_path: &str,
    content: &str,
) -> Result<(&'static str, Tree), anyhow::Error> {
    let language = detect_language_for_path(file_path)?;
    let tree = parse_for_language(language, file_path, content)?;
    Ok((language, tree))
}

pub(crate) fn parse_for_language(
    language: &str,
    file_path: &str,
    content: &str,
) -> Result<Tree, anyhow::Error> {
    let mut parser = Parser::new();
    let tree_sitter_language = crate::language::get_tree_sitter_language(language)?;
    parser
        .set_language(&tree_sitter_language)
        .map_err(|e| anyhow::anyhow!("Failed to set parser language for {}: {}", language, e))?;

    parser
        .parse(content, None)
        .ok_or_else(|| anyhow::anyhow!("Failed to parse file: {}", file_path))
}

pub(crate) fn detect_language_for_path(file_path: &str) -> Result<&'static str, anyhow::Error> {
    let extension = Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    crate::language::detect_language_from_extension(extension)
        .ok_or_else(|| anyhow::anyhow!("Unsupported file extension: {}", extension))
}
