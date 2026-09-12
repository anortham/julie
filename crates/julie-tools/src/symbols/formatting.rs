//! Response formatting for symbol queries
//!
//! Handles formatting symbol data into structured responses for MCP clients.

use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use tracing::debug;

use julie_core::Symbol;

/// Format raw code output - just the source code, no metadata wrapper
///
/// This format is optimal for AI agents that can read code directly.
/// Returns code bodies separated by blank lines with a minimal file header.
const FORMAT_CODE_CHAR_LIMIT: usize = 50_000;

fn format_code_output(
    file_path: &str,
    symbols: &[Symbol],
    next: Option<String>,
    enforce_display_cap: bool,
) -> CallToolResult {
    let mut output = String::new();

    // Minimal file header
    output.push_str(&format!("// === {} ===\n\n", file_path));

    // Extract code from each symbol, stopping at the character cap
    let mut truncated = false;
    for (i, symbol) in symbols.iter().enumerate() {
        if let Some(code) = &symbol.code_context {
            if enforce_display_cap && output.len() + code.len() > FORMAT_CODE_CHAR_LIMIT {
                output.push_str(&format!(
                    "\n// ... truncated ({} of {} symbols shown — output exceeded {} chars)",
                    i,
                    symbols.len(),
                    FORMAT_CODE_CHAR_LIMIT
                ));
                truncated = true;
                break;
            }
            output.push_str(code);
            // Add separator between symbols (but not after the last one)
            if i < symbols.len() - 1 {
                output.push_str("\n\n");
            }
        }
    }

    // Trim trailing whitespace but ensure single newline at end
    let mut output = if truncated {
        output + "\n"
    } else {
        output.trim_end().to_string() + "\n"
    };
    if let Some(next) = next {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&next);
    }

    CallToolResult::text_content(vec![Content::text(output)])
}

/// Map a SymbolKind to its source-code keyword (with trailing space).
///
/// Used to detect when a signature already contains the kind keyword so
/// we can skip the redundant kind prefix in lean output.
fn kind_keyword(kind: &julie_extractors::SymbolKind) -> Option<&'static str> {
    use julie_extractors::SymbolKind;
    match kind {
        SymbolKind::Function | SymbolKind::Method => Some("fn "),
        SymbolKind::Struct => Some("struct "),
        SymbolKind::Class => Some("class "),
        SymbolKind::Interface => Some("interface "),
        SymbolKind::Trait => Some("trait "),
        SymbolKind::Enum => Some("enum "),
        SymbolKind::Module => Some("mod "),
        SymbolKind::Namespace => Some("namespace "),
        _ => None,
    }
}

/// Format lean text overview — scannable symbol list for agents
///
/// Output format:
/// ```text
/// src/foo.rs — 12 symbols
///
///   struct Foo (10-25, public)
///     fn new() -> Self (12-15, public)
///     fn process(&self, data: &[u8]) (17-24, public)
///   fn helper(x: i32) -> bool (30-45, private)
/// ```
fn format_lean_symbols(
    file_path: &str,
    symbols: &[Symbol],
    next: Option<String>,
) -> CallToolResult {
    let mut output = String::new();

    output.push_str(&format!("{} — {} symbols\n", file_path, symbols.len()));

    for symbol in symbols {
        let indent = if symbol.parent_id.is_some() {
            "    "
        } else {
            "  "
        };
        let kind = symbol.kind.to_string();

        // Use signature if available, otherwise just name
        let name_display = if let Some(sig) = &symbol.signature {
            // Signature often includes the kind keyword, use as-is
            sig.clone()
        } else {
            symbol.name.clone()
        };

        let vis = symbol
            .visibility
            .as_ref()
            .map(|v| format!("{:?}", v).to_lowercase())
            .unwrap_or_default();

        let vis_str = if vis.is_empty() || vis == "public" {
            String::new()
        } else {
            format!(", {}", vis)
        };

        // Skip the kind prefix when the signature already contains the kind
        // keyword, e.g. "pub struct Foo" already signals its kind.
        let signature_has_kind = kind_keyword(&symbol.kind)
            .map(|kw| name_display.contains(kw))
            .unwrap_or(false);

        if signature_has_kind {
            output.push_str(&format!(
                "{}{} ({}-{}{})\n",
                indent, name_display, symbol.start_line, symbol.end_line, vis_str,
            ));
        } else {
            output.push_str(&format!(
                "{}{} {} ({}-{}{})\n",
                indent, kind, name_display, symbol.start_line, symbol.end_line, vis_str,
            ));
        }
    }

    let mut output = output.trim_end().to_string();
    if let Some(next) = next {
        output.push('\n');
        output.push_str(&next);
    }
    CallToolResult::text_content(vec![Content::text(output)])
}

/// Format symbol query response with structured content
pub fn format_symbol_response(
    file_path: &str,
    symbols: Vec<Symbol>,
    target: Option<&str>,
    next: Option<String>,
    body_pages: Vec<super::body_extraction::BodyPage>,
    explicit_body_window: bool,
) -> anyhow::Result<CallToolResult> {
    let body_status = body_pages
        .iter()
        .map(|page| {
            format!(
                "body: {} lines {}..{} of {} status={} complete={} end_reached={} page_text=structuredContent.body_pages",
                page.symbol,
                page.returned_range[0],
                page.returned_range[1],
                page.total_lines,
                page.status,
                page.complete,
                page.end_reached
            )
        })
        .collect::<Vec<_>>();
    let trailer = body_status.into_iter().chain(next).collect::<Vec<_>>();
    let trailer = (!trailer.is_empty()).then(|| trailer.join("\n"));
    // Auto-select format: "code" when code bodies are available, "lean" otherwise
    let has_code_bodies = symbols.iter().any(|s| s.code_context.is_some());
    let effective_format = if has_code_bodies { "code" } else { "lean" };

    // Handle "code" format - returns raw code without metadata
    if effective_format == "code" {
        debug!(
            "📋 Returning {} symbols as raw code (target: {:?})",
            symbols.len(),
            target
        );
        let mut result = format_code_output(file_path, &symbols, trailer, !explicit_body_window);
        result.structured_content = Some(serde_json::json!({
            "file_path": file_path,
            "symbols": symbols,
            "body_pages": body_pages,
        }));
        return Ok(result);
    }

    // Everything else (including "lean", unknown formats) → lean text overview
    debug!(
        "📋 Returning {} symbols as lean overview (target: {:?})",
        symbols.len(),
        target
    );
    let mut result = format_lean_symbols(file_path, &symbols, trailer);
    result.structured_content = Some(serde_json::json!({
        "file_path": file_path,
        "symbols": symbols,
        "body_pages": body_pages,
    }));
    Ok(result)
}
