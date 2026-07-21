//! Derive web navigation edges from structural facts.
//!
//! Phase 1: `http_call` edges join `http.client_request.v1` (frontend HTTP
//! client call) facts with route-handler facts (e.g. `symfony.route.v1`,
//! `laravel.route.v1`) on normalized path + method. Unmatched client calls
//! degrade to an external-endpoint edge so `trace` can still surface the call
//! target even when no in-workspace handler exists.

use std::collections::HashMap;

use anyhow::Result;
use julie_core::database::{SymbolDatabase, WebEdge, WebEdgeKind};
use julie_extractors::base::StructuralFact;

/// Minimum combined confidence to emit a matched (in-workspace) edge.
/// Below this, a client call degrades to an external-endpoint edge.
const HTTP_MATCH_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// Pattern ids that represent an HTTP client call (edge origin).
pub const HTTP_CLIENT_CALL_PATTERN_IDS: &[&str] = &["http.client_request.v1"];

/// Pattern ids that represent a backend route handler (edge target).
///
/// Every pattern in this list shares the same join shape as `symfony.route.v1`:
/// metadata carries `verb` (sometimes optional/absent for catch-all routes) and
/// `normalized_route_template`. The derivation normalizes the client call's
/// literal path against the handler's template segment-by-segment.
///
/// **Language-agnostic coverage ledger (implemented):** Symfony, Laravel, Axum,
/// Actix (attribute + scope), FastAPI, Flask, Spring (Java/Kotlin), Go net/http,
/// Gin, Echo, Rails, ASP.NET (minimal API + attribute), Express, Fastify,
/// NestJS, Ktor, Phoenix, Next.js route handlers, Nuxt server routes, Razor
/// page directives.
///
/// **Tracked checklist — different-shape route families (NOT in this list,
/// need alternate join logic — see plan open question #3):**
///   - `django.url_pattern.v1` — `normalized_route_template` (OPT) + `view_target`, no verb.
///   - `laravel.resource_route.v1` / `rails.resource_route.v1` / `phoenix.resource_route.v1` — `resource_name` / `normalized_resource_path`, no per-route verb (REST bundles — expand to 5/7 routes).
///   - `*.nest.v1` / `*.mount.v1` / `*.forward.v1` / `*.route_prefix.v1` / `*.include_router.v1` / `*.router_mount.v1` / `aspnet.minimal_api.route_group.v1` / `django.url_include.v1` — mount/prefix facts (`normalized_mount_path`); compose with child routes, not direct handler matches.
///   - `vue.route_definition.v1` / `react.route_definition.v1` — SPA client routes (`target_path` / `effective_route_template`), not server handlers.
///   - `nextjs.file_route.v1` / `nuxt.file_route.v1` — file-page routes (`route_path`), implicit GET.
///   - `*.route_reference.v1` / `htmx.attribute.v1` — client-side navigation references (`target_path`), origins not targets.
///
/// These are verified-not-applicable for the simple verb+template join; each
/// needs its own derivation path and is tracked here rather than silently
/// excluded.
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

/// Pattern ids for SQL facts that represent a query/mutation referencing one
/// or more tables (edge origin). `sql.select_query.v1` is intentionally absent:
/// the extractor does not capture source table names for SELECT — it records
/// only `projection_count`, `source_count` (a count, not identifiers), and
/// boolean clause flags (see `sql_structural_facts.rs::select_query_fact` in
/// julie-extractors, c8324f8). There is no `table_name`/`source_tables` key to
/// join on, so SELECT→table edges require extractor support first. T-SQL has
/// no separate pattern family — `sql.merge_statement.v1` covers T-SQL MERGE.
pub const SQL_QUERY_PATTERN_IDS: &[&str] = &[
    "sql.view_definition.v1",
    "sql.update_statement.v1",
    "sql.insert_statement.v1",
    "sql.delete_statement.v1",
    "sql.merge_statement.v1",
];

/// Pattern id for the SQL table-definition fact (edge target side).
pub const SQL_TABLE_DEFINITION_PATTERN_ID: &str = "sql.table_definition.v1";

/// Pattern ids that define a SQL table (used to resolve table names to
/// in-workspace table symbols).
pub const SQL_TABLE_PATTERN_IDS: &[&str] = &[SQL_TABLE_DEFINITION_PATTERN_ID];

