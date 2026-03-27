//! Output formatting for get_context results.
//!
//! Renders pivots (with code bodies or signatures), neighbors (with signatures or names),
//! a file map, and centrality hints into a structured text response.

use super::allocation::{Allocation, NeighborMode};

/// All data needed to format a get_context response.
///
/// This struct decouples the formatter from pipeline internals — the pipeline
/// pre-processes search results into these flat entries, and the formatter
/// just renders them.
pub struct ContextData {
    /// The original search query.
    pub query: String,
    /// Pivot symbols (primary results), pre-processed for rendering.
    pub pivots: Vec<PivotEntry>,
    /// Neighbor symbols (graph-expanded), pre-processed for rendering.
    pub neighbors: Vec<NeighborEntry>,
    /// Token allocation (determines rendering modes).
    pub allocation: Allocation,
}

/// Output rendering style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Readable,
    Compact,
}

impl OutputFormat {
    pub fn from_option(value: Option<&str>) -> Self {
        match value {
            Some(v) if v.eq_ignore_ascii_case("readable") => Self::Readable,
            _ => Self::Compact,
        }
    }
}

/// Pre-processed pivot for formatting.
///
/// Contains everything needed to render a pivot section — no database
/// lookups or symbol resolution needed at format time.
pub struct PivotEntry {
    /// Symbol name.
    pub name: String,
    /// Relative file path.
    pub file_path: String,
    /// Line number (1-based).
    pub start_line: u32,
    /// Symbol kind as display string (e.g. "function", "struct").
    pub kind: String,
    /// Graph centrality reference score.
    pub reference_score: f64,
    /// Code body or signature (already selected by pipeline based on PivotMode).
    pub content: String,
    /// Names of symbols that call/reference this pivot (incoming).
    pub incoming_names: Vec<String>,
    /// Names of symbols this pivot calls/references (outgoing).
    pub outgoing_names: Vec<String>,
    /// Test quality tier (thorough/adequate/minimal/untested) from metadata, if available.
    pub test_quality_label: Option<String>,
}

/// Pre-processed neighbor for formatting.
pub struct NeighborEntry {
    /// Symbol name.
    pub name: String,
    /// Relative file path.
    pub file_path: String,
    /// Line number (1-based).
    pub start_line: u32,
    /// Symbol kind as display string.
    pub kind: String,
    /// Signature (present for SignatureAndDoc/SignatureOnly modes).
    pub signature: Option<String>,
    /// First line of doc comment (present for SignatureAndDoc mode).
    pub doc_summary: Option<String>,
}

/// Centrality hint label based on reference_score.
fn centrality_label(reference_score: f64) -> &'static str {
    if reference_score >= 20.0 {
        "high"
    } else if reference_score >= 5.0 {
        "medium"
    } else {
        "low"
    }
}

/// Format a complete get_context response from pre-processed data.
///
/// Produces structured text with sections:
/// 1. Header with query and summary counts
/// 2. Pivot sections with location, centrality, content, callers/callees
/// 3. Neighbors section (format varies by NeighborMode)
/// 4. Files section showing which symbols appear in each file
pub fn format_context(data: &ContextData) -> String {
    format_context_with_mode(data, OutputFormat::Readable)
}

pub fn format_context_with_mode(data: &ContextData, output_format: OutputFormat) -> String {
    match output_format {
        OutputFormat::Readable => format_context_readable(data),
        OutputFormat::Compact => format_context_compact(data),
    }
}

fn format_context_readable(data: &ContextData) -> String {
    if data.pivots.is_empty() {
        return format!(
            "=== Context: \"{}\" ===\nNo relevant symbols found.\n\
            Try fast_search(query=\"{}\") for exact matches, or verify the workspace is indexed",
            data.query, data.query
        );
    }

    let mut out = String::with_capacity(2048);

    // --- Header ---
    let file_count = count_unique_files(data);
    out.push_str(&format!(
        "=== Context: \"{}\" ===\n",
        data.query
    ));
    out.push_str(&format!(
        "Found {} pivot{}, {} neighbor{} across {} file{}\n",
        data.pivots.len(),
        if data.pivots.len() == 1 { "" } else { "s" },
        data.neighbors.len(),
        if data.neighbors.len() == 1 { "" } else { "s" },
        file_count,
        if file_count == 1 { "" } else { "s" },
    ));

    // --- Pivot sections ---
    for pivot in &data.pivots {
        out.push('\n');
        out.push_str(&format!(
            "-- Pivot: {} ---\n",
            pivot.name
        ));
        let quality_tag = pivot
            .test_quality_label
            .as_ref()
            .map(|l| format!("  [{} quality]", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "{}:{} ({}){}\n",
            pivot.file_path, pivot.start_line, pivot.kind, quality_tag
        ));
        let label = centrality_label(pivot.reference_score);
        out.push_str(&format!("  Centrality: {}\n", label));

        // Code content
        out.push('\n');
        for line in pivot.content.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }

        // Callers (incoming)
        let incoming_names = dedup_names(&pivot.incoming_names);
        if !incoming_names.is_empty() {
            out.push('\n');
            out.push_str(&format!(
                "  Callers ({}): {}\n",
                incoming_names.len(),
                incoming_names.join(", ")
            ));
        }

        // Calls (outgoing)
        let outgoing_names = dedup_names(&pivot.outgoing_names);
        if !outgoing_names.is_empty() {
            out.push_str(&format!("  Calls: {}\n", outgoing_names.join(", ")));
        }
    }

    // --- Neighbors section ---
    if !data.neighbors.is_empty() {
        out.push('\n');
        out.push_str("-- Neighbors ---\n");
        for neighbor in &data.neighbors {
            format_neighbor(&mut out, neighbor, &data.allocation.neighbor_mode);
        }
    }

    out
}

