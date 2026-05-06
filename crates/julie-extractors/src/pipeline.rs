use crate::ExtractionResults;
use std::path::Path;
use tree_sitter::{Node, Parser, Tree};

use crate::base::RecordOffset;
use crate::base::{NormalizedSpan, ParseDiagnostic, ParseDiagnosticKind};

pub fn extract_canonical(
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    if file_path.ends_with(".jsonl") {
        return extract_jsonl_canonical(file_path, content, workspace_root);
    }

    extract_canonical_with_parse(file_path, content, workspace_root, parse_for_language)
}

pub(crate) fn extract_canonical_with_parse<F>(
    file_path: &str,
    content: &str,
    workspace_root: &Path,
    parse: F,
) -> Result<ExtractionResults, anyhow::Error>
where
    F: FnOnce(&str, &str, &str) -> Result<Option<Tree>, anyhow::Error>,
{
    let language = detect_language_for_source(file_path, content)?;
    let Some(tree) = parse(language, file_path, content)? else {
        return Ok(degraded_parse_failure_result(content));
    };

    let mut results =
        crate::registry::extract_for_language(language, &tree, file_path, content, workspace_root)?;
    results.parse_diagnostics = parse_diagnostics_for_tree(&tree);
    Ok(results)
}

fn extract_jsonl_canonical(
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<ExtractionResults, anyhow::Error> {
    extract_jsonl_canonical_with_parser_factory(file_path, content, workspace_root, || {
        configured_parser_for_language("json")
    })
}

pub(crate) fn extract_jsonl_canonical_with_parser_factory<F>(
    file_path: &str,
    content: &str,
    workspace_root: &Path,
    parser_factory: F,
) -> Result<ExtractionResults, anyhow::Error>
where
    F: FnOnce() -> Result<Parser, anyhow::Error>,
{
    let mut results = ExtractionResults::empty();
    let mut parser = parser_factory()?;

    for (line_delta, byte_delta, line) in jsonl_records(content) {
        let Some(tree) = parse_with_parser(&mut parser, file_path, line)? else {
            let mut record_results = degraded_parse_failure_result(line);
            record_results.apply_record_offset(RecordOffset {
                line_delta,
                byte_delta,
            });
            results.extend(record_results);
            continue;
        };
        let mut record_results =
            crate::registry::extract_for_language("json", &tree, file_path, line, workspace_root)?;
        record_results.parse_diagnostics = parse_diagnostics_for_tree(&tree);
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

pub(crate) fn parse_for_language(
    language: &str,
    file_path: &str,
    content: &str,
) -> Result<Option<Tree>, anyhow::Error> {
    let mut parser = configured_parser_for_language(language)?;
    parse_with_parser(&mut parser, file_path, content)
}

pub(crate) fn configured_parser_for_language(language: &str) -> Result<Parser, anyhow::Error> {
    let mut parser = Parser::new();
    let tree_sitter_language = crate::language::get_tree_sitter_language(language)?;
    parser
        .set_language(&tree_sitter_language)
        .map_err(|e| anyhow::anyhow!("Failed to set parser language for {}: {}", language, e))?;

    Ok(parser)
}

fn parse_with_parser(
    parser: &mut Parser,
    _file_path: &str,
    content: &str,
) -> Result<Option<Tree>, anyhow::Error> {
    Ok(parser.parse(content, None))
}

fn degraded_parse_failure_result(content: &str) -> ExtractionResults {
    let mut results = ExtractionResults::empty();
    results
        .parse_diagnostics
        .push(total_parse_failure_diagnostic(content));
    results
}

fn total_parse_failure_diagnostic(content: &str) -> ParseDiagnostic {
    let (end_line, end_column) = content_end_position(content);
    ParseDiagnostic {
        kind: ParseDiagnosticKind::Error,
        start_line: 1,
        start_column: 0,
        end_line,
        end_column,
        start_byte: 0,
        end_byte: content.len() as u32,
    }
}

fn content_end_position(content: &str) -> (u32, u32) {
    let mut line = 1;
    let mut column = 0;

    for byte in content.bytes() {
        if byte == b'\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }

    (line, column)
}

pub fn parse_diagnostics_for_tree(tree: &Tree) -> Vec<ParseDiagnostic> {
    let mut diagnostics = Vec::new();
    collect_parse_diagnostics(tree.root_node(), &mut diagnostics);
    diagnostics
}

fn collect_parse_diagnostics(node: Node<'_>, diagnostics: &mut Vec<ParseDiagnostic>) {
    if node.is_error() {
        diagnostics.push(parse_diagnostic_for_node(node, ParseDiagnosticKind::Error));
    }
    if node.is_missing() {
        diagnostics.push(parse_diagnostic_for_node(
            node,
            ParseDiagnosticKind::Missing,
        ));
    }

    if !node.has_error() {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_parse_diagnostics(child, diagnostics);
    }
}

fn parse_diagnostic_for_node(node: Node<'_>, kind: ParseDiagnosticKind) -> ParseDiagnostic {
    let span = NormalizedSpan::from_node(&node);
    ParseDiagnostic {
        kind,
        start_line: span.start_line,
        start_column: span.start_column,
        end_line: span.end_line,
        end_column: span.end_column,
        start_byte: span.start_byte,
        end_byte: span.end_byte,
    }
}

pub(crate) fn detect_language_for_path(file_path: &str) -> Result<&'static str, anyhow::Error> {
    let extension = Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    crate::language::detect_language_from_extension(extension)
        .ok_or_else(|| anyhow::anyhow!("Unsupported file extension: {}", extension))
}

pub(crate) fn detect_language_for_source(
    file_path: &str,
    content: &str,
) -> Result<&'static str, anyhow::Error> {
    let extension = Path::new(file_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    if extension.eq_ignore_ascii_case("h") && header_contains_cpp_syntax(content) {
        return Ok("cpp");
    }

    detect_language_for_path(file_path)
}

fn header_contains_cpp_syntax(content: &str) -> bool {
    let code = c_family_code_without_comments_and_strings(content);
    code.contains("::")
        || code.contains("public:")
        || code.contains("private:")
        || code.contains("protected:")
        || [
            "class",
            "concept",
            "consteval",
            "constexpr",
            "constinit",
            "final",
            "friend",
            "namespace",
            "noexcept",
            "override",
            "requires",
            "template",
            "typename",
        ]
        .into_iter()
        .any(|keyword| contains_identifier_token(&code, keyword))
}

fn c_family_code_without_comments_and_strings(content: &str) -> String {
    let mut code = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for comment_ch in chars.by_ref() {
                    if comment_ch == '\n' {
                        code.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut previous = '\0';
                for comment_ch in chars.by_ref() {
                    if comment_ch == '\n' {
                        code.push('\n');
                    } else {
                        code.push(' ');
                    }
                    if previous == '*' && comment_ch == '/' {
                        break;
                    }
                    previous = comment_ch;
                }
            }
            '"' | '\'' => {
                scrub_quoted_literal(ch, &mut chars, &mut code);
            }
            _ => code.push(ch),
        }
    }

    code
}

fn scrub_quoted_literal(
    quote: char,
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    code: &mut String,
) {
    code.push(' ');
    let mut escaped = false;

    for literal_ch in chars.by_ref() {
        if literal_ch == '\n' {
            code.push('\n');
        } else {
            code.push(' ');
        }

        if escaped {
            escaped = false;
            continue;
        }
        if literal_ch == '\\' {
            escaped = true;
            continue;
        }
        if literal_ch == quote {
            break;
        }
    }
}

fn contains_identifier_token(code: &str, token: &str) -> bool {
    code.match_indices(token).any(|(start, _)| {
        let end = start + token.len();
        let before_is_identifier = start
            .checked_sub(1)
            .and_then(|index| code.as_bytes().get(index).copied())
            .is_some_and(is_identifier_byte);
        let after_is_identifier = code
            .as_bytes()
            .get(end)
            .copied()
            .is_some_and(is_identifier_byte);
        !before_is_identifier && !after_is_identifier
    })
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
