//! HTTP client call -> route handler edges derived from structural facts, by
//! the same matching rules the SQL-era `web_edges` projection used.

use std::collections::HashMap;

use julie_facts::rows::StructuralFactRow;
use serde_json::Value;

use super::{Edge, EdgeKind, SymbolId, SymbolTable};

const HTTP_MATCH_CONFIDENCE_THRESHOLD: f32 = 0.5;

pub const HTTP_CLIENT_CALL_PATTERN_IDS: &[&str] = &["http.client_request.v1"];

pub const ROUTE_HANDLER_PATTERN_IDS: &[&str] = &[
    "symfony.route.v1",
    "laravel.route.v1",
    "axum.route.v1",
    "actix.attribute_route.v1",
    "actix.scope_route.v1",
    "fastapi.route.v1",
    "flask.route.v1",
    "spring.request_mapping.v1",
    "go.net_http.route.v1",
    "gin.route.v1",
    "echo.route.v1",
    "rails.route.v1",
    "aspnet.minimal_api.route.v1",
    "aspnet.attribute_route.v1",
    "express.route.v1",
    "fastify.route.v1",
    "nestjs.route.v1",
    "ktor.route.v1",
    "phoenix.route.v1",
    "nextjs.route_handler.v1",
    "nuxt.server_route.v1",
    "razor.page_directive.v1",
];

/// Every pattern id `web_route_edges` reads; use it to filter the facts query.
pub fn http_pattern_ids() -> Vec<String> {
    HTTP_CLIENT_CALL_PATTERN_IDS
        .iter()
        .chain(ROUTE_HANDLER_PATTERN_IDS)
        .map(|id| id.to_string())
        .collect()
}

fn meta_str(fact: &StructuralFactRow, key: &str) -> Option<String> {
    fact.metadata
        .as_ref()?
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn route_template(fact: &StructuralFactRow) -> Option<String> {
    meta_str(fact, "normalized_route_template").or_else(|| meta_str(fact, "route_template"))
}

fn containing_symbol(symbols: &SymbolTable, fact: &StructuralFactRow) -> Option<SymbolId> {
    symbols.id_at(symbols.file_index(&fact.path)?, fact.containing_ordinal?)
}

fn split_path(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

fn is_parametric_segment(seg: &str) -> bool {
    let s = seg.trim();
    !s.is_empty()
        && (s.starts_with(':')
            || (s.starts_with('{') && s.ends_with('}'))
            || (s.starts_with('<') && s.ends_with('>')))
}

/// `:id`, `{id}` and `<id>` segments match any literal segment.
pub fn route_matches(literal_path: &str, template: &str) -> bool {
    let lit = split_path(literal_path);
    let tmpl = split_path(template);
    lit.len() == tmpl.len()
        && lit
            .iter()
            .zip(&tmpl)
            .all(|(lp, tp)| is_parametric_segment(tp) || lp == tp)
}

/// A handler with no verb is a catch-all; a client call with no verb never matches.
fn verbs_match(client_verb: Option<&str>, handler_verb: Option<&str>) -> bool {
    match (client_verb, handler_verb) {
        (Some(c), Some(h)) => c.eq_ignore_ascii_case(h),
        (Some(_), None) => true,
        (None, _) => false,
    }
}

struct Handler {
    symbol: SymbolId,
    verb: Option<String>,
    template: String,
    confidence: f32,
}

fn best_handler(
    client: &StructuralFactRow,
    verb: Option<&str>,
    target_path: &str,
    handlers: &[Handler],
) -> Option<(SymbolId, f32)> {
    let mut best: Option<(SymbolId, f32)> = None;
    let mut ambiguous = false;
    for handler in handlers {
        if !verbs_match(verb, handler.verb.as_deref())
            || !route_matches(target_path, &handler.template)
        {
            continue;
        }
        let combined = client.confidence.min(handler.confidence);
        match best {
            Some((_, score)) if combined < score => {}
            Some((symbol, score)) if combined == score => ambiguous |= symbol != handler.symbol,
            _ => {
                best = Some((handler.symbol, combined));
                ambiguous = false;
            }
        }
    }
    if ambiguous { None } else { best }
}

/// One `WebRoute` edge per client call whose verb and path match exactly one
/// best handler at or above the confidence threshold.
pub fn web_route_edges(symbols: &SymbolTable, facts: &[StructuralFactRow]) -> Vec<Edge> {
    let mut handlers_by_segments: HashMap<usize, Vec<Handler>> = HashMap::new();
    for fact in facts {
        if !ROUTE_HANDLER_PATTERN_IDS.contains(&fact.pattern_id.as_str()) {
            continue;
        }
        let (Some(template), Some(symbol)) =
            (route_template(fact), containing_symbol(symbols, fact))
        else {
            continue;
        };
        handlers_by_segments
            .entry(split_path(&template).len())
            .or_default()
            .push(Handler {
                symbol,
                verb: meta_str(fact, "verb"),
                template,
                confidence: fact.confidence,
            });
    }

    let mut edges = Vec::new();
    for client in facts {
        if !HTTP_CLIENT_CALL_PATTERN_IDS.contains(&client.pattern_id.as_str()) {
            continue;
        }
        let (Some(from), Some(target_path)) = (
            containing_symbol(symbols, client),
            meta_str(client, "target_path"),
        ) else {
            continue;
        };
        let verb = meta_str(client, "verb");
        let bucket = handlers_by_segments
            .get(&split_path(&target_path).len())
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        match best_handler(client, verb.as_deref(), &target_path, bucket) {
            Some((to, confidence))
                if confidence >= HTTP_MATCH_CONFIDENCE_THRESHOLD && to != from =>
            {
                edges.push(Edge {
                    from,
                    to,
                    kind: EdgeKind::WebRoute,
                })
            }
            _ => {}
        }
    }
    edges
}