fn format_context_compact(data: &ContextData) -> String {
    if data.pivots.is_empty() {
        return format!(
            "Context \"{}\" | no relevant symbols\n\
            Try fast_search(query=\"{}\") for exact matches, or verify the workspace is indexed",
            data.query, data.query
        );
    }

    let mut out = String::with_capacity(1536);
    let file_count = count_unique_files(data);
    out.push_str(&format!(
        "Context \"{}\" | pivots={} neighbors={} files={}\n",
        data.query,
        data.pivots.len(),
        data.neighbors.len(),
        file_count
    ));

    for pivot in &data.pivots {
        let label = centrality_label(pivot.reference_score);
        let quality_tag = pivot
            .test_quality_label
            .as_ref()
            .map(|l| format!(" quality={}", l))
            .unwrap_or_default();
        out.push_str(&format!(
            "PIVOT {} {}:{} kind={} centrality={}{}\n",
            pivot.name,
            pivot.file_path,
            pivot.start_line,
            pivot.kind,
            label,
            quality_tag
        ));
        for line in pivot.content.lines() {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
        let incoming_names = dedup_names(&pivot.incoming_names);
        if !incoming_names.is_empty() {
            out.push_str(&format!("  callers={}\n", incoming_names.join(",")));
        }
        let outgoing_names = dedup_names(&pivot.outgoing_names);
        if !outgoing_names.is_empty() {
            out.push_str(&format!("  calls={}\n", outgoing_names.join(",")));
        }
    }

    for neighbor in &data.neighbors {
        format_neighbor_compact(&mut out, neighbor, &data.allocation.neighbor_mode);
    }

    out
}

fn dedup_names(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = names.to_vec();
    out.sort();
    out.dedup();
    out
}

fn format_neighbor_compact(out: &mut String, neighbor: &NeighborEntry, mode: &NeighborMode) {
    match mode {
        NeighborMode::SignatureAndDoc => {
            let sig = neighbor.signature.as_deref().unwrap_or(&neighbor.name);
            out.push_str(&format!(
                "NEIGHBOR {} {}:{} kind={} sig={}",
                neighbor.name, neighbor.file_path, neighbor.start_line, neighbor.kind, sig
            ));
            if let Some(doc) = &neighbor.doc_summary {
                out.push_str(&format!(" doc=\"{}\"", doc));
            }
            out.push('\n');
        }
        NeighborMode::SignatureOnly => {
            let sig = neighbor.signature.as_deref().unwrap_or(&neighbor.name);
            out.push_str(&format!(
                "NEIGHBOR {} {}:{} kind={} sig={}\n",
                neighbor.name, neighbor.file_path, neighbor.start_line, neighbor.kind, sig
            ));
        }
        NeighborMode::NameAndLocation => {
            out.push_str(&format!(
                "NEIGHBOR {} {}:{} kind={}\n",
                neighbor.name, neighbor.file_path, neighbor.start_line, neighbor.kind
            ));
        }
    }
}

/// Format a single neighbor entry based on the active NeighborMode.
fn format_neighbor(out: &mut String, neighbor: &NeighborEntry, mode: &NeighborMode) {
    match mode {
        NeighborMode::SignatureAndDoc => {
            let sig = neighbor.signature.as_deref().unwrap_or(&neighbor.name);
            out.push_str(&format!(
                "  {} {}:{} {}\n",
                neighbor.name, neighbor.file_path, neighbor.start_line, sig
            ));
            if let Some(doc) = &neighbor.doc_summary {
                out.push_str(&format!("  {}\n", doc));
            }
        }
        NeighborMode::SignatureOnly => {
            let sig = neighbor.signature.as_deref().unwrap_or(&neighbor.name);
            out.push_str(&format!(
                "  {} {}:{} {}\n",
                neighbor.name, neighbor.file_path, neighbor.start_line, sig
            ));
        }
        NeighborMode::NameAndLocation => {
            out.push_str(&format!(
                "  {} {}:{}\n",
                neighbor.name, neighbor.file_path, neighbor.start_line
            ));
        }
    }
}

/// Count unique files across all pivots and neighbors.
fn count_unique_files(data: &ContextData) -> usize {
    let mut files = std::collections::HashSet::new();
    for p in &data.pivots {
        files.insert(p.file_path.as_str());
    }
    for n in &data.neighbors {
        files.insert(n.file_path.as_str());
    }
    files.len()
}
