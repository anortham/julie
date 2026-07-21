use super::*;
use anyhow::Result;
use tracing::debug;

impl SymbolDatabase {
    /// Create the `type_arguments` table: ordered, nested generic type
    /// arguments captured at *use sites* (e.g. `new List<Foo>()`,
    /// `CreateMap<A,B>()`, `axios.get<User>(...)`).
    ///
    /// Self-referential (`parent_arg_id`) to preserve arbitrary nesting; keyed
    /// to the use-site identifier (`identifier_id`); `ordinal` preserves
    /// argument order (the whole point — e.g. `CreateMap` source-vs-dest).
    /// `target_symbol_id` is resolved on demand by consumers (NULL at extract,
    /// mirroring `identifiers`). Carries its own `file_path` so per-file cleanup
    /// is a flat `DELETE ... WHERE file_path = ?1` with no dependency on
    /// identifier delete-ordering (cross-cutting Rule 1).
    ///
    /// `pub(crate)` so `migration_027_add_type_arguments` can call it; the
    /// `CREATE ... IF NOT EXISTS` DDL is the single source of truth for both
    /// fresh DBs (via `initialize_schema`) and upgrades (via migration 027).
    pub(crate) fn create_type_arguments_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS type_arguments (
                id               TEXT PRIMARY KEY,
                identifier_id    TEXT NOT NULL REFERENCES identifiers(id) ON DELETE CASCADE,
                parent_arg_id    TEXT REFERENCES type_arguments(id) ON DELETE CASCADE,
                ordinal          INTEGER NOT NULL,
                type_name        TEXT NOT NULL,
                target_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
                file_path        TEXT NOT NULL,
                language         TEXT NOT NULL,
                last_indexed     INTEGER DEFAULT 0
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_type_args_identifier ON type_arguments(identifier_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_type_args_parent ON type_arguments(parent_arg_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_type_args_name ON type_arguments(type_name)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_type_args_file ON type_arguments(file_path)",
            [],
        )?;

        debug!("Created type_arguments table and indexes");
        Ok(())
    }

    /// Create the `literals` table: string-literal call-arguments captured at
    /// recognized HTTP/DB carrier sites (Miller bridge Phase 3), e.g. the
    /// `/api/users` of `fetch(\"/api/users\")` or the SQL body of a Dapper
    /// `conn.Query<T>(\"SELECT ... FROM Users\")`.
    ///
    /// Mirrors `identifiers`' column shape (so it carries `file_path` and a
    /// `containing_symbol_id`) but swaps `name`→`literal_text` and adds the
    /// classified `kind`, the verbatim `carrier` callee, and the 0-based
    /// `arg_position`. `literal_text` is the DECODED contents (delimiters
    /// stripped; interpolation holes folded to `{}`); `kind` is a read-time
    /// reclassifiable hint (`url`/`sql`/`route`/`other`) — the verbatim
    /// `carrier` is persisted so consumers can reclassify unknown clients.
    ///
    /// Deliberately has no index on `literal_text` (the table is name-free,
    /// unlike `identifiers`, so URLs/SQL never pollute the name index or skew
    /// centrality). Carries its own `file_path` so per-file cleanup is a flat
    /// `DELETE ... WHERE file_path = ?1` independent of identifier
    /// delete-ordering (cross-cutting Rule 1).
    ///
    /// `pub(crate)` so `migration_028_add_literals` can call it; the
    /// `CREATE ... IF NOT EXISTS` DDL is the single source of truth for both
    /// fresh DBs (via `initialize_schema`) and upgrades (via migration 028).
    pub(crate) fn create_literals_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS literals (
                id                   TEXT PRIMARY KEY,
                literal_text         TEXT NOT NULL,
                kind                 TEXT NOT NULL,  -- url, sql, route, other
                carrier              TEXT,           -- callee that introduced it (fetch, axios.get, Query)
                arg_position         INTEGER NOT NULL,
                language             TEXT NOT NULL,

                -- Location
                file_path            TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                start_line           INTEGER NOT NULL,
                start_col            INTEGER NOT NULL,
                end_line             INTEGER NOT NULL,
                end_col              INTEGER NOT NULL,
                start_byte           INTEGER,
                end_byte             INTEGER,

                -- Semantic link (NULL until/unless an enclosing symbol is found)
                containing_symbol_id TEXT REFERENCES symbols(id) ON DELETE CASCADE,
                confidence           REAL DEFAULT 1.0,

                -- Infrastructure
                last_indexed         INTEGER DEFAULT 0
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_literals_kind ON literals(kind)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_literals_containing ON literals(containing_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_literals_file ON literals(file_path)",
            [],
        )?;

        debug!("Created literals table and indexes");
        Ok(())
    }

    pub(crate) fn create_source_regions_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS source_regions (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                language TEXT NOT NULL,
                kind TEXT NOT NULL,
                containing_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                metadata TEXT,
                last_indexed INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_source_regions_file_kind
                ON source_regions(file_path, kind);
            CREATE INDEX IF NOT EXISTS idx_source_regions_containing
                ON source_regions(containing_symbol_id);",
        )?;
        debug!("Created source_regions table and indexes");
        Ok(())
    }

