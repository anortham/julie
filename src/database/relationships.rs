// Relationship operations for tracing data flow

use super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::debug;

impl SymbolDatabase {
    pub fn delete_relationships_for_file(&self, file_path: &str) -> Result<()> {
        // Delete relationships where either the from_symbol or to_symbol belongs to the file
        let count = self.conn.execute(
            "DELETE FROM relationships
             WHERE from_symbol_id IN (
                 SELECT id FROM symbols WHERE file_path = ?1
             )
             OR to_symbol_id IN (
                 SELECT id FROM symbols WHERE file_path = ?1
             )",
            params![file_path],
        )?;

        debug!("Deleted {} relationships for file '{}'", count, file_path);
        Ok(())
    }

    pub fn get_outgoing_relationships(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
             FROM relationships
             WHERE from_symbol_id = ?1",
        )?;

        let rel_iter = stmt.query_map(params![symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for rel_result in rel_iter {
            relationships.push(rel_result?);
        }

        debug!(
            "Found {} outgoing relationships from symbol '{}'",
            relationships.len(),
            symbol_id
        );
        Ok(relationships)
    }

    /// Get relationships TO a symbol (where symbol is the target/referenced)
    /// Uses indexed query on to_symbol_id for O(log n) performance
    /// Complements get_outgoing_relationships() which finds relationships FROM a symbol
    pub fn get_relationships_to_symbol(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
            FROM relationships
            WHERE to_symbol_id = ?1
        ",
        )?;

        let rows = stmt.query_map([symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for row_result in rows {
            relationships.push(row_result?);
        }

        Ok(relationships)
    }

    /// Get relationships TO multiple symbols in a single batch query.
    /// Chunked in batches of 500 to stay within SQLite's bind parameter limit (999).
    pub fn get_relationships_to_symbols(&self, symbol_ids: &[String]) -> Result<Vec<Relationship>> {
        if symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        const CHUNK_SIZE: usize = 500;
        let mut relationships = Vec::new();

        for chunk in symbol_ids.chunks(CHUNK_SIZE) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let query = format!(
                "SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
                 FROM relationships
                 WHERE to_symbol_id IN ({})",
                placeholders.join(", ")
            );

            let mut stmt = self.conn.prepare(&query)?;
            let params: Vec<&dyn rusqlite::ToSql> =
                chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

            let rows = stmt.query_map(&params[..], |row| self.row_to_relationship(row))?;
            for row in rows {
                relationships.push(row?);
            }
        }

        Ok(relationships)
    }

    /// Get relationships FROM multiple symbols in a single batch query.
    /// Chunked in batches of 500 to stay within SQLite's bind parameter limit (999).
    pub fn get_outgoing_relationships_for_symbols(
        &self,
        symbol_ids: &[String],
    ) -> Result<Vec<Relationship>> {
        if symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        const CHUNK_SIZE: usize = 500;
        let mut relationships = Vec::new();

        for chunk in symbol_ids.chunks(CHUNK_SIZE) {
            let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let query = format!(
                "SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
                 FROM relationships
                 WHERE from_symbol_id IN ({})",
                placeholders.join(", ")
            );

            let mut stmt = self.conn.prepare(&query)?;
            let params: Vec<&dyn rusqlite::ToSql> =
                chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

            let rows = stmt.query_map(&params[..], |row| self.row_to_relationship(row))?;
            for row in rows {
                relationships.push(row?);
            }
        }

        Ok(relationships)
    }

    /// Get relationships pointing TO these symbols, filtered by identifier kind.
    /// Chunked in batches of 499 (leaves one slot for the identifier_kind param).
    pub fn get_relationships_to_symbols_filtered_by_kind(
        &self,
        symbol_ids: &[String],
        identifier_kind: &str,
    ) -> Result<Vec<Relationship>> {
        if symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Reserve one bind slot for identifier_kind, so symbol IDs get 499 slots per chunk.
        const CHUNK_SIZE: usize = 499;
        let mut relationships = Vec::new();

        for chunk in symbol_ids.chunks(CHUNK_SIZE) {
            // First N params are symbol IDs, last param is identifier_kind.
            let id_placeholders: Vec<String> =
                (1..=chunk.len()).map(|i| format!("?{}", i)).collect();
            let kind_idx = chunk.len() + 1;

            let query = format!(
                "SELECT DISTINCT r.id, r.from_symbol_id, r.to_symbol_id, r.kind, r.file_path, r.line_number, r.confidence, r.metadata
                 FROM relationships r
                 INNER JOIN identifiers i ON r.file_path = i.file_path AND r.line_number = i.start_line
                 WHERE r.to_symbol_id IN ({})
                   AND i.kind = ?{}",
                id_placeholders.join(", "),
                kind_idx
            );

            let mut stmt = self.conn.prepare(&query)?;

            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            for id in chunk {
                params.push(Box::new(id.clone()));
            }
            params.push(Box::new(identifier_kind.to_string()));

            let param_refs: Vec<&dyn rusqlite::ToSql> = params
                .iter()
                .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
                .collect();

            let rows = stmt.query_map(&param_refs[..], |row| self.row_to_relationship(row))?;
            for row in rows {
                relationships.push(row?);
            }
        }

        Ok(relationships)
    }

    pub fn get_file_relationship_statistics(
        &self,
    ) -> Result<std::collections::HashMap<String, usize>> {
        use std::collections::HashMap;

        let mut by_file = HashMap::new();

        // This is a more complex query: count relationships per file
        // We need to join symbols with relationships to count how many relationships involve symbols from each file
        let rel_query = "SELECT s.file_path, COUNT(DISTINCT r.id) as count \
                         FROM symbols s \
                         LEFT JOIN relationships r ON (r.from_symbol_id = s.id OR r.to_symbol_id = s.id) \
                         GROUP BY s.file_path";

        let mut stmt = self.conn.prepare(rel_query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;

        for row in rows {
            let (file_path, count) = row?;
            by_file.insert(file_path, count);
        }

        Ok(by_file)
    }

    /// Compute reference_score for all symbols based on weighted incoming relationships.
    /// Self-references (recursion) are excluded.
    ///
    /// Weights by relationship kind:
    ///   Calls=3, Implements/Imports/Extends/Instantiates=2,
    ///   Uses/References/Returns/Parameter/Defines/Overrides/Joins/Composition=1,
    ///   Contains=0 (structural, not a usage signal)
    ///
    /// After computing direct scores:
    /// - Propagates centrality from interfaces/base classes to implementations (70%).
    /// - Propagates constructor centrality to parent class (70%), fixing DI patterns
    ///   where all references target the constructor, leaving the class at zero.
    /// - De-weights symbols in test files by 90%, preventing test doubles/subclasses
    ///   from stealing centrality from real production definitions.
    /// - Propagates centrality from C/C++ header declarations to implementations (70%).
    pub fn compute_reference_scores(&self) -> Result<()> {
        // Wrap all 5 UPDATE steps in one transaction so a partial failure leaves
        // reference_score in a consistent state (all-or-nothing).
        let tx = self.conn.unchecked_transaction()?;

        // Step 1: Compute direct reference scores from incoming relationships
        tx.execute(
            "UPDATE symbols SET reference_score = COALESCE(
                (SELECT SUM(
                    CASE r.kind
                        WHEN 'calls' THEN 3.0
                        WHEN 'implements' THEN 2.0
                        WHEN 'imports' THEN 2.0
                        WHEN 'extends' THEN 2.0
                        WHEN 'instantiates' THEN 2.0
                        WHEN 'uses' THEN 1.0
                        WHEN 'references' THEN 1.0
                        WHEN 'returns' THEN 1.0
                        WHEN 'parameter' THEN 1.0
                        WHEN 'defines' THEN 1.0
                        WHEN 'overrides' THEN 1.0
                        WHEN 'joins' THEN 1.0
                        WHEN 'composition' THEN 1.0
                        WHEN 'contains' THEN 0.0
                        ELSE 1.0
                    END
                )
                FROM relationships r
                WHERE r.to_symbol_id = symbols.id
                  AND r.from_symbol_id != symbols.id),
                0.0
            )",
            [],
        )?;

        // Step 1b: Boost centrality from identifier references (cross-file only).
        // Languages reference types through type annotations (var x: Foo, extends Foo)
        // and imports (const Foo = @import("Foo.zig"), import { Foo } from "./foo").
        // These are captured as identifiers but not always as relationships,
        // so without this step, heavily-imported/referenced types would have zero centrality.
        // Weight: type_usage=1.0, import=2.0 (same as relationship weights).
        //
        // Name matching: exact OR qualified suffix match. QML and other languages use
        // namespace-qualified references (Kirigami.ScrollablePage, Phoenix.Router) where
        // the last component matches the symbol name. Without suffix matching, all QML
        // components would have centrality 0.00 despite heavy usage.
        tx.execute(
            "UPDATE symbols SET reference_score = reference_score + COALESCE(
                (SELECT SUM(
                    CASE i.kind
                        WHEN 'import' THEN 2.0
                        WHEN 'type_usage' THEN 1.0
                        ELSE 0.0
                    END
                )
                 FROM identifiers i
                 WHERE (i.name = symbols.name
                        OR i.name LIKE '%.' || REPLACE(REPLACE(REPLACE(symbols.name, '\\', '\\\\'), '%', '\\%'), '_', '\\_') ESCAPE '\\')
                   AND i.kind IN ('type_usage', 'import')
                   AND i.file_path != symbols.file_path),
                0.0
            )
            WHERE kind IN ('class', 'struct', 'enum', 'interface', 'trait', 'type', 'module', 'namespace', 'constant')
              AND NOT (
                  -- Directory segments (matches is_test_path() in scoring.rs)
                  file_path LIKE '%/test/%' OR file_path LIKE '%/tests/%'
                  OR file_path LIKE 'test/%' OR file_path LIKE 'tests/%'
                  OR file_path LIKE '%/spec/%' OR file_path LIKE '%/specs/%'
                  OR file_path LIKE 'spec/%' OR file_path LIKE 'specs/%'
                  OR file_path LIKE '%/__tests__/%' OR file_path LIKE '__tests__/%'
                  -- C# convention: MyProject.Tests/ or MyProject.Test/
                  OR file_path LIKE '%.Tests/%' OR file_path LIKE '%.Test/%'
                  -- Python: test_*.py
                  OR file_path LIKE '%/test\\_%' ESCAPE '\\' OR file_path LIKE 'test\\_%' ESCAPE '\\'
                  -- Go: *_test.go
                  OR file_path LIKE '%\\_test.go' ESCAPE '\\'
                  -- C/C++: *_test.c, *_test.cc, *_test.cpp
                  OR file_path LIKE '%\\_test.c' ESCAPE '\\' OR file_path LIKE '%\\_test.cc' ESCAPE '\\'
                  OR file_path LIKE '%\\_test.cpp' ESCAPE '\\'
                  -- JS/TS: *.test.ts, *.test.tsx, *.test.js, *.test.jsx
                  OR file_path LIKE '%.test.ts' OR file_path LIKE '%.test.tsx'
                  OR file_path LIKE '%.test.js' OR file_path LIKE '%.test.jsx'
                  -- JS/TS: *.spec.ts, *.spec.tsx, *.spec.js, *.spec.jsx
                  OR file_path LIKE '%.spec.ts' OR file_path LIKE '%.spec.tsx'
                  OR file_path LIKE '%.spec.js' OR file_path LIKE '%.spec.jsx'
              )",
            [],
        )?;

        // Step 2: Propagate centrality from interfaces/base classes to implementations.
        // When class Foo implements IBar, Foo gets 70% of IBar's centrality added.
        // This fixes C# DI patterns where all references go through interfaces,
        // leaving concrete implementations with zero centrality.
        tx.execute(
            "UPDATE symbols SET reference_score = reference_score + COALESCE(
                (SELECT SUM(target_sym.reference_score * 0.7)
                 FROM relationships r
                 JOIN symbols target_sym ON target_sym.id = r.to_symbol_id
                 WHERE r.from_symbol_id = symbols.id
                   AND r.kind IN ('implements', 'extends')
                   AND r.from_symbol_id != r.to_symbol_id
                ), 0.0
            )
            WHERE EXISTS (
                SELECT 1 FROM relationships r
                WHERE r.from_symbol_id = symbols.id
                  AND r.kind IN ('implements', 'extends')
            )",
            [],
        )?;

