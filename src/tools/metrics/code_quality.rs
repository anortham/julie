// Formatting for code quality metrics (doc coverage, dead code)

use crate::database::analytics::{DeadCodeCandidate, DocCoverageStats, UndocumentedSymbol};

pub fn format_doc_coverage(
    stats: &DocCoverageStats,
    undocumented: &[UndocumentedSymbol],
) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "Documentation Coverage: {}/{} public symbols documented ({:.1}%)\n",
        stats.documented, stats.total_public, stats.coverage_pct
    ));

    // Per-language breakdown
    if !stats.by_language.is_empty() {
        lines.push("By Language:".to_string());
        for lang in &stats.by_language {
            lines.push(format!(
                "  {:<14} {:>4}/{:<4} ({:>5.1}%)",
                lang.language, lang.documented, lang.total, lang.coverage_pct
            ));
        }
        lines.push(String::new());
    }

    // Top undocumented symbols by centrality
    if !undocumented.is_empty() {
        lines.push("Highest-Impact Undocumented Symbols:".to_string());
        for sym in undocumented {
            let sig = sym.signature.as_deref().unwrap_or(&sym.name);
            // Truncate long signatures
            let display_sig = if sig.len() > 60 {
                format!("{}...", &sig[..57])
            } else {
                sig.to_string()
            };
            lines.push(format!(
                "  [{:<9}] {} ({}:{})",
                sym.kind, display_sig, sym.file_path, sym.language
            ));
        }
    } else {
        lines.push("All public symbols are documented.".to_string());
    }

    lines.join("\n")
}

pub fn format_dead_code(
    candidates: &[DeadCodeCandidate],
    total_dead: i64,
    total_public: i64,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "Dead Code Candidates: {} public functions/methods with zero references",
        total_dead
    ));
    if total_public > 0 {
        let pct = (total_dead as f64 / total_public as f64) * 100.0;
        lines.push(format!(
            "({:.1}% of {} public symbols)\n",
            pct, total_public
        ));
    } else {
        lines.push(String::new());
    }

    if total_dead > candidates.len() as i64 {
        lines.push(format!(
            "Showing top {} of {}:\n",
            candidates.len(),
            total_dead
        ));
    }

    if candidates.is_empty() {
        lines.push("No dead code candidates found.".to_string());
        return lines.join("\n");
    }

    // Group by kind for readability
    let mut by_kind: std::collections::BTreeMap<&str, Vec<&DeadCodeCandidate>> =
        std::collections::BTreeMap::new();
    for c in candidates {
        by_kind.entry(c.kind.as_str()).or_default().push(c);
    }

    for (kind, syms) in &by_kind {
        lines.push(format!("{} ({}):", kind, syms.len()));
        for sym in syms {
            let sig = sym.signature.as_deref().unwrap_or(&sym.name);
            let display_sig = if sig.len() > 60 {
                format!("{}...", &sig[..57])
            } else {
                sig.to_string()
            };
            lines.push(format!("  {} ({})", display_sig, sym.file_path));
        }
        lines.push(String::new());
    }

    lines.join("\n")
}