    pub(crate) fn create_structural_facts_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS structural_facts (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                language TEXT NOT NULL,
                pattern_id TEXT NOT NULL,
                capture_name TEXT NOT NULL,
                node_kind TEXT NOT NULL,
                containing_symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                confidence REAL NOT NULL,
                metadata TEXT,
                last_indexed INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_structural_facts_pattern
                ON structural_facts(pattern_id);
            CREATE INDEX IF NOT EXISTS idx_structural_facts_file
                ON structural_facts(file_path);
            CREATE INDEX IF NOT EXISTS idx_structural_facts_language_pattern
                ON structural_facts(language, pattern_id);
            CREATE INDEX IF NOT EXISTS idx_structural_facts_containing
                ON structural_facts(containing_symbol_id);",
        )?;
        debug!("Created structural_facts table and indexes");
        Ok(())
    }

    pub(crate) fn create_complexity_metrics_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS complexity_metrics (
                id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                language TEXT NOT NULL,
                scope TEXT NOT NULL,
                symbol_id TEXT REFERENCES symbols(id) ON DELETE SET NULL,
                algorithm_id TEXT NOT NULL,
                covered_lines INTEGER NOT NULL,
                covered_bytes INTEGER NOT NULL,
                decision_count INTEGER NOT NULL,
                loop_count INTEGER NOT NULL,
                max_nesting_depth INTEGER NOT NULL,
                parameter_count INTEGER,
                start_line INTEGER NOT NULL,
                start_col INTEGER NOT NULL,
                end_line INTEGER NOT NULL,
                end_col INTEGER NOT NULL,
                start_byte INTEGER NOT NULL,
                end_byte INTEGER NOT NULL,
                metadata TEXT,
                last_indexed INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_complexity_metrics_symbol
                ON complexity_metrics(symbol_id);
            CREATE INDEX IF NOT EXISTS idx_complexity_metrics_file
                ON complexity_metrics(file_path);
            CREATE INDEX IF NOT EXISTS idx_complexity_metrics_language_scope
                ON complexity_metrics(language, scope);",
        )?;
        debug!("Created complexity_metrics table and indexes");
        Ok(())
    }

    /// Create the `web_edges` table: *derived* navigation edges produced by
    /// julie-pipeline from `structural_facts` (e.g. `http_call` joining an
    /// `http.client_request.v1` client-call fact to a route-handler fact).
    /// Unlike `relationships` (whose `kind` is the extractor's closed
    /// `RelationshipKind` enum), web edges are Julie-owned derived data, so
    /// their `kind` is a free-form string (`http_call` / `sql_query`) and the
    /// table lives in julie-core.
    ///
    /// Each edge has either an in-workspace `to_symbol_id` (matched handler /
    /// table) or a `to_external` endpoint/table label (unmatched call). Carries
    /// its own `file_path` so per-file cleanup is a flat `DELETE ... WHERE
    /// file_path = ?1` (cross-cutting Rule 1).
    ///
    /// `pub(crate)` so `migration_030_add_web_edges` can call it; the
    /// `CREATE ... IF NOT EXISTS` DDL is the single source of truth for both
    /// fresh DBs (via `initialize_schema`) and upgrades (via migration 030).
    pub(crate) fn create_web_edges_table(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS web_edges (
                id               TEXT PRIMARY KEY,
                from_symbol_id   TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                to_symbol_id     TEXT REFERENCES symbols(id) ON DELETE SET NULL,
                to_external      TEXT,
                kind             TEXT NOT NULL,   -- http_call | sql_query
                method           TEXT,
                path             TEXT,
                table_name        TEXT,
                file_path        TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                line_number      INTEGER NOT NULL,
                confidence       REAL NOT NULL,
                metadata         TEXT,
                last_indexed     INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_web_edges_from
                ON web_edges(from_symbol_id);
            CREATE INDEX IF NOT EXISTS idx_web_edges_to
                ON web_edges(to_symbol_id);
            CREATE INDEX IF NOT EXISTS idx_web_edges_kind
                ON web_edges(kind);
            CREATE INDEX IF NOT EXISTS idx_web_edges_file
                ON web_edges(file_path);
            CREATE INDEX IF NOT EXISTS idx_web_edges_external
                ON web_edges(to_external);",
        )?;
        debug!("Created web_edges table and indexes");
        Ok(())
    }
}
