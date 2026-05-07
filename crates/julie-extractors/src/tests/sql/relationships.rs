use super::{RelationshipKind, SymbolKind, extract_symbols_and_relationships};

#[cfg(test)]
mod tests {
    use super::*;

    /// SQL is in NO_PENDING_CAPABILITIES. When a FK references a table that is not
    /// defined in the same file, the relationship must be suppressed entirely.
    /// A dead synthetic ID like "external_users" must never appear as to_symbol_id.
    #[test]
    fn test_sql_pending_relationships_do_not_use_dead_synthetic_ids() {
        let sql_code = r#"
CREATE TABLE orders (
    id BIGINT PRIMARY KEY,
    user_id BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id)
);
"#;
        let (_symbols, relationships) = extract_symbols_and_relationships(sql_code);

        // users is not defined in this file; SQL is in NO_PENDING_CAPABILITIES,
        // so the relationship must be suppressed (not emitted with a synthetic ID)
        for rel in &relationships {
            assert!(
                !rel.to_symbol_id.starts_with("external_"),
                "to_symbol_id must not be a dead synthetic placeholder, got: {}",
                rel.to_symbol_id
            );
        }

        // FK from orders to undefined table must be fully suppressed
        assert_eq!(
            relationships.len(),
            0,
            "FK to undefined table must be suppressed, not emitted with synthetic external_ ID"
        );
    }

    /// SQL extractor does not extract view-source or trigger-target relationships.
    /// This test documents that gap: no such relationships are emitted even when
    /// the file contains CREATE VIEW and CREATE TRIGGER statements.
    /// The gap is recorded in fixtures/extraction/capabilities.json under "sql"
    /// (see H25 in docs/findings/COMPILED-FINDINGS.md).
    #[test]
    fn test_sql_view_and_trigger_relationships_target_real_tables() {
        let sql_code = r#"
CREATE TABLE products (
    id BIGINT PRIMARY KEY,
    name VARCHAR(255) NOT NULL
);

CREATE VIEW active_products AS
    SELECT id, name FROM products WHERE name IS NOT NULL;

CREATE TRIGGER log_product_insert
    AFTER INSERT ON products
    FOR EACH ROW
    BEGIN
        INSERT INTO audit_log (table_name) VALUES ('products');
    END;
"#;
        let (_symbols, relationships) = extract_symbols_and_relationships(sql_code);

        // View-source and trigger-target relationships are not yet extracted.
        // This is a documented gap in capabilities.json — only foreign_key_constraint
        // and JOIN relationships are supported.
        let view_or_trigger_rels: Vec<_> = relationships
            .iter()
            .filter(|r| {
                r.metadata
                    .as_ref()
                    .and_then(|m| m.get("relationshipType"))
                    .and_then(|v| v.as_str())
                    .map(|t| t == "view_source" || t == "trigger_target")
                    .unwrap_or(false)
            })
            .collect();

        assert_eq!(
            view_or_trigger_rels.len(),
            0,
            "view-source and trigger-target relationships are not yet extracted (documented gap)"
        );

        // The gap must be documented in capabilities.json so callers know SQL
        // only covers foreign_key_constraint and JOIN relationships, not
        // view-source or trigger-target (H25 in COMPILED-FINDINGS.md).
        let capabilities_json =
            include_str!("../../../../../fixtures/extraction/capabilities.json");
        let capabilities: serde_json::Value =
            serde_json::from_str(capabilities_json).expect("capabilities.json must be valid JSON");

        let sql_entry = capabilities["languages"]
            .as_array()
            .expect("languages must be an array")
            .iter()
            .find(|lang| lang["language"].as_str() == Some("sql"))
            .expect("sql entry must exist in capabilities.json");

        let gaps = sql_entry["capability_gaps"]
            .as_array()
            .expect("SQL must have a capability_gaps array in capabilities.json to document the view-source and trigger-target extraction gap (H25)");

        let has_relationships_gap = gaps
            .iter()
            .any(|gap| gap["capability"].as_str() == Some("relationships"));

        assert!(
            has_relationships_gap,
            "capabilities.json must record a 'relationships' capability_gap for SQL covering the view-source and trigger-target extraction gap"
        );
    }

    #[test]
    fn test_type_inference_and_relationships() {
        let sql_code = r#"
CREATE TABLE orders (
    id BIGINT PRIMARY KEY,
    user_id BIGINT NOT NULL,
    total_amount DECIMAL(10,2),
    status VARCHAR(20) DEFAULT 'pending',
    created_at TIMESTAMP DEFAULT NOW(),

    FOREIGN KEY (user_id) REFERENCES users(id)
);

CREATE TABLE users (
    id BIGINT PRIMARY KEY,
    username VARCHAR(255) NOT NULL
);

CREATE TABLE order_items (
    id BIGINT PRIMARY KEY,
    order_id BIGINT NOT NULL,
    product_id BIGINT NOT NULL,
    quantity INT NOT NULL,
    unit_price DECIMAL(8,2) NOT NULL,

    FOREIGN KEY (order_id) REFERENCES orders(id) ON DELETE CASCADE,
    FOREIGN KEY (product_id) REFERENCES products(id)
);

-- Join query for relationship testing
SELECT
    u.username,
    o.total_amount,
    oi.quantity,
    p.name as product_name
FROM users u
JOIN orders o ON u.id = o.user_id
JOIN order_items oi ON o.id = oi.order_id
JOIN products p ON oi.product_id = p.id
WHERE o.status = 'completed';
"#;

        let (symbols, relationships) = extract_symbols_and_relationships(sql_code);

        // 2 FKs with both tables in file (orders->users, order_items->orders)
        // + 2 JOINs with both tables in file (users->orders, users->order_items).
        // The FK from order_items->products is suppressed because products is not
        // defined in this file (SQL is in NO_PENDING_CAPABILITIES).
        // The JOIN to products is also suppressed for the same reason.
        assert_eq!(relationships.len(), 4);

        let mut referenced_tables = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::References)
            .map(target_table)
            .collect::<Vec<_>>();
        referenced_tables.sort_unstable();
        // products is not in this file, so the FK to it is suppressed entirely
        assert_eq!(referenced_tables, vec!["orders", "users"]);

        let order_user_relation = relationships
            .iter()
            .find(|r| r.kind == RelationshipKind::References && target_table(r) == "users")
            .expect("orders.user_id should reference users");
        assert_eq!(
            order_user_relation.line_number,
            line_number(sql_code, "FOREIGN KEY (user_id)")
        );

        let total_amount_column = symbols
            .iter()
            .find(|s| s.name == "total_amount")
            .expect("total_amount column should be extracted");
        assert_eq!(total_amount_column.kind, SymbolKind::Field);

        let status_column = symbols
            .iter()
            .find(|s| s.name == "status")
            .expect("status column should be extracted");
        assert_eq!(status_column.kind, SymbolKind::Field);

        let join_relations = relationships
            .iter()
            .filter(|r| r.kind == RelationshipKind::Joins)
            .collect::<Vec<_>>();
        assert_eq!(join_relations.len(), 2);

        let users = symbols.iter().find(|s| s.name == "users").unwrap();
        let mut joined_tables = Vec::new();

        for relation in &join_relations {
            assert_eq!(relation.from_symbol_id, users.id);
            assert_ne!(relation.from_symbol_id, relation.to_symbol_id);

            let table_name = relation
                .metadata
                .as_ref()
                .and_then(|m| m.get("tableName"))
                .and_then(|v| v.as_str())
                .unwrap();
            let target = symbols.iter().find(|s| s.name == table_name).unwrap();

            assert_eq!(relation.to_symbol_id, target.id);
            assert_eq!(
                relation.line_number,
                line_number(sql_code, &format!("JOIN {table_name}"))
            );
            joined_tables.push(table_name);
        }

        joined_tables.sort_unstable();
        assert_eq!(joined_tables, vec!["order_items", "orders"]);
    }

    fn target_table(relationship: &crate::base::Relationship) -> &str {
        relationship
            .metadata
            .as_ref()
            .and_then(|metadata| {
                metadata
                    .get("targetTable")
                    .or_else(|| metadata.get("tableName"))
            })
            .and_then(|value| value.as_str())
            .expect("relationship should record target table metadata")
    }

    fn line_number(code: &str, needle: &str) -> u32 {
        code[..code.find(needle).expect("needle must be present")]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count() as u32
            + 1
    }
}
