use super::{RelationshipKind, extract_symbols_and_relationships};

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(!relationships.is_empty());

        let order_user_relation = relationships.iter().find(|r| {
            r.kind == RelationshipKind::References
                && r.metadata
                    .as_ref()
                    .and_then(|m| m.get("targetTable"))
                    .and_then(|v| v.as_str())
                    == Some("users")
        });
        assert!(order_user_relation.is_some());

        let order_items_order_relation = relationships.iter().find(|r| {
            r.kind == RelationshipKind::References
                && r.metadata
                    .as_ref()
                    .and_then(|m| m.get("targetTable"))
                    .and_then(|v| v.as_str())
                    == Some("orders")
        });
        assert!(order_items_order_relation.is_some());

        let total_amount_column = symbols.iter().find(|s| s.name == "total_amount");
        assert!(total_amount_column.is_some());

        let status_column = symbols.iter().find(|s| s.name == "status");
        assert!(status_column.is_some());

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
            joined_tables.push(table_name);
        }

        joined_tables.sort_unstable();
        assert_eq!(joined_tables, vec!["order_items", "orders"]);
    }
}
