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

    /// CREATE VIEW and CREATE TRIGGER must point at the real table symbols they
    /// depend on. A documented gap is not evidence, it is a bug with paperwork.
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
        let (symbols, relationships) = extract_symbols_and_relationships(sql_code);

        let products = symbols
            .iter()
            .find(|symbol| symbol.name == "products" && symbol.kind == SymbolKind::Class)
            .expect("products table should be extracted");
        let view = symbols
            .iter()
            .find(|symbol| symbol.name == "active_products" && symbol.kind == SymbolKind::Interface)
            .expect("active_products view should be extracted");
        let trigger = symbols
            .iter()
            .find(|symbol| symbol.name == "log_product_insert" && symbol.kind == SymbolKind::Method)
            .expect("log_product_insert trigger should be extracted");

        let mut view_or_trigger_rels = relationships
            .iter()
            .filter(|relationship| {
                matches!(
                    relationship
                        .metadata
                        .as_ref()
                        .and_then(|metadata| metadata.get("relationshipType"))
                        .and_then(|value| value.as_str()),
                    Some("view_source" | "trigger_target")
                )
            })
            .collect::<Vec<_>>();
        view_or_trigger_rels.sort_by_key(|relationship| {
            relationship
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("relationshipType"))
                .and_then(|value| value.as_str())
                .unwrap()
        });

        assert_eq!(
            view_or_trigger_rels.len(),
            2,
            "SQL should emit one view-source edge and one trigger-target edge; got {:?}",
            view_or_trigger_rels
                .iter()
                .map(|relationship| relationship
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("relationshipType"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("<missing>"))
                .collect::<Vec<_>>()
        );

        let trigger_relation = view_or_trigger_rels[0];
        assert_eq!(trigger_relation.kind, RelationshipKind::References);
        assert_eq!(trigger_relation.from_symbol_id, trigger.id);
        assert_eq!(trigger_relation.to_symbol_id, products.id);
        assert_eq!(
            trigger_relation.line_number,
            line_number(sql_code, "AFTER INSERT ON products")
        );
        assert_eq!(target_table(trigger_relation), "products");

        let view_relation = view_or_trigger_rels[1];
        assert_eq!(view_relation.kind, RelationshipKind::References);
        assert_eq!(view_relation.from_symbol_id, view.id);
        assert_eq!(view_relation.to_symbol_id, products.id);
        assert_eq!(
            view_relation.line_number,
            line_number(sql_code, "FROM products")
        );
        assert_eq!(target_table(view_relation), "products");
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