/// Split `facts` into (client-call facts, route-handler facts) by pattern id.
pub fn classify_http_facts(facts: &[StructuralFact]) -> (Vec<StructuralFact>, Vec<StructuralFact>) {
    let mut client_calls = Vec::new();
    let mut route_handlers = Vec::new();
    for fact in facts {
        if HTTP_CLIENT_CALL_PATTERN_IDS.contains(&fact.pattern_id.as_str()) {
            client_calls.push(fact.clone());
        } else if ROUTE_HANDLER_PATTERN_IDS.contains(&fact.pattern_id.as_str()) {
            route_handlers.push(fact.clone());
        }
    }
    (client_calls, route_handlers)
}

/// True if `facts` contains any web-relevant structural fact (an HTTP client
/// call, a route handler, or a SQL query/table fact). Used to gate the
/// post-index web-edge rebuild on the watcher hot path so non-web file saves
/// skip the rebuild cost.
pub fn facts_contain_web_patterns(facts: &[StructuralFact]) -> bool {
    facts.iter().any(|fact| {
        HTTP_CLIENT_CALL_PATTERN_IDS.contains(&fact.pattern_id.as_str())
            || ROUTE_HANDLER_PATTERN_IDS.contains(&fact.pattern_id.as_str())
            || SQL_QUERY_PATTERN_IDS.contains(&fact.pattern_id.as_str())
            || SQL_TABLE_PATTERN_IDS.contains(&fact.pattern_id.as_str())
    })
}

/// Derive `http_call` edges by joining client-call facts with route-handler
/// facts on normalized path + method. Each client call emits exactly one
/// edge: a matched edge when a handler's normalized route template matches
/// the client's literal target path (and method) with sufficient confidence,
/// otherwise an external-endpoint edge.
///
/// Handlers are bucketed by route-template segment count so each client call
/// only checks handlers whose template could possibly match (same segment
/// count). This makes derivation O(C + H) in the common case instead of
/// O(C × H) — important because `rebuild_web_edges` runs on every web-file
/// save.
pub fn derive_http_call_edges(
    client_call_facts: &[StructuralFact],
    route_handler_facts: &[StructuralFact],
) -> Vec<WebEdge> {
    // Index handlers by template segment count. A route template can only
    // match a literal path with the same number of segments, so bucketing
    // shrinks the per-client inner loop from all handlers to a small bucket.
    let mut handlers_by_segcount: HashMap<usize, Vec<&StructuralFact>> = HashMap::new();
    for handler in route_handler_facts {
        let Some(template) = get_str(handler.metadata.as_ref(), "normalized_route_template")
            .or_else(|| get_str(handler.metadata.as_ref(), "route_template"))
        else {
            continue;
        };
        let segcount = split_path(&template).len();
        handlers_by_segcount
            .entry(segcount)
            .or_default()
            .push(handler);
    }

    let mut edges = Vec::new();
    for client in client_call_facts {
        let Some(from_symbol_id) = non_empty(&client.containing_symbol_id) else {
            continue;
        };
        let verb = get_str(client.metadata.as_ref(), "verb");
        let target_path = get_str(client.metadata.as_ref(), "target_path");
        let method = verb.as_deref().map(|v| v.to_uppercase());
        let path = target_path.clone();
        let file_path = client.file_path.clone();
        let line_number = client.start_line;

        let best = target_path.as_deref().and_then(|p| {
            let segcount = split_path(p).len();
            let bucket = handlers_by_segcount
                .get(&segcount)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            best_handler_match(client, verb.as_deref(), p, bucket)
        });

        match best {
            Some((handler, combined)) if combined >= HTTP_MATCH_CONFIDENCE_THRESHOLD => {
                let to_symbol_id = non_empty(&handler.containing_symbol_id);
                let to_external = if to_symbol_id.is_none() {
                    Some(format_external(&method, &path))
                } else {
                    None
                };
                edges.push(WebEdge {
                    from_symbol_id,
                    to_symbol_id,
                    to_external,
                    kind: WebEdgeKind::HttpCall,
                    method,
                    path,
                    table: None,
                    file_path,
                    line_number,
                    confidence: combined,
                    metadata: None,
                });
            }
            _ => {
                edges.push(WebEdge {
                    from_symbol_id,
                    to_symbol_id: None,
                    to_external: Some(format_external(&method, &path)),
                    kind: WebEdgeKind::HttpCall,
                    method,
                    path,
                    table: None,
                    file_path,
                    line_number,
                    confidence: client.confidence,
                    metadata: None,
                });
            }
        }
    }
    edges
}

