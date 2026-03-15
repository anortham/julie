//! Kind-aware formatting for deep_dive tool
//!
//! Produces lean text output tailored to each symbol kind:
//! - Function/Method: callers, callees, types
//! - Trait/Interface: required methods, implementations
//! - Struct/Class: fields, methods, used by
//! - Module/Namespace: public exports

use crate::extractors::base::{RelationshipKind, SymbolKind};

use super::data::{RefEntry, SimilarEntry, SymbolContext};

/// Format a SymbolContext for the given depth level.
pub fn format_symbol_context(ctx: &SymbolContext, depth: &str) -> String {
    let mut out = String::new();

    // === Header: location + kind + visibility + signature ===
    format_header(&mut out, ctx);

    // === Kind-specific body ===
    match ctx.symbol.kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constructor => {
            format_callable(&mut out, ctx, depth);
        }
        SymbolKind::Trait | SymbolKind::Interface => {
            format_trait_or_interface(&mut out, ctx, depth);
        }
        SymbolKind::Class => {
            format_class_or_struct(&mut out, ctx, depth);
        }
        SymbolKind::Enum => {
            format_enum(&mut out, ctx, depth);
        }
        SymbolKind::Module | SymbolKind::Namespace => {
            format_module(&mut out, ctx, depth);
        }
        _ => {
            // Generic: show refs
            format_generic(&mut out, ctx, depth);
        }
    }

    // === Semantic similarity (context and full depth) ===
    format_similar_section(&mut out, &ctx.similar);

    out.trim_end().to_string()
}

fn format_header(out: &mut String, ctx: &SymbolContext) {
    let s = &ctx.symbol;
    let kind = s.kind.to_string();
    let vis = s
        .visibility
        .as_ref()
        .map(|v| format!(", {}", v.to_string().to_lowercase()))
        .unwrap_or_default();

    out.push_str(&format!(
        "{}:{} ({}{})\n",
        s.file_path, s.start_line, kind, vis
    ));

    if let Some(sig) = &s.signature {
        out.push_str(&format!("  {}\n", sig));
    }

    // Show test quality info when the symbol itself is a test
    format_test_quality_info(out, s);
    format_change_risk_info(out, s, ctx.incoming_total);
}

fn format_body(out: &mut String, ctx: &SymbolContext, depth: &str) {
    if depth == "overview" {
        return;
    }
    if let Some(code) = &ctx.symbol.code_context {
        let limit = body_line_limit(depth);
        let total_lines = code.lines().count();
        out.push_str("\nBody:\n");
        for line in code.lines().take(limit) {
            out.push_str(&format!("  {}\n", line));
        }
        if total_lines > limit {
            out.push_str(&format!("  ... ({} more lines)\n", total_lines - limit));
        }
    }
}

fn body_line_limit(depth: &str) -> usize {
    match depth {
        "context" => 30,
        "full" => 100,
        _ => 0,
    }
}

/// Show test file locations at context and full depth, with quality tiers when available
fn format_test_locations(out: &mut String, ctx: &SymbolContext, depth: &str) {
    if (depth != "full" && depth != "context") || ctx.test_refs.is_empty() {
        return;
    }
    out.push_str(&format!("\nTest locations ({}):\n", ctx.test_refs.len()));
    for r in &ctx.test_refs {
        if let Some(sym) = &r.symbol {
            let quality_tag = extract_quality_tier(&sym.metadata);
            out.push_str(&format!(
                "  {}:{}  {}{}\n",
                r.file_path, r.line_number, sym.name, quality_tag
            ));
        } else {
            out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
        }
    }
}

/// Extract quality tier tag from symbol metadata, e.g. "  [thorough]"
fn extract_quality_tier(
    metadata: &Option<std::collections::HashMap<String, serde_json::Value>>,
) -> String {
    metadata
        .as_ref()
        .and_then(|m| m.get("test_quality"))
        .and_then(|tq| tq.get("quality_tier"))
        .and_then(|v| v.as_str())
        .map(|tier| format!("  [{}]", tier))
        .unwrap_or_default()
}

