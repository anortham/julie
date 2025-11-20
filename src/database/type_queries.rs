//! Type intelligence queries for fast_explore Types mode
//!
//! Provides database query methods for:
//! - Finding implementations of interfaces/traits
//! - Finding functions returning specific types
//! - Finding functions accepting specific types as parameters
//! - Type hierarchy exploration

use anyhow::Result;
use rusqlite::OptionalExtension;
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::database::helpers::SYMBOL_COLUMNS;
use crate::extractors::Symbol;

impl SymbolDatabase {
    /// Find all symbols that implement a given interface/trait
    ///
    /// Queries the relationships table for "implements" relationships
    /// where the target type matches the given type_name.
    ///
    /// # Example
    /// ```ignore
    /// let implementations = db.find_implementations("PaymentProcessor", Some("typescript"))?;
    /// // Returns: [StripeProcessor, PayPalProcessor]
    /// ```
    pub fn find_type_implementations(
        &self,
        type_name: &str,
        language: Option<&str>,
    ) -> Result<Vec<Symbol>> {
        debug!(
            "Querying implementations of type: {} (language: {:?})",
            type_name, language
        );

        // Build query with prefixed columns for JOIN
        let columns_with_prefix = SYMBOL_COLUMNS
            .split(", ")
            .map(|col| format!("s.{}", col))
            .collect::<Vec<_>>()
            .join(", ");

        let query = if language.is_some() {
            format!(
                "SELECT DISTINCT {} FROM symbols s
                 INNER JOIN relationships r ON s.id = r.from_symbol_id
                 INNER JOIN symbols t ON r.to_symbol_id = t.id
                 WHERE r.kind = 'implements'
                 AND t.name = ?1
                 AND s.language = ?2",
                columns_with_prefix
            )
        } else {
            format!(
                "SELECT DISTINCT {} FROM symbols s
                 INNER JOIN relationships r ON s.id = r.from_symbol_id
                 INNER JOIN symbols t ON r.to_symbol_id = t.id
                 WHERE r.kind = 'implements'
                 AND t.name = ?1",
                columns_with_prefix
            )
        };

        let mut stmt = self.conn.prepare(&query)?;
        let mut implementations = Vec::new();

        if let Some(lang) = language {
            let rows = stmt.query_map([type_name, lang], |row| self.row_to_symbol(row))?;
            for row in rows {
                implementations.push(row?);
            }
        } else {
            let rows = stmt.query_map([type_name], |row| self.row_to_symbol(row))?;
            for row in rows {
                implementations.push(row?);
            }
        }

        debug!(
            "Found {} implementations of {}",
            implementations.len(),
            type_name
        );
        Ok(implementations)
    }

    /// Find all functions/methods that return a given type
    ///
    /// Queries the types table for symbols with matching resolved_type,
    /// filtered to only include functions and methods.
    ///
    /// # Example
    /// ```ignore
    /// let returners = db.find_functions_returning_type("PaymentResult", Some("typescript"))?;
    /// // Returns: [process(), refund(), createPayment()]
    /// ```
    pub fn find_functions_returning_type(
        &self,
        type_name: &str,
        language: Option<&str>,
    ) -> Result<Vec<Symbol>> {
        debug!(
            "Querying functions returning type: {} (language: {:?})",
            type_name, language
        );

        let type_pattern = format!("%{}%", type_name);

        // Build query with prefixed columns for JOIN
        let columns_with_prefix = SYMBOL_COLUMNS
            .split(", ")
            .map(|col| format!("s.{}", col))
            .collect::<Vec<_>>()
            .join(", ");

        let query = if language.is_some() {
            format!(
                "SELECT DISTINCT {} FROM symbols s
                 INNER JOIN types t ON s.id = t.symbol_id
                 WHERE t.resolved_type LIKE ?1
                 AND (s.kind = 'function' OR s.kind = 'method')
                 AND s.language = ?2",
                columns_with_prefix
            )
        } else {
            format!(
                "SELECT DISTINCT {} FROM symbols s
                 INNER JOIN types t ON s.id = t.symbol_id
                 WHERE t.resolved_type LIKE ?1
                 AND (s.kind = 'function' OR s.kind = 'method')",
                columns_with_prefix
            )
        };

        let mut stmt = self.conn.prepare(&query)?;
        let mut returners = Vec::new();

        if let Some(lang) = language {
            let rows =
                stmt.query_map([&type_pattern as &str, lang], |row| self.row_to_symbol(row))?;
            for row in rows {
                returners.push(row?);
            }
        } else {
            let rows = stmt.query_map([&type_pattern], |row| self.row_to_symbol(row))?;
            for row in rows {
                returners.push(row?);
            }
        }

        debug!(
            "Found {} functions returning {}",
            returners.len(),
            type_name
        );
        Ok(returners)
    }