fn best_handler_match<'a>(
    client: &'a StructuralFact,
    verb: Option<&str>,
    target_path: &str,
    handlers: &[&'a StructuralFact],
) -> Option<(&'a StructuralFact, f32)> {
    let mut best: Option<(&StructuralFact, f32)> = None;
    for handler in handlers {
        let h_verb = get_str(handler.metadata.as_ref(), "verb");
        if !verbs_match(verb, h_verb.as_deref()) {
            continue;
        }
        let template = get_str(handler.metadata.as_ref(), "normalized_route_template")
            .or_else(|| get_str(handler.metadata.as_ref(), "route_template"));
        let Some(template) = template else {
            continue;
        };
        if !route_matches(target_path, &template) {
            continue;
        }
        let combined = client.confidence.min(handler.confidence);
        match &best {
            Some((_, bc)) if combined <= *bc => {}
            _ => best = Some((handler, combined)),
        }
    }
    best
}

/// Compare a client-call verb with a handler's verb. A handler with no recorded
/// verb (catch-all routes: `any`/`any_service`, `app.all`, `@All`, Razor page
/// directives, Nuxt server routes without a filename suffix) accepts any
/// method, so a missing handler verb matches any client verb. A client call
/// with no recorded verb cannot be matched reliably.
fn verbs_match(client_verb: Option<&str>, handler_verb: Option<&str>) -> bool {
    match (client_verb, handler_verb) {
        (Some(c), Some(h)) => c.eq_ignore_ascii_case(h),
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn non_empty(s: &Option<String>) -> Option<String> {
    match s {
        Some(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn get_str(meta: Option<&HashMap<String, serde_json::Value>>, key: &str) -> Option<String> {
    let map = meta?;
    let v = map.get(key)?;
    v.as_str().map(|s| s.to_string())
}

fn split_path(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

fn is_parametric_segment(seg: &str) -> bool {
    let s = seg.trim();
    if s.is_empty() {
        return false;
    }
    s.starts_with(':')
        || (s.starts_with('{') && s.ends_with('}'))
        || (s.starts_with('<') && s.ends_with('>'))
}

/// Match a literal request path against a route template, segment by
/// segment. Parametric segments (`:id`, `{id}`, `<id>`) match any single
/// non-empty literal segment; literal segments must match exactly.
pub fn route_matches(literal_path: &str, template: &str) -> bool {
    let lit = split_path(literal_path);
    let tmpl = split_path(template);
    if lit.len() != tmpl.len() {
        return false;
    }
    for (lp, tp) in lit.iter().zip(tmpl.iter()) {
        if is_parametric_segment(tp) {
            continue;
        }
        if lp != tp {
            return false;
        }
    }
    true
}

fn format_external(method: &Option<String>, path: &Option<String>) -> String {
    match (method, path) {
        (Some(m), Some(p)) => format!("{} {}", m, p),
        (Some(m), None) => m.clone(),
        (None, Some(p)) => p.clone(),
        (None, None) => String::from("?"),
    }
}

/// Split `facts` into (query facts, table-definition facts) by pattern id.
/// Query facts are SQL queries/mutations that reference tables (the edge
/// origin); table-definition facts resolve table names to in-workspace
/// table symbols (the edge target).
pub fn classify_sql_facts(facts: &[StructuralFact]) -> (Vec<StructuralFact>, Vec<StructuralFact>) {
    let mut queries = Vec::new();
    let mut tables = Vec::new();
    for fact in facts {
        if SQL_QUERY_PATTERN_IDS.contains(&fact.pattern_id.as_str()) {
            queries.push(fact.clone());
        } else if SQL_TABLE_PATTERN_IDS.contains(&fact.pattern_id.as_str()) {
            tables.push(fact.clone());
        }
    }
    (queries, tables)
}

/// Extract the table names a SQL query fact references, by pattern id.
/// - `view_definition`: `source_tables` array (one edge per source table).
/// - `update`/`insert`/`delete`: `table_name` string.
/// - `merge`: `target_table` string.
fn query_target_tables(fact: &StructuralFact) -> Vec<String> {
    match fact.pattern_id.as_str() {
        "sql.view_definition.v1" => get_str_array(fact.metadata.as_ref(), "source_tables"),
        "sql.update_statement.v1" | "sql.insert_statement.v1" | "sql.delete_statement.v1" => {
            get_str(fact.metadata.as_ref(), "table_name")
                .into_iter()
                .collect()
        }
        "sql.merge_statement.v1" => get_str(fact.metadata.as_ref(), "target_table")
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn get_str_array(meta: Option<&HashMap<String, serde_json::Value>>, key: &str) -> Vec<String> {
    let map = match meta {
        Some(m) => m,
        None => return Vec::new(),
    };
    let arr = match map.get(key).and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect()
}

/// Derive `sql_query` edges by joining SQL query/mutation facts (edge origin,
/// attached to their enclosing scope-bearing symbol — a routine or view) with
/// `sql.table_definition.v1` facts (edge target, attached to the table
/// symbol) on table name. Each referenced table emits one edge: a matched
/// edge when a table definition with the same name exists in the workspace,
/// otherwise an external-table edge (`table:<name>`). Facts with no
/// containing symbol (top-level ad-hoc statements) are skipped — they cannot
/// attach to the symbol graph.
pub fn derive_sql_query_edges(
    query_facts: &[StructuralFact],
    table_facts: &[StructuralFact],
) -> Vec<WebEdge> {
    // table_name -> table symbol id (first definition wins; duplicate
    // definitions of the same table name are a schema error, not a join key).
    let mut table_symbols: HashMap<String, String> = HashMap::new();
    for fact in table_facts {
        if fact.pattern_id != SQL_TABLE_DEFINITION_PATTERN_ID {
            continue;
        }
        let Some(table_name) = get_str(fact.metadata.as_ref(), "table_name") else {
            continue;
        };
        if let Some(sym_id) = non_empty(&fact.containing_symbol_id) {
            table_symbols.entry(table_name).or_insert(sym_id);
        }
    }

    let mut edges = Vec::new();
    for fact in query_facts {
        let Some(from_symbol_id) = non_empty(&fact.containing_symbol_id) else {
            continue;
        };
        for table_name in query_target_tables(fact) {
            if table_name.is_empty() {
                continue;
            }
            let to_symbol_id = table_symbols.get(&table_name).cloned();
            let to_external = if to_symbol_id.is_none() {
                Some(format!("table:{table_name}"))
            } else {
                None
            };
            edges.push(WebEdge {
                from_symbol_id: from_symbol_id.clone(),
                to_symbol_id,
                to_external,
                kind: WebEdgeKind::SqlQuery,
                method: None,
                path: None,
                table: Some(table_name),
                file_path: fact.file_path.clone(),
                line_number: fact.start_line,
                confidence: fact.confidence,
                metadata: None,
            });
        }
    }
    edges
}

/// Rebuild the entire `web_edges` table from the workspace's persisted
/// `structural_facts`. Reads every client-call + route-handler fact (Phase 1)
/// and every SQL query + table-definition fact (Phase 2), derives
/// `http_call` and `sql_query` edges, and atomically replaces the table.
///
/// This is the post-persistence derivation pass: because web edges are a
/// cross-file join (frontend client-call ↔ backend handler; routine ↔ table),
/// they cannot be derived per-file inside the watcher, so they are recomputed
/// from the full fact set after any indexing (batch or watcher). Returns the
/// edge count.
pub fn rebuild_web_edges(db: &mut SymbolDatabase) -> Result<usize> {
    let mut pattern_ids: Vec<&str> = HTTP_CLIENT_CALL_PATTERN_IDS.to_vec();
    pattern_ids.extend(ROUTE_HANDLER_PATTERN_IDS);
    pattern_ids.extend(SQL_QUERY_PATTERN_IDS);
    pattern_ids.extend(SQL_TABLE_PATTERN_IDS);
    let facts = db.load_all_structural_facts_by_pattern_ids(&pattern_ids)?;
    let (client_calls, route_handlers) = classify_http_facts(&facts);
    let mut edges = derive_http_call_edges(&client_calls, &route_handlers);
    let (sql_queries, sql_tables) = classify_sql_facts(&facts);
    edges.extend(derive_sql_query_edges(&sql_queries, &sql_tables));
    let count = edges.len();
    db.replace_all_web_edges(&edges)?;
    Ok(count)
}