        // Step 3: Propagate constructor centrality to parent class.
        // In C# / Java / TypeScript DI patterns, all references target the constructor,
        // leaving the class itself with zero centrality. Give the class 70% of its
        // highest-scoring constructor's score (same factor as interface→implementation).
        tx.execute(
            "UPDATE symbols SET reference_score = reference_score + COALESCE(
                (SELECT MAX(ctor.reference_score) * 0.7
                 FROM symbols ctor
                 WHERE ctor.parent_id = symbols.id
                   AND ctor.kind = 'constructor'
                   AND ctor.reference_score > 0
                ), 0.0
            )
            WHERE kind IN ('class', 'struct')
              AND reference_score = 0.0
              AND EXISTS (
                  SELECT 1 FROM symbols ctor
                  WHERE ctor.parent_id = symbols.id
                    AND ctor.kind = 'constructor'
                    AND ctor.reference_score > 0
              )",
            [],
        )?;

        // Step 4: De-weight symbols located in test files.
        // Test files often contain test doubles, subclasses, or mocks that share names
        // with real production symbols. Without de-weighting, these test symbols can
        // steal centrality from the real definitions (e.g., a test Flask subclass
        // accumulating more references than the real Flask class).
        //
        // Language-agnostic test path patterns (matches Rust, Python, JS/TS, C#, Java,
        // Go, Ruby, Swift, etc.):
        //   Directory segments: test, tests, spec, specs, __tests__, .Tests, .Test
        //   File prefixes: test_*.py (uses ESCAPE '\\' so \_ matches literal underscore)
        //   File suffixes: _test.go, .test.ts, .spec.ts, etc.
        tx.execute(
            "UPDATE symbols SET reference_score = reference_score * 0.1
             WHERE reference_score > 0
               AND (
                   -- Directory-based: /test/, /tests/, /spec/, /specs/, /__tests__/
                   file_path LIKE '%/test/%'
                   OR file_path LIKE '%/tests/%'
                   OR file_path LIKE '%/spec/%'
                   OR file_path LIKE '%/specs/%'
                   OR file_path LIKE '%/__tests__/%'
                   -- Starts with test/ or tests/ (no leading slash)
                   OR file_path LIKE 'test/%'
                   OR file_path LIKE 'tests/%'
                   OR file_path LIKE 'spec/%'
                   OR file_path LIKE 'specs/%'
                   OR file_path LIKE '__tests__/%'
                   -- C# convention: MyProject.Tests/ or MyProject.Test/
                   OR file_path LIKE '%.Tests/%'
                   OR file_path LIKE '%.Test/%'
                   -- Python: test_*.py files (\\_ escapes _ as literal with ESCAPE '\\')
                   OR file_path LIKE '%/test\\_%' ESCAPE '\\'
                   OR file_path LIKE 'test\\_%' ESCAPE '\\'
                   -- Go: *_test.go (\\ escapes _ as literal with ESCAPE '\\')
                   OR file_path LIKE '%\\_test.go' ESCAPE '\\'
                   -- JS/TS: *.test.ts, *.test.tsx, *.test.js, *.test.jsx
                   OR file_path LIKE '%.test.ts'
                   OR file_path LIKE '%.test.tsx'
                   OR file_path LIKE '%.test.js'
                   OR file_path LIKE '%.test.jsx'
                   -- JS/TS: *.spec.ts, *.spec.tsx, *.spec.js, *.spec.jsx
                   OR file_path LIKE '%.spec.ts'
                   OR file_path LIKE '%.spec.tsx'
                   OR file_path LIKE '%.spec.js'
                   OR file_path LIKE '%.spec.jsx'
               )",
            [],
        )?;

        // Step 5: Propagate centrality from C/C++ header declarations to implementations.
        // In C/C++, .h declarations accumulate all refs (via #include) while .c/.cpp
        // implementations get zero. Give implementations 70% of their declaration's score.
        // Same propagation factor as interface→implementation (Step 2).
        tx.execute(
            "UPDATE symbols SET reference_score = reference_score + COALESCE(
                (SELECT MAX(header_sym.reference_score) * 0.7
                 FROM symbols header_sym
                 WHERE header_sym.name = symbols.name
                   AND header_sym.kind = symbols.kind
                   AND header_sym.id != symbols.id
                   AND (header_sym.file_path LIKE '%.h'
                        OR header_sym.file_path LIKE '%.hpp'
                        OR header_sym.file_path LIKE '%.hh')
                   AND header_sym.reference_score > 0
                ), 0.0
            )
            WHERE reference_score = 0.0
              AND (file_path LIKE '%.c'
                   OR file_path LIKE '%.cpp'
                   OR file_path LIKE '%.cc'
                   OR file_path LIKE '%.cxx')
              AND kind = 'function'
              AND EXISTS (
                  SELECT 1 FROM symbols header_sym
                  WHERE header_sym.name = symbols.name
                    AND header_sym.kind = symbols.kind
                    AND (header_sym.file_path LIKE '%.h'
                         OR header_sym.file_path LIKE '%.hpp'
                         OR header_sym.file_path LIKE '%.hh')
                    AND header_sym.reference_score > 0
              )",
            [],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Get reference_score for a batch of symbol IDs.
    /// Returns a HashMap of id -> score for efficient lookup during search scoring.
    ///
    /// Batches queries in chunks of 900 to stay within SQLite's bind parameter limit (~999).
    pub fn get_reference_scores(&self, ids: &[&str]) -> Result<HashMap<String, f64>> {
        const MAX_BIND_PARAMS: usize = 900;

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let mut scores = HashMap::new();
        for chunk in ids.chunks(MAX_BIND_PARAMS) {
            let placeholders: Vec<&str> = chunk.iter().map(|_| "?").collect();
            let sql = format!(
                "SELECT id, reference_score FROM symbols WHERE id IN ({})",
                placeholders.join(", ")
            );

            let mut stmt = self.conn.prepare(&sql)?;

            let params: Vec<&dyn rusqlite::ToSql> =
                chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

            let rows = stmt.query_map(&params[..], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?;

            for row in rows {
                let (id, score) = row?;
                scores.insert(id, score);
            }
        }
        Ok(scores)
    }

    /// Get relationship type statistics using SQL aggregation (avoids loading all relationships into memory)
    /// Returns HashMap<relationship_kind, count> grouped by relationship type
    /// Used by FastExploreTool's intelligent_dependencies mode
    pub fn get_relationship_type_statistics(&self) -> Result<HashMap<String, i64>> {
        let mut by_kind = HashMap::new();

        // SQL GROUP BY aggregation - counts relationships by kind without loading data into memory
        let query = "SELECT kind, COUNT(*) as count \
                     FROM relationships \
                     GROUP BY kind";

        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row in rows {
            let (kind, count) = row?;
            by_kind.insert(kind, count);
        }

        Ok(by_kind)
    }
}
