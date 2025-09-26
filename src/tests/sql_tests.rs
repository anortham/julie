#[cfg(test)]
mod sql_extractor_tests {
    use crate::extractors::base::{Symbol, SymbolKind, Relationship, RelationshipKind};
    use crate::extractors::sql::SqlExtractor;
    
    

    fn init_parser() -> tree_sitter::Parser {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_sequel::LANGUAGE.into()).expect("Error loading SQL grammar");
        parser
    }

    fn extract_symbols(code: &str) -> Vec<Symbol> {
        let mut parser = init_parser();

        let tree = parser.parse(code, None).unwrap();

        let mut extractor = SqlExtractor::new("sql".to_string(), "test.sql".to_string(), code.to_string());
        extractor.extract_symbols(&tree)
    }

    fn extract_symbols_and_relationships(code: &str) -> (Vec<Symbol>, Vec<Relationship>) {
        let mut parser = init_parser();
        let tree = parser.parse(code, None).unwrap();

        let mut extractor = SqlExtractor::new("sql".to_string(), "test.sql".to_string(), code.to_string());
        let symbols = extractor.extract_symbols(&tree);
        let relationships = extractor.extract_relationships(&tree, &symbols);
        (symbols, relationships)
    }

    #[test]
    fn test_ddl_extract_tables_columns_and_constraints() {
        let sql_code = r#"
-- User management tables
CREATE TABLE users (
    id BIGINT PRIMARY KEY AUTO_INCREMENT,
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    first_name VARCHAR(100),
    last_name VARCHAR(100),
    date_of_birth DATE,
    is_active BOOLEAN DEFAULT TRUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,

    CONSTRAINT chk_email_format CHECK (email LIKE '%@%.%'),
    CONSTRAINT chk_age CHECK (date_of_birth < CURDATE()),
    INDEX idx_username (username),
    INDEX idx_email (email),
    INDEX idx_created_at (created_at)
);

CREATE TABLE user_profiles (
    user_id BIGINT,
    bio TEXT,
    avatar_url VARCHAR(500),
    social_links JSON,
    preferences JSON DEFAULT '{}',

    PRIMARY KEY (user_id),
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Enum table for user roles
CREATE TABLE user_roles (
    id INT PRIMARY KEY,
    role_name ENUM('admin', 'moderator', 'user', 'guest') NOT NULL,
    permissions JSON
);

-- Complex table with various column types
CREATE TABLE analytics_events (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL,
    user_id BIGINT,
    session_id VARCHAR(100),
    event_data JSONB,
    ip_address INET,
    user_agent TEXT,
    occurred_at TIMESTAMPTZ DEFAULT NOW(),

    FOREIGN KEY (user_id) REFERENCES users(id),
    PARTITION BY RANGE (occurred_at)
);
"#;

        let symbols = extract_symbols(sql_code);

        // Should extract tables
        let users_table = symbols.iter().find(|s| s.name == "users" && s.kind == SymbolKind::Class);
        assert!(users_table.is_some());
        assert!(users_table.unwrap().signature.as_ref().unwrap().contains("CREATE TABLE users"));

        let user_profiles_table = symbols.iter().find(|s| s.name == "user_profiles");
        assert!(user_profiles_table.is_some());

        let user_roles_table = symbols.iter().find(|s| s.name == "user_roles");
        assert!(user_roles_table.is_some());

        let analytics_table = symbols.iter().find(|s| s.name == "analytics_events");
        assert!(analytics_table.is_some());

        // Should extract columns as fields
        let id_column = symbols.iter().find(|s| s.name == "id" && s.kind == SymbolKind::Field);
        assert!(id_column.is_some());
        assert!(id_column.unwrap().signature.as_ref().unwrap().contains("BIGINT PRIMARY KEY"));

        let username_column = symbols.iter().find(|s| s.name == "username");
        assert!(username_column.is_some());
        assert!(username_column.unwrap().signature.as_ref().unwrap().contains("VARCHAR(50) UNIQUE NOT NULL"));

        let email_column = symbols.iter().find(|s| s.name == "email");
        assert!(email_column.is_some());

        let is_active_column = symbols.iter().find(|s| s.name == "is_active");
        assert!(is_active_column.is_some());
        assert!(is_active_column.unwrap().signature.as_ref().unwrap().contains("BOOLEAN DEFAULT TRUE"));

        // Should extract JSON columns
        let social_links_column = symbols.iter().find(|s| s.name == "social_links");
        assert!(social_links_column.is_some());
        assert!(social_links_column.unwrap().signature.as_ref().unwrap().contains("JSON"));

        let event_data_column = symbols.iter().find(|s| s.name == "event_data");
        assert!(event_data_column.is_some());
        assert!(event_data_column.unwrap().signature.as_ref().unwrap().contains("JSONB"));

        // Should extract constraints
        let constraints = symbols.iter().filter(|s| s.kind == SymbolKind::Interface).collect::<Vec<_>>();
        assert!(constraints.len() >= 2);

        // Should extract indexes
        let indexes = symbols.iter().filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("INDEX"))).collect::<Vec<_>>();
        assert!(indexes.len() >= 3);
    }

    #[test]
    fn test_dml_extract_complex_queries_and_ctes() {
        let sql_code = r#"
-- Complex CTE query with window functions
WITH monthly_user_stats AS (
    SELECT
        DATE_TRUNC('month', created_at) as month,
        COUNT(*) as new_users,
        COUNT(*) OVER (ORDER BY DATE_TRUNC('month', created_at)) as cumulative_users
    FROM users
    WHERE created_at >= '2023-01-01'
    GROUP BY DATE_TRUNC('month', created_at)
),
active_user_metrics AS (
    SELECT
        user_id,
        COUNT(DISTINCT DATE(occurred_at)) as active_days,
        AVG(EXTRACT(EPOCH FROM (occurred_at - LAG(occurred_at) OVER (PARTITION BY user_id ORDER BY occurred_at)))) as avg_session_gap
    FROM analytics_events
    WHERE occurred_at >= NOW() - INTERVAL '30 days'
    GROUP BY user_id
    HAVING COUNT(DISTINCT DATE(occurred_at)) > 5
)
SELECT
    u.username,
    u.email,
    mus.month,
    mus.new_users,
    aum.active_days,
    aum.avg_session_gap,
    CASE
        WHEN aum.active_days > 20 THEN 'high_activity'
        WHEN aum.active_days > 10 THEN 'medium_activity'
        ELSE 'low_activity'
    END as activity_level,
    ROW_NUMBER() OVER (PARTITION BY mus.month ORDER BY aum.active_days DESC) as activity_rank
FROM users u
JOIN monthly_user_stats mus ON DATE_TRUNC('month', u.created_at) = mus.month
LEFT JOIN active_user_metrics aum ON u.id = aum.user_id
WHERE u.is_active = TRUE
ORDER BY mus.month DESC, aum.active_days DESC;

-- Recursive CTE for hierarchical data
WITH RECURSIVE user_hierarchy AS (
    -- Base case: top-level users
    SELECT id, username, manager_id, 0 as level, username as path
    FROM users
    WHERE manager_id IS NULL

    UNION ALL

    -- Recursive case: users with managers
    SELECT u.id, u.username, u.manager_id, uh.level + 1,
           uh.path || ' -> ' || u.username as path
    FROM users u
    JOIN user_hierarchy uh ON u.manager_id = uh.id
    WHERE uh.level < 10  -- Prevent infinite recursion
)
SELECT * FROM user_hierarchy ORDER BY level, path;

-- UPSERT operation (PostgreSQL syntax)
INSERT INTO user_profiles (user_id, bio, avatar_url)
VALUES (1, 'Software Engineer', 'https://example.com/avatar.jpg')
ON CONFLICT (user_id)
DO UPDATE SET
    bio = EXCLUDED.bio,
    avatar_url = EXCLUDED.avatar_url,
    updated_at = NOW();
"#;

        let symbols = extract_symbols(sql_code);

        // Should extract CTEs as functions or views
        let monthly_stats_function = symbols.iter().find(|s| s.name == "monthly_user_stats");
        assert!(monthly_stats_function.is_some());

        let active_user_metrics = symbols.iter().find(|s| s.name == "active_user_metrics");
        assert!(active_user_metrics.is_some());

        let user_hierarchy = symbols.iter().find(|s| s.name == "user_hierarchy");
        assert!(user_hierarchy.is_some());
        assert!(user_hierarchy.unwrap().signature.as_ref().unwrap().contains("RECURSIVE"));

        // Should extract main query columns/expressions as fields
        let activity_level = symbols.iter().find(|s| s.name == "activity_level");
        assert!(activity_level.is_some());

        let activity_rank = symbols.iter().find(|s| s.name == "activity_rank");
        assert!(activity_rank.is_some());

        // Should handle window functions
        let window_functions = symbols.iter().filter(|s| s.signature.as_ref().map_or(false, |sig| sig.contains("OVER ("))).collect::<Vec<_>>();
        assert!(window_functions.len() > 0);
    }

    #[test]
    fn test_stored_procedures_functions_and_triggers() {
        let sql_code = r#"
-- Stored procedure with parameters
DELIMITER $$
CREATE PROCEDURE GetUserAnalytics(
    IN p_user_id BIGINT,
    IN p_start_date DATE,
    IN p_end_date DATE,
    OUT p_total_events INT,
    OUT p_unique_sessions INT
)
BEGIN
    DECLARE EXIT HANDLER FOR SQLEXCEPTION
    BEGIN
        ROLLBACK;
        RESIGNAL;
    END;

    START TRANSACTION;

    SELECT
        COUNT(*) INTO p_total_events,
        COUNT(DISTINCT session_id) INTO p_unique_sessions
    FROM analytics_events
    WHERE user_id = p_user_id
      AND DATE(occurred_at) BETWEEN p_start_date AND p_end_date;

    COMMIT;
END$$
DELIMITER ;

-- User-defined function
CREATE FUNCTION CalculateUserScore(p_user_id BIGINT)
RETURNS DECIMAL(10,2)
READS SQL DATA
DETERMINISTIC
BEGIN
    DECLARE v_score DECIMAL(10,2) DEFAULT 0.0;
    DECLARE v_event_count INT;
    DECLARE v_account_age_days INT;

    SELECT COUNT(*), DATEDIFF(NOW(), created_at)
    INTO v_event_count, v_account_age_days
    FROM analytics_events ae
    JOIN users u ON ae.user_id = u.id
    WHERE ae.user_id = p_user_id;

    SET v_score = (v_event_count * 0.1) + (v_account_age_days * 0.01);

    RETURN COALESCE(v_score, 0.0);
END;

-- PostgreSQL function with JSON processing
CREATE OR REPLACE FUNCTION update_user_preferences(
    p_user_id BIGINT,
    p_preferences JSONB
) RETURNS BOOLEAN
LANGUAGE plpgsql
AS $$
DECLARE
    v_current_prefs JSONB;
    v_merged_prefs JSONB;
BEGIN
    -- Get current preferences
    SELECT preferences INTO v_current_prefs
    FROM user_profiles
    WHERE user_id = p_user_id;

    -- Merge with new preferences
    v_merged_prefs := COALESCE(v_current_prefs, '{}'::jsonb) || p_preferences;

    -- Update the user profile
    UPDATE user_profiles
    SET preferences = v_merged_prefs,
        updated_at = NOW()
    WHERE user_id = p_user_id;

    RETURN FOUND;
EXCEPTION
    WHEN OTHERS THEN
        RAISE LOG 'Error updating preferences for user %: %', p_user_id, SQLERRM;
        RETURN FALSE;
END;
$$;

-- Trigger for audit logging
CREATE TRIGGER audit_user_changes
    AFTER UPDATE ON users
    FOR EACH ROW
    WHEN (OLD.* IS DISTINCT FROM NEW.*)
BEGIN
    INSERT INTO audit_log (
        table_name,
        record_id,
        action,
        new_values,
        new_values,
        changed_by,
        changed_at
    ) VALUES (
        'users',
        NEW.id,
        'UPDATE',
        json_object(OLD.*),
        json_object(NEW.*),
        USER(),
        NOW()
    );
END;

-- View with complex aggregations
CREATE VIEW user_engagement_summary AS
SELECT
    u.id,
    u.username,
    u.email,
    COUNT(DISTINCT ae.session_id) as total_sessions,
    COUNT(ae.id) as total_events,
    MIN(ae.occurred_at) as first_event,
    MAX(ae.occurred_at) as last_event,
    AVG(EXTRACT(EPOCH FROM (ae.occurred_at - LAG(ae.occurred_at) OVER (PARTITION BY u.id ORDER BY ae.occurred_at)))) as avg_time_between_events,
    EXTRACT(DAYS FROM (MAX(ae.occurred_at) - MIN(ae.occurred_at))) as engagement_span_days,
    CalculateUserScore(u.id) as user_score
FROM users u
LEFT JOIN analytics_events ae ON u.id = ae.user_id
WHERE u.is_active = TRUE
GROUP BY u.id, u.username, u.email
HAVING COUNT(ae.id) > 0;
"#;

        let symbols = extract_symbols(sql_code);


        // Should extract stored procedures
        let get_user_analytics = symbols.iter().find(|s| s.name == "GetUserAnalytics" && s.kind == SymbolKind::Function);
        assert!(get_user_analytics.is_some());
        assert!(get_user_analytics.unwrap().signature.as_ref().unwrap().contains("CREATE PROCEDURE"));

        // Should extract function parameters
        let user_id_param = symbols.iter().find(|s| s.name == "p_user_id");
        assert!(user_id_param.is_some());
        assert!(user_id_param.unwrap().signature.as_ref().unwrap().contains("IN p_user_id BIGINT"));

        let total_events_param = symbols.iter().find(|s| s.name == "p_total_events");
        assert!(total_events_param.is_some());
        assert!(total_events_param.unwrap().signature.as_ref().unwrap().contains("OUT"));

        // Should extract user-defined functions
        let calculate_user_score = symbols.iter().find(|s| s.name == "CalculateUserScore");
        assert!(calculate_user_score.is_some());
        assert!(calculate_user_score.unwrap().signature.as_ref().unwrap().contains("RETURNS DECIMAL(10,2)"));

        let update_user_prefs = symbols.iter().find(|s| s.name == "update_user_preferences");
        assert!(update_user_prefs.is_some());
        assert!(update_user_prefs.unwrap().signature.as_ref().unwrap().contains("RETURNS BOOLEAN"));
        assert!(update_user_prefs.unwrap().signature.as_ref().unwrap().contains("LANGUAGE plpgsql"));

        // Should extract variables
        let score_var = symbols.iter().find(|s| s.name == "v_score");
        assert!(score_var.is_some());
        assert!(score_var.unwrap().signature.as_ref().unwrap().contains("DECLARE v_score DECIMAL(10,2)"));

        let current_prefs_var = symbols.iter().find(|s| s.name == "v_current_prefs");
        assert!(current_prefs_var.is_some());
        assert!(current_prefs_var.unwrap().signature.as_ref().unwrap().contains("JSONB"));

        // Should extract triggers
        let audit_trigger = symbols.iter().find(|s| s.name == "audit_user_changes");
        assert!(audit_trigger.is_some());
        assert!(audit_trigger.unwrap().signature.as_ref().unwrap().contains("CREATE TRIGGER"));
        assert!(audit_trigger.unwrap().signature.as_ref().unwrap().contains("AFTER UPDATE ON users"));

        // Should extract views
        let engagement_view = symbols.iter().find(|s| s.name == "user_engagement_summary");
        assert!(engagement_view.is_some());
        assert!(engagement_view.unwrap().signature.as_ref().unwrap().contains("CREATE VIEW"));

        // Should extract view columns
        let total_sessions = symbols.iter().find(|s| s.name == "total_sessions");
        assert!(total_sessions.is_some());

        let user_score = symbols.iter().find(|s| s.name == "user_score");
        assert!(user_score.is_some());
    }

    #[test]
    fn test_database_schema_and_indexes() {
        let sql_code = r#"
-- Create schema
CREATE SCHEMA analytics;
CREATE SCHEMA user_management;

-- Unique indexes
CREATE UNIQUE INDEX idx_users_email_active
ON users (email)
WHERE is_active = TRUE;

-- Composite indexes
CREATE INDEX idx_events_user_time
ON analytics_events (user_id, occurred_at DESC)
INCLUDE (event_type, event_data);

-- Partial indexes
CREATE INDEX idx_recent_active_users
ON users (created_at, last_login_at)
WHERE is_active = TRUE
  AND last_login_at > NOW() - INTERVAL '30 days';

-- GIN index for JSON data
CREATE INDEX idx_event_data_gin
ON analytics_events
USING GIN (event_data jsonb_path_ops);

-- Full-text search index
CREATE INDEX idx_user_profiles_text_search
ON user_profiles
USING GIN (to_tsvector('english', bio));

-- Constraints
ALTER TABLE users
ADD CONSTRAINT chk_username_length
CHECK (LENGTH(username) >= 3 AND LENGTH(username) <= 50);

ALTER TABLE users
ADD CONSTRAINT chk_password_strength
CHECK (LENGTH(password_hash) >= 8);

-- Foreign key with custom actions
ALTER TABLE user_profiles
ADD CONSTRAINT fk_user_profiles_user_id
FOREIGN KEY (user_id) REFERENCES users(id)
ON DELETE CASCADE ON UPDATE RESTRICT;

-- Check constraint with complex logic
ALTER TABLE analytics_events
ADD CONSTRAINT chk_event_data_structure
CHECK (
    event_data IS NULL OR (
        jsonb_typeof(event_data) = 'object' AND
        event_data ? 'timestamp' AND
        jsonb_typeof(event_data->'timestamp') = 'string'
    )
);

-- Sequence
CREATE SEQUENCE user_id_seq
    START WITH 1000
    INCREMENT BY 1
    MINVALUE 1000
    MAXVALUE 9999999999
    CACHE 100;

-- Domain
CREATE DOMAIN email_address AS VARCHAR(255)
    CHECK (VALUE ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$');

-- Enum type
CREATE TYPE user_status AS ENUM (
    'pending',
    'active',
    'suspended',
    'deleted'
);

-- Custom aggregate function
CREATE AGGREGATE mode(anyelement) (
    SFUNC = mode_state,
    STYPE = internal,
    FINALFUNC = mode_final
);
"#;

        let symbols = extract_symbols(sql_code);


        // Should extract schemas
        let analytics_schema = symbols.iter().find(|s| s.name == "analytics");
        assert!(analytics_schema.is_some());
        assert!(analytics_schema.unwrap().signature.as_ref().unwrap().contains("CREATE SCHEMA analytics"));

        let user_mgmt_schema = symbols.iter().find(|s| s.name == "user_management");
        assert!(user_mgmt_schema.is_some());

        // Should extract indexes
        let email_index = symbols.iter().find(|s| s.name == "idx_users_email_active");
        assert!(email_index.is_some());
        assert!(email_index.unwrap().signature.as_ref().unwrap().contains("CREATE UNIQUE INDEX"));
        assert!(email_index.unwrap().signature.as_ref().unwrap().contains("WHERE is_active = TRUE"));

        let composite_index = symbols.iter().find(|s| s.name == "idx_events_user_time");
        assert!(composite_index.is_some());
        assert!(composite_index.unwrap().signature.as_ref().unwrap().contains("(user_id, occurred_at DESC)"));
        assert!(composite_index.unwrap().signature.as_ref().unwrap().contains("INCLUDE"));

        let gin_index = symbols.iter().find(|s| s.name == "idx_event_data_gin");
        assert!(gin_index.is_some());
        assert!(gin_index.unwrap().signature.as_ref().unwrap().contains("USING GIN"));

        let text_search_index = symbols.iter().find(|s| s.name == "idx_user_profiles_text_search");
        assert!(text_search_index.is_some());
        assert!(text_search_index.unwrap().signature.as_ref().unwrap().contains("to_tsvector"));

        // Should extract constraints
        let username_constraint = symbols.iter().find(|s| s.name == "chk_username_length");
        assert!(username_constraint.is_some());
        assert!(username_constraint.unwrap().signature.as_ref().unwrap().contains("CHECK (LENGTH(username)"));

        let password_constraint = symbols.iter().find(|s| s.name == "chk_password_strength");
        assert!(password_constraint.is_some());

        let fk_constraint = symbols.iter().find(|s| s.name == "fk_user_profiles_user_id");
        assert!(fk_constraint.is_some());
        assert!(fk_constraint.unwrap().signature.as_ref().unwrap().contains("FOREIGN KEY"));
        assert!(fk_constraint.unwrap().signature.as_ref().unwrap().contains("ON DELETE CASCADE"));

        let json_constraint = symbols.iter().find(|s| s.name == "chk_event_data_structure");
        assert!(json_constraint.is_some());
        assert!(json_constraint.unwrap().signature.as_ref().unwrap().contains("jsonb_typeof"));

        // Should extract sequences
        let user_sequence = symbols.iter().find(|s| s.name == "user_id_seq");
        assert!(user_sequence.is_some());
        assert!(user_sequence.unwrap().signature.as_ref().unwrap().contains("CREATE SEQUENCE"));
        assert!(user_sequence.unwrap().signature.as_ref().unwrap().contains("START WITH 1000"));

        // Should extract domains
        let email_domain = symbols.iter().find(|s| s.name == "email_address");
        assert!(email_domain.is_some());
        assert!(email_domain.unwrap().signature.as_ref().unwrap().contains("CREATE DOMAIN"));

        // Should extract custom types
        let user_status_enum = symbols.iter().find(|s| s.name == "user_status");
        assert!(user_status_enum.is_some());
        assert!(user_status_enum.unwrap().signature.as_ref().unwrap().contains("CREATE TYPE"));
        assert!(user_status_enum.unwrap().signature.as_ref().unwrap().contains("ENUM"));

        // Should extract aggregate functions
        let mode_aggregate = symbols.iter().find(|s| s.name == "mode");
        assert!(mode_aggregate.is_some());
        assert!(mode_aggregate.unwrap().signature.as_ref().unwrap().contains("CREATE AGGREGATE"));
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

        // Should extract foreign key relationships
        assert!(relationships.len() > 0);

        let order_user_relation = relationships.iter().find(|r|
            r.kind == RelationshipKind::References &&
            r.metadata.as_ref().and_then(|m| m.get("targetTable")).and_then(|v| v.as_str()) == Some("users")
        );
        assert!(order_user_relation.is_some());

        let order_items_order_relation = relationships.iter().find(|r|
            r.kind == RelationshipKind::References &&
            r.metadata.as_ref().and_then(|m| m.get("targetTable")).and_then(|v| v.as_str()) == Some("orders")
        );
        assert!(order_items_order_relation.is_some());

        // Should extract column types
        let total_amount_column = symbols.iter().find(|s| s.name == "total_amount");
        assert!(total_amount_column.is_some());

        let status_column = symbols.iter().find(|s| s.name == "status");
        assert!(status_column.is_some());

        // Should extract join relationships from queries
        let join_relations = relationships.iter().filter(|r| r.kind == RelationshipKind::Joins).collect::<Vec<_>>();
        assert!(join_relations.len() >= 1);
    }
}