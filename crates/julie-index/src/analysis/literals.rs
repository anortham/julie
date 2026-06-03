//! Config-driven classification + bloat gate for captured string-literal
//! call-args (Miller bridge Phase 3).
//!
//! Mirrors [`crate::analysis::test_roles`]: the extractor layer captures
//! literals config-free (every string-literal call-arg, `kind = Other`, with
//! the verbatim callee `carrier`). This pass consults each language's
//! `[literal_carriers]` config to set the authoritative `kind` on a carrier
//! match and **drop** any literal whose carrier is not recognized — that drop
//! is the bloat gate (only carrier-recognized literals reach the DB).
//!
//! `carrier` matching is case-insensitive; the config sets are stored lowercase
//! (see [`crate::search::LanguageConfigs::build_literal_carrier_configs`]) and
//! the literal's carrier is lowercased before lookup. `kind` remains a
//! read-time-reclassifiable hint among the stored set because the verbatim
//! `carrier` is persisted alongside it.

use std::collections::{HashMap, HashSet};

use crate::extractors::{Literal, LiteralKind};

/// Per-language carrier vocabulary: the callee texts that mark a string-literal
/// argument as a URL, SQL, or route literal. Built once per indexing run from
/// the embedded TOML. All entries are lowercase for case-insensitive matching.
#[derive(Debug, Clone, Default)]
pub struct LiteralCarrierConfig {
    pub url: HashSet<String>,
    pub sql: HashSet<String>,
    pub route: HashSet<String>,
}

/// Does `carrier` match any entry in this lowercased config `set`?
///
/// Two match forms, so one shared rule covers every language's carrier
/// emission strategy (bare method name, or `object.property` member path):
/// - **Exact:** the set contains the full carrier verbatim. A *dotted* config
///   entry (`axios.get`) therefore matches only that exact `object.method`,
///   never a different receiver's `.get` — so the generic `.get()`/`.post()`
///   methods don't flood literals.
/// - **Last-segment:** the set contains the carrier's final dot-segment. A
///   *bare* config entry (`query`, `execute`, `fetch`) thus matches any
///   receiver — `pool.query`, `conn.Query`, `db.execute`, or a bare `fetch` —
///   which is the only way to recognize local-variable DB/HTTP receivers whose
///   names can't be enumerated in config (the TS-SQL local-receiver case).
///
/// `carrier` is already lowercased; the set is lowercased at build time.
fn carrier_matches(set: &HashSet<String>, carrier: &str) -> bool {
    if set.contains(carrier) {
        return true;
    }
    match carrier.rsplit_once('.') {
        Some((_, last)) => set.contains(last),
        None => false,
    }
}

/// Classify each captured literal by its `carrier` against that language's
/// config, setting `kind` on a match and dropping every non-match in place.
///
/// Runs on the mutable batch before persistence on every write path (the
/// multi-path trap — see the extract chokepoint + watcher wiring). A literal is
/// dropped when: its language has no carrier config, it has no `carrier`, or its
/// carrier matches none of the url/sql/route sets (see [`carrier_matches`] for
/// the exact-vs-last-segment matching rule).
pub fn classify_literals_by_carrier(
    literals: &mut Vec<Literal>,
    carrier_configs: &HashMap<String, LiteralCarrierConfig>,
) {
    literals.retain_mut(|literal| {
        let Some(config) = carrier_configs.get(&literal.language) else {
            // No carrier vocabulary for this language -> nothing to recognize.
            return false;
        };
        let Some(carrier) = literal.carrier.as_deref() else {
            // No callee text -> cannot match a carrier.
            return false;
        };
        let carrier = carrier.to_lowercase();
        // url > sql > route on the rare overlap; deterministic.
        if carrier_matches(&config.url, &carrier) {
            literal.kind = LiteralKind::Url;
            true
        } else if carrier_matches(&config.sql, &carrier) {
            literal.kind = LiteralKind::Sql;
            true
        } else if carrier_matches(&config.route, &carrier) {
            literal.kind = LiteralKind::Route;
            true
        } else {
            // Carrier not recognized -> drop (the bloat gate).
            false
        }
    });
}
