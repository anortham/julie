//! rewrite_symbol tool: symbol-aware editing using live parser spans.
//!
//! The agent references a symbol by name. Julie resolves the symbol in the
//! index, verifies the file is fresh, reparses the live file content, then
//! rewrites the live symbol span or a node-derived subspan.

use anyhow::{Result, anyhow};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::Path;
use tracing::debug;

use crate::extractors::{ExtractorManager, Symbol};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResultExt;
use crate::tools::navigation::resolution::{
    WorkspaceTarget, file_path_matches_suffix, resolve_workspace_filter,
};
use crate::utils::file_utils::secure_path_resolution;
use rmcp::model::{CallToolResult, Content};
use tree_sitter::{Node, Parser, Tree};

use super::EditingTransaction;
use super::validation::{
    check_bracket_balance, format_dry_run_diff, format_unified_diff, should_check_balance,
};

fn default_dry_run() -> bool {
    true
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}

fn parse_symbol_line_hint(symbol: &str) -> (&str, Option<u32>) {
    let Some((name, line_text)) = symbol.rsplit_once('@') else {
        return (symbol, None);
    };
    if name.is_empty() || line_text.is_empty() || !line_text.chars().all(|c| c.is_ascii_digit()) {
        return (symbol, None);
    }
    match line_text.parse::<u32>() {
        Ok(line) => (name, Some(line)),
        Err(_) => (symbol, None),
    }
}

fn symbol_matches_line(symbol: &Symbol, line: u32) -> bool {
    symbol.start_line <= line && line <= symbol.end_line
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RewriteSymbolTool {
    /// Symbol name to edit. Supports qualified names like `MyClass::method` and
    /// same-file disambiguators like `MyClass::method@42`.
    pub symbol: String,

    /// Operation to perform. All operations target the symbol's span as extracted from the
    /// language's tree-sitter grammar.
    ///
    /// - replace_full: Replace the entire symbol span (signature + body if any).
    /// - replace_body: Replace the grammar's `body` field. For brace-delimited languages
    ///   (Rust, C, Java, Go, JS/TS, C#, Swift, Kotlin, Scala, PHP, etc.) the replaced
    ///   span INCLUDES the enclosing braces, so your `content` must supply the full
    ///   `{ ... }` block. For indentation-delimited languages (Python) the replaced
    ///   span is the indented suite. For declarations without a body (trait methods,
    ///   interface methods, forward declarations) this operation returns an error.
    /// - replace_signature: Replace the text up to the start of the body field. Returns
    ///   an error if the symbol has no body field.
    /// - insert_after / insert_before: Insert content on the line after/before the symbol.
    /// - add_doc: Insert a documentation comment before the symbol. Errors if the symbol
    ///   already has documentation.
    pub operation: String,

    /// New code/text content for the operation
    pub content: String,

    /// Disambiguate when multiple symbols share a name (partial file path match)
    #[serde(default)]
    pub file_path: Option<String>,

    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,

    /// Preview diff without applying (default: true). Always preview first.
    #[serde(
        default = "default_dry_run",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub dry_run: bool,
}

struct WorkspaceEditTarget {
    db: std::sync::Arc<std::sync::Mutex<crate::database::SymbolDatabase>>,
    workspace_root: std::path::PathBuf,
}

struct LiveSymbolContext {
    live_symbol: Symbol,
    tree: Tree,
}

struct RewriteApplication {
    indexed_symbol: Symbol,
    resolved_path: String,
    original_content: String,
    modified_content: String,
    span_context: SpanContext,
    symbol_span_bytes: usize,
    live_start_line: u32,
    live_end_line: u32,
    match_count: usize,
}

#[derive(Debug)]
pub(crate) struct RewriteSymbolFailure {
    pub kind: &'static str,
    pub message: String,
}

impl std::fmt::Display for RewriteSymbolFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RewriteSymbolFailure {}

fn rewrite_symbol_error(kind: &'static str, message: impl Into<String>) -> anyhow::Error {
    anyhow!(RewriteSymbolFailure {
        kind,
        message: message.into(),
    })
}

pub(crate) fn failure_kind(error: &anyhow::Error) -> &'static str {
    error
        .downcast_ref::<RewriteSymbolFailure>()
        .map(|error| error.kind)
        .unwrap_or("execution_error")
}

#[derive(Debug, Clone, Copy)]
struct ByteRange {
    start: usize,
    end: usize,
}

