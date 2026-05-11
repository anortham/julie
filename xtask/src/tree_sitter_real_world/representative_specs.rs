use std::path::Path;

use rusqlite::{Connection, params};
use serde::Deserialize;

use super::TreeSitterRealWorldRepo;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepresentativeSpec {
    SymbolKind {
        name: String,
        expected_kind: String,
        #[serde(default)]
        language: Option<String>,
    },
    ReferenceCountAtLeast {
        name: String,
        min: i64,
        #[serde(default)]
        language: Option<String>,
    },
    ParentIdLinks {
        child_name: String,
        parent_name: String,
        #[serde(default)]
        language: Option<String>,
    },
    IdentifierAtPosition {
        name: String,
        kind_filter: String,
        file_path_contains: String,
        line_min: i64,
        #[serde(default)]
        language: Option<String>,
    },
    RelationshipEndpoints {
        from_name: String,
        to_name: String,
        relationship_kind: String,
        #[serde(default)]
        language: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct ResolvedSymbol {
    id: String,
    kind: String,
    parent_id: Option<String>,
}

pub(super) fn representative_spec_failures(
    repo: &TreeSitterRealWorldRepo,
    db_path: &Path,
) -> Vec<String> {
    if repo.representative_specs.is_empty() {
        return Vec::new();
    }

    let conn = match Connection::open(db_path) {
        Ok(conn) => conn,
        Err(error) => {
            return vec![format!(
                "{}: representative_specs.db_open: expected readable sqlite db at `{}`, got `{}`",
                repo.name,
                db_path.display(),
                error
            )];
        }
    };

    let mut failures = Vec::new();
    for spec in &repo.representative_specs {
        match spec {
            RepresentativeSpec::SymbolKind {
                name,
                expected_kind,
                language,
            } => {
                let Some(symbol) =
                    resolve_symbol(repo, &conn, "symbol_kind", name, language, &mut failures)
                else {
                    continue;
                };
                if symbol.kind != *expected_kind {
                    failures.push(format!(
                        "{}: representative_specs.symbol_kind({}): expected kind `{}`, got `{}`",
                        repo.name, name, expected_kind, symbol.kind
                    ));
                }
            }
            RepresentativeSpec::ReferenceCountAtLeast {
                name,
                min,
                language,
            } => {
                let Some(symbol) = resolve_symbol(
                    repo,
                    &conn,
                    "reference_count_at_least",
                    name,
                    language,
                    &mut failures,
                ) else {
                    continue;
                };
                let Some(actual) = query_count(
                    repo,
                    "reference_count_at_least",
                    name,
                    &conn,
                    "SELECT \
                        (SELECT COUNT(*) FROM identifiers WHERE target_symbol_id = ?1) + \
                        (SELECT COUNT(*) FROM relationships WHERE to_symbol_id = ?2)",
                    params![symbol.id.as_str(), symbol.id.as_str()],
                    &mut failures,
                ) else {
                    continue;
                };
                if actual < *min {
                    failures.push(format!(
                        "{}: representative_specs.reference_count_at_least({}): expected >={}, got {}",
                        repo.name, name, min, actual
                    ));
                }
            }
            RepresentativeSpec::ParentIdLinks {
                child_name,
                parent_name,
                language,
            } => {
                let Some(child) = resolve_symbol(
                    repo,
                    &conn,
                    "parent_id_links",
                    child_name,
                    language,
                    &mut failures,
                ) else {
                    continue;
                };
                let Some(parent) = resolve_symbol(
                    repo,
                    &conn,
                    "parent_id_links",
                    parent_name,
                    language,
                    &mut failures,
                ) else {
                    continue;
                };
                if child.parent_id.as_deref() != Some(parent.id.as_str()) {
                    let child_parent = child.parent_id.as_deref().unwrap_or("NULL");
                    failures.push(format!(
                        "{}: representative_specs.parent_id_links({} -> {}): expected child.parent_id == parent.id, got `{}` vs `{}`",
                        repo.name, child_name, parent_name, child_parent, parent.id
                    ));
                }
            }
            RepresentativeSpec::IdentifierAtPosition {
                name,
                kind_filter,
                file_path_contains,
                line_min,
                language,
            } => {
                let Some(_symbol) = resolve_symbol(
                    repo,
                    &conn,
                    "identifier_at_position",
                    name,
                    language,
                    &mut failures,
                ) else {
                    continue;
                };
                let like = format!("%{file_path_contains}%");
                let Some(actual) = query_count(
                    repo,
                    "identifier_at_position",
                    name,
                    &conn,
                    "SELECT COUNT(*) FROM identifiers \
                     WHERE name = ?1 \
                       AND language = ?2 \
                       AND kind = ?3 \
                       AND file_path LIKE ?4 \
                       AND start_line >= ?5",
                    params![
                        name,
                        language.as_deref().unwrap_or(&repo.language),
                        kind_filter,
                        like,
                        line_min
                    ],
                    &mut failures,
                ) else {
                    continue;
                };
                if actual == 0 {
                    failures.push(format!(
                        "{}: representative_specs.identifier_at_position({}): expected kind `{}` with file path containing `{}` and line >= {}, got 0 matches",
                        repo.name, name, kind_filter, file_path_contains, line_min
                    ));
                }
            }
            RepresentativeSpec::RelationshipEndpoints {
                from_name,
                to_name,
                relationship_kind,
                language,
            } => {
                let Some(from_symbol) = resolve_symbol(
                    repo,
                    &conn,
                    "relationship_endpoints",
                    from_name,
                    language,
                    &mut failures,
                ) else {
                    continue;
                };
                let Some(to_symbol) = resolve_symbol(
                    repo,
                    &conn,
                    "relationship_endpoints",
                    to_name,
                    language,
                    &mut failures,
                ) else {
                    continue;
                };
                let Some(actual) = query_count(
                    repo,
                    "relationship_endpoints",
                    from_name,
                    &conn,
                    "SELECT COUNT(*) FROM relationships \
                     WHERE from_symbol_id = ?1 AND to_symbol_id = ?2 AND kind = ?3",
                    params![from_symbol.id, to_symbol.id, relationship_kind],
                    &mut failures,
                ) else {
                    continue;
                };
                if actual == 0 {
                    failures.push(format!(
                        "{}: representative_specs.relationship_endpoints({} -{}-> {}): expected at least 1 relationship, got 0",
                        repo.name, from_name, relationship_kind, to_name
                    ));
                }
            }
        }
    }

    failures
}

fn query_count<P>(
    repo: &TreeSitterRealWorldRepo,
    spec_label: &str,
    subject: &str,
    conn: &Connection,
    sql: &str,
    params: P,
    failures: &mut Vec<String>,
) -> Option<i64>
where
    P: rusqlite::Params,
{
    match conn.query_row(sql, params, |row| row.get(0)) {
        Ok(count) => Some(count),
        Err(error) => {
            failures.push(format!(
                "{}: representative_specs.query_error({}): failed to count `{}`: {}",
                repo.name, spec_label, subject, error
            ));
            None
        }
    }
}

fn resolve_symbol(
    repo: &TreeSitterRealWorldRepo,
    conn: &Connection,
    spec_label: &str,
    symbol_name: &str,
    language: &Option<String>,
    failures: &mut Vec<String>,
) -> Option<ResolvedSymbol> {
    let lookup_language = language.as_deref().unwrap_or(&repo.language);
    let mut stmt = match conn.prepare(
        "SELECT id, kind, parent_id \
         FROM symbols \
         WHERE name = ?1 AND language = ?2 \
         LIMIT 1",
    ) {
        Ok(stmt) => stmt,
        Err(error) => {
            failures.push(format!(
                "{}: representative_specs.query_error({}): failed to prepare symbol lookup for `{}` in language `{}`: {}",
                repo.name, spec_label, symbol_name, lookup_language, error
            ));
            return None;
        }
    };

    let mut rows = match stmt.query(params![symbol_name, lookup_language]) {
        Ok(rows) => rows,
        Err(error) => {
            failures.push(format!(
                "{}: representative_specs.query_error({}): failed to query symbol `{}` in language `{}`: {}",
                repo.name, spec_label, symbol_name, lookup_language, error
            ));
            return None;
        }
    };

    let Some(row) = (match rows.next() {
        Ok(row) => row,
        Err(error) => {
            failures.push(format!(
                "{}: representative_specs.query_error({}): failed to iterate symbol lookup for `{}` in language `{}`: {}",
                repo.name, spec_label, symbol_name, lookup_language, error
            ));
            return None;
        }
    }) else {
        failures.push(format!(
            "{}: representative_specs.unresolvable_symbol({}): {} expected resolvable symbol in language `{}`",
            repo.name, symbol_name, spec_label, lookup_language
        ));
        return None;
    };

    let id: String = match row.get(0) {
        Ok(id) => id,
        Err(error) => {
            failures.push(format!(
                "{}: representative_specs.query_error({}): failed to read symbol id for `{}`: {}",
                repo.name, spec_label, symbol_name, error
            ));
            return None;
        }
    };
    let kind: String = match row.get(1) {
        Ok(kind) => kind,
        Err(error) => {
            failures.push(format!(
                "{}: representative_specs.query_error({}): failed to read symbol kind for `{}`: {}",
                repo.name, spec_label, symbol_name, error
            ));
            return None;
        }
    };
    let parent_id: Option<String> = match row.get(2) {
        Ok(parent_id) => parent_id,
        Err(error) => {
            failures.push(format!(
                "{}: representative_specs.query_error({}): failed to read parent id for `{}`: {}",
                repo.name, spec_label, symbol_name, error
            ));
            return None;
        }
    };

    if id.is_empty() {
        failures.push(format!(
            "{}: representative_specs.unresolvable_symbol({}): {} expected resolvable symbol in language `{}`",
            repo.name, symbol_name, spec_label, lookup_language
        ));
        return None;
    }
    Some(ResolvedSymbol {
        id,
        kind,
        parent_id,
    })
}

#[cfg(test)]
#[path = "representative_specs_tests.rs"]
mod tests;
