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
        assert!(!join_relations.is_empty());
    }
}