    /// Find all functions/methods that accept a given type as a parameter
    ///
    /// Queries symbols for functions/methods whose signatures contain the type_name.
    /// This is a simple string match on signatures - not perfect but functional.
    ///
    /// # Example
    /// ```ignore
    /// let acceptors = db.find_functions_with_parameter_type("PaymentProcessor", Some("typescript"))?;
    /// // Returns: [createPayment(processor: PaymentProcessor, ...)]
    /// ```
    pub fn find_functions_with_parameter_type(
        &self,
        type_name: &str,
        language: Option<&str>,
    ) -> Result<Vec<Symbol>> {
        debug!(
            "Querying functions with parameter type: {} (language: {:?})",
            type_name, language
        );

        let type_pattern = format!("%{}%", type_name);

        let query = if language.is_some() {
            format!(
                "SELECT {} FROM symbols
                 WHERE (kind = 'function' OR kind = 'method')
                 AND signature LIKE ?1
                 AND language = ?2",
                SYMBOL_COLUMNS
            )
        } else {
            format!(
                "SELECT {} FROM symbols
                 WHERE (kind = 'function' OR kind = 'method')
                 AND signature LIKE ?1",
                SYMBOL_COLUMNS
            )
        };

        let mut stmt = self.conn.prepare(&query)?;
        let mut acceptors = Vec::new();

        if let Some(lang) = language {
            let rows =
                stmt.query_map([&type_pattern as &str, lang], |row| self.row_to_symbol(row))?;
            for row in rows {
                acceptors.push(row?);
            }
        } else {
            let rows = stmt.query_map([&type_pattern], |row| self.row_to_symbol(row))?;
            for row in rows {
                acceptors.push(row?);
            }
        }

        debug!(
            "Found {} functions with parameter type {}",
            acceptors.len(),
            type_name
        );
        Ok(acceptors)
    }

    /// Find type hierarchy relationships (extends, inherits)
    ///
    /// Returns both:
    /// - Parents: types that this type extends/inherits from
    /// - Children: types that extend/inherit from this type
    ///
    /// # Example
    /// ```ignore
    /// let (parents, children) = db.find_type_hierarchy("BaseClass", Some("typescript"))?;
    /// // Returns: ([], [DerivedClass1, DerivedClass2])
    /// ```
    pub fn find_type_hierarchy(
        &self,
        type_name: &str,
        language: Option<&str>,
    ) -> Result<(Vec<Symbol>, Vec<Symbol>)> {
        debug!(
            "Querying type hierarchy for: {} (language: {:?})",
            type_name, language
        );

        // Find the symbol for this type first
        let type_symbol_ids = self.find_symbol_ids_by_name(type_name, language)?;

        if type_symbol_ids.is_empty() {
            debug!("Type {} not found in database", type_name);
            return Ok((vec![], vec![]));
        }

        let mut parents = Vec::new();
        let mut children = Vec::new();

        for symbol_id in type_symbol_ids {
            // Find parent types (this type extends FROM them)
            let parent_rels = self.get_outgoing_relationships(&symbol_id)?;
            for rel in parent_rels {
                if matches!(rel.kind, crate::extractors::RelationshipKind::Extends) {
                    if let Ok(Some(parent)) = self.get_symbol_by_id(&rel.to_symbol_id) {
                        parents.push(parent);
                    }
                }
            }

            // Find child types (other types extend FROM this type)
            let child_rels = self.get_relationships_to_symbol(&symbol_id)?;
            for rel in child_rels {
                if matches!(rel.kind, crate::extractors::RelationshipKind::Extends) {
                    if let Ok(Some(child)) = self.get_symbol_by_id(&rel.from_symbol_id) {
                        children.push(child);
                    }
                }
            }
        }

        debug!(
            "Found {} parents and {} children for {}",
            parents.len(),
            children.len(),
            type_name
        );
        Ok((parents, children))
    }

    /// Helper: Find symbol IDs by name (may return multiple matches)
    fn find_symbol_ids_by_name(&self, name: &str, language: Option<&str>) -> Result<Vec<String>> {
        let mut ids = Vec::new();

        if let Some(lang) = language {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM symbols WHERE name = ?1 AND language = ?2")?;
            let rows = stmt.query_map([name, lang], |row| row.get::<_, String>(0))?;
            for row in rows {
                ids.push(row?);
            }
        } else {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM symbols WHERE name = ?1")?;
            let rows = stmt.query_map([name], |row| row.get::<_, String>(0))?;
            for row in rows {
                ids.push(row?);
            }
        }

        Ok(ids)
    }

    /// Get the resolved type for a specific symbol
    ///
    /// Queries the types table for the resolved_type of the given symbol_id.
    /// Returns None if no type information exists for this symbol.
    ///
    /// # Example
    /// ```ignore
    /// let symbol_type = db.get_type_for_symbol("symbol_123")?;
    /// // Returns: Some("Promise<UserProfile>") or None
    /// ```
    pub fn get_type_for_symbol(&self, symbol_id: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT resolved_type FROM types WHERE symbol_id = ?1")?;
        let type_result = stmt.query_row([symbol_id], |row| row.get(0)).optional()?;
        Ok(type_result)
    }
}
