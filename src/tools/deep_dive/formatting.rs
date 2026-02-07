//! Kind-aware formatting for deep_dive tool
//!
//! Produces lean text output tailored to each symbol kind:
//! - Function/Method: callers, callees, types
//! - Trait/Interface: required methods, implementations
//! - Struct/Class: fields, methods, used by
//! - Module/Namespace: public exports

use crate::extractors::base::{RelationshipKind, SymbolKind};

use super::data::{RefEntry, SymbolContext};

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

    out.push_str(&format!("{}:{} ({}{})\n", s.file_path, s.start_line, kind, vis));

    if let Some(sig) = &s.signature {
        out.push_str(&format!("  {}\n", sig));
    }
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

/// Show test file locations at full depth
fn format_test_locations(out: &mut String, ctx: &SymbolContext, depth: &str) {
    if depth != "full" || ctx.test_refs.is_empty() {
        return;
    }
    out.push_str(&format!("\nTest locations ({}):\n", ctx.test_refs.len()));
    for r in &ctx.test_refs {
        if let Some(sym) = &r.symbol {
            out.push_str(&format!("  {}:{}  {}\n", r.file_path, r.line_number, sym.name));
        } else {
            out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
        }
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
        .filter(|r| matches!(r.kind, RelationshipKind::Parameter | RelationshipKind::Returns))
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
                out.push_str(&format!("  {}  {}:{}  {}\n", sym.name, r.file_path, r.line_number, kind));
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
        out.push_str(&format!("\nImplementations ({}):\n", ctx.implementations.len()));
        for imp in &ctx.implementations {
            out.push_str(&format!("  {}:{}  {}\n", imp.file_path, imp.start_line, imp.name));
        }
    }

    format_test_locations(out, ctx, depth);
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
        .filter(|r| matches!(r.kind, RelationshipKind::Implements | RelationshipKind::Extends))
        .collect();

    if !implements.is_empty() {
        out.push_str(&format!("\nImplements ({}):\n", implements.len()));
        for r in &implements {
            if let Some(sym) = &r.symbol {
                out.push_str(&format!("  {}  {}:{}\n", sym.name, sym.file_path, sym.start_line));
            } else {
                out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
            }
        }
    }

    // Used by
    if !ctx.incoming.is_empty() {
        format_ref_section(out, "Used by", &ctx.incoming.iter().collect::<Vec<_>>(), ctx.incoming_total, depth);
    }

    format_test_locations(out, ctx, depth);
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
        format_ref_section(out, "Used by", &ctx.incoming.iter().collect::<Vec<_>>(), ctx.incoming_total, depth);
    }

    format_test_locations(out, ctx, depth);
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

    let label = if public.is_empty() { "Exports" } else { "Public exports" };
    let exports = if public.is_empty() { &ctx.children } else { &public.iter().map(|s| (*s).clone()).collect::<Vec<_>>() };

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
        let mut by_file: std::collections::BTreeMap<&str, Vec<&str>> = std::collections::BTreeMap::new();
        for r in &imports {
            let file = r.symbol.as_ref().map(|s| s.file_path.as_str()).unwrap_or(r.file_path.as_str());
            let name = r.symbol.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
            by_file.entry(file).or_default().push(name);
        }

        out.push_str(&format!("\nDependencies ({}):\n", by_file.len()));
        for (file, names) in &by_file {
            out.push_str(&format!("  {}  {}\n", file, names.join(", ")));
        }
    }

    format_test_locations(out, ctx, depth);
    format_body(out, ctx, depth);
}

fn format_generic(out: &mut String, ctx: &SymbolContext, depth: &str) {
    if !ctx.incoming.is_empty() {
        format_ref_section(out, "Referenced by", &ctx.incoming.iter().collect::<Vec<_>>(), ctx.incoming_total, depth);
    }
    if !ctx.outgoing.is_empty() {
        format_ref_section(out, "References", &ctx.outgoing.iter().collect::<Vec<_>>(), ctx.outgoing_total, depth);
    }
    format_test_locations(out, ctx, depth);
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
        match depth {
            "full" => {
                // Full: show signature + body if available
                if let Some(sym) = &r.symbol {
                    let name = sym.signature.as_deref().unwrap_or(&sym.name);
                    out.push_str(&format!("  {}:{}  {}\n", r.file_path, r.line_number, name));
                    if let Some(code) = &sym.code_context {
                        for line in code.lines().take(10) {
                            out.push_str(&format!("    {}\n", line));
                        }
                    }
                } else {
                    out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
                }
            }
            "context" => {
                // Context: show signature
                if let Some(sym) = &r.symbol {
                    let name = sym.signature.as_deref().unwrap_or(&sym.name);
                    out.push_str(&format!("  {}:{}  {}\n", r.file_path, r.line_number, name));
                } else {
                    out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
                }
            }
            _ => {
                // Overview: just location + name
                if let Some(sym) = &r.symbol {
                    out.push_str(&format!("  {}:{}  {}\n", r.file_path, r.line_number, sym.name));
                } else {
                    out.push_str(&format!("  {}:{}\n", r.file_path, r.line_number));
                }
            }
        }
    }
}