/// Format test quality info line when the primary symbol IS a test
fn format_test_quality_info(out: &mut String, symbol: &crate::extractors::base::Symbol) {
    let metadata = match &symbol.metadata {
        Some(m) => m,
        None => return,
    };

    // Only show for test symbols
    let is_test = metadata
        .get("is_test")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !is_test {
        return;
    }

    let tq = match metadata.get("test_quality") {
        Some(v) => v,
        None => return,
    };

    let tier = tq
        .get("quality_tier")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Build detail parts from available metrics
    let mut details = Vec::new();
    if let Some(count) = tq.get("assertion_count").and_then(|v| v.as_u64()) {
        details.push(format!("{} assertions", count));
    }
    if let Some(count) = tq.get("mock_count").and_then(|v| v.as_u64()) {
        details.push(format!("{} mocks", count));
    }
    if let Some(density) = tq.get("assertion_density").and_then(|v| v.as_f64()) {
        details.push(format!("{:.2} density", density));
    }

    if details.is_empty() {
        out.push_str(&format!("  Test quality: {}\n", tier));
    } else {
        out.push_str(&format!(
            "  Test quality: {} ({})\n",
            tier,
            details.join(", ")
        ));
    }
}

/// Format change risk section for production symbols.
/// Skipped for test symbols (they have quality tiers instead).
fn format_change_risk_info(out: &mut String, symbol: &crate::extractors::base::Symbol, incoming_count: usize) {
    let metadata = match &symbol.metadata {
        Some(m) => m,
        None => return,
    };

    // Skip test symbols
    if metadata.get("is_test").and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }

    let risk = match metadata.get("change_risk") {
        Some(r) => r,
        None => return,
    };

    let score = risk.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let label = risk.get("label").and_then(|v| v.as_str()).unwrap_or("LOW");
    let factors = risk.get("factors");

    let vis = factors
        .and_then(|f| f.get("visibility"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let kind = factors
        .and_then(|f| f.get("kind"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Build summary line: "Change Risk: HIGH (0.82) — 14 callers, public, thin tests"
    let coverage = metadata.get("test_coverage");
    let test_summary = match coverage {
        Some(tc) => {
            let count = tc.get("test_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let best = tc.get("best_tier").and_then(|v| v.as_str()).unwrap_or("none");
            if count > 0 {
                format!("{} tests", best)
            } else {
                "untested".to_string()
            }
        }
        None => "untested".to_string(),
    };

    out.push_str(&format!(
        "\nChange Risk: {} ({:.2}) — {} callers, {}, {}\n",
        label, score, incoming_count, vis, test_summary
    ));

    // Detail lines
    if let Some(f) = factors {
        let centrality = f.get("centrality").and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push_str(&format!("  centrality: {:.2} ({} direct callers)\n", centrality, incoming_count));
        out.push_str(&format!("  visibility: {}\n", vis));

        if let Some(tc) = coverage {
            let count = tc.get("test_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let best = tc.get("best_tier").and_then(|v| v.as_str()).unwrap_or("none");
            let worst = tc.get("worst_tier").and_then(|v| v.as_str()).unwrap_or("none");
            out.push_str(&format!("  test coverage: {} tests (best: {}, worst: {})\n", count, best, worst));
        } else {
            out.push_str("  test coverage: untested\n");
        }

        out.push_str(&format!("  kind: {}\n", kind));
    }
}

/// Format security risk section for production symbols.
/// Only shown when metadata contains security_risk key.
fn format_security_risk_info(out: &mut String, symbol: &crate::extractors::base::Symbol, incoming_count: usize) {
    let metadata = match &symbol.metadata {
        Some(m) => m,
        None => return,
    };

    // Skip test symbols
    if metadata.get("is_test").and_then(|v| v.as_bool()).unwrap_or(false) {
        return;
    }

    let security = match metadata.get("security_risk") {
        Some(r) => r,
        None => return,
    };

    let score = security.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let label = security.get("label").and_then(|v| v.as_str()).unwrap_or("LOW");
    let signals = security.get("signals");

    // Build summary: "Security Risk: HIGH (0.85) — calls execute, raw_sql; public; accepts string params"
    let mut summary_parts = Vec::new();

    if let Some(sigs) = signals {
        if let Some(sinks) = sigs.get("sink_calls").and_then(|v| v.as_array()) {
            if !sinks.is_empty() {
                let names: Vec<&str> = sinks.iter().filter_map(|v| v.as_str()).collect();
                summary_parts.push(format!("calls {}", names.join(", ")));
            }
        }
        if let Some(exp) = sigs.get("exposure").and_then(|v| v.as_f64()) {
            if exp >= 0.5 {
                summary_parts.push("public".to_string());
            }
        }
        if sigs.get("input_handling").and_then(|v| v.as_f64()).unwrap_or(0.0) > 0.0 {
            summary_parts.push("accepts string params".to_string());
        }
    }

    let summary = if summary_parts.is_empty() {
        String::new()
    } else {
        format!(" — {}", summary_parts.join("; "))
    };

    out.push_str(&format!(
        "\nSecurity Risk: {} ({:.2}){}\n",
        label, score, summary
    ));

    // Detail lines
    if let Some(sigs) = signals {
        let exposure = sigs.get("exposure").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if exposure >= 0.5 {
            out.push_str("  exposure: public\n");
        } else {
            out.push_str(&format!("  exposure: {:.2}\n", exposure));
        }

        let input = sigs.get("input_handling").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if input > 0.0 {
            out.push_str("  input handling: yes (signature contains input type patterns)\n");
        }

        if let Some(sinks) = sigs.get("sink_calls").and_then(|v| v.as_array()) {
            if !sinks.is_empty() {
                let names: Vec<&str> = sinks.iter().filter_map(|v| v.as_str()).collect();
                out.push_str(&format!("  sink calls: {}\n", names.join(", ")));
            }
        }

        let blast = sigs.get("blast_radius").and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push_str(&format!("  blast radius: {:.2} ({} callers)\n", blast, incoming_count));

        let untested = sigs.get("untested").and_then(|v| v.as_bool()).unwrap_or(false);
        out.push_str(&format!("  untested: {}\n", if untested { "yes" } else { "no" }));
    }
}

// === Kind-specific formatters ===

fn format_callable(out: &mut String, ctx: &SymbolContext, depth: &str) {
    // Callers (incoming Calls relationships)
    let callers: Vec<&RefEntry> = ctx
        .incoming
        .iter()
        .filter(|r| matches!(r.kind, RelationshipKind::Calls))
        .collect();
    let other_incoming: Vec<&RefEntry> = ctx
        .incoming
        .iter()
        .filter(|r| !matches!(r.kind, RelationshipKind::Calls))
        .collect();

    if !callers.is_empty() || ctx.incoming_total > 0 {
        format_ref_section(out, "Callers", &callers, ctx.incoming_total, depth);
    }

    // Callees (outgoing Calls relationships)
    let callees: Vec<&RefEntry> = ctx
        .outgoing
        .iter()
        .filter(|r| matches!(r.kind, RelationshipKind::Calls))
        .collect();

    if !callees.is_empty() {
        format_ref_section(out, "Callees", &callees, ctx.outgoing_total, depth);
    }

    // Types (outgoing Parameter/Returns relationships, deduped by name)
    let type_refs: Vec<&RefEntry> = ctx
        .outgoing
        .iter()
        .filter(|r| {
            matches!(
                r.kind,
                RelationshipKind::Parameter | RelationshipKind::Returns
            )
        })
        .collect();

    if !type_refs.is_empty() {
        let mut seen = std::collections::HashSet::new();
        let unique_types: Vec<&RefEntry> = type_refs
            .into_iter()
            .filter(|r| {
                let name = r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or("");
                seen.insert(name.to_string())
            })
            .collect();

        out.push_str(&format!("\nTypes ({}):\n", unique_types.len()));
        for r in &unique_types {
            if let Some(sym) = &r.symbol {
                let kind = sym.kind.to_string();
                out.push_str(&format!(
                    "  {}  {}:{}  {}\n",
                    sym.name, r.file_path, r.line_number, kind
                ));
            } else {
                out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
            }
        }
    }

    // Other references (type_usage, member_access, etc.)
    if !other_incoming.is_empty() {
        format_ref_section(out, "Referenced by", &other_incoming, 0, depth);
    }

    format_test_locations(out, ctx, depth);
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_body(out, ctx, depth);
}

fn format_trait_or_interface(out: &mut String, ctx: &SymbolContext, depth: &str) {
    // Required methods (children)
    let methods: Vec<&_> = ctx
        .children
        .iter()
        .filter(|c| matches!(c.kind, SymbolKind::Method | SymbolKind::Function))
        .collect();

    if !methods.is_empty() {
        out.push_str(&format!("\nRequired methods ({}):\n", methods.len()));
        for m in &methods {
            if let Some(sig) = &m.signature {
                out.push_str(&format!("  {}  :{}\n", sig, m.start_line));
            } else {
                out.push_str(&format!("  {}  :{}\n", m.name, m.start_line));
            }
        }
    }

    // Implementations
    if !ctx.implementations.is_empty() {
        out.push_str(&format!(
            "\nImplementations ({}):\n",
            ctx.implementations.len()
        ));
        for imp in &ctx.implementations {
            out.push_str(&format!(
                "  {}:{}  {}\n",
                imp.file_path, imp.start_line, imp.name
            ));
        }
    }

    format_test_locations(out, ctx, depth);
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_body(out, ctx, depth);
}

fn format_class_or_struct(out: &mut String, ctx: &SymbolContext, depth: &str) {
    // Fields (properties, variables)
    let fields: Vec<&_> = ctx
        .children
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                SymbolKind::Property | SymbolKind::Variable | SymbolKind::Constant
            )
        })
        .collect();

    if !fields.is_empty() {
        out.push_str(&format!("\nFields ({}):\n", fields.len()));
        for f in &fields {
            if let Some(sig) = &f.signature {
                out.push_str(&format!("  {}\n", sig));
            } else {
                out.push_str(&format!("  {}\n", f.name));
            }
        }
    }

    // Methods
    let methods: Vec<&_> = ctx
        .children
        .iter()
        .filter(|c| {
            matches!(
                c.kind,
                SymbolKind::Method | SymbolKind::Function | SymbolKind::Constructor
            )
        })
        .collect();

    if !methods.is_empty() {
        out.push_str(&format!("\nMethods ({}):\n", methods.len()));
        for m in &methods {
            if let Some(sig) = &m.signature {
                out.push_str(&format!("  {}  :{}\n", sig, m.start_line));
            } else {
                out.push_str(&format!("  {}()  :{}\n", m.name, m.start_line));
            }
        }
    }

    // Implements (outgoing Implements/Extends relationships)
    let implements: Vec<&RefEntry> = ctx
        .outgoing
        .iter()
        .filter(|r| {
            matches!(
                r.kind,
                RelationshipKind::Implements | RelationshipKind::Extends
            )
        })
        .collect();

    if !implements.is_empty() {
        out.push_str(&format!("\nImplements ({}):\n", implements.len()));
        for r in &implements {
            if let Some(sym) = &r.symbol {
                out.push_str(&format!(
                    "  {}  {}:{}\n",
                    sym.name, sym.file_path, sym.start_line
                ));
            } else {
                out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
            }
        }
    }

    // Used by
    if !ctx.incoming.is_empty() {
        format_ref_section(
            out,
            "Used by",
            &ctx.incoming.iter().collect::<Vec<_>>(),
            ctx.incoming_total,
            depth,
        );
    }

    format_test_locations(out, ctx, depth);
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_body(out, ctx, depth);
}

fn format_enum(out: &mut String, ctx: &SymbolContext, depth: &str) {
    // Enum members
    let members: Vec<&_> = ctx
        .children
        .iter()
        .filter(|c| matches!(c.kind, SymbolKind::EnumMember | SymbolKind::Constant))
        .collect();

    if !members.is_empty() {
        out.push_str(&format!("\nMembers ({}):\n", members.len()));
        for m in &members {
            out.push_str(&format!("  {}\n", m.name));
        }
    }

    // Methods on enum (if any)
    let methods: Vec<&_> = ctx
        .children
        .iter()
        .filter(|c| matches!(c.kind, SymbolKind::Method | SymbolKind::Function))
        .collect();

    if !methods.is_empty() {
        out.push_str(&format!("\nMethods ({}):\n", methods.len()));
        for m in &methods {
            if let Some(sig) = &m.signature {
                out.push_str(&format!("  {}  :{}\n", sig, m.start_line));
            } else {
                out.push_str(&format!("  {}()  :{}\n", m.name, m.start_line));
            }
        }
    }

    if !ctx.incoming.is_empty() {
        format_ref_section(
            out,
            "Used by",
            &ctx.incoming.iter().collect::<Vec<_>>(),
            ctx.incoming_total,
            depth,
        );
    }

    format_test_locations(out, ctx, depth);
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_body(out, ctx, depth);
}

fn format_module(out: &mut String, ctx: &SymbolContext, depth: &str) {
    // Public exports
    let public: Vec<&_> = ctx
        .children
        .iter()
        .filter(|c| {
            c.visibility
                .as_ref()
                .map(|v| matches!(v, crate::extractors::base::Visibility::Public))
                .unwrap_or(false)
        })
        .collect();

    let label = if public.is_empty() {
        "Exports"
    } else {
        "Public exports"
    };
    let exports = if public.is_empty() {
        &ctx.children
    } else {
        &public.iter().map(|s| (*s).clone()).collect::<Vec<_>>()
    };

    if !exports.is_empty() {
        out.push_str(&format!("\n{} ({}):\n", label, exports.len()));
        for s in exports {
            let kind = s.kind.to_string();
            if let Some(sig) = &s.signature {
                out.push_str(&format!("  {} {}  :{}\n", kind, sig, s.start_line));
            } else {
                out.push_str(&format!("  {} {}  :{}\n", kind, s.name, s.start_line));
            }
        }
    }

    // Dependencies (outgoing Imports relationships, grouped by file)
    let imports: Vec<&RefEntry> = ctx
        .outgoing
        .iter()
        .filter(|r| matches!(r.kind, RelationshipKind::Imports))
        .collect();

    if !imports.is_empty() {
        // Group by target file
        let mut by_file: std::collections::BTreeMap<&str, Vec<&str>> =
            std::collections::BTreeMap::new();
        for r in &imports {
            let file = r
                .symbol
                .as_ref()
                .map(|s| s.file_path.as_str())
                .unwrap_or(r.file_path.as_str());
            let name = r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
            by_file.entry(file).or_default().push(name);
        }

        out.push_str(&format!("\nDependencies ({}):\n", by_file.len()));
        for (file, names) in &by_file {
            out.push_str(&format!("  {}  {}\n", file, names.join(", ")));
        }
    }

    format_test_locations(out, ctx, depth);
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_body(out, ctx, depth);
}

fn format_generic(out: &mut String, ctx: &SymbolContext, depth: &str) {
    if !ctx.incoming.is_empty() {
        format_ref_section(
            out,
            "Referenced by",
            &ctx.incoming.iter().collect::<Vec<_>>(),
            ctx.incoming_total,
            depth,
        );
    }
    if !ctx.outgoing.is_empty() {
        format_ref_section(
            out,
            "References",
            &ctx.outgoing.iter().collect::<Vec<_>>(),
            ctx.outgoing_total,
            depth,
        );
    }
    format_test_locations(out, ctx, depth);
    format_change_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_security_risk_info(out, &ctx.symbol, ctx.incoming_total);
    format_body(out, ctx, depth);
}

/// Format a section of references with depth-aware detail
fn format_ref_section(
    out: &mut String,
    label: &str,
    refs: &[&RefEntry],
    total: usize,
    depth: &str,
) {
    if refs.is_empty() {
        return;
    }

    let header = if total > refs.len() {
        format!("\n{} ({} of {}):\n", label, refs.len(), total)
    } else {
        format!("\n{} ({}):\n", label, refs.len())
    };
    out.push_str(&header);

    for r in refs {
        let kind = format!("{:?}", r.kind);
        match depth {
            "full" => {
                // Full: show signature + body if available
                if let Some(sym) = &r.symbol {
                    let name = sym.signature.as_deref().unwrap_or(&sym.name);
                    out.push_str(&format!(
                        "  {}:{}  {} ({})\n",
                        r.file_path, r.line_number, name, kind
                    ));
                    if let Some(code) = &sym.code_context {
                        for line in code.lines().take(10) {
                            out.push_str(&format!("    {}\n", line));
                        }
                    }
                } else {
                    out.push_str(&format!(
                        "  {}:{} ({})\n",
                        r.file_path, r.line_number, kind
                    ));
                }
            }
            "context" => {
                // Context: show signature + kind
                if let Some(sym) = &r.symbol {
                    let name = sym.signature.as_deref().unwrap_or(&sym.name);
                    out.push_str(&format!(
                        "  {}:{}  {} ({})\n",
                        r.file_path, r.line_number, name, kind
                    ));
                } else {
                    out.push_str(&format!(
                        "  {}:{} ({})\n",
                        r.file_path, r.line_number, kind
                    ));
                }
            }
            _ => {
                // Overview: location + name + kind
                if let Some(sym) = &r.symbol {
                    out.push_str(&format!(
                        "  {}:{}  {} ({})\n",
                        r.file_path, r.line_number, sym.name, kind
                    ));
                } else {
                    out.push_str(&format!(
                        "  {}:{} ({})\n",
                        r.file_path, r.line_number, kind
                    ));
                }
            }
        }
    }
}

fn format_similar_section(out: &mut String, similar: &[SimilarEntry]) {
    if similar.is_empty() {
        return;
    }

    out.push_str(&format!("\nSemantically Similar ({}):\n", similar.len()));

    for entry in similar {
        let kind = entry.symbol.kind.to_string();
        let vis = entry
            .symbol
            .visibility
            .as_ref()
            .map(|v| v.to_string().to_lowercase())
            .unwrap_or_default();
        let kind_vis = if vis.is_empty() {
            kind
        } else {
            format!("{}, {}", kind, vis)
        };

        out.push_str(&format!(
            "  {:<25} {:.2}  {}:{} ({})\n",
            entry.symbol.name,
            entry.score,
            entry.symbol.file_path,
            entry.symbol.start_line,
            kind_vis,
        ));
    }
}