fn detect_line_ending(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

fn validate_operation(operation: &str) -> bool {
    matches!(
        operation,
        "replace_full"
            | "replace_body"
            | "replace_signature"
            | "insert_after"
            | "insert_before"
            | "add_doc"
    )
}

fn check_file_freshness(
    db: &std::sync::MutexGuard<'_, crate::database::SymbolDatabase>,
    file_path: &str,
    current_hash: &str,
) -> Result<()> {
    match db.get_file_hash(file_path)? {
        Some(indexed_hash) if indexed_hash == current_hash => Ok(()),
        Some(_) => Err(anyhow!(
            "File '{}' has changed since last indexing. Run manage_workspace(operation=\"index\") or wait for the file watcher to catch up, then retry.",
            file_path
        )),
        None => Err(anyhow!(
            "File '{}' is not in the index. Run manage_workspace(operation=\"index\") first.",
            file_path
        )),
    }
}

fn replace_byte_range(source: &str, range: ByteRange, replacement: &str) -> Result<String> {
    if range.start > range.end || range.end > source.len() {
        return Err(anyhow!(
            "Byte range {}..{} is outside file bounds ({})",
            range.start,
            range.end,
            source.len()
        ));
    }

    let mut result = String::with_capacity(source.len() + replacement.len());
    result.push_str(&source[..range.start]);
    result.push_str(replacement);
    result.push_str(&source[range.end..]);
    Ok(result)
}

fn insert_before_line(source: &str, byte_index: usize, new_content: &str) -> Result<String> {
    if byte_index > source.len() {
        return Err(anyhow!(
            "Insert position {} is outside file bounds ({})",
            byte_index,
            source.len()
        ));
    }

    let eol = detect_line_ending(source);
    let insert_at = source[..byte_index]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let mut result = String::with_capacity(source.len() + new_content.len() + eol.len());
    result.push_str(&source[..insert_at]);
    result.push_str(new_content);
    if !new_content.ends_with('\n') && !new_content.ends_with("\r\n") {
        result.push_str(eol);
    }
    result.push_str(&source[insert_at..]);
    Ok(result)
}

fn insert_after_line(source: &str, byte_index: usize, new_content: &str) -> Result<String> {
    if byte_index > source.len() {
        return Err(anyhow!(
            "Insert position {} is outside file bounds ({})",
            byte_index,
            source.len()
        ));
    }

    let eol = detect_line_ending(source);
    let insert_at = match source[byte_index..].find('\n') {
        Some(offset) => byte_index + offset + 1,
        None => source.len(),
    };

    let mut result = String::with_capacity(source.len() + new_content.len() + eol.len() * 2);
    result.push_str(&source[..insert_at]);
    if insert_at == source.len() && !source.is_empty() && !source.ends_with('\n') {
        result.push_str(eol);
    }
    result.push_str(new_content);
    if !new_content.ends_with('\n') && !new_content.ends_with("\r\n") {
        result.push_str(eol);
    }
    result.push_str(&source[insert_at..]);
    Ok(result)
}

fn parse_live_tree(file_path: &str, content: &str) -> Result<Tree> {
    let language = crate::utils::language::detect_language(Path::new(file_path))
        .ok_or_else(|| anyhow!("Could not detect language for '{}'", file_path))?;
    let ts_language = crate::language::get_tree_sitter_language(&language)?;
    let mut parser = Parser::new();
    parser.set_language(&ts_language)?;
    parser
        .parse(content, None)
        .ok_or_else(|| anyhow!("Failed to parse {} file '{}'", language, file_path))
}

fn find_exact_span_node(node: Node<'_>, start: usize, end: usize) -> Option<Node<'_>> {
    if node.start_byte() == start && node.end_byte() == end {
        return Some(node);
    }
    if node.start_byte() > start || node.end_byte() < end {
        return None;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_exact_span_node(child, start, end) {
            return Some(found);
        }
    }
    None
}

fn trim_trailing_ascii_whitespace(source: &str, start: usize, end: usize) -> usize {
    let mut trimmed_end = end;
    while trimmed_end > start && source.as_bytes()[trimmed_end - 1].is_ascii_whitespace() {
        trimmed_end -= 1;
    }
    trimmed_end
}

fn live_symbol_context(
    indexed_symbol: &Symbol,
    file_path: &str,
    content: &str,
    workspace_root: &Path,
) -> Result<LiveSymbolContext> {
    let extractor = ExtractorManager::new();
    let live_symbols = extractor.extract_symbols(file_path, content, workspace_root)?;
    let live_symbol = if let Some(symbol) = live_symbols
        .iter()
        .find(|symbol| symbol.id == indexed_symbol.id)
        .cloned()
    {
        symbol
    } else {
        let mut candidates = live_symbols
            .into_iter()
            .filter(|symbol| {
                symbol.name == indexed_symbol.name
                    && symbol.kind == indexed_symbol.kind
                    && symbol.file_path == indexed_symbol.file_path
            })
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return Err(anyhow!(
                "Live parse could not recover symbol '{}' in '{}'",
                indexed_symbol.name,
                file_path
            ));
        }

        candidates.sort_by_key(|symbol| {
            (
                (symbol.start_line as i64 - indexed_symbol.start_line as i64).abs(),
                (symbol.start_column as i64 - indexed_symbol.start_column as i64).abs(),
            )
        });
        candidates.remove(0)
    };

    let tree = parse_live_tree(file_path, content)?;
    Ok(LiveSymbolContext { live_symbol, tree })
}

fn collect_node_field_names(node: Node<'_>) -> String {
    let mut cursor = node.walk();
    let mut field_names = std::collections::BTreeSet::new();
    if cursor.goto_first_child() {
        loop {
            if let Some(name) = cursor.field_name() {
                field_names.insert(name.to_string());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    if field_names.is_empty() {
        "no named fields".to_string()
    } else {
        field_names.into_iter().collect::<Vec<_>>().join(", ")
    }
}

fn detect_language_name(file_path: &str) -> String {
    crate::utils::language::detect_language(std::path::Path::new(file_path))
        .map(|l| format!("{l:?}"))
        .unwrap_or_else(|| "unknown".to_string())
}

enum SpanContext {
    Replace {
        byte_start: usize,
        byte_end: usize,
        start_line: usize,
        end_line: usize,
        old_content: String,
    },
    Anchor {
        byte: usize,
        line: usize,
    },
}

fn span_for_operation(
    operation: &str,
    original_content: &str,
    live_symbol: &Symbol,
    tree: &Tree,
) -> Result<Option<ByteRange>> {
    let full_range = ByteRange {
        start: live_symbol.start_byte as usize,
        end: live_symbol.end_byte as usize,
    };

    match operation {
        "replace_full" => Ok(Some(full_range)),
        "replace_body" => {
            let node = find_exact_span_node(
                tree.root_node(),
                live_symbol.start_byte as usize,
                live_symbol.end_byte as usize,
            )
            .ok_or_else(|| {
                anyhow!(
                    "Could not locate live syntax node for '{}'",
                    live_symbol.name
                )
            })?;
            match node.child_by_field_name("body") {
                Some(body) => Ok(Some(ByteRange {
                    start: body.start_byte(),
                    end: body.end_byte(),
                })),
                None => {
                    let fields_str = collect_node_field_names(node);
                    Err(anyhow!(
                        "Operation 'replace_body' is not supported for '{}' ({:?}); node has fields: [{}] but no 'body' field.",
                        live_symbol.name,
                        live_symbol.kind,
                        fields_str
                    ))
                }
            }
        }
        "replace_signature" => {
            let node = find_exact_span_node(
                tree.root_node(),
                live_symbol.start_byte as usize,
                live_symbol.end_byte as usize,
            )
            .ok_or_else(|| {
                anyhow!(
                    "Could not locate live syntax node for '{}'",
                    live_symbol.name
                )
            })?;
            if let Some(body) = node.child_by_field_name("body") {
                Ok(Some(ByteRange {
                    start: live_symbol.start_byte as usize,
                    end: trim_trailing_ascii_whitespace(
                        original_content,
                        live_symbol.start_byte as usize,
                        body.start_byte(),
                    ),
                }))
            } else {
                let language_name = detect_language_name(&live_symbol.file_path);
                Err(anyhow!(
                    "replace_signature is not supported for symbol '{}' (kind: {:?}); it has no body-delimited signature in the {} grammar.",
                    live_symbol.name,
                    live_symbol.kind,
                    language_name
                ))
            }
        }
        "insert_before" | "insert_after" | "add_doc" => Ok(None),
        _ => Err(anyhow!("Unsupported operation '{}'", operation)),
    }
}

fn format_span_header(ctx: &SpanContext, file_path: &str) -> String {
    match ctx {
        SpanContext::Replace {
            byte_start,
            byte_end,
            start_line,
            end_line,
            old_content,
        } => {
            let char_count = byte_end - byte_start;
            let mut header = format!(
                "Replacing {char_count} chars at bytes {byte_start}..{byte_end} (lines {start_line}-{end_line}) in {file_path}\n--- Old content ---\n"
            );
            let lines: Vec<&str> = old_content.lines().collect();
            const MAX_LINES: usize = 30;
            const HEAD_LINES: usize = 15;
            const TAIL_LINES: usize = 5;
            if lines.len() > MAX_LINES {
                for line in &lines[..HEAD_LINES] {
                    header.push_str(line);
                    header.push('\n');
                }
                let elided = lines.len() - HEAD_LINES - TAIL_LINES;
                header.push_str(&format!("... {elided} lines elided ...\n"));
                for line in &lines[lines.len() - TAIL_LINES..] {
                    header.push_str(line);
                    header.push('\n');
                }
            } else {
                header.push_str(old_content);
                if !old_content.ends_with('\n') {
                    header.push('\n');
                }
            }
            header.push_str("--- Diff ---\n");
            header
        }
        SpanContext::Anchor { byte, line } => {
            format!("Inserting at byte {byte} (line {line}) in {file_path}\n--- Diff ---\n")
        }
    }
}

impl RewriteSymbolTool {
    pub(crate) fn request_input_bytes(&self) -> u64 {
        serde_json::to_vec(self)
            .map(|bytes| bytes.len() as u64)
            .unwrap_or(0)
    }

    pub(crate) fn base_metrics_metadata(&self) -> Value {
        let input_bytes = self.request_input_bytes();
        json!({
            "kind": "rewrite_symbol",
            "dry_run": self.dry_run,
            "applied": false,
            "input_bytes": input_bytes,
            "operation": self.operation,
            "symbol": self.symbol,
            "content_bytes": self.content.len(),
            "match_count": serde_json::Value::Null,
        })
    }

    async fn resolve_workspace_target(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<WorkspaceEditTarget> {
        match resolve_workspace_filter(self.workspace.as_deref(), handler).await? {
            WorkspaceTarget::Primary => {
                let primary_snapshot = handler.primary_workspace_snapshot().await?;
                Ok(WorkspaceEditTarget {
                    db: primary_snapshot.database.clone(),
                    workspace_root: primary_snapshot.binding.workspace_root,
                })
            }
            WorkspaceTarget::Target(workspace_id) => Ok(WorkspaceEditTarget {
                db: handler.get_database_for_workspace(&workspace_id).await?,
                workspace_root: handler.get_workspace_root_for_target(&workspace_id).await?,
            }),
        }
    }

    async fn prepare_rewrite(&self, handler: &JulieServerHandler) -> Result<RewriteApplication> {
        let requested_symbol = self.symbol.clone();
        let (parsed_symbol_name, line_hint) = parse_symbol_line_hint(&requested_symbol);

        if parsed_symbol_name.is_empty() {
            return Err(rewrite_symbol_error(
                "validation",
                "symbol name is required",
            ));
        }
        if self.content.is_empty() {
            return Err(rewrite_symbol_error("validation", "content is required"));
        }
        if !validate_operation(&self.operation) {
            return Err(rewrite_symbol_error(
                "validation",
                format!(
                    "operation must be one of replace_full, replace_body, replace_signature, insert_after, insert_before, add_doc; got '{}'",
                    self.operation
                ),
            ));
        }

        let target = self.resolve_workspace_target(handler).await?;

        let symbol_name = parsed_symbol_name.to_string();
        let symbol_name_for_lookup = symbol_name.clone();
        let file_path_filter = self.file_path.clone();
        let file_path_for_error = self.file_path.clone();
        let db_arc = target.db.clone();
        let matches = tokio::task::spawn_blocking(move || -> Result<Vec<Symbol>> {
            let db = db_arc
                .lock()
                .map_err(|error| anyhow!("Database lock error: {}", error))?;
            let symbols =
                crate::tools::deep_dive::data::find_symbol(&db, &symbol_name_for_lookup, None)?;
            let filtered = if let Some(ref filter) = file_path_filter {
                symbols
                    .into_iter()
                    .filter(|symbol| file_path_matches_suffix(&symbol.file_path, filter))
                    .collect()
            } else {
                symbols
            };
            Ok(if let Some(line) = line_hint {
                filtered
                    .into_iter()
                    .filter(|symbol| symbol_matches_line(symbol, line))
                    .collect()
            } else {
                filtered
            })
        })
        .await??;

        if matches.is_empty() {
            if let Some(line) = line_hint {
                if let Some(ref file_path) = file_path_for_error {
                    return Err(rewrite_symbol_error(
                        "symbol_not_found",
                        format!(
                            "symbol '{}' not found at line {} in '{}'. Use fast_search or get_symbols to verify the location.",
                            symbol_name, line, file_path
                        ),
                    ));
                }
                return Err(rewrite_symbol_error(
                    "symbol_not_found",
                    format!(
                        "symbol '{}' not found at line {} in index. Use fast_search or get_symbols to verify the name.",
                        symbol_name, line
                    ),
                ));
            }
            if let Some(ref file_path) = file_path_for_error {
                return Err(rewrite_symbol_error(
                    "symbol_not_found",
                    format!(
                        "symbol '{}' not found in '{}'. Use fast_search or get_symbols to verify the location.",
                        symbol_name, file_path
                    ),
                ));
            }
            return Err(rewrite_symbol_error(
                "symbol_not_found",
                format!(
                    "symbol '{}' not found in index. Use fast_search or get_symbols to verify the name.",
                    symbol_name
                ),
            ));
        }

        if matches.len() > 1 {
            let same_file = matches
                .first()
                .map(|first| {
                    matches
                        .iter()
                        .all(|symbol| symbol.file_path == first.file_path)
                })
                .unwrap_or(false);
            let locations = matches
                .iter()
                .map(|symbol| {
                    format!(
                        "  {} at {}:{}-{} (try {}@{})",
                        symbol.name,
                        symbol.file_path,
                        symbol.start_line,
                        symbol.end_line,
                        symbol_name,
                        symbol.start_line
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let hint = if file_path_for_error.is_some() || same_file {
                "Provide symbol@line to disambiguate"
            } else {
                "Provide file_path or symbol@line to disambiguate"
            };
            return Err(rewrite_symbol_error(
                "ambiguous_symbol",
                format!(
                    "'{}' matches {} symbols. {}:\n{}",
                    symbol_name,
                    matches.len(),
                    hint,
                    locations
                ),
            ));
        }

        let indexed_symbol = matches.into_iter().next().expect("one symbol");
        let resolved_path =
            secure_path_resolution(&indexed_symbol.file_path, &target.workspace_root)?;
        let resolved_str = resolved_path.to_string_lossy().to_string();

        let original_content = std::fs::read_to_string(&resolved_path).map_err(|error| {
            anyhow!("Cannot read file '{}': {}", indexed_symbol.file_path, error)
        })?;
        let current_hash = blake3::hash(original_content.as_bytes())
            .to_hex()
            .to_string();
        {
            let db = target
                .db
                .lock()
                .map_err(|error| anyhow!("Database lock error: {}", error))?;
            if let Err(error) = check_file_freshness(&db, &indexed_symbol.file_path, &current_hash)
            {
                return Err(rewrite_symbol_error("stale_index", error.to_string()));
            }
        }
        let live = live_symbol_context(
            &indexed_symbol,
            &indexed_symbol.file_path,
            &original_content,
            &target.workspace_root,
        )?;

        let (modified_content, span_context) = match self.operation.as_str() {
            "replace_full" | "replace_body" | "replace_signature" => {
                let range = match span_for_operation(
                    &self.operation,
                    &original_content,
                    &live.live_symbol,
                    &live.tree,
                ) {
                    Ok(Some(r)) => r,
                    Ok(None) => {
                        return Err(rewrite_symbol_error(
                            "unsupported_operation",
                            format!(
                                "Operation '{}' did not resolve a byte range",
                                self.operation
                            ),
                        ));
                    }
                    Err(e) => {
                        return Err(rewrite_symbol_error("unsupported_operation", e.to_string()));
                    }
                };
                let old_content = original_content[range.start..range.end].to_string();
                let start_line = original_content[..range.start].lines().count() + 1;
                let end_line = start_line + old_content.lines().count().saturating_sub(1);
                let modified = replace_byte_range(&original_content, range, &self.content)?;
                (
                    modified,
                    SpanContext::Replace {
                        byte_start: range.start,
                        byte_end: range.end,
                        start_line,
                        end_line,
                        old_content,
                    },
                )
            }
            "insert_before" | "add_doc" => {
                if self.operation == "add_doc" && live.live_symbol.doc_comment.is_some() {
                    return Err(rewrite_symbol_error(
                        "already_documented",
                        format!("symbol '{}' already has documentation", self.symbol),
                    ));
                }
                let byte = live.live_symbol.start_byte as usize;
                let line = original_content[..byte].lines().count() + 1;
                let modified = insert_before_line(&original_content, byte, &self.content)?;
                (modified, SpanContext::Anchor { byte, line })
            }
            "insert_after" => {
                let byte = live.live_symbol.end_byte as usize;
                let line = original_content[..byte].lines().count();
                let modified = insert_after_line(&original_content, byte, &self.content)?;
                (modified, SpanContext::Anchor { byte, line })
            }
            _ => unreachable!(),
        };

        let symbol_span_bytes = (live.live_symbol.end_byte as usize)
            .saturating_sub(live.live_symbol.start_byte as usize);
        Ok(RewriteApplication {
            indexed_symbol,
            resolved_path: resolved_str,
            original_content,
            modified_content,
            span_context,
            symbol_span_bytes,
            live_start_line: live.live_symbol.start_line,
            live_end_line: live.live_symbol.end_line,
            match_count: 1,
        })
    }

    pub(crate) async fn success_metrics_metadata(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<Value> {
        let application = self.prepare_rewrite(handler).await?;
        let diff = format_unified_diff(
            &application.original_content,
            &application.modified_content,
            &application.indexed_symbol.file_path,
        );
        let changed_bytes = super::edit_file::changed_region_bytes(
            &application.original_content,
            &application.modified_content,
        );
        let mut metadata = self.base_metrics_metadata();
        if let Some(object) = metadata.as_object_mut() {
            object.insert(
                "file_path".to_string(),
                json!(application.indexed_symbol.file_path),
            );
            object.insert(
                "file_size_bytes".to_string(),
                json!(application.original_content.len()),
            );
            object.insert(
                "symbol_span_bytes".to_string(),
                json!(application.symbol_span_bytes),
            );
            object.insert("diff_bytes".to_string(), json!(diff.len()));
            object.insert("changed_bytes".to_string(), json!(changed_bytes));
            object.insert("match_count".to_string(), json!(application.match_count));
            object.insert(
                "applied".to_string(),
                json!(
                    !self.dry_run && application.modified_content != application.original_content
                ),
            );
        }
        Ok(metadata)
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        let application = self.prepare_rewrite(handler).await?;

        if application.modified_content == application.original_content {
            let message = format!(
                "No changes: {} with supplied content would not modify the file. Symbol '{}' at {}:{}-{} is already in the requested state.",
                self.operation,
                self.symbol,
                application.indexed_symbol.file_path,
                application.live_start_line,
                application.live_end_line
            );
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        let balance_warning = if should_check_balance(&application.indexed_symbol.file_path) {
            check_bracket_balance(&application.original_content, &application.modified_content)
        } else {
            None
        };

        let diff = format_unified_diff(
            &application.original_content,
            &application.modified_content,
            &application.indexed_symbol.file_path,
        );

        if self.dry_run {
            debug!(
                "rewrite_symbol dry_run for {} in {}",
                self.symbol, application.indexed_symbol.file_path
            );
            let span_header = format_span_header(
                &application.span_context,
                &application.indexed_symbol.file_path,
            );
            let preview_diff = format_dry_run_diff(&diff);
            let mut message = format!(
                "Dry run preview (set dry_run=false to apply):\n\n{}{}",
                span_header, preview_diff
            );
            if let Some(ref warning) = balance_warning {
                message.push_str(&format!("\n\n{}", warning));
            }
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        let transaction = EditingTransaction::begin(&application.resolved_path)?;
        transaction
            .commit_if_unchanged(&application.modified_content, &application.original_content)?;

        debug!(
            "rewrite_symbol {} applied to {}",
            self.operation, application.indexed_symbol.file_path
        );
        let mut message = format!(
            "Applied {} on '{}' in {}:\n\n{}",
            self.operation, self.symbol, application.indexed_symbol.file_path, diff
        );
        if let Some(warning) = balance_warning {
            message.push_str(&format!("\n\n{}", warning));
        }
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
