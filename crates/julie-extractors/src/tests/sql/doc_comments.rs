use super::extract_symbols;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_doc_comment_from_create_table() {
        let code = r#"
            -- Stores user account information and authentication credentials
            -- Includes user profile, preferences, and security settings
            CREATE TABLE users (
                id SERIAL PRIMARY KEY,
                username VARCHAR(255),
                email VARCHAR(255)
            );
        "#;

        let symbols = extract_symbols(code);
        let users_table = symbols.iter().find(|s| s.name == "users").unwrap();

        assert!(users_table.doc_comment.is_some());
        let doc = users_table.doc_comment.as_ref().unwrap();
        assert!(doc.contains("user account information"));
    }

    #[test]
    fn test_extract_block_comment_from_create_table() {
        let code = r#"
            /*
             * Stores analytics event data for user behavior tracking
             * Contains timestamps, user references, and event metadata
             */
            CREATE TABLE analytics_events (
                id SERIAL PRIMARY KEY,
                user_id INT,
                event_type VARCHAR(100)
            );
        "#;

        let symbols = extract_symbols(code);
        let events_table = symbols
            .iter()
            .find(|s| s.name == "analytics_events")
            .unwrap();

        assert!(events_table.doc_comment.is_some());
        let doc = events_table.doc_comment.as_ref().unwrap();
        assert!(doc.contains("analytics event data"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_view() {
        let code = r#"
            -- Summarizes user engagement metrics and activity statistics
            -- Includes total sessions, event counts, and time-based metrics
            CREATE VIEW user_engagement_summary AS
            SELECT
                u.id,
                u.username,
                COUNT(*) as event_count
            FROM users u
            LEFT JOIN analytics_events ae ON u.id = ae.user_id
            GROUP BY u.id, u.username;
        "#;

        let symbols = extract_symbols(code);
        let view = symbols
            .iter()
            .find(|s| s.name == "user_engagement_summary")
            .unwrap();

        assert!(view.doc_comment.is_some());
        let doc = view.doc_comment.as_ref().unwrap();
        assert!(doc.contains("engagement metrics"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_index() {
        let code = r#"
            -- Index for fast user email lookups during authentication
            -- Optimizes WHERE clauses filtering by email address
            CREATE UNIQUE INDEX idx_users_email
            ON users (email)
            WHERE is_active = TRUE;
        "#;

        let symbols = extract_symbols(code);
        let index = symbols
            .iter()
            .find(|s| s.name == "idx_users_email")
            .unwrap();

        assert!(index.doc_comment.is_some());
        let doc = index.doc_comment.as_ref().unwrap();
        assert!(doc.contains("email lookups"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_trigger() {
        let code = r#"
            -- Automatically logs all user profile changes for audit compliance
            -- Records old and new values for all modified columns
            CREATE TRIGGER audit_user_profile_changes
            AFTER UPDATE ON user_profiles
            FOR EACH ROW
            BEGIN
                INSERT INTO audit_log (table_name, action) VALUES ('user_profiles', 'UPDATE');
            END;
        "#;

        let symbols = extract_symbols(code);
        let trigger = symbols
            .iter()
            .find(|s| s.name == "audit_user_profile_changes")
            .unwrap();

        assert!(trigger.doc_comment.is_some());
        let doc = trigger.doc_comment.as_ref().unwrap();
        assert!(doc.contains("audit compliance"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_procedure() {
        let code = r#"
            -- Validates user credentials and returns authentication status
            -- Performs database lookup and hash verification
            CREATE PROCEDURE validate_credentials(
                IN p_username VARCHAR(255),
                OUT p_valid BOOLEAN
            )
            BEGIN
                SELECT COUNT(*) INTO @count FROM users WHERE username = p_username;
                SET p_valid = (@count > 0);
            END;
        "#;

        let symbols = extract_symbols(code);
        let proc = symbols
            .iter()
            .find(|s| s.name == "validate_credentials")
            .unwrap();

        assert!(proc.doc_comment.is_some());
        let doc = proc.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Validates user credentials"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_function() {
        let code = r#"
            -- Calculates user engagement score based on activity metrics
            -- Returns normalized score between 0 and 100
            CREATE FUNCTION calculate_user_score(p_user_id BIGINT)
            RETURNS DECIMAL(10,2)
            LANGUAGE plpgsql
            AS $$
            BEGIN
                RETURN 42.5;
            END;
            $$;
        "#;

        let symbols = extract_symbols(code);
        let func = symbols
            .iter()
            .find(|s| s.name == "calculate_user_score")
            .unwrap();

        assert!(func.doc_comment.is_some());
        let doc = func.doc_comment.as_ref().unwrap();
        assert!(doc.contains("engagement score"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_domain() {
        let code = r#"
            -- Validates email addresses with standard RFC 5322 format rules
            -- Enforces constraints for email field usage across tables
            CREATE DOMAIN email_address AS VARCHAR(255)
            CHECK (VALUE ~ '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$');
        "#;

        let symbols = extract_symbols(code);
        let domain = symbols.iter().find(|s| s.name == "email_address").unwrap();

        assert!(domain.doc_comment.is_some());
        let doc = domain.doc_comment.as_ref().unwrap();
        assert!(doc.contains("email addresses"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_type() {
        let code = r#"
            -- Enumeration of possible user account states in the system
            -- Used to enforce valid status values across user-related tables
            CREATE TYPE user_status AS ENUM (
                'pending',
                'active',
                'suspended',
                'deleted'
            );
        "#;

        let symbols = extract_symbols(code);
        let type_sym = symbols.iter().find(|s| s.name == "user_status").unwrap();

        assert!(type_sym.doc_comment.is_some());
        let doc = type_sym.doc_comment.as_ref().unwrap();
        assert!(doc.contains("account states"));
    }

    #[test]
    fn test_extract_doc_comment_from_create_sequence() {
        let code = r#"
            -- Auto-increment sequence for user IDs starting from 1000
            -- Provides unique identifiers with controlled increment rate
            CREATE SEQUENCE user_id_seq
            START WITH 1000
            INCREMENT BY 1
            MINVALUE 1000
            MAXVALUE 9999999999;
        "#;

        let symbols = extract_symbols(code);
        let seq = symbols.iter().find(|s| s.name == "user_id_seq").unwrap();

        assert!(seq.doc_comment.is_some());
        let doc = seq.doc_comment.as_ref().unwrap();
        assert!(doc.contains("Auto-increment") || doc.contains("auto-increment"));
    }

    #[test]
    fn test_extract_doc_comment_from_cte() {
        let code = r#"
            WITH user_engagement AS (
                -- Calculates total events per user from analytics table
                -- Filters only active users with recent activity
                SELECT
                    user_id,
                    COUNT(*) as total_events,
                    MAX(occurred_at) as last_event
                FROM analytics_events
                GROUP BY user_id
            )
            SELECT * FROM user_engagement;
        "#;

        let symbols = extract_symbols(code);
        let cte = symbols
            .iter()
            .find(|s| s.name == "user_engagement")
            .unwrap();

        assert!(cte.doc_comment.is_some());
        let doc = cte.doc_comment.as_ref().unwrap();
        assert!(doc.contains("total events"));
    }

    #[test]
    fn test_no_doc_comment_when_missing() {
        let code = r#"
            CREATE TABLE simple_table (
                id INT PRIMARY KEY,
                name VARCHAR(100)
            );
        "#;

        let symbols = extract_symbols(code);
        let table = symbols.iter().find(|s| s.name == "simple_table").unwrap();

        // Should have None or empty doc_comment when no comment is provided
        assert!(table.doc_comment.is_none() || table.doc_comment.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_extract_multiple_doc_comments_in_single_file() {
        let code = r#"
            -- Users table for authentication and profile storage
            CREATE TABLE users (
                id INT PRIMARY KEY,
                email VARCHAR(255)
            );

            -- Events table for activity tracking
            CREATE TABLE events (
                id INT PRIMARY KEY,
                user_id INT
            );

            -- View of active users and their recent activity
            CREATE VIEW active_users AS
            SELECT u.id, u.email FROM users u WHERE active = TRUE;
        "#;

        let symbols = extract_symbols(code);

        let users = symbols.iter().find(|s| s.name == "users").unwrap();
        assert!(users.doc_comment.is_some());
        assert!(
            users
                .doc_comment
                .as_ref()
                .unwrap()
                .contains("authentication")
        );

        let events = symbols.iter().find(|s| s.name == "events").unwrap();
        assert!(events.doc_comment.is_some());
        assert!(
            events
                .doc_comment
                .as_ref()
                .unwrap()
                .contains("activity tracking")
        );

        let view = symbols.iter().find(|s| s.name == "active_users").unwrap();
        assert!(view.doc_comment.is_some());
        assert!(view.doc_comment.as_ref().unwrap().contains("active users"));
    }
}
