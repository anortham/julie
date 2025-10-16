use super::*;

use crate::SymbolKind;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_indexes_and_performance_optimizations() {
        let code = r#"
-- Single column indexes
CREATE INDEX idx_users_email ON users(email);
CREATE UNIQUE INDEX idx_users_username ON users(username);
CREATE INDEX idx_posts_created_at ON posts(created_at DESC);

-- Composite indexes
CREATE INDEX idx_orders_customer_date ON orders(customer_id, order_date);
CREATE INDEX idx_products_category_price ON products(category_id, price DESC);

-- Partial indexes
CREATE INDEX idx_active_users ON users(email) WHERE active = true;
CREATE INDEX idx_high_value_orders ON orders(total_amount) WHERE total_amount > 1000.00;

-- Functional indexes
CREATE INDEX idx_users_lower_email ON users(LOWER(email));
CREATE INDEX idx_posts_title_search ON posts(TO_TSVECTOR('english', title));

-- Index with specific storage parameters
CREATE INDEX idx_large_table_data ON large_table(data)
    WITH (fillfactor = 70, autovacuum_enabled = false);

-- GIN indexes for arrays and full-text search
CREATE INDEX idx_posts_tags ON posts USING GIN(tags);
CREATE INDEX idx_posts_content_search ON posts USING GIN(TO_TSVECTOR('english', content));

-- GiST indexes for geometric data
CREATE INDEX idx_locations_geom ON locations USING GIST(ST_Point(longitude, latitude));

-- BRIN indexes for large tables with correlation
CREATE INDEX idx_logs_timestamp ON logs USING BRIN(timestamp)
    WITH (pages_per_range = 128);

-- Expression indexes
CREATE INDEX idx_calculated_score ON products((price * rating));
CREATE INDEX idx_full_name ON users((first_name || ' ' || last_name));

-- Conditional indexes with complex conditions
CREATE INDEX idx_recent_posts ON posts(published_at)
    WHERE published_at > CURRENT_DATE - INTERVAL '30 days'
    AND status = 'published';

-- Index on JSONB columns
CREATE INDEX idx_user_metadata ON users USING GIN(metadata);
CREATE INDEX idx_user_settings_theme ON users((metadata->>'theme'));
CREATE INDEX idx_user_preferences ON users USING GIN((metadata->'preferences'));

-- Covering indexes (include additional columns)
CREATE INDEX idx_orders_customer_status_date ON orders(customer_id, status, order_date)
    INCLUDE (total_amount, shipping_address);

-- Concurrent index creation
CREATE INDEX CONCURRENTLY idx_concurrent_example ON large_table(column_name);

-- Drop indexes
DROP INDEX IF EXISTS idx_users_email;
DROP INDEX idx_posts_created_at, idx_orders_customer_date;

-- Index maintenance
REINDEX INDEX idx_users_email;
REINDEX TABLE users;

-- Index statistics
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;

-- Index usage analysis
SELECT
    indexrelname,
    idx_tup_read,
    idx_tup_fetch,
    idx_scan
FROM pg_stat_user_indexes
WHERE idx_scan = 0
ORDER BY idx_tup_read DESC;
"#;

        let symbols = extract_symbols(code);

        // Test index creation statements
        let idx_users_email = symbols.iter().find(|s| s.name == "idx_users_email");
        assert!(idx_users_email.is_some());
        assert_eq!(idx_users_email.unwrap().kind, SymbolKind::Property); // Indexes are properties

        let idx_users_username = symbols.iter().find(|s| s.name == "idx_users_username");
        assert!(idx_users_username.is_some());

        let idx_posts_created_at = symbols.iter().find(|s| s.name == "idx_posts_created_at");
        assert!(idx_posts_created_at.is_some());

        // Test composite indexes
        let idx_orders_customer_date = symbols.iter().find(|s| s.name == "idx_orders_customer_date");
        assert!(idx_orders_customer_date.is_some());

        let idx_products_category_price = symbols.iter().find(|s| s.name == "idx_products_category_price");
        assert!(idx_products_category_price.is_some());

        // Test partial indexes
        let idx_active_users = symbols.iter().find(|s| s.name == "idx_active_users");
        assert!(idx_active_users.is_some());

        let idx_high_value_orders = symbols.iter().find(|s| s.name == "idx_high_value_orders");
        assert!(idx_high_value_orders.is_some());

        // Test functional indexes
        let idx_users_lower_email = symbols.iter().find(|s| s.name == "idx_users_lower_email");
        assert!(idx_users_lower_email.is_some());

        let idx_posts_title_search = symbols.iter().find(|s| s.name == "idx_posts_title_search");
        assert!(idx_posts_title_search.is_some());

        // Test special index types
        let idx_large_table_data = symbols.iter().find(|s| s.name == "idx_large_table_data");
        assert!(idx_large_table_data.is_some());

        let idx_posts_tags = symbols.iter().find(|s| s.name == "idx_posts_tags");
        assert!(idx_posts_tags.is_some());

        let idx_locations_geom = symbols.iter().find(|s| s.name == "idx_locations_geom");
        assert!(idx_locations_geom.is_some());

        let idx_logs_timestamp = symbols.iter().find(|s| s.name == "idx_logs_timestamp");
        assert!(idx_logs_timestamp.is_some());

        // Test expression indexes
        let idx_calculated_score = symbols.iter().find(|s| s.name == "idx_calculated_score");
        assert!(idx_calculated_score.is_some());

        let idx_full_name = symbols.iter().find(|s| s.name == "idx_full_name");
        assert!(idx_full_name.is_some());

        // Test conditional indexes
        let idx_recent_posts = symbols.iter().find(|s| s.name == "idx_recent_posts");
        assert!(idx_recent_posts.is_some());

        // Test JSONB indexes
        let idx_user_metadata = symbols.iter().find(|s| s.name == "idx_user_metadata");
        assert!(idx_user_metadata.is_some());

        let idx_user_settings_theme = symbols.iter().find(|s| s.name == "idx_user_settings_theme");
        assert!(idx_user_settings_theme.is_some());

        // Test covering indexes
        let idx_orders_customer_status_date = symbols.iter().find(|s| s.name == "idx_orders_customer_status_date");
        assert!(idx_orders_customer_status_date.is_some());

        // Test concurrent index
        let idx_concurrent_example = symbols.iter().find(|s| s.name == "idx_concurrent_example");
        assert!(idx_concurrent_example.is_some());
    }
}