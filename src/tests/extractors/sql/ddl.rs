use super::{extract_symbols, SymbolKind};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tables_columns_and_constraints() {
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

        let users_table = symbols
            .iter()
            .find(|s| s.name == "users" && s.kind == SymbolKind::Class);
        assert!(users_table.is_some());
        assert!(users_table
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("CREATE TABLE users"));

        let user_profiles_table = symbols.iter().find(|s| s.name == "user_profiles");
        assert!(user_profiles_table.is_some());

        let user_roles_table = symbols.iter().find(|s| s.name == "user_roles");
        assert!(user_roles_table.is_some());

        let analytics_table = symbols.iter().find(|s| s.name == "analytics_events");
        assert!(analytics_table.is_some());

        let id_column = symbols
            .iter()
            .find(|s| s.name == "id" && s.kind == SymbolKind::Field);
        assert!(id_column.is_some());
        assert!(id_column
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("BIGINT PRIMARY KEY"));

        let username_column = symbols.iter().find(|s| s.name == "username");
        assert!(username_column.is_some());
        assert!(username_column
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("VARCHAR(50) UNIQUE NOT NULL"));

        let email_column = symbols.iter().find(|s| s.name == "email");
        assert!(email_column.is_some());

        let is_active_column = symbols.iter().find(|s| s.name == "is_active");
        assert!(is_active_column.is_some());
        assert!(is_active_column
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("BOOLEAN DEFAULT TRUE"));

        let social_links_column = symbols.iter().find(|s| s.name == "social_links");
        assert!(social_links_column.is_some());
        assert!(social_links_column
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("JSON"));

        let event_data_column = symbols.iter().find(|s| s.name == "event_data");
        assert!(event_data_column.is_some());
        assert!(event_data_column
            .unwrap()
            .signature
            .as_ref()
            .unwrap()
            .contains("JSONB"));

        let constraints = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Interface)
            .collect::<Vec<_>>();
        assert!(constraints.len() >= 2);

        let indexes = symbols
            .iter()
            .filter(|s| {
                s.signature
                    .as_ref()
                    .map_or(false, |sig| sig.contains("INDEX"))
            })
            .collect::<Vec<_>>();
        assert!(indexes.len() >= 3);
    }
}
