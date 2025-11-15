use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_schema_and_index_metadata() {
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

        let analytics_schema = symbols.iter().find(|s| s.name == "analytics");
        assert!(analytics_schema.is_some());
        assert!(
            analytics_schema
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE SCHEMA analytics")
        );

        let user_mgmt_schema = symbols.iter().find(|s| s.name == "user_management");
        assert!(user_mgmt_schema.is_some());

        let email_index = symbols.iter().find(|s| s.name == "idx_users_email_active");
        assert!(email_index.is_some());
        assert!(
            email_index
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE UNIQUE INDEX")
        );
        assert!(
            email_index
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("WHERE is_active = TRUE")
        );

        let composite_index = symbols.iter().find(|s| s.name == "idx_events_user_time");
        assert!(composite_index.is_some());
        assert!(
            composite_index
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("(user_id, occurred_at DESC)")
        );
        assert!(
            composite_index
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("INCLUDE")
        );

        let gin_index = symbols.iter().find(|s| s.name == "idx_event_data_gin");
        assert!(gin_index.is_some());
        assert!(
            gin_index
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("USING GIN")
        );

        let text_search_index = symbols
            .iter()
            .find(|s| s.name == "idx_user_profiles_text_search");
        assert!(text_search_index.is_some());
        assert!(
            text_search_index
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("to_tsvector")
        );

        let username_constraint = symbols.iter().find(|s| s.name == "chk_username_length");
        assert!(username_constraint.is_some());
        assert!(
            username_constraint
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CHECK (LENGTH(username)")
        );

        let password_constraint = symbols.iter().find(|s| s.name == "chk_password_strength");
        assert!(password_constraint.is_some());

        let fk_constraint = symbols
            .iter()
            .find(|s| s.name == "fk_user_profiles_user_id");
        assert!(fk_constraint.is_some());
        assert!(
            fk_constraint
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("FOREIGN KEY")
        );
        assert!(
            fk_constraint
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("ON DELETE CASCADE")
        );

        let json_constraint = symbols
            .iter()
            .find(|s| s.name == "chk_event_data_structure");
        assert!(json_constraint.is_some());
        assert!(
            json_constraint
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("jsonb_typeof")
        );

        let user_sequence = symbols.iter().find(|s| s.name == "user_id_seq");
        assert!(user_sequence.is_some());
        assert!(
            user_sequence
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE SEQUENCE")
        );
        assert!(
            user_sequence
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("START WITH 1000")
        );

        let email_domain = symbols.iter().find(|s| s.name == "email_address");
        assert!(email_domain.is_some());
        assert!(
            email_domain
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE DOMAIN")
        );

        let user_status_enum = symbols.iter().find(|s| s.name == "user_status");
        assert!(user_status_enum.is_some());
        assert!(
            user_status_enum
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE TYPE")
        );
        assert!(
            user_status_enum
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("ENUM")
        );

        let mode_aggregate = symbols.iter().find(|s| s.name == "mode");
        assert!(mode_aggregate.is_some());
        assert!(
            mode_aggregate
                .unwrap()
                .signature
                .as_ref()
                .unwrap()
                .contains("CREATE AGGREGATE")
        );
    }
}
