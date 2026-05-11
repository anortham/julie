use std::path::Path;

use rusqlite::{Connection, params};

use super::representative_spec_failures;
use crate::tree_sitter_real_world::TreeSitterRealWorldRepo;

fn write_real_world_test_db(path: &Path) {
    let conn = Connection::open(path).expect("open sqlite db");
    conn.execute_batch(
        "
            CREATE TABLE symbols (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                language TEXT NOT NULL,
                parent_id TEXT
            );
            CREATE TABLE identifiers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                language TEXT NOT NULL,
                file_path TEXT NOT NULL,
                start_line INTEGER NOT NULL,
                target_symbol_id TEXT
            );
            CREATE TABLE relationships (
                id TEXT PRIMARY KEY,
                from_symbol_id TEXT NOT NULL,
                to_symbol_id TEXT NOT NULL,
                kind TEXT NOT NULL
            );
            ",
    )
    .expect("create tables");

    conn.execute(
        "INSERT INTO symbols (id, name, kind, language, parent_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "router-id",
            "Phoenix.Router",
            "class",
            "elixir",
            Option::<String>::None
        ],
    )
    .expect("insert router symbol");
    conn.execute(
        "INSERT INTO symbols (id, name, kind, language, parent_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "match-id",
            "Phoenix.Router.match",
            "function",
            "elixir",
            "other-parent-id"
        ],
    )
    .expect("insert router match symbol");
    conn.execute(
        "INSERT INTO symbols (id, name, kind, language, parent_id) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "endpoint-id",
            "Phoenix.Endpoint",
            "module",
            "elixir",
            Option::<String>::None
        ],
    )
    .expect("insert endpoint symbol");

    conn.execute(
        "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, target_symbol_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "ident-1",
            "Phoenix.Router",
            "type_usage",
            "elixir",
            "lib/blog_web/router.ex",
            3,
            "router-id"
        ],
    )
    .expect("insert identifier");

    conn.execute(
        "INSERT INTO relationships (id, from_symbol_id, to_symbol_id, kind) VALUES (?1, ?2, ?3, ?4)",
        params!["rel-1", "endpoint-id", "router-id", "Calls"],
    )
    .expect("insert relationship");
}

#[test]
fn representative_specs_default_to_empty() {
    let repo: TreeSitterRealWorldRepo = toml::from_str(
        r#"
name = "phoenix"
language = "elixir"
profile_tags = ["release"]
min_files = 1
min_language_files = 1
min_symbols = 1
min_relationships = 0
"#,
    )
    .expect("deserialize repo without representative specs");

    assert!(
        repo.representative_specs.is_empty(),
        "representative_specs should default to an empty vector"
    );
}

#[test]
fn identifier_at_position_matches_named_identifier_without_target_link() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let db_path = tmp.path().join("symbols.db");
    write_real_world_test_db(&db_path);
    let conn = Connection::open(&db_path).expect("open sqlite db");
    conn.execute(
        "INSERT INTO identifiers (id, name, kind, language, file_path, start_line, target_symbol_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "ident-2",
            "Phoenix.Router",
            "type_usage",
            "elixir",
            "lib/blog_web/router.ex",
            7,
            Option::<String>::None
        ],
    )
    .expect("insert unlinked identifier");

    let repo: TreeSitterRealWorldRepo = toml::from_str(
        r#"
name = "phoenix"
language = "elixir"
profile_tags = ["release"]
min_files = 1
min_language_files = 1
min_symbols = 1
min_relationships = 0

[[representative_specs]]
kind = "identifier_at_position"
name = "Phoenix.Router"
kind_filter = "type_usage"
file_path_contains = "lib/blog_web/router.ex"
line_min = 5
"#,
    )
    .expect("deserialize representative specs");

    let failures = representative_spec_failures(&repo, &db_path);

    assert_eq!(
        failures,
        Vec::<String>::new(),
        "expected unlinked named identifier span to satisfy spec"
    );
}

#[test]
fn hard_failures_enforces_representative_specs() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let db_path = tmp.path().join("symbols.db");
    write_real_world_test_db(&db_path);

    let repo: TreeSitterRealWorldRepo = toml::from_str(
        r#"
name = "phoenix"
language = "elixir"
profile_tags = ["release"]
min_files = 1
min_language_files = 1
min_symbols = 1
min_relationships = 0

[[representative_specs]]
kind = "symbol_kind"
name = "Phoenix.Router"
expected_kind = "module"

[[representative_specs]]
kind = "reference_count_at_least"
name = "Phoenix.Router"
min = 3

[[representative_specs]]
kind = "parent_id_links"
child_name = "Phoenix.Router.match"
parent_name = "Phoenix.Router"

[[representative_specs]]
kind = "identifier_at_position"
name = "Phoenix.Router"
kind_filter = "type_usage"
file_path_contains = "lib/blog_web/router.ex"
line_min = 5

[[representative_specs]]
kind = "relationship_endpoints"
from_name = "Phoenix.Endpoint"
to_name = "Phoenix.Router"
relationship_kind = "Uses"

[[representative_specs]]
kind = "reference_count_at_least"
name = "Missing.Symbol"
min = 1
"#,
    )
    .expect("deserialize representative specs");

    let failures = representative_spec_failures(&repo, &db_path);

    assert!(
        failures.iter().any(|failure| failure == "phoenix: representative_specs.symbol_kind(Phoenix.Router): expected kind `module`, got `class`"),
        "expected symbol_kind failure, got: {failures:?}"
    );
    assert!(
        failures.iter().any(|failure| failure == "phoenix: representative_specs.reference_count_at_least(Phoenix.Router): expected >=3, got 2"),
        "expected reference_count_at_least failure, got: {failures:?}"
    );
    assert!(
        failures.iter().any(|failure| failure == "phoenix: representative_specs.parent_id_links(Phoenix.Router.match -> Phoenix.Router): expected child.parent_id == parent.id, got `other-parent-id` vs `router-id`"),
        "expected parent_id_links failure, got: {failures:?}"
    );
    assert!(
        failures.iter().any(|failure| failure == "phoenix: representative_specs.identifier_at_position(Phoenix.Router): expected kind `type_usage` with file path containing `lib/blog_web/router.ex` and line >= 5, got 0 matches"),
        "expected identifier_at_position failure, got: {failures:?}"
    );
    assert!(
        failures.iter().any(|failure| failure == "phoenix: representative_specs.relationship_endpoints(Phoenix.Endpoint -Uses-> Phoenix.Router): expected at least 1 relationship, got 0"),
        "expected relationship_endpoints failure, got: {failures:?}"
    );
    assert!(
        failures.iter().any(|failure| failure == "phoenix: representative_specs.unresolvable_symbol(Missing.Symbol): reference_count_at_least expected resolvable symbol in language `elixir`"),
        "expected unresolvable_symbol failure, got: {failures:?}"
    );
}
